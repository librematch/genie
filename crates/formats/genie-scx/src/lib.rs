//! A reader, writer, and converter for all versions of Age of Empires scenarios.
//!
//! This crate aims to support every single scenario that exists. If a scenario file from any Age
//! of Empires 1 or Age of Empires 2 version does not work, please upload it and file an issue!

#![deny(future_incompatible)]
#![deny(nonstandard_style)]
#![deny(rust_2018_idioms)]
#![deny(unsafe_code)]
#![warn(unused)]
#![warn(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::unimplemented,
    clippy::todo
)]

mod ai;
mod bitmap;
pub mod convert;
mod errors;
mod format;
mod header;
mod map;
mod player;
mod triggers;
mod values;
mod victory;

use format::ScxFormat;
use genie_error::GenieResult;
use std::io::{Read, Write};

pub use format::{ScenarioObject, TribeScen};
pub use genie_support::{StringKey, UnitKindId};
pub use header::{DlcOptions, ScxHeader};
pub use map::{Map, Tile};
pub use player::{ScenarioPlayerData, WorldPlayerData};
pub use triggers::{Trigger, TriggerCondition, TriggerEffect, TriggerSystem};
pub use values::*;
pub use victory::{
    VictoryConditions, VictoryEntry, VictoryPointEntry, VictoryState, VictoryVersion,
};

/// A Scenario file.
#[derive(Debug, Clone)]
pub struct Scenario {
    format: ScxFormat,
    version: VersionBundle,
}

impl Scenario {
    /// Read a scenario file.
    ///
    /// # Errors
    ///
    /// Returns an error if reading or decompressing fails, if the format version is not
    /// supported, or if the data is malformed (unknown IDs, strings that cannot be decoded).
    pub fn read_from(input: impl Read) -> GenieResult<Self> {
        let format = ScxFormat::load_scenario(input)?;
        let version = format.version();

        Ok(Self { format, version })
    }

    /// Write the scenario file to an output stream.
    ///
    /// Equivalent to `scen.write_to_version(scen.version())`.
    ///
    /// # Errors
    ///
    /// The same as [`Scenario::write_to_version`].
    pub fn write_to(&self, output: impl Write) -> GenieResult<()> {
        self.format.write_to(output, self.version())
    }

    /// Write the scenario file to an output stream, targeting specific game versions.
    ///
    /// # Errors
    ///
    /// Returns an error if writing or compressing fails, if a string cannot be encoded, or if
    /// the target version cannot hold the scenario's disabled technologies, units or buildings.
    pub fn write_to_version(&self, output: impl Write, version: &VersionBundle) -> GenieResult<()> {
        self.format.write_to(output, version)
    }

    /// Get the format version of this SCX file.
    #[inline]
    #[must_use]
    pub const fn format_version(&self) -> ScxVersion {
        self.version().format
    }

    /// Get the header version for this SCX file.
    #[inline]
    #[must_use]
    pub const fn header_version(&self) -> u32 {
        self.version().header
    }

    /// Get the data version for this SCX file.
    #[inline]
    #[must_use]
    pub const fn data_version(&self) -> f32 {
        self.version().data
    }

    /// Get the header.
    #[inline]
    #[must_use]
    pub const fn header(&self) -> &ScxHeader {
        &self.format.header
    }

    /// Get the scenario description.
    #[inline]
    #[must_use]
    pub fn description(&self) -> Option<&str> {
        self.format.tribe_scen.description()
    }

    /// Get the scenario filename.
    #[inline]
    #[must_use]
    pub fn filename(&self) -> &str {
        &self.format.tribe_scen.base.name
    }

    /// Get data about the game versions this scenario file was made for.
    #[inline]
    #[must_use]
    pub const fn version(&self) -> &VersionBundle {
        &self.version
    }

    /// Check if this scenario requires the given DLC (for HD Edition scenarios only).
    #[inline]
    #[must_use]
    pub fn requires_dlc(&self, dlc: DlcPackage) -> bool {
        self.header()
            .dlc_options
            .as_ref()
            .is_some_and(|options| options.dependencies.contains(&dlc))
    }

    /// Get the `UserPatch` mod name of the mod that was used to create this scenario.
    ///
    /// This returns the short name, like "WK" for `WololoKingdoms` or "aoc" for Age of Chivalry.
    #[inline]
    #[must_use]
    pub fn mod_name(&self) -> Option<&str> {
        self.format.mod_name()
    }

    /// Iterate over all the objects placed in the scenario.
    #[inline]
    pub fn objects(&self) -> impl Iterator<Item = &ScenarioObject> {
        self.format
            .player_objects
            .iter()
            .flat_map(|list| list.iter())
    }

    /// Iterate mutably over all the objects placed in the scenario.
    #[inline]
    pub fn objects_mut(&mut self) -> impl Iterator<Item = &mut ScenarioObject> {
        self.format
            .player_objects
            .iter_mut()
            .flat_map(|list| list.iter_mut())
    }

    /// Get the players' initial resources and population limits.
    #[must_use]
    pub fn world_players(&self) -> &[WorldPlayerData] {
        &self.format.world_players
    }

    /// Get the players' scenario settings: names, camera, diplomacy and victory conditions.
    #[must_use]
    pub fn scenario_players(&self) -> &[ScenarioPlayerData] {
        &self.format.scenario_players
    }

    /// Get the map/terrain data for this scenario.
    #[inline]
    #[must_use]
    pub const fn map(&self) -> &Map {
        &self.format.map
    }

    /// Get the (mutable) map/terrain data for this scenario.
    #[inline]
    pub const fn map_mut(&mut self) -> &mut Map {
        &mut self.format.map
    }

    /// Get trigger data for this scenario if it exists.
    #[inline]
    #[must_use]
    pub const fn triggers(&self) -> Option<&TriggerSystem> {
        self.format.triggers.as_ref()
    }

    /// Get (mutable) trigger data for this scenario if it exists.
    #[inline]
    pub const fn triggers_mut(&mut self) -> Option<&mut TriggerSystem> {
        self.format.triggers.as_mut()
    }
}
