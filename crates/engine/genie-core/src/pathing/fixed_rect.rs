//! A rectangle in the pathing's fixed point ([`FixedRect`]).

use crate::pathing::FixedPoint;

/// A rectangle in fixed point (x0, y0, x1, y1 in units of 2^-22 tile): a shape of the
/// obstruction mesh, the box the short pather tests a point against.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FixedRect(
    /// Its x0, y0, x1 and y1.
    [i32; 4],
);

impl FixedRect {
    /// The rectangle from (`x0`, `y0`) to (`x1`, `y1`).
    #[must_use]
    pub const fn new(x0: i32, y0: i32, x1: i32, y1: i32) -> Self {
        Self([x0, y0, x1, y1])
    }

    /// Its x0, y0, x1 and y1, in that order.
    #[must_use]
    pub const fn get(self) -> [i32; 4] {
        self.0
    }

    /// Its low x.
    #[must_use]
    pub const fn x0(self) -> i32 {
        self.0[0]
    }

    /// Its low y.
    #[must_use]
    pub const fn y0(self) -> i32 {
        self.0[1]
    }

    /// Its high x.
    #[must_use]
    pub const fn x1(self) -> i32 {
        self.0[2]
    }

    /// Its high y.
    #[must_use]
    pub const fn y1(self) -> i32 {
        self.0[3]
    }

    /// Corner `k` (`FUN_14087c6b0`): 0 = (x0, y0), 1 = (x1, y0), 2 = (x1, y1), 3 = (x0, y1).
    #[must_use]
    pub const fn corner(self, k: u32) -> FixedPoint {
        let [x0, y0, x1, y1] = self.0;
        match k {
            0 => FixedPoint::new(x0, y0),
            1 => FixedPoint::new(x1, y0),
            2 => FixedPoint::new(x1, y1),
            _ => FixedPoint::new(x0, y1),
        }
    }
}
