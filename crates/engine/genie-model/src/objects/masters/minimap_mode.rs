//! A master's minimap mode (`+0x62e`): how its objects show on the minimap.

use derive_more::Display;
use serde::Deserialize;
use serde::Serialize;

/// A master's minimap mode (`map_draw_level`, `+0x62e`; the `.dat`'s `minimap_mode`, openage's
/// names): the dot its objects draw on the minimap. The simulation only keeps it; every byte is
/// kept.
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
pub struct MinimapMode(u8);

impl MinimapMode {
    /// 0: no dot.
    pub const NONE: Self = Self(0);
    /// 1: a square dot.
    pub const SQUARE: Self = Self(1);
    /// 2: a diamond dot.
    pub const DIAMOND: Self = Self(2);
    /// 3: a diamond dot that keeps its colour when selected.
    pub const DIAMOND_KEEP_COLOUR: Self = Self(3);
    /// 4: a large dot every player sees.
    pub const LARGE: Self = Self(4);

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

impl genie_diff::Diff for MinimapMode {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::MinimapMode;

    /// The data's values are the named ones, any other is kept.
    #[test]
    fn minimap_mode_is_the_data_byte() {
        assert_eq!(MinimapMode::of_data(255).plus(2).get(), 1);
        assert_eq!(MinimapMode::of_data(0), MinimapMode::NONE);
        assert_eq!(MinimapMode::of_data(10).get(), 10);
        assert_eq!(MinimapMode::NONE.to_string(), "0");
    }
}
