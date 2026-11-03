//! The polygons' outlines: the tracer, the border walk, the edge that holds a point and the
//! walk of a new polygon.

use crate::GenieError;
use crate::pathing::FixedDistance;
use crate::pathing::FixedRect;
use crate::pathing::mesh::Contact;
use crate::pathing::mesh::Mesh;
use crate::pathing::short_path::axis_dir::AxisDir;
use crate::pathing::short_path::{
    Corner, Emit, FixedPoint, Foot, Kind, NodeIndex, Poly, PolyIndex, Run, Search, TRACE_STEPS,
    UNSET, VertexIndex, WalkDir, cheb, clamp_to_box, contact_point, corner_emits, in_box, inside,
    shrunk_corner,
};
use crate::pathing::{ShapeId, ShapeSide};
use crate::world::slots::CheckedIndex;

/// A vertex the tracer adds to the outline being traced ([`Search::append`]'s arguments after
/// the polygon).
#[derive(Clone, Copy, Debug)]
pub(super) struct Traced {
    /// The vertex.
    pub(super) vertex: FixedPoint,
    /// The outline's first vertex.
    pub(super) first: FixedPoint,
    /// Whether the start side runs vertically.
    pub(super) vertical: bool,
    /// The start side.
    pub(super) start_side: ShapeSide,
    /// The side the vertex is on.
    pub(super) side: ShapeSide,
    /// The shape it lies on, none on the bounds' border.
    pub(super) shape: Option<ShapeId>,
    /// The turns so far (4 a full one).
    pub(super) winding: i32,
}

/// How an outline's trace ended (`FUN_140fa8fa0`'s result).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Outline {
    /// 0: the outline closed.
    Closed,
    /// 1: it ran [`TRACE_STEPS`] steps without closing.
    StepLimit,
}

/// Whether a traced vertex closed the polygon's outline or joined it (`FUN_140fa2fa0`'s 1
/// or 0).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub(super) enum VertexTrace {
    /// The outline closes: the vertex is not added.
    Closes,
    /// The vertex joined the polygon.
    Appended,
}

/// A new polygon's walk: the polygon and the node it comes from.
struct NewWalk {
    poly: PolyIndex,
    parent: NodeIndex,
}

impl Search<'_> {
    /// `FUN_140fa2fa0`: append a vertex to the polygon being traced, unless the outline closes:
    /// after a full turn (`|winding| == 4`) the vertex passes the first one along the start side's
    /// line ([`VertexTrace::Closes`], else [`VertexTrace::Appended`]). On closing, a first
    /// vertex whose side repeats the last one's is dropped.
    pub(super) fn append(poly: &mut Poly, vertex: Traced) -> VertexTrace {
        let Traced {
            vertex: pos,
            first,
            vertical,
            start_side: s0,
            side,
            shape,
            winding,
        } = vertex;
        if let (Some(&last), 4) = (poly.vertices.last(), winding.wrapping_abs()) {
            let closes = if vertical {
                last.x() == first.x()
                    && pos.x() == first.x()
                    && if s0 == ShapeSide::HighX {
                        last.y() <= first.y() && first.y() <= pos.y()
                    } else {
                        first.y() <= last.y() && pos.y() <= first.y()
                    }
            } else {
                last.y() == first.y()
                    && pos.y() == first.y()
                    && if s0 == ShapeSide::LowY {
                        last.x() <= first.x() && first.x() <= pos.x()
                    } else {
                        first.x() <= last.x() && pos.x() <= first.x()
                    }
            };
            if closes {
                if poly.sides.last() == poly.sides.first() {
                    let _ = poly.vertices.remove(0);
                    let _ = poly.sides.remove(0);
                    let _ = poly.shapes.remove(0);
                }
                return VertexTrace::Closes;
            }
        }
        poly.vertices.push(pos);
        poly.sides.push(side);
        poly.shapes.push(shape);
        VertexTrace::Appended
    }

    /// The coordinate of `p` along a side's axis (x for sides 0 and 2, y for 1 and 3).
    pub(super) const fn along(p: FixedPoint, side: ShapeSide) -> i32 {
        if side.along_y() { p.y() } else { p.x() }
    }

    /// `FUN_140fa9630`: the trace left the bounds: walk the bounds' border from `pos` in the turn
    /// the side gives, to the first shape side the sweep meets (0, its shape) or the border's
    /// corner (1, none).
    pub(super) fn border_walk(
        &self,
        pos: &mut FixedPoint,
        first: &mut FixedPoint,
        side: &mut ShapeSide,
        u: &mut i32,
        steps: i32,
    ) -> Result<Option<ShapeId>, GenieError> {
        let b = self.bounds;
        let w = side.prev();
        let (target, dir) = match w {
            ShapeSide::LowY => (FixedPoint::new(b.x0(), b.y1()), AxisDir::PlusY),
            ShapeSide::HighX => (FixedPoint::new(b.x0(), b.y0()), AxisDir::MinusX),
            ShapeSide::HighY => (FixedPoint::new(b.x1(), b.y0()), AxisDir::MinusY),
            ShapeSide::LowX => (FixedPoint::new(b.x1(), b.y1()), AxisDir::PlusX),
        };
        let sw = self.ctx.sweep(*pos, target, dir)?;
        let new_u = Self::along(*pos, w);
        let Some((hid, hs, hp)) = sw.counted()? else {
            if steps == 0 {
                *first = target;
            }
            *u = new_u;
            *side = w;
            *pos = target;
            return Ok(None);
        };
        if steps == 0 {
            *first = hp;
        }
        *u = new_u;
        *side = hs;
        *pos = hp;
        Ok(Some(hid))
    }

    /// `FUN_140fa8fa0`: trace the outline of the cluster of touching shapes from shape `shape0`'s
    /// side `s0` at `start` (counter-clockwise, the shapes on the left): along each side to its
    /// nearest contact ahead (ties to the first in the list) or its end; around the bounds' border
    /// where it leaves them. Closed when the outline closes, the step limit after 2,000 steps.
    pub(super) fn trace(
        &self,
        shape0: ShapeId,
        s0: ShapeSide,
        start: Option<FixedPoint>,
        poly: &mut Poly,
    ) -> Result<Outline, GenieError> {
        let d = self.ctx.d;
        let mesh = self.ctx.mesh;
        let b = self.bounds;
        let vertical = s0.along_y();
        let mut steps: i32 = 0;
        let mut winding: i32 = 0;
        let mut wprev: i32 = 0;
        let mut shape = shape0;
        let mut side = s0;
        let mut pos = match start {
            Some(p) => p,
            None => shrunk_corner(*mesh.shapes.checked(shape.index())?, s0.first_corner(), d),
        };
        let mut u = Self::along(pos, s0);
        let mut first = pos;
        'segment: loop {
            let contacts = mesh.sides.checked(shape.index())?.checked(side.index())?;
            let next_side;
            if let Some(k) = self.nearest_contact(contacts, side, u) {
                let c = *contacts.checked(k)?;
                let q = contact_point(c.at, c.side, d, c.at_side_start);
                pos = q;
                if steps == 0 {
                    first = q;
                }
                if side != c.side {
                    winding -= 1;
                }
                u = Self::along(q, c.side);
                side = c.side;
                shape = c.other;
            } else {
                next_side = side.next();
                let corner = shrunk_corner(
                    *mesh.shapes.checked(shape.index())?,
                    next_side.first_corner(),
                    d,
                );
                if steps == 0 {
                    first = corner;
                }
                // an empty side list measures the old point; it lies on the side's line too
                u = if contacts.is_empty() {
                    Self::along(pos, side.prev())
                } else {
                    Self::along(corner, side.prev())
                };
                pos = corner;
                winding += 1;
                side = next_side;
            }
            // the vertices' shape: the walk's, none on the border's corners
            let mut on = Some(shape);
            let mut wp = wprev;
            loop {
                steps += 1;
                if TRACE_STEPS <= steps {
                    return Ok(Outline::StepLimit);
                }
                let out = !in_box(pos, b);
                if out {
                    side = side.opposite();
                    let clamped = clamp_to_box(pos, b);
                    if pos == first {
                        first = clamped;
                    }
                    pos = clamped;
                }
                loop {
                    if Self::append(
                        poly,
                        Traced {
                            vertex: pos,
                            first,
                            vertical,
                            start_side: s0,
                            side,
                            shape: on,
                            winding: wp,
                        },
                    ) == VertexTrace::Closes
                    {
                        poly.costs = vec![UNSET; poly.vertices.len()];
                        return Ok(Outline::Closed);
                    }
                    if !out {
                        wprev = winding;
                        continue 'segment;
                    }
                    on = self.border_walk(&mut pos, &mut first, &mut side, &mut u, steps)?;
                    if let Some(met) = on {
                        shape = met;
                        break;
                    }
                    wp -= 1;
                    steps += 1;
                    if TRACE_STEPS <= steps {
                        return Ok(Outline::StepLimit);
                    }
                    winding = wp;
                }
                winding = wp - 2;
            }
        }
    }

    /// [`Search::trace`]'s next contact on a side: of the side's contacts (`contacts`), those
    /// wider than twice `d`, not ignored and whose contact point lies in the bounds, the one
    /// nearest ahead of `u` along `side` (each one's run start moved by `d`, back for a flagged
    /// contact on the same side; the sides 2 and 3 measure backwards); ties to the first.
    pub(super) fn nearest_contact(
        &self,
        contacts: &[Contact],
        side: ShapeSide,
        u: i32,
    ) -> Option<usize> {
        let d = self.ctx.d;
        let mut chosen = None;
        let mut best_key = 0x7fc0_0000_i32;
        for (k, c) in contacts.iter().enumerate() {
            if (d >= c.half) || self.ctx.ignored(c.other) {
                continue;
            }
            let q = contact_point(c.at, c.side, d, c.at_side_start);
            if !in_box(q, self.bounds) {
                continue;
            }
            // the sides 0 and 1 measure forward (the game's `side < 2`, and any side past 3)
            let forward = matches!(side, ShapeSide::LowY | ShapeSide::HighX);
            let off = if side == c.side && c.at_side_start {
                d.get().wrapping_neg()
            } else {
                d.get()
            };
            let mut key = if forward {
                c.along.wrapping_add(off)
            } else {
                c.along.wrapping_sub(off)
            };
            key = key.wrapping_sub(u);
            if !forward {
                key = key.wrapping_neg();
            }
            if -1 < key && key < best_key {
                best_key = key;
                chosen = Some(k);
            }
        }
        chosen
    }

    /// `FUN_140fa77b0`: the polygon's edge that holds `p` going along `side` (the start of the
    /// edge's run), or the vertex `p` is when it lies in the overlap of its shape with `shape`;
    /// None if none.
    pub(super) fn edge_holding(
        poly: &Poly,
        p: FixedPoint,
        side: ShapeSide,
        mesh: &Mesh,
        shape: Option<ShapeId>,
        d: FixedDistance,
    ) -> Option<VertexIndex> {
        let shape = shape?;
        let n = poly.vertices.len() as u32;
        if n == 0 {
            return None;
        }
        let v = &poly.vertices;
        let sides = &poly.sides;
        let mut limit = n;
        let mut prev = *sides.get(n.wrapping_sub(2) as usize % sides.len())?;
        let mut cur = n - 1;
        let mut i: u32 = 0;
        loop {
            let cur_side = *sides.get(cur as usize)?;
            let mut side_i = *sides.get(i as usize)?;
            let mut nxt = i;
            if side_i == cur_side {
                loop {
                    i = (i + 1) % n;
                    side_i = *sides.get(i as usize)?;
                    if side_i != cur_side {
                        break;
                    }
                }
                nxt = i;
                if i < cur && cur != n - 1 {
                    limit = 0;
                }
            }
            let vc = *v.get(cur as usize)?;
            if Self::vertex_in_overlap(mesh, (p, vc, *poly.shapes.get(cur as usize)?), shape, d)
                || (side == cur_side
                    && Self::on_edge(cur_side, (vc, *v.get(nxt as usize)?), p, (prev, side_i)))
            {
                return Some(VertexIndex::new(cur));
            }
            i = nxt + 1;
            prev = cur_side;
            cur = nxt;
            if i >= limit {
                return None;
            }
        }
    }

    /// `FUN_140fa77b0`'s vertex test: the vertex `vc` of shape `vs` is `p`, and `p` lies in the
    /// overlap of that shape with `shape` (the higher low corner, the lower high one), shrunk by
    /// `d` on every side.
    pub(super) fn vertex_in_overlap(
        mesh: &Mesh,
        (p, vc, vs): (FixedPoint, FixedPoint, Option<ShapeId>),
        shape: ShapeId,
        d: FixedDistance,
    ) -> bool {
        let d = d.get();
        let Some(vs) = vs.filter(|_| p == vc) else {
            return false;
        };
        let (Some(&sr), Some(&vr)) = (mesh.shapes.get(shape.index()), mesh.shapes.get(vs.index()))
        else {
            return false;
        };
        let y1 = if sr.y1() - d < vr.y1() - d {
            sr.y1() - d
        } else {
            vr.y1() - d
        };
        let x1 = if sr.x1() - d < vr.x1() - d {
            sr.x1() - d
        } else {
            vr.x1() - d
        };
        let y0 = if vr.y0() + d < sr.y0() + d {
            sr.y0()
        } else {
            vr.y0()
        };
        let x0 = if vr.x0() + d < sr.x0() + d {
            sr.x0()
        } else {
            vr.x0()
        };
        let o = FixedRect::new(x0 + d, y0 + d, x1, y1);
        o.x0() < o.x1() && o.y0() < o.y1() && inside(o, p, FixedDistance::ZERO)
    }

    /// `FUN_140fa77b0`'s edge test: `p` lies on the edge of side `cur_side` from `vc` to `vn`; the
    /// side before the edge's run (`prev`) and the one after it (`next`) decide whether each end
    /// counts.
    pub(super) const fn on_edge(
        cur_side: ShapeSide,
        (vc, vn): (FixedPoint, FixedPoint),
        p: FixedPoint,
        (prev, next): (ShapeSide, ShapeSide),
    ) -> bool {
        use ShapeSide::{HighX, HighY, LowX, LowY};
        // the game's unsigned compares of the side numbers: `prev < 2`, `prev - 1 < 2` and the
        // like name two neighbouring sides
        match cur_side {
            LowY if vc.y() == p.y() => {
                let (a, b) = (vc.x(), vn.x());
                let start = if matches!(prev, LowY | HighX) {
                    a <= p.x()
                } else {
                    a < p.x()
                };
                start
                    && if matches!(next, LowX | LowY) {
                        p.x() <= b
                    } else {
                        p.x() < b
                    }
            }
            HighX if vc.x() == p.x() => {
                let (a, b) = (vc.y(), vn.y());
                let start = if matches!(prev, HighX | HighY) {
                    a <= p.y()
                } else {
                    a < p.y()
                };
                start
                    && if matches!(next, LowY | HighX) {
                        p.y() <= b
                    } else {
                        p.y() < b
                    }
            }
            HighY if vc.y() == p.y() => {
                let (a, b) = (vc.x(), vn.x());
                let start = if matches!(next, HighX | HighY) {
                    b <= p.x()
                } else {
                    b < p.x()
                };
                start
                    && if matches!(prev, HighY | LowX) {
                        p.x() <= a
                    } else {
                        p.x() < a
                    }
            }
            LowX if vc.x() == p.x() => {
                let (a, b) = (vc.y(), vn.y());
                let start = if matches!(next, HighY | LowX) {
                    b <= p.y()
                } else {
                    b < p.y()
                };
                start
                    && if matches!(prev, LowY | LowX) {
                        p.y() <= a
                    } else {
                        p.y() < a
                    }
            }
            _ => false,
        }
    }

    /// `FUN_140fa7b50`: from `p` on a new polygon's closing edge, walk the outline both ways at
    /// once, the cheaper side first (ties forward): each corner the goal test does not end
    /// becomes a corner node toward the goal when it emits; each edge gets its foot test.
    pub(super) fn walk_new(
        &mut self,
        pi: PolyIndex,
        p: FixedPoint,
        g0: FixedDistance,
        parent: NodeIndex,
    ) -> Result<(), GenieError> {
        let poly = self.poly(pi)?.clone();
        let n = poly.count();
        let walk = NewWalk { poly: pi, parent };
        let (mut cb, mut cf) = self.walk_new_start(&walk, &poly, p, g0)?;
        let mut bi = VertexIndex::new(n - 1);
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
                let w = poly.vertex(bi.pred())?;
                let dx = vv.x() - w.x();
                self.foot_test(Foot {
                    start: vv,
                    end: w,
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
                cb = cb.wrapping_add(cheb(vv, w));
                bi = bi.pred();
            } else {
                let vv = poly.vertex(fi)?;
                self.walk_new_corner(
                    &walk,
                    &poly,
                    (fi, fi.prev_in(n)),
                    cf,
                    (WalkDir::Forward, VertexIndex::FIRST, fi),
                )?;
                fi = fi.succ();
                let w = poly.vertex(fi)?;
                let dx = vv.x() - w.x();
                self.foot_test(Foot {
                    start: vv,
                    end: w,
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
                cf = cf.wrapping_add(cheb(vv, w));
            }
            if fi >= bi {
                return Ok(());
            }
        }
    }

    /// The walk of a new polygon's start: the foot tests of the two edges from `p` to the
    /// closing vertices, and the costs of the two walks (the larger axis distance on top of
    /// `g0`), backward then forward.
    fn walk_new_start(
        &mut self,
        walk: &NewWalk,
        poly: &Poly,
        p: FixedPoint,
        g0: FixedDistance,
    ) -> Result<(FixedDistance, FixedDistance), GenieError> {
        let last = VertexIndex::new(poly.count() - 1);
        let (vb, vf) = (poly.vertex(last)?, poly.vertex(VertexIndex::FIRST)?);
        let (dxb, dyb) = (vb.x() - p.x(), vb.y() - p.y());
        let (dxf, dyf) = (vf.x() - p.x(), vf.y() - p.y());
        self.foot_test(Foot {
            start: p,
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
            start: p,
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
    /// cost `g` as a walk in direction `dir` over the run `lo`..`hi`: where the goal test does not
    /// end it, the sides differ and the corner emits a token, it becomes a corner node and the
    /// polygon is flagged.
    fn walk_new_corner(
        &mut self,
        walk: &NewWalk,
        poly: &Poly,
        (at, before): (VertexIndex, VertexIndex),
        g: FixedDistance,
        (dir, lo, hi): (WalkDir, VertexIndex, VertexIndex),
    ) -> Result<(), GenieError> {
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
            cost: g,
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
            let node = self.add(g, walk.parent, vv, kind);
            self.push(node);
            let pm = self.poly_mut(walk.poly)?;
            pm.flag = true;
            *pm.cost_mut(at)? = g;
        }
        Ok(())
    }
}
