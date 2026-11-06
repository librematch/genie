use crate::area;
use crate::element::{ReadHeader, WriteHeader};
use crate::fits;
use crate::header_reader::RecordingHeaderReader;
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use genie_error::GenieResult;
use genie_support::{ReadSkipExt, read_i8s, read_le, read_list};
use std::io::{Read, Write};

/// Data about a map tile.
#[derive(Debug, Default, Clone, Copy)]
pub struct Tile {
    /// The terrain type of this tile.
    pub terrain: u8,
    /// The elevation level of this tile.
    pub elevation: u8,
    /// The original terrain type of this tile, if it was later replaced, for example by placing a
    /// Farm. `UserPatch` 1.5 only.
    pub original_terrain: Option<u8>,
}

impl ReadHeader for Tile {
    /// Read a tile from an input stream.
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let terrain = input.read_u8()?;
        let (terrain, elevation, original_terrain) = if terrain == 0xFF {
            (input.read_u8()?, input.read_u8()?, Some(input.read_u8()?))
        } else {
            (terrain, input.read_u8()?, None)
        };
        Ok(Self {
            terrain,
            elevation,
            original_terrain,
        })
    }
}

impl WriteHeader for Tile {
    /// Write a tile to an output stream.
    fn write_to<W: Write>(&self, output: &mut W) -> GenieResult<()> {
        if let Some(terrain) = self.original_terrain {
            output.write_u8(0xFF)?;
            output.write_u8(self.terrain)?;
            output.write_u8(self.elevation)?;
            output.write_u8(terrain)?;
        } else {
            output.write_u8(self.terrain)?;
            output.write_u8(self.elevation)?;
        }
        Ok(())
    }
}

/// A zone map: the map split into zones, the areas a unit with one set of passability rules (one
/// terrain restriction) can move within.
#[derive(Debug, Clone)]
pub struct MapZone {
    /// Zone information—this is a Vec<> of a fixed size, and can only be accessed as a slice
    /// through the `info()` accessors to prevent modifications to the size.
    info: Vec<i8>,
    tiles: Vec<i32>,
    /// The zone of each tile, row by row.
    pub zone_map: Vec<i8>,
    /// The passability of each terrain under this zone map's terrain restriction.
    pub passability_rules: Vec<f32>,
    /// The number of zones.
    pub num_zones: u32,
}

impl Default for MapZone {
    fn default() -> Self {
        Self {
            info: vec![0; 255],
            tiles: vec![0; 255],
            zone_map: Vec::default(),
            passability_rules: Vec::default(),
            num_zones: Default::default(),
        }
    }
}

impl MapZone {
    /// The 255 per-zone info bytes. Not known yet what they hold.
    ///
    /// # Panics
    ///
    /// Panics if there are not exactly 255 of them; `MapZone`'s constructors always make 255.
    #[must_use]
    pub fn info(&self) -> &[i8] {
        assert_eq!(self.info.len(), 255);
        &self.info
    }

    /// The 255 per-zone info bytes, mutable; see [`MapZone::info`].
    ///
    /// # Panics
    ///
    /// Panics if there are not exactly 255 of them; `MapZone`'s constructors always make 255.
    pub fn info_mut(&mut self) -> &mut [i8] {
        assert_eq!(self.info.len(), 255);
        &mut self.info
    }

    /// The 255 per-zone `i32` values; probably the number of tiles in each zone.
    ///
    /// # Panics
    ///
    /// Panics if there are not exactly 255 of them; `MapZone`'s constructors always make 255.
    #[must_use]
    pub fn tiles(&self) -> &[i32] {
        assert_eq!(self.tiles.len(), 255);
        &self.tiles
    }

    /// The 255 per-zone `i32` values, mutable; see [`MapZone::tiles`].
    ///
    /// # Panics
    ///
    /// Panics if there are not exactly 255 of them; `MapZone`'s constructors always make 255.
    pub fn tiles_mut(&mut self) -> &mut [i32] {
        assert_eq!(self.tiles.len(), 255);
        &mut self.tiles
    }
}

impl ReadHeader for MapZone {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let mut zone = Self::default();
        input.read_i8_into(&mut zone.info)?;
        input.read_i32_into::<LE>(&mut zone.tiles)?;
        let tiles = input.tile_count();
        zone.zone_map = read_i8s(input, tiles)?;

        let num_rules = input.read_u32::<LE>()?;
        zone.passability_rules = read_le(input, fits(num_rules)?, f32::from_le_bytes)?;

        zone.num_zones = input.read_u32::<LE>()?;
        Ok(zone)
    }
}

impl WriteHeader for MapZone {
    fn write_to<W: Write>(&self, output: &mut W) -> GenieResult<()> {
        for val in &self.info {
            output.write_i8(*val)?;
        }
        for val in &self.tiles {
            output.write_i32::<LE>(*val)?;
        }
        for val in &self.zone_map {
            output.write_i8(*val)?;
        }
        output.write_u32::<LE>(fits(self.passability_rules.len())?)?;
        for val in &self.passability_rules {
            output.write_f32::<LE>(*val)?;
        }
        output.write_u32::<LE>(self.num_zones)?;
        Ok(())
    }
}

/// Which players can see or have explored each tile.
#[derive(Debug, Default, Clone)]
pub struct VisibilityMap {
    /// Width of the visibility map.
    pub width: u32,
    /// Height of the visibility map.
    pub height: u32,
    /// Visibility flags for each tile.
    pub visibility: Vec<u32>,
}

impl ReadHeader for VisibilityMap {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let width = input.read_u32::<LE>()?;
        let height = input.read_u32::<LE>()?;
        let visibility = read_le(input, area(width, height)?, u32::from_le_bytes)?;
        Ok(Self {
            width,
            height,
            visibility,
        })
    }
}

impl WriteHeader for VisibilityMap {
    fn write_to<W: Write>(&self, output: &mut W) -> GenieResult<()> {
        output.write_u32::<LE>(self.width)?;
        output.write_u32::<LE>(self.height)?;
        for value in &self.visibility {
            output.write_u32::<LE>(*value)?;
        }
        Ok(())
    }
}

/// Information about the map being played.
#[derive(Debug, Default, Clone)]
pub struct Map {
    /// Width of the map.
    pub width: u32,
    /// Height of the map.
    pub height: u32,
    /// Map zones.
    pub zones: Vec<MapZone>,
    /// Is the "All Visible" flag set?
    pub all_visible: bool,
    /// Is fog of war enabled?
    pub fog_of_war: bool,
    /// The tiles in this map, containing terrain and elevation data.
    pub tiles: Vec<Tile>,
    /// The visibility map, containing line of sight data for each player.
    pub visibility: VisibilityMap,
}

impl ReadHeader for Map {
    /// Read map data from an input stream.
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let mut map = Self {
            width: input.read_u32::<LE>()?,
            height: input.read_u32::<LE>()?,
            ..Default::default()
        };

        input.set_map_size(map.width, map.height);
        let num_zones = input.read_u32::<LE>()?;
        map.zones = read_list(fits(num_zones)?, || MapZone::read_from(input))?;
        map.all_visible = input.read_u8()? != 0;
        map.fog_of_war = input.read_u8()? != 0;
        map.tiles = read_list(area(map.width, map.height)?, || Tile::read_from(input))?;

        {
            let data_count = input.read_u32::<LE>()?;
            let _capacity = input.read_u32::<LE>()?;
            input.skip(u64::from(data_count) * 4)?;
            for _ in 0..data_count {
                let count = input.read_u32::<LE>()?;
                input.skip(u64::from(count) * 8)?;
            }
        };

        map.visibility = VisibilityMap::read_from(input)?;

        Ok(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{writes_back, writes_back_with};

    /// A visibility map, a zone and a map whose sizes claim more than the file holds: each
    /// list grows as its entries come, so the read fails at the file's end.
    #[test]
    fn map_lists_past_the_file_fail_at_its_end() {
        use crate::test_support::header_reader;
        let huge: Vec<u8> = [u32::MAX, u32::MAX]
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect();
        assert!(VisibilityMap::read_from(&mut header_reader(&huge, 11.97)).is_err());
        let mut map = huge;
        map.extend([0u8; 6]);
        assert!(Map::read_from(&mut header_reader(&map, 11.97)).is_err());
        let mut zone = vec![0u8; 255 + 255 * 4];
        zone.extend(u32::MAX.to_le_bytes());
        let mut input = header_reader(&zone, 11.97);
        input.set_map_size(0, 0);
        assert!(MapZone::read_from(&mut input).is_err());
    }

    /// A tile with its original terrain behind the marker 0xFF, one without in two bytes.
    #[test]
    fn tile_writes_back_with_or_without_its_original_terrain() {
        let changed = writes_back(
            &Tile {
                terrain: 2,
                elevation: 1,
                original_terrain: Some(0),
            },
            11.97,
        );
        assert_eq!(changed.original_terrain, Some(0));
        let plain = writes_back(
            &Tile {
                terrain: 6,
                elevation: 0,
                original_terrain: None,
            },
            11.97,
        );
        assert_eq!((plain.terrain, plain.original_terrain), (6, None));
    }

    /// A zone map: its 255 infos and tiles, a byte per map tile, the passability rules and
    /// the zone count.
    #[test]
    fn zone_map_writes_back_for_its_map_size() {
        let zone = MapZone {
            zone_map: vec![1, 2, 3, 4],
            passability_rules: vec![1.0, 0.5],
            num_zones: 3,
            ..MapZone::default()
        };
        let read = writes_back_with(&zone, 11.97, |recording_header_reader| {
            recording_header_reader.set_map_size(2, 2);
        });
        assert_eq!((read.info().len(), read.tiles().len()), (255, 255));
        assert_eq!(
            (read.zone_map, read.passability_rules, read.num_zones),
            (vec![1, 2, 3, 4], vec![1.0, 0.5], 3)
        );
    }
}
