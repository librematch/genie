//! A unit AI's construction, its order and target, its patrol and its moves.

use crate::GenieError;
use crate::groups::FormationKind;
use crate::map::Point;
use crate::map::Position;
use crate::math::fmath::{at_most_or_unordered, sqrt};
use crate::math::rng::Well512;
use crate::movement::PathAsk;
use crate::movement::{PathFlags, WaypointIndex};
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::masters::ClassId;
use crate::unit_ai::AiAction;
use crate::unit_ai::AlertMode;
use crate::unit_ai::Attackers;
use crate::unit_ai::IdleOutcome;
use crate::unit_ai::OrderForce;
use crate::unit_ai::OrderKind;
use crate::unit_ai::OrderPriority;
use crate::unit_ai::SurroundingFlags;
use crate::unit_ai::UnitAiKind;
use crate::unit_ai::UnitStance;
use crate::unit_ai::world_view::UnitView;
use crate::unit_ai::{
    ARTIFACT_IDLE_TIMEOUT, AiMove, COMPUTER_SOLDIER_IDLE_TIMEOUT, DESIRED_DISTANCE, Effect, Found,
    HUMAN_ORDER_CAP, HUNTED_IDLE_TIMEOUT, Home, Hunted, IDLE_TIMEOUT, LION_IDLE_TIMEOUT, Memory,
    NewObject, ORDER_HISTORY, OrderRecord, PRIEST_IDLE_TIMEOUT, SIEGE_IDLE_TIMEOUT,
    SOLDIER_IDLE_TIMEOUT, Surroundings, TOWER_IDLE_TIMEOUT, UnitAi, UnitSeen,
    adjusted_idle_timeout, notify_capacity, siege_stance, soldier_stance,
};
use crate::world::Millis;
use crate::world::Seconds;
use crate::world::WorldTime;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::layer::generated::UnitAiTuning;
use crate::world::slots::CheckedIndex;

/// Whether the order setter ([`UnitAi::set_order_with`]) keeps the order before as the last
/// (1) or leaves the last as it is (0).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OrderHistory {
    /// The order before becomes the last.
    Recorded,
    /// The last stays.
    Skipped,
}

/// Whether a work command's notify of the unit AI ([`UnitAi::notify_work`]) clears its notifies
/// (`FUN_140c64e50`): a unit of a type below 70 always, another only with a direct order
/// ([`crate::orders::OrderSource::notifies`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Notifies {
    /// They are cleared with time 0.
    Cleared,
    /// They stay.
    Kept,
}

impl UnitAi {
    /// The unit AI of `ai_kind` a new object gets, as `FUN_141215560`'s constructors make it
    /// (the factory `FUN_141215a90` picks among them; the types 3, 6 and 7 only a load makes).
    /// `UnitAIModule`'s constructor (`0x140c2e080`)
    /// draws three AI-stream values, in this order: the last retarget time `r % 500`, the
    /// adjusted idle timeout `r % 1500 + 2250` and the retarget timer `750 + r % 500`; it sets
    /// the stance (`+0x1a9`) to 3 and the state position to the object's place. Then each
    /// type's own constructor:
    ///
    /// - 2, the hunted animal (`FUN_140c11920`, [`HUNTED_CARED`]): its idle timeout 4000
    ///   (`FUN_140c41bc0`, whose slot 0x1a8 `FUN_140c17170` makes the adjusted one 5/4 of it
    ///   without a draw), its origin (the object's place) and leash ([`NewObject::hunt_range`]);
    ///   the king's (class 59) stance 0, the others' stays 3.
    /// - 3 (`TribePreditorUnitAIModule`, `FUN_140c11da0`), 0xa (the trade ship's,
    ///   `FUN_140c12ab0`), 0xb (the transport ship's, `FUN_140c12c30`), 0xc (the fishing ship's,
    ///   `FUN_140c11860`, the base's `+0x1e8` 7), 0x11 (the building's, `FUN_140c11470`,
    ///   [`BUILDING_CARED`]), 0x12 (the siege tower's, `FUN_140c11fb0`) and 8 (the civilian's,
    ///   `FUN_140c115d0`, [`CIVILIAN_CARED`], the base's `+0x1e8` 7): stance 0.
    /// - 4, the boar (`FUN_140c11720`): stance 0, its origin the object's place, not furious.
    /// - 5, the lion (`FUN_140c11bd0`, [`LION_CARED`]; the world's `+0x6b4` of 1 adds four
    ///   classes): stance 0, its idle timeout 6000 and a fourth draw (slot 0x1a8,
    ///   `0x140c41e00`).
    /// - 6 and 7 (`TribeRangedUnitAIModule`, `FUN_140c11f80`, `FUN_140c11f50`): the base's.
    /// - 9, the priest (`FUN_140c11e30`): stance 1, its idle timeout 500 and a fourth draw.
    /// - 0xd, the soldier (`FUN_140c12300`, [`SOLDIER_CARED`]; a computer player's adds 10): its
    ///   idle timeout 500 (a computer player's 1500) and a fourth draw, then its stance
    ///   ([`soldier_stance`]).
    /// - 0xe, the artifact (`FUN_140c11340`, [`ARTIFACT_CARED`]): stance 0, its idle timeout
    ///   5000 and a fourth draw.
    /// - 0xf, the tower (`FUN_140c12960`, [`TOWER_CARED`]): stance 0, its idle timeout 1000 and
    ///   a fourth draw.
    /// - 0x10, the siege unit (`FUN_140c12040`): its stance ([`siege_stance`]), its idle timeout
    ///   1000 and a fourth draw.
    /// - 0x13, livestock (`FUN_140c11d70`): stance 1.
    ///
    /// [`ARTIFACT_CARED`]: crate::unit_ai::ARTIFACT_CARED
    /// [`BUILDING_CARED`]: crate::unit_ai::BUILDING_CARED
    /// [`CIVILIAN_CARED`]: crate::unit_ai::CIVILIAN_CARED
    /// [`HUNTED_CARED`]: crate::unit_ai::HUNTED_CARED
    /// [`LION_CARED`]: crate::unit_ai::LION_CARED
    /// [`SOLDIER_CARED`]: crate::unit_ai::SOLDIER_CARED
    /// [`TOWER_CARED`]: crate::unit_ai::TOWER_CARED
    #[must_use]
    pub fn constructed(
        ai_kind: UnitAiKind,
        new: &NewObject,
        ai: &mut Well512,
        roles: &Roles,
    ) -> Self {
        let mut made = Self::base_constructed(ai_kind, new.position, ai);
        match ai_kind {
            UnitAiKind::Hunted => {
                made.set_idle_timeout(HUNTED_IDLE_TIMEOUT, ai);
                made.hunted = Some(Hunted {
                    origin: new.position,
                    leash: new.hunt_range,
                });
                if roles.of_class(new.class).contains(ClassRoles::KING) {
                    made.stance = UnitStance::Aggressive;
                }
            }
            UnitAiKind::Boar => {
                made.stance = UnitStance::Aggressive;
                made.home = Some(Home {
                    origin: new.position,
                    furious: false,
                    homeward: false,
                });
            }
            UnitAiKind::Lion => {
                made.stance = UnitStance::Aggressive;
                made.set_idle_timeout(LION_IDLE_TIMEOUT, ai);
            }
            // the base's stance 3
            UnitAiKind::Ranged | UnitAiKind::Ranged2 => {}
            UnitAiKind::Priest => {
                made.stance = UnitStance::Defensive;
                made.set_idle_timeout(PRIEST_IDLE_TIMEOUT, ai);
            }
            UnitAiKind::Soldier => {
                let timeout = if new.player_kind.is_computer() {
                    COMPUTER_SOLDIER_IDLE_TIMEOUT
                } else {
                    SOLDIER_IDLE_TIMEOUT
                };
                made.set_idle_timeout(timeout, ai);
                made.stance = soldier_stance(new, roles);
            }
            UnitAiKind::Artifact => {
                made.stance = UnitStance::Aggressive;
                made.set_idle_timeout(ARTIFACT_IDLE_TIMEOUT, ai);
            }
            UnitAiKind::Tower => {
                made.stance = UnitStance::Aggressive;
                made.set_idle_timeout(TOWER_IDLE_TIMEOUT, ai);
            }
            UnitAiKind::Siege => {
                made.stance = siege_stance(new, roles);
                made.set_idle_timeout(SIEGE_IDLE_TIMEOUT, ai);
            }
            UnitAiKind::Livestock => made.stance = UnitStance::Defensive,
            // 3, 8, 0xa, 0xb, 0xc, 0x11 and 0x12
            _ => made.stance = UnitStance::Aggressive,
        }
        made
    }

    /// `UnitAIModule`'s constructor (`0x140c2e080`): its three draws, the base's fields, the
    /// stance (`+0x1a9`) 3 of the two bytes at `+0x1a8` (0x300) and the state position the
    /// object's place.
    pub(super) fn base_constructed(
        ai_kind: UnitAiKind,
        position: Position,
        ai: &mut Well512,
    ) -> Self {
        let last_retarget_time = WorldTime::from_millis(ai.rand15() % 500);
        let adjusted_idle_timeout = adjusted_idle_timeout(IDLE_TIMEOUT, ai);
        let retarget_timer = Millis::new(750 + ai.rand15() % 500);
        Self {
            ai_kind,
            current_order: None,
            order_priority: OrderPriority::NONE,
            current_action: None,
            current_target: None,
            target_class: None,
            target_point: Position::from([-1.0; 3]),
            desired_distance: DESIRED_DISTANCE,
            defend_target: None,
            last_order: None,
            last_action: None,
            last_target: None,
            last_target_class: None,
            idle_timer: Millis::ZERO,
            adjusted_idle_timeout,
            idle_timeout: IDLE_TIMEOUT,
            secondary_timer: Millis::new(750),
            look_around_timer: Millis::ZERO,
            look_around_timeout: Millis::new(1000),
            last_retarget_time,
            retarget_timer,
            stance: UnitStance::NoAttack,
            // the constructor's word at +0x1d4 (0x140c2e080)
            formation_kind: FormationKind::Horde,
            // the object's place
            state_position: position.ground(),
            retarget_disabled: 0,
            alert_mode: AlertMode::Calm,
            time_since_enemy: Seconds::ZERO,
            patrol_path: None,
            patrol_waypoint: WaypointIndex::FIRST,
            stop_after_kill: false,
            home: None,
            hunted: None,
            last_update_time: Millis::ZERO,
            orders: Vec::new(),
            building_orders: Vec::new(),
            notifies: Vec::new(),
            notify_capacity: notify_capacity(ai_kind, 0),
            processing: false,
            buffered: Vec::new(),
            ordered_on_target: false,
            attackers: Attackers::default(),
            order_history: Vec::new(),
            retries: Vec::new(),
            best_this_update: None,
        }
    }

    /// `FUN_140c42320` for each retarget entry an attack search's `bestUnitToAttack` added
    /// ([`Found::retries`]): it joins the end of the entries.
    pub fn add_retries(&mut self, found: &Found) {
        self.retries.extend(found.retries.iter().flatten());
    }

    /// The memory `bestUnitToAttack` reads ([`Memory`]).
    #[must_use]
    pub fn memory(&self) -> Memory<'_> {
        Memory {
            retries: &self.retries,
            history: &self.order_history,
        }
    }

    /// `FUN_140c42080` once the world holds the target (the callers' `FUN_1415417a0`; an order on
    /// a point records nothing): the order joins the history first, a ninth recycling the
    /// oldest ([`ORDER_HISTORY`]).
    pub fn remember_order(&mut self, record: OrderRecord) {
        self.order_history.insert(0, record);
        self.order_history.truncate(ORDER_HISTORY);
    }

    /// The full update's start (`0x140c66b10`): its retarget entries by the world's objects
    /// (`seen`, [`UnitView::seen_unit`]): a target the world no longer holds or below 1 hit point
    /// (`COMISS`, a NaN stays) empties them all; otherwise one whose time (`timeout`) is not past
    /// `now` stays and the others go, in order.
    ///
    /// # Errors
    ///
    /// What the world's lookup does not model.
    pub fn expire_retries(
        &mut self,
        now: WorldTime,
        seen: &mut dyn FnMut(ObjectId) -> Result<Option<UnitSeen>, GenieError>,
    ) -> Result<(), GenieError> {
        let mut i = 0;
        while i < self.retries.len() {
            let (target, timeout) = *self.retries.checked(i)?;
            if seen(target)?.is_none_or(|u| u.hp < HitPoints::new(1.0)) {
                self.retries.clear();
                break;
            }
            if now < timeout {
                i += 1;
            } else {
                let _ = self.retries.remove(i);
            }
        }
        Ok(())
    }

    /// `FUN_140c41bc0`: the idle timeout (`+0x168`) is `timeout`, then slot 0x1a8 redraws the
    /// adjusted one ([`UnitAi::update_adjusted_idle_timeout`]).
    pub(super) fn set_idle_timeout(&mut self, timeout: Millis, ai: &mut Well512) {
        self.idle_timeout = timeout;
        self.update_adjusted_idle_timeout(ai);
    }

    /// What an order (INTERACT, MOVE, RETREAT) does to each of its units' AI before it runs
    /// (`FUN_140e6cfc0`): the patrol path goes (`FUN_140c68de0`: `+0x1c0`, its waypoint `+0x1c8`
    /// 0); an adjusted idle timeout of 750 or more is redrawn
    /// (`UpdateAdjustedIdleTimeout`, unit-AI slot 53, `0x140c41e00`), and the secondary timer is
    /// capped at 750.
    pub fn reset_for_order(&mut self, ai: &mut Well512) {
        self.zero_patrol();
        if self.adjusted_idle_timeout > Millis::new(749) {
            self.update_adjusted_idle_timeout(ai);
        }
        self.secondary_timer = self.secondary_timer.min(Millis::new(750));
    }

    /// `zeroPatrolInformation` (`FUN_140c68de0`): the saved patrol goes and its waypoint is 0.
    pub fn zero_patrol(&mut self) {
        self.patrol_path = None;
        self.patrol_waypoint = WaypointIndex::FIRST;
    }

    /// `savePatrolInformation` (`FUN_140c63d20`): the saved patrol goes; the unit's slot 0x468
    /// gives the new one and its waypoint (0 without one).
    pub(super) fn save_patrol(&mut self, side: &UnitView<'_, '_>) {
        let (path, waypoint) = side.patrol_information().unzip();
        self.patrol_path = path;
        self.patrol_waypoint = waypoint.unwrap_or(WaypointIndex::FIRST);
    }

    /// `restorePatrolInformation` (`FUN_140c5e5b0`): without a saved patrol,
    /// [`IdleOutcome::Idle`]. Else the unit
    /// walks it on ([`Effect::RestorePatrol`]); a unit in no group (the world's
    /// `FUN_14152ff40`) takes the patrol order back, the attack move's (733) when it attack-moves
    /// (`+0x7fb`), else 711 (leaving the other one redraws an adjusted idle timeout of exactly
    /// 750, slot 0x1a8); the secondary timer is 750.
    /// [`IdleOutcome::Acted`].
    pub(super) fn restore_patrol(
        &mut self,
        at: &Surroundings,
        ai: &mut Well512,
        side: &mut UnitView<'_, '_>,
    ) -> Result<IdleOutcome, GenieError> {
        if self.patrol_path.is_none() {
            return Ok(IdleOutcome::Idle);
        }
        side.effect_now(Effect::RestorePatrol, self, ai)?;
        if at.group.is_none() {
            let (order, other) = if at.flags.contains(SurroundingFlags::ATTACK_MOVING) {
                (OrderKind::AttackMove, OrderKind::Patrol)
            } else {
                (OrderKind::Patrol, OrderKind::AttackMove)
            };
            if self.current_order == Some(other) && self.adjusted_idle_timeout == Millis::new(750) {
                self.update_adjusted_idle_timeout(ai);
            }
            self.current_order = Some(order);
        }
        self.secondary_timer = Millis::new(750);
        Ok(IdleOutcome::Acted)
    }

    /// What a `MoveTo`'s update in state 3 (`0x140aa2a70`) reads of its unit's AI for the path
    /// it asks (`ai`, None for a unit without one), the unit at `at` and the `MoveTo`'s point
    /// `point` (x, y): without an AI a unit in range asks anyway and the attack move stays
    /// (`param_10` 2). With one, on a patrol order (711, 733) the path loops; the saved patrol, if
    /// any, gives the waypoint to walk on from and its back and forth (`getPatrolLinear`) and goes
    /// (`zeroPatrolInformation`); else the path goes back and forth when the point is at least 3
    /// from the unit (`COMISS`, `SETBE`: a NaN distance is). The attack move is on order 733, off
    /// on any other, as it was while a saved patrol is still held.
    #[must_use]
    pub fn path_ask(
        ai: Option<&mut Self>,
        at: Point,
        point: Point,
        unit_ai_tuning: &UnitAiTuning,
    ) -> PathAsk {
        let Some(ai) = ai else {
            return PathAsk {
                ends_in_range: false,
                ..PathAsk::default()
            };
        };
        let patrolling = matches!(
            ai.current_order,
            Some(OrderKind::Patrol | OrderKind::AttackMove)
        );
        let mut ask = PathAsk {
            ends_in_range: !patrolling,
            ..PathAsk::default()
        };
        if patrolling {
            ask.patrol = true;
            if let Some(saved) = ai.patrol_path.as_ref() {
                ask.resume = ai.patrol_waypoint;
                ask.back_and_forth = saved.flags.contains(PathFlags::BACK_AND_FORTH);
                ai.zero_patrol();
            } else {
                let (dx, dy) = (point.x() - at.x(), point.y() - at.y());
                let d = sqrt(dy * dy + dx * dx);
                ask.back_and_forth = at_most_or_unordered(unit_ai_tuning.patrol_turn_reach, d);
            }
        }
        ask.attack_move = ai
            .patrol_path
            .is_none()
            .then_some(ai.current_order == Some(OrderKind::AttackMove));
        ask
    }

    /// `UpdateAdjustedIdleTimeout` (slot 0x1a8): the base draws around the idle timeout
    /// (`0x140c41e00`); a deer's or fox's (factory type 2, `FUN_140c17170`) is 5/4 of it.
    pub(super) fn update_adjusted_idle_timeout(&mut self, ai: &mut Well512) {
        self.adjusted_idle_timeout = if self.ai_kind == UnitAiKind::Hunted {
            Millis::new(self.idle_timeout.get().wrapping_mul(5) >> 2)
        } else {
            adjusted_idle_timeout(self.idle_timeout, ai)
        };
    }

    /// The order setter with its history ([`UnitAi::set_order_with`], [`OrderHistory::Recorded`]).
    pub(crate) fn set_order(&mut self, order: Option<OrderKind>, ai: &mut Well512) {
        self.set_order_with(order, OrderHistory::Recorded, ai);
    }

    /// The order setter (`FUN_140c64b60`): leaving a patrol order (711, 733) redraws an adjusted
    /// idle timeout of exactly 750 (slot 0x1a8) and caps the secondary timer at 750, the
    /// compare signed (`0x2ee < (int)+0x16c`); with [`OrderHistory::Recorded`] (its third
    /// argument 1) the order before becomes the last (`+0x64`), else the last stays; then the
    /// order.
    pub(crate) fn set_order_with(
        &mut self,
        order: Option<OrderKind>,
        history: OrderHistory,
        ai: &mut Well512,
    ) {
        let leaving = |o| self.current_order == Some(o) && order != Some(o);
        if leaving(OrderKind::Patrol) || leaving(OrderKind::AttackMove) {
            if self.adjusted_idle_timeout == Millis::new(750) {
                self.update_adjusted_idle_timeout(ai);
            }
            if 750 < self.secondary_timer.get() as i32 {
                self.secondary_timer = Millis::new(750);
            }
        }
        if history == OrderHistory::Recorded {
            self.last_order = self.current_order;
        }
        self.current_order = order;
    }

    /// The end of a Build whose AI holds queued orders (`0x140ab0370` case 0xdf): no order and no
    /// action (`FUN_140c64b60` with −1 and 1, `FUN_140c64b50` with −1); leaving a patrol order
    /// caps the secondary timer ([`UnitAi::cap_patrol_secondary`]); the redraw of an adjusted
    /// idle timeout of exactly 750 (slot 0x1a8, the AI stream's) is not modelled here.
    pub const fn end_order_and_action(&mut self) -> Result<(), GenieError> {
        if self.patrol_redraws() {
            return Err(GenieError::unsupported(
                "the end of a Build on a patrol order",
            ));
        }
        self.cap_patrol_secondary();
        self.last_order = self.current_order;
        self.current_order = None;
        self.set_action(None);
        Ok(())
    }

    /// `FUN_140c64b50`.
    pub(crate) const fn set_action(&mut self, action: Option<AiAction>) {
        self.last_action = self.current_action;
        self.current_action = action;
    }

    /// `FUN_140c64c10`: the target and its class become the last ones (`+0x6c`, `+0x70`), the
    /// target takes `id` (none: −1) and the point (`+0x50`..`+0x58`); the class (`+0x4c`) is the
    /// master's of the object the world holds under `id` (`FUN_1415417a0`; `class`), else it
    /// stays.
    pub(crate) const fn target_at(
        &mut self,
        id: Option<ObjectId>,
        class: Option<ClassId>,
        point: Position,
    ) {
        self.last_target = self.current_target;
        self.last_target_class = self.target_class;
        self.current_target = id;
        self.target_point = point;
        if let Some(class) = class {
            self.target_class = class.or_none();
        }
    }

    /// `FUN_140c64be0`: the target (none: −1), its class and its point.
    pub(crate) const fn set_target(
        &mut self,
        target: Option<ObjectId>,
        class: Option<ClassId>,
        point: Position,
    ) {
        self.last_target = self.current_target;
        self.last_target_class = self.target_class;
        self.target_class = class;
        self.target_point = point;
        self.current_target = target;
    }

    /// The work command's notify of its new action (`FUN_14106e4e0`, object slot 0x828): the unit
    /// AI takes the order and action values of the action's type, and the target with its class
    /// (None for a point) and the order's point (`FUN_140c64b60`, `FUN_140c64b50`,
    /// `FUN_140c64be0`); then, with [`Notifies::Cleared`], `FUN_140c64e50` clears the notifies with
    /// time 0 (`FUN_140c5dc10`; for a unit of type 70 or more only with its `+0xa14`, which is 1
    /// but inside slot 0x128's work command, where INTERACT passes 1 and the queued order on a
    /// target 0). The order history (`FUN_140c42080`) is not held.
    pub fn notify_work(
        &mut self,
        order: Option<OrderKind>,
        action: Option<AiAction>,
        target: Option<(ObjectId, ClassId)>,
        point: Position,
        notifies: Notifies,
        ai: &mut Well512,
    ) -> Result<(), GenieError> {
        self.set_order(order, ai);
        self.set_action(action);
        let (id, class) = target.map_or((None, None), |(id, class)| (Some(id), class.or_none()));
        self.set_target(id, class, point);
        if notifies == Notifies::Cleared {
            self.clear_notifies(Millis::ZERO)?;
        }
        Ok(())
    }

    /// MOVE's notify (`FUN_140e67800`): a unit that took the move as a `MoveTo` (`moved`) gets the
    /// move order and action and forgets its target (`FUN_140c4dcb0`); every unit then takes the
    /// target (None for a point) and the order's point on the ground (`FUN_140c64be0`).
    pub fn notify_move(
        &mut self,
        moved: bool,
        target: Option<(ObjectId, ClassId)>,
        point: Point,
        ai: &mut Well512,
    ) {
        if moved {
            self.set_order(Some(OrderKind::Move), ai);
            self.set_action(Some(AiAction::Move));
            self.forget_targets();
        }
        let (id, class) = target.map_or((None, None), |(id, class)| (Some(id), class.or_none()));
        self.set_target(id, class, point.with_height(0.0));
    }

    /// MOVE's notify to a unit that took its cell of the grid (`FUN_140e67800`): the move order
    /// (`FUN_140c64b60`) and action (`FUN_140c64b50`), then no target and the cell on the ground
    /// (`FUN_140c64be0`); the target it had becomes its last as [`UnitAi::set_target`] does.
    pub fn notify_grid_move(&mut self, cell: Point, ai: &mut Well512) {
        self.set_order(Some(OrderKind::Move), ai);
        self.set_action(Some(AiAction::Move));
        self.set_target(None, None, cell.with_height(0.0));
    }

    /// The AI's move (slot 0x130, `FUN_140c55620`): with `force` or no action value, the unit takes
    /// it (its slot 0x340, which the caller runs), and the AI holds the move action and the point
    /// without a target (`FUN_140c64c10` with −1, which leaves the target class). The game
    /// changes the AI only on slot 0x340's 1: the caller's slot 0x340 is `FUN_1410866e0`, which
    /// always gives 1, but for classes 51 and 54 (`FUN_141216c50`, their packing), which it
    /// does not run.
    pub(super) const fn move_to(&mut self, m: AiMove) -> Option<AiMove> {
        if matches!(m.force, OrderForce::Unforced) && self.current_action.is_some() {
            return None;
        }
        self.set_action(Some(AiAction::Move));
        // FUN_140c64c10 with -1: the world holds no object -1
        self.target_at(None, None, m.point);
        Some(m)
    }

    /// `FUN_140c41e50`: the unit's last human order time drops to `FUN_140fe0d10`'s 5000, not on
    /// orders 701, 711, 712 and 733.
    pub(super) fn cap_human_order(
        &mut self,
        side: &mut UnitView<'_, '_>,
        rng: &mut Well512,
    ) -> Result<(), GenieError> {
        if !matches!(
            self.current_order,
            Some(OrderKind::Defend | OrderKind::Patrol | OrderKind::Follow | OrderKind::AttackMove)
        ) {
            side.effect_now(Effect::CapHumanOrder(HUMAN_ORDER_CAP), self, rng)?;
        }
        Ok(())
    }
}
