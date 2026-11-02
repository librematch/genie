//! Types related to the random map data from Age of Empires 1.

use crate::conv::fits;
use crate::unit_type::UnitTypeId;
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use genie_support::ReadSkipExt;
use std::io::{Read, Result, Write};

/// A random map: its basic layout, and the lands, terrains, objects and elevations placed on it.
#[derive(Debug, Default, Clone)]
pub struct RandomMapInfo {
    id: i32,
    /// Distances of the land from the map's south-west, north-west, north-east and south-east
    /// edges.
    pub borders: (i32, i32, i32, i32),
    /// How the borders are used (genieutils' `BorderUsage`).
    pub border_fade: i32,
    /// Shape of the water (genieutils' `WaterShape`).
    pub water_border: i32,
    /// The terrain the whole map starts out as.
    pub base_terrain: i32,
    /// Share of the map that is land, in percent (genieutils' `LandCoverage`).
    pub land_percent: i32,
    /// The lands placed on the map.
    pub lands: Vec<RandomMapLand>,
    /// The terrains placed on the map.
    pub terrains: Vec<RandomMapTerrain>,
    /// The objects placed on the map.
    pub objects: Vec<RandomMapObject>,
    /// The elevations (hills) placed on the map.
    pub elevations: Vec<RandomMapElevation>,
}

impl RandomMapInfo {
    /// Read a map's header from an input stream. The lists get their lengths but default
    /// entries; [`finish`](Self::finish) reads the entries, which the file stores after all map
    /// headers.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    ///
    /// Also fails if a count does not fit a `usize` (on targets with 16-bit pointers).
    pub fn read_from<R: Read>(input: &mut R) -> Result<Self> {
        let mut info = Self {
            id: input.read_i32::<LE>()?,
            borders: (
                input.read_i32::<LE>()?,
                input.read_i32::<LE>()?,
                input.read_i32::<LE>()?,
                input.read_i32::<LE>()?,
            ),
            border_fade: input.read_i32::<LE>()?,
            water_border: input.read_i32::<LE>()?,
            base_terrain: input.read_i32::<LE>()?,
            land_percent: input.read_i32::<LE>()?,
            ..Default::default()
        };

        let _some_id = input.read_i32::<LE>()?;
        let num_lands = input.read_u32::<LE>()?;
        let _pointer = input.read_u32::<LE>()?;
        let num_terrains = input.read_u32::<LE>()?;
        let _pointer = input.read_u32::<LE>()?;
        let num_objects = input.read_u32::<LE>()?;
        let _pointer = input.read_u32::<LE>()?;
        let num_elevations = input.read_u32::<LE>()?;
        let _pointer = input.read_u32::<LE>()?;

        info.lands = vec![RandomMapLand::default(); fits(num_lands)?];
        info.terrains = vec![RandomMapTerrain::default(); fits(num_terrains)?];
        info.objects = vec![RandomMapObject::default(); fits(num_objects)?];
        info.elevations = vec![RandomMapElevation::default(); fits(num_elevations)?];

        Ok(info)
    }

    /// Read the map's lands, terrains, objects and elevations from an input stream, skipping the
    /// copies of the header data in between.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn finish<R: Read>(&mut self, input: &mut R) -> Result<()> {
        // duplicate data
        input.skip(44)?;
        for land in &mut self.lands {
            *land = RandomMapLand::read_from(input)?;
        }

        // duplicate data
        input.skip(8)?;
        for terrain in &mut self.terrains {
            *terrain = RandomMapTerrain::read_from(input)?;
        }

        // duplicate data
        input.skip(8)?;
        for object in &mut self.objects {
            *object = RandomMapObject::read_from(input)?;
        }

        // duplicate data
        input.skip(8)?;
        for elevation in &mut self.elevations {
            *elevation = RandomMapElevation::read_from(input)?;
        }
        Ok(())
    }

    /// Write the map's header to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    ///
    /// Also fails if a list has more than `u32::MAX` entries.
    pub fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_i32::<LE>(self.id)?;
        output.write_i32::<LE>(self.borders.0)?;
        output.write_i32::<LE>(self.borders.1)?;
        output.write_i32::<LE>(self.borders.2)?;
        output.write_i32::<LE>(self.borders.3)?;
        output.write_i32::<LE>(self.border_fade)?;
        output.write_i32::<LE>(self.water_border)?;
        output.write_i32::<LE>(self.base_terrain)?;
        output.write_i32::<LE>(self.land_percent)?;

        output.write_i32::<LE>(0)?; // some id
        output.write_u32::<LE>(fits(self.lands.len())?)?;
        output.write_u32::<LE>(0)?; // pointer
        output.write_u32::<LE>(fits(self.terrains.len())?)?;
        output.write_u32::<LE>(0)?; // pointer
        output.write_u32::<LE>(fits(self.objects.len())?)?;
        output.write_u32::<LE>(0)?; // pointer
        output.write_u32::<LE>(fits(self.elevations.len())?)?;
        output.write_u32::<LE>(0)?; // pointer

        Ok(())
    }

    /// Write the map's lands, terrains, objects and elevations to an output stream, each list
    /// preceded by a copy of the header data.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    ///
    /// Also fails if a list has more than `u32::MAX` entries.
    pub fn write_commands_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_i32::<LE>(self.borders.0)?;
        output.write_i32::<LE>(self.borders.1)?;
        output.write_i32::<LE>(self.borders.2)?;
        output.write_i32::<LE>(self.borders.3)?;
        output.write_i32::<LE>(self.border_fade)?;
        output.write_i32::<LE>(self.water_border)?;
        output.write_i32::<LE>(self.base_terrain)?;
        output.write_i32::<LE>(self.land_percent)?;
        output.write_u32::<LE>(0)?; // some id

        write_counted(output, &self.lands, RandomMapLand::write_to)?;
        write_counted(output, &self.terrains, RandomMapTerrain::write_to)?;
        write_counted(output, &self.objects, RandomMapObject::write_to)?;
        write_counted(output, &self.elevations, RandomMapElevation::write_to)
    }
}

/// A list's count, a pointer and its entries.
fn write_counted<W: Write, T>(
    output: &mut W,
    list: &[T],
    write: impl Fn(&T, &mut W) -> Result<()>,
) -> Result<()> {
    output.write_u32::<LE>(fits(list.len())?)?;
    output.write_u32::<LE>(0)?; // pointer
    for item in list {
        write(item, output)?;
    }
    Ok(())
}

/// A land of a random map: an area with its own terrain, e.g. a player's start area.
#[derive(Debug, Default, Clone, Copy)]
pub struct RandomMapLand {
    /// ID of the land.
    pub id: i32,
    /// Terrain of the land.
    pub terrain_type: u8,
    /// Distance kept from other lands, in tiles (genieutils' `LandSpacing`).
    pub land_avoidance_tiles: i32,
    /// Radius of the square the land grows from (genieutils' `BaseSize`).
    pub base_square_radius: i32,
    /// Zone the land belongs to.
    pub zone: i8,
    /// How the land is placed (genieutils' `PlacementType`).
    pub placement_type: i8,
    /// X position of the land's base (genieutils' `BaseX`).
    pub x: i32,
    /// Y position of the land's base (genieutils' `BaseY`).
    pub y: i32,
    /// Share of the map the land covers, in percent (genieutils' `LandProportion`).
    pub amount_of_land_used_percent: i8,
    /// Whether one land is made per player (genieutils' `ByPlayerFlag`).
    pub by_player_flag: i8,
    /// Radius of the start area (genieutils' `StartAreaRadius`).
    pub radius: i32,
    /// How far the land's terrain edge fades (genieutils' `TerrainEdgeFade`).
    pub fade: i32,
    /// How clumpy the land's shape is.
    pub clumpiness_factor: i32,
}

impl RandomMapLand {
    /// Read a land from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from<R: Read>(input: &mut R) -> Result<Self> {
        let mut land = Self {
            id: input.read_i32::<LE>()?,
            terrain_type: input.read_u8()?,
            ..Default::default()
        };
        let _padding = input.read_u16::<LE>()?;
        let _padding = input.read_u8()?;
        land.land_avoidance_tiles = input.read_i32::<LE>()?;
        land.base_square_radius = input.read_i32::<LE>()?;
        land.zone = input.read_i8()?;
        land.placement_type = input.read_i8()?;
        let _padding = input.read_u16::<LE>()?;
        land.x = input.read_i32::<LE>()?;
        land.y = input.read_i32::<LE>()?;
        land.amount_of_land_used_percent = input.read_i8()?;
        land.by_player_flag = input.read_i8()?;
        let _padding = input.read_u16::<LE>()?;
        land.radius = input.read_i32::<LE>()?;
        land.fade = input.read_i32::<LE>()?;
        land.clumpiness_factor = input.read_i32::<LE>()?;
        Ok(land)
    }

    /// Write this land to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    pub fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_i32::<LE>(self.id)?;
        output.write_u8(self.terrain_type)?;
        output.write_u16::<LE>(0)?;
        output.write_u8(0)?;
        output.write_i32::<LE>(self.land_avoidance_tiles)?;
        output.write_i32::<LE>(self.base_square_radius)?;
        output.write_i8(self.zone)?;
        output.write_i8(self.placement_type)?;
        output.write_u16::<LE>(0)?;
        output.write_i32::<LE>(self.x)?;
        output.write_i32::<LE>(self.y)?;
        output.write_i8(self.amount_of_land_used_percent)?;
        output.write_i8(self.by_player_flag)?;
        output.write_u16::<LE>(0)?;
        output.write_i32::<LE>(self.radius)?;
        output.write_i32::<LE>(self.fade)?;
        output.write_i32::<LE>(self.clumpiness_factor)?;
        Ok(())
    }
}

/// A terrain placed in clumps on a random map.
#[derive(Debug, Default, Clone, Copy)]
pub struct RandomMapTerrain {
    /// Share of the map covered, in percent (genieutils' `Proportion`).
    pub percent: i32,
    /// The terrain placed.
    pub terrain_type: i32,
    /// Number of clumps.
    pub clumps: i32,
    /// Distance kept from the edges of other terrains (genieutils' `EdgeSpacing`).
    pub spacing: i32,
    /// The terrain on which the clumps are placed (genieutils' `PlacementTerrain`).
    pub base_terrain_type: i32,
    /// How clumpy the clumps are.
    pub clumpiness_factor: i32,
}

impl RandomMapTerrain {
    /// Read a map terrain from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from<R: Read>(input: &mut R) -> Result<Self> {
        Ok(Self {
            percent: input.read_i32::<LE>()?,
            terrain_type: input.read_i32::<LE>()?,
            clumps: input.read_i32::<LE>()?,
            spacing: input.read_i32::<LE>()?,
            base_terrain_type: input.read_i32::<LE>()?,
            clumpiness_factor: input.read_i32::<LE>()?,
        })
    }

    /// Write this map terrain to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    pub fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_i32::<LE>(self.percent)?;
        output.write_i32::<LE>(self.terrain_type)?;
        output.write_i32::<LE>(self.clumps)?;
        output.write_i32::<LE>(self.spacing)?;
        output.write_i32::<LE>(self.base_terrain_type)?;
        output.write_i32::<LE>(self.clumpiness_factor)?;
        Ok(())
    }
}

/// Objects placed in groups on a random map.
#[derive(Debug, Default, Clone, Copy)]
pub struct RandomMapObject {
    /// The unit type placed.
    pub unit_type: UnitTypeId,
    /// The terrain the objects are placed on (genieutils' `HostTerrain`).
    pub terrain_type: i32,
    /// How the group is placed (genieutils' `GroupPlacing`).
    pub group_flag: i8,
    /// genieutils' `ScaleFlag`: whether the count scales with the map size.
    pub scale_flag: i8,
    /// Objects per group.
    pub group_size: i32,
    /// Random variation of the group size (genieutils' `Fluctuation`).
    pub group_size_variance: i32,
    /// Groups per player.
    pub group_count: i32,
    /// Radius of a group (genieutils' `GroupArena`).
    pub group_area: i32,
    /// Who owns the objects (genieutils' `PlayerID`): -1 for each player, 0 for none.
    pub player_id: i32,
    /// genieutils reads this value as `SetPlaceForAllPlayers`; genie-rs takes it as a land ID.
    pub land_id: i32,
    /// Minimum distance from the players' start positions.
    pub min_distance_to_players: i32,
    /// Maximum distance from the players' start positions.
    pub max_distance_to_players: i32,
}

impl RandomMapObject {
    /// Read a map object entry from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    ///
    /// Also fails if the unit type ID does not fit a [`UnitTypeId`].
    pub fn read_from<R: Read>(input: &mut R) -> Result<Self> {
        let mut object = Self {
            unit_type: fits(input.read_u32::<LE>()?)?,
            terrain_type: input.read_i32::<LE>()?,
            group_flag: input.read_i8()?,
            scale_flag: input.read_i8()?,
            ..Default::default()
        };
        let _padding = input.read_u16::<LE>()?;
        object.group_size = input.read_i32::<LE>()?;
        object.group_size_variance = input.read_i32::<LE>()?;
        object.group_count = input.read_i32::<LE>()?;
        object.group_area = input.read_i32::<LE>()?;
        object.player_id = input.read_i32::<LE>()?;
        object.land_id = input.read_i32::<LE>()?;
        object.min_distance_to_players = input.read_i32::<LE>()?;
        object.max_distance_to_players = input.read_i32::<LE>()?;
        Ok(object)
    }

    /// Write this map object entry to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    pub fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_u32::<LE>(self.unit_type.into())?;
        output.write_i32::<LE>(self.terrain_type)?;
        output.write_i8(self.group_flag)?;
        output.write_i8(self.scale_flag)?;
        output.write_u16::<LE>(0)?;
        output.write_i32::<LE>(self.group_size)?;
        output.write_i32::<LE>(self.group_size_variance)?;
        output.write_i32::<LE>(self.group_count)?;
        output.write_i32::<LE>(self.group_area)?;
        output.write_i32::<LE>(self.player_id)?;
        output.write_i32::<LE>(self.land_id)?;
        output.write_i32::<LE>(self.min_distance_to_players)?;
        output.write_i32::<LE>(self.max_distance_to_players)?;
        Ok(())
    }
}

/// Hills placed in clumps on a random map.
///
/// genie-rs named the fields; genieutils and openage read the second, fourth, fifth and sixth
/// values with other meanings, as the field docs say.
#[derive(Debug, Default, Clone, Copy)]
pub struct RandomMapElevation {
    /// Share of the map raised, in percent (genieutils' `Proportion`).
    pub percent: i32,
    /// genieutils reads this value as `Terrain`: the terrain the hills are placed on.
    pub height: i32,
    /// Number of clumps (genieutils' `ClumpCount`).
    pub clumps: i32,
    /// genieutils reads this value as `BaseTerrain`.
    pub spacing: i32,
    /// genieutils reads this value as `BaseElevation`.
    pub base_terrain_type: i32,
    /// genieutils reads this value as `TileSpacing`.
    pub base_elevation: i32,
}

impl RandomMapElevation {
    /// Read a map elevation from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    ///
    /// Also fails if the stored percentage, read as a u32, is above `i32::MAX`.
    pub fn read_from<R: Read>(input: &mut R) -> Result<Self> {
        Ok(Self {
            percent: fits(input.read_u32::<LE>()?)?,
            height: input.read_i32::<LE>()?,
            clumps: input.read_i32::<LE>()?,
            spacing: input.read_i32::<LE>()?,
            base_terrain_type: input.read_i32::<LE>()?,
            base_elevation: input.read_i32::<LE>()?,
        })
    }

    /// Write this map elevation to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    pub fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_i32::<LE>(self.percent)?;
        output.write_i32::<LE>(self.height)?;
        output.write_i32::<LE>(self.clumps)?;
        output.write_i32::<LE>(self.spacing)?;
        output.write_i32::<LE>(self.base_terrain_type)?;
        output.write_i32::<LE>(self.base_elevation)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A map elevation's six words, read in file order and written back byte for byte.
    #[test]
    fn map_elevation_writes_back_its_six_words() {
        let bytes: Vec<u8> = [30i32, 2, 5, 3, -1, 0]
            .iter()
            .flat_map(|w| w.to_le_bytes())
            .collect();
        let elevation = RandomMapElevation::read_from(&mut bytes.as_slice()).unwrap();
        assert_eq!(
            (
                elevation.percent,
                elevation.height,
                elevation.clumps,
                elevation.spacing,
                elevation.base_terrain_type,
                elevation.base_elevation
            ),
            (30, 2, 5, 3, -1, 0)
        );
        let mut out = vec![];
        elevation.write_to(&mut out).unwrap();
        assert_eq!(out, bytes);
    }
}
