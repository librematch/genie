//! A master's `disabled` (`+0x5f1`, [`DisableMode`]).

use derive_more::Display;
use serde::Deserialize;
use serde::Serialize;

/// A master's `disabled` (`+0x5f1`, the `.dat`'s byte): whether an effect's enable command leaves its availability as it is; the attribute setters change it as a byte, and the trainable count reads its bits (`FUN_141625a20`: bit 0, bits 1 and 2). Every byte is kept.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    PartialOrd,
    Ord,
    Display,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct DisableMode(u8);

impl DisableMode {
    /// 0: an enable command changes the availability.
    pub const NONE: Self = Self(0);
    /// 1: an enable command leaves the availability.
    pub const DISABLED: Self = Self(1);

    /// The value of the data's byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// The byte, as the data and the save hold it.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// An enable command leaves the availability: the byte is not 0.
    #[must_use]
    pub const fn is_disabled(self) -> bool {
        self.0 != 0
    }

    /// The byte plus `n`, wrapping as the attribute setters' add does (`FUN_141126400`).
    #[must_use]
    pub const fn plus(self, n: u8) -> Self {
        Self(self.0.wrapping_add(n))
    }
}

impl genie_diff::Diff for DisableMode {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::DisableMode;

    /// The data's bytes are kept: 0 and 1 are the named values, and any byte but 0 sets it.
    #[test]
    fn disable_mode_keeps_the_data_byte() {
        assert_eq!(DisableMode::of_data(0), DisableMode::NONE);
        assert_eq!(DisableMode::of_data(1), DisableMode::DISABLED);
        assert_eq!(DisableMode::of_data(2).get(), 2);
        assert_eq!(DisableMode::default(), DisableMode::NONE);
        assert!(!DisableMode::NONE.is_disabled());
        assert!(DisableMode::of_data(2).is_disabled());
        assert_eq!(DisableMode::of_data(2).to_string(), "2");
        assert_eq!(DisableMode::DISABLED.plus(1).get(), 2);
    }
}
