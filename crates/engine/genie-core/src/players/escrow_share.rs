//! The share of a resource's income a computer player sets aside.

use crate::players::Amount;

/// The share of a resource's income a computer player sets aside (`escrow_percentage`,
/// `+0x6ec`, saved, one a resource): a positive income times it joins the escrow.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct EscrowShare(f32);

impl EscrowShare {
    /// Nothing set aside.
    pub const NONE: Self = Self(0.0);

    /// The share of the save's float `value`.
    #[must_use]
    pub const fn of_data(value: f32) -> Self {
        Self(value)
    }

    /// The part of `income` set aside: the share times it (one `f32` product).
    #[must_use]
    pub fn of(self, income: Amount) -> Amount {
        self.0 * income
    }

    /// The float, as the save holds it.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The part set aside is the share times the income.
    #[test]
    fn the_part_set_aside_is_the_share_times_the_income() {
        let share = EscrowShare::of_data(0.25);
        assert_eq!(share.of(Amount::new(80.0)), Amount::new(20.0));
        assert_eq!(EscrowShare::NONE.of(Amount::new(80.0)), Amount::ZERO);
        assert_eq!(share.get(), 0.25);
    }
}
