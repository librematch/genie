//! How an upgrade effect changes a master's objects ([`UpgradeMode`]).

use serde::Deserialize;
use serde::Serialize;

/// An upgrade effect's mode (its third argument, `FUN_141657d10`'s): 2 changes the effect's
/// own object alone, −1 and 3 keep the objects' masters, 3 also takes the objects whose own
/// master has the upgraded master's id; any other walks the objects and gives them masters
/// of their own.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct UpgradeMode(i16);

impl UpgradeMode {
    /// −1: the objects keep their masters.
    pub const KEEP: Self = Self(-1);
    /// 0: the objects' default walk.
    pub const WALK: Self = Self(0);
    /// 2: the effect's own object alone.
    pub const ONE_OBJECT: Self = Self(2);
    /// 3: the objects keep their masters, and those with an own master of the id too.
    pub const OWN_MASTERS: Self = Self(3);

    /// The mode of the effect's short `value`.
    #[must_use]
    pub const fn of_data(value: i16) -> Self {
        Self(value)
    }

    /// The short.
    #[must_use]
    pub const fn get(self) -> i16 {
        self.0
    }

    /// The objects keep their masters (−1 and 3).
    #[must_use]
    pub const fn keeps_masters(self) -> bool {
        matches!(self.0, -1 | 3)
    }

    /// The effect's own object alone (2).
    #[must_use]
    pub const fn one_object(self) -> bool {
        self.0 == 2
    }

    /// The objects whose own master has the id count too (3).
    #[must_use]
    pub const fn own_masters(self) -> bool {
        self.0 == 3
    }
}

#[cfg(test)]
mod tests {
    use crate::technology::UpgradeMode;

    /// −1 and 3 keep the masters, 2 is one object, 3 takes the own masters, 0 none of these.
    #[test]
    fn mode_names_its_walk() {
        let of = |upgrade_mode: UpgradeMode| {
            (
                upgrade_mode.keeps_masters(),
                upgrade_mode.one_object(),
                upgrade_mode.own_masters(),
            )
        };
        assert_eq!(of(UpgradeMode::KEEP), (true, false, false));
        assert_eq!(of(UpgradeMode::WALK), (false, false, false));
        assert_eq!(of(UpgradeMode::ONE_OBJECT), (false, true, false));
        assert_eq!(of(UpgradeMode::OWN_MASTERS), (true, false, true));
        assert_eq!(UpgradeMode::of_data(7).get(), 7);
    }
}
