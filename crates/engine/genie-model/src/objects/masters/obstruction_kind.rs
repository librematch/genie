//! A master's obstruction type as one type: the shape of the record its objects file in the
//! obstruction manager, the value the `.dat` names `obstruction_type`.

use std::fmt::{Display, Error, Formatter};

/// A master's obstruction type (`+0x651`, the `.dat`'s `obstruction_type`; an obstruction
/// record's byte `+1`): whether its objects get a record and a mesh shape, the shape the walk
/// tests (a box or a circle) and whether the record marks its tiles. The `.dat` holds types the
/// simulation does not tell apart, so the type is a newtype: the named ones are those its tests
/// name.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ObstructionKind(u8);

impl ObstructionKind {
    /// Type 0: no record and no mesh shape.
    pub const NONE: Self = Self(0);
    /// Type 1: a record that stays filed when the object's marks clear.
    pub const TYPE_1: Self = Self(1);
    /// Type 2: a box.
    pub const TYPE_2: Self = Self(2);
    /// Type 3: a box the walk tests and that marks no tiles.
    pub const TYPE_3: Self = Self(3);
    /// Type 5: a circle (`FUN_141051630`).
    pub const CIRCLE: Self = Self(5);
    /// Type 10, which the game files as 2 (a gate of it opens through `FUN_140f65580`).
    pub const TYPE_10: Self = Self(10);
    /// Type 11: a box of the master's outline radii.
    pub const TYPE_11: Self = Self(11);
    /// Type 12: no record and no mesh shape, as 0.
    pub const TYPE_12: Self = Self(12);
    /// Type 13: a box.
    pub const TYPE_13: Self = Self(13);

    /// Its value.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// The type an attribute effect sets from a byte (`FUN_141125ae0`): its 14-byte table keeps
    /// 0 to 3, 5 and 10 to 13 and makes the others 0. A byte past the table reads the routine's
    /// stack (the game's undefined read, which no `.dat` effect reaches): kept.
    #[must_use]
    pub const fn set(value: u8) -> Self {
        if value < 14 && (0x3c2f_u32 >> value) & 1 == 0 {
            Self::NONE
        } else {
            Self(value)
        }
    }

    /// `FUN_141056460`'s first test on the type: neither 0 nor 12 (the master's obstruction
    /// class decides too).
    #[must_use]
    pub const fn recorded(self) -> bool {
        self.0 != Self::NONE.0 && self.0 != Self::TYPE_12.0
    }

    /// The type the game files a record of this type as: 10 as 2.
    #[must_use]
    pub const fn filed(self) -> Self {
        if self.0 == Self::TYPE_10.0 {
            Self::TYPE_2
        } else {
            self
        }
    }

    /// The types whose records mark the tiles under them (`FUN_141056460`: 0, 1, 2, 11, 13 and
    /// 10, which the game files as 2).
    #[must_use]
    pub const fn marks_tiles(self) -> bool {
        self.0 == Self::TYPE_10.0 || (self.0 < 14 && (0x2807_u32 >> self.0) & 1 != 0)
    }

    /// The (filed) types the queries test (`FUN_141051e00`): the boxes 2, 3, 11 and 13 and the
    /// circle 5.
    #[must_use]
    pub const fn tested(self) -> bool {
        matches!(self.0, 2 | 3 | 5 | 11 | 13)
    }
}

impl From<u8> for ObstructionKind {
    /// The type a master's byte holds: every value is one.
    fn from(value: u8) -> Self {
        Self(value)
    }
}

impl Display for ObstructionKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        self.0.fmt(f)
    }
}

impl genie_diff::Diff for ObstructionKind {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::ObstructionKind;

    /// 0 and 12 get no record; 10 is filed as 2; the boxes and the circle are tested; the types
    /// 0, 1, 2, 10, 11 and 13 mark their tiles.
    #[test]
    fn the_types_record_file_test_and_mark_as_the_game_does() {
        let types = |f: fn(ObstructionKind) -> bool| -> Vec<u8> {
            (0..16)
                .map(ObstructionKind::from)
                .filter(|&t| f(t))
                .map(ObstructionKind::get)
                .collect()
        };
        assert_eq!(
            types(ObstructionKind::recorded),
            [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 14, 15]
        );
        assert_eq!(ObstructionKind::TYPE_10.filed(), ObstructionKind::TYPE_2);
        assert_eq!(ObstructionKind::CIRCLE.filed(), ObstructionKind::CIRCLE);
        assert_eq!(types(ObstructionKind::tested), [2, 3, 5, 11, 13]);
        assert_eq!(types(ObstructionKind::marks_tiles), [0, 1, 2, 10, 11, 13]);
    }

    /// An effect keeps the table's types and the bytes past it and makes the others 0.
    #[test]
    fn an_effect_sets_the_tables_types_and_clears_the_others() {
        let set: Vec<u8> = (0..16).map(|b| ObstructionKind::set(b).get()).collect();
        assert_eq!(set, [0, 1, 2, 3, 0, 5, 0, 0, 0, 0, 10, 11, 12, 13, 14, 15]);
    }
}
