//! The obstruction manager's old short pather (`MShortPather`, the names of the `AoK HD` PDB; build
//! 158041's `FindShortPath` is `FUN_140f85580`): a search around convex hulls of the obstructions
//! near a straight walk: the short path of a unit wider than half a tile (`FUN_140f9e540`'s
//! dispatch, [`crate::movement::short_path_for`]) and the length of a unit's short path to an
//! object (slot 0x3e8).
//!
//! The pather first tries the quick path (`findQuickPath`): the straight walk, or one detour
//! around a single square obstruction. Else it wraps the obstructions of the tiles about the walk
//! (the area: the walk's tiles and two more on each side) into hulls (`buildHulls`): each run of
//! blocked tiles of a row is one box, each tall object on a tile its box, all grown by the unit's
//! radius and merged into a hull with every hull they overlap (`addShapeToHulls`, a gift wrap of
//! both point sets: `MergePointsIntoHull`). A start inside a hull walks out of it first along the
//! sides of the boxes (`pathWithinHull`, the exit paths), a goal inside one is reached the same
//! way backwards (the entrance paths, whose ends join the hull's points); then an A* over the
//! hulls' points: a node that sees the goal ends the search, else its walk to the goal crosses the
//! nearest hull, whose points it follows both ways around. A node beyond the area grows the area
//! by two tiles each way, once. The path found is smoothed (`buildPath`: a node that sees its
//! grandparent skips its parent).
//!
//! Every line test is the obstruction manager's (flags 0x40: moving objects do not count unless
//! the caller's include list holds them), with the unit's radius halved as the buffer, and the
//! pather's ignore list: the caller's objects (slot 0x3e8's the unit and its target) and the tile
//! under a listed one-tile object that the unit's terrain cannot cross.

use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GroundOffset;
use crate::map::Point;
use crate::map::Rect;
use crate::map::TileRect;
use crate::map::spatial::{RecordId, Spatial};
use crate::math::fmath::sqrt;
use crate::objects::ObjectId;
use crate::players::PlayerId;
use crate::world::slots::CheckedIndexExt;
use genie_error::GenieResult;
use std::collections::BTreeSet;

mod area;
mod convex;
mod find;
mod node_lists;
mod obstacles;
mod rounds;
mod setup;
mod walk_in;
mod walks;

#[cfg(test)]
use self::node_lists::{NewNode, UnrefusedNode};
pub(crate) use self::setup::{NO_RESTRICTION, NO_TERRAIN};
pub use self::setup::{quick_reach, quick_reach_from};
#[cfg(test)]
use self::walk_in::WalkIn;
#[cfg(test)]
use self::walks::WalkOut;
use crate::map::TerrainMask;
#[cfg(test)]
use crate::map::spatial::{Placed, tile_record_id};
use crate::math::image::hull_path::{
    DIRECTIONS as HULL_PATH_DIRECTIONS, EPSILON as HULL_PATH_EPSILON,
    FULL_TURN as HULL_PATH_FULL_TURN, HALF as HULL_PATH_HALF, HALF_TURN as HULL_PATH_HALF_TURN,
    HIGH_T as HULL_PATH_HIGH_T, LOW_T as HULL_PATH_LOW_T, MOST as HULL_PATH_MOST,
    NO_ANGLE as HULL_PATH_NO_ANGLE, NUDGE as HULL_PATH_NUDGE, ONE as HULL_PATH_ONE,
    QUADRANT as HULL_PATH_QUADRANT, SIDES as HULL_PATH_SIDES, SQRT_2 as HULL_PATH_SQRT_2,
    TURN as HULL_PATH_TURN, UNTURN as HULL_PATH_UNTURN,
};
use crate::world::objects_ref::ObjectsRef;
use std::ops::Index;

/// How many records the manager's query lists before it stops (`FUN_141051900`: `0x18 < count`).
const LINE_HITS: usize = 25;
/// `MShortPather::IterationCap` (`+0x18`, 0x40 from the constructor `FUN_140f84e40`): how many
/// nodes the search expands.
const ITERATION_CAP: i32 = 0x40;
/// How many nodes a walk out of a hull or into one expands (`pathWithinHull`'s cap, 0x40).
const HULL_ITERATIONS: i32 = 0x40;
/// The exit and the entrance paths the search keeps, each (the pather's `+0x108` and `+0x110`).
const PATHS: usize = 4;
/// How far the area may grow before the search gives up: a grown area of more than 100 tiles.
const MOST_AREA: i32 = 100;
/// How many times the area grows (`FindShortPath`'s loop: twice ends it).
const GROWTHS: i32 = 2;

/// `DAT_142e7f384`: half (the buffer is half the radius; a box's middle).
const HALF: f32 = HULL_PATH_HALF;
/// `DAT_142f43288`: the nudge toward a hull's middle, off a box's side, and the range's slack.
const NUDGE: f32 = HULL_PATH_NUDGE;
/// `DAT_142eb0c60`: two coordinates this close are the same point.
const EPSILON: f32 = HULL_PATH_EPSILON;
/// `DAT_14330eb8c`: one (a direction's length; the intersection's inverse).
const ONE: f32 = HULL_PATH_ONE;
/// `DAT_142ed55d4`, `DAT_142f4328c`: the parameters within which two segments meet.
const LOW_T: f32 = HULL_PATH_LOW_T;
const HIGH_T: f32 = HULL_PATH_HIGH_T;
/// `DAT_14330ebf4`: the largest float, the first distance of the ray searches.
const MOST: f32 = HULL_PATH_MOST;
/// `DAT_142f43290`: the square root of 2, a detour point's distance from a square's middle.
const SQRT_2: f32 = HULL_PATH_SQRT_2;
/// The gift wrap's pseudo-angle (`MergePointsIntoHull`): none yet (`DAT_14330ec20`), the quadrants
/// added left of the y axis and below the x axis (`DAT_142e8c150` 2, `DAT_142eb0f48` 4), the degrees
/// of one (`DAT_142f43294`) and a turn (`DAT_142f02860`, `DAT_142f4329c`).
const NO_ANGLE: f32 = HULL_PATH_NO_ANGLE;
const HALF_TURN: f32 = HULL_PATH_HALF_TURN;
const FULL_TURN: f32 = HULL_PATH_FULL_TURN;
const QUADRANT: f32 = HULL_PATH_QUADRANT;
const TURN: f32 = HULL_PATH_TURN;
const UNTURN: f32 = HULL_PATH_UNTURN;
/// `DAT_1439e6870`: the 16 directions of the first steps out of a box (the game's table: its 12th
/// repeats the 11th).
const DIRECTIONS: [GroundOffset; 16] = GroundOffset::table(HULL_PATH_DIRECTIONS);
/// `DAT_1439e68f0`: the four ray directions of a walk out of a hull or into one (down, up, left,
/// right).
const SIDES: [GroundOffset; 4] = GroundOffset::table(HULL_PATH_SIDES);

/// A box in a hull (`ObstructionInfo`, 0x14 bytes): an obstruction's box grown by the unit's
/// radius and the obstruction's id (an object's, or a tile's [`tile_record_id`]).
///
/// [`tile_record_id`]: crate::map::spatial::tile_record_id
#[derive(Clone, Copy, Debug, PartialEq)]
struct Shape {
    bounds: Rect,
    id: RecordId,
}

impl Shape {
    /// Whether `point` lies inside the box, its sides excluded (`FUN_140f89dc0`'s test).
    fn holds(&self, point: Point) -> bool {
        let rect = self.bounds;
        rect.x0() < point.x()
            && point.x() < rect.x1()
            && rect.y0() < point.y()
            && point.y() < rect.y1()
    }
}

/// A convex hull (`MConvexHull`, 0x40 bytes): its points, the entrance path ending at each point
/// (`+0x18`, the goal's hull only), its bounding box (`+0x20`) and its boxes (`+0x30`).
#[derive(Clone, Debug, Default, PartialEq)]
struct Hull {
    points: Vec<Point>,
    entrances: Vec<Option<EntranceIndex>>,
    min: Point,
    max: Point,
    objects: Vec<Shape>,
}

/// A hull's place in the pather's hulls: the order the building made them in, where a merge
/// moves the last hull into the place of the one it absorbed (`addShapeToHulls`). A node of the
/// A* holds it in its `+0x28` ([`HullIndex::slot`]), −1 for none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct HullIndex(usize);

impl HullIndex {
    /// The hull at place `index` of the hulls.
    const fn new(index: usize) -> Self {
        Self(index)
    }

    /// Its place in the hulls.
    const fn index(self) -> usize {
        self.0
    }

    /// The value a search node's `+0x28` holds for it.
    fn slot(self) -> i32 {
        i32::try_from(self.0).unwrap_or(i32::MAX)
    }
}

/// An entrance path's place in the pather's entrance paths (`+0x110`, at most [`PATHS`]): a
/// point of the goal's hull holds the one ending at it (the game's −1 for none).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct EntranceIndex(usize);

/// A node's place in the pather's nodes (`PathingNode`s in the order made; the two lists'
/// heads first).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct NodeIndex(usize);

/// One of the pather's two node lists, each an open list headed by its own node and a closed
/// list: the search's (0, `+0x20` and `+0x90`) and the walks' in and out of a hull (1, `+0x58`
/// and `+0xa0`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct NodeList(usize);

impl NodeList {
    /// The search's list.
    const SEARCH: Self = Self(0);
    /// The walks' list.
    const WALKS: Self = Self(1);

    /// The node heading its open list.
    const fn head(self) -> NodeIndex {
        NodeIndex(self.0)
    }
}

/// The pather's nodes, read and changed by [`NodeIndex`]: the two lists' heads, then the
/// nodes in the order made.
#[derive(Clone, Debug)]
struct Nodes(Vec<Node>);

impl Nodes {
    /// The two heads alone.
    fn heads() -> Self {
        Self(vec![Node::head(), Node::head()])
    }

    /// Node `node_index`.
    fn checked(&self, node_index: NodeIndex) -> GenieResult<&Node> {
        self.0.checked(node_index.0)
    }

    /// Node `node_index`, if there is one.
    fn get(&self, node_index: NodeIndex) -> Option<&Node> {
        self.0.get(node_index.0)
    }

    /// Node `node_index`, to change.
    fn checked_mut(&mut self, node_index: NodeIndex) -> GenieResult<&mut Node> {
        self.0.checked_mut(node_index.0)
    }

    /// Append `node`; its place.
    fn push(&mut self, node: Node) -> NodeIndex {
        self.0.push(node);
        NodeIndex(self.0.len() - 1)
    }
}

impl Index<NodeIndex> for Nodes {
    type Output = Node;

    fn index(&self, node_index: NodeIndex) -> &Node {
        &self.0[node_index.0]
    }
}

/// The two lists' closed nodes, in the order closed.
#[derive(Clone, Debug, Default)]
struct ClosedLists([Vec<NodeIndex>; 2]);

impl ClosedLists {
    /// List `node_list`'s closed nodes.
    const fn of(&self, node_list: NodeList) -> &Vec<NodeIndex> {
        &self.0[node_list.0]
    }

    /// List `node_list`'s closed nodes, to change.
    const fn of_mut(&mut self, node_list: NodeList) -> &mut Vec<NodeIndex> {
        &mut self.0[node_list.0]
    }
}

/// A search node (`PathingNode`, 0x38 bytes).
#[derive(Clone, Copy, Debug)]
struct Node {
    at: Point,
    parent: Option<NodeIndex>,
    next: Option<NodeIndex>,
    prev: Option<NodeIndex>,
    /// The straight distance to the goal (`+0x20`).
    to_goal: Distance,
    /// The walk from the start (`+0x24`).
    cost: Distance,
    /// The hull of a hull point ([`HullIndex::slot`]); in a walk out of a hull, the box the node
    /// lies on (`+0x28`, −1 none). The game's one field for both, which no reader tells apart.
    hull: i32,
    /// The point of that hull; in a walk out of a hull, the box's side, or the hull's edge the
    /// node left it by (`+0x2c`, −1 none): [`HullPather::may_cast`] reads the edge as a side.
    vertex: i32,
    /// The ray that made the node in a walk out of a hull (`+0x30`).
    direction: i32,
}

impl Node {
    const fn head() -> Self {
        Self {
            at: Point::new(0.0, 0.0),
            parent: None,
            next: None,
            prev: None,
            to_goal: Distance::ZERO,
            cost: Distance::ZERO,
            hull: -1,
            vertex: -1,
            direction: -1,
        }
    }
}

/// The straight walk from `from` to `to`: the root of the squares' sum, y's first, as the pather
/// reckons every node's step, distance to the goal and straight walk.
fn walk_length(from: Point, to: Point) -> Distance {
    Distance::new(sqrt(
        (from.y() - to.y()) * (from.y() - to.y()) + (from.x() - to.x()) * (from.x() - to.x()),
    ))
}

/// A path (`BPath`): its waypoints (x, y; the game keeps a height of 0 between) and the cost the
/// search gave it (`+4`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Waypoints {
    /// The waypoints.
    pub points: Vec<Point>,
    /// The cost.
    pub(crate) cost: Distance,
}

impl Waypoints {
    /// `BPath::InsertWaypoint` (`FUN_140f95060`): a waypoint at `index`, unless the one before or
    /// at it is the same point.
    fn insert(&mut self, index: usize, point: Point) {
        if 0 < index && self.points.get(index - 1) == Some(&point) {
            return;
        }
        if self.points.get(index) == Some(&point) {
            return;
        }
        self.points.insert(index, point);
    }

    /// `FUN_140f95a10`: a waypoint at the end.
    fn push(&mut self, point: Point) {
        self.insert(self.points.len(), point);
    }

    /// `BPath::calculatePathLength` (`FUN_140f95a60`): the waypoints' distances summed.
    #[must_use]
    pub fn length(&self) -> Distance {
        let mut total = 0.0;
        for points in self.points.windows(2) {
            let [from, to] = points else {
                continue;
            };
            let (dx, dy) = (to.x() - from.x(), to.y() - from.y());
            total += sqrt(dx * dx + 0.0 * 0.0 + dy * dy);
        }
        Distance::new(total)
    }
}

/// What the pather knows of its target object: its place, radii and whether it moves (its top
/// speed, slot 0x208, is not 0: [`crate::map::spatial::Placed::moves`]; its id is the search's,
/// -1 for the goal query's target).
#[derive(Clone, Copy, Debug)]
struct Target {
    at: Point,
    radius: Footprint,
    moves: bool,
}

/// One search's query: the start and the goal, the target on the goal (its id -1 for none and for
/// the goal query's), and the range.
#[derive(Clone, Copy, Debug)]
struct HullQuery {
    from: Point,
    to: Point,
    target: Option<Target>,
    target_id: Option<ObjectId>,
    range: Distance,
}

/// A walk out of a hull or into one (`pathWithinHull`): its result and whether a ray left the
/// hull's boxes (`*param_18`).
struct Within {
    result: WalkResult,
    left_boxes: bool,
}

/// How a walk within a hull ended: `pathWithinHull`'s result, each variant's number given.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum WalkResult {
    /// A node saw the goal, or was within range of it or of the target (0).
    Reached,
    /// No node did; the path goes to the one nearest the goal (1).
    Nearest,
    /// No way found (2).
    NoWay,
    /// No first step made (3).
    Stuck,
    /// Ways kept, the walk's start (out) or goal (in) outside the hull's boxes (4).
    Ways,
    /// Ways kept, that point inside a box (5).
    WaysFromBox,
}

/// What the obstruction manager's one old pather (`+0x380`) carries from a search to the next:
/// its area and its hulls, which a kept search (`FindShortPath`'s argument 11) reuses. The
/// default is the constructor's (`FUN_140f84e40`): the area -1, no hulls.
#[derive(Clone, Debug, PartialEq)]
pub struct KeptHulls {
    area: TileRect,
    hulls: Vec<Hull>,
}

/// The constructor's area (`FUN_140f84e40`, `+0xe8`..`+0xf4` all −1): none yet, so a kept search
/// makes its own (its x0 tells, [`HullPather::find_keeping`]).
const NO_AREA: TileRect = TileRect::new(-1, -1, -1, -1);

impl Default for KeptHulls {
    fn default() -> Self {
        Self {
            area: NO_AREA,
            hulls: Vec::new(),
        }
    }
}

/// The old short pather over a world, for one unit: its terrain mask, player and radius, and the
/// ignore list.
#[derive(Debug)]
pub struct HullPather<'world> {
    world: &'world mut Spatial,
    /// The world's objects, whose gaits the hulls and the line tests read.
    by_id: ObjectsRef<'world>,
    mask: TerrainMask,
    radius: Distance,
    /// `FindShortPath`'s player (its argument 12, the unit's player's number `+0x718`), whom slot
    /// 0x678 asks about a gate; none (-1) for a unit the world does not hold.
    player: Option<PlayerId>,
    /// The gates its player may pass (slot 0x678, `FUN_1411d4a30`, a gate's), by id, when the
    /// caller gives them ([`HullPather::with_gates`]): the world's update knows the gates'
    /// states and the players' diplomacy; the pather does not.
    gates: Option<&'world BTreeSet<ObjectId>>,
    /// The objects `FindShortPath` includes (its arguments 13 and 14): in the hulls even when
    /// ignored or moving.
    include: Vec<ObjectId>,
    /// The manager's buffer for the line tests (`+0x57`, `+0x58`): half the radius.
    buffer: Option<Distance>,
    /// The ignore list (`+0xc0` on, `+0xc4` count, `+0xd0` ids), which the manager's tests read.
    ignore: Vec<RecordId>,
    hulls: Vec<Hull>,
    /// The nodes; 0 and 1 head the open lists (`+0x20`, `+0x58`).
    nodes: Nodes,
    /// The closed lists (`+0x90`, `+0xa0`).
    closed: ClosedLists,
    /// The area in tiles (`+0xe8`).
    area: TileRect,
    start: Point,
    goal: Point,
    exits: Vec<Waypoints>,
    entrances: Vec<Waypoints>,
    /// The map less the radius (`+0x118`).
    bounds: Rect,
}

#[cfg(test)]
mod tests;
