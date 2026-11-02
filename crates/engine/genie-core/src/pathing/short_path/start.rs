//! The start's node, the hits' points and the two legs of an L.

use crate::math::conversions::cvttss2si_64;
use crate::pathing::mesh::ONE;
use crate::pathing::short_path::axis_dir::AxisDir;
use crate::pathing::short_path::{
    FixedPoint, Hit, Kind, Node, NodeIndex, RayFraction, Search, TO_FIXED, TO_FLOAT, contact_point,
    in_box, shrunk_corner, snap_to_side, strictly_inside, through_float, tile_of,
};
use crate::pathing::{FixedDistance, FixedDistanceSquared};
use crate::pathing::{ShapeId, ShapeSide};
use crate::world::slots::CheckedIndex;
use crate::{GenieError, math::fmath};

/// The corners of a side of a shape as the start's scan walks it ([`shrunk_corner`]'s
/// numbers): side 0 from corner 0 to 1, side 1 from 1 to 2, side 2 from 3 to 2, side 3 from 0
/// to 3.
const fn side_corners(side: ShapeSide) -> (u32, u32) {
    match side {
        ShapeSide::LowY => (0, 1),
        ShapeSide::HighX => (1, 2),
        ShapeSide::HighY => (3, 2),
        ShapeSide::LowX => (0, 3),
    }
}

/// A side's direction from its first corner to its second ([`side_corners`]): one tile along x
/// or y.
const fn side_dir(side: ShapeSide) -> FixedPoint {
    if side.along_y() {
        FixedPoint::new(0, ONE)
    } else {
        FixedPoint::new(ONE, 0)
    }
}

/// The distance from `p` to `a` in fixed point, through floats as the game takes it.
fn float_distance(p: FixedPoint, a: FixedPoint) -> FixedDistance {
    let [px, py] = p.get();
    let dy = (py.wrapping_sub(a.y())) as f32 * TO_FLOAT;
    let dx = (px.wrapping_sub(a.x())) as f32 * TO_FLOAT;
    FixedDistance::new((fmath::sqrt(dx * dx + dy * dy) * TO_FIXED) as i32)
}

/// What the search's start found (`FUN_140fa4410`'s result).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Started {
    /// 0: the start is free; its node is open.
    Free,
    /// 1: a shape holds the start; the cheapest of its escape candidates stays open.
    Escaped,
    /// 2: a shape holds the start, with fewer than two escape candidates.
    Stuck,
    /// 3: the start lies within range already.
    InRange,
}

impl Search<'_> {
    /// `FUN_140fa4410`: the start's node (type 0, cost 0, the Euclidean estimate); in range when
    /// the start already lies within range, free when no shape holds it (the node opened), else
    /// the escape candidates (type 2) around it within twice the radius: stuck for fewer than
    /// two, else escaped with only the cheapest kept open.
    pub(super) fn start(&mut self, p: FixedPoint) -> Result<Started, GenieError> {
        let g = &self.goal;
        if g.range2 >= g.rect_distance2(p) {
            return Ok(Started::InRange);
        }
        let fx = (g.at.x().wrapping_sub(p.x())) as f32 * TO_FLOAT;
        let fy = (g.at.y().wrapping_sub(p.y())) as f32 * TO_FLOAT;
        // CVTTSS2SI RAX at 0x140fa4558, EAX stored: a start 512 tiles or more away wraps
        let f = cvttss2si_64(fmath::sqrt(fx * fx + fy * fy) * TO_FIXED) as i32;
        let start = Node {
            g: FixedDistance::ZERO,
            f: FixedDistance::new(f),
            idx: NodeIndex::next_of(&self.arena),
            parent: None,
            at: p,
            kind: Kind::Start,
        };
        self.arena.push(start);
        if self.ctx.holder(p)?.is_none() {
            self.push(start);
            return Ok(Started::Free);
        }
        self.escape_candidates(p, start.idx)?;
        if self.open.len() < 2 {
            return Ok(Started::Stuck);
        }
        let keep = self.open.len() - 1;
        let _ = self.open.drain(..keep);
        let n = self.open.checked_mut(0)?;
        let f = self.goal.heuristic(n.at).wrapping_add(n.g);
        n.f = f;
        let idx = n.idx;
        self.node_mut(idx)?.f = f;
        Ok(Started::Escaped)
    }

    /// `FUN_140fa4410`'s scan around a held start `p`: every side of every shape not ignored
    /// over the tiles under the corners of the box twice the radius out (each tile once, the map's
    /// only) gives its escape candidates ([`Search::escape_side`]), children of node `root`.
    pub(super) fn escape_candidates(
        &mut self,
        p: FixedPoint,
        root: NodeIndex,
    ) -> Result<(), GenieError> {
        let mesh = self.ctx.mesh;
        let w = mesh.width as i32;
        let total = mesh.width * mesh.width;
        let rr = ((i64::from(self.goal.r.get()) << 23) >> 22) as i32;
        // squared in 64 bits, its low 32 kept
        let rr2 =
            FixedDistanceSquared::new(i64::from(((i64::from(rr) * i64::from(rr)) >> 22) as i32));
        let [px, py] = p.get();
        let tile = |x: i32, y: i32| (tile_of(y).wrapping_mul(w)).wrapping_add(tile_of(x)) as u32;
        let tiles = [
            tile(px - rr, py - rr),
            tile(px + rr, py - rr),
            tile(px + rr, py + rr),
            tile(px - rr, py + rr),
        ];
        for ti in 0..4 {
            let here = *tiles.checked(ti)?;
            if tiles.head(ti)?.contains(&here) || total <= here {
                continue;
            }
            let list = mesh.tiles.checked(here as usize)?.clone();
            for &id in &list {
                if self.ctx.ignored(id) {
                    continue;
                }
                for side in ShapeSide::ALL {
                    self.escape_side(p, root, (id, side), rr2)?;
                }
            }
        }
        Ok(())
    }

    /// Side `side` of shape `id`, shrunk by the search's `d`, when its line comes within `rr2` (the
    /// doubled radius, squared) of the start `p`: the points where that circle crosses the side,
    /// the start's foot on it ([`Search::escape_foot`]), its first corner within the circle and
    /// its contacts ([`Search::escape_contacts`]) become escape candidates, each in the search's
    /// box and held by no other shape.
    pub(super) fn escape_side(
        &mut self,
        p: FixedPoint,
        root: NodeIndex,
        (id, side): (ShapeId, ShapeSide),
        rr2: FixedDistanceSquared,
    ) -> Result<(), GenieError> {
        let d = self.ctx.d;
        let r = *self.ctx.mesh.shapes.checked(id.index())?;
        let [px, py] = p.get();
        let (first, second) = side_corners(side);
        let a = shrunk_corner(r, first, d);
        let e = shrunk_corner(r, second, d);
        let dir = side_dir(side);
        let (t, dist2, span) = if side.along_y() {
            let u = i64::from(dir.y());
            let t = ((i64::from(py - a.y()) * u) >> 22) as i32;
            let perp = i64::from((py - a.y()) - (((i64::from(t) * u) >> 22) as i32));
            let dx = i64::from(px - a.x());
            let dist2 = FixedDistanceSquared::new(((perp * perp) >> 22) + ((dx * dx) >> 22));
            (t, dist2, e.y() - a.y())
        } else {
            let u = i64::from(dir.x());
            let t = ((i64::from(px - a.x()) * u) >> 22) as i32;
            let perp = i64::from((px - a.x()) - (((i64::from(t) * u) >> 22) as i32));
            let dy = i64::from(py - a.y());
            let dist2 = FixedDistanceSquared::new(((dy * dy) >> 22) + ((perp * perp) >> 22));
            (t, dist2, e.x() - a.x())
        };
        if dist2 > rr2 {
            return Ok(());
        }
        let b = self.bounds;
        let h = (fmath::sqrt((rr2.get() - dist2.get()) as f32 * TO_FLOAT) * TO_FIXED) as i32;
        for c in [t - h, h + t] {
            if 0 < c && c < span {
                let q = FixedPoint::new(
                    ((i64::from(dir.x()) * i64::from(c)) >> 22) as i32 + a.x(),
                    ((i64::from(dir.y()) * i64::from(c)) >> 22) as i32 + a.y(),
                );
                if in_box(q, b) && self.ctx.holder_but(q, id, d)?.is_none() {
                    self.side_node(float_distance(p, q), root, id, side, q);
                }
            }
        }
        self.escape_foot(p, root, (id, side), (a, e))?;
        // the side's first corner
        if in_box(a, b) && self.ctx.holder_but(a, id, d)?.is_none() {
            let dx = i64::from(px - a.x());
            let dy = i64::from(py - a.y());
            let d2 = FixedDistanceSquared::new(((dx * dx) >> 22) + ((dy * dy) >> 22));
            if d2 < rr2 {
                let g =
                    FixedDistance::new((fmath::sqrt(d2.get() as f32 * TO_FLOAT) * TO_FIXED) as i32);
                self.side_node(g, root, id, side, a);
            }
        }
        self.escape_contacts(p, root, (id, side), rr2)?;
        Ok(())
    }

    /// The start `p`'s foot on side `side` of shape `id` (its corners `a` and `e`), strictly
    /// between them, in the search's box and held by no shape: an escape candidate at the
    /// perpendicular distance.
    pub(super) fn escape_foot(
        &mut self,
        p: FixedPoint,
        root: NodeIndex,
        (id, side): (ShapeId, ShapeSide),
        (a, e): (FixedPoint, FixedPoint),
    ) -> Result<(), GenieError> {
        let [px, py] = p.get();
        let b = self.bounds;
        if !side.along_y() {
            if a.x() < px && px < e.x() {
                let q = p.with_y(a.y());
                if in_box(q, b) && self.ctx.holder(q)?.is_none() {
                    self.side_node(
                        FixedDistance::new((py - a.y()).wrapping_abs()),
                        root,
                        id,
                        side,
                        q,
                    );
                }
            }
        } else if a.y() < py && py < e.y() {
            let q = p.with_x(a.x());
            if in_box(q, b) && self.ctx.holder(q)?.is_none() {
                self.side_node(
                    FixedDistance::new((a.x() - px).wrapping_abs()),
                    root,
                    id,
                    side,
                    q,
                );
            }
        }
        Ok(())
    }

    /// The contacts on side `side` of shape `id`: each wider than the search's `d`, with a shape
    /// not ignored, at a point in the search's box and held by no shape (a contact on a side of
    /// its own number only when its other shape holds the point nudged one unit into it), within
    /// `rr2` of the start `p`: an escape candidate, 0x32 dearer on a side of its own number.
    pub(super) fn escape_contacts(
        &mut self,
        p: FixedPoint,
        root: NodeIndex,
        (id, side): (ShapeId, ShapeSide),
        rr2: FixedDistanceSquared,
    ) -> Result<(), GenieError> {
        let [px, py] = p.get();
        let mesh = self.ctx.mesh;
        let d = self.ctx.d;
        let b = self.bounds;
        for &c in mesh.sides.checked(id.index())?.checked(side.index())? {
            if (d >= c.half) || self.ctx.ignored(c.other) {
                continue;
            }
            let q = contact_point(c.at, c.side, d, c.at_side_start);
            if !in_box(q, b) {
                continue;
            }
            if c.side == side {
                let nudged = match side {
                    ShapeSide::LowY => q.with_y(q.y() + 1),
                    ShapeSide::HighX => q.with_x(q.x() + 1),
                    ShapeSide::HighY => q.with_y(q.y() - 1),
                    ShapeSide::LowX => q.with_x(q.x() - 1),
                };
                if !strictly_inside(*mesh.shapes.checked(c.other.index())?, nudged, d) {
                    continue;
                }
            }
            if self.ctx.holder(q)?.is_some() {
                continue;
            }
            let dx = i64::from(px - q.x());
            let dy = i64::from(py - q.y());
            let d2 = FixedDistanceSquared::new(((dy * dy) >> 22) + ((dx * dx) >> 22));
            if d2 < rr2 {
                let g0 = (fmath::sqrt(d2.get() as f32 * TO_FLOAT) * TO_FIXED) as i32;
                let g = FixedDistance::new(if c.side == side { g0 + 0x32 } else { g0 });
                self.side_node(g, root, id, side, q);
            }
        }
        Ok(())
    }

    /// The point where the walk from `p` toward `t` meets the hit at fraction `frac` (`>> 22`).
    pub(super) fn at_fraction(p: FixedPoint, t: FixedPoint, frac: RayFraction) -> FixedPoint {
        let f = i64::from(frac.get());
        FixedPoint::new(
            ((i64::from(t.x().wrapping_sub(p.x())) * f) >> 22) as i32 + p.x(),
            ((i64::from(t.y().wrapping_sub(p.y())) * f) >> 22) as i32 + p.y(),
        )
    }

    /// A hit point snapped onto the hit shape's side, then onto that side of every shape of its
    /// tile that still holds it, until none does (`LAB_140fa7102`, `LAB_140fa5f50`, unbounded as
    /// the game's). It ends: a holder holds the point strictly inside, so the snap keeps the other
    /// coordinate and moves this one past the holder's side, always the same way (side 0 and 3
    /// lower, 1 and 2 higher); no shape holds it again, and the mesh has finitely many. Returns
    /// the shape the point settled on.
    pub(super) fn settle(&self, h: &mut FixedPoint, hit: Hit) -> Result<ShapeId, GenieError> {
        let d = self.ctx.d;
        let mut shape = hit.shape;
        snap_to_side(
            h,
            *self.ctx.mesh.shapes.checked(shape.index())?,
            hit.side,
            d,
        );
        while let Some(id) = self.ctx.holder(*h)? {
            snap_to_side(h, *self.ctx.mesh.shapes.checked(id.index())?, hit.side, d);
            shape = id;
        }
        Ok(shape)
    }

    /// `FUN_140fa2b20`: the two legs of an L toward `c` from node `b`, the first along direction
    /// `a`, the second along `second`; each leg's cost is |Δx|, then |Δy|, of `delta` through float.
    pub(super) fn legs(
        &mut self,
        b: Node,
        c: FixedPoint,
        a: AxisDir,
        second: AxisDir,
        delta: FixedPoint,
    ) -> Result<(), GenieError> {
        let p = b.at;
        let m = if a.along_x() {
            FixedPoint::new(c.x(), p.y())
        } else {
            FixedPoint::new(p.x(), c.y())
        };
        let sw = self.ctx.sweep(p, m, a)?;
        let (g, parent, sw) = if sw.found {
            (
                through_float(delta.x().wrapping_abs()).wrapping_add(b.g),
                b.idx,
                sw,
            )
        } else {
            let mid = self.add(
                through_float(delta.x().wrapping_abs()).wrapping_add(b.g),
                b.idx,
                m,
                Kind::Waypoint,
            );
            let sw2 = self.ctx.sweep(m, c, second)?;
            (
                through_float(delta.y().wrapping_abs()).wrapping_add(mid.g),
                mid.idx,
                sw2,
            )
        };
        let (Some(shape), Some(side), Some(at)) = (sw.shape, sw.side, sw.point) else {
            return Err(GenieError::unsupported(
                "an L toward a shape's centre that meets nothing (stack garbage)",
            ));
        };
        self.side_node(g, parent, shape, side, at);
        Ok(())
    }
}
