//! A master's selection effect (`+0x684`): how a selected object of it shows.

use serde::Deserialize;
use serde::Serialize;
use std::fmt::{Display, Error, Formatter};

/// A master's selection effect (`+0x684`, the `.dat`'s `selection_effect`; openage's names): its
/// hit point bar and outline when selected. The simulation only keeps it; every byte is kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SelectionEffect(u8);

impl SelectionEffect {
    /// 0: none.
    pub const NONE: Self = Self(0);

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

impl Display for SelectionEffect {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        self.0.fmt(f)
    }
}

impl genie_diff::Diff for SelectionEffect {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::SelectionEffect;

    /// The data's values are the named ones, any other is kept.
    #[test]
    fn selection_effect_is_the_data_byte() {
        assert_eq!(SelectionEffect::of_data(0), SelectionEffect::NONE);
        assert_eq!(SelectionEffect::of_data(9).get(), 9);
        assert_eq!(SelectionEffect::NONE.to_string(), "0");
    }
}
