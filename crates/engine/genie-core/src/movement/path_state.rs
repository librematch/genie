//! A path level's state: whether it asks for a path, walks it or is done with it.

use crate::GenieError;
use std::fmt::{Display, Error, Formatter};

/// A path level's state (`+0x28`, the save's `state`), which the motion step (`0x141150f00`)
/// switches on: a level that wants a path asks for one, an arrived level and one without a plan
/// hand back to their parent level. The values without a state have no case.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PathState {
    /// 0: reset (`FUN_140f98e80`) or new, or a level that handed its walk to a new level below it.
    Idle = 0,
    /// 2: it asks for a path (`FUN_140f9b800`, a reached waypoint, a parent its level below handed
    /// back to).
    WantsPath = 2,
    /// 3: it walks its path (`FUN_140f9b7c0`, the level that walks).
    Walking = 3,
    /// 4: it reached its last waypoint or its target is in range; it hands back to its parent.
    Arrived = 4,
    /// 5: it ran out of tries without a path while its top level has more points; it hands back
    /// to its parent as an arrived one.
    Unplanned = 5,
    /// 6: reset and given up (`FUN_140f9b4b0`): the unit stops.
    GaveUp = 6,
}

impl PathState {
    /// The state of a value of the save or the game's memory.
    ///
    /// # Errors
    ///
    /// A value that names no state of a path level.
    pub fn of_int(value: i32) -> Result<Self, GenieError> {
        Ok(match value {
            0 => Self::Idle,
            2 => Self::WantsPath,
            3 => Self::Walking,
            4 => Self::Arrived,
            5 => Self::Unplanned,
            6 => Self::GaveUp,
            _ => {
                return Err(
                    GenieError::invalid_input("no state of a path level").with("state", value)
                );
            }
        })
    }

    /// Its number.
    #[must_use]
    pub const fn get(self) -> i32 {
        self as i32
    }
}

impl Display for PathState {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{}", self.get())
    }
}

impl genie_diff::Diff for PathState {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::movement::PathState;

    /// Each state reads back from its value; the values without a state are errors.
    #[test]
    fn its_states_read_back_from_their_values() {
        for v in [i32::MIN, -1, 1, 7, 256, i32::MAX] {
            assert!(PathState::of_int(v).is_err(), "{v}");
        }
        for v in 0..=6 {
            match PathState::of_int(v) {
                Ok(s) => assert_eq!(s.get(), v),
                Err(_) => assert_eq!(v, 1),
            }
        }
        assert_eq!(PathState::GaveUp.to_string(), "6");
    }
}
