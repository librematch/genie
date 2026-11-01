//! The fixed-point geometry: a shrunk rectangle's corners and sides, a contact's point, the
//! segment and ray hits.

use crate::pathing::FixedDistance;
use crate::pathing::FixedPoint;
use crate::pathing::FixedRect;
use crate::pathing::ShapeSide;

/// `FUN_140fa8460`: corner `k` of `r` shrunk by `d`.
pub(super) const fn shrunk_corner(r: FixedRect, k: u32, d: FixedDistance) -> FixedPoint {
    let d = d.get();
    match k {
        0 => FixedPoint::new(r.x0() + d, r.y0() + d),
        1 => FixedPoint::new(r.x1() - d, r.y0() + d),
        2 => FixedPoint::new(r.x1() - d, r.y1() - d),
        3 => FixedPoint::new(r.x0() + d, r.y1() - d),
        _ => FixedPoint::new(r.x0(), r.y0()),
    }
}

/// `FUN_140fa8350`: a contact's junction moved by `d` into the free space beside its side.
pub(super) const fn contact_point(
    p: FixedPoint,
    side: ShapeSide,
    d: FixedDistance,
    flag: u8,
) -> FixedPoint {
    let [x, y] = p.get();
    let d = d.get();
    let f = flag == 1;
    match side {
        ShapeSide::HighX => {
            if f {
                FixedPoint::new(x - d, y + d)
            } else {
                FixedPoint::new(x - d, y - d)
            }
        }
        ShapeSide::HighY => {
            if f {
                FixedPoint::new(x - d, y - d)
            } else {
                FixedPoint::new(x + d, y - d)
            }
        }
        ShapeSide::LowX => {
            if f {
                FixedPoint::new(x + d, y - d)
            } else {
                FixedPoint::new(x + d, y + d)
            }
        }
        ShapeSide::LowY => {
            if f {
                FixedPoint::new(x + d, y + d)
            } else {
                FixedPoint::new(x - d, y + d)
            }
        }
    }
}

/// `FUN_140fa39f0`: `p` moved onto side `side` of `r` shrunk by `d`, held within the side.
pub(super) fn snap_to_side(p: &mut FixedPoint, r: FixedRect, side: ShapeSide, d: FixedDistance) {
    let d = d.get();
    let hold = |v: i32, lo: i32, hi: i32| {
        if v < lo {
            lo
        } else if hi < v {
            hi
        } else {
            v
        }
    };
    match side {
        ShapeSide::LowY => {
            *p = FixedPoint::new(hold(p.x(), r.x0() + d, r.x1() - d), r.y0() + d);
        }
        ShapeSide::HighX => {
            *p = FixedPoint::new(r.x1() - d, hold(p.y(), r.y0() + d, r.y1() - d));
        }
        ShapeSide::HighY => {
            *p = FixedPoint::new(hold(p.x(), r.x0() + d, r.x1() - d), r.y1() - d);
        }
        ShapeSide::LowX => {
            *p = FixedPoint::new(r.x0() + d, hold(p.y(), r.y0() + d, r.y1() - d));
        }
    }
}

/// How a ray meets a segment or a shape's side (the non-zero results of [`segment_hit`], the
/// ray's side tests and [`ray_rect`]; the game's 0 for none is `None`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Meet {
    /// 1: a proper crossing.
    Cross,
    /// 2: a touch or a graze.
    Touch,
    /// 3: a touch at the ray's start on an end of the segment.
    TouchAtStart,
}

/// A place along a ray in fixed point, `ONE` its end (the raycast's `t`): a hit lowers it to
/// the hit's place.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct RayFraction(i32);

impl RayFraction {
    /// The raycast's start (`0x466666`, 1.1): past the ray's end.
    pub(super) const START: Self = Self(0x46_6666);

    /// The place `v` (`ONE` the ray's end).
    pub(super) const fn new(v: i32) -> Self {
        Self(v)
    }

    /// Its value.
    pub(super) const fn get(self) -> i32 {
        self.0
    }
}

/// `FUN_140faa380`: segment `a`–`b` against segment `c`–`e` in fixed point; `t` the parameter along
/// `a`–`b`. A proper crossing nearer than `t`; a touch or graze (tolerances 1e-4
/// and 1e-3); a touch at `a` itself on an end of `c`–`e`; none. Sets `t` on a hit.
pub(super) fn segment_hit(
    a: FixedPoint,
    b: FixedPoint,
    c: FixedPoint,
    e: FixedPoint,
    t: &mut RayFraction,
) -> Option<Meet> {
    let w = |v: i32| i64::from(v);
    let (x1, y1, x2, y2) = (w(a.x()), w(a.y()), w(b.x()), w(b.y()));
    let (x3, y3, x4, y4) = (w(c.x()), w(c.y()), w(e.x()), w(e.y()));
    let den = (((y4 - y3) * (x2 - x1)) >> 22) - (((y2 - y1) * (x4 - x3)) >> 22);
    if den == 0 {
        return None;
    }
    let ua =
        (((((y1 - y3) * (x4 - x3)) >> 22) - (((y4 - y3) * (x1 - x3)) >> 22)) * 0x40_0000) / den;
    let ub =
        (((((y1 - y3) * (x2 - x1)) >> 22) - (((y2 - y1) * (x1 - x3)) >> 22)) * 0x40_0000) / den;
    let current = i64::from(t.get());
    let mut test_ub_low = true;
    if ua < 0x1a3 {
        if !(-0x1062..=0x40_1062).contains(&ua) || current <= ua {
            return None;
        }
    } else if 0x3f_fe5d < ua {
        if 0x40_1062 < ua || current <= ua {
            return None;
        }
    } else {
        if current <= ua {
            return None;
        }
        if 0x1a2 < ub {
            if ub < 0x3f_fe5e {
                *t = RayFraction::new(ua as i32);
                return Some(Meet::Cross);
            }
            test_ub_low = false;
        }
    }
    if test_ub_low && ub < -0x1062 {
        return None;
    }
    if 0x40_1062 < ub {
        return None;
    }
    *t = RayFraction::new(ua as i32);
    Some(if ua == 0 && (ub & !0x40_0000) == 0 {
        Meet::TouchAtStart
    } else {
        Meet::Touch
    })
}

/// `FUN_140faa4e0`: the ray `a`–`b` against the sides of the rectangle `r` it faces (the right side
/// for a ray going left, the left for one going right, then the bottom or the top): how it meets
/// one, as [`segment_hit`], and the side; none for none. A y side wins only when strictly nearer.
pub(super) fn ray_rect(
    a: FixedPoint,
    b: FixedPoint,
    r: FixedRect,
    t: &mut RayFraction,
) -> Option<(Meet, ShapeSide)> {
    let (dx, dy) = (b.x() - a.x(), b.y() - a.y());
    let mut found = None;
    // the side across x it faces: the right (1) for a ray going left, the left (3) for one going
    // right
    let (k, s) = match dx {
        ..0 => (ray_right_side(a, b, r, t), ShapeSide::HighX),
        0 => (None, ShapeSide::HighX),
        _ => (ray_left_side(a, b, r, t), ShapeSide::LowX),
    };
    if let Some(k) = k {
        found = Some((k, s));
    }
    // then the bottom (0) for a ray going up in y, the top (2) for one going down
    let (k, s) = match dy {
        1.. => (ray_bottom_side(a, b, r, t), ShapeSide::LowY),
        0 => return found,
        _ => (ray_top_side(a, b, r, t), ShapeSide::HighY),
    };
    let Some(k) = k else {
        return found;
    };
    Some((k, s))
}

/// [`ray_rect`]'s ratio of a side's offset `num` along the ray's `den` (`num << 22 / den`).
pub(super) fn ray_ratio(num: i32, den: i32) -> i32 {
    ((i64::from(num) << 22) / i64::from(den)) as i32
}

/// [`ray_rect`]'s right side (x1) for the ray `a`–`b` going left: a ray along x (no y step)
/// crosses it within its open span from at or right of it to left of it, nearer than `t` (a
/// crossing, its ratio into `t`); any other ray as [`segment_hit`]; none for none.
pub(super) fn ray_right_side(
    a: FixedPoint,
    b: FixedPoint,
    r: FixedRect,
    t: &mut RayFraction,
) -> Option<Meet> {
    let [_, ry0, rx1, ry1] = r.get();
    if b.y() - a.y() != 0 {
        return segment_hit(
            a,
            b,
            FixedPoint::new(rx1, ry0),
            FixedPoint::new(rx1, ry1),
            t,
        );
    }
    if a.y() <= ry0 || ry1 <= a.y() {
        return None;
    }
    if rx1 <= b.x() || a.x() < rx1 {
        return None;
    }
    let u = ray_ratio(rx1 - a.x(), b.x() - a.x());
    if t.get() <= u {
        return None;
    }
    *t = RayFraction::new(u);
    Some(Meet::Cross)
}

/// [`ray_rect`]'s left side (x0) for the ray `a`–`b` going right, as [`ray_right_side`]: a ray
/// along x crosses it from at or left of it to right of it.
pub(super) fn ray_left_side(
    a: FixedPoint,
    b: FixedPoint,
    r: FixedRect,
    t: &mut RayFraction,
) -> Option<Meet> {
    let [rx0, ry0, _, ry1] = r.get();
    if b.y() - a.y() != 0 {
        return segment_hit(
            a,
            b,
            FixedPoint::new(rx0, ry0),
            FixedPoint::new(rx0, ry1),
            t,
        );
    }
    if a.y() <= ry0 || ry1 <= a.y() {
        return None;
    }
    if b.x() <= rx0 || rx0 < a.x() {
        return None;
    }
    let u = ray_ratio(rx0 - a.x(), b.x() - a.x());
    if t.get() <= u {
        return None;
    }
    *t = RayFraction::new(u);
    Some(Meet::Cross)
}

/// [`ray_rect`]'s bottom side (y0) for the ray `a`–`b` going up in y: a ray along y (no x step)
/// crosses it within its open span from at or below it to above it, nearer than `t` (a
/// crossing, its ratio into `t`); any other ray as [`segment_hit`]; none for none.
pub(super) fn ray_bottom_side(
    a: FixedPoint,
    b: FixedPoint,
    r: FixedRect,
    t: &mut RayFraction,
) -> Option<Meet> {
    let [rx0, ry0, rx1, _] = r.get();
    if b.x() - a.x() != 0 {
        return segment_hit(
            a,
            b,
            FixedPoint::new(rx0, ry0),
            FixedPoint::new(rx1, ry0),
            t,
        );
    }
    if !(rx0 < a.x() && a.x() < rx1) {
        return None;
    }
    if b.y() <= ry0 || ry0 < a.y() {
        return None;
    }
    let u = ray_ratio(ry0 - a.y(), b.y() - a.y());
    if t.get() <= u {
        return None;
    }
    *t = RayFraction::new(u);
    Some(Meet::Cross)
}

/// [`ray_rect`]'s top side (y1) for the ray `a`–`b` going down in y, as [`ray_bottom_side`]: a
/// ray along y crosses it from at or above it to below it.
pub(super) fn ray_top_side(
    a: FixedPoint,
    b: FixedPoint,
    r: FixedRect,
    t: &mut RayFraction,
) -> Option<Meet> {
    let [rx0, _, rx1, ry1] = r.get();
    if b.x() - a.x() != 0 {
        return segment_hit(
            a,
            b,
            FixedPoint::new(rx0, ry1),
            FixedPoint::new(rx1, ry1),
            t,
        );
    }
    if !(rx0 < a.x() && a.x() < rx1) {
        return None;
    }
    if ry1 <= b.y() || a.y() < ry1 {
        return None;
    }
    let u = ray_ratio(ry1 - a.y(), b.y() - a.y());
    if t.get() <= u {
        return None;
    }
    *t = RayFraction::new(u);
    Some(Meet::Cross)
}
