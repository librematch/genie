//! The search's run: its main loop and a popped node's expansion by its type.

use crate::math::fmath::sqrt;
use crate::pathing::short_path::axis_dir::AxisDir;
use crate::pathing::short_path::outline::Outline;
use crate::pathing::short_path::{
    Callback, Corner, Ctx, Emit, FixedPoint, Hit, Kind, Meet, NUDGES, Node, NodeIndex,
    OPEN_CORNER_WITHOUT_EMIT, Poly, PolyIndex, RECT_HEURISTIC, RayFraction, Run, Search, ShortPath,
    Started, TO_FIXED, TO_FLOAT, UNSET, VertexIndex, WalkDir, cheb, corner_emits, float_distance,
    in_box, isqrt, through_float,
};
use crate::pathing::{FixedDistance, ShapeId, ShapeSide, ShortFound};
use crate::world::slots::CheckedIndexExt;
use crate::{GenieError, GenieResult};

/// What a polygon's re-walk keeps fixed: the node it comes from, the polygon and its vertex count.
struct Rewalk {
    parent: NodeIndex,
    poly: PolyIndex,
    count: u32,
}

impl Search<'_> {
    /// Type 2, a node on a shape's side: a polygon already holding its point is walked again from
    /// that edge (both ways, while cheaper than the vertices' costs); else the outline of its
    /// cluster is traced into a new polygon and walked. Some(4) when the outline fails.
    pub(super) fn side_expand(
        &mut self,
        node: Node,
        shape: ShapeId,
        side: ShapeSide,
    ) -> GenieResult<Option<ShortPath>> {
        let fixed_point = node.at;
        let fixed_distance = self.ctx.clearance;
        for index in 0..self.polys.len() {
            let Some(vertex_index) = Self::edge_holding(
                self.polys.checked(index)?,
                fixed_point,
                side,
                self.ctx.mesh,
                Some(shape),
                fixed_distance,
            ) else {
                continue;
            };
            if !RECT_HEURISTIC || !self.polys.checked(index)?.flag {
                return Ok(None);
            }
            self.rewalk(node, PolyIndex::of(index), vertex_index)?;
            return Ok(None);
        }
        self.polys.push(Poly::default());
        let mut poly = Poly::default();
        // the game's code: 1 the trace's step limit, 2 a vertex on an older polygon's edge
        let mut failed =
            self.trace(shape, side, Some(fixed_point), &mut poly)? == Outline::StepLimit;
        let last = self.polys.len() - 1;
        'older: for earlier in 0..last {
            for index in 0..poly.vertices.len() {
                let hit = Self::edge_holding(
                    self.polys.checked(earlier)?,
                    *poly.vertices.checked(index)?,
                    *poly.sides.checked(index)?,
                    self.ctx.mesh,
                    *poly.shapes.checked(index)?,
                    fixed_distance,
                );
                if hit.is_some() {
                    failed = true;
                    continue 'older;
                }
            }
        }
        let empty = poly.vertices.is_empty();
        *self.polys.checked_mut(last)? = poly;
        if failed || empty {
            return Ok(Some(ShortPath {
                result: ShortFound::Invalid,
                points: Vec::new(),
            }));
        }
        self.walk_new(PolyIndex::of(last), fixed_point, node.cost, node.idx)?;
        Ok(None)
    }

    /// The re-walk of a polygon from edge `vertex_index` (0x140fa63d4..0x140fa6639): backward from
    /// `v[vertex_index]`, forward from `v[vertex_index + 1]`, the forward side on equal costs, each
    /// while its cost stays below the vertex's; vertices no corner node reached are passed over.
    pub(super) fn rewalk(
        &mut self,
        node: Node,
        pi: PolyIndex,
        vertex_index: VertexIndex,
    ) -> GenieResult<()> {
        let fixed_point = node.at;
        let poly = self.poly(pi)?;
        let count = poly.count();
        let next0 = vertex_index.next_in(count);
        let mut bv = vertex_index;
        let mut fv = next0;
        let mut gb = cheb(poly.vertex(vertex_index)?, fixed_point).wrapping_add(node.cost);
        let mut gf = cheb(poly.vertex(next0)?, fixed_point).wrapping_add(node.cost);
        let alive = |cost: FixedDistance, fixed_distance: FixedDistance| {
            fixed_distance < cost || cost == UNSET
        };
        let mut alive_b = alive(poly.cost(vertex_index)?, gb);
        let mut alive_f = alive(poly.cost(next0)?, gf);
        let walk = Rewalk {
            parent: node.idx,
            poly: pi,
            count,
        };
        loop {
            if !(alive_b || alive_f) {
                return Ok(());
            }
            let forward = (gf <= gb && alive_f) || !alive_b;
            let poly = self.poly(pi)?.clone();
            if forward {
                self.rewalk_corner(&walk, &poly, (fv, gf), (WalkDir::Forward, vertex_index, fv))?;
                (fv, gf, alive_f) =
                    self.rewalk_advance(&walk, &poly, (fv, gf), WalkDir::Forward)?;
            } else {
                self.rewalk_corner(&walk, &poly, (bv, gb), (WalkDir::Backward, bv, next0))?;
                (bv, gb, alive_b) =
                    self.rewalk_advance(&walk, &poly, (bv, gb), WalkDir::Backward)?;
            }
        }
    }

    /// The re-walk's corner at vertex `at` of `poly`, reached with cost `fixed_distance` as a walk
    /// in direction `dir` over the run `lo`..`hi`: a vertex a corner node reached before and no
    /// goal candidate, where the sides differ and the corner emits a token, becomes a corner node.
    fn rewalk_corner(
        &mut self,
        walk: &Rewalk,
        poly: &Poly,
        (at, fixed_distance): (VertexIndex, FixedDistance),
        (dir, lo, hi): (WalkDir, VertexIndex, VertexIndex),
    ) -> GenieResult<()> {
        let count = walk.count;
        let fixed_point = poly.vertex(at)?;
        let sp = poly.side(at.prev_in(count))?;
        let sn = poly.side(at)?;
        let sh = poly.shape(at)?;
        let run = Run {
            poly: walk.poly,
            dir,
            lo,
            hi,
        };
        if poly.cost(at)? != UNSET
            && !self.goal_test(Corner {
                cost: fixed_distance,
                parent: walk.parent,
                run,
                point: fixed_point,
            })?
            && sp != sn
            && sh.is_some()
            && let Some(token) = corner_emits(sp, sn, fixed_point, self.goal.at)
        {
            let kind = Kind::Corner {
                run: Some(run),
                emit: Some(Emit {
                    token,
                    prev: sp,
                    next: sn,
                }),
            };
            let node = self.add(fixed_distance, walk.parent, fixed_point, kind);
            self.push(node);
            *self.poly_mut(walk.poly)?.cost_mut(at)? = fixed_distance;
        }
        Ok(())
    }

    /// The re-walk on from vertex `from` (cost `fixed_distance`) to the next vertex a corner node
    /// reached, forward or backward: its place, the cost there and whether it is still below the
    /// vertex's.
    fn rewalk_advance(
        &self,
        walk: &Rewalk,
        poly: &Poly,
        (from, start_cost): (VertexIndex, FixedDistance),
        dir: WalkDir,
    ) -> GenieResult<(VertexIndex, FixedDistance, bool)> {
        let count = walk.count;
        // the costs as the search holds them, the vertices from the walk's copy
        let costs = self.poly(walk.poly)?;
        let (mut vi, mut fixed_distance) = (from, start_cost);
        let mut at = poly.vertex(from)?;
        loop {
            vi = if dir == WalkDir::Forward {
                vi.next_in(count)
            } else {
                vi.prev_in(count)
            };
            let fixed_point = poly.vertex(vi)?;
            fixed_distance = fixed_distance.wrapping_add(cheb(at, fixed_point));
            let cost = costs.cost(vi)?;
            at = fixed_point;
            if cost != UNSET {
                return Ok((vi, fixed_distance, fixed_distance < cost));
            }
        }
    }

    /// Type 3, a polygon corner: the raycast toward the goal's range point ignoring the corner's
    /// own touch (the token); a touch tries the four nudged points; then a crossing becomes a side
    /// node, a clear ray a terminal goal candidate, and a ray that still touches the axis L.
    pub(super) fn corner_expand(&mut self, corner: Node, emit: Emit) -> GenieResult<()> {
        let at = corner.at;
        let mut range_point = range_point(self.goal.range, at, self.goal.at)?;
        let mut frac = RayFraction::START;
        let reach = corner.cost.wrapping_add(float_distance(range_point, at));
        if !self.beats_goal(reach)? {
            return Ok(());
        }
        let mut found =
            self.ctx
                .raycast(at, range_point, &mut frac, Callback::Corner(emit.token))?;
        if touches(found) {
            let fixed_rect = self.bounds;
            for dir in NUDGES {
                let tj = FixedPoint::new(
                    ((i64::from(dir.x()) * i64::from(self.goal.rx.get())) >> 22) as i32
                        + range_point.x(),
                    ((i64::from(dir.y()) * i64::from(self.goal.ry.get())) >> 22) as i32
                        + range_point.y(),
                );
                if !in_box(tj, fixed_rect) {
                    continue;
                }
                let Some(token) = corner_emits(emit.prev, emit.next, at, tj) else {
                    continue;
                };
                let mut f2 = RayFraction::START;
                found = self.ctx.raycast(at, tj, &mut f2, Callback::Corner(token))?;
                if !touches(found) {
                    frac = f2;
                    range_point = tj;
                    break;
                }
            }
            if touches(found) {
                self.corner_axis(corner, range_point)?;
                return Ok(());
            }
        }
        if let Some((Meet::Cross, hit)) = found {
            let mut fixed_point = Self::at_fraction(at, range_point, frac);
            let shape = self.settle(&mut fixed_point, hit)?;
            if fixed_point != at {
                let fixed_distance = corner.cost.wrapping_add(float_distance(at, fixed_point));
                self.side_node(fixed_distance, corner.idx, shape, hit.side, fixed_point);
            }
            return Ok(());
        }
        let fixed_distance = corner.cost.wrapping_add(float_distance(at, range_point));
        let kind = Kind::Corner {
            run: None,
            emit: None,
        };
        let node = self.add(fixed_distance, corner.idx, range_point, kind);
        if self.beats_goal(node.cost)? {
            self.goal_node = Some(node.idx);
        }
        Ok(())
    }

    /// Type 3 when every ray touches: along x toward `toward`, then along y; a leg's hit becomes a
    /// side node; with both legs clear, one tile toward `toward` (at most) becomes a waypoint that
    /// may be the best (by cost, not estimate).
    pub(super) fn corner_axis(&mut self, corner: Node, toward: FixedPoint) -> GenieResult<()> {
        let corner_at = corner.at;
        let dx = toward.x().wrapping_sub(corner_at.x());
        let dy = toward.y().wrapping_sub(corner_at.y());
        let fixed_point = FixedPoint::new(toward.x(), corner_at.y());
        let sw = self.ctx.sweep(corner_at, fixed_point, AxisDir::of_dx(dx))?;
        if let Some((shape, side, at)) = sw.counted()? {
            let fixed_distance = corner.cost.wrapping_add(float_distance(corner_at, at));
            self.side_node(fixed_distance, corner.idx, shape, side, at);
            return Ok(());
        }
        let sw2 = self.ctx.sweep(fixed_point, toward, AxisDir::of_dy(dy))?;
        let Some((shape, side, at)) = sw2.counted()? else {
            let step = if dx.wrapping_abs() < 0x40_0000 {
                dx
            } else {
                (1.0f32.copysign(dx as f32 * TO_FLOAT) * TO_FIXED) as i32
            };
            let node = self.add(
                corner
                    .cost
                    .wrapping_add(FixedDistance::new(dx.wrapping_abs())),
                corner.idx,
                FixedPoint::new(step.wrapping_add(corner_at.x()), corner_at.y()),
                Kind::Waypoint,
            );
            // the game's test of the best for none (−1) never holds: it starts as the start node
            if node.cost < self.node(self.best)?.cost {
                self.best = node.idx;
            }
            return Ok(());
        };
        let mid = self.add(
            through_float(dx.wrapping_abs()).wrapping_add(corner.cost),
            corner.idx,
            fixed_point,
            Kind::Waypoint,
        );
        let fixed_distance = through_float((at.y().wrapping_sub(corner_at.y())).wrapping_abs())
            .wrapping_add(mid.cost);
        self.side_node(fixed_distance, mid.idx, shape, side, at);
        Ok(())
    }
}

impl Search<'_> {
    /// The search itself, after the prologue: the start, the main loop, the path.
    pub(super) fn run(&mut self, start: FixedPoint) -> GenieResult<ShortPath> {
        let started = self.start(start)?;
        match started {
            Started::InRange => {
                return Ok(ShortPath {
                    result: ShortFound::InRange,
                    points: Vec::new(),
                });
            }
            Started::Stuck => {
                return Ok(ShortPath {
                    result: ShortFound::Blocked,
                    points: Vec::new(),
                });
            }
            Started::Free | Started::Escaped => {}
        }
        self.goal_node = None;
        self.best = NodeIndex::START;
        while let Some(node) = self.open.pop() {
            if let Some(direct) = self.expand(node, start)? {
                return Ok(direct);
            }
        }
        // without a goal node in range, the best one
        let (result, goal) = match self.goal_node {
            Some(goal) => (ShortFound::Reached, goal),
            None => (ShortFound::Partial, self.best),
        };
        self.goal_node = Some(goal);
        // the path from the goal back, without the root
        let mut pts: Vec<FixedPoint> = Vec::new();
        let mut node = *self.node(goal)?;
        while let Some(up) = node.parent {
            let parent = *self.node(up)?;
            match node.kind {
                Kind::Corner {
                    run: Some(Run { poly, dir, lo, hi }),
                    ..
                } => {
                    pts.push(node.at);
                    let poly = self.poly(poly)?;
                    let count = poly.count();
                    if dir == WalkDir::Backward {
                        let mut vertex_index = lo;
                        while vertex_index != hi {
                            pts.push(poly.vertex(vertex_index)?);
                            vertex_index = vertex_index.next_in(count);
                        }
                    } else {
                        let mut vertex_index = hi;
                        while vertex_index != lo {
                            vertex_index = vertex_index.prev_in(count);
                            pts.push(poly.vertex(vertex_index)?);
                        }
                    }
                }
                _ => pts.push(node.at),
            }
            node = parent;
        }
        if started == Started::Free {
            pts.push(start);
        }
        // the smoothing, with half the radius
        let radius = i64::from(self.goal.radius.get());
        let r2 = ((radius * 0x40_0000 + ((radius >> 63) & 0x7f_ffff)) >> 23) as i32;
        let sight = Ctx {
            mesh: self.ctx.mesh,
            ignore: self.ctx.ignore,
            clearance: FixedDistance::new(0x20_0000 - r2),
        };
        let mut index = 0;
        while index < pts.len() {
            let mut ahead = index + 2;
            while ahead < pts.len() {
                if !sight.sight_blocked(*pts.checked(ahead)?, *pts.checked(index)?)? {
                    let _ = pts.drain(index + 1..ahead);
                    ahead = index + 1;
                }
                ahead += 1;
            }
            index += 1;
        }
        if started != Started::Free {
            pts.push(start);
        }
        pts.reverse();
        Ok(ShortPath {
            result,
            points: pts,
        })
    }

    /// One popped node of the main loop; Some when the search ends at once (the direct path).
    pub(super) fn expand(
        &mut self,
        node: Node,
        start: FixedPoint,
    ) -> GenieResult<Option<ShortPath>> {
        let at = node.at;
        let clearance = self.ctx.clearance;
        match node.kind {
            Kind::Waypoint => {
                let _ = self.pop_test(&node)?;
            }
            Kind::Start => {
                if self.pop_test(&node)? {
                    return Ok(None);
                }
                let range_point = range_point(self.goal.range, at, self.goal.at)?;
                let mut frac = RayFraction::START;
                let found = self
                    .ctx
                    .raycast(at, range_point, &mut frac, Callback::Box)?;
                match found {
                    None => {
                        return Ok(Some(ShortPath {
                            result: ShortFound::Reached,
                            points: vec![start, at, range_point],
                        }));
                    }
                    Some((Meet::Touch, hit)) => {
                        self.touch_from(node, at, range_point, frac, hit)?;
                    }
                    Some((_, hit)) => {
                        let mut fixed_point = Self::at_fraction(at, range_point, frac);
                        let shape = self.settle(&mut fixed_point, hit)?;
                        let fixed_distance =
                            node.cost.wrapping_add(float_distance(at, fixed_point));
                        self.side_node(fixed_distance, node.idx, shape, hit.side, fixed_point);
                    }
                }
            }
            Kind::Side { shape, side } => {
                if self.pop_test(&node)? {
                    return Ok(None);
                }
                let _ = self.side_expand(node, shape, side)?;
            }
            Kind::Corner { emit, .. } => {
                if self.pop_test(&node)? {
                    return Ok(None);
                }
                self.corner_expand(node, emit.ok_or(OPEN_CORNER_WITHOUT_EMIT)?)?;
            }
        }
        let _ = clearance;
        Ok(None)
    }

    /// Type 0, `k == 2`: back off to a free point toward `to`, then the two L-shaped legs toward
    /// the touched shape's centre.
    pub(super) fn touch_from(
        &mut self,
        node: Node,
        from: FixedPoint,
        to: FixedPoint,
        hit_t: RayFraction,
        hit: Hit,
    ) -> GenieResult<()> {
        // back from the hit a tenth of the ray (`0x66666`) at a time
        let mut frac = RayFraction::new(hit_t.get() - 0x6_6666);
        let mut base = node;
        while 1 <= frac.get() {
            let qp = Self::at_fraction(from, to, frac);
            if self.ctx.holder(qp)?.is_none() {
                let scale = i64::from(frac.get());
                let dx = ((i64::from(to.x().wrapping_sub(from.x())) * scale) >> 22) as i32;
                let dy = ((i64::from(to.y().wrapping_sub(from.y())) * scale) >> 22) as i32;
                let fy = dy as f32 * TO_FLOAT;
                let fx = dx as f32 * TO_FLOAT;
                let fixed_distance =
                    FixedDistance::new((sqrt(fx * fx + fy * fy) * TO_FIXED) as i32);
                base = self.add(
                    node.cost.wrapping_add(fixed_distance),
                    node.idx,
                    qp,
                    Kind::Waypoint,
                );
                break;
            }
            frac = RayFraction::new(frac.get() - 0x6_6666);
        }
        let checked = *self.ctx.mesh.shapes.checked(hit.shape.index())?;
        let centre = FixedPoint::new(
            checked.x0().wrapping_add(
                (((i64::from(checked.x1() - checked.x0()) * 0x40_0000)
                    + ((i64::from(checked.x1() - checked.x0()) >> 63) & 0x7f_ffff))
                    >> 23) as i32,
            ),
            checked.y0().wrapping_add(
                (((i64::from(checked.y1() - checked.y0()) * 0x40_0000)
                    + ((i64::from(checked.y1() - checked.y0()) >> 63) & 0x7f_ffff))
                    >> 23) as i32,
            ),
        );
        let fixed_rect = self.bounds;
        let fixed_point = FixedPoint::new(
            if fixed_rect.x0() <= centre.x() {
                if fixed_rect.x1() < centre.x() {
                    fixed_rect.x1()
                } else {
                    centre.x()
                }
            } else {
                fixed_rect.x0()
            },
            if fixed_rect.y0() <= centre.y() {
                if fixed_rect.y1() < centre.y() {
                    fixed_rect.y1()
                } else {
                    centre.y()
                }
            } else {
                fixed_rect.y0()
            },
        );
        let delta = FixedPoint::new(
            fixed_point.x().wrapping_sub(from.x()),
            fixed_point.y().wrapping_sub(from.y()),
        );
        let dir_x = AxisDir::of_dx(delta.x());
        let dir_y = AxisDir::of_dy(delta.y());
        self.legs(base, fixed_point, dir_x, dir_y, delta)?;
        self.legs(base, fixed_point, dir_y, dir_x, delta)
    }
}

/// Whether a raycast's hit is a touch (its result 2), which the corner's nudges try to get past.
const fn touches(found: Option<(Meet, Hit)>) -> bool {
    matches!(found, Some((Meet::Touch, _)))
}

/// `FUN_140fa31b0`: the point on the circle of `range` around `goal` toward `fixed_point` (the goal
/// itself for no range).
pub(super) fn range_point(
    range: FixedDistance,
    fixed_point: FixedPoint,
    goal: FixedPoint,
) -> GenieResult<FixedPoint> {
    let mut out = goal;
    if range != FixedDistance::ZERO {
        let range = range.get();
        let dx = i64::from(fixed_point.x().wrapping_sub(goal.x()));
        let dy = i64::from(fixed_point.y().wrapping_sub(goal.y()));
        let len = isqrt(((dy * dy) as u64 & !0x3f_ffff).wrapping_add((dx * dx) as u64 & !0x3f_ffff))
            as i32;
        if len == 0 {
            return Err(GenieError::unsupported(
                "the range point of a point on the goal (a division by zero)",
            ));
        }
        let ux = ((dx << 22) / i64::from(len)) as i32;
        let uy = ((dy << 22) / i64::from(len)) as i32;
        out = FixedPoint::new(
            out.x()
                .wrapping_add(((i64::from(ux) * i64::from(range)) >> 22) as i32),
            out.y()
                .wrapping_add(((i64::from(uy) * i64::from(range)) >> 22) as i32),
        );
    }
    Ok(out)
}
