//! A unit's make action's state: making the unit its building trains.

use crate::GenieError;
use derive_more::Display;
use strum::FromRepr;

/// A unit's make action's state (`+0x18`, set by `set_state`, `FUN_140acfec0`): its update
/// (`0x140ad0490`) is done in 1, works in 6 and counts its timer down in 10, and its destructor
/// (`FUN_140ace3f0`, `(state - 6) & 0xfb == 0`) takes the unit's population back in 6 and 10. The
/// bytes without a state have no case.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Display, FromRepr)]
#[repr(u8)]
#[display("{}", self.get())]
pub enum MakeObjState {
    /// 0: made, not started (the building's update's), or its start found no master of the player;
    /// its update waits.
    New = 0,
    /// 1: done: its unit came; the list removes it.
    Done = 1,
    /// 6: making its unit (its start's `set_state(6)`, `FUN_140acf0a0`).
    Making = 6,
    /// 10: its unit found no spot beside the building (the spawn's `set_state(10)`,
    /// `FUN_140ad0240`): its timer (`+0x44`) runs down from 5 s and the spawn tries again; not
    /// modelled (the spot search fails with an error).
    NoSpot = 10,
}

impl MakeObjState {
    /// The state of a byte of the save or the game's memory.
    ///
    /// # Errors
    ///
    /// A byte that names no state of a unit's make action.
    pub fn of_byte(value: u8) -> Result<Self, GenieError> {
        Self::from_repr(value).ok_or_else(|| {
            GenieError::invalid_input("no state of a unit's make action").with("state", value)
        })
    }

    /// Its number.
    #[must_use]
    pub const fn get(self) -> u8 {
        self as u8
    }
}

impl genie_diff::Diff for MakeObjState {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::production::MakeObjState;

    /// Each state reads back from its byte; the bytes without a state are errors.
    #[test]
    fn its_states_read_back_from_their_bytes() {
        for b in 0..=255_u8 {
            match MakeObjState::of_byte(b) {
                Ok(s) => assert_eq!(s.get(), b),
                Err(_) => assert!(matches!(b, 2..=5 | 7..=9 | 11..), "{b}"),
            }
        }
        assert_eq!(MakeObjState::NoSpot.to_string(), "10");
    }
}
