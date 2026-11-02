//! The actions' outcomes, the stops and the order queue.

use crate::GenieError;
use crate::map::Distance;
use crate::map::Point;
use crate::map::Position;
use crate::math::fmath::below_or_unordered;
use crate::math::rng::Well512;
use crate::objects::ObjectState;
use crate::objects::masters::ClassId;
use crate::orders::Dispatch;
use crate::players::PlayerType;
use crate::unit_ai::AiAction;
use crate::unit_ai::OrderKind;
use crate::unit_ai::OrderOutcome;
use crate::unit_ai::OrderPriority;
use crate::unit_ai::QueueEnd;
use crate::unit_ai::StopOrder;
use crate::unit_ai::UnitStance;
use crate::unit_ai::{
    AiMove, ENDED_ACTIONS, Effect, GO_ON, Notify, PRODUCED, QueuedOrder, Surroundings, UnitAi,
    UnitSide, WALK_BACK, point_gap_squared,
};
use crate::world::Millis;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::slots::CheckedIndex;

impl UnitAi {
    /// The base handler's case `0x1f9` ("Action %d has failed.", `FUN_140c589c0`): with an order
    /// the state position takes the unit's place first; on the scout order (`0x2da`) the
    /// player's slot 0x2e8 with `0x211` is not modelled. A failed attack (600) on the attack order
    /// (700) or with the attack action current runs processIdle (slot 0x240 with 0,
    /// [`UnitAi::process_idle`]): an action it produced breaks off (0xd), none makes a human
    /// player's unit forget its target and stop (slot 0xb0 with 1). Without an
    /// order, the failure of the current action forgets the target (`FUN_140c5e1e0`) and stops
    /// it (slot 0xb0 with 1; slot 0x48 hands the notify to the owner, not held); with one, so
    /// does the failure of the order's own action (action + 100 = order); on order `0x2d4` a
    /// failed `0x262` forgets the target and ends the order too (slot 0xb0 with 0), on `0x2bd`
    /// the action value goes (the last action takes it); any other failed attack forgets the
    /// target and stops; the rest go on.
    pub(super) fn action_failed(
        &mut self,
        n: Notify,
        at: &Surroundings,
        player_type: PlayerType,
        ai: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<u32, GenieError> {
        let (order, action, p0) = (self.current_order, self.current_action, n.p[0]);
        if order.is_some() {
            self.state_position = at.position.ground();
            if order == Some(OrderKind::Scout) {
                return Err(GenieError::unsupported(
                    "a failed action on the scout order (the player's slot 0x2e8 with 0x211)",
                ));
            }
        }
        let attack = AiAction::Attack.value();
        if p0 == attack && (order == Some(OrderKind::Attack) || action == Some(AiAction::Attack)) {
            let produced = self.process_idle(at, player_type, ai, side)?;
            if produced {
                return Ok(PRODUCED);
            }
            if player_type.is_computer() {
                return Err(GenieError::unsupported(
                    "a computer player's failed attack without a new action",
                ));
            }
            self.remove_target();
            self.stop_current(StopOrder::Keep, at, side, ai)?;
            return Ok(GO_ON);
        }
        let own = order.map_or_else(|| AiAction::field(action) == p0, |o| o.owns(action));
        if own {
            self.remove_target();
            self.stop_current(StopOrder::Keep, at, side, ai)?;
            return Ok(GO_ON);
        }
        let moved = p0 == AiAction::Move.value();
        match order {
            Some(OrderKind::GroupGather) if moved => {
                self.remove_target();
                self.stop_current(StopOrder::End, at, side, ai)?;
                return Ok(GO_ON);
            }
            Some(OrderKind::Defend) if moved => {
                self.set_action(None);
                return Ok(GO_ON);
            }
            _ => {}
        }
        if p0 == attack {
            self.remove_target();
            self.stop_current(StopOrder::Keep, at, side, ai)?;
        }
        Ok(GO_ON)
    }

    /// The base handler's case `0x1fb` ("Action %d has been invalidated.", `FUN_140c589c0`): an
    /// attack (600) invalidated on the attack order (700) or with `0x202` retargets: with
    /// retargeting off (`+0x1d6`) a target still in the world breaks off (not modelled), else
    /// processIdle (slot 0x240 with 0, [`UnitAi::process_idle`]; the building's handler never
    /// passes the notify on). Unless it is 5 the notify goes on; with 5 a human player's unit
    /// forgets its target (`FUN_140c5e1e0`) and stops (slot 0xb0 with 1); a computer player's
    /// tests (slot 0x130) are not modelled. Without an order, the invalidation of the current
    /// action forgets the target and stops it (slot 0xb0 with 1; then slot 0x48 hands the notify
    /// to the owner, not held); with one, so does the invalidation of the order's own action
    /// (action + 100 = order); order `0x2d4` and `0x2bd` on action `0x262` are not modelled; any
    /// other invalidated attack forgets the target and stops; the rest go on.
    pub(super) fn action_invalidated(
        &mut self,
        n: Notify,
        at: &Surroundings,
        player_type: PlayerType,
        ai: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<u32, GenieError> {
        let (order, action, p0) = (self.current_order, self.current_action, n.p[0]);
        let attack = AiAction::Attack.value();
        if (order == Some(OrderKind::Attack) || n.p[2] == 0x202) && p0 == attack {
            if self.retarget_disabled != 0 {
                return Err(GenieError::unsupported(
                    "an invalidated attack with retargeting off (+0x1d6)",
                ));
            }
            let produced = self.process_idle(at, player_type, ai, side)?;
            if produced {
                return Ok(GO_ON);
            }
            if player_type.is_computer() {
                return Err(GenieError::unsupported(
                    "a computer player's invalidated attack (slot 0x130)",
                ));
            }
            self.remove_target();
            self.stop_current(StopOrder::Keep, at, side, ai)?;
            return Ok(GO_ON);
        }
        let own = if order.is_none() {
            AiAction::field(action) == p0
        } else {
            if matches!(order, Some(OrderKind::GroupGather | OrderKind::Defend))
                && p0 == AiAction::Move.value()
            {
                return Err(GenieError::unsupported(
                    "an invalidated action 0x262 on order 0x2d4 or 0x2bd",
                ));
            }
            order.is_some_and(|o| o.owns(action))
        };
        if own || p0 == attack {
            self.remove_target();
            self.stop_current(StopOrder::Keep, at, side, ai)?;
        }
        Ok(GO_ON)
    }

    /// The base handler's case `0x1fa` (`FUN_140c589c0`): with an order, or in a group with a
    /// command (`+0x3a8`), the state position takes the unit's place. Without an order, a
    /// completion of the current action (or of an attack) stops it. On the attack order 700 an
    /// attack's completion ("I have an attack order and my attack action has completed.") runs
    /// processIdle (slot 0x240 with 0): when it produced an action the processing breaks (0xd),
    /// else the target goes and the action stops, ending the order (slot 0xb0 with 0). With
    /// another order, the completion of the order's own action (action + 100 = order) stops it,
    /// and so do an attack's; order 724 and 701 end their move, 712 a heal's.
    pub(super) fn action_completed(
        &mut self,
        n: Notify,
        at: &Surroundings,
        player_type: PlayerType,
        ai_rng: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<u32, GenieError> {
        let p1 = n.p[0];
        let order = self.current_order;
        let action = self.current_action;
        if order.is_some() || at.group.is_some_and(|g| g.commanded) {
            self.state_position = at.position.ground();
        }
        let attack = AiAction::Attack.value();
        if order.is_none() {
            if AiAction::field(action) == p1 || p1 == attack {
                self.remove_target();
                self.stop_current(StopOrder::Keep, at, side, ai_rng)?;
            }
            return Ok(GO_ON);
        }
        if order == Some(OrderKind::Attack) && p1 == attack {
            if self.process_idle(at, player_type, ai_rng, side)? {
                return Ok(PRODUCED);
            }
            self.remove_target();
            self.stop_current(StopOrder::End, at, side, ai_rng)?;
            return Ok(GO_ON);
        }
        if AiAction::field(action) == p1 && order.is_some_and(|o| o.owns(action)) {
            self.remove_target();
            self.stop_current(StopOrder::Keep, at, side, ai_rng)?;
            return Ok(GO_ON);
        }
        let moved = p1 == AiAction::Move.value();
        match order {
            Some(OrderKind::GroupGather) if moved => {
                self.remove_target();
                self.stop_current(StopOrder::End, at, side, ai_rng)?;
                return Ok(GO_ON);
            }
            Some(OrderKind::Defend) if moved => {
                self.set_action(None);
                return Ok(GO_ON);
            }
            Some(OrderKind::Follow) if moved && action == Some(AiAction::MoveAside) => {
                self.set_action(None);
                return Ok(GO_ON);
            }
            _ => {}
        }
        if p1 == attack {
            self.remove_target();
            self.stop_current(StopOrder::Keep, at, side, ai_rng)?;
        }
        Ok(GO_ON)
    }

    /// `FUN_140c5e1e0`: the target becomes the last one, and none (its point −1).
    pub(crate) fn remove_target(&mut self) {
        if self.current_target.is_some() {
            self.last_target = self.current_target;
            self.last_target_class = self.target_class;
            self.current_target = None;
            self.target_class = None;
            self.target_point = Position::from([-1.0; 3]);
        }
    }

    /// Slot 0xb0 (`FUN_140c64ed0`), stopping the current action: it becomes the last action; with
    /// [`StopOrder::End`] or an action of the order (action + 100 = order) the order ends too
    /// (its priority with it), the state position taking the unit's place but for class 18. Then
    /// the unit's stopAction (slot 0x3c0), no action, and `+0x1a8` cleared. A patrol order (711,
    /// 733) ends however `order` stands, its secondary timer capped; the redraw of an
    /// adjusted idle timeout of exactly 750 (the AI stream's) is not modelled here.
    pub(super) fn stop_current(
        &mut self,
        order: StopOrder,
        at: &Surroundings,
        side: &mut dyn UnitSide,
        rng: &mut Well512,
    ) -> Result<(), GenieError> {
        let roles = &at.roles;
        self.stop_with(
            order,
            Some((at.position.ground(), at.class)),
            side,
            rng,
            roles,
        )
    }

    /// [`UnitAi::stop_current`] with the unit's position and master's class (`place`), which only
    /// an order the stop ends reads (None: the caller knows it does not end one).
    pub(crate) fn stop_with(
        &mut self,
        order: StopOrder,
        place: Option<(Point, Option<ClassId>)>,
        side: &mut dyn UnitSide,
        rng: &mut Well512,
        roles: &Roles,
    ) -> Result<(), GenieError> {
        let action = self.current_action;
        let current = self.current_order;
        if self.patrol_redraws() {
            return Err(GenieError::unsupported("a stop on a patrol order"));
        }
        self.last_action = action;
        if order == StopOrder::End
            || current.is_some_and(|o| o.owns(action))
            || matches!(current, Some(OrderKind::Patrol | OrderKind::AttackMove))
        {
            self.cap_patrol_secondary();
            self.last_order = current;
            self.current_order = None;
            self.order_priority = OrderPriority::NONE;
            let (position, class) = place.ok_or(GenieError::unsupported(
                "a stop ending an order without the unit's place",
            ))?;
            // The class whose busy units skip the idle check (`-0xe98` raw).
            // The class of the attackers and targets a villager's damage notify looks at (`-0xea2` raw).
            // The class whose stop leaves the state position (`-0xe9a` raw, slot 0xb0).
            if !roles.of_some_class(class).contains(ClassRoles::MONK) {
                self.state_position = position;
            }
        }
        side.effect_now(Effect::Stop, self, rng)?;
        self.current_action = None;
        self.stop_after_kill = false;
        Ok(())
    }

    /// `FUN_140c4dcb0` ("Forgetting current target"): the last target (`+0x6c`, `+0x70`) and the
    /// current one (`+0x48`, `+0x4c`) go, the target point with them. Its only other line is the
    /// debug log behind `DAT_143f0900d`, which changes no state.
    pub(crate) fn forget_targets(&mut self) {
        self.last_target = None;
        self.last_target_class = None;
        self.current_target = None;
        self.target_class = None;
        self.target_point = Position::from([-1.0; 3]);
    }

    /// `FUN_140c5e7d0`, the walk back to the state position: a unit more than a tile from it
    /// (the squared footprint gap, `FUN_140a847c0`, against [`WALK_BACK`]; `COMISS` then `JNC`,
    /// so a gap that is not a number walks) moves there at its own height within [`WALK_BACK`],
    /// forced and running (the unit's slot 0x340, which the caller runs: it takes the move but
    /// for classes 51 and 54, whose slot `FUN_141216c50` the caller does not model). On the
    /// move the action value becomes the move's (`0x262`, the old one the last); the target
    /// stays. The move gives the unit its `MoveTo` at once: the update's later checks see it
    /// busy. True when it walks.
    pub(super) fn walk_back(
        &mut self,
        at: &Surroundings,
        side: &mut dyn UnitSide,
        rng: &mut Well512,
    ) -> Result<bool, GenieError> {
        let gap = point_gap_squared(self.state_position, at.position, at.size);
        if !below_or_unordered(WALK_BACK, gap.get()) {
            return Ok(false);
        }
        let (x, y) = (self.state_position.x(), self.state_position.y());
        side.effect_now(
            Effect::Move(AiMove {
                point: Position::new(x, y, at.position.z()),
                range: Distance::new(WALK_BACK),
                force: true,
                walk: false,
            }),
            self,
            rng,
        )?;
        self.set_action(Some(AiAction::Move));
        Ok(true)
    }

    /// The reset at the full update once the unit's actions ended (the unit's slot 0x430 false):
    /// an action value of [`ENDED_ACTIONS`] goes; `0x2bf` of stance 2 also sets the state
    /// position.
    pub(super) fn reset_ended_action(&mut self, at: &Surroundings) {
        if self
            .current_action
            .is_some_and(|a| ENDED_ACTIONS.contains(&a))
        {
            if self.current_action == Some(AiAction::MoveAside)
                && self.stance == UnitStance::STAND_GROUND
            {
                self.state_position = at.position.ground();
            }
            self.current_action = None;
        }
    }

    /// The order check of a unit that is not busy: an order of these values the unit can give up
    /// (`FUN_140c4eea0` with 0) ends, its target removed (`FUN_140c5e1e0`), the patrol order 733
    /// resetting its timers ([`UnitAi::leave_patrol_timers`]); its priority and the last order
    /// stay. The patrol order 711 is not among them. The game's switch also lists `0x269`,
    /// `0x2cf` and `0x2d0`, which no order holds ([`OrderKind`]).
    pub(super) fn check_order(&mut self, at: &Surroundings, ai: &mut Well512) {
        let ends = matches!(
            self.current_order,
            Some(
                OrderKind::Attack
                    | OrderKind::Gather
                    | OrderKind::Move
                    | OrderKind::Hunt
                    | OrderKind::AttackMove
                    | OrderKind::AutoBoarLuring
            )
        );
        if ends && self.can_order(at, false) {
            self.remove_target();
            self.leave_patrol_timers(ai);
            self.current_order = None;
        }
    }

    /// On a patrol order (711, 733) with an adjusted idle timeout of exactly 750: leaving the
    /// order redraws it (slot 0x1a8), which draws from the AI stream.
    pub(super) const fn patrol_redraws(&self) -> bool {
        matches!(
            self.current_order,
            Some(OrderKind::Patrol | OrderKind::AttackMove)
        ) && self.adjusted_idle_timeout.get() == 750
    }

    /// The rest of leaving a patrol order (711, 733) once the redraw is not due
    /// ([`UnitAi::patrol_redraws`]): the secondary timer is capped at 750 (`0x2ee < (int)+0x16c`).
    pub(super) const fn cap_patrol_secondary(&mut self) {
        if matches!(
            self.current_order,
            Some(OrderKind::Patrol | OrderKind::AttackMove)
        ) && 750 < self.secondary_timer.get() as i32
        {
            self.secondary_timer = Millis::new(750);
        }
    }

    /// What leaving a patrol order (711, 733) does to the timers wherever the game sets the
    /// order by hand: an adjusted idle timeout of exactly 750 is redrawn (slot 0x1a8) and the
    /// secondary timer is capped at 750 (`0x2ee < (int)+0x16c`).
    pub(super) fn leave_patrol_timers(&mut self, ai: &mut Well512) {
        if matches!(
            self.current_order,
            Some(OrderKind::Patrol | OrderKind::AttackMove)
        ) {
            if self.adjusted_idle_timeout == Millis::new(750) {
                self.update_adjusted_idle_timeout(ai);
            }
            if 750 < self.secondary_timer.get() as i32 {
                self.secondary_timer = Millis::new(750);
            }
        }
    }

    /// `FUN_140c4eea0`: a unit takes orders unless its master has a task of action type 0x99
    /// (`FUN_1415c60b0`) and its target exists in the world (`FUN_1415417a0` on `+0x48`) or it
    /// has an action value (`+0x44`). With `task_only` (its second argument, which the full
    /// update and `FUN_140c68980` pass as 0 and only the slot 4 of `FUN_140c2bd20` as 1) a unit
    /// that passes the test takes orders only with that task.
    pub(super) const fn can_order(&self, at: &Surroundings, task_only: bool) -> bool {
        if at.task_0x99 && (at.target.is_some() || self.current_action.is_some()) {
            return false;
        }
        !task_only || at.task_0x99
    }

    /// The unit AI between its owner's turns (`FUN_140c68980`, from `RGE_Combat_Object::update`
    /// for a unit of a type below 70 or not inside another): with no notifies queued, a unit
    /// without an action value or with an action it may interrupt (slot 0x2d0) runs its order
    /// queue. Its effects, as the full update's; `side` answers what the orders ask of the unit,
    /// `player_type` is its owner's type and `ai` the AI stream.
    ///
    /// # Errors
    ///
    /// What an order's slot does not model.
    pub fn off_turn(
        &mut self,
        at: &Surroundings,
        side: &mut dyn UnitSide,
        player_type: PlayerType,
        ai: &mut Well512,
    ) -> Result<(), GenieError> {
        if !self.notifies.is_empty() || self.orders.is_empty() {
            return Ok(());
        }
        if (self.current_action.is_some() || at.busy) && !at.interruptible {
            return Ok(());
        }
        if self.can_order(at, false) {
            self.run_orders(at, side, player_type, ai)?;
        }
        Ok(())
    }

    /// Slot 0x28 (`FUN_140c55c90`, every type's; slot 0x30, `FUN_140c563d0`, takes 14 arguments
    /// and passes it the order's x and y as the shown point and its last argument's low byte as
    /// the ready flag), an order for the
    /// queue: a unit in a state above 2 takes none (false); an order of kind −1 neither (true),
    /// which no [`QueuedOrder`] holds: its callers without an order ([`UnitAi::queue_current`])
    /// stop first;
    /// an order [`Dispatch::Queued`] goes into the queue at its `end` (`FUN_140c42640`: before
    /// the first queued order at [`QueueEnd::Front`], else after the last). One
    /// [`Dispatch::Now`], which keeps or drops the current order and runs the queue at once, is
    /// not modelled.
    pub fn queue_order(
        &mut self,
        unit_state: ObjectState,
        order: QueuedOrder,
        dispatch: Dispatch,
        end: QueueEnd,
    ) -> Result<bool, GenieError> {
        if ObjectState::Alive < unit_state {
            return Ok(false);
        }

        if dispatch == Dispatch::Now {
            // the immediate branch is [`UnitAi::order_now`], which reads the unit's id and place
            // and gives effects; [`UnitAi::take_order_now`] is its entry
            return Err(GenieError::unsupported(
                "an immediate order for the unit AI's queue",
            ));
        }
        self.insert_order(order, end);
        Ok(true)
    }

    /// `FUN_140c4dfa0`: an order in the queue is not ready (its `+0x2c` is 0).
    #[must_use]
    pub fn has_unready_order(&self) -> bool {
        self.orders.iter().any(|o| !o.ready)
    }

    /// The order queue (`FUN_140c5d790`): while the AI has an action, an order below priority
    /// 100 waits; each order runs through slot 0x230 ([`UnitAi::process_order`] with the unit's
    /// `at`, `side` and owner's type `player_type`, the AI stream `ai`): one that is not ready (0xe)
    /// stays and the next is tried, any other result removes it, and 1 (taken) ends the queue's
    /// turn.
    ///
    /// # Errors
    ///
    /// What an order's slot does not model.
    pub fn run_orders(
        &mut self,
        at: &Surroundings,
        side: &mut dyn UnitSide,
        player_type: PlayerType,
        ai: &mut Well512,
    ) -> Result<(), GenieError> {
        let roles = &at.roles;
        let mut k = 0;
        while k < self.orders.len() {
            if self.current_action.is_some() && self.orders.checked(k)?.priority.waits() {
                return Ok(());
            }
            let order = *self.orders.checked(k)?;
            let r = self.process_order(&order, Some((at, player_type)), None, ai, side, roles)?;
            if r == OrderOutcome::NotReady {
                k += 1;
                continue;
            }
            // the game removes the order only while k is inside the queue: slot 0x230 may have
            // shrunk it (FUN_140c5de60)
            if k < self.orders.len() {
                let _ = self.orders.remove(k);
            }
            if r == OrderOutcome::Taken {
                return Ok(());
            }
        }
        Ok(())
    }
}
