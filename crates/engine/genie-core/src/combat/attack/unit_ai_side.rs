//! The attack the unit AI commands while it runs ([`crate::unit_ai::UnitSide::attack`] through
//! its view, [`UnitView`]): the new Attack, a forced attack again on the unit's own target, the
//! target as a hit reads it, the start's readiness, the target's sight and a villager's swap.

use crate::GenieError;
use crate::combat::attack::{AttackReads, Fight};
use crate::combat::damage::Victim;
use crate::combat::victims::{victim_in, victim_of};
use crate::map::Tile;
use crate::map::spatial::Placed;
use crate::math::rng::Stream;
use crate::movement::Speed;
use crate::objects::ObjectId;
use crate::objects::masters::ClassId;
use crate::objects::masters::FogVisibility;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::TaskGroup;
use crate::objects::masters::TaskSprites;
use crate::objects::masters::class;
use crate::objects::masters::ids::VILLAGER_FEMALE;
use crate::objects::masters::ids::VILLAGER_MALE;
use crate::objects::masters::player_master;
use crate::objects::release::Releaser;
use crate::unit_ai::UnitAi;
use crate::unit_ai::UnitStance;
use crate::unit_ai::task_finder::world::SideWorld;
use crate::unit_ai::world_view::owner_view;
use crate::unit_ai::world_view::{NOT_HELD, UnitView};
use crate::world::slots::PlayerSlots;

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
        force: bool,
        ai: &UnitAi,
    ) -> Result<crate::unit_ai::Attacked, GenieError> {
        // the unit's master, a copy: the attack's start changes the unit (its master's switch)
        let own = self
            .unit
            .master(self.c.w.master_tables(), self.player)?
            .clone();
        let master = &own;
        if matches!(master.class, class::CLASS_51 | class::CLASS_54) {
            return Err(GenieError::unsupported(
                "the attack command of a unit of class 51 or 54",
            ));
        }
        let no = crate::unit_ai::Attacked::default;
        let Some(t) = self.c.w.spatial.get_known(target)?.copied() else {
            return Ok(no());
        };
        if self
            .actions
            .list
            .attack()
            .is_some_and(|a| a.target.map(crate::combat::AttackTargetId::id) == Some(target))
        {
            if force {
                self.attack_again(target, &t, master, ai)?;
            }
            return Ok(crate::unit_ai::Attacked {
                attacks: true,
                posted: Vec::new(),
            });
        }
        let master = &self.swapped_master(own)?;
        let me_at = *self
            .c
            .w
            .spatial
            .get(self.unit.id)
            .ok_or(crate::world::ATTACKER_UNPLACED)?;
        let target_master = player_master(&self.c.w.masters, t.player, t.master).ok_or(
            GenieError::unsupported("an attack's target of a master the slice does not have"),
        )?;
        let visible = self.target_visible(&t, target_master)?;
        let unit = &mut *self.unit;
        // the attack command's Attack (`FUN_141081a80`) takes no MoveTo sprite: the unit's
        let look = self.c.w.parts.combat_of(unit.id).abilities.look.as_ref();
        let arms = crate::combat::attack::arms_looking(
            master,
            look,
            &self.c.w.graphics,
            TaskSprites::NONE,
            None,
        );
        let standing = arms.idle_sprite.ok_or(crate::world::NO_STANDING_SPRITE)?;
        if unit.sprite.is_none() {
            return Err(GenieError::unsupported(
                "an attack command of a unit whose sprite the slice does not hold",
            ));
        }
        let by = self.attack_clears_actions(master.class)?;
        let unit = &mut *self.unit;
        let speed = crate::combat::attack::speed_of(self.c.w.parts.mover_of(unit.id))?;
        let e = crate::combat::attack::engagement(
            Some(ai),
            speed,
            master,
            &me_at,
            &t,
            self.c.w.world_time,
        )?;
        // the constructor clears +0x900 for a target other than its own (none yet)
        self.c.w.parts.combat_mut(unit.id)?.attack_count = crate::combat::AttackCount::ZERO;
        if unit.life.inside.is_some() {
            return self
                .attack_from_inside(crate::combat::attack::Attack::own(target, &e, arms), by);
        }
        crate::objects::sprite::new_sprite(
            &mut unit.sprite,
            self.c.w.parts.move_stats_mut(unit.id),
            standing,
            self.c.w.streams.get(Stream::Sprite),
        );
        let cmd = crate::combat::attack::AttackCommand {
            target,
            engagement: &e,
            arms,
            visible,
            target_owner: t.player,
            ready: self.ready_reads(target, master, ai)?,
        };
        let unit = &mut *self.unit;
        let (attack, posted) = crate::combat::attack::Attack::command(self.c, unit, &cmd)?;
        self.actions.list.set_attack(Some(attack));
        self.take(target, by)?;
        Ok(crate::unit_ai::Attacked {
            attacks: true,
            posted,
        })
    }

    /// The attack command's master: a villager switches to its player's villager of its task
    /// swap group first ([`UnitView::villager_swap`]; slot 0x90 with 0: the action list clears,
    /// then the master change at once, [`UnitView::switch_unit_master`]), else it keeps `own`.
    fn swapped_master(
        &mut self,
        own: crate::objects::masters::Master,
    ) -> Result<crate::objects::masters::Master, GenieError> {
        let Some(next) = self.villager_swap(&own).cloned() else {
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
        mut attack: crate::combat::attack::Attack,
        by: Releaser,
    ) -> Result<crate::unit_ai::Attacked, GenieError> {
        let unit = &mut *self.unit;
        attack.fail_at_start(
            &mut unit.sprite,
            self.c.w.parts.motion_mut(unit.id),
            self.c.w.streams.get(Stream::Sprite),
        )?;
        let target = attack.target;
        self.actions.list.set_attack(Some(attack));
        if let Some(target) = target {
            self.take(target.id(), by)?;
        }
        Ok(crate::unit_ai::Attacked {
            attacks: true,
            posted: Vec::new(),
        })
    }

    /// The action list's clearing before the attack command's Attack (`FUN_140a98620`): an
    /// Attack on another target tears down (its `MoveTo`'s stops the unit) and lets its targets
    /// go (slot 0x180) through the view, as do the others. The releaser, the unit as master
    /// class `class`.
    fn attack_clears_actions(&mut self, class: ClassId) -> Result<Releaser, GenieError> {
        let by = Releaser {
            id: self.unit.id,
            class: Some(class),
            player: self.player,
        };
        let (mut releases, mut leaves) = (Vec::new(), Vec::new());
        self.unit.tear_down_actions(
            (self.actions, self.c.w.parts.unread.get(self.unit.id)),
            self.c.w.parts.motion.layer_mut(self.unit.id),
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
    /// clearing the unit's attack count (`+0x900`). Its answer is not read.
    fn attack_again(
        &mut self,
        target: ObjectId,
        t: &Placed,
        master: &crate::objects::masters::Master,
        ai: &UnitAi,
    ) -> Result<(), GenieError> {
        let mark = |id: ObjectId, p: &Placed| -> Result<crate::combat::attack::Mark, GenieError> {
            let u = owner_view(
                &self.c.w.players,
                &self.c.w.index,
                self.c.player,
                self.new_owner,
            )?
            .get(id)
            .ok_or(NOT_HELD)?;
            let m = player_master(&self.c.w.masters, p.player, p.master).ok_or(NOT_HELD)?;
            Ok(crate::combat::attack::Mark {
                id,
                kind: m.kind,
                class: m.class,
                state: u.life.state,
                position: p.position,
                radius: p.radius,
            })
        };
        let target_mark = mark(target, t)?;
        let main = match owner_view(
            &self.c.w.players,
            &self.c.w.index,
            self.c.player,
            self.new_owner,
        )?
        .get(target)
        .and_then(|u| self.c.w.parts.building_of(u.id).annex_of)
        {
            Some(m) => match self.c.w.spatial.get(m).copied() {
                Some(p) => Some(mark(m.id(), &p)?),
                None => None,
            },
            None => None,
        };
        let unit = &mut *self.unit;
        let wielder = crate::combat::attack::Wielder {
            area_effect: master.area_effect,
            speed: self
                .c
                .w
                .parts
                .mover_of(unit.id)
                .map_or(Speed::ZERO, |m| m.stats.speed),
            position: unit.position(),
            radius: master.radius,
        };
        let can_move = ai.current_order.is_some() || ai.stance <= UnitStance::DEFENSIVE;
        let attack = self
            .actions
            .list
            .attack_mut()
            .ok_or(crate::combat::attack::unit::NO_ATTACK)?;
        let w = attack.work(Some(target_mark), main, &wielder, |_| can_move);
        let by = Releaser {
            id: unit.id,
            class: Some(master.class),
            player: self.player,
        };
        if w.clears_count {
            self.c.w.parts.combat_mut(unit.id)?.attack_count = crate::combat::AttackCount::ZERO;
        }
        if let Some(old) = w.lets_go {
            self.release(old, by)?;
        }
        if let Some(new) = w.takes {
            self.take(new, by)?;
        }
        Ok(())
    }

    /// What a hit of the unit reads of the object `id` ([`victim_in`]): one of the owner's live
    /// objects or another player's; None for an object the view does not hold (the owner's
    /// sleeping objects, or the old owner's of a unit taken over in its update, whose attributes
    /// it does not hold).
    fn victim_of_target(&self, id: ObjectId) -> Result<Option<Victim>, GenieError> {
        let view = owner_view(
            &self.c.w.players,
            &self.c.w.index,
            self.c.player,
            self.new_owner,
        )?;
        let Some((q, t)) = view.find(id) else {
            return Ok(None);
        };
        let attributes = if view.own(id).is_some() {
            &self.c.w.players.player(self.player)?.attributes
        } else {
            let Some(owner) = view.players().player(q) else {
                return Ok(None);
            };
            &owner.attributes[..]
        };
        let master = t.master(self.c.w.master_tables(), q)?;
        let dip = SideWorld {
            player: self.player,
            relations: &self.c.w.players.player(self.player)?.relations,
            foreign: view.players(),
            drop_sites: &self.c.w.drop_sites,
        };
        Ok(Some(victim_in(
            (
                t,
                self.c.w.parts.unit_ai.contains(t.id),
                self.c.w.parts.building_of(t.id),
                self.c.w.parts.combat_of(t.id),
                self.c.w.parts.actions.of(t.id),
                self.c.w.parts.unread.get(t.id),
            ),
            (q, master, attributes),
            (self.player, &dip),
            &self.c.w.spatial,
            self.c.w.mode_680,
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
        master: &crate::objects::masters::Master,
        ai: &UnitAi,
    ) -> Result<Option<crate::combat::attack::ReadyReads>, GenieError> {
        let Some(victim) = self.victim_of_target(target)? else {
            return Ok(None);
        };
        let unit = &*self.unit;
        let own = victim_of(
            (
                unit,
                self.c.w.parts.unit_ai.contains(unit.id),
                self.c.w.parts.building_of(unit.id),
                self.c.w.parts.combat_of(unit.id),
                &*self.actions,
                self.c.w.parts.unread.get(unit.id),
            ),
            master,
            &self.c.w.players.player(self.player)?.attributes,
            self.player,
            self.c.w.spatial.terrain.as_ref(),
            self.c.w.mode_680,
        );
        let reads = AttackReads::of(
            self.c.w.parts.combat_of(unit.id),
            master,
            (
                crate::objects::masters::OwnerMasters::new(
                    &self.c.w.parts.own_master,
                    self.c.w.masters.of(self.player),
                ),
                &self.c.w.graphics,
            ),
            Some(ai),
            (self.computer, self.c.w.world_time),
        );
        Ok(Some(crate::combat::attack::ReadyReads {
            reads,
            fight: Fight { victim, own },
        }))
    }

    /// The attack command's sight of its target `t` (of master `target_master`): its tile seen by the
    /// unit's player, or explored when the master shows in the fog (its fog flag).
    ///
    /// # Errors
    ///
    /// A tile the map's sight does not hold.
    fn target_visible(
        &self,
        t: &Placed,
        target_master: &crate::objects::masters::Master,
    ) -> Result<bool, GenieError> {
        let (tx, ty) = (t.position.x() as i32, t.position.y() as i32);
        Ok(self.c.w.spatial.sees_tile(self.player, Tile::new(tx, ty))
            || (target_master.fog_visibility != FogVisibility::INVISIBLE
                && self.c.w.spatial.explored(self.player, Tile::new(tx, ty))?))
    }

    /// The attack command's villager swap: a villager (class 4) switches to its player's villager
    /// of its task swap group (`+0x7b8` 1: the master table's `+0x298`, 83; else `+0x928`, 293)
    /// when the player has it.
    fn villager_swap(
        &self,
        master: &crate::objects::masters::Master,
    ) -> Option<&crate::objects::masters::Master> {
        if master.class != class::VILLAGER {
            return None;
        }
        let swap = if master.task_by_group == TaskGroup::MALE_VILLAGERS {
            // The villagers a villager's attack command switches to by its task swap group (its player's
            // master table's entries at `+0x298` and `+0x928`).
            VILLAGER_MALE
        } else {
            VILLAGER_FEMALE
        };
        player_master(&self.c.w.masters, self.player, swap)
    }
}
