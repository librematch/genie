//! A path level's place in a unit's path data.

use std::cmp::Ordering;
use std::fmt::{Display, Error, Formatter};

/// A path level's place (`mWaypointLevel`, the level's `+0x14`): the top level holds the
/// order's waypoints (2), its plan's stretches below it (1), and the path the unit walks (0).
/// The byte is saved; a value past 2 is kept as it is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WaypointLevel {
    /// 0: the path the unit walks.
    #[default]
    Walked,
    /// 1: a top level's plan, whose stretches the next pass serves.
    Planned,
    /// 2: the top level, the order's waypoints.
    Top,
    /// A value the game names nowhere, as the save or the game holds it.
    Other(Unnamed),
}

/// A raw value of [`WaypointLevel`] that names none of its variants: only the set's own
/// constructors make one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Unnamed(u8);

impl WaypointLevel {
    /// The value of raw `value`: its variant, else `Other`.
    const fn of_raw(value: u8) -> Self {
        match value {
            0 => Self::Walked,
            1 => Self::Planned,
            2 => Self::Top,
            other => Self::Other(Unnamed(other)),
        }
    }

    /// Its raw value.
    const fn raw(self) -> u8 {
        match self {
            Self::Walked => 0,
            Self::Planned => 1,
            Self::Top => 2,
            Self::Other(Unnamed(value)) => value,
        }
    }

    /// The level of byte `value`.
    #[must_use]
    pub const fn of_byte(value: u8) -> Self {
        Self::of_raw(value)
    }

    /// Its byte.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.raw()
    }
}

impl PartialOrd for WaypointLevel {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for WaypointLevel {
    /// The game's order: by the raw value.
    fn cmp(&self, other: &Self) -> Ordering {
        self.raw().cmp(&other.raw())
    }
}

impl Display for WaypointLevel {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> Result<(), Error> {
        write!(formatter, "{}", self.raw())
    }
}

impl genie_diff::Diff for WaypointLevel {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        differ.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::movement::WaypointLevel;

    /// The walked path is below the plan, the plan below the top level.
    #[test]
    fn the_levels_are_ordered_from_the_walked_path_up() {
        assert!(WaypointLevel::Walked < WaypointLevel::Planned);
        assert!(WaypointLevel::Planned < WaypointLevel::Top);
        assert_eq!(
            [WaypointLevel::Walked, WaypointLevel::of_byte(7)].map(WaypointLevel::get),
            [0, 7]
        );
    }

    /// Each named value and one the set names nowhere go to their variant and back.
    #[test]
    fn every_waypoint_level_value_maps_to_its_variant_and_back() {
        for value in [0, 1, 2, 3] {
            assert_eq!(WaypointLevel::of_raw(value).raw(), value);
        }
        assert!(matches!(WaypointLevel::of_raw(3), WaypointLevel::Other(_)));
    }
}
