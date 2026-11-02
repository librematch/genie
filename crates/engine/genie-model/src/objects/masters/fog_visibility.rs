//! A master's fog visibility (`+0x605`): whether its objects show in the fog of war.

use std::fmt::{Display, Error, Formatter};

/// A master's fog visibility (`+0x605`, the `.dat`'s `fog_visibility`; openage's names): an Attack
/// goes on against an object of any but [`FogVisibility::INVISIBLE`] in the fog. Every byte is
/// kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct FogVisibility(u8);

impl FogVisibility {
    /// 0: not shown in the fog (units).
    pub const INVISIBLE: Self = Self(0);
    /// 1: shown in the fog (buildings).
    pub const VISIBLE: Self = Self(1);
    /// 2: shown in the fog while alive.
    pub const VISIBLE_IF_ALIVE: Self = Self(2);
    /// 3: shown only in the fog; the placement test reads it where the tile is seen.
    pub const ONLY_IN_FOG: Self = Self(3);
    /// 4: its ghost shows in the fog.
    pub const DOPPELGANGER: Self = Self(4);

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

impl Display for FogVisibility {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        self.0.fmt(f)
    }
}

impl genie_diff::Diff for FogVisibility {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::FogVisibility;

    /// The data's values are the named ones, any other is kept.
    #[test]
    fn fog_visibility_is_the_data_byte() {
        assert_eq!(FogVisibility::of_data(0), FogVisibility::INVISIBLE);
        assert_eq!(FogVisibility::of_data(9).get(), 9);
        assert_eq!(FogVisibility::INVISIBLE.to_string(), "0");
    }
}
