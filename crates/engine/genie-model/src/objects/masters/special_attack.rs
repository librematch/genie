//! A master's special attack (`+0x8d8`): what a hit on its objects or their charge does.

use serde::Deserialize;
use serde::Serialize;
use std::fmt::{Display, Error, Formatter};

/// A master's special attack (`special_attack_flag`, `+0x8d8`), as a hit on its object reads it
/// (`FUN_1412108e0`). Every byte is kept.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SpecialAttack(u8);

impl SpecialAttack {
    /// 0: none.
    pub const NONE: Self = Self(0);
    /// 1: an idle object facing its attacker takes the damage to 0.66.
    pub const FRONT_DEFENCE: Self = Self(1);
    /// 2: an idle object with a unit AI facing a melee attacker strikes first.
    pub const RETALIATION: Self = Self(2);
    /// 3: a charge attack on its target in reach (`FUN_1412161b0`).
    pub const CHARGE: Self = Self(3);

    /// The value of the data's byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// The byte, as the data and the save hold it.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl Display for SpecialAttack {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        self.0.fmt(f)
    }
}

impl genie_diff::Diff for SpecialAttack {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::SpecialAttack;

    /// The data's values are the named ones, any other is kept.
    #[test]
    fn special_attack_is_the_data_byte() {
        assert_eq!(SpecialAttack::of_data(0), SpecialAttack::NONE);
        assert_eq!(SpecialAttack::of_data(7).get(), 7);
        assert_eq!(SpecialAttack::NONE.to_string(), "0");
    }
}
