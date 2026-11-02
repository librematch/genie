//! The count of farms or fish traps a queue paid for.

use crate::production::CostCount;

/// The farms (`farm_queue_count`, `+0x141c`) or fish traps (`fishTrapQueueCount`, `+0x1420`) a
/// player's queue paid for (saved), and the count a GAME command buys or takes back.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QueueCount(i32);

impl QueueCount {
    /// None queued.
    pub const ZERO: Self = Self(0);
    /// One: the least a command buys or takes back, and the count a reseed takes.
    pub const ONE: Self = Self(1);

    /// The count of the save's or the command's int `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// Whether it holds at least one.
    #[must_use]
    pub const fn any(self) -> bool {
        0 < self.0
    }

    /// The count as the cost test's and the payment's multiplier.
    #[must_use]
    pub const fn factor(self) -> CostCount {
        CostCount::new(self.0 as f32)
    }

    /// The int, as the save holds it.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

impl std::ops::Add for QueueCount {
    type Output = Self;

    /// The int sum.
    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl std::ops::AddAssign for QueueCount {
    fn add_assign(&mut self, other: Self) {
        self.0 += other.0;
    }
}

impl std::ops::Sub for QueueCount {
    type Output = Self;

    /// The int difference.
    fn sub(self, other: Self) -> Self {
        Self(self.0 - other.0)
    }
}

impl std::ops::SubAssign for QueueCount {
    fn sub_assign(&mut self, other: Self) {
        self.0 -= other.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A count adds, subtracts and holds at least one from 1 on.
    #[test]
    fn count_adds_subtracts_and_knows_when_it_holds_any() {
        let mut c = QueueCount::of_data(3);
        c += QueueCount::ONE;
        assert_eq!(c, QueueCount::of_data(4));
        c -= QueueCount::of_data(4);
        assert!(!c.any() && !QueueCount::of_data(-1).any());
        assert!(QueueCount::ONE.any());
        assert_eq!(
            QueueCount::of_data(2) + QueueCount::ONE - QueueCount::of_data(5),
            QueueCount::of_data(-2)
        );
        assert_eq!(
            (QueueCount::of_data(7).factor(), QueueCount::ONE.get()),
            (CostCount::new(7.0), 1)
        );
    }
}
