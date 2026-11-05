//! A queued order's issuer: the player whose command gave the order, or the object that
//! queued it.

use crate::objects::ObjectId;
use crate::players::PlayerId;
use std::hash::{Hash, Hasher};

/// A queued order's issuer (`+0x0`): the player whose command gave the order (a move, an
/// unload, a drop site's order) or the object that queued it (a unit's current order kept or
/// queued back, its garrison, its way back to its work). The game holds both as one integer and
/// compares it as one, so two issuers are equal by that value ([`Issuer::value`]): an object's
/// equals the player of the same number.
#[derive(Clone, Copy, Debug)]
pub enum Issuer {
    /// A player's command.
    Player(PlayerId),
    /// An object's own order.
    Object(ObjectId),
}

impl Issuer {
    /// The integer the game holds for the issuer.
    #[must_use]
    pub fn value(self) -> i32 {
        match self {
            Self::Player(player_id) => i32::from(player_id.get()),
            Self::Object(id) => id.get(),
        }
    }

    /// The issuer `value` of a saved order in the queue of object `own`, in a world of
    /// `players` players; none for −1 (a guard's defend order has none). The save holds only
    /// the integer. Every object that queues an order queues it for itself, so the object's own
    /// id names the object, a player's number a player and any other value another object.
    #[must_use]
    pub fn of_saved(value: i32, own: ObjectId, players: usize) -> Option<Self> {
        if value == own.get() {
            return Some(Self::Object(own));
        }
        match u8::try_from(value) {
            Ok(number) if usize::from(number) < players => {
                Some(Self::Player(PlayerId::new(number)))
            }
            _ => ObjectId::of_field(value).map(Self::Object),
        }
    }
}

impl From<PlayerId> for Issuer {
    fn from(player_id: PlayerId) -> Self {
        Self::Player(player_id)
    }
}

impl From<ObjectId> for Issuer {
    fn from(id: ObjectId) -> Self {
        Self::Object(id)
    }
}

impl PartialEq for Issuer {
    fn eq(&self, other: &Self) -> bool {
        self.value() == other.value()
    }
}

impl Eq for Issuer {}

impl Hash for Issuer {
    /// By value, as [`PartialEq`] compares.
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value().hash(state);
    }
}

impl PartialEq<ObjectId> for Issuer {
    fn eq(&self, other: &ObjectId) -> bool {
        self.value() == other.get()
    }
}

impl genie_diff::Diff for Issuer {
    /// By value, as the game compares it.
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        differ.equal(&self.value(), &other.value());
    }
}

#[cfg(test)]
mod tests {
    use crate::players::PlayerId;
    use crate::test_ids::object_id;
    use crate::unit_ai::Issuer;
    use std::hash::{BuildHasher, BuildHasherDefault, DefaultHasher};

    /// A saved issuer is the queue's own object at its id, a player at a player's number and
    /// another object at any other value; −1 names none.
    #[test]
    fn saved_issuer_is_the_own_object_player_or_another_object() {
        let own = object_id(5);
        assert!(matches!(
            Issuer::of_saved(5, own, 9),
            Some(Issuer::Object(id)) if id == own
        ));
        assert!(matches!(
            Issuer::of_saved(3, own, 9),
            Some(Issuer::Player(player_id)) if player_id == PlayerId::new(3)
        ));
        assert!(matches!(
            Issuer::of_saved(9, own, 9),
            Some(Issuer::Object(id)) if id == object_id(9)
        ));
        assert_eq!(Issuer::of_saved(-1, own, 9), None);
    }

    /// Two issuers are equal by their values, as the game compares the integer: an object's
    /// equals the player of its number.
    #[test]
    fn issuers_compare_by_their_values() {
        let player = Issuer::from(PlayerId::new(5));
        assert_eq!(player, Issuer::from(object_id(5)));
        assert_eq!(player, object_id(5));
        assert_ne!(player, Issuer::from(object_id(6)));
        assert_eq!(Issuer::from(object_id(7)).value(), 7);
    }

    /// An issuer hashes by its value, as it compares: a player and the object of its number
    /// hash alike.
    #[test]
    fn issuers_hash_by_their_values() {
        let hasher = BuildHasherDefault::<DefaultHasher>::default();
        let player = hasher.hash_one(Issuer::from(PlayerId::new(5)));
        assert_eq!(player, hasher.hash_one(Issuer::from(object_id(5))));
        assert_ne!(player, hasher.hash_one(Issuer::from(object_id(6))));
    }
}
