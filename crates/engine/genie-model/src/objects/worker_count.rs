//! The count of the actions that work on an object, and the caps on it.

/// A count of workers: the actions that have an object as their target (`worker_num`,
/// `+0x644`: a farm's farmer, a resource's gatherers, a building's builders), and a cap on
/// them (a drop site's target's `worker_num`, a search's argument, [`WorkerCount::NO_CAP`]
/// for none). Every value is kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WorkerCount(i32);

impl WorkerCount {
    /// No worker.
    pub const ZERO: Self = Self(0);
    /// One worker.
    pub const ONE: Self = Self(1);
    /// A cap of none (−1).
    pub const NO_CAP: Self = Self(-1);
    /// The most workers the game counts on an object (`0x226`): a let past it adds none.
    pub const MOST: Self = Self(550);

    /// The count of the save's or the data's int `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// The count `more` workers on (a worker that joins is 1, one already counted 0).
    #[must_use]
    pub const fn plus(self, more: i32) -> Self {
        Self(self.0 + more)
    }

    /// Whether this cap holds below `workers`: a cap of 0 or more under the count.
    #[must_use]
    pub const fn caps(self, workers: Self) -> bool {
        0 <= self.0 && self.0 < workers.0
    }

    /// The count as a float (`CVTSI2SS`).
    #[must_use]
    pub const fn as_f32(self) -> f32 {
        self.0 as f32
    }

    /// The int, as the save holds it.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

impl genie_diff::Diff for WorkerCount {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::WorkerCount;

    /// A cap of 0 or more holds below a count; none never does.
    #[test]
    fn cap_holds_below_the_count() {
        let two = WorkerCount::of_data(2);
        assert!(WorkerCount::ONE.caps(two));
        assert!(!two.caps(two));
        assert!(!WorkerCount::NO_CAP.caps(two));
        assert!(WorkerCount::ZERO.caps(WorkerCount::ONE));
        assert_eq!(WorkerCount::ONE.plus(1), two);
        assert_eq!(two.as_f32(), 2.0);
        assert!(WorkerCount::ZERO < WorkerCount::ONE);
    }
}
