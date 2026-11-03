//! What a player knows of a tile.

/// What a player knows of a tile (`FUN_141663500`, a byte): 0xf when it sees it, 0x80 when it
/// explored it, else 0; a building's slot 0x1e0 answers the same bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TileSight {
    /// Neither seen nor explored (0).
    #[default]
    Unknown,
    /// Explored (0x80).
    Explored,
    /// Seen (0xf).
    Seen,
    /// A value the game names nowhere, as the save or the game holds it.
    Other(Unnamed),
}

/// A raw value of [`TileSight`] that names none of its variants: only the set's own
/// constructors make one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Unnamed(u8);

impl TileSight {
    /// The value of raw `value`: its variant, else `Other`.
    const fn of_raw(value: u8) -> Self {
        match value {
            0 => Self::Unknown,
            0x80 => Self::Explored,
            0xf => Self::Seen,
            other => Self::Other(Unnamed(other)),
        }
    }

    /// Its raw value.
    const fn raw(self) -> u8 {
        match self {
            Self::Unknown => 0,
            Self::Explored => 0x80,
            Self::Seen => 0xf,
            Self::Other(Unnamed(value)) => value,
        }
    }

    /// The sight of the byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self::of_raw(value)
    }

    /// The byte.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.raw()
    }

    /// Whether the player sees the tile.
    #[must_use]
    pub const fn seen(self) -> bool {
        self.raw() == Self::Seen.raw()
    }
}

impl genie_diff::Diff for TileSight {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.raw().diff(&other.raw(), d);
    }
}

#[cfg(test)]
mod tests {
    use crate::map::spatial::TileSight;

    /// Only 0xf is seen.
    #[test]
    fn only_0xf_is_seen() {
        assert!(TileSight::Seen.seen());
        assert!(!TileSight::Explored.seen() && !TileSight::Unknown.seen());
        assert_eq!(TileSight::default(), TileSight::Unknown);
        assert_eq!(TileSight::of_data(0x80), TileSight::Explored);
    }

    /// Each named value and one the set names nowhere go to their variant and back.
    #[test]
    fn every_tile_sight_value_maps_to_its_variant_and_back() {
        for value in [0, 0x80, 0xf, 1] {
            assert_eq!(TileSight::of_raw(value).raw(), value);
        }
        assert!(matches!(TileSight::of_raw(1), TileSight::Other(_)));
    }
}
