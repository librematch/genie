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
mod format;
mod header;
mod map;
mod player;
mod triggers;
mod types;
mod victory;

use format::ScxFormat;
use genie_support::{ReadStringError, WriteStringError};
use std::io::{self, Read, Write};

pub use format::{ScenarioObject, TribeScen};
pub use genie_support::{DecodeStringError, EncodeStringError};
pub use genie_support::{StringKey, UnitTypeId};
pub use header::{DlcOptions, ScxHeader};
pub use map::{Map, Tile};
pub use player::{ScenarioPlayerData, WorldPlayerData};
pub use triggers::{Trigger, TriggerCondition, TriggerEffect, TriggerSystem};
pub use types::*;
pub use victory::{VictoryConditions, VictoryEntry, VictoryPointEntry, VictoryState};

/// Error type for SCX methods, containing all types of errors that may occur while reading or
/// writing scenario files.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The scenario that's attempted to be read does not contain a file name.
    #[error("must have a file name")]
    MissingFileNameError,
    /// Attempted to read a scenario with an unsupported format version identifier.
    #[error("unsupported format version {:?}", .0)]
    UnsupportedFormatVersionError(ScxVersion),
    /// Attempted to write a scenario with disabled technologies, to a version that doesn't support
    /// this many disabled technologies.
    #[error("too many disabled techs: got {}, but requested version supports up to 20", .0)]
    TooManyDisabledTechsError(i32),
    /// Attempted to write a scenario with disabled technologies, to a version that doesn't support
    /// disabling technologies.
    #[error("requested version does not support disabling techs")]
    CannotDisableTechsError,
    /// Attempted to write a scenario with disabled units, to a version that doesn't support
    /// disabling units.
    #[error("requested version does not support disabling units")]
    CannotDisableUnitsError,
    /// Attempted to write a scenario with disabled buildings, to a version that doesn't support
    /// this many disabled buildings.
    #[error("too many disabled buildings: got {}, but requested version supports up to {}", .0, .1)]
    TooManyDisabledBuildingsError(i32, i32),
    /// Attempted to write a scenario with disabled buildings, to a version that doesn't support
    /// disabling buildings.
    #[error("requested version does not support disabling buildings")]
    CannotDisableBuildingsError,
    /// Failed to decode a string from the scenario file, probably because of a wrong encoding.
    #[error(transparent)]
    DecodeStringError(#[from] DecodeStringError),
    /// Failed to encode a string into the scenario file, probably because of a wrong encoding.
    #[error(transparent)]
    EncodeStringError(#[from] EncodeStringError),
    /// The given ID is not a known diplomatic stance.
    #[error(transparent)]
    ParseDiplomaticStanceError(#[from] ParseDiplomaticStanceError),
    /// The given ID is not a known data set.
    #[error(transparent)]
    ParseDataSetError(#[from] ParseDataSetError),
    /// The given ID is not a known HD Edition DLC.
    #[error(transparent)]
    ParseDlcPackageError(#[from] ParseDlcPackageError),
    /// The given ID is not a known starting age in `AoE1` or `AoE2`.
    #[error(transparent)]
    ParseStartingAgeError(#[from] ParseStartingAgeError),
    /// The given ID is not a known error code.
    #[error(transparent)]
    ParseAiErrorCodeError(#[from] num_enum::TryFromPrimitiveError<ai::AiErrorCode>),
    /// The given ID is not a known victory condition state.
    #[error(transparent)]
    ParseVictoryConditionStateError(#[from] num_enum::TryFromPrimitiveError<VictoryState>),
    /// A string is too long for the length prefix it is written with.
    #[error("string too long for its length prefix")]
    StringTooLongError,
    /// An embedded AI file lacks its file name.
    #[error("an embedded AI file has no file name")]
    MissingAiFileNameError,
    /// An embedded AI file has no content.
    #[error("an embedded AI file is empty")]
    EmptyAiFileError,
    /// The scenario's description is longer than its length prefix allows.
    #[error("description too long for its length prefix")]
    DescriptionTooLongError,
    /// The scenario has more triggers than the format counts.
    #[error("more triggers than the format can count")]
    TooManyTriggersError,
    /// The version bundle for the requested game is not modelled.
    #[error("no version bundle for this game is modelled")]
    UnsupportedGameError,
    /// An error occurred while reading or writing.
    #[error(transparent)]
    IoError(#[from] io::Error),
}

impl From<ReadStringError> for Error {
    fn from(err: ReadStringError) -> Self {
        match err {
            ReadStringError::IoError(err) => Self::IoError(err),
            ReadStringError::DecodeStringError(err) => Self::DecodeStringError(err),
        }
    }
}

impl From<WriteStringError> for Error {
    fn from(err: WriteStringError) -> Self {
        match err {
            WriteStringError::IoError(err) => Self::IoError(err),
            WriteStringError::EncodeStringError(err) => Self::EncodeStringError(err),
            WriteStringError::TooLong => Self::StringTooLongError,
        }
    }
}

/// Result type for SCX methods.
pub type Result<T> = std::result::Result<T, Error>;

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
    pub fn read_from(input: impl Read) -> Result<Self> {
        let format = ScxFormat::load_scenario(input)?;
        let version = format.version();

        Ok(Self { format, version })
    }

    /// Read a scenario file.
    ///
    /// # Errors
    ///
    /// The same as [`Scenario::read_from`].
    #[deprecated = "Use Scenario::read_from instead."]
    pub fn from<R: Read>(input: &mut R) -> Result<Self> {
        Self::read_from(input)
    }

    /// Write the scenario file to an output stream.
    ///
    /// Equivalent to `scen.write_to_version(scen.version())`.
    ///
    /// # Errors
    ///
    /// The same as [`Scenario::write_to_version`].
    pub fn write_to(&self, output: impl Write) -> Result<()> {
        self.format.write_to(output, self.version())
    }

    /// Write the scenario file to an output stream, targeting specific game versions.
    ///
    /// # Errors
    ///
    /// Returns an error if writing or compressing fails, if a string cannot be encoded, or if
    /// the target version cannot hold the scenario's disabled technologies, units or buildings.
    pub fn write_to_version(&self, output: impl Write, version: &VersionBundle) -> Result<()> {
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

impl From<Error> for genie_error::GenieError {
    /// The error as a [`genie_error::GenieError`]: its kind and reason, the original as the
    /// source.
    #[track_caller]
    fn from(e: Error) -> Self {
        let to = match &e {
            Error::MissingFileNameError => Self::invalid_input("a scenario without a file name"),
            Error::UnsupportedFormatVersionError(_) => {
                Self::unsupported("a scenario format version this reader does not take")
            }
            Error::TooManyDisabledTechsError(_) => {
                Self::invalid_input("more disabled technologies than the version takes")
            }
            Error::CannotDisableTechsError => {
                Self::unsupported("disabled technologies in a version without them")
            }
            Error::CannotDisableUnitsError => {
                Self::unsupported("disabled units in a version without them")
            }
            Error::TooManyDisabledBuildingsError(..) => {
                Self::invalid_input("more disabled buildings than the version takes")
            }
            Error::CannotDisableBuildingsError => {
                Self::unsupported("disabled buildings in a version without them")
            }
            Error::DecodeStringError(_) => Self::invalid_input("a string that does not decode"),
            Error::EncodeStringError(_) => Self::invalid_input("a string that does not encode"),
            Error::ParseDiplomaticStanceError(_)
            | Error::ParseDataSetError(_)
            | Error::ParseDlcPackageError(_)
            | Error::ParseStartingAgeError(_)
            | Error::ParseAiErrorCodeError(_)
            | Error::ParseVictoryConditionStateError(_) => {
                Self::invalid_input("an id this reader does not know")
            }
            Error::StringTooLongError => {
                Self::invalid_input("a string too long for its length prefix")
            }
            Error::MissingAiFileNameError => {
                Self::invalid_input("an embedded AI file without a file name")
            }
            Error::EmptyAiFileError => Self::invalid_input("an embedded AI file without content"),
            Error::DescriptionTooLongError => {
                Self::invalid_input("a description too long for its length prefix")
            }
            Error::TooManyTriggersError => {
                Self::invalid_input("more triggers than the format can count")
            }
            Error::UnsupportedGameError => {
                Self::missing_data("a version bundle this crate does not hold")
            }
            Error::IoError(_) => Self::io("a read or write that failed"),
        };
        to.with_source(e)
    }
}

#[cfg(test)]
mod genie_error_tests {
    use super::*;

    /// The errors convert into `GenieError`s with their kinds and reasons, the original as
    /// source.
    #[test]
    fn scenario_errors_convert_into_genie_errors() {
        let e: genie_error::GenieError = Error::MissingFileNameError.into();
        assert_eq!(
            (e.kind(), e.reason()),
            (
                genie_error::ErrorKind::InvalidInput,
                "a scenario without a file name"
            )
        );
        assert!(std::error::Error::source(&e).is_some());
        let e: genie_error::GenieError = Error::CannotDisableTechsError.into();
        assert_eq!(
            (e.kind(), e.reason()),
            (
                genie_error::ErrorKind::Unsupported,
                "disabled technologies in a version without them"
            )
        );
        assert!(std::error::Error::source(&e).is_some());
        let e: genie_error::GenieError = Error::IoError(io::Error::other("x")).into();
        assert_eq!(
            (e.kind(), e.reason()),
            (genie_error::ErrorKind::Io, "a read or write that failed")
        );
        assert!(std::error::Error::source(&e).is_some());
    }

    /// Each scenario error's kind and reason.
    #[test]
    fn each_scenario_error_has_its_kind_and_reason() {
        use genie_error::ErrorKind::{InvalidInput, Io, Unsupported};
        let unknown_id = "an id this reader does not know";
        let cases: Vec<(Error, genie_error::ErrorKind, &str)> = vec![
            (
                Error::UnsupportedFormatVersionError(ScxVersion(*b"9.99")),
                Unsupported,
                "a scenario format version this reader does not take",
            ),
            (
                Error::TooManyDisabledTechsError(21),
                InvalidInput,
                "more disabled technologies than the version takes",
            ),
            (
                Error::CannotDisableUnitsError,
                Unsupported,
                "disabled units in a version without them",
            ),
            (
                Error::TooManyDisabledBuildingsError(31, 30),
                InvalidInput,
                "more disabled buildings than the version takes",
            ),
            (
                Error::CannotDisableBuildingsError,
                Unsupported,
                "disabled buildings in a version without them",
            ),
            (
                DecodeStringError.into(),
                InvalidInput,
                "a string that does not decode",
            ),
            (
                EncodeStringError.into(),
                InvalidInput,
                "a string that does not encode",
            ),
            (
                DiplomaticStance::try_from(9).unwrap_err().into(),
                InvalidInput,
                unknown_id,
            ),
            (
                DataSet::try_from(9).unwrap_err().into(),
                InvalidInput,
                unknown_id,
            ),
            (
                StartingAge::try_from(99, 1.2).unwrap_err().into(),
                InvalidInput,
                unknown_id,
            ),
            (
                ai::AiErrorCode::try_from(999).unwrap_err().into(),
                InvalidInput,
                unknown_id,
            ),
            (
                num_enum::TryFromPrimitiveError::<VictoryState> { number: 99 }.into(),
                InvalidInput,
                unknown_id,
            ),
            (
                Error::IoError(io::Error::other("x")),
                Io,
                "a read or write that failed",
            ),
        ];
        for (error, kind, reason) in cases {
            let e: genie_error::GenieError = error.into();
            assert_eq!((e.kind(), e.reason()), (kind, reason));
        }
    }
}
