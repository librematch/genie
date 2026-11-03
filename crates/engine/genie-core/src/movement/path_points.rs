//! Path points, the gaps between footprints and the position setter's tests.

use crate::groups::UnitGroups;
use crate::map::Distance;
use crate::map::DistanceSquared;
use crate::map::Footprint;
use crate::map::Point;
use crate::map::Position;
use crate::map::spatial::Placed;
use crate::math::fmath::BVector;
use crate::math::fmath::sqrt;
use crate::math::rng::Well512;
use crate::movement::WaypointIndex;
use crate::movement::{EPSILON, Motion, PathData};
use crate::objects::ObjectId;
use crate::pathing::Pathing;
use crate::pathing::tile_path::Path;
use crate::players::PlayerKind;
use crate::world::Seconds;
use crate::world::WorldTime;
use crate::world::layer::generated::MovementTuning;

/// `FUN_140f98dd0`: the path ends before its point `i` (the points from `i` on go) and is
/// measured again (`FUN_140f95a60`, true); an index outside the path, or an empty path, changes
/// nothing (false).
pub(super) fn cut_path(path: &mut Path, i: i32) -> bool {
    match usize::try_from(i) {
        Ok(k) if k < path.points.len() => {
            path.points.truncate(k);
            path.measure();
            true
        }
        _ => false,
    }
}

/// A point of a path (`FUN_140f9bcd0`): outside it, the game's static point (`DAT_143f0dc88`),
/// zero.
pub(super) fn point_or_zero(path: &Path, index: WaypointIndex) -> BVector {
    index
        .slot()
        .and_then(|i| path.points.get(i))
        .copied()
        .unwrap_or(BVector::ZERO)
}

/// A waypoint of a level's path, or the game's static zero point outside it.
pub(super) fn point_or_zero_of(level: &PathData, index: WaypointIndex) -> BVector {
    index
        .slot()
        .and_then(|i| level.points.get(i))
        .copied()
        .unwrap_or(BVector::ZERO)
}

/// The position setter's test for a move (`FUN_141161950`): a coordinate changed by more than
/// 1e-7.
pub(super) fn moved_far(a: Position, b: Position) -> bool {
    [b.x() - a.x(), b.y() - a.y(), b.z() - a.z()]
        .iter()
        .any(|d| EPSILON < d.abs())
}

/// The motion step's start-test line for the ignore list (`FUN_141150f00`'s calls of
/// `FUN_14114ece0`): from the unit's place to the waypoint, both on the ground (x, y).
pub(super) const fn ground_line(at: Position, waypoint: BVector) -> (Point, Point) {
    (at.ground(), waypoint.ground())
}

/// The length on the ground from `a` to `b`.
pub(super) fn ground(a: BVector, b: BVector) -> Distance {
    let mut d = a - b;
    d = d.with_height(0.0);
    Distance::new(sqrt(d.x() * d.x() + 0.0 + d.y() * d.y()))
}

/// Whether a walk's goal is in the short pather's reach ([`Mover::near_goal`]).
///
/// [`Mover::near_goal`]: crate::movement::Mover::near_goal
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum GoalReach {
    /// In reach.
    Near,
    /// Out of it.
    Far,
}

/// The short pather's range for a request (`0x141150f00`): the level's range less 0.1 when its
/// goal is in reach ([`Mover::near_goal`]), at least 0; else 0.
///
/// [`Mover::near_goal`]: crate::movement::Mover::near_goal
pub(super) fn short_range(
    near: GoalReach,
    range: Distance,
    movement_tuning: &MovementTuning,
) -> Distance {
    if near == GoalReach::Far {
        return Distance::ZERO;
    }
    let r = range - Distance::new(movement_tuning.range_floor);
    if r <= Distance::ZERO {
        Distance::ZERO
    } else {
        r
    }
}

/// What [`Mover::avoid`] makes of the walk's hits: the hits it keeps and the pauses it gives.
///
/// [`Mover::avoid`]: crate::movement::Mover::avoid
pub(super) struct Avoided {
    pub(super) kept: Vec<ObjectId>,
    pub(super) pauses: Vec<(ObjectId, Seconds)>,
    /// The step hit only the walked level's target: the waypoint counts as reached.
    pub(super) at_target: bool,
}

/// A path too short to walk: fewer than two points or shorter than 0.01.
pub(super) fn too_short(path: &Path, movement_tuning: &MovementTuning) -> bool {
    path.points.len() < 2 || path.length < Distance::new(movement_tuning.shortest)
}

/// `FUN_140f9b8f0`: a midpoint between points `i` and `i + 1` (from `i` half the stretch along
/// it), unless the stretch is shorter than 1e-7.
pub(super) fn split(path: &mut Path, i: i32) {
    let Ok(at) = usize::try_from(i) else {
        return;
    };
    // a signed `count - 1`: an empty path has no stretch
    let Some(&[a, b]) = path.points.get(at..at + 2) else {
        return;
    };
    let d = b - a;
    let length = sqrt(d.x() * d.x() + d.height() * d.height() + d.y() * d.y());
    if length < EPSILON {
        return;
    }
    let again = sqrt(d.x() * d.x() + d.height() * d.height() + d.y() * d.y());
    let half = length * 0.5;
    let step = BVector::new(
        (d.x() / again) * half,
        (d.height() / again) * half,
        (d.y() / again) * half,
    );
    let _ = path.insert(at + 1, a + step);
}

/// What the motion step knows of where the walked level goes: its target, the gap to it
/// (`fVar38`, far without one) and the gap to where the top level's path ends, or the target's
/// (`fVar40`).
#[derive(Clone, Copy, Debug)]
pub(super) struct Goal<'w> {
    pub(super) target: Option<(ObjectId, &'w Placed)>,
    pub(super) gap: Distance,
    pub(super) to_end: Distance,
}

/// What the motion step's planning needs of the world besides the objects' places.
#[derive(Debug)]
pub struct Planning<'a> {
    /// The planners, the mesh and the short pather.
    pub pathing: &'a mut Pathing,
    /// The world's time this update (`FUN_1415300d0`), which a new path records.
    pub now: WorldTime,
    /// The owner's type (`player+0x10`): a computer player's (`FUN_141659cf0`) plans may stop
    /// near the goal.
    pub player_kind: PlayerKind,
    /// The Object stream (`DAT_143f07160`), which a long new path's repath time draws from.
    pub object: &'a mut Well512,
    /// The world's unit groups as they are now, which the long pather reads for a unit in a
    /// group (its group's differentiator units, `FUN_140f9eb70`).
    pub groups: &'a UnitGroups,
}

impl Planning<'_> {
    /// The same planning for a call that ends before this one is used again.
    pub const fn reborrow(&mut self) -> Planning<'_> {
        Planning {
            pathing: &mut *self.pathing,
            now: self.now,
            player_kind: self.player_kind,
            object: &mut *self.object,
            groups: self.groups,
        }
    }
}

/// Where the motion step goes on after a pass over the walked level.
pub(super) enum Next {
    /// Back to the top of the step (`LAB_141151124`).
    Again,
    /// On to the level's state (`LAB_141151b6d`) with what the pass knew of its goal.
    Dispatch,
    /// The step returns.
    Done(Motion),
    /// The step's end on a loop back at its first waypoint (`LAB_141155637`,
    /// [`Mover::loop_end`]).
    ///
    /// [`Mover::loop_end`]: crate::movement::Mover::loop_end
    LoopEnd,
}

/// Where the motion step goes on after a level's path request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum AfterRequest {
    /// Back to the top of the step (`LAB_141151124`) for the level now walked.
    Again,
    /// On to the walk (`LAB_141153bd1`).
    Walk,
    /// The unit stopped (and may have paused itself): the step returns 0.
    Paused,
    /// The level gave up and the unit stopped: the step returns 1.
    GaveUp,
}

/// The squared distance between two footprints on the ground (`FUN_14118f8c0`): per axis the
/// centre distance minus both radii, at least 0, squared and summed.
#[must_use]
pub fn footprint_gap_squared(
    a: Position,
    radius_a: Footprint,
    b: Position,
    radius_b: Footprint,
) -> DistanceSquared {
    let x = ((b.x() - a.x()).abs() - (radius_a.x() + radius_b.x())).max(0.0);
    let y = ((b.y() - a.y()).abs() - (radius_a.y() + radius_b.y())).max(0.0);
    DistanceSquared::new(x * x + y * y)
}

/// The distance between two footprints on the ground (`FUN_14118f680`): per axis the centre
/// distance minus both radii, at least 0, then the length of the two.
#[must_use]
pub fn footprint_gap(
    a: Position,
    radius_a: Footprint,
    b: Position,
    radius_b: Footprint,
) -> Distance {
    let x = ((b.x() - a.x()).abs() - (radius_b.x() + radius_a.x())).max(0.0);
    let y = ((b.y() - a.y()).abs() - (radius_b.y() + radius_a.y())).max(0.0);
    Distance::new(sqrt(x * x + y * y))
}
