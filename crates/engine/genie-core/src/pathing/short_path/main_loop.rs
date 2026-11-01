//! The search's run: its main loop and a popped node's expansion by its type.

use crate::pathing::short_path::axis_dir::AxisDir;
use crate::pathing::short_path::outline::Outline;
use crate::pathing::short_path::{
    Callback, Corner, Ctx, Emit, FixedPoint, Hit, Kind, Meet, NUDGES, Node, NodeIndex,
    OPEN_CORNER_WITHOUT_EMIT, Poly, PolyIndex, RECT_HEURISTIC, RayFraction, Run, Search, ShortPath,
    Started, TO_FIXED, TO_FLOAT, UNSET, VertexIndex, WalkDir, cheb, corner_emits, float_distance,
    in_box, isqrt, through_float,
};
use crate::pathing::{FixedDistance, ShapeId, ShapeSide, ShortFound};
use crate::world::slots::CheckedIndex;
use crate::{GenieError, math::fmath};

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
        n: Node,
        shape: ShapeId,
        side: ShapeSide,
    ) -> Result<Option<ShortPath>, GenieError> {
        let p = n.at;
        let d = self.ctx.d;
        for i in 0..self.polys.len() {
            let Some(e) = Self::edge_holding(
                self.polys.checked(i)?,
                p,
                side,
                self.ctx.mesh,
                Some(shape),
                d,
            ) else {
                continue;
            };
            if !RECT_HEURISTIC || !self.polys.checked(i)?.flag {
                return Ok(None);
            }
            self.rewalk(n, PolyIndex::of(i), e)?;
            return Ok(None);
        }
        self.polys.push(Poly::default());
        let mut poly = Poly::default();
        // the game's code: 1 the trace's step limit, 2 a vertex on an older polygon's edge
        let mut failed = self.trace(shape, side, Some(p), &mut poly)? == Outline::StepLimit;
        let last = self.polys.len() - 1;
        'older: for j in 0..last {
            for k in 0..poly.vertices.len() {
                let hit = Self::edge_holding(
                    self.polys.checked(j)?,
                    *poly.vertices.checked(k)?,
                    *poly.sides.checked(k)?,
                    self.ctx.mesh,
                    *poly.shapes.checked(k)?,
                    d,
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
        self.walk_new(PolyIndex::of(last), p, n.g, n.idx)?;
        Ok(None)
    }

    /// The re-walk of a polygon from edge `e` (0x140fa63d4..0x140fa6639): backward from `v[e]`,
    /// forward from `v[e + 1]`, the forward side on equal costs, each while its cost stays below
    /// the vertex's; vertices no corner node reached are passed over.
    pub(super) fn rewalk(
        &mut self,
        n: Node,
        pi: PolyIndex,
        e: VertexIndex,
    ) -> Result<(), GenieError> {
        let p = n.at;
        let poly = self.poly(pi)?;
        let m = poly.count();
        let next0 = e.next_in(m);
        let mut bv = e;
        let mut fv = next0;
        let mut gb = cheb(poly.vertex(e)?, p).wrapping_add(n.g);
        let mut gf = cheb(poly.vertex(next0)?, p).wrapping_add(n.g);
        let alive = |cost: FixedDistance, g: FixedDistance| g < cost || cost == UNSET;
        let mut alive_b = alive(poly.cost(e)?, gb);
        let mut alive_f = alive(poly.cost(next0)?, gf);
        let walk = Rewalk {
            parent: n.idx,
            poly: pi,
            count: m,
        };
        loop {
            if !(alive_b || alive_f) {
                return Ok(());
            }
            let forward = (gf <= gb && alive_f) || !alive_b;
            let poly = self.poly(pi)?.clone();
            if forward {
                self.rewalk_corner(&walk, &poly, (fv, gf), (WalkDir::Forward, e, fv))?;
                (fv, gf, alive_f) = self.rewalk_advance(&walk, &poly, (fv, gf), true)?;
            } else {
                self.rewalk_corner(&walk, &poly, (bv, gb), (WalkDir::Backward, bv, next0))?;
                (bv, gb, alive_b) = self.rewalk_advance(&walk, &poly, (bv, gb), false)?;
            }
        }
    }

    /// The re-walk's corner at vertex `at` of `poly`, reached with cost `g` as a walk in
    /// direction `dir` over the run `lo`..`hi`: a vertex a corner node reached before and no goal
    /// candidate, where the sides differ and the corner emits a token, becomes a corner node.
    fn rewalk_corner(
        &mut self,
        walk: &Rewalk,
        poly: &Poly,
        (at, g): (VertexIndex, FixedDistance),
        (dir, lo, hi): (WalkDir, VertexIndex, VertexIndex),
    ) -> Result<(), GenieError> {
        let m = walk.count;
        let v = poly.vertex(at)?;
        let sp = poly.side(at.prev_in(m))?;
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
                cost: g,
                parent: walk.parent,
                run,
                point: v,
            })?
            && sp != sn
            && sh.is_some()
            && let Some(token) = corner_emits(sp, sn, v, self.goal.at)
        {
            let kind = Kind::Corner {
                run: Some(run),
                emit: Some(Emit {
                    token,
                    prev: sp,
                    next: sn,
                }),
            };
            let node = self.add(g, walk.parent, v, kind);
            self.push(node);
            *self.poly_mut(walk.poly)?.cost_mut(at)? = g;
        }
        Ok(())
    }

    /// The re-walk on from vertex `from` (cost `g`) to the next vertex a corner node reached,
    /// forward or backward: its place, the cost there and whether it is still below the
    /// vertex's.
    fn rewalk_advance(
        &self,
        walk: &Rewalk,
        poly: &Poly,
        (from, g): (VertexIndex, FixedDistance),
        forward: bool,
    ) -> Result<(VertexIndex, FixedDistance, bool), GenieError> {
        let m = walk.count;
        // the costs as the search holds them, the vertices from the walk's copy
        let costs = self.poly(walk.poly)?;
        let (mut vi, mut g) = (from, g);
        let mut at = poly.vertex(from)?;
        loop {
            vi = if forward {
                vi.next_in(m)
            } else {
                vi.prev_in(m)
            };
            let w = poly.vertex(vi)?;
            g = g.wrapping_add(cheb(at, w));
            let cost = costs.cost(vi)?;
            at = w;
            if cost != UNSET {
                return Ok((vi, g, g < cost));
            }
        }
    }

    /// Type 3, a polygon corner: the raycast toward the goal's range point ignoring the corner's
    /// own touch (the token); a touch tries the four nudged points; then a crossing becomes a side
    /// node, a clear ray a terminal goal candidate, and a ray that still touches the axis L.
    pub(super) fn corner_expand(&mut self, n: Node, emit: Emit) -> Result<(), GenieError> {
        let p = n.at;
        let mut t = range_point(self.goal.range, p, self.goal.at)?;
        let mut frac = RayFraction::START;
        let reach = n.g.wrapping_add(float_distance(t, p));
        if !self.beats_goal(reach)? {
            return Ok(());
        }
        let mut found = self
            .ctx
            .raycast(p, t, &mut frac, Callback::Corner(emit.token))?;
        if touches(found) {
            let b = self.bounds;
            for dir in NUDGES {
                let tj = FixedPoint::new(
                    ((i64::from(dir.x()) * i64::from(self.goal.rx)) >> 22) as i32 + t.x(),
                    ((i64::from(dir.y()) * i64::from(self.goal.ry)) >> 22) as i32 + t.y(),
                );
                if !in_box(tj, b) {
                    continue;
                }
                let Some(token) = corner_emits(emit.prev, emit.next, p, tj) else {
                    continue;
                };
                let mut f2 = RayFraction::START;
                found = self.ctx.raycast(p, tj, &mut f2, Callback::Corner(token))?;
                if !touches(found) {
                    frac = f2;
                    t = tj;
                    break;
                }
            }
            if touches(found) {
                self.corner_axis(n, t)?;
                return Ok(());
            }
        }
        if let Some((Meet::Cross, hit)) = found {
            let mut h = Self::at_fraction(p, t, frac);
            let shape = self.settle(&mut h, hit)?;
            if h != p {
                let g = n.g.wrapping_add(float_distance(p, h));
                self.side_node(g, n.idx, shape, hit.side, h);
            }
            return Ok(());
        }
        let g = n.g.wrapping_add(float_distance(p, t));
        let kind = Kind::Corner {
            run: None,
            emit: None,
        };
        let node = self.add(g, n.idx, t, kind);
        if self.beats_goal(node.g)? {
            self.goal_node = Some(node.idx);
        }
        Ok(())
    }

    /// Type 3 when every ray touches: along x toward `t`, then along y; a leg's hit becomes a
    /// side node; with both legs clear, one tile toward `t` (at most) becomes a waypoint that may
    /// be the best (by cost, not estimate).
    pub(super) fn corner_axis(&mut self, n: Node, t: FixedPoint) -> Result<(), GenieError> {
        let p = n.at;
        let dx = t.x().wrapping_sub(p.x());
        let dy = t.y().wrapping_sub(p.y());
        let m = FixedPoint::new(t.x(), p.y());
        let sw = self.ctx.sweep(p, m, AxisDir::of_dx(dx))?;
        if let Some((shape, side, at)) = sw.counted()? {
            let g = n.g.wrapping_add(float_distance(p, at));
            self.side_node(g, n.idx, shape, side, at);
            return Ok(());
        }
        let sw2 = self.ctx.sweep(m, t, AxisDir::of_dy(dy))?;
        let Some((shape, side, at)) = sw2.counted()? else {
            let step = if dx.wrapping_abs() < 0x40_0000 {
                dx
            } else {
                (1.0f32.copysign(dx as f32 * TO_FLOAT) * TO_FIXED) as i32
            };
            let w = self.add(
                n.g.wrapping_add(FixedDistance::new(dx.wrapping_abs())),
                n.idx,
                FixedPoint::new(step.wrapping_add(p.x()), p.y()),
                Kind::Waypoint,
            );
            // the game's test of the best for none (−1) never holds: it starts as the start node
            if w.g < self.node(self.best)?.g {
                self.best = w.idx;
            }
            return Ok(());
        };
        let mid = self.add(
            through_float(dx.wrapping_abs()).wrapping_add(n.g),
            n.idx,
            m,
            Kind::Waypoint,
        );
        let g = through_float((at.y().wrapping_sub(p.y())).wrapping_abs()).wrapping_add(mid.g);
        self.side_node(g, mid.idx, shape, side, at);
        Ok(())
    }
}

impl Search<'_> {
    /// The search itself, after the prologue: the start, the main loop, the path.
    pub(super) fn run(&mut self, start: FixedPoint) -> Result<ShortPath, GenieError> {
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
        while let Some(n) = self.open.pop() {
            if let Some(direct) = self.expand(n, start)? {
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
                    let v = self.poly(poly)?;
                    let n = v.count();
                    if dir == WalkDir::Backward {
                        let mut k = lo;
                        while k != hi {
                            pts.push(v.vertex(k)?);
                            k = k.next_in(n);
                        }
                    } else {
                        let mut k = hi;
                        while k != lo {
                            k = k.prev_in(n);
                            pts.push(v.vertex(k)?);
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
        let r = i64::from(self.goal.r.get());
        let r2 = ((r * 0x40_0000 + ((r >> 63) & 0x7f_ffff)) >> 23) as i32;
        let sight = Ctx {
            mesh: self.ctx.mesh,
            ignore: self.ctx.ignore,
            d: FixedDistance::new(0x20_0000 - r2),
        };
        let mut i = 0;
        while i < pts.len() {
            let mut j = i + 2;
            while j < pts.len() {
                if !sight.sight_blocked(*pts.checked(j)?, *pts.checked(i)?)? {
                    let _ = pts.drain(i + 1..j);
                    j = i + 1;
                }
                j += 1;
            }
            i += 1;
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
        n: Node,
        start: FixedPoint,
    ) -> Result<Option<ShortPath>, GenieError> {
        let p = n.at;
        let d = self.ctx.d;
        match n.kind {
            Kind::Waypoint => {
                let _ = self.pop_test(&n)?;
            }
            Kind::Start => {
                if self.pop_test(&n)? {
                    return Ok(None);
                }
                let t = range_point(self.goal.range, p, self.goal.at)?;
                let mut frac = RayFraction::START;
                let found = self.ctx.raycast(p, t, &mut frac, Callback::Box)?;
                match found {
                    None => {
                        return Ok(Some(ShortPath {
                            result: ShortFound::Reached,
                            points: vec![start, p, t],
                        }));
                    }
                    Some((Meet::Touch, hit)) => self.touch_from(n, p, t, frac, hit)?,
                    Some((_, hit)) => {
                        let mut h = Self::at_fraction(p, t, frac);
                        let shape = self.settle(&mut h, hit)?;
                        let g = n.g.wrapping_add(float_distance(p, h));
                        self.side_node(g, n.idx, shape, hit.side, h);
                    }
                }
            }
            Kind::Side { shape, side } => {
                if self.pop_test(&n)? {
                    return Ok(None);
                }
                let _ = self.side_expand(n, shape, side)?;
            }
            Kind::Corner { emit, .. } => {
                if self.pop_test(&n)? {
                    return Ok(None);
                }
                self.corner_expand(n, emit.ok_or(OPEN_CORNER_WITHOUT_EMIT)?)?;
            }
        }
        let _ = d;
        Ok(None)
    }

    /// Type 0, `k == 2`: back off to a free point toward `t`, then the two L-shaped legs toward
    /// the touched shape's centre.
    pub(super) fn touch_from(
        &mut self,
        n: Node,
        p: FixedPoint,
        t: FixedPoint,
        hit_t: RayFraction,
        hit: Hit,
    ) -> Result<(), GenieError> {
        // back from the hit a tenth of the ray (`0x66666`) at a time
        let mut frac = RayFraction::new(hit_t.get() - 0x6_6666);
        let mut base = n;
        while 1 <= frac.get() {
            let qp = Self::at_fraction(p, t, frac);
            if self.ctx.holder(qp)?.is_none() {
                let f = i64::from(frac.get());
                let dx = ((i64::from(t.x().wrapping_sub(p.x())) * f) >> 22) as i32;
                let dy = ((i64::from(t.y().wrapping_sub(p.y())) * f) >> 22) as i32;
                let fy = dy as f32 * TO_FLOAT;
                let fx = dx as f32 * TO_FLOAT;
                let g = FixedDistance::new((fmath::sqrt(fx * fx + fy * fy) * TO_FIXED) as i32);
                base = self.add(n.g.wrapping_add(g), n.idx, qp, Kind::Waypoint);
                break;
            }
            frac = RayFraction::new(frac.get() - 0x6_6666);
        }
        let r = *self.ctx.mesh.shapes.checked(hit.shape.index())?;
        let centre = FixedPoint::new(
            r.x0().wrapping_add(
                (((i64::from(r.x1() - r.x0()) * 0x40_0000)
                    + ((i64::from(r.x1() - r.x0()) >> 63) & 0x7f_ffff))
                    >> 23) as i32,
            ),
            r.y0().wrapping_add(
                (((i64::from(r.y1() - r.y0()) * 0x40_0000)
                    + ((i64::from(r.y1() - r.y0()) >> 63) & 0x7f_ffff))
                    >> 23) as i32,
            ),
        );
        let b = self.bounds;
        let c = FixedPoint::new(
            if b.x0() <= centre.x() {
                if b.x1() < centre.x() {
                    b.x1()
                } else {
                    centre.x()
                }
            } else {
                b.x0()
            },
            if b.y0() <= centre.y() {
                if b.y1() < centre.y() {
                    b.y1()
                } else {
                    centre.y()
                }
            } else {
                b.y0()
            },
        );
        let delta = FixedPoint::new(c.x().wrapping_sub(p.x()), c.y().wrapping_sub(p.y()));
        let dir_x = AxisDir::of_dx(delta.x());
        let dir_y = AxisDir::of_dy(delta.y());
        self.legs(base, c, dir_x, dir_y, delta)?;
        self.legs(base, c, dir_y, dir_x, delta)
    }
}

/// Whether a raycast's hit is a touch (its result 2), which the corner's nudges try to get past.
const fn touches(found: Option<(Meet, Hit)>) -> bool {
    matches!(found, Some((Meet::Touch, _)))
}

/// `FUN_140fa31b0`: the point on the circle of `range` around `goal` toward `p` (the goal itself
/// for no range).
pub(super) fn range_point(
    range: FixedDistance,
    p: FixedPoint,
    goal: FixedPoint,
) -> Result<FixedPoint, GenieError> {
    let mut out = goal;
    if range != FixedDistance::ZERO {
        let range = range.get();
        let dx = i64::from(p.x().wrapping_sub(goal.x()));
        let dy = i64::from(p.y().wrapping_sub(goal.y()));
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
