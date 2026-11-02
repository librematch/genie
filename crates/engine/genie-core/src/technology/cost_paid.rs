//! How a technology's cost entry is paid (its paid flag).

/// How a technology's cost entry is paid (the `.dat`'s paid flag): 0 not at all, 1 from the
/// player's attribute, 2 from the building (`FUN_141204310`). Every byte is kept; any but 0 is
/// paid.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct CostPaid(u8);

impl CostPaid {
    /// 0: not paid.
    pub const UNPAID: Self = Self(0);
    /// 1: from the player's attribute.
    pub const PLAYER: Self = Self(1);
    /// 2: from the building.
    pub const BUILDING: Self = Self(2);

    /// The value of the data's byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// Whether the entry is paid (any but 0).
    #[must_use]
    pub const fn is_paid(self) -> bool {
        self.0 != Self::UNPAID.0
    }

    /// The byte, as the data and the save hold it.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl std::fmt::Display for CostPaid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl genie_diff::Diff for CostPaid {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::technology::CostPaid;

    /// The data's values are the named ones, any other is kept.
    #[test]
    fn paid_flag_is_the_data_byte() {
        assert_eq!(CostPaid::of_data(0), CostPaid::UNPAID);
        assert_eq!(CostPaid::of_data(5).get(), 5);
        assert_eq!(CostPaid::UNPAID.to_string(), "0");
        assert!(!CostPaid::UNPAID.is_paid() && CostPaid::of_data(5).is_paid());
    }
}
