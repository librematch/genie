//! A reader for Age of Empires game data files.
//!
//! This crate aims to support every data file that exists, but is for now being tested with `AoE1`,
//! `AoE2`, and `AoE2`: HD Edition.

#![deny(future_incompatible)]
#![deny(nonstandard_style)]
#![deny(rust_2018_idioms)]
#![deny(unsafe_code)]
#![warn(unused)]
#![warn(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::unimplemented,
    clippy::todo
)]

mod civ;
mod color_table;
mod conv;
mod dat_read;
mod dat_write;
pub mod de;
pub mod random_map;
mod sound;
pub mod sprite;
mod task;
mod tech;
pub mod tech_tree;
pub mod terrain;
pub mod unit_kind;

pub use crate::civ::{Civilization, CivilizationId};
pub use crate::color_table::{ColorTable, PaletteIndex};
use crate::random_map::RandomMapInfo;
pub use crate::sound::{Sound, SoundId, SoundItem};
use crate::sprite::{Sprite, SpriteId};
pub use crate::task::{Task, TaskList};
pub use crate::tech::{Tech, TechEffect};
use crate::tech_tree::TechTree;
use crate::terrain::{Terrain, TerrainBorder, TerrainId, TerrainRestriction, TileSize};
use genie_support::TechId;
use std::cmp::{Ordering, PartialOrd};
use std::fmt;
use std::str::from_utf8;

/// A game version targeted by a data file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum GameVersion {
    /// The original expansion-less Age of Empires 2: Age of Kings.
    Aok,
    /// Age of Empires 2 with the The Conquerors expansion.
    Aoc,
    /// Age of `EMpires` 2: HD Edition.
    Hd,
}

impl GameVersion {
    /// Get the most likely internal game data version number for a given game version.
    const fn as_f32(self) -> f32 {
        use GameVersion::{Aoc, Aok, Hd};
        match self {
            Aok => 11.5,
            Aoc => 11.97,
            Hd => 12.0,
        }
    }
}

/// A data file version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileVersion([u8; 8]);

impl From<[u8; 8]> for FileVersion {
    fn from(identifier: [u8; 8]) -> Self {
        assert!(matches!(
            identifier,
            // "VER *.*\0"
            [b'V', b'E', b'R', b' ', b'0'..=b'9', b'.', b'0'..=b'9', 0]
        ));
        Self(identifier)
    }
}

impl From<&str> for FileVersion {
    fn from(string: &str) -> Self {
        assert!(string.len() <= 8);
        let mut bytes = [0; 8];
        (bytes[..string.len()]).copy_from_slice(string.as_bytes());
        Self::from(bytes)
    }
}

impl fmt::Display for FileVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match from_utf8(&self.0[0..7]) {
            Ok(text) => write!(formatter, "{text}"),
            Err(_) => write!(formatter, "{:?}", self.0),
        }
    }
}

impl PartialOrd for FileVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match self.major_version().partial_cmp(&other.major_version()) {
            None | Some(Ordering::Equal) => {
                self.minor_version().partial_cmp(&other.minor_version())
            }
            Some(order) => Some(order),
        }
    }
}

impl FileVersion {
    /// Get the major version component, eg the 5 in "VER 5.8".
    const fn major_version(self) -> u8 {
        self.0[4] - b'0'
    }
    /// Get the minor version component, eg the 8 in "VER 5.8".
    const fn minor_version(self) -> u8 {
        self.0[6] - b'0'
    }

    /// Is this file built for Star Wars: Galactic Battlegrounds?
    #[must_use]
    pub const fn is_swgb(self) -> bool {
        false
    }

    /// Is this file built for Age of Empires II: The Conquerors?
    #[must_use]
    pub const fn is_aoc(self) -> bool {
        matches!(&self.0, b"VER 5.7\0")
    }

    /// Is this file built for Age of Empires II: Definitive Edition?
    #[must_use]
    pub fn is_de2(self) -> bool {
        self >= Self(*b"VER 5.8\0")
    }
}

/// A data file.
#[derive(Debug, Clone)]
pub struct DatFile {
    file_version: FileVersion,
    game_version: GameVersion,
    /// Terrain restriction tables.
    pub terrain_tables: Vec<TerrainRestriction>,
    /// Tile size data.
    pub tile_sizes: Vec<TileSize>,
    /// Terrains.
    pub terrains: Vec<Terrain>,
    /// Terrain border data, specifying how different terrains blend.
    pub terrain_borders: Vec<TerrainBorder>,
    /// Random map data from `AoE1`.
    random_maps: Vec<RandomMapInfo>,
    /// Data about player colours.
    pub color_tables: Vec<ColorTable>,
    /// The available sounds.
    pub sounds: Vec<Sound>,
    /// The available sprites.
    pub sprites: Vec<Option<Sprite>>,
    /// Tech effect data.
    pub effects: Vec<TechEffect>,
    /// Task lists for unit types.
    pub task_lists: Vec<Option<TaskList>>,
    /// The available civilizations.
    pub civilizations: Vec<Civilization>,
    /// Techs or researches.
    pub techs: Vec<Tech>,
    /// Tech tree data.
    pub tech_tree: TechTree,
}

impl DatFile {
    /// Get a tech by its ID.
    pub fn tech(&self, id: impl Into<TechId>) -> Option<&Tech> {
        let id: TechId = id.into();
        self.techs.get(usize::from(id))
    }

    /// Get a terrain type by its ID.
    pub fn terrain(&self, id: impl Into<TerrainId>) -> Option<&Terrain> {
        let id: TerrainId = id.into();
        self.terrains.get(usize::from(id))
    }

    /// Get the GAIA civilization.
    #[must_use]
    pub fn gaia(&self) -> Option<&Civilization> {
        self.civilization(0)
    }

    /// Get a civilization by its ID.
    pub fn civilization(&self, id: impl Into<CivilizationId>) -> Option<&Civilization> {
        let id: CivilizationId = id.into();
        self.civilizations.get(usize::from(id))
    }

    /// Get a sound by its ID.
    pub fn sound(&self, id: impl Into<SoundId>) -> Option<&Sound> {
        let id: SoundId = id.into();
        self.sounds.get(usize::from(id))
    }

    /// Get a sprite by its ID.
    pub fn sprite(&self, id: impl Into<SpriteId>) -> Option<&Sprite> {
        let id: SpriteId = id.into();
        self.sprites.get(usize::from(id)).and_then(Option::as_ref)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use genie_error::GenieResult;
    #[cfg(test)]
    use std::fs::read;
    use std::{
        collections::hash_map::DefaultHasher,
        fs::File,
        hash::{Hash, Hasher},
        io::Cursor,
    };

    #[test]
    fn aok() -> GenieResult<()> {
        let mut file = File::open("fixtures/aok.dat")?;
        let dat = DatFile::read_from(&mut file)?;
        assert_eq!(dat.civilizations.len(), 14);
        Ok(())
    }

    #[test]
    fn aoc() -> GenieResult<()> {
        let mut file = File::open("fixtures/aoc1.0c.dat")?;
        let dat = DatFile::read_from(&mut file)?;
        assert_eq!(dat.civilizations.len(), 19);
        Ok(())
    }

    #[test]
    fn non_7bit_ascii_tech_name() -> GenieResult<()> {
        let mut file = File::open("fixtures/age-of-chivalry.dat")?;
        let dat = DatFile::read_from(&mut file)?;
        assert_eq!(dat.techs[859].name(), "Székely (enable)");
        Ok(())
    }

    #[test]
    fn hd_edition() -> GenieResult<()> {
        let mut file = File::open("fixtures/hd.dat")?;
        let dat = DatFile::read_from(&mut file)?;
        assert_eq!(dat.civilizations.len(), 32);
        Ok(())
    }

    #[test]
    fn reserialize() -> GenieResult<()> {
        let original = read("fixtures/aoc1.0c.dat")?;
        let mut cursor = Cursor::new(&original);
        let dat = DatFile::read_from(&mut cursor)?;
        let mut serialized = vec![];
        dat.write_to(&mut serialized)?;

        let mut cursor = Cursor::new(&serialized);
        let dat2 = DatFile::read_from(&mut cursor)?;

        let mut orig_hasher = DefaultHasher::new();
        let mut new_hasher = DefaultHasher::new();
        format!("{dat:?}").hash(&mut orig_hasher);
        format!("{dat2:?}").hash(&mut new_hasher);
        assert_eq!(orig_hasher.finish(), new_hasher.finish());

        Ok(())
    }
}
