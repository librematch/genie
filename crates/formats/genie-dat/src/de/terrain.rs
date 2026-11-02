//! The terrain restrictions and the terrain block: the map settings, the tile shapes and the
//! terrains.

use crate::de::{DebugString, Version};
use binrw::binrw;

/// A terrain restriction: for each terrain, whether a unit using it may move or build there, and
/// the damage multiplier it takes there.
#[binrw]
#[brw(little)]
#[br(import(terrains: i16))]
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainRestriction {
    /// One value per terrain: 0 is impassable, above 0 passable, above 0.05 also buildable; a unit
    /// whose armour uses this restriction takes damage multiplied by the value.
    #[br(count = terrains)]
    pub passable_buildable_dmg_multiplier: Vec<f32>,
    /// One set of walking graphics per terrain.
    #[br(count = terrains)]
    pub terrain_pass_graphics: Vec<TerrainPassGraphic>,
}

/// The graphics of a unit entering, leaving and walking on one terrain.
#[binrw]
#[brw(little)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainPassGraphic {
    /// Graphic shown when leaving the terrain.
    pub exit_tile_sprite_id: i32,
    /// Graphic shown when entering the terrain.
    pub enter_tile_sprite_id: i32,
    /// Graphic shown while walking on the terrain.
    pub walk_tile_sprite_id: i32,
    /// Rate of the walking graphic; its unit is not known yet.
    pub walk_sprite_rate: i32,
}

/// genieutils' `SharedTerrain::TILE_TYPE_COUNT`.
pub(super) const TILE_TYPES: usize = 19;
/// genieutils' `Terrain::getTerrainCount` for `GV_C8` and later.
pub(super) const TERRAINS: usize = 200;
/// genieutils' `Terrain::TERRAIN_UNITS_SIZE`.
pub(super) const TERRAIN_UNITS: usize = 30;

/// The map settings and the terrain definitions. Many fields are the in-memory map state the file
/// was saved from.
#[binrw]
#[brw(little)]
#[br(import(v: Version))]
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainBlock {
    /// An in-memory pointer the game saved with the file.
    pub virtual_function_ptr: u32,
    /// An in-memory pointer the game saved with the file.
    pub map_pointer: u32,
    /// Map width in tiles (saved map state).
    pub map_width: i32,
    /// Map height in tiles (saved map state).
    pub map_height: i32,
    /// World width (saved map state).
    pub world_width: i32,
    /// World height (saved map state).
    pub world_height: i32,
    /// The size of each of the 19 tile shapes: flat and the slopes.
    pub tile_sizes: [TileSize; TILE_TYPES],
    /// Padding after the tile sizes; kept to write the file back.
    pub padding_ts: i16,
    /// The 200 terrain slots, indexed by terrain id.
    #[br(count = TERRAINS)]
    pub terrains: Vec<Terrain>,
    /// Lowest x of the map (saved map state).
    pub map_min_x: f32,
    /// Lowest y of the map (saved map state).
    pub map_min_y: f32,
    /// Highest x of the map (saved map state).
    pub map_max_x: f32,
    /// Highest y of the map (saved map state).
    pub map_max_y: f32,
    /// Highest x of the map plus one (saved map state).
    pub map_max_plus_one_x: f32,
    /// Highest y of the map plus one (saved map state).
    pub map_max_plus_one_y: f32,
    /// Number of terrains in use.
    pub terrains_used2: i16,
    /// Number of terrain borders in use.
    pub borders_used: i16,
    /// Not known yet; read as an i16 after the borders count.
    pub max_terrain: i16,
    /// Width of a tile in pixels.
    pub tile_width: i16,
    /// Height of a tile in pixels.
    pub tile_height: i16,
    /// Half the tile height in pixels.
    pub tile_half_height: i16,
    /// Half the tile width in pixels.
    pub tile_half_width: i16,
    /// Height of one elevation level in pixels.
    pub elev_height: i16,
    /// Current row of the map view (saved map state).
    pub cur_row: i16,
    /// Current column of the map view (saved map state).
    pub cur_col: i16,
    /// First row of the drawn block (saved map state).
    pub block_beg_row: i16,
    /// Last row of the drawn block (saved map state).
    pub block_end_row: i16,
    /// First column of the drawn block (saved map state).
    pub block_beg_col: i16,
    /// Last column of the drawn block (saved map state).
    pub block_end_col: i16,
    /// An in-memory pointer the game saved with the file.
    pub search_map_ptr: u32,
    /// An in-memory pointer the game saved with the file.
    pub search_map_rows_ptr: u32,
    /// Whether any terrain animation frame changed (saved map state).
    pub any_frame_change: u8,
    /// Whether the whole map is revealed (saved map state).
    pub map_visible_flag: u8,
    /// Whether fog of war is on (saved map state).
    pub fog_flag: u8,
}

/// The size of one tile shape (flat, or one of the slopes).
#[binrw]
#[brw(little)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct TileSize {
    /// Width in pixels.
    pub width: i16,
    /// Height in pixels.
    pub height: i16,
    /// Vertical offset in pixels.
    pub delta_y: i16,
}

/// The frames of a terrain's graphic that draw one tile shape.
#[binrw]
#[brw(little)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct FrameData {
    /// Number of frames.
    pub frame_count: i16,
    /// Number of angles.
    pub angle_count: i16,
    /// Index of the first frame.
    pub shape_id: i16,
}

/// A terrain definition.
#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq)]
pub struct Terrain {
    /// Whether the slot holds a terrain.
    pub enabled: u8,
    /// Not known yet; read as a u8 flag.
    pub random: u8,
    /// Whether the terrain is water.
    pub is_water: u8,
    /// Whether the scenario editor hides the terrain.
    pub hide_in_editor: u8,
    /// Language string id of the terrain's name.
    pub string_id: i32,
    /// Internal name.
    pub name: DebugString,
    /// Name of the terrain's texture file.
    pub name2: DebugString,
    /// Id of the terrain's SLP; -1 for none.
    pub slp: i32,
    /// An in-memory pointer the game saved with the file.
    pub shape_ptr: i32,
    /// Sound of the terrain; -1 for none.
    pub sound_id: i32,
    /// Wwise event id that starts the terrain's sound.
    pub wwise_sound_id: u32,
    /// Wwise event id that stops the terrain's sound.
    pub wwise_sound_stop_id: u32,
    /// Blend priority: where two terrains meet, the higher one blends over the other.
    pub blend_priority: i32,
    /// Blend type: which set of blending masks the terrain uses.
    pub blend_type: i32,
    /// Name of the terrain's overlay mask texture.
    pub overlay_mask_name: DebugString,
    /// Minimap palette indices: high, medium and low.
    pub colors: [u8; 3],
    /// Minimap palette indices of cliffs: left and right.
    pub cliff_colors: [u8; 2],
    /// Not known yet; read as a terrain id (u8).
    pub passable_terrain: u8,
    /// Not known yet; read as a terrain id (u8).
    pub impassable_terrain: u8,
    /// Whether the terrain is animated.
    pub is_animated: u8,
    /// Number of animation frames.
    pub animation_frames: i16,
    /// Number of frames to pause after the last one.
    pub pause_frames: i16,
    /// Time between animation frames.
    pub interval: f32,
    /// Pause between animation loops.
    pub pause_between_loops: f32,
    /// Current animation frame (saved map state).
    pub frame: i16,
    /// Current frame to draw (saved map state).
    pub draw_frame: i16,
    /// Time the animation frame last changed (saved map state).
    pub animate_last: f32,
    /// Whether the frame changed since the terrain was drawn (saved map state).
    pub frame_changed: u8,
    /// Whether the terrain was drawn (saved map state).
    pub drawn: u8,
    /// The frames of each of the 19 tile shapes.
    pub elevation_graphics: [FrameData; TILE_TYPES],
    /// Terrain whose graphic is drawn in this one's place; -1 for its own.
    pub terrain_to_draw: i16,
    /// Rows and columns of the terrain's texture.
    pub terrain_dimensions: [i16; 2],
    /// Per terrain unit: a density whose difference to `terrain_unit_density` is not known yet.
    pub terrain_unit_masked_density: [i16; TERRAIN_UNITS],
    /// Units placed with the terrain (e.g. a forest's trees); -1 for none.
    pub terrain_unit_id: [i16; TERRAIN_UNITS],
    /// Per terrain unit: how densely it is placed.
    pub terrain_unit_density: [i16; TERRAIN_UNITS],
    /// Per terrain unit: whether it is centred on its tile.
    pub terrain_unit_centering: [u8; TERRAIN_UNITS],
    /// Number of the terrain-unit entries in use.
    pub number_of_terrain_units_used: i16,
    /// Not known yet; read as an i16.
    pub phantom: i16,
}
