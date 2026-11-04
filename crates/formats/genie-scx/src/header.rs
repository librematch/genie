use crate::Result;
use crate::values::{DataSet, DlcPackage, ScxVersion};
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use genie_support::{ReadStringsExt, read_list, write_opt_i32_str};
use std::convert::TryFrom;
use std::io::{Read, Write};
use std::result::Result as StdResult;
use std::time::{SystemTime, SystemTimeError, UNIX_EPOCH};

/// The data set and the DLC packages a scenario needs (HD Edition and later).
#[derive(Debug, Clone)]
pub struct DlcOptions {
    /// Version of the DLC options structure.
    pub version: i32,
    /// The base data set.
    pub game_data_set: DataSet,
    /// The DLCs that are required by this scenario.
    pub dependencies: Vec<DlcPackage>,
}

impl Default for DlcOptions {
    fn default() -> Self {
        Self {
            version: 1000,
            game_data_set: DataSet::BaseGame,
            dependencies: vec![DlcPackage::AgeOfKings, DlcPackage::AgeOfConquerors],
        }
    }
}

impl DlcOptions {
    /// Read DLC options, including the unversioned form older files have.
    ///
    /// # Errors
    ///
    /// Returns an error if reading fails.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        // If version is 0 or 1, it's actually the dataset identifier from
        // before DLCOptions was versioned.
        let version_or_data_set = input.read_i32::<LE>()?;
        let game_data_set =
            DataSet::from(if version_or_data_set == 0 || version_or_data_set == 1 {
                version_or_data_set
            } else {
                input.read_i32::<LE>()?
            });

        // Set version to 0 for old DLCOptions.
        let version = if version_or_data_set == 1 {
            0
        } else {
            version_or_data_set
        };

        let num_dependencies = input.read_u32::<LE>()?;
        let dependencies = read_list(num_dependencies as usize, || {
            input.read_i32::<LE>().map(DlcPackage::from)
        })?;

        Ok(Self {
            version,
            game_data_set,
            dependencies,
        })
    }

    /// Write the DLC options in the versioned form (version 1000).
    ///
    /// # Errors
    ///
    /// Returns an error if writing fails.
    // The format stores the number of dependencies as a u32.
    #[allow(clippy::cast_possible_truncation)]
    pub fn write_to(&self, mut output: impl Write) -> Result<()> {
        output.write_u32::<LE>(1000)?;
        output.write_i32::<LE>(self.game_data_set.into())?;
        output.write_u32::<LE>(self.dependencies.len() as u32)?;
        for dlc_id in &self.dependencies {
            output.write_i32::<LE>((*dlc_id).into())?;
        }
        Ok(())
    }
}

/// The scenario header: version, timestamp, description, player count and DLC needs.
#[derive(Debug, Clone)]
pub struct ScxHeader {
    /// Version of the header.
    ///
    /// Versions 2 and up include a save timestamp.
    /// Versions 3 and up contain HD Edition DLC information.
    pub version: u32,
    /// Unix timestamp when this scenario was created, in seconds.
    pub timestamp: u32,
    /// Description text about the scenario.
    pub description: Option<String>,
    /// Name of the person who created this scenario. Only available in DE2.
    pub author_name: Option<String>,
    /// Whether the scenario has any victory conditions for singleplayer.
    pub any_sp_victory: bool,
    /// How many players are supported by this scenario.
    pub active_player_count: u32,
    /// HD Edition DLC information.
    pub dlc_options: Option<DlcOptions>,
}

impl ScxHeader {
    /// Parse an SCX header from a byte stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading fails, if a string cannot be decoded, or if the DLC options
    /// hold an unknown ID.
    pub fn read_from(mut input: impl Read, format_version: ScxVersion) -> Result<Self> {
        let _header_size = input.read_u32::<LE>()?;
        let version = input.read_u32::<LE>()?;
        log::debug!("Header version {version}");
        let timestamp = if version >= 2 {
            input.read_u32::<LE>()?
        } else {
            0
        };
        let description = if format_version == *b"3.13" {
            input.read_hd_style_str()?
        } else {
            input.read_u32_length_prefixed_str()?
        };

        let any_sp_victory = input.read_u32::<LE>()? != 0;
        let active_player_count = input.read_u32::<LE>()?;

        let dlc_options = if version > 2 && format_version != *b"3.13" {
            Some(DlcOptions::read_from(&mut input)?)
        } else {
            None
        };

        let author_name;
        if version >= 5 {
            author_name = input.read_u32_length_prefixed_str()?;
            let _num_triggers = input.read_u32::<LE>()?;
        } else {
            author_name = None;
        }

        Ok(Self {
            version,
            timestamp,
            description,
            author_name,
            any_sp_victory,
            active_player_count,
            dlc_options,
        })
    }

    /// Serialize an SCX header to a byte stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing fails or the author name cannot be encoded.
    ///
    /// # Panics
    ///
    /// Panics if the description with its NULL terminator is longer than its length prefix
    /// allows (`u16` in format 3.13, `u32` otherwise), or if the author name is too long for
    /// its `i32` prefix.
    // The format stores the header size as a u32.
    #[allow(clippy::cast_possible_truncation)]
    pub fn write_to(
        &self,
        output: impl Write,
        format_version: ScxVersion,
        version: u32,
    ) -> Result<()> {
        let mut intermediate = vec![];

        intermediate.write_u32::<LE>(version)?;

        if version >= 2 {
            intermediate.write_u32::<LE>(self.timestamp)?;
        }

        let mut description_bytes = vec![];
        if let Some(ref description) = self.description {
            description_bytes.write_all(description.as_bytes())?;
        }
        description_bytes.push(0);
        if format_version == *b"3.13" {
            let len = u16::try_from(description_bytes.len())
                .map_err(|_| crate::Error::DescriptionTooLongError)?;
            intermediate.write_u16::<LE>(len)?;
        } else {
            let len = u32::try_from(description_bytes.len())
                .map_err(|_| crate::Error::DescriptionTooLongError)?;
            intermediate.write_u32::<LE>(len)?;
        }
        intermediate.write_all(&description_bytes)?;

        intermediate.write_u32::<LE>(u32::from(self.any_sp_victory))?;
        intermediate.write_u32::<LE>(self.active_player_count)?;

        if version > 2 && format_version != *b"3.13" {
            let def = DlcOptions::default();
            let dlc_options = self.dlc_options.as_ref().unwrap_or(&def);
            dlc_options.write_to(&mut intermediate)?;
        }

        if version >= 5 {
            write_opt_i32_str(&mut intermediate, &self.author_name)?;
            // TODO should be number of triggers
            intermediate.write_u32::<LE>(0)?;
        }

        // Make `output` mutable here so we don't accidentally use it above.
        let mut output = output;
        output.write_u32::<LE>(intermediate.len() as u32)?;
        output.write_all(&intermediate)?;

        Ok(())
    }

    /// Update the timestamp to the current time.
    ///
    /// # Errors
    ///
    /// Returns an error if the system clock is before the Unix epoch.
    // The format stores a 32-bit Unix timestamp.
    #[allow(clippy::cast_possible_truncation)]
    pub fn touch(&mut self) -> StdResult<(), SystemTimeError> {
        let system_time = SystemTime::now();
        let duration = system_time.duration_since(UNIX_EPOCH)?;
        self.timestamp = duration.as_secs() as u32;
        Ok(())
    }
}
