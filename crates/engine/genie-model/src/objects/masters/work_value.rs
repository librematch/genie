//! A task's work values.

use serde::Deserialize;
use serde::Serialize;

/// A task's work value (`work_val1`, `+0x28`, and `work_val2`, `+0x2c`, floats of the `.dat`):
/// what its action reads by its type. A gather's and a build's `work_val1` scales the worker's
/// work rate, a generation's the attribute it grows, a movement damage's the hit points it takes
/// and its `work_val2` the seconds between; a task whose `work_val1` is at most 0 is no target's
/// (the task finder). Every value is kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct WorkValue(f32);

impl WorkValue {
    /// None.
    pub const ZERO: Self = Self(0.0);
    /// One: a task the slice makes for a work without the data's.
    pub const ONE: Self = Self(1.0);

    /// The value of the data's float `value`.
    #[must_use]
    pub const fn new(value: f32) -> Self {
        Self(value)
    }

    /// Whether it is above 0 (`COMISS`: a NaN is not).
    #[must_use]
    pub const fn positive(self) -> bool {
        0.0 < self.0
    }

    /// The float, as the data and the save hold it.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl genie_diff::Diff for WorkValue {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::WorkValue;

    /// Only a value above 0 is positive; a NaN is not.
    #[test]
    fn only_value_above_zero_is_positive() {
        assert!(WorkValue::ONE.positive());
        assert!(!WorkValue::ZERO.positive());
        assert!(!WorkValue::new(-1.0).positive());
        assert!(!WorkValue::new(f32::NAN).positive());
        assert_eq!(WorkValue::new(0.5).get(), 0.5);
    }
}
