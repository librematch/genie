//! What a player knows of a tile.

/// What a player knows of a tile (`FUN_141663500`, a byte): 0xf when it sees it, 0x80 when it
/// explored it, else 0; a building's slot 0x1e0 answers the same bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TileSight(u8);

impl TileSight {
    /// Neither seen nor explored (0).
    pub const UNKNOWN: Self = Self(0);
    /// Explored (0x80).
    pub const EXPLORED: Self = Self(0x80);
    /// Seen (0xf).
    pub const SEEN: Self = Self(0xf);

    /// The sight of the byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// The byte.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// Whether the player sees the tile.
    #[must_use]
    pub const fn seen(self) -> bool {
        self.0 == Self::SEEN.0
    }
}

impl genie_diff::Diff for TileSight {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::map::spatial::TileSight;

    /// Only 0xf is seen.
    #[test]
    fn only_0xf_is_seen() {
        assert!(TileSight::SEEN.seen());
        assert!(!TileSight::EXPLORED.seen() && !TileSight::UNKNOWN.seen());
        assert_eq!(TileSight::default(), TileSight::UNKNOWN);
        assert_eq!(TileSight::of_data(0x80), TileSight::EXPLORED);
    }
}
