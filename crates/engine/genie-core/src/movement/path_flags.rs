//! A path's flags ([`PathFlags`], `+0xc`) and a path level's ([`LevelFlags`], `+0x3c`).

bitflags::bitflags! {
    /// A path's own flags (`+0xc`): how its walk goes on past the last waypoint. Every bit is
    /// kept (`_ = !0`).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct PathFlags: u32 {
        /// The walk loops (bit 0: a patrol's leg): past the last waypoint it goes on
        /// (`FUN_140f98bc0`).
        const LOOPS = 1;
        /// The loop goes back and forth (bit 1), not round.
        const BACK_AND_FORTH = 1 << 1;
        /// A back-and-forth walk goes forward (bit 2); off, it goes back.
        const FORWARD = 1 << 2;
        /// Any other bit.
        const _ = !0;
    }
}

impl PathFlags {
    /// No flag.
    pub const NONE: Self = Self::empty();
    /// A new path's flags (`FUN_14152eea0`'s construction): back and forth, forward.
    pub const NEW: Self = Self::BACK_AND_FORTH.union(Self::FORWARD);
}

bitflags::bitflags! {
    /// A path level's flags (`+0x3c`), which the walk sets as it goes (`FUN_140f9b4d0`). Every
    /// bit is kept (`_ = !0`).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct LevelFlags: u32 {
        /// A waypoint was reached (bit 0).
        const REACHED = 1;
        /// The waypoint reached was its place, away from the target (bit 1): the next step
        /// takes the waypoint's place.
        const AT_WAYPOINT = 1 << 1;
        /// The walk met only its target (bit 2): the range no longer counts. Cleared on the
        /// walked level, it goes on for the parents.
        const MET_TARGET = 1 << 2;
        /// The tile pathfinder found only the nearest point (bit 3).
        const NEAREST = 1 << 3;
        /// Any other bit.
        const _ = !0;
    }
}

impl LevelFlags {
    /// No flag.
    pub const NONE: Self = Self::empty();
}

impl genie_diff::Diff for PathFlags {
    /// By equality: the flags are one value.
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        differ.equal(self, other);
    }
}

impl genie_diff::Diff for LevelFlags {
    /// By equality: the flags are one value.
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        differ.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::movement::{LevelFlags, PathFlags};

    /// The bits are the game's: a new path is 6, a level's nearest point bit 3.
    #[test]
    fn the_path_and_level_bits_are_the_game_s() {
        assert_eq!(PathFlags::NEW.bits(), 6);
        assert_eq!((PathFlags::NEW | PathFlags::LOOPS).bits(), 7);
        assert_eq!((PathFlags::NEW & !PathFlags::BACK_AND_FORTH).bits(), 4);
        assert_eq!(LevelFlags::NEAREST.bits(), 8);
        let mut level_flags = LevelFlags::NONE;
        level_flags.set(LevelFlags::MET_TARGET, true);
        assert_eq!(level_flags, LevelFlags::from_bits_retain(4));
        assert_eq!(format!("{:?}", LevelFlags::REACHED), "LevelFlags(REACHED)");
    }
}
