//! The polygons' outlines: the tracer, the border walk, the edge that holds a point and the
//! walk of a new polygon.

use crate::GenieResult;
use crate::pathing::FixedDistance;
use crate::pathing::FixedRect;
use crate::pathing::mesh::Contact;
use crate::pathing::mesh::Mesh;
use crate::pathing::short_path::axis_dir::AxisDir;
use crate::pathing::short_path::{
    FixedPoint, Poly, Search, TRACE_STEPS, UNSET, VertexIndex, clamp_to_box, contact_point, in_box,
    inside, shrunk_corner,
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

    /// The coordinate of `fixed_point` along a side's axis (x for sides 0 and 2, y for 1 and 3).
    pub(super) const fn along(fixed_point: FixedPoint, side: ShapeSide) -> i32 {
        if side.along_y() {
            fixed_point.y()
        } else {
            fixed_point.x()
        }
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
    ) -> GenieResult<Option<ShapeId>> {
        let fixed_rect = self.bounds;
        let shape_side = side.prev();
        let (target, dir) = match shape_side {
            ShapeSide::LowY => (
                FixedPoint::new(fixed_rect.x0(), fixed_rect.y1()),
                AxisDir::PlusY,
            ),
            ShapeSide::HighX => (
                FixedPoint::new(fixed_rect.x0(), fixed_rect.y0()),
                AxisDir::MinusX,
            ),
            ShapeSide::HighY => (
                FixedPoint::new(fixed_rect.x1(), fixed_rect.y0()),
                AxisDir::MinusY,
            ),
            ShapeSide::LowX => (
                FixedPoint::new(fixed_rect.x1(), fixed_rect.y1()),
                AxisDir::PlusX,
            ),
        };
        let sw = self.ctx.sweep(*pos, target, dir)?;
        let new_u = Self::along(*pos, shape_side);
        let Some((hid, hs, hp)) = sw.counted()? else {
            if steps == 0 {
                *first = target;
            }
            *u = new_u;
            *side = shape_side;
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
    ) -> GenieResult<Outline> {
        let fixed_distance = self.ctx.d;
        let mesh = self.ctx.mesh;
        let fixed_rect = self.bounds;
        let vertical = s0.along_y();
        let mut steps: i32 = 0;
        let mut winding: i32 = 0;
        let mut wprev: i32 = 0;
        let mut shape = shape0;
        let mut side = s0;
        let mut pos = match start {
            Some(p) => p,
            None => shrunk_corner(
                *mesh.shapes.checked(shape.index())?,
                s0.first_corner(),
                fixed_distance,
            ),
        };
        let mut u = Self::along(pos, s0);
        let mut first = pos;
        'segment: loop {
            let contacts = mesh.sides.checked(shape.index())?.checked(side.index())?;
            let next_side;
            if let Some(k) = self.nearest_contact(contacts, side, u) {
                let contact = *contacts.checked(k)?;
                let fixed_point = contact_point(
                    contact.at,
                    contact.side,
                    fixed_distance,
                    contact.at_side_start,
                );
                pos = fixed_point;
                if steps == 0 {
                    first = fixed_point;
                }
                if side != contact.side {
                    winding -= 1;
                }
                u = Self::along(fixed_point, contact.side);
                side = contact.side;
                shape = contact.other;
            } else {
                next_side = side.next();
                let corner = shrunk_corner(
                    *mesh.shapes.checked(shape.index())?,
                    next_side.first_corner(),
                    fixed_distance,
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
                let out = !in_box(pos, fixed_rect);
                if out {
                    side = side.opposite();
                    let clamped = clamp_to_box(pos, fixed_rect);
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

    /// [`Search::trace`]'s next contact on a side: of the side's contacts (`contacts`), those wider
    /// than twice `fixed_distance`, not ignored and whose contact point lies in the bounds, the one
    /// nearest ahead of `u` along `side` (each one's run start moved by `fixed_distance`, back for
    /// a flagged contact on the same side; the sides 2 and 3 measure backwards); ties to the first.
    pub(super) fn nearest_contact(
        &self,
        contacts: &[Contact],
        side: ShapeSide,
        u: i32,
    ) -> Option<usize> {
        let fixed_distance = self.ctx.d;
        let mut chosen = None;
        let mut best_key = 0x7fc0_0000_i32;
        for (index, contact) in contacts.iter().enumerate() {
            if (fixed_distance >= contact.half) || self.ctx.ignored(contact.other) {
                continue;
            }
            let fixed_point = contact_point(
                contact.at,
                contact.side,
                fixed_distance,
                contact.at_side_start,
            );
            if !in_box(fixed_point, self.bounds) {
                continue;
            }
            // the sides 0 and 1 measure forward (the game's `side < 2`, and any side past 3)
            let forward = matches!(side, ShapeSide::LowY | ShapeSide::HighX);
            let off = if side == contact.side && contact.at_side_start {
                fixed_distance.get().wrapping_neg()
            } else {
                fixed_distance.get()
            };
            let mut key = if forward {
                contact.along.wrapping_add(off)
            } else {
                contact.along.wrapping_sub(off)
            };
            key = key.wrapping_sub(u);
            if !forward {
                key = key.wrapping_neg();
            }
            if -1 < key && key < best_key {
                best_key = key;
                chosen = Some(index);
            }
        }
        chosen
    }

    /// `FUN_140fa77b0`: the polygon's edge that holds `fixed_point` going along `side` (the start
    /// of the edge's run), or the vertex `fixed_point` is when it lies in the overlap of its shape
    /// with `shape`; None if none.
    pub(super) fn edge_holding(
        poly: &Poly,
        fixed_point: FixedPoint,
        side: ShapeSide,
        mesh: &Mesh,
        shape: Option<ShapeId>,
        fixed_distance: FixedDistance,
    ) -> Option<VertexIndex> {
        let shape = shape?;
        let count = poly.vertices.len() as u32;
        if count == 0 {
            return None;
        }
        let fixed_points = &poly.vertices;
        let sides = &poly.sides;
        let mut limit = count;
        let mut prev = *sides.get(count.wrapping_sub(2) as usize % sides.len())?;
        let mut cur = count - 1;
        let mut i: u32 = 0;
        loop {
            let cur_side = *sides.get(cur as usize)?;
            let mut side_i = *sides.get(i as usize)?;
            let mut nxt = i;
            if side_i == cur_side {
                loop {
                    i = (i + 1) % count;
                    side_i = *sides.get(i as usize)?;
                    if side_i != cur_side {
                        break;
                    }
                }
                nxt = i;
                if i < cur && cur != count - 1 {
                    limit = 0;
                }
            }
            let vc = *fixed_points.get(cur as usize)?;
            if Self::vertex_in_overlap(
                mesh,
                (fixed_point, vc, *poly.shapes.get(cur as usize)?),
                shape,
                fixed_distance,
            ) || (side == cur_side
                && Self::on_edge(
                    cur_side,
                    (vc, *fixed_points.get(nxt as usize)?),
                    fixed_point,
                    (prev, side_i),
                ))
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

    /// `FUN_140fa77b0`'s vertex test: the vertex `vc` of shape `vs` is `fixed_point`, and
    /// `fixed_point` lies in the overlap of that shape with `shape` (the higher low corner, the
    /// lower high one), shrunk by `fixed_distance` on every side.
    pub(super) fn vertex_in_overlap(
        mesh: &Mesh,
        (fixed_point, vc, vs): (FixedPoint, FixedPoint, Option<ShapeId>),
        shape: ShapeId,
        fixed_distance: FixedDistance,
    ) -> bool {
        let d = fixed_distance.get();
        let Some(vs) = vs.filter(|_| fixed_point == vc) else {
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
        let fixed_rect = FixedRect::new(x0 + d, y0 + d, x1, y1);
        fixed_rect.x0() < fixed_rect.x1()
            && fixed_rect.y0() < fixed_rect.y1()
            && inside(fixed_rect, fixed_point, FixedDistance::ZERO)
    }

    /// `FUN_140fa77b0`'s edge test: `fixed_point` lies on the edge of side `cur_side` from `vc` to
    /// `vn`; the side before the edge's run (`prev`) and the one after it (`next`) decide whether
    /// each end counts.
    pub(super) const fn on_edge(
        cur_side: ShapeSide,
        (vc, vn): (FixedPoint, FixedPoint),
        fixed_point: FixedPoint,
        (prev, next): (ShapeSide, ShapeSide),
    ) -> bool {
        use ShapeSide::{HighX, HighY, LowX, LowY};
        // the game's unsigned compares of the side numbers: `prev < 2`, `prev - 1 < 2` and the
        // like name two neighbouring sides
        match cur_side {
            LowY if vc.y() == fixed_point.y() => {
                let (a, b) = (vc.x(), vn.x());
                let start = if matches!(prev, LowY | HighX) {
                    a <= fixed_point.x()
                } else {
                    a < fixed_point.x()
                };
                start
                    && if matches!(next, LowX | LowY) {
                        fixed_point.x() <= b
                    } else {
                        fixed_point.x() < b
                    }
            }
            HighX if vc.x() == fixed_point.x() => {
                let (a, b) = (vc.y(), vn.y());
                let start = if matches!(prev, HighX | HighY) {
                    a <= fixed_point.y()
                } else {
                    a < fixed_point.y()
                };
                start
                    && if matches!(next, LowY | HighX) {
                        fixed_point.y() <= b
                    } else {
                        fixed_point.y() < b
                    }
            }
            HighY if vc.y() == fixed_point.y() => {
                let (a, b) = (vc.x(), vn.x());
                let start = if matches!(next, HighX | HighY) {
                    b <= fixed_point.x()
                } else {
                    b < fixed_point.x()
                };
                start
                    && if matches!(prev, HighY | LowX) {
                        fixed_point.x() <= a
                    } else {
                        fixed_point.x() < a
                    }
            }
            LowX if vc.x() == fixed_point.x() => {
                let (a, b) = (vc.y(), vn.y());
                let start = if matches!(next, HighY | LowX) {
                    b <= fixed_point.y()
                } else {
                    b < fixed_point.y()
                };
                start
                    && if matches!(prev, LowY | LowX) {
                        fixed_point.y() <= a
                    } else {
                        fixed_point.y() < a
                    }
            }
            _ => false,
        }
    }
}
