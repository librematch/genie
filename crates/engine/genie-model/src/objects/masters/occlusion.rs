//! A master's occlusion mode (`+0x650`): how its objects show behind others.

use std::fmt::{Display, Error, Formatter};

/// A master's occlusion mode (`occlusion_flag`, `+0x650`; the `.dat`'s `occlusion_mode`): how the
/// renderer shows its objects behind others. The simulation only keeps it; every byte is kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Occlusion(u8);

impl Occlusion {
    /// 0: no outline behind others.
    pub const NONE: Self = Self(0);

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

impl Display for Occlusion {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        self.0.fmt(f)
    }
}

impl genie_diff::Diff for Occlusion {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::Occlusion;

    /// The data's values are the named ones, any other is kept.
    #[test]
    fn an_occlusion_mode_is_the_data_byte() {
        assert_eq!(Occlusion::of_data(255).plus(2).get(), 1);
        assert_eq!(Occlusion::of_data(0), Occlusion::NONE);
        assert_eq!(Occlusion::of_data(6).get(), 6);
        assert_eq!(Occlusion::NONE.to_string(), "0");
    }
}
