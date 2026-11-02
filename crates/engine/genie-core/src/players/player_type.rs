//! A player's type as one type: `player+0x10`, the save's class byte.

use std::fmt::{Display, Error, Formatter};

/// A player's type (`player+0x10`, the save's `type`): which `WorldPlayer` class the game made
/// for it. Every byte is a type (a save may hold types the slice does not name).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PlayerType(u8);

impl PlayerType {
    /// Type 0: the scenario editor's phantom player.
    pub const PHANTOM: Self = Self(0);
    /// Type 1: a human player.
    pub const HUMAN: Self = Self(1);
    /// Type 2: Gaia (`WorldPlayerGaia`).
    pub const GAIA: Self = Self(2);
    /// Type 3: a computer player, whose income fills its escrow.
    pub const COMPUTER: Self = Self(3);

    /// Its value.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// `FUN_141659cf0`: a computer player's (`player+0x10 == 3`).
    #[must_use]
    pub const fn is_computer(self) -> bool {
        self.0 == Self::COMPUTER.0
    }
}

impl From<u8> for PlayerType {
    /// The type a saved player's byte holds: every value is one.
    fn from(value: u8) -> Self {
        Self(value)
    }
}

impl Display for PlayerType {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        self.0.fmt(f)
    }
}

impl genie_diff::Diff for PlayerType {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::players::PlayerType;

    /// A saved byte is its type; the default is the phantom's 0.
    #[test]
    fn saved_byte_is_its_type() {
        assert_eq!(PlayerType::from(3), PlayerType::COMPUTER);
        assert_eq!(PlayerType::GAIA.get(), 2);
        assert_eq!(PlayerType::default(), PlayerType::PHANTOM);
    }
}
