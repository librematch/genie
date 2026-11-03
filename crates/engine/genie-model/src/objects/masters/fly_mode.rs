//! A master's fly mode (`+0x608`): where its objects' height goes on the ground.

use serde::Deserialize;
use serde::Serialize;
use std::fmt::{Display, Error, Formatter};

/// A master's fly mode (`+0x608`, the `.dat`'s `fly_mode`): where an object's height goes against
/// the ground's (`FUN_141193a10`, `genie_core::combat::missile::grounded`). Every byte is kept; one
/// not named keeps its height.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct FlyMode(u8);

impl FlyMode {
    /// 0: on the ground.
    pub const GROUND: Self = Self(0);
    /// 1: lifted to the ground when below it.
    pub const ABOVE: Self = Self(1);

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
}

impl Display for FlyMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        self.0.fmt(f)
    }
}

impl genie_diff::Diff for FlyMode {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::FlyMode;

    /// The data's values are the named ones, any other is kept.
    #[test]
    fn fly_mode_is_the_data_byte() {
        assert_eq!(FlyMode::of_data(0), FlyMode::GROUND);
        assert_eq!(FlyMode::of_data(2).get(), 2);
        assert_eq!(FlyMode::GROUND.to_string(), "0");
    }
}
