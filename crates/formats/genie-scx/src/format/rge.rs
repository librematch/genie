//! The RGE part of the scenario data ([`RgeScen`]), read and written.

use crate::Result;
use crate::bitmap::Bitmap;
use crate::format::tribe_read::read_separator;
use crate::format::write_opt_string_key;
use crate::player::{PlayerBaseProperties, PlayerFiles};
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use genie_support::{ReadStringsExt, StringKey, read_opt_u32, write_opt_str, write_str};
use std::io::{Read, Write};

#[derive(Debug, Default, Clone)]
pub(crate) struct RgeScen {
    /// Data version.
    pub(crate) version: f32,
    /// Names for each player.
    pub(super) player_names: Vec<Option<String>>,
    /// Name IDs for each player.
    pub(super) player_string_table: Vec<Option<StringKey>>,
    pub(super) player_base_properties: Vec<PlayerBaseProperties>,
    pub(super) victory_conquest: bool,
    /// File name of this scenario.
    pub(crate) name: String,
    pub(super) description_string_table: Option<StringKey>,
    pub(super) hints_string_table: Option<StringKey>,
    pub(super) win_message_string_table: Option<StringKey>,
    pub(super) loss_message_string_table: Option<StringKey>,
    pub(super) history_string_table: Option<StringKey>,
    pub(super) scout_string_table: Option<StringKey>,
    pub(super) description: Option<String>,
    pub(super) hints: Option<String>,
    pub(super) win_message: Option<String>,
    pub(super) loss_message: Option<String>,
    pub(super) history: Option<String>,
    pub(super) scout: Option<String>,
    pub(super) pregame_cinematic: Option<String>,
    pub(super) victory_cinematic: Option<String>,
    pub(super) loss_cinematic: Option<String>,
    #[allow(dead_code)]
    pub(super) mission_bmp: Option<String>,
    pub(super) player_build_lists: Vec<Option<String>>,
    pub(super) player_city_plans: Vec<Option<String>>,
    pub(super) player_ai_rules: Vec<Option<String>>,
    pub(super) player_files: Vec<PlayerFiles>,
    pub(super) ai_rules_kinds: Vec<i8>,
}

impl RgeScen {
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        let version = input.read_f32::<LE>()?;
        log::debug!("RGEScen version {version}");
        let mut scen = Self {
            version,
            ..Self::default()
        };
        scen.read_players(&mut input)?;
        scen.victory_conquest = read_timeline(&mut input, version)?;
        // File name may be empty for embedded scenario data inside recorded games.
        scen.name = input.read_u16_length_prefixed_str()?.unwrap_or_default();
        scen.read_string_tables(&mut input)?;
        scen.read_texts(&mut input)?;
        scen.read_cinematics(&mut input)?;
        scen.read_player_files(&mut input)?;
        read_separator(&mut input, version)?;
        Ok(scen)
    }

    /// The players' names after 1.13, string ids after 1.16 and base properties after 1.13;
    /// 16 of each, none or defaults before.
    fn read_players(&mut self, mut input: impl Read) -> Result<()> {
        let version = self.version;
        self.player_names = vec![None; 16];
        if version > 1.13 {
            for name in &mut self.player_names {
                *name = input.read_str(256)?;
            }
        }
        self.player_string_table = vec![None; 16];
        if version > 1.16 {
            for string_id in &mut self.player_string_table {
                *string_id = read_opt_u32(&mut input)?;
            }
        }
        self.player_base_properties = vec![PlayerBaseProperties::default(); 16];
        if version > 1.13 {
            for properties in &mut self.player_base_properties {
                properties.active = input.read_i32::<LE>()?;
                properties.player_kind = input.read_i32::<LE>()?;
                properties.civilization = input.read_i32::<LE>()?;
                properties.posture = input.read_i32::<LE>()?;
            }
        }
        Ok(())
    }

    /// The string ids of the description, hints, win and loss messages and history from 1.16
    /// on, of the scout from 1.22 on.
    fn read_string_tables(&mut self, mut input: impl Read) -> Result<()> {
        if self.version >= 1.16 {
            self.description_string_table = read_opt_u32(&mut input)?;
            self.hints_string_table = read_opt_u32(&mut input)?;
            self.win_message_string_table = read_opt_u32(&mut input)?;
            self.loss_message_string_table = read_opt_u32(&mut input)?;
            self.history_string_table = read_opt_u32(&mut input)?;
        }
        if self.version >= 1.22 {
            self.scout_string_table = read_opt_u32(&mut input)?;
        }
        Ok(())
    }

    /// The description; the hints, win and loss messages and history from 1.11 on; the scout
    /// from 1.22 on.
    fn read_texts(&mut self, mut input: impl Read) -> Result<()> {
        self.description = input.read_u16_length_prefixed_str()?;
        if self.version >= 1.11 {
            self.hints = input.read_u16_length_prefixed_str()?;
            self.win_message = input.read_u16_length_prefixed_str()?;
            self.loss_message = input.read_u16_length_prefixed_str()?;
            self.history = input.read_u16_length_prefixed_str()?;
        }
        if self.version >= 1.22 {
            self.scout = input.read_u16_length_prefixed_str()?;
        }
        Ok(())
    }

    /// The pregame, victory and loss cinematics, the mission bitmap's name from 1.09 on and the
    /// mission picture from 1.10 on (read past).
    fn read_cinematics(&mut self, mut input: impl Read) -> Result<()> {
        self.pregame_cinematic = input.read_u16_length_prefixed_str()?;
        self.victory_cinematic = input.read_u16_length_prefixed_str()?;
        self.loss_cinematic = input.read_u16_length_prefixed_str()?;
        if self.version >= 1.09 {
            self.mission_bmp = input.read_u16_length_prefixed_str()?;
        }
        if self.version >= 1.10 {
            let _mission_picture = Bitmap::read_from(&mut input)?;
        }
        Ok(())
    }

    /// Each player's build list, city plan and AI rules (from 1.08 on) names, the embedded
    /// files and the AI rules types from 1.20 on.
    // The format stores the embedded file lengths as i32; valid files never make them negative.
    #[allow(clippy::cast_sign_loss)]
    fn read_player_files(&mut self, mut input: impl Read) -> Result<()> {
        let version = self.version;
        self.player_build_lists = vec![None; 16];
        for build_list in &mut self.player_build_lists {
            *build_list = input.read_u16_length_prefixed_str()?;
        }
        self.player_city_plans = vec![None; 16];
        for city_plan in &mut self.player_city_plans {
            *city_plan = input.read_u16_length_prefixed_str()?;
        }
        self.player_ai_rules = vec![None; 16];
        if version >= 1.08 {
            for ai_rules in &mut self.player_ai_rules {
                *ai_rules = input.read_u16_length_prefixed_str()?;
            }
        }
        self.player_files = vec![PlayerFiles::default(); 16];
        for files in &mut self.player_files {
            let build_list_length = input.read_i32::<LE>()? as usize;
            let city_plan_length = input.read_i32::<LE>()? as usize;
            let ai_rules_length = if version >= 1.08 {
                input.read_i32::<LE>()? as usize
            } else {
                0
            };
            files.build_list = input.read_str(build_list_length)?;
            files.city_plan = input.read_str(city_plan_length)?;
            files.ai_rules = input.read_str(ai_rules_length)?;
        }
        self.ai_rules_kinds = vec![0; 16];
        if version >= 1.20 {
            for rule_kind in &mut self.ai_rules_kinds {
                *rule_kind = input.read_i8()?;
            }
        }
        Ok(())
    }

    pub fn write_to(&self, mut output: impl Write, version: f32) -> Result<()> {
        output.write_f32::<LE>(version)?;
        self.write_players(&mut output, version)?;
        self.write_timeline(&mut output, version)?;
        write_str(&mut output, &self.name)?;
        self.write_string_tables(&mut output, version)?;
        self.write_texts(&mut output, version)?;
        self.write_cinematics(&mut output, version)?;
        self.write_player_files(&mut output, version)?;
        output.write_i32::<LE>(-99)?;
        Ok(())
    }

    /// The players' names padded to 256 bytes after 1.13, string ids after 1.16 and base
    /// properties after 1.13.
    fn write_players(&self, mut output: impl Write, version: f32) -> Result<()> {
        if version > 1.13 {
            assert_eq!(self.player_names.len(), 16);
            for name in &self.player_names {
                let mut padded_bytes = Vec::with_capacity(256);
                if let Some(name) = name {
                    let name_bytes = name.as_bytes();
                    padded_bytes.write_all(name_bytes)?;
                }
                padded_bytes.extend(vec![0; 256 - padded_bytes.len()]);
                output.write_all(&padded_bytes)?;
            }
        }
        if version > 1.16 {
            assert_eq!(self.player_string_table.len(), 16);
            for id in &self.player_string_table {
                write_opt_string_key(&mut output, id.as_ref())?;
            }
        }
        if version > 1.13 {
            assert_eq!(self.player_base_properties.len(), 16);
            for props in &self.player_base_properties {
                output.write_i32::<LE>(props.active)?;
                output.write_i32::<LE>(props.player_kind)?;
                output.write_i32::<LE>(props.civilization)?;
                output.write_i32::<LE>(props.posture)?;
            }
        }
        Ok(())
    }

    /// The conquest victory from 1.07 on, an empty timeline, the civilization locks (none) from
    /// 1.28 on.
    fn write_timeline(&self, mut output: impl Write, version: f32) -> Result<()> {
        if version >= 1.07 {
            output.write_u8(u8::from(self.victory_conquest))?;
        }
        // RGE_Timeline
        output.write_i16::<LE>(0)?;
        output.write_i16::<LE>(0)?;
        output.write_f32::<LE>(-1.0)?;
        if version >= 1.28 {
            // Civ Lock data
            for _ in 0..16 {
                output.write_u32::<LE>(0)?;
            }
        }
        Ok(())
    }

    /// The texts' string ids from 1.16 on, the scout's from 1.22 on.
    fn write_string_tables(&self, mut output: impl Write, version: f32) -> Result<()> {
        if version >= 1.16 {
            write_opt_string_key(&mut output, self.description_string_table.as_ref())?;
            write_opt_string_key(&mut output, self.hints_string_table.as_ref())?;
            write_opt_string_key(&mut output, self.win_message_string_table.as_ref())?;
            write_opt_string_key(&mut output, self.loss_message_string_table.as_ref())?;
            write_opt_string_key(&mut output, self.history_string_table.as_ref())?;
        }
        if version >= 1.22 {
            write_opt_string_key(&mut output, self.scout_string_table.as_ref())?;
        }
        Ok(())
    }

    /// The description; the hints, win and loss messages and history from 1.11 on; the scout
    /// from 1.22 on.
    fn write_texts(&self, mut output: impl Write, version: f32) -> Result<()> {
        write_opt_str(&mut output, &self.description)?;
        if version >= 1.11 {
            write_opt_str(&mut output, &self.hints)?;
            write_opt_str(&mut output, &self.win_message)?;
            write_opt_str(&mut output, &self.loss_message)?;
            write_opt_str(&mut output, &self.history)?;
        }
        if version >= 1.22 {
            write_opt_str(&mut output, &self.scout)?;
        }
        Ok(())
    }

    /// The cinematics, no mission bitmap from 1.09 on and an empty mission picture from 1.10 on.
    fn write_cinematics(&self, mut output: impl Write, version: f32) -> Result<()> {
        write_opt_str(&mut output, &self.pregame_cinematic)?;
        write_opt_str(&mut output, &self.victory_cinematic)?;
        write_opt_str(&mut output, &self.loss_cinematic)?;
        if version >= 1.09 {
            // mission_bmp
            write_opt_str(&mut output, &None)?;
        }
        if version >= 1.10 {
            // mission_picture
            output.write_u32::<LE>(0)?;
            output.write_u32::<LE>(0)?;
            output.write_u32::<LE>(0)?;
            output.write_u16::<LE>(1)?;
        }
        Ok(())
    }

    /// Each player's build list, city plan and AI rules (from 1.08 on) names, the embedded
    /// files ([`write_files`]) and the AI rules types from 1.20 on.
    fn write_player_files(&self, mut output: impl Write, version: f32) -> Result<()> {
        assert_eq!(self.player_build_lists.len(), 16);
        for build_list in &self.player_build_lists {
            write_opt_str(&mut output, build_list)?;
        }
        assert_eq!(self.player_city_plans.len(), 16);
        for city_plan in &self.player_city_plans {
            write_opt_str(&mut output, city_plan)?;
        }
        if version >= 1.08 {
            assert_eq!(self.player_ai_rules.len(), 16);
            for ai_rules in &self.player_ai_rules {
                write_opt_str(&mut output, ai_rules)?;
            }
        }
        assert_eq!(self.player_files.len(), 16);
        for files in &self.player_files {
            write_files(&mut output, files, version)?;
        }
        if version >= 1.20 {
            assert_eq!(self.ai_rules_kinds.len(), 16);
            for ai_rules_kind in &self.ai_rules_kinds {
                output.write_i8(*ai_rules_kind)?;
            }
        }
        Ok(())
    }
}

/// The conquest victory from 1.07 on (else on), the timeline (none) and the civilization locks
/// from 1.28 on.
///
/// # Panics
///
/// Panics on a timeline with entries.
fn read_timeline(mut input: impl Read, version: f32) -> Result<bool> {
    let victory_conquest = if version >= 1.07 {
        input.read_u8()? != 0
    } else {
        true
    };

    dbg!(victory_conquest);

    {
        let timeline_count = input.read_i16::<LE>()?;
        let timeline_available = input.read_i16::<LE>()?;
        let old_time = input.read_f32::<LE>()?;
        dbg!(timeline_count, timeline_available, old_time);
        assert_eq!(timeline_count, 0, "Unexpected RGE_Timeline");
        // assert_eq!(timeline_available, 0, "Unexpected RGE_Timeline");
    }

    if version >= 1.28 {
        let civ_lock = &mut [0; 16];
        input.read_u32_into::<LE>(civ_lock)?;
    }
    Ok(victory_conquest)
}

/// A player's embedded files: the lengths of the build list, city plan and AI rules (from
/// 1.08 on), then their bytes.
// The format stores the embedded file lengths as u32.
#[allow(clippy::cast_possible_truncation)]
fn write_files(mut output: impl Write, files: &PlayerFiles, version: f32) -> Result<()> {
    if let Some(build_list) = &files.build_list {
        output.write_u32::<LE>(build_list.len() as u32)?;
    } else {
        output.write_u32::<LE>(0)?;
    }
    if let Some(city_plan) = &files.city_plan {
        output.write_u32::<LE>(city_plan.len() as u32)?;
    } else {
        output.write_u32::<LE>(0)?;
    }
    if version >= 1.08 {
        if let Some(ai_rules) = &files.ai_rules {
            output.write_u32::<LE>(ai_rules.len() as u32)?;
        } else {
            output.write_u32::<LE>(0)?;
        }
    }
    if let Some(build_list) = &files.build_list {
        output.write_all(build_list.as_bytes())?;
    }
    if let Some(city_plan) = &files.city_plan {
        output.write_all(city_plan.as_bytes())?;
    }
    if version >= 1.08
        && let Some(ai_rules) = &files.ai_rules
    {
        output.write_all(ai_rules.as_bytes())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scenario data of `version` with 16 players' lists, each named and with its files.
    fn scenario(version: f32) -> RgeScen {
        let named = |what: &str| (0..16).map(|i| Some(format!("{what}{i}"))).collect();
        RgeScen {
            version,
            player_names: named("player"),
            player_string_table: (0..16u32).map(|i| Some(StringKey::from(i * 10))).collect(),
            player_base_properties: (0..16)
                .map(|i| PlayerBaseProperties {
                    posture: i,
                    player_kind: 2,
                    civilization: i + 1,
                    active: 1,
                })
                .collect(),
            victory_conquest: false,
            description_string_table: Some(StringKey::from(1u32)),
            history_string_table: Some(StringKey::from(5u32)),
            scout_string_table: Some(StringKey::from(6u32)),
            description: Some("about".to_owned()),
            hints: Some("hint".to_owned()),
            history: Some("past".to_owned()),
            scout: Some("scout".to_owned()),
            pregame_cinematic: Some("intro".to_owned()),
            player_build_lists: named("build"),
            player_city_plans: named("plan"),
            player_ai_rules: named("rules"),
            player_files: (0..16)
                .map(|i| PlayerFiles {
                    build_list: Some(format!("b{i}")),
                    city_plan: None,
                    ai_rules: Some(format!("(defrule {i})")),
                })
                .collect(),
            ai_rules_kinds: (0..16).collect(),
            ..RgeScen::default()
        }
    }

    /// What `write` puts out, read back by `read` into a scenario of `version`.
    fn round_trip(
        version: f32,
        write: impl FnOnce(&mut Vec<u8>) -> Result<()>,
        read: impl FnOnce(&mut RgeScen, &mut &[u8]) -> Result<()>,
    ) -> RgeScen {
        let mut bytes = vec![];
        write(&mut bytes).unwrap();
        let mut back = RgeScen {
            version,
            ..RgeScen::default()
        };
        let mut input = bytes.as_slice();
        read(&mut back, &mut input).unwrap();
        assert!(input.is_empty(), "{version}: {} bytes left", input.len());
        back
    }

    /// The players' names and base properties after 1.13, their string ids after 1.16; each
    /// list has 16 entries, none or defaults before.
    #[test]
    fn the_players_follow_their_version() {
        for version in [1.13, 1.16, 1.17] {
            let scen = scenario(version);
            let back = round_trip(
                version,
                |o| scen.write_players(o, version),
                |b, i| b.read_players(i),
            );
            let named = 1.13 < version;
            assert_eq!(back.player_names[3].is_some(), named, "{version}");
            assert_eq!(
                back.player_base_properties[3].civilization,
                if named { 4 } else { 0 }
            );
            assert_eq!(back.player_string_table[3].is_some(), 1.16 < version);
            assert_eq!(back.player_names.len(), 16);
        }
    }

    /// The conquest victory from 1.07 on (on before), the empty timeline, the civilization
    /// locks from 1.28 on.
    #[test]
    fn the_timeline_reads_the_conquest_victory_and_the_locks() {
        let scen = scenario(1.28);
        for (version, len, conquest) in [(1.06, 8, true), (1.07, 9, false), (1.28, 73, false)] {
            let mut bytes = vec![];
            scen.write_timeline(&mut bytes, version).unwrap();
            assert_eq!(bytes.len(), len);
            let mut input = bytes.as_slice();
            assert_eq!(read_timeline(&mut input, version).unwrap(), conquest);
            assert_eq!(input, []);
        }
    }

    /// A timeline with entries is not read.
    #[test]
    #[should_panic(expected = "Unexpected RGE_Timeline")]
    fn timeline_with_entries_panics() {
        let bytes = [1u8, 0, 0, 0, 0, 0, 0, 0];
        let _ = read_timeline(bytes.as_slice(), 1.06);
    }

    /// The texts' string ids from 1.16 on (the scout's from 1.22 on), the description, the
    /// hints to history from 1.11 on and the scout from 1.22 on.
    #[test]
    fn the_texts_follow_their_version() {
        for version in [1.10, 1.11, 1.16, 1.22] {
            let scen = scenario(version);
            let back = round_trip(
                version,
                |o| {
                    scen.write_string_tables(&mut *o, version)?;
                    scen.write_texts(o, version)
                },
                |b, i| {
                    b.read_string_tables(&mut *i)?;
                    b.read_texts(i)
                },
            );
            assert_eq!(back.description.as_deref(), Some("about"));
            assert_eq!(back.hints.is_some(), 1.11 <= version, "{version}");
            assert_eq!(back.history_string_table.is_some(), 1.16 <= version);
            assert_eq!(back.scout_string_table.is_some(), 1.22 <= version);
            assert_eq!(back.scout.is_some(), 1.22 <= version);
        }
    }

    /// The cinematics, the mission bitmap's name from 1.09 on (written as none), the empty
    /// mission picture from 1.10 on.
    #[test]
    fn the_cinematics_follow_their_version() {
        for (version, len) in [(1.08, 6 + 6), (1.09, 6 + 8), (1.10, 6 + 8 + 14)] {
            let scen = scenario(version);
            let mut bytes = vec![];
            scen.write_cinematics(&mut bytes, version).unwrap();
            assert_eq!(bytes.len(), len, "{version}");
            let back = round_trip(
                version,
                |o| scen.write_cinematics(o, version),
                |b, i| b.read_cinematics(i),
            );
            assert_eq!(back.pregame_cinematic.as_deref(), Some("intro"));
            assert_eq!(back.mission_bmp, None);
        }
    }

    /// The build lists, city plans, the AI rules' names from 1.08 on, each player's files
    /// (the AI rules from 1.08 on) and the AI rules types from 1.20 on.
    #[test]
    fn the_player_files_follow_their_version() {
        for version in [1.07, 1.08, 1.20] {
            let scen = scenario(version);
            let back = round_trip(
                version,
                |o| scen.write_player_files(o, version),
                |b, i| b.read_player_files(i),
            );
            assert_eq!(back.player_build_lists[2].as_deref(), Some("build2"));
            assert_eq!(back.player_files[2].build_list.as_deref(), Some("b2"));
            assert_eq!(back.player_files[2].city_plan, None);
            let rules = 1.08 <= version;
            assert_eq!(back.player_ai_rules[2].is_some(), rules, "{version}");
            assert_eq!(back.player_files[2].ai_rules.is_some(), rules);
            assert_eq!(back.ai_rules_kinds[2], if 1.20 <= version { 2 } else { 0 });
        }
    }

    /// A whole RGE part written and read back.
    #[test]
    fn the_rge_part_reads_back_what_it_wrote() {
        let scen = RgeScen {
            name: "map.scx".to_owned(),
            ..scenario(1.22)
        };
        let mut bytes = vec![];
        scen.write_to(&mut bytes, 1.22).unwrap();
        let back = RgeScen::read_from(bytes.as_slice()).unwrap();
        assert_eq!((back.version, back.name.as_str()), (1.22, "map.scx"));
        assert_eq!(back.player_names[15].as_deref(), Some("player15"));
    }
}
