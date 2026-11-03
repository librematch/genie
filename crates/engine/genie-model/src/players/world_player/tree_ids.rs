//! The ids the tech tree table holds: its masters and its technologies, as the game's ints.

use crate::objects::masters::MasterId;
use crate::technology::TechId;
use serde::Deserialize;
use serde::Serialize;

/// A master the tech tree names (a building, a unit, a link: an int of the tree's JSON). A
/// lookup answers 0 out of the ages or without the civilisation, −1 without the building or
/// the unit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TreeMaster(i32);

impl TreeMaster {
    /// −1: no master.
    pub const NONE: Self = Self(-1);

    /// The master of the tree's int `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// The int.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }

    /// The tree's int of the master `id`.
    #[must_use]
    pub fn of_master(id: MasterId) -> Self {
        Self(i32::from(id.get()))
    }

    /// The master it names, none outside the masters' ids.
    #[must_use]
    pub fn master(self) -> Option<MasterId> {
        i16::try_from(self.0).ok().and_then(MasterId::of_data)
    }
}

/// A technology the tech tree names (an int of the tree's JSON; a lookup answers 0 or −1 as for
/// [`TreeMaster`]).
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TreeTech(i32);

impl TreeTech {
    /// The technology of the tree's int `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// The int.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }

    /// The tree's int of the technology `id`.
    #[must_use]
    pub fn of_tech(id: TechId) -> Self {
        Self(i32::from(id.get()))
    }

    /// Its place in a player's technologies, none below 0.
    #[must_use]
    pub fn index(self) -> Option<usize> {
        usize::try_from(self.0).ok()
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::MasterId;
    use crate::players::world_player::{TreeMaster, TreeTech};
    use crate::technology::TechId;

    /// A tree's int names a master or a technology only within their ids.
    #[test]
    fn tree_int_names_an_id_within_range() {
        assert_eq!(
            TreeMaster::of_master(MasterId::named(74)).master(),
            Some(MasterId::named(74))
        );
        assert_eq!(TreeMaster::NONE.master(), None);
        assert_eq!(TreeMaster::of_data(0x10000).master(), None);
        assert_eq!(TreeTech::of_tech(TechId::FIRST_AGE).index(), Some(101));
        assert_eq!(TreeTech::of_data(-1).index(), None);
        assert_eq!(TreeTech::of_data(7).get(), 7);
    }
}
