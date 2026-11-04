//! A master's charge targets (`+0x924`): the groups of classes its charge works on.

use serde::Deserialize;
use serde::Serialize;

/// A master's charge targets (`charge_target`, `+0x924`): one bit per group of classes its charge
/// works on (`FUN_1412623d0`); every bit (−1) works on every object, none (0) on every object but
/// buildings. Every short is kept.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ChargeTarget(i16);

impl ChargeTarget {
    /// −1, every bit: every object.
    pub const EVERY: Self = Self(-1);
    /// 0, no bit: every object but buildings (type 80) other than classes 51 and 54.
    pub const ALL_BUT_BUILDINGS: Self = Self(0);

    /// The targets of the data's short `value`.
    #[must_use]
    pub const fn of_data(value: i16) -> Self {
        Self(value)
    }

    /// Its 16 bits.
    #[must_use]
    pub const fn bits(self) -> u16 {
        self.0.cast_unsigned()
    }

    /// Whether the bit of class group `group` (0 to 15) is set.
    #[must_use]
    pub const fn holds(self, group: u32) -> bool {
        self.bits() >> group & 1 != 0
    }

    /// The short, as the data and the save hold it.
    #[must_use]
    pub const fn get(self) -> i16 {
        self.0
    }
}

impl genie_diff::Diff for ChargeTarget {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::ChargeTarget;

    /// −1 holds every bit, 0 none, and a group's bit is its place.
    #[test]
    fn the_targets_hold_bit_per_group() {
        assert_eq!(ChargeTarget::EVERY.bits(), 0xffff);
        assert!((0..16).all(|g| ChargeTarget::EVERY.holds(g)));
        assert!(!(0..16).any(|g| ChargeTarget::ALL_BUT_BUILDINGS.holds(g)));
        let charge_target = ChargeTarget::of_data(0b1010_0000);
        assert!(charge_target.holds(5) && charge_target.holds(7) && !charge_target.holds(6));
        assert_eq!(charge_target.get(), 0xa0);
    }
}
