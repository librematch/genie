//! How fast an amount changes ([`AmountRate`], an attribute's amount per second), with the
//! game's `f32` operations.

use crate::players::Amount;
use crate::world::Seconds;
use derive_more::{Display, Neg};
use serde::Deserialize;
use serde::Serialize;
use std::ops::Mul;

/// An amount of a player attribute per second, as the game's `f32` holds it: what a dead
/// object's remains lose of their attribute a second (a master's `resource_decay`, `+0x620`),
/// negated on the hunted target whose decay a hunt stopped. Over [`Seconds`] it is an
/// [`Amount`]. Each operator is the one `f32` operation the game does, in its order; a
/// comparison is `f32`'s (a NaN compares false).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, PartialOrd, Serialize, Deserialize, Display, Neg,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct AmountRate(f32);

impl AmountRate {
    /// No change.
    pub const ZERO: Self = Self(0.0);

    /// `per_second` of the attribute a second.
    #[must_use]
    pub const fn new(per_second: f32) -> Self {
        Self(per_second)
    }

    /// The amount a second, as the save and the game's arithmetic hold it.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl Mul<Seconds> for AmountRate {
    type Output = Amount;

    /// The amount in `dt` (one `f32` product, the rate first).
    fn mul(self, dt: Seconds) -> Amount {
        Amount::new(self.0 * dt.get())
    }
}

impl genie_diff::Diff for AmountRate {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

#[cfg(test)]
mod tests {
    use crate::players::AmountRate;
    use crate::world::Seconds;

    /// A rate over seconds is an amount, one `f32` product in the game's operand order; the
    /// negation flips the sign bit alone, a NaN's too.
    #[test]
    fn an_amount_rate_over_seconds_is_an_amount() {
        let (amount_rate, dt) = (AmountRate::new(0.7), Seconds::new(0.3));
        assert_eq!(
            (amount_rate * dt).get().to_bits(),
            (0.7_f32 * 0.3).to_bits()
        );
        assert_eq!(
            (-amount_rate).get().to_bits(),
            0.7_f32.to_bits() ^ 0x8000_0000
        );
        let nan = f32::from_bits(0x7fc0_0001);
        assert_eq!(
            (-AmountRate::new(nan)).get().to_bits(),
            0x7fc0_0001 ^ 0x8000_0000
        );
        assert!(AmountRate::ZERO < amount_rate);
        assert_eq!(AmountRate::new(f32::NAN).partial_cmp(&amount_rate), None);
        assert_eq!(AmountRate::new(1.5).to_string(), "1.5");
    }
}
