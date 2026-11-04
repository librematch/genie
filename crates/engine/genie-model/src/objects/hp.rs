//! An object's hit points ([`HitPoints`]), with the game's `f32` operations, and a master's whole
//! hit points ([`MasterHitPoints`]).

use derive_more::{Add, AddAssign, Display, Neg, Sub, SubAssign};
use serde::Deserialize;
use serde::Serialize;
use std::ops::{Div, Mul, MulAssign};

/// Hit points as the game's `f32` holds them: an object's (`+0x40`), its powerup's room, what a
/// heal, a gain or a loss moves, a building's loss while it is built. A master holds whole hit
/// points ([`MasterHitPoints`], [`HitPoints::of_master`]). Each operator is the one `f32` operation
/// the game does, in its order; a comparison is `f32`'s (a NaN compares false).
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
pub struct HitPoints(f32);

impl HitPoints {
    /// None left.
    pub const ZERO: Self = Self(0.0);

    /// `points` hit points.
    #[must_use]
    pub const fn new(points: f32) -> Self {
        Self(points)
    }

    /// A master's whole hit points as the game converts them (`CVTSI2SS`, exact for an `i16`).
    #[must_use]
    pub fn of_master(points: MasterHitPoints) -> Self {
        Self(f32::from(points.0))
    }

    /// The points, as the save and the game's arithmetic hold them.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

/// A master's hit points (`+0x530`, the `.dat`'s `hit_points`): the whole points an object of it
/// starts with and heals up to (before powerups), a short the effects set, add to and scale
/// (capped at 32767 where the game caps them). Every short is kept; [`HitPoints::of_master`] is
/// the game's conversion.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct MasterHitPoints(i16);

impl MasterHitPoints {
    /// The points of the data's short `value`.
    #[must_use]
    pub const fn of_data(value: i16) -> Self {
        Self(value)
    }

    /// The short, as the data and the save hold it.
    #[must_use]
    pub const fn get(self) -> i16 {
        self.0
    }
}

impl genie_diff::Diff for MasterHitPoints {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

impl Mul<f32> for HitPoints {
    type Output = Self;

    /// The points scaled by `f` (one `f32` product, the points first).
    fn mul(self, f: f32) -> Self {
        Self(self.0 * f)
    }
}

impl MulAssign<f32> for HitPoints {
    /// The points scaled by `f` in place (an effect's multiplier).
    fn mul_assign(&mut self, f: f32) {
        self.0 *= f;
    }
}

impl Mul<HitPoints> for f32 {
    type Output = HitPoints;

    /// The points scaled by `self` (one `f32` product, the factor first).
    fn mul(self, hp: HitPoints) -> HitPoints {
        HitPoints(self * hp.0)
    }
}

impl Div<f32> for HitPoints {
    type Output = Self;

    /// The points divided by `f` (one `f32` quotient).
    fn div(self, f: f32) -> Self {
        Self(self.0 / f)
    }
}

impl Div for HitPoints {
    type Output = f32;

    /// The ratio of two amounts of points (one `f32` quotient: a share of the most).
    fn div(self, other: Self) -> f32 {
        self.0 / other.0
    }
}

impl genie_diff::Diff for HitPoints {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::{HitPoints, MasterHitPoints};

    /// Each operator is the one `f32` operation, bit for bit and in its operand order.
    #[test]
    fn hit_points_do_the_game_f32_operations() {
        let (a, b) = (HitPoints::new(0.1), HitPoints::new(0.2));
        assert_eq!((a + b).get().to_bits(), (0.1_f32 + 0.2).to_bits());
        assert_eq!((a - b).get().to_bits(), (0.1_f32 - 0.2).to_bits());
        assert_eq!((-a).get().to_bits(), (-0.1_f32).to_bits());
        assert_eq!((a * 3.0).get().to_bits(), (0.1_f32 * 3.0).to_bits());
        let mut scaled = a;
        scaled *= 3.0;
        assert_eq!(scaled, a * 3.0);
        assert_eq!((3.0 * a).get().to_bits(), (3.0_f32 * 0.1).to_bits());
        assert_eq!((a / 3.0).get().to_bits(), (0.1_f32 / 3.0).to_bits());
        assert_eq!((b / a).to_bits(), (0.2_f32 / 0.1).to_bits());
        let mut hp = HitPoints::new(45.0);
        hp -= HitPoints::new(4.4);
        assert_eq!(hp.get().to_bits(), (45.0_f32 - 4.4).to_bits());
        hp += HitPoints::new(4.4);
        assert_eq!(hp.get().to_bits(), (45.0_f32 - 4.4 + 4.4).to_bits());
        assert!(HitPoints::ZERO < a && a <= a);
        assert_eq!(HitPoints::new(f32::NAN).partial_cmp(&HitPoints::ZERO), None);
        assert_eq!(HitPoints::new(2.5).to_string(), "2.5");
    }

    /// A master's whole points convert exactly, the negative ones too.
    #[test]
    fn master_whole_points_convert_exactly() {
        let of = |points| HitPoints::of_master(MasterHitPoints::of_data(points));
        assert_eq!(of(25), HitPoints::new(25.0));
        assert_eq!(of(i16::MAX).get(), 32767.0);
        assert_eq!(of(-1).get(), -1.0);
        assert!(MasterHitPoints::default() < MasterHitPoints::of_data(i16::MAX));
    }
}
