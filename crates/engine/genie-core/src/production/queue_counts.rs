//! A production queue's counts: the units an entry makes, the entries, the serials and the
//! changes.

/// How many units a queue entry makes (an entry's `+0x2`, `mUnitCount`) and their sum over the
/// queue (`+0xbf4`, `mProductionQueueTotalUnits`): shorts, the game's 16-bit arithmetic.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QueuedUnits(i16);

impl QueuedUnits {
    /// None.
    pub const ZERO: Self = Self(0);
    /// One unit.
    pub const ONE: Self = Self(1);

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

    /// The count plus `n`, wrapping as the game's short.
    #[must_use]
    pub const fn plus(self, n: Self) -> Self {
        Self(self.0.wrapping_add(n.0))
    }

    /// The count less `n`, wrapping as the game's short.
    #[must_use]
    pub const fn minus(self, n: Self) -> Self {
        Self(self.0.wrapping_sub(n.0))
    }

    /// The count negated (a cancel's change of the queued units), wrapping as the game's short.
    #[must_use]
    pub const fn negated(self) -> Self {
        Self(self.0.wrapping_neg())
    }
}

impl std::iter::Sum for QueuedUnits {
    /// The 16-bit sum.
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::ZERO, Self::plus)
    }
}

impl genie_diff::Diff for QueuedUnits {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

/// How many of a queue's slots are entries (`+0xbf2`, `mProductionQueueCount`, a short).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QueueLength(i16);

impl QueueLength {
    /// No entries.
    pub const ZERO: Self = Self(0);

    /// The length of the save's short `value`.
    #[must_use]
    pub const fn of_data(value: i16) -> Self {
        Self(value)
    }

    /// The short.
    #[must_use]
    pub const fn get(self) -> i16 {
        self.0
    }

    /// The entries as a slot count, none below 0.
    #[must_use]
    pub fn slots(self) -> usize {
        usize::try_from(self.0).unwrap_or(0)
    }

    /// One entry more.
    #[must_use]
    pub const fn grown(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    /// One entry less.
    #[must_use]
    pub const fn shrunk(self) -> Self {
        Self(self.0.wrapping_sub(1))
    }
}

impl genie_diff::Diff for QueueLength {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

/// The serial a queue entry takes (the building's `+0xcd8`, an entry's `+0x20`): one more for
/// each entry the building opens. Not saved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntrySerial(i64);

impl EntrySerial {
    /// The first.
    pub const FIRST: Self = Self(0);

    /// The serial `value` (a load numbers its slots from 0).
    #[must_use]
    pub const fn new(value: i64) -> Self {
        Self(value)
    }

    /// The number.
    #[must_use]
    pub const fn get(self) -> i64 {
        self.0
    }

    /// The serial after it.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

impl genie_diff::Diff for EntrySerial {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

/// The changes of a queue's entries (`+0xc00`, a byte that grows by one with each, wrapping).
/// Not saved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QueueChanges(u8);

impl QueueChanges {
    /// None.
    pub const NONE: Self = Self(0);

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

    /// One change more (wrapping).
    #[must_use]
    pub const fn bumped(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

impl genie_diff::Diff for QueueChanges {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::production::{EntrySerial, QueueChanges, QueueLength, QueuedUnits};

    /// The counts wrap as shorts, the changes as a byte; the sum is the 16-bit one.
    #[test]
    fn the_queue_counts_wrap_as_the_game_s_integers() {
        let max = QueuedUnits::of_data(i16::MAX);
        assert_eq!(max.plus(QueuedUnits::ONE).get(), i16::MIN);
        assert_eq!(QueuedUnits::ZERO.minus(QueuedUnits::ONE).get(), -1);
        assert_eq!(QueuedUnits::ONE.negated().get(), -1);
        assert_eq!(
            [max, QueuedUnits::ONE]
                .into_iter()
                .sum::<QueuedUnits>()
                .get(),
            i16::MIN
        );
        assert_eq!(QueueChanges::of_data(255).bumped(), QueueChanges::NONE);
        assert_eq!(EntrySerial::FIRST.next().get(), 1);
        assert_eq!(QueueLength::ZERO.shrunk().slots(), 0);
        assert_eq!(QueueLength::ZERO.grown().grown().slots(), 2);
    }
}
