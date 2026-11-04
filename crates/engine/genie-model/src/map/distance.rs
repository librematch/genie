//! A distance on the map in tiles ([`Distance`]) and its square ([`DistanceSquared`]), each
//! with the game's `f32` operations.

use derive_more::{Add, AddAssign, Display, Neg, Sub, SubAssign};
use serde::Deserialize;
use serde::Serialize;
use std::ops::{Div, Mul, MulAssign};

/// A distance on the map in tiles as the game's `f32` holds it: a master's range, minimum range,
/// work range, search radius and line of sight, a reach, a gap between two footprints, a group's
/// ranges. Each operator is the one `f32` operation the game does, in its order; a comparison is
/// `f32`'s (a NaN compares false).
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    PartialOrd,
    Serialize,
    Deserialize,
    Display,
    Add,
    Sub,
    Neg,
    AddAssign,
    SubAssign,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Distance(f32);

impl Distance {
    /// No distance.
    pub const ZERO: Self = Self(0.0);
    /// The farthest, the game's `FLT_MAX`: no distance found yet.
    pub const MAX: Self = Self(f32::MAX);

    /// A distance of `tiles` tiles.
    #[must_use]
    pub const fn new(tiles: f32) -> Self {
        Self(tiles)
    }

    /// The tiles, as the save, the `.dat` and the game's arithmetic hold them.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }

    /// The distance times itself (one `f32` product): what a squared gap compares with.
    #[must_use]
    pub fn squared(self) -> DistanceSquared {
        DistanceSquared(self.0 * self.0)
    }
}

impl Mul<f32> for Distance {
    type Output = Self;

    /// The distance scaled by `f` (one `f32` product, the distance first).
    fn mul(self, f: f32) -> Self {
        Self(self.0 * f)
    }
}

impl Mul<Distance> for f32 {
    type Output = Distance;

    /// The distance scaled by `self` (one `f32` product, the factor first).
    fn mul(self, d: Distance) -> Distance {
        Distance(self * d.0)
    }
}

impl MulAssign<f32> for Distance {
    /// The distance scaled by `f` in place (one `f32` product, the distance first: a master's
    /// range times a technology's factor).
    fn mul_assign(&mut self, f: f32) {
        self.0 *= f;
    }
}

impl Div<f32> for Distance {
    type Output = Self;

    /// The distance divided by `f` (one `f32` quotient).
    fn div(self, f: f32) -> Self {
        Self(self.0 / f)
    }
}

impl Div for Distance {
    type Output = f32;

    /// The ratio of two distances (one `f32` quotient).
    fn div(self, other: Self) -> f32 {
        self.0 / other.0
    }
}

impl genie_diff::Diff for Distance {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

/// A distance in whole tiles as the game's `int` holds it: a player's formation distances
/// (`mMinColumnDistance`, `mColumnToLineDistance`) and its attack-move setting
/// (`mAttackMoveMaxDistance`); the readers convert it (`CVTSI2SS`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WholeDistance(i32);

impl WholeDistance {
    /// No distance.
    pub const ZERO: Self = Self(0);
    /// None at all (−1): a pick without a search distance.
    pub const NONE: Self = Self(-1);

    /// The distance of the save's or the command's int `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// The distance as a float (`CVTSI2SS`, rounding beyond 2^24).
    #[must_use]
    pub const fn distance(self) -> Distance {
        Distance(self.0 as f32)
    }

    /// The int, as the save holds it.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

/// A distance squared, in square tiles, as the game's `f32` holds it (a `gap_squared`): the game
/// compares squares where it saves the root. A comparison is `f32`'s (a NaN compares false).
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd, Display)]
pub struct DistanceSquared(f32);

impl DistanceSquared {
    /// No distance.
    pub const ZERO: Self = Self(0.0);
    /// The farthest, the game's `FLT_MAX`: no square found yet.
    pub const MAX: Self = Self(f32::MAX);

    /// A square of `tiles` square tiles.
    #[must_use]
    pub const fn new(tiles: f32) -> Self {
        Self(tiles)
    }

    /// The square tiles, as the save and the game's arithmetic hold them.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl genie_diff::Diff for DistanceSquared {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::map::{Distance, DistanceSquared};

    /// Each operator is the one `f32` operation, bit for bit and in its operand order.
    #[test]
    fn distances_do_the_game_f32_operations() {
        let (a, b) = (Distance::new(0.1), Distance::new(0.2));
        assert_eq!((a + b).get().to_bits(), (0.1_f32 + 0.2).to_bits());
        assert_eq!((a - b).get().to_bits(), (0.1_f32 - 0.2).to_bits());
        assert_eq!((-a).get().to_bits(), (-0.1_f32).to_bits());
        assert_eq!((a * 3.0).get().to_bits(), (0.1_f32 * 3.0).to_bits());
        assert_eq!((3.0 * a).get().to_bits(), (3.0_f32 * 0.1).to_bits());
        assert_eq!((a / 3.0).get().to_bits(), (0.1_f32 / 3.0).to_bits());
        assert_eq!((b / a).to_bits(), (0.2_f32 / 0.1).to_bits());
        let mut d = Distance::new(1.0);
        d *= 3.0;
        assert_eq!(d.get().to_bits(), 3.0_f32.to_bits());
        d = Distance::new(1.0);
        d -= a;
        assert_eq!(d.get().to_bits(), (1.0_f32 - 0.1).to_bits());
        d += a;
        assert_eq!(d.get().to_bits(), (1.0_f32 - 0.1 + 0.1).to_bits());
        assert!(Distance::ZERO < a && a <= a && a < Distance::MAX);
        assert_eq!(Distance::new(f32::NAN).partial_cmp(&Distance::ZERO), None);
        assert_eq!(Distance::new(0.5).to_string(), "0.5");
    }

    /// A NaN factor first keeps its payload in the product (`mulss` returns its first NaN
    /// operand), so the factor's side is part of the operation.
    #[test]
    fn nan_factor_keeps_its_side() {
        let nan = f32::from_bits(0x7fc0_0001);
        assert_eq!(
            (nan * Distance::new(2.0)).get().to_bits(),
            (nan * 2.0).to_bits()
        );
        assert_eq!(
            (Distance::new(2.0) * nan).get().to_bits(),
            (2.0 * nan).to_bits()
        );
    }

    /// A distance squared is its one `f32` product with itself.
    #[test]
    fn distance_squared_is_its_product_with_itself() {
        let d = Distance::new(1.1);
        assert_eq!(d.squared(), DistanceSquared::new(1.1 * 1.1));
        assert_eq!(d.squared().get().to_bits(), (1.1_f32 * 1.1).to_bits());
        assert!(DistanceSquared::ZERO < d.squared());
        assert_eq!(DistanceSquared::new(2.25).to_string(), "2.25");
    }

    /// A whole distance converts to its float, rounding beyond 2^24 as `CVTSI2SS`.
    #[test]
    fn whole_distance_converts_to_its_float() {
        use crate::map::WholeDistance;
        assert_eq!(WholeDistance::of_data(10).distance(), Distance::new(10.0));
        assert_eq!(WholeDistance::of_data(-16).distance().get(), -16.0);
        assert_eq!(
            WholeDistance::of_data(16_777_217).distance().get(),
            16_777_216.0
        );
        assert_eq!(WholeDistance::ZERO.get(), 0);
        assert!(WholeDistance::of_data(3) < WholeDistance::of_data(4));
    }
}
