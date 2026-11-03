//! The game options' values: the game mode, the ages, the visibility, the resource level, the
//! sub-mode's rules, the content mode and the treaty.

use serde::Deserialize;
use serde::Serialize;
use std::cmp::Ordering;

/// A game mode (`mGameMode`, the options' `+0x34`), named by aoc-mgz's game types; mode 9 and
/// 14 have no name there, and 16 is the mode whose QUEUE trains one military unit at a time
/// early on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GameMode {
    /// Random Map (0).
    #[default]
    RandomMap,
    /// Regicide (1): a player loses with the last king ([`crate::world::settings::Settings::regicide`]).
    Regicide,
    /// Death Match (2).
    DeathMatch,
    /// Scenario (3).
    Scenario,
    /// Campaign (4).
    Campaign,
    /// King of the Hill (5).
    KingOfTheHill,
    /// Wonder Race (6).
    WonderRace,
    /// Defend the Wonder (7).
    DefendTheWonder,
    /// Turbo Random Map (8).
    TurboRandomMap,
    /// Capture the Relic (10).
    CaptureTheRelic,
    /// Sudden Death (11): a player loses with the last town centre.
    SuddenDeath,
    /// Battle Royale (12), whose status changes shrink the map.
    BattleRoyale,
    /// Empire Wars (13), whose first update puts the villagers to work.
    EmpireWars,
    /// Co-op Campaign (15).
    CoOpCampaign,
    /// Mode 16, whose QUEUE trains one military unit at a time in the first age
    /// (`FUN_141614450`).
    OneUnit,
    /// A value the game names nowhere, as the save or the game holds it.
    Other(UnnamedGameMode),
}

/// A raw value of [`GameMode`] that names none of its variants: only the set's own
/// constructors make one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UnnamedGameMode(i32);

impl GameMode {
    /// The value of raw `value`: its variant, else `Other`.
    const fn of_raw(value: i32) -> Self {
        match value {
            0 => Self::RandomMap,
            1 => Self::Regicide,
            2 => Self::DeathMatch,
            3 => Self::Scenario,
            4 => Self::Campaign,
            5 => Self::KingOfTheHill,
            6 => Self::WonderRace,
            7 => Self::DefendTheWonder,
            8 => Self::TurboRandomMap,
            10 => Self::CaptureTheRelic,
            11 => Self::SuddenDeath,
            12 => Self::BattleRoyale,
            13 => Self::EmpireWars,
            15 => Self::CoOpCampaign,
            16 => Self::OneUnit,
            other => Self::Other(UnnamedGameMode(other)),
        }
    }

    /// Its raw value.
    const fn raw(self) -> i32 {
        match self {
            Self::RandomMap => 0,
            Self::Regicide => 1,
            Self::DeathMatch => 2,
            Self::Scenario => 3,
            Self::Campaign => 4,
            Self::KingOfTheHill => 5,
            Self::WonderRace => 6,
            Self::DefendTheWonder => 7,
            Self::TurboRandomMap => 8,
            Self::CaptureTheRelic => 10,
            Self::SuddenDeath => 11,
            Self::BattleRoyale => 12,
            Self::EmpireWars => 13,
            Self::CoOpCampaign => 15,
            Self::OneUnit => 16,
            Self::Other(UnnamedGameMode(value)) => value,
        }
    }

    /// The mode of the options' int `value`: every int is one.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self::of_raw(value)
    }

    /// The int.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.raw()
    }

    /// `FUN_141302540`: a scenario mode: Scenario, Campaign, King of the Hill or Co-op Campaign.
    #[must_use]
    pub const fn scenario(self) -> bool {
        matches!(self.raw(), 3 | 4 | 5 | 15)
    }

    /// `FUN_141302450`: the mode's descriptor (`DAT_143f6b330[mode]`, the first for a mode
    /// above 16) has `+0x9c` 1, as `FUN_1412d99e0` makes them (`FUN_1412d9360`'s fifth
    /// argument): all but the scenario modes 3, 4, 5, 14 and 15.
    #[must_use]
    pub const fn standard(self) -> bool {
        !matches!(self.raw(), 3 | 4 | 5 | 14 | 15)
    }
}

/// An age option (`mStartingAge`, `+0x3c`, and `mEndingAge`, `+0x40`), and a tech tree unit's
/// age: 0 the mode's default, 2 Feudal, 3 Castle, 4 Imperial, 5 post-Imperial, 6.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(from = "i32", into = "i32")]
pub enum Age {
    /// The mode's default (0).
    #[default]
    Default,
    /// The Feudal Age (2).
    Feudal,
    /// The Castle Age (3).
    Castle,
    /// The Imperial Age (4).
    Imperial,
    /// The post-Imperial Age (5), `FUN_1412fc110`'s last age for an ending age of 0.
    PostImperial,
    /// A value the game names nowhere, as the save or the game holds it.
    Other(UnnamedAge),
}

/// A raw value of [`Age`] that names none of its variants: only the set's own
/// constructors make one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UnnamedAge(i32);

impl Age {
    /// The value of raw `value`: its variant, else `Other`.
    const fn of_raw(value: i32) -> Self {
        match value {
            0 => Self::Default,
            2 => Self::Feudal,
            3 => Self::Castle,
            4 => Self::Imperial,
            5 => Self::PostImperial,
            other => Self::Other(UnnamedAge(other)),
        }
    }

    /// Its raw value.
    const fn raw(self) -> i32 {
        match self {
            Self::Default => 0,
            Self::Feudal => 2,
            Self::Castle => 3,
            Self::Imperial => 4,
            Self::PostImperial => 5,
            Self::Other(UnnamedAge(value)) => value,
        }
    }

    /// The age of the int `value`: every int is one.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self::of_raw(value)
    }

    /// The int.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.raw()
    }

    /// The age before it (the int less 1, wrapping).
    #[must_use]
    pub const fn before(self) -> Self {
        Self::of_raw(self.raw().wrapping_sub(1))
    }
}

impl From<i32> for Age {
    /// The value of a serialized raw `value`.
    fn from(value: i32) -> Self {
        Self::of_raw(value)
    }
}

impl From<Age> for i32 {
    /// The raw value it serializes as.
    fn from(value: Age) -> Self {
        value.raw()
    }
}

impl PartialOrd for Age {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Age {
    /// The game's order: by the raw value.
    fn cmp(&self, other: &Self) -> Ordering {
        self.raw().cmp(&other.raw())
    }
}

/// The visibility option (`mVisibilityType`, `+0x48`): normal, explored or all visible.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Visibility {
    /// Normal (0).
    #[default]
    Normal,
    /// Explored (1).
    Explored,
    /// All visible (2).
    AllVisible,
    /// A value the game names nowhere, as the save or the game holds it.
    Other(UnnamedVisibility),
}

/// A raw value of [`Visibility`] that names none of its variants: only the set's own
/// constructors make one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UnnamedVisibility(i32);

impl Visibility {
    /// The value of raw `value`: its variant, else `Other`.
    const fn of_raw(value: i32) -> Self {
        match value {
            0 => Self::Normal,
            1 => Self::Explored,
            2 => Self::AllVisible,
            other => Self::Other(UnnamedVisibility(other)),
        }
    }

    /// Its raw value.
    const fn raw(self) -> i32 {
        match self {
            Self::Normal => 0,
            Self::Explored => 1,
            Self::AllVisible => 2,
            Self::Other(UnnamedVisibility(value)) => value,
        }
    }

    /// The visibility of the options' int `value`: every int is one.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self::of_raw(value)
    }

    /// The int.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.raw()
    }

    /// The players have explored the whole map from the start (explored or all visible).
    #[must_use]
    pub const fn explored_all(self) -> bool {
        matches!(self.raw(), 1 | 2)
    }
}

/// The resource level option (`mResourceLevel`, `+0x54`), which the technology modes 19 to 25
/// test for 0 to 6.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResourceLevel(i32);

impl ResourceLevel {
    /// The level of the options' int `value`: every int is one.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// The int.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

bitflags::bitflags! {
    /// The rules a sub-mode adds to the game mode (`mSubGameModeFlag`, `+0x94`, read as an int).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct SubModeFlags: i32 {
        /// Bit 0, the Empire Wars rule (`FUN_141324b60(options + 0x18) & 1`).
        const EMPIRE_WARS = 1;
        /// Bit 1, the sudden death rule (`FUN_141324b60(options + 0x18) & 2`).
        const SUDDEN_DEATH = 2;
        /// Bit 2, the regicide rule (`FUN_141302520`: `+0x94 & 4`).
        const REGICIDE = 4;
        /// Bit 3, which technology mode 8 takes as game mode 6 (Wonder Race).
        const WONDER_RACE = 8;
    }
}

/// The content mode option (`mSelectedContentMode`, `+0x98`) as the load reads it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentMode(i32);

impl ContentMode {
    /// Mode 1: a hunt whose Attack on a predator ended ends too when its unit AI has orders
    /// queued.
    pub const PREDATOR_HUNTS_END: Self = Self(1);

    /// The mode of the options' int `value`: every int is one.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// The int.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

/// The treaty's length in minutes (`mTreatyLength`), 0 for none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TreatyLength(u32);

impl TreatyLength {
    /// No treaty.
    pub const NONE: Self = Self(0);

    /// The length of the options' `minutes`.
    #[must_use]
    pub const fn of_minutes(minutes: u32) -> Self {
        Self(minutes)
    }

    /// The minutes.
    #[must_use]
    pub const fn minutes(self) -> u32 {
        self.0
    }

    /// A treaty holds (a length above 0).
    #[must_use]
    pub const fn holds(self) -> bool {
        self.0 != 0
    }
}

#[cfg(test)]
mod tests {
    use crate::world::{Age, GameMode, TreatyLength, Visibility};

    /// The modes' rules, the ages' order and the options' tests are the game's.
    #[test]
    fn the_options_keep_the_game_values() {
        for m in [3, 4, 5, 15] {
            assert!(GameMode::of_data(m).scenario() && !GameMode::of_data(m).standard());
        }
        assert!(!GameMode::of_data(14).scenario() && !GameMode::of_data(14).standard());
        assert!(GameMode::RandomMap.standard() && !GameMode::RandomMap.scenario());
        assert_eq!(GameMode::default(), GameMode::RandomMap);
        assert!(Age::Feudal < Age::Castle && Age::Imperial < Age::PostImperial);
        assert_eq!(Age::PostImperial.before(), Age::Imperial);
        assert!(Visibility::Explored.explored_all() && Visibility::AllVisible.explored_all());
        assert!(!Visibility::Normal.explored_all());
        assert!(!TreatyLength::NONE.holds() && TreatyLength::of_minutes(10).holds());
    }

    /// Each named value and one the set names nowhere go to their variant and back.
    #[test]
    fn every_age_value_maps_to_its_variant_and_back() {
        for value in [0, 2, 3, 4, 5, 1] {
            assert_eq!(Age::of_raw(value).raw(), value);
        }
        assert!(matches!(Age::of_raw(1), Age::Other(_)));
    }

    /// Each named value and one the set names nowhere go to their variant and back.
    #[test]
    fn every_visibility_value_maps_to_its_variant_and_back() {
        for value in [0, 1, 2, 3] {
            assert_eq!(Visibility::of_raw(value).raw(), value);
        }
        assert!(matches!(Visibility::of_raw(3), Visibility::Other(_)));
    }

    /// Each named value and one the set names nowhere go to their variant and back.
    #[test]
    fn every_game_mode_value_maps_to_its_variant_and_back() {
        for value in [0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12, 13, 15, 16, 9] {
            assert_eq!(GameMode::of_raw(value).raw(), value);
        }
        assert!(matches!(GameMode::of_raw(9), GameMode::Other(_)));
    }
}
