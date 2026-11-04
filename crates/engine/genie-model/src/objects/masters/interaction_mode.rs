//! A master's interaction mode (`+0x62d`): how a player may select and order its objects.

use derive_more::Display;
use serde::Deserialize;
use serde::Serialize;

/// A master's interaction mode (`select_level`, `+0x62d`; the `.dat`'s `interaction_mode`,
/// openage's names): whether a player may select its objects and give them orders. The simulation
/// only keeps it; every byte is kept.
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
pub struct InteractionMode(u8);

impl InteractionMode {
    /// 0: no interaction.
    pub const NONE: Self = Self(0);
    /// 1: a bird's.
    pub const BIRD: Self = Self(1);
    /// 2: selectable.
    pub const SELECTABLE: Self = Self(2);
    /// 3: selectable, takes attacks.
    pub const SELECT_ATTACK: Self = Self(3);
    /// 4: selectable, takes attacks and moves.
    pub const SELECT_ATTACK_MOVE: Self = Self(4);
    /// 5: selectable, takes moves.
    pub const SELECT_MOVE: Self = Self(5);

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

impl genie_diff::Diff for InteractionMode {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::InteractionMode;

    /// The data's values are the named ones, any other is kept.
    #[test]
    fn an_interaction_mode_is_the_data_byte() {
        assert_eq!(InteractionMode::of_data(255).plus(2).get(), 1);
        assert_eq!(InteractionMode::of_data(0), InteractionMode::NONE);
        assert_eq!(InteractionMode::of_data(9).get(), 9);
        assert_eq!(InteractionMode::NONE.to_string(), "0");
    }
}
