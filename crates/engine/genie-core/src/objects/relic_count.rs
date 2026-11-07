//! A count of relics.

/// A count of relics (an int): a building's `relic_count` (`+0xc20`) and
/// `specific_relic_count` (`+0xc24`), the world's relics an effect spawned (`+0x844`). It
/// compares signed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RelicCount(i32);

impl RelicCount {
    /// None.
    pub const ZERO: Self = Self(0);

    /// The count of the save's int `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// The int.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }

    /// One relic more.
    #[must_use]
    pub const fn plus_one(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    /// One relic less (`DEC`: below 0 too).
    #[must_use]
    pub const fn minus_one(self) -> Self {
        Self(self.0.wrapping_sub(1))
    }
}

impl genie_diff::Diff for RelicCount {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::RelicCount;

    /// A count grows by one, wrapping as an int.
    #[test]
    fn count_grows_by_one() {
        assert_eq!(RelicCount::ZERO.plus_one().get(), 1);
        assert_eq!(RelicCount::of_data(i32::MAX).plus_one().get(), i32::MIN);
    }

    /// A count shrinks by one, below 0 and wrapping as an int.
    #[test]
    fn count_shrinks_by_one() {
        assert_eq!(RelicCount::ZERO.minus_one().get(), -1);
        assert_eq!(RelicCount::of_data(i32::MIN).minus_one().get(), i32::MAX);
    }
}
