//! Reader/writer for Age of Empires 2 Hotkey info files.
//!
//! Hotkey files in `AoE2` contain groups, each of which contain some number of
//! hotkeys. Hotkeys have a string ID, a keycode, and flags
//! for Ctrl/Alt/Shift modifiers. The index of the hotkey in its
//! group determines the action that will be taken when it is activated.

#![deny(future_incompatible)]
#![deny(nonstandard_style)]
#![deny(rust_2018_idioms)]
#![deny(unsafe_code)]
#![warn(unused)]

mod catalogue;
mod errors;
mod file;
mod group;
mod key;
mod metadata;

pub use self::catalogue::{
    ArcheryRangeHotkeys, BarracksHotkeys, BlacksmithHotkeys, CastleHotkeys, DockHotkeys,
    FormationHotkeys, GameCommandHotkeys, HotkeyGroupId, MarketHotkeys, MilitaryUnitHotkeys,
    MillHotkeys, MonasteryHotkeys, ScrollHotkeys, SiegeWorkshopHotkeys, StableHotkeys,
    TownCenterHotkeys, UnitCommandHotkeys, VillagerHotkeys,
};
pub use self::errors::{GroupIndexError, HotkeyIndexError, IndexError};
pub use self::file::HotkeyInfo;
pub use self::group::HotkeyGroup;
pub use self::key::Hotkey;
pub use self::metadata::{HotkeyInfoMetadata, default_him, keycode_id};
#[cfg(test)]
use std::vec;

impl From<IndexError> for genie_error::GenieError {
    /// The error as a [`genie_error::GenieError`]: its kind and reason, the original as the
    /// source.
    #[track_caller]
    fn from(e: IndexError) -> Self {
        let to = match &e {
            IndexError::GroupIndex(_) => {
                Self::invalid_input("a hotkey group index past the groups")
            }
            IndexError::HotkeyIndex(_) => {
                Self::invalid_input("a hotkey index past the group's hotkeys")
            }
        };
        to.with_source(e)
    }
}

impl From<GroupIndexError> for genie_error::GenieError {
    /// The error as a [`genie_error::GenieError`]: its kind and reason, the original as the
    /// source.
    #[track_caller]
    fn from(e: GroupIndexError) -> Self {
        Self::invalid_input("a hotkey group index past the groups").with_source(e)
    }
}

impl From<HotkeyIndexError> for genie_error::GenieError {
    /// The error as a [`genie_error::GenieError`]: its kind and reason, the original as the
    /// source.
    #[track_caller]
    fn from(e: HotkeyIndexError) -> Self {
        Self::invalid_input("a hotkey index past the group's hotkeys").with_source(e)
    }
}

#[cfg(test)]
mod genie_error_tests {
    use super::*;
    #[cfg(test)]
    use std::error::Error;

    /// The errors convert into `GenieError`s with their kinds and reasons, the original as
    /// source.
    #[test]
    fn index_errors_convert_into_genie_errors() {
        let e: genie_error::GenieError = GroupIndexError::new(5, 3).into();
        assert_eq!(
            (e.kind(), e.reason()),
            (
                genie_error::ErrorKind::InvalidInput,
                "a hotkey group index past the groups"
            )
        );
        let e: genie_error::GenieError =
            IndexError::HotkeyIndex(HotkeyIndexError::new(9, 2)).into();
        assert_eq!(e.reason(), "a hotkey index past the group's hotkeys");
        assert!(Error::source(&e).is_some());
    }
}

#[cfg(test)]
mod tests;
