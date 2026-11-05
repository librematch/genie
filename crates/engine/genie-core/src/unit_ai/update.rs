//! The full update: its order's tail, its target's block and the hunters' and gatherers'
//! searches.

use crate::gathering::DropSiteId;
use crate::math::rng::Well512;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::WorkerCount;
use crate::objects::masters::ClassId;
use crate::objects::masters::GatherGroup;
use crate::objects::masters::MasterKind;
use crate::ownership::CaptureKind;
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::players::QueryRelation;
use crate::unit_ai::AiAction;
use crate::unit_ai::AiCommand;
use crate::unit_ai::IdleOutcome;
use crate::unit_ai::OrderForce;
use crate::unit_ai::OrderKind;
use crate::unit_ai::OrderPriority;
use crate::unit_ai::OrderTest;
use crate::unit_ai::Pace;
use crate::unit_ai::StopOrder;
use crate::unit_ai::SurroundingFlags;
use crate::unit_ai::world_view::UnitView;
use crate::unit_ai::{
    AfterOrder, AiMove, BREAK, CIVILIAN_CARED, DEAD, Effect, GO_ON, Gatherable, GroupState,
    LIVE_TARGET_ACTIONS, Memory, NO_POINT, Notify, Pick, ResourceSearch, Retarget, Search,
    Surroundings, UnitAi, civilian_kind, civilian_search_radius, retarget_kind,
};
use crate::world::Millis;
use crate::world::WorldTime;
use crate::world::layer::ClassRoles;
use crate::world::slots::CheckedIndexExt;
use crate::{GenieError, GenieResult};

/// What the full update's tail takes besides the surroundings ([`UnitAi::update_tail`]'s
/// arguments, `0x140c66b10` from its slot 73 call on).
#[derive(Clone, Copy, Debug)]
pub(super) struct UpdateTail {
    /// Which retarget check slot 73 runs.
    pub(super) retarget: Retarget,
    /// The world's time.
    pub(super) world_time: WorldTime,
    /// The owner's type (`player+0x10`; [`PlayerKind::is_computer`] is `FUN_141659cf0`).
    pub(super) player_kind: PlayerKind,
    /// The unit was busy after the notifies, as the update read it before its order queue.
    pub(super) busy: bool,
    /// The unit's group as the update read it.
    pub(super) group: Option<GroupState>,
}

impl UnitAi {
    /// The full update (slot 4, `0x140c66b10`; the boar's `0x140c2c010` adds its tail):
    /// `dt` is the time the owner accumulated since its last turn (`world+0x18c[player]`),
    /// `player_type` the owner's type (`player+0x10`). In the game's order: the target
    /// block ([`UnitAi::target_block`]), the state position of a unit with an order, the timers, the notifies ([`UnitAi::post`]: each through
    /// slot 0x238, then `FUN_140c5dc10`), the reset of an action value whose actions ended, the
    /// order check of a unit no longer busy, the order queue ([`UnitAi::run_orders`]), slot 73's
    /// retarget check, the idle part (slot 72 once the idle timer passes its timeout), the
    /// look-around check. What it does to the unit are calls on its side at once
    /// ([`UnitView::effect_now`]); an order on a target ends it, returning the rest, which the
    /// caller runs after the order ([`UnitAi::after_order`]). A unit in a group ([`Surroundings::group`]) keeps its state position
    /// while the group has a command, runs its order queue only while the group is idle, its idle
    /// check only while the group is idle and the unit still, and takes the patrol branches while
    /// the group patrols. Its start lets the expired retarget entries go
    /// ([`UnitAi::expire_retries`]) and forgets the update's best unit to attack
    /// ([`UnitAi::best_this_update`]).
    pub(crate) fn update(
        &mut self,
        dt: Millis,
        world_time: WorldTime,
        player_kind: PlayerKind,
        at: &Surroundings,
        ai: &mut Well512,
        side: &mut UnitView<'_, '_>,
    ) -> GenieResult<Option<AfterOrder>> {
        let retarget = retarget_kind(self.ai_kind)?;
        self.ordered_on_target = false;
        self.best_this_update = None;
        // the full update's start: the time since the last enemy sighting grows by the world's dt
        self.time_since_enemy += at.dt_seconds;
        self.expire_retries(world_time, &mut |id| Self::world_object(id, at, side))?;
        self.target_block(at, player_kind, ai, side)?;
        self.keep_attackers(at, side);
        // a unit with an order, or in a group with a command (`+0x3a8`), keeps its place as its
        // state position (0x140c674c9)
        if self.current_order.is_some()
            || at
                .group
                .is_some_and(|group_state| group_state.contains(GroupState::COMMANDED))
        {
            self.state_position = at.position.ground();
        }
        self.secondary_timer = self.secondary_timer.saturating_sub(dt);
        self.look_around_timer = self.look_around_timer.wrapping_add(dt);
        self.idle_timer = self.idle_timer.wrapping_add(dt);
        let result = self.process_notifies(dt, at, player_kind, ai, side)?;
        if result == DEAD {
            self.boar_tail(at, player_kind, ai, side)?;
            return Ok(None);
        }
        self.end_notifies(dt, at, side, ai)?;
        // slot 0x430 again: the update's stops emptied the action list, its commands filled it
        let busy = side.busy_now();
        if !busy {
            self.reset_ended_action(at);
            if self.current_order.is_some() {
                self.check_order(at, ai);
            }
        }
        // slot 0x2d0 as the action list stands, which only a queue with orders reads
        if self.current_action.is_none()
            && (!busy || (!self.orders.is_empty() && side.interruptible_now()?))
            && result != 0xd
            && self.can_order(at, OrderTest::Any)
            && at
                .group
                .is_none_or(|group_state| group_state.contains(GroupState::IDLE))
            && !at.flags.contains(SurroundingFlags::GARRISONED)
        {
            self.run_orders(at, side, player_kind, ai)?;
        }
        // an order on a target ran its work here (slot 0x230's case 0x2db, the unit's slot
        // 0x128): the rest of the update reads what it gave the AI ([`UnitAi::after_order`])
        if self.ordered_on_target {
            return Ok(Some(AfterOrder {
                busy,
                group: at.group,
            }));
        }
        let tail = UpdateTail {
            retarget,
            world_time,
            player_kind,
            busy,
            group: at.group,
        };
        self.update_tail(tail, at, ai, side)?;
        Ok(None)
    }

    /// The full update's rest after the order on a target it took ([`AfterOrder`]), which the
    /// game runs after the order's work inside the update, and the unit's update once it ran
    /// the order:
    /// [`UnitAi::update`]'s tail with what the order gave the AI and the unit, `at` its
    /// surroundings then; the idle part reads the update's own `busy` and group from before the
    /// order.
    ///
    /// # Errors
    ///
    /// What the tail does not model.
    pub(crate) fn after_order(
        &mut self,
        after: AfterOrder,
        world_time: WorldTime,
        player_kind: PlayerKind,
        at: &Surroundings,
        ai: &mut Well512,
        side: &mut UnitView<'_, '_>,
    ) -> GenieResult<()> {
        let retarget = retarget_kind(self.ai_kind)?;
        let tail = UpdateTail {
            retarget,
            world_time,
            player_kind,
            busy: after.busy,
            group: after.group,
        };
        self.update_tail(tail, at, ai, side)
    }

    /// The full update after its order queue (`0x140c66b10` from its slot 73 call on): slot 73
    /// (the base's retarget check [`UnitAi::retarget_check`], the villager's
    /// [`UnitAi::civilian_misc`], the artifact's [`UnitAi::artifact_misc`], livestock's
    /// nothing), the idle part ([`UnitAi::idle_part`]), the look-around check and the boar's
    /// tail. `busy` and `group` are what the update read before its order queue.
    pub(super) fn update_tail(
        &mut self,
        tail: UpdateTail,
        at: &Surroundings,
        ai: &mut Well512,
        side: &mut UnitView<'_, '_>,
    ) -> GenieResult<()> {
        let UpdateTail {
            retarget,
            world_time,
            player_kind,
            busy,
            group,
        } = tail;
        match retarget {
            Retarget::Base => {
                let _ = self.retarget_check(world_time, player_kind, at, ai, side)?;
            }
            // the priest's checks but on the order on a target and for a computer player's
            Retarget::Priest => {
                if self.current_order != Some(OrderKind::OnTarget) && !player_kind.is_computer() {
                    let _ = self.retarget_check(world_time, player_kind, at, ai, side)?;
                }
            }
            Retarget::Civilian => self.civilian_misc(at, side, ai)?,
            Retarget::Never => {}
            Retarget::Artifact => self.artifact_misc(at, side, ai)?,
        }
        self.idle_part(busy, group, at, player_kind, ai, side)?;
        self.look_around(player_kind, ai)?;
        self.top_target_tail(at, side, ai)?;
        self.drop_started_builds(side)?;
        self.boar_tail(at, player_kind, ai, side)
    }

    /// The full update's tail on the target of its unit's top action (slot 400 at `0x140c684fc`,
    /// after the look-around check), as the action list stands after the update's stops and
    /// commands ([`UnitView::top_target_now`]). A Gaia unit
    /// whose target is a Gaia object of type 70 or above that another player can take more than
    /// once (`+0x924` 2) lets it go ("Tasking GAIA unit to stop attacking an object which can be
    /// captured multiple times"): the target goes (`FUN_140c5e1e0`) and the action stops (slot
    /// 0xb0 with 1). Then a target outside a pasture (`+0x790` −1) that cannot be targeted
    /// (`+0x689` 0), or cannot be attacked (`+0x68a` 0) while the action value is the attack's
    /// (600), does the same ("Tasking unit to stop attacking an object which has no targeting
    /// flag enabled") but on the trade action (`0x267`); it reads the action value the first
    /// stop left.
    pub(super) fn top_target_tail(
        &mut self,
        at: &Surroundings,
        side: &mut UnitView<'_, '_>,
        rng: &mut Well512,
    ) -> GenieResult<()> {
        let id = side.top_target_now();
        let Some(target_flags) = id.map(|id| side.target_flags(id)).transpose()?.flatten() else {
            return Ok(());
        };
        let captured_again = at.player == PlayerId::GAIA
            && target_flags.owner == PlayerId::GAIA
            && MasterKind::COMBAT <= target_flags.kind
            && target_flags.capture == CaptureKind::Again;
        let action = if captured_again {
            None
        } else {
            self.current_action
        };
        let untargeted = target_flags.pasture.is_none()
            && (target_flags.untargetable
                || (!target_flags.attackable && action == Some(AiAction::Attack)))
            && action != Some(AiAction::Trade);
        if captured_again {
            self.remove_target();
            self.stop_current(StopOrder::Keep, at, side, rng)?;
        }
        if untargeted {
            self.remove_target();
            self.stop_current(StopOrder::Keep, at, side, rng)?;
        }
        Ok(())
    }

    /// The end of the full update (`0x140c686e0`): unless the AI's order is the build order
    /// (`0x2be`), each queued build order, the last first, whose foundation the world no longer
    /// holds or holds in a state other than 0 (`+0x640`) leaves the queue.
    pub(super) fn drop_started_builds(&mut self, side: &UnitView<'_, '_>) -> GenieResult<()> {
        if self.current_order == Some(OrderKind::Build) {
            return Ok(());
        }
        for index in (0..self.orders.len()).rev() {
            let order = *self.orders.checked(index)?;
            if order.order != OrderKind::Build {
                continue;
            }
            // none (−1) is a foundation the world does not hold
            let seen = match order.target {
                Some(object_id) => side.seen_unit(object_id)?,
                None => None,
            };
            if seen.is_none_or(|unit_seen| unit_seen.state != ObjectState::Foundation) {
                let _ = self.orders.remove(index);
            }
        }
        Ok(())
    }

    /// The boar's slot 4 (`0x140c2c010`) after the base, however that ended: its second idle
    /// check runs only for a timer the base's left past the timeout (a unit with an action), and
    /// a boar without a target calms down.
    pub(super) fn boar_tail(
        &mut self,
        at: &Surroundings,
        player_kind: PlayerKind,
        ai: &mut Well512,
        side: &mut UnitView<'_, '_>,
    ) -> GenieResult<()> {
        if let Some(home) = self.home
            && !home.furious
            && self.adjusted_idle_timeout < self.idle_timer
        {
            self.idle_timer = Millis::ZERO;
            let _ = self.process_idle(at, player_kind, ai, side)?;
        }
        self.calm_down();
        Ok(())
    }

    /// The target block at the full update's start (`0x140c66b10`): a unit inside another
    /// (`+0x2f0`) drops its target, as does one whose target no longer exists (an attack action's
    /// idle timer then takes its timeout; `FUN_140c5e1e0` finds no target left). A dead target (a
    /// state above 2, not a building) of a live unit ends an action that needs a live one
    /// ([`LIVE_TARGET_ACTIONS`]), an attack whose top action is no longer in its state 2 (slot
    /// 0x2b0: the swing on the kill) or any action with `+0x1a8` set: `+0x1a8` clears; an attack
    /// looks for its next target ([`UnitAi::next_after_kill`]); but for a hunt or `0x2be`, any
    /// other action loses its target and stops (slot 0xb0 with 1). The range test of a target on
    /// a patrol order or in a patrolling group (`FUN_140c41690`) answers 0 for a player that is
    /// not a computer; a computer's (the ship and priority rules), a computer's priority by the
    /// target's distance and action `0x2be`'s check (`FUN_140c40f10`) are not modelled.
    pub(super) fn target_block(
        &mut self,
        at: &Surroundings,
        player_kind: PlayerKind,
        ai: &mut Well512,
        side: &mut UnitView<'_, '_>,
    ) -> GenieResult<()> {
        if self.current_target.is_some() && at.flags.contains(SurroundingFlags::GARRISONED) {
            self.current_target = None;
        }
        if self.current_target.is_none() {
            return Ok(());
        }
        let action = self.current_action;
        match at.target {
            None => {
                self.current_target = None;
                if action == Some(AiAction::Attack) {
                    self.idle_timer = self.idle_timeout;
                }
            }
            Some(seen) => {
                if at.state == ObjectState::Alive
                    && ObjectState::Alive < seen.state
                    && seen.kind != MasterKind::BUILDING
                {
                    let attack_over = action == Some(AiAction::Attack)
                        && at
                            .action_state
                            .ok_or(GenieError::unsupported(
                                "an attack whose target died, its top action not held",
                            ))?
                            .get()
                            != 2;
                    if self.stop_after_kill
                        || action.is_some_and(|ai_action| LIVE_TARGET_ACTIONS.contains(&ai_action))
                        || attack_over
                    {
                        self.stop_after_kill = false;
                        if action == Some(AiAction::Attack) {
                            self.next_after_kill(at, player_kind, ai, side)?;
                        } else if !matches!(action, Some(AiAction::Hunt | AiAction::BoarLuring)) {
                            self.remove_target();
                            self.stop_current(StopOrder::Keep, at, side, ai)?;
                        }
                    }
                }
                // on a patrol order, or in a group that patrols (`FUN_141017c40`,
                // `FUN_141017c30`), FUN_140c41690 tests the target's range; for a player that
                // is not a computer (FUN_141659cf0) it answers 0 at once
                if player_kind.is_computer()
                    && (matches!(
                        self.current_order,
                        Some(OrderKind::Patrol | OrderKind::AttackMove)
                    ) || at
                        .group
                        .is_some_and(|group_state| group_state.contains(GroupState::PATROL)))
                {
                    return Err(GenieError::unsupported(
                        "the patrol range of a unit AI's target",
                    ));
                }
                // a computer's order 0x262 (MOVE's action value) of priority 0..100 rechecks its
                // target's distance: no order holds 0x262 ([`OrderKind`])
            }
        }
        if self.current_action == Some(AiAction::BoarLuring) {
            return Err(GenieError::unsupported("action 0x2be's target check"));
        }
        Ok(())
    }

    /// The villager's case 0x202 on its hunt (`FUN_140c22bf0`, action 0x265): the class to hunt is
    /// the notify's (its second parameter), else the AI's target class, else its last; with none
    /// the target goes and the action stops (slot 0xb0 with 1; slot 0x48 hands the notify to the
    /// owner, not held). Of a class the civilian's slot 0x1b0 gives no kind, `ClosestObject`
    /// (every relation, below one worker for class 49) over the civilian's search
    /// ([`civilian_search_radius`]) names the next target, which it hunts (`HuntObject`, slot 0xc8
    /// with 1, [`UnitAi::hunt_object`]) and the processing breaks; finding none, a human player's
    /// villager does nothing more. A class the slot names a kind of searches by it (slot 0x80,
    /// `ClosestResourceObject` from the notify's drop site, its third parameter: kind 0 below 2
    /// workers, the others any; [`UnitView::closest_resource`]). A farm found (class 49) the unit
    /// works ([`UnitAi::task_to_farm`]). A computer player's villager that found nothing is not
    /// modelled.
    pub(super) fn hunter_search(
        &mut self,
        notify: Notify,
        at: &Surroundings,
        player_kind: PlayerKind,
        side: &mut UnitView<'_, '_>,
        rng: &mut Well512,
    ) -> GenieResult<u32> {
        let roles = &at.roles;
        let class = ClassId::of_param(notify.params[1])?
            .or(self.target_class)
            .or(self.last_target_class);
        let Some(class) = class else {
            self.remove_target();
            self.stop_current(StopOrder::Keep, at, side, rng)?;
            return Ok(GO_ON);
        };
        let kind = civilian_kind(class, roles);
        let found = match kind {
            None => {
                let found = side.closest(&Search {
                    at: at.position.ground(),
                    radius: civilian_search_radius(at.los, at.search_radius),
                    classes: &CIVILIAN_CARED,
                    size: at.size,
                    pick: Pick::Closest {
                        group: Some(class),
                        relation: None,
                        state: None,
                        master: None,
                        workers: if roles.of_class(class).contains(ClassRoles::FARM) {
                            WorkerCount::ONE
                        } else {
                            WorkerCount::NO_CAP
                        },
                        spared: None,
                        reach: true,
                    },
                    memory: Memory::default(),
                })?;
                self.alert(found, at.dt_seconds);
                match (found.id, found.position) {
                    (Some(id), Some(position)) => Some(Gatherable {
                        id,
                        position,
                        class,
                    }),
                    (Some(_), None) => {
                        return Err(GenieError::unsupported("a found object without its place"));
                    }
                    (None, _) => None,
                }
            }
            Some(kind) => {
                // slot 0x80 from the notify's drop site: kind 0 below 2 workers, any other any
                side.closest_resource(ResourceSearch {
                    kind,
                    cap: if kind == GatherGroup::WOOD {
                        WorkerCount::of_data(2)
                    } else {
                        WorkerCount::NO_CAP
                    },
                    site: DropSiteId::of_field(notify.params[2]),
                    current: self.current_target,
                    point: NO_POINT,
                    min_state: ObjectState::Alive,
                })?
            }
        };
        let Some(found) = found else {
            if player_kind.is_computer() {
                return Err(GenieError::unsupported(
                    "a computer player's hunter that found nothing",
                ));
            }
            return Ok(GO_ON);
        };
        if roles.of_class(class).contains(ClassRoles::FARM) {
            return self.task_to_farm(found.id, at, side, rng);
        }
        self.hunt_object(found.id, found.class, found.position, at, side, rng)?;
        Ok(BREAK)
    }

    /// The hunt's farm found (class 49, `FUN_140c22bf0`'s case 0x202 on action 0x265): the world's
    /// object of its id (`FUN_140c54e30`), at whose place the unit works it after the AI's stop
    /// (slot 0xb0 with 1; the unit's slot 0x140, `FUN_141070c90`: slot 0x7e8 with the
    /// carried-resource test, any task and `+0xa14` 0, [`AiCommand::WorkOn`]; its count `+0x888`
    /// is not held), and the processing breaks. A farm the world does not hold ("Failed to be
    /// tasked to new farm.") loses the target, stops the AI and goes on (slot 0x48 hands the
    /// notify to the owner, not held).
    fn task_to_farm(
        &mut self,
        farm: ObjectId,
        at: &Surroundings,
        side: &mut UnitView<'_, '_>,
        rng: &mut Well512,
    ) -> GenieResult<u32> {
        let Some(held) = Self::world_object(farm, at, side)? else {
            self.remove_target();
            self.stop_current(StopOrder::Keep, at, side, rng)?;
            return Ok(GO_ON);
        };
        self.stop_current(StopOrder::Keep, at, side, rng)?;
        side.effect_now(
            Effect::Command(AiCommand::WorkOn {
                target: held.id,
                point: held.position,
            }),
            self,
            rng,
        )?;
        Ok(BREAK)
    }

    /// The villager's case 0x202 on its gather (`FUN_140c22bf0`, action 0x261): the class to
    /// gather is the AI's target class, else its last, else the notify's (its second parameter);
    /// with none the target goes and the action stops (slot 0xb0 with 1; slot 0x48 hands the
    /// notify to the owner, not held). The civilian's slot 0x1b0 names its gather group: by it
    /// `ClosestResourceObject` (slot 0x80, [`UnitView::closest_resource`]) from the notify's drop
    /// site (its third parameter), below 4 workers for stone and gold (groups 3 and 4); a class
    /// without a group takes `ClosestObject` (every relation, any workers) over the civilian's
    /// search ([`civilian_search_radius`]). What it finds it gathers (`GatherObject`, slot 0xd0
    /// with 1, [`UnitAi::gather_object`]) and the processing breaks; finding nothing, a human
    /// player's villager does nothing more, a computer player's stops as above. For a farm (the
    /// notify's class 49) `ClosestObject`'s ninth argument is 0: friends' farms pass.
    pub(super) fn gatherer_search(
        &mut self,
        notify: Notify,
        at: &Surroundings,
        player_kind: PlayerKind,
        side: &mut UnitView<'_, '_>,
        rng: &mut Well512,
    ) -> GenieResult<u32> {
        let roles = &at.roles;
        let noted = ClassId::of_param(notify.params[1])?;
        let class = self.target_class.or(self.last_target_class).or(noted);
        let Some(class) = class else {
            self.remove_target();
            self.stop_current(StopOrder::Keep, at, side, rng)?;
            return Ok(GO_ON);
        };
        let kind = civilian_kind(class, roles);
        let found = match kind {
            None => {
                // "Calling closest object for a farm that's alive": the friends' pass over
                let spared = (roles.of_some_class(noted).contains(ClassRoles::FARM))
                    .then_some(QueryRelation::Friends);
                let found = side.closest(&Search {
                    at: at.position.ground(),
                    radius: civilian_search_radius(at.los, at.search_radius),
                    classes: &CIVILIAN_CARED,
                    size: at.size,
                    pick: Pick::Closest {
                        group: Some(class),
                        relation: None,
                        state: None,
                        master: None,
                        workers: WorkerCount::NO_CAP,
                        spared,
                        reach: true,
                    },
                    memory: Memory::default(),
                })?;
                self.alert(found, at.dt_seconds);
                match (found.id, found.position) {
                    (Some(id), Some(position)) => Some(Gatherable {
                        id,
                        position,
                        class,
                    }),
                    (Some(_), None) => {
                        return Err(GenieError::unsupported("a found object without its place"));
                    }
                    (None, _) => None,
                }
            }
            Some(kind) => {
                let cap = if matches!(kind, GatherGroup::STONE | GatherGroup::GOLD) {
                    WorkerCount::of_data(4)
                } else {
                    WorkerCount::NO_CAP
                };
                side.closest_resource(ResourceSearch {
                    kind,
                    cap,
                    site: DropSiteId::of_field(notify.params[2]),
                    current: self.current_target,
                    point: NO_POINT,
                    min_state: ObjectState::Alive,
                })?
            }
        };
        let Some(found) = found else {
            if player_kind.is_computer() {
                self.remove_target();
                self.stop_current(StopOrder::Keep, at, side, rng)?;
            }
            return Ok(GO_ON);
        };
        self.gather_object(found, side, rng)?;
        Ok(BREAK)
    }

    /// `GatherObject` (slot 0xd0, `FUN_140c4dd50`) forced on what the search found: the unit's
    /// gather command runs on it ([`AiCommand::GatherObject`]); taken as done (the caller stops on a
    /// failure): the gather action value (`0x261`) becomes current, the last the one before, the
    /// target with its class and place (the last ones kept).
    pub(super) fn gather_object(
        &mut self,
        found: Gatherable,
        side: &mut UnitView<'_, '_>,
        rng: &mut Well512,
    ) -> GenieResult<()> {
        side.effect_now(
            Effect::Command(AiCommand::GatherObject(found.id)),
            self,
            rng,
        )?;
        self.set_action(Some(AiAction::Gather));
        self.set_target(Some(found.id), found.class.or_none(), found.position);
        Ok(())
    }

    /// The target block's attack whose target died (`0x140c66b10`): processIdle (slot 0x240 with
    /// 0, [`UnitAi::process_idle`]) may find what to do next (6); with nothing (5) the AI
    /// moves to its old target's point within the unit's weapon range (slot 0x130, forced,
    /// running: it always takes it). Then the order ends: it becomes the last order, the order and
    /// its priority −1, a patrol order (711, 733) resetting its timers
    /// ([`UnitAi::leave_patrol_timers`]).
    pub(super) fn next_after_kill(
        &mut self,
        at: &Surroundings,
        player_kind: PlayerKind,
        ai: &mut Well512,
        side: &mut UnitView<'_, '_>,
    ) -> GenieResult<()> {
        let produced = self.process_idle(at, player_kind, ai, side)?;
        if produced == IdleOutcome::Idle {
            let move_to = self.move_to(AiMove {
                point: self.target_point,
                range: at.weapon_range,
                force: OrderForce::Forced,
                pace: Pace::Run,
            });
            if let Some(ai_move) = move_to {
                side.effect_now(Effect::Move(ai_move), self, ai)?;
            }
        }
        self.leave_patrol_timers(ai);
        self.last_order = self.current_order;
        self.current_order = None;
        self.order_priority = OrderPriority::NONE;
        Ok(())
    }
}
