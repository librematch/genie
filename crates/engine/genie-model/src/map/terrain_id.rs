//! A terrain's id ([`TerrainId`]): a map tile's terrain and the `.dat`'s terrain records' index.

use genie_diff::Diff;
use std::fmt::{Display, Error, Formatter};

/// A terrain by its id: a map tile's terrain (`+0x9`), the terrain it restores (`+0xa`) and its
/// layer (`+0xc`), the index of the `.dat`'s terrain records
/// (`genie_core::map::terrain::Terrain::terrains`). A master holds its terrains as shorts, −1 for
/// none ([`TerrainId::of_data`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Diff)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TerrainId(u8);

impl TerrainId {
    /// The terrain of id `id`.
    #[must_use]
    pub const fn named(id: u8) -> Self {
        Self(id)
    }

    /// The terrain a master's or the save's short names: None for −1 or any id below 0, and
    /// for an id beyond a tile's byte, which no terrain record has.
    #[must_use]
    pub const fn of_data(id: i16) -> Option<Self> {
        if 0 <= id && id <= 0xff {
            Some(Self(id as u8))
        } else {
            None
        }
    }

    /// The id, as a tile holds it.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// Its place in the `.dat`'s terrain records and in a restriction's values.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl Display for TerrainId {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use crate::map::TerrainId;

    /// A short from 0 to 255 names a terrain; −1, any id below and one beyond a byte are none.
    #[test]
    fn terrain_id_is_tile_byte() {
        assert_eq!(TerrainId::of_data(0), Some(TerrainId::named(0)));
        assert_eq!(TerrainId::of_data(255), Some(TerrainId::named(255)));
        assert_eq!(TerrainId::of_data(-1), None);
        assert_eq!(TerrainId::of_data(-2), None);
        assert_eq!(TerrainId::of_data(256), None);
        let t = TerrainId::named(27);
        assert_eq!(
            (t.get(), t.index(), t.to_string()),
            (27, 27, "27".to_owned())
        );
    }
}
