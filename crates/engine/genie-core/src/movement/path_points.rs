//! Path points, the gaps between footprints and the position setter's tests.

use crate::map::Distance;
use crate::map::DistanceSquared;
use crate::map::Footprint;
use crate::map::Point;
use crate::map::Position;
use crate::map::spatial::Placed;
use crate::math::Angle;
use crate::math::fmath;
use crate::math::fmath::BVector;
use crate::math::rng::Well512;
use crate::movement::Speed;
use crate::movement::TurnRate;
use crate::movement::{EPSILON, Motion, PathData, RANGE_FLOOR, SHORTEST};
use crate::objects::ObjectId;
use crate::pathing::Pathing;
use crate::pathing::tile_path::Path;
use crate::world::Seconds;
use crate::world::WorldTime;

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
pub(super) fn point_or_zero(path: &Path, index: i32) -> BVector {
    usize::try_from(index)
        .ok()
        .and_then(|i| path.points.get(i))
        .copied()
        .unwrap_or(BVector::ZERO)
}

/// A waypoint of a level's path, or the game's static zero point outside it.
pub(super) fn point_or_zero_of(level: &PathData, index: i32) -> BVector {
    usize::try_from(index)
        .ok()
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
pub(super) fn ground(a: BVector, b: BVector) -> f32 {
    let mut d = fmath::sub(a, b);
    d = d.with_height(0.0);
    fmath::sqrt(d.x() * d.x() + 0.0 + d.y() * d.y())
}

/// The short pather's range for a request (`0x141150f00`): the level's range less 0.1 when its
/// goal is in reach ([`Mover::near_goal`]), at least 0; else 0.
///
/// [`Mover::near_goal`]: crate::movement::Mover::near_goal
pub(super) fn short_range(near: bool, range: Distance) -> Distance {
    if !near {
        return Distance::ZERO;
    }
    let r = range - Distance::new(RANGE_FLOOR);
    if r <= Distance::ZERO {
        Distance::ZERO
    } else {
        r
    }
}

/// A master whose object flags have bit 1 turns as if dt were this many times longer
/// (`DAT_14330ebcc`).
pub(super) const FAST_TURN: f32 = crate::math::image::movement::FAST_TURN;
/// The pause a kept hit's ranking gives a unit the walker outranks: 0.1 for an ally
/// (`DAT_142eb5690`), 0.5 for another (`DAT_142e7f384`).
pub(super) const ALLY_PAUSE: Seconds = Seconds::new(crate::math::image::movement::ALLY_PAUSE);
pub(super) const OTHER_PAUSE: Seconds = Seconds::new(crate::math::image::movement::OTHER_PAUSE);
/// The pause a level at its last waypoint without a path gives its own unit while it may still
/// try (`DAT_142eb5690`).
pub(super) const STUCK_PAUSE: Seconds = Seconds::new(crate::math::image::movement::STUCK_PAUSE);
/// What [`Mover::avoid`] makes of the walk's hits: the hits it keeps and the pauses it gives.
///
/// [`Mover::avoid`]: crate::movement::Mover::avoid
pub(super) struct Avoided {
    pub(super) kept: Vec<ObjectId>,
    pub(super) pauses: Vec<(ObjectId, Seconds)>,
    /// The step hit only the walked level's target: the waypoint counts as reached.
    pub(super) at_target: bool,
}
/// 2π (`DAT_142eb56ac`): over a revolution's time, the most a unit turns per second.
pub(super) const TWO_PI: f32 = crate::math::image::movement::TWO_PI;

/// The most a unit of a moving master turns per second (`+0x774`) as every copy of the master
/// sets it (master slot 0x80, `FUN_1410e3a70`, which also leaves the turn radius 0 and its
/// speed the largest float): with a turn speed (`+0x758`) and a walking sprite, the sprite's
/// facets times the turn speed are the time of a revolution (`+0x778`, slot 0x98
/// `FUN_1410e39e0`), and 2π over that is the rate (`+0x77c`, copied to `+0x774`); without either
/// the largest float. The `.dat`'s own turning values do not survive the copy.
#[must_use]
pub fn turn_rate(
    turn_speed: Seconds,
    walking_facets: Option<crate::objects::FacetCount>,
) -> TurnRate {
    turning(turn_speed, walking_facets, Turning::PRESET).0.rate
}

/// A moving master's turning values (`+0x76c`..`+0x77c`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Turning {
    /// `+0x76c`: the turn radius.
    pub radius: Distance,
    /// `+0x770`: the speed at which the turn radius is its value.
    pub radius_speed: Speed,
    /// `+0x774`: the most the unit turns per second.
    pub rate: TurnRate,
    /// `+0x778`: a revolution's time (slot 0x98, `FUN_1410e39e0`).
    pub revolution: Seconds,
    /// `+0x77c`: 2π over the revolution's time.
    pub revolution_rate: TurnRate,
}

impl Turning {
    /// What the master's three constructors (`FUN_1410d9440`, `FUN_1410d95b0`,
    /// `FUN_1410d9790`) set: radius 0, the largest float, and the 8-byte stores
    /// `0x7f7fffff` at `+0x774` and `+0x77c` (the largest float, then 0 above it).
    pub const PRESET: Self = Self {
        radius: Distance::ZERO,
        radius_speed: Speed::MAX,
        rate: TurnRate::MAX,
        revolution: Seconds::ZERO,
        revolution_rate: TurnRate::MAX,
    };
}

/// `FUN_1410e3a70` (master slot 0x80) on a master's turning values `before`, with its answer: a
/// turn speed (`+0x758`) of 0 resets them (radius 0, the largest float for the radius speed,
/// the rate and the revolution's rate, the revolution time 0; false); with a walking sprite
/// (`+0x748`) the facets times the turn speed are the revolution's time (`FUN_1410e39e0`: 2π
/// over it, the largest float for 0) and the rate takes 2π over it (true); without one nothing
/// changes (false).
#[must_use]
pub fn turning(
    turn_speed: Seconds,
    walking_facets: Option<crate::objects::FacetCount>,
    before: Turning,
) -> (Turning, bool) {
    if turn_speed == Seconds::ZERO {
        return (Turning::PRESET, false);
    }
    let Some(facets) = walking_facets else {
        return (before, false);
    };
    let revolution = Seconds::new(f32::from(facets.get()) * turn_speed.get());
    let revolution_rate = if revolution == Seconds::ZERO {
        TurnRate::MAX
    } else {
        Angle::new(TWO_PI) / revolution
    };
    let after = Turning {
        revolution,
        revolution_rate,
        rate: revolution_rate,
        ..before
    };
    (after, true)
}

/// A path too short to walk: fewer than two points or shorter than 0.01.
pub(super) fn too_short(path: &Path) -> bool {
    path.points.len() < 2 || path.length < SHORTEST
}

/// `FUN_140f9b8f0`: a midpoint between points `i` and `i + 1` (from `i` half the stretch along
/// it), unless the stretch is shorter than 1e-7.
pub(super) fn split(path: &mut Path, i: i32) {
    let Ok(at) = usize::try_from(i) else {
        return;
    };
    // a signed `count - 1`: an empty path has no stretch
    if path.points.len() as i64 - 1 <= at as i64 {
        return;
    }
    let (a, b) = (path.points[at], path.points[at + 1]);
    let d = fmath::sub(b, a);
    let length = fmath::sqrt(d.x() * d.x() + d.height() * d.height() + d.y() * d.y());
    if length < EPSILON {
        return;
    }
    let again = fmath::sqrt(d.x() * d.x() + d.height() * d.height() + d.y() * d.y());
    let half = length * 0.5;
    let step = BVector::new(
        (d.x() / again) * half,
        (d.height() / again) * half,
        (d.y() / again) * half,
    );
    let _ = path.insert(at + 1, fmath::add(a, step));
}

/// What the motion step knows of where the walked level goes: its target, the gap to it
/// (`fVar38`, far without one) and the gap to where the top level's path ends, or the target's
/// (`fVar40`).
#[derive(Clone, Copy, Debug)]
pub(super) struct Goal<'w> {
    pub(super) target: Option<(ObjectId, &'w Placed)>,
    pub(super) gap: Distance,
    pub(super) to_end: f32,
}

/// What the motion step's planning needs of the world besides the objects' places.
#[derive(Debug)]
pub struct Planning<'a> {
    /// The planners, the mesh and the short pather.
    pub pathing: &'a mut Pathing,
    /// The world's time this update (`FUN_1415300d0`), which a new path records.
    pub now: WorldTime,
    /// The owner is a computer player (`FUN_141659cf0`, `player+0x10 == 3`): its plans may stop
    /// near the goal.
    pub computer: bool,
    /// The Object stream (`DAT_143f07160`), which a long new path's repath time draws from.
    pub object: &'a mut Well512,
    /// The owner's construction rate (attribute 195), which a builder's work reads.
    pub construction_rate: f32,
    /// The world's unit groups as they are now, which the long pather reads for a unit in a
    /// group (its group's differentiator units, `FUN_140f9eb70`).
    pub groups: &'a crate::groups::UnitGroups,
}

impl Planning<'_> {
    /// The same planning for a call that ends before this one is used again.
    pub const fn reborrow(&mut self) -> Planning<'_> {
        Planning {
            pathing: &mut *self.pathing,
            now: self.now,
            computer: self.computer,
            object: &mut *self.object,
            construction_rate: self.construction_rate,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    a_radius: Footprint,
    b: Position,
    b_radius: Footprint,
) -> DistanceSquared {
    let x = ((b.x() - a.x()).abs() - (a_radius.x() + b_radius.x())).max(0.0);
    let y = ((b.y() - a.y()).abs() - (a_radius.y() + b_radius.y())).max(0.0);
    DistanceSquared::new(x * x + y * y)
}

/// The distance between two footprints on the ground (`FUN_14118f680`): per axis the centre
/// distance minus both radii, at least 0, then the length of the two.
#[must_use]
pub fn footprint_gap(
    a: Position,
    a_radius: Footprint,
    b: Position,
    b_radius: Footprint,
) -> Distance {
    let x = ((b.x() - a.x()).abs() - (b_radius.x() + a_radius.x())).max(0.0);
    let y = ((b.y() - a.y()).abs() - (b_radius.y() + a_radius.y())).max(0.0);
    Distance::new(fmath::sqrt(x * x + y * y))
}
