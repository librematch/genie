//! Reader/writer for Age of Empires 2 Hotkey info files.
//!
//! Hotkey files in `AoE2` contain groups, each of which contain some number of
//! hotkeys. Hotkeys have a string ID, a keycode, and flags
//! for Ctrl/Alt/Shift modifiers. The index of the hotkey in its
//! group determines the action that will be taken when it is activated.

#![deny(future_incompatible)]
// a hotkey file's bindings by key code: a lookup outside the simulation, which holds no hash container (CODING_STYLE.md
// section 12)
#![allow(clippy::disallowed_types)]
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
pub use self::file::HotkeyInfo;
pub use self::group::HotkeyGroup;
pub use self::key::Hotkey;
pub use self::metadata::{HotkeyInfoMetadata, default_him, keycode_id};
#[cfg(test)]
use std::vec;

#[cfg(test)]
mod tests;
