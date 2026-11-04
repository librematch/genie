//! The start's node, the hits' points and the two legs of an L.

use crate::math::conversions::cvttss2si_64;
use crate::math::fmath::sqrt;
use crate::pathing::mesh::ONE;
use crate::pathing::short_path::axis_dir::AxisDir;
use crate::pathing::short_path::{
    FixedPoint, Hit, Kind, Node, NodeIndex, RayFraction, Search, TO_FIXED, TO_FLOAT, contact_point,
    in_box, shrunk_corner, snap_to_side, strictly_inside, through_float, tile_of,
};
use crate::pathing::{FixedDistance, FixedDistanceSquared};
use crate::pathing::{ShapeId, ShapeSide};
use crate::world::slots::CheckedIndexExt;
use crate::{GenieError, GenieResult};

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

/// The distance from `from` to `to` in fixed point, through floats as the game takes it.
fn float_distance(from: FixedPoint, to: FixedPoint) -> FixedDistance {
    let [px, py] = from.get();
    let dy = (py.wrapping_sub(to.y())) as f32 * TO_FLOAT;
    let dx = (px.wrapping_sub(to.x())) as f32 * TO_FLOAT;
    FixedDistance::new((sqrt(dx * dx + dy * dy) * TO_FIXED) as i32)
}

/// What the search's start found (`FUN_140fa4410`'s result).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
    pub(super) fn start(&mut self, fixed_point: FixedPoint) -> GenieResult<Started> {
        let goal = &self.goal;
        if goal.range2 >= goal.rect_distance2(fixed_point) {
            return Ok(Started::InRange);
        }
        let fx = (goal.at.x().wrapping_sub(fixed_point.x())) as f32 * TO_FLOAT;
        let fy = (goal.at.y().wrapping_sub(fixed_point.y())) as f32 * TO_FLOAT;
        // CVTTSS2SI RAX at 0x140fa4558, EAX stored: a start 512 tiles or more away wraps
        let estimate = cvttss2si_64(sqrt(fx * fx + fy * fy) * TO_FIXED) as i32;
        let start = Node {
            cost: FixedDistance::ZERO,
            total: FixedDistance::new(estimate),
            idx: NodeIndex::next_of(&self.arena),
            parent: None,
            at: fixed_point,
            kind: Kind::Start,
        };
        self.arena.push(start);
        if self.ctx.holder(fixed_point)?.is_none() {
            self.push(start);
            return Ok(Started::Free);
        }
        self.escape_candidates(fixed_point, start.idx)?;
        if self.open.len() < 2 {
            return Ok(Started::Stuck);
        }
        let keep = self.open.len() - 1;
        let _ = self.open.drain(..keep);
        let node = self.open.checked_mut(0)?;
        let fixed_distance = self.goal.heuristic(node.at).wrapping_add(node.cost);
        node.total = fixed_distance;
        let idx = node.idx;
        self.node_mut(idx)?.total = fixed_distance;
        Ok(Started::Escaped)
    }

    /// `FUN_140fa4410`'s scan around a held start `fixed_point`: every side of every shape not
    /// ignored over the tiles under the corners of the box twice the radius out (each tile once,
    /// the map's only) gives its escape candidates ([`Search::escape_side`]), children of node
    /// `root`.
    pub(super) fn escape_candidates(
        &mut self,
        fixed_point: FixedPoint,
        root: NodeIndex,
    ) -> GenieResult<()> {
        let mesh = self.ctx.mesh;
        let width = mesh.width as i32;
        let total = mesh.width * mesh.width;
        let rr = ((i64::from(self.goal.radius.get()) << 23) >> 22) as i32;
        // squared in 64 bits, its low 32 kept
        let rr2 =
            FixedDistanceSquared::new(i64::from(((i64::from(rr) * i64::from(rr)) >> 22) as i32));
        let [px, py] = fixed_point.get();
        let tile =
            |x: i32, y: i32| (tile_of(y).wrapping_mul(width)).wrapping_add(tile_of(x)) as u32;
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
                    self.escape_side(fixed_point, root, (id, side), rr2)?;
                }
            }
        }
        Ok(())
    }

    /// Side `side` of shape `id`, shrunk by the search's `clearance`, when its line comes within
    /// `rr2` (the doubled radius, squared) of the start `point`: the points where that circle
    /// crosses the side, the start's foot on it ([`Search::escape_foot`]), its first corner within
    /// the circle and its contacts ([`Search::escape_contacts`]) become escape candidates, each in
    /// the search's box and held by no other shape.
    pub(super) fn escape_side(
        &mut self,
        point: FixedPoint,
        root: NodeIndex,
        (id, side): (ShapeId, ShapeSide),
        rr2: FixedDistanceSquared,
    ) -> GenieResult<()> {
        let clearance = self.ctx.clearance;
        let checked = *self.ctx.mesh.shapes.checked(id.index())?;
        let [px, py] = point.get();
        let (first, second) = side_corners(side);
        let corner = shrunk_corner(checked, first, clearance);
        let shrunk_corner = shrunk_corner(checked, second, clearance);
        let dir = side_dir(side);
        let (offset, dist2, span) = if side.along_y() {
            let component = i64::from(dir.y());
            let along = ((i64::from(py - corner.y()) * component) >> 22) as i32;
            let perp =
                i64::from((py - corner.y()) - (((i64::from(along) * component) >> 22) as i32));
            let dx = i64::from(px - corner.x());
            let dist2 = FixedDistanceSquared::new(((perp * perp) >> 22) + ((dx * dx) >> 22));
            (along, dist2, shrunk_corner.y() - corner.y())
        } else {
            let component = i64::from(dir.x());
            let along = ((i64::from(px - corner.x()) * component) >> 22) as i32;
            let perp =
                i64::from((px - corner.x()) - (((i64::from(along) * component) >> 22) as i32));
            let dy = i64::from(py - corner.y());
            let dist2 = FixedDistanceSquared::new(((dy * dy) >> 22) + ((perp * perp) >> 22));
            (along, dist2, shrunk_corner.x() - corner.x())
        };
        if dist2 > rr2 {
            return Ok(());
        }
        let fixed_rect = self.bounds;
        let half_chord = (sqrt((rr2.get() - dist2.get()) as f32 * TO_FLOAT) * TO_FIXED) as i32;
        for crossing in [offset - half_chord, half_chord + offset] {
            if 0 < crossing && crossing < span {
                let fixed_point = FixedPoint::new(
                    ((i64::from(dir.x()) * i64::from(crossing)) >> 22) as i32 + corner.x(),
                    ((i64::from(dir.y()) * i64::from(crossing)) >> 22) as i32 + corner.y(),
                );
                if in_box(fixed_point, fixed_rect)
                    && self.ctx.holder_but(fixed_point, id, clearance)?.is_none()
                {
                    self.side_node(
                        float_distance(point, fixed_point),
                        root,
                        id,
                        side,
                        fixed_point,
                    );
                }
            }
        }
        self.escape_foot(point, root, (id, side), (corner, shrunk_corner))?;
        // the side's first corner
        if in_box(corner, fixed_rect) && self.ctx.holder_but(corner, id, clearance)?.is_none() {
            let dx = i64::from(px - corner.x());
            let dy = i64::from(py - corner.y());
            let d2 = FixedDistanceSquared::new(((dx * dx) >> 22) + ((dy * dy) >> 22));
            if d2 < rr2 {
                let fixed_distance =
                    FixedDistance::new((sqrt(d2.get() as f32 * TO_FLOAT) * TO_FIXED) as i32);
                self.side_node(fixed_distance, root, id, side, corner);
            }
        }
        self.escape_contacts(point, root, (id, side), rr2)?;
        Ok(())
    }

    /// The start `point`'s foot on side `side` of shape `id` (its corners `start` and `end`),
    /// strictly between them, in the search's box and held by no shape: an escape candidate at the
    /// perpendicular distance.
    pub(super) fn escape_foot(
        &mut self,
        point: FixedPoint,
        root: NodeIndex,
        (id, side): (ShapeId, ShapeSide),
        (start, end): (FixedPoint, FixedPoint),
    ) -> GenieResult<()> {
        let [px, py] = point.get();
        let fixed_rect = self.bounds;
        if !side.along_y() {
            if start.x() < px && px < end.x() {
                let fixed_point = point.with_y(start.y());
                if in_box(fixed_point, fixed_rect) && self.ctx.holder(fixed_point)?.is_none() {
                    self.side_node(
                        FixedDistance::new((py - start.y()).wrapping_abs()),
                        root,
                        id,
                        side,
                        fixed_point,
                    );
                }
            }
        } else if start.y() < py && py < end.y() {
            let fixed_point = point.with_x(start.x());
            if in_box(fixed_point, fixed_rect) && self.ctx.holder(fixed_point)?.is_none() {
                self.side_node(
                    FixedDistance::new((start.x() - px).wrapping_abs()),
                    root,
                    id,
                    side,
                    fixed_point,
                );
            }
        }
        Ok(())
    }

    /// The contacts on side `side` of shape `id`: each wider than the search's `clearance`, with a
    /// shape not ignored, at a point in the search's box and held by no shape (a contact on a side
    /// of its own number only when its other shape holds the point nudged one unit into it), within
    /// `rr2` of the start `point`: an escape candidate, 0x32 dearer on a side of its own number.
    pub(super) fn escape_contacts(
        &mut self,
        point: FixedPoint,
        root: NodeIndex,
        (id, side): (ShapeId, ShapeSide),
        rr2: FixedDistanceSquared,
    ) -> GenieResult<()> {
        let [px, py] = point.get();
        let mesh = self.ctx.mesh;
        let clearance = self.ctx.clearance;
        let fixed_rect = self.bounds;
        for &contact in mesh.sides.checked(id.index())?.checked(side.index())? {
            if (clearance >= contact.half) || self.ctx.ignored(contact.other) {
                continue;
            }
            let fixed_point =
                contact_point(contact.at, contact.side, clearance, contact.at_side_start);
            if !in_box(fixed_point, fixed_rect) {
                continue;
            }
            if contact.side == side {
                let nudged = match side {
                    ShapeSide::LowY => fixed_point.with_y(fixed_point.y() + 1),
                    ShapeSide::HighX => fixed_point.with_x(fixed_point.x() + 1),
                    ShapeSide::HighY => fixed_point.with_y(fixed_point.y() - 1),
                    ShapeSide::LowX => fixed_point.with_x(fixed_point.x() - 1),
                };
                if !strictly_inside(
                    *mesh.shapes.checked(contact.other.index())?,
                    nudged,
                    clearance,
                ) {
                    continue;
                }
            }
            if self.ctx.holder(fixed_point)?.is_some() {
                continue;
            }
            let dx = i64::from(px - fixed_point.x());
            let dy = i64::from(py - fixed_point.y());
            let d2 = FixedDistanceSquared::new(((dy * dy) >> 22) + ((dx * dx) >> 22));
            if d2 < rr2 {
                let g0 = (sqrt(d2.get() as f32 * TO_FLOAT) * TO_FIXED) as i32;
                let fixed_distance =
                    FixedDistance::new(if contact.side == side { g0 + 0x32 } else { g0 });
                self.side_node(fixed_distance, root, id, side, fixed_point);
            }
        }
        Ok(())
    }

    /// The point where the walk from `fixed_point` toward `to` meets the hit at fraction `frac`
    /// (`>> 22`).
    pub(super) fn at_fraction(
        fixed_point: FixedPoint,
        to: FixedPoint,
        frac: RayFraction,
    ) -> FixedPoint {
        let scale = i64::from(frac.get());
        FixedPoint::new(
            ((i64::from(to.x().wrapping_sub(fixed_point.x())) * scale) >> 22) as i32
                + fixed_point.x(),
            ((i64::from(to.y().wrapping_sub(fixed_point.y())) * scale) >> 22) as i32
                + fixed_point.y(),
        )
    }

    /// A hit point snapped onto the hit shape's side, then onto that side of every shape of its
    /// tile that still holds it, until none does (`LAB_140fa7102`, `LAB_140fa5f50`, unbounded as
    /// the game's). It ends: a holder holds the point strictly inside, so the snap keeps the other
    /// coordinate and moves this one past the holder's side, always the same way (side 0 and 3
    /// lower, 1 and 2 higher); no shape holds it again, and the mesh has finitely many. Returns
    /// the shape the point settled on.
    pub(super) fn settle(&self, fixed_point: &mut FixedPoint, hit: Hit) -> GenieResult<ShapeId> {
        let fixed_distance = self.ctx.clearance;
        let mut shape = hit.shape;
        snap_to_side(
            fixed_point,
            *self.ctx.mesh.shapes.checked(shape.index())?,
            hit.side,
            fixed_distance,
        );
        while let Some(id) = self.ctx.holder(*fixed_point)? {
            snap_to_side(
                fixed_point,
                *self.ctx.mesh.shapes.checked(id.index())?,
                hit.side,
                fixed_distance,
            );
            shape = id;
        }
        Ok(shape)
    }

    /// `FUN_140fa2b20`: the two legs of an L toward `corner` from node `node`, the first along
    /// direction `axis_dir`, the second along `second`; each leg's cost is |Δx|, then |Δy|, of
    /// `delta` through float.
    pub(super) fn legs(
        &mut self,
        node: Node,
        corner: FixedPoint,
        axis_dir: AxisDir,
        second: AxisDir,
        delta: FixedPoint,
    ) -> GenieResult<()> {
        let node_at = node.at;
        let fixed_point = if axis_dir.along_x() {
            FixedPoint::new(corner.x(), node_at.y())
        } else {
            FixedPoint::new(node_at.x(), corner.y())
        };
        let sw = self.ctx.sweep(node_at, fixed_point, axis_dir)?;
        let (fixed_distance, parent, sw) = if sw.found {
            (
                through_float(delta.x().wrapping_abs()).wrapping_add(node.cost),
                node.idx,
                sw,
            )
        } else {
            let mid = self.add(
                through_float(delta.x().wrapping_abs()).wrapping_add(node.cost),
                node.idx,
                fixed_point,
                Kind::Waypoint,
            );
            let sw2 = self.ctx.sweep(fixed_point, corner, second)?;
            (
                through_float(delta.y().wrapping_abs()).wrapping_add(mid.cost),
                mid.idx,
                sw2,
            )
        };
        let (Some(shape), Some(side), Some(at)) = (sw.shape, sw.side, sw.point) else {
            return Err(GenieError::unsupported(
                "an L toward a shape's centre that meets nothing (stack garbage)",
            ));
        };
        self.side_node(fixed_distance, parent, shape, side, at);
        Ok(())
    }
}
