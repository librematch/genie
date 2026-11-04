//! A point on the ground as one type ([`Point`]): x and y, apart from a place with its height
//! ([`crate::geometry::Position`]) and an object's radii (genie-core's `map::Footprint`).

use crate::conversions::cvttss2si;
use crate::geometry::Position;
use crate::geometry::Tile;

/// An axis of the ground: a routine that runs along either one names it, not a number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Axis {
    /// The map's x.
    X,
    /// The map's y.
    Y,
}

/// A point on the map's ground, x and y in tiles: where a command sends its units, builds,
/// unloads or flares (a command's two floats, which the game takes as the packet holds them,
/// any bits), and a place's x and y where a system reads only those ([`Position::ground`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point(
    /// Its x and y.
    [f32; 2],
);

impl Point {
    /// The point at the origin.
    pub const ORIGIN: Self = Self([0.0; 2]);

    /// The point at `x`, `y`.
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self([x, y])
    }

    /// Its x and y, in that order (a save's or a packet's two floats).
    #[must_use]
    pub const fn get(self) -> [f32; 2] {
        self.0
    }

    /// Its x.
    #[must_use]
    pub const fn x(self) -> f32 {
        self.0[0]
    }

    /// Its y.
    #[must_use]
    pub const fn y(self) -> f32 {
        self.0[1]
    }

    /// The point with x `x`.
    #[must_use]
    pub const fn with_x(self, x: f32) -> Self {
        Self([x, self.0[1]])
    }

    /// The point with y `y`.
    #[must_use]
    pub const fn with_y(self, y: f32) -> Self {
        Self([self.0[0], y])
    }

    /// Its component along `axis`.
    #[must_use]
    pub const fn along(self, axis: Axis) -> f32 {
        match axis {
            Axis::X => self.0[0],
            Axis::Y => self.0[1],
        }
    }

    /// The point with its component along `axis` `v`.
    #[must_use]
    pub const fn with_along(self, axis: Axis, v: f32) -> Self {
        match axis {
            Axis::X => self.with_x(v),
            Axis::Y => self.with_y(v),
        }
    }

    /// The place at this point and height `z`.
    #[must_use]
    pub const fn with_height(self, z: f32) -> Position {
        Position::new(self.0[0], self.0[1], z)
    }

    /// The tile it lies in, x and y each cut toward zero by `CVTTSS2SI` (a NaN or a value past
    /// 32 bits is `i32::MIN`).
    #[must_use]
    pub fn tile(self) -> Tile {
        Tile::new(cvttss2si(self.0[0]), cvttss2si(self.0[1]))
    }
}

impl From<[f32; 2]> for Point {
    /// The point of two floats in the map's order (x, y), as a save or a packet holds them.
    fn from(c: [f32; 2]) -> Self {
        Self(c)
    }
}

impl genie_diff::Diff for Point {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

#[cfg(test)]
mod tests {
    use crate::geometry::{Axis, Point, Position, Tile};

    /// A point's x and y keep their bits, and a height makes it a place.
    #[test]
    fn point_keeps_its_bits_and_height_makes_it_place() {
        let point = Point::new(-0.0, f32::from_bits(0x7fc0_0001));
        assert_eq!(point.get().map(f32::to_bits), [0x8000_0000, 0x7fc0_0001]);
        assert_eq!(
            (point.x().to_bits(), point.y().to_bits()),
            (0x8000_0000, 0x7fc0_0001)
        );
        assert_eq!(Point::from([1.0, 2.0]), Point::new(1.0, 2.0));
        assert_eq!(Point::ORIGIN.with_x(1.0).with_y(2.0), Point::new(1.0, 2.0));
        assert_eq!(
            Point::new(1.0, 2.0).with_height(3.0),
            Position::new(1.0, 2.0, 3.0)
        );
        assert_eq!(Position::new(1.0, 2.0, 3.0).ground(), Point::new(1.0, 2.0));
    }

    /// A point's tile cuts x and y toward zero; a NaN or a value past 32 bits is `i32::MIN`.
    #[test]
    fn point_tile_cuts_toward_zero() {
        assert_eq!(Point::new(3.9, -0.5).tile(), Tile::new(3, 0));
        assert_eq!(Point::new(-1.5, 7.0).tile(), Tile::new(-1, 7));
        assert_eq!(
            Point::new(f32::NAN, 3e9).tile(),
            Tile::new(i32::MIN, i32::MIN)
        );
    }

    /// A point's component along an axis is its x or its y, and setting it changes only that.
    #[test]
    fn point_component_along_axis_is_that_coordinate() {
        let point = Point::new(1.0, 2.0);
        assert_eq!((point.along(Axis::X), point.along(Axis::Y)), (1.0, 2.0));
        assert_eq!(point.with_along(Axis::X, 5.0), Point::new(5.0, 2.0));
        assert_eq!(point.with_along(Axis::Y, 5.0), Point::new(1.0, 5.0));
    }
}
