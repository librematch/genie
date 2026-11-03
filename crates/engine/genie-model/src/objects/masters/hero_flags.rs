//! A master's hero flags ([`HeroFlags`], `hero_flag`).

use serde::Deserialize;
use serde::Serialize;

bitflags::bitflags! {
    /// A master's hero flags (`hero_flag`, `+0x8d9`; the `.dat`'s hero mode). A full hero has
    /// each feature; [`HeroFlags::INVERTED`] takes away the features it is set with. Every bit
    /// is kept (`_ = !0`).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct HeroFlags: u8 {
        /// A full hero (bit 0): every feature.
        const FULL_HERO = 1;
        /// It cannot be converted (bit 1, the conversion's target test).
        const NO_CONVERSION = 1 << 1;
        /// It regenerates as a hero (bit 2, `FUN_1412033a0`).
        const REGENERATES = 1 << 2;
        /// A new object of it takes the defensive stance (bit 3, `FUN_140c12300`).
        const DEFENSIVE = 1 << 3;
        /// It joins a group's protected sub-formation (bit 4, `FUN_141011300`).
        const PROTECTED_FORMATION = 1 << 4;
        /// The features set with it are taken away (bit 7, the byte's sign).
        const INVERTED = 1 << 7;
        /// Any other bit.
        const _ = !0;
    }
}

impl HeroFlags {
    /// No flag.
    pub const NONE: Self = Self::empty();

    /// The master has `feature`: it is a full hero or has the feature, and the feature is not
    /// inverted.
    #[must_use]
    pub const fn grants(self, feature: Self) -> bool {
        self.intersects(Self::FULL_HERO.union(feature))
            && !self.contains(Self::INVERTED.union(feature))
    }
}

impl genie_diff::Diff for HeroFlags {
    /// By equality: the flags are one value.
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}
