//! A unit's movement (`RGE_Moving_Object`, MovingObject.cpp in 158041): the velocity step that
//! moves it along its forward vector, and the motion step that steers it along its path.
//!
//! Per tick, `RGE_Moving_Object::update` (`0x14114dbb0`) runs first: it brings the forward speed
//! toward the acceleration's target and moves the unit `dt × speed` along the forward vector it had
//! after the last tick ([`Mover::step_velocity`]). The actions run later in the same update; a
//! `MoveTo` calls the motion step (object slot 193, `0x141150f00`, [`Mover::motion`]), which turns the
//! unit toward its waypoint and sets the acceleration for the next tick.
//!
//! Vectors are the game's `BVector`s ([`fmath::BVector`](crate::math::fmath::BVector): x, height,
//! y).

use self::long_walk::{Reach, include_kept, may_plan, owner_number, reach};
use self::path_points::{
    AfterRequest, Avoided, Goal, Next, cut_path, ground, ground_line, moved_far, point_or_zero,
    point_or_zero_of, short_range, split, too_short,
};
use crate::GenieError;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Position;
use crate::map::Radii;
use crate::map::spatial::{Placed, Spatial};
use crate::math::Angle;
use crate::math::fmath::BVector;
use crate::math::fmath::{maxss, minss};
use crate::objects::ActionState;
use crate::objects::ObjectId;
use crate::objects::layers::Animated;
use crate::objects::masters::MasterId;
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::world::Seconds;
use crate::world::WorldTime;
use genie_diff::Diff;

mod acceleration;
pub(crate) mod ai_moves;
pub mod explore;
mod explore_state;
pub mod fly;
mod fly_state;
mod levels;
mod long_walk;
use genie_model::movement::move_algorithm;
pub mod move_to;
mod new_path;
mod path_end;
mod path_flags;
mod path_points;
mod path_state;
pub(crate) use self::levels::Levels;
pub use self::move_algorithm::MoveAlgorithm;
pub use self::path_counts::{LinkedPathKind, PlanTries, StepPlans, SubstituteCount, WaitDelays};
pub use self::path_flags::{LevelFlags, PathFlags};
pub use self::path_state::PathState;
pub use self::waypoint_index::WaypointIndex;
mod request;
mod request_plans;
pub use self::acceleration::Acceleration;
pub use self::speed::Speed;
pub use genie_model::movement::TurnRate;
use genie_model::movement::speed;
mod step;
pub use self::step::GroupSpeed;
use genie_model::movement::move_stats;
mod path_counts;
mod unit;
mod walk;
mod waypoint;
mod waypoint_index;
mod waypoint_level;
pub use self::waypoint_level::WaypointLevel;

#[cfg(test)]
use self::long_walk::old_pather_for;
pub(crate) use self::long_walk::{
    LongWalker, ShortAsk, TileAsk, Walker, long_path_for, lowered_units, short_path_for,
    short_path_including, tile_plan_for,
};
pub use self::move_stats::MoveStats;
use self::path_end::ReachAsk;
pub use self::path_points::{Planning, footprint_gap, footprint_gap_squared};
use self::request::LongAsk;
use crate::groups::{GroupId, UnitGroups};
#[cfg(test)]
use crate::math::image::movement::TWO_PI;
use crate::math::image::movement::{EPSILON as MOVEMENT_EPSILON, FAR as MOVEMENT_FAR};
#[cfg(test)]
use crate::math::rng::Well512;
use crate::objects::facing::SpriteFacets;
#[cfg(test)]
use crate::pathing::hull_path::HullPather;
#[cfg(test)]
use crate::pathing::tile_path::Path;
#[cfg(test)]
use crate::pathing::{Pathing, PlanBudget};
use crate::world::WALKER_UNMOVING;
use crate::world::index::ObjectsRef;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::layer::generated::MovementTuning;
pub use genie_model::movement::{Turning, turn_rate, turning};

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
    pub resume: WaypointIndex,
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
            resume: WaypointIndex::NONE,
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
    pub linked_kind: LinkedPathKind,
    /// The path's waypoints (`+0x18`, the path's `+0x438`..`+0x440`).
    pub points: Vec<BVector>,
    /// The level holds a path (`+0x18`, or the one the world's table finds by its id `+0x20`): a
    /// reset level (id −1) holds none, nor does a bird's top level, and the motion step ends at
    /// it.
    pub has_path: bool,
    /// The path's own flags (`+0xc`).
    pub path_flags: PathFlags,
    /// `+0x24`: the index of the waypoint the unit walks to.
    pub waypoint: WaypointIndex,
    /// `+0x28`: its state.
    pub state: PathState,
    /// `+0x2c`: how near the target counts as there.
    pub range: Distance,
    /// `+0x30`: the target object's id, none for a point.
    pub target: Option<ObjectId>,
    /// `+0x34`, in seconds.
    pub pause_time: Seconds,
    /// `+0x3c`: what the walk met on this level ([`LevelFlags`]).
    pub flags: LevelFlags,
    /// The path's length (its `+4`, `mPathLengthValue`).
    pub length: Distance,
    /// The path's creation time (its `+8`, `mCreationTimeValue`): the world's time when it was
    /// made.
    pub created: WorldTime,
    /// `+0x38`, `mContinueCounter`: the plans the level may still try before it gives up.
    pub continue_counter: PlanTries,
    /// `+0x40`, `mRepathTime`: 2 to 2.5 seconds, drawn when the path is planned.
    pub repath_time: Seconds,
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
    pub wait_delays: WaitDelays,
    /// `+0x7fb`, `mAttackMoving`: a level spends no try, and one without a path pauses.
    pub attack_moving: bool,
    /// `+0x870`, `lastMoveTimeValue`: the world's time when the unit last moved.
    pub last_move: WorldTime,
    /// `+0x824`..`+0x82c`, `positionValue`: where it last moved to (x, y, height), which the
    /// position setter compares the new place with (`FUN_141161950`).
    pub moved_at: Position,
    /// `+0x874`, `mConsecutiveSubstituteCount`: above 2 a walk counts its waypoint reached.
    pub substitute_count: SubstituteCount,
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
    pub sprite_facets: Option<SpriteFacets>,
}

/// The moving unit, as the motion step needs it besides its [`Mover`], whose place it walks
/// ([`Walk::position`](crate::movement::move_to::Walk::position)).
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
    pub group: Option<GroupId>,
    /// `+0x650`, `mLastHumanOrderTime`: its rank against a unit it meets (the walk's pause and
    /// the path end's keep), as the walk starts.
    pub human_order: WorldTime,
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

/// Whether an object moves (object slot 185, [`Mover::is_moving`]): the mesh holds the shape
/// only of one that stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveState {
    /// Its velocity is longer than 0.
    Moving,
    /// It stands, or it does not move at all.
    Standing,
}

impl MoveState {
    /// The state of an object with `mover` (none for one that does not move).
    #[must_use]
    pub fn of_mover(mover: Option<&Mover>) -> Self {
        if mover.is_some_and(Mover::is_moving) {
            Self::Moving
        } else {
            Self::Standing
        }
    }
}

/// The gap below which the game takes a distance as zero (`DAT_142eb0c60`).
const EPSILON: f32 = MOVEMENT_EPSILON;
/// The world's up axis (`DAT_143cc2f30`), the yaw's axis.
const UP: BVector = BVector::new(0.0, 1.0, 0.0);

/// No distance (`DAT_14330ebf4`).
const FAR: f32 = MOVEMENT_FAR;
/// The motion step's switches (`DAT_1439e3cc0`, `DAT_1439e3cc4`, `DAT_1439e3cc8`): the short
/// pather, the tile pathfinder and the long pather run; all three are 1 at run time.
const SHORT_PATHER: bool = true;
const TILE_PATHER: bool = true;
const LONG_PATHER: bool = true;

/// `FUN_1411615a0`: how deep two boxes (centre, radii on the ground) overlap, the lesser of the
/// two axes; 0 when they do not.
fn overlap(a: BVector, radius_a: Footprint, b: BVector, radius_b: Footprint) -> f32 {
    let (ar, ad, br, bd) = (radius_a.x(), radius_a.y(), radius_b.x(), radius_b.y());
    let (ax1, ax0, ay0, ay1) = (a.x() + ar, a.x() - ar, a.y() - ad, a.y() + ad);
    let (by0, by1, bx0, bx1) = (b.y() - bd, b.y() + bd, b.x() - br, b.x() + br);
    // COMISS/JNC at 0x1411615fd..0x141161610: only an ordered `>=` leaves, a NaN goes on
    if bx0 >= ax1 || ax0 >= bx1 || by0 >= ay1 || ay0 >= by1 {
        return 0.0;
    }
    let x = minss(ax1, bx1) - maxss(ax0, bx0);
    let y = minss(ay1, by1) - maxss(ay0, by0);
    minss(x, y)
}

/// The point `FUN_140f9bcd0` returns outside a path: `DAT_143f0dc88`, in `.bss` (zero at the
/// load; probed zero in records/425906646 with no write in the update after).
pub(crate) const OUTSIDE_PATH: BVector = BVector::ZERO;

/// A planned path shorter than this replaces no short path (`DAT_143cc2f28`), and a path is long
/// when it is longer than this and than 1.25 times the way (`DAT_14330ebb8`, `DAT_142eb56a8`).
const LONG: Distance = Distance::new(5.0);
const DETOUR: f32 = 1.25;
/// How many plans one motion step makes at most.
const PLANS: StepPlans = StepPlans::new(3);
/// The motion step's include list (`DAT_143f4a000`, its count `local_19bc`) holds at most 0x100
/// kept hits (`CMP RCX, 0x100; JGE` at 0x1411547cc).
const INCLUDED: usize = 0x100;

impl PathData {
    /// `FUN_140f98bc0` with 1 on the level's path: the waypoint after `index`. A path that does
    /// not loop ([`PathFlags::LOOPS`]) has none (−1); a loop goes on to its next point, or 0 past
    /// its last; a path that goes back and forth ([`PathFlags::BACK_AND_FORTH`]) turns at its
    /// ends: at its first point it goes forward again ([`PathFlags::FORWARD`] set), at its last
    /// back (cleared). The units' and the groups' walks both take it.
    pub(crate) fn looping_next(&mut self, index: WaypointIndex) -> WaypointIndex {
        let n = self.points.len();
        if !self.path_flags.contains(PathFlags::LOOPS) {
            return WaypointIndex::NONE;
        }
        if !self.path_flags.contains(PathFlags::BACK_AND_FORTH) {
            let next = index.next();
            return if next.within(n) {
                next
            } else {
                WaypointIndex::FIRST
            };
        }
        if self.path_flags.contains(PathFlags::FORWARD) {
            let next = index.next();
            if !next.within(n) {
                self.path_flags.remove(PathFlags::FORWARD);
                return WaypointIndex::last_of(n).previous();
            }
            next
        } else {
            let next = index.previous();
            if next < WaypointIndex::FIRST {
                self.path_flags.insert(PathFlags::FORWARD);
                return WaypointIndex::SECOND;
            }
            next
        }
    }

    /// Whether the level's target is object `id`, as the walk tests a hit (`0 <= target &&
    /// id == target`): a recyclable object's id is no level's target there.
    pub(crate) fn targets(&self, id: ObjectId) -> bool {
        self.target.is_some_and(|t| !t.is_recyclable() && t == id)
    }

    /// A new level (`FUN_140f8f670`) with a new path (`FUN_14152eea0`, flags 6, made `now`).
    pub(crate) const fn new(now: WorldTime) -> Self {
        Self {
            level: WaypointLevel::Walked,
            linked_kind: LinkedPathKind::None,
            points: Vec::new(),
            has_path: true,
            path_flags: PathFlags::NEW,
            waypoint: WaypointIndex::FIRST,
            state: PathState::Idle,
            range: Distance::ZERO,
            target: None,
            pause_time: Seconds::ZERO,
            flags: LevelFlags::NONE,
            length: Distance::ZERO,
            created: now,
            continue_counter: PlanTries::FULL,
            repath_time: Seconds::ZERO,
            ignore: Vec::new(),
            ignored: Vec::new(),
        }
    }

    /// A new level without a path (`FUN_140f8f670` alone): target -1, the plans
    /// [`PlanTries::FULL`], everything else 0.
    #[must_use]
    pub const fn without_path() -> Self {
        Self {
            level: WaypointLevel::Walked,
            linked_kind: LinkedPathKind::None,
            points: Vec::new(),
            has_path: false,
            path_flags: PathFlags::NONE,
            waypoint: WaypointIndex::FIRST,
            state: PathState::Idle,
            range: Distance::ZERO,
            target: None,
            pause_time: Seconds::ZERO,
            flags: LevelFlags::NONE,
            length: Distance::ZERO,
            created: WorldTime::ZERO,
            continue_counter: PlanTries::FULL,
            repath_time: Seconds::ZERO,
            ignore: Vec::new(),
            ignored: Vec::new(),
        }
    }

    /// The waypoint at `index` (`FUN_140f9bcd0`); outside the path the game returns its static
    /// point `DAT_143f0dc88` ([`OUTSIDE_PATH`]).
    fn point(&self, index: WaypointIndex) -> BVector {
        index
            .slot()
            .and_then(|i| self.points.get(i))
            .copied()
            .unwrap_or(OUTSIDE_PATH)
    }

    /// `FUN_140f96050`'s test of the top level: a level without a path (`+0x18` null and none in
    /// the world's table by its id `+0x20`) has none (0); else waypoints after the current one:
    /// not `count - 1 <= waypoint` in 32 bits, so an empty path has more for a waypoint below -1.
    const fn has_more(&self) -> bool {
        self.has_path && !self.waypoint.at_or_past_last(self.points.len())
    }
}

/// `LAB_141152ec1`'s test whether a waypoint `way` away is far enough for the long pather: a
/// tile or more without a target; with one (its own gap, `FUN_14118f680`, and its master), its
/// gap a tile or more (`COMISS`/`JNC` at 0x141152f0e: a NaN is near) or its master 45 however
/// near.
fn far_waypoint(
    way: Distance,
    target: Option<(Distance, MasterId)>,
    roles: &Roles,
    movement_tuning: &MovementTuning,
) -> bool {
    let way = way.get();
    target.map_or(movement_tuning.close <= way, |(gap, master)| {
        // A target of this master (raw `0x60c9`) or class gives the tile planner of a reach test this
        // much more range (`FUN_140fa0370`, `DAT_142eb1c98`).
        movement_tuning.close <= gap.get() || roles.of_master(master).contains(MasterRoles::DOCK)
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
    annex: impl Fn(Distance) -> Distance,
    by_id: ObjectsRef<'_>,
    roles: &Roles,
) -> Distance {
    // The class the motion step measures by its annexes (`-0xe85` encoded).
    if !roles.of_class(target.class).contains(ClassRoles::GATE) {
        return own;
    }
    let mut gap = own;
    for a in by_id.annexes(id).ids().take(2) {
        if let Some(p) = world.get(a) {
            gap = Distance::new(minss(
                annex(footprint_gap(at, radius, p.position, p.radius)).get(),
                gap.get(),
            ));
        }
    }
    gap
}

/// [`Mover::avoid`]'s rank for the pause of a walker (`me` of `player`, of `player_type`)
/// over the hit `id` (`other` its record): a human walker ranks another player's unit, or one of
/// its own ordered at another time, by their last human order times (`+0x650`: the older order
/// pauses); else two members of one unit group by their places in it (None for a group the world
/// does not hold: neither pauses), any other two by id (slot 0x5f8).
fn pause_rank(
    me: &Me,
    (player, player_kind): (PlayerId, PlayerKind),
    (id, other): (ObjectId, &Placed),
    groups: Option<&UnitGroups>,
    by_id: ObjectsRef<'_>,
) -> Option<bool> {
    let mine = me.human_order;
    let theirs = by_id.gait(id).map_or(WorldTime::ZERO, |g| g.human_order);
    Some(
        if !player_kind.is_computer() && (other.player != player || theirs != mine) {
            theirs < mine
        } else if let Some(group) = me.group
            && by_id.gait(id).and_then(|g| g.group) == Some(group)
        {
            let g = groups.and_then(|gs| gs.get(group))?;
            g.index_of(me.id) < g.index_of(id)
        } else {
            me.id < id
        },
    )
}

/// [`Mover::reached`]'s waypoints too near the map's edge for the unit (`FUN_140f98d50`): from
/// the level's waypoint on, each whose x or y lies within the unit's radius (`radius`) of an
/// edge of the map (`map`) goes, until one lies inside.
fn drop_edge_waypoints(level: &mut PathData, radius: Radii, map: GridSize) {
    let (rx, ry) = (radius.x(), radius.y());
    let (w, h) = (map.width(), map.height());
    while level.waypoint.within(level.points.len()) {
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
fn remove_point(points: &mut Vec<BVector>, i: WaypointIndex) -> bool {
    match i.slot() {
        Some(k) if k < points.len() => {
            let _ = points.remove(k);
            true
        }
        _ => false,
    }
}

/// Slot 0x5d0 of a unit whose `MoveTo` went, on its `mover`.
///
/// # Errors
///
/// A unit that does not move ([`crate::world::WALKER_UNMOVING`]), and the stop's.
pub(crate) fn stop_mover(mover: Option<&mut Mover>) -> Result<(), GenieError> {
    mover.ok_or(WALKER_UNMOVING)?.stop()
}

#[cfg(test)]
pub(crate) mod tests;
