//! A master's `create_doppelganger_on_death` (`+0x682`): whether its objects leave ghosts.

use derive_more::Display;
use serde::Deserialize;
use serde::Serialize;

/// A master's `create_doppelganger_on_death` (`+0x682`): whether its dying objects leave ghosts in
/// the other players' fog. Every byte is kept; only 2 leaves them.
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
pub struct GhostMode(u8);

impl GhostMode {
    /// 0: no ghost.
    pub const NONE: Self = Self(0);
    /// 2: its objects leave ghosts (`genie_core::vision::ghost`).
    pub const LEAVES: Self = Self(2);

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

impl genie_diff::Diff for GhostMode {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::GhostMode;

    /// The data's values are the named ones, any other is kept.
    #[test]
    fn ghost_mode_is_the_data_byte() {
        assert_eq!(GhostMode::of_data(0), GhostMode::NONE);
        assert_eq!(GhostMode::of_data(1).get(), 1);
        assert_eq!(GhostMode::NONE.to_string(), "0");
    }
}
