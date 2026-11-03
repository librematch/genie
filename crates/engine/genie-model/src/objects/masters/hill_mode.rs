//! A master's hill mode (`+0x604`): how much its site's tiles may differ in height.

use serde::Deserialize;
use serde::Serialize;
use std::fmt::{Display, Error, Formatter};

/// A master's hill mode (`+0x604`, the `.dat`'s `hill_mode`): which tiles a site of it may stand on
/// in the placement test (`genie_core::production::spawn`'s hill test, `FUN_141123f80`). Every byte
/// is kept; one not named takes any tile.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct HillMode(u8);

impl HillMode {
    /// 0: any tile (the `.dat`'s gates, farms, walls and towers).
    pub const ANY: Self = Self(0);
    /// 1: flat ground and the four side slopes.
    pub const SIDE_SLOPES: Self = Self(1);
    /// 2: flat ground alone (the `.dat`'s town centres and docks).
    pub const FLAT: Self = Self(2);
    /// 3: the box's first column and last row within one step of the base elevation (the `.dat`'s
    /// other masters).
    pub const ONE_STEP: Self = Self(3);

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

impl Display for HillMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        self.0.fmt(f)
    }
}

impl genie_diff::Diff for HillMode {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::HillMode;

    /// The data's values are the named ones, any other is kept.
    #[test]
    fn hill_mode_is_the_data_byte() {
        assert_eq!(HillMode::of_data(0), HillMode::ANY);
        assert_eq!(HillMode::of_data(9).get(), 9);
        assert_eq!(HillMode::ANY.to_string(), "0");
    }
}
