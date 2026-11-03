//! The game modes a technology is for.

use serde::Deserialize;
use serde::Serialize;

/// A technology's game modes (`+0x28`, the `.dat`'s `full_tech_mode`, a short): 0 and 1 every
/// one, 2 to 34 a rule, a mode or a starting age, a negative one its opposite
/// ([`crate::world::settings::Settings::tech_mode_allows`]).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TechMode(i16);

impl TechMode {
    /// 1: every mode, and the civilisation's own technology stays under the full tech tree.
    pub const KEPT_IN_FULL_TREE: Self = Self(1);

    /// The mode of the `.dat`'s short `value`.
    #[must_use]
    pub const fn of_data(value: i16) -> Self {
        Self(value)
    }

    /// The short.
    #[must_use]
    pub const fn get(self) -> i16 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use crate::technology::TechMode;

    /// The short reads back; mode 1 is the full tech tree's exception.
    #[test]
    fn the_short_reads_back() {
        assert_eq!(TechMode::of_data(-27).get(), -27);
        assert_eq!(TechMode::KEPT_IN_FULL_TREE, TechMode::of_data(1));
    }
}
