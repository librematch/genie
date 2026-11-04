//! A regeneration rate: the hit points an object gains a minute.

use derive_more::{Add, AddAssign};
use serde::Deserialize;
use serde::Serialize;
use std::ops::MulAssign;

/// A regeneration rate in hit points a minute (a float): a master's (`regeneration_rate`,
/// `+0x8b4`, which the effects set, add to and scale), an object's powerup (`+0xa0c`) and
/// stinger, and a hero's bonus. A negative rate takes hit points. Each operator is the one
/// `f32` operation the game does; every value is kept.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, PartialOrd, Serialize, Deserialize, Add, AddAssign,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Regeneration(f32);

impl Regeneration {
    /// No regeneration.
    pub const ZERO: Self = Self(0.0);

    /// The rate of the data's or the save's float `value`.
    #[must_use]
    pub const fn new(value: f32) -> Self {
        Self(value)
    }

    /// The float, as the data and the save hold it.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl MulAssign<f32> for Regeneration {
    /// An effect's multiply (`f32`).
    fn mul_assign(&mut self, factor: f32) {
        self.0 *= factor;
    }
}

impl genie_diff::Diff for Regeneration {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::Regeneration;

    /// A rate adds and scales as `f32`.
    #[test]
    fn regeneration_rate_adds_and_scales_as_floats() {
        let mut regeneration = Regeneration::new(30.0) + Regeneration::new(5.0);
        assert_eq!(regeneration, Regeneration::new(35.0));
        regeneration += Regeneration::new(-5.0);
        regeneration *= 0.5;
        assert_eq!(regeneration.get(), 15.0);
        assert!(Regeneration::ZERO < regeneration);
    }
}
