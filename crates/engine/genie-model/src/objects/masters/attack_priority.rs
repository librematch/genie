//! A master's attack priority: which targets its unit AI's checks let it choose.

use derive_more::Display;
use serde::Deserialize;
use serde::Serialize;

/// A master's attack priority (`+0x92c`, `FUN_141202220`; the `.dat`'s combat part): a number
/// the unit AIs' attack checks read by value. Priority 3 limits most of them to the classes of
/// `PRIORITY_3_CLASSES`; 1 and 2 let the lion and the predators choose anything; 0 holds the
/// rams and the trebuchets to their own targets and 1 ranks the best target.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    Display,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct AttackPriority(u8);

impl AttackPriority {
    /// 0.
    pub const PRIORITY_0: Self = Self(0);
    /// 1.
    pub const PRIORITY_1: Self = Self(1);
    /// 2.
    pub const PRIORITY_2: Self = Self(2);
    /// 3: most attack checks choose only the classes of `PRIORITY_3_CLASSES`.
    pub const PRIORITY_3: Self = Self(3);

    /// The priority of byte `value`.
    #[must_use]
    pub const fn of_byte(value: u8) -> Self {
        Self(value)
    }

    /// Its byte.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// Priority 3: the checks that read it choose only the classes of `PRIORITY_3_CLASSES`.
    #[must_use]
    pub const fn limited(self) -> bool {
        self.0 == Self::PRIORITY_3.0
    }

    /// Priority 4 or more (`priority & 0xfc`, the siege's building test).
    #[must_use]
    pub const fn past_3(self) -> bool {
        self.0 & 0xfc != 0
    }
}

impl genie_diff::Diff for AttackPriority {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        differ.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::AttackPriority;

    /// Priority 3 is the limited one; 4 and up are past 3.
    #[test]
    fn priority_3_limits_and_4_is_past_it() {
        assert!(AttackPriority::PRIORITY_3.limited() && !AttackPriority::PRIORITY_2.limited());
        assert_eq!(
            [0, 3, 4, 0xff].map(|byte| AttackPriority::of_byte(byte).past_3()),
            [false, false, true, true]
        );
        assert_eq!(AttackPriority::PRIORITY_1.get(), 1);
    }
}
