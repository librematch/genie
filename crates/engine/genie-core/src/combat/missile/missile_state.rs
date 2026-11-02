//! A missile's state: launched, flying, ended.

use crate::GenieError;
use std::fmt::{Display, Error, Formatter};

/// A missile action's state (`+0x18`, set by `set_state`, `FUN_140a9de70`): its update flies in
/// state 4. The bytes without a state have no case, and no step modelled here sets them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MissileState {
    /// 0: made, not started (the volley's).
    New = 0,
    /// 1: ended: the object's slot 0xd8, its hit points to 0.
    Done = 1,
    /// 3: launched toward its point (`set_state(3)`), which goes on to 4 at once.
    Launch = 3,
    /// 4: flying.
    Flying = 4,
}

impl MissileState {
    /// The state of a byte of the save or the game's memory.
    ///
    /// # Errors
    ///
    /// A byte that names no state of a missile.
    pub fn of_byte(value: u8) -> Result<Self, GenieError> {
        Ok(match value {
            0 => Self::New,
            1 => Self::Done,
            3 => Self::Launch,
            4 => Self::Flying,
            _ => {
                return Err(GenieError::invalid_input("no state of a missile").with("state", value));
            }
        })
    }

    /// Its number.
    #[must_use]
    pub const fn get(self) -> u8 {
        self as u8
    }
}

impl Display for MissileState {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{}", self.get())
    }
}

impl genie_diff::Diff for MissileState {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::combat::missile::MissileState;

    /// Each state reads back from its byte; the bytes without a state are errors.
    #[test]
    fn its_states_read_back_from_their_bytes() {
        for b in 0..=255_u8 {
            match MissileState::of_byte(b) {
                Ok(s) => assert_eq!(s.get(), b),
                Err(_) => assert!(matches!(b, 2 | 5..), "{b}"),
            }
        }
        assert_eq!(MissileState::Flying.to_string(), "4");
    }
}
