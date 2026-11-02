//! A master's blast level and blast defence ([`BlastLevel`], [`BlastDefense`]).

use crate::map::Distance;
use genie_diff::Diff;
use getset::CopyGetters;

bitflags::bitflags! {
    /// A master's blast level (`area_effect_level`, `+0x830`): a level in its low bits, which
    /// the game compares as a whole byte with a [`BlastDefense`] ([`BlastLevel::reaches`]), and
    /// flags above it. Every bit is kept (`_ = !0`).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct BlastLevel: u8 {
        /// A ranged attacker's blast spares its own and allied objects (bit 2, `FUN_1410bfcb0`);
        /// a melee one's always does.
        const SPARES_FRIENDS = 1 << 2;
        /// A ranged attacker picks its target without looking for friends the blast would hit
        /// (bit 3, `FUN_1410bf160`).
        const SKIPS_FRIEND_CHECK = 1 << 3;
        /// A cone blast's cone is wide (bit 5): 1.67 facets within the wide radius.
        const WIDE_CONE = 1 << 5;
        /// A melee attacker's blast falls off with the distance, `(r - d) / r`, off the main
        /// target (bit 6).
        const FALLS_OFF = 1 << 6;
        /// The blast reaches only a cone in the attacker's facing (bit 7).
        const CONE = 1 << 7;
        /// Any other bit.
        const _ = !0;
    }
}

impl BlastLevel {
    /// No level, no flag.
    pub const NONE: Self = Self::empty();

    /// The blast reaches an object of blast defence `defense`: the whole byte is at most the
    /// defence.
    #[must_use]
    pub const fn reaches(self, defense: BlastDefense) -> bool {
        self.bits() <= defense.0
    }

    /// `FUN_1410bfcb0`: the blast of an attacker of weapon range `weapon_range` spares its own
    /// and allied objects: a melee one's (a range of at most 1, or NaN) always, a ranged one's
    /// with [`BlastLevel::SPARES_FRIENDS`].
    #[must_use]
    pub fn spares_friends(self, weapon_range: Distance) -> bool {
        let ranged = Distance::new(1.0) < weapon_range;
        !ranged || self.contains(Self::SPARES_FRIENDS)
    }
}

impl Diff for BlastLevel {
    /// By equality: the level is one value.
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

/// A master's blast defence (`area_effect_object_level`): the blast levels that reach it
/// ([`BlastLevel::reaches`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Diff, CopyGetters)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct BlastDefense(
    /// Its byte, as the master holds it.
    #[getset(get_copy = "pub")]
    u8,
);

impl BlastDefense {
    /// Above 1 every blast reaches the object (the blast's `1 < defense`, the movement damage's
    /// `+0x62b`).
    pub const ALWAYS_REACHED: Self = Self(2);

    /// The defence of byte `level`.
    #[must_use]
    pub const fn new(level: u8) -> Self {
        Self(level)
    }
}
