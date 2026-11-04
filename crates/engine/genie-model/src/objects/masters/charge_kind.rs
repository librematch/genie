//! A master's charge type (`+0x922`): what its charge does.

use derive_more::Display;
use serde::Deserialize;
use serde::Serialize;

/// A master's charge type (`charge_type`, `+0x922`): what its charge (`+0x9e0`) does. Every short
/// the data or an effect leaves is kept; the values the game tests are named.
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
pub struct ChargeKind(i16);

impl ChargeKind {
    /// 0: no charge.
    pub const NONE: Self = Self(0);
    /// 1: a full charge adds to the attack.
    pub const ATTACK: Self = Self(1);
    /// 3: a full charge adds to the attack, and an area attack tramples only where it works.
    pub const AREA: Self = Self(3);
    /// 4: a charge of 1 blocks a missile's hit (`FUN_14108d8a0`).
    pub const MISSILE_SHIELD: Self = Self(4);
    /// 5: a charge of 1 blocks a melee hit (`FUN_14108d8a0`).
    pub const MELEE_SHIELD: Self = Self(5);
    /// −4: the charge works on every object (`FUN_1412623d0`).
    pub const EVERY_TARGET: Self = Self(-4);

    /// The charge type of the data's short `value`.
    #[must_use]
    pub const fn of_data(value: i16) -> Self {
        Self(value)
    }

    /// Types 1 and 3: a full charge adds to the attack (`FUN_141203e30`, `FUN_14120e920`).
    #[must_use]
    pub const fn adds_to_attack(self) -> bool {
        matches!(self.0, 1 | 3)
    }

    /// Types 6 and 7: a ready charge swaps in the projectile's weapons and adds the charge event
    /// to the range (slot 0x2f8).
    #[must_use]
    pub const fn ranged(self) -> bool {
        matches!(self.0, 6 | 7)
    }

    /// A type above 0: the master has a charge an upgrade carries over.
    #[must_use]
    pub const fn charges(self) -> bool {
        0 < self.0
    }

    /// The short, as the data and the save hold it.
    #[must_use]
    pub const fn get(self) -> i16 {
        self.0
    }
}

impl genie_diff::Diff for ChargeKind {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::ChargeKind;

    /// The attack adders are 1 and 3, the ranged 6 and 7, the charging ones those above 0.
    #[test]
    fn charge_kind_groups() {
        let of = |test: fn(ChargeKind) -> bool| -> Vec<i16> {
            (-5..9)
                .map(ChargeKind::of_data)
                .filter(|&t| test(t))
                .map(ChargeKind::get)
                .collect()
        };
        assert_eq!(of(ChargeKind::adds_to_attack), [1, 3]);
        assert_eq!(of(ChargeKind::ranged), [6, 7]);
        assert_eq!(of(ChargeKind::charges), [1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(ChargeKind::of_data(-4), ChargeKind::EVERY_TARGET);
        assert_eq!(ChargeKind::AREA.to_string(), "3");
    }
}
