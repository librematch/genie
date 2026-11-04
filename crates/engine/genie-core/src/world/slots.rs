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

/// Player `p`, and the objects at a position of its lists, as errors where a slice's index
/// would panic.
pub(crate) trait PlayerSlots {
    /// Player `p`.
    ///
    /// # Errors
    ///
    /// A player the table does not hold.
    fn player(&self, p: PlayerId) -> GenieResult<&Player>;

    /// Player `p`, to change.
    ///
    /// # Errors
    ///
    /// A player the table does not hold.
    fn player_mut(&mut self, p: PlayerId) -> GenieResult<&mut Player>;

    /// The object at position `i` of player `p`'s live list.
    ///
    /// # Errors
    ///
    /// A player or a position the lists do not hold.
    fn live(&self, p: PlayerId, i: ListIndex) -> GenieResult<&Unit> {
        self.player(p)?.live(i).map_err(|e| e.with("player", p))
    }

    /// The object at position `i` of player `p`'s live list, to change.
    ///
    /// # Errors
    ///
    /// A player or a position the lists do not hold.
    fn live_mut(&mut self, p: PlayerId, i: ListIndex) -> GenieResult<&mut Unit> {
        self.player_mut(p)?
            .live_mut(i)
            .map_err(|e| e.with("player", p))
    }

    /// The object at position `i` of player `p`'s sleeping list.
    ///
    /// # Errors
    ///
    /// A player or a position the lists do not hold.
    fn asleep(&self, p: PlayerId, i: ListIndex) -> GenieResult<&Unit> {
        self.player(p)?.asleep(i).map_err(|e| e.with("player", p))
    }

    /// The object at position `i` of player `p`'s sleeping list, to change.
    ///
    /// # Errors
    ///
    /// A player or a position the lists do not hold.
    fn asleep_mut(&mut self, p: PlayerId, i: ListIndex) -> GenieResult<&mut Unit> {
        self.player_mut(p)?
            .asleep_mut(i)
            .map_err(|e| e.with("player", p))
    }

    /// The object at `place` (the world's table by id gave it).
    ///
    /// # Errors
    ///
    /// A player or a position the lists do not hold.
    fn at(&self, place: Place) -> GenieResult<&Unit> {
        match place {
            Place::Live(p, i) => self.live(p, i),
            Place::Sleeping(p, i) => self.asleep(p, i),
        }
    }

    /// The object at `place`, to change.
    ///
    /// # Errors
    ///
    /// A player or a position the lists do not hold.
    fn at_mut(&mut self, place: Place) -> GenieResult<&mut Unit> {
        match place {
            Place::Live(p, i) => self.live_mut(p, i),
            Place::Sleeping(p, i) => self.asleep_mut(p, i),
        }
    }
}

/// A position of a slice the caller holds, as an error where `v[i]` would panic: the index
/// comes from the code's own loop or search, so a miss is an invariant broken.
pub(crate) trait CheckedIndex<T> {
    /// The element at `i`.
    ///
    /// # Errors
    ///
    /// A position the slice does not hold.
    fn checked(&self, i: usize) -> GenieResult<&T>;

    /// The element at `i`, to change.
    ///
    /// # Errors
    ///
    /// A position the slice does not hold.
    fn checked_mut(&mut self, i: usize) -> GenieResult<&mut T>;

    /// The first `n` elements.
    ///
    /// # Errors
    ///
    /// A slice shorter than `n`.
    fn head(&self, n: usize) -> GenieResult<&[T]>;

    /// The first `n` elements, to change.
    ///
    /// # Errors
    ///
    /// A slice shorter than `n`.
    fn head_mut(&mut self, n: usize) -> GenieResult<&mut [T]>;

    /// The elements from `from` on, to change: none at the end.
    ///
    /// # Errors
    ///
    /// A slice shorter than `from`.
    fn tail_mut(&mut self, from: usize) -> GenieResult<&mut [T]>;
}

impl<T> CheckedIndex<T> for [T] {
    fn checked(&self, i: usize) -> GenieResult<&T> {
        self.get(i).ok_or_else(|| NO_ELEMENT.with("index", i))
    }

    fn checked_mut(&mut self, i: usize) -> GenieResult<&mut T> {
        self.get_mut(i).ok_or_else(|| NO_ELEMENT.with("index", i))
    }

    fn head(&self, n: usize) -> GenieResult<&[T]> {
        self.get(..n).ok_or_else(|| NO_ELEMENT.with("end", n))
    }

    fn head_mut(&mut self, n: usize) -> GenieResult<&mut [T]> {
        self.get_mut(..n).ok_or_else(|| NO_ELEMENT.with("end", n))
    }

    fn tail_mut(&mut self, from: usize) -> GenieResult<&mut [T]> {
        self.get_mut(from..)
            .ok_or_else(|| NO_ELEMENT.with("start", from))
    }
}

impl PlayerSlots for [Player] {
    fn player(&self, p: PlayerId) -> GenieResult<&Player> {
        self.get(p.index())
            .ok_or_else(|| NO_PLAYER.with("player", p))
    }

    fn player_mut(&mut self, p: PlayerId) -> GenieResult<&mut Player> {
        self.get_mut(p.index())
            .ok_or_else(|| NO_PLAYER.with("player", p))
    }
}

impl Player {
    /// The object at position `i` of the live list.
    ///
    /// # Errors
    ///
    /// A position the list does not hold.
    pub(crate) fn live(&self, i: ListIndex) -> GenieResult<&Unit> {
        self.objects.get(i).ok_or_else(|| NO_SLOT.with("index", i))
    }

    /// The object at position `i` of the live list, to change.
    ///
    /// # Errors
    ///
    /// A position the list does not hold.
    pub(crate) fn live_mut(&mut self, i: ListIndex) -> GenieResult<&mut Unit> {
        self.objects
            .get_mut(i)
            .ok_or_else(|| NO_SLOT.with("index", i))
    }

    /// The object at position `i` of the sleeping list.
    ///
    /// # Errors
    ///
    /// A position the list does not hold.
    pub(crate) fn asleep(&self, i: ListIndex) -> GenieResult<&Unit> {
        self.sleeping.get(i).ok_or_else(|| NO_SLOT.with("index", i))
    }

    /// The object at position `i` of the sleeping list, to change.
    ///
    /// # Errors
    ///
    /// A position the list does not hold.
    pub(crate) fn asleep_mut(&mut self, i: ListIndex) -> GenieResult<&mut Unit> {
        self.sleeping
            .get_mut(i)
            .ok_or_else(|| NO_SLOT.with("index", i))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A position past a slice's end is an error that names it.
    #[test]
    fn position_past_the_end_is_an_error_naming_it() {
        let mut v = [1, 2, 3];
        assert_eq!(*v.checked(2).unwrap(), 3);
        *v.checked_mut(0).unwrap() = 9;
        assert_eq!(v, [9, 2, 3]);
        let e = v.checked(3).unwrap_err();
        assert_eq!(e.context().collect::<Vec<_>>(), [("index", "3")]);
        assert!(v.checked_mut(4).is_err());
        assert_eq!(v.head(2).unwrap(), &[9, 2]);
        assert!(v.head(4).is_err());
        v.tail_mut(1).unwrap()[0] = 7;
        assert_eq!(v, [9, 7, 3]);
        assert_eq!(v.tail_mut(3).unwrap(), [0_i32; 0]);
        let e = v.tail_mut(4).unwrap_err();
        assert_eq!(e.context().collect::<Vec<_>>(), [("start", "4")]);
    }

    /// A player or a position the table does not hold is an error that names it.
    #[test]
    fn missing_player_or_slot_is_an_error_naming_it() {
        let mut players = [Player::default(), Player::default()];
        assert!(players.player(PlayerId::new(1)).is_ok());
        let e = players.player(PlayerId::new(2)).unwrap_err();
        assert_eq!(e.context().collect::<Vec<_>>(), [("player", "2")]);
        assert!(players.player_mut(PlayerId::new(5)).is_err());
        let e = players
            .live(PlayerId::new(1), ListIndex::new(0))
            .unwrap_err();
        let keys: Vec<_> = e.context().map(|(k, v)| (k, v.to_owned())).collect();
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
