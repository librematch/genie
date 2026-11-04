//! The embeddable scenario data ([`TribeScen`]) and its reader.

use crate::Result;
use crate::format::RgeScen;
use crate::player::PlayerStartResources;
use crate::values::{DiplomaticStance, StartingAge};
use crate::victory::{LegacyVictoryInfo, VictoryInfo};
use byteorder::{LE, ReadBytesExt};
use genie_support::{ReadStringsExt, f32_eq};
use std::io::Read;

/// Embeddable scenario data. This includes all scenario settings, but not map data, triggers, and
/// placed objects.
///
/// The game saves this structure in scenario files, and also in saved and recorded game files.
// Each bool is a setting the format stores.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Default, Clone)]
pub struct TribeScen {
    /// "Engine" data.
    ///
    /// This distinction doesn't make much sense as a user of this library, but
    /// it exists internally in `AoC` and affects the storage format (eg.  some
    /// fields are stored twice).
    pub(crate) base: RgeScen,
    /// Starting resources for players.
    pub(super) player_start_resources: Vec<PlayerStartResources>,
    /// Victory settings.
    pub(super) victory: VictoryInfo,
    /// Whether all victory conditions need to be met for victory to occur.
    pub(super) victory_all_flag: bool,
    /// Type of victory condition to use in multiplayer games.
    pub(super) mp_victory_kind: i32,
    /// Required score to attain multiplayer victory.
    pub(super) victory_score: i32,
    /// Time at which the highest-scoring player will win the multiplayer match.
    pub(super) victory_time: i32,
    /// Initial diplomacy stances between players.
    pub(super) diplomacy: Vec<Vec<DiplomaticStance>>,
    pub(super) legacy_victory_info: Vec<Vec<LegacyVictoryInfo>>,
    /// Whether Allied Victory is enabled for each player.
    pub(super) allied_victory: Vec<i32>,
    pub(super) teams_locked: bool,
    pub(super) can_change_teams: bool,
    pub(super) random_start_locations: bool,
    pub(super) max_teams: u8,
    /// Number of disabled techs per player.
    ///
    /// TODO only use `disabled_techs` for this
    pub(super) num_disabled_techs: Vec<i32>,
    /// Disabled tech IDs per player.
    pub(super) disabled_techs: Vec<Vec<i32>>,
    /// Number of disabled units per player.
    ///
    /// TODO only use `disabled_units` for this
    pub(super) num_disabled_units: Vec<i32>,
    /// Disabled unit IDs per player.
    pub(super) disabled_units: Vec<Vec<i32>>,
    /// Number of disabled buildings per player.
    ///
    /// TODO only use `disabled_buildings` for this
    pub(super) num_disabled_buildings: Vec<i32>,
    /// Disabled building IDs per player.
    pub(super) disabled_buildings: Vec<Vec<i32>>,
    /// (What exactly?)
    ///
    /// According to [`AoE2ScenarioParser`][].
    ///
    /// [`AoE2ScenarioParser`]: <https://github.com/KSneijders/AoE2ScenarioParser/blob/8e3abd422164961aa5c7857350475088790804f8/AoE2ScenarioParser/pieces/options.py>
    #[allow(dead_code)]
    pub(super) combat_mode: i32,
    /// (What exactly?)
    ///
    /// According to [`AoE2ScenarioParser`][].
    ///
    /// [`AoE2ScenarioParser`]: <https://github.com/KSneijders/AoE2ScenarioParser/blob/8e3abd422164961aa5c7857350475088790804f8/AoE2ScenarioParser/pieces/options.py>
    #[allow(dead_code)]
    pub(super) naval_mode: i32,
    /// Whether "All Techs" is enabled.
    pub(super) all_techs: bool,
    /// The starting age per player.
    pub(super) player_start_ages: Vec<StartingAge>,
    /// The initial camera location.
    pub(super) view: (i32, i32),
    /// The map type.
    pub(super) map_kind: Option<i32>,
    pub(super) base_priorities: Vec<i8>,
    /// The water definition type used. (DE2 and up)
    pub(super) water_definition: Option<String>,
    /// The colour mood used. (DE2 and up)
    pub(super) color_mood: Option<String>,
    /// Is collide-and-correct pathing enabled?
    ///
    /// Only supported for DE2 and up; defaults to `false` in earlier versions.
    pub(super) collide_and_correct: bool,
    /// Is villager force drop enabled?
    ///
    /// Only supported for DE2 and up; defaults to `false` in earlier versions.
    pub(super) villager_force_drop: bool,
}

impl TribeScen {
    /// Read scenario data from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading fails, or if a string cannot be decoded.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        let mut base = RgeScen::read_from(&mut input)?;
        let version = base.version;
        let player_start_resources = read_players(&mut input, &mut base)?;
        read_separator(&mut input, version)?;
        let victory = VictoryInfo::read_from(&mut input)?;
        let victory_all_flag = input.read_i32::<LE>()? != 0;
        let (mp_victory_kind, victory_score, victory_time) = read_mp_victory(&mut input, version)?;
        log::debug!("Victory values: {mp_victory_kind} {victory_score} {victory_time}");
        let diplomacy = read_diplomacy(&mut input)?;
        let legacy_victory_info = read_legacy_victory_info(&mut input)?;
        read_separator(&mut input, version)?;
        let mut allied_victory = vec![0i32; 16];
        for setting in &mut allied_victory {
            *setting = input.read_i32::<LE>()?;
        }
        let (teams_locked, can_change_teams, random_start_locations, max_teams) =
            read_teams(&mut input, version)?;
        let disabled = DisabledLists::read_from(&mut input, version)?;
        let (combat_mode, naval_mode, all_techs) = read_modes(&mut input, version)?;
        let player_start_ages = read_start_ages(&mut input, version)?;
        log::debug!("starting ages: {player_start_ages:?}");
        read_separator(&mut input, version)?;
        let view = read_view(&mut input, version)?;
        let map_kind = read_map_kind(&mut input, version)?;
        let base_priorities = read_base_priorities(&mut input, version)?;
        let options = DeOptions::read_from(&mut input, version)?;
        Ok(Self {
            base,
            player_start_resources,
            victory,
            victory_all_flag,
            mp_victory_kind,
            victory_score,
            victory_time,
            diplomacy,
            legacy_victory_info,
            allied_victory,
            teams_locked,
            can_change_teams,
            random_start_locations,
            max_teams,
            num_disabled_techs: disabled.num_techs,
            disabled_techs: disabled.techs,
            num_disabled_units: disabled.num_units,
            disabled_units: disabled.units,
            num_disabled_buildings: disabled.num_buildings,
            disabled_buildings: disabled.buildings,
            combat_mode,
            naval_mode,
            all_techs,
            player_start_ages,
            view,
            map_kind,
            base_priorities,
            water_definition: options.water_definition,
            color_mood: options.color_mood,
            collide_and_correct: options.collide_and_correct,
            villager_force_drop: options.villager_force_drop,
        })
    }
}

/// The separator -99 the versions from 1.02 on put between sections.
pub(super) fn read_separator(mut input: impl Read, version: f32) -> Result<()> {
    if version >= 1.02 {
        let sep = input.read_i32::<LE>()?;
        debug_assert_eq!(sep, -99);
    }
    Ok(())
}

/// The players' starting resources; up to 1.13 (before the base held them) each player's name,
/// then its activity, resources, type, civilization and posture, into `base`.
fn read_players(mut input: impl Read, base: &mut RgeScen) -> Result<Vec<PlayerStartResources>> {
    let version = base.version;
    let mut player_start_resources = vec![PlayerStartResources::default(); 16];
    // Moved to RGEScen in 1.13
    if version <= 1.13 {
        for name in &mut base.player_names {
            *name = input.read_str(256)?;
        }
        for (properties, start_resources) in base
            .player_base_properties
            .iter_mut()
            .zip(player_start_resources.iter_mut())
        {
            properties.active = input.read_i32::<LE>()?;
            *start_resources = PlayerStartResources::read_from(&mut input, version)?;
            properties.player_kind = input.read_i32::<LE>()?;
            properties.civilization = input.read_i32::<LE>()?;
            properties.posture = input.read_i32::<LE>()?;
        }
    } else {
        for resources in &mut player_start_resources {
            *resources = PlayerStartResources::read_from(&mut input, version)?;
        }
    }
    Ok(player_start_resources)
}

/// The multiplayer victory's type, score and time from 1.13 on; before, 4, 900 and 9000.
fn read_mp_victory(mut input: impl Read, version: f32) -> Result<(i32, i32, i32)> {
    if version >= 1.13 {
        Ok((
            input.read_i32::<LE>()?,
            input.read_i32::<LE>()?,
            input.read_i32::<LE>()?,
        ))
    } else {
        Ok((4, 900, 9000))
    }
}

/// Each player's stance toward each player, 16 by 16.
fn read_diplomacy(mut input: impl Read) -> Result<Vec<Vec<DiplomaticStance>>> {
    let mut diplomacy = vec![vec![DiplomaticStance::Neutral; 16]; 16];
    for player_diplomacy in &mut diplomacy {
        for stance in player_diplomacy.iter_mut() {
            *stance = DiplomaticStance::from(input.read_i32::<LE>()?);
        }
    }
    Ok(diplomacy)
}

/// Each player's 12 legacy victory entries.
fn read_legacy_victory_info(mut input: impl Read) -> Result<Vec<Vec<LegacyVictoryInfo>>> {
    let mut legacy_victory_info = vec![vec![LegacyVictoryInfo::default(); 12]; 16];
    for list in &mut legacy_victory_info {
        for victory_info in list.iter_mut() {
            *victory_info = LegacyVictoryInfo::read_from(&mut input)?;
        }
    }
    Ok(legacy_victory_info)
}

/// The teams' settings: locked, changeable, random start locations and the most teams from
/// 1.24 on; 1.23 stores only the lock; before, unlocked, changeable, random and 4.
fn read_teams(mut input: impl Read, version: f32) -> Result<(bool, bool, bool, u8)> {
    if version >= 1.24 {
        Ok((
            input.read_i8()? != 0,
            input.read_i8()? != 0,
            input.read_i8()? != 0,
            input.read_u8()?,
        ))
    } else if f32_eq!(version, 1.23) {
        Ok((input.read_i32::<LE>()? != 0, true, true, 4))
    } else {
        Ok((false, true, true, 4))
    }
}

/// The combat mode after 1.04, the naval mode and "All Techs" from 1.12 on; else 0, 0, off.
fn read_modes(mut input: impl Read, version: f32) -> Result<(i32, i32, bool)> {
    let combat_mode = if version > 1.04 {
        input.read_i32::<LE>()?
    } else {
        0
    };
    let (naval_mode, all_techs) = if version >= 1.12 {
        (input.read_i32::<LE>()?, input.read_i32::<LE>()? != 0)
    } else {
        (0, false)
    };
    Ok((combat_mode, naval_mode, all_techs))
}

/// Each player's starting age after 1.05; before, the game's default.
fn read_start_ages(mut input: impl Read, version: f32) -> Result<Vec<StartingAge>> {
    let mut player_start_ages = vec![StartingAge::Default; 16];
    if version > 1.05 {
        for start_age in &mut player_start_ages {
            *start_age = StartingAge::from_i32(input.read_i32::<LE>()?, version);
        }
    }
    Ok(player_start_ages)
}

/// The initial view from 1.19 on; (-1, -1) before.
fn read_view(mut input: impl Read, version: f32) -> Result<(i32, i32)> {
    if version >= 1.19 {
        Ok((input.read_i32::<LE>()?, input.read_i32::<LE>()?))
    } else {
        Ok((-1, -1))
    }
}

/// The map type from 1.21 on, -1 and -2 none.
fn read_map_kind(mut input: impl Read, version: f32) -> Result<Option<i32>> {
    if version >= 1.21 {
        match input.read_i32::<LE>()? {
            // HD Edition uses -2 instead of -1?
            -2 | -1 => Ok(None),
            id => Ok(Some(id)),
        }
    } else {
        Ok(None)
    }
}

/// The players' base priorities from 1.24 on; 0 before.
fn read_base_priorities(mut input: impl Read, version: f32) -> Result<Vec<i8>> {
    let mut base_priorities = vec![0; 16];
    if version >= 1.24 {
        input.read_i8_into(&mut base_priorities)?;
    }
    Ok(base_priorities)
}

/// Definitive Edition's options.
#[derive(Debug, Default, PartialEq, Eq)]
struct DeOptions {
    water_definition: Option<String>,
    color_mood: Option<String>,
    collide_and_correct: bool,
    villager_force_drop: bool,
}

impl DeOptions {
    /// The options as `version` stores them: the water definition from 1.30 on, the colour
    /// mood from 1.32 on, collide-and-correct from 1.36 on and the villagers' force drop from
    /// 1.37 on.
    fn read_from(mut input: impl Read, version: f32) -> Result<Self> {
        let mut options = Self::default();
        if version >= 1.35 {
            // Duplicated here from TriggerSystem … we can discard it because the TriggerSystem
            // will read the same number later.
            let _trigger_count = input.read_u32::<LE>()?;
        }
        if version >= 1.30 {
            let _str_signature = input.read_u16::<LE>()?;
            options.water_definition = input.read_u16_length_prefixed_str()?;
        }
        if version >= 1.32 {
            let _str_signature = input.read_u16::<LE>()?;
            options.color_mood = input.read_u16_length_prefixed_str()?;
        }
        if version >= 1.36 {
            options.collide_and_correct = input.read_u8()? != 0;
        }
        if version >= 1.37 {
            options.villager_force_drop = input.read_u8()? != 0;
        }
        Ok(options)
    }
}

/// The disabled technologies, units and buildings per player, each with its count.
struct DisabledLists {
    num_techs: Vec<i32>,
    techs: Vec<Vec<i32>>,
    num_units: Vec<i32>,
    units: Vec<Vec<i32>>,
    num_buildings: Vec<i32>,
    buildings: Vec<Vec<i32>>,
}

impl DisabledLists {
    /// The lists as `version` stores them: from 1.28 on each count and that many ids; from 1.18
    /// on the counts and 30 ids each (buildings 20 before 1.25); after 1.03 20 technologies
    /// without a count (guessed); before, none.
    fn read_from(mut input: impl Read, version: f32) -> Result<Self> {
        let mut lists = Self {
            num_techs: vec![0; 16],
            techs: vec![vec![]; 16],
            num_units: vec![0; 16],
            units: vec![vec![]; 16],
            num_buildings: vec![0; 16],
            buildings: vec![vec![]; 16],
        };
        if version >= 1.28 {
            // Definitive Edition only stores the exact number of disabled techs/units/buildings.
            read_counted(&mut input, &mut lists.num_techs, &mut lists.techs)?;
            read_counted(&mut input, &mut lists.num_units, &mut lists.units)?;
            read_counted(&mut input, &mut lists.num_buildings, &mut lists.buildings)?;
        } else if version >= 1.18 {
            // AoC and friends store up to 20 or 30 of each.
            read_fixed(&mut input, &mut lists.num_techs, &mut lists.techs, 30)?;
            read_fixed(&mut input, &mut lists.num_units, &mut lists.units, 30)?;
            let max_disabled_buildings = if version >= 1.25 { 30 } else { 20 };
            read_fixed(
                &mut input,
                &mut lists.num_buildings,
                &mut lists.buildings,
                max_disabled_buildings,
            )?;
        } else if version > 1.03 {
            read_guessed_techs(&mut input, &mut lists.num_techs, &mut lists.techs)?;
        } else {
            // <= 1.03 did not support disabling anything
        }
        Ok(lists)
    }
}

/// The counts, then each list of its count's ids.
// The format stores the counts as i32, never negative in valid files.
#[allow(clippy::cast_sign_loss)]
fn read_counted(mut input: impl Read, nums: &mut [i32], lists: &mut [Vec<i32>]) -> Result<()> {
    input.read_i32_into::<LE>(nums)?;
    for (list, &num) in lists.iter_mut().zip(nums.iter()) {
        *list = vec![0; num as usize];
        input.read_i32_into::<LE>(list)?;
    }
    Ok(())
}

/// The counts, then each list of `size` ids.
fn read_fixed(
    mut input: impl Read,
    nums: &mut [i32],
    lists: &mut [Vec<i32>],
    size: usize,
) -> Result<()> {
    input.read_i32_into::<LE>(nums)?;
    for list in lists {
        *list = vec![0; size];
        input.read_i32_into::<LE>(list)?;
    }
    Ok(())
}

/// Old scenarios' 20 technologies per player without a count: the guessed count is the place
/// of the first id not above 0, plus one (0 when every id is above 0).
// A guessed count is at most 20.
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
fn read_guessed_techs(
    mut input: impl Read,
    nums: &mut [i32],
    lists: &mut [Vec<i32>],
) -> Result<()> {
    // Old scenarios only allowed disabling up to 20 techs per player.
    for (num, list) in nums.iter_mut().zip(lists.iter_mut()) {
        *list = vec![0; 20];
        input.read_i32_into::<LE>(list)?;
        // The number of disabled techs wasn't stored either, so we need to guess it!
        *num = list
            .iter()
            .position(|val| *val <= 0)
            .map_or(0, |index| (index as i32) + 1);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Little-endian words.
    fn words(ws: &[i32]) -> Vec<u8> {
        ws.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    /// The separator from 1.02 on; before, nothing.
    #[test]
    fn the_separator_stands_from_1_02_on() {
        let bytes = words(&[-99]);
        let mut input = bytes.as_slice();
        read_separator(&mut input, 1.01).unwrap();
        assert_eq!(input.len(), 4);
        read_separator(&mut input, 1.02).unwrap();
        assert_eq!(input, []);
    }

    /// The multiplayer victory from 1.13 on; before, type 4, score 900, time 9000.
    #[test]
    fn the_multiplayer_victory_reads_from_1_13_on() {
        let bytes = words(&[1, 2, 3]);
        assert_eq!(
            read_mp_victory(bytes.as_slice(), 1.12).unwrap(),
            (4, 900, 9000)
        );
        assert_eq!(read_mp_victory(bytes.as_slice(), 1.13).unwrap(), (1, 2, 3));
    }

    /// The teams: four bytes from 1.24 on, the lock's word in 1.23, the defaults before.
    #[test]
    fn the_teams_settings_follow_their_version() {
        let bytes = [1u8, 0, 1, 8];
        assert_eq!(
            read_teams(bytes.as_slice(), 1.24).unwrap(),
            (true, false, true, 8)
        );
        let lock = words(&[1]);
        assert_eq!(
            read_teams(lock.as_slice(), 1.23).unwrap(),
            (true, true, true, 4)
        );
        assert_eq!(read_teams(&[][..], 1.22).unwrap(), (false, true, true, 4));
    }

    /// The combat mode after 1.04, the naval mode and "All Techs" from 1.12 on.
    #[test]
    fn the_modes_follow_their_version() {
        let bytes = words(&[5, 6, 1]);
        assert_eq!(read_modes(bytes.as_slice(), 1.04).unwrap(), (0, 0, false));
        assert_eq!(read_modes(bytes.as_slice(), 1.05).unwrap(), (5, 0, false));
        assert_eq!(read_modes(bytes.as_slice(), 1.12).unwrap(), (5, 6, true));
    }

    /// The starting ages after 1.05, each in its version's numbering; an unknown one fails.
    #[test]
    fn the_starting_ages_read_after_1_05() {
        let mut ages = vec![2; 16];
        ages[0] = 0;
        let bytes = words(&ages);
        assert_eq!(
            read_start_ages(bytes.as_slice(), 1.05).unwrap(),
            vec![StartingAge::Default; 16]
        );
        let read = read_start_ages(bytes.as_slice(), 1.24).unwrap();
        assert_eq!(
            (read[0], read[1]),
            (StartingAge::DarkAge, StartingAge::CastleAge)
        );
        let read = read_start_ages(bytes.as_slice(), 1.25).unwrap();
        assert_eq!(
            (read[0], read[1]),
            (StartingAge::Default, StartingAge::DarkAge)
        );
        // a number past the version's last age is kept
        assert_eq!(
            read_start_ages(words(&[9; 16]).as_slice(), 1.25).unwrap()[0],
            StartingAge::Other(9)
        );
    }

    /// The view from 1.19 on, the map type from 1.21 on (-1 and -2 none), the base priorities
    /// from 1.24 on.
    #[test]
    fn the_view_map_kind_and_priorities_follow_their_version() {
        let view = words(&[10, 20]);
        assert_eq!(read_view(view.as_slice(), 1.18).unwrap(), (-1, -1));
        assert_eq!(read_view(view.as_slice(), 1.19).unwrap(), (10, 20));
        assert_eq!(read_map_kind(words(&[7]).as_slice(), 1.20).unwrap(), None);
        assert_eq!(
            read_map_kind(words(&[7]).as_slice(), 1.21).unwrap(),
            Some(7)
        );
        for none in [-1, -2] {
            assert_eq!(
                read_map_kind(words(&[none]).as_slice(), 1.21).unwrap(),
                None
            );
        }
        let priorities = [3u8; 16];
        assert_eq!(
            read_base_priorities(priorities.as_slice(), 1.23).unwrap(),
            vec![0; 16]
        );
        assert_eq!(
            read_base_priorities(priorities.as_slice(), 1.24).unwrap(),
            vec![3; 16]
        );
    }

    /// Definitive Edition's options: the trigger count from 1.35 on, the water definition from
    /// 1.30 on, the colour mood from 1.32 on, the two flags from 1.36 and 1.37 on.
    #[test]
    fn the_de_options_follow_their_version() {
        let string = |text: &str| {
            let mut b = vec![0x60, 0x0a];
            b.extend(u16::try_from(text.len()).unwrap().to_le_bytes());
            b.extend(text.as_bytes());
            b
        };
        assert_eq!(
            DeOptions::read_from(&[][..], 1.29).unwrap(),
            DeOptions::default()
        );
        let water = string("WATER");
        assert_eq!(
            DeOptions::read_from(water.as_slice(), 1.30).unwrap(),
            DeOptions {
                water_definition: Some("WATER".to_owned()),
                ..DeOptions::default()
            }
        );
        let mut all = words(&[4]);
        all.extend(string("W"));
        all.extend(string("MOOD"));
        all.extend([1, 1]);
        assert_eq!(
            DeOptions::read_from(all.as_slice(), 1.37).unwrap(),
            DeOptions {
                water_definition: Some("W".to_owned()),
                color_mood: Some("MOOD".to_owned()),
                collide_and_correct: true,
                villager_force_drop: true,
            }
        );
    }

    /// The disabled lists: from 1.28 on each count and that many ids; from 1.18 on the counts
    /// and 30 ids each, buildings 20 before 1.25; after 1.03 the technologies alone, guessed;
    /// before, none.
    #[test]
    fn the_disabled_lists_follow_their_version() {
        let mut counts = vec![0; 16];
        counts[0] = 2;
        let mut exact = words(&counts);
        exact.extend(words(&[101, 102]));
        exact.extend(words(&[0; 16]));
        exact.extend(words(&counts));
        exact.extend(words(&[70, 71]));
        let lists = DisabledLists::read_from(exact.as_slice(), 1.28).unwrap();
        assert_eq!(
            (&lists.techs[0], &lists.units[0], &lists.buildings[0]),
            (&vec![101, 102], &vec![], &vec![70, 71])
        );
        for (version, buildings) in [(1.18, 20), (1.25, 30)] {
            let mut fixed = vec![];
            for size in [30, 30, buildings] {
                fixed.extend(words(&counts));
                fixed.extend(words(&vec![5; 16 * size]));
            }
            let lists = DisabledLists::read_from(fixed.as_slice(), version).unwrap();
            assert_eq!(
                (
                    lists.num_units[0],
                    lists.techs[0].len(),
                    lists.buildings[15].len()
                ),
                (2, 30, buildings)
            );
        }
        let old = words(&[[3, 4, -1].as_slice(), &[0; 17], &[0; 20 * 15]].concat());
        let lists = DisabledLists::read_from(old.as_slice(), 1.04).unwrap();
        assert_eq!((lists.num_techs[0], lists.num_techs[1]), (3, 1));
        assert_eq!(lists.techs[0][..3], [3, 4, -1]);
        let none = DisabledLists::read_from(&[][..], 1.03).unwrap();
        assert!(none.techs.iter().all(Vec::is_empty));
    }

    /// The guessed count: the place of the first id not above 0, plus one; 0 when every id is
    /// above 0.
    #[test]
    fn an_old_scenario_count_is_guessed_from_its_ids() {
        let bytes = words(&[[1; 20].as_slice(), &[0; 20]].concat());
        let mut nums = [9; 2];
        let mut lists = [vec![], vec![]];
        read_guessed_techs(bytes.as_slice(), &mut nums, &mut lists).unwrap();
        assert_eq!(nums, [0, 1]);
    }

    /// The stances, 16 by 16; an unknown one is kept.
    #[test]
    fn the_diplomacy_reads_each_stance() {
        let mut stances = vec![0; 256];
        stances[17] = 3;
        let diplomacy = read_diplomacy(words(&stances).as_slice()).unwrap();
        assert_eq!(
            (diplomacy[0][0], diplomacy[1][1]),
            (DiplomaticStance::Ally, DiplomaticStance::Enemy)
        );
        stances[5] = 7;
        assert_eq!(
            read_diplomacy(words(&stances).as_slice()).unwrap()[0][5],
            DiplomaticStance::Other(7)
        );
    }
}
