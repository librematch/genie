//! A sprite's sequence flags ([`SpriteFlags`], the `.dat`'s `sequence_type`).

use serde::Deserialize;
use serde::Serialize;

bitflags::bitflags! {
    /// A sprite's sequence flags: the `.dat`'s `sequence_type` (the sprite's `+0x1a`). Every
    /// bit is kept (`_ = !0`).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct SpriteFlags: u8 {
        /// The sprite is animated (bit 0): it steps through its frames.
        const ANIMATED = 1;
        /// The sprite turns (bit 1): a new object with more than one facet draws its angle.
        const TURNS = 1 << 1;
        /// A new instance draws its first frame (bit 2, `FUN_1413b2ef0`, `FUN_1413b3490`).
        const RANDOM_FIRST_FRAME = 1 << 2;
        /// The sprite plays once (bit 3): it stops on its last frame.
        const ONCE = 1 << 3;
        /// The sprite plays backwards (bit 4): its first frame is its last.
        const BACKWARDS = 1 << 4;
        /// Any other bit.
        const _ = !0;
    }
}

impl SpriteFlags {
    /// No flag.
    pub const NONE: Self = Self::empty();
}

impl genie_diff::Diff for SpriteFlags {
    /// By equality: the flags are one value.
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        differ.equal(self, other);
    }
}
