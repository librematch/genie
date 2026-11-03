//! A path level's link to its parent and the plans it may still try; a mover's wait delays and
//! substitute count.

use crate::pathing::DelayCount;

/// What an arrived level hands its parent (`+0x10`, `mLinkedPathType`): 1 marks the parent's
/// waypoint reached, 2 has the parent ask again (a looping parent moves on first), 3 has it ask
/// again; any other value hands nothing over.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct LinkedPathKind(i32);

impl LinkedPathKind {
    /// No link (0).
    pub const NONE: Self = Self(0);
    /// The parent's waypoint is reached (1).
    pub const REACHES_WAYPOINT: Self = Self(1);
    /// The parent asks again, a looping one moving on first (2).
    pub const MOVES_ON: Self = Self(2);
    /// The parent asks again (3).
    pub const ASKS_AGAIN: Self = Self(3);

    /// The link of the save's int `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// The int.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

impl genie_diff::Diff for LinkedPathKind {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

/// The plans a path level may still try before it gives up (`+0x38`, `mContinueCounter`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlanTries(i32);

impl PlanTries {
    /// A new level's tries (10: `FUN_140f8f670`, `FUN_140f9b800`, `FUN_140f98e80`).
    pub const FULL: Self = Self(10);

    /// The tries of the save's int `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// The int.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }

    /// One try spent (wrapping as the game's int).
    #[must_use]
    pub const fn spent(self) -> Self {
        Self(self.0.wrapping_sub(1))
    }

    /// Whether a try is left: above 0.
    #[must_use]
    pub const fn left(self) -> bool {
        0 < self.0
    }
}

impl genie_diff::Diff for PlanTries {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

/// The plans a mover's path request let go for want of budget (`+0x7fa`, `numberWaitDelays`, a
/// byte: trade carts).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WaitDelays(u8);

impl WaitDelays {
    /// None.
    pub const NONE: Self = Self(0);

    /// The count of the save's byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// The byte.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// The delays as a path budget's delay count (the byte widened).
    #[must_use]
    pub const fn count(self) -> DelayCount {
        DelayCount::of_data(self.0 as i32)
    }

    /// One delay more (wrapping as a byte).
    #[must_use]
    pub const fn plus_one(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

impl genie_diff::Diff for WaitDelays {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

/// A mover's consecutive substitutes (`+0x874`, `mConsecutiveSubstituteCount`): above 2 a walk
/// counts its waypoint reached.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SubstituteCount(i32);

impl SubstituteCount {
    /// None.
    pub const NONE: Self = Self(0);

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
}

impl genie_diff::Diff for SubstituteCount {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

/// The plans one motion step made (`0x141150f00`'s `local_19a8`): a unit makes at most 3 in
/// one step.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StepPlans(u32);

impl StepPlans {
    /// None yet.
    pub const ZERO: Self = Self(0);

    /// A count of `n` plans.
    #[must_use]
    pub const fn new(n: u32) -> Self {
        Self(n)
    }

    /// The count.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// One plan more.
    #[must_use]
    pub const fn plus_one(self) -> Self {
        Self(self.0 + 1)
    }
}

#[cfg(test)]
mod tests {
    use crate::movement::{PlanTries, StepPlans, WaitDelays};

    /// A level's tries run out at 0; the delays wrap as a byte.
    #[test]
    fn the_tries_run_out_at_zero() {
        assert!(PlanTries::of_data(1).left() && !PlanTries::of_data(1).spent().left());
        assert_eq!(PlanTries::FULL.get(), 10);
        assert_eq!(WaitDelays::of_data(255).plus_one(), WaitDelays::NONE);
    }

    /// A step's plans count up one by one.
    #[test]
    fn step_plans_count_up() {
        assert_eq!(StepPlans::ZERO.plus_one().plus_one(), StepPlans::new(2));
        assert!(StepPlans::new(2) < StepPlans::new(3));
    }
}
