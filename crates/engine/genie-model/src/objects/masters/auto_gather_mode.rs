//! A master's `enable_auto_gather` (`+0x681`, [`AutoGatherMode`]).

use derive_more::Display;
use serde::Deserialize;
use serde::Serialize;

/// A master's `enable_auto_gather` (`+0x681`, the `.dat`'s byte): whether gatherers take it up by themselves; an object of it blocking a spawn point stays (`FUN_141194170`). Every byte is kept.
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
pub struct AutoGatherMode(u8);

impl AutoGatherMode {
    /// 0: no automatic gathering.
    pub const OFF: Self = Self(0);
    /// 1: gatherers take it up by themselves.
    pub const ON: Self = Self(1);

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

    /// Automatic gathering is on: the byte is not 0.
    #[must_use]
    pub const fn is_on(self) -> bool {
        self.0 != 0
    }
}

impl genie_diff::Diff for AutoGatherMode {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::AutoGatherMode;

    /// The data's bytes are kept: 0 and 1 are the named values, and any byte but 0 sets it.
    #[test]
    fn auto_gather_mode_keeps_the_data_byte() {
        assert_eq!(AutoGatherMode::of_data(0), AutoGatherMode::OFF);
        assert_eq!(AutoGatherMode::of_data(1), AutoGatherMode::ON);
        assert_eq!(AutoGatherMode::of_data(2).get(), 2);
        assert_eq!(AutoGatherMode::default(), AutoGatherMode::OFF);
        assert!(!AutoGatherMode::OFF.is_on());
        assert!(AutoGatherMode::of_data(2).is_on());
        assert_eq!(AutoGatherMode::of_data(2).to_string(), "2");
    }
}
