//! One value per player, looked up by the player ([`ByPlayer`]).

use crate::GenieError;
use crate::players::PlayerId;

/// A player the row does not hold, where the caller's own loop over the players named it.
const NO_PLAYER: GenieError = GenieError::unreachable("a player the row does not hold");

/// One value per player, in the players' order (Gaia first): a row the game indexes by a
/// player's number, such as a player's relations (`player+0x260`) or the spatial copies of
/// every player's rows. A player past the row has no value.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ByPlayer<T>(Vec<T>);

impl<T> ByPlayer<T> {
    /// The row of `values`, for players 0, 1, … in their order.
    #[must_use]
    pub const fn new(values: Vec<T>) -> Self {
        Self(values)
    }

    /// Player `p`'s value, none past the row.
    #[must_use]
    pub fn get(&self, p: PlayerId) -> Option<&T> {
        self.0.get(p.index())
    }

    /// Player `p`'s value to change, none past the row.
    pub fn get_mut(&mut self, p: PlayerId) -> Option<&mut T> {
        self.0.get_mut(p.index())
    }

    /// Player `p`'s value, as an error where the caller's own loop names a player past the row.
    ///
    /// # Errors
    ///
    /// A player the row does not hold.
    pub fn checked(&self, p: PlayerId) -> Result<&T, GenieError> {
        self.get(p).ok_or_else(|| NO_PLAYER.with("player", p))
    }

    /// Player `p`'s value to change, as an error past the row.
    ///
    /// # Errors
    ///
    /// A player the row does not hold.
    pub fn checked_mut(&mut self, p: PlayerId) -> Result<&mut T, GenieError> {
        self.get_mut(p).ok_or_else(|| NO_PLAYER.with("player", p))
    }

    /// Player `p`'s value becomes `value`; a row shorter than `p` grows first, with the
    /// default value for the players between.
    pub fn set(&mut self, p: PlayerId, value: T)
    where
        T: Clone + Default,
    {
        if let Some(slot) = self.0.get_mut(p.index()) {
            *slot = value;
        } else {
            self.0.resize(p.index(), T::default());
            self.0.push(value);
        }
    }

    /// How many players the row holds.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.0.len()
    }

    /// The row holds no player.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Each player with its value, in the players' order.
    pub fn iter(&self) -> impl Iterator<Item = (PlayerId, &T)> + Clone {
        PlayerId::each(self.0.len()).zip(&self.0)
    }

    /// The values in the players' order.
    #[must_use]
    pub fn values(&self) -> &[T] {
        &self.0
    }
}

impl<T> FromIterator<T> for ByPlayer<T> {
    /// The row of the values in the players' order.
    fn from_iter<I: IntoIterator<Item = T>>(values: I) -> Self {
        Self(values.into_iter().collect())
    }
}

impl<T: genie_diff::Diff> genie_diff::Diff for ByPlayer<T> {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::players::{ByPlayer, PlayerId};

    /// A player's value is at its number; a player past the row has none.
    #[test]
    fn player_value_is_at_its_number() {
        let mut row: ByPlayer<u8> = [7, 8, 9].into_iter().collect();
        assert_eq!(row.get(PlayerId::new(1)), Some(&8));
        assert_eq!(row.get(PlayerId::new(3)), None);
        *row.get_mut(PlayerId::new(2)).unwrap() = 4;
        assert!(row.get_mut(PlayerId::new(3)).is_none());
        assert_eq!(row.checked(PlayerId::new(0)), Ok(&7));
        assert!(row.checked(PlayerId::new(3)).is_err());
        assert!(row.checked_mut(PlayerId::new(3)).is_err());
        assert_eq!(row.values(), &[7, 8, 4]);
        let mut grown = row.clone();
        grown.set(PlayerId::new(5), 1);
        grown.set(PlayerId::new(0), 2);
        assert_eq!(grown.values(), &[2, 8, 4, 0, 0, 1]);
        assert_eq!((row.len(), row.is_empty()), (3, false));
        let each: Vec<(u8, u8)> = row.iter().map(|(p, &v)| (p.get(), v)).collect();
        assert_eq!(each, [(0, 7), (1, 8), (2, 4)]);
        assert_eq!(row, ByPlayer::new(vec![7, 8, 4]));
        assert!(ByPlayer::<u8>::default().is_empty());
    }
}
