//! A master's hit chance (`base_hit_chance`): the percent of its missiles and strikes that hit.

use serde::Deserialize;
use serde::Serialize;

/// A master's hit chance (`base_hit_chance`, the `.dat`'s accuracy percent): the percent of its
/// shots that hit, which a missile's aim (`FUN_140a9c600`'s draw) and a strike's Object-stream
/// draw test. A short the effects set, add to and scale; every value is kept.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct HitChance(i16);

impl HitChance {
    /// The chance of the data's short `value`.
    #[must_use]
    pub const fn of_data(value: i16) -> Self {
        Self(value)
    }

    /// The short plus `amount` (an effect's add, wrapping as the game's 16-bit add).
    #[must_use]
    pub const fn plus(self, amount: i16) -> Self {
        Self(self.0.wrapping_add(amount))
    }

    /// The percent, as the data and the save hold it.
    #[must_use]
    pub const fn get(self) -> i16 {
        self.0
    }
}

impl genie_diff::Diff for HitChance {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::HitChance;

    /// The percent is kept, an add wraps.
    #[test]
    fn hit_chance_is_percent() {
        assert_eq!(HitChance::of_data(70).get(), 70);
        assert_eq!(HitChance::of_data(i16::MAX).plus(1).get(), i16::MIN);
        assert!(HitChance::of_data(30) < HitChance::of_data(70));
    }
}
