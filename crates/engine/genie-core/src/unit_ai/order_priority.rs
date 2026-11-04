//! An order's priority.

/// An order's priority (an order's `+0x8`, the unit AI's `+0x40`): below 100 a queued order
/// waits while the unit AI has an action; a command's orders take 100, and −1 is none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OrderPriority(i32);

impl OrderPriority {
    /// None (−1): the unit AI without a current order.
    pub const NONE: Self = Self(-1);
    /// A command's (100): it does not wait for the unit AI's action.
    pub const COMMAND: Self = Self(100);

    /// The priority of the save's int `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// The int.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }

    /// Whether a queued order of it waits while the unit AI has an action: below 100.
    #[must_use]
    pub const fn waits(self) -> bool {
        self.0 < Self::COMMAND.0
    }
}

impl genie_diff::Diff for OrderPriority {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

#[cfg(test)]
mod tests {
    use crate::unit_ai::OrderPriority;

    /// Below a command's 100 an order waits; none is −1.
    #[test]
    fn below_command_priority_an_order_waits() {
        assert!(OrderPriority::of_data(99).waits() && OrderPriority::NONE.waits());
        assert!(!OrderPriority::COMMAND.waits() && !OrderPriority::of_data(101).waits());
        assert_eq!(OrderPriority::NONE.get(), -1);
    }
}
