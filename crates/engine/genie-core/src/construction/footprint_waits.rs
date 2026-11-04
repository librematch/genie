//! A builder's waits for an occupied footprint.

/// A builder's waits for an occupied footprint (`+0x889`, a byte; not saved, 0 at a load): at
/// [`FootprintWaits::MOST`] the Build halts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FootprintWaits(u8);

impl FootprintWaits {
    /// None.
    pub const NONE: Self = Self(0);
    /// The waits a builder makes before it gives up (`0x15`).
    pub const MOST: Self = Self(21);

    /// The count `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// The byte.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// One wait more (wrapping as a byte).
    #[must_use]
    pub const fn plus_one(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

impl genie_diff::Diff for FootprintWaits {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

#[cfg(test)]
mod tests {
    use crate::construction::FootprintWaits;

    /// The waits count up to the most.
    #[test]
    fn the_waits_count_up_to_the_most() {
        assert_eq!(FootprintWaits::NONE.plus_one().get(), 1);
        assert!(FootprintWaits::of_data(20) < FootprintWaits::MOST);
    }
}
