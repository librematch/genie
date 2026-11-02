//! A player's path budget's values: its caps, its plans and a walker's waits.

/// A path budget's cap (`pathingAttemptCapValue` `+0x674`, `pathingDelayCapValue` `+0x678`,
/// saved ints): −1 for none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PlanCap(i32);

impl PlanCap {
    /// −1: no cap.
    pub const NONE: Self = Self(-1);

    /// The cap of the save's int `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// The int.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }

    /// Whether the cap is set (not −1).
    #[must_use]
    pub const fn is_set(self) -> bool {
        self.0 != -1
    }
}

/// The plans a player's units and groups made since its last update (`+0x67c`, an int, not
/// saved).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlanCount(i32);

impl PlanCount {
    /// None: the player's update starts the count over.
    pub const ZERO: Self = Self(0);

    /// The count of the int `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// The int.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }

    /// The count with one more plan (`FUN_141663c50`).
    #[must_use]
    pub const fn plus_one(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    /// Whether the plans reached the set cap `cap`.
    #[must_use]
    pub const fn reached(self, cap: PlanCap) -> bool {
        cap.is_set() && cap.0 <= self.0
    }
}

/// The times a walker or a group waited for its player's path budget (a group's
/// `mNumberWaitDelays`, an int; a mover's byte is [`crate::movement::WaitDelays`]), which a set
/// delay cap bounds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DelayCount(i32);

impl DelayCount {
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

    /// The count with one more wait.
    #[must_use]
    pub const fn plus_one(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    /// Whether the waits are within the delay cap `cap`: no cap, or at most it.
    #[must_use]
    pub const fn within(self, cap: PlanCap) -> bool {
        !cap.is_set() || self.0 <= cap.0
    }
}

#[cfg(test)]
mod tests {
    use crate::pathing::{DelayCount, PlanCap, PlanCount};

    /// A cap of −1 never binds; a set one binds the plans from its value on and the waits past
    /// it.
    #[test]
    fn set_cap_binds_and_none_never_does() {
        assert!(!PlanCount::of_data(99).reached(PlanCap::NONE));
        assert!(PlanCount::of_data(6).reached(PlanCap::of_data(6)));
        assert!(!PlanCount::of_data(5).reached(PlanCap::of_data(6)));
        assert!(DelayCount::of_data(9).within(PlanCap::NONE));
        assert!(DelayCount::of_data(2).within(PlanCap::of_data(2)));
        assert!(!DelayCount::of_data(3).within(PlanCap::of_data(2)));
        assert_eq!(PlanCount::ZERO.plus_one().get(), 1);
        assert_eq!(DelayCount::ZERO.plus_one(), DelayCount::of_data(1));
    }
}
