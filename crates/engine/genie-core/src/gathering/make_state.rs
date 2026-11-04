//! A Make's state: making food.

use crate::{GenieError, GenieResult};
use derive_more::Display;
use strum::FromRepr;

/// A Make's state (`+0x18`, set by `set_state`, `FUN_140a99d60`). The bytes without a state have no
/// case, and no step modelled here sets them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Display, FromRepr)]
#[repr(u8)]
#[display("{}", self.get())]
pub enum MakeState {
    /// 0: made, not started (the constructor's).
    New = 0,
    /// 1: done; the list removes it.
    Done = 1,
    /// 6: making the player's food (`first_in_stack`'s `set_state(6)`).
    Making = 6,
}

impl MakeState {
    /// The state of a byte of the save or the game's memory.
    ///
    /// # Errors
    ///
    /// A byte that names no state of a Make.
    pub fn of_byte(value: u8) -> GenieResult<Self> {
        Self::from_repr(value)
            .ok_or_else(|| GenieError::invalid_input("no state of a Make").with("state", value))
    }

    /// Its number.
    #[must_use]
    pub const fn get(self) -> u8 {
        self as u8
    }
}

impl genie_diff::Diff for MakeState {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        differ.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::gathering::make::MakeState;

    /// Each state reads back from its byte; the bytes without a state are errors.
    #[test]
    fn its_states_read_back_from_their_bytes() {
        for index in 0..=255_u8 {
            match MakeState::of_byte(index) {
                Ok(make_state) => assert_eq!(make_state.get(), index),
                Err(_) => assert!(matches!(index, 2..=5 | 7..), "{index}"),
            }
        }
        assert_eq!(MakeState::Making.to_string(), "6");
    }
}
