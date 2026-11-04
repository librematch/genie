//! The writer of the embeddable scenario data ([`TribeScen::write_to`]).

use crate::errors::{
    buildings_not_disabled, techs_not_disabled, too_many_disabled_buildings,
    too_many_disabled_techs, units_not_disabled,
};
use crate::format::TribeScen;
use byteorder::{LE, WriteBytesExt};
use genie_error::GenieResult;
use genie_support::{f32_eq, write_opt_str};
use std::io::Write;

impl TribeScen {
    /// Write scenario data to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing fails, if a string cannot be encoded or a string key does not
    /// fit a u32, or if `version` cannot hold the disabled technologies, units or buildings.
    ///
    /// # Panics
    ///
    /// Panics if a per-player list does not have 16 entries, or if a player name is longer than
    /// 256 bytes.
    pub fn write_to(
        &self,
        mut output: impl Write,
        version: f32,
        num_triggers: u32,
    ) -> GenieResult<()> {
        self.base.write_to(&mut output, version)?;
        self.write_players(&mut output, version)?;
        write_separator(&mut output, version)?;
        self.victory.write_to(&mut output)?;
        output.write_i32::<LE>(i32::from(self.victory_all_flag))?;
        self.write_victory_settings(&mut output, version)?;
        write_separator(&mut output, version)?;
        self.write_teams(&mut output, version)?;
        self.write_disabled(&mut output, version)?;
        self.write_modes(&mut output, version)?;
        write_separator(&mut output, version)?;
        self.write_view(&mut output, version)?;
        self.write_de_options(&mut output, version, num_triggers)
    }

    /// The players' starting resources; up to 1.13 each player's name padded to 256 bytes, then
    /// its activity, resources, type, civilization and posture.
    fn write_players(&self, mut output: impl Write, version: f32) -> GenieResult<()> {
        if version <= 1.13 {
            assert_eq!(self.base.player_names.len(), 16);
            for name in &self.base.player_names {
                let mut padded_bytes = Vec::with_capacity(256);
                if let Some(name) = name {
                    let name_bytes = name.as_bytes();
                    padded_bytes.write_all(name_bytes)?;
                }
                padded_bytes.extend(vec![0; 256 - padded_bytes.len()]);
                output.write_all(&padded_bytes)?;
            }
            assert_eq!(self.base.player_base_properties.len(), 16);
            assert_eq!(self.player_start_resources.len(), 16);
            for (properties, resources) in self
                .base
                .player_base_properties
                .iter()
                .zip(&self.player_start_resources)
            {
                output.write_i32::<LE>(properties.active)?;
                resources.write_to(&mut output, version)?;
                output.write_i32::<LE>(properties.player_kind)?;
                output.write_i32::<LE>(properties.civilization)?;
                output.write_i32::<LE>(properties.posture)?;
            }
        } else {
            assert_eq!(self.player_start_resources.len(), 16);
            for start_resources in &self.player_start_resources {
                start_resources.write_to(&mut output, version)?;
            }
        }
        Ok(())
    }

    /// The multiplayer victory's type, score and time from 1.13 on, the 16 by 16 stances and
    /// each player's legacy victory entries.
    fn write_victory_settings(&self, mut output: impl Write, version: f32) -> GenieResult<()> {
        if version >= 1.13 {
            output.write_i32::<LE>(self.mp_victory_kind)?;
            output.write_i32::<LE>(self.victory_score)?;
            output.write_i32::<LE>(self.victory_time)?;
        }
        assert_eq!(self.diplomacy.len(), 16);
        for player_diplomacy in &self.diplomacy {
            assert_eq!(player_diplomacy.len(), 16);
            for stance in player_diplomacy {
                output.write_i32::<LE>((*stance).into())?;
            }
        }
        assert_eq!(self.legacy_victory_info.len(), 16);
        for list in &self.legacy_victory_info {
            for entry in list {
                entry.write_to(&mut output)?;
            }
        }
        Ok(())
    }

    /// The allied victories, then the teams' settings: four from 1.24 on, the lock alone in
    /// 1.23.
    fn write_teams(&self, mut output: impl Write, version: f32) -> GenieResult<()> {
        for value in &self.allied_victory {
            output.write_i32::<LE>(*value)?;
        }
        if version >= 1.24 {
            output.write_i8(i8::from(self.teams_locked))?;
            output.write_i8(i8::from(self.can_change_teams))?;
            output.write_i8(i8::from(self.random_start_locations))?;
            output.write_u8(self.max_teams)?;
        } else if f32_eq!(version, 1.23) {
            output.write_i32::<LE>(i32::from(self.teams_locked))?;
        }
        Ok(())
    }

    /// The disabled technologies, units and buildings as `version` stores them: from 1.28 on
    /// each count and that many ids; from 1.18 on the counts and 30 ids each (buildings 20
    /// before 1.25); after 1.03 20 technologies without a count; before, none. A version that
    /// cannot hold them is an error.
    fn write_disabled(&self, mut output: impl Write, version: f32) -> GenieResult<()> {
        if version >= 1.28 {
            write_counted(&mut output, &self.num_disabled_techs, &self.disabled_techs)?;
            write_counted(&mut output, &self.num_disabled_units, &self.disabled_units)?;
            write_counted(
                &mut output,
                &self.num_disabled_buildings,
                &self.disabled_buildings,
            )
        } else if version >= 1.18 {
            let max_disabled_buildings = if version >= 1.25 { 30 } else { 20 };
            let most = *self.num_disabled_buildings.iter().max().unwrap_or(&0);
            if most > max_disabled_buildings {
                return Err(too_many_disabled_buildings(most, max_disabled_buildings));
            }
            write_fixed(
                &mut output,
                &self.num_disabled_techs,
                &self.disabled_techs,
                30,
            )?;
            write_fixed(
                &mut output,
                &self.num_disabled_units,
                &self.disabled_units,
                30,
            )?;
            write_fixed(
                &mut output,
                &self.num_disabled_buildings,
                &self.disabled_buildings,
                max_disabled_buildings,
            )
        } else if version > 1.03 {
            let most = *self.num_disabled_techs.iter().max().unwrap_or(&0);
            if most > 20 {
                return Err(too_many_disabled_techs(most));
            }
            self.nothing_else_disabled()?;
            // Old scenarios only allowed disabling up to 20 techs per player.
            for player_disabled_techs in &self.disabled_techs {
                write_padded(&mut output, player_disabled_techs, 20)?;
            }
            Ok(())
        } else {
            // <= 1.03 did not support disabling anything
            if self.num_disabled_techs.iter().any(|&n| n > 0) {
                return Err(techs_not_disabled());
            }
            self.nothing_else_disabled()
        }
    }

    /// No disabled units and buildings, which versions before 1.18 cannot hold.
    fn nothing_else_disabled(&self) -> GenieResult<()> {
        if self.num_disabled_units.iter().any(|&n| n > 0) {
            return Err(units_not_disabled());
        }
        if self.num_disabled_buildings.iter().any(|&n| n > 0) {
            return Err(buildings_not_disabled());
        }
        Ok(())
    }

    /// The combat mode after 1.04, the naval mode and "All Techs" from 1.12 on, each player's
    /// starting age after 1.05.
    fn write_modes(&self, mut output: impl Write, version: f32) -> GenieResult<()> {
        if version > 1.04 {
            output.write_i32::<LE>(0)?;
        }
        if version >= 1.12 {
            output.write_i32::<LE>(0)?;
            output.write_i32::<LE>(i32::from(self.all_techs))?;
        }
        if version > 1.05 {
            for start_age in &self.player_start_ages {
                output.write_i32::<LE>(start_age.to_i32(version))?;
            }
        }
        Ok(())
    }

    /// The initial view from 1.19 on, the map type from 1.21 on (none as -1) and the players'
    /// base priorities from 1.24 on.
    fn write_view(&self, mut output: impl Write, version: f32) -> GenieResult<()> {
        if version >= 1.19 {
            output.write_i32::<LE>(self.view.0)?;
            output.write_i32::<LE>(self.view.1)?;
        }
        if version >= 1.21 {
            output.write_i32::<LE>(self.map_kind.unwrap_or(-1))?;
        }
        if version >= 1.24 {
            assert_eq!(self.base_priorities.len(), 16);
            for priority in &self.base_priorities {
                output.write_i8(*priority)?;
            }
        }
        Ok(())
    }

    /// Definitive Edition's options: from 1.28 on the trigger count and the water definition,
    /// from 1.36 on the colour mood and collide-and-correct, from 1.37 on the villagers' force
    /// drop.
    fn write_de_options(
        &self,
        mut output: impl Write,
        version: f32,
        num_triggers: u32,
    ) -> GenieResult<()> {
        if version >= 1.28 {
            output.write_u32::<LE>(num_triggers)?;
            output.write_u16::<LE>(0)?;
            write_opt_str(&mut output, &self.water_definition)?;
        }
        if version >= 1.36 {
            output.write_u8(0)?;
            output.write_u8(0)?;
            write_opt_str(&mut output, &self.color_mood)?;
            output.write_u8(u8::from(self.collide_and_correct))?;
        }
        if version >= 1.37 {
            output.write_u8(u8::from(self.villager_force_drop))?;
        }
        Ok(())
    }
}

/// The separator -99 the versions from 1.02 on put between sections.
fn write_separator(mut output: impl Write, version: f32) -> GenieResult<()> {
    if version >= 1.02 {
        output.write_i32::<LE>(-99)?;
    }
    Ok(())
}

/// The counts, then each list's first count ids (-1 past its end).
// The disabled counts are never negative in valid data.
#[allow(clippy::cast_sign_loss)]
fn write_counted(mut output: impl Write, nums: &[i32], lists: &[Vec<i32>]) -> GenieResult<()> {
    for num in nums {
        output.write_i32::<LE>(*num)?;
    }
    for (list, &num) in lists.iter().zip(nums) {
        write_padded(&mut output, list, num as usize)?;
    }
    Ok(())
}

/// The counts, then each list's first `size` ids (-1 past its end).
// The most disabled buildings, 20 or 30, are not negative.
#[allow(clippy::cast_sign_loss)]
fn write_fixed(
    mut output: impl Write,
    nums: &[i32],
    lists: &[Vec<i32>],
    size: i32,
) -> GenieResult<()> {
    for num in nums {
        output.write_i32::<LE>(*num)?;
    }
    for list in lists {
        write_padded(&mut output, list, size as usize)?;
    }
    Ok(())
}

/// A list's first `size` ids, -1 past its end.
fn write_padded(mut output: impl Write, list: &[i32], size: usize) -> GenieResult<()> {
    for index in 0..size {
        output.write_i32::<LE>(*list.get(index).unwrap_or(&-1))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::values::StartingAge;

    /// Little-endian words.
    fn words(ws: &[i32]) -> Vec<u8> {
        ws.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    /// Scenario data with 16 players' lists, the first player's disabled ones `disabled`.
    fn scenario(disabled: [Vec<i32>; 3]) -> TribeScen {
        let mut scen = TribeScen {
            num_disabled_techs: vec![0; 16],
            disabled_techs: vec![vec![]; 16],
            num_disabled_units: vec![0; 16],
            disabled_units: vec![vec![]; 16],
            num_disabled_buildings: vec![0; 16],
            disabled_buildings: vec![vec![]; 16],
            allied_victory: vec![0; 16],
            player_start_ages: vec![StartingAge::Default; 16],
            base_priorities: vec![0; 16],
            ..TribeScen::default()
        };
        let [techs, units, buildings] = disabled;
        scen.num_disabled_techs[0] = i32::try_from(techs.len()).unwrap();
        scen.disabled_techs[0] = techs;
        scen.num_disabled_units[0] = i32::try_from(units.len()).unwrap();
        scen.disabled_units[0] = units;
        scen.num_disabled_buildings[0] = i32::try_from(buildings.len()).unwrap();
        scen.disabled_buildings[0] = buildings;
        scen
    }

    /// What `write` puts out.
    fn out(write: impl FnOnce(&mut Vec<u8>) -> GenieResult<()>) -> Vec<u8> {
        let mut bytes = vec![];
        write(&mut bytes).unwrap();
        bytes
    }

    /// The separator from 1.02 on.
    #[test]
    fn the_separator_is_written_from_1_02_on() {
        assert_eq!(out(|o| write_separator(o, 1.01)), []);
        assert_eq!(out(|o| write_separator(o, 1.02)), words(&[-99]));
    }

    /// A list's first ids, -1 past its end; the counted lists their counts first, the fixed
    /// ones too.
    #[test]
    fn the_lists_are_padded_to_their_size() {
        assert_eq!(out(|o| write_padded(o, &[4, 5], 3)), words(&[4, 5, -1]));
        assert_eq!(
            out(|o| write_counted(o, &[1, 0], &[vec![7, 8], vec![9]])),
            words(&[1, 0, 7])
        );
        assert_eq!(
            out(|o| write_fixed(o, &[1, 0], &[vec![7], vec![]], 2)),
            words(&[1, 0, 7, -1, -1, -1])
        );
    }

    /// The disabled lists of each version: counted from 1.28 on, 30 each from 1.18 on with 20
    /// buildings before 1.25, 20 technologies after 1.03, none before.
    #[test]
    fn the_disabled_lists_follow_their_version() {
        let scen = scenario([vec![101], vec![], vec![70]]);
        let mut counts = vec![0; 16];
        counts[0] = 1;
        let counted = out(|o| scen.write_disabled(o, 1.28));
        let mut expected = words(&counts);
        expected.extend(words(&[101]));
        expected.extend(words(&[0; 16]));
        expected.extend(words(&counts));
        expected.extend(words(&[70]));
        assert_eq!(counted, expected);
        for (version, buildings) in [(1.18, 20), (1.25, 30)] {
            let fixed = out(|o| scen.write_disabled(o, version));
            assert_eq!(fixed.len(), 4 * (3 * 16 + 16 * (30 + 30 + buildings)));
        }
        let techs = scenario([vec![101], vec![], vec![]]);
        assert_eq!(out(|o| techs.write_disabled(o, 1.04)).len(), 4 * 16 * 20);
        assert_eq!(
            out(|o| scenario(Default::default()).write_disabled(o, 1.03)),
            []
        );
    }

    /// The versions that cannot hold a scenario's disabled lists fail: more than 20 or 30
    /// buildings, more than 20 technologies, units or buildings before 1.18, anything before
    /// 1.04.
    #[test]
    fn version_that_cannot_hold_the_disabled_lists_fails() {
        let fails = |scen: &TribeScen, version| scen.write_disabled(&mut vec![], version);
        let buildings = scenario([vec![], vec![], vec![70; 21]]);
        assert_eq!(
            fails(&buildings, 1.24),
            Err(too_many_disabled_buildings(21, 20))
        );
        assert!(fails(&buildings, 1.25).is_ok());
        let techs = scenario([vec![101; 21], vec![], vec![]]);
        assert_eq!(fails(&techs, 1.04), Err(too_many_disabled_techs(21)));
        let units = scenario([vec![], vec![4], vec![]]);
        assert_eq!(fails(&units, 1.04), Err(units_not_disabled()));
        assert_eq!(fails(&buildings, 1.03), Err(buildings_not_disabled()));
        assert_eq!(fails(&techs, 1.03), Err(techs_not_disabled()));
    }

    /// The allied victories, then four team bytes from 1.24 on, the lock's word in 1.23.
    #[test]
    fn the_teams_follow_their_version() {
        let scen = TribeScen {
            teams_locked: true,
            max_teams: 8,
            ..scenario(Default::default())
        };
        let allied = words(&[0; 16]);
        assert_eq!(out(|o| scen.write_teams(o, 1.22)), allied);
        assert_eq!(
            out(|o| scen.write_teams(o, 1.23)),
            [allied.clone(), words(&[1])].concat()
        );
        assert_eq!(
            out(|o| scen.write_teams(o, 1.24)),
            [allied, vec![1, 0, 0, 8]].concat()
        );
    }

    /// The modes: the combat mode after 1.04, the naval mode and "All Techs" from 1.12 on,
    /// the starting ages after 1.05.
    #[test]
    fn the_modes_follow_their_version() {
        let scen = TribeScen {
            all_techs: true,
            ..scenario(Default::default())
        };
        assert_eq!(out(|o| scen.write_modes(o, 1.04)), []);
        assert_eq!(out(|o| scen.write_modes(o, 1.05)), words(&[0]));
        let modern = out(|o| scen.write_modes(o, 1.25));
        assert_eq!(modern, [words(&[0, 0, 1]), words(&[0; 16])].concat());
    }

    /// The view from 1.19 on, the map type from 1.21 on (none as -1), the priorities from
    /// 1.24 on.
    #[test]
    fn the_view_follows_its_version() {
        let scen = TribeScen {
            view: (10, 20),
            ..scenario(Default::default())
        };
        assert_eq!(out(|o| scen.write_view(o, 1.18)), []);
        assert_eq!(out(|o| scen.write_view(o, 1.21)), words(&[10, 20, -1]));
        assert_eq!(out(|o| scen.write_view(o, 1.24)).len(), 12 + 16);
    }

    /// Definitive Edition's options: the trigger count and water definition from 1.28 on, the
    /// mood and collide-and-correct from 1.36 on, the force drop from 1.37 on.
    #[test]
    fn the_de_options_follow_their_version() {
        let scen = TribeScen {
            collide_and_correct: true,
            villager_force_drop: true,
            ..scenario(Default::default())
        };
        assert_eq!(out(|o| scen.write_de_options(o, 1.27, 5)), []);
        let water = out(|o| scen.write_de_options(o, 1.28, 5));
        assert_eq!(water[..6], [5, 0, 0, 0, 0, 0]);
        let all = out(|o| scen.write_de_options(o, 1.37, 5));
        assert_eq!(all[all.len() - 2..], [1, 1]);
        assert_eq!(all.len(), water.len() + 2 + (water.len() - 6) + 2);
    }
}
