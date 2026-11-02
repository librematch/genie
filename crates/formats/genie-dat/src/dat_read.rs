//! The reader of a data file ([`DatFile::read_from`]): its sections in file order.

use crate::civ::Civilization;
use crate::color_table::ColorTable;
use crate::conv::invalid_data;
use crate::random_map::RandomMapInfo;
use crate::sound::Sound;
use crate::sprite::Sprite;
use crate::task::TaskList;
use crate::tech::{Tech, TechEffect};
use crate::tech_tree::TechTree;
use crate::terrain::{Terrain, TerrainBorder, TerrainRestriction, TileSize};
use crate::{DatFile, FileVersion, GameVersion};
use byteorder::{LE, ReadBytesExt};
use flate2::read::DeflateDecoder;
use genie_support::ReadSkipExt;
use std::io::{Read, Result};

impl DatFile {
    /// Read a data file from a compressed byte stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading or inflating `input` fails, or if an ID is out of range.
    ///
    /// Also fails if a civilization's player type is not 1, or if a name does not fit its fixed-size
    /// buffer after decoding (see [`Civilization::read_from`] and [`TechEffect::read_from`]).
    pub fn read_from(input: impl Read) -> Result<Self> {
        let mut input = DeflateDecoder::new(input);

        let mut file_version = [0u8; 8];
        input.read_exact(&mut file_version)?;
        let file_version = FileVersion(file_version);

        let num_terrain_tables = input.read_u16::<LE>()?;
        let num_terrains = input.read_u16::<LE>()?;
        let game_version = game_version(file_version, num_terrains);
        // AoC hardcodes to 42 terrains, but says 41 terrains in the data file.
        // The 42nd terrain is zeroed out.
        let num_terrains_fixed = if game_version == GameVersion::Aoc && num_terrains == 41 {
            42
        } else {
            num_terrains
        };

        // Two lists of pointers
        input.skip(4 * u64::from(num_terrain_tables) + 4 * u64::from(num_terrain_tables))?;

        let terrain_tables = read_array(num_terrain_tables.into(), || {
            TerrainRestriction::read_from(&mut input, file_version, num_terrains)
        })?;
        let color_tables = read_color_tables(&mut input)?;
        let sounds = read_sounds(&mut input, file_version)?;
        let sprites = read_sprites(&mut input)?;
        skip_map_header(&mut input)?;
        let tile_sizes = read_tile_sizes(&mut input)?;
        let terrains = read_array(num_terrains_fixed.into(), || {
            Terrain::read_from(&mut input, file_version, num_terrains_fixed)
        })?;
        let terrain_borders = read_array(16, || TerrainBorder::read_from(&mut input))?;
        skip_map_bounds(&mut input)?;
        skip_map_search(&mut input)?;
        let random_maps = read_random_maps(&mut input)?;
        let num_effects = input.read_u32::<LE>()? as usize;
        let effects = read_array(num_effects, || TechEffect::read_from(&mut input))?;
        let task_lists = read_task_lists(&mut input)?;
        let civilizations = read_civilizations(&mut input, game_version)?;
        let num_techs = input.read_u16::<LE>()?;
        let techs = read_array(num_techs.into(), || Tech::read_from(&mut input))?;
        skip_kill_rates(&mut input)?;
        let tech_tree = TechTree::read_from(&mut input)?;

        Ok(Self {
            file_version,
            game_version,
            terrain_tables,
            tile_sizes,
            terrains,
            terrain_borders,
            random_maps,
            color_tables,
            sounds,
            sprites,
            effects,
            task_lists,
            civilizations,
            techs,
            tech_tree,
        })
    }
}

/// `num` items, each of `read`.
fn read_array<T>(num: usize, mut read: impl FnMut() -> Result<T>) -> Result<Vec<T>> {
    let mut list = vec![];
    for _ in 0..num {
        list.push(read()?);
    }
    Ok(list)
}

/// The game a file is for: version 5.7 with 32 terrains is `AoK`, with 100 HD, else `AoC`.
fn game_version(file_version: FileVersion, num_terrains: u16) -> GameVersion {
    if file_version == FileVersion(*b"VER 5.7\0") {
        match num_terrains {
            32 => GameVersion::Aok,
            100 => GameVersion::Hd,
            // 41 terrains is AoC. TODO support different versions
            _ => GameVersion::Aoc,
        }
    } else {
        GameVersion::Aoc // TODO support different versions
    }
}

/// The colour tables and their count.
fn read_color_tables(mut input: impl Read) -> Result<Vec<ColorTable>> {
    let num_color_tables = input.read_u16::<LE>()?;
    read_array(num_color_tables.into(), || {
        ColorTable::read_from(&mut input)
    })
}

/// The sounds and their count.
fn read_sounds(mut input: impl Read, file_version: FileVersion) -> Result<Vec<Sound>> {
    let num_sounds = input.read_u16::<LE>()?;
    read_array(num_sounds.into(), || {
        Sound::read_from(&mut input, file_version)
    })
}

/// The sprites: their count, whether each exists, then those that do.
fn read_sprites(mut input: impl Read) -> Result<Vec<Option<Sprite>>> {
    let num_sprites = input.read_u16::<LE>()?;
    let sprites_exist = read_array(num_sprites.into(), || {
        input.read_u32::<LE>().map(|n| n != 0)
    })?;
    let mut sprites = vec![];
    for exists in sprites_exist {
        sprites.push(if exists {
            Some(Sprite::read_from(&mut input)?)
        } else {
            None
        });
    }
    Ok(sprites)
}

/// The map's raw pointers and its bogus sizes, read past.
pub(super) fn skip_map_header(mut input: impl Read) -> Result<()> {
    // Some raw pointer values
    let _map_vtable_pointer = input.read_i32::<LE>()?;
    let _tiles_pointer = input.read_i32::<LE>()?;

    // Bogus stuff
    let _map_width = input.read_i32::<LE>()?;
    let _map_height = input.read_i32::<LE>()?;
    let _world_width = input.read_i32::<LE>()?;
    let _world_height = input.read_i32::<LE>()?;
    Ok(())
}

/// The 19 tile sizes and the padding after them.
fn read_tile_sizes(mut input: impl Read) -> Result<Vec<TileSize>> {
    let mut tile_sizes = vec![TileSize::default(); 19];
    for val in &mut tile_sizes {
        *val = TileSize::read_from(&mut input)?;
    }
    let _padding = input.read_i16::<LE>()?;
    Ok(tile_sizes)
}

/// The map's bounds and its tile and block sizes, read past.
pub(super) fn skip_map_bounds(mut input: impl Read) -> Result<()> {
    // Should just skip all this shit probably
    let _map_row_offset = input.read_i32::<LE>()?;
    let _map_min_x = input.read_f32::<LE>()?;
    let _map_min_y = input.read_f32::<LE>()?;
    let _map_max_x = input.read_f32::<LE>()?;
    let _map_max_y = input.read_f32::<LE>()?;
    let _map_max_x = input.read_f32::<LE>()?;
    let _map_max_y = input.read_f32::<LE>()?;
    let _additional_terrain_count = input.read_u16::<LE>()?;
    let _borders_used = input.read_u16::<LE>()?;
    let _max_terrain = input.read_u16::<LE>()?;
    let _tile_width = input.read_u16::<LE>()?;
    let _tile_height = input.read_u16::<LE>()?;
    let _tile_half_width = input.read_u16::<LE>()?;
    let _tile_half_height = input.read_u16::<LE>()?;
    let _elev_height = input.read_u16::<LE>()?;
    let _current_row = input.read_u16::<LE>()?;
    let _current_column = input.read_u16::<LE>()?;
    let _block_begin_row = input.read_u16::<LE>()?;
    let _block_end_row = input.read_u16::<LE>()?;
    let _block_begin_column = input.read_u16::<LE>()?;
    let _block_end_column = input.read_u16::<LE>()?;
    Ok(())
}

/// The map's search pointers and flags and the pointers after them, read past.
pub(super) fn skip_map_search(mut input: impl Read) -> Result<()> {
    let _seach_map_pointer = input.read_i32::<LE>()?;
    let _seach_map_rows_pointer = input.read_i32::<LE>()?;
    let _any_frame_change = input.read_u8()?;
    let _map_visible = input.read_u8()?;
    let _map_fog_of_war = input.read_u8()?;

    // Lots more pointers and stuff
    input.skip(21 + 157 * 4)?;
    Ok(())
}

/// The random maps: their count and pointer, each map's head, then each map's lists.
fn read_random_maps(mut input: impl Read) -> Result<Vec<RandomMapInfo>> {
    let num_random_maps = input.read_u32::<LE>()? as usize;
    let _random_maps_pointer = input.read_u32::<LE>()?;

    let mut random_maps = read_array(num_random_maps, || RandomMapInfo::read_from(&mut input))?;
    for map in &mut random_maps {
        map.finish(&mut input)?;
    }
    Ok(random_maps)
}

/// The task lists: their count, then each behind its flag.
fn read_task_lists(mut input: impl Read) -> Result<Vec<Option<TaskList>>> {
    let num_task_lists = input.read_u32::<LE>()? as usize;
    read_array(num_task_lists, || {
        if input.read_u8()? != 0 {
            TaskList::read_from(&mut input).map(Some)
        } else {
            Ok(None)
        }
    })
}

/// The civilizations: their count, then each behind its player type (always 1).
fn read_civilizations(
    mut input: impl Read,
    game_version: GameVersion,
) -> Result<Vec<Civilization>> {
    let num_civilizations = input.read_u16::<LE>()?;
    read_array(num_civilizations.into(), || {
        let player_type = input.read_i8()?;
        if player_type != 1 {
            return Err(invalid_data("a civilization whose player type is not 1"));
        }
        Civilization::read_from(&mut input, game_version)
    })
}

/// The kill and hit point rates and totals, read past.
pub(super) fn skip_kill_rates(mut input: impl Read) -> Result<()> {
    let _time_slice = input.read_u32::<LE>()?;
    let _unit_kill_rate = input.read_u32::<LE>()?;
    let _unit_kill_total = input.read_u32::<LE>()?;
    let _unit_hit_point_rate = input.read_u32::<LE>()?;
    let _unit_hit_point_total = input.read_u32::<LE>()?;
    let _razing_kill_rate = input.read_u32::<LE>()?;
    let _razing_kill_total = input.read_u32::<LE>()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(test)]
    use std::io::Error;

    /// Version 5.7 with 32 terrains is for Age of Kings, with 100 for HD Edition, with any
    /// other count for The Conquerors; any other version too.
    #[test]
    fn the_game_follows_the_version_and_terrain_count() {
        let v57 = FileVersion(*b"VER 5.7\0");
        assert_eq!(game_version(v57, 32), GameVersion::Aok);
        assert_eq!(game_version(v57, 100), GameVersion::Hd);
        assert_eq!(game_version(v57, 41), GameVersion::Aoc);
        assert_eq!(
            game_version(FileVersion(*b"VER 5.8\0"), 32),
            GameVersion::Aoc
        );
    }

    /// A count of items, each read in turn; the first error ends it.
    #[test]
    fn an_array_reads_its_count_of_items() {
        let mut next = 0;
        let read = read_array(3, || {
            next += 1;
            Ok(next)
        });
        assert_eq!(read.unwrap(), [1, 2, 3]);
        let failed = read_array(3, || Err::<u8, _>(Error::other("short")));
        assert!(failed.is_err());
    }

    /// The task lists behind their flags, a list of none for a 0 flag.
    #[test]
    fn task_list_flag_says_whether_it_follows() {
        let bytes = [2, 0, 0, 0, 0, 0];
        let lists = read_task_lists(bytes.as_slice()).unwrap();
        assert!(lists.iter().all(Option::is_none));
        assert_eq!(lists.len(), 2);
    }

    /// The sprites' flags say which follow; none do here.
    #[test]
    fn sprite_flag_says_whether_it_follows() {
        let bytes = [2, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let sprites = read_sprites(bytes.as_slice()).unwrap();
        assert_eq!(sprites.len(), 2);
        assert!(sprites.iter().all(Option::is_none));
    }
}
