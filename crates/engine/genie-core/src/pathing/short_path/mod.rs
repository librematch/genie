//! The short pather (`FUN_140f9e540` -> `FUN_140fa53d0`): a fixed-point any-angle search around the
//! shapes of the obstruction manager's [`Mesh`], for the path a unit walks next.
//!
//! All coordinates are fixed point, 2^22 units per tile ([`ONE`]). A unit of radius `r` tests every
//! shape shrunk by `d = 0x200000 - r` (shapes are stored grown by half a tile). The map of the
//! search, its node types and its ties: `docs/AoE2DE_s_exe_SHORT_PATHER.md`.

use crate::GenieError;
use crate::math::fmath;
use crate::pathing::FixedDistance;
use crate::pathing::FixedRect;
use crate::pathing::mesh::{Mesh, ONE};
use crate::pathing::{FixedPoint, ShapeId, ShapeSide};
use crate::world::slots::CheckedIndex;

mod axis_dir;
mod call;
mod geometry;
mod main_loop;
mod nodes;
mod outline;
mod raycast;
mod start;
mod token;

pub use self::call::{Call, Entry, Listed, Pather, Query, ShortPath, Target, to_path};
#[cfg(test)]
use self::geometry::segment_hit;
use self::geometry::{Meet, RayFraction, contact_point, ray_rect, shrunk_corner, snap_to_side};
use self::nodes::{Corner, Foot, cheb, clamp_to_box, clip, corner_emits, in_box};
use self::start::Started;
use self::token::Token;

/// `DAT_142f43b10`, `DAT_142f43a10`: fixed point to float and back.
const TO_FLOAT: f32 = crate::math::image::short_path::TO_FLOAT;
pub(crate) const TO_FIXED: f32 = crate::math::image::short_path::TO_FIXED;

/// `(v + 0x3fffff) >> 22` for a negative `v`: the tile of a coordinate, truncated toward zero.
const fn tile_of(v: i32) -> i32 {
    if v < 0 {
        v.wrapping_add(0x3f_ffff) >> 22
    } else {
        v >> 22
    }
}

/// `FUN_14087c720`: the integer square root of a 64-bit value, bit by bit (its low 32 bits).
#[must_use]
pub const fn isqrt(mut v: u64) -> u32 {
    let mut root: u64 = 0;
    let mut bit: u64 = 1 << 62;
    if v <= 0x3fff_ffff_ffff_ffff {
        loop {
            bit >>= 2;
            if v >= bit {
                break;
            }
        }
    }
    while bit != 0 {
        if bit + root <= v {
            v -= bit + root;
            root += bit * 2;
        }
        root >>= 1;
        bit >>= 2;
    }
    root as u32
}

/// `(int)((float)v * 2^-22 * 2^22)`: a cost through float, 24 bits kept.
fn through_float(v: i32) -> FixedDistance {
    FixedDistance::new(((v as f32) * TO_FLOAT * TO_FIXED) as i32)
}

/// The Euclidean distance in fixed point through float (`sqrtf` of the float differences).
fn float_distance(a: FixedPoint, b: FixedPoint) -> FixedDistance {
    let dy = (a.y().wrapping_sub(b.y())) as f32 * TO_FLOAT;
    let dx = (a.x().wrapping_sub(b.x())) as f32 * TO_FLOAT;
    FixedDistance::new((fmath::sqrt(dx * dx + dy * dy) * TO_FIXED) as i32)
}

/// `FUN_14087c610`: `p` strictly inside `r` shrunk by `k`.
const fn strictly_inside(r: FixedRect, p: FixedPoint, k: FixedDistance) -> bool {
    let [x, y] = p.get();
    let k = k.get();
    r.x0() + k < x && r.y0() + k < y && x < r.x1() - k && y < r.y1() - k
}

/// `FUN_14087c650`: `p` inside `r` shrunk by `k`, edges included.
const fn inside(r: FixedRect, p: FixedPoint, k: FixedDistance) -> bool {
    let [x, y] = p.get();
    let k = k.get();
    r.x0() + k <= x && r.y0() + k <= y && x <= r.x1() - k && y <= r.y1() - k
}

/// A hit of the raycast or a sweep: the shape and its side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    /// The shape hit.
    pub shape: ShapeId,
    /// Its side.
    pub side: ShapeSide,
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
    const fn counted(&self) -> Result<Option<(ShapeId, ShapeSide, FixedPoint)>, GenieError> {
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
pub struct Ctx<'a> {
    /// The mesh.
    pub mesh: &'a Mesh,
    /// Per shape id: ignored (`pather + 0xd8`).
    pub ignore: &'a [bool],
    /// How far the searches shrink a shape's box (`FUN_140fa8460`, `FUN_14087c650`).
    pub d: FixedDistance,
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
    r: FixedDistance,
    rx: i32,
    ry: i32,
    rrx: i64,
    rry: i64,
    range: FixedDistance,
    range2: i64,
}

impl Goal {
    /// The squared distance from `p` to the target's box grown by the radius (`>> 22` each).
    fn rect_distance2(&self, p: FixedPoint) -> i64 {
        let lx = i64::from((p.x().wrapping_sub(self.at.x())).wrapping_abs()) - self.rrx;
        let ly = i64::from((p.y().wrapping_sub(self.at.y())).wrapping_abs()) - self.rry;
        let a = if lx < 0 { 0 } else { lx * lx };
        let b = if ly < 0 { 0 } else { ly * ly };
        (b >> 22) + (a >> 22)
    }

    /// `FUN_140fa3f10`: the estimate from `p` to the goal.
    fn heuristic(&self, p: FixedPoint) -> FixedDistance {
        if !RECT_HEURISTIC {
            let fy = (self.at.y().wrapping_sub(p.y())) as f32 * TO_FLOAT;
            let fx = (self.at.x().wrapping_sub(p.x())) as f32 * TO_FLOAT;
            return FixedDistance::new((fmath::sqrt(fx * fx + fy * fy) * TO_FIXED) as i32);
        }
        let lx = i64::from((p.x().wrapping_sub(self.at.x())).wrapping_abs()) - self.rrx;
        let ly = i64::from((p.y().wrapping_sub(self.at.y())).wrapping_abs()) - self.rry;
        let a = if lx < 0 { 0 } else { (lx * lx) as u64 };
        let b = if ly < 0 { 0 } else { (ly * ly) as u64 };
        FixedDistance::new(isqrt((b & !0x3f_ffff).wrapping_add(a & !0x3f_ffff)) as i32)
    }
}

/// A node's payload (`+0x18..+0x30`, a `std::variant`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PolyIndex(u32);

impl PolyIndex {
    /// The polygon at slice index `i`, as the game's 32 bits.
    const fn of(i: usize) -> Self {
        Self(i as u32)
    }

    /// The place as a slice index.
    const fn get(self) -> usize {
        self.0 as usize
    }
}

/// The run of a polygon's vertices a corner node or a goal candidate was walked over: the
/// polygon, the walk's direction round it and the run's first and last vertex, which the path
/// back reads (a node's `+0x1c..+0x2c`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Run {
    poly: PolyIndex,
    dir: WalkDir,
    lo: VertexIndex,
    hi: VertexIndex,
}

/// A vertex's place in its polygon's columns (the vertices, their sides, shapes and costs):
/// a run's ends, a held edge's start, a walk's corner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct VertexIndex(u32);

impl VertexIndex {
    /// The polygon's first vertex.
    const FIRST: Self = Self(0);

    /// Vertex `i`.
    const fn new(i: u32) -> Self {
        Self(i)
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WalkDir {
    /// 0: forward, to the higher vertices.
    Forward,
    /// 1: backward, to the lower vertices.
    Backward,
}

/// What an open corner node emits its ray with (`FUN_140fa3bc0`): the token the ray ignores
/// and the sides around the corner, before and after it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Emit {
    token: Token,
    prev: ShapeSide,
    next: ShapeSide,
}

/// A node's place in the search's arena (a node's `+0x0c`, its parent's at `+0x10`; the game's
/// −1 for none, a root's parent, is `None`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NodeIndex(u32);

impl NodeIndex {
    /// The start node, the arena's first.
    const START: Self = Self(0);

    /// Node `i`.
    const fn new(i: u32) -> Self {
        Self(i)
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
    g: FixedDistance,
    f: FixedDistance,
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

    /// Vertex `i`'s point.
    fn vertex(&self, i: VertexIndex) -> Result<FixedPoint, GenieError> {
        self.vertices.checked(i.get()).copied()
    }

    /// The side from vertex `i` to the next.
    fn side(&self, i: VertexIndex) -> Result<ShapeSide, GenieError> {
        self.sides.checked(i.get()).copied()
    }

    /// Vertex `i`'s shape, none on the bounds' border.
    fn shape(&self, i: VertexIndex) -> Result<Option<ShapeId>, GenieError> {
        self.shapes.checked(i.get()).copied()
    }

    /// The best cost a corner node reached vertex `i` with, [`UNSET`] for none yet.
    fn cost(&self, i: VertexIndex) -> Result<FixedDistance, GenieError> {
        self.costs.checked(i.get()).copied()
    }

    /// The best cost at vertex `i`, to change.
    fn cost_mut(&mut self, i: VertexIndex) -> Result<&mut FixedDistance, GenieError> {
        self.costs.checked_mut(i.get())
    }
}

/// One search's state (the pather's arrays at `+0x08`..`+0x80`).
struct Search<'a> {
    ctx: Ctx<'a>,
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
