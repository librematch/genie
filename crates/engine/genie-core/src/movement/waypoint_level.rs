//! A path level's place in a unit's path data.

use std::fmt::{Display, Error, Formatter};

/// A path level's place (`mWaypointLevel`, the level's `+0x14`): the top level holds the
/// order's waypoints (2), its plan's stretches below it (1), and the path the unit walks (0).
/// The byte is saved; a value past 2 is kept as it is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WaypointLevel(u8);

impl WaypointLevel {
    /// 0: the path the unit walks.
    pub const WALKED: Self = Self(0);
    /// 1: a top level's plan, whose stretches the next pass serves.
    pub const PLANNED: Self = Self(1);
    /// 2: the top level, the order's waypoints.
    pub const TOP: Self = Self(2);

    /// The level of byte `value`.
    #[must_use]
    pub const fn of_byte(value: u8) -> Self {
        Self(value)
    }

    /// Its byte.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl Display for WaypointLevel {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{}", self.0)
    }
}

impl genie_diff::Diff for WaypointLevel {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::movement::WaypointLevel;

    /// The walked path is below the plan, the plan below the top level.
    #[test]
    fn the_levels_are_ordered_from_the_walked_path_up() {
        assert!(WaypointLevel::WALKED < WaypointLevel::PLANNED);
        assert!(WaypointLevel::PLANNED < WaypointLevel::TOP);
        assert_eq!(
            [WaypointLevel::WALKED, WaypointLevel::of_byte(7)].map(WaypointLevel::get),
            [0, 7]
        );
    }
}
