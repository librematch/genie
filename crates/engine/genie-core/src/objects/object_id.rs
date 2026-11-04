//! An object's id as one type: the id that names an object across ticks, as the commands,
//! their unit lists and their targets hold it.

use crate::math::msvc::IntKey;
use crate::{GenieError, GenieResult};
use derive_more::Display;

/// An object's id (`+0x8`), which names the object across ticks: a live or sleeping object's
/// is 0 or more, a recyclable master's object (a missile, a ghost) has a negative one. Every
/// field that names an object takes −1 for none, so no id is −1: such a field holds
/// `Option<ObjectId>` ([`ObjectId::of_field`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Display)]
pub struct ObjectId(i32);

impl ObjectId {
    /// Its value, as the object and the fields that name it hold it.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }

    /// A recyclable master's object (a missile, a ghost), whose id is negative: the game tests
    /// the sign (`id < 0`) where such an object takes no part (the sight, the pasture).
    #[must_use]
    pub const fn is_recyclable(self) -> bool {
        self.0 < 0
    }

    /// The id the world gives the next new object (`mNextObjectId` counts up).
    #[must_use]
    pub const fn successor(self) -> Self {
        Self(self.0 + 1)
    }

    /// The id a new recyclable object takes after this one (`next_reusable_object_id` counts
    /// down).
    #[must_use]
    pub const fn predecessor(self) -> Self {
        Self(self.0 - 1)
    }

    /// The stand-in for a saved object without a static part, which the slice does not model:
    /// no object's id, and not [`ObjectId::TAKEN_OUT`] (it stays in its list).
    pub const UNSAVED: Self = Self(i32::MIN + 1);

    /// The stand-in for an object a reader takes a view of where the game reads none (its −1):
    /// no object's id, so a lookup or write-back by it finds nothing ([`crate::combat::damage::Victim`]
    /// of a hunt without a target).
    pub(crate) const NOBODY: Self = Self(i32::MIN + 2);

    /// The stand-in a list leaves in the slot of a unit taken out of it
    /// ([`crate::objects::unit::TAKEN_OUT`]): no object's id.
    pub(crate) const TAKEN_OUT: Self = Self(i32::MIN);

    /// The value every field takes for none.
    const NONE: i32 = -1;

    /// The id of a raw field that holds −1 for none.
    #[must_use]
    pub const fn of_field(value: i32) -> Option<Self> {
        if value == Self::NONE {
            None
        } else {
            Some(Self(value))
        }
    }

    /// The value of a field that holds −1 for none, for an outgoing boundary only: a writer of
    /// a format the game reads (the command codec's encoder), a log line in the game's terms, or
    /// where the game turns none into −1 and uses the number (a sort key, [`Attackers`]). genie's
    /// own code keeps the type.
    ///
    /// [`Attackers`]: crate::unit_ai::Attackers
    #[must_use]
    pub const fn field(id: Option<Self>) -> i32 {
        match id {
            Some(id) => id.0,
            None => Self::NONE,
        }
    }
}

impl genie_diff::Diff for ObjectId {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        differ.equal(self, other);
    }
}

/// A map of objects by id hashes the id's value (`std::hash<int>`).
impl IntKey for ObjectId {
    fn int(self) -> i32 {
        self.0
    }
}

/// What a lookup by id takes: an id, or a field that may hold none. A key that names none finds
/// no object, as the game's table finds none for −1 (no object's id is −1).
pub trait ObjectKey: Copy {
    /// The id it names.
    fn key(self) -> Option<ObjectId>;
}

impl ObjectKey for ObjectId {
    fn key(self) -> Option<ObjectId> {
        Some(self)
    }
}

impl<K: ObjectKey> ObjectKey for Option<K> {
    fn key(self) -> Option<ObjectId> {
        self.and_then(ObjectKey::key)
    }
}

impl TryFrom<i32> for ObjectId {
    type Error = GenieError;

    /// The id of value `value`.
    ///
    /// # Errors
    ///
    /// −1, which every field takes for none.
    fn try_from(value: i32) -> GenieResult<Self> {
        Self::of_field(value).ok_or_else(|| {
            GenieError::invalid_input("an object id of −1, which names none").with("id", value)
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::ErrorKind;
    use crate::objects::ObjectId;

    /// An id of `value`, which the test knows is one.
    fn id(value: i32) -> ObjectId {
        ObjectId::try_from(value).unwrap()
    }

    /// A field's −1 is none, any other value an id, and each converts back to its value.
    #[test]
    fn field_minus_one_is_none_and_any_other_value_an_id() {
        assert_eq!(ObjectId::of_field(-1), None);
        for value in [0, 5, -2, i32::MIN, i32::MAX] {
            assert_eq!(ObjectId::of_field(value).map(ObjectId::get), Some(value));
            assert_eq!(ObjectId::field(ObjectId::of_field(value)), value);
        }
        assert_eq!(ObjectId::field(None), -1);
    }

    /// −1 is no id; a negative one other than it is (a recyclable master's object).
    #[test]
    fn minus_one_is_no_id() {
        assert_eq!(
            ObjectId::try_from(-1).map_err(|error| error.kind()),
            Err(ErrorKind::InvalidInput)
        );
        assert_eq!(id(-2).get(), -2);
        assert_eq!(id(7).get(), 7);
    }

    /// A map keyed by ids lists them in the order of one keyed by their values: the game hashes
    /// the value (`std::hash<int>`).
    #[test]
    fn map_of_ids_lists_them_as_their_values() {
        use crate::math::msvc::unordered_map_order;
        let values = [1, 2, 9, 2, -2];
        let by_value: Vec<ObjectId> = unordered_map_order(values).into_iter().map(id).collect();
        assert_eq!(unordered_map_order(values.map(id)), by_value);
        assert_eq!(by_value, [9, 1, 2, -2].map(id));
    }

    /// A lookup key names an id, none or (a raw field) −1 for none.
    #[test]
    fn lookup_key_names_its_id_or_none() {
        use crate::objects::ObjectKey;
        assert_eq!(id(7).key(), Some(id(7)));
        assert_eq!(Some(id(7)).key(), Some(id(7)));
        assert_eq!(None::<ObjectId>.key(), None);
    }
}
