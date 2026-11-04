//! A master's obstruction class as one type: what its obstruction record blocks, the value the
//! `.dat` names `obstruction_class`.

use derive_more::Display;
use serde::Deserialize;
use serde::Serialize;

/// A master's obstruction class (`+0x652`, the `.dat`'s `obstruction_class`, the blockage
/// class; an obstruction record's class): the kind of object its record blocks as. The game's
/// tests name 0 (no record) and 5 (a gate, whose tile marks carry bit `0x8000`); the `.dat`'s
/// other classes are named by the masters that hold them.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    PartialOrd,
    Ord,
    Display,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ObstructionClass(u8);

impl ObstructionClass {
    /// Class 0: no record and no mesh shape (`FUN_141056460`).
    pub const NONE: Self = Self(0);
    /// Class 1: the `.dat`'s trees, rocks and props.
    pub const RESOURCE: Self = Self(1);
    /// Class 2: the `.dat`'s units.
    pub const UNIT: Self = Self(2);
    /// Class 3: the `.dat`'s buildings.
    pub const BUILDING: Self = Self(3);
    /// Class 4: the `.dat`'s walls and the gates' fixed sides.
    pub const WALL: Self = Self(4);
    /// Class 5: the `.dat`'s gate parts; a record of it marks its tiles with bit `0x8000`.
    pub const GATE: Self = Self(5);
    /// Class 6: the `.dat`'s cliffs.
    pub const CLIFF: Self = Self(6);

    /// Its value.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// The class's bits in a tile's mark (`FUN_141056460`: the low six).
    #[must_use]
    pub const fn mark_bits(self) -> u8 {
        self.0 & 0x3f
    }
}

impl From<u8> for ObstructionClass {
    /// The class a master's byte holds: every value is one.
    fn from(value: u8) -> Self {
        Self(value)
    }
}

impl genie_diff::Diff for ObstructionClass {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        differ.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::ObstructionClass;

    /// A class keeps its byte and marks a tile with its low six bits.
    #[test]
    fn class_marks_its_low_six_bits() {
        assert_eq!(ObstructionClass::from(5), ObstructionClass::GATE);
        assert_eq!(ObstructionClass::GATE.get(), 5);
        assert_eq!(ObstructionClass::from(0xc3).mark_bits(), 3);
        assert_eq!(ObstructionClass::default(), ObstructionClass::NONE);
    }
}
