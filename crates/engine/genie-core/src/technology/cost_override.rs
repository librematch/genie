//! A research's cost overrides.

use crate::objects::masters::CostAmount;
use crate::players::attribute::AttributeId;
use genie_diff::Diff;

/// How a research uses its cost override (`+0x4e`, a short): −2 the `.dat`'s entry where it is
/// paid, 0 none, 1 the override (any other value counts as the override where the listed
/// costs read it, and as none where the payment does).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OverrideUse(i16);

impl OverrideUse {
    /// The `.dat`'s entry where it is paid (−2).
    pub const DAT: Self = Self(-2);
    /// No cost (0).
    pub const NONE: Self = Self(0);
    /// The override (1).
    pub const OVERRIDE: Self = Self(1);

    /// The use of the save's short `value`.
    #[must_use]
    pub const fn of_data(value: i16) -> Self {
        Self(value)
    }

    /// The short.
    #[must_use]
    pub const fn get(self) -> i16 {
        self.0
    }
}

impl Diff for OverrideUse {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

/// One of a research's three cost overrides (`+0x42`, `+0x48`, `+0x4e`): the amount, the
/// attribute and its use.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Diff)]
pub struct CostOverride {
    /// The amount.
    pub amount: CostAmount,
    /// The attribute, none for −1.
    pub attribute: Option<AttributeId>,
    /// Its use.
    pub used: OverrideUse,
}

impl CostOverride {
    /// A research's state without overrides: −1 of no attribute, the `.dat`'s entry used.
    pub const UNSET: Self = Self {
        amount: CostAmount::of_data(-1),
        attribute: None,
        used: OverrideUse::DAT,
    };
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::CostAmount;
    use crate::technology::{CostOverride, OverrideUse};

    /// The unset override is −1 of no attribute with the `.dat`'s entry used; the default none.
    #[test]
    fn the_unset_override_uses_the_dat_entry() {
        assert_eq!(CostOverride::UNSET.amount, CostAmount::of_data(-1));
        assert_eq!(CostOverride::UNSET.used, OverrideUse::DAT);
        assert_eq!(CostOverride::default().used, OverrideUse::NONE);
        assert_eq!(OverrideUse::OVERRIDE.get(), 1);
    }
}
