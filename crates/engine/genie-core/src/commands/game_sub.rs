//! GAME's sub-commands.

/// GAME's sub-command (`+4`, a word), its values named as aoc-mgz's `GameActionModeEnum` and by
/// their handlers. The switch has no case for the others (2, 3, 7, 8, 0xc, 0xf, 0x17 and up):
/// they do nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GameSub(u16);

impl GameSub {
    /// The player at `+8` changes its relation to the player at `+0xa` (slot 0x30).
    pub const DIPLOMACY: Self = Self(0);
    /// The game speed (`+0xc`).
    pub const SPEED: Self = Self(1);
    /// The quick-build cheat (`FUN_140c91630`).
    pub const QUICK_BUILD: Self = Self(4);
    /// The allied victory flag (`FUN_1416765b0`).
    pub const ALLIED_VICTORY: Self = Self(5);
    /// A cheat code (`FUN_1415c92c0`).
    pub const CHEAT: Self = Self(6);
    /// The production queue's flag (`+0x1358`).
    pub const PRODUCTION_QUEUE: Self = Self(9);
    /// Treason, the old Spies (`FUN_141620400`).
    pub const TREASON: Self = Self(0xa);
    /// One of a computer player's AI settings.
    pub const AI_SETTING: Self = Self(0xb);
    /// The mill's queue buys farms (`FUN_141617130` with master 0x32).
    pub const FARM_QUEUE: Self = Self(0xd);
    /// The mill's queue gives farms back (`FUN_141630f10` with master 0x32).
    pub const FARM_UNQUEUE: Self = Self(0xe);
    /// The automatic farm queue flips (`FUN_14163a5f0`).
    pub const AUTO_FARM_QUEUE: Self = Self(0x10);
    /// The dock's queue buys fish traps (`FUN_141616540`).
    pub const FISH_TRAP_QUEUE: Self = Self(0x11);
    /// The dock's queue gives fish traps back (`FUN_1416308f0`).
    pub const FISH_TRAP_UNQUEUE: Self = Self(0x12);
    /// The automatic fish trap queue flips (`FUN_14163a610`).
    pub const AUTO_FISH_TRAP_QUEUE: Self = Self(0x13);
    /// The offensive posture flips (slot 0xa0, `FUN_14163a630`).
    pub const POSTURE: Self = Self(0x14);
    /// The mill's queue buys pastures (`FUN_141617130` with master 0x761).
    pub const PASTURE_QUEUE: Self = Self(0x15);
    /// The mill's queue gives pastures back (`FUN_141630f10` with master 0x761).
    pub const PASTURE_UNQUEUE: Self = Self(0x16);
    /// A sub-command `do_command` lets through for any player (with 1 and 4).
    pub const OPEN_0XF: Self = Self(0xf);

    /// The sub-command of the packet's word `value`.
    #[must_use]
    pub const fn of_data(value: u16) -> Self {
        Self(value)
    }

    /// The word.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::GameSub;

    /// The named sub-commands keep their words.
    #[test]
    fn the_sub_commands_keep_their_words() {
        assert_eq!(GameSub::DIPLOMACY.get(), 0);
        assert_eq!(GameSub::POSTURE, GameSub::of_data(0x14));
        assert_eq!(GameSub::PASTURE_UNQUEUE.get(), 0x16);
    }
}
