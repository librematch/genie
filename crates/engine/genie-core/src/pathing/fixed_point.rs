//! A point in the pathing's fixed point ([`FixedPoint`]).

use std::fmt::{Display, Error, Formatter};

/// A point in fixed point, x and y in units of 2^-22 tile ([`crate::pathing::mesh::ONE`] is a
/// tile): the obstruction mesh's corners and contacts and the short pather's points.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FixedPoint(
    /// Its x and y.
    [i32; 2],
);

impl FixedPoint {
    /// The point at the origin.
    pub const ORIGIN: Self = Self([0; 2]);

    /// The point at `x`, `y`.
    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self([x, y])
    }

    /// Its x and y, in that order.
    #[must_use]
    pub const fn get(self) -> [i32; 2] {
        self.0
    }

    /// Its x.
    #[must_use]
    pub const fn x(self) -> i32 {
        self.0[0]
    }

    /// Its y.
    #[must_use]
    pub const fn y(self) -> i32 {
        self.0[1]
    }

    /// The point with x `x`.
    #[must_use]
    pub const fn with_x(self, x: i32) -> Self {
        Self([x, self.0[1]])
    }

    /// The point with y `y`.
    #[must_use]
    pub const fn with_y(self, y: i32) -> Self {
        Self([self.0[0], y])
    }
}

impl Display for FixedPoint {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "({}, {})", self.0[0], self.0[1])
    }
}

#[cfg(test)]
mod tests {
    use crate::pathing::FixedPoint;

    /// x and y keep their places; each `with_` changes one.
    #[test]
    fn fixed_point_keeps_both_coordinates() {
        let p = FixedPoint::new(3, -4);
        assert_eq!((p.x(), p.y(), p.get()), (3, -4, [3, -4]));
        assert_eq!(p.with_x(7), FixedPoint::new(7, -4));
        assert_eq!(p.with_y(7), FixedPoint::new(3, 7));
        assert_eq!(FixedPoint::ORIGIN, FixedPoint::default());
        assert_eq!(p.to_string(), "(3, -4)");
    }
}
