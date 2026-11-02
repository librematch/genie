//! The idle part of the update (`0x140c66b10` after slot 73): slot 72's idle timer, the patrol
//! branch, class 3's and 52's making branch and the end.

use crate::GenieError;
use crate::math::rng::Well512;
use crate::objects::masters::ClassId;
use crate::objects::masters::class;
use crate::objects::masters::master_type::BUILDING;
use crate::players::PlayerId;
use crate::players::PlayerType;
use crate::unit_ai::ActionKind;
use crate::unit_ai::OrderKind;
use crate::unit_ai::{GroupState, SINGLE_VOLLEY, Surroundings, UnitAi, UnitSide, group_lets_idle};
use crate::world::Millis;

impl UnitAi {
    /// The idle part of the full update (`0x140c66b10` after slot 73): a unit without an action
    /// value (and without a target or not busy) runs slot 72 once its idle timer passes its
    /// timeout, unless it is a busy unit of class 20 or its group ([`group_lets_idle`]) holds it;
    /// else the patrol branch ([`UnitAi::patrol_idle_part`]), class 3's and 52's on actions
    /// `0x26b`/`0x26c` ([`UnitAi::making_idle_part`]) or the end ([`UnitAi::idle_part_end`]).
    /// `busy` and `group` are what the update read before its order queue.
    pub(super) fn idle_part(
        &mut self,
        busy: bool,
        group: Option<GroupState>,
        at: &Surroundings,
        player_type: PlayerType,
        ai: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<(), GenieError> {
        let class_20 = busy && at.class == Some(class::TRANSPORT);
        if !class_20
            && self.adjusted_idle_timeout < self.idle_timer
            && self.current_action.is_none()
            && (self.current_target.is_none() || !busy)
            && group_lets_idle(group)
        {
            // slot 0x240 with 1; the idle part reads no result
            self.idle_timer = Millis::ZERO;
            let _ = self.process_idle(at, player_type, ai, side)?;
            return Ok(());
        }
        if self.patrols(group) {
            return self.patrol_idle_part(at, player_type, ai, side);
        }
        // Classes whose actions `0x26b`, `0x26c` take another branch of the idle part (`-0xea9`,
        // `-0xe78` raw).
        // A building's master type (`'P'`, `FUN_1411942c0`).
        // The combat type (`0x46`), from which on the full update's tail reads a target's `+0x924`.
        if matches!(at.class, Some(class::BUILDING | class::TOWER))
            && matches!(
                self.current_action,
                Some(ActionKind::Make | ActionKind::Research)
            )
        {
            return self.making_idle_part(at, player_type, ai, side);
        }
        self.idle_part_end(at.class, at.player, group)
    }

    /// The idle part's patrol branch (`0x140c67f0b`): "I have a patrol order, checking to see if
    /// my secondary timer (%d) has expired." While it runs nothing more happens. Expired, it
    /// restarts at 750 (`PATROL_ATTACK_DELAY`; an adjusted idle timeout of exactly 750 is redrawn,
    /// slot 0x1a8) and processIdle runs (slot 0x240 with 1, [`UnitAi::process_idle`]) unless the
    /// unit's top action holds a building as its target (slot 400). The branch on the byte
    /// `DAT_143f01835` cannot run: nothing writes it.
    pub(super) fn patrol_idle_part(
        &mut self,
        at: &Surroundings,
        player_type: PlayerType,
        ai: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<(), GenieError> {
        if self.secondary_timer != Millis::ZERO {
            return Ok(());
        }
        self.secondary_timer = Millis::new(750);
        if self.adjusted_idle_timeout == Millis::new(750) {
            self.update_adjusted_idle_timeout(ai);
        }
        if Self::top_target_live(at, side)?.is_some_and(|t| t.kind == BUILDING) {
            return Ok(());
        }
        let _ = self.process_idle(at, player_type, ai, side)?;
        Ok(())
    }

    /// The idle part's patrol branch (`0x140c67f0b`): a patrol order (`0x2c7`, `0x2dd`), or a
    /// group that patrols ([`GroupState::patrol`]) off the hero power's order (`0x326`).
    pub(super) fn patrols(&self, group: Option<GroupState>) -> bool {
        matches!(
            self.current_order,
            Some(OrderKind::Patrol | OrderKind::AttackMove)
        ) || group.is_some_and(|g| g.patrol && self.current_order != Some(OrderKind::HeroPower))
    }

    /// The idle part past slot 72's test of a unit of class 3 or 52 on the make action (`0x26b`,
    /// `0x26c`): a building (`FUN_1411942c0`; `FUN_1411206a0` rules out classes 51 and 54) whose
    /// master has no missile (`+0x83c` below 0, `FUN_1410bfca0`) and whose firepower (`+0xa00`)
    /// is at most 1 (`DAT_14330eb8c`) forgets its target (`FUN_140c5e1e0`). With no target and its
    /// idle timer past its timeout, the timer restarts and processIdle runs (slot 0x240 with 1,
    /// [`UnitAi::process_idle`]); the idle timer is not reset otherwise.
    pub(super) fn making_idle_part(
        &mut self,
        at: &Surroundings,
        player_type: PlayerType,
        ai: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<(), GenieError> {
        if self.current_target.is_some()
            && at.kind == BUILDING
            && !at.ranged
            && at.firepower <= SINGLE_VOLLEY
        {
            self.remove_target();
        }
        if self.idle_timer <= self.adjusted_idle_timeout || self.current_target.is_some() {
            return Ok(());
        }
        self.idle_timer = Millis::ZERO;
        self.process_idle(at, player_type, ai, side).map(|_| ())
    }

    /// The idle part past slot 72: the patrol branch ([`UnitAi::patrols`]) and class 3's and
    /// 52's actions `0x26b`/`0x26c` (but in [`UnitAi::idle_part`], [`UnitAi::making_idle_part`])
    /// are not modelled. Action `0x2c0` (a member moved aside) ends, the last action taking it,
    /// unless the unit's group moves (`FUN_141046600`); an action or a target keeps the idle timer
    /// of a player above Gaia at 0.
    pub(super) fn idle_part_end(
        &mut self,
        class: Option<ClassId>,
        player: PlayerId,
        group: Option<GroupState>,
    ) -> Result<(), GenieError> {
        if self.patrols(group) {
            return Err(GenieError::unsupported("a patrol order's idle part"));
        }
        if matches!(class, Some(class::BUILDING | class::TOWER))
            && matches!(
                self.current_action,
                Some(ActionKind::Make | ActionKind::Research)
            )
        {
            return Err(GenieError::unsupported(
                "a unit of class 3 or 52 on action 0x26b/0x26c",
            ));
        }
        if self.current_action == Some(ActionKind::GroupAside) {
            if group.is_none_or(|g| !g.moving) {
                self.last_action = Some(ActionKind::GroupAside);
                self.current_action = None;
            }
            return Ok(());
        }
        if (self.current_action.is_some() || self.current_target.is_some())
            && player != PlayerId::GAIA
        {
            self.idle_timer = Millis::ZERO;
        }
        Ok(())
    }
}
