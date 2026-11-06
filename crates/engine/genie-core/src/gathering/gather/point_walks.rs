//! The walks to a point and the waits: back to search (state 11), back to reseed (0xf2), for
//! a farm (0xf3), the search (3) and a move's walk (15).

use crate::GenieResult;
use crate::combat::damage::Victim;
use crate::gathering::gather::GatherState;
use crate::gathering::gather::Stand;
use crate::gathering::gather::WalkEnd;
use crate::gathering::gather::{
    FARM_WAIT_TIME, GATHERER_UNMOVING, GatherAction, GatherReads, NO_GATHER_POINT, Next,
    SEARCH_WAIT, Walks,
};
use crate::map::Distance;
use crate::map::Position;
use crate::movement::move_to::SubCode;
use crate::movement::move_to::{MoveToAction, TargetMoved};
use crate::movement::{Me, Mover};
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::WorkerCount;
use crate::objects::lifecycle::Held;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::objects::sprite::Graphic;
use crate::objects::unit::Unit;
use crate::players::PlayerKind;
use crate::world::Seconds;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::layer::generated::MoveToTuning;
use crate::world::step::Step;

/// Whether a gatherer of `master` whose Gather has the target class `target_class` waits for
/// another farm where another would search (`set_state(0xf3)`, `0x140a8e1d0`): the farmers'
/// masters, and the fishing ships' on a farm-class target (a fish trap).
#[must_use]
pub fn waits_for_farm(master: MasterId, target_class: Option<ClassId>, roles: &Roles) -> bool {
    roles.of_master(master).contains(MasterRoles::FARM_WAITER)
        // The farm class, whose gather the slice does not model yet.
        // The villagers' class, whose `MoveTo` waits once when the motion step gives up.
        || (roles.of_master(master).contains(MasterRoles::TRAP_WAITER) && roles.of_some_class(target_class).contains(ClassRoles::FARM))
}

impl GatherAction {
    /// `set_state(11)` (`0x140a8cd40` case 0xb) with its target's radius on x (`+0x5d8`; none
    /// for a stop farm, masters 50 and 1187): the sub-actions go, the timer clears and a quiet
    /// `MoveTo` to the point within that radius plus the task's range starts
    /// ([`GatherAction::move_to_point`]).
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub(super) fn set_walk_back(
        &mut self,
        radius: Distance,
        mover: &mut Mover,
        walks: &Walks<'_>,
        move_to_tuning: &MoveToTuning,
    ) -> GenieResult<()> {
        self.walk_back(radius, mover, walks, move_to_tuning)
    }

    /// [`GatherAction::set_walk_back`] for the caller (state 8's end without a target: no
    /// target's radius).
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub fn walk_back(
        &mut self,
        radius: Distance,
        mover: &mut Mover,
        walks: &Walks<'_>,
        move_to_tuning: &MoveToTuning,
    ) -> GenieResult<()> {
        self.walk_to_point(
            |move_to| GatherState::WalkBack { move_to },
            radius,
            mover,
            walks,
            move_to_tuning,
        )
    }

    /// `set_state(0xf2)` (`0x140a8cd40` case 0xf2): the point takes `mGatherX`, `mGatherY`
    /// unless `mGatherX` is −1 (`UCOMISS`: a NaN takes them), then case 0xb's walk to it within
    /// `radius` plus the task's range ([`GatherAction::walk_back`]'s, in state 0xf2).
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub(super) fn begin_reseed_walk(
        &mut self,
        radius: Distance,
        mover: &mut Mover,
        walks: &Walks<'_>,
        move_to_tuning: &MoveToTuning,
    ) -> GenieResult<()> {
        let (x, y) = (self.gather_at.x(), self.gather_at.y());
        if x != NO_GATHER_POINT {
            self.point = self.point.with_x(x).with_y(y);
        }
        self.walk_to_point(
            |move_to| GatherState::ReseedWalk { move_to },
            radius,
            mover,
            walks,
            move_to_tuning,
        )
    }

    /// `set_state` in the walk `walk` makes, 11 or 0xf2 (case 0xb's walk): the sub-actions go,
    /// the timer clears and a quiet `MoveTo` to the point within `radius` plus the task's range
    /// starts ([`GatherAction::move_to_point`], `+0x61` left 0).
    pub(super) fn walk_to_point(
        &mut self,
        walk: fn(Option<MoveToAction>) -> GatherState,
        radius: Distance,
        mover: &mut Mover,
        walks: &Walks<'_>,
        move_to_tuning: &MoveToTuning,
    ) -> GenieResult<()> {
        self.tear_down(Some(mover))?;
        self.state = walk(None);
        self.timer = Seconds::ZERO;
        let range = radius + self.task.range;
        let move_to = self.move_to_point(
            self.point,
            range,
            TargetMoved::Follows,
            mover,
            walks,
            move_to_tuning,
        )?;
        self.state = walk(move_to);
        Ok(())
    }

    /// State 11's update (`0x140a8e1d0` case 10): the sub-action list's code: 0 walks on; once
    /// it ended, arrived, failed or let its target go (1, 2, 3, 5) the gatherer searches from
    /// there (`set_state(3)`, [`GatherAction::begin_search`]). A quiet `MoveTo` never gives 4,
    /// which would walk back again.
    pub(super) fn update_walk_back(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        me: Me,
        gather_reads: &GatherReads,
        roles: &Roles,
    ) -> GenieResult<Next> {
        match self.update_list(step, unit, me, gather_reads, roles)? {
            SubCode::UnderWay => Ok(Next::Stay),
            SubCode::Done | SubCode::Failed | SubCode::TargetGone => {
                self.begin_search(step, unit, gather_reads)
            }
        }
    }

    /// `set_state(0xf3)` (case 0xf3; `FUN_140a98620` first): as the search's
    /// ([`GatherAction::begin_search`], the same notify) with a timer of a second.
    pub(super) fn begin_farm_wait(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        gather_reads: &GatherReads,
    ) -> GenieResult<Next> {
        self.tear_down(step.world.parts.mover_mut(unit.id))?;
        self.state = GatherState::FarmWait;
        self.timer = FARM_WAIT_TIME;
        Self::show(step, unit, self.stand_sprite(unit, gather_reads));
        Ok(Next::Search)
    }

    /// State 0xf3's update (`0x140a8e1d0` case 0xf2): the timer counts up by the world's dt
    /// while below 0; once at 0 a gatherer that never gathered (`+0x80`) at a farm (its target
    /// type 49) another farmer works too (`+0x644` above 1) walks back to its point to search
    /// (`set_state(11)` with the farm's radius, none for a stop farm, then its target goes:
    /// slot 0x68 with none); one that never gathered otherwise idles (`set_state(2)`); one that
    /// gathered walks back to where it gathered to reseed (`set_state(0xf2)`). A timer at 0 or
    /// more does nothing.
    pub(super) fn update_farm_wait(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        target: Option<&Victim>,
        gather_reads: &GatherReads,
        roles: &Roles,
    ) -> GenieResult<Next> {
        if Seconds::ZERO <= self.timer {
            return Ok(Next::Stay);
        }
        self.timer += step.dt.seconds();
        if self.timer < Seconds::ZERO {
            return Ok(Next::Stay);
        }
        let radius = target.map_or(0.0, |victim| {
            if roles
                .of_master(victim.master_id)
                .contains(MasterRoles::STOP_FARM)
            {
                0.0
            } else {
                victim.radius.x()
            }
        });
        if !self.worked {
            if roles
                .of_some_class(self.target_class)
                .contains(ClassRoles::FARM)
                && target.is_some_and(|victim| WorkerCount::ONE < victim.workers)
            {
                let walks = gather_reads.walks(
                    &step.world.model.graphics,
                    unit,
                    step.world.parts.mover_of(unit.id),
                );
                let mover = step
                    .world
                    .parts
                    .mover_mut(unit.id)
                    .ok_or(GATHERER_UNMOVING)?;
                self.set_walk_back(
                    Distance::new(radius),
                    mover,
                    &walks,
                    &step.world.model.tuning.move_to,
                )?;
                self.target = None;
                return Ok(Next::Stay);
            }
            self.tear_down(step.world.parts.mover_mut(unit.id))?;
            self.set_idle(step, unit, gather_reads);
            return Ok(Next::Stay);
        }
        let walks = gather_reads.walks(
            &step.world.model.graphics,
            unit,
            step.world.parts.mover_of(unit.id),
        );
        let mover = step
            .world
            .parts
            .mover_mut(unit.id)
            .ok_or(GATHERER_UNMOVING)?;
        self.begin_reseed_walk(
            Distance::new(radius),
            mover,
            &walks,
            &step.world.model.tuning.move_to,
        )?;
        Ok(Next::Stay)
    }

    /// State 0xf2's update (`0x140a8e1d0` case 0xf1): the sub-action list's code: 0 walks on. Once
    /// the walk arrived or its target went (1, 2, 5: the list drops it, its destructor stops the
    /// unit) a target in a state of 2 or less, or 6, is faced again (`FUN_1411944f0` false:
    /// `set_state(6)`); else, and after a failed walk (code 3), the caller reseeds
    /// ([`Next::Reseed`]). A quiet `MoveTo` never gives 4, which would walk again.
    pub(super) fn update_reseed_walk(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        me: Me,
        target: Option<&Victim>,
        gather_reads: &GatherReads,
        roles: &Roles,
    ) -> GenieResult<Next> {
        match self.update_list(step, unit, me, gather_reads, roles)? {
            SubCode::UnderWay => Ok(Next::Stay),
            SubCode::Done | SubCode::TargetGone => {
                self.tear_down(step.world.parts.mover_mut(unit.id))?;
                if let Some(victim) = target
                    && !(ObjectState::Alive < victim.state && victim.state != ObjectState::Undead)
                {
                    return self.set_face(step, unit, victim, gather_reads, roles);
                }
                Ok(Next::Reseed {
                    end: WalkEnd::Arrived,
                })
            }
            SubCode::Failed => Ok(Next::Reseed {
                end: WalkEnd::Failed,
            }),
        }
    }

    /// State 0xf2's end without a reseed ([`Next::Reseed`]): the sub-actions go
    /// (`FUN_140a98620`); after a walk that arrived a computer player's gatherer is done
    /// (`set_state(1)`: the timer clears, the unit stands), any other idles (`set_state(2)`,
    /// [`GatherAction::set_idle`]: the targets it lets go are returned for the caller's
    /// release). The unit stands with `stand` (`FUN_140a8b6b0`).
    ///
    /// # Errors
    ///
    /// The unit's stop that the motion step does not model.
    pub(crate) fn after_reseed(
        &mut self,
        end: WalkEnd,
        player_kind: PlayerKind,
        mover: Option<&mut Mover>,
        stand: Stand<'_>,
    ) -> GenieResult<[Option<ObjectId>; 2]> {
        let mut mover = mover;
        self.tear_down(mover.as_deref_mut())?;
        self.timer = Seconds::ZERO;
        stand.show(mover.map(|mover| &mut mover.stats));
        if end == WalkEnd::Arrived && player_kind.is_computer() {
            // State 1: done; its update returns 1 and the action list removes it.
            self.state = GatherState::Done;
            return Ok([None; 2]);
        }
        self.state = GatherState::Idle;
        let released = self.targets();
        self.target = None;
        self.carry_to = None;
        self.point = Position::from([-1.0; 3]);
        Ok(released)
    }

    /// `set_state(3)` (case 3; `FUN_140a98620` first: the `MoveTo` goes and stops the unit): the
    /// caller posts the unit AI's notify ([`Next::Search`]), the timer waits 10 seconds and the
    /// unit stands ([`GatherAction::stand_sprite`]).
    pub(super) fn begin_search(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        gather_reads: &GatherReads,
    ) -> GenieResult<Next> {
        self.tear_down(step.world.parts.mover_mut(unit.id))?;
        self.state = GatherState::Search;
        self.timer = SEARCH_WAIT;
        Self::show(step, unit, self.stand_sprite(unit, gather_reads));
        Ok(Next::Search)
    }

    /// `set_state(1)` (case 1): the timer clears and the unit stands.
    pub(super) fn set_done(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        gather_reads: &GatherReads,
    ) {
        self.state = GatherState::Done;
        self.timer = Seconds::ZERO;
        Self::show(step, unit, self.stand_sprite(unit, gather_reads));
    }

    /// The sprite the gatherer stands with (`FUN_140a8b6b0`): the task's carrying sprite while
    /// it carries at least 1 of the task's resource, else its master's standing sprite.
    pub(super) fn stand_sprite(&self, unit: &Unit, gather_reads: &GatherReads) -> Option<Graphic> {
        let resource = self.task.resource;
        let Held { kind, amount } = unit.life.held();
        match gather_reads.sprites.carrying {
            Some(graphic) if resource.is_some() && kind == resource && 1 <= amount.get() as i32 => {
                Some(graphic)
            }
            _ => gather_reads.sprites.standing,
        }
    }

    /// State 15's update (`0x140a8e1d0` case 0xe): the sub-action list's code: 0 walks on. Once
    /// it ended or its target went (1, 2, 5) a computer player's gatherer is done
    /// (`set_state(1)`), a human's idles (`set_state(2)`); a failed walk (code 3) idles both. A
    /// quiet `MoveTo` never gives 4, which would walk again.
    pub(super) fn update_moved(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        me: Me,
        gather_reads: &GatherReads,
        roles: &Roles,
    ) -> GenieResult<Next> {
        match self.update_list(step, unit, me, gather_reads, roles)? {
            SubCode::UnderWay => Ok(Next::Stay),
            code @ (SubCode::Done | SubCode::Failed | SubCode::TargetGone) => {
                self.tear_down(step.world.parts.mover_mut(unit.id))?;
                if code != SubCode::Failed && gather_reads.player_kind.is_computer() {
                    self.set_done(step, unit, gather_reads);
                } else {
                    self.set_idle(step, unit, gather_reads);
                }
                Ok(Next::Stay)
            }
        }
    }
}
