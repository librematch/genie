//! The terrain classes an object may stand on, as the obstruction manager's tile words test them.

use std::fmt::{Display, Error, Formatter};

/// The terrain classes an object may stand on (its terrain restriction's, `FUN_141054630`'s
/// table): a bit per class of the obstruction manager's tile word, whose low byte holds the
/// tile's class ([`crate::map::terrain::terrain_class`]); bit 7 of the word is a temporary
/// record's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TerrainMask(u8);

impl TerrainMask {
    /// No class: an object of a restriction the table lacks.
    pub const NONE: Self = Self(0);
    /// Every class.
    pub const ALL: Self = Self(0xff);

    /// The mask of class bits `bits`.
    #[must_use]
    pub const fn of_bits(bits: u8) -> Self {
        Self(bits)
    }

    /// Its bits.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// The mask with `other`'s classes too (`OR`).
    #[must_use]
    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// The tile word's class bits that block an object of the mask: the classes outside it
    /// (`mask ^ 0xff`).
    #[must_use]
    pub const fn blocked(self) -> u64 {
        (self.0 ^ 0xff) as u64
    }

    /// Its classes as a tile word holds them below the temporary record's bit (`mask & 0x7f`,
    /// `FUN_141052b90`).
    #[must_use]
    pub const fn classes(self) -> u64 {
        (self.0 & 0x7f) as u64
    }
}

impl Display for TerrainMask {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{:#04x}", self.0)
    }
}

impl genie_diff::Diff for TerrainMask {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::map::TerrainMask;

    /// A mask blocks the classes outside it; its classes leave the temporary record's bit out.
    #[test]
    fn mask_blocks_the_classes_outside_it() {
        let land = TerrainMask::of_bits(0x83);
        assert_eq!(
            (land.bits(), land.blocked(), land.classes()),
            (0x83, 0x7c, 0x03)
        );
        assert_eq!(TerrainMask::NONE.blocked(), 0xff);
        assert_eq!(TerrainMask::ALL.blocked(), 0);
    }
}
