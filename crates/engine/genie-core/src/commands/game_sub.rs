//! GAME's sub-commands.

/// GAME's sub-command (`+4`, a word), its values named as aoc-mgz's `GameActionModeEnum` and by
/// their handlers. The switch has no case for the others (2, 3, 7, 8, 0xc, 0xf, 0x17 and up):
/// they do nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GameSub {
    /// The player at `+8` changes its relation to the player at `+0xa` (slot 0x30).
    Diplomacy,
    /// The game speed (`+0xc`).
    Speed,
    /// The quick-build cheat (`FUN_140c91630`).
    QuickBuild,
    /// The allied victory flag (`FUN_1416765b0`).
    AlliedVictory,
    /// A cheat code (`FUN_1415c92c0`).
    Cheat,
    /// The production queue's flag (`+0x1358`).
    ProductionQueue,
    /// Treason, the old Spies (`FUN_141620400`).
    Treason,
    /// One of a computer player's AI settings.
    AiSetting,
    /// The mill's queue buys farms (`FUN_141617130` with master 0x32).
    FarmQueue,
    /// The mill's queue gives farms back (`FUN_141630f10` with master 0x32).
    FarmUnqueue,
    /// The automatic farm queue flips (`FUN_14163a5f0`).
    AutoFarmQueue,
    /// The dock's queue buys fish traps (`FUN_141616540`).
    FishTrapQueue,
    /// The dock's queue gives fish traps back (`FUN_1416308f0`).
    FishTrapUnqueue,
    /// The automatic fish trap queue flips (`FUN_14163a610`).
    AutoFishTrapQueue,
    /// The offensive posture flips (slot 0xa0, `FUN_14163a630`).
    Posture,
    /// The mill's queue buys pastures (`FUN_141617130` with master 0x761).
    PastureQueue,
    /// The mill's queue gives pastures back (`FUN_141630f10` with master 0x761).
    PastureUnqueue,
    /// A sub-command `do_command` lets through for any player (with 1 and 4).
    Open0xf,
    /// A value the game names nowhere, as the save or the game holds it.
    Other(Unnamed),
}

/// A raw value of [`GameSub`] that names none of its variants: only the set's own
/// constructors make one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Unnamed(u16);

impl GameSub {
    /// The value of raw `value`: its variant, else `Other`.
    const fn of_raw(value: u16) -> Self {
        match value {
            0 => Self::Diplomacy,
            1 => Self::Speed,
            4 => Self::QuickBuild,
            5 => Self::AlliedVictory,
            6 => Self::Cheat,
            9 => Self::ProductionQueue,
            0xa => Self::Treason,
            0xb => Self::AiSetting,
            0xd => Self::FarmQueue,
            0xe => Self::FarmUnqueue,
            0x10 => Self::AutoFarmQueue,
            0x11 => Self::FishTrapQueue,
            0x12 => Self::FishTrapUnqueue,
            0x13 => Self::AutoFishTrapQueue,
            0x14 => Self::Posture,
            0x15 => Self::PastureQueue,
            0x16 => Self::PastureUnqueue,
            0xf => Self::Open0xf,
            other => Self::Other(Unnamed(other)),
        }
    }

    /// Its raw value.
    const fn raw(self) -> u16 {
        match self {
            Self::Diplomacy => 0,
            Self::Speed => 1,
            Self::QuickBuild => 4,
            Self::AlliedVictory => 5,
            Self::Cheat => 6,
            Self::ProductionQueue => 9,
            Self::Treason => 0xa,
            Self::AiSetting => 0xb,
            Self::FarmQueue => 0xd,
            Self::FarmUnqueue => 0xe,
            Self::AutoFarmQueue => 0x10,
            Self::FishTrapQueue => 0x11,
            Self::FishTrapUnqueue => 0x12,
            Self::AutoFishTrapQueue => 0x13,
            Self::Posture => 0x14,
            Self::PastureQueue => 0x15,
            Self::PastureUnqueue => 0x16,
            Self::Open0xf => 0xf,
            Self::Other(Unnamed(value)) => value,
        }
    }

    /// The sub-command of the packet's word `value`.
    #[must_use]
    pub const fn of_data(value: u16) -> Self {
        Self::of_raw(value)
    }

    /// The word.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.raw()
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::GameSub;

    /// The named sub-commands keep their words.
    #[test]
    fn the_sub_commands_keep_their_words() {
        assert_eq!(GameSub::Diplomacy.get(), 0);
        assert_eq!(GameSub::Posture, GameSub::of_data(0x14));
        assert_eq!(GameSub::PastureUnqueue.get(), 0x16);
    }

    /// Each named value and one the set names nowhere go to their variant and back.
    #[test]
    fn every_game_sub_value_maps_to_its_variant_and_back() {
        for value in [
            0, 1, 4, 5, 6, 9, 0xa, 0xb, 0xd, 0xe, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0xf, 2,
        ] {
            assert_eq!(GameSub::of_raw(value).raw(), value);
        }
        assert!(matches!(GameSub::of_raw(2), GameSub::Other(_)));
    }
}
