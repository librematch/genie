//! An angle in radians ([`Angle`]), with the game's `f32` operations.

use derive_more::{Add, Display, Neg, Sub};
use std::ops::{Div, Mul};

/// An angle on the map in radians, as the game's `f32` holds it: where an object faces
/// (`+0x7e8`, 0 to 2π, genie-core's `objects::facing`), a turn or a yaw, a missile's spawn angle,
/// a blast's cone. Each operator is the one `f32` operation the game does, in its order; a
/// comparison is `f32`'s (a NaN compares false).
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd, Display, Add, Sub, Neg)]
pub struct Angle(f32);

impl Angle {
    /// No angle: facing along the map's first diagonal's start, no turn.
    pub const ZERO: Self = Self(0.0);

    /// `radians`.
    #[must_use]
    pub const fn new(radians: f32) -> Self {
        Self(radians)
    }

    /// The radians, as the save and the game's arithmetic hold them.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl Mul<f32> for Angle {
    type Output = Self;

    /// The angle scaled by `f` (one `f32` product, the angle first).
    fn mul(self, f: f32) -> Self {
        Self(self.0 * f)
    }
}

impl Div<f32> for Angle {
    type Output = Self;

    /// The angle divided by `f` (one `f32` quotient: a facet's share of a turn).
    fn div(self, f: f32) -> Self {
        Self(self.0 / f)
    }
}

impl genie_diff::Diff for Angle {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::Angle;

    /// Each operator is the one `f32` operation, bit for bit and in its operand order.
    #[test]
    fn angles_do_the_game_f32_operations() {
        let (a, b) = (Angle::new(0.1), Angle::new(0.2));
        assert_eq!((a + b).get().to_bits(), (0.1_f32 + 0.2).to_bits());
        assert_eq!((a - b).get().to_bits(), (0.1_f32 - 0.2).to_bits());
        assert_eq!((-a).get().to_bits(), (-0.1_f32).to_bits());
        assert_eq!((a * 3.0).get().to_bits(), (0.1_f32 * 3.0).to_bits());
        assert_eq!((a / 3.0).get().to_bits(), (0.1_f32 / 3.0).to_bits());
        assert!(Angle::ZERO < a && a <= a);
        assert_eq!(Angle::new(f32::NAN).partial_cmp(&Angle::ZERO), None);
        assert_eq!(Angle::new(2.5).to_string(), "2.5");
    }
}
