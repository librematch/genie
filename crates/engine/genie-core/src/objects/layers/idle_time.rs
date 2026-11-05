//! A villager's idle time (`RGE_Action_Object` `+0x898` to `+0x8a0`): what its update counts
//! while it may be interrupted and adds to its owner's statistics once it dies
//! (`RGE_Action_Object::update`, `0x14106ac90`).

use crate::world::{Millis, WorldTime};
use genie_diff::Diff;

/// The idle time of a villager not yet added to its owner's statistics. The load's constructor
/// (`FUN_141068160`) and the others start it at zero: the save does not hold it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Diff)]
pub struct IdleTime {
    /// `+0x898`: the world time its idle began, 0 while it works. The game tests the value, so
    /// an idle that begins at time 0 begins again in the next update.
    pub since: WorldTime,
    /// `+0x89c`: its idle milliseconds (an unsigned int, `CVTSI2SS` of its zero extension).
    pub idle: Millis,
    /// `+0x8a0`: the idles it began.
    pub count: IdleCount,
}

/// The number of idles a villager began (`+0x8a0`, an int: `INC`, then `CVTDQ2PS`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IdleCount(i32);

impl IdleCount {
    /// None.
    pub const ZERO: Self = Self(0);

    /// One idle more (the game's `INC`, wrapping).
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    /// The count as a float (`CVTDQ2PS`).
    #[must_use]
    pub const fn as_f32(self) -> f32 {
        self.0 as f32
    }
}

impl Diff for IdleCount {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}
