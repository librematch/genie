//! `BMotion`'s path step (`FUN_14104a270`): its passes and the reached waypoint.

use crate::groups::Motion;
use crate::groups::motion::{
    CLOSE, EPSILON, GroupView, Lead, LeadGroup, NEAR, PASSES, point, request, reset_level,
    set_flag, steer,
};
use crate::map::DistanceSquared;
use crate::map::Footprint;
use crate::map::Position;
use crate::map::spatial::{Placed, Spatial};
use crate::movement::LevelFlags;
use crate::movement::LinkedPathType;
use crate::movement::PathFlags;
use crate::movement::PathState;
use crate::movement::WaypointLevel;
use crate::movement::{Planning, footprint_gap, footprint_gap_squared};
use crate::world::Seconds;
use crate::world::slots::CheckedIndex;
use crate::{GenieError, math::fmath};

/// One pass's end (the routine's labels).
pub(super) enum Pass {
    /// `LAB_14104bd5d`: the next pass.
    Next,
    /// `LAB_14104bd53`: the walked level's waypoint counts as reached (its bit 0), then the next
    /// pass.
    Reached,
    /// The step ends with this answer.
    Done(bool),
}

/// The world as `BMotion`'s path step reads it.
pub(crate) struct StepWorld<'w, 'p> {
    pub(crate) spatial: &'w mut Spatial,
    pub(crate) plan: &'w mut Planning<'p>,
    pub(crate) dt: Seconds,
    /// The objects by id: what the walk reads of the units it meets.
    pub(crate) by_id: crate::world::index::ObjectsRef<'w>,
}

/// `FUN_14104a270` (the path step of `BMotion`'s update) on `motion`, for a group whose lead is
/// `lead`: `budget` (`param_2`) tests a computer player's path budget before a plan. Returns the
/// routine's answer (1 when the walk goes on or arrived, 0 when it waits).
///
/// A group without path data does nothing; a paused walked level waits out its pause. Then up
/// to [`PASSES`] passes over the walked level, as a unit's motion step's: a reached waypoint
/// moves the level on (or back to the waypoint it left when it is not in range, or to its parent
/// waypoint on a looping path); an arrived level (states 4, 5) hands over to its parent by its
/// link; a level in state 2 gets a new level with a path to its waypoint, planned from the
/// group's place for the lead (the tile pathfinder for a top level, else for a waypoint within
/// [`SHORT_REACH`] the short pather, a near waypoint the straight way when nothing stands on it,
/// else the long pather), which becomes level 1 waiting for its stretches (a top level's plan)
/// or level 0 walked at once; a walking level steers toward its waypoint: a units' query along
/// the step finds what stands in the way (the target alone ends the step there; a unit moved
/// after the path was made sends the level back to its parent), else the group turns toward the
/// waypoint, the members' centre pulls it when every member acts on a target, and the forward
/// speed takes the top speed.
///
/// # Errors
///
/// What the planners or the queries do not model.
///
/// [`SHORT_REACH`]: crate::groups::motion::SHORT_REACH
pub(crate) fn path_step(
    motion: &mut Motion,
    group: &GroupView,
    lead: &Lead,
    world: &mut StepWorld<'_, '_>,
    budget: bool,
) -> Result<bool, GenieError> {
    if motion.paths.is_empty() {
        return Ok(false);
    }
    let budget = budget && group.player_type.is_computer();
    let dt = world.dt;
    let level = motion.paths.checked_mut(0)?;
    if Seconds::ZERO < level.pause_time {
        level.pause_time -= dt;
        if Seconds::ZERO < level.pause_time {
            motion.stand();
            return Ok(false);
        }
        level.pause_time = Seconds::ZERO;
    }
    let mut passes = 1;
    loop {
        match pass(motion, group, lead, world, budget)? {
            Pass::Done(r) => return Ok(r),
            Pass::Reached => set_flag(&mut motion.paths, LevelFlags::REACHED, true),
            Pass::Next => {}
        }
        passes += 1;
        if PASSES < passes {
            return Ok(false);
        }
    }
}

/// One pass of [`path_step`] (`LAB_14104a3fd` to `LAB_14104bd5d`).
pub(super) fn pass(
    motion: &mut Motion,
    group: &GroupView,
    lead: &Lead,
    world: &mut StepWorld<'_, '_>,
    budget: bool,
) -> Result<Pass, GenieError> {
    let spatial = &mut *world.spatial;
    let level = motion.paths.checked(0)?;
    if !level.has_path {
        return Ok(Pass::Done(false));
    }
    let target_id = level.target;
    // the game tests the sign: a recyclable object's id is no target here
    let target: Option<Placed> = match target_id.filter(|t| !t.is_recyclable()) {
        Some(t) => spatial.get_known(t)?.copied(),
        None => None,
    };
    let at = motion.position[0];
    // `fVar39`: the target's footprint gap to the group's place, squared (FUN_140a847c0)
    let target_gap2 = target.as_ref().map_or(DistanceSquared::MAX, |t| {
        footprint_gap_squared(
            t.position,
            t.radius,
            Position::new(at.x(), at.y(), 0.0),
            Footprint::new(0.0, 0.0),
        )
    });
    let flags = level.flags;
    if flags.contains(LevelFlags::REACHED) {
        reached(motion, target.as_ref())?;
    }
    let state = motion.paths.checked(0)?.state;
    if matches!(state, PathState::Arrived | PathState::Unplanned) {
        if motion.paths.len() < 2 {
            reset_level(&mut motion.paths)?;
            motion.stand();
            return Ok(Pass::Done(true));
        }
        let linked = motion.paths.checked(0)?.linked_type;
        if linked == LinkedPathType::REACHES_WAYPOINT || flags.contains(LevelFlags::MET_TARGET) {
            let _ = motion.paths.remove(0);
            return Ok(Pass::Reached);
        }
        if linked == LinkedPathType::MOVES_ON || linked == LinkedPathType::ASKS_AGAIN {
            let _ = motion.paths.remove(0);
            let patrols = lead.group.as_ref().is_some_and(LeadGroup::patrols);
            let parent = motion.paths.checked_mut(0)?;
            parent.state = PathState::WantsPath;
            if linked == LinkedPathType::MOVES_ON && parent.has_path {
                if parent.path_flags.contains(PathFlags::LOOPS) {
                    let w = parent.waypoint;
                    parent.waypoint = parent.looping_next(w);
                }
                if patrols {
                    parent.waypoint = parent.waypoint.next();
                    if !parent.waypoint.within(parent.points.len()) {
                        parent.state = PathState::Arrived;
                    }
                }
            }
            return Ok(Pass::Next);
        }
        // another link: the level walks on in its state
    }
    if motion.paths.checked(0)?.state == PathState::WantsPath
        && let Some(p) = request(motion, lead, world, budget, target.as_ref(), target_gap2)?
    {
        return Ok(p);
    }
    steer(motion, group, lead, world, target.as_ref())
}

/// The reached waypoint (the walked level's [`crate::movement::LevelFlags::REACHED`],
/// `0x14104a4bf`): with [`crate::movement::LevelFlags::AT_WAYPOINT`] the group takes the
/// waypoint's place; both clear and the level moves on. On a path that does not loop,
/// past its last waypoint, a group not yet within the range plus 0.1 of where it was going (its
/// target's footprint, else the waypoint it left) steps back to that waypoint and asks again
/// (state 2), else the level arrived (state 4); with waypoints left a level above 0 asks for the
/// next stretch (state 2). A level that reached its target with bit 2 clears it (and sets it on
/// its parents). A looping path takes its next waypoint (`FUN_140f98bc0`) and asks again.
pub(super) fn reached(motion: &mut Motion, target: Option<&Placed>) -> Result<(), GenieError> {
    let level = motion.paths.checked(0)?;
    if level.flags.contains(LevelFlags::AT_WAYPOINT) {
        if level.waypoint.within(level.points.len()) {
            let p = *level.points.checked(level.waypoint.get() as usize)?;
            motion.new_position[0] = p;
            motion.position[0] = p;
        }
        set_flag(&mut motion.paths, LevelFlags::AT_WAYPOINT, false);
    }
    set_flag(&mut motion.paths, LevelFlags::REACHED, false);
    let at = motion.position[0];
    let level = motion.paths.checked_mut(0)?;
    level.waypoint = level.waypoint.next();
    let left = level.waypoint.previous();
    if level.path_flags.contains(PathFlags::LOOPS) {
        level.waypoint = level.looping_next(left);
        level.state = PathState::WantsPath;
        return Ok(());
    }
    let d = fmath::length(fmath::sub(at, point(level, left)));
    let mut beyond = match target {
        None => (d - level.range.get()) - NEAR,
        Some(t) => {
            (footprint_gap(
                t.position,
                t.radius,
                Position::new(at.x(), at.y(), 0.0),
                Footprint::new(0.0, 0.0),
            )
            .get()
                - level.range.get())
                - NEAR
        }
    };
    if target.is_some() && level.flags.contains(LevelFlags::MET_TARGET) {
        beyond = 0.0;
        set_flag(&mut motion.paths, LevelFlags::MET_TARGET, false);
    }
    let level = motion.paths.checked_mut(0)?;
    if level.waypoint.within(level.points.len()) {
        if level.level != WaypointLevel::WALKED {
            level.state = PathState::WantsPath;
        }
    } else if EPSILON <= beyond
        && (target.is_some() || CLOSE <= beyond || level.level == WaypointLevel::WALKED)
        && EPSILON <= d
    {
        level.waypoint = level.waypoint.previous();
        level.state = PathState::WantsPath;
    } else {
        level.state = PathState::Arrived;
    }
    Ok(())
}
