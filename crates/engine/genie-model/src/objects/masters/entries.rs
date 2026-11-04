//! A master's selected entries: the damage sprite (`+0x731`) and the train location (`+0x940`)
//! the effects change.

use serde::Deserialize;
use serde::Serialize;

/// The damage sprite entry the attributes 91, 156 and 157 change (`+0x731`,
/// `mDamageSpriteEntryMod`, a byte): an index into the damage sprites, 0xff with none left.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Hash,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct DamageGraphicEntry(u8);

impl DamageGraphicEntry {
    /// The entry of the save's byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// The entry an effect selects: the int `n`'s low byte.
    #[must_use]
    pub const fn of_selected(n: i32) -> Self {
        Self(n as u8)
    }

    /// The byte.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// The index into the damage sprites.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// The train location entry the attributes 42, 43, 58 and 101 change (`+0x940`,
/// `mTrainLocationEntryMod`, a signed short): an index into the train locations, −1 with none.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Hash,
)]
pub struct TrainLocationEntry(i16);

impl TrainLocationEntry {
    /// The entry of the save's short `value`.
    #[must_use]
    pub const fn of_data(value: i16) -> Self {
        Self(value)
    }

    /// The entry an effect selects: the int `n`'s low 16 bits.
    #[must_use]
    pub const fn of_selected(n: i32) -> Self {
        Self(n as i16)
    }

    /// The short.
    #[must_use]
    pub const fn get(self) -> i16 {
        self.0
    }

    /// The index into the train locations (`FUN_1412669d0`: sign-extended, then unsigned, so a
    /// negative entry is beyond any count).
    #[must_use]
    pub fn index(self) -> Option<usize> {
        usize::try_from(i64::from(self.0) as u64).ok()
    }
}

impl genie_diff::Diff for DamageGraphicEntry {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

impl genie_diff::Diff for TrainLocationEntry {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::{DamageGraphicEntry, TrainLocationEntry};

    /// An effect's int selects by its low bits; a negative train location entry selects none
    /// that a list holds.
    #[test]
    fn the_entries_select_by_their_low_bits() {
        assert_eq!(DamageGraphicEntry::of_selected(-1).get(), 0xff);
        assert_eq!(DamageGraphicEntry::of_selected(0x102).index(), 2);
        assert_eq!(TrainLocationEntry::of_selected(0x1_0003).get(), 3);
        assert_eq!(TrainLocationEntry::of_data(2).index(), Some(2));
        let none = TrainLocationEntry::of_data(-1).index();
        assert!(none.is_none_or(|i| i > 1 << 40));
    }
}
