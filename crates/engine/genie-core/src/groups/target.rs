//! A group's target: an object, none, or a target of another kind.

use crate::groups::GroupId;
use crate::objects::{ObjectId, ObjectKey};

/// The kind (`mType`) of an object target, which the group's commands on an object write.
const OBJECT: i32 = 1;

/// A group's target (`mTarget` with its kind `mType`): the object a command on an
/// object names, none (both −1, what forgetting the target writes), or a target of another
/// kind. The attack's target object (`FUN_14103f4e0`) reads another kind's target as a group's
/// id; every other reader looks the integer up as an object, whatever the kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GroupTarget {
    /// No target.
    #[default]
    None,
    /// An object (kind 1).
    Object(ObjectId),
    /// A target of another kind, both integers as the save holds them.
    Other {
        /// `mType`.
        kind: i32,
        /// `mTarget`.
        target: i32,
    },
}

impl GroupTarget {
    /// The target of a save's `target` and `kind`.
    #[must_use]
    pub const fn of_saved(target: i32, kind: i32) -> Self {
        match (kind, ObjectId::of_field(target)) {
            (-1, None) => Self::None,
            (OBJECT, Some(id)) => Self::Object(id),
            _ => Self::Other { kind, target },
        }
    }

    /// The `(target, kind)` the save writes.
    #[must_use]
    pub const fn saved(self) -> (i32, i32) {
        match self {
            Self::None => (-1, -1),
            Self::Object(id) => (id.get(), OBJECT),
            Self::Other { kind, target } => (target, kind),
        }
    }

    /// The group's id another kind's target names, as the attack's target object reads it (a
    /// −1 names none: the lookup by id finds none for it).
    #[must_use]
    pub const fn group(self) -> Option<GroupId> {
        match self {
            Self::Other { target, .. } => GroupId::of_field(target),
            Self::None | Self::Object(_) => None,
        }
    }
}

/// A lookup by the target reads its integer, as the game's readers do whatever the kind.
impl ObjectKey for GroupTarget {
    fn key(self) -> Option<ObjectId> {
        ObjectId::of_field(self.saved().0)
    }
}

impl genie_diff::Diff for GroupTarget {
    /// By the two integers the save holds.
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(&self.saved(), &other.saved());
    }
}

#[cfg(test)]
mod tests {
    use crate::groups::GroupTarget;
    use crate::objects::ObjectKey;
    use crate::test_ids::object_id;

    /// A saved target is none at (−1, −1), an object at kind 1 and another kind's target else,
    /// and it writes back the integers it was read from.
    #[test]
    fn saved_target_is_none_an_object_or_another_kind() {
        assert_eq!(GroupTarget::of_saved(-1, -1), GroupTarget::None);
        assert_eq!(
            GroupTarget::of_saved(7, 1),
            GroupTarget::Object(object_id(7))
        );
        assert_eq!(
            GroupTarget::of_saved(3, 2),
            GroupTarget::Other { kind: 2, target: 3 }
        );
        assert_eq!(
            GroupTarget::of_saved(-1, 1),
            GroupTarget::Other {
                kind: 1,
                target: -1
            }
        );
        for (target, kind) in [(-1, -1), (7, 1), (3, 2), (-1, 1), (5, -1)] {
            assert_eq!(GroupTarget::of_saved(target, kind).saved(), (target, kind));
        }
    }

    /// Only another kind's target names a group; a lookup reads the integer of every kind.
    #[test]
    fn lookup_reads_the_integer_of_every_kind() {
        assert_eq!(
            GroupTarget::of_saved(3, 2).group().map(|g| g.get()),
            Some(3)
        );
        assert_eq!(GroupTarget::of_saved(-1, 2).group(), None);
        assert_eq!(GroupTarget::of_saved(7, 1).group(), None);
        assert_eq!(GroupTarget::of_saved(3, 2).key(), Some(object_id(3)));
        assert_eq!(GroupTarget::of_saved(7, 1).key(), Some(object_id(7)));
        assert_eq!(GroupTarget::None.key(), None);
    }
}
