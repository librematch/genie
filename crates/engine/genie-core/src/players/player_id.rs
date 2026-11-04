//! A player's number as one type: the player a command comes from or names, and the index of
//! the world's players.

use crate::{GenieError, GenieResult};
use derive_more::Display;
use getset::CopyGetters;

/// A player's number: 0 for Gaia, then the players in their order. A command's byte 1 holds
/// the player who sent it, a comm player the game looks up whatever its value
/// (`FUN_140e6b210`; the slice takes comm and world players as equal), so every byte is one.
/// The world's players are in this order ([`PlayerId::index`]).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, CopyGetters, Display,
)]
pub struct PlayerId(
    /// Its number, as a command's byte holds it.
    #[getset(get_copy = "pub")]
    u8,
);

impl PlayerId {
    /// Gaia, the neutral player 0 (also the default: a record not yet given an owner).
    pub const GAIA: Self = Self(0);

    /// Its place in the world's players.
    #[must_use]
    pub fn index(self) -> usize {
        usize::from(self.0)
    }
}

impl PlayerId {
    /// The first `count` players in their order, Gaia first: the world's players. A table past a
    /// command's byte (256) keeps only the players a command can name.
    pub fn each(count: usize) -> impl Iterator<Item = Self> + Clone {
        (0..count).filter_map(|index| Self::try_from(index).ok())
    }
}

impl genie_diff::Diff for PlayerId {
    /// By equality: a player is one value.
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        differ.equal(self, other);
    }
}

impl PlayerId {
    /// The value a field takes for none.
    const NONE: i32 = -1;

    /// The player of a saved field that holds −1 for none (a queued order's target owner).
    ///
    /// # Errors
    ///
    /// A negative value other than −1 or one past a command's byte.
    pub fn of_field(value: i32) -> GenieResult<Option<Self>> {
        if value == Self::NONE {
            return Ok(None);
        }
        u8::try_from(value).map(|n| Some(Self(n))).map_err(|_| {
            GenieError::invalid_input("a player field no player has").with("player", value)
        })
    }

    /// The value of a field that holds −1 for none.
    #[must_use]
    pub fn field(player: Option<Self>) -> i32 {
        player.map_or(Self::NONE, |player_id| i32::from(player_id.0))
    }

    /// The player of number `number`, as a command's byte holds it (for a constant).
    #[must_use]
    pub const fn new(number: u8) -> Self {
        Self(number)
    }
}

impl From<u8> for PlayerId {
    /// The player of number `number`, as a command's byte holds it.
    fn from(number: u8) -> Self {
        Self::new(number)
    }
}

impl TryFrom<usize> for PlayerId {
    type Error = GenieError;

    /// The player at `index` of the world's players.
    ///
    /// # Errors
    ///
    /// An index past a byte, which no command names.
    fn try_from(index: usize) -> GenieResult<Self> {
        u8::try_from(index).map(Self::from).map_err(|_| {
            GenieError::invalid_input("a player past a command's byte").with("player", index)
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::ErrorKind;
    use crate::players::PlayerId;

    /// A player's index is its number, and the index converts back.
    #[test]
    fn player_index_is_its_number() {
        for number in [0, 8, u8::MAX] {
            let player_id = PlayerId::from(number);
            assert_eq!(
                (player_id.get(), player_id.index()),
                (number, usize::from(number))
            );
            assert_eq!(PlayerId::try_from(player_id.index()), Ok(player_id));
        }
    }

    /// A saved −1 is no player, a number up to a byte a player, any other value an error; each
    /// converts back to its value.
    #[test]
    fn player_field_minus_one_is_none() {
        assert_eq!(PlayerId::of_field(-1), Ok(None));
        assert_eq!(PlayerId::of_field(3), Ok(Some(PlayerId::new(3))));
        assert_eq!(PlayerId::field(Some(PlayerId::new(3))), 3);
        assert_eq!(PlayerId::field(None), -1);
        for bad in [-2, 256] {
            assert_eq!(
                PlayerId::of_field(bad).map_err(|error| error.kind()),
                Err(ErrorKind::InvalidInput)
            );
        }
    }

    /// The players of a table of `count` are its indexes in order; a table past a byte keeps
    /// the first 256.
    #[test]
    fn each_gives_the_players_in_order() {
        let players: Vec<usize> = PlayerId::each(3).map(PlayerId::index).collect();
        assert_eq!(players, [0, 1, 2]);
        assert_eq!(PlayerId::each(300).count(), 256);
        assert_eq!(PlayerId::each(0).count(), 0);
    }

    /// A player prints as its number and differs by equality.
    #[test]
    fn player_prints_as_its_number_and_differs_by_equality() {
        use genie_diff::{Scope, differences};
        let (one, two) = (PlayerId::new(1), PlayerId::new(2));
        assert_eq!(PlayerId::new(7).to_string(), "7");
        assert_eq!(differences(&one, &one, Scope::All).len(), 0);
        assert_eq!(differences(&one, &two, Scope::All).len(), 1);
    }

    /// An index past a byte is no player.
    #[test]
    fn an_index_past_byte_is_no_player() {
        assert_eq!(
            PlayerId::try_from(256_usize).map_err(|error| error.kind()),
            Err(ErrorKind::InvalidInput)
        );
    }
}
