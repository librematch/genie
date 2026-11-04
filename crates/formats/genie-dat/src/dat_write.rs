//! The writer of a data file ([`DatFile::write_to`]): its sections in file order.

use crate::conv::fits;
use crate::{DatFile, GameVersion};
use byteorder::{LE, WriteBytesExt};
use flate2::{Compression, write::DeflateEncoder};
use genie_error::GenieResult;
use std::io::Write;

impl DatFile {
    /// Serialize this data file to an output stream. Compression is applied by this function.
    ///
    /// # Errors
    ///
    /// Returns an error if writing or compressing fails.
    ///
    /// Also fails if a list is too long for its count field, or if a member cannot be written (see
    /// the `write_to` methods of the member types).
    pub fn write_to<W: Write>(&self, output: &mut W) -> GenieResult<()> {
        let num_terrains = if self.game_version == GameVersion::Aoc && self.terrains.len() == 42 {
            41
        } else {
            self.terrains.len()
        };

        let mut output = DeflateEncoder::new(output, Compression::default());
        output.write_all(&self.file_version.0)?;
        output.write_u16::<LE>(fits(self.terrain_tables.len())?)?;
        output.write_u16::<LE>(fits(num_terrains)?)?;

        // Two lists of pointers
        output.write_all(&vec![
            0u8;
            4 * self.terrain_tables.len()
                + 4 * self.terrain_tables.len()
        ])?;

        for table in &self.terrain_tables {
            table.write_to(&mut output, self.file_version, fits(num_terrains)?)?;
        }

        self.write_graphics(&mut output)?;
        write_map_header(&mut output)?;
        self.write_terrains(&mut output)?;
        write_map_bounds(&mut output)?;
        write_map_search(&mut output)?;
        self.write_random_maps(&mut output)?;
        self.write_tasks(&mut output)?;
        self.write_civilizations(&mut output)?;
        write_kill_rates(&mut output)?;
        self.tech_tree.write_to(&mut output)?;

        Ok(())
    }

    /// The colour tables, the sounds and the sprites, each with its count; the sprites' flags
    /// whether each exists come before those that do.
    fn write_graphics(&self, mut output: impl Write) -> GenieResult<()> {
        output.write_u16::<LE>(fits(self.color_tables.len())?)?;
        for table in &self.color_tables {
            table.write_to(&mut output)?;
        }

        output.write_u16::<LE>(fits(self.sounds.len())?)?;
        for sound in &self.sounds {
            sound.write_to(&mut output, self.file_version)?;
        }

        output.write_u16::<LE>(fits(self.sprites.len())?)?;
        for maybe_sprite in &self.sprites {
            output.write_u32::<LE>(match maybe_sprite {
                Some(_) => 1,
                None => 0,
            })?;
        }
        for sprite in self.sprites.iter().flatten() {
            sprite.write_to(&mut output)?;
        }
        Ok(())
    }

    /// The tile sizes, the padding, the terrains and their borders.
    // The terrain count is truncated to the format's u16, as before.
    #[allow(clippy::cast_possible_truncation)]
    fn write_terrains(&self, mut output: impl Write) -> GenieResult<()> {
        for size in &self.tile_sizes {
            size.write_to(&mut output)?;
        }

        // Padding
        output.write_i16::<LE>(0)?;

        for terrain in &self.terrains {
            terrain.write_to(&mut output, self.file_version, self.terrains.len() as u16)?;
        }
        for border in &self.terrain_borders {
            border.write_to(&mut output)?;
        }
        Ok(())
    }

    /// The random maps: their count, a null pointer, each map's head, then each map's lists.
    // The count is truncated to the format's u32, as before.
    #[allow(clippy::cast_possible_truncation)]
    fn write_random_maps(&self, mut output: impl Write) -> GenieResult<()> {
        output.write_u32::<LE>(self.random_maps.len() as u32)?;
        output.write_u32::<LE>(0)?; // pointer

        for map in &self.random_maps {
            map.write_to(&mut output)?;
        }
        for map in &self.random_maps {
            map.write_commands_to(&mut output)?;
        }
        Ok(())
    }

    /// The effects and the task lists (each behind its flag), each with its count.
    // The counts are truncated to the format's u32, as before.
    #[allow(clippy::cast_possible_truncation)]
    fn write_tasks(&self, mut output: impl Write) -> GenieResult<()> {
        output.write_u32::<LE>(self.effects.len() as u32)?;
        for effect in &self.effects {
            effect.write_to(&mut output)?;
        }

        output.write_u32::<LE>(self.task_lists.len() as u32)?;
        for task_list in &self.task_lists {
            if let Some(task_list) = task_list {
                output.write_u8(1)?;
                task_list.write_to(&mut output)?;
            } else {
                output.write_u8(0)?;
            }
        }
        Ok(())
    }

    /// The civilizations (each behind its player type 1) and the techs, each with its count.
    // The counts are truncated to the format's u16, as before.
    #[allow(clippy::cast_possible_truncation)]
    fn write_civilizations(&self, mut output: impl Write) -> GenieResult<()> {
        output.write_u16::<LE>(self.civilizations.len() as u16)?;
        for civilization in &self.civilizations {
            output.write_i8(1)?; // player type
            civilization.write_to(&mut output, self.game_version)?;
        }

        output.write_u16::<LE>(self.techs.len() as u16)?;
        for tech in &self.techs {
            tech.write_to(&mut output)?;
        }
        Ok(())
    }
}

/// The map's pointers and sizes, all 0.
fn write_map_header(mut output: impl Write) -> GenieResult<()> {
    output.write_u32::<LE>(0)?; // map vtable pointer
    output.write_u32::<LE>(0)?; // map tiles pointer
    output.write_u32::<LE>(0)?; // map width
    output.write_u32::<LE>(0)?; // map height
    output.write_u32::<LE>(0)?; // world width
    output.write_u32::<LE>(0)?; // world height
    Ok(())
}

/// The map's bounds and its tile and block sizes, all 0.
fn write_map_bounds(mut output: impl Write) -> GenieResult<()> {
    // TODO put correct values in
    output.write_i32::<LE>(0)?;
    for _ in 0..6 {
        output.write_f32::<LE>(0.0)?;
    }
    for _ in 0..14 {
        output.write_u16::<LE>(0)?;
    }
    Ok(())
}

/// The map's search pointers and flags and the pointers after them, all 0.
fn write_map_search(mut output: impl Write) -> GenieResult<()> {
    output.write_i32::<LE>(0)?;
    output.write_i32::<LE>(0)?;
    output.write_u8(0)?;
    output.write_u8(0)?;
    output.write_u8(0)?;

    // Lots more pointers and stuff
    let nulls = [0; 21 + 157 * 4];
    output.write_all(&nulls)?;
    Ok(())
}

/// The kill and hit point rates and totals, all 0.
fn write_kill_rates(mut output: impl Write) -> GenieResult<()> {
    for _ in 0..7 {
        output.write_u32::<LE>(0)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dat_read::{skip_kill_rates, skip_map_bounds, skip_map_header, skip_map_search};

    /// Each block the writer fills with zeros is as long as the reader reads past; a short one
    /// fails, but for the pointers the search block skips.
    #[test]
    fn each_zeroed_block_is_as_long_as_the_reader_skips() {
        type Block = (
            fn(&mut Vec<u8>) -> GenieResult<()>,
            fn(&mut &[u8]) -> GenieResult<()>,
            usize,
        );
        let blocks: [Block; 4] = [
            (|o| write_map_header(o), |i| skip_map_header(i), 24),
            (|o| write_map_bounds(o), |i| skip_map_bounds(i), 4 + 24 + 28),
            (|o| write_map_search(o), |i| skip_map_search(i), 8 + 3 + 649),
            (|o| write_kill_rates(o), |i| skip_kill_rates(i), 28),
        ];
        for (write, skip, len) in blocks {
            let mut bytes = vec![];
            write(&mut bytes).unwrap();
            assert_eq!(bytes.len(), len);
            assert!(bytes.iter().all(|&b| b == 0));
            let mut input = bytes.as_slice();
            skip(&mut input).unwrap();
            assert_eq!(input, []);
        }
        // a short block fails where the reader reads its words; the skip after the search
        // pointers reads what there is
        assert!(skip_map_header(&mut &[0u8; 23][..]).is_err());
        assert!(skip_kill_rates(&mut &[0u8; 27][..]).is_err());
        assert!(skip_map_search(&mut &[0u8; 11][..]).is_ok());
    }
}
