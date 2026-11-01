//! The walks to a point and the waits: back to search (state 11), back to reseed (0xf2), for
//! a farm (0xf3), the search (3) and a move's walk (15).

use crate::GenieError;
use crate::combat::damage::Victim;
use crate::gathering::gather::GatherState;
use crate::gathering::gather::update::{QUIET_CODE, UNMOVING};
use crate::gathering::gather::{
    FARM_WAIT_TIME, FARM_WAITERS, GatherAction, GatherReads, NO_GATHER_POINT, Next, SEARCH_WAIT,
    STOP_FARMS, TRAP_WAITERS, Walks,
};
use crate::map::Distance;
use crate::map::Position;
use crate::math::rng::Well512;
use crate::movement::move_to::TargetMoved;
use crate::movement::{Me, Mover};
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::lifecycle::Held;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::objects::masters::class;
use crate::objects::sprite::ActiveSprite;
use crate::objects::sprite::new_sprite;
use crate::objects::unit::Unit;
use crate::world::Seconds;
use crate::world::step::Step;

/// Whether a gatherer of `master` whose Gather has the target class `target_class` waits for
/// another farm where another would search (`set_state(0xf3)`, `0x140a8e1d0`): the farmers'
/// masters, and the fishing ships' on a farm-class target (a fish trap).
#[must_use]
pub fn waits_for_a_farm(master: MasterId, target_class: Option<ClassId>) -> bool {
    FARM_WAITERS.contains(&master)
        // The farm class, whose gather the slice does not model yet.
        // The villagers' class, whose `MoveTo` waits once when the motion step gives up.
        || (TRAP_WAITERS.contains(&master) && target_class == Some(class::FARM))
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
    ) -> Result<(), GenieError> {
        self.walk_back(radius, mover, walks)
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
    ) -> Result<(), GenieError> {
        self.walk_to_point(GatherState::WalkBack, radius, mover, walks)
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
    ) -> Result<(), GenieError> {
        let (x, y) = (self.gather_at.x(), self.gather_at.y());
        if x != NO_GATHER_POINT {
            self.point = self.point.with_x(x).with_y(y);
        }
        self.walk_to_point(GatherState::ReseedWalk, radius, mover, walks)
    }

    /// `set_state` in `state` 11 or 0xf2 (case 0xb's walk): the sub-actions go, the timer
    /// clears and a quiet `MoveTo` to the point within `radius` plus the task's range starts
    /// ([`GatherAction::move_to_point`], `+0x61` left 0).
    pub(super) fn walk_to_point(
        &mut self,
        state: GatherState,
        radius: Distance,
        mover: &mut Mover,
        walks: &Walks<'_>,
    ) -> Result<(), GenieError> {
        self.tear_down(Some(mover))?;
        self.state = state;
        self.timer = Seconds::ZERO;
        let range = radius + self.task.range;
        self.move_to = self.move_to_point(self.point, range, TargetMoved::Follows, mover, walks)?;
        Ok(())
    }

    /// State 11's update (`0x140a8e1d0` case 10): the sub-action list's code: 0 walks on; once
    /// it ended, arrived, failed or let its target go (1, 2, 3, 5) the gatherer searches from
    /// there (`set_state(3)`, [`GatherAction::begin_search`]). A quiet `MoveTo` never gives 4,
    /// which would walk back again.
    pub(super) fn update_walk_back(
        &mut self,
        c: &mut Step<'_>,
        unit: &mut Unit,
        me: Me,
        r: &GatherReads,
    ) -> Result<Next, GenieError> {
        match self.update_list(c, unit, me, r)?.get() {
            0 => Ok(Next::Stay),
            1 | 2 | 3 | 5 => self.begin_search(c, unit, r),
            code => Err(QUIET_CODE.with("code", code)),
        }
    }

    /// `set_state(0xf3)` (case 0xf3; `FUN_140a98620` first): as the search's
    /// ([`GatherAction::begin_search`], the same notify) with a timer of a second.
    pub(super) fn begin_farm_wait(
        &mut self,
        c: &mut Step<'_>,
        unit: &mut Unit,
        r: &GatherReads,
    ) -> Result<Next, GenieError> {
        self.tear_down(unit.body.mover_mut())?;
        self.state = GatherState::FarmWait;
        self.timer = FARM_WAIT_TIME;
        Self::show(c, unit, self.stand_sprite(unit, r));
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
        c: &mut Step<'_>,
        unit: &mut Unit,
        target: Option<&Victim>,
        r: &GatherReads,
    ) -> Result<Next, GenieError> {
        if Seconds::ZERO <= self.timer {
            return Ok(Next::Stay);
        }
        self.timer += c.dt.seconds();
        if self.timer < Seconds::ZERO {
            return Ok(Next::Stay);
        }
        let radius = target.map_or(0.0, |t| {
            if STOP_FARMS.contains(&t.master_id) {
                0.0
            } else {
                t.radius.x()
            }
        });
        if !self.worked {
            if self.target_class == Some(class::FARM)
                && target.is_some_and(|t| crate::objects::WorkerCount::ONE < t.workers)
            {
                let walks = r.walks(&c.w.graphics, unit);
                let mover = unit.body.mover_mut().ok_or(UNMOVING)?;
                self.set_walk_back(Distance::new(radius), mover, &walks)?;
                self.target = None;
                return Ok(Next::Stay);
            }
            self.tear_down(unit.body.mover_mut())?;
            self.set_idle(c, unit, r);
            return Ok(Next::Stay);
        }
        let walks = r.walks(&c.w.graphics, unit);
        let mover = unit.body.mover_mut().ok_or(UNMOVING)?;
        self.begin_reseed_walk(Distance::new(radius), mover, &walks)?;
        Ok(Next::Stay)
    }

    /// State 0xf2's update (`0x140a8e1d0` case 0xf1): the sub-action list's code: 0 walks on. Once
    /// the walk arrived or its target went (1, 2, 5: the list drops it, its destructor stops the
    /// unit) a target in a state of 2 or less, or 6, is faced again (`FUN_1411944f0` false:
    /// `set_state(6)`); else, and after a failed walk (code 3), the caller reseeds
    /// ([`Next::Reseed`]). A quiet `MoveTo` never gives 4, which would walk again.
    pub(super) fn update_reseed_walk(
        &mut self,
        c: &mut Step<'_>,
        unit: &mut Unit,
        me: Me,
        target: Option<&Victim>,
        r: &GatherReads,
    ) -> Result<Next, GenieError> {
        match self.update_list(c, unit, me, r)?.get() {
            0 => Ok(Next::Stay),
            1 | 2 | 5 => {
                self.tear_down(unit.body.mover_mut())?;
                if let Some(t) = target
                    && !(ObjectState::Alive < t.state && t.state != ObjectState::Undead)
                {
                    return self.set_face(c, unit, t, r);
                }
                Ok(Next::Reseed { arrived: true })
            }
            3 => Ok(Next::Reseed { arrived: false }),
            code => Err(QUIET_CODE.with("code", code)),
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
        arrived: bool,
        computer: bool,
        mover: Option<&mut Mover>,
        stand: Option<crate::objects::sprite::Graphic>,
        sprite: &mut Option<ActiveSprite>,
        rng: &mut Well512,
    ) -> Result<[Option<ObjectId>; 2], GenieError> {
        let mut mover = mover;
        self.tear_down(mover.as_deref_mut())?;
        self.timer = Seconds::ZERO;
        if let Some(s) = stand {
            new_sprite(sprite, mover.map(|m| &mut m.stats), s, rng);
        }
        if arrived && computer {
            // State 1: done; its update returns 1 and the action list removes it.
            self.state = GatherState::Done;
            return Ok([None; 2]);
        }
        self.state = GatherState::Idle;
        let released = [self.target, self.carry_to];
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
        c: &mut Step<'_>,
        unit: &mut Unit,
        r: &GatherReads,
    ) -> Result<Next, GenieError> {
        self.tear_down(unit.body.mover_mut())?;
        self.state = GatherState::Search;
        self.timer = SEARCH_WAIT;
        Self::show(c, unit, self.stand_sprite(unit, r));
        Ok(Next::Search)
    }

    /// `set_state(1)` (case 1): the timer clears and the unit stands.
    pub(super) fn set_done(&mut self, c: &mut Step<'_>, unit: &mut Unit, r: &GatherReads) {
        self.state = GatherState::Done;
        self.timer = Seconds::ZERO;
        Self::show(c, unit, self.stand_sprite(unit, r));
    }

    /// The sprite the gatherer stands with (`FUN_140a8b6b0`): the task's carrying sprite while
    /// it carries at least 1 of the task's resource, else its master's standing sprite.
    pub(super) fn stand_sprite(
        &self,
        unit: &Unit,
        r: &GatherReads,
    ) -> Option<crate::objects::sprite::Graphic> {
        let resource = self.task.resource;
        let Held { kind, amount } = unit.life.held();
        match r.sprites.carrying {
            Some(c) if resource.is_some() && kind == resource && 1 <= amount.get() as i32 => {
                Some(c)
            }
            _ => r.sprites.standing,
        }
    }

    /// State 15's update (`0x140a8e1d0` case 0xe): the sub-action list's code: 0 walks on. Once
    /// it ended or its target went (1, 2, 5) a computer player's gatherer is done
    /// (`set_state(1)`), a human's idles (`set_state(2)`); a failed walk (code 3) idles both. A
    /// quiet `MoveTo` never gives 4, which would walk again.
    pub(super) fn update_moved(
        &mut self,
        c: &mut Step<'_>,
        unit: &mut Unit,
        me: Me,
        r: &GatherReads,
    ) -> Result<Next, GenieError> {
        match self.update_list(c, unit, me, r)?.get() {
            0 => Ok(Next::Stay),
            code @ (1 | 2 | 3 | 5) => {
                self.tear_down(unit.body.mover_mut())?;
                if code != 3 && r.computer {
                    self.set_done(c, unit, r);
                } else {
                    self.set_idle(c, unit, r);
                }
                Ok(Next::Stay)
            }
            code => Err(QUIET_CODE.with("code", code)),
        }
    }
}
