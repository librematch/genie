//! The path request's planners for a unit with a footprint: the tile pathfinder's first plan,
//! the short pather's tail, the long pather for a far waypoint and the tile pathfinder again.

use crate::map::Distance;
use crate::movement::LevelFlags;
use crate::movement::Levels;
use crate::movement::PathState;
use crate::movement::WaypointIndex;
use crate::world::Seconds;
use crate::world::WorldTime;
use std::ops::ControlFlow;

use crate::GenieError;
use crate::map::spatial::Spatial;
use crate::math::fmath::BVector;
use crate::movement::WaypointLevel;
use crate::movement::{
    AfterRequest, CLOSE, DETOUR, EPSILON, FAR, Goal, LONG, LONG_PATHER, LongAsk, Me, Mover,
    PathData, Planning, RANGE_FLOOR, SHORT_PATHER, SHORT_REACH, ShortAsk, TARGET_SHORT_REACH,
    TILE_PATHER, TileAsk, far_waypoint, footprint_gap, ground, ground_line, point_or_zero,
    short_range, too_short,
};
use crate::objects::ObjectId;
use crate::pathing::ShortFound;
use crate::pathing::tile_path::{Found, Path};

/// The new level and its path, and what the request's planners found: a path (`local_19d0`),
/// one partway (`bVar35`), one the tile pathfinder planned (`bVar6`), a failed short path
/// (`bVar4`), and how far from the waypoint the short path ends (`fVar41`).
pub(super) struct NewPath {
    pub(super) new: PathData,
    pub(super) path: Path,
    pub(super) found: bool,
    pub(super) partial: bool,
    pub(super) planned: bool,
    failed: bool,
    end_gap: Distance,
}

impl NewPath {
    /// A new level made `now`, its path empty, nothing found.
    pub(super) fn new(now: WorldTime) -> Self {
        Self {
            new: PathData::new(now),
            path: Path::default(),
            found: false,
            partial: false,
            planned: false,
            failed: false,
            end_gap: Distance::new(FAR),
        }
    }
}

/// The request's way to the walked level's waypoint: the goal and the include list, where the
/// unit last moved to (`+0x824`), the waypoint and the ground distance between, the unit's
/// radius, the level's number, range and target, and whether the unit is a trade cart.
pub(super) struct Way<'a> {
    pub(super) goal: Goal<'a>,
    pub(super) include: &'a [ObjectId],
    pub(super) position: BVector,
    pub(super) waypoint: BVector,
    pub(super) distance: Distance,
    pub(super) radius: Distance,
    pub(super) level: WaypointLevel,
    pub(super) range: Distance,
    pub(super) target_id: Option<ObjectId>,
    pub(super) trade_cart: bool,
}

impl Mover {
    /// A solid unit's planning: a level 2 plans with the tile pathfinder first
    /// ([`Mover::first_plan`]); then the short pather's tail ([`Mover::short_tail`]), the long
    /// pather for a far waypoint ([`Mover::long_tail`]) and, for a failed or short path, the tile
    /// pathfinder again ([`Mover::tile_again`]). A path much longer than the way (beyond
    /// [`DETOUR`] times it and [`LONG`]) repaths after two to two and a half seconds (the object
    /// stream's draw). A step where the planning ends the request.
    pub(super) fn plan_solid(
        &mut self,
        me: &Me,
        world: &mut Spatial,
        plan: &mut Planning<'_>,
        w: &Way<'_>,
        np: &mut NewPath,
    ) -> Result<Option<AfterRequest>, GenieError> {
        // FUN_141158210 is 1 only on an `E` tile in game type 12
        if plan.pathing.game_type == 12 {
            return Err(GenieError::unsupported(
                "a path request in game type 12 (its tile escape)",
            ));
        }
        if w.level == WaypointLevel::TOP && TILE_PATHER {
            self.first_plan(me, world, plan, w, np)?;
        }
        let tail = match self.short_tail(me, world, plan, w, np)? {
            ControlFlow::Break(step) => return Ok(Some(step)),
            ControlFlow::Continue(tail) => tail,
        };
        if tail && let Some(step) = self.long_tail(me, world, plan, w, np)? {
            return Ok(Some(step));
        }
        if (!np.found || too_short(&np.path)) && w.level == WaypointLevel::TOP && TILE_PATHER {
            self.tile_again(me, world, plan, w, np)?;
        }
        if !(np.path.length <= w.distance * DETOUR || np.path.length <= LONG) {
            np.new.repath_time = Seconds::new((plan.object.rand15() as f32 * 0.5) / 32767.0 + 2.0);
        }
        Ok(None)
    }

    /// A level 2's first plan: with a planner, the short pather's path to a target near the
    /// waypoint ([`Mover::near_target_path`]), else the tile pathfinder's
    /// ([`Mover::first_tile_plan`]); without one, the straight way. The path is measured and
    /// the new level made now.
    fn first_plan(
        &mut self,
        me: &Me,
        world: &mut Spatial,
        plan: &mut Planning<'_>,
        w: &Way<'_>,
        np: &mut NewPath,
    ) -> Result<(), GenieError> {
        if self.planner_exists(world, plan) {
            if !self.near_target_path(me, world, plan, w, np)? {
                self.first_tile_plan(me, world, plan, w, np)?;
            }
        } else {
            let _ = np.path.insert(0, w.position);
            let _ = np.path.push(w.waypoint);
            np.planned = true;
            np.found = true;
        }
        np.path.measure();
        np.new.created = plan.now;
        Ok(())
    }

    /// A target with the waypoint within [`TARGET_SHORT_REACH`]: the short pather's path
    /// ([`Mover::short_path_to`]). Whether it found one (result 1, longer than [`EPSILON`]);
    /// else the short path failed and empties.
    fn near_target_path(
        &self,
        me: &Me,
        world: &mut Spatial,
        plan: &mut Planning<'_>,
        w: &Way<'_>,
        np: &mut NewPath,
    ) -> Result<bool, GenieError> {
        if w.goal.target.is_none()
            || TARGET_SHORT_REACH < w.distance.get()
            || w.distance.get().is_nan()
        {
            return Ok(false);
        }
        let near = self.near_goal(w.distance, w.goal.gap, w.goal.to_end)?;
        let r = self.short_path_to(me, world, plan, w, near, np)?;
        if r != ShortFound::Reached || np.path.length.get() <= EPSILON {
            np.failed = true;
            np.path.points.clear();
            return Ok(false);
        }
        np.found = true;
        Ok(true)
    }

    /// The tile pathfinder's first plan, to within the target's radius (a target that does not
    /// move) and the unit's: its results 0, 3 and 4, and 2 (partway) off a level 2 or on one
    /// flagged 8, keep nothing. A plan ending farther from the waypoint than the unit is
    /// empties; one that does not counts found, and a still target's partway plan on a level 2
    /// flags it 8.
    fn first_tile_plan(
        &mut self,
        me: &Me,
        world: &mut Spatial,
        plan: &mut Planning<'_>,
        w: &Way<'_>,
        np: &mut NewPath,
    ) -> Result<(), GenieError> {
        let target_moves = w.goal.target.is_some_and(|(_, t)| t.moves);
        let target_radius = w
            .goal
            .target
            .filter(|_| !target_moves)
            .map_or(0.0, |(_, t)| t.radius.x());
        let ask = TileAsk {
            goal: w.waypoint,
            radii: Distance::new(target_radius + w.radius.get()),
            range: w.range,
            target: w.goal.target.map(|(_, t)| t),
        };
        let r =
            self.tile_plan(me, world, plan, ask, &mut np.path)?
                .ok_or(GenieError::unreachable(
                    "a plan without the planner it tested",
                ))?;
        // the game also tests for 4, which the planner never returns
        if !r.leads() {
            return Ok(());
        }
        let level = self.paths.walked_mut()?;
        if r == Found::Nearest
            && (level.level != WaypointLevel::TOP || level.flags.contains(LevelFlags::NEAREST))
        {
            return Ok(());
        }
        np.planned = true;
        let last = point_or_zero(&np.path, WaypointIndex::last_of(np.path.points.len()));
        let farther = w.distance < ground(w.waypoint, last);
        if farther {
            np.path.points.clear();
        }
        np.found = !farther;
        if !target_moves && !farther && level.level == WaypointLevel::TOP && r == Found::Nearest {
            level.flags |= LevelFlags::NEAREST;
        }
        Ok(())
    }

    /// The short pather's path to the waypoint (`FUN_140f9e540`) within the short range, with
    /// the ignore list of the walk's line and the include list; the path is measured, the new
    /// level made now and given the ignored units. The short pather's result.
    fn short_path_to(
        &self,
        me: &Me,
        world: &mut Spatial,
        plan: &mut Planning<'_>,
        w: &Way<'_>,
        near: bool,
        np: &mut NewPath,
    ) -> Result<ShortFound, GenieError> {
        let line = ground_line(self.moved_at, w.waypoint);
        let ignore = self.ignore_list(me, world, plan, w.goal.target, line)?;
        let ask = ShortAsk {
            goal: w.waypoint,
            range: short_range(near, w.range),
            target: w.target_id,
            ignore: &ignore,
            include: w.include,
        };
        let r = self.short_path(me, world, plan, ask, &mut np.path)?;
        np.path.measure();
        np.new.created = plan.now;
        self.give_ignored(&mut np.new, plan.now);
        Ok(r)
    }

    /// Without a path of three points: the short pather's, when it did not fail and the
    /// waypoint is within [`SHORT_REACH`] or the goal near ([`Mover::short_try`]); still none,
    /// a waypoint or target nearer than [`CLOSE`] takes the close-in path (the straight way,
    /// [`Mover::close_in`]). Whether the long pather follows (no path, or a blocked close-in
    /// one: `LAB_141152ec1`; a found path, when it is too short); a step where the short path
    /// ends the request.
    fn short_tail(
        &mut self,
        me: &Me,
        world: &mut Spatial,
        plan: &mut Planning<'_>,
        w: &Way<'_>,
        np: &mut NewPath,
    ) -> Result<ControlFlow<AfterRequest, bool>, GenieError> {
        let near = self.near_goal(w.distance, w.goal.gap, w.goal.to_end)?;
        if np.found && 3 <= np.path.points.len() {
            return Ok(ControlFlow::Continue(too_short(&np.path)));
        }
        if SHORT_PATHER
            && !np.failed
            && (w.distance <= Distance::new(SHORT_REACH) || near)
            && let Some(step) = self.short_try(me, world, plan, w, near, np)?
        {
            return Ok(ControlFlow::Break(step));
        }
        if np.found {
            return Ok(ControlFlow::Continue(too_short(&np.path)));
        }
        if (w.distance < Distance::new(CLOSE) || w.goal.gap < Distance::new(CLOSE))
            && self.close_in(me, w.waypoint, w.target_id, world)?
        {
            np.path.points.clear();
            let _ = np.path.push(w.position);
            let _ = np.path.push(w.waypoint);
            np.path.measure();
            np.new.created = plan.now;
            np.found = true;
            return Ok(ControlFlow::Continue(false));
        }
        Ok(ControlFlow::Continue(true))
    }

    /// The short pather's try in the tail: a blocked path (3, 4, or none of length) finds
    /// nothing; a path of no length is the waypoint in range ([`Mover::waypoint_in_range`]);
    /// else it is found, partway for 2, ending where it ends.
    fn short_try(
        &mut self,
        me: &Me,
        world: &mut Spatial,
        plan: &mut Planning<'_>,
        w: &Way<'_>,
        near: bool,
        np: &mut NewPath,
    ) -> Result<Option<AfterRequest>, GenieError> {
        np.planned = false;
        np.path.points.clear();
        let r = self.short_path_to(me, world, plan, w, near, np)?;
        let length = np.path.length.get();
        let blocked = r.fails()
            || length <= EPSILON
                && (r != ShortFound::InRange || (1 < np.path.points.len() && EPSILON <= length));
        if blocked {
            // far from its end (LONG < to_end) the game tells the units ahead to make way
            // (FUN_140f8a230, FUN_140f88750), which return at once: their switch is a zeroed
            // byte of .data that nothing writes (0x143f01836)
            return Ok(None);
        }
        if length <= EPSILON {
            return self.waypoint_in_range(w, plan.now).map(Some);
        }
        np.found = true;
        np.partial = r == ShortFound::Partial;
        let last = point_or_zero(&np.path, WaypointIndex::last_of(np.path.points.len()));
        np.end_gap = ground(w.waypoint, last);
        Ok(None)
    }

    /// The unit in range of its waypoint: the new level is dropped. A goal beyond the range (at
    /// least [`RANGE_FLOOR`] for a target and no range) on the last waypoint of a path made this
    /// update stops the unit (slot 0x5d0), resets the level to state 6 (`FUN_140f9b4b0`) and
    /// gives up; short of it the level goes on (flag 1). Within it the level takes state 4.
    pub(super) fn waypoint_in_range(
        &mut self,
        w: &Way<'_>,
        now: WorldTime,
    ) -> Result<AfterRequest, GenieError> {
        let reach = if w.goal.target.is_none() || Distance::ZERO < w.range {
            w.range
        } else {
            Distance::new(RANGE_FLOOR)
        };
        let level = self.paths.walked_mut()?;
        if reach < w.goal.gap {
            if level.waypoint == WaypointIndex::last_of(level.points.len()) && level.created == now
            {
                self.stop()?;
                self.reset_level()?;
                self.paths.walked_mut()?.state = PathState::GaveUp;
                return Ok(AfterRequest::GaveUp);
            }
            level.flags |= LevelFlags::REACHED;
        } else {
            level.state = PathState::Arrived;
        }
        Ok(AfterRequest::Again)
    }

    /// `LAB_141152ec1`: a far waypoint ([`far_waypoint`]; any for a trade cart, else beyond
    /// [`SHORT_REACH`]) takes the long pather's path ([`Mover::long_plan`]), made now: whether it
    /// reached the goal, and the gap its end left. A step where the long plan stops the unit.
    fn long_tail(
        &mut self,
        me: &Me,
        world: &mut Spatial,
        plan: &Planning<'_>,
        w: &Way<'_>,
        np: &mut NewPath,
    ) -> Result<Option<AfterRequest>, GenieError> {
        if !(LONG_PATHER && (w.trade_cart || SHORT_REACH < w.distance.get())) {
            return Ok(None);
        }
        let ground_radius = self.stats.radius.ground();
        let far = far_waypoint(
            w.distance,
            w.goal.target.map(|(_, t)| {
                let gap = footprint_gap(self.position(), ground_radius, t.position, t.radius);
                (gap, t.master)
            }),
        );
        if !far {
            return Ok(None);
        }
        np.planned = false;
        let target = w.goal.target;
        let ask = LongAsk {
            waypoint: w.waypoint,
            way: w.distance,
            range: w.range,
            target,
        };
        let Some((reached, farther)) = self.long_plan(me, world, plan, ask, &mut np.path)? else {
            return Ok(Some(AfterRequest::Paused));
        };
        np.new.created = plan.now;
        np.found = reached;
        if let Some(end) = farther {
            np.end_gap = end;
        }
        Ok(None)
    }

    /// A failed or short path: the tile pathfinder once more, on a path of its own. Ending
    /// farther from the waypoint than the unit is, it empties the path; else a plan of result
    /// 1, 2 or above 4 that ends nearer the waypoint than the short path did takes its place,
    /// unless it is shorter than [`LONG`] (then nothing is found).
    fn tile_again(
        &self,
        me: &Me,
        world: &mut Spatial,
        plan: &mut Planning<'_>,
        w: &Way<'_>,
        np: &mut NewPath,
    ) -> Result<(), GenieError> {
        let target_radius = w
            .goal
            .target
            .filter(|(_, t)| !t.moves)
            .map_or(0.0, |(_, t)| t.radius.x());
        let mut other = Path::default();
        let ask = TileAsk {
            goal: w.waypoint,
            radii: Distance::new(target_radius + w.radius.get()),
            range: w.range,
            target: w.goal.target.map(|(_, t)| t),
        };
        let Some(r) = self.tile_plan(me, world, plan, ask, &mut other)? else {
            return Ok(());
        };
        let last = point_or_zero(&other, WaypointIndex::last_of(other.points.len()));
        let end = ground(w.waypoint, last);
        if end <= w.distance {
            // the game's `result - 1 < 2 || 4 < result`: above 4 the planner never returns
            if r.leads() && end < np.end_gap {
                np.path.points.clear();
                for &p in &other.points {
                    let _ = np.path.push(p);
                }
                np.planned = true;
                np.found = true;
                np.path.measure();
                np.new.created = plan.now;
                if np.path.length < LONG {
                    np.path.points.clear();
                    np.found = false;
                }
            }
        } else {
            np.path.points.clear();
        }
        Ok(())
    }
}
