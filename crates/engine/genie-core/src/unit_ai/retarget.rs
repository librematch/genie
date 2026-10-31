//! The idle part of the update, the retarget check and search, and the types' other checks.

use crate::GenieError;
use crate::map::Distance;
use crate::map::Position;
use crate::math::rng::Well512;
use crate::objects::ActionState;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::ClassId;
use crate::objects::masters::class;
use crate::objects::masters::master_type::BUILDING;
use crate::players::PlayerId;
use crate::unit_ai::ActionKind;
use crate::unit_ai::NearestAttacker;
use crate::unit_ai::OrderKind;
use crate::unit_ai::UnitStance;
use crate::unit_ai::{
    AiMove, AttackAsk, DEFEND_LEASH, DEFEND_PATROL_LEASH, DEFEND_SLACK, Effect, FOLLOW_SLACK_HIGH,
    FOLLOW_SLACK_LOW, GroupState, INVALID_POINT, Pick, RETARGET_RANGE, RETARGET_REACH,
    RetargetSearch, SINGLE_VOLLEY, STAND_GROUND_PATROL_LEASH, STAND_GROUND_SLACK, Search,
    Surroundings, UnitAi, UnitSide, group_lets_idle, held_action_state, patrol_distance,
    point_gap_squared, searches_on_class,
};
use crate::world::Millis;
use crate::world::WorldTime;

/// The least span [`UnitAi::set_randomized_retarget_timer`] draws around (`999 < span`: the
/// immediate `1000`).
const RETARGET_SPAN_LEAST: Millis = Millis::new(1000);

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
        computer: bool,
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
            let _ = self.process_idle(at, computer, ai, side)?;
            return Ok(());
        }
        if self.patrols(group) {
            return self.patrol_idle_part(at, computer, ai, side);
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
            return self.making_idle_part(at, computer, ai, side);
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
        computer: bool,
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
        let _ = self.process_idle(at, computer, ai, side)?;
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
        computer: bool,
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
        self.process_idle(at, computer, ai, side).map(|_| ())
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

    /// Slot 73 (`0x140c567c0`, processMisc). Its order part first, by the order (an action's
    /// value plus 100 without one): the follow order's ([`UnitAi::follow`]) and the defend
    /// order's ([`UnitAi::defend_misc`]). Then the leash checks (not on the orders 0x2c7, 0x2dd and 0x323, nor in a
    /// group that patrols, [`GroupState::patrol`]): stance 1's (Defend) stops a unit more than 5
    /// tiles (`DEFEND_LEASH`, squared) from its state position, forgets its target and walks
    /// back ([`UnitAi::walk_back`]), then goes on. Stance 2's (stand ground) stops a unit with an
    /// action value (`+0x44`, not `0x2bf` or `0x2c0`) whose squared gap to its state position
    /// exceeds its range plus [`STAND_GROUND_SLACK`] (`COMISS`/`JBE`: a gap that is not a number
    /// stays), forgets its target and walks back, and slot 73 ends there (10); a unit without an
    /// order but with an action of its own can stray. With patrol information (`+0x1c0`,
    /// `FUN_140f95e10`) either leash is not modelled. Then the retarget check:
    /// with retargeting on (`+0x1d6` 0), a timer used up is redrawn (`750 + r % 500`); a human
    /// player's unit of a range above 3 on the attack order 700 stops there ("I have a range >
    /// 3.0 with an attack order, so I don't retarget."), as does a unit with a target of Gaia's
    /// ("I have a GAIA target, so I can't retarget.") or without a target (or one gone), and
    /// one with a player's target off the attack order but for a building or wall target
    /// (classes 3, 27,
    /// 39, 49, 52) or a trebuchet's (51, 54), whose search for a better target (slot 0x538 with
    /// 9, then slot 0x520) is not modelled; nor is the attack order's search. True when the
    /// timer ran out.
    pub(super) fn retarget_check(
        &mut self,
        world_time: WorldTime,
        computer: bool,
        at: &Surroundings,
        ai: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<bool, GenieError> {
        // without an order, the order whose own action the current one is (`action + 100`)
        let order = self
            .current_order
            .or_else(|| self.current_action.and_then(ActionKind::order));
        if order == Some(OrderKind::Defend) && self.defend_misc(at, computer, ai, side)? {
            return Ok(false);
        }
        if order == Some(OrderKind::Follow) && self.follow(at, side, ai)? {
            return Ok(false);
        }
        if self.leash_ends(at, ai, side)? {
            return Ok(false);
        }
        if self.retarget_disabled != 0 {
            return Ok(false);
        }
        if world_time.since(self.last_retarget_time) < self.retarget_timer {
            return Ok(false);
        }
        self.last_retarget_time = world_time;
        self.retarget_timer = Millis::new(750 + ai.rand15() % 500);
        let target = at.target.map(|t| (t.owner, t.class));
        let listed = at.action_state.map(|_| at.attack_listed);
        if let Some(search) =
            self.retarget_outcome(target, at.weapon_range, computer, at.action_state, listed)?
        {
            let _ = self.retarget_search(search, computer, at, side, ai)?;
        }
        Ok(true)
    }

    /// [`UnitAi::retarget_check`]'s leashes (not on the orders 0x2c7, 0x2dd and 0x323, nor in a
    /// group that patrols), measured from the unit's footprint (`FUN_140a847c0`): with patrol
    /// information, the patrol leashes; else the defend stance's and stand ground's. True when
    /// slot 73 ends there (its 9 and 10).
    ///
    /// # Errors
    ///
    /// What the stop and the patrol's distance do not model.
    pub(super) fn leash_ends(
        &mut self,
        at: &Surroundings,
        ai: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<bool, GenieError> {
        // the leashes (unless on orders 0x2c7, 0x2dd and 0x323, which no order holds, or in a
        // group that patrols: FUN_141017c30, FUN_141017c40), measured from the unit's footprint
        // (FUN_140a847c0)
        let leashed = self.stance.leashed()
            && !matches!(
                self.current_order,
                Some(OrderKind::Patrol | OrderKind::AttackMove)
            )
            && !at.group.is_some_and(|g| g.patrol);
        // with patrol information (+0x1c0) the leash is the distance from where the unit last
        // moved to (slot 0x520) to the patrol's path (FUN_140f95e10): beyond it the unit stops
        // (slot 0xb0 with 1), forgets its target and walks the patrol on
        // (restorePatrolInformation): "That's too far out, returning to patrol." (10); within
        // it the defend stance ends slot 73 (9), stand ground goes on to the retarget check
        if let Some(saved) = self.patrol_path.as_ref().filter(|_| leashed) {
            let leash = if self.stance == UnitStance::DEFENSIVE {
                DEFEND_PATROL_LEASH
            } else {
                STAND_GROUND_PATROL_LEASH
            };
            if leash < patrol_distance(&saved.path.points, at.moved_at)? {
                self.stop_current(true, at, side, ai)?;
                self.remove_target();
                let _ = self.restore_patrol(at, ai, side)?;
                return Ok(true);
            }
            if self.stance == UnitStance::DEFENSIVE {
                return Ok(true);
            }
        }
        if leashed && self.stance == UnitStance::DEFENSIVE && self.patrol_path.is_none() {
            // the defend stance's: a unit more than 5 tiles (DAT_142ed6f20, squared) from its
            // state position stops (slot 0xb0 with 1), forgets its target and walks back ("We're
            // too far out, returning."), then on to the retarget check
            let gap = point_gap_squared(self.state_position, at.position, at.size);
            if DEFEND_LEASH < gap.get() {
                self.stop_current(true, at, side, ai)?;
                self.remove_target();
                let _ = self.walk_back(at, side, ai)?;
            }
        }
        if leashed
            && self.stance == UnitStance::STAND_GROUND
            && self.patrol_path.is_none()
            && !matches!(
                self.current_action,
                Some(ActionKind::MoveAside | ActionKind::GroupAside)
            )
        {
            // stand ground's: the squared gap against the range plus 0.25 (ADDSS, then COMISS
            // and JBE); a unit with an action value stops, forgets its target and walks back,
            // and slot 73 returns 10 ("We're too far out, returning.", "SO Call 40.")
            let gap = point_gap_squared(self.state_position, at.position, at.size).get();
            if (at.weapon_range + STAND_GROUND_SLACK).get() < gap && self.current_action.is_some() {
                self.stop_current(true, at, side, ai)?;
                self.remove_target();
                let _ = self.walk_back(at, side, ai)?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// The retarget's search for a better target (`0x140c567c0` past its checks):
    /// `bestUnitToAttack` from the current target's place (slot 0x520) within 8
    /// (`DAT_142eb9430`), else from the unit's own without a reference distance; an idle check
    /// with path checks, any class without a target or with a building's, in the attack work
    /// state the checks read (none without a target). Its retarget entries join the AI's, its
    /// search sets the alert mode. Then, unless a computer player's unit in stance 0 keeps a
    /// building target: on the attack order a found object whose attack category differs from
    /// a live current target's (state 2) ends it (9). Without a building target (the attack
    /// order, so) nothing more happens (9). With one: a found building but a trebuchet's
    /// classes (51, 54), unless the current target is a farm (49) and it is none, keeps the
    /// current target (10); a found annex names its main building. A found object other than the
    /// current target is attacked (slot 0xb8, not any: [`UnitAi::attack_slot`], 10).
    pub(super) fn retarget_search(
        &mut self,
        search: RetargetSearch,
        computer: bool,
        at: &Surroundings,
        side: &mut dyn UnitSide,
        rng: &mut Well512,
    ) -> Result<u32, GenieError> {
        let current = Self::world_object(self.current_target, at, side)?;
        let (from, distance, any_class, work_state) =
            current.map_or((at.position, 0.0, true, false), |t| {
                (
                    t.position,
                    RETARGET_REACH,
                    search.building,
                    search.work_state,
                )
            });
        let (classes, radius) = self.search_area(at, computer)?;
        let found = side.closest(&Search {
            at: from.ground(),
            radius: Distance::new(radius),
            classes,
            size: at.size,
            pick: Pick::Attack {
                lowest: if at.scenario && !computer { 2 } else { 0 },
                seeker: self.seeker(at),
                ask: AttackAsk {
                    distance: Distance::new(distance),
                    idle: true,
                    paths: true,
                    extra: None,
                    any_class,
                    work_state,
                },
            },
            memory: self.memory(),
        })?;
        self.add_retries(&found);
        self.attack_search_alert(found, computer)?;
        let keeps = search.building && computer && self.stance == UnitStance::AGGRESSIVE;
        let new = Self::world_object(found.id, at, side)?;
        if !keeps
            && self.current_order == Some(OrderKind::Attack)
            && let (Some(n), Some(c)) = (new, current)
            && c.state == ObjectState::Alive
            && crate::unit_ai::best_target::category(n.master, n.class)
                != crate::unit_ai::best_target::category(c.master, c.class)
        {
            return Ok(9);
        }
        if !search.building {
            return Ok(9);
        }
        let mut id = found.id;
        if let Some(n) = new {
            if n.kind == BUILDING {
                let farm_rule =
                    current.is_none_or(|c| c.class != class::FARM) || n.class == class::FARM;
                if !matches!(n.class, class::CLASS_51 | class::CLASS_54) && farm_rule {
                    return Ok(10);
                }
            }
            if BUILDING <= n.kind
                && let Some(main) = n.main
            {
                id = Some(main);
            }
        }
        let Some(id) = id.filter(|&id| Some(id) != self.current_target) else {
            return Ok(9);
        };
        let _ = self.attack_slot(id, false, at, side, rng)?;
        Ok(10)
    }

    /// The retarget check after its timer's redraw: a human player's unit of a range above 3 on
    /// the attack order stops there, as does a unit with a Gaia target or without one (or one
    /// gone; `target` its owner and class as the world holds it), and one with a player's target
    /// off the attack order but for a building, wall or trebuchet target. Then a unit without an
    /// Attack in its action list (slot 0x538 with 9: `attack_listed`, None when the slice does
    /// not hold the list) stops, as does a unit in stand ground or no attack (stance above 1)
    /// with an order, and one whose top action is in an attack's work state (6 or 7, slot 0x2b0:
    /// `action_state`, None when not held) without a building target. What the search for a
    /// better target reads ([`UnitAi::retarget_search`]), or None.
    pub(super) fn retarget_outcome(
        &self,
        target: Option<(PlayerId, ClassId)>,
        weapon_range: Distance,
        computer: bool,
        action_state: Option<ActionState>,
        attack_listed: Option<bool>,
    ) -> Result<Option<RetargetSearch>, GenieError> {
        let attack_order = self.current_order == Some(OrderKind::Attack);
        if attack_order && RETARGET_RANGE < weapon_range.get() && !computer {
            return Ok(None);
        }
        let mut search = false;
        if self.current_target.is_some() {
            match target {
                Some((PlayerId::GAIA, _)) => return Ok(None),
                Some((_, class)) if searches_on_class(class) => {
                    if attack_order && !computer {
                        return Ok(None);
                    }
                    search = true;
                }
                // a target gone from the world counts as none (FUN_1415417a0 finds nothing)
                Some(_) | None => {}
            }
        }
        if !attack_order && !search {
            return Ok(None);
        }
        // slot 0x538 with 9: without an Attack in the action list, no search (an action list
        // the slice does not hold goes on to the state's test below, which stops)
        if attack_listed == Some(false) {
            return Ok(None);
        }
        // a unit in stand ground or no attack with an order goes no further
        if UnitStance::DEFENSIVE < self.stance && self.current_order.is_some() {
            return Ok(None);
        }
        // "I have an game attack action without an order ... inAttackWorkState"
        if matches!(held_action_state(action_state)?.get(), 6 | 7) && !search {
            return Ok(None);
        }
        let work_state = matches!(held_action_state(action_state)?.get(), 6 | 7);
        Ok(Some(RetargetSearch {
            building: search,
            work_state,
        }))
    }

    /// The civilian's slot 73 (`FUN_140c21da0`, processMisc): on the attack order (700) without
    /// a target (`+0x48` −1) the action stops and the order ends (slot 0xb0 with 0). A computer
    /// player's villager whose player's `+0x622` is 1 also stops on the attack order or action
    /// when its target, a player's object, is beyond its range plus 1 (`FUN_141193e30`), which
    /// is not modelled: `+0x622` is not held. It returns 9 (no retarget).
    pub(super) fn civilian_misc(
        &mut self,
        at: &Surroundings,
        side: &mut dyn UnitSide,
        rng: &mut Well512,
    ) -> Result<(), GenieError> {
        if self.current_order == Some(OrderKind::Attack) && self.current_target.is_none() {
            self.stop_current(false, at, side, rng)?;
        }
        Ok(())
    }

    /// processMisc's defend order (`0x140c567c0`, "I have a defend order for Unit #%d"). A defend
    /// target (`+0x60`) the world does not hold, or not the owner's own or an ally's
    /// (`FUN_141663d90` with the ally flag), ends it ("Unit #%d is invalid, stopping."); one held,
    /// it first gives a target point at (−1, x) its place (`UCOMISS`: not a NaN) and ends the
    /// order once dead (above state 2) but for Gaia's ("Unit #%d is dead, stopping."). The end:
    /// the defend target and the target go (`FUN_140c5e1e0`), the current action stops (slot 0xb0
    /// with 1), a patrol order leaves its timers ([`UnitAi::leave_patrol_timers`]) and the order
    /// is −1. Else, but on the attack action (600), the evacuation (`0x2bf`) or a move toward the
    /// defend target (`0x262` with the unit's top action's target, slot 0x280, the defend
    /// target): with a defend target the target point takes its place (x, y, z) and the distance
    /// is the footprints' squared gap (`FUN_14118f8c0`), without one the squared gap from the
    /// unit's footprint to the point (`FUN_140a847c0`); beyond the desired distance plus 1,
    /// squared (`ADDSS`, `MULSS`, `COMISS` with `JBE`: a NaN stays), the unit moves to the
    /// target (slot 0x138, [`UnitAi::move_to_object`], within the desired distance) or to the
    /// point (slot 0x130 forced, running, within the desired distance). Within it, a defend
    /// target with a unit AI names the attacker of its nearest to it (its AI's slot 0x78,
    /// [`UnitSide::defended_attacker`]), which the unit attacks (slot 0xb8 forced, not any:
    /// [`UnitAi::attack_slot`]); with none, its processIdle (slot 0x240 with 0,
    /// [`UnitAi::process_idle`]) and an action it produced end slot 73. True when slot 73 ends
    /// here (10, or 9 after the order ended).
    pub(super) fn defend_misc(
        &mut self,
        at: &Surroundings,
        computer: bool,
        ai: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<bool, GenieError> {
        let mut defended = None;
        if let Some(target) = self.defend_target {
            let held = Self::world_object(target, at, side)?.filter(|t| t.allied);
            let ends = match held {
                None => true,
                Some(t) => {
                    if self.target_point.x() == INVALID_POINT {
                        self.target_point = self
                            .target_point
                            .with_x(t.position.x())
                            .with_y(t.position.y());
                    }
                    ObjectState::Alive < t.state && t.owner != PlayerId::GAIA
                }
            };
            if ends {
                self.defend_target = None;
                self.remove_target();
                self.stop_current(true, at, side, ai)?;
                self.leave_patrol_timers(ai);
                self.current_order = None;
                return Ok(true);
            }
            defended = held.map(|t| (target, t));
        }
        let toward_it = self.current_action == Some(ActionKind::Move)
            && Self::top_target_live(at, side)?.map(|t| t.id) == self.defend_target;
        if matches!(
            self.current_action,
            Some(ActionKind::Attack | ActionKind::MoveAside)
        ) || toward_it
        {
            return Ok(false);
        }
        let gap = if let Some((_, t)) = defended {
            self.target_point = t.position;
            t.gap_squared
        } else {
            point_gap_squared(self.target_point.ground(), at.position, at.size)
        };
        let reach = self.desired_distance.get() + DEFEND_SLACK;
        if reach * reach < gap.get() {
            if let Some((target, t)) = defended {
                let range = self.desired_distance;
                self.move_to_object(target, (t.state, t.class, t.position), range, side, ai)?;
            } else {
                let m = self.move_to(AiMove {
                    point: self.target_point,
                    range: self.desired_distance,
                    force: true,
                    walk: false,
                });
                if let Some(m) = m {
                    side.effect_now(Effect::Move(m), self, ai)?;
                }
            }
            return Ok(true);
        }
        if let Some((target, _)) = defended {
            match side.defended_attacker(target)? {
                NearestAttacker::Attacker(attacker) => {
                    let _ = self.attack_slot(attacker, true, at, side, ai)?;
                    return Ok(true);
                }
                NearestAttacker::NoAttacker => return self.process_idle(at, computer, ai, side),
                NearestAttacker::NoUnitAi => {}
            }
        }
        Ok(false)
    }

    /// processMisc's follow order (`0x140c567c0`, "I have a follow order for Unit #%d"): a
    /// target gone or dying (above state 2) or on a tile the unit's player does not see now ends
    /// it ("Target is gone or dying, stopping.", "He's NOT on a visible tile, stopping."): the
    /// defend target `+0x60` and the target go (`FUN_140c5e1e0`), the current action
    /// stops (slot 0xb0 with 1) and the order is −1. On action `0x2bf` or `0x2c0` the target
    /// point becomes (−1, −1). Otherwise a target more than a tile off the AI's target point on
    /// either axis gives the point its place (x, y, z); when the footprints' squared gap
    /// (`FUN_14118f8c0`) then exceeds the desired distance squared, the AI moves to it
    /// ([`UnitAi::move_to_object`] within the desired distance). True when the check ends there.
    pub(super) fn follow(
        &mut self,
        at: &Surroundings,
        side: &mut dyn UnitSide,
        rng: &mut Well512,
    ) -> Result<bool, GenieError> {
        // the world holds the current target (`at.target`) only when the AI has one
        let (Some(id), Some(target)) = (
            self.current_target,
            at.target
                .filter(|t| t.state <= ObjectState::Alive && t.visible),
        ) else {
            self.defend_target = None;
            self.remove_target();
            self.stop_current(true, at, side, rng)?;
            self.current_order = None;
            return Ok(true);
        };
        if matches!(
            self.current_action,
            Some(ActionKind::MoveAside | ActionKind::GroupAside)
        ) {
            self.target_point = self.target_point.with_x(-1.0).with_y(-1.0);
            return Ok(false);
        }
        let dx = target.position.x() - self.target_point.x();
        let dy = target.position.y() - self.target_point.y();
        let moved = dx < FOLLOW_SLACK_LOW
            || FOLLOW_SLACK_HIGH < dx
            || dy < FOLLOW_SLACK_LOW
            || FOLLOW_SLACK_HIGH < dy;
        if !moved {
            return Ok(false);
        }
        self.target_point = target.position;
        let gap = crate::movement::footprint_gap_squared(
            at.position,
            at.size,
            target.position,
            target.radius,
        );
        let range = self.desired_distance;
        if range.squared() < gap {
            let held = (target.state, target.class, target.position);
            self.move_to_object(id, held, range, side, rng)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// The AI's move to an object (slot 0x138, `FUN_140c55490`, its fourth argument 1 as its only
    /// caller, the follow order, passes it): the unit's slot 0x348 ([`Effect::MoveToObject`])
    /// takes a target the world holds, and the AI the move action and the target with its class
    /// and point (`FUN_140c64c10`; the target and class it had become the last). The routine's
    /// branch for a target the world does not hold after slot 0x348 cannot run: slot 0x348
    /// (`FUN_141086440`) answers 0 for such a target, and the routine then changes nothing.
    pub(super) fn move_to_object(
        &mut self,
        id: ObjectId,
        (state, class, position): (ObjectState, ClassId, Position),
        range: Distance,
        side: &mut dyn UnitSide,
        rng: &mut Well512,
    ) -> Result<(), GenieError> {
        side.effect_now(
            Effect::MoveToObject {
                target: id,
                state,
                range,
            },
            self,
            rng,
        )?;
        self.set_action(Some(ActionKind::Move));
        self.target_at(Some(id), Some(class), position);
        Ok(())
    }

    /// `setRandomizedRetargetTimer` (`FUN_140c64cf0`): the retarget timer (`+0x1cc`) becomes a
    /// draw of the AI stream (`rand15`) modulo half of `span` milliseconds (at least 1000) plus
    /// three quarters of it, so between three and five quarters of it.
    pub fn set_randomized_retarget_timer(&mut self, span: Millis, ai: &mut Well512) {
        let span = span.max(RETARGET_SPAN_LEAST).get();
        self.retarget_timer =
            Millis::new((ai.rand15() % (span >> 1)).wrapping_add(span.wrapping_mul(3) >> 2));
    }

    /// The boar's end of its full update: without a target it is no longer furious.
    pub(super) fn calm_down(&mut self) {
        let calm = self.current_target.is_none();
        if let Some(home) = self.home.as_mut().filter(|_| calm) {
            home.furious = false;
        }
    }

    /// The look-around check at the end of the full update (`LAB_140c6809c`): a timer past its
    /// timeout restarts with a timeout drawn around 5 s.
    pub(super) fn look_around(
        &mut self,
        computer: bool,
        ai: &mut Well512,
    ) -> Result<(), GenieError> {
        if self.look_around_timer > self.look_around_timeout {
            // the human branch; `FUN_14152b470` sends players allied with a computer elsewhere
            if computer {
                return Err(GenieError::unsupported(
                    "look-around timer of a computer player",
                ));
            }
            self.look_around_timer = Millis::ZERO;
            self.look_around_timeout = Millis::new(4500 + ai.rand15() % 1000);
        }
        Ok(())
    }
}
