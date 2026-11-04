//! A whole scenario file ([`ScxFormat`]): its version, its load, its write and its hash.

use crate::VersionBundle;
use crate::ai::AiInfo;
use crate::errors::unknown_version;
use crate::format::{ScenarioObject, TribeScen};
use crate::header::ScxHeader;
use crate::map::Map;
use crate::player::{ScenarioPlayerData, WorldPlayerData};
use crate::triggers::TriggerSystem;
use crate::values::ScxVersion;
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use flate2::Compression;
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use genie_error::GenieResult;
use genie_support::reserved;
use std::io::{Read, Write};

#[derive(Debug, Clone)]
pub struct ScxFormat {
    /// Version of the SCX format.
    pub(crate) version: ScxVersion,
    /// Uncompressed header containing metadata for display.
    pub(crate) header: ScxHeader,
    /// ID for the next-placed/created object.
    pub(crate) next_object_id: i32,
    /// Scenario data.
    pub(crate) tribe_scen: TribeScen,
    /// Map data.
    pub(crate) map: Map,
    /// Player data.
    pub(crate) world_players: Vec<WorldPlayerData>,
    /// Objects data.
    pub(crate) player_objects: Vec<Vec<ScenarioObject>>,
    /// Player data.
    pub(crate) scenario_players: Vec<ScenarioPlayerData>,
    /// Triggers (only in `AoK` and up).
    pub(crate) triggers: Option<TriggerSystem>,
    /// AI information (`AoK` and up).
    pub(super) ai_info: Option<AiInfo>,
}

impl ScxFormat {
    /// Extract version bundle information from a parsed SCX file.
    pub fn version(&self) -> VersionBundle {
        VersionBundle {
            format: self.version,
            header: self.header.version,
            data: self.tribe_scen.version(),
            triggers: self.triggers.as_ref().map(TriggerSystem::version),
            map: self.map.version(),
            ..VersionBundle::aoc()
        }
    }

    pub(super) fn load_inner(
        version: ScxVersion,
        player_version: f32,
        mut input: impl Read,
    ) -> GenieResult<Self> {
        fn read_scenario_players(
            mut input: impl Read,
            player_version: f32,
        ) -> GenieResult<Vec<ScenarioPlayerData>> {
            let num = input.read_u32::<LE>()?;
            let mut players = reserved(num as usize);
            log::debug!("number of scenario players: {num}");
            for _ in 1..num {
                players.push(ScenarioPlayerData::read_from(&mut input, player_version)?);
            }
            Ok(players)
        }

        fn read_player_objects(
            mut input: impl Read,
            num_players: u32,
            version: ScxVersion,
        ) -> GenieResult<Vec<Vec<ScenarioObject>>> {
            let mut player_objects = reserved(num_players as usize);
            for _ in 0..num_players {
                let num_objects = input.read_u32::<LE>()?;
                let mut objects = reserved(num_objects as usize);
                log::debug!("number of objects: {num_objects}");
                for _ in 0..num_objects {
                    objects.push(ScenarioObject::read_from(&mut input, version)?);
                }
                player_objects.push(objects);
            }
            Ok(player_objects)
        }

        let header = ScxHeader::read_from(&mut input, version)?;

        let mut input = DeflateDecoder::new(&mut input);
        let next_object_id = input.read_i32::<LE>()?;

        let tribe_scen = TribeScen::read_from(&mut input)?;

        let map = Map::read_from(&mut input)?;

        let num_players = input.read_u32::<LE>()?;
        log::debug!("number of players: {num_players}");
        let mut world_players = reserved(num_players as usize);
        for _ in 1..num_players {
            world_players.push(WorldPlayerData::read_from(&mut input, player_version)?);
        }

        // The order is flipped … thanks DE
        let (scenario_players, player_objects) = if version >= ScxVersion(*b"1.36") {
            let players = read_scenario_players(&mut input, player_version)?;
            let objects = read_player_objects(&mut input, num_players, version)?;
            (players, objects)
        } else {
            let objects = read_player_objects(&mut input, num_players, version)?;
            let players = read_scenario_players(&mut input, player_version)?;
            (players, objects)
        };

        let triggers = if version < ScxVersion(*b"1.14") {
            None
        } else {
            Some(TriggerSystem::read_from(&mut input)?)
        };

        let ai_info = if version > ScxVersion(*b"1.17") && version < ScxVersion(*b"2.00") {
            AiInfo::read_from(&mut input)?
        } else {
            None
        };

        Ok(Self {
            version,
            header,
            next_object_id,
            tribe_scen,
            map,
            world_players,
            player_objects,
            scenario_players,
            triggers,
            ai_info,
        })
    }

    pub fn load_scenario(mut input: impl Read) -> GenieResult<Self> {
        let mut format_version = [0; 4];
        input.read_exact(&mut format_version)?;
        let format_version = ScxVersion(format_version);
        let Some(player_version) = format_version.to_player_version() else {
            return Err(unknown_version(format_version));
        };
        Self::load_inner(format_version, player_version, input)
    }

    // The format stores the object count as an i32.
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    pub(super) fn write_player_objects(
        &self,
        mut output: impl Write,
        format_version: ScxVersion,
    ) -> GenieResult<()> {
        for objects in &self.player_objects {
            output.write_i32::<LE>(objects.len() as i32)?;
            for object in objects {
                object.write_to(&mut output, format_version)?;
            }
        }
        Ok(())
    }

    // The format stores the player count as an i32.
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    pub(super) fn write_scenario_players(
        &self,
        mut output: impl Write,
        player_version: f32,
        victory_version: f32,
    ) -> GenieResult<()> {
        output.write_i32::<LE>(self.scenario_players.len() as i32 + 1)?;
        for player in &self.scenario_players {
            player.write_to(&mut output, player_version, victory_version)?;
        }
        Ok(())
    }

    // The format stores the player count as an i32.
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    pub fn write_to(&self, mut output: impl Write, version: &VersionBundle) -> GenieResult<()> {
        let Some(player_version) = version.format.to_player_version() else {
            return Err(unknown_version(version.format));
        };

        output.write_all(version.format.as_bytes())?;
        self.header
            .write_to(&mut output, version.format, version.header)?;

        let mut output = DeflateEncoder::new(output, Compression::default());
        output.write_i32::<LE>(self.next_object_id)?;

        let num_triggers = self
            .triggers
            .as_ref()
            .map_or(0, TriggerSystem::num_triggers);
        self.tribe_scen
            .write_to(&mut output, version.data, num_triggers)?;
        self.map.write_to(&mut output, version.map)?;

        output.write_i32::<LE>(self.player_objects.len() as i32)?;
        for player in &self.world_players {
            player.write_to(&mut output, player_version)?;
        }

        if version.format >= ScxVersion(*b"1.36") {
            self.write_scenario_players(&mut output, player_version, version.victory)?;
            self.write_player_objects(&mut output, version.format)?;
        } else {
            self.write_player_objects(&mut output, version.format)?;
            self.write_scenario_players(&mut output, player_version, version.victory)?;
        }

        if version.format > ScxVersion(*b"1.13") {
            let def = TriggerSystem::default();
            let triggers = self.triggers.as_ref().unwrap_or(&def);
            triggers.write_to(&mut output, version.triggers.unwrap_or(1.6))?;
        }

        if version.format > ScxVersion(*b"1.17") && version.format < ScxVersion(*b"2.00") {
            let def = AiInfo::default();
            let ai_info = self.ai_info.as_ref().unwrap_or(&def);
            ai_info.write_to(&mut output)?;
        }

        // The inner writer is not needed once the compressed stream is finished.
        let _ = output.finish()?;

        Ok(())
    }

    /// Get the name of the `UserPatch` mod that was used to create this scenario, if applicable.
    ///
    /// Returns None if no mod was used.
    pub fn mod_name(&self) -> Option<&str> {
        self.tribe_scen.base.player_names[9].as_deref()
    }

    /// Hash the scenario, for comparison with other instances.
    ///
    /// This is only available in tests and the implementation is horrifying :)
    #[cfg(test)]
    pub fn hash(&self) -> u64 {
        use std::{
            collections::hash_map::DefaultHasher,
            hash::{Hash, Hasher},
        };
        let mut hasher = DefaultHasher::new();
        format!("{self:#?}").hash(&mut hasher);
        hasher.finish()
    }
}
