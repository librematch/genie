use crate::{AOE1_DE, AOE2_DE, CampaignHeader, CpxVersion, ScenarioMeta};
use byteorder::{LE, ReadBytesExt};
use chardet::detect as detect_encoding;
use encoding_rs::Encoding;
use genie_error::{GenieError, GenieResult};
use genie_scx::{DlcPackage, Scenario};
use genie_support::{read_bytes, read_list};
use std::io::{Cursor, Read, Seek, SeekFrom};

/// A string of the campaign that does not decode.
const UNDECODED_STRING: GenieError =
    GenieError::invalid_input("a campaign string that does not decode");
/// A campaign or a scenario without its name.
const NO_NAME: GenieError = GenieError::invalid_input("a campaign or scenario without a name");

/// Decode a string with unknown encoding.
fn decode_str(bytes: &[u8]) -> GenieResult<String> {
    if bytes.is_empty() {
        return Ok(String::new());
    }

    let (encoding_name, _confidence, _language) = detect_encoding(bytes);
    Encoding::for_label(encoding_name.as_bytes())
        .ok_or(UNDECODED_STRING)
        .and_then(|encoding| {
            let (decoded, _enc, failed) = encoding.decode(bytes);
            if failed {
                return Err(UNDECODED_STRING);
            }
            Ok(decoded.to_string())
        })
}

pub fn read_fixed_str<R: Read>(input: &mut R, len: usize) -> GenieResult<Option<String>> {
    let mut bytes = read_bytes(input, len)?;

    if let Some(end) = bytes.iter().position(|&byte| byte == 0) {
        bytes.truncate(end);
    }
    if bytes.is_empty() {
        Ok(None)
    } else {
        decode_str(&bytes).map(Some)
    }
}

fn read_hd_or_later_string<R: Read>(input: &mut R) -> GenieResult<Option<String>> {
    let open = input.read_u16::<LE>()?;
    // Check that this actually is the start of a string
    if open != 0x0A60 {
        return Err(UNDECODED_STRING);
    }
    let len = input.read_u16::<LE>()? as usize;
    let bytes = read_bytes(input, len)?;
    decode_str(&bytes).map(Some)
}

fn read_campaign_header<R: Read>(input: &mut R) -> GenieResult<CampaignHeader> {
    let mut version = [0; 4];
    input.read_exact(&mut version)?;

    let num_scenarios;
    let name;

    if version == AOE1_DE {
        num_scenarios = input.read_u32::<LE>()? as usize;
        name = read_hd_or_later_string(input)?.ok_or(NO_NAME)?;
    } else {
        // DE2 added package dependency data. We don't store that right now, because DE always
        // supports all packages.[citation needed]
        if version == AOE2_DE {
            let num_dependencies = input.read_u32::<LE>()?;
            let _dependencies = read_list(num_dependencies as usize, || {
                input.read_i32::<LE>().map(DlcPackage::from)
            })?;
        }

        name = read_fixed_str(input, 256)?.ok_or(NO_NAME)?;
        num_scenarios = input.read_u32::<LE>()? as usize;
    }

    Ok(CampaignHeader {
        version,
        name,
        num_scenarios,
    })
}

fn read_scenario_meta_de2<R: Read>(input: &mut R) -> GenieResult<ScenarioMeta> {
    let size = input.read_u32::<LE>()? as usize;
    let offset = input.read_u32::<LE>()? as usize;
    let name = read_hd_or_later_string(input)?.ok_or(NO_NAME)?;
    let filename = read_hd_or_later_string(input)?.ok_or(NO_NAME)?;

    Ok(ScenarioMeta {
        size,
        offset,
        name,
        filename,
    })
}

// `usize` is 64 bits wide on the usual targets; a 32-bit target truncates the u64 values.
#[allow(clippy::cast_possible_truncation)]
fn read_scenario_meta_de<R: Read>(input: &mut R) -> GenieResult<ScenarioMeta> {
    let size = input.read_u64::<LE>()? as usize;
    let offset = input.read_u64::<LE>()? as usize;
    let name = read_hd_or_later_string(input)?.ok_or(NO_NAME)?;
    let filename = read_hd_or_later_string(input)?.ok_or(NO_NAME)?;

    Ok(ScenarioMeta {
        size,
        offset,
        name,
        filename,
    })
}

// The classic format stores size and offset as i32; a negative value is not expected.
#[allow(clippy::cast_sign_loss)]
fn read_scenario_meta<R: Read>(input: &mut R) -> GenieResult<ScenarioMeta> {
    let size = input.read_i32::<LE>()? as usize;
    let offset = input.read_i32::<LE>()? as usize;
    let name = read_fixed_str(input, 255)?.ok_or(NO_NAME)?;
    let filename = read_fixed_str(input, 255)?.ok_or(NO_NAME)?;
    let mut padding = [0; 2];
    input.read_exact(&mut padding)?;

    Ok(ScenarioMeta {
        size,
        offset,
        name,
        filename,
    })
}

/// A campaign file containing scenario files.
#[derive(Debug, Clone)]
pub struct Campaign<R>
where
    R: Read + Seek,
{
    reader: R,
    header: CampaignHeader,
    entries: Vec<ScenarioMeta>,
}

impl<R> Campaign<R>
where
    R: Read + Seek,
{
    /// Create a campaign instance from a readable input.
    ///
    /// This immediately reads the campaign header and scenario metadata, but not the scenario
    /// files themselves.
    ///
    /// # Errors
    ///
    /// Returns an error if reading fails, if a string cannot be decoded, or if the campaign or a
    /// scenario has no name.
    pub fn read_from(mut input: R) -> GenieResult<Self> {
        let header = read_campaign_header(&mut input)?;
        let mut entries = vec![];
        let read_entry = if header.version == *b"2.00" {
            read_scenario_meta_de2
        } else if header.version == *b"1.10" {
            read_scenario_meta_de
        } else {
            read_scenario_meta
        };
        for _ in 0..header.num_scenarios {
            entries.push(read_entry(&mut input)?);
        }

        Ok(Self {
            reader: input,
            header,
            entries,
        })
    }

    /// Consume this Campaign instance and get the reader.
    pub fn into_inner(self) -> R {
        self.reader
    }

    /// Get the campaign file version.
    pub const fn version(&self) -> CpxVersion {
        self.header.version
    }

    /// Get the user-facing name of this campaign.
    pub fn name(&self) -> &str {
        &self.header.name
    }

    /// Iterate over the scenario metadata for this campaign.
    pub fn entries(&self) -> impl Iterator<Item = &ScenarioMeta> {
        self.entries.iter()
    }

    /// Get the number of scenarios in this campaign.
    pub const fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if this campaign contains no scenario files.
    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get the user-facing name of the scenario at the given index.
    pub fn scenario_name(&self, id: usize) -> Option<&str> {
        self.entries.get(id).map(|entry| entry.name.as_ref())
    }

    /// Get the filename of the scenario at the given index.
    pub fn scenario_filename(&self, id: usize) -> Option<&str> {
        self.entries.get(id).map(|entry| entry.filename.as_ref())
    }

    /// Return the index of the scenario with the given filename, if it exists.
    fn index_of(&self, filename: &str) -> Option<usize> {
        self.entries
            .iter()
            .position(|entry| entry.filename == filename)
    }

    /// Get a scenario by its file name.
    ///
    /// # Errors
    ///
    /// Returns an error if no scenario has that file name, if reading fails, or if the scenario
    /// cannot be parsed.
    pub fn by_name(&mut self, filename: &str) -> GenieResult<Scenario> {
        let bytes = self.by_name_raw(filename)?;
        Scenario::read_from(Cursor::new(bytes)).map_err(|error| error.with("scenario", filename))
    }

    /// Get a scenario by its campaign index.
    ///
    /// # Errors
    ///
    /// Returns an error if the index is out of range, if reading fails, or if the scenario cannot
    /// be parsed.
    pub fn by_index(&mut self, index: usize) -> GenieResult<Scenario> {
        let bytes = self.by_index_raw(index)?;
        Scenario::read_from(Cursor::new(bytes)).map_err(|error| error.with("index", index))
    }

    /// Get a scenario file buffer by its file name.
    ///
    /// # Errors
    ///
    /// Returns an error if no scenario has that file name, or if reading fails.
    pub fn by_name_raw(&mut self, filename: &str) -> GenieResult<Vec<u8>> {
        let index = self.index_of(filename).ok_or_else(|| {
            GenieError::missing_data("a scenario the campaign does not hold")
                .with("scenario", filename)
        })?;
        self.by_index_raw(index)
    }

    /// Get a scenario file buffer by its campaign index.
    ///
    /// The buffer is shorter than the entry's size if the input ends early.
    ///
    /// # Errors
    ///
    /// Returns an error if the index is out of range, or if seeking or reading fails.
    pub fn by_index_raw(&mut self, index: usize) -> GenieResult<Vec<u8>> {
        let Some(entry) = self.entries.get(index) else {
            return Err(
                GenieError::missing_data("a scenario the campaign does not hold")
                    .with("index", index),
            );
        };

        let mut result = vec![];

        // The new position and the byte count are not needed: `result` holds the bytes.
        let _ = self.reader.seek(SeekFrom::Start(entry.offset as u64))?;
        let _ = self
            .reader
            .by_ref()
            .take(entry.size as u64)
            .read_to_end(&mut result)?;

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AOE_AOK, AOE1_DE, AOE2_DE};
    use anyhow::Context;
    use std::fs::File;

    /// A fixed string longer than the input fails at its end, not on its allocation.
    #[test]
    fn fixed_string_past_the_input_fails_at_its_end() {
        let mut input: &[u8] = b"short";
        assert!(read_fixed_str(&mut input, usize::MAX).is_err());
    }

    /// A scenario the campaign does not hold, by index or by name, is missing data.
    #[test]
    fn scenario_the_campaign_does_not_hold_is_missing() -> anyhow::Result<()> {
        let file = File::open("test/campaigns/Armies at War A Combat Showcase.cpn")?;
        let mut campaign = Campaign::read_from(file)?;
        for error in [
            campaign.by_index_raw(1).unwrap_err(),
            campaign.by_name_raw("none.scn").unwrap_err(),
        ] {
            assert_eq!(
                (error.kind(), error.reason()),
                (
                    genie_error::ErrorKind::MissingData,
                    "a scenario the campaign does not hold"
                )
            );
        }
        Ok(())
    }

    /// Try to parse a file with an encoding that is not compatible with UTF-8.
    /// Source: <http://aok.heavengames.com/blacksmith/showfile.php?fileid=884>
    #[test]
    fn detect_encoding() -> anyhow::Result<()> {
        let file = File::open("./test/campaigns/DER FALL VON SACSAHUAMAN - TEIL I.cpx")?;
        let cpx = Campaign::read_from(file)?;
        assert_eq!(cpx.version(), AOE_AOK);
        assert_eq!(cpx.name(), "DER FALL VON SACSAHUAMÁN - TEIL I");
        assert_eq!(cpx.len(), 1);

        let names: Vec<_> = cpx.entries().map(|e| &e.name).collect();
        assert_eq!(names, vec!["Der Weg nach Sacsahuamán"]);
        let filenames: Vec<_> = cpx
            .entries()
            .map(|scenario_meta| &scenario_meta.filename)
            .collect();
        assert_eq!(filenames, vec!["Der Weg nach Sacsahuamán.scx"]);
        Ok(())
    }

    /// Source: <http://aoe.heavengames.com/dl-php/showfile.php?fileid=1678>
    #[test]
    fn aoe1_trial_cpn() -> anyhow::Result<()> {
        let file = File::open("test/campaigns/Armies at War A Combat Showcase.cpn")?;
        let mut campaign = Campaign::read_from(file).context("could not read meta")?;

        assert_eq!(campaign.version(), AOE_AOK);
        assert_eq!(campaign.name(), "Armies at War, A Combat Showcase");
        assert_eq!(campaign.len(), 1);
        let names: Vec<_> = campaign.entries().map(|e| &e.name).collect();
        assert_eq!(names, vec!["Bronze Age Art of War"]);
        let filenames: Vec<_> = campaign
            .entries()
            .map(|scenario_meta| &scenario_meta.filename)
            .collect();
        assert_eq!(filenames, vec!["Bronze Age Art of War.scn"]);

        let by_index = campaign
            .by_index_raw(0)
            .context("could not read raw file")?;
        let by_name = campaign
            .by_name_raw("Bronze Age Art of War.scn")
            .context("could not read raw file")?;
        assert_eq!(by_index, by_name);
        Ok(())
    }

    #[test]
    fn aoe1_beta_cpn() -> anyhow::Result<()> {
        let file = File::open("test/campaigns/Rise of Egypt Learning Campaign.cpn")?;
        let campaign = Campaign::read_from(file).context("could not read meta")?;

        assert_eq!(campaign.version(), AOE_AOK);
        assert_eq!(campaign.name(), "Rise of Egypt Learning Campaign");
        assert_eq!(campaign.len(), 12);
        let filenames: Vec<_> = campaign
            .entries()
            .map(|scenario_meta| &scenario_meta.filename)
            .collect();
        assert_eq!(
            filenames,
            vec![
                "HUNTING.scn",
                "FORAGING.scn",
                "Discoveries.scn",
                "Dawn of a New Age.scn",
                "SKIRMISH.scn",
                "Lands Unknown.scn",
                "FARMING.scn",
                "TRADE.scn",
                "CRUSADE.scn",
                "Establish a Second Colony.scn",
                "Naval Battle.scn",
                "Siege Battle.scn",
            ]
        );
        Ok(())
    }

    #[test]
    fn aoe_de() -> anyhow::Result<()> {
        let file = File::open("test/campaigns/10 The First Punic War.aoecpn")?;
        let campaign = Campaign::read_from(file)?;

        assert_eq!(campaign.version(), AOE1_DE);
        assert_eq!(campaign.name(), "10 The First Punic War");
        assert_eq!(campaign.len(), 3);
        let filenames: Vec<_> = campaign
            .entries()
            .map(|scenario_meta| &scenario_meta.filename)
            .collect();
        assert_eq!(
            filenames,
            vec![
                "Scxt1-01-The Battle of Agrigentum.aoescn",
                "Scxt1-02-The Battle of Mylae.aoescn",
                "Scxt1-03-The Battle of Tunis.aoescn",
            ]
        );

        /* Enable when genie_scx supports DE1 scenarios better
        let mut c = c;
        for i in 0..c.len() {
            let _scen = c.by_index(i)?;
        }
        */

        Ok(())
    }

    #[test]
    fn aoe_de2() -> GenieResult<()> {
        let f = File::open("test/campaigns/acam1.aoe2campaign")?;
        let c = Campaign::read_from(f)?;

        assert_eq!(c.version(), AOE2_DE);
        assert_eq!(c.name(), "acam1");
        assert_eq!(c.len(), 5);
        let filenames: Vec<_> = c.entries().map(|e| &e.filename).collect();
        assert_eq!(
            filenames,
            vec![
                "A1_Tariq1.aoe2scenario",
                "A1_Tariq2.aoe2scenario",
                "A1_Tariq3.aoe2scenario",
                "A1_Tariq4.aoe2scenario",
                "A1_Tariq5.aoe2scenario",
            ]
        );

        let file = File::open("test/campaigns/rcam3.aoe2campaign")?;
        let campaign = Campaign::read_from(file)?;

        assert_eq!(campaign.version(), AOE2_DE);
        assert_eq!(campaign.name(), "rcam3");
        assert_eq!(campaign.len(), 5);
        let filenames: Vec<_> = campaign
            .entries()
            .map(|scenario_meta| &scenario_meta.filename)
            .collect();
        assert_eq!(
            filenames,
            vec![
                "R3_Bayinnaung_1.aoe2scenario",
                "R3_Bayinnaung_2.aoe2scenario",
                "R3_Bayinnaung_3.aoe2scenario",
                "R3_Bayinnaung_4.aoe2scenario",
                "R3_Bayinnaung_5.aoe2scenario",
            ]
        );

        /* Enable when genie_scx supports DE2 scenarios better
        let mut c = c;
        for i in 0..c.len() {
            let _scen = c.by_index(i)?;
        }
        */

        Ok(())
    }
}
