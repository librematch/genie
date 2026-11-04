//! The attack the unit AI commands through its view while it runs ([`UnitView::attack`]): the
//! new Attack, a forced attack again on the unit's own target, the
//! target as a hit reads it, the start's readiness, the target's sight and a villager's swap.

use crate::combat::attack::{
    Attack, AttackCommand, Mark, ReadyReads, Wielder, arms_looking, engagement, speed_of,
};
use crate::combat::attack::{AttackReads, Fight};
use crate::combat::damage::Victim;
use crate::combat::victims::{victim_in, victim_of};
use crate::combat::{AttackCount, AttackTargetId};
use crate::map::Tile;
use crate::map::spatial::Placed;
use crate::math::rng::Stream;
use crate::movement::Speed;
use crate::objects::ObjectId;
use crate::objects::masters::ClassId;
use crate::objects::masters::FogVisibility;
use crate::objects::masters::PlayersMastersExt;
use crate::objects::masters::TaskGroup;
use crate::objects::masters::TaskSprites;
use crate::objects::masters::player_master;
use crate::objects::masters::{Master, OwnerMasters};
use crate::objects::release::Releaser;
use crate::objects::sprite::new_sprite;
use crate::unit_ai::Attacked;
use crate::unit_ai::OrderForce;
use crate::unit_ai::UnitAi;
use crate::unit_ai::UnitStance;
use crate::unit_ai::task_finder::world::SideWorld;
use crate::unit_ai::world_view::owner_view;
use crate::unit_ai::world_view::{NOT_HELD, UnitView};
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::slots::PlayerSlotsExt;
use crate::world::{ATTACKER_UNPLACED, NO_STANDING_SPRITE};
use crate::{GenieError, GenieResult};

impl UnitView<'_, '_> {
    /// The attack command (slot 0x338, `FUN_14120cea0`; classes 51 and 54 are not modelled) ->
    /// `FUN_141081a80`: a target the world does not hold fails; `+0x648` takes the world's time
    /// (not held); the unit's own Attack on the target answers true, forced after its slot 0x48
    /// on it ([`UnitView::attack_again`]); a villager's master switch (slot 0x90) is not modelled.
    /// Then the new
    /// Attack ([`crate::combat::attack::Attack::command`]): the constructor takes its target (slot 0x178,
    /// through the view) and clears the unit's `+0x900` (its target is new), the action list
    /// clears ([`UnitView::attack_clears_actions`]), and the install's start (`0x140a7f8c0`) of a unit inside
    /// another fails it at once (`set_state(13)`, [`crate::combat::attack::Attack::fail_at_start`]); else
    /// it puts the unit on its standing sprite (slot 0x68 with `+0x80`) and runs the range test
    /// ([`crate::combat::attack::Attack::begin`]): readyToAttack's failure
    /// before a `set_state(4)` leaves the animation at speed 1; a `MoveTo` that starts arrived
    /// shows the standing sprite; a unit that cannot move in reach goes ready (`set_state(6)`,
    /// [`crate::combat::attack::Attack::enter_ready`]: readyToAttack again, then the swing, `set_state(7)`,
    /// when it faces its target and may attack), whose notifies join the start's. A notify posted
    /// while the AI runs (its buffer, `FUN_140c55270`) is not modelled.
    pub(crate) fn start_attack_now(
        &mut self,
        target: ObjectId,
        force: OrderForce,
        ai: &UnitAi,
        roles: &Roles,
    ) -> GenieResult<Attacked> {
        // the unit's master, a copy: the attack's start changes the unit (its master's switch)
        let own = self
            .unit
            .master(self.step.world.master_tables(), self.player)?
            .clone();
        let master = &own;
        if roles
            .of_class(master.class)
            .intersects(ClassRoles::PACKED_TREBUCHET | ClassRoles::UNPACKED_TREBUCHET)
        {
            return Err(GenieError::unsupported(
                "the attack command of a unit of class 51 or 54",
            ));
        }
        let no = Attacked::default;
        let Some(placed) = self.step.world.spatial.get_known(target)?.copied() else {
            return Ok(no());
        };
        if let Some(&attack) =
            self.actions.list.attack().filter(|target_attack| {
                target_attack.target.map(AttackTargetId::id) == Some(target)
            })
        {
            if force == OrderForce::Forced {
                self.attack_again((target, attack), &placed, master, ai, roles)?;
            }
            return Ok(Attacked {
                attacks: true,
                posted: Vec::new(),
            });
        }
        let master = &self.swapped_master(own, roles)?;
        let me_at = *self
            .step
            .world
            .spatial
            .get(self.unit.id)
            .ok_or(ATTACKER_UNPLACED)?;
        let target_master =
            player_master(&self.step.world.model.masters, placed.player, placed.master).ok_or(
                GenieError::unsupported("an attack's target of a master the slice does not have"),
            )?;
        let visible = self.target_visible(&placed, target_master)?;
        let unit = &mut *self.unit;
        // the attack command's Attack (`FUN_141081a80`) takes no MoveTo sprite: the unit's
        let look = self
            .step
            .world
            .parts
            .combat_of(unit.id)
            .abilities
            .look
            .as_ref();
        let arms = arms_looking(
            master,
            look,
            &self.step.world.model.graphics,
            TaskSprites::NONE,
            None,
            roles,
        );
        let standing = arms.idle_sprite.ok_or(NO_STANDING_SPRITE)?;
        if unit.sprite.is_none() {
            return Err(GenieError::unsupported(
                "an attack command of a unit whose sprite the slice does not hold",
            ));
        }
        let by = self.attack_clears_actions(master.class)?;
        let unit = &mut *self.unit;
        let speed = speed_of(self.step.world.parts.mover_of(unit.id))?;
        let engagement = engagement(
            Some(ai),
            speed,
            master,
            &me_at,
            &placed,
            self.step.world.world_time,
        )?;
        // the constructor clears +0x900 for a target other than its own (none yet)
        self.step.world.parts.combat_mut(unit.id)?.attack_count = AttackCount::ZERO;
        if unit.life.inside.is_some() {
            return self.attack_from_inside(Attack::own(target, &engagement, arms), by, roles);
        }
        new_sprite(
            &mut unit.sprite,
            self.step.world.parts.move_stats_mut(unit.id),
            standing,
            self.step.world.streams.get_mut(Stream::Sprite),
            self.step.world.model.tuning.sprite,
        );
        let cmd = AttackCommand {
            target,
            engagement: &engagement,
            arms,
            visible,
            target_owner: placed.player,
            ready: self.ready_reads(target, master, ai, roles)?,
        };
        let unit = &mut *self.unit;
        let (attack, posted) = Attack::command(self.step, unit, &cmd, roles)?;
        self.actions.list.set_attack(Some(attack));
        self.take(target, by, roles)?;
        Ok(Attacked {
            attacks: true,
            posted,
        })
    }

    /// The attack command's master: a villager switches to its player's villager of its task
    /// swap group first ([`UnitView::villager_swap`]; slot 0x90 with 0: the action list clears,
    /// then the master change at once, [`UnitView::switch_unit_master`]), else it keeps `own`.
    fn swapped_master(&mut self, own: Master, roles: &Roles) -> GenieResult<Master> {
        let Some(next) = self.villager_swap(&own, roles).cloned() else {
            return Ok(own);
        };
        self.switch_unit_master(&own, next.id)?;
        Ok(next)
    }

    /// The install's start (`0x140a7f8c0`) of the Attack of a unit inside another: it fails at
    /// once (`set_state(13)`, [`crate::combat::attack::Attack::fail_at_start`]) and holds its
    /// target, which it takes (slot 0x178, as `by`).
    fn attack_from_inside(
        &mut self,
        mut attack: Attack,
        by: Releaser,
        roles: &Roles,
    ) -> GenieResult<Attacked> {
        let unit = &mut *self.unit;
        attack.fail_at_start(
            &mut unit.sprite,
            self.step.world.parts.motion_mut(unit.id),
            self.step.world.streams.get_mut(Stream::Sprite),
            self.step.world.model.tuning.sprite,
        )?;
        let target = attack.target;
        self.actions.list.set_attack(Some(attack));
        if let Some(target) = target {
            self.take(target.id(), by, roles)?;
        }
        Ok(Attacked {
            attacks: true,
            posted: Vec::new(),
        })
    }

    /// The action list's clearing before the attack command's Attack (`FUN_140a98620`): an
    /// Attack on another target tears down (its `MoveTo`'s stops the unit) and lets its targets
    /// go (slot 0x180) through the view, as do the others. The releaser, the unit as master
    /// class `class`.
    fn attack_clears_actions(&mut self, class: ClassId) -> GenieResult<Releaser> {
        let by = Releaser {
            id: self.unit.id,
            class: Some(class),
            player: self.player,
        };
        let (mut releases, mut leaves) = (Vec::new(), Vec::new());
        self.unit.tear_down_actions(
            (self.actions, self.step.world.parts.unread.get(self.unit.id)),
            self.step.world.parts.motion.layer_mut(self.unit.id),
            &mut releases,
            &mut leaves,
            by,
        )?;
        self.torn_down((leaves, releases))?;
        Ok(by)
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
            let player_master =
                player_master(&self.step.world.model.masters, placed.player, placed.master)
                    .ok_or(NOT_HELD)?;
            Ok(Mark {
                id,
                kind: player_master.kind,
                class: player_master.class,
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
        let unit = &mut *self.unit;
        let wielder = Wielder {
            area_effect: master.combat_part().area_effect,
            speed: self
                .step
                .world
                .parts
                .mover_of(unit.id)
                .map_or(Speed::ZERO, |mover| mover.stats.speed),
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
            drop_sites: &self.step.world.model.drop_sites,
        };
        Ok(Some(victim_in(
            (
                unit,
                self.step.world.parts.unit_ai.contains(unit.id),
                self.step.world.parts.building_of(unit.id),
                self.step.world.parts.combat_of(unit.id),
                self.step.world.parts.actions.of(unit.id),
                self.step.world.parts.unread.get(unit.id),
            ),
            (player_id, master, attributes),
            (self.player, &dip),
            &self.step.world.spatial,
            self.step.world.mode_680,
            roles,
        )))
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
            (
                unit,
                self.step.world.parts.unit_ai.contains(unit.id),
                self.step.world.parts.building_of(unit.id),
                self.step.world.parts.combat_of(unit.id),
                &*self.actions,
                self.step.world.parts.unread.get(unit.id),
            ),
            master,
            &self.step.world.players.player(self.player)?.attributes,
            self.player,
            self.step.world.spatial.terrain.as_ref(),
            self.step.world.mode_680,
            roles,
        );
        let reads = AttackReads::of(
            self.step.world.parts.combat_of(unit.id),
            master,
            (
                OwnerMasters::new(
                    &self.step.world.parts.own_master,
                    self.step.world.model.masters.of(self.player),
                ),
                &self.step.world.model.graphics,
            ),
            Some(ai),
            (self.player_kind, self.step.world.world_time),
            roles,
            &self.step.world.model.tuning.turn,
        );
        Ok(Some(ReadyReads {
            reads,
            fight: Fight { victim, own },
        }))
    }

    /// The attack command's sight of its target `placed` (of master `target_master`): its tile seen
    /// by the unit's player, or explored when the master shows in the fog (its fog flag).
    ///
    /// # Errors
    ///
    /// A tile the map's sight does not hold.
    fn target_visible(&self, placed: &Placed, target_master: &Master) -> GenieResult<bool> {
        let (tx, ty) = (placed.position.x() as i32, placed.position.y() as i32);
        Ok(self
            .step
            .world
            .spatial
            .sees_tile(self.player, Tile::new(tx, ty))
            || (target_master.fog_visibility != FogVisibility::INVISIBLE
                && self
                    .step
                    .world
                    .spatial
                    .explored(self.player, Tile::new(tx, ty))?))
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
