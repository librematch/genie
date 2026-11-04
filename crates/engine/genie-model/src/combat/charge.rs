//! An object's charge ([`Charge`]) and how fast it fills ([`ChargeRate`]), with the game's `f32`
//! operations.

use crate::world::Seconds;
use derive_more::{AddAssign, Display, Sub, SubAssign};
use serde::Deserialize;
use serde::Serialize;
use std::ops::{Mul, MulAssign};

/// A charge as the game's `f32` holds it: an object's charge (`+0x9e0`) and its master's most
/// (`max_charge`, `+0x918`). A ready charge (its most for charge types 1 and 3, else 1) adds
/// itself to the hit's attack. Each operator is the one `f32` operation the game does, in its
/// order; a comparison is `f32`'s (a NaN compares false).
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
    Sub,
    AddAssign,
    SubAssign,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Charge(f32);

impl Charge {
    /// None: an empty charge.
    pub const ZERO: Self = Self(0.0);

    /// A charge of `points`.
    #[must_use]
    pub const fn new(points: f32) -> Self {
        Self(points)
    }

    /// The points, as the save and the game's arithmetic hold them.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }

    /// The charge without its sign (`ANDPS` with the sign mask: a change of the most).
    #[must_use]
    pub const fn abs(self) -> Self {
        Self(self.0.abs())
    }
}

impl MulAssign<f32> for Charge {
    /// The charge scaled by `f` in place (an effect's multiplier).
    fn mul_assign(&mut self, f: f32) {
        self.0 *= f;
    }
}

impl genie_diff::Diff for Charge {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

/// How fast a charge fills, its points per second as the game's `f32` holds them (a master's
/// `recharge_rate`); over [`Seconds`] it is a [`Charge`]. Each operator is the one `f32`
/// operation the game does, in its order.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(
    Clone, Copy, Debug, Default, PartialEq, PartialOrd, Serialize, Deserialize, Display, AddAssign,
)]
pub struct ChargeRate(f32);

impl ChargeRate {
    /// No refill.
    pub const ZERO: Self = Self(0.0);

    /// `per_second` points a second.
    #[must_use]
    pub const fn new(per_second: f32) -> Self {
        Self(per_second)
    }

    /// The points a second, as the save and the game's arithmetic hold them.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl MulAssign<f32> for ChargeRate {
    /// The rate scaled by `f` in place (an effect's multiplier).
    fn mul_assign(&mut self, f: f32) {
        self.0 *= f;
    }
}

impl Mul<Seconds> for ChargeRate {
    type Output = Charge;

    /// The charge it fills in `dt` (one `f32` product, the rate first).
    fn mul(self, dt: Seconds) -> Charge {
        Charge(self.0 * dt.get())
    }
}

impl genie_diff::Diff for ChargeRate {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::combat::{Charge, ChargeRate};
    use crate::world::Seconds;

    /// Each operator is the one `f32` operation, bit for bit and in its operand order; a
    /// comparison is `f32`'s.
    #[test]
    fn charges_do_the_game_f32_operations() {
        let mut c = Charge::new(0.1);
        c += Charge::new(0.2);
        assert_eq!(c.get().to_bits(), (0.1_f32 + 0.2).to_bits());
        c *= 3.0;
        assert_eq!(c.get().to_bits(), ((0.1_f32 + 0.2) * 3.0).to_bits());
        c -= Charge::new(0.4);
        assert_eq!(c.get().to_bits(), ((0.1_f32 + 0.2) * 3.0 - 0.4).to_bits());
        let less = Charge::new(1.0) - Charge::new(2.5);
        assert_eq!(less.get().to_bits(), (1.0_f32 - 2.5).to_bits());
        assert_eq!(less.abs(), Charge::new(1.5));
        assert_eq!(Charge::new(-0.0).abs().get().to_bits(), 0);
        let mut r = ChargeRate::new(0.7);
        r += ChargeRate::new(0.1);
        assert_eq!(r.get().to_bits(), (0.7_f32 + 0.1).to_bits());
        r *= 2.0;
        assert_eq!(r.get().to_bits(), ((0.7_f32 + 0.1) * 2.0).to_bits());
        assert_eq!(
            (r * Seconds::new(0.3)).get().to_bits(),
            (((0.7_f32 + 0.1) * 2.0) * 0.3).to_bits()
        );
        assert!(Charge::ZERO < c && ChargeRate::ZERO < r);
        assert_eq!(Charge::new(f32::NAN).partial_cmp(&c), None);
        assert_eq!(
            (
                Charge::new(1.5).to_string(),
                ChargeRate::new(2.5).to_string()
            ),
            ("1.5".to_owned(), "2.5".to_owned())
        );
    }
}
