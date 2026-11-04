//! A master's formation category (`+0x8bc`): the sub-formation its units take in a group.

use serde::Deserialize;
use serde::Serialize;

/// A master's formation category (`mFormationCategory`, `+0x8bc`): which sub-formation of a group
/// its units take (`genie_core::groups::sub_formation`: 1 to 4 their own, 0 and 5 the protected one,
/// 6 the first). 0 keeps a unit out of the groups an order's splitter forms (`FUN_140fd76d0`).
/// Every byte is kept.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct FormationClass(u8);

impl FormationClass {
    /// 0: not grouped by an order's splitter (the `.dat`'s villagers); protected in a formation.
    pub const UNGROUPED: Self = Self(0);
    /// 5: the protected sub-formation.
    pub const PROTECTED: Self = Self(5);
    /// 6: the first sub-formation (category 0).
    pub const FIRST: Self = Self(6);

    /// The category of the data's byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// Whether an order's splitter groups its units (any but 0).
    #[must_use]
    pub const fn grouped(self) -> bool {
        self.0 != Self::UNGROUPED.0
    }

    /// The byte, as the data and the save hold it.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl genie_diff::Diff for FormationClass {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::FormationClass;

    /// Only 0 is ungrouped; every byte is kept.
    #[test]
    fn only_0_is_ungrouped() {
        assert!(!FormationClass::UNGROUPED.grouped());
        assert!(FormationClass::of_data(3).grouped() && FormationClass::FIRST.grouped());
        assert_eq!(FormationClass::of_data(5), FormationClass::PROTECTED);
        assert_eq!(FormationClass::of_data(200).get(), 200);
    }
}
