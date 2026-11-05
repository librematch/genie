//! The soldier's idle check (slot 72, `FUN_140c20db0`, factory type 13): its search and
//! attack, its end without one, and its `NoAttack` stance.

use crate::math::rng::Well512;
use crate::objects::ObjectId;
use crate::objects::masters::MasterKind;
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::players::game_status::GameStatus;
use crate::unit_ai::Answer;
use crate::unit_ai::IdleOutcome;
use crate::unit_ai::OrderHistory;
use crate::unit_ai::OrderKind;
use crate::unit_ai::SurroundingFlags;
use crate::unit_ai::TargetRules;
use crate::unit_ai::UnitStance;
use crate::unit_ai::world_view::UnitView;
use crate::unit_ai::{
    AttackAsk, Memory, Pick, SOLDIER_CARED, Search, Surroundings, UnitAi, attack_search_radius,
    computer_idle_check,
};
use crate::world::layer::MasterRoles;
use crate::world::slots::PlayerSlotsExt;
use crate::{GenieError, GenieResult};

impl UnitAi {
    /// The soldier's idle check (slot 72, `FUN_140c20db0`, factory type 13). Before its cap
    /// (`FUN_140c41e50`, [`UnitAi::cap_human_order`]), the unit of a player that lost (`+0x89` 2,
    /// `FUN_141663e60`) stops, and slot 0x250 (`FUN_140c5d9a0`) retries no order, the
    /// soldier's slot 0x60 taking none as retryable. A Gaia unit stops unless it is of the combat
    /// type with `+0x924` 0 outside the scenario modes (`FUN_141302540`); one in `NoAttack`
    /// (stance 3) searches and returns to its state position ([`UnitAi::no_attack_idle`]); master
    /// 706 stops. Its target is the target of its top action within its range (slot 400, slot
    /// 0x1e8) with retargeting on (`+0x1d6` 0), else its current target; neither is modelled, nor a
    /// computer's branches (patrol data, priorities, evasion). Without one, `bestUnitToAttack` from
    /// its place ([`Pick::Attack`]): an object found it attacks (slot 0xb8 forced,
    /// [`UnitAi::attack_object`]; then, with retargeting on and off order 0x2bd, the order goes)
    /// and processIdle produced ([`IdleOutcome::Acted`]); a patrol order's saved patrol data is not
    /// modelled. Else a patrol order stops ([`UnitAi::soldier_idle_end`]). Then, outside a unit
    /// group (`+0x6d0` −1), a unit in Defend or
    /// `StandGround` (stance 1, 2) walks back to its state position ([`UnitAi::walk_back`]), which
    /// leaves the result 5; a patrol path (`+0x1c0`) and the healer's ally search (`FUN_140c16f00`,
    /// for a master with a task of action type 0x69) are not modelled.
    pub(super) fn soldier_idle(
        &mut self,
        at: &Surroundings,
        player_kind: PlayerKind,
        ai: &mut Well512,
        side: &mut UnitView<'_, '_>,
    ) -> GenieResult<IdleOutcome> {
        if side.step.world.players.player(side.player)?.game_status == GameStatus::Lost {
            return Ok(IdleOutcome::Idle);
        }
        self.cap_human_order(side, ai)?;
        let searches = !at.flags.contains(SurroundingFlags::SCENARIO)
            && MasterKind::COMBAT <= at.kind
            && !at.capture.any();
        if at.player == PlayerId::GAIA && !searches {
            return Ok(IdleOutcome::Idle);
        }
        if self.stance == UnitStance::NoAttack {
            return self
                .no_attack_idle(at, player_kind, side, ai)
                .map(|()| IdleOutcome::Idle);
        }
        if at
            .roles
            .of_master(at.master)
            .contains(MasterRoles::IDLES_STILL)
        {
            return Ok(IdleOutcome::Idle);
        }
        // its target: with retargeting on, its top action's within its range (slot 400, slot
        // 0x1e8); with it off, its current target while the world holds it, else the top
        // action's
        let held =
            (self.retarget_disabled != 0 && at.target.is_some()).then_some(self.current_target);
        let target = match held {
            Some(object_id) => Some(object_id),
            None => Self::top_target_in_range(at, side)?.map(Some),
        };
        if player_kind.is_computer() {
            computer_idle_check()?;
        }
        let target = match target {
            Some(object_id) => object_id,
            None => self.soldier_best(at, player_kind, side)?,
        };
        if let Some(object_id) = target
            && self.soldier_attack(object_id, at, ai, side)? == Answer::Taken
        {
            return Ok(IdleOutcome::Acted);
        }
        self.soldier_idle_end(at, ai, side)
    }

    /// [`UnitAi::soldier_idle`]'s `bestUnitToAttack` without a target, from its place
    /// ([`Pick::Attack`] over [`SOLDIER_CARED`]): the object found, if any.
    ///
    /// # Errors
    ///
    /// What the search and its alert do not model.
    fn soldier_best(
        &mut self,
        at: &Surroundings,
        player_kind: PlayerKind,
        side: &mut UnitView<'_, '_>,
    ) -> GenieResult<Option<ObjectId>> {
        let unit_ai_tuning = &at.unit_ai_tuning;
        let found = side.closest(&Search {
            at: at.position.ground(),
            radius: attack_search_radius(at),
            classes: &SOLDIER_CARED,
            size: at.size,
            pick: Pick::Attack {
                lowest: if at.flags.contains(SurroundingFlags::SCENARIO) {
                    2
                } else {
                    0
                },
                seeker: self.seeker(at),
                ask: AttackAsk::IDLE,
            },
            memory: self.memory(),
            ai_target: self.current_target,
        })?;
        self.add_retries(&found);
        self.attack_search_alert(found, player_kind, unit_ai_tuning)?;
        Ok(found.id)
    }

    /// [`UnitAi::soldier_idle`]'s attack on `target`, a patrol order's patrol saved first ("I need
    /// to save the patrol information in case the attack works."): slot 0xb8 forced
    /// (`FUN_140c17550`: the base's, then the unit's group); once it attacks, with retargeting on
    /// and off order 0x2bd the order goes (`FUN_140c64b60` with −1, 0); a failed attack on a
    /// patrol order loses the saved patrol. [`Answer::Taken`] when it attacks.
    ///
    /// # Errors
    ///
    /// What the attack does not model.
    fn soldier_attack(
        &mut self,
        target: ObjectId,
        at: &Surroundings,
        ai: &mut Well512,
        side: &mut UnitView<'_, '_>,
    ) -> GenieResult<Answer> {
        if matches!(
            self.current_order,
            Some(OrderKind::Patrol | OrderKind::AttackMove)
        ) {
            self.save_patrol(side);
        }
        if self.attack_slot(target, TargetRules::Own, at, side, ai)? == Answer::Taken {
            if self.retarget_disabled == 0 && self.current_order != Some(OrderKind::Defend) {
                self.set_order_with(None, OrderHistory::Skipped, ai);
            }
            return Ok(Answer::Taken);
        }
        // "Attack failed, so I'm nuking the saved patrol information."
        if matches!(
            self.current_order,
            Some(OrderKind::Patrol | OrderKind::AttackMove)
        ) {
            self.zero_patrol();
        }
        Ok(Answer::NotTaken)
    }

    /// [`UnitAi::soldier_idle`]'s end without an attack: a patrol order stops there ("I have a
    /// patrol order, so I'm stopping here."). Else, with the alert mode on, its alert object
    /// takes its nearest armed enemy ([`UnitAi::watch_alert`]); a unit in a group (its `+0x6d0`, the
    /// field, not whether the world holds the group: "And I'm in a formation, returning no
    /// action."); a unit in Defend or `StandGround` walks back to its state position
    /// ("returning to state pos.", its result not the processIdle's); then the saved patrol, if
    /// any, goes on (`restorePatrolInformation`: processIdle produced). Else
    /// [`IdleOutcome::Idle`].
    ///
    /// # Errors
    ///
    /// The healer's search for an ally (a master with a task of action type 0x69), and what the
    /// walk and the patrol do not model.
    fn soldier_idle_end(
        &mut self,
        at: &Surroundings,
        ai: &mut Well512,
        side: &mut UnitView<'_, '_>,
    ) -> GenieResult<IdleOutcome> {
        if matches!(
            self.current_order,
            Some(OrderKind::Patrol | OrderKind::AttackMove)
        ) {
            return Ok(IdleOutcome::Idle);
        }
        self.watch_alert(side)?;
        if side.unit.group.id.is_some() {
            return Ok(IdleOutcome::Idle);
        }
        if self.stance.leashed() {
            let _ = self.walk_back(at, side, ai)?;
        }
        if self.restore_patrol(at, ai, side)? == IdleOutcome::Acted {
            return Ok(IdleOutcome::Acted);
        }
        if at.flags.contains(SurroundingFlags::TASK_0X69) {
            return Err(GenieError::unsupported(
                "a healer's search for an ally to heal",
            ));
        }
        Ok(IdleOutcome::Idle)
    }

    /// The soldier's idle check in `NoAttack` (stance 3, `FUN_140c20db0`: "searching and returning
    /// to state pos."): the search alone from its place (slot 0x50, `FUN_140c63e20` with mode 1,
    /// [`Pick::Look`]) sets the alert mode to whether it found enemies, their sighting time
    /// restarting ([`UnitAi::attack_search_alert`]); then, but on the patrol orders, it walks
    /// back to its state position ([`UnitAi::walk_back`], its result unread); with the alert mode
    /// on, its alert object takes its nearest armed enemy ([`UnitAi::watch_alert`]); last the healer's ally
    /// search (`FUN_140c16f00`). Not modelled: a defend order's search (from its target's
    /// objects), a computer's cared classes (`+0x190` with 10) and the healer's search.
    pub(super) fn no_attack_idle(
        &mut self,
        at: &Surroundings,
        player_kind: PlayerKind,
        side: &mut UnitView<'_, '_>,
        rng: &mut Well512,
    ) -> GenieResult<()> {
        if self.current_order == Some(OrderKind::Defend) {
            return Err(GenieError::unsupported(
                "a defend order's search (its target's objects)",
            ));
        }
        if player_kind.is_computer() {
            return Err(GenieError::unsupported(
                "a computer player's soldier's NoAttack search",
            ));
        }
        let found = side.closest(&Search {
            at: at.position.ground(),
            radius: attack_search_radius(at),
            classes: &SOLDIER_CARED,
            size: at.size,
            pick: Pick::Look,
            memory: Memory::default(),
            ai_target: self.current_target,
        })?;
        self.attack_search_alert(found, player_kind, &at.unit_ai_tuning)?;
        if !matches!(
            self.current_order,
            Some(OrderKind::Patrol | OrderKind::AttackMove)
        ) {
            let _ = self.walk_back(at, side, rng)?;
        }
        self.watch_alert(side)?;
        if at.flags.contains(SurroundingFlags::TASK_0X69) {
            return Err(GenieError::unsupported(
                "a healer's search for an ally to heal",
            ));
        }
        Ok(())
    }
}
