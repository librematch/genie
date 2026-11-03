//! An effect's id (a technology's `+0x3a`, a player's effect override `+0x62`).

use serde::Deserialize;
use serde::Serialize;
use std::fmt::{Display, Error, Formatter};

/// An effect's id: its place in the world's effect table (`+0x128`), as a technology (`+0x3a`) and
/// a player's effect override (`+0x62`) hold it, −1 for none. Every short is kept.
/// The default is 0, as a zeroed record holds it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct EffectId(i16);

impl EffectId {
    /// −1: no effect.
    pub const NONE: Self = Self(-1);

    /// The value of the data's short `value`.
    #[must_use]
    pub const fn of_data(value: i16) -> Self {
        Self(value)
    }

    /// The short, as the data and the save hold it.
    #[must_use]
    pub const fn get(self) -> i16 {
        self.0
    }
}

impl Display for EffectId {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        self.0.fmt(f)
    }
}

impl genie_diff::Diff for EffectId {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::technology::EffectId;

    /// The data's values are the named ones, any other is kept.
    #[test]
    fn an_effect_id_keeps_the_data_short() {
        assert_eq!(EffectId::of_data(-1), EffectId::NONE);
        assert_eq!(EffectId::of_data(42).get(), 42);
        assert_eq!(EffectId::NONE.to_string(), "-1");
    }
}
