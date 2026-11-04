//! A moving master's move algorithm ([`MoveAlgorithm`], `move_algorithm`).

use serde::Deserialize;
use serde::Serialize;

bitflags::bitflags! {
    /// A moving master's move algorithm (`+0x768`, the `.dat`'s `move_algorithm`). Every bit is
    /// kept (`_ = !0`).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct MoveAlgorithm: u8 {
        /// The unit takes its two attack or standing sprites in turn instead of drawing one
        /// (bit 0, slot 0x5d8): a stop resets the turn, the idle swap draws nothing.
        const ALTERNATES_SPRITES = 1;
        /// Any other bit.
        const _ = !0;
    }
}

impl MoveAlgorithm {
    /// No flag.
    pub const NONE: Self = Self::empty();
}

impl genie_diff::Diff for MoveAlgorithm {
    /// By equality: the flags are one value.
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        differ.equal(self, other);
    }
}
