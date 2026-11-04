//! The unit's calls its unit AI makes through its view, each at once where the game's AI calls
//! the unit's slot: the attack command, the under-attack flag, an [`Effect`], the notify its
//! owner hears, the order slots' commands, the wait for a foundation and the stop of a soldier
//! in a group. The long answers live in their systems as `UnitView` methods (FD-041-D09): the
//! attack command in `combat/attack/unit_ai_side.rs` and the wait for a foundation in
//! `construction/wait_solidify.rs`.

use crate::math::rng::{Stream, Well512};
use crate::objects::ObjectId;
use crate::objects::unit::UnitFlags;
use crate::unit_ai::Answer;
use crate::unit_ai::OrderForce;
use crate::unit_ai::world_view::UnitView;
use crate::unit_ai::{Attacked, Effect, Notify, UNMODELLED_COMMAND, UnitAi, UnitCommand};
use crate::{GenieError, GenieResult};
use std::mem::{swap, take};

impl UnitView<'_, '_> {
    /// The unit's attack command on object `target` (slot 0x338, `FUN_14120cea0`), forced or
    /// not, while its unit AI stands as `ai`: true when the unit attacks it, with the notifies
    /// the command posts to the unit AI (kind and parameters), which the AI queues.
    ///
    /// # Errors
    ///
    /// What the command does not model.
    pub(crate) fn attack(
        &mut self,
        target: ObjectId,
        force: OrderForce,
        ai: &UnitAi,
    ) -> GenieResult<Attacked> {
        let roles = &self.c.w.model.roles.clone();
        self.start_attack_now(target, force, ai, roles)
    }

    /// The unit's `+0x685`, `underAttackValue`, becomes `value` (`FUN_14119d2d0`).
    pub(crate) fn set_under_attack(&mut self, value: bool) {
        self.unit.flags.set(UnitFlags::UNDER_ATTACK, value);
    }

    /// The unit's call its unit AI makes ([`Effect`]: its stop, a move, a command of its own), at
    /// once where the game's AI calls the unit's slot ([`Unit::run_effect`]). The call runs on
    /// the unit holding its AI as it stands (`ai`, which the call reads and changes as the
    /// game's does) and with the AI stream (`rng`), from which the routines it runs draw as the
    /// game's do. The notifies held for the AI are posted to it first, as the game posted them
    /// where they happened.
    ///
    /// # Errors
    ///
    /// What the call does not model.
    ///
    /// [`Unit::run_effect`]: crate::objects::unit::Unit::run_effect
    pub(crate) fn effect_now(
        &mut self,
        effect: Effect,
        ai: &mut UnitAi,
        rng: &mut Well512,
    ) -> GenieResult<()> {
        let roles = &self.c.w.model.roles.clone();
        let held = self.c.side_notifies.take().unwrap_or_default();
        let posted = held
            .into_iter()
            .try_for_each(|(notify, state)| ai.post(notify, state));
        self.c.w.return_unit_ai(self.unit.id, take(ai));
        swap(self.c.w.streams.get(Stream::Ai), rng);
        let class = self
            .unit
            .master(self.c.w.master_tables(), self.c.player)
            .map(|master| master.class);
        let done = class.and_then(|class| {
            posted.and_then(|()| {
                self.unit
                    .run_effect(self.actions, effect, Some(class), self.c, roles)
            })
        });
        swap(self.c.w.streams.get(Stream::Ai), rng);
        self.c.side_notifies = Some(Vec::new());
        *ai = self.c.w.lend_unit_ai(self.unit.id).ok_or_else(|| {
            GenieError::unreachable("a call of the unit AI took the unit's AI")
                .with("object", self.unit.id)
        })?;
        done
    }

    /// The unit AI hands the notify it handled to its owner (slot 0x48, then the owner's slot
    /// 0x2e8, whose hear logic a computer player's AI runs). A human player's owner does nothing
    /// the simulation reads.
    ///
    /// # Errors
    ///
    /// What the AI's hear logic does not model.
    pub(crate) fn owner_hears(&mut self, notify: Notify) -> GenieResult<()> {
        let roles = &self.c.w.model.roles.clone();
        self.hears(notify, roles)
    }

    /// A command of the unit's that the unit AI's order slots give ([`UnitCommand`]): taken when
    /// the unit takes it (the slot's 1).
    ///
    /// # Errors
    ///
    /// The commands the slice holds no action for.
    pub(crate) fn command(&mut self, command: UnitCommand) -> GenieResult<Answer> {
        let roles = &self.c.w.model.roles.clone();
        let slots = self.c.w.model.slots;
        match command {
            UnitCommand::Repair(target) => self.repair_command(target, slots, roles),
            UnitCommand::Enter(target) => self.enter_command(target, slots, roles),
            // slot 0x3a8 (FUN_14108d050): its Transport, made at once, answers 1
            UnitCommand::Unload {
                to,
                over_wall: false,
            } => {
                self.c.lend_at(
                    (&mut *self.unit, &mut *self.actions),
                    |world, player_id, list_index| world.transport_order(player_id, list_index, to),
                )?;
                Ok(Answer::Taken)
            }
            _ => Err(UNMODELLED_COMMAND),
        }
    }

    /// The unit's slot 0x388 (`FUN_141226fc0`) on object `target`: when the world holds it
    /// (`FUN_1415417a0`; the owner's sleeping list, which the unit does not see here, holds only
    /// objects of a type below 30, never a building) and the unit's slot 0x820 finds a task on
    /// it (any action type, `0xffff`, with the master switch:
    /// [`task_search`](crate::unit_ai::task_finder::world::task_search) on the target and its
    /// main building), a new `TRIBE_Action_Wait_Solidify` on that task
    /// ([`crate::construction::wait_solidify`]) replaces the unit's actions (`FUN_140a98620`,
    /// slot 0x808); taken then. The caller makes the wait after the update
    /// ([`Wait`](crate::unit_ai::world_view::Wait),
    /// [`World::wait_for_foundation`](crate::world::World::wait_for_foundation)).
    ///
    /// # Errors
    ///
    /// What the task search does not model.
    pub(crate) fn wait_solidify(&mut self, target: ObjectId) -> GenieResult<Answer> {
        let roles = &self.c.w.model.roles.clone();
        let slots = self.c.w.model.slots;
        self.start_wait_now(target, slots, roles)
    }

    /// The soldier's `AttackObject` (`FUN_140c17550`) after the attack, for a unit whose group
    /// the world holds: its future path data goes (slot 0x550, `FUN_14114d6a0`, which the slice
    /// holds none of), and a unit that moves (slot 0x5c8) or speeds up (slot 0x5a0, its velocity
    /// data's forward acceleration above 0, `COMISS`/`JBE`) stops (slot 0x5d0).
    ///
    /// # Errors
    ///
    /// What the stop does not model.
    pub(crate) fn group_attack_stop(&mut self) -> GenieResult<()> {
        let Some(mover) = self.c.w.parts.mover_mut(self.unit.id) else {
            return Ok(());
        };
        let speeding = mover
            .velocity
            .as_ref()
            .is_some_and(|velocity| 0.0 < velocity.acceleration.y());
        if mover.is_moving() || speeding {
            mover.stop()?;
        }
        Ok(())
    }
}
