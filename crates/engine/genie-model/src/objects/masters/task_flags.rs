//! A task's flags ([`TaskFlags`], `combat_level_flag`).

use serde::Deserialize;
use serde::Serialize;

bitflags::bitflags! {
    /// A task's flags (`combat_level_flag`, the task's `+0x15`). Every bit is kept (`_ = !0`).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct TaskFlags: u8 {
        /// A combat unit's generation or heal task works whatever its top action (bit 1).
        const UNITS_ALWAYS = 1 << 1;
        /// A building's generation or heal task works whatever it is built (bit 2).
        const BUILDINGS_ALWAYS = 1 << 2;
        /// An aura task's amount is a share of its target's master's value (bit 0, slot 277's
        /// powerup key, `FUN_141200dd0`'s fifth argument).
        const AURA_SHARE = 1;
        /// An aura task searches a rounded box (mode 0), else a box (bit 1, the bit of
        /// [`TaskFlags::UNITS_ALWAYS`] as an aura task reads it; slot 277's area key).
        const AURA_ROUNDED = 1 << 1;
        /// An aura task works only while its object's timed ability lasts, and plays no sound
        /// (bit 3, `FUN_141204a10`, slot 277).
        const AURA_WHILE_ABILITY = 1 << 3;
        /// An aura task never works (bit 4, `FUN_141204a10`).
        const AURA_OFF = 1 << 4;
        /// An aura task gives its object a master of its own (bit 6, slot 277's
        /// `FUN_141186d10`).
        const AURA_OWN_MASTER = 1 << 6;
        /// Any other bit.
        const _ = !0;
    }
}

impl TaskFlags {
    /// No flag.
    pub const NONE: Self = Self::empty();
}

impl genie_diff::Diff for TaskFlags {
    /// By equality: the flags are one value.
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        differ.equal(self, other);
    }
}
