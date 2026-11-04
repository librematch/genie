//! An update's number.

use derive_more::Display;

/// The updates the world ran (its `ticks`; the tick oracle's trace rows count them from 0): what
/// a freeze, a stop or a report records as its update.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Display)]
pub struct Tick(u32);

impl Tick {
    /// No update run yet.
    pub const ZERO: Self = Self(0);

    /// The count `value`.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// The count.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// The count after one more update.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

impl genie_diff::Diff for Tick {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

#[cfg(test)]
mod tests {
    use crate::world::Tick;

    /// The count steps by one and prints as its number.
    #[test]
    fn the_count_steps_by_one() {
        assert_eq!(Tick::ZERO.next(), Tick::new(1));
        assert_eq!(Tick::new(42).to_string(), "42");
    }
}
