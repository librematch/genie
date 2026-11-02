//! An amount of a player attribute ([`Amount`]), with the game's `f32` operations.

use crate::objects::masters::CarryCapacity;
use std::fmt::{Display, Error, Formatter};
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

/// An amount of a player attribute (a resource) as the game's `f32` holds it: what an object
/// holds of its attribute (`+0x310`), what a unit carries at most (its master's
/// `attribute_max_amount`, [`Amount::of_master`]), what a gather, a hunt or a drop-off moves.
/// Each operator is the one `f32` operation the game does, in its order; a comparison is
/// `f32`'s (a NaN compares false).
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
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

impl Add for Amount {
    type Output = Self;

    /// The `f32` sum.
    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl Sub for Amount {
    type Output = Self;

    /// The `f32` difference.
    fn sub(self, other: Self) -> Self {
        Self(self.0 - other.0)
    }
}

impl AddAssign for Amount {
    /// The `f32` sum in place (a gather, a deposit).
    fn add_assign(&mut self, other: Self) {
        self.0 += other.0;
    }
}

impl SubAssign for Amount {
    /// The `f32` difference in place (a take, a payment).
    fn sub_assign(&mut self, other: Self) {
        self.0 -= other.0;
    }
}

impl Neg for Amount {
    type Output = Self;

    /// The amount negated (its sign bit flipped).
    fn neg(self) -> Self {
        Self(-self.0)
    }
}

impl Mul<f32> for Amount {
    type Output = Self;

    /// The amount scaled by `f` (one `f32` product, the amount first).
    fn mul(self, f: f32) -> Self {
        Self(self.0 * f)
    }
}

impl Mul<Amount> for f32 {
    type Output = Amount;

    /// The amount scaled by `self` (one `f32` product, the factor first).
    fn mul(self, a: Amount) -> Amount {
        Amount(self * a.0)
    }
}

impl Div<f32> for Amount {
    type Output = Self;

    /// The amount divided by `f` (one `f32` quotient).
    fn div(self, f: f32) -> Self {
        Self(self.0 / f)
    }
}

impl Div for Amount {
    type Output = f32;

    /// The ratio of two amounts (one `f32` quotient: a share).
    fn div(self, other: Self) -> f32 {
        self.0 / other.0
    }
}

impl Display for Amount {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        self.0.fmt(f)
    }
}

impl genie_diff::Diff for Amount {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::CarryCapacity;
    use crate::players::Amount;

    /// Each operator is the one `f32` operation, bit for bit and in its operand order.
    #[test]
    fn amounts_do_the_game_f32_operations() {
        let (a, b) = (Amount::new(0.1), Amount::new(0.2));
        assert_eq!((a + b).get().to_bits(), (0.1_f32 + 0.2).to_bits());
        assert_eq!((a - b).get().to_bits(), (0.1_f32 - 0.2).to_bits());
        assert_eq!((-a).get().to_bits(), (-0.1_f32).to_bits());
        assert_eq!((a * 3.0).get().to_bits(), (0.1_f32 * 3.0).to_bits());
        assert_eq!((3.0 * a).get().to_bits(), (3.0_f32 * 0.1).to_bits());
        assert_eq!((a / 3.0).get().to_bits(), (0.1_f32 / 3.0).to_bits());
        assert_eq!((b / a).to_bits(), (0.2_f32 / 0.1).to_bits());
        let mut held = Amount::new(10.0);
        held += Amount::new(0.7);
        assert_eq!(held.get().to_bits(), (10.0_f32 + 0.7).to_bits());
        held -= Amount::new(0.7);
        assert_eq!(held.get().to_bits(), (10.0_f32 + 0.7 - 0.7).to_bits());
        assert_eq!(
            (a - b).abs().get().to_bits(),
            (0.1_f32 - 0.2).abs().to_bits()
        );
        assert_eq!(Amount::new(-0.0).abs().get().to_bits(), 0.0_f32.to_bits());
        assert!(Amount::ZERO < a && a <= a);
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
