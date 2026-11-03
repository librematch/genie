//! A master's interface kind (`+0x62f`): which command panel its objects show.

use serde::Deserialize;
use serde::Serialize;
use std::fmt::{Display, Error, Formatter};

/// A master's interface kind (`unit_level`, `+0x62f`; the `.dat`'s `interface_kind`, openage's
/// names): the command panel a selected object of it shows. The simulation only keeps it; every
/// byte is kept.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct InterfaceKind(u8);

impl InterfaceKind {
    /// 0: delete, garrison and stop only.
    pub const LIVING: Self = Self(0);
    /// 1: an animal.
    pub const ANIMAL: Self = Self(1);
    /// 2: a civilian building (the first build page).
    pub const CIVILIAN_BUILDING: Self = Self(2);
    /// 3: a villager.
    pub const VILLAGER: Self = Self(3);
    /// 4: a military unit.
    pub const MILITARY: Self = Self(4);

    /// The value of the data's byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// The byte plus `amount` (an effect's add, wrapping as the game's add).
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

impl Display for InterfaceKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        self.0.fmt(f)
    }
}

impl genie_diff::Diff for InterfaceKind {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::InterfaceKind;

    /// The data's values are the named ones, any other is kept.
    #[test]
    fn an_interface_kind_is_the_data_byte() {
        assert_eq!(InterfaceKind::of_data(255).plus(2).get(), 1);
        assert_eq!(InterfaceKind::of_data(0), InterfaceKind::LIVING);
        assert_eq!(InterfaceKind::of_data(12).get(), 12);
        assert_eq!(InterfaceKind::LIVING.to_string(), "0");
    }
}
