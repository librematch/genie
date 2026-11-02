//! A master's type as one type: the byte at `+0x8` that tells a static object from a combat unit
//! or a building, which the `.dat` names `unit_type`.

use genie_diff::{Diff, Differ};
use std::fmt::{Display, Error, Formatter};

/// A master's type (`+0x8`, the `.dat`'s `unit_type`): 10 to 80 in steps the game names
/// ([`crate::objects::masters::master_type`]). The routines that test a type compare it with a
/// threshold (`kind >= COMBAT`), so it is ordered. The `.dat` holds values the code does not name
/// (0 for an empty slot, 90 and above for the data's own kinds), so the type is a newtype with
/// named constants, not an enum: any byte the data holds is a type, and the named ones are values
/// of the type (the reasoning of [`crate::objects::masters::ClassId`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MasterKind(
    /// Its value, as the `.dat` and the save hold it.
    u8,
);

impl MasterKind {
    /// Its value, for a packet, a dump and the checkpoint's words.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// The type a master holds (`+0x8`, the `.dat`'s `unit_type`): any byte is one.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// A type the code names, in a constant: only [`crate::objects::masters::master_type`] makes
    /// them.
    pub(super) const fn named(value: u8) -> Self {
        Self(value)
    }
}

impl Display for MasterKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        self.0.fmt(f)
    }
}

impl Diff for MasterKind {
    fn diff(&self, other: &Self, d: &mut Differ) {
        d.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::{MasterKind, master_type};

    /// Any byte is a type, and the named ones compare as the game's thresholds do.
    #[test]
    fn type_is_any_byte_and_the_named_ones_are_ordered() {
        assert_eq!(MasterKind::of_data(70), master_type::COMBAT);
        assert_eq!(MasterKind::of_data(0).get(), 0);
        assert!(master_type::ACTION < master_type::COMBAT);
        assert!(master_type::BUILDING >= master_type::COMBAT);
    }
}
