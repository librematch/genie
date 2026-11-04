//! An Artifact's state: checking for a new owner and waiting between checks.

use crate::{GenieError, GenieResult};
use derive_more::Display;
use strum::FromRepr;

/// An Artifact's state (`+0x18`): its step (`0x140aad250`) switches on it. Its `set_state`'s walks
/// (4 and 5) pass on to 6 at once; the bytes without a state have no case, and no step modelled
/// here sets them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Display, FromRepr)]
#[repr(u8)]
#[display("{}", self.get())]
pub enum ArtifactState {
    /// 6: checking for a new owner.
    Check = 6,
    /// 10: waiting between checks.
    Wait = 10,
    /// 100: giving the animal to Gaia (case `'d'`).
    ToGaia = 100,
}

impl ArtifactState {
    /// The state of a byte of the save or the game's memory.
    ///
    /// # Errors
    ///
    /// A byte that names no state of an Artifact.
    pub fn of_byte(value: u8) -> GenieResult<Self> {
        Self::from_repr(value).ok_or_else(|| {
            GenieError::invalid_input("no state of an Artifact").with("state", value)
        })
    }

    /// Its number.
    #[must_use]
    pub const fn get(self) -> u8 {
        self as u8
    }
}

impl genie_diff::Diff for ArtifactState {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::ownership::artifact::ArtifactState;

    /// Each state reads back from its byte; the bytes without a state are errors.
    #[test]
    fn its_states_read_back_from_their_bytes() {
        for b in 0..=255_u8 {
            match ArtifactState::of_byte(b) {
                Ok(s) => assert_eq!(s.get(), b),
                Err(_) => assert!(matches!(b, 0..=5 | 7..=9 | 11..=99 | 101..), "{b}"),
            }
        }
        assert_eq!(ArtifactState::ToGaia.to_string(), "100");
    }
}
