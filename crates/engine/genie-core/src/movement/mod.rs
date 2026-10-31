//! A unit's movement (`RGE_Moving_Object`, MovingObject.cpp in 158041): the velocity step that
//! moves it along its forward vector, and the motion step that steers it along its path.
//!
//! Per tick, `RGE_Moving_Object::update` (`0x14114dbb0`) runs first: it brings the forward speed
//! toward the acceleration's target and moves the unit `dt × speed` along the forward vector it had
//! after the last tick ([`Mover::step_velocity`]). The actions run later in the same update; a
//! `MoveTo` calls the motion step (object slot 193, `0x141150f00`, [`Mover::motion`]), which turns the
//! unit toward its waypoint and sets the acceleration for the next tick.
//!
//! Vectors are the game's `BVector`s ([`fmath::BVector`]: x, height, y).

use self::long_walk::{Reach, include_kept, may_plan, owner_number, reach};
use self::path_points::{
    ALLY_PAUSE, AfterRequest, Avoided, FAST_TURN, Goal, Next, OTHER_PAUSE, STUCK_PAUSE, cut_path,
    ground, ground_line, moved_far, point_or_zero, point_or_zero_of, short_range, split, too_short,
};
use crate::GenieError;
use crate::map::Distance;
use crate::map::DistanceSquared;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Position;
use crate::map::Radii;
use crate::map::spatial::{Placed, Spatial};
use crate::math::Angle;
use crate::math::fmath::{self, BVector};
use crate::objects::ActionState;
use crate::objects::ObjectId;
use crate::objects::layers::Animated;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::objects::masters::class;
use crate::objects::masters::ids::ID_45;
use crate::objects::unit::Unit;
use crate::players::PlayerId;
use crate::world::Seconds;
use crate::world::WorldTime;
use genie_diff::Diff;

pub(crate) mod ai_moves;
pub mod explore;
mod explore_state;
pub mod fly;
mod fly_state;
mod long_walk;
pub mod move_to;
mod new_path;
mod path_end;
mod path_points;
mod path_state;
pub use self::path_state::PathState;
mod request;
mod request_plans;
mod speed;
mod turn_rate;
pub use self::speed::Speed;
pub use self::turn_rate::TurnRate;
mod step;
pub use self::step::GroupSpeed;
mod unit;
mod walk;
mod waypoint;
mod waypoint_level;
pub use self::waypoint_level::WaypointLevel;

#[cfg(test)]
use self::long_walk::old_pather_for;
pub(crate) use self::long_walk::{
    LongWalker, ShortAsk, TileAsk, Walker, long_path_for, lowered_units, short_path_for,
    short_path_including, tile_plan_for,
};
use self::path_end::ReachAsk;
#[cfg(test)]
use self::path_points::TWO_PI;
pub use self::path_points::{
    Planning, Turning, footprint_gap, footprint_gap_squared, turn_rate, turning,
};
use self::request::LongAsk;
#[cfg(test)]
use crate::math::rng::Well512;
#[cfg(test)]
use crate::pathing::hull_path::HullPather;
#[cfg(test)]
use crate::pathing::tile_path::Path;
#[cfg(test)]
use crate::pathing::{Pathing, PlanBudget};

/// The velocity data (`+0x868`): velocity and acceleration in the unit's own frame; component 2 is
/// along the forward vector.
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct Velocity {
    /// The velocity.
    pub velocity: BVector,
    /// The acceleration.
    pub acceleration: BVector,
}

/// What a `MoveTo`'s update in state 3 (`0x140aa2a70`) reads of its unit's AI for its path
/// request ([`Mover::request_path_asked`]); [`PathAsk::default`] is a `MoveTo` without that say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PathAsk {
    /// A unit already in range ends the `MoveTo` (`set_state` 1): one with a unit AI not on a
    /// patrol order (711, 733); else the path request runs.
    pub ends_in_range: bool,
    /// `param_7` 1: the path loops (its flags' bit 0), on a patrol order.
    pub patrol: bool,
    /// `param_8` 1: the path goes back and forth (bit 1).
    pub back_and_forth: bool,
    /// `param_9`: the waypoint of the saved patrol the unit walks on from, −1 for a new path from
    /// the unit to the goal.
    pub resume: i32,
    /// `param_10`: 1 (`Some(true)`) the unit attack-moves (`+0x7fb`) and the path does not loop,
    /// 0 it no longer does, 2 (`None`) it stays as it was.
    pub attack_move: Option<bool>,
}

impl Default for PathAsk {
    fn default() -> Self {
        Self {
            ends_in_range: true,
            patrol: false,
            back_and_forth: false,
            resume: -1,
            attack_move: None,
        }
    }
}

/// One level of a unit's path data (`+0x858`; each level's parent at `+8`).
#[derive(Clone, Debug, PartialEq, Diff)]
pub struct PathData {
    /// `+0x14`, `mWaypointLevel`: 0 is the path the unit walks.
    pub level: WaypointLevel,
    /// `+0x10`, `mLinkedPathType`: what a finished level hands its parent.
    pub linked_type: i32,
    /// The path's waypoints (`+0x18`, the path's `+0x438`..`+0x440`).
    pub points: Vec<BVector>,
    /// The level holds a path (`+0x18`, or the one the world's table finds by its id `+0x20`): a
    /// reset level (id −1) holds none, nor does a bird's top level, and the motion step ends at
    /// it.
    pub has_path: bool,
    /// The path's own flags (`+0xc`).
    pub path_flags: u32,
    /// `+0x24`: the index of the waypoint the unit walks to.
    pub waypoint: i32,
    /// `+0x28`: its state.
    pub state: PathState,
    /// `+0x2c`: how near the target counts as there.
    pub range: Distance,
    /// `+0x30`: the target object's id, none for a point.
    pub target: Option<ObjectId>,
    /// `+0x34`, in seconds.
    pub pause_time: Seconds,
    /// `+0x3c`: bit 0 a waypoint was reached, bit 1 it was the waypoint itself, bit 3 the tile
    /// pathfinder found only the nearest point.
    pub flags: u32,
    /// The path's length (its `+4`, `mPathLengthValue`).
    pub length: Distance,
    /// The path's creation time (its `+8`, `mCreationTimeValue`): the world's time when it was
    /// made.
    pub created: WorldTime,
    /// `+0x38`, `mContinueCounter`: the plans the level may still try before it gives up.
    pub continue_counter: i32,
    /// `+0x40`, `mRepathTime`.
    pub repath_time: f32,
    /// The path's hash set of ignored units (`+0x18`, `FUN_140f96120`), which the save writes as
    /// `numberIgnoreUnits` and the ids after it (`0x140f98f00`); empty in this game (the probe
    /// `path_sets.py`).
    pub ignore: Vec<ObjectId>,
    /// The path's ignore units (`+0x450`, the ids from `+0x454`), which the walk's avoidance reads
    /// (`FUN_140f98d20`): a short path made in the tick its top path was made takes the unit's
    /// own (`+0x670`, `FUN_140f9b7a0`). The save does not hold them: a loaded path takes them
    /// back by that rule (the oracle's import).
    #[diff(unsaved)]
    pub ignored: Vec<ObjectId>,
}

/// What movement reads of the unit's master.
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct MoveStats {
    /// The master's `fly_mode` (`+0x608`): how its place takes the ground's height
    /// ([`Mover::grounded`]).
    pub fly_mode: u8,
    /// The top speed (object slot 65, `maximumSpeed`, `0x141202c70` -> `FUN_14114d1d0`): the
    /// master's speed ([`MoveStats::base`]), times the current sprite's speed factor when that is
    /// above 0. A column derived from the two: the sprite's one transform writes it
    /// ([`crate::objects::sprite::new_sprite`]), and so does a change of the base.
    pub speed: Speed,
    /// The master's speed (`+0x740`, the master's slot 0xe8): the top speed's base.
    pub base: Speed,
    /// Master `+0x774`: the most the unit turns per second ([`turn_rate`]).
    pub turn_rate: TurnRate,
    /// Master `+0x76c` and `+0x770`: the turn radius is `turn_radius × speed / turn_radius_speed`
    /// (0 and the largest float for every master the game copies, [`turn_rate`]).
    pub turn_radius: Distance,
    /// `+0x770`: the speed at which the turn radius is its value.
    pub turn_radius_speed: Speed,
    /// Master `+0x5d8`, `+0x5dc`, `+0x5e0`.
    #[diff(gap = "an own master's radius (a foundation's, a dying annex's) is not modelled")]
    pub radius: Radii,
    /// The master's terrain restriction (`+0x606`) and the terrain classes it lets the unit stand
    /// on (its mask).
    pub restriction: crate::map::RestrictionId,
    /// The terrain classes the restriction lets it stand on (its mask).
    pub restriction_mask: crate::map::TerrainMask,
    /// The master's turn time (`+0x758`, the `.dat`'s `rotation_speed`): the seconds between the
    /// facet steps of a unit facing a target; 0 faces at once (`FUN_141162db0`).
    pub turn_time: Seconds,
    /// The master's move algorithm (`+0x768`, the `.dat`'s `move_algorithm`): bit 0 has a stop
    /// reset the unit's sprite (slot 0x5d8).
    pub move_algorithm: u8,
    /// Master `+0x780`, `min_collision_size_multiplier`: the walk's query grows moving allies by
    /// the unit's radius times this.
    pub min_collision: f32,
    /// The master's object flags (`+0x654`) have bit 1: the unit turns a hundred times faster.
    pub fast_turn: bool,
    /// The facets the facing routines read of the unit's current sprite: the master's, when
    /// every sprite it shows has the same count and bit 1 (None otherwise, or without sprites).
    pub facets: Option<crate::objects::facing::SpriteFacets>,
    /// The master's weapon range (slot 0x230, `+0x828`; 0 for an object without combat values),
    /// which the drop-off walker's start test reads (`FUN_140fa0110`).
    pub weapon_range: Distance,
}

impl MoveStats {
    /// The top speed with the sprite `shown` (None: none): the master's speed [`MoveStats::base`]
    /// times the sprite's factor ([`crate::objects::sprite::top_speed`]). The one writer of
    /// [`MoveStats::speed`] for a unit: a new sprite ([`crate::objects::sprite::new_sprite`]), a
    /// standing show ([`Mover::shows`]) and a change of the base call it.
    pub(crate) fn show(&mut self, shown: Option<&crate::objects::sprite::Graphic>) {
        self.speed = Speed::new(crate::objects::sprite::top_speed(self.base.get(), shown));
    }
}

/// The moving object's own layer (`RGE_Moving_Object`, types 30 and up) with the animated layer
/// below it ([`crate::objects::layers`]): a moving unit's movement state.
#[derive(Clone, Debug, PartialEq, Diff)]
pub struct Mover {
    /// The animated layer below it (`+0x7b8`), which its stop clears ([`Mover::stop`]).
    pub animated: Animated,
    /// `+0x830`, a unit vector on the ground.
    pub forward: BVector,
    /// `+0x83c`.
    pub right: BVector,
    /// Its velocity data, when it has any.
    pub velocity: Option<Velocity>,
    /// The path data, the level the unit walks first, each next one its parent.
    pub paths: Vec<PathData>,
    /// `+0x878`: moveForward's distance factor.
    pub factor: f32,
    /// What its master gives its movement.
    pub stats: MoveStats,
    /// `+0x7f9`, `waitingToMoveValue`.
    pub waiting: u8,
    /// `+0x7fa`, `numberWaitDelays`: plans that failed for want of budget (trade carts).
    pub wait_delays: u8,
    /// `+0x7fb`, `mAttackMoving`: a level spends no try, and one without a path pauses.
    pub attack_moving: bool,
    /// `+0x870`, `lastMoveTimeValue`: the world's time when the unit last moved.
    pub last_move: WorldTime,
    /// `+0x824`..`+0x82c`, `positionValue`: where it last moved to (x, y, height), which the
    /// position setter compares the new place with (`FUN_141161950`).
    pub moved_at: Position,
    /// `+0x874`, `mConsecutiveSubstituteCount`: above 2 a walk counts its waypoint reached.
    pub substitute_count: i32,
    /// The unit's ignore list (`+0x670`, count `+0x678`; the save's `numberUnits` and `tempInt`
    /// of the static part): the other units of an order to several (`FUN_14118c790`), which the
    /// short pather ignores for a top path made this update. A stop empties it.
    pub ignore_units: Vec<ObjectId>,
    /// The unit's own waypoints (`+0x7f0`, count `+0x7fc`; the save's `userDefinedWaypoints`),
    /// in the game's layout (x, height, y): a patrol's points before its last (slot 0x448,
    /// [`Mover::add_user_waypoint`]), which the next path request walks through and empties
    /// ([`Mover::request_path_asked`]).
    pub user_waypoints: Vec<BVector>,
    /// `+0x7ec`, `turn_towards_time`: the world time in seconds of its last facet step toward a
    /// target (`FUN_141162db0`).
    pub last_turn: Seconds,
    /// `+0x7e8`: where the unit faces, radians 0 to 2π ([`crate::objects::facing`]).
    pub angle: Angle,
    /// The pauses its walk gave units it met (slot 0x5f0 on each: the met unit's walked level's
    /// `+0x34`), for the caller to hand on.
    pub pauses: Vec<(ObjectId, Seconds)>,
    /// The kept hits its walk met of the walked level's target's master, of a player neither
    /// counts an ally: the motion step tells the unit AI of each (its slot 0x38 with
    /// [`crate::unit_ai::NotifyKind::IN_THE_WAY`] and the hit's id), which the caller passes on.
    pub in_the_way: Vec<ObjectId>,
    /// The facets of the unit's current sprite (`+0x140` -> `+0xa`, and its sequence flag bit
    /// 1), which the yaw and the facing read: the sprite the slice holds as the unit's update
    /// starts ([`crate::objects::unit::Unit`]'s), or a new unit's standing sprite; None for neither (the
    /// master's [`MoveStats::facets`] then). Not saved: the update's copy of its sprite's.
    #[diff(unsaved)]
    pub sprite_facets: Option<crate::objects::facing::SpriteFacets>,
}

/// The moving unit, as the motion step needs it besides its [`Mover`], whose place it walks
/// ([`Mover::position`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Me {
    /// The unit.
    pub id: ObjectId,
    /// Its top action's state (slot 0x2b0, `RGE_Action_Object::actionState`; 0 without one),
    /// which the short pather's ignore list reads: an action that walks after changing its state
    /// sets its own.
    pub action_state: ActionState,
    /// Its group (`mGroupID`, `+0x6d0`; -1 for none), whose differentiator units the long pather
    /// lowers (`FUN_140f9eb70`).
    pub group: Option<crate::groups::GroupId>,
}

/// What the motion step returns to the `MoveTo` action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    /// 0: still on the way.
    Moving,
    /// 1: the unit cannot move or has no path.
    Done,
    /// 2: it arrived.
    Arrived,
}

/// The gap below which the game takes a distance as zero (`DAT_142eb0c60`).
const EPSILON: f32 = crate::math::image::movement::EPSILON;
/// The world's up axis (`DAT_143cc2f30`), the yaw's axis.
const UP: BVector = BVector::new(0.0, 1.0, 0.0);
/// How far inside the map's edge a moving unit's footprint stays (`DAT_142eb99cc`), and how far
/// below the edge a position stays (`DAT_142eb1e9c`).
const EDGE_GAP: f32 = crate::math::image::movement::EDGE_GAP;
const EDGE_POSITION_GAP: f32 = crate::math::image::movement::EDGE_POSITION_GAP;
/// A walk to a point checks its range only above this (`DAT_142eb5690`).
const RANGE_FLOOR: f32 = crate::math::image::movement::RANGE_FLOOR;
/// A point within this many tiles is the short pather's to reach (`FUN_140fa0370`,
/// `DAT_142eb0f4c`).
const REACH_SHORT: f32 = crate::math::image::movement::REACH_SHORT;
/// A point whose squared gap to the unit's footprint is below this is not reached when the
/// short pather did not reach it (`FUN_140fa0370`, `DAT_14330eb8c`).
const REACH_NEAR: DistanceSquared = DistanceSquared::new(crate::math::image::movement::REACH_NEAR);
/// The master the long pather lets move without a speed (`FUN_140f9eb70`, raw id `0x2a`).
pub(crate) const LONG_PATH_MOVER: MasterId = MasterId::named(42);
const WIDE_CLASS: ClassId = class::CLASS_33;
const WIDE_RANGE: Distance = Distance::new(crate::math::image::movement::WIDE_RANGE);
/// A long path's stretch is checked for walls with the unit's radius times this
/// (`DAT_142e85190`).
const LONG_PATH_GROW: f32 = crate::math::image::movement::LONG_PATH_GROW;
/// No distance (`DAT_14330ebf4`).
const FAR: f32 = crate::math::image::movement::FAR;
/// The motion step's switches (`DAT_1439e3cc0`, `DAT_1439e3cc4`, `DAT_1439e3cc8`): the short
/// pather, the tile pathfinder and the long pather run; all three are 1 at run time.
const SHORT_PATHER: bool = true;
const TILE_PATHER: bool = true;
const LONG_PATHER: bool = true;
/// The short pather serves a waypoint this near (`DAT_142ec9ec0`), and one this near a target
/// before the tile pathfinder (`DAT_142eb0f4c`).
const SHORT_REACH: f32 = crate::math::image::movement::SHORT_REACH;
const TARGET_SHORT_REACH: f32 = crate::math::image::movement::TARGET_SHORT_REACH;
/// A waypoint or target nearer than this takes the close-in path (`DAT_14330eb8c`).
const CLOSE: f32 = crate::math::image::movement::CLOSE;
/// A unit pushed into a met object counts beyond its radius times this (`DAT_142eb880c`).
const PUSHED: f32 = crate::math::image::movement::PUSHED;

/// `FUN_1411615a0`: how deep two boxes (centre, radii on the ground) overlap, the lesser of the
/// two axes; 0 when they do not.
fn overlap(a: BVector, a_radius: Footprint, b: BVector, b_radius: Footprint) -> f32 {
    let (ar, ad, br, bd) = (a_radius.x(), a_radius.y(), b_radius.x(), b_radius.y());
    let (ax1, ax0, ay0, ay1) = (a.x() + ar, a.x() - ar, a.y() - ad, a.y() + ad);
    let (by0, by1, bx0, bx1) = (b.y() - bd, b.y() + bd, b.x() - br, b.x() + br);
    // COMISS/JNC at 0x1411615fd..0x141161610: only an ordered `>=` leaves, a NaN goes on
    if bx0 >= ax1 || ax0 >= bx1 || by0 >= ay1 || ay0 >= by1 {
        return 0.0;
    }
    let x = fmath::minss(ax1, bx1) - fmath::maxss(ax0, bx0);
    let y = fmath::minss(ay1, by1) - fmath::maxss(ay0, by0);
    fmath::minss(x, y)
}

/// A unit's own waypoint on a tile's edge moves this far into the tile (`FUN_14114e920`,
/// `DAT_142e7f384`).
const USER_WAYPOINT_MIDDLE: f32 = crate::math::image::movement::USER_WAYPOINT_MIDDLE;
/// How far level 0's look ahead reaches on its parent's path, squared (`DAT_14330ebcc`).
const LOOK_AHEAD: f32 = crate::math::image::movement::LOOK_AHEAD;
/// `FUN_141142750` grows the walk's query by the unit's radius times `DAT_142e7f384`.
const STRAIGHT_GROW: f32 = crate::math::image::movement::STRAIGHT_GROW;
/// A path shorter than this is too short (`DAT_142eb99cc`).
const SHORTEST: Distance = Distance::new(crate::math::image::movement::SHORTEST);
/// The point `FUN_140f9bcd0` returns outside a path: `DAT_143f0dc88`, in `.bss` (zero at the
/// load; probed zero in records/425906646 with no write in the update after).
pub(crate) const OUTSIDE_PATH: BVector = BVector::ZERO;

/// A planned path shorter than this replaces no short path (`DAT_143cc2f28`), and a path is long
/// when it is longer than this and than 1.25 times the way (`DAT_14330ebb8`, `DAT_142eb56a8`).
const LONG: Distance = Distance::new(5.0);
const DETOUR: f32 = 1.25;
/// A new level's stretches longer than this squared get a midpoint (`DAT_142ed6f20`).
const SPLIT: f32 = crate::math::image::movement::SPLIT;
/// How many plans one motion step makes at most.
const PLANS: u32 = 3;
/// What a gate's annex's gap loses against the gate's own (`XMM14`, the motion step's load at
/// 0x1411510e7, the same as [`RANGE_FLOOR`]'s).
const ANNEX_GAP: f32 = crate::math::image::movement::RANGE_FLOOR;
/// A path level's plans before it gives up and the plans it may still try (`FUN_140f8f670`,
/// `FUN_140f9b800`, `FUN_140f98e80`).
pub(crate) const CONTINUE: i32 = 10;
/// A new path's flags (`FUN_14152eea0`'s construction).
pub(crate) const NEW_PATH_FLAGS: u32 = 6;
/// A group member's own move's reason (`mReason`, slot 0x5e0, `FUN_141017590`): moved aside
/// (`NO_OWN_MOVE` is none).
pub(crate) const ASIDE: i32 = 2;
/// The widest unit the new short pather takes (`DAT_142e7f384`, `FUN_140f9e540`'s load at
/// 0x140f9e713).
const NEW_PATHER_RADIUS: f32 = crate::math::image::movement::NEW_PATHER_RADIUS;
/// The motion step's include list (`DAT_143f4a000`, its count `local_19bc`) holds at most 0x100
/// kept hits (`CMP RCX, 0x100; JGE` at 0x1411547cc).
const INCLUDED: usize = 0x100;

impl PathData {
    /// Whether the level's target is object `id`, as the walk tests a hit (`0 <= target &&
    /// id == target`): a recyclable object's id is no level's target there.
    pub(crate) fn targets(&self, id: ObjectId) -> bool {
        self.target.is_some_and(|t| !t.is_recyclable() && t == id)
    }

    /// A new level (`FUN_140f8f670`) with a new path (`FUN_14152eea0`, flags 6, made `now`).
    pub(crate) const fn new(now: WorldTime) -> Self {
        Self {
            level: WaypointLevel::WALKED,
            linked_type: 0,
            points: Vec::new(),
            has_path: true,
            path_flags: 6,
            waypoint: 0,
            state: PathState::Idle,
            range: Distance::ZERO,
            target: None,
            pause_time: Seconds::ZERO,
            flags: 0,
            length: Distance::ZERO,
            created: now,
            continue_counter: CONTINUE,
            repath_time: 0.0,
            ignore: Vec::new(),
            ignored: Vec::new(),
        }
    }

    /// A new level without a path (`FUN_140f8f670` alone): target -1, the plans
    /// [`CONTINUE`], everything else 0.
    #[must_use]
    pub const fn without_path() -> Self {
        Self {
            level: WaypointLevel::WALKED,
            linked_type: 0,
            points: Vec::new(),
            has_path: false,
            path_flags: 0,
            waypoint: 0,
            state: PathState::Idle,
            range: Distance::ZERO,
            target: None,
            pause_time: Seconds::ZERO,
            flags: 0,
            length: Distance::ZERO,
            created: WorldTime::ZERO,
            continue_counter: CONTINUE,
            repath_time: 0.0,
            ignore: Vec::new(),
            ignored: Vec::new(),
        }
    }

    /// The waypoint at `index` (`FUN_140f9bcd0`); outside the path the game returns its static
    /// point `DAT_143f0dc88` ([`OUTSIDE_PATH`]).
    fn point(&self, index: i32) -> BVector {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.points.get(i))
            .copied()
            .unwrap_or(OUTSIDE_PATH)
    }

    /// `FUN_140f96050`'s test of the top level: a level without a path (`+0x18` null and none in
    /// the world's table by its id `+0x20`) has none (0); else waypoints after the current one:
    /// not `count - 1 <= waypoint` in 32 bits, so an empty path has more for a waypoint below -1.
    const fn has_more(&self) -> bool {
        self.has_path && self.waypoint < self.points.len() as i32 - 1
    }
}

/// `LAB_141152ec1`'s test whether a waypoint `way` away is far enough for the long pather: a
/// tile or more without a target; with one (its own gap, `FUN_14118f680`, and its master), its
/// gap a tile or more (`COMISS`/`JNC` at 0x141152f0e: a NaN is near) or its master 45 however
/// near.
fn far_waypoint(way: Distance, target: Option<(Distance, MasterId)>) -> bool {
    let way = way.get();
    target.map_or(CLOSE <= way, |(gap, master)| {
        // A target of this master (raw `0x60c9`) or class gives the tile planner of a reach test this
        // much more range (`FUN_140fa0370`, `DAT_142eb1c98`).
        CLOSE <= gap.get() || master == ID_45
    })
}

/// [`Mover::motion_passes`]'s pause of the `level` walked: a paused walk's pause runs down by
/// `dt`; true while some is left, else it ends at 0.
fn pause_holds(level: &mut PathData, dt: Seconds) -> bool {
    if Seconds::ZERO < level.pause_time {
        level.pause_time -= dt;
        if Seconds::ZERO < level.pause_time {
            return true;
        }
        level.pause_time = Seconds::ZERO;
    }
    false
}

/// A gap to the walked level's target `target` (its id and place): for a gate (class 39), the
/// least (`MINSS`) of `own` and its first two annexes' (slot 0x680) gaps from the unit (`near`:
/// its place and radii) through `annex`; for anything else `own`.
pub(crate) fn gate_gap(
    world: &Spatial,
    (at, radius): (Position, Footprint),
    (id, target): (ObjectId, &Placed),
    own: Distance,
    annex: impl Fn(f32) -> f32,
) -> f32 {
    // The class the motion step measures by its annexes (`-0xe85` encoded).
    if target.class != class::GATE {
        return own.get();
    }
    let mut gap = own;
    for a in world.annexes_of(id).into_iter().take(2) {
        if let Some(p) = world.get(a) {
            gap = Distance::new(fmath::minss(
                annex(footprint_gap(at, radius, p.position, p.radius).get()),
                gap.get(),
            ));
        }
    }
    gap.get()
}

/// `FUN_140f98bc0` with 1 on a level's path: the waypoint after `index`. A path that does not
/// loop (flags bit 0) has none (-1); a loop goes on to its next point, or 0 past its last; a path
/// that goes back and forth (bit 1) turns at its ends, bit 2 the way back, which the turn sets
/// or clears.
const fn looping_next(level: &mut PathData, index: i32) -> i32 {
    let n = level.points.len() as i32;
    if level.path_flags & 1 == 0 {
        return -1;
    }
    if level.path_flags & 2 == 0 {
        return if index + 1 < n { index + 1 } else { 0 };
    }
    if level.path_flags & 4 == 0 {
        let next = index - 1;
        if next < 0 {
            level.path_flags |= 4;
            return 1;
        }
        next
    } else {
        let next = index + 1;
        if n <= next {
            level.path_flags &= !4;
            return n - 2;
        }
        next
    }
}

/// [`Mover::avoid`]'s rank for the pause of a walker (`me` of `player`, a `computer`'s or not)
/// over the hit `id` (`other` its record): a human walker ranks another player's unit, or one of
/// its own ordered at another time, by their last human order times (`+0x650`: the older order
/// pauses); else two members of one unit group by their places in it (None for a group the world
/// does not hold: neither pauses), any other two by id (slot 0x5f8).
fn pause_rank(
    me: &Me,
    (player, computer): (PlayerId, bool),
    (id, other): (ObjectId, &Placed),
    world: &Spatial,
    groups: Option<&crate::groups::UnitGroups>,
) -> Option<bool> {
    let mine = world.gait(me.id).map_or(WorldTime::ZERO, |g| g.human_order);
    let theirs = world.gait(id).map_or(WorldTime::ZERO, |g| g.human_order);
    Some(if !computer && (other.player != player || theirs != mine) {
        theirs < mine
    } else if let Some(group) = me.group
        && world.gait(id).and_then(|g| g.group) == Some(group)
    {
        let g = groups.and_then(|gs| gs.get(group))?;
        g.index_of(me.id) < g.index_of(id)
    } else {
        me.id < id
    })
}

/// [`Mover::reached`]'s waypoints too near the map's edge for the unit (`FUN_140f98d50`): from
/// the level's waypoint on, each whose x or y lies within the unit's radius (`radius`) of an
/// edge of the map (`map`) goes, until one lies inside.
fn drop_edge_waypoints(level: &mut PathData, radius: Radii, map: GridSize) {
    let (rx, ry) = (radius.x(), radius.y());
    let (w, h) = (map.width(), map.height());
    while level.waypoint < level.points.len() as i32 {
        let [x, _, y] = level.point(level.waypoint).get();
        if rx <= x && ry <= y && x <= w as f32 - rx && y <= h as f32 - ry {
            break;
        }
        // the index is the waypoint just incremented, and no writer sets one below -1: it is
        // never negative, so the point goes (the game's loop would spin on a failed removal)
        let removed = remove_point(&mut level.points, level.waypoint);
        debug_assert!(removed, "a waypoint below -1");
    }
}

/// `FUN_140f98d50`: a path's point `i` goes, the later ones move down (true); an index outside
/// the path (below 0 or past its last point) changes nothing (false).
fn remove_point(points: &mut Vec<BVector>, i: i32) -> bool {
    match usize::try_from(i) {
        Ok(k) if k < points.len() => {
            let _ = points.remove(k);
            true
        }
        _ => false,
    }
}

impl Unit {
    /// Slot 0x5d0 of a unit whose `MoveTo` went.
    pub(crate) fn stop_mover(&mut self) -> Result<(), GenieError> {
        self.body
            .mover_mut()
            .ok_or(crate::world::WALKER_UNMOVING)?
            .stop()
    }
}

#[cfg(test)]
pub(crate) mod tests;
