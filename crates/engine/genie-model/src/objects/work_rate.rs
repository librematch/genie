//! A worker's work rate ([`WorkRate`]), with the game's `f32` operations.

use crate::objects::masters::WorkValue;
use crate::world::Seconds;
use derive_more::{Add, AddAssign, Display};
use serde::Deserialize;
use serde::Serialize;
use std::ops::{Mul, MulAssign};

/// The work an object does per second, as the game's `f32` holds it: a master's `work_rate`
/// (`+0x798`) and an object's powerup of it (`mWorkRatePowerup`, `+0xa30`, slot 0x6e8), their
/// sum as a gatherer, a hunter, a builder, a repairer, a farm or a producing building works.
/// A task's work value scales it ([`WorkRate`] times an `f32`), and over [`Seconds`] it is the
/// work done, in the task's own unit (an amount, build or repair points, training progress).
/// Each operator is the one `f32` operation the game does, in its order; a comparison is
/// `f32`'s (a NaN compares false).
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    PartialOrd,
    Serialize,
    Deserialize,
    Display,
    Add,
    AddAssign,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct WorkRate(f32);

impl WorkRate {
    /// No work.
    pub const ZERO: Self = Self(0.0);

    /// `per_second` of work a second.
    #[must_use]
    pub const fn new(per_second: f32) -> Self {
        Self(per_second)
    }

    /// The work a second, as the save and the game's arithmetic hold it.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl Mul<f32> for WorkRate {
    type Output = Self;

    /// The rate scaled by `f` (one `f32` product, the rate first: a task's work value).
    fn mul(self, f: f32) -> Self {
        Self(self.0 * f)
    }
}

impl MulAssign<f32> for WorkRate {
    /// The rate scaled by `f` in place (an effect's multiplier).
    fn mul_assign(&mut self, f: f32) {
        self.0 *= f;
    }
}

impl Mul<Seconds> for WorkRate {
    type Output = f32;

    /// The work done in `dt` (one `f32` product, the rate first), in the task's unit.
    fn mul(self, dt: Seconds) -> f32 {
        self.0 * dt.get()
    }
}

impl Mul<WorkValue> for WorkRate {
    type Output = Self;

    /// The rate a task's work value scales (one `f32` product, the rate first).
    fn mul(self, value: WorkValue) -> Self {
        Self(self.0 * value.get())
    }
}

impl genie_diff::Diff for WorkRate {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::WorkRate;
    use crate::objects::masters::WorkValue;
    use crate::world::Seconds;

    /// Each operator is the one `f32` operation, bit for bit and in its operand order.
    #[test]
    fn work_rates_do_the_game_f32_operations() {
        let (a, b) = (WorkRate::new(0.1), WorkRate::new(0.2));
        assert_eq!((a + b).get().to_bits(), (0.1_f32 + 0.2).to_bits());
        let mut c = a;
        c += b;
        assert_eq!(c.get().to_bits(), (0.1_f32 + 0.2).to_bits());
        assert_eq!((a * 3.0).get().to_bits(), (0.1_f32 * 3.0).to_bits());
        let mut d = a;
        d *= 3.0;
        assert_eq!(d, a * 3.0);
        assert_eq!(
            ((a * 3.0) * Seconds::new(0.7)).to_bits(),
            ((0.1_f32 * 3.0) * 0.7).to_bits()
        );
        assert_eq!(
            (a * WorkValue::new(0.7)).get().to_bits(),
            (0.1_f32 * 0.7).to_bits()
        );
        assert!(WorkRate::ZERO < a && a <= a);
        assert_eq!(WorkRate::new(f32::NAN).partial_cmp(&a), None);
        assert_eq!(WorkRate::new(2.5).to_string(), "2.5");
    }
}
