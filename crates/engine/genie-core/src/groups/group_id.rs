//! A unit group's id as one type: the id the world gives each group it creates, which a unit's
//! group link, a group's differentiator groups and the lookups by id name.

use crate::{GenieError, GenieResult};
use derive_more::Display;
use getset::CopyGetters;

/// A unit group's id (`mID`, `+0xd8`): the world's group counter (`mUnitGroupCounter`,
/// `world+0x5c`) as the group was created (`FUN_14152efc0`). A unit's link (`mGroupID`, `+0x6d0`)
/// takes −1 for no group, and the lookup by id (`FUN_14152ff40`) finds none for −1, so no
/// group's id is −1: such a field holds `Option<GroupId>` ([`GroupId::of_field`]).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, CopyGetters, Display,
)]
pub struct GroupId(
    /// Its value, as the group and the fields that name it hold it.
    #[getset(get_copy = "pub")]
    i32,
);

impl GroupId {
    /// The value a field takes for none.
    const NONE: i32 = -1;

    /// The id the world gives the group after this one (`FUN_14152efc0`: the counter up by one,
    /// a 32-bit add).
    #[must_use]
    pub const fn successor(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    /// The group of a raw field that holds −1 for none.
    #[must_use]
    pub const fn of_field(value: i32) -> Option<Self> {
        if value == Self::NONE {
            None
        } else {
            Some(Self(value))
        }
    }

    /// The value of a field that holds −1 for none, for an outgoing boundary only (a report line
    /// in the game's terms, a save field the game writes).
    #[must_use]
    pub const fn field(id: Option<Self>) -> i32 {
        match id {
            Some(id) => id.0,
            None => Self::NONE,
        }
    }
}

impl genie_diff::Diff for GroupId {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

impl TryFrom<i32> for GroupId {
    type Error = GenieError;

    /// The group of id `value`.
    ///
    /// # Errors
    ///
    /// −1, which every field that names a group takes for none.
    fn try_from(value: i32) -> GenieResult<Self> {
        Self::of_field(value).ok_or_else(|| {
            GenieError::invalid_input("a group id of −1, which names none").with("group", value)
        })
    }
}

/// A saved list of group ids holds each as a 32-bit word (`mDifferentiatorGroups`).
impl TryFrom<u32> for GroupId {
    type Error = GenieError;

    /// The group of the word `value`, read as the game's `int`.
    ///
    /// # Errors
    ///
    /// The word of −1, which names no group.
    fn try_from(value: u32) -> GenieResult<Self> {
        Self::try_from(i32::from_ne_bytes(value.to_ne_bytes()))
    }
}

#[cfg(test)]
mod tests {
    use crate::ErrorKind;
    use crate::groups::GroupId;

    /// A field's −1 is none, any other value a group, and each converts back to its value.
    #[test]
    fn field_minus_one_is_none_and_any_other_value_group() {
        assert_eq!(GroupId::of_field(-1), None);
        for value in [0, 5, -2, i32::MIN, i32::MAX] {
            assert_eq!(GroupId::of_field(value).map(|g| g.get()), Some(value));
            assert_eq!(GroupId::field(GroupId::of_field(value)), value);
        }
        assert_eq!(GroupId::field(None), -1);
    }

    /// −1 is no group, as a value and as a saved word.
    #[test]
    fn minus_one_is_no_group() {
        assert_eq!(
            GroupId::try_from(-1_i32).map_err(|e| e.kind()),
            Err(ErrorKind::InvalidInput)
        );
        assert_eq!(
            GroupId::try_from(u32::MAX).map_err(|e| e.kind()),
            Err(ErrorKind::InvalidInput)
        );
        assert_eq!(GroupId::try_from(7_u32).map(|g| g.get()), Ok(7));
    }

    /// The next group's id is one more, wrapping as the game's 32-bit add.
    #[test]
    fn the_next_group_id_is_one_more() {
        let id = GroupId::try_from(6_i32).unwrap();
        assert_eq!(id.successor().get(), 7);
        assert_eq!(
            GroupId::try_from(i32::MAX).unwrap().successor().get(),
            i32::MIN
        );
    }
}
