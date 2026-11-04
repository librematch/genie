//! A research's state: researching a technology at its building.

use crate::{GenieError, GenieResult};
use derive_more::Display;
use strum::FromRepr;

/// A research's state (`+0x18`, its `set_state` `FUN_140ad29a0` only stores it): its update
/// (`FUN_140ad2a20`) is done in 1 and works in 6, and its destructor (`FUN_140ad0be0`) tells 1 from
/// the rest. The bytes without a state have no case.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Display, FromRepr)]
#[repr(u8)]
#[display("{}", self.get())]
pub enum MakeTechState {
    /// 0: made, not started (the building's update's, `FUN_140ad0700`); its update waits.
    New = 0,
    /// 1: done: the technology is researched, or its start found it not researchable; the list
    /// removes it.
    Done = 1,
    /// 6: researching (its start's `set_state(6)`, `FUN_140ad1d80`).
    Researching = 6,
}

impl MakeTechState {
    /// The state of a byte of the save or the game's memory.
    ///
    /// # Errors
    ///
    /// A byte that names no state of a research.
    pub fn of_byte(value: u8) -> GenieResult<Self> {
        Self::from_repr(value)
            .ok_or_else(|| GenieError::invalid_input("no state of a research").with("state", value))
    }

    /// Its number.
    #[must_use]
    pub const fn get(self) -> u8 {
        self as u8
    }
}

impl genie_diff::Diff for MakeTechState {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        differ.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::production::MakeTechState;

    /// Each state reads back from its byte; the bytes without a state are errors.
    #[test]
    fn its_states_read_back_from_their_bytes() {
        for index in 0..=255_u8 {
            match MakeTechState::of_byte(index) {
                Ok(make_tech_state) => assert_eq!(make_tech_state.get(), index),
                Err(_) => assert!(matches!(index, 2..=5 | 7..), "{index}"),
            }
        }
        assert_eq!(MakeTechState::Researching.to_string(), "6");
    }
}
