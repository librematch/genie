//! A task's flags ([`TaskFlags`], `combat_level_flag`).

bitflags::bitflags! {
    /// A task's flags (`combat_level_flag`, the task's `+0x15`). Every bit is kept (`_ = !0`).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct TaskFlags: u8 {
        /// A combat unit's generation or heal task works whatever its top action (bit 1).
        const UNITS_ALWAYS = 1 << 1;
        /// A building's generation or heal task works whatever it is built (bit 2).
        const BUILDINGS_ALWAYS = 1 << 2;
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
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}
