//! The checked reach into the world's player table and the players' object lists.
//!
//! An index into either comes from the world's own places (a loop over the players, a
//! [`crate::world::index::Place`], a `position_of`), so a miss is an invariant broken, not an input
//! the recording got wrong: each lookup returns an `unreachable` error that names the index.
//! The trait is implemented for the slice, not for [`crate::world::World`], so a caller keeps
//! the borrow of the one field (`self.players`) and still reads the others.

use crate::objects::ListIndex;
use crate::objects::unit::Unit;
use crate::players::PlayerId;
use crate::world::index::Place;
use crate::world::player::Player;
use crate::{GenieError, GenieResult};

/// A player the world does not hold.
const NO_PLAYER: GenieError = GenieError::unreachable("a player the world does not hold");

/// A slot of a player's live or sleeping list that the list does not hold.
const NO_SLOT: GenieError =
    GenieError::unreachable("an object slot the player's list does not hold");

/// A position the slice the caller holds does not hold.
const NO_ELEMENT: GenieError = GenieError::unreachable("a position the list does not hold");

/// A player, and the objects at a position of its lists, as errors where a slice's index
/// would panic.
pub(crate) trait PlayerSlotsExt {
    /// Player `player_id`.
    ///
    /// # Errors
    ///
    /// A player the table does not hold.
    fn player(&self, player_id: PlayerId) -> GenieResult<&Player>;

    /// Player `player_id`, to change.
    ///
    /// # Errors
    ///
    /// A player the table does not hold.
    fn player_mut(&mut self, player_id: PlayerId) -> GenieResult<&mut Player>;

    /// The object at position `list_index` of player `player_id`'s live list.
    ///
    /// # Errors
    ///
    /// A player or a position the lists do not hold.
    fn live(&self, player_id: PlayerId, list_index: ListIndex) -> GenieResult<&Unit> {
        self.player(player_id)?
            .live(list_index)
            .map_err(|error| error.with("player", player_id))
    }

    /// The object at position `list_index` of player `player_id`'s live list, to change.
    ///
    /// # Errors
    ///
    /// A player or a position the lists do not hold.
    fn live_mut(&mut self, player_id: PlayerId, list_index: ListIndex) -> GenieResult<&mut Unit> {
        self.player_mut(player_id)?
            .live_mut(list_index)
            .map_err(|error| error.with("player", player_id))
    }

    /// The object at position `list_index` of player `player_id`'s sleeping list.
    ///
    /// # Errors
    ///
    /// A player or a position the lists do not hold.
    fn asleep(&self, player_id: PlayerId, list_index: ListIndex) -> GenieResult<&Unit> {
        self.player(player_id)?
            .asleep(list_index)
            .map_err(|error| error.with("player", player_id))
    }

    /// The object at position `list_index` of player `player_id`'s sleeping list, to change.
    ///
    /// # Errors
    ///
    /// A player or a position the lists do not hold.
    fn asleep_mut(&mut self, player_id: PlayerId, list_index: ListIndex) -> GenieResult<&mut Unit> {
        self.player_mut(player_id)?
            .asleep_mut(list_index)
            .map_err(|error| error.with("player", player_id))
    }

    /// The object at `place` (the world's table by id gave it).
    ///
    /// # Errors
    ///
    /// A player or a position the lists do not hold.
    fn at(&self, place: Place) -> GenieResult<&Unit> {
        match place {
            Place::Live(owner, slot) => self.live(owner, slot),
            Place::Sleeping(player_id, list_index) => self.asleep(player_id, list_index),
        }
    }

    /// The object at `place`, to change.
    ///
    /// # Errors
    ///
    /// A player or a position the lists do not hold.
    fn at_mut(&mut self, place: Place) -> GenieResult<&mut Unit> {
        match place {
            Place::Live(owner, slot) => self.live_mut(owner, slot),
            Place::Sleeping(player_id, list_index) => self.asleep_mut(player_id, list_index),
        }
    }
}

/// A position of a slice the caller holds, as an error where `slice[index]` would panic: the index
/// comes from the code's own loop or search, so a miss is an invariant broken.
pub(crate) trait CheckedIndexExt<T> {
    /// The element at `index`.
    ///
    /// # Errors
    ///
    /// A position the slice does not hold.
    fn checked(&self, index: usize) -> GenieResult<&T>;

    /// The element at `index`, to change.
    ///
    /// # Errors
    ///
    /// A position the slice does not hold.
    fn checked_mut(&mut self, index: usize) -> GenieResult<&mut T>;

    /// The first `end` elements.
    ///
    /// # Errors
    ///
    /// A slice shorter than `end`.
    fn head(&self, end: usize) -> GenieResult<&[T]>;

    /// The first `end` elements, to change.
    ///
    /// # Errors
    ///
    /// A slice shorter than `end`.
    fn head_mut(&mut self, end: usize) -> GenieResult<&mut [T]>;

    /// The elements from `from` on, to change: none at the end.
    ///
    /// # Errors
    ///
    /// A slice shorter than `from`.
    fn tail_mut(&mut self, from: usize) -> GenieResult<&mut [T]>;
}

impl<T> CheckedIndexExt<T> for [T] {
    fn checked(&self, index: usize) -> GenieResult<&T> {
        self.get(index)
            .ok_or_else(|| NO_ELEMENT.with("index", index))
    }

    fn checked_mut(&mut self, index: usize) -> GenieResult<&mut T> {
        self.get_mut(index)
            .ok_or_else(|| NO_ELEMENT.with("index", index))
    }

    fn head(&self, end: usize) -> GenieResult<&[T]> {
        self.get(..end).ok_or_else(|| NO_ELEMENT.with("end", end))
    }

    fn head_mut(&mut self, end: usize) -> GenieResult<&mut [T]> {
        self.get_mut(..end)
            .ok_or_else(|| NO_ELEMENT.with("end", end))
    }

    fn tail_mut(&mut self, from: usize) -> GenieResult<&mut [T]> {
        self.get_mut(from..)
            .ok_or_else(|| NO_ELEMENT.with("start", from))
    }
}

impl PlayerSlotsExt for [Player] {
    fn player(&self, player_id: PlayerId) -> GenieResult<&Player> {
        self.get(player_id.index())
            .ok_or_else(|| NO_PLAYER.with("player", player_id))
    }

    fn player_mut(&mut self, player_id: PlayerId) -> GenieResult<&mut Player> {
        self.get_mut(player_id.index())
            .ok_or_else(|| NO_PLAYER.with("player", player_id))
    }
}

impl Player {
    /// The object at position `list_index` of the live list.
    ///
    /// # Errors
    ///
    /// A position the list does not hold.
    pub(crate) fn live(&self, list_index: ListIndex) -> GenieResult<&Unit> {
        self.objects
            .get(list_index)
            .ok_or_else(|| NO_SLOT.with("index", list_index))
    }

    /// The object at position `list_index` of the live list, to change.
    ///
    /// # Errors
    ///
    /// A position the list does not hold.
    pub(crate) fn live_mut(&mut self, list_index: ListIndex) -> GenieResult<&mut Unit> {
        self.objects
            .get_mut(list_index)
            .ok_or_else(|| NO_SLOT.with("index", list_index))
    }

    /// The object at position `list_index` of the sleeping list.
    ///
    /// # Errors
    ///
    /// A position the list does not hold.
    pub(crate) fn asleep(&self, list_index: ListIndex) -> GenieResult<&Unit> {
        self.sleeping
            .get(list_index)
            .ok_or_else(|| NO_SLOT.with("index", list_index))
    }

    /// The object at position `list_index` of the sleeping list, to change.
    ///
    /// # Errors
    ///
    /// A position the list does not hold.
    pub(crate) fn asleep_mut(&mut self, list_index: ListIndex) -> GenieResult<&mut Unit> {
        self.sleeping
            .get_mut(list_index)
            .ok_or_else(|| NO_SLOT.with("index", list_index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A position past a slice's end is an error that names it.
    #[test]
    fn position_past_the_end_is_an_error_naming_it() {
        let mut values = [1, 2, 3];
        assert_eq!(*values.checked(2).unwrap(), 3);
        *values.checked_mut(0).unwrap() = 9;
        assert_eq!(values, [9, 2, 3]);
        let checked = values.checked(3).unwrap_err();
        assert_eq!(checked.context().collect::<Vec<_>>(), [("index", "3")]);
        assert!(values.checked_mut(4).is_err());
        assert_eq!(values.head(2).unwrap(), &[9, 2]);
        assert!(values.head(4).is_err());
        values.tail_mut(1).unwrap()[0] = 7;
        assert_eq!(values, [9, 7, 3]);
        assert_eq!(values.tail_mut(3).unwrap(), [0_i32; 0]);
        let error = values.tail_mut(4).unwrap_err();
        assert_eq!(error.context().collect::<Vec<_>>(), [("start", "4")]);
    }

    /// A player or a position the table does not hold is an error that names it.
    #[test]
    fn missing_player_or_slot_is_an_error_naming_it() {
        let mut players = [Player::default(), Player::default()];
        assert!(players.player(PlayerId::new(1)).is_ok());
        let player = players.player(PlayerId::new(2)).unwrap_err();
        assert_eq!(player.context().collect::<Vec<_>>(), [("player", "2")]);
        assert!(players.player_mut(PlayerId::new(5)).is_err());
        let error = players
            .live(PlayerId::new(1), ListIndex::new(0))
            .unwrap_err();
        let keys: Vec<_> = error
            .context()
            .map(|(key, text)| (key, text.to_owned()))
            .collect();
        assert_eq!(
            keys,
            [("index", "0".to_owned()), ("player", "1".to_owned())]
        );
        assert!(
            players
                .live_mut(PlayerId::new(0), ListIndex::new(3))
                .is_err()
        );
        assert!(players.asleep(PlayerId::new(0), ListIndex::new(0)).is_err());
        assert!(
            players
                .asleep_mut(PlayerId::new(9), ListIndex::new(0))
                .is_err()
        );
        assert!(
            players
                .player_mut(PlayerId::new(0))
                .unwrap()
                .asleep_mut(ListIndex::new(0))
                .is_err()
        );
        assert!(
            players
                .player(PlayerId::new(0))
                .unwrap()
                .live(ListIndex::new(0))
                .is_err()
        );
    }
}
