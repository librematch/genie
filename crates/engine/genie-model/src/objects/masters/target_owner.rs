//! A task's owner type (`+0x18`): whose targets the task is for.

use serde::Deserialize;
use serde::Serialize;
use std::fmt::{Display, Error, Formatter};

/// A task's owner type (`+0x18`, `owner_type`; the `.dat`'s `target_diplomacy`): whose targets
/// it is for, as the task's test on a target reads it (`FUN_1411a8620`). Every byte is kept;
/// one not named here is for anyone's.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TargetOwner(u8);

impl TargetOwner {
    /// 0: anyone's (as every value not named here).
    pub const ANY: Self = Self(0);
    /// 1: the unit's own player's.
    pub const OWN: Self = Self(1);
    /// 2: an enemy player's: not its own, not Gaia's, not one it is allied to.
    pub const ENEMY: Self = Self(2);
    /// 3: Gaia's.
    pub const GAIA: Self = Self(3);
    /// 4: its own, a mutually allied player's or Gaia's, the unit itself included.
    pub const FRIENDLY: Self = Self(4);
    /// 5: an enemy's, or Gaia's for a unit whose master has a combat level.
    pub const ENEMY_OR_GAIA: Self = Self(5);
    /// 6: another player's.
    pub const OTHER: Self = Self(6);

    /// The owner type of the data's byte `value`.
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

impl Display for TargetOwner {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{}", self.0)
    }
}

impl genie_diff::Diff for TargetOwner {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::TargetOwner;

    /// The data's bytes are the named types, anything else is kept as it is.
    #[test]
    fn an_owner_kind_is_the_data_byte() {
        assert_eq!(TargetOwner::of_data(2), TargetOwner::ENEMY);
        assert_eq!(TargetOwner::default(), TargetOwner::ANY);
        assert_eq!(TargetOwner::of_data(9).get(), 9);
        assert_eq!(TargetOwner::OTHER.to_string(), "6");
    }
}
