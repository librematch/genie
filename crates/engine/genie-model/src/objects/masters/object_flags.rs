//! A master's object flags ([`ObjectFlags`]): its traits and the master its upper half names.

use crate::objects::masters::MasterId;
use serde::Deserialize;
use serde::Serialize;

bitflags::bitflags! {
    /// A master's object flags (`+0x654`, the `.dat`'s unit trait in the low byte): what kind
    /// of unit it is, by the masters build 158041's `.dat` gives each bit. The upper half
    /// (`+0x656`) is the trait piece, a master ([`ObjectFlags::piece`]). Every bit is kept
    /// (`_ = !0`).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct ObjectFlags: u32 {
        /// It takes units inside (bit 0: the rams, the siege towers, the transport ships;
        /// AGE's "Garrison unit").
        const GARRISON = 1;
        /// A ship (bit 1: AGE's "Ship unit"): it turns fast.
        const SHIP = 1 << 1;
        /// A unit that builds a building of its own (bit 2: the Serjeants).
        const BUILDS = 1 << 2;
        /// A unit of two forms (bit 3: the Ratha, the War Chariot, the Immortal): the trait
        /// piece is the other.
        const TRANSFORMS = 1 << 3;
        /// A scout (bit 4: the scout cavalry, the eagles, the camels, the horses): it stands
        /// its ground in the Dark Age and hunts no stored prey.
        const SCOUT = 1 << 4;
        /// A building a gather takes as its target as any other (bit 5; no master of build
        /// 158041's `.dat` has it).
        const GATHERED_BUILDING = 1 << 5;
        /// A building whose completion sets its tiles' classes (bit 6: the ports and the
        /// moles; `FUN_141055790`).
        const TILE_CLASSES = 1 << 6;
        /// The make action takes the train time its queue entry recorded (bit 7,
        /// `FUN_140acf280`; no master of build 158041's `.dat` has it).
        const OWN_TRAIN_TIME = 1 << 7;
        /// Every other bit, and the trait piece.
        const _ = !0;
    }
}

impl ObjectFlags {
    /// No flag and no piece.
    pub const NONE: Self = Self::empty();

    /// The master the upper half names (`+0x656`, the `.dat`'s trait piece, read as the
    /// game's unsigned short); none for −1 and the other negative shorts.
    #[must_use]
    pub fn piece(self) -> Option<MasterId> {
        MasterId::of_data((self.bits() >> 16) as u16 as i16)
    }
}

impl genie_diff::Diff for ObjectFlags {
    /// By equality: the flags are one value.
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        differ.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::{MasterId, ObjectFlags};

    /// The traits are the low bits, the piece the upper half; no bit is dropped.
    #[test]
    fn the_traits_are_the_low_bits_and_the_piece_the_upper_half() {
        let ratha = ObjectFlags::from_bits_retain(8 | (1740 << 16));
        assert!(ratha.contains(ObjectFlags::TRANSFORMS) && !ratha.contains(ObjectFlags::SHIP));
        assert_eq!(ratha.piece(), Some(MasterId::named(1740)));
        assert_eq!(ObjectFlags::from_bits_retain(0xffff_0001).piece(), None);
        assert_eq!(ObjectFlags::NONE.piece(), Some(MasterId::named(0)));
        assert_eq!(ratha.bits(), 8 | (1740 << 16));
        assert_eq!(format!("{:?}", ObjectFlags::SCOUT), "ObjectFlags(SCOUT)");
    }
}
