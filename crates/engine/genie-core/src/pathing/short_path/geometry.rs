//! The fixed-point geometry: a shrunk rectangle's corners and sides, a contact's point, the
//! segment and ray hits.

use crate::pathing::FixedDistance;
use crate::pathing::FixedPoint;
use crate::pathing::FixedRect;
use crate::pathing::ShapeSide;

/// `FUN_140fa8460`: corner `corner` of `fixed_rect` shrunk by `fixed_distance`.
pub(super) const fn shrunk_corner(
    fixed_rect: FixedRect,
    corner: u32,
    fixed_distance: FixedDistance,
) -> FixedPoint {
    let fixed_distance = fixed_distance.get();
    match corner {
        0 => FixedPoint::new(
            fixed_rect.x0() + fixed_distance,
            fixed_rect.y0() + fixed_distance,
        ),
        1 => FixedPoint::new(
            fixed_rect.x1() - fixed_distance,
            fixed_rect.y0() + fixed_distance,
        ),
        2 => FixedPoint::new(
            fixed_rect.x1() - fixed_distance,
            fixed_rect.y1() - fixed_distance,
        ),
        3 => FixedPoint::new(
            fixed_rect.x0() + fixed_distance,
            fixed_rect.y1() - fixed_distance,
        ),
        _ => FixedPoint::new(fixed_rect.x0(), fixed_rect.y0()),
    }
}

/// `FUN_140fa8350`: a contact's junction moved by `fixed_distance` into the free space beside its
/// side (the other way along it for a contact at its side's start).
pub(super) const fn contact_point(
    fixed_point: FixedPoint,
    side: ShapeSide,
    fixed_distance: FixedDistance,
    at_side_start: bool,
) -> FixedPoint {
    let [x, y] = fixed_point.get();
    let fixed_distance = fixed_distance.get();
    match side {
        ShapeSide::HighX => {
            if at_side_start {
                FixedPoint::new(x - fixed_distance, y + fixed_distance)
            } else {
                FixedPoint::new(x - fixed_distance, y - fixed_distance)
            }
        }
        ShapeSide::HighY => {
            if at_side_start {
                FixedPoint::new(x - fixed_distance, y - fixed_distance)
            } else {
                FixedPoint::new(x + fixed_distance, y - fixed_distance)
            }
        }
        ShapeSide::LowX => {
            if at_side_start {
                FixedPoint::new(x + fixed_distance, y - fixed_distance)
            } else {
                FixedPoint::new(x + fixed_distance, y + fixed_distance)
            }
        }
        ShapeSide::LowY => {
            if at_side_start {
                FixedPoint::new(x + fixed_distance, y + fixed_distance)
            } else {
                FixedPoint::new(x - fixed_distance, y + fixed_distance)
            }
        }
    }
}

/// `FUN_140fa39f0`: `fixed_point` moved onto side `side` of `fixed_rect` shrunk by
/// `fixed_distance`, held within the side.
pub(super) fn snap_to_side(
    fixed_point: &mut FixedPoint,
    fixed_rect: FixedRect,
    side: ShapeSide,
    fixed_distance: FixedDistance,
) {
    let fixed_distance = fixed_distance.get();
    let hold = |value: i32, lo: i32, hi: i32| {
        if value < lo {
            lo
        } else if hi < value {
            hi
        } else {
            value
        }
    };
    match side {
        ShapeSide::LowY => {
            *fixed_point = FixedPoint::new(
                hold(
                    fixed_point.x(),
                    fixed_rect.x0() + fixed_distance,
                    fixed_rect.x1() - fixed_distance,
                ),
                fixed_rect.y0() + fixed_distance,
            );
        }
        ShapeSide::HighX => {
            *fixed_point = FixedPoint::new(
                fixed_rect.x1() - fixed_distance,
                hold(
                    fixed_point.y(),
                    fixed_rect.y0() + fixed_distance,
                    fixed_rect.y1() - fixed_distance,
                ),
            );
        }
        ShapeSide::HighY => {
            *fixed_point = FixedPoint::new(
                hold(
                    fixed_point.x(),
                    fixed_rect.x0() + fixed_distance,
                    fixed_rect.x1() - fixed_distance,
                ),
                fixed_rect.y1() - fixed_distance,
            );
        }
        ShapeSide::LowX => {
            *fixed_point = FixedPoint::new(
                fixed_rect.x0() + fixed_distance,
                hold(
                    fixed_point.y(),
                    fixed_rect.y0() + fixed_distance,
                    fixed_rect.y1() - fixed_distance,
                ),
            );
        }
    }
}

/// How a ray meets a segment or a shape's side (the non-zero results of [`segment_hit`], the
/// ray's side tests and [`ray_rect`]; the game's 0 for none is `None`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Meet {
    /// 1: a proper crossing.
    Cross,
    /// 2: a touch or a graze.
    Touch,
    /// 3: a touch at the ray's start on an end of the segment.
    TouchAtStart,
}

/// A place along a ray in fixed point, `ONE` its end (the raycast's fraction): a hit lowers it to
/// the hit's place.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct RayFraction(i32);

impl RayFraction {
    /// The raycast's start (`0x466666`, 1.1): past the ray's end.
    pub(super) const START: Self = Self(0x46_6666);

    /// The place `value` (`ONE` the ray's end).
    pub(super) const fn new(value: i32) -> Self {
        Self(value)
    }

    /// Its value.
    pub(super) const fn get(self) -> i32 {
        self.0
    }
}

/// `FUN_140faa380`: segment `start`–`end` against segment `other_start`–`fixed_point` in fixed
/// point; `ray_fraction` the parameter along `start`–`end`. A proper crossing nearer than
/// `ray_fraction`; a touch or graze (tolerances 1e-4 and 1e-3); a touch at `start` itself on an end
/// of `other_start`–`fixed_point`; none. Sets `ray_fraction` on a hit.
pub(super) fn segment_hit(
    start: FixedPoint,
    end: FixedPoint,
    other_start: FixedPoint,
    fixed_point: FixedPoint,
    ray_fraction: &mut RayFraction,
) -> Option<Meet> {
    let wide = |value: i32| i64::from(value);
    let (x1, y1, x2, y2) = (
        wide(start.x()),
        wide(start.y()),
        wide(end.x()),
        wide(end.y()),
    );
    let (x3, y3, x4, y4) = (
        wide(other_start.x()),
        wide(other_start.y()),
        wide(fixed_point.x()),
        wide(fixed_point.y()),
    );
    let den = (((y4 - y3) * (x2 - x1)) >> 22) - (((y2 - y1) * (x4 - x3)) >> 22);
    if den == 0 {
        return None;
    }
    let ua =
        (((((y1 - y3) * (x4 - x3)) >> 22) - (((y4 - y3) * (x1 - x3)) >> 22)) * 0x40_0000) / den;
    let ub =
        (((((y1 - y3) * (x2 - x1)) >> 22) - (((y2 - y1) * (x1 - x3)) >> 22)) * 0x40_0000) / den;
    let current = i64::from(ray_fraction.get());
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
                *ray_fraction = RayFraction::new(ua as i32);
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
    *ray_fraction = RayFraction::new(ua as i32);
    Some(if ua == 0 && (ub & !0x40_0000) == 0 {
        Meet::TouchAtStart
    } else {
        Meet::Touch
    })
}

/// `FUN_140faa4e0`: the ray `start`–`fixed_point` against the sides of the rectangle `fixed_rect`
/// it faces (the right side for a ray going left, the left for one going right, then the bottom or
/// the top): how it meets one, as [`segment_hit`], and the side; none for none. A y side wins only
/// when strictly nearer.
pub(super) fn ray_rect(
    start: FixedPoint,
    fixed_point: FixedPoint,
    fixed_rect: FixedRect,
    ray_fraction: &mut RayFraction,
) -> Option<(Meet, ShapeSide)> {
    let (dx, dy) = (fixed_point.x() - start.x(), fixed_point.y() - start.y());
    let mut found = None;
    // the side across x it faces: the right (1) for a ray going left, the left (3) for one going
    // right
    let (meet, shape_side) = match dx {
        ..0 => (
            ray_right_side(start, fixed_point, fixed_rect, ray_fraction),
            ShapeSide::HighX,
        ),
        0 => (None, ShapeSide::HighX),
        _ => (
            ray_left_side(start, fixed_point, fixed_rect, ray_fraction),
            ShapeSide::LowX,
        ),
    };
    if let Some(meet) = meet {
        found = Some((meet, shape_side));
    }
    // then the bottom (0) for a ray going up in y, the top (2) for one going down
    let (meet, shape_side) = match dy {
        1.. => (
            ray_bottom_side(start, fixed_point, fixed_rect, ray_fraction),
            ShapeSide::LowY,
        ),
        0 => return found,
        _ => (
            ray_top_side(start, fixed_point, fixed_rect, ray_fraction),
            ShapeSide::HighY,
        ),
    };
    let Some(meet) = meet else {
        return found;
    };
    Some((meet, shape_side))
}

/// [`ray_rect`]'s ratio of a side's offset `num` along the ray's `den` (`num << 22 / den`).
pub(super) fn ray_ratio(num: i32, den: i32) -> i32 {
    ((i64::from(num) << 22) / i64::from(den)) as i32
}

/// [`ray_rect`]'s right side (x1) for the ray `start`–`fixed_point` going left: a ray along x (no y
/// step) crosses it within its open span from at or right of it to left of it, nearer than
/// `ray_fraction` (a crossing, its ratio into `ray_fraction`); any other ray as [`segment_hit`];
/// none for none.
pub(super) fn ray_right_side(
    start: FixedPoint,
    fixed_point: FixedPoint,
    fixed_rect: FixedRect,
    ray_fraction: &mut RayFraction,
) -> Option<Meet> {
    let [_, ry0, rx1, ry1] = fixed_rect.get();
    if fixed_point.y() - start.y() != 0 {
        return segment_hit(
            start,
            fixed_point,
            FixedPoint::new(rx1, ry0),
            FixedPoint::new(rx1, ry1),
            ray_fraction,
        );
    }
    if start.y() <= ry0 || ry1 <= start.y() {
        return None;
    }
    if rx1 <= fixed_point.x() || start.x() < rx1 {
        return None;
    }
    let ray_ratio = ray_ratio(rx1 - start.x(), fixed_point.x() - start.x());
    if ray_fraction.get() <= ray_ratio {
        return None;
    }
    *ray_fraction = RayFraction::new(ray_ratio);
    Some(Meet::Cross)
}

/// [`ray_rect`]'s left side (x0) for the ray `start`–`fixed_point` going right, as
/// [`ray_right_side`]: a ray along x crosses it from at or left of it to right of it.
pub(super) fn ray_left_side(
    start: FixedPoint,
    fixed_point: FixedPoint,
    fixed_rect: FixedRect,
    ray_fraction: &mut RayFraction,
) -> Option<Meet> {
    let [rx0, ry0, _, ry1] = fixed_rect.get();
    if fixed_point.y() - start.y() != 0 {
        return segment_hit(
            start,
            fixed_point,
            FixedPoint::new(rx0, ry0),
            FixedPoint::new(rx0, ry1),
            ray_fraction,
        );
    }
    if start.y() <= ry0 || ry1 <= start.y() {
        return None;
    }
    if fixed_point.x() <= rx0 || rx0 < start.x() {
        return None;
    }
    let ray_ratio = ray_ratio(rx0 - start.x(), fixed_point.x() - start.x());
    if ray_fraction.get() <= ray_ratio {
        return None;
    }
    *ray_fraction = RayFraction::new(ray_ratio);
    Some(Meet::Cross)
}

/// [`ray_rect`]'s bottom side (y0) for the ray `start`–`fixed_point` going up in y: a ray along y
/// (no x step) crosses it within its open span from at or below it to above it, nearer than
/// `ray_fraction` (a crossing, its ratio into `ray_fraction`); any other ray as [`segment_hit`];
/// none for none.
pub(super) fn ray_bottom_side(
    start: FixedPoint,
    fixed_point: FixedPoint,
    fixed_rect: FixedRect,
    ray_fraction: &mut RayFraction,
) -> Option<Meet> {
    let [rx0, ry0, rx1, _] = fixed_rect.get();
    if fixed_point.x() - start.x() != 0 {
        return segment_hit(
            start,
            fixed_point,
            FixedPoint::new(rx0, ry0),
            FixedPoint::new(rx1, ry0),
            ray_fraction,
        );
    }
    if !(rx0 < start.x() && start.x() < rx1) {
        return None;
    }
    if fixed_point.y() <= ry0 || ry0 < start.y() {
        return None;
    }
    let ray_ratio = ray_ratio(ry0 - start.y(), fixed_point.y() - start.y());
    if ray_fraction.get() <= ray_ratio {
        return None;
    }
    *ray_fraction = RayFraction::new(ray_ratio);
    Some(Meet::Cross)
}

/// [`ray_rect`]'s top side (y1) for the ray `start`–`fixed_point` going down in y, as
/// [`ray_bottom_side`]: a ray along y crosses it from at or above it to below it.
pub(super) fn ray_top_side(
    start: FixedPoint,
    fixed_point: FixedPoint,
    fixed_rect: FixedRect,
    ray_fraction: &mut RayFraction,
) -> Option<Meet> {
    let [rx0, _, rx1, ry1] = fixed_rect.get();
    if fixed_point.x() - start.x() != 0 {
        return segment_hit(
            start,
            fixed_point,
            FixedPoint::new(rx0, ry1),
            FixedPoint::new(rx1, ry1),
            ray_fraction,
        );
    }
    if !(rx0 < start.x() && start.x() < rx1) {
        return None;
    }
    if ry1 <= fixed_point.y() || start.y() < ry1 {
        return None;
    }
    let ray_ratio = ray_ratio(ry1 - start.y(), fixed_point.y() - start.y());
    if ray_fraction.get() <= ray_ratio {
        return None;
    }
    *ray_fraction = RayFraction::new(ray_ratio);
    Some(Meet::Cross)
}
