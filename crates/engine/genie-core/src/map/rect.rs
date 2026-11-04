//! A box on the map's ground as one type ([`Rect`]): two corners, x0 and y0 then x1 and y1, apart
//! from a point ([`Point`]) and a walk between two points ([`Segment`]).

use crate::map::Distance;
use crate::map::GridSize;
use crate::map::Point;

/// A box on the map's ground, x0, y0, x1 and y1 in tiles: an obstruction record's box
/// (`+0x20`..`+0x2c`), a query's rectangle, a pather's bounds. Its corners need not be in order: a
/// test that needs the low and the high edges orders them ([`Rect::ordered`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect(
    /// Its x0, y0, x1 and y1.
    [f32; 4],
);

impl Rect {
    /// The box from corner `x0`, `y0` to corner `x1`, `y1`.
    #[must_use]
    pub const fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self([x0, y0, x1, y1])
    }

    /// The box from corner `from` to corner `to`.
    #[must_use]
    pub const fn between(from: Point, to: Point) -> Self {
        Self([from.x(), from.y(), to.x(), to.y()])
    }

    /// The box a walk from `from` to `to` spans: the lower x and y of the two (`f32::min`) and the
    /// higher (`f32::max`).
    #[must_use]
    pub const fn spanning(from: Point, to: Point) -> Self {
        Self([
            from.x().min(to.x()),
            from.y().min(to.y()),
            from.x().max(to.x()),
            from.y().max(to.y()),
        ])
    }

    /// Its x0, y0, x1 and y1, in that order (a record's four floats).
    #[must_use]
    pub const fn get(self) -> [f32; 4] {
        self.0
    }

    /// Its first corner's x.
    #[must_use]
    pub const fn x0(self) -> f32 {
        self.0[0]
    }

    /// Its first corner's y.
    #[must_use]
    pub const fn y0(self) -> f32 {
        self.0[1]
    }

    /// Its second corner's x.
    #[must_use]
    pub const fn x1(self) -> f32 {
        self.0[2]
    }

    /// Its second corner's y.
    #[must_use]
    pub const fn y1(self) -> f32 {
        self.0[3]
    }

    /// Its first corner, x0 and y0.
    #[must_use]
    pub const fn corner0(self) -> Point {
        Point::new(self.0[0], self.0[1])
    }

    /// Its second corner, x1 and y1.
    #[must_use]
    pub const fn corner1(self) -> Point {
        Point::new(self.0[2], self.0[3])
    }

    /// The box `by` larger on each side, as the game grows one: `x0 - by`, `y0 - by`, then
    /// `by + x1`, `by + y1`.
    #[must_use]
    pub fn grown(self, by: Distance) -> Self {
        let [x0, y0, x1, y1] = self.0;
        Self([x0 - by.get(), y0 - by.get(), by.get() + x1, by.get() + y1])
    }

    /// Whether it and `other` overlap, edges included: `x0 <= other.x1`, `other.x0 <= x1`, then
    /// the same on y, each a `<=` (a NaN does not overlap).
    #[must_use]
    pub fn overlaps(self, other: Self) -> bool {
        self.x0() <= other.x1()
            && other.x0() <= self.x1()
            && self.y0() <= other.y1()
            && other.y0() <= self.y1()
    }

    /// Whether it holds point `point`, edges included: `point.x <= x1`, `x0 <= point.x`, then the
    /// same on y (a NaN is not held).
    #[must_use]
    pub fn holds(self, point: Point) -> bool {
        point.x() <= self.x1()
            && self.x0() <= point.x()
            && point.y() <= self.y1()
            && self.y0() <= point.y()
    }

    /// Whether it leaves a map of `size` tiles (width, height): a low corner below 0 or a high
    /// one beyond the size (`COMISS`: a NaN does not leave).
    #[must_use]
    pub fn leaves_map(self, map: GridSize) -> bool {
        let (width, height) = (map.width(), map.height());
        self.x0() < 0.0
            || self.y0() < 0.0
            || (width as f32) < self.x1()
            || (height as f32) < self.y1()
    }

    /// The box with its corners in order, low x and y first: each axis's second value when it is
    /// below the first (`x1 < x0`), else the first, for the low edge, and the other way round
    /// for the high edge.
    #[must_use]
    pub fn ordered(self) -> Self {
        let [x0, y0, x1, y1] = self.0;
        Self([
            if x1 < x0 { x1 } else { x0 },
            if y1 < y0 { y1 } else { y0 },
            if x0 < x1 { x1 } else { x0 },
            if y0 < y1 { y1 } else { y0 },
        ])
    }
}

impl From<[f32; 4]> for Rect {
    /// The box of four floats in their order (x0, y0, x1, y1), as a record holds them.
    fn from(corners: [f32; 4]) -> Self {
        Self(corners)
    }
}

impl genie_diff::Diff for Rect {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

/// A straight walk on the ground from one point to another: a step a pather or a mover tests
/// against the records.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Segment {
    /// Where it starts.
    pub(crate) from: Point,
    /// Where it ends.
    pub(crate) to: Point,
}

#[cfg(test)]
mod tests {
    use crate::map::Distance;
    use crate::map::{GridSize, Point, Rect};

    /// A box keeps its corners' bits in their order, grows the low corner by subtracting first,
    /// overlaps another with the edges included but not with a NaN, and orders its corners.
    #[test]
    fn box_keeps_its_bits_grows_overlaps_and_orders_its_corners() {
        let rect = Rect::new(-0.0, 1.0, f32::from_bits(0x7fc0_0001), 3.0);
        assert_eq!(
            rect.get().map(f32::to_bits),
            [0x8000_0000, 1.0f32.to_bits(), 0x7fc0_0001, 3.0f32.to_bits()]
        );
        assert_eq!(
            Rect::from([1.0, 2.0, 3.0, 4.0]),
            Rect::new(1.0, 2.0, 3.0, 4.0)
        );
        assert_eq!(
            Rect::between(Point::new(1.0, 2.0), Point::new(3.0, 4.0)),
            Rect::new(1.0, 2.0, 3.0, 4.0)
        );
        let rect = Rect::new(1.0, 2.0, 3.0, 4.0);
        assert_eq!(
            (rect.x0(), rect.y0(), rect.x1(), rect.y1()),
            (1.0, 2.0, 3.0, 4.0)
        );
        assert_eq!(
            (rect.corner0(), rect.corner1()),
            (Point::new(1.0, 2.0), Point::new(3.0, 4.0))
        );
        assert_eq!(
            rect.grown(Distance::new(0.5)),
            Rect::new(0.5, 1.5, 3.5, 4.5)
        );
        assert!(rect.overlaps(Rect::new(3.0, 4.0, 5.0, 6.0)));
        assert!(!rect.overlaps(Rect::new(3.5, 4.0, 5.0, 6.0)));
        assert!(!rect.overlaps(Rect::new(f32::NAN, 2.0, 3.0, 4.0)));
        assert_eq!(
            Rect::new(3.0, 4.0, 1.0, 2.0).ordered(),
            Rect::new(1.0, 2.0, 3.0, 4.0)
        );
        assert_eq!(rect.ordered(), rect);
        assert!(rect.holds(Point::new(1.0, 4.0)) && rect.holds(Point::new(2.0, 3.0)));
        assert!(!rect.holds(Point::new(0.5, 3.0)) && !rect.holds(Point::new(2.0, f32::NAN)));
        assert_eq!(
            Rect::spanning(Point::new(3.0, 2.0), Point::new(1.0, 4.0)),
            Rect::new(1.0, 2.0, 3.0, 4.0)
        );
        let map = GridSize::new;
        assert!(!rect.leaves_map(map(3, 4)));
        assert!(rect.leaves_map(map(2, 4)) && rect.leaves_map(map(3, 3)));
        assert!(Rect::new(-0.5, 2.0, 3.0, 4.0).leaves_map(map(9, 9)));
        assert!(!Rect::new(f32::NAN, 2.0, 3.0, 4.0).leaves_map(map(3, 4)));
    }
}
