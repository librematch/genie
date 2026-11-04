//! A player's type as one type: `player+0x10`, the save's class byte.

use crate::{GenieError, GenieResult};
use std::fmt::{Display, Error, Formatter};
use strum::FromRepr;

/// A player's type (`player+0x10`, the save's `type`): which `WorldPlayer` class the game made
/// for it. The player factory (`WorldPlayerFactory.cpp`, the creators `FUN_141679be0`
/// registers) makes these four; a saved byte of another type is the game's
/// `InvalidPlayerType`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, FromRepr)]
#[repr(u8)]
pub enum PlayerKind {
    /// Type 0: the scenario editor's phantom player.
    #[default]
    Phantom = 0,
    /// Type 1: a human player.
    Human = 1,
    /// Type 2: Gaia (`WorldPlayerGaia`).
    Gaia = 2,
    /// Type 3: a computer player, whose income fills its escrow.
    Computer = 3,
}

impl PlayerKind {
    /// Its value.
    #[must_use]
    pub const fn get(self) -> u8 {
        self as u8
    }

    /// `FUN_141659cf0`: a computer player's (`player+0x10 == 3`).
    #[must_use]
    pub const fn is_computer(self) -> bool {
        matches!(self, Self::Computer)
    }
}

impl TryFrom<u8> for PlayerKind {
    type Error = GenieError;

    /// The type a saved player's byte holds.
    ///
    /// # Errors
    ///
    /// A byte of a type no creator handles (the game's `InvalidPlayerType`).
    fn try_from(value: u8) -> GenieResult<Self> {
        Self::from_repr(value)
            .ok_or_else(|| GenieError::invalid_input("InvalidPlayerType").with("type", value))
    }
}

impl Display for PlayerKind {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> Result<(), Error> {
        self.get().fmt(formatter)
    }
}

impl genie_diff::Diff for PlayerKind {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        differ.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::players::PlayerKind;

    /// A saved byte is its type; the default is the phantom's 0.
    #[test]
    fn saved_byte_is_its_kind() {
        assert_eq!(PlayerKind::try_from(3).unwrap(), PlayerKind::Computer);
        assert_eq!(PlayerKind::try_from(0).unwrap(), PlayerKind::Phantom);
        assert_eq!(PlayerKind::Gaia.get(), 2);
        assert_eq!(PlayerKind::default(), PlayerKind::Phantom);
        assert_eq!(PlayerKind::Human.to_string(), "1");
    }

    /// A byte no creator handles is the game's `InvalidPlayerType`.
    #[test]
    fn unhandled_byte_is_refused() {
        for byte in [4, 0xff] {
            let error = PlayerKind::try_from(byte).unwrap_err();
            assert!(error.to_string().contains("InvalidPlayerType"), "{error}");
        }
    }

    /// Only the computer's type is a computer player's.
    #[test]
    fn only_computer_is_computer() {
        let kinds = [
            PlayerKind::Phantom,
            PlayerKind::Human,
            PlayerKind::Gaia,
            PlayerKind::Computer,
        ];
        let computers: Vec<_> = kinds
            .into_iter()
            .filter(|player_kind| player_kind.is_computer())
            .collect();
        assert_eq!(computers, [PlayerKind::Computer]);
    }
}
