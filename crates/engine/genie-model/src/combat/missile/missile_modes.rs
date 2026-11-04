//! A missile master's modes ([`SmartMode`], [`VanishMode`]).

use serde::Deserialize;
use serde::Serialize;

bitflags::bitflags! {
    /// A missile master's `smart_mode`. Every bit is kept (`_ = !0`).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct SmartMode: u8 {
        /// The missile leads its target: it aims where the target will be (bit 0).
        const LEADS_TARGET = 1;
        /// Any other bit.
        const _ = !0;
    }
}

impl SmartMode {
    /// No mode.
    pub const NONE: Self = Self::empty();
}

bitflags::bitflags! {
    /// A missile master's `vanish_mode` (`missile_die_info`). Every bit is kept (`_ = !0`).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct VanishMode: u8 {
        /// The missile flies on through what it hits (bit 0).
        const PASSES_THROUGH = 1;
        /// The landed missile stays (bit 1): it is not destroyed at its end.
        const STAYS = 1 << 1;
        /// Any other bit.
        const _ = !0;
    }
}

impl VanishMode {
    /// No mode.
    pub const NONE: Self = Self::empty();
}

impl genie_diff::Diff for SmartMode {
    /// By equality: the flags are one value.
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        differ.equal(self, other);
    }
}

impl genie_diff::Diff for VanishMode {
    /// By equality: the flags are one value.
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        differ.equal(self, other);
    }
}
