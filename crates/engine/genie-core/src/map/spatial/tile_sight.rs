//! What a player knows of a tile.

/// What a player knows of a tile (`FUN_141663500`, a byte): 0xf when it sees it, 0x80 when it
/// explored it, else 0; a building's slot 0x1e0 answers the same bytes. The set is closed: the
/// routine answers no other byte.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum TileSight {
    /// Neither seen nor explored (0).
    #[default]
    Unknown = 0,
    /// Explored (0x80).
    Explored = 0x80,
    /// Seen (0xf).
    Seen = 0xf,
}

impl TileSight {
    /// The byte.
    #[must_use]
    pub const fn get(self) -> u8 {
        self as u8
    }

    /// Whether the player sees the tile.
    #[must_use]
    pub const fn seen(self) -> bool {
        matches!(self, Self::Seen)
    }
}

impl genie_diff::Diff for TileSight {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.get().diff(&other.get(), d);
    }
}

#[cfg(test)]
mod tests {
    use crate::map::spatial::TileSight;

    /// Only 0xf is seen; the bytes are the game's.
    #[test]
    fn only_0xf_is_seen() {
        assert!(TileSight::Seen.seen());
        assert!(!TileSight::Explored.seen() && !TileSight::Unknown.seen());
        assert_eq!(TileSight::default(), TileSight::Unknown);
        assert_eq!(
            [TileSight::Unknown, TileSight::Explored, TileSight::Seen].map(TileSight::get),
            [0, 0x80, 0xf]
        );
    }
}
