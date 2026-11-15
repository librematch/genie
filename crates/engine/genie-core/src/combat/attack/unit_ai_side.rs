//! The attack the unit AI commands through its view while it runs ([`UnitView::attack`]): the
//! new Attack, a forced attack again on the unit's own target, the
//! target as a hit reads it, the start's readiness, the target's sight and a villager's swap.

use crate::combat::AttackCount;
use crate::combat::attack::AttackSource;
use crate::combat::attack::AttackState;
use crate::combat::attack::{
    Attack, Mark, OwnStart, Posted, ReadyReads, TARGET_GONE, Wielder, arms_looking, engagement,
    target_in_sight,
};
use crate::combat::attack::{AttackReads, Fight};
use crate::combat::damage::Victim;
use crate::combat::victims::{ChargeOf, VictimLayers, VictimView};
use crate::combat::victims::{victim_in, victim_of};
use crate::map::spatial::Placed;
use crate::math::rng::Stream;
use crate::math::rng::Well512;
use crate::objects::ObjectId;
use crate::objects::actions::Action;
use crate::objects::masters::ActionKind;
use crate::objects::masters::Master;
use crate::objects::masters::TaskGroup;
use crate::objects::masters::TaskSprites;
use crate::objects::masters::player_master;
use crate::objects::masters::player_masters;
use crate::objects::release::Releaser;
use crate::objects::sprite::new_sprite;
use crate::objects::teardown::HeadStart;
use crate::orders::{UNPACK_INSTALL, placed_reach};
use crate::unit_ai::Attacked;
use crate::unit_ai::OrderForce;
use crate::unit_ai::UnitAi;
use crate::unit_ai::UnitStance;
use crate::unit_ai::task_finder::world::SideWorld;
use crate::unit_ai::world_view::owner_view;
use crate::unit_ai::world_view::{NOT_HELD, UnitView};
use crate::unit_ai::{AiCommand, Effect};
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::slots::PlayerSlotsExt;
use crate::world::{ATTACKER_UNPLACED, NO_STANDING_SPRITE};
use crate::{GenieError, GenieResult};

impl UnitView<'_, '_> {
    /// The attack command (slot 0x338, `FUN_14120cea0`; a trebuchet's own part first,
    /// [`UnitView::trebuchet_attack`]) -> `FUN_141081a80`: a target the world does not hold fails;
    /// a top Attack whose target is the object (slot 0x190's lookup, [`UnitView::top_target_now`]:
    /// never a recyclable one) answers true, forced after its slot 0x48 on it
    /// ([`UnitView::attack_again`]); a villager switches its master first (slot 0x90,
    /// [`UnitView::swapped_master`]); `+0x648` takes the world's time (not held). Then the new
    /// Attack ([`UnitView::construct_attack`]: it takes its target), the list's deletion
    /// (`FUN_140a98620`, [`Unit::tear_down_actions`]: what the old actions let go follows the
    /// take) and slot 0x808 with it (`0x141081ff7`, [`Unit::set_only_own_action_with`], with the
    /// running AI's target): below state 3 the list's add starts it
    /// ([`UnitView::start_added_attack`]), in state 3 or later it deletes it unstarted, its
    /// destructor letting the target go. The command answers true either way, with the notifies
    /// the start posted. The command answers two top actions before the switch, an unforced one
    /// of type 7 on the target (true) and one of type `0x7e` (false), but genie's list holds
    /// neither type; a unit the load could not read whole stops at the deletion with the
    /// import's error. A notify posted while the AI runs (its buffer, `FUN_140c55270`) is not
    /// modelled.
    ///
    /// [`Unit::tear_down_actions`]: crate::objects::unit::Unit::tear_down_actions
    /// [`Unit::set_only_own_action_with`]: crate::objects::unit::Unit::set_only_own_action_with
    pub(crate) fn start_attack_now(
        &mut self,
        target: ObjectId,
        force: OrderForce,
        ai: &mut UnitAi,
        rng: &mut Well512,
        roles: &Roles,
    ) -> GenieResult<Attacked> {
        // the unit's master, a copy: the attack's start changes the unit (its master's switch)
        let own = self
            .unit
            .master(self.step.world.master_tables(), self.player)?
            .clone();
        let master = &own;
        if let Some(attacked) = self.trebuchet_attack(target, master, ai, rng, roles)? {
            return Ok(attacked);
        }
        let ai = &*ai;
        let Some(placed) = self.step.world.spatial.get_known(target)?.copied() else {
            return Ok(Attacked::default());
        };
        // the head's target as `FUN_140a78450` finds it: a recyclable one names no object
        if let Some(&Action::Attack(attack)) = self.actions.list.head()
            && self.top_target_now() == Some(target)
        {
            if force == OrderForce::Forced {
                self.attack_again((target, attack), &placed, master, ai, roles)?;
            }
            return Ok(Attacked {
                attacks: true,
                posted: Vec::new(),
            });
        }
        let master = &self.swapped_master(own, ai.current_target, roles)?;
        if self.unit.sprite.is_none() {
            return Err(GenieError::unsupported(
                "an attack command of a unit whose sprite the slice does not hold",
            ));
        }
        let by = Releaser {
            id: self.unit.id,
            class: Some(master.class),
            player: self.player,
        };
        let attack = self.construct_attack(target, &placed, master, ai, by, roles)?;
        self.unit
            .tear_down_actions(self.actions, self.step, ai.current_target, by)?;
        let posted = match self.unit.set_only_own_action_with(
            self.actions,
            Some(attack),
            self.step,
            ai.current_target,
            by,
        )? {
            Some(attack) => self.start_added_attack(attack, master, ai, roles)?,
            None => Vec::new(),
        };
        Ok(Attacked {
            attacks: true,
            posted,
        })
    }

    /// The attack command's Attack (`FUN_141081a80` -> `FUN_140a7c210` with its unit's master's
    /// range `+0x828`, least range `+0x850` and sprites `+0x83c`, `+0x842`: no task and no
    /// `MoveTo` sprite) on `target` at `placed`, of the unit of `master` (the switched one) whose
    /// unit AI runs as `ai`. The target differs from the new Attack's none, so the unit's attack
    /// count (`+0x900`) clears, and the Attack takes it (`FUN_140a7a230`, slot 0x178 as `by`,
    /// through the view). It holds the master's sprites with the unit's look ([`arms_looking`]),
    /// and the range, the least range, the target's place and whether the target is the unit's
    /// player's as the two stand ([`Attack::own`]; the start reads the rest of their
    /// [`engagement`] anew). Its own choice of what it takes (an area-effect unit's annex target
    /// its main building, a class 60 none, one past state 6 not taken) is the task
    /// attack-constructor-past-state-6.
    ///
    /// # Errors
    ///
    /// A unit the map does not place, its master's charge range, and the take's.
    fn construct_attack(
        &mut self,
        target: ObjectId,
        placed: &Placed,
        master: &Master,
        ai: &UnitAi,
        by: Releaser,
        roles: &Roles,
    ) -> GenieResult<Attack> {
        let id = self.unit.id;
        self.step.world.parts.combat_mut(id)?.attack_count = AttackCount::ZERO;
        self.take(target, by, roles)?;
        let world = &self.step.world;
        let arms = arms_looking(
            master,
            world.parts.combat_of(id).abilities.look.as_ref(),
            &world.model.graphics,
            TaskSprites::NONE,
            None,
            roles,
        );
        let me_at = world.spatial.get(id).ok_or(ATTACKER_UNPLACED)?;
        let engagement = engagement(
            Some(ai),
            world.top_speed_with(self.player, self.unit, ai.current_target)?,
            master,
            me_at,
            placed,
            world.world_time,
            world.parts.combat_of(id).next_volley,
        )?;
        Ok(Attack::own(target, &engagement, arms))
    }

    /// The list's add of the attack command's new `attack` below state 3 (`FUN_140a977e0`): the
    /// list holds it at its head, and its slot 0x28 with 1 (`0x140a7f8c0`) starts it
    /// ([`UnitView::begin_added_attack`]) on the unit of `master` with its unit AI as it runs
    /// (`ai`). The started Attack goes back to the list, before an error too. Returns the
    /// notifies the start posted, in order.
    ///
    /// # Errors
    ///
    /// The start's.
    fn start_added_attack(
        &mut self,
        mut attack: Attack,
        master: &Master,
        ai: &UnitAi,
        roles: &Roles,
    ) -> GenieResult<Vec<Posted>> {
        self.actions.list.set_attack(Some(attack));
        let started = self.begin_added_attack(&mut attack, master, ai, roles);
        self.actions.list.set_attack(Some(attack));
        started
    }

    /// `RGE_Action_Attack`'s slot 0x28 with 1 (`0x140a7f8c0`) on the attack command's `attack`
    /// at the head of the list, as [`Unit::start_listed_attack`] runs it from the world, with the
    /// unit AI that runs (`ai`, the unit's `+0x6b0`: never null here). A unit inside an object
    /// (`+0x2f0`) fails it (`set_state(13)`, [`Attack::finish_in`]: speed 1, the standing
    /// sprite). Any other shows the sprite the Attack stands with (slot 0x68 with `+0x80`),
    /// then the start goes on ([`Attack::start_own`], which clears the attack count `+0x900`)
    /// on what it reads of the unit of `master` and of its target after the sprite: its top
    /// speed (slot 0x208), the target's sight ([`target_in_sight`]) and readyToAttack's reads
    /// ([`UnitView::ready_reads`]). The start reads only whether the top speed is 0, which no
    /// sprite changes (`FUN_14114d1d0` keeps the base for a factor of 0 or less).
    ///
    /// [`Unit::start_listed_attack`]: crate::objects::unit::Unit::start_listed_attack
    ///
    /// # Errors
    ///
    /// An Attack without a target or on one the world does not hold, a unit without a standing
    /// sprite or one the map does not place, and the start's.
    fn begin_added_attack(
        &mut self,
        attack: &mut Attack,
        master: &Master,
        ai: &UnitAi,
        roles: &Roles,
    ) -> GenieResult<Vec<Posted>> {
        let unit = &mut *self.unit;
        let world = &mut *self.step.world;
        let sprite_tuning = world.model.tuning.sprite;
        if unit.life.inside.is_some() {
            attack.finish_in(
                AttackState::Failed,
                unit,
                world.parts.motion_mut(unit.id),
                world.streams.get_mut(Stream::Sprite),
                sprite_tuning,
            )?;
            return Ok(Vec::new());
        }
        let standing = attack.arms.idle_sprite.ok_or(NO_STANDING_SPRITE)?;
        new_sprite(
            &mut unit.sprite,
            world.parts.move_stats_mut(unit.id),
            standing,
            world.streams.get_mut(Stream::Sprite),
            sprite_tuning,
        );
        let target = attack.target.ok_or(TARGET_GONE)?.id();
        let placed = *world.spatial.get_known(target)?.ok_or(TARGET_GONE)?;
        let me_at = world.spatial.get(unit.id).ok_or(ATTACKER_UNPLACED)?;
        let engagement = engagement(
            Some(ai),
            world.top_speed_with(self.player, unit, ai.current_target)?,
            master,
            me_at,
            &placed,
            world.world_time,
            world.parts.combat_of(unit.id).next_volley,
        )?;
        let target_master = world.master_tables().of_placed(target, &placed)?;
        let start = OwnStart {
            target,
            engagement: &engagement,
            visible: target_in_sight(
                &world.spatial,
                self.player,
                &placed,
                target_master.fog_visibility,
            )?,
            target_owner: placed.player,
            ready: self.ready_reads(target, master, ai, roles)?,
        };
        attack.start_own(self.step, self.unit, HeadStart::Added, &start, roles)
    }

    /// Slot 0x338's (`FUN_14120cea0`) own part for a trebuchet, of class 51 (packed) or 54
    /// (unpacked); none hands the unit over to `FUN_141081a80`, as for any other class. Under its
    /// Pack or Unpack (top action `0x7c`, `0x7d`; `FUN_140a986f0`) it answers false, under
    /// another action it hands over. Without one, a target the world does not hold
    /// (`FUN_1415417a0`) answers false; class 54 works the target at its place (slot 0x7e8 with
    /// the retargeting byte 0 and any task, [`AiCommand::WorkOn`] at once) and
    /// answers true; class 51 unpacks within its reach (`FUN_1410862c0`, [`placed_reach`]:
    /// `FUN_140ae58a0` through slot 0x808, [`UNPACK_INSTALL`]) and hands over out of it. A unit
    /// the load could not read whole may be under its Pack or Unpack ([`UNPACK_INSTALL`] too).
    fn trebuchet_attack(
        &mut self,
        target: ObjectId,
        master: &Master,
        ai: &mut UnitAi,
        rng: &mut Well512,
        roles: &Roles,
    ) -> GenieResult<Option<Attacked>> {
        let class = roles.of_class(master.class);
        if !class.intersects(ClassRoles::PACKED_TREBUCHET | ClassRoles::UNPACKED_TREBUCHET) {
            return Ok(None);
        }
        if self.step.world.parts.unread.contains(self.unit.id) {
            return Err(UNPACK_INSTALL);
        }
        match self.actions.top_action_kind() {
            Some(ActionKind::PACK | ActionKind::UNPACK) => return Ok(Some(Attacked::default())),
            Some(_) => return Ok(None),
            None => {}
        }
        let Some(placed) = self.step.world.spatial.get_known(target)?.copied() else {
            return Ok(Some(Attacked::default()));
        };
        if class.contains(ClassRoles::UNPACKED_TREBUCHET) {
            let work = AiCommand::WorkOn {
                target,
                point: placed.position,
            };
            self.effect_now(Effect::Command(work), ai, rng)?;
            return Ok(Some(Attacked {
                attacks: true,
                posted: Vec::new(),
            }));
        }
        let me_at = self
            .step
            .world
            .spatial
            .get(self.unit.id)
            .ok_or(ATTACKER_UNPLACED)?;
        if placed_reach(me_at, &placed, master).0 {
            return Err(UNPACK_INSTALL);
        }
        Ok(None)
    }

    /// The attack command's master: a villager switches to its player's villager of its task
    /// swap group first ([`UnitView::villager_swap`]; slot 0x90 with 0: the action list clears,
    /// then the master change at once, [`UnitView::switch_unit_master`]), else it keeps `own`;
    /// `ai_target` is the running unit AI's target.
    fn swapped_master(
        &mut self,
        own: Master,
        ai_target: Option<ObjectId>,
        roles: &Roles,
    ) -> GenieResult<Master> {
        let Some(next) = self.villager_swap(&own, roles).cloned() else {
            return Ok(own);
        };
        self.switch_unit_master(&own, next.id, ai_target)?;
        Ok(next)
    }

    /// The forced attack command on the target of the unit's own Attack (`FUN_141081a80`): the
    /// top Attack's slot 0x48 (`FUN_140a841d0`, [`crate::combat::attack::Attack::work`]) on the target
    /// and its place, with the main building of an annex for a unit of an area effect, the
    /// unit's top speed and canMoveToAttack ([`crate::unit_ai::Seeker::can_move`]); what it lets
    /// go and takes goes through the view at once ([`crate::world::lets::Lets`]), a new target
    /// clearing the unit's attack count (`+0x900`). Its answer is not read. `attack` is the
    /// copy of the top Attack on `target`, which goes back to the list before the releases.
    fn attack_again(
        &mut self,
        (target, mut attack): (ObjectId, Attack),
        target_placed: &Placed,
        master: &Master,
        ai: &UnitAi,
        roles: &Roles,
    ) -> GenieResult<()> {
        let mark = |id: ObjectId, placed: &Placed| -> GenieResult<Mark> {
            let unit = owner_view(
                &self.step.world.players,
                &self.step.world.index,
                self.step.player,
                self.new_owner,
            )?
            .get(id)
            .ok_or(NOT_HELD)?;
            let marked = self.step.world.master_tables().of_placed(id, placed)?;
            Ok(Mark {
                id,
                kind: marked.kind,
                class: marked.class,
                state: unit.life.state,
                position: placed.position,
                radius: placed.radius,
            })
        };
        let target_mark = mark(target, target_placed)?;
        let main = match owner_view(
            &self.step.world.players,
            &self.step.world.index,
            self.step.player,
            self.new_owner,
        )?
        .get(target)
        .and_then(|unit| self.step.world.parts.building_of(unit.id).annex_of)
        {
            Some(role_id) => match self.step.world.spatial.get(role_id).copied() {
                Some(placed) => Some(mark(role_id.id(), &placed)?),
                None => None,
            },
            None => None,
        };
        let speed = self
            .step
            .world
            .top_speed_with(self.player, self.unit, ai.current_target)?;
        let unit = &mut *self.unit;
        let wielder = Wielder {
            area_effect: master.combat_part().area_effect,
            speed,
            position: unit.position(),
            radius: master.radius,
        };
        let can_move = ai.current_order.is_some() || ai.stance <= UnitStance::Defensive;
        let worked = attack.work(Some(target_mark), main, &wielder, |_| can_move, roles);
        // the reads above reach no action of the unit's: the copy is the head
        self.actions.list.set_attack(Some(attack));
        let by = Releaser {
            id: unit.id,
            class: Some(master.class),
            player: self.player,
        };
        if worked.clears_count {
            self.step.world.parts.combat_mut(unit.id)?.attack_count = AttackCount::ZERO;
        }
        if let Some(old) = worked.lets_go {
            self.release(old, by)?;
        }
        if let Some(new) = worked.takes {
            self.take(new, by, roles)?;
        }
        Ok(())
    }

    /// What a hit of the unit reads of the object `id` ([`victim_in`]): one of the owner's live
    /// objects or another player's; None for an object the view does not hold (the owner's
    /// sleeping objects, or the old owner's of a unit taken over in its update, whose attributes
    /// it does not hold).
    fn victim_of_target(&self, id: ObjectId, roles: &Roles) -> GenieResult<Option<Victim>> {
        let view = owner_view(
            &self.step.world.players,
            &self.step.world.index,
            self.step.player,
            self.new_owner,
        )?;
        let Some((player_id, unit)) = view.find(id) else {
            return Ok(None);
        };
        let attributes = if view.own(id).is_some() {
            &self.step.world.players.player(self.player)?.attributes
        } else {
            let Some(owner) = view.players().player(player_id) else {
                return Ok(None);
            };
            &owner.attributes[..]
        };
        let master = unit.master(self.step.world.master_tables(), player_id)?;
        let dip = SideWorld {
            player: self.player,
            relations: &self.step.world.players.player(self.player)?.relations,
            foreign: view.players(),
            treaty: self.step.world.treaty.as_ref(),
            drop_sites: &self.step.world.model.drop_sites,
        };
        Ok(Some(victim_in(
            VictimView {
                unit,
                layers: VictimLayers::of(
                    &self.step.world.parts,
                    unit.id,
                    self.step.world.parts.actions.of(unit.id),
                ),
                master,
                charge_of: ChargeOf(&|with| self.step.world.special_charge(unit, with)),
                player: player_id,
                masters: player_masters(&self.step.world.model.masters, player_id)?,
                attributes,
                terrain: self.step.world.spatial.terrain.as_ref(),
                mode_680: self.step.world.mode_680,
            },
            self.player,
            &dip,
            &self.step.world.spatial,
            roles,
        )?))
    }

    /// What the start's `set_state(6)` of the unit's new Attack on `target` reads
    /// ([`crate::combat::attack::ReadyReads`]): the target as a hit reads it
    /// ([`UnitView::victim_of_target`]; None when the world does not hold it live), the unit as a
    /// combat object, its unit AI's stance, leash and queue ([`AttackReads::of`] with the AI that
    /// runs).
    fn ready_reads(
        &self,
        target: ObjectId,
        master: &Master,
        ai: &UnitAi,
        roles: &Roles,
    ) -> GenieResult<Option<ReadyReads>> {
        let Some(victim) = self.victim_of_target(target, roles)? else {
            return Ok(None);
        };
        let unit = &*self.unit;
        let own = victim_of(
            VictimView {
                unit,
                layers: VictimLayers::of(&self.step.world.parts, unit.id, &*self.actions),
                master,
                charge_of: ChargeOf(&|with| {
                    self.step
                        .world
                        .special_charge_with(unit, ai.current_target, with)
                }),
                player: self.player,
                masters: player_masters(&self.step.world.model.masters, self.player)?,
                attributes: &self.step.world.players.player(self.player)?.attributes,
                terrain: self.step.world.spatial.terrain.as_ref(),
                mode_680: self.step.world.mode_680,
            },
            roles,
        )?;
        let reads = AttackReads::of(
            AttackSource {
                combat: self.step.world.parts.combat_of(unit.id),
                top_speed: self.step.world.top_speed_terms_with(
                    self.player,
                    unit,
                    ai.current_target,
                )?,
                master,
                graphics: &self.step.world.model.graphics,
                ai: Some(ai),
                player_kind: self.player_kind,
                now: self.step.world.world_time,
            },
            roles,
            &self.step.world.model.tuning.turn,
        );
        Ok(Some(ReadyReads {
            reads,
            fight: Fight { victim, own },
        }))
    }

    /// The attack command's villager swap: a villager (class 4) switches to its player's villager
    /// of its task swap group (`+0x7b8` 1: the master table's `+0x298`, 83; else `+0x928`, 293)
    /// when the player has it.
    fn villager_swap(&self, master: &Master, roles: &Roles) -> Option<&Master> {
        if !roles.of_class(master.class).contains(ClassRoles::VILLAGER) {
            return None;
        }
        let swap = if master.action_part().task_by_group == TaskGroup::MALE_VILLAGERS {
            // The villagers a villager's attack command switches to by its task swap group (its player's
            // master table's entries at `+0x298` and `+0x928`).
            self.step.world.model.slots.master.villager_male
        } else {
            self.step.world.model.slots.master.villager_female
        };
        player_master(&self.step.world.model.masters, self.player, swap)
    }
}
