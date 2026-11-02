//! A player's technology's state: whether it may be researched, is being researched or is
//! researched.

use crate::GenieError;
use std::fmt::{Display, Error, Formatter};

/// A player's technology's state (`Tech_Player_Tree` `+0x4`, the save's `state`). The states order
/// as their numbers: the research's tests take ranges of them (0 to 2 may complete,
/// `FUN_1413cdcd0`). The values without a state have no case.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ResearchState {
    /// −1: disabled: the civ, an effect (mode 12 with 0) or a requirement the civ disables
    /// keeps it from the player.
    Disabled = -1,
    /// 0: not available yet: its requirements are not met (a zeroed entry's).
    #[default]
    Unavailable = 0,
    /// 1: it may be researched.
    Available = 1,
    /// 2: its research runs at a building: the research's start (`0x140ad1d80`) passed its test
    /// (`FUN_1413cbef0`).
    Researching = 2,
    /// 3: researched.
    Researched = 3,
    /// 4: paid for as a building queued it (`FUN_1413d4370`); its research has not started.
    Queued = 4,
}

impl ResearchState {
    /// The state of a value of the save or the game's memory.
    ///
    /// # Errors
    ///
    /// A value that names no state of a technology.
    pub fn of_int(value: i16) -> Result<Self, GenieError> {
        Ok(match value {
            -1 => Self::Disabled,
            0 => Self::Unavailable,
            1 => Self::Available,
            2 => Self::Researching,
            3 => Self::Researched,
            4 => Self::Queued,
            _ => {
                return Err(
                    GenieError::invalid_input("no state of a technology").with("state", value)
                );
            }
        })
    }

    /// Its number.
    #[must_use]
    pub const fn get(self) -> i16 {
        self as i16
    }
}

impl Display for ResearchState {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{}", self.get())
    }
}

impl genie_diff::Diff for ResearchState {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::technology::ResearchState;

    /// Each state reads back from its value, in the order of their numbers; the values without
    /// a state are errors.
    #[test]
    fn its_states_read_back_from_their_values_in_order() {
        let mut last = None;
        for v in -1..=4 {
            let s = ResearchState::of_int(v).unwrap();
            assert_eq!(s.get(), v);
            assert!(last < Some(s), "{v}");
            last = Some(s);
        }
        for v in [i16::MIN, -2, 5, 255, i16::MAX] {
            assert!(ResearchState::of_int(v).is_err(), "{v}");
        }
        assert_eq!(ResearchState::Disabled.to_string(), "-1");
    }
}
