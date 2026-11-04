//! A Wait Solidify's state: waiting for its foundation to turn solid.

use crate::GenieError;
use derive_more::Display;
use strum::FromRepr;

/// A Wait Solidify's state (`+0x18`): the constructor's 10, changed only by its `set_state` (slot
/// 0x80, `FUN_140ae8b60`), which sets the state its caller gives: 1 and 13 every action shares, 2 a
/// stop's. The other bytes no caller modelled here gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Display, FromRepr)]
#[repr(u8)]
#[display("{}", self.get())]
pub enum WaitSolidifyState {
    /// 1: done; the list removes it.
    Done = 1,
    /// 2: stopped (`set_state(2)`).
    Stopped = 2,
    /// 10: waiting (the constructor's).
    Waiting = 10,
    /// 13 ("Failed"), the state every action shares.
    Failed = 13,
}

impl WaitSolidifyState {
    /// The state of a byte of the save or the game's memory.
    ///
    /// # Errors
    ///
    /// A byte that names no state of a Wait Solidify.
    pub fn of_byte(value: u8) -> Result<Self, GenieError> {
        Self::from_repr(value).ok_or_else(|| {
            GenieError::invalid_input("no state of a Wait Solidify").with("state", value)
        })
    }

    /// Its number.
    #[must_use]
    pub const fn get(self) -> u8 {
        self as u8
    }
}

impl genie_diff::Diff for WaitSolidifyState {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::construction::wait_solidify::WaitSolidifyState;

    /// Each state reads back from its byte; the bytes without a state are errors.
    #[test]
    fn its_states_read_back_from_their_bytes() {
        for b in 0..=255_u8 {
            match WaitSolidifyState::of_byte(b) {
                Ok(s) => assert_eq!(s.get(), b),
                Err(_) => assert!(matches!(b, 0 | 3..=9 | 11..=12 | 14..), "{b}"),
            }
        }
        assert_eq!(WaitSolidifyState::Failed.to_string(), "13");
    }
}
