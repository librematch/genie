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
}

impl genie_diff::Diff for RelicCount {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::RelicCount;

    /// A count grows by one, wrapping as an int.
    #[test]
    fn a_count_grows_by_one() {
        assert_eq!(RelicCount::ZERO.plus_one().get(), 1);
        assert_eq!(RelicCount::of_data(i32::MAX).plus_one().get(), i32::MIN);
    }
}
