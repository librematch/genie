//! A number of the history's updates: the period the `.dat` gives (`time_slice`) and a count
//! of runs (FD-046: the model's part of the history).

/// A number of the history's updates, one a game second: the period (the `.dat`'s
/// `time_slice`, `world+0x8b8`) and the runs since the last one (`update_history_count`,
/// `+0x28`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HistoryUpdates(i32);

impl HistoryUpdates {
    /// The number of the save's or the `.dat`'s int `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// One update more, wrapping as the game's int add.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    /// This number less `period`, wrapping as the game's int sub.
    #[must_use]
    pub const fn wrapping_sub(self, period: Self) -> Self {
        Self(self.0.wrapping_sub(period.0))
    }

    /// The int, as the save holds it.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

impl genie_diff::Diff for HistoryUpdates {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

/// The world constructor's history period (`FUN_141599e50`'s immediate 0x19 at `world+0x8b8`),
/// until the `.dat`'s `time_slice` replaces it.
pub const HISTORY_PERIOD: HistoryUpdates = HistoryUpdates::of_data(25);

#[cfg(test)]
mod tests {
    use crate::players::history::HistoryUpdates;

    /// Update counts wrap as the game's ints.
    #[test]
    fn update_counts_wrap() {
        assert_eq!(HistoryUpdates::of_data(i32::MAX).next().get(), i32::MIN);
        let period = HistoryUpdates::of_data(30);
        assert_eq!(HistoryUpdates::of_data(31).wrapping_sub(period).get(), 1);
        assert!(HistoryUpdates::of_data(29) < period);
    }
}
