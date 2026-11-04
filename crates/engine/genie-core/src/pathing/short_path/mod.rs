//! The short pather (`FUN_140f9e540` -> `FUN_140fa53d0`): a fixed-point any-angle search around the
//! shapes of the obstruction manager's [`Mesh`], for the path a unit walks next.
//!
//! All coordinates are fixed point, 2^22 units per tile ([`ONE`]). A unit of radius `radius` tests
//! every shape shrunk by `clearance = 0x200000 - radius` (shapes are stored grown by half a tile).
//! The map of the search, its node types and its ties: `docs/AoE2DE_s_exe_SHORT_PATHER.md`.

use crate::math::fmath::sqrt;
use crate::pathing::FixedRect;
use crate::pathing::mesh::{Mesh, ONE};
use crate::pathing::{FixedDistance, FixedDistanceSquared};
use crate::pathing::{FixedPoint, ShapeId, ShapeSide};
use crate::world::slots::CheckedIndexExt;
use crate::{GenieError, GenieResult};

mod axis_dir;
mod call;
mod geometry;
mod main_loop;
mod nodes;
mod outline;
mod raycast;
mod start;
mod token;
mod walk;

use self::call::Fixed;
pub use self::call::{Call, Entry, Listed, Pather, Query, ShortPath, Target, to_path};
#[cfg(test)]
use self::geometry::segment_hit;
use self::geometry::{Meet, RayFraction, contact_point, ray_rect, shrunk_corner, snap_to_side};
use self::nodes::{Corner, Foot, cheb, clamp_to_box, clip, corner_emits, in_box};
use self::start::Started;
use self::token::Token;
use crate::math::image::short_path::{
    TO_FIXED as SHORT_PATH_TO_FIXED, TO_FLOAT as SHORT_PATH_TO_FLOAT,
};

/// `DAT_142f43b10`, `DAT_142f43a10`: fixed point to float and back.
const TO_FLOAT: f32 = SHORT_PATH_TO_FLOAT;
pub(crate) const TO_FIXED: f32 = SHORT_PATH_TO_FIXED;

/// `(v + 0x3fffff) >> 22` for a negative `value`: the tile of a coordinate, truncated toward zero.
const fn tile_of(value: i32) -> i32 {
    if value < 0 {
        value.wrapping_add(0x3f_ffff) >> 22
    } else {
        value >> 22
    }
}

/// `FUN_14087c720`: the integer square root of a 64-bit value, bit by bit (its low 32 bits).
#[must_use]
pub const fn isqrt(mut value: u64) -> u32 {
    let mut root: u64 = 0;
    let mut bit: u64 = 1 << 62;
    if value <= 0x3fff_ffff_ffff_ffff {
        loop {
            bit >>= 2;
            if value >= bit {
                break;
            }
        }
    }
    while bit != 0 {
        if bit + root <= value {
            value -= bit + root;
            root += bit * 2;
        }
        root >>= 1;
        bit >>= 2;
    }
    root as u32
}

/// `(int)((float)value * 2^-22 * 2^22)`: a cost through float, 24 bits kept.
fn through_float(value: i32) -> FixedDistance {
    FixedDistance::new(((value as f32) * TO_FLOAT * TO_FIXED) as i32)
}

/// The Euclidean distance in fixed point through float (`sqrtf` of the float differences).
fn float_distance(from: FixedPoint, to: FixedPoint) -> FixedDistance {
    let dy = (from.y().wrapping_sub(to.y())) as f32 * TO_FLOAT;
    let dx = (from.x().wrapping_sub(to.x())) as f32 * TO_FLOAT;
    FixedDistance::new((sqrt(dx * dx + dy * dy) * TO_FIXED) as i32)
}

/// `FUN_14087c610`: `fixed_point` strictly inside `fixed_rect` shrunk by `fixed_distance`.
const fn strictly_inside(
    fixed_rect: FixedRect,
    fixed_point: FixedPoint,
    fixed_distance: FixedDistance,
) -> bool {
    let [x, y] = fixed_point.get();
    let fixed_distance = fixed_distance.get();
    fixed_rect.x0() + fixed_distance < x
        && fixed_rect.y0() + fixed_distance < y
        && x < fixed_rect.x1() - fixed_distance
        && y < fixed_rect.y1() - fixed_distance
}

/// `FUN_14087c650`: `fixed_point` inside `fixed_rect` shrunk by `fixed_distance`, edges included.
const fn inside(
    fixed_rect: FixedRect,
    fixed_point: FixedPoint,
    fixed_distance: FixedDistance,
) -> bool {
    let [x, y] = fixed_point.get();
    let fixed_distance = fixed_distance.get();
    fixed_rect.x0() + fixed_distance <= x
        && fixed_rect.y0() + fixed_distance <= y
        && x <= fixed_rect.x1() - fixed_distance
        && y <= fixed_rect.y1() - fixed_distance
}

/// A hit of the raycast or a sweep: the shape and its side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    /// The shape hit.
    pub(crate) shape: ShapeId,
    /// Its side.
    pub(crate) side: ShapeSide,
}

/// A sweep that counts a shape without the point and side its counting hit writes first.
const COUNT_WITHOUT_HIT: GenieError =
    GenieError::unreachable("a counting hit that wrote no point and side before it counted");

/// An axis sweep's outputs as the game leaves them: whether a shape counted, the shape, and the
/// point and side of the last hit tested (written even for a shape the sweep then let pass).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Sweep {
    found: bool,
    shape: Option<ShapeId>,
    point: Option<FixedPoint>,
    side: Option<ShapeSide>,
}

impl Sweep {
    /// The counted shape with the last hit's side and point: a sweep that counts a shape has
    /// written a point and a side by then (the hit that counts writes them first).
    const fn counted(&self) -> GenieResult<Option<(ShapeId, ShapeSide, FixedPoint)>> {
        if !self.found {
            return Ok(None);
        }
        let (Some(shape), Some(side), Some(at)) = (self.shape, self.side, self.point) else {
            return Err(COUNT_WITHOUT_HIT);
        };
        Ok(Some((shape, side, at)))
    }
}

/// The raycast's callbacks (`FUN_140fa1f60`, `FUN_140fa2180`, `FUN_140fa2850`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Callback {
    /// A: a touch at the corner the ray starts from on the token's side (or of the token's shape)
    /// is ignored; the token becomes the shape's of each touch ignored. The game keeps the token
    /// in a global (`DAT_143f0f360`), but sets it before each raycast that reads it, so no value
    /// carries over from one ray to the next.
    Corner(Token),
    /// B: the shapes whose box meets the ray's box, a flat box grown by 1.
    Box,
    /// C: as B, without the growth (the smoothing's line of sight).
    Sight,
}

/// What the searches share for a unit: the mesh, the shapes it ignores, its shrinking.
#[derive(Debug)]
pub struct Ctx<'mesh> {
    /// The mesh.
    pub(crate) mesh: &'mesh Mesh,
    /// Per shape id: ignored (`pather + 0xd8`).
    pub(crate) ignore: &'mesh [bool],
    /// How far the searches shrink a shape's box (`FUN_140fa8460`, `FUN_14087c650`).
    pub(crate) clearance: FixedDistance,
}

/// `DAT_1439e60e5`: 1 in the image and at run time: the rectangle heuristic (0 is the Euclidean).
const RECT_HEURISTIC: bool = true;
/// `DAT_143cc1f50`: a polygon vertex no corner node reached yet.
const UNSET: FixedDistance = FixedDistance::new(-0x40_0000);
/// `DAT_143cc1f54`: the outline tracer's step limit.
const TRACE_STEPS: i32 = 2000;
/// `DAT_143cc1f60`: the four unit directions (x, y) of a type-3 node's nudges.
const NUDGES: [FixedPoint; 4] = [
    FixedPoint::new(0, ONE),
    FixedPoint::new(ONE, 0),
    FixedPoint::new(0, -ONE),
    FixedPoint::new(-ONE, 0),
];
/// An open corner node without its emission: only the goal candidates, which never open, lack
/// one.
const OPEN_CORNER_WITHOUT_EMIT: GenieError =
    GenieError::unreachable("an open corner node without the token and sides it emits with");

/// The goal of a search (`local_148`): its point, the unit's radius, the target's half sizes,
/// their sums and the range.
#[derive(Clone, Copy, Debug)]
struct Goal {
    at: FixedPoint,
    radius: FixedDistance,
    rx: FixedDistance,
    ry: FixedDistance,
    rrx: FixedDistance,
    rry: FixedDistance,
    range: FixedDistance,
    range2: FixedDistanceSquared,
}

impl Goal {
    /// The goal at `at` of the query `fixed`: the target's half sizes, their sums with the radius
    /// (32-bit adds) and the range, squared in 64 bits.
    fn toward(at: FixedPoint, fixed: &Fixed) -> Self {
        let fixed_distance = fixed.radius;
        Self {
            at,
            radius: fixed_distance,
            rx: fixed.half_x,
            ry: fixed.half_y,
            rrx: fixed_distance.wrapping_add(fixed.half_x),
            rry: fixed_distance.wrapping_add(fixed.half_y),
            range: fixed.range,
            range2: FixedDistanceSquared::of(fixed.range),
        }
    }

    /// The squared distance from `fixed_point` to the target's box grown by the radius (`>> 22`
    /// each).
    fn rect_distance2(&self, fixed_point: FixedPoint) -> FixedDistanceSquared {
        let lx = i64::from((fixed_point.x().wrapping_sub(self.at.x())).wrapping_abs())
            - i64::from(self.rrx.get());
        let ly = i64::from((fixed_point.y().wrapping_sub(self.at.y())).wrapping_abs())
            - i64::from(self.rry.get());
        let lx_squared = if lx < 0 { 0 } else { lx * lx };
        let ly_squared = if ly < 0 { 0 } else { ly * ly };
        FixedDistanceSquared::new((ly_squared >> 22) + (lx_squared >> 22))
    }

    /// `FUN_140fa3f10`: the estimate from `fixed_point` to the goal.
    fn heuristic(&self, fixed_point: FixedPoint) -> FixedDistance {
        if !RECT_HEURISTIC {
            let fy = (self.at.y().wrapping_sub(fixed_point.y())) as f32 * TO_FLOAT;
            let fx = (self.at.x().wrapping_sub(fixed_point.x())) as f32 * TO_FLOAT;
            return FixedDistance::new((sqrt(fx * fx + fy * fy) * TO_FIXED) as i32);
        }
        let lx = i64::from((fixed_point.x().wrapping_sub(self.at.x())).wrapping_abs())
            - i64::from(self.rrx.get());
        let ly = i64::from((fixed_point.y().wrapping_sub(self.at.y())).wrapping_abs())
            - i64::from(self.rry.get());
        let lx_squared = if lx < 0 { 0 } else { (lx * lx) as u64 };
        let ly_squared = if ly < 0 { 0 } else { (ly * ly) as u64 };
        FixedDistance::new(
            isqrt((ly_squared & !0x3f_ffff).wrapping_add(lx_squared & !0x3f_ffff)) as i32,
        )
    }
}

/// A node's payload (`+0x18..+0x30`, a `std::variant`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Kind {
    /// 0: the start. Its shape field is always none: `FUN_140fa4410`, the only maker of a type-0
    /// node, writes -1 there (`local_178`), so the expansion's branch for a type-0 node on a
    /// shape (a side node, `FUN_140fa3fe0`) never runs.
    Start,
    /// 1: a waypoint.
    Waypoint,
    /// 2: a point on a shape's side.
    Side { shape: ShapeId, side: ShapeSide },
    /// 3: a polygon corner or a terminal node: the run of the polygon it was walked over (none
    /// for the terminal goal candidate of a clear ray, the game's direction 0 with vertices and
    /// polygon −1) and what the corner emits its ray with (none for a goal candidate, which
    /// never opens: the game's token 5 or −1 and sides 0 or 0xff).
    Corner {
        run: Option<Run>,
        emit: Option<Emit>,
    },
}

/// A polygon's place in the search's list (a corner node's `+0x28`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct PolyIndex(u32);

impl PolyIndex {
    /// The polygon at slice index `index`, as the game's 32 bits.
    const fn of(index: usize) -> Self {
        Self(index as u32)
    }

    /// The place as a slice index.
    const fn get(self) -> usize {
        self.0 as usize
    }
}

/// The run of a polygon's vertices a corner node or a goal candidate was walked over: the
/// polygon, the walk's direction round it and the run's first and last vertex, which the path
/// back reads (a node's `+0x1c..+0x2c`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Run {
    poly: PolyIndex,
    dir: WalkDir,
    lo: VertexIndex,
    hi: VertexIndex,
}

/// A vertex's place in its polygon's columns (the vertices, their sides, shapes and costs):
/// a run's ends, a held edge's start, a walk's corner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct VertexIndex(u32);

impl VertexIndex {
    /// The polygon's first vertex.
    const FIRST: Self = Self(0);

    /// Vertex `index`.
    const fn new(index: u32) -> Self {
        Self(index)
    }

    /// The place as a slice index.
    const fn get(self) -> usize {
        self.0 as usize
    }

    /// The vertex after it round a polygon of `count` vertices: `(i + 1) % count`.
    const fn next_in(self, count: u32) -> Self {
        Self((self.0 + 1) % count)
    }

    /// The vertex before it round a polygon of `count` vertices: `(i + count - 1) % count`.
    const fn prev_in(self, count: u32) -> Self {
        Self((self.0 + count - 1) % count)
    }

    /// The next place along the polygon's list, past its end too (a walk's forward end).
    const fn succ(self) -> Self {
        Self(self.0 + 1)
    }

    /// The place before it along the polygon's list (a walk's backward end, never the first).
    const fn pred(self) -> Self {
        Self(self.0 - 1)
    }
}

/// The direction of a walk round a polygon's vertices (a corner node's, a goal candidate's):
/// forward (0) or backward (1), the way the path back reads the vertices it covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum WalkDir {
    /// 0: forward, to the higher vertices.
    Forward,
    /// 1: backward, to the lower vertices.
    Backward,
}

/// What an open corner node emits its ray with (`FUN_140fa3bc0`): the token the ray ignores
/// and the sides around the corner, before and after it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Emit {
    token: Token,
    prev: ShapeSide,
    next: ShapeSide,
}

/// A node's place in the search's arena (a node's `+0x0c`, its parent's at `+0x10`; the game's
/// −1 for none, a root's parent, is `None`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct NodeIndex(u32);

impl NodeIndex {
    /// The start node, the arena's first.
    const START: Self = Self(0);

    /// Node `index`.
    const fn new(index: u32) -> Self {
        Self(index)
    }

    /// The place the next node appended to `arena` gets (its length, as the game's 32 bits).
    const fn next_of(arena: &[Node]) -> Self {
        Self::new(arena.len() as u32)
    }

    /// The place as a slice index.
    const fn get(self) -> usize {
        self.0 as usize
    }
}

/// A node (`0x34` bytes).
#[derive(Clone, Copy, Debug)]
struct Node {
    cost: FixedDistance,
    total: FixedDistance,
    idx: NodeIndex,
    parent: Option<NodeIndex>,
    at: FixedPoint,
    kind: Kind,
}

/// A polygon (`0x68` bytes): the outline of a cluster of touching shapes, held in the bounds.
#[derive(Clone, Debug, Default)]
struct Poly {
    vertices: Vec<FixedPoint>,
    sides: Vec<ShapeSide>,
    /// Each vertex's shape, none for a vertex on the bounds' border.
    shapes: Vec<Option<ShapeId>>,
    costs: Vec<FixedDistance>,
    flag: bool,
}

impl Poly {
    /// The vertex count, as the game's 32 bits.
    const fn count(&self) -> u32 {
        self.vertices.len() as u32
    }

    /// Vertex `vertex_index`'s point.
    fn vertex(&self, vertex_index: VertexIndex) -> GenieResult<FixedPoint> {
        self.vertices.checked(vertex_index.get()).copied()
    }

    /// The side from vertex `vertex_index` to the next.
    fn side(&self, vertex_index: VertexIndex) -> GenieResult<ShapeSide> {
        self.sides.checked(vertex_index.get()).copied()
    }

    /// Vertex `vertex_index`'s shape, none on the bounds' border.
    fn shape(&self, vertex_index: VertexIndex) -> GenieResult<Option<ShapeId>> {
        self.shapes.checked(vertex_index.get()).copied()
    }

    /// The best cost a corner node reached vertex `vertex_index` with, [`UNSET`] for none yet.
    fn cost(&self, vertex_index: VertexIndex) -> GenieResult<FixedDistance> {
        self.costs.checked(vertex_index.get()).copied()
    }

    /// The best cost at vertex `vertex_index`, to change.
    fn cost_mut(&mut self, vertex_index: VertexIndex) -> GenieResult<&mut FixedDistance> {
        self.costs.checked_mut(vertex_index.get())
    }
}

/// One search's state (the pather's arrays at `+0x08`..`+0x80`).
struct Search<'mesh> {
    ctx: Ctx<'mesh>,
    goal: Goal,
    bounds: FixedRect,
    open: Vec<Node>,
    arena: Vec<Node>,
    polys: Vec<Poly>,
    /// `local_1a0`, `local_194`: the goal node so far (none, −1, until one is in range) and the
    /// node of the lowest estimate (the start first).
    goal_node: Option<NodeIndex>,
    best: NodeIndex,
}

#[cfg(test)]
mod tests;
