//! The old short pather's queries, the map's edge records and the segment tests.

use crate::map::Distance;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::TerrainMask;
use crate::map::Tile;
use crate::map::spatial::{QUERY_HITS, RecordId, Spatial};
use crate::map::{Rect, Segment};
use crate::math::fmath::below_or_unordered;
use crate::objects::masters::ObstructionKind;
use crate::pathing::tile_path::SegmentTest;
use crate::world::index::ObjectsRef;
use crate::world::layer::ClassRoles;
use crate::{GenieError, GenieResult};

/// A record a pather query met (an entry of `FUN_141051900`'s list): an object's, a temporary
/// tile record's or a map edge's ([`RecordId`]), with its box (`+0x20`..`+0x2c`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    /// The record's id (`+0x18`).
    pub(crate) id: RecordId,
    /// Its box.
    pub(crate) bounds: Rect,
}

/// The id of `tile`'s temporary record (`FUN_1410510c0`) and of the tile in a pather's ignore
/// list (`FUN_140f89e40`).
#[must_use]
pub const fn tile_record_id(tile: Tile) -> RecordId {
    RecordId::Tile(tile)
}

/// Where the old short pather's query stops: at its first hit (`FUN_141052350`) or at the 25th
/// (`FUN_141051900`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum HitLimit {
    /// The first hit.
    First,
    /// The 25th.
    Most,
}

impl Spatial {
    /// The old short pather's line queries (flags 0x40: moving objects left out, every kind
    /// tested), after `FUN_141052b90` put the temporary records (kind 3, the tile's box) of the
    /// walk's box, grown by `buffer`, that the unit's `mask` lacks ([`Spatial::put_tile_records`];
    /// they stay with those of the pather's earlier walks until `FindShortPath` removes them,
    /// [`Spatial::clear_tile_records`]): the records whose box, grown by `buffer`, meets the walk
    /// `a` -> `b` (`FUN_1410512f0`, a circle's `FUN_141051630` alike), whose cell the query visits
    /// ([`Spatial::visits`]) and whose id is not in `ignore` (the manager's list `+0x2c`, the
    /// pather's). An object that moves (its record's `+3` bit 0x10, set from slot 0x5c8 at the end
    /// of its update, [`Gait::moving`]) is left out, as is a gate the player may pass (slot 0x678,
    /// not modelled). [`HitLimit::First`] stops at the first hit (`FUN_141052350`); else at the
    /// 25th (`FUN_141051900`). The hits are in the tiles' row order, then by id; the game's are in
    /// its grid's order, which its callers do not read. A walk whose grown box leaves the map is
    /// blocked (`FUN_141052350`) or meets the map's edge records first ([`Spatial::edge_records`]).
    ///
    /// [`Gait::moving`]: crate::map::spatial::Gait::moving
    pub(super) fn pather_query(
        &mut self,
        a: Point,
        b: Point,
        buffer: Option<Distance>,
        mask: TerrainMask,
        (ignore, by_id): (&[RecordId], ObjectsRef<'_>),
        limit: HitLimit,
    ) -> GenieResult<Vec<Hit>> {
        const MOST_HITS: usize = 25;
        let roles = by_id.roles();
        let lo_x = if b.x() <= a.x() { b.x() } else { a.x() };
        let hi_x = if a.x() <= b.x() { b.x() } else { a.x() };
        let lo_y = if b.y() <= a.y() { b.y() } else { a.y() };
        let hi_y = if a.y() <= b.y() { b.y() } else { a.y() };
        let distance = buffer.unwrap_or(Distance::ZERO);
        self.put_tile_records(a, b, distance, mask);
        let walk_box = Rect::new(lo_x, lo_y, hi_x, hi_y);
        let grown = walk_box.grown(distance);
        let mut hits = Vec::new();
        if grown.leaves_map(self.map) {
            hits = self.edge_records(grown, QUERY_HITS);
            if limit == HitLimit::First {
                return Ok(hits);
            }
        }
        self.check_known_records(grown)?;
        let walk = Segment { from: a, to: b };
        let meets = |kind: ObstructionKind, bx: Rect| {
            walk_box.overlaps(bx.grown(distance)) && record_meets_segment(kind, bx, distance, walk)
        };
        let level = self.grid.level(0);
        let shift = self.grid.shift(level);
        for &(y, x) in &self.tile_records {
            let id = tile_record_id(Tile::new(x, y));
            let bx = Rect::new(x as f32, y as f32, x as f32 + 1.0, y as f32 + 1.0);
            if self.visits_cell(level, x >> shift, y >> shift, grown)
                && !ignore.contains(&id)
                && meets(ObstructionKind::KIND_3, bx)
            {
                hits.push(Hit { id, bounds: bx });
                if limit == HitLimit::First || MOST_HITS <= hits.len() {
                    return Ok(hits);
                }
            }
        }
        for (id, placed) in self.records_in(grown) {
            let Some(obstruction) = placed.obstruction else {
                continue;
            };
            let kind = obstruction.kind.filed();
            if !kind.tested()
                || by_id.moving(id)
                || ignore.contains(&RecordId::from(id))
                || !self.visits(placed, kind, grown)
            {
                continue;
            }
            let bx = placed.bounds();
            if !meets(kind, bx) {
                continue;
            }
            if kind == ObstructionKind::KIND_11 {
                return Err(GenieError::unsupported(
                    "a pather query meeting a record of kind 11 (its own radii)",
                ));
            }
            // The class of a gate, which a player's allies may pass (slot 0x678, `FUN_1411d4a30`).
            if roles.of_class(placed.class).contains(ClassRoles::GATE) {
                return Err(GenieError::unsupported(
                    "a pather query meeting a gate (its players' passage, slot 0x678)",
                ));
            }
            hits.push(Hit {
                id: RecordId::from(id),
                bounds: bx,
            });
            if limit == HitLimit::First || MOST_HITS <= hits.len() {
                break;
            }
        }
        Ok(hits)
    }

    /// Whether the old short pather's walk `a` -> `b` meets a record ([`Spatial::pather_query`]:
    /// `FUN_141052b90`, then `FUN_141052350` with flags 0x40).
    pub fn pather_blocked(
        &mut self,
        a: Point,
        b: Point,
        buffer: Option<Distance>,
        mask: TerrainMask,
        ignore: &[RecordId],
        by_id: ObjectsRef<'_>,
    ) -> GenieResult<bool> {
        Ok(!self
            .pather_query(a, b, buffer, mask, (ignore, by_id), HitLimit::First)?
            .is_empty())
    }

    /// The records the old short pather's walk `a` -> `b` meets, at most 25
    /// ([`Spatial::pather_query`]: `FUN_141052b90`, then `FUN_141051900` with flags 0x40).
    pub fn pather_hits(
        &mut self,
        a: Point,
        b: Point,
        buffer: Option<Distance>,
        mask: TerrainMask,
        ignore: &[RecordId],
        by_id: ObjectsRef<'_>,
    ) -> GenieResult<Vec<Hit>> {
        self.pather_query(a, b, buffer, mask, (ignore, by_id), HitLimit::Most)
    }
}

impl Spatial {
    /// The map edge's records a query rectangle `rect` (`x0, y0, x1, y1`) that leaves the map meets
    /// (`FUN_141057290`, which `FUN_141051900` lists first, before any test of kind, flags or
    /// ignore list): for each edge it crosses (left, top, right, bottom), the manager's record for
    /// that edge (`+0xb0`, `+0xb8`, `+0xc0`, `+0xc8`), set to the tile box where the rectangle's
    /// diagonal `(x0, y0)` -> `(x1, y1)` crosses the edge's line, id `x << 12 | y | 0xff000000`.
    /// The list takes at most `cap` records (its `*param_6`): the routine returns once the list
    /// holds that many, after any edge. The terms are the game's, each with its zero factor
    /// (`0 × dy0`, `(w − w) × dy`, ...), so an infinite or NaN operand gives NaN as there; each
    /// crossing is cast by `CVTTSS2SI` into 32 bits (a NaN or huge one is `i32::MIN`).
    #[must_use]
    // the game's (w - w) and (h - h) factors: 0, but NaN times an infinite or NaN term
    #[allow(clippy::eq_op)]
    pub fn edge_records(&self, rect: Rect, cap: usize) -> Vec<Hit> {
        use crate::math::conversions::cvttss2si;
        let (x0, y0, x1, y1) = (rect.x0(), rect.y0(), rect.x1(), rect.y1());
        // the manager's map size as floats (+0xa8, +0xac)
        let (w, h) = (self.map.width() as f32, self.map.height() as f32);
        let (dx, dy) = (x1 - x0, y1 - y0);
        let (dy0, dx0) = (y0 - 0.0, x0 - 0.0);
        let at = |t: f32| {
            let x = cvttss2si(dx * t + x0);
            let y = cvttss2si(dy * t + y0);
            Hit {
                id: RecordId::Edge(Tile::new(x, y)),
                bounds: Rect::new(x as f32, y as f32, x as f32 + 1.0, y as f32 + 1.0),
            }
        };
        let mut hits = Vec::new();
        let mut edges: [Option<f32>; 4] = [None; 4];
        if x0 < 0.0 {
            let hh = h - 0.0;
            edges[0] = Some((dy0 * 0.0 - hh * dx0) / (dx * hh - dy * 0.0));
        }
        if y0 < 0.0 {
            let hh = h - 0.0;
            edges[1] = Some((hh * dy0 - dx0 * 0.0) / (dx * 0.0 - hh * dy));
        }
        if w < x1 {
            let hh = h - 0.0;
            edges[2] = Some(((w - w) * dy0 - (x0 - w) * hh) / (hh * dx - (w - w) * dy));
        }
        if h < y1 {
            let ww = w - 0.0;
            edges[3] = Some(((y0 - h) * ww - (h - h) * dx0) / ((h - h) * dx - ww * dy));
        }
        for t in edges.into_iter().flatten() {
            hits.push(at(t));
            if cap <= hits.len() {
                break;
            }
        }
        hits
    }
}

/// `FUN_1410576a0`, Sedgewick's ccw: 1 when `p0`, `p1`, `p2` turn counter-clockwise (or lie in
/// a line with `p2` beyond `p1`), -1 clockwise (or `p0` between), 0 when `p2` lies on the segment.
pub(super) fn ccw(p0: Point, p1: Point, p2: Point) -> i32 {
    let (dx1, dy1) = (p1.x() - p0.x(), p1.y() - p0.y());
    let (dx2, dy2) = (p2.x() - p0.x(), p2.y() - p0.y());
    // every test is COMISS/JA (0x1410576ee..0x14105772e): an unordered one falls through, so a
    // NaN reaches the last answer, 0
    if dy2 * dx1 > dx2 * dy1 {
        return 1;
    }
    if dx2 * dy1 > dy2 * dx1 || 0.0 > dx2 * dx1 || 0.0 > dy2 * dy1 {
        return -1;
    }
    if dy2 * dy2 + dx2 * dx2 > dy1 * dy1 + dx1 * dx1 {
        return 1;
    }
    0
}

/// The smoothing's segment test for a unit (`FUN_140f7dbb0`, [`crate::pathing::tile_path::SegmentTest`]):
/// each walk puts its temporary tile records ([`Spatial::put_tile_records`]) and is tested
/// ([`Spatial::segment_meets`]) against them and those of the pass's earlier walks; the pass's
/// end removes them ([`Spatial::clear_tile_records`]).
#[derive(Debug)]
pub struct SmoothingWalls<'a> {
    pub(super) world: &'a mut Spatial,
    pub(super) radius: Distance,
    pub(super) mask: TerrainMask,
    pub(super) ignore: &'a [RecordId],
    /// Whether a walk was tested.
    pub(crate) ran: bool,
}

impl<'a> SmoothingWalls<'a> {
    /// The test of a unit `radius` wide (the manager's buffer) whose terrain mask is `mask`,
    /// with the manager's ignore list `ignore`.
    #[must_use]
    pub const fn new(
        world: &'a mut Spatial,
        radius: Distance,
        mask: TerrainMask,
        ignore: &'a [RecordId],
    ) -> Self {
        Self {
            world,
            radius,
            mask,
            ignore,
            ran: false,
        }
    }
}

impl<E> SegmentTest<E> for SmoothingWalls<'_> {
    fn blocked(&mut self, a: Point, b: Point) -> Result<bool, E> {
        self.ran = true;
        // the pass's buffer, the unit's radius (FUN_1410562d0 with 1)
        self.world.set_manager_buffer(Some(self.radius));
        self.world.put_tile_records(a, b, self.radius, self.mask);
        Ok(self.world.segment_meets(a, b, self.radius, self.ignore))
    }

    fn pass_ends(&mut self) {
        self.world.set_manager_buffer(None);
        self.world.clear_tile_records();
    }
}

/// A straight walk's test against records (`FUN_141052350`, `FUN_141052800`): the walk's box
/// (its ends' lower and upper x and y) and the query's box, grown by the manager's buffer.
pub(super) struct WalkTest {
    pub(super) walk: Segment,
    pub(super) lo: Point,
    pub(super) hi: Point,
    pub(super) grow: Distance,
    pub(super) query: Rect,
}

impl WalkTest {
    /// The walk from `a` to `b` of a unit `grow` wide; None when its grown box leaves a map of
    /// `map` tiles (the query then counts as met).
    pub(super) fn new(a: Point, b: Point, grow: Distance, map: GridSize) -> Option<Self> {
        let lo = Point::new(
            if b.x() <= a.x() { b.x() } else { a.x() },
            if b.y() <= a.y() { b.y() } else { a.y() },
        );
        let hi = Point::new(
            if a.x() <= b.x() { b.x() } else { a.x() },
            if a.y() <= b.y() { b.y() } else { a.y() },
        );
        let query = Rect::between(lo, hi).grown(grow);
        if query.leaves_map(map) {
            return None;
        }
        Some(Self {
            walk: Segment { from: a, to: b },
            lo,
            hi,
            grow,
            query,
        })
    }

    /// Whether a record's box `bx`, grown by the buffer, meets the walk's box (edges included)
    /// and segment (`FUN_141052e10`).
    pub(super) fn meets(&self, bx: Rect) -> bool {
        let distance = self.grow;
        Rect::between(self.lo, self.hi).overlaps(bx.grown(distance))
            && box_meets_segment(bx, distance, self.walk)
    }
}

/// `FUN_141054ec0`: whether segments `p1`–`p2` and `p3`–`p4` meet.
pub(super) fn segments_meet(p1: Point, p2: Point, p3: Point, p4: Point) -> bool {
    (ccw(p1, p2, p4) * ccw(p1, p2, p3)) as f32 <= 0.0
        && (ccw(p3, p4, p2) * ccw(p3, p4, p1)) as f32 <= 0.0
}

/// `FUN_141052e10` (the segment test's) and `FUN_1410512f0` (the walk test's) with the box grown
/// by `distance` (the manager's `+0x57`/`+0x58`): the walk crosses one of the grown box's edges or
/// starts or ends inside it, the box ordered (low and high edges, [`Rect::ordered`]) for the
/// inside tests.
pub(super) fn box_meets_segment(rect: Rect, distance: Distance, walk: Segment) -> bool {
    segment_meets_box(rect, distance.get(), walk, rect.ordered())
}

/// A record's test against the walk by its kind (`FUN_141051e00`, `FUN_141052800`): a circle's
/// (kind 5, `FUN_141051630` and `FUN_141053130`) reads its box as it stands for the inside tests;
/// the others' (kinds 2, 3, 11, 13) order it ([`box_meets_segment`]).
pub(super) fn record_meets_segment(
    kind: ObstructionKind,
    rect: Rect,
    distance: Distance,
    walk: Segment,
) -> bool {
    if kind == ObstructionKind::CIRCLE {
        segment_meets_box(rect, distance.get(), walk, rect)
    } else {
        box_meets_segment(rect, distance, walk)
    }
}

/// The four edges of box `rect` grown by `r`, each tested against the walk
/// (`FUN_141052ca0`: `FUN_141054ec0`), then the inside tests of the walk's ends against `inside`
/// grown by `r`. `FUN_141052ca0` also lists each crossing (record, id, point) at the manager's
/// `+0x40` while its `+0x54` is set; `FUN_141051900` sets `+0x54` only for a list at `+0x40`
/// with a cap `+0x50` of at least 1, and nothing gives the manager one: the only stores to
/// `+0x40`, `+0x48` and `+0x50` in its code (0x14104f000..0x141060000) are its constructor's
/// and `FUN_1410560b0`'s zeros, and none of the routines that address it through
/// `DAT_143f0a088` writes them. So the listing never runs, and the edges' order (the
/// `FUN_1410512f0` tests all four, `FUN_141052e10` stops at the first crossing) does not show.
pub(super) fn segment_meets_box(rect: Rect, r: f32, walk: Segment, inside: Rect) -> bool {
    let (bx0, by0, bx1, by1) = (rect.x0(), rect.y0(), rect.x1(), rect.y1());
    let (lo_x, lo_y, hi_x, hi_y) = (inside.x0(), inside.y0(), inside.x1(), inside.y1());
    let (start, end) = (walk.from, walk.to);
    let edges = [
        (Point::new(bx0 - r, by0 - r), Point::new(r + bx1, by0 - r)),
        (Point::new(r + bx1, by0 - r), Point::new(r + bx1, r + by1)),
        (Point::new(r + bx1, r + by1), Point::new(bx0 - r, r + by1)),
        (Point::new(bx0 - r, r + by1), Point::new(bx0 - r, by0 - r)),
    ];
    if edges
        .iter()
        .any(|&(e0, e1)| segments_meet(start, end, e0, e1))
    {
        return true;
    }
    // COMISS/JC (0x141052fb8..0x141052ff9, 0x1410530dc..0x141053119; 0x1410514ad..0x141051620,
    // 0x14105189e..0x1410518f2): a NaN coordinate is out
    let outside = |point: Point| {
        below_or_unordered(point.x(), lo_x - r)
            || below_or_unordered(r + hi_x, point.x())
            || below_or_unordered(point.y(), lo_y - r)
            || below_or_unordered(r + hi_y, point.y())
    };
    !(outside(start) && outside(end))
}
