//! The game options the simulation reads: the object `FUN_140c87b90` returns (`GameOptions`,
//! 0x1088 bytes), which the header saves and its load `FUN_141305310` fills. Each field names the
//! option's member and offset as the load reads it (`&mWalkableFarms` into `+0x69`, …); the
//! recording's header holds them all (genie-save's `GameOptions`).

use crate::technology::TechMode;
use crate::world::{
    Age, ContentMode, GameMode, ResourceLevel, SubModeFlags, TreatyLength, Visibility,
};

/// How farmers work a farm: the walkable farms option (`mWalkableFarms`, `+0x69`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FarmMode {
    /// One stop (0): a Gather's `set_state(7)` takes where the gathering started.
    #[default]
    Fixed,
    /// Farmers walk their farm's stops (any other byte): a farm's Gather of four stops, and a
    /// Gather's `set_state(7)` leaves `mGatherX`, `mGatherY` at −1.
    Walkable,
}

impl FarmMode {
    /// The mode of the option's byte `value`: walkable when it is not 0.
    #[must_use]
    pub const fn of_option(value: u8) -> Self {
        if value == 0 {
            Self::Fixed
        } else {
            Self::Walkable
        }
    }
}

bitflags::bitflags! {
    /// The game options that are on or off ([`Settings`]).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct OptionFlags: u8 {
        /// `mFullTechTreeEnabled` (`+0x63`): a civilisation's own technologies of a mode other than
        /// 1 are disabled like another civilisation's
        /// (`genie_core::world::World::update_dependents`).
        const FULL_TECH_TREE = 1;
        /// `mVillagerForceDrop` (`+0x1070`), which [`Settings::carry_rule`] reads.
        const VILLAGER_FORCE_DROP = 1 << 1;
        /// `mCheatsEnabled` (`+0x1074`, `FUN_1412ffe00`): players may order each other's units, and
        /// GAME's cheat sub-commands run. The lobby sets it before the game starts; nothing in the
        /// game changes it, so a playback takes it from the header as the other options.
        const CHEATS_ENABLED = 1 << 2;
        /// `mTeamsLocked` (`+0x64`): the world's teams stay as the game began
        /// (`genie_core::world::World::resolve_teams`).
        const TEAMS_LOCKED = 1 << 3;
        /// `mTurboEnabled` (`+0x67`).
        const TURBO = 1 << 4;
        /// `mSharedExploration` (`+0x68`).
        const SHARED_EXPLORATION = 1 << 5;
        /// `mAntiquityMode` (`+0x6a`).
        const ANTIQUITY = 1 << 6;
    }
}

/// What the simulation reads of the game options.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    /// `mGameMode` (`+0x34`).
    pub game_mode: GameMode,
    /// `mEndingAge` (`+0x40`): the last age of the game, 0 for the default
    /// ([`Settings::last_age`]).
    pub ending_age: Age,
    /// `mVisibilityType` (`+0x48`): 0 normal, 1 explored, 2 all visible.
    pub visibility_kind: Visibility,
    /// `mSubGameModeFlag` (`+0x94`): the rules a mode adds to another (bit 1 sudden death, bit 2
    /// regicide), read as an int (`FUN_141324b60` on `+0x18` reads `+0x18 + 0x7c`).
    pub sub_game_mode_flag: SubModeFlags,
    /// `mFullTechTreeEnabled`, `mVillagerForceDrop`, `mCheatsEnabled`, `mTeamsLocked`,
    /// `mTurboEnabled`, `mSharedExploration` and `mAntiquityMode`.
    pub flags: OptionFlags,
    /// `mWalkableFarms` (`+0x69`): how farmers work a farm.
    pub farm_mode: FarmMode,
    /// `mSelectedContentMode` (`+0x98`). The option is a tagged value read through
    /// `FUN_140ec0cd0` (`+0x9c` the kind: 0 for the value itself, 1 for 2 where the value is 3,
    /// else 0); the load reads the value alone and the kind stays 0, as the object's
    /// constructor and `FUN_14130f2b0` leave it.
    pub selected_content_mode: ContentMode,
    /// `mTreatyLength`: the treaty's minutes, 0 for none. The world's treaty monitor (`+0x820`,
    /// `FUN_141461190`) fills its players' zones only while a treaty holds (`FUN_141461c00`);
    /// the slice holds no treaty.
    pub treaty_length: TreatyLength,
    /// `mStartingAge` (`+0x3c`): 0 the mode's default, 2 to 6 the ages.
    pub starting_age: Age,
    /// `mResourceLevel` (`+0x54`).
    pub resource_level: ResourceLevel,
}

/// A technology's mode (its `+0x28`) that tests the root's `+0x560` entry (`FUN_140c87f20`),
/// which the slice does not hold.
const ROOT_MODE: crate::GenieError =
    crate::GenieError::unsupported("a technology of mode 15 or -15 (FUN_140c87f20's entry)");

/// A technology mode that [`Settings::tech_mode_allows`] sent to a handler of other modes.
fn mode_outside(reason: &'static str, mode: u16) -> crate::GenieError {
    crate::GenieError::unreachable(reason).with("mode", mode)
}

impl Settings {
    /// `FUN_1413cf2f0`: whether a technology of mode `mode` (its `+0x28`) is for this game.
    /// Modes 0 and 1 (and any not listed) are for every game; a negative mode is the opposite of
    /// its positive one (but −32, every game). By the game mode: 2 (0), 3 (2), 4 (13 or the sub
    /// mode flag's bit 0), 5 (7), 6 (8), 7 (11 or bit 1), 8 (6 or bit 3), 9 (10), 10 (12), 11
    /// (1), 13 (4, 3, 5 or 15), 16 (16); 12 turbo, 14 bit 2, 17 farms not walkable, 18 shared
    /// exploration, 19 to 25 a resource level of 0 to 6, 26 antiquity. By the starting age, its
    /// default 0 by the mode: 27 Feudal (2, or 0 but in modes 13, 2 and 12), 28 Castle (3, or 0
    /// in mode 13), 29 Imperial (4), 30 post-Imperial (5), 31 6 (or 0 in modes 2 and 12); 32 at
    /// least Castle (3 and up as unsigned, or 0 in modes 13, 2 and 12), 33 at least Imperial (4
    /// up, or 0 in modes 2 and 12), 34 at least 5 (or 0 in modes 2 and 12).
    ///
    /// # Errors
    ///
    /// Modes 15 and −15 (the root's `+0x560` entry, not held).
    pub fn tech_mode_allows(&self, mode: TechMode) -> Result<bool, crate::GenieError> {
        let mode = mode.get();
        let positive = match mode.unsigned_abs() {
            n @ 2..=18 => self.rule_mode(n)?,
            n @ 19..=25 => self.resource_level.get() == i32::from(n) - 19,
            26 => self.flags.contains(OptionFlags::ANTIQUITY),
            32 if mode < 0 => return Ok(true),
            n @ 27..=34 => self.age_mode(n)?,
            _ => return Ok(true),
        };
        Ok(if mode < 0 { !positive } else { positive })
    }

    /// [`Settings::tech_mode_allows`]'s modes 2 to 18 (`n`), by the game mode, the sub mode
    /// flag's bits and the options.
    ///
    /// # Errors
    ///
    /// Mode 15 (the root's `+0x560` entry, not held).
    fn rule_mode(&self, n: u16) -> Result<bool, crate::GenieError> {
        let (m, flags) = (self.game_mode.get(), self.sub_game_mode_flag);
        Ok(match n {
            2 => m == 0,
            3 => m == 2,
            4 => self.empire_wars(),
            5 => m == 7,
            6 => m == 8,
            7 => m == 11 || flags.contains(SubModeFlags::SUDDEN_DEATH),
            8 => m == 6 || flags.contains(SubModeFlags::WONDER_RACE),
            9 => m == 10,
            10 => m == 12,
            11 => m == 1,
            12 => self.flags.contains(OptionFlags::TURBO),
            13 => matches!(m, 4 | 3 | 5 | 15),
            14 => flags.contains(SubModeFlags::REGICIDE),
            15 => return Err(ROOT_MODE),
            16 => m == 16,
            17 => self.farm_mode == FarmMode::Fixed,
            18 => self.flags.contains(OptionFlags::SHARED_EXPLORATION),
            _ => return Err(mode_outside("a rule mode outside 2 to 18", n)),
        })
    }

    /// [`Settings::tech_mode_allows`]'s modes 27 to 34 (`n`), by the starting age, its default 0
    /// by the game mode.
    fn age_mode(&self, n: u16) -> Result<bool, crate::GenieError> {
        let (m, age) = (self.game_mode.get(), self.starting_age.get());
        let below = |n: u32| (age as u32) < n;
        let default_in = |modes: &[i32]| age == 0 && modes.contains(&m);
        Ok(match n {
            27 => age == 2 || (age == 0 && !matches!(m, 13 | 2 | 12)),
            28 => age == 3 || default_in(&[13]),
            29 => age == 4,
            30 => age == 5,
            31 => age == 6 || default_in(&[2, 12]),
            32 => !below(3) || default_in(&[13, 2, 12]),
            33 => !below(4) || default_in(&[2, 12]),
            34 => !below(5) || default_in(&[2, 12]),
            _ => return Err(mode_outside("an age mode outside 27 to 34", n)),
        })
    }

    /// `FUN_141302150`: with the villagers' force drop on, a game mode other than King of the
    /// Hill whose descriptor's `+0x9c` is not 1 (not `standard_mode`): a hunter drops another
    /// resource it carries.
    #[must_use]
    pub fn carry_rule(&self) -> bool {
        self.flags.contains(OptionFlags::VILLAGER_FORCE_DROP)
            && self.game_mode != GameMode::KingOfTheHill
            && !self.game_mode.standard()
    }

    /// `FUN_1412fc110`: the game's last age, `mEndingAge` or 5 for 0. A tech tree's unit of a
    /// later age than the one before it is not in the tree ([`crate::players::world_player`]).
    #[must_use]
    pub fn last_age(&self) -> Age {
        if self.ending_age == Age::Default {
            Age::PostImperial
        } else {
            self.ending_age
        }
    }

    /// The players have explored the whole map from the start (`mVisibilityType` 1 or 2): the
    /// world player's update sets their attribute 22 to 100 rather than their explored share
    /// (`WorldPlayer::update`, `0x14163b100`).
    #[must_use]
    pub const fn explored_all(&self) -> bool {
        self.visibility_kind.explored_all()
    }

    /// `FUN_141302520`: the regicide rule holds, the game mode Regicide (1) or the sub-mode's
    /// bit 2: a player without a king (master 434) loses.
    #[must_use]
    pub fn regicide(&self) -> bool {
        self.game_mode == GameMode::Regicide
            || self.sub_game_mode_flag.contains(SubModeFlags::REGICIDE)
    }

    /// The Empire Wars rule holds (`BaseWorld::update` at `0x141550c1c`, `FUN_1413cf2f0`'s mode
    /// 4): the game mode Empire Wars (13) or the sub-mode's bit 0. Its first update puts the
    /// villagers to work (`genie_core::orders::start_work`).
    #[must_use]
    pub fn empire_wars(&self) -> bool {
        self.game_mode == GameMode::EmpireWars
            || self.sub_game_mode_flag.contains(SubModeFlags::EMPIRE_WARS)
    }

    /// The sudden death rule holds (`WorldPlayer::update` at `0x14163b92a`): the game mode Sudden
    /// Death (11) or the sub-mode's bit 1: a player who had a town centre loses without one.
    #[must_use]
    pub fn sudden_death(&self) -> bool {
        self.game_mode == GameMode::SuddenDeath
            || self.sub_game_mode_flag.contains(SubModeFlags::SUDDEN_DEATH)
    }

    /// The content mode read through `FUN_140ec0cd0` is 1 (`0x140ac8655`): a hunt whose Attack
    /// on a predator (class 10) ended ends too (`set_state` 1) when its unit AI has orders
    /// queued.
    #[must_use]
    pub fn predator_hunts_end(&self) -> bool {
        self.selected_content_mode == ContentMode::PREDATOR_HUNTS_END
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::technology::TechMode;

    /// `FUN_141302150`: the force drop counts in the scenario modes but King of the Hill; in
    /// the standard modes, and in any mode without it, a hunter keeps what it carries.
    #[test]
    fn the_carry_rule_needs_the_force_drop_in_scenario_mode_but_king_of_the_hill() {
        let with = |game_mode: i32, villager_force_drop| Settings {
            game_mode: GameMode::of_data(game_mode),
            flags: if villager_force_drop {
                OptionFlags::VILLAGER_FORCE_DROP
            } else {
                OptionFlags::empty()
            },
            ..Settings::default()
        };
        for mode in [3, 4, 14, 15] {
            assert!(with(mode, true).carry_rule(), "mode {mode}");
            assert!(!with(mode, false).carry_rule(), "mode {mode}");
        }
        // King of the Hill and the standard modes, a mode above 16 (the first descriptor's)
        for mode in [5, 0, 1, 2, 6, 11, 13, 16, 17, -1] {
            assert!(!with(mode, true).carry_rule(), "mode {mode}");
        }
    }

    /// `mVisibilityType` 1 (explored) and 2 (all visible) explore the whole map; 0 (normal) and
    /// the others do not.
    #[test]
    fn the_explored_and_all_visible_kinds_explore_the_whole_map() {
        let with = |visibility_kind: i32| Settings {
            visibility_kind: Visibility::of_data(visibility_kind),
            ..Settings::default()
        };
        assert!(with(1).explored_all() && with(2).explored_all());
        for t in [0, 3, -1] {
            assert!(!with(t).explored_all(), "type {t}");
        }
    }

    /// `FUN_141302520`: Regicide (mode 1) or the sub-mode's bit 2; the sudden death rule: Sudden
    /// Death (mode 11) or the sub-mode's bit 1. Each bit alone, in any mode.
    #[test]
    fn regicide_and_sudden_death_come_from_the_mode_or_the_sub_mode_bits() {
        let with = |game_mode: i32, sub_game_mode_flag: i32| Settings {
            game_mode: GameMode::of_data(game_mode),
            sub_game_mode_flag: SubModeFlags::from_bits_retain(sub_game_mode_flag),
            ..Settings::default()
        };
        assert!(with(1, 0).regicide() && !with(1, 0).sudden_death());
        assert!(with(11, 0).sudden_death() && !with(11, 0).regicide());
        assert!(with(0, 4).regicide() && !with(0, 4).sudden_death());
        assert!(with(0, 2).sudden_death() && !with(0, 2).regicide());
        assert!(with(13, 6).regicide() && with(13, 6).sudden_death());
        for mode in [0, 2, 3, 12, 13] {
            assert!(!with(mode, 1).regicide() && !with(mode, 1).sudden_death());
        }
    }

    /// Empire Wars: the mode 13 or the sub-mode's bit 0 (426422314: a random map game with the
    /// bit, whose villagers start at work), alone or with the other bits.
    #[test]
    fn empire_wars_comes_from_the_mode_or_the_sub_mode_bit_0() {
        let with = |game_mode: i32, sub_game_mode_flag: i32| Settings {
            game_mode: GameMode::of_data(game_mode),
            sub_game_mode_flag: SubModeFlags::from_bits_retain(sub_game_mode_flag),
            ..Settings::default()
        };
        assert!(with(13, 0).empire_wars());
        assert!(with(0, 1).empire_wars());
        assert!(with(1, 7).empire_wars());
        for (mode, flag) in [(0, 0), (0, 2), (0, 4), (11, 6), (12, 8), (14, 0)] {
            assert!(!with(mode, flag).empire_wars(), "{mode} {flag}");
        }
    }

    /// `FUN_1412fc110`: an ending age of 0 is the fifth; any other is itself.
    #[test]
    fn the_last_age_is_the_ending_age_or_5() {
        let with = |ending_age: i32| Settings {
            ending_age: Age::of_data(ending_age),
            ..Settings::default()
        };
        assert_eq!(with(0).last_age(), Age::PostImperial);
        for age in [1, 2, 4, 6, -1] {
            assert_eq!(with(age).last_age(), Age::of_data(age));
        }
    }

    /// The content mode 1 alone ends a predator's hunt.
    #[test]
    fn predator_hunt_ends_in_content_mode_1() {
        let with = |selected_content_mode: i32| Settings {
            selected_content_mode: ContentMode::of_data(selected_content_mode),
            ..Settings::default()
        };
        assert!(with(1).predator_hunts_end());
        for mode in [0, 2, 3] {
            assert!(!with(mode).predator_hunts_end());
        }
    }

    /// `FUN_1413cf2f0`, each mode with a game it allows (and its negative not) and a game of
    /// mode 9 at the default age, which none of these allows.
    #[test]
    fn each_technology_mode_takes_its_game_mode_flag_option_or_age() {
        let mode = |game_mode: i32| Settings {
            game_mode: GameMode::of_data(game_mode),
            ..Settings::default()
        };
        let flag = |sub_game_mode_flag: i32| Settings {
            sub_game_mode_flag: SubModeFlags::from_bits_retain(sub_game_mode_flag),
            ..mode(9)
        };
        let age = |starting_age: i32| Settings {
            starting_age: Age::of_data(starting_age),
            ..mode(9)
        };
        let cases = [
            (2, mode(0)),
            (3, mode(2)),
            (4, mode(13)),
            (4, flag(1)),
            (5, mode(7)),
            (6, mode(8)),
            (7, mode(11)),
            (7, flag(2)),
            (8, mode(6)),
            (8, flag(8)),
            (9, mode(10)),
            (10, mode(12)),
            (11, mode(1)),
            (
                12,
                Settings {
                    flags: OptionFlags::TURBO,
                    ..mode(9)
                },
            ),
            (13, mode(4)),
            (13, mode(3)),
            (13, mode(5)),
            (13, mode(15)),
            (14, flag(4)),
            (16, mode(16)),
            (
                18,
                Settings {
                    flags: OptionFlags::SHARED_EXPLORATION,
                    ..mode(9)
                },
            ),
            (28, age(3)),
            (28, mode(13)),
            (29, age(4)),
            (30, age(5)),
            (31, age(6)),
            (31, mode(12)),
            (32, age(3)),
            (33, age(4)),
            (33, mode(2)),
            (34, age(5)),
        ];
        for (m, allows) in cases {
            assert_eq!(
                allows.tech_mode_allows(TechMode::of_data(m)),
                Ok(true),
                "{m}"
            );
            assert_eq!(
                allows.tech_mode_allows(TechMode::of_data(-m)),
                Ok(m == 32),
                "-{m}"
            );
            assert_eq!(
                mode(9).tech_mode_allows(TechMode::of_data(m)),
                Ok(false),
                "{m} in mode 9"
            );
        }
        let walkable = Settings {
            farm_mode: FarmMode::Walkable,
            ..mode(9)
        };
        assert_eq!(walkable.tech_mode_allows(TechMode::of_data(17)), Ok(false));
        assert_eq!(mode(9).tech_mode_allows(TechMode::of_data(-32)), Ok(true));
        assert_eq!(
            mode(9).tech_mode_allows(TechMode::of_data(15)),
            Err(ROOT_MODE)
        );
        assert_eq!(mode(9).tech_mode_allows(TechMode::of_data(27)), Ok(true));
        assert_eq!(mode(13).tech_mode_allows(TechMode::of_data(27)), Ok(false));
        assert_eq!(age(2).tech_mode_allows(TechMode::of_data(27)), Ok(true));
        let level = |resource_level: i32| Settings {
            resource_level: ResourceLevel::of_data(resource_level),
            ..mode(9)
        };
        for n in 19..=25 {
            assert_eq!(
                level(i32::from(n) - 19).tech_mode_allows(TechMode::of_data(n)),
                Ok(true),
                "{n}"
            );
            assert_eq!(
                level(7).tech_mode_allows(TechMode::of_data(n)),
                Ok(false),
                "{n}"
            );
        }
        let antiquity = Settings {
            flags: OptionFlags::ANTIQUITY,
            ..mode(9)
        };
        assert_eq!(antiquity.tech_mode_allows(TechMode::of_data(26)), Ok(true));
        assert_eq!(mode(9).tech_mode_allows(TechMode::of_data(26)), Ok(false));
        assert_eq!(mode(9).tech_mode_allows(TechMode::of_data(1)), Ok(true));
        assert_eq!(mode(9).tech_mode_allows(TechMode::of_data(35)), Ok(true));
        assert_eq!(
            mode(9).tech_mode_allows(TechMode::of_data(17)),
            Ok(mode(9).farm_mode == FarmMode::Fixed)
        );
    }

    /// `FUN_1413cf2f0` by mode: the game mode's (2, 3, 13), the sub mode flag's (4), the
    /// options' (17, 26), the resource level's (19 to 25), the starting age's with its default 0
    /// by the game mode (27, 28, 32, 34), a negative mode the opposite (but −32), 0, 1 and those
    /// not listed for every game; 15 is the root's entry, not held.
    #[test]
    fn technology_mode_reads_the_game_options() {
        let s = Settings {
            game_mode: GameMode::of_data(2),
            ..Settings::default()
        };
        assert_eq!(s.tech_mode_allows(TechMode::of_data(0)), Ok(true));
        assert_eq!(s.tech_mode_allows(TechMode::of_data(1)), Ok(true));
        assert_eq!(s.tech_mode_allows(TechMode::of_data(99)), Ok(true));
        assert_eq!(s.tech_mode_allows(TechMode::of_data(3)), Ok(true));
        assert_eq!(s.tech_mode_allows(TechMode::of_data(-3)), Ok(false));
        assert_eq!(s.tech_mode_allows(TechMode::of_data(2)), Ok(false));
        assert_eq!(s.tech_mode_allows(TechMode::of_data(-2)), Ok(true));
        assert_eq!(s.tech_mode_allows(TechMode::of_data(13)), Ok(false));
        assert!(
            s.tech_mode_allows(TechMode::of_data(15)).is_err()
                && s.tech_mode_allows(TechMode::of_data(-15)).is_err()
        );
        // mode 2 (death match) with the default age: at least Castle and Imperial and 5, not
        // Feudal (27) and not 28
        assert_eq!(
            [27, 28, 32, 33, 34].map(|m| s.tech_mode_allows(TechMode::of_data(m))),
            [Ok(false), Ok(false), Ok(true), Ok(true), Ok(true)]
        );
        assert_eq!(s.tech_mode_allows(TechMode::of_data(-32)), Ok(true));
        let feudal = Settings {
            starting_age: Age::of_data(2),
            ..Settings::default()
        };
        assert_eq!(
            [27, -27, 32, 34].map(|m| feudal.tech_mode_allows(TechMode::of_data(m))),
            [Ok(true), Ok(false), Ok(false), Ok(false)]
        );
        // the default age in the random map mode (0) is Feudal's (27)
        assert_eq!(
            Settings::default().tech_mode_allows(TechMode::of_data(27)),
            Ok(true)
        );
        let options = Settings {
            sub_game_mode_flag: SubModeFlags::from_bits_retain(1),
            resource_level: ResourceLevel::of_data(3),
            farm_mode: FarmMode::Walkable,
            flags: OptionFlags::ANTIQUITY,
            ..Settings::default()
        };
        assert_eq!(
            [4, 22, 21, -22, 17, -17, 26].map(|m| options.tech_mode_allows(TechMode::of_data(m))),
            [
                Ok(true),
                Ok(true),
                Ok(false),
                Ok(false),
                Ok(false),
                Ok(true),
                Ok(true)
            ]
        );
    }
}
