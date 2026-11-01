//! The game options' values: the game mode, the ages, the visibility, the resource level, the
//! sub-mode's rules, the content mode and the treaty.

/// A game mode (`mGameMode`, the options' `+0x34`), named by aoc-mgz's game types; mode 9 and
/// 14 have no name there, and 16 is the mode whose QUEUE trains one military unit at a time
/// early on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GameMode(i32);

impl GameMode {
    /// Random Map (0).
    pub const RANDOM_MAP: Self = Self(0);
    /// Regicide (1): a player loses with the last king ([`crate::world::settings::Settings::regicide`]).
    pub const REGICIDE: Self = Self(1);
    /// Death Match (2).
    pub const DEATH_MATCH: Self = Self(2);
    /// Scenario (3).
    pub const SCENARIO: Self = Self(3);
    /// Campaign (4).
    pub const CAMPAIGN: Self = Self(4);
    /// King of the Hill (5).
    pub const KING_OF_THE_HILL: Self = Self(5);
    /// Wonder Race (6).
    pub const WONDER_RACE: Self = Self(6);
    /// Defend the Wonder (7).
    pub const DEFEND_THE_WONDER: Self = Self(7);
    /// Turbo Random Map (8).
    pub const TURBO_RANDOM_MAP: Self = Self(8);
    /// Capture the Relic (10).
    pub const CAPTURE_THE_RELIC: Self = Self(10);
    /// Sudden Death (11): a player loses with the last town centre.
    pub const SUDDEN_DEATH: Self = Self(11);
    /// Battle Royale (12), whose status changes shrink the map.
    pub const BATTLE_ROYALE: Self = Self(12);
    /// Empire Wars (13), whose first update puts the villagers to work.
    pub const EMPIRE_WARS: Self = Self(13);
    /// Co-op Campaign (15).
    pub const CO_OP_CAMPAIGN: Self = Self(15);
    /// Mode 16, whose QUEUE trains one military unit at a time in the first age
    /// (`FUN_141614450`).
    pub const ONE_UNIT: Self = Self(16);

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

    /// `FUN_141302540`: a scenario mode: Scenario, Campaign, King of the Hill or Co-op Campaign.
    #[must_use]
    pub const fn scenario(self) -> bool {
        matches!(self.0, 3 | 4 | 5 | 15)
    }

    /// `FUN_141302450`: the mode's descriptor (`DAT_143f6b330[mode]`, the first for a mode
    /// above 16) has `+0x9c` 1, as `FUN_1412d99e0` makes them (`FUN_1412d9360`'s fifth
    /// argument): all but the scenario modes 3, 4, 5, 14 and 15.
    #[must_use]
    pub const fn standard(self) -> bool {
        !matches!(self.0, 3 | 4 | 5 | 14 | 15)
    }
}

/// An age option (`mStartingAge`, `+0x3c`, and `mEndingAge`, `+0x40`), and a tech tree unit's
/// age: 0 the mode's default, 2 Feudal, 3 Castle, 4 Imperial, 5 post-Imperial, 6.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Age(i32);

impl Age {
    /// The mode's default (0).
    pub const DEFAULT: Self = Self(0);
    /// The Feudal Age (2).
    pub const FEUDAL: Self = Self(2);
    /// The Castle Age (3).
    pub const CASTLE: Self = Self(3);
    /// The Imperial Age (4).
    pub const IMPERIAL: Self = Self(4);
    /// The post-Imperial Age (5), `FUN_1412fc110`'s last age for an ending age of 0.
    pub const POST_IMPERIAL: Self = Self(5);

    /// The age of the int `value`: every int is one.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// The int.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }

    /// The age before it (the int less 1, wrapping).
    #[must_use]
    pub const fn before(self) -> Self {
        Self(self.0.wrapping_sub(1))
    }
}

/// The visibility option (`mVisibilityType`, `+0x48`): normal, explored or all visible.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Visibility(i32);

impl Visibility {
    /// Normal (0).
    pub const NORMAL: Self = Self(0);
    /// Explored (1).
    pub const EXPLORED: Self = Self(1);
    /// All visible (2).
    pub const ALL_VISIBLE: Self = Self(2);

    /// The visibility of the options' int `value`: every int is one.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// The int.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }

    /// The players have explored the whole map from the start (explored or all visible).
    #[must_use]
    pub const fn explored_all(self) -> bool {
        matches!(self.0, 1 | 2)
    }
}

/// The resource level option (`mResourceLevel`, `+0x54`), which the technology modes 19 to 25
/// test for 0 to 6.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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
    fn the_options_keep_the_game_s_values() {
        for m in [3, 4, 5, 15] {
            assert!(GameMode::of_data(m).scenario() && !GameMode::of_data(m).standard());
        }
        assert!(!GameMode::of_data(14).scenario() && !GameMode::of_data(14).standard());
        assert!(GameMode::RANDOM_MAP.standard() && !GameMode::RANDOM_MAP.scenario());
        assert_eq!(GameMode::default(), GameMode::RANDOM_MAP);
        assert!(Age::FEUDAL < Age::CASTLE && Age::IMPERIAL < Age::POST_IMPERIAL);
        assert_eq!(Age::POST_IMPERIAL.before(), Age::IMPERIAL);
        assert!(Visibility::EXPLORED.explored_all() && Visibility::ALL_VISIBLE.explored_all());
        assert!(!Visibility::NORMAL.explored_all());
        assert!(!TreatyLength::NONE.holds() && TreatyLength::of_minutes(10).holds());
    }
}
