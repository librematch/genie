//! A new polygon's walk (`FUN_140fa7b50`): from a point on its closing edge along the outline both
//! ways at once, its corners nodes toward the goal.

use crate::GenieResult;
use crate::pathing::FixedDistance;
use crate::pathing::short_path::{
    Corner, Emit, FixedPoint, Foot, Kind, NodeIndex, Poly, PolyIndex, Run, Search, VertexIndex,
    WalkDir, cheb, corner_emits,
};

/// A new polygon's walk: the polygon and the node it comes from.
struct NewWalk {
    poly: PolyIndex,
    parent: NodeIndex,
}

impl Search<'_> {
    /// `FUN_140fa7b50`: from `p` on a new polygon's closing edge, walk the outline both ways at
    /// once, the cheaper side first (ties forward): each corner the goal test does not end
    /// becomes a corner node toward the goal when it emits; each edge gets its foot test.
    pub(super) fn walk_new(
        &mut self,
        pi: PolyIndex,
        p: FixedPoint,
        g0: FixedDistance,
        parent: NodeIndex,
    ) -> GenieResult<()> {
        let poly = self.poly(pi)?.clone();
        let count = poly.count();
        let walk = NewWalk { poly: pi, parent };
        let (mut cb, mut cf) = self.walk_new_start(&walk, &poly, p, g0)?;
        let mut bi = VertexIndex::new(count - 1);
        let mut fi = VertexIndex::FIRST;
        if bi == VertexIndex::FIRST {
            return Ok(());
        }
        loop {
            if cb < cf {
                let vv = poly.vertex(bi)?;
                self.walk_new_corner(
                    &walk,
                    &poly,
                    (bi, bi.pred()),
                    cb,
                    (WalkDir::Backward, bi, VertexIndex::FIRST),
                )?;
                let vertex = poly.vertex(bi.pred())?;
                let dx = vv.x() - vertex.x();
                self.foot_test(Foot {
                    start: vv,
                    end: vertex,
                    extent_x: dx,
                    run: Run {
                        poly: pi,
                        dir: WalkDir::Backward,
                        lo: bi.pred(),
                        hi: VertexIndex::FIRST,
                    },
                    start_cost: cb,
                    parent,
                })?;
                cb = cb.wrapping_add(cheb(vv, vertex));
                bi = bi.pred();
            } else {
                let vv = poly.vertex(fi)?;
                self.walk_new_corner(
                    &walk,
                    &poly,
                    (fi, fi.prev_in(count)),
                    cf,
                    (WalkDir::Forward, VertexIndex::FIRST, fi),
                )?;
                fi = fi.succ();
                let fixed_point = poly.vertex(fi)?;
                let dx = vv.x() - fixed_point.x();
                self.foot_test(Foot {
                    start: vv,
                    end: fixed_point,
                    extent_x: dx,
                    run: Run {
                        poly: pi,
                        dir: WalkDir::Forward,
                        lo: VertexIndex::FIRST,
                        hi: fi,
                    },
                    start_cost: cf,
                    parent,
                })?;
                cf = cf.wrapping_add(cheb(vv, fixed_point));
            }
            if fi >= bi {
                return Ok(());
            }
        }
    }

    /// The walk of a new polygon's start: the foot tests of the two edges from `fixed_point` to the
    /// closing vertices, and the costs of the two walks (the larger axis distance on top of
    /// `g0`), backward then forward.
    fn walk_new_start(
        &mut self,
        walk: &NewWalk,
        poly: &Poly,
        fixed_point: FixedPoint,
        g0: FixedDistance,
    ) -> GenieResult<(FixedDistance, FixedDistance)> {
        let last = VertexIndex::new(poly.count() - 1);
        let (vb, vf) = (poly.vertex(last)?, poly.vertex(VertexIndex::FIRST)?);
        let (dxb, dyb) = (vb.x() - fixed_point.x(), vb.y() - fixed_point.y());
        let (dxf, dyf) = (vf.x() - fixed_point.x(), vf.y() - fixed_point.y());
        self.foot_test(Foot {
            start: fixed_point,
            end: vb,
            extent_x: dxb,
            run: Run {
                poly: walk.poly,
                dir: WalkDir::Backward,
                lo: last,
                hi: VertexIndex::FIRST,
            },
            start_cost: g0,
            parent: walk.parent,
        })?;
        self.foot_test(Foot {
            start: fixed_point,
            end: vf,
            extent_x: dxf,
            run: Run {
                poly: walk.poly,
                dir: WalkDir::Forward,
                lo: VertexIndex::FIRST,
                hi: VertexIndex::FIRST,
            },
            start_cost: g0,
            parent: walk.parent,
        })?;
        let cost = |dx: i32, dy: i32| {
            FixedDistance::new(if dy.wrapping_abs() < dx.wrapping_abs() {
                dx.wrapping_abs()
            } else {
                dy.wrapping_abs()
            })
            .wrapping_add(g0)
        };
        Ok((cost(dxb, dyb), cost(dxf, dyf)))
    }

    /// The corner at vertex `at` of a new polygon (its side before it at `before`), reached with
    /// cost `fixed_distance` as a walk in direction `dir` over the run `lo`..`hi`: where the goal test does not
    /// end it, the sides differ and the corner emits a token, it becomes a corner node and the
    /// polygon is flagged.
    fn walk_new_corner(
        &mut self,
        walk: &NewWalk,
        poly: &Poly,
        (at, before): (VertexIndex, VertexIndex),
        fixed_distance: FixedDistance,
        (dir, lo, hi): (WalkDir, VertexIndex, VertexIndex),
    ) -> GenieResult<()> {
        let vv = poly.vertex(at)?;
        let sp = poly.side(before)?;
        let sn = poly.side(at)?;
        let sh = poly.shape(at)?;
        let run = Run {
            poly: walk.poly,
            dir,
            lo,
            hi,
        };
        if !self.goal_test(Corner {
            cost: fixed_distance,
            parent: walk.parent,
            run,
            point: vv,
        })? && sp != sn
            && sh.is_some()
            && let Some(token) = corner_emits(sp, sn, vv, self.goal.at)
        {
            let kind = Kind::Corner {
                run: Some(run),
                emit: Some(Emit {
                    token,
                    prev: sp,
                    next: sn,
                }),
            };
            let node = self.add(fixed_distance, walk.parent, vv, kind);
            self.push(node);
            let pm = self.poly_mut(walk.poly)?;
            pm.flag = true;
            *pm.cost_mut(at)? = fixed_distance;
        }
        Ok(())
    }
}
