//! A cost's whole amount.

use serde::Deserialize;
use serde::Serialize;

/// A cost's whole amount (a short): a master's cost slot (`+0x880`), a technology's `.dat`
/// cost, a research's cost adjustment (`+0x6`) and a cost override's amount (`+0x42`). The
/// arithmetic on it is the game's 16-bit one.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CostAmount(i16);

impl CostAmount {
    /// None.
    pub const ZERO: Self = Self(0);

    /// The amount of the data's short `value`.
    #[must_use]
    pub const fn of_data(value: i16) -> Self {
        Self(value)
    }

    /// The short.
    #[must_use]
    pub const fn get(self) -> i16 {
        self.0
    }

    /// The amount as a float (`CVTSI2SS`, exact for a short).
    #[must_use]
    pub fn as_f32(self) -> f32 {
        f32::from(self.0)
    }

    /// The amount plus `by`, wrapping as the game's short (an effect's addition, an
    /// adjustment).
    #[must_use]
    pub const fn plus(self, by: i16) -> Self {
        Self(self.0.wrapping_add(by))
    }
}

impl genie_diff::Diff for CostAmount {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::CostAmount;

    /// The float is the short's.
    #[test]
    fn the_float_is_the_short_s() {
        assert_eq!(
            CostAmount::of_data(-50).as_f32().to_bits(),
            (-50.0f32).to_bits()
        );
        assert_eq!(CostAmount::default(), CostAmount::ZERO);
        assert_eq!(CostAmount::of_data(i16::MAX).plus(1).get(), i16::MIN);
    }
}
