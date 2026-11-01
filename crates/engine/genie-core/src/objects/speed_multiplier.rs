//! A sprite's speed factor.

use crate::movement::Speed;

/// A `.dat` graphic's `speed_multiplier` (`+0xc`): it scales the sprite's frame duration and,
/// above 0, a moving object's top speed.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct SpeedMultiplier(f32);

impl SpeedMultiplier {
    /// 0: the top speed is the master's.
    pub const ZERO: Self = Self(0.0);
    /// 1: the sprite runs at its frame duration.
    pub const ONE: Self = Self(1.0);

    /// The factor of the `.dat`'s float `value`.
    #[must_use]
    pub const fn of_data(value: f32) -> Self {
        Self(value)
    }

    /// The float.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }

    /// A master's speed `base` with the sprite (slot 65, `FUN_14114d1d0`): times the factor
    /// when that is above 0 (`COMISS`, `JBE`: a NaN is not), else `base`.
    #[must_use]
    pub fn applied_to(self, base: Speed) -> Speed {
        if 0.0 < self.0 {
            Speed::new(base.get() * self.0)
        } else {
            base
        }
    }
}

impl genie_diff::Diff for SpeedMultiplier {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::movement::Speed;
    use crate::objects::SpeedMultiplier;

    /// Only a factor above 0 scales the speed; 0, a negative factor and NaN leave it.
    #[test]
    fn only_a_factor_above_zero_scales_the_speed() {
        let base = Speed::new(0.8);
        assert_eq!(
            SpeedMultiplier::of_data(1.5).applied_to(base),
            Speed::new(0.8 * 1.5)
        );
        for f in [0.0, -1.0, f32::NAN] {
            assert_eq!(SpeedMultiplier::of_data(f).applied_to(base), base);
        }
        assert_eq!(SpeedMultiplier::ONE.get(), 1.0);
    }
}
