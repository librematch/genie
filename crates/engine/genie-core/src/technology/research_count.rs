//! A technology's research counts and their cap.

/// How often a technology was researched or is queued (`+0xc`, `+0x58`, shorts).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResearchCount(i16);

impl ResearchCount {
    /// None.
    pub const ZERO: Self = Self(0);

    /// The count of the save's short `value`.
    #[must_use]
    pub const fn of_data(value: i16) -> Self {
        Self(value)
    }

    /// The short.
    #[must_use]
    pub const fn get(self) -> i16 {
        self.0
    }

    /// The count plus `n` queued, wrapping as the game's short.
    #[must_use]
    pub const fn plus(self, n: i16) -> Self {
        Self(self.0.wrapping_add(n))
    }

    /// The count less `n` done or cancelled, wrapping as the game's short.
    #[must_use]
    pub const fn minus(self, n: i16) -> Self {
        Self(self.0.wrapping_sub(n))
    }
}

impl genie_diff::Diff for ResearchCount {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

/// The cap of a technology's researches queued and done (`+0x56`, a short): below 0 (as an
/// unsigned short above 0x7fff) it is 0x100.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResearchCap(i16);

impl ResearchCap {
    /// None set (−1): the cap of 0x100.
    pub const UNSET: Self = Self(-1);
    /// The cap a negative one stands for.
    const UNSET_LIMIT: i16 = 0x100;

    /// The cap of the save's short `value`.
    #[must_use]
    pub const fn of_data(value: i16) -> Self {
        Self(value)
    }

    /// The short.
    #[must_use]
    pub const fn get(self) -> i16 {
        self.0
    }

    /// The number of researches it allows (`FUN_1413d1050`: the short as unsigned above 0x7fff
    /// allows 0x100).
    #[must_use]
    pub const fn limit(self) -> i16 {
        if self.0 < 0 {
            Self::UNSET_LIMIT
        } else {
            self.0
        }
    }
}

impl genie_diff::Diff for ResearchCap {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

#[cfg(test)]
mod tests {
    use crate::technology::{ResearchCap, ResearchCount};

    /// A negative cap allows 0x100, any other its own count.
    #[test]
    fn negative_cap_allows_0x100() {
        assert_eq!(ResearchCap::UNSET.limit(), 0x100);
        assert_eq!(ResearchCap::of_data(i16::MIN).limit(), 0x100);
        assert_eq!(ResearchCap::of_data(3).limit(), 3);
        assert_eq!(ResearchCap::of_data(0).limit(), 0);
        assert_eq!(ResearchCount::default(), ResearchCount::ZERO);
        assert_eq!(ResearchCount::of_data(i16::MAX).plus(1).get(), i16::MIN);
        assert_eq!(ResearchCount::ZERO.minus(1).get(), -1);
    }
}
