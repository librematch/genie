//! A master's `enabled` (`+0x5f0`, [`Availability`]).

use derive_more::Display;
use serde::Deserialize;
use serde::Serialize;

/// A master's `enabled` (`+0x5f0`, the `.dat`'s byte): whether its player may make it. An effect's enable command and the attribute setters change it, the setters as a byte (an add wraps, a scale rounds). Every byte is kept.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    PartialOrd,
    Ord,
    Display,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Availability(u8);

impl Availability {
    /// 0: the player may not make it.
    pub const UNAVAILABLE: Self = Self(0);
    /// 1: the player may make it.
    pub const AVAILABLE: Self = Self(1);

    /// The value of the data's byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// The byte, as the data and the save hold it.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// The player may make it: the byte is not 0.
    #[must_use]
    pub const fn is_available(self) -> bool {
        self.0 != 0
    }

    /// The byte plus `by`, wrapping as the attribute setters' add does (`FUN_141126400`).
    #[must_use]
    pub const fn plus(self, by: u8) -> Self {
        Self(self.0.wrapping_add(by))
    }
}

impl genie_diff::Diff for Availability {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::Availability;

    /// The data's bytes are kept: 0 and 1 are the named values, and any byte but 0 sets it.
    #[test]
    fn availability_keeps_the_data_byte() {
        assert_eq!(Availability::of_data(0), Availability::UNAVAILABLE);
        assert_eq!(Availability::of_data(1), Availability::AVAILABLE);
        assert_eq!(Availability::of_data(2).get(), 2);
        assert_eq!(Availability::default(), Availability::UNAVAILABLE);
        assert!(!Availability::UNAVAILABLE.is_available());
        assert!(Availability::of_data(2).is_available());
        assert_eq!(Availability::of_data(2).to_string(), "2");
        assert_eq!(Availability::of_data(255).plus(2), Availability::AVAILABLE);
    }
}
