//! An amount of a player attribute ([`Amount`]), with the game's `f32` operations.

use crate::objects::masters::CarryCapacity;
use derive_more::{Add, AddAssign, Display, Neg, Sub, SubAssign};
use serde::Deserialize;
use serde::Serialize;
use std::iter::Sum;
use std::ops::{Div, Mul};

/// An amount of a player attribute (a resource) as the game's `f32` holds it: what an object
/// holds of its attribute (`+0x310`), what a unit carries at most (its master's
/// `attribute_max_amount`, [`Amount::of_master`]), what a gather, a hunt or a drop-off moves.
/// Each operator is the one `f32` operation the game does, in its order; a comparison is
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
pub struct Amount(f32);

impl Amount {
    /// None.
    pub const ZERO: Self = Self(0.0);

    /// `amount` of the attribute.
    #[must_use]
    pub const fn new(amount: f32) -> Self {
        Self(amount)
    }

    /// A master's whole amount as the game converts it (`CVTSI2SS`, exact for an `i16`).
    #[must_use]
    pub fn of_master(amount: CarryCapacity) -> Self {
        Self(f32::from(amount.get()))
    }

    /// The amount, as the save and the game's arithmetic hold it.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }

    /// The amount without its sign (`ANDPS` with the sign mask: a load's distance from the
    /// capacity).
    #[must_use]
    pub const fn abs(self) -> Self {
        Self(self.0.abs())
    }
}

impl Mul<f32> for Amount {
    type Output = Self;

    /// The amount scaled by `factor` (one `f32` product, the amount first).
    fn mul(self, factor: f32) -> Self {
        Self(self.0 * factor)
    }
}

impl Mul<Amount> for f32 {
    type Output = Amount;

    /// The amount scaled by `self` (one `f32` product, the factor first).
    fn mul(self, amount: Amount) -> Amount {
        Amount(self * amount.0)
    }
}

impl Div<f32> for Amount {
    type Output = Self;

    /// The amount divided by `divisor` (one `f32` quotient).
    fn div(self, divisor: f32) -> Self {
        Self(self.0 / divisor)
    }
}

impl Div for Amount {
    type Output = f32;

    /// The ratio of two amounts (one `f32` quotient: a share).
    fn div(self, other: Self) -> f32 {
        self.0 / other.0
    }
}

impl Sum for Amount {
    /// The `f32` sum in the iterator's order from 0.0, as the game's loops start: `f32`'s own
    /// `Sum` starts at −0.0, so a sum of nothing or of −0.0 alone would differ in its sign.
    fn sum<I: Iterator<Item = Self>>(amounts: I) -> Self {
        amounts.fold(Self::ZERO, |sum, amount| sum + amount)
    }
}

impl genie_diff::Diff for Amount {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::CarryCapacity;
    use crate::players::Amount;
    use std::iter::{empty, once};

    /// A sum folds from 0.0 in order: a sum of nothing and of −0.0 alone are 0.0, not −0.0.
    #[test]
    fn amounts_sum_from_zero_in_order() {
        let none: Amount = empty().sum();
        assert_eq!(none.get().to_bits(), 0.0_f32.to_bits());
        let negative_zero: Amount = once(Amount::new(-0.0)).sum();
        assert_eq!(negative_zero.get().to_bits(), 0.0_f32.to_bits());
        let three = [0.1, 0.2, 0.3].map(Amount::new).into_iter().sum::<Amount>();
        assert_eq!(three.get().to_bits(), (0.0_f32 + 0.1 + 0.2 + 0.3).to_bits());
    }

    /// Each operator is the one `f32` operation, bit for bit and in its operand order.
    #[test]
    fn amounts_do_the_game_f32_operations() {
        let (first, second) = (Amount::new(0.1), Amount::new(0.2));
        assert_eq!((first + second).get().to_bits(), (0.1_f32 + 0.2).to_bits());
        assert_eq!((first - second).get().to_bits(), (0.1_f32 - 0.2).to_bits());
        assert_eq!((-first).get().to_bits(), (-0.1_f32).to_bits());
        assert_eq!((first * 3.0).get().to_bits(), (0.1_f32 * 3.0).to_bits());
        assert_eq!((3.0 * first).get().to_bits(), (3.0_f32 * 0.1).to_bits());
        assert_eq!((first / 3.0).get().to_bits(), (0.1_f32 / 3.0).to_bits());
        assert_eq!((second / first).to_bits(), (0.2_f32 / 0.1).to_bits());
        let mut held = Amount::new(10.0);
        held += Amount::new(0.7);
        assert_eq!(held.get().to_bits(), (10.0_f32 + 0.7).to_bits());
        held -= Amount::new(0.7);
        assert_eq!(held.get().to_bits(), (10.0_f32 + 0.7 - 0.7).to_bits());
        assert_eq!(
            (first - second).abs().get().to_bits(),
            (0.1_f32 - 0.2).abs().to_bits()
        );
        assert_eq!(Amount::new(-0.0).abs().get().to_bits(), 0.0_f32.to_bits());
        assert!(Amount::ZERO < first && first <= first);
        assert_eq!(Amount::new(f32::NAN).partial_cmp(&Amount::ZERO), None);
        assert_eq!(Amount::new(2.5).to_string(), "2.5");
    }

    /// A master's whole amount converts exactly, a negative one too.
    #[test]
    fn master_whole_amount_converts_exactly() {
        let of = |amount| Amount::of_master(CarryCapacity::of_data(amount));
        assert_eq!(of(10), Amount::new(10.0));
        assert_eq!(of(i16::MAX).get(), 32767.0);
        assert_eq!(of(-1).get(), -1.0);
        assert!(CarryCapacity::default() < CarryCapacity::of_data(1));
    }
}
