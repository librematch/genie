//! A gather point's state: a point SET GATHER POINT made, or a byte SET GATHER STATE gave.

use crate::GenieError;
use std::fmt::{Display, Error, Formatter};
use std::num::NonZeroU8;

/// A building's gather point's state (`+0xc30`, the save's `gather_point_exists`): 1
/// ([`GatherPointState::POINT`]) for a point SET GATHER POINT made, else the byte SET GATHER
/// STATE's packet gave the building ([`GatherPointState::KEPT`], 0xff, the one SET GATHER POINT
/// does not move to another target). A state of 0 is no gather point: `Option` is `None`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GatherPointState(NonZeroU8);

impl GatherPointState {
    /// 1: a point SET GATHER POINT made; only this state sends a new unit to it
    /// (`FUN_140ace9c0`'s `+0xc30 != 1` return).
    pub const POINT: Self = Self(NonZeroU8::MIN);
    /// 0xff: the state SET GATHER STATE gives with a byte of 0xff: the target and its master stay
    /// whatever SET GATHER POINT asks, and a unit let out of the building walks to the point.
    pub const KEPT: Self = Self(NonZeroU8::MAX);

    /// The state of a SET GATHER STATE packet's byte; 0 is none (the packet clears the point).
    #[must_use]
    pub const fn of_byte(value: u8) -> Option<Self> {
        match NonZeroU8::new(value) {
            Some(v) => Some(Self(v)),
            None => None,
        }
    }

    /// The state of a saved value (`gather_point_exists`); 0 is none.
    ///
    /// # Errors
    ///
    /// A value past a byte: no command gives one.
    pub fn of_saved(value: i32) -> Result<Option<Self>, GenieError> {
        let byte = u8::try_from(value).map_err(|_| {
            GenieError::invalid_input("a gather point state past a byte").with("state", value)
        })?;
        Ok(Self::of_byte(byte))
    }

    /// Its number.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0.get()
    }
}

impl Display for GatherPointState {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{}", self.get())
    }
}

impl genie_diff::Diff for GatherPointState {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::GatherPointState;

    /// A byte reads as its state, 0 as none; a saved value past a byte is an error.
    #[test]
    fn state_reads_from_its_byte_and_0_is_none() {
        assert_eq!(GatherPointState::of_byte(0), None);
        assert_eq!(GatherPointState::of_byte(1), Some(GatherPointState::POINT));
        assert_eq!(
            GatherPointState::of_byte(0xff),
            Some(GatherPointState::KEPT)
        );
        assert_eq!(
            GatherPointState::of_byte(7).map(GatherPointState::get),
            Some(7)
        );
        assert_eq!(GatherPointState::of_saved(0), Ok(None));
        assert_eq!(
            GatherPointState::of_saved(255),
            Ok(Some(GatherPointState::KEPT))
        );
        assert!(GatherPointState::of_saved(256).is_err());
        assert!(GatherPointState::of_saved(-1).is_err());
        assert_eq!(GatherPointState::KEPT.to_string(), "255");
    }
}
