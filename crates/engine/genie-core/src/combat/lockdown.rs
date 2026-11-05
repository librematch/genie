//! How often an object is locked down.

/// A TRIBE combat object's `locked_down_count` (`+0xa08`, a saved `i32`): the locks that hold it
/// in place. Above 0 its top speed is 0 (slot 0x208, `FUN_141202c70`). The constructors and the
/// combat setup leave 0; slot 0x4a0 adds a lock ([`LockdownCount::locked`]) and slot 0x4a8
/// takes one off ([`LockdownCount::released`]), which only the conversion's state changes call
/// (`TRIBE_Action_Convert`'s slot 0x80, `FUN_140ab9ad0`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LockdownCount(i32);

impl LockdownCount {
    /// No lock: the constructors' value.
    pub const ZERO: Self = Self(0);

    /// The count of the save's `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// The count, as the save holds it.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }

    /// A lock holds it: the count is above 0 (`CMP` with 0, `JLE`: signed).
    #[must_use]
    pub const fn holds(self) -> bool {
        0 < self.0
    }

    /// Slot 0x4a0 (`0x1412163b0`): one lock more (`INC`, which wraps).
    #[must_use]
    pub const fn locked(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    /// Slot 0x4a8 (`FUN_141226040`): one lock fewer, none while the count is below 1, which then
    /// stays.
    #[must_use]
    pub const fn released(self) -> Option<Self> {
        if self.0 < 1 {
            None
        } else {
            Some(Self(self.0 - 1))
        }
    }
}

impl genie_diff::Diff for LockdownCount {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

#[cfg(test)]
mod tests {
    use crate::combat::LockdownCount;

    /// A lock holds above 0 only; a release takes one off from 1 up and leaves a lower count.
    #[test]
    fn lockdown_holds_above_zero_and_releases_from_one_up() {
        let one = LockdownCount::ZERO.locked();
        assert_eq!(one, LockdownCount::of_data(1));
        assert!(one.holds() && !LockdownCount::ZERO.holds());
        assert!(!LockdownCount::of_data(-3).holds());
        assert_eq!(one.released(), Some(LockdownCount::ZERO));
        assert_eq!(LockdownCount::ZERO.released(), None);
        assert_eq!(LockdownCount::of_data(-3).released(), None);
        assert_eq!(
            LockdownCount::of_data(i32::MAX).locked(),
            LockdownCount::of_data(i32::MIN)
        );
    }
}
