//! A master's run pattern (`+0x7b9`).

use serde::Deserialize;
use serde::Serialize;
use std::fmt::{Display, Error, Formatter};

/// A master's run pattern (`run_pattern`, `+0x7b9`): the effects set and add to it, and the
/// simulation only keeps it; every byte is kept.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RunPattern(u8);

impl RunPattern {
    /// 0: none.
    pub const NONE: Self = Self(0);

    /// The value of the data's byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// The byte plus `amount` (an effect's add, wrapping as the game's add).
    #[must_use]
    pub const fn plus(self, amount: u8) -> Self {
        Self(self.0.wrapping_add(amount))
    }

    /// The byte, as the data and the save hold it.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl Display for RunPattern {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        self.0.fmt(f)
    }
}

impl genie_diff::Diff for RunPattern {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::RunPattern;

    /// The data's values are the named ones, any other is kept.
    #[test]
    fn run_pattern_is_the_data_byte() {
        assert_eq!(RunPattern::of_data(255).plus(2).get(), 1);
        assert_eq!(RunPattern::of_data(0), RunPattern::NONE);
        assert_eq!(RunPattern::of_data(3).get(), 3);
        assert_eq!(RunPattern::NONE.to_string(), "0");
    }
}
