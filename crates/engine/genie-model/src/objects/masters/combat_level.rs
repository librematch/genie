//! A master's combat level (`+0x62c`): which of the unit AI's kinds of fighter it is.

use serde::Deserialize;
use serde::Serialize;

/// A master's combat level (`+0x62c`, the `.dat`'s `combat_level`), every byte the data or an
/// effect leaves kept. Named by the masters of build 158041's `.dat` that carry each value.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CombatLevel(u8);

impl CombatLevel {
    /// 0: none, a master the search for a combat level passes over.
    pub const NONE: Self = Self(0);
    /// 1: an animal that fights back (boars, elephants, the rhino); a predator of it is a boar
    /// to the unit AI.
    pub const ANIMAL: Self = Self(1);
    /// 2: a building.
    pub const BUILDING: Self = Self(2);
    /// 3: a civilian (villagers, fishing ships, trade units).
    pub const CIVILIAN: Self = Self(3);
    /// 4: a military unit; a predator of it is the one a low-priority attacker takes on.
    pub const MILITARY: Self = Self(4);
    /// 5: a monk.
    pub const MONK: Self = Self(5);

    /// The level of the data's byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// Whether it has a level (not 0).
    #[must_use]
    pub const fn is_some(self) -> bool {
        self.0 != Self::NONE.0
    }

    /// The byte plus `amount` (an effect's add, wrapping as the game's byte add).
    #[must_use]
    pub const fn plus(self, amount: u8) -> Self {
        Self(self.0.wrapping_add(amount))
    }

    /// The byte, as the data and the save hold it.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl genie_diff::Diff for CombatLevel {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::CombatLevel;

    /// Every byte is kept, an add wraps, and only 0 is no level.
    #[test]
    fn combat_level_keeps_its_byte() {
        assert!(!CombatLevel::NONE.is_some());
        assert!(CombatLevel::ANIMAL.is_some());
        assert_eq!(CombatLevel::of_data(255).plus(2), CombatLevel::ANIMAL);
        assert_eq!(CombatLevel::default(), CombatLevel::NONE);
        assert_eq!(CombatLevel::MILITARY.get(), 4);
    }
}
