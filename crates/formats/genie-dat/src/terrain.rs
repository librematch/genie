//! Types related to terrains.

use crate::FileVersion;
use crate::conv::invalid_data;
use crate::conv::opt_fits;
use crate::sound::SoundId;
use crate::sprite::{GraphicId, SpriteId};
use crate::unit_type::UnitTypeId;
use arrayvec::ArrayString;
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use genie_support::{
    MapInto, fallible_try_from, fallible_try_into, infallible_try_into, read_opt_u16, read_opt_u32,
};
use std::io::{Read, Result, Write};

/// An ID identifying a terrain.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TerrainId(u16);

impl From<u8> for TerrainId {
    fn from(n: u8) -> Self {
        Self(n.into())
    }
}

impl From<u16> for TerrainId {
    fn from(n: u16) -> Self {
        Self(n)
    }
}

impl From<TerrainId> for u16 {
    fn from(n: TerrainId) -> Self {
        n.0
    }
}

impl From<TerrainId> for usize {
    fn from(n: TerrainId) -> Self {
        n.0.into()
    }
}

fallible_try_into!(TerrainId, i16);
infallible_try_into!(TerrainId, u32);
fallible_try_from!(TerrainId, i32);
fallible_try_from!(TerrainId, u32);

type TerrainName = ArrayString<13>;

/// The graphics of one terrain under a terrain restriction: the sprites drawn when a unit leaves,
/// enters and walks on a tile of that terrain (genieutils' `TerrainPassGraphic`).
#[derive(Debug, Default, Clone, Copy)]
pub struct TerrainPassGraphic {
    exit_tile_sprite: Option<SpriteId>,
    enter_tile_sprite: Option<SpriteId>,
    walk_tile_sprite: Option<SpriteId>,
    walk_rate: Option<f32>,
    replication_amount: Option<i32>,
}

/// A terrain restriction (terrain table): for each terrain, whether and how well the units that use
/// this table can be on it, and the graphics they use there.
#[derive(Debug, Clone)]
pub struct TerrainRestriction {
    passability: Vec<f32>,
    pass_graphics: Vec<TerrainPassGraphic>,
}

/// The pixel size of one of the 19 tile shapes (the flat tile and the slopes).
#[derive(Debug, Default, Clone, Copy)]
pub struct TileSize {
    /// Width in pixels.
    pub width: i16,
    /// Height in pixels.
    pub height: i16,
    /// Vertical pixel offset of the shape (genieutils' `DeltaY`).
    pub delta_z: i16,
}

/// How a terrain or terrain border is animated (e.g. water).
#[derive(Debug, Default, Clone, Copy)]
pub struct TerrainAnimation {
    /// Whether the terrain is animated.
    pub enabled: bool,
    num_frames: i16,
    num_pause_frames: i16,
    frame_interval: f32,
    replay_delay: f32,
    frame: i16,
    draw_frame: i16,
    animate_last: f32,
    frame_changed: bool,
    drawn: bool,
}

/// Which frames of a terrain's SLP draw one tile shape (genieutils' `FrameData`).
#[derive(Debug, Default, Clone, Copy)]
pub struct TerrainSpriteFrame {
    /// Number of frames (variations) of the shape.
    pub num_frames: i16,
    /// Number of facets (angles) of the shape.
    pub num_facets: i16,
    /// Index of the shape's first frame in the SLP (genieutils' `ShapeID`).
    pub frame_id: i16,
}

/// An object that is placed on a terrain when the map is generated (e.g. the trees of a forest).
#[derive(Debug, Default, Clone, Copy)]
pub struct TerrainObject {
    /// The unit type placed.
    pub object_id: UnitTypeId,
    /// How densely it is placed.
    pub density: i16,
    /// Whether the object is centred on its tile (genieutils' `TerrainUnitCentering`).
    pub placement_flag: i8,
}

/// A terrain type: its graphics, sound, minimap colours, animation and the objects placed on it.
#[derive(Debug, Default, Clone)]
pub struct Terrain {
    /// Is this terrain enabled?
    pub enabled: bool,
    random: u8,
    /// Internal name of the terrain.
    name: TerrainName,
    /// Internal name of the SLP graphic.
    slp_name: TerrainName,
    /// SLP graphic ID for this terrain.
    pub slp_id: Option<GraphicId>,
    /// The Sound ID for this terrain.
    pub sound_id: Option<SoundId>,
    wwise_sound_id: Option<u32>,
    wwise_stop_sound_id: Option<u32>,
    blend_priority: Option<i32>,
    blend_mode: Option<i32>,
    /// The colour tiles with this terrain will have on the minimap when on a downhill slope.
    pub minimap_color_high: u8,
    /// The colour tiles with this terrain will have on the minimap when on a flat tile.
    pub minimap_color_medium: u8,
    /// The colour tiles with this terrain will have on the minimap when on an uphill slope.
    pub minimap_color_low: u8,
    /// The colour tiles with this terrain will have on the minimap when next to a cliff.
    pub minimap_color_cliff_lt: u8,
    /// The colour tiles with this terrain will have on the minimap when next to a cliff.
    pub minimap_color_cliff_rt: u8,
    /// A terrain ID (genieutils' `PassableTerrain`), or `None` (0xFF); its use is not known yet.
    pub passable_terrain_id: Option<u8>,
    /// A terrain ID (genieutils' `ImpassableTerrain`), or `None` (0xFF); its use is not known yet.
    pub impassable_terrain_id: Option<u8>,
    /// How the terrain is animated.
    pub animation: TerrainAnimation,
    /// The frames of each of the 19 tile shapes (the flat tile and the slopes).
    pub elevation_sprites: Vec<TerrainSpriteFrame>,
    /// The terrain whose graphics are drawn for this one, or `None`.
    pub terrain_id_to_draw: Option<TerrainId>,
    rows: i16,
    cols: i16,
    /// For each terrain, the terrain border drawn between this terrain and that one.
    pub borders: Vec<i16>,
    /// The 30 slots of objects placed on this terrain. All slots are kept: the file's count of
    /// used slots is not applied when reading.
    pub terrain_objects: Vec<TerrainObject>,
}

/// A terrain border: the graphic that blends two adjacent terrains.
#[derive(Debug, Default, Clone)]
pub struct TerrainBorder {
    /// Whether this border is used.
    pub enabled: bool,
    random: u8,
    name: TerrainName,
    slp_name: TerrainName,
    /// SLP graphic ID of the border, or `None`.
    pub slp_id: Option<GraphicId>,
    /// Sound ID of the border, or `None`.
    pub sound_id: Option<SoundId>,
    /// Three palette indices (genieutils' `Colors`), like a terrain's minimap colours.
    pub color: (u8, u8, u8),
    /// How the border is animated.
    pub animation: TerrainAnimation,
    /// For each of the 19 tile shapes, its 12 frame entries.
    pub frames: Vec<Vec<TerrainSpriteFrame>>,
    /// Unused according to Chariot.
    draw_tile: i8,
    /// The terrain drawn under the border, or `None`.
    pub underlay_terrain: Option<i16>,
    /// genieutils' `BorderStyle`; its values are not known yet.
    pub border_style: i16,
}

impl TerrainPassGraphic {
    /// Read the pass graphics of one terrain from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails, or if a sprite ID is out of range.
    pub fn read_from(mut input: impl Read, version: FileVersion) -> Result<Self> {
        let mut pass = Self {
            exit_tile_sprite: read_opt_u32(&mut input)?,
            enter_tile_sprite: read_opt_u32(&mut input)?,
            walk_tile_sprite: read_opt_u32(&mut input)?,
            ..Default::default()
        };
        if version.is_swgb() {
            pass.walk_rate = Some(input.read_f32::<LE>()?);
        } else {
            pass.replication_amount = Some(input.read_i32::<LE>()?);
        }
        Ok(pass)
    }

    /// Serialize this object to a binary output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    pub fn write_to(&self, mut output: impl Write, version: FileVersion) -> Result<()> {
        output.write_i32::<LE>(self.exit_tile_sprite.map_into().unwrap_or(-1))?;
        output.write_i32::<LE>(self.enter_tile_sprite.map_into().unwrap_or(-1))?;
        output.write_i32::<LE>(self.walk_tile_sprite.map_into().unwrap_or(-1))?;
        // TODO decide on correct default values for these
        if version.is_swgb() {
            output.write_f32::<LE>(self.walk_rate.unwrap_or(0.0))?;
        } else {
            output.write_i32::<LE>(self.replication_amount.unwrap_or(-1))?;
        }
        Ok(())
    }
}

impl TerrainRestriction {
    /// Read a terrain restriction for `num_terrains` terrains from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails, or if a sprite ID is out of range.
    pub fn read_from(
        mut input: impl Read,
        version: FileVersion,
        num_terrains: u16,
    ) -> Result<Self> {
        let mut passability = vec![0.0; num_terrains as usize];
        for value in &mut passability {
            *value = input.read_f32::<LE>()?;
        }

        // Apparently AoK+ only
        let mut pass_graphics = Vec::with_capacity(num_terrains as usize);
        for _ in 0..num_terrains {
            pass_graphics.push(TerrainPassGraphic::read_from(&mut input, version)?);
        }

        Ok(Self {
            passability,
            pass_graphics,
        })
    }

    /// Serialize this object to a binary output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    ///
    /// Also fails if the restriction does not hold exactly `num_terrains` entries.
    pub fn write_to(
        &self,
        mut output: impl Write,
        version: FileVersion,
        num_terrains: u16,
    ) -> Result<()> {
        if self.passability.len() != usize::from(num_terrains)
            || self.pass_graphics.len() != usize::from(num_terrains)
        {
            return Err(invalid_data(
                "a terrain table that does not match the terrain count",
            ));
        }
        for value in &self.passability {
            output.write_f32::<LE>(*value)?;
        }
        for graphic in &self.pass_graphics {
            graphic.write_to(&mut output, version)?;
        }
        Ok(())
    }
}

impl TileSize {
    /// Read a tile size from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from<R: Read>(input: &mut R) -> Result<Self> {
        let width = input.read_i16::<LE>()?;
        let height = input.read_i16::<LE>()?;
        let delta_z = input.read_i16::<LE>()?;
        Ok(Self {
            width,
            height,
            delta_z,
        })
    }

    /// Serialize this object to a binary output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    pub fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_i16::<LE>(self.width)?;
        output.write_i16::<LE>(self.height)?;
        output.write_i16::<LE>(self.delta_z)?;
        Ok(())
    }
}

impl TerrainAnimation {
    /// Read animation settings from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from<R: Read>(input: &mut R) -> Result<Self> {
        Ok(Self {
            enabled: input.read_u8()? != 0,
            num_frames: input.read_i16::<LE>()?,
            num_pause_frames: input.read_i16::<LE>()?,
            frame_interval: input.read_f32::<LE>()?,
            replay_delay: input.read_f32::<LE>()?,
            frame: input.read_i16::<LE>()?,
            draw_frame: input.read_i16::<LE>()?,
            animate_last: input.read_f32::<LE>()?,
            frame_changed: input.read_u8()? != 0,
            drawn: input.read_u8()? != 0,
        })
    }

    /// Serialize this object to a binary output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    pub fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_u8(u8::from(self.enabled))?;
        output.write_i16::<LE>(self.num_frames)?;
        output.write_i16::<LE>(self.num_pause_frames)?;
        output.write_f32::<LE>(self.frame_interval)?;
        output.write_f32::<LE>(self.replay_delay)?;
        output.write_i16::<LE>(self.frame)?;
        output.write_i16::<LE>(self.draw_frame)?;
        output.write_f32::<LE>(self.animate_last)?;
        output.write_u8(u8::from(self.frame_changed))?;
        output.write_u8(u8::from(self.drawn))?;
        Ok(())
    }
}

impl TerrainSpriteFrame {
    /// Read a tile shape's frame data from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails.
    pub fn read_from<R: Read>(input: &mut R) -> Result<Self> {
        let num_frames = input.read_i16::<LE>()?;
        let num_facets = input.read_i16::<LE>()?;
        let frame_id = input.read_i16::<LE>()?;
        Ok(Self {
            num_frames,
            num_facets,
            frame_id,
        })
    }

    /// Serialize this object to a binary output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    pub fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_i16::<LE>(self.num_frames)?;
        output.write_i16::<LE>(self.num_facets)?;
        output.write_i16::<LE>(self.frame_id)?;
        Ok(())
    }
}

impl Terrain {
    /// Get the internal name of this terrain.
    #[must_use]
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Read a Terrain object from an input stream; `num_terrains` is the number of border entries.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails, or if an ID is out of range.
    pub fn read_from(
        mut input: impl Read,
        version: FileVersion,
        num_terrains: u16,
    ) -> Result<Self> {
        let mut terrain = Self {
            enabled: input.read_u8()? != 0,
            random: input.read_u8()?,
            ..Default::default()
        };
        read_terrain_name(&mut input, &mut terrain.name)?;
        read_terrain_name(&mut input, &mut terrain.slp_name)?;
        // println!("{}", terrain.name);
        terrain.slp_id = read_opt_u32(&mut input)?;
        let _slp_pointer = input.read_i32::<LE>()?;
        terrain.sound_id = read_opt_u32(&mut input)?;
        if version.is_de2() {
            terrain.wwise_sound_id = read_opt_u32(&mut input)?;
            terrain.wwise_stop_sound_id = read_opt_u32(&mut input)?;
        } else {
            terrain.blend_priority = Some(input.read_i32::<LE>()?);
            terrain.blend_mode = Some(input.read_i32::<LE>()?);
        }
        terrain.read_minimap(&mut input)?;
        terrain.animation = TerrainAnimation::read_from(&mut input)?;
        for _ in 0..19 {
            terrain
                .elevation_sprites
                .push(TerrainSpriteFrame::read_from(&mut input)?);
        }
        terrain.terrain_id_to_draw = read_opt_u16(&mut input)?;
        terrain.rows = input.read_i16::<LE>()?;
        terrain.cols = input.read_i16::<LE>()?;
        for _ in 0..num_terrains {
            terrain.borders.push(input.read_i16::<LE>()?);
        }

        terrain.terrain_objects = read_terrain_objects(&mut input)?;
        let _padding = input.read_u16::<LE>()?;

        Ok(terrain)
    }

    /// The minimap's five colours and the passable and impassable terrains (0xFF none).
    fn read_minimap(&mut self, mut input: impl Read) -> Result<()> {
        self.minimap_color_high = input.read_u8()?;
        self.minimap_color_medium = input.read_u8()?;
        self.minimap_color_low = input.read_u8()?;
        self.minimap_color_cliff_lt = input.read_u8()?;
        self.minimap_color_cliff_rt = input.read_u8()?;
        self.passable_terrain_id = match input.read_u8()? {
            0xFF => None,
            id => Some(id),
        };
        self.impassable_terrain_id = match input.read_u8()? {
            0xFF => None,
            id => Some(id),
        };
        Ok(())
    }

    /// The minimap's five colours and the passable and impassable terrains (0xFF none).
    fn write_minimap<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_u8(self.minimap_color_high)?;
        output.write_u8(self.minimap_color_medium)?;
        output.write_u8(self.minimap_color_low)?;
        output.write_u8(self.minimap_color_cliff_lt)?;
        output.write_u8(self.minimap_color_cliff_rt)?;
        output.write_u8(self.passable_terrain_id.unwrap_or(0xFF))?;
        output.write_u8(self.impassable_terrain_id.unwrap_or(0xFF))?;
        Ok(())
    }

    /// Serialize this object to a binary output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    ///
    /// Also fails if there are not exactly `num_terrains` borders, or if the SLP ID or the terrain to
    /// draw does not fit the format's signed field.
    pub fn write_to<W: Write>(
        &self,
        output: &mut W,
        _version: FileVersion,
        num_terrains: u16,
    ) -> Result<()> {
        if self.borders.len() != usize::from(num_terrains) {
            return Err(invalid_data(
                "a terrain border list that does not match the terrain count",
            ));
        }
        output.write_u8(u8::from(self.enabled))?;
        output.write_u8(self.random)?;
        write_terrain_name(output, &self.name)?;
        write_terrain_name(output, &self.slp_name)?;
        output.write_i32::<LE>(opt_fits(self.slp_id, -1)?)?;
        output.write_i32::<LE>(0)?; // slp pointer
        output.write_i32::<LE>(self.sound_id.map_or(-1, i32::from))?;
        output.write_i32::<LE>(self.blend_priority.unwrap_or(-1))?;
        output.write_i32::<LE>(self.blend_mode.unwrap_or(-1))?;
        self.write_minimap(output)?;
        self.animation.write_to(output)?;
        for frame in &self.elevation_sprites {
            frame.write_to(output)?;
        }
        output.write_i16::<LE>(opt_fits(self.terrain_id_to_draw, -1)?)?;
        output.write_i16::<LE>(self.rows)?;
        output.write_i16::<LE>(self.cols)?;
        for border in &self.borders {
            output.write_i16::<LE>(*border)?;
        }

        write_terrain_objects(output, &self.terrain_objects)?;
        output.write_u16::<LE>(0)?; // padding

        Ok(())
    }
}

/// A terrain's 30 objects: their ids, densities and placement flags, each in a block, then a
/// count the files hold as 0 (read past).
fn read_terrain_objects(mut input: impl Read) -> Result<Vec<TerrainObject>> {
    let mut terrain_objects = vec![TerrainObject::default(); 30];
    for object in &mut terrain_objects {
        object.object_id = input.read_u16::<LE>()?.into();
    }
    for object in &mut terrain_objects {
        object.density = input.read_i16::<LE>()?;
    }
    for object in &mut terrain_objects {
        object.placement_flag = input.read_i8()?;
    }
    let _num_terrain_objects = input.read_u16::<LE>()?;
    // Why is num_terrain_objects always 0?
    // terrain_objects.truncate(num_terrain_objects as usize);
    Ok(terrain_objects)
}

/// A terrain's 30 object slots (0 past its objects) in three blocks, then their count.
// The format stores the count of terrain objects as a u16 (truncated, as before).
#[allow(clippy::cast_possible_truncation)]
fn write_terrain_objects<W: Write>(output: &mut W, objects: &[TerrainObject]) -> Result<()> {
    for index in 0..30 {
        if let Some(object) = objects.get(index) {
            output.write_u16::<LE>(object.object_id.into())?;
        } else {
            output.write_u16::<LE>(0)?;
        }
    }
    for index in 0..30 {
        if let Some(object) = objects.get(index) {
            output.write_i16::<LE>(object.density)?;
        } else {
            output.write_i16::<LE>(0)?;
        }
    }
    for index in 0..30 {
        if let Some(object) = objects.get(index) {
            output.write_i8(object.placement_flag)?;
        } else {
            output.write_i8(0)?;
        }
    }
    output.write_u16::<LE>(objects.len() as u16)
}

impl TerrainBorder {
    /// Read a terrain border from an input stream.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from `input` fails, or if an ID is out of range.
    pub fn read_from(mut input: impl Read) -> Result<Self> {
        let mut border = Self {
            enabled: input.read_u8()? != 0,
            random: input.read_u8()?,
            ..Default::default()
        };
        read_terrain_name(&mut input, &mut border.name)?;
        read_terrain_name(&mut input, &mut border.slp_name)?;
        border.slp_id = read_opt_u32(&mut input)?;
        let _slp_pointer = input.read_i32::<LE>()?;
        border.sound_id = read_opt_u32(&mut input)?;
        border.color = (input.read_u8()?, input.read_u8()?, input.read_u8()?);
        border.animation = TerrainAnimation::read_from(&mut input)?;
        for _ in 0..19 {
            let mut frames_list = vec![TerrainSpriteFrame::default(); 12];
            for frame in &mut frames_list {
                *frame = TerrainSpriteFrame::read_from(&mut input)?;
            }
            border.frames.push(frames_list);
        }

        border.draw_tile = input.read_i8()?;
        let _padding = input.read_u8()?;
        border.underlay_terrain = read_opt_u16(&mut input)?;
        border.border_style = input.read_i16::<LE>()?;

        Ok(border)
    }

    /// Serialize this object to a binary output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    ///
    /// Also fails if the SLP ID does not fit the format's `i32`.
    pub fn write_to<W: Write>(&self, output: &mut W) -> Result<()> {
        output.write_u8(u8::from(self.enabled))?;
        output.write_u8(self.random)?;
        write_terrain_name(output, &self.name)?;
        write_terrain_name(output, &self.slp_name)?;
        output.write_i32::<LE>(opt_fits(self.slp_id, -1)?)?;
        output.write_i32::<LE>(0)?; // slp pointer
        output.write_i32::<LE>(self.sound_id.map_or(-1, i32::from))?;
        output.write_u8(self.color.0)?;
        output.write_u8(self.color.1)?;
        output.write_u8(self.color.2)?;
        self.animation.write_to(output)?;
        for frames_list in &self.frames {
            for frame in frames_list {
                frame.write_to(output)?;
            }
        }
        output.write_i8(self.draw_tile)?;
        output.write_u8(0)?; // padding
        output.write_i16::<LE>(self.underlay_terrain.unwrap_or(-1))?;
        output.write_i16::<LE>(self.border_style)?;
        Ok(())
    }
}

fn read_terrain_name<R: Read>(input: &mut R, output: &mut TerrainName) -> Result<()> {
    let bytes = &mut [0; 13];
    input.read_exact(bytes)?;
    bytes
        .iter()
        .copied()
        .take_while(|b| *b != 0)
        .map(char::from)
        .for_each(|c| output.push(c));
    Ok(())
}

fn write_terrain_name<W: Write>(output: &mut W, name: &TerrainName) -> Result<()> {
    let bytes = &mut [0; 13];
    bytes[..name.len()].copy_from_slice(name.as_bytes());
    output.write_all(bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A terrain's objects: 30 slots in three blocks (0 past the objects given), then their
    /// count, which the reader passes over; read back, the 30 slots.
    #[test]
    fn terrain_objects_fill_thirty_slots() {
        let objects = [TerrainObject {
            object_id: 59.into(),
            density: 3,
            placement_flag: 1,
        }];
        let mut bytes = vec![];
        write_terrain_objects(&mut bytes, &objects).unwrap();
        assert_eq!(bytes.len(), 30 * 5 + 2);
        assert_eq!(bytes[bytes.len() - 2..], [1, 0]);
        let read = read_terrain_objects(bytes.as_slice()).unwrap();
        assert_eq!(read.len(), 30);
        assert_eq!(
            (
                u16::from(read[0].object_id),
                read[0].density,
                read[0].placement_flag
            ),
            (59, 3, 1)
        );
        assert_eq!((u16::from(read[1].object_id), read[29].density), (0, 0));
    }

    /// The minimap's colours and the passable and impassable terrains, 0xFF for none.
    #[test]
    fn terrain_minimap_reads_back_with_none_as_0xff() {
        let terrain = Terrain {
            minimap_color_high: 1,
            minimap_color_cliff_rt: 5,
            passable_terrain_id: Some(2),
            impassable_terrain_id: None,
            ..Terrain::default()
        };
        let mut bytes = vec![];
        terrain.write_minimap(&mut bytes).unwrap();
        assert_eq!(bytes, [1, 0, 0, 0, 5, 2, 0xff]);
        let mut read = Terrain::default();
        read.read_minimap(bytes.as_slice()).unwrap();
        assert_eq!(
            (
                read.minimap_color_cliff_rt,
                read.passable_terrain_id,
                read.impassable_terrain_id
            ),
            (5, Some(2), None)
        );
    }
}
