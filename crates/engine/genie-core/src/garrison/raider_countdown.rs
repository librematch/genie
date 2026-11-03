//! A combat object's raider countdown.

/// A combat object's `raider_build_countdown` (`+0x9a0`, an int, saved): the seconds it may stay
/// inside a building. A unit let out of its container has none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RaiderCountdown(i32);

impl RaiderCountdown {
    /// None: a unit outside, or one that never counted down.
    pub const NONE: Self = Self(0);

    /// The countdown of the save's int `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// The int.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }

    /// Whether it runs: above 0 (a building's update, `FUN_1411e7220`).
    #[must_use]
    pub const fn runs(self) -> bool {
        0 < self.0
    }

    /// Whether it is set: not 0 (a building's take-in, its sign aside).
    #[must_use]
    pub const fn is_set(self) -> bool {
        self.0 != 0
    }
}

impl genie_diff::Diff for RaiderCountdown {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::garrison::RaiderCountdown;

    /// It runs above 0 and is set at any value but 0.
    #[test]
    fn countdown_runs_above_zero_and_is_set_off_zero() {
        assert!(RaiderCountdown::of_data(20).runs());
        assert!(!RaiderCountdown::of_data(-1).runs() && RaiderCountdown::of_data(-1).is_set());
        assert!(!RaiderCountdown::NONE.runs() && !RaiderCountdown::NONE.is_set());
        assert_eq!(RaiderCountdown::default(), RaiderCountdown::NONE);
    }
}
