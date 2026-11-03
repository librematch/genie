//! A master's `recyclable` (`+0x680`, [`RecycleMode`]).

use serde::Deserialize;
use serde::Serialize;
use std::fmt::{Display, Error, Formatter};

/// A master's `recyclable` (`+0x680`, the `.dat`'s byte): whether its objects are recycled: a recycled object takes its id from the negative range (`FUN_1415301b0`). Every byte is kept.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RecycleMode(u8);

impl RecycleMode {
    /// 0: its objects keep their ids.
    pub const KEPT: Self = Self(0);
    /// 1: its objects are recycled, their ids negative.
    pub const RECYCLED: Self = Self(1);

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

    /// Its objects are recycled: the byte is not 0 (the import's and the spawn's test).
    #[must_use]
    pub const fn recycled(self) -> bool {
        self.0 != 0
    }
}

impl Display for RecycleMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        self.0.fmt(f)
    }
}

impl genie_diff::Diff for RecycleMode {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::RecycleMode;

    /// The data's bytes are kept: 0 and 1 are the named values, and any byte but 0 sets it.
    #[test]
    fn recycle_mode_keeps_the_data_byte() {
        assert_eq!(RecycleMode::of_data(0), RecycleMode::KEPT);
        assert_eq!(RecycleMode::of_data(1), RecycleMode::RECYCLED);
        assert_eq!(RecycleMode::of_data(2).get(), 2);
        assert_eq!(RecycleMode::default(), RecycleMode::KEPT);
        assert!(!RecycleMode::KEPT.recycled());
        assert!(RecycleMode::of_data(2).recycled());
        assert_eq!(RecycleMode::of_data(2).to_string(), "2");
    }
}
