//! Where the units of an order go: their centroid, offsets, spreads and grid points.

use crate::map::GridSize;
use crate::map::Point;
use crate::math::fmath::{maxss, sqrt};
use crate::world::layer::generated::GroupTuning;
use crate::world::layer::generated::OrderTuning;

/// The box of an order's units (`0x140e68b90`, the loop after the splitter): the least and the
/// greatest x and y, where a value below the least so far moves only the least (the greatest
/// starts at 0, the least at `OrderTuning::box_start`), and the sums. A box at most `OrderTuning::spread` each way gives
/// the centroid (the sums alone without a point); a wider one None.
pub(super) fn centroid(points: &[Point], order_tuning: &OrderTuning) -> Option<Point> {
    let (mut x, mut y) = (
        Spread::new(order_tuning.box_start),
        Spread::new(order_tuning.box_start),
    );
    for point in points {
        x.take(point.x());
        y.take(point.y());
    }
    if order_tuning.spread < x.most - x.least || order_tuning.spread < y.most - y.least {
        return None;
    }
    if points.is_empty() {
        return Some(Point::new(x.sum, y.sum));
    }
    let count = points.len() as f32;
    Some(Point::new(x.sum / count, y.sum / count))
}

/// One axis of [`centroid`]'s box: the least and the greatest value so far and the sum.
struct Spread {
    least: f32,
    most: f32,
    sum: f32,
}

impl Spread {
    /// No value yet: the least at `start`, the greatest and the sum at 0.
    const fn new(start: f32) -> Self {
        Self {
            least: start,
            most: 0.0,
            sum: 0.0,
        }
    }

    /// A value: below the least so far it moves only the least, else above the greatest the
    /// greatest; the sum takes it either way.
    fn take(&mut self, v: f32) {
        if self.least <= v {
            if self.most < v {
                self.most = v;
            }
        } else {
            self.least = v;
        }
        self.sum += v;
    }
}

/// A unit's point in an order to several (`0x140e68b90`): its offset from their centroid added to
/// the order's point, `OrderTuning::edge` inside the map where it falls off it.
pub(super) fn offset_point(
    position: Point,
    centroid: Point,
    point: Point,
    map: GridSize,
    order_tuning: &OrderTuning,
) -> Point {
    Point::new(
        inside_map(
            position.x() - centroid.x() + point.x(),
            map.width(),
            order_tuning,
        ),
        inside_map(
            position.y() - centroid.y() + point.y(),
            map.height(),
            order_tuning,
        ),
    )
}

/// An order's point on one axis kept on a map of `size` tiles: below 0 it becomes `OrderTuning::edge`, at
/// the size or beyond the size less `OrderTuning::edge`.
pub(super) fn inside_map(v: f32, size: i32, order_tuning: &OrderTuning) -> f32 {
    if v < 0.0 {
        order_tuning.edge
    } else if size as f32 <= v {
        size as f32 - order_tuning.edge
    } else {
        v
    }
}

/// FOLLOW's box (`FUN_14161c920`, the loop at `0x14161cc72`) over the places (x, y) of the loose
/// units that move, as its code has it: each unit's lesser and greater coordinate
/// (`CMOVBE` on `COMISS x, y` and `COMISS y, x`: a tie or NaN picks x) replace the last unit's,
/// which first fold into a running least (from `OrderTuning::box_start`) and greatest (from 0), each by
/// `COMISS`/`CMOVBE` (a tie or NaN takes the last unit's); the sums run on.
/// The box passes when the last unit's spread and the running one's are each at most
/// `OrderTuning::spread` (`COMISS`, `JC`: NaN fails); then it gives the mean place (the sums over the
/// count, none without units).
pub(super) fn follow_spread(points: &[Point], order_tuning: &OrderTuning) -> Option<Point> {
    use crate::math::fmath::{at_most_or_unordered, below_or_unordered};
    let (mut lesser, mut least, mut greater, mut greatest) = (
        order_tuning.box_start,
        order_tuning.box_start,
        0.0f32,
        0.0f32,
    );
    let (mut sx, mut sy) = (0.0f32, 0.0f32);
    for point in points {
        let (x, y) = (point.x(), point.y());
        least = if at_most_or_unordered(lesser, least) {
            lesser
        } else {
            least
        };
        greatest = if at_most_or_unordered(greatest, greater) {
            greater
        } else {
            greatest
        };
        lesser = if at_most_or_unordered(x, y) { x } else { y };
        greater = if at_most_or_unordered(y, x) { x } else { y };
        sx += x;
        sy += y;
    }
    if below_or_unordered(order_tuning.spread, greater - lesser)
        || below_or_unordered(order_tuning.spread, greatest - least)
    {
        return None;
    }
    if !points.is_empty() {
        let count = points.len() as i32 as f32;
        (sx, sy) = (sx / count, sy / count);
    }
    Some(Point::new(sx, sy))
}

/// A FOLLOW offset's coordinate on a map of `size` tiles (`FUN_14161c920`): above the size less
/// `GroupTuning::short_length` (at least 0, `MAXSS`) it is that (`COMISS`, `JA`), below 0 it is 0, else itself
/// (a NaN too).
pub(super) fn follow_axis(v: f32, size: i32, group_tuning: &GroupTuning) -> f32 {
    let most = maxss(size as f32 - group_tuning.short_length, 0.0);
    if v > most {
        most
    } else if 0.0 > v {
        0.0
    } else {
        v
    }
}

/// The cell of the `i`th of `n` units in MOVE's grid (`FUN_140e67800`): `side` cells a row, the
/// root of `n` times `OrderTuning::grid_area` truncated; the unit's column and row, less half the side, a
/// `OrderTuning::grid_step` each from the order's point, kept on the map ([`inside_map`]).
pub(super) fn grid_point(
    i: usize,
    n: usize,
    point: Point,
    map: GridSize,
    order_tuning: &OrderTuning,
) -> Point {
    let side = sqrt(n as i32 as f32 * order_tuning.grid_area) as i32;
    let (i, half) = (i as i32, side / 2);
    let x = (i % side - half) as f32 * order_tuning.grid_step + point.x();
    let y = (i / side - half) as f32 * order_tuning.grid_step + point.y();
    Point::new(
        inside_map(x, map.width(), order_tuning),
        inside_map(y, map.height(), order_tuning),
    )
}
