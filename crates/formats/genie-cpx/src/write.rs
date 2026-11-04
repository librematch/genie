use crate::{AOE_AOK, AOE1_DE, AOE2_DE, CampaignHeader, CpxVersion, ScenarioMeta};
use byteorder::{LE, WriteBytesExt};
use genie_scx::{DlcPackage, Result as ScxResult, Scenario};
use std::io::{self, Write};

/// Type for errors that could occur during writing.
#[derive(Debug, thiserror::Error)]
pub enum WriteCampaignError {
    /// An I/O error occurred during writing.
    #[error("{}", .0)]
    IoError(#[from] io::Error),
    /// A scenario could not be found, either because the original campaign file was corrupt, or
    /// the scenario file exists but could not be parsed.
    #[error("missing scenario data for index {}", .0)]
    NotFoundError(usize),
}

/// `value` as a `T` when it is below `limit`, else an error naming `what`.
fn below<T: TryFrom<usize>>(value: usize, limit: usize, what: &str) -> io::Result<T> {
    let too_large = || {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{what} {value} is not below {limit}"),
        )
    };
    if limit <= value {
        return Err(too_large());
    }
    T::try_from(value).map_err(|_| too_large())
}

/// The padded `bytes` of a name of at most `width - 1` bytes.
fn padded(bytes: &[u8], width: usize, what: &str) -> io::Result<Vec<u8>> {
    let _ = below::<usize>(bytes.len(), width, what)?;
    let mut out = bytes.to_vec();
    out.resize(width, 0);
    Ok(out)
}

fn write_variable_str<W: Write>(output: &mut W, value: &str) -> io::Result<()> {
    output.write_u16::<LE>(0x0A60)?;
    let len: u16 = below(value.len(), usize::from(u16::MAX), "a string's length")?;
    output.write_u16::<LE>(len)?;
    output.write_all(value.as_bytes())?;
    Ok(())
}

/// Write the campaign header to the `output` stream.
fn write_campaign_header<W: Write>(header: &CampaignHeader, output: &mut W) -> io::Result<()> {
    let num_scenarios: u32 = below(
        header.num_scenarios,
        i32::MAX as usize,
        "the number of scenarios",
    )?;

    output.write_all(&header.version)?;
    if header.version == AOE2_DE {
        // DE2 always supports all DLC, so we can just write them all out by default
        let dependencies = [
            DlcPackage::AgeOfKings,
            DlcPackage::AgeOfConquerors,
            DlcPackage::TheForgotten,
            DlcPackage::AfricanKingdoms,
            DlcPackage::RiseOfTheRajas,
            DlcPackage::LastKhans,
        ];
        output.write_u32::<LE>(below(dependencies.len(), 7, "the number of packages")?)?;
        for dep in &dependencies {
            output.write_i32::<LE>(i32::from(*dep))?;
        }
        output.write_all(&padded(
            header.name.as_bytes(),
            256,
            "the campaign name's length",
        )?)?;
        output.write_u32::<LE>(num_scenarios)?;
    } else if header.version == AOE1_DE {
        output.write_u32::<LE>(num_scenarios)?;
        write_variable_str(output, &header.name)?;
    } else {
        output.write_all(&padded(
            header.name.as_bytes(),
            256,
            "the campaign name's length",
        )?)?;
        output.write_u32::<LE>(num_scenarios)?;
    }
    Ok(())
}

/// Write metadata for a single scenario into the `output` stream in the classic AoE/AoK format.
fn write_scenario_meta<W: Write>(meta: &ScenarioMeta, output: &mut W) -> io::Result<()> {
    output.write_i32::<LE>(below(meta.size, i32::MAX as usize, "a scenario's size")?)?;
    output.write_i32::<LE>(below(
        meta.offset,
        i32::MAX as usize,
        "a scenario's offset",
    )?)?;
    output.write_all(&padded(
        meta.name.as_bytes(),
        255,
        "a scenario name's length",
    )?)?;
    output.write_all(&padded(
        meta.filename.as_bytes(),
        255,
        "a file name's length",
    )?)?;

    Ok(())
}

/// Write metadata for a single scenario into the `output` stream, in the `AoE1`: DE format.
fn write_scenario_meta_de<W: Write>(meta: &ScenarioMeta, output: &mut W) -> io::Result<()> {
    // `u64::MAX as usize` is `usize::MAX` on every target.
    output.write_u64::<LE>(below(meta.size, usize::MAX, "a scenario's size")?)?;
    output.write_u64::<LE>(below(meta.offset, usize::MAX, "a scenario's offset")?)?;
    write_variable_str(output, &meta.name)?;
    write_variable_str(output, &meta.filename)?;
    Ok(())
}

/// Write metadata for a single scenario into the `output` stream, in the `AoE2`: DE format.
fn write_scenario_meta_de2<W: Write>(meta: &ScenarioMeta, output: &mut W) -> io::Result<()> {
    output.write_u32::<LE>(below(meta.size, u32::MAX as usize, "a scenario's size")?)?;
    output.write_u32::<LE>(below(
        meta.offset,
        u32::MAX as usize,
        "a scenario's offset",
    )?)?;
    write_variable_str(output, &meta.name)?;
    write_variable_str(output, &meta.filename)?;
    Ok(())
}

/// Describes a scenario file to be added to the campaign.
#[derive(Debug)]
struct CampaignEntry {
    name: String,
    filename: String,
    bytes: Vec<u8>,
}

impl CampaignEntry {
    /// Get the user-visible name of this entry.
    fn name(&self) -> &str {
        &self.name
    }

    /// Get the file name of this entry.
    fn filename(&self) -> &str {
        &self.filename
    }

    /// Get the size in bytes of this entry.
    const fn size(&self) -> usize {
        self.bytes.len()
    }

    /// Get the byte array for this entry.
    fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// A campaign file writer. Instantiate it, then add scenario files to it.
///
/// This has to keep all scenario files in memory until the file is written on a call to
/// `flush()`.
#[derive(Debug)]
pub struct CampaignWriter<W> {
    writer: W,
    header: CampaignHeader,
    scenarios: Vec<CampaignEntry>,
}

impl<W: Write> CampaignWriter<W> {
    /// Create a new campaign with user-visible name `name`, writing to the `writer` stream.
    pub fn new(name: &str, writer: W) -> Self {
        Self {
            writer,
            header: CampaignHeader::new(name),
            scenarios: vec![],
        }
    }

    /// Set the file version to output.
    #[must_use]
    pub fn version(mut self, version: CpxVersion) -> Self {
        debug_assert!(
            [AOE_AOK, AOE1_DE, AOE2_DE].contains(&version),
            "unknown or unsupported version"
        );
        self.header.version = version;
        self
    }

    /// Add a scenario (as a byte array) to this campaign.
    pub fn add_raw(&mut self, name: &str, filename: &str, scx: Vec<u8>) {
        self.scenarios.push(CampaignEntry {
            name: name.to_owned(),
            filename: filename.to_owned(),
            bytes: scx,
        });
    }

    /// Add a Scenario instance from genie-scx to this campaign.
    ///
    /// This returns a Result because it serializes the scenario to an in-memory byte array, which
    /// may fail.
    ///
    /// # Errors
    ///
    /// Returns an error if the scenario cannot be serialized.
    pub fn add(&mut self, name: &str, scx: &Scenario) -> ScxResult<()> {
        let mut bytes = vec![];
        scx.write_to(&mut bytes)?;
        self.scenarios.push(CampaignEntry {
            name: name.to_owned(),
            filename: scx.filename().to_owned(),
            bytes,
        });
        Ok(())
    }

    /// Consume the `CampaignWriter` instance, returning the inner `Write` instance.
    pub fn into_inner(self) -> W {
        self.writer
    }

    /// Write the campaign header.
    fn write_header(&mut self) -> io::Result<()> {
        self.header.num_scenarios = self.scenarios.len();
        write_campaign_header(&self.header, &mut self.writer)
    }

    /// Get the size in bytes of all metadata.
    const fn meta_size(&self) -> usize {
        // version + number of scenarios + campaign name
        let header_size = size_of::<CpxVersion>()
            + size_of::<i32>() // number of scenarios
            + 256; // campaign name
        header_size + self.scenarios.len() * (2 * size_of::<i32>() + 255 + 255)
    }

    /// Get the size in bytes of all metadata for a `AoE1`: DE campaign file.
    fn meta_size_de(&self) -> usize {
        // Length of a single variable string is (4 + byte length)
        const fn strlen(s: &str) -> usize {
            s.len() + 4
        }
        let header_size = size_of::<CpxVersion>()
            + size_of::<u32>() // number of scenarios
            + strlen(&self.header.name);
        self.scenarios.iter().fold(header_size, |acc, scen| {
            acc + 2 * size_of::<u64>() + strlen(&scen.name) + strlen(&scen.filename)
        })
    }

    /// Get the size in bytes of all metadata for a `AoE2`: DE campaign file.
    fn meta_size_de2(&self) -> usize {
        // Length of a single variable string is (4 + byte length)
        const fn strlen(s: &str) -> usize {
            s.len() + 4
        }
        let header_size = size_of::<CpxVersion>()
            + 7 * size_of::<i32>() // DLC dependencies
            + size_of::<u32>() // number of scenarios
            + 256; // campaign name
        self.scenarios.iter().fold(header_size, |acc, scen| {
            acc + 2 * size_of::<u32>() + strlen(&scen.name) + strlen(&scen.filename)
        })
    }

    /// Write the scenario metadata block.
    fn write_metas(&mut self) -> io::Result<()> {
        let write_meta = if self.header.version == AOE2_DE {
            write_scenario_meta_de2
        } else if self.header.version == AOE1_DE {
            write_scenario_meta_de
        } else {
            write_scenario_meta
        };

        // TODO: perhaps write to a buffer first instead of having these brittle calculations that
        // may get out of sync
        let mut offset = if self.header.version == AOE2_DE {
            self.meta_size_de2()
        } else if self.header.version == AOE1_DE {
            self.meta_size_de()
        } else {
            self.meta_size()
        };

        for scen in &self.scenarios {
            let meta = ScenarioMeta {
                size: scen.size(),
                offset,
                name: scen.name().to_owned(),
                filename: scen.filename().to_owned(),
            };
            write_meta(&meta, &mut self.writer)?;
            offset += scen.size();
        }
        Ok(())
    }

    /// Write the scenario data.
    fn write_scenarios(&mut self) -> io::Result<()> {
        for scen in &self.scenarios {
            self.writer.write_all(scen.bytes())?;
        }
        Ok(())
    }

    /// Write the scenarios to the output stream, consuming the `CampaignWriter` object.
    ///
    /// Returns the inner `Write`.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to the output stream fails.
    ///
    /// # Panics
    ///
    /// Panics if the campaign has `i32::MAX` scenarios or more, or if a name does not fit the
    /// version's fields: 255 bytes or more for the campaign name (classic and `AoE2`: DE) or a
    /// scenario's name or file name (classic), `u16::MAX` bytes or more for a variable-length
    /// string, or a scenario size or offset too large for the version's integer width.
    pub fn flush(mut self) -> io::Result<W> {
        self.write_header()?;
        self.write_metas()?;
        self.write_scenarios()?;

        Ok(self.into_inner())
    }
}

impl From<WriteCampaignError> for genie_error::GenieError {
    /// The error as a [`genie_error::GenieError`]: its kind and reason, the original as the
    /// source.
    #[track_caller]
    fn from(e: WriteCampaignError) -> Self {
        let to = match &e {
            WriteCampaignError::IoError(_) => Self::io("a read or write that failed"),
            WriteCampaignError::NotFoundError(_) => {
                Self::invalid_input("a campaign scenario without its data")
            }
        };
        to.with_source(e)
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
    fn campaign_write_errors_convert_into_genie_errors() {
        let e: genie_error::GenieError = WriteCampaignError::NotFoundError(2).into();
        assert_eq!(
            (e.kind(), e.reason()),
            (
                genie_error::ErrorKind::InvalidInput,
                "a campaign scenario without its data"
            )
        );
        assert!(Error::source(&e).is_some());
    }
}

#[cfg(test)]
mod limit_tests {
    use super::*;

    fn invalid(e: &io::Error) -> bool {
        e.kind() == io::ErrorKind::InvalidData
    }

    /// A value at its limit is an error, one below it converts.
    #[test]
    fn value_at_its_limit_is_an_error() {
        assert_eq!(below::<u16>(5, 6, "x").unwrap(), 5);
        assert!(invalid(&below::<u16>(6, 6, "x").unwrap_err()));
        assert!(invalid(&below::<u8>(300, 1000, "x").unwrap_err()));
    }

    /// A name of the width or more does not pad.
    #[test]
    fn name_of_the_width_does_not_pad() {
        assert_eq!(padded(b"ab", 4, "n").unwrap(), b"ab\0\0");
        assert!(invalid(&padded(b"abcd", 4, "n").unwrap_err()));
    }

    /// A string of more than a u16's length, a name too long and a size past the format's
    /// range fail to write.
    #[test]
    fn what_does_not_fit_fails_to_write() {
        let mut out = Vec::new();
        assert!(invalid(
            &write_variable_str(&mut out, &"a".repeat(70_000)).unwrap_err()
        ));
        let meta = ScenarioMeta {
            size: u32::MAX as usize,
            offset: 0,
            name: String::new(),
            filename: String::new(),
        };
        assert!(invalid(
            &write_scenario_meta_de2(&meta, &mut out).unwrap_err()
        ));
        let meta = ScenarioMeta {
            size: 1,
            offset: 0,
            name: "n".repeat(255),
            filename: String::new(),
        };
        assert!(invalid(&write_scenario_meta(&meta, &mut out).unwrap_err()));
    }
}
