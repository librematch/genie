//! A convex hull ([`Hull`], `MConvexHull`): the point test, the intersection, a box's overlap,
//! the merge and the rays to a box or out of the hull.

use crate::GenieResult;
use crate::map::Axis;
use crate::math::fmath::at_most_or_unordered;
use crate::pathing::hull_path::{
    EPSILON, FULL_TURN, HALF, HALF_TURN, HIGH_T, Hull, LOW_T, MOST, NO_ANGLE, NUDGE, ONE, Point,
    QUADRANT, TURN, UNTURN,
};
use crate::world::layer::generated::HullPathTuning;
use crate::world::slots::CheckedIndex;

/// Whether a hull's meeting at the walk's start counts (`getHullIntersection`'s flag): a walk
/// from a hull's vertex leaves it out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum AtStart {
    /// It counts.
    Counts,
    /// It does not.
    Ignored,
}

impl Hull {
    /// `MConvexHull::InsideHull` (`FUN_140f87760`): whether `point`, moved a little toward the
    /// bounding box's middle, lies inside the hull (the crossings of a ray to the right).
    pub(super) fn inside(&self, point: Point) -> bool {
        let px = point.x() + ((self.max.x() + self.min.x()) * HALF - point.x()) * NUDGE;
        let py = point.y() + ((self.max.y() + self.min.y()) * HALF - point.y()) * NUDGE;
        let mut inside = false;
        let mut previous = self.points.last().copied();
        for pi in &self.points {
            let Some(pj) = previous else {
                break;
            };
            let ((xi, yi), (xj, yj)) = ((pi.x(), pi.y()), (pj.x(), pj.y()));
            if ((yi <= py && py < yj) || (yj <= py && py < yi))
                && px < ((py - yi) * (xj - xi)) / (yj - yi) + xi
            {
                inside = !inside;
            }
            previous = Some(*pi);
        }
        inside
    }

    /// `MConvexHull::getHullIntersection` (`FUN_140f879e0`): the nearest point where the walk `a`
    /// -> `b` meets the hull's sides, and the side (from point `index - 1` to point `index`, the
    /// last side 0); a meeting at `a` itself counts only with [`AtStart::Counts`].
    pub(super) fn intersection(
        &self,
        start: AtStart,
        a: Point,
        b: Point,
        hull_path_tuning: &HullPathTuning,
    ) -> Option<(Point, usize)> {
        let count = self.points.len();
        let (dx, dy) = (b.x() - a.x(), b.y() - a.y());
        let mut best = hull_path_tuning.far;
        let mut found: Option<(Point, usize)> = None;
        for index in 1..=count {
            let p0 = *self.points.get(index - 1)?;
            let p1 = if index == count {
                *self.points.first()?
            } else {
                *self.points.get(index)?
            };
            let (ex, ey) = (p1.x() - p0.x(), p1.y() - p0.y());
            let (rx, ry) = (a.x() - p0.x(), a.y() - p0.y());
            let denom = dx * ey - dy * ex;
            if denom.abs() < EPSILON {
                continue;
            }
            let inv = ONE / denom;
            let t = inv * (ry * ex - rx * ey);
            let u = (ry * dx - dy * rx) * inv;
            if !((LOW_T..=HIGH_T).contains(&t) && (LOW_T..=HIGH_T).contains(&u)) {
                continue;
            }
            let p = Point::new(dx * t + a.x(), dy * t + a.y());
            if start == AtStart::Ignored
                && (p.x() - a.x()).abs() < EPSILON
                && (p.y() - a.y()).abs() < EPSILON
            {
                continue;
            }
            let d = (p.y() - a.y()) * (p.y() - a.y()) + (p.x() - a.x()) * (p.x() - a.x());
            if found.is_none() || d < best {
                found = Some((p, index));
                best = d;
            }
        }
        found.map(|(point, k)| (point, if k == count { 0 } else { k }))
    }

    /// `MConvexHull::DoesShapeIntersect` (`FUN_140f85330`) for the box `min`..`max`: 3 when the
    /// box lies inside the hull, 2 when some of its corners do (with which do not, by corner: low
    /// left, low right, high right, high left), 1 when a hull point lies in the box or a box side
    /// meets the hull, else 0.
    pub(super) fn shape_overlap(
        &self,
        min: Point,
        max: Point,
        hull_path_tuning: &HullPathTuning,
    ) -> (u8, [bool; 4]) {
        let ((x0, y0), (x1, y1)) = ((min.x(), min.y()), (max.x(), max.y()));
        let mut outside = [true; 4];
        if !(x0 <= self.max.x() && y0 <= self.max.y() && self.min.x() <= x1 && self.min.y() <= y1) {
            return (0, outside);
        }
        let corners = [min, Point::new(x1, y0), max, Point::new(x0, y1)];
        let mut inside = 0;
        for (c, outside) in corners.iter().zip(outside.iter_mut()) {
            if self.inside(*c) {
                inside += 1;
                *outside = false;
            }
        }
        if inside == 4 {
            return (3, outside);
        }
        if inside != 0 {
            return (2, outside);
        }
        if self
            .points
            .iter()
            .any(|p| x0 <= p.x() && p.x() <= x1 && y0 <= p.y() && p.y() <= y1)
        {
            return (1, outside);
        }
        let sides = [
            (min, Point::new(x1, y0)),
            (max, Point::new(x1, y0)),
            (max, Point::new(x0, y1)),
            (min, Point::new(x0, y1)),
        ];
        let meets = sides.iter().any(|&(a, b)| {
            self.intersection(AtStart::Counts, a, b, hull_path_tuning)
                .is_some()
        });
        (u8::from(meets), outside)
    }

    /// `MConvexHull::MergePointsIntoHull` (`FUN_140f87ca0`): the hull of its points and `more` (a
    /// new point the same as one of the hull's within [`EPSILON`] is left out; the new points are
    /// not compared with each other): from the highest point (the rightmost of the highest), a
    /// gift wrap by the largest pseudo-angle turned by the last step's (the farthest of equals),
    /// until no point is left or the first point turns no less.
    pub(super) fn merge(&mut self, more: &[Point]) -> GenieResult<()> {
        let mut scratch = self.points.clone();
        for p in more {
            let seen = self
                .points
                .iter()
                .any(|s| (p.x() - s.x()).abs() < EPSILON && (p.y() - s.y()).abs() < EPSILON);
            if !seen {
                scratch.push(*p);
            }
        }
        if scratch.len() > self.entrances.len() {
            self.entrances.resize(scratch.len(), None);
        }
        let mut k = 0;
        let mut first = *scratch.checked(0)?;
        for (i, s) in scratch.iter().enumerate() {
            if first.y() <= s.y() && (first.y() < s.y() || first.x() < s.x()) {
                first = *s;
                k = i;
            }
        }
        self.min = *scratch.checked(k)?;
        self.max = *scratch.checked(k)?;
        self.points.clear();
        let mut turned = 0.0;
        let mut step = 0.0;
        let mut left = scratch.len();
        loop {
            let cur = *scratch.checked(k)?;
            self.points.push(cur);
            if cur.x() < self.min.x() {
                self.min = self.min.with_x(cur.x());
            }
            if cur.y() < self.min.y() {
                self.min = self.min.with_y(cur.y());
            }
            if self.max.x() < cur.x() {
                self.max = self.max.with_x(cur.x());
            }
            if self.max.y() < cur.y() {
                self.max = self.max.with_y(cur.y());
            }
            if k < left - 1 {
                scratch.swap(k, left - 1);
            }
            left -= 1;
            if left == 0 {
                return Ok(());
            }
            let mut best: Option<usize> = None;
            let mut best_angle = NO_ANGLE;
            let mut best_d = NO_ANGLE;
            for (index, point) in scratch.head(left)?.iter().enumerate() {
                let a = pseudo_angle(point.x() - cur.x(), point.y() - cur.y());
                let angle = turn(a * QUADRANT + turned);
                if best_angle <= angle {
                    let d = (cur.y() - point.y()) * (cur.y() - point.y())
                        + (cur.x() - point.x()) * (cur.x() - point.x());
                    if angle <= best_angle {
                        if d <= best_d {
                            continue;
                        }
                    } else {
                        best_angle = angle;
                    }
                    best_d = d;
                    step = a * QUADRANT;
                    best = Some(index);
                }
            }
            let Some(next) = best else {
                return Ok(());
            };
            if 1 < self.points.len() {
                let a = pseudo_angle(first.x() - cur.x(), first.y() - cur.y());
                let angle = turn(a * QUADRANT + turned);
                if best_angle < angle {
                    return Ok(());
                }
                let d = (cur.y() - first.y()) * (cur.y() - first.y())
                    + (cur.x() - first.x()) * (cur.x() - first.x());
                if angle == best_angle && best_d < d {
                    return Ok(());
                }
            }
            turned = TURN - step;
            k = next;
        }
    }

    /// `FUN_140f89dc0`: the first box holding `point` (sides excluded).
    pub(super) fn box_holding(&self, point: Point) -> Option<usize> {
        self.objects.iter().position(|shape| shape.holds(point))
    }

    /// `FUN_140f8ea30`: the nearest box side a ray from `point` meets toward `side` (0 down, 1 up,
    /// 2 left, 3 right), with the box and the box's side (0 its top, 1 its right, 2 its bottom,
    /// 3 its left: the far side when `point` is inside the box). The box `own` counts only strictly
    /// beyond `point`. The span tests skip a box by `COMISS`/`JA` (0x140f8ec1c, 0x140f8eb6e): an
    /// unordered coordinate lies in every span.
    pub(super) fn ray_to_box(
        &self,
        point: Point,
        side: i32,
        own: i32,
    ) -> Option<(Point, usize, i32)> {
        let mut best = MOST;
        let mut found = None;
        let within =
            |lo: f32, v: f32, hi: f32| at_most_or_unordered(lo, v) && at_most_or_unordered(v, hi);
        for (index, shape) in self.objects.iter().enumerate() {
            let rect = shape.bounds;
            let (x0, y0, x1, y1) = (rect.x0(), rect.y0(), rect.x1(), rect.y1());
            let mine = index as i32 == own;
            let hit = match side {
                0 if within(x0, point.x(), x1) => {
                    let past = if mine {
                        y1 < point.y()
                    } else {
                        y1 <= point.y()
                    };
                    if past {
                        Some((y1, 0, point.y() - y1))
                    } else if point.y() <= y0 {
                        None
                    } else {
                        Some((y0, 2, point.y() - y0))
                    }
                }
                1 if within(x0, point.x(), x1) => {
                    let past = if mine {
                        point.y() < y0
                    } else {
                        point.y() <= y0
                    };
                    if past {
                        Some((y0, 2, y0 - point.y()))
                    } else if y1 <= point.y() {
                        None
                    } else {
                        Some((y1, 0, y1 - point.y()))
                    }
                }
                2 if within(y0, point.y(), y1) => {
                    let past = if mine {
                        x1 < point.x()
                    } else {
                        x1 <= point.x()
                    };
                    if past {
                        Some((x1, 1, point.x() - x1))
                    } else if point.x() <= x0 {
                        None
                    } else {
                        Some((x0, 3, point.x() - x0))
                    }
                }
                3 if within(y0, point.y(), y1) => {
                    let past = if mine {
                        point.x() < x0
                    } else {
                        point.x() <= x0
                    };
                    if past {
                        Some((x0, 3, x0 - point.x()))
                    } else if x1 <= point.x() {
                        None
                    } else {
                        Some((x1, 1, x1 - point.x()))
                    }
                }
                _ => None,
            };
            if let Some((edge, box_side, d)) = hit
                && d < best
            {
                best = d;
                let at = if side < 2 {
                    point.with_y(edge)
                } else {
                    point.with_x(edge)
                };
                found = Some((at, index, box_side));
            }
        }
        found
    }

    /// `FUN_140f8e530`: where a ray from `point` toward `side` leaves the hull, and the hull side
    /// (from point `k - 1` to point `k`) it leaves by; along a side parallel to the ray, at `point`
    /// itself while `point` is within the side's span.
    pub(super) fn ray_to_hull(&self, point: Point, side: i32) -> Option<(Point, usize)> {
        let mut best = MOST;
        let mut found = None;
        // the ray's axis (a), the other (b), and whether it runs toward smaller values
        let (a, b, down) = match side {
            0 => (Axis::Y, Axis::X, true),
            1 => (Axis::Y, Axis::X, false),
            2 => (Axis::X, Axis::Y, true),
            3 => (Axis::X, Axis::Y, false),
            _ => return None,
        };
        let (pa, pb) = (point.along(a), point.along(b));
        let mut previous = self.points.last();
        for (index, pi) in self.points.iter().enumerate() {
            let Some(pj) = previous else {
                break;
            };
            previous = Some(pi);
            let (bi, bj) = (pi.along(b), pj.along(b));
            let (lo_b, hi_b) = if bi < bj { (bi, bj) } else { (bj, bi) };
            if lo_b <= pb && pb <= hi_b {
                let (ai, aj) = (pi.along(a), pj.along(a));
                let sloped = EPSILON <= (bj - bi).abs();
                let at = |q: f32| ((q - bi) / (bj - bi)) * (aj - ai) + ai;
                let (lo, hi) = if ai < aj { (ai, aj) } else { (aj, ai) };
                let hit = if down {
                    if lo <= pa {
                        if sloped {
                            let y = at(pb);
                            (y <= pa).then_some(y)
                        } else if pa < hi {
                            Some(pa)
                        } else {
                            Some(hi)
                        }
                    } else {
                        None
                    }
                } else {
                    if pa <= hi {
                        if sloped {
                            let y = at(pb);
                            (pa <= y).then_some(y)
                        } else if lo < pa {
                            Some(pa)
                        } else {
                            Some(lo)
                        }
                    } else {
                        None
                    }
                };
                if let Some(e) = hit {
                    let d = if down { pa - e } else { e - pa };
                    if d < best {
                        best = d;
                        found = Some((point.with_along(a, e), index));
                    }
                }
            }
        }
        found
    }
}

/// The gift wrap's pseudo-angle of a step, in quadrants (0 to 4, counterclockwise from the x
/// axis): the diamond angle of `MergePointsIntoHull`.
pub(super) fn pseudo_angle(dx: f32, dy: f32) -> f32 {
    let t = dy.abs() + dx.abs();
    let a = if t == 0.0 { 0.0 } else { dy / t };
    if 0.0 <= dx {
        if dy < 0.0 { a + FULL_TURN } else { a }
    } else {
        HALF_TURN - a
    }
}

/// An angle past a full turn turns back once.
pub(super) fn turn(angle: f32) -> f32 {
    if TURN < angle { angle + UNTURN } else { angle }
}
