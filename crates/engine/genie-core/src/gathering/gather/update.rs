//! One update (slot 6, `0x140a8e1d0`) in states 4, 6 and 7: the walk's `MoveTo`, the units
//! in the way and the teardown.

use crate::combat::attack::Aim;
use crate::combat::attack::face_target;
use crate::combat::damage::Victim;
use crate::gathering::gather::GatherState;
use crate::gathering::gather::{
    GatherAction, GatherReads, Next, SEARCH_WAIT, StopRate, waits_for_farm,
};
use crate::gathering::hunt::asked_drop_off;
use crate::gathering::hunt::walk_sprite;
use crate::map::Distance;
use crate::math::rng::Stream;
use crate::movement::footprint_gap_squared;
use crate::movement::move_to::Patience;
use crate::movement::move_to::SubCode;
use crate::movement::move_to::WalkTo;
use crate::movement::move_to::{GiveUp, MoveTo};
use crate::movement::move_to::{ListUpdate, update_sub_actions};
use crate::movement::{Me, Mover};
use crate::objects::ActionState;
use crate::objects::ObjectState;
use crate::objects::WorkerCount;
use crate::objects::sprite::Graphic;
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::players::attribute::AttributeId;
use crate::world::Seconds;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::layer::Slots;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};

/// A gatherer without movement state, which the Gather's walks and turns need.
pub(super) const UNMOVING: GenieError =
    GenieError::unsupported("a gatherer without movement state");

impl GatherAction {
    /// The object the gather's `MoveTo` walks to, which the update holds as `target`, the unit
    /// `gap` from its footprint.
    const fn walk_to(&self, target: &Victim, gap: Distance) -> WalkTo {
        WalkTo {
            id: Some(target.id),
            at: target.position,
            gap,
            state: Some(target.state),
        }
    }

    /// Slot 0x78 (`FUN_140a78060`, the base action's, [`crate::gathering::hunt::asked_drop_off`] on the
    /// task's resource): true when the unit's load asks for `set_state(8)`; false for a load of
    /// another resource (the unit AI takes notify 0x21c, the caller's).
    #[must_use]
    pub fn drop_off_asked(
        &self,
        held_kind: Option<AttributeId>,
        attributes: usize,
        slots: Slots,
    ) -> bool {
        asked_drop_off(self.task.resource, held_kind, attributes, slots)
    }

    /// The teardown (`FUN_140a73fc0`) and `set_state`'s first step (`FUN_140a98620`): the
    /// `MoveTo` goes and stops the unit (slot 0x5d0); the caller releases the targets.
    ///
    /// # Errors
    ///
    /// The unit's stop that the motion step does not model.
    pub fn tear_down(&mut self, mover: Option<&mut Mover>) -> GenieResult<()> {
        if self.state.take_move_to().is_some()
            && let Some(m) = mover
        {
            m.stop()?;
        }
        Ok(())
    }

    /// The sprite the walks show (`FUN_140a8b660`, slot 0x68 with it): the task's carrying sprite
    /// while the unit carries at least 1 of the task's resource, else its moving one.
    pub(super) fn walk_sprite(&self, unit: &Unit, gather_reads: &GatherReads) -> Option<Graphic> {
        walk_sprite(self.task.resource, unit.life.held(), &gather_reads.sprites)
    }

    /// One update (slot 6, `0x140a8e1d0`) of `unit`'s Gather in states other than 8, `target`
    /// the target the caller holds (None once it went). State 4 walks the `MoveTo`
    /// (`FUN_140a992b0`); its end (codes 1 and 2) is `set_state(6)`. State 6 faces the target
    /// (`FUN_141162db0`) and, while it holds something, `set_state(7)`. State 7 gathers
    /// ([`GatherAction::gather`]). Without a target a loaded gatherer takes its load off
    /// ([`Next::Drop`]). The world's dt (`step.dt`) runs its timers.
    ///
    /// # Errors
    ///
    /// What the update does not model.
    pub(crate) fn update(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        me: Me,
        target: Option<&mut Victim>,
        gather_reads: &GatherReads,
        roles: &Roles,
    ) -> GenieResult<Next> {
        // a farm's time per stop follows the gatherer's work rate, every update
        if target.as_deref().is_some_and(|victim| {
            roles
                .of_master(victim.master_id)
                .contains(MasterRoles::STOP_FARM)
        }) {
            self.time_per_stop = StopRate {
                work_rate: gather_reads.work_rate,
                capacity: gather_reads.capacity,
                productivity: gather_reads.stop_productivity,
            }
            .time_per_stop(self.task.work_value, self.stops);
        }
        let dt = step.dt.seconds();
        match self.state {
            GatherState::Done => return Ok(Next::Done),
            GatherState::Idle => return Ok(Next::Stay),
            GatherState::Moved { .. } => {
                return self.update_moved(step, unit, me, gather_reads, roles);
            }
            GatherState::WalkBack { .. } => {
                return self.update_walk_back(step, unit, me, gather_reads, roles);
            }
            GatherState::FarmWait => {
                return self.update_farm_wait(step, unit, target.as_deref(), gather_reads, roles);
            }
            GatherState::ReseedWalk { .. } => {
                return self.update_reseed_walk(
                    step,
                    unit,
                    me,
                    target.as_deref(),
                    gather_reads,
                    roles,
                );
            }
            GatherState::Search => return Ok(self.update_search(dt)),
            _ => {}
        }
        let Some(target) = target.filter(|_| self.target.is_some()) else {
            return self.update_without_target(step, unit, gather_reads, roles);
        };
        match self.state {
            GatherState::Walk { .. } if self.timer < Seconds::ZERO => {
                self.update_walk_wait(step, unit, target, dt, gather_reads)
            }
            GatherState::Walk { .. } => {
                self.update_walk(step, unit, me, target, gather_reads, roles)
            }
            GatherState::Face => self.update_face(step, unit, target, gather_reads, roles),
            GatherState::Work => self.update_work(step, unit, target, gather_reads, roles),
            _ => Err(GenieError::unsupported(
                "a Gather action in a state the slice does not model",
            )),
        }
    }

    /// Case 2, the search's wait: the timer counts up by the world's dt; at 0 it waits again
    /// ([`SEARCH_WAIT`]) and asks again ([`Next::Search`]).
    fn update_search(&mut self, dt: Seconds) -> Next {
        if self.timer < Seconds::ZERO {
            self.timer += dt;
            if Seconds::ZERO <= self.timer {
                self.timer = SEARCH_WAIT;
                return Next::Search;
            }
        }
        Next::Stay
    }

    /// Cases 3, 5 and 6 without a target: a load goes to its drop site ([`Next::Drop`]); a farmer
    /// waits for another farm (0xf3); else the search (`set_state(3)`). Any other state is not
    /// modelled.
    fn update_without_target(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        gather_reads: &GatherReads,
        roles: &Roles,
    ) -> GenieResult<Next> {
        if !matches!(
            self.state,
            GatherState::Walk { .. } | GatherState::Face | GatherState::Work
        ) {
            return Err(GenieError::unsupported(
                "a Gather action in a state the slice does not model",
            ));
        }
        if Amount::ZERO < unit.life.amount {
            return Ok(Next::Drop);
        }
        if waits_for_farm(gather_reads.master, self.target_class, roles) {
            return self.begin_farm_wait(step, unit, gather_reads);
        }
        self.begin_search(step, unit, gather_reads)
    }

    /// Case 3's wait after a failed walk (the timer below 0 counts up by dt): at its end a
    /// gatherer of one stop walks to its point to search there (`set_state(11)`, within the
    /// target's radius, 0 for a farm of stops), one of a farm's stops to the next (4).
    fn update_walk_wait(
        &mut self,
        step: &mut Step<'_>,
        unit: &Unit,
        target: &Victim,
        dt: Seconds,
        gather_reads: &GatherReads,
    ) -> GenieResult<Next> {
        self.timer += dt;
        if self.timer < Seconds::ZERO {
            return Ok(Next::Stay);
        }
        self.timer = Seconds::ZERO;
        let walks = gather_reads.walks(
            &step.w.model.graphics,
            (unit, step.w.parts.mover_of(unit.id)),
        );
        let mover = step.w.parts.mover_mut(unit.id).ok_or(UNMOVING)?;
        if self.stops.several() {
            let rng = step.w.streams.get(Stream::Action);
            let to = self.walk_to(target, Distance::ZERO);
            self.set_walk(to, target.radius, mover, rng, &walks, &step.w.model.tuning)?;
        } else {
            let radius = if step
                .w
                .model
                .roles
                .of_master(target.master_id)
                .contains(MasterRoles::STOP_FARM)
            {
                0.0
            } else {
                target.radius.x()
            };
            self.set_walk_back(
                Distance::new(radius),
                mover,
                &walks,
                &step.w.model.tuning.move_to,
            )?;
        }
        Ok(Next::Stay)
    }

    /// Case 5, the face: a farm not built yet is searched past (`set_state(3)`), a pasture short
    /// of its animals is not modelled, a farm another farmer works waits for another (0xf3; the
    /// search's notify is the caller's). Else the unit turns to its target (`FUN_141162db0`;
    /// turning over time it faces next update) and works (`set_state(7)`, its time at the stop
    /// 0) on a target holding something or one regathered; an empty one is not modelled.
    fn update_face(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        target: &Victim,
        gather_reads: &GatherReads,
        roles: &Roles,
    ) -> GenieResult<Next> {
        if roles.of_class(target.class).contains(ClassRoles::FARM) {
            if target.state < ObjectState::Alive {
                return self.begin_search(step, unit, gather_reads);
            }
            if roles
                .of_master(target.master_id)
                .intersects(MasterRoles::PASTURE_MANGROVE | MasterRoles::PASTURE)
            {
                return Err(GenieError::unsupported(
                    "a gather facing a pasture (its animals, slot 0x780)",
                ));
            }
            if WorkerCount::ONE < target.workers {
                return self.begin_farm_wait(step, unit, gather_reads);
            }
        }
        let at = unit.place.position();
        let mover = step.w.parts.mover_mut(unit.id).ok_or(UNMOVING)?;
        if face_target(mover, at, target.position, step.w.world_time)? == Aim::Turning {
            return Ok(Next::Stay);
        }
        if Amount::ZERO < target.amount
            || roles
                .of_master(target.master_id)
                .contains(MasterRoles::REGATHERED)
        {
            self.set_work(step, unit, gather_reads);
            self.time_at_stop = Seconds::ZERO;
            return Ok(Next::Stay);
        }
        Err(GenieError::unsupported(
            "a gather's target empty when faced (set_state 3)",
        ))
    }

    /// Case 6, the work: on a farm's stops (the game's `+0xc14` being 0) the time at the stop
    /// runs; past the time per stop with a stop left the farmer walks to the next (its count
    /// stays when the food is sold at once, attribute 96). Else it gathers
    /// ([`GatherAction::gather`]).
    fn update_work(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        target: &mut Victim,
        gather_reads: &GatherReads,
        roles: &Roles,
    ) -> GenieResult<Next> {
        let dt = step.dt.seconds();
        if self.stops.several() {
            self.time_at_stop += dt;
            if self.time_per_stop <= self.time_at_stop && self.current_stop.before_last(self.stops)
            {
                if gather_reads.food_sold <= Amount::ZERO {
                    self.current_stop = self.current_stop.next();
                }
                let walks = gather_reads.walks(
                    &step.w.model.graphics,
                    (unit, step.w.parts.mover_of(unit.id)),
                );
                let mover = step.w.parts.mover_mut(unit.id).ok_or(UNMOVING)?;
                let rng = step.w.streams.get(Stream::Action);
                let to = self.walk_to(target, Distance::ZERO);
                self.set_walk(to, target.radius, mover, rng, &walks, &step.w.model.tuning)?;
                return Ok(Next::Stay);
            }
        }
        self.gather(step, unit, target, gather_reads, roles)
    }

    /// State 4's sub-action list (case 3, [`GatherAction::update_list`]): its code 0 walks or
    /// waits; 1 and 2 (it ended: the list drops it and its destructor stops the unit) are
    /// `set_state(6)`. A failed `MoveTo` (3) near a target of more than 3 workers searches, else
    /// waits a random while. Its target let go (4, 5) lets the Gather's target go and walks back
    /// to its point (`set_state(11)` within the task's range). A quiet `MoveTo` never gives 4,
    /// and none gives 6 (the game's `set_state(13)` with the search's notify).
    pub(super) fn update_walk(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        me: Me,
        target: &Victim,
        gather_reads: &GatherReads,
        roles: &Roles,
    ) -> GenieResult<Next> {
        match self.update_list(step, unit, me, gather_reads, roles)? {
            SubCode::UnderWay => Ok(Next::Stay),
            SubCode::Done => {
                self.tear_down(step.w.parts.mover_mut(unit.id))?;
                self.set_face(step, unit, target, gather_reads, roles)
            }
            // code 3: near a target with more than 3 workers it searches (set_state 3), else it
            // waits a random while (LAB_140a8ef80)
            SubCode::Failed => {
                let mover = step.w.parts.mover_of(unit.id).ok_or(UNMOVING)?;
                let me = mover.stats.radius.ground();
                let at = unit.place.position();
                let gap = footprint_gap_squared(at, me, target.position, target.radius);
                if target.workers.as_f32() <= step.w.model.tuning.gather.wait_workers
                    || step.w.model.tuning.gather.wait_gap <= gap.get()
                {
                    self.random_wait(step, unit);
                    return Ok(Next::Stay);
                }
                self.begin_search(step, unit, gather_reads)
            }
            // its target let go: the Gather's goes too (slot 0x68 with none; the caller lets it
            // go), then `set_state(11)` with no target's radius
            SubCode::TargetGone => {
                self.target = None;
                let walks = gather_reads.walks(
                    &step.w.model.graphics,
                    (unit, step.w.parts.mover_of(unit.id)),
                );
                let mover = step.w.parts.mover_mut(unit.id).ok_or(UNMOVING)?;
                self.set_walk_back(Distance::ZERO, mover, &walks, &step.w.model.tuning.move_to)?;
                Ok(Next::Stay)
            }
        }
    }

    /// The Gather's sub-action list's update (`FUN_140a992b0`: its first action's, the quiet
    /// `MoveTo`'s, [`MoveToAction::sub_update`]) and its code, which the state's case reads: 0
    /// while it walks or waits, 1 once it ended, 3 once it failed, 5 once its target was let
    /// go; 1 for an empty list ("There is no list."). The start shows the task's sprite and
    /// the motion step gives up as a villager's does ([`MoveWait::give_up`]); a start or a move
    /// posts the units in the unit's way to its unit AI at once
    /// ([`Unit::post_in_the_way`](crate::objects::unit::Unit::post_in_the_way)).
    ///
    /// # Errors
    ///
    /// The walk's and the notifies'.
    ///
    /// [`MoveToAction::sub_update`]: crate::movement::move_to::MoveToAction::sub_update
    /// [`MoveWait::give_up`]: crate::movement::move_to::MoveWait::give_up
    pub(super) fn update_list(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        mut me: Me,
        gather_reads: &GatherReads,
        roles: &Roles,
    ) -> GenieResult<SubCode> {
        me.action_state = ActionState::new(self.state.get());
        let state = self
            .state
            .move_to()
            .map(|move_to_action| move_to_action.state);
        // the computer player's test is not held: its villagers would fail at once
        let how = ListUpdate {
            gives_up: GiveUp {
                patience: if roles
                    .of_class(gather_reads.class)
                    .contains(ClassRoles::VILLAGER)
                {
                    Patience::WaitsOnce
                } else {
                    Patience::Fails
                },
                stance: gather_reads.stance,
            },
            ..ListUpdate::default()
        };
        let code = match self.state.walk_mut() {
            Some(move_to) => update_sub_actions(move_to, step, unit, me, how)?,
            // a state that does not walk has no sub-action: its list's update is the empty one's
            None => update_sub_actions(&mut None, step, unit, me, how)?,
        };
        if matches!(state, Some(MoveTo::Start | MoveTo::Moving)) {
            unit.post_in_the_way(
                step.w.parts.motion.mover_mut(unit.id),
                &mut step.w.parts.unit_ai,
            )?;
        }
        Ok(code)
    }
}
