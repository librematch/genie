//! A master's gather group as one type: the kind of resource the unit AI's resource searches
//! look for, the value the `.dat` names `resource_gather_group`.

use crate::GenieError;
use std::fmt::{Display, Error as FmtError, Formatter};

/// A master's gather group (`+0x683`, the `.dat`'s `resource_gather_group`): the kind of
/// resource an object that gathers by itself (`enable_auto_gather`) offers, by which the unit
/// AI's resource searches (`ClosestResourceObject`, `genie_core::unit_ai::ResourceSearch`) and the
/// players' visible resources group the objects. The civilian's slot 0x1b0 gives one by a
/// target's class (`genie_core::unit_ai::civilian_kind`), the work's search by a resource held
/// (`genie_core::gathering::gather_search::gather_group`). The `.dat` may hold others, so the
/// group is a newtype with named constants.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GatherGroup(u8);

impl GatherGroup {
    /// The trees' (class 15), wood.
    pub const WOOD: Self = Self(0);
    /// The forage bushes' (class 7), food.
    pub const FORAGE: Self = Self(1);
    /// The fish's (classes 5, 31 and 33); a resource held other than the four's searches it
    /// too.
    pub const FISH: Self = Self(2);
    /// The stone mines' (class 8).
    pub const STONE: Self = Self(3);
    /// The gold mines' (classes 32 and 63).
    pub const GOLD: Self = Self(4);

    /// Its value.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// Its place in a table by group (the visible resources' lists).
    #[must_use]
    pub fn index(self) -> usize {
        usize::from(self.0)
    }
}

impl From<u8> for GatherGroup {
    /// The group a master's byte holds: every value is one.
    fn from(value: u8) -> Self {
        Self(value)
    }
}

impl TryFrom<i32> for GatherGroup {
    type Error = GenieError;

    /// The group of a saved or passed `i32`.
    ///
    /// # Errors
    ///
    /// A value outside a byte.
    fn try_from(value: i32) -> Result<Self, GenieError> {
        u8::try_from(value).map(Self).map_err(|_| {
            GenieError::invalid_input("a gather group outside a byte").with("group", value)
        })
    }
}

impl Display for GatherGroup {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), FmtError> {
        self.0.fmt(f)
    }
}

impl genie_diff::Diff for GatherGroup {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::ErrorKind;
    use crate::objects::masters::GatherGroup;

    /// A master's byte is its group and its table's place; an `i32` outside a byte is no group.
    #[test]
    fn gather_group_is_byte() {
        assert_eq!(GatherGroup::from(4), GatherGroup::GOLD);
        assert_eq!((GatherGroup::FISH.get(), GatherGroup::FISH.index()), (2, 2));
        assert_eq!(GatherGroup::try_from(3), Ok(GatherGroup::STONE));
        for bad in [-1_i32, 256] {
            assert_eq!(
                GatherGroup::try_from(bad).map_err(|e| e.kind()),
                Err(ErrorKind::InvalidInput)
            );
        }
    }
}
