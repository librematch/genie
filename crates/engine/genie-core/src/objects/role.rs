//! An object's id in a role: the id of the object a field or a routine needs in one part (the
//! container a unit is inside, a foundation, a pasture, a drop site, a building's main part, a
//! missile's shooter, an action's target: the Gather's, the Hunt's, the Repair's, the Attack's,
//! the missile's), so that an id of another role or of any object does not compile there.

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

use crate::objects::{ObjectId, ObjectKey};

/// A part an object plays for another ([`RoleId`]): the role's name, which `Debug` prints.
pub trait Role {
    /// The role's type name (`ContainerId`).
    const NAME: &'static str;
}

/// An object's id in role `R` (the user, 2026-10-30: "so we can't put a random object into
/// `footprint_clearing`"): a routine of the role takes it, a routine of any object the
/// [`ObjectId`] it converts to ([`RoleId::id`], [`ObjectKey`] for the lookups). The game does
/// not check the role where it stores the id (BUILD's builders take any object as their
/// foundation, a missile keeps the id of a shooter that is gone), so no test makes one: the
/// role starts where the save's field of the role is read or where the code gives the object
/// the role ([`RoleId::new`]), and the type carries it from there.
pub struct RoleId<R: Role>(ObjectId, PhantomData<fn() -> R>);

impl<R: Role> RoleId<R> {
    /// Object `id` in the role: the save's field of the role, or the routine that gives the
    /// object the role.
    #[must_use]
    pub const fn new(id: ObjectId) -> Self {
        Self(id, PhantomData)
    }

    /// The id of a raw field of the role that holds −1 for none ([`ObjectId::of_field`]).
    #[must_use]
    pub const fn of_field(value: i32) -> Option<Self> {
        match ObjectId::of_field(value) {
            Some(id) => Some(Self::new(id)),
            None => None,
        }
    }

    /// The object's id, for a routine of any object.
    #[must_use]
    pub const fn id(self) -> ObjectId {
        self.0
    }
}

impl<R: Role> Clone for RoleId<R> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<R: Role> Copy for RoleId<R> {}

impl<R: Role> PartialEq for RoleId<R> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<R: Role> Eq for RoleId<R> {}

impl<R: Role> PartialOrd for RoleId<R> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<R: Role> Ord for RoleId<R> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl<R: Role> Hash for RoleId<R> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl<R: Role> fmt::Debug for RoleId<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({})", R::NAME, self.0)
    }
}

impl<R: Role> fmt::Display for RoleId<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<R: Role> From<RoleId<R>> for ObjectId {
    fn from(role: RoleId<R>) -> Self {
        role.0
    }
}

impl<R: Role> ObjectKey for RoleId<R> {
    fn key(self) -> Option<ObjectId> {
        Some(self.0)
    }
}

/// A map of a role's objects by id hashes the id's value, as one of any objects
/// ([`crate::math::msvc::IntKey`]).
impl<R: Role> crate::math::msvc::IntKey for RoleId<R> {
    fn int(self) -> i32 {
        self.0.get()
    }
}

impl<R: Role> genie_diff::Diff for RoleId<R> {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::role::{Role, RoleId};
    use crate::objects::{ObjectId, ObjectKey};

    /// A role of the tests only.
    enum Tested {}

    impl Role for Tested {
        const NAME: &'static str = "TestedId";
    }

    /// Object `value` in the tests' role.
    fn tested(value: i32) -> RoleId<Tested> {
        RoleId::new(ObjectId::try_from(value).unwrap())
    }

    /// A role's id names its object for any object's routine and its lookups, and prints its
    /// role.
    #[test]
    fn role_id_names_its_object_and_prints_its_role() {
        let id = tested(7);
        assert_eq!(id.id(), ObjectId::try_from(7).unwrap());
        assert_eq!(ObjectId::from(id), id.id());
        assert_eq!(id.key(), Some(id.id()));
        assert_eq!(Some(id).key(), Some(id.id()));
        assert_eq!(None::<RoleId<Tested>>.key(), None);
        assert_eq!(format!("{id:?}"), "TestedId(7)");
        assert_eq!(id.to_string(), "7");
    }

    /// A raw field's −1 is none, another value the object in the role; the ids order and compare
    /// as their objects'.
    #[test]
    fn field_minus_one_is_none_and_ids_order_as_their_objects() {
        assert_eq!(RoleId::<Tested>::of_field(-1), None);
        assert_eq!(RoleId::<Tested>::of_field(-2), Some(tested(-2)));
        assert!(tested(-2) < tested(3));
        assert_eq!(tested(3), tested(3).clone());
        assert_ne!(tested(3), tested(4));
    }
}
