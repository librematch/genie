//! A master's combat abilities ([`CombatAbility`], `combat_ability`).

use serde::Deserialize;
use serde::Serialize;

bitflags::bitflags! {
    /// A master's combat abilities (`combat_ability`, `+0x840`), named by their readers. Every
    /// bit is kept (`_ = !0`).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct CombatAbility: u8 {
        /// Its attacks (of a master of type 60 and up) meet no melee or pierce armor (bit 0).
        const IGNORES_ARMOR = 1;
        /// It keeps its melee and pierce armor against [`CombatAbility::IGNORES_ARMOR`] (bit 1).
        const RESISTS_IGNORING = 1 << 1;
        /// Its hits mark the target (bit 2, `FUN_141206250`).
        const MARKS_TARGET = 1 << 2;
        /// A volley shoots all its missiles, without the draw of how many (bit 4).
        const FULL_VOLLEY = 1 << 4;
        /// Its hits apply its effects to a target not allied both ways (bit 7,
        /// `FUN_14107bdf0`).
        const HIT_EFFECTS = 1 << 7;
        /// Any other bit.
        const _ = !0;
    }
}

impl CombatAbility {
    /// No ability.
    pub const NONE: Self = Self::empty();
}

impl genie_diff::Diff for CombatAbility {
    /// By equality: the flags are one value.
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}
