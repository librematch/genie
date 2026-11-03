//! A master's `disappears_when_built` (`+0x984`, [`BuiltMode`]).

use serde::Deserialize;
use serde::Serialize;
use std::fmt::{Display, Error, Formatter};

/// A master's `disappears_when_built` (`+0x984`, the `.dat`'s byte): whether a building of it goes once it is built. Every byte is kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct BuiltMode(u8);

impl BuiltMode {
    /// 0: the building stays.
    pub const STAYS: Self = Self(0);
    /// 1: the building goes once it is built.
    pub const DISAPPEARS: Self = Self(1);

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

    /// The building goes once it is built: the byte is not 0.
    #[must_use]
    pub const fn disappears(self) -> bool {
        self.0 != 0
    }
}

impl Display for BuiltMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        self.0.fmt(f)
    }
}

impl genie_diff::Diff for BuiltMode {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::BuiltMode;

    /// The data's bytes are kept: 0 and 1 are the named values, and any byte but 0 sets it.
    #[test]
    fn built_mode_keeps_the_data_byte() {
        assert_eq!(BuiltMode::of_data(0), BuiltMode::STAYS);
        assert_eq!(BuiltMode::of_data(1), BuiltMode::DISAPPEARS);
        assert_eq!(BuiltMode::of_data(2).get(), 2);
        assert_eq!(BuiltMode::default(), BuiltMode::STAYS);
        assert!(!BuiltMode::STAYS.disappears());
        assert!(BuiltMode::of_data(2).disappears());
        assert_eq!(BuiltMode::of_data(2).to_string(), "2");
    }
}
