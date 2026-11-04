//! A `MoveTo`'s wait state: whether it waited after the motion step gave up.

use crate::{GenieError, GenieResult};
use derive_more::Display;
use strum::FromRepr;

/// A `MoveTo`'s wait state (`+0x6c`, `movement_wait_state`): a `MoveTo` waits once after the motion
/// step gave up ([`MoveWait::give_up`]). The bytes without a state have no case.
///
/// [`MoveWait::give_up`]: crate::movement::move_to::MoveWait::give_up
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Display, FromRepr)]
#[repr(u8)]
#[display("{}", self.get())]
pub enum MoveWaitState {
    /// 0: it never waited (the constructors').
    Never = 0,
    /// 1: it waits before it asks again (`give_up`'s, `0x140aa2a70`).
    Waiting = 1,
    /// 2: it asked for its path again after its wait (its start's, state 3); it never waits again.
    AskedAgain = 2,
}

impl MoveWaitState {
    /// The state of a byte of the save or the game's memory.
    ///
    /// # Errors
    ///
    /// A byte that names no state of a `MoveTo`'s wait.
    pub fn of_byte(value: u8) -> GenieResult<Self> {
        Self::from_repr(value).ok_or_else(|| {
            GenieError::invalid_input("no state of a MoveTo's wait").with("state", value)
        })
    }

    /// Its number.
    #[must_use]
    pub const fn get(self) -> u8 {
        self as u8
    }
}

impl genie_diff::Diff for MoveWaitState {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::movement::move_to::MoveWaitState;

    /// Each state reads back from its byte; the bytes without a state are errors.
    #[test]
    fn its_states_read_back_from_their_bytes() {
        for b in 0..=255_u8 {
            match MoveWaitState::of_byte(b) {
                Ok(s) => assert_eq!(s.get(), b),
                Err(_) => assert!(matches!(b, 3..), "{b}"),
            }
        }
        assert_eq!(MoveWaitState::AskedAgain.to_string(), "2");
    }
}
