//! A gather point's state: a point SET GATHER POINT made, or a byte SET GATHER STATE gave.

use crate::{GenieError, GenieResult};
use derive_more::Display;
use std::num::NonZeroU8;

/// A building's gather point's state (`+0xc30`, the save's `gather_point_exists`): 1
/// ([`GatherPointState::Point`]) for a point SET GATHER POINT made, else the byte SET GATHER
/// STATE's packet gave the building ([`GatherPointState::Kept`], 0xff, the one SET GATHER POINT
/// does not move to another target). A state of 0 is no gather point: `Option` is `None`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Display)]
#[display("{}", self.get())]
pub enum GatherPointState {
    /// 1: a point SET GATHER POINT made; only this state sends a new unit to it
    /// (`FUN_140ace9c0`'s `+0xc30 != 1` return).
    Point,
    /// 0xff: the state SET GATHER STATE gives with a byte of 0xff: the target and its master stay
    /// whatever SET GATHER POINT asks, and a unit let out of the building walks to the point.
    Kept,
    /// Another byte SET GATHER STATE's packet gave the building.
    Other(Unnamed),
}

/// A raw value of [`GatherPointState`] that names none of its variants: only the set's own
/// constructors make one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Unnamed(NonZeroU8);

impl GatherPointState {
    /// The state of raw `value`: its variant, else `Other`.
    const fn of_raw(value: NonZeroU8) -> Self {
        match value.get() {
            1 => Self::Point,
            0xff => Self::Kept,
            _ => Self::Other(Unnamed(value)),
        }
    }

    /// Its raw value.
    const fn raw(self) -> NonZeroU8 {
        match self {
            Self::Point => NonZeroU8::MIN,
            Self::Kept => NonZeroU8::MAX,
            Self::Other(Unnamed(value)) => value,
        }
    }

    /// The state of a SET GATHER STATE packet's byte; 0 is none (the packet clears the point).
    #[must_use]
    pub const fn of_byte(value: u8) -> Option<Self> {
        match NonZeroU8::new(value) {
            Some(non_zero) => Some(Self::of_raw(non_zero)),
            None => None,
        }
    }

    /// The state of a saved value (`gather_point_exists`); 0 is none.
    ///
    /// # Errors
    ///
    /// A value past a byte: no command gives one.
    pub fn of_saved(value: i32) -> GenieResult<Option<Self>> {
        let byte = u8::try_from(value).map_err(|_| {
            GenieError::invalid_input("a gather point state past a byte").with("state", value)
        })?;
        Ok(Self::of_byte(byte))
    }

    /// Its number.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.raw().get()
    }
}

impl genie_diff::Diff for GatherPointState {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        differ.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::GatherPointState;
    use std::num::NonZeroU8;

    /// A byte reads as its state, 0 as none; a saved value past a byte is an error.
    #[test]
    fn state_reads_from_its_byte_and_0_is_none() {
        assert_eq!(GatherPointState::of_byte(0), None);
        assert_eq!(GatherPointState::of_byte(1), Some(GatherPointState::Point));
        assert_eq!(
            GatherPointState::of_byte(0xff),
            Some(GatherPointState::Kept)
        );
        assert_eq!(
            GatherPointState::of_byte(7).map(GatherPointState::get),
            Some(7)
        );
        assert_eq!(GatherPointState::of_saved(0), Ok(None));
        assert_eq!(
            GatherPointState::of_saved(255),
            Ok(Some(GatherPointState::Kept))
        );
        assert!(GatherPointState::of_saved(256).is_err());
        assert!(GatherPointState::of_saved(-1).is_err());
        assert_eq!(GatherPointState::Kept.to_string(), "255");
    }

    /// Each named state and one the set names nowhere go to their variant and back.
    #[test]
    fn every_gather_point_state_value_maps_to_its_variant_and_back() {
        for value in [1, 0xff, 7] {
            let raw = NonZeroU8::new(value).unwrap();
            assert_eq!(GatherPointState::of_raw(raw).raw(), raw);
        }
        assert!(matches!(
            GatherPointState::of_raw(NonZeroU8::new(7).unwrap()),
            GatherPointState::Other(_)
        ));
    }
}
