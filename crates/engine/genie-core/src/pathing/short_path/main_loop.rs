//! The search's run: its main loop and a popped node's expansion by its type.

use crate::pathing::short_path::axis_dir::AxisDir;
use crate::pathing::short_path::{
    Callback, Corner, Ctx, Emit, FixedPoint, Hit, Kind, NONE, NUDGES, Node,
    OPEN_CORNER_WITHOUT_EMIT, Poly, RECT_HEURISTIC, Search, ShortPath, T_START, TO_FIXED, TO_FLOAT,
    UNSET, WalkDir, cheb, corner_emits, float_distance, in_box, isqrt, through_float,
};
use crate::pathing::{FixedDistance, ShapeId, ShapeSide};
use crate::world::slots::CheckedIndex;
use crate::{GenieError, math::fmath};

/// What a polygon's re-walk keeps fixed: the node it comes from, the polygon and its vertex count.
struct Rewalk {
    parent: u32,
    poly: u32,
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
            self.rewalk(n, i as u32, e)?;
            return Ok(None);
        }
        self.polys.push(Poly::default());
        let mut poly = Poly::default();
        let mut rc = self.trace(shape, side, Some(p), &mut poly)?;
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
                    rc = 2;
                    continue 'older;
                }
            }
        }
        let empty = poly.vertices.is_empty();
        *self.polys.checked_mut(last)? = poly;
        if rc == 1 || rc == 2 || empty {
            return Ok(Some(ShortPath {
                result: 4,
                points: Vec::new(),
            }));
        }
        self.walk_new(last as u32, p, n.g, n.idx)?;
        Ok(None)
    }

    /// The re-walk of a polygon from edge `e` (0x140fa63d4..0x140fa6639): backward from `v[e]`,
    /// forward from `v[e + 1]`, the forward side on equal costs, each while its cost stays below
    /// the vertex's; vertices no corner node reached are passed over.
    pub(super) fn rewalk(&mut self, n: Node, pi: u32, e: u32) -> Result<(), GenieError> {
        let p = n.at;
        let poly = self.polys.checked(pi as usize)?;
        let m = poly.vertices.len() as u32;
        let next0 = (e + 1) % m;
        let mut bv = e;
        let mut fv = next0;
        let mut gb = cheb(*poly.vertices.checked(e as usize)?, p).wrapping_add(n.g);
        let mut gf = cheb(*poly.vertices.checked(next0 as usize)?, p).wrapping_add(n.g);
        let alive = |cost: i32, g: i32| g < cost || cost == UNSET;
        let mut alive_b = alive(*poly.costs.checked(e as usize)?, gb);
        let mut alive_f = alive(*poly.costs.checked(next0 as usize)?, gf);
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
            let poly = self.polys.checked(pi as usize)?.clone();
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
        (at, g): (u32, i32),
        (dir, lo, hi): (WalkDir, u32, u32),
    ) -> Result<(), GenieError> {
        let m = walk.count;
        let v = *poly.vertices.checked(at as usize)?;
        let sp = *poly.sides.checked(((at + m - 1) % m) as usize)?;
        let sn = *poly.sides.checked(at as usize)?;
        let sh = *poly.shapes.checked(at as usize)?;
        if *poly.costs.checked(at as usize)? != UNSET
            && !self.goal_test(Corner {
                cost: g,
                parent: walk.parent,
                dir,
                low: lo,
                high: hi,
                poly: walk.poly,
                point: v,
            })?
            && sp != sn
            && sh.is_some()
            && let Some(token) = corner_emits(sp, sn, v, self.goal.at)
        {
            let kind = Kind::Corner {
                dir,
                lo,
                hi,
                poly: walk.poly,
                emit: Some(Emit {
                    token,
                    prev: sp,
                    next: sn,
                }),
            };
            let node = self.add(g, walk.parent, v, kind);
            self.push(node);
            *self
                .polys
                .checked_mut(walk.poly as usize)?
                .costs
                .checked_mut(at as usize)? = g;
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
        (from, g): (u32, i32),
        forward: bool,
    ) -> Result<(u32, i32, bool), GenieError> {
        let m = walk.count;
        let costs = &self.polys.checked(walk.poly as usize)?.costs;
        let (mut vi, mut g) = (from, g);
        let mut at = *poly.vertices.checked(from as usize)?;
        loop {
            vi = if forward {
                (vi + 1) % m
            } else {
                (m - 1 + vi) % m
            };
            let w = *poly.vertices.checked(vi as usize)?;
            g = g.wrapping_add(cheb(at, w));
            let cost = *costs.checked(vi as usize)?;
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
        let mut frac = T_START;
        let reach = n.g.wrapping_add(float_distance(t, p));
        if self.goal_node != NONE && (reach >= self.arena.checked(self.goal_node as usize)?.g) {
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
                let mut f2 = T_START;
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
        if let Some((1, hit)) = found {
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
            dir: WalkDir::Forward,
            lo: NONE,
            hi: NONE,
            poly: NONE,
            emit: None,
        };
        let node = self.add(g, n.idx, t, kind);
        if self.goal_node == NONE || node.g < self.arena.checked(self.goal_node as usize)?.g {
            self.goal_node = node.idx;
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
                n.g.wrapping_add(dx.wrapping_abs()),
                n.idx,
                FixedPoint::new(step.wrapping_add(p.x()), p.y()),
                Kind::Waypoint,
            );
            if self.best == NONE || w.g < self.arena.checked(self.best as usize)?.g {
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
        let res4410 = self.start(start)?;
        match res4410 {
            3 => {
                return Ok(ShortPath {
                    result: 0,
                    points: Vec::new(),
                });
            }
            2 => {
                return Ok(ShortPath {
                    result: 3,
                    points: Vec::new(),
                });
            }
            _ => {}
        }
        self.goal_node = NONE;
        self.best = 0;
        while let Some(n) = self.open.pop() {
            if let Some(direct) = self.expand(n, start)? {
                return Ok(direct);
            }
        }
        let mut result = 1;
        if self.goal_node == NONE {
            result = 2;
            self.goal_node = self.best;
        }
        // the path from the goal back, without the root
        let mut pts: Vec<FixedPoint> = Vec::new();
        let mut node = *self.arena.checked(self.goal_node as usize)?;
        let mut is_root = node.parent == NONE;
        while !is_root {
            let parent = *self.arena.checked(node.parent as usize)?;
            match node.kind {
                Kind::Corner {
                    poly, dir, lo, hi, ..
                } if poly != NONE => {
                    pts.push(node.at);
                    let v = &self.polys.checked(poly as usize)?.vertices;
                    let n = v.len() as u32;
                    if dir == WalkDir::Backward {
                        let mut k = lo;
                        while k != hi {
                            pts.push(*v.checked(k as usize)?);
                            k = (k + 1) % n;
                        }
                    } else {
                        let mut k = hi;
                        while k != lo {
                            k = (k + n - 1) % n;
                            pts.push(*v.checked(k as usize)?);
                        }
                    }
                }
                _ => pts.push(node.at),
            }
            is_root = parent.parent == NONE;
            node = parent;
        }
        if res4410 == 0 {
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
        if res4410 != 0 {
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
                let mut frac = T_START;
                let found = self.ctx.raycast(p, t, &mut frac, Callback::Box)?;
                match found {
                    None => {
                        return Ok(Some(ShortPath {
                            result: 1,
                            points: vec![start, p, t],
                        }));
                    }
                    Some((2, hit)) => self.touch_from(n, p, t, frac, hit)?,
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
        hit_t: i32,
        hit: Hit,
    ) -> Result<(), GenieError> {
        let mut frac = hit_t - 0x6_6666;
        let mut base = n;
        while 1 <= frac {
            let qp = Self::at_fraction(p, t, frac);
            if self.ctx.holder(qp)?.is_none() {
                let dx = ((i64::from(t.x().wrapping_sub(p.x())) * i64::from(frac)) >> 22) as i32;
                let dy = ((i64::from(t.y().wrapping_sub(p.y())) * i64::from(frac)) >> 22) as i32;
                let fy = dy as f32 * TO_FLOAT;
                let fx = dx as f32 * TO_FLOAT;
                let g = (fmath::sqrt(fx * fx + fy * fy) * TO_FIXED) as i32;
                base = self.add(n.g.wrapping_add(g), n.idx, qp, Kind::Waypoint);
                break;
            }
            frac -= 0x6_6666;
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
const fn touches(found: Option<(i32, Hit)>) -> bool {
    matches!(found, Some((2, _)))
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
