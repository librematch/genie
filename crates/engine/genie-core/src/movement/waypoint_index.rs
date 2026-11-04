//! A waypoint's place in a level's path.

/// A waypoint's place in a path level's points (`+0x24`, an int; a saved patrol's
/// `mPatrolCurrentWaypoint`, `+0x1c8`): the point the unit or group walks to. −1 is none (a path
/// that does not loop has no next waypoint; a new path from the unit asks with it). The
/// arithmetic is the game's 32-bit one, and the comparisons with a path's count are signed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WaypointIndex(i32);

impl WaypointIndex {
    /// None (−1).
    pub const NONE: Self = Self(-1);
    /// The first point (0).
    pub const FIRST: Self = Self(0);
    /// The second point (1), where a new path's walk starts.
    pub const SECOND: Self = Self(1);

    /// The waypoint of the save's int `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// The last point of a path of `count` points (`count − 1`; −1 for none).
    #[must_use]
    pub const fn last_of(count: usize) -> Self {
        Self(count as i32 - 1)
    }

    /// The int.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }

    /// The point after it.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    /// The point before it.
    #[must_use]
    pub const fn previous(self) -> Self {
        Self(self.0.wrapping_sub(1))
    }

    /// The point `count` after it (a look ahead).
    #[must_use]
    pub const fn ahead(self, count: i32) -> Self {
        Self(self.0.wrapping_add(count))
    }

    /// Its place in the points, none below 0.
    #[must_use]
    pub fn slot(self) -> Option<usize> {
        usize::try_from(self.0).ok()
    }

    /// Whether it lies before the end of a path of `count` points (`waypoint < count`).
    #[must_use]
    pub const fn within(self, count: usize) -> bool {
        self.0 < count as i32
    }

    /// Whether it is the last point of `count` or past it (`count − 1 <= waypoint`).
    #[must_use]
    pub const fn at_or_past_last(self, count: usize) -> bool {
        Self::last_of(count).0 <= self.0
    }
}

impl genie_diff::Diff for WaypointIndex {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

#[cfg(test)]
mod tests {
    use crate::movement::WaypointIndex;

    /// The tests against a path's count are signed: −1 lies within any path, the last point of
    /// an empty one is −1.
    #[test]
    fn the_tests_against_count_are_signed() {
        assert!(WaypointIndex::NONE.within(0) && !WaypointIndex::FIRST.within(0));
        assert_eq!(WaypointIndex::last_of(0), WaypointIndex::NONE);
        assert!(WaypointIndex::of_data(2).at_or_past_last(3));
        assert!(!WaypointIndex::SECOND.at_or_past_last(3));
        assert_eq!(WaypointIndex::NONE.slot(), None);
        assert_eq!(WaypointIndex::SECOND.slot(), Some(1));
        assert_eq!(WaypointIndex::FIRST.next(), WaypointIndex::SECOND);
        assert_eq!(WaypointIndex::FIRST.previous(), WaypointIndex::NONE);
        assert_eq!(WaypointIndex::of_data(i32::MAX).next().get(), i32::MIN);
        assert_eq!(WaypointIndex::SECOND.ahead(5).get(), 6);
    }
}
