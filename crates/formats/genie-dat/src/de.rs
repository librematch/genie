//! The data file of Age of Empires II: Definitive Edition, file versions VER 7.1 to VER 8.9.
//!
//! Ported from genieutils (github.com/Tapsa/genieutils, LGPL-3.0; the library the game ships with
//! Advanced Genie Editor in `Tools_Builds/docs/Source.7z`): every struct follows its class's
//! `serializeObject`, and every version gate compares genieutils' `GameVersion` ordinals, of
//! which the DE files take GV_C14 (VER 7.1) to GV_C32 (VER 8.9). Gates that are constant over that
//! range (the AoE, AoK and SWGB ones) are folded.
//!
//! Counts are kept as the file stores them, so a file reads and writes back byte for byte; a
//! caller that changes a list's length updates its count. The file on disk is raw-deflate
//! compressed: [`DatFile::read_from`] inflates, [`DatFile::to_bytes`] gives the inflated bytes.

use binrw::{BinRead, BinResult, BinWrite, Endian, binrw};
use flate2::read::DeflateDecoder;
use std::io::{Cursor, Read, Seek, Write};

/// genieutils' `GameVersion` ordinal of a DE file: VER 7.1 is C14, VER 8.9 is C32.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub u8);

impl Version {
    /// The version of a file version string (`VER 7.1` to `VER 8.9`).
    pub fn of(file_version: &[u8; 8]) -> Option<Version> {
        const VERSIONS: [&[u8; 7]; 19] = [
            b"VER 7.1", b"VER 7.2", b"VER 7.3", b"VER 7.4", b"VER 7.5", b"VER 7.6", b"VER 7.7",
            b"VER 7.8", b"VER 7.9", b"VER 8.0", b"VER 8.1", b"VER 8.2", b"VER 8.3", b"VER 8.4",
            b"VER 8.5", b"VER 8.6", b"VER 8.7", b"VER 8.8", b"VER 8.9",
        ];
        VERSIONS
            .iter()
            .position(|v| file_version[..7] == v[..])
            .map(|i| Version(14 + i as u8))
    }

    /// Whether the file is at least genieutils' GV_C`n`.
    pub fn at_least(self, n: u8) -> bool {
        self.0 >= n
    }
}

/// Why a file could not be read.
#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    /// The file version is not one of VER 7.1 to VER 8.9.
    UnsupportedVersion([u8; 8]),
    Parse(binrw::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::UnsupportedVersion(v) => {
                write!(
                    f,
                    "unsupported file version {:?}",
                    String::from_utf8_lossy(v)
                )
            }
            Error::Parse(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<binrw::Error> for Error {
    fn from(e: binrw::Error) -> Self {
        Error::Parse(e)
    }
}

/// A string as DE stores it (`serializeDebugString`): the marker 0x0A60, a u16 length, the bytes.
#[binrw]
#[brw(little, magic = 0x0A60u16)]
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct DebugString {
    pub len: u16,
    #[br(count = len)]
    pub bytes: Vec<u8>,
}

impl DebugString {
    pub fn as_str(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.bytes)
    }
}

/// Read `pointers.len()` entries, one for each non-zero pointer (`serializeSubWithPointers`).
fn read_present<T, A, R>(
    reader: &mut R,
    endian: Endian,
    pointers: &[i32],
    args: A,
) -> BinResult<Vec<Option<T>>>
where
    T: for<'a> BinRead<Args<'a> = A>,
    A: Clone,
    R: Read + Seek,
{
    pointers
        .iter()
        .map(|&p| {
            (p != 0)
                .then(|| T::read_options(reader, endian, args.clone()))
                .transpose()
        })
        .collect()
}

/// Write the present entries, in order.
fn write_present<T, W>(list: &[Option<T>], writer: &mut W, endian: Endian) -> BinResult<()>
where
    T: for<'a> BinWrite<Args<'a> = ()>,
    W: Write + Seek,
{
    for entry in list.iter().flatten() {
        entry.write_options(writer, endian, ())?;
    }
    Ok(())
}

/// The whole file, inflated.
#[binrw]
#[brw(little)]
#[br(import(v: Version))]
#[derive(Clone, Debug, PartialEq)]
pub struct DatFile {
    pub file_version: [u8; 8],
    pub terrain_restriction_count: i16,
    pub terrains_used: i16,
    #[br(count = terrain_restriction_count)]
    pub float_ptr_terrain_tables: Vec<i32>,
    #[br(count = terrain_restriction_count)]
    pub terrain_pass_graphic_pointers: Vec<i32>,
    #[br(count = terrain_restriction_count, args { inner: (terrains_used,) })]
    pub terrain_restrictions: Vec<TerrainRestriction>,
    pub player_colour_count: i16,
    #[br(count = player_colour_count)]
    pub player_colours: Vec<PlayerColour>,
    pub sound_count: i16,
    #[br(count = sound_count)]
    pub sounds: Vec<Sound>,
    pub graphic_count: i16,
    #[br(count = graphic_count)]
    pub graphic_pointers: Vec<i32>,
    #[br(parse_with = |r, e, _: ()| read_present(r, e, &graphic_pointers, ()))]
    #[bw(write_with = |l, w, e, _: ()| write_present(l, w, e))]
    pub graphics: Vec<Option<Graphic>>,
    #[br(args(v))]
    pub terrain_block: TerrainBlock,
    pub random_maps: RandomMaps,
    pub effect_count: i32,
    #[br(count = effect_count)]
    pub effects: Vec<Effect>,
    pub unit_header_count: i32,
    #[br(count = unit_header_count, args { inner: (v,) })]
    pub unit_headers: Vec<UnitHeader>,
    pub civ_count: i16,
    #[br(count = civ_count, args { inner: (v,) })]
    pub civs: Vec<Civ>,
    pub tech_count: i16,
    #[br(count = tech_count, args { inner: (v,) })]
    pub techs: Vec<Tech>,
    pub time_slice: i32,
    pub unit_kill_rate: i32,
    pub unit_kill_total: i32,
    pub unit_hit_point_rate: i32,
    pub unit_hit_point_total: i32,
    pub razing_kill_rate: i32,
    pub razing_kill_total: i32,
    #[br(args(v))]
    pub tech_tree: TechTree,
}

impl DatFile {
    /// Read a compressed file as the game ships it.
    pub fn read_from(input: impl Read) -> Result<Self, Error> {
        let mut data = Vec::new();
        DeflateDecoder::new(input).read_to_end(&mut data)?;
        Self::from_inflated(&data)
    }

    /// Read inflated bytes.
    pub fn from_inflated(data: &[u8]) -> Result<Self, Error> {
        let mut version = [0u8; 8];
        version.copy_from_slice(data.get(..8).ok_or(Error::UnsupportedVersion([0; 8]))?);
        let v = Version::of(&version).ok_or(Error::UnsupportedVersion(version))?;
        Ok(DatFile::read_le_args(&mut Cursor::new(data), (v,))?)
    }

    /// The inflated bytes the file writes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        let mut out = Cursor::new(Vec::new());
        self.write_le(&mut out)?;
        Ok(out.into_inner())
    }

    /// The file's version.
    pub fn version(&self) -> Version {
        Version::of(&self.file_version).expect("checked when read")
    }
}

#[binrw]
#[brw(little)]
#[br(import(terrains: i16))]
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainRestriction {
    #[br(count = terrains)]
    pub passable_buildable_dmg_multiplier: Vec<f32>,
    #[br(count = terrains)]
    pub terrain_pass_graphics: Vec<TerrainPassGraphic>,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainPassGraphic {
    pub exit_tile_sprite_id: i32,
    pub enter_tile_sprite_id: i32,
    pub walk_tile_sprite_id: i32,
    pub walk_sprite_rate: i32,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerColour {
    pub id: i32,
    pub player_color_base: i32,
    pub unit_outline_color: i32,
    pub unit_selection_color1: i32,
    pub unit_selection_color2: i32,
    pub minimap_colour: i32,
    pub minimap_color2: i32,
    pub minimap_color3: i32,
    pub statistics_text: i32,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq)]
pub struct Sound {
    pub id: i16,
    pub play_delay: i16,
    pub file_count: i16,
    pub cache_time: i32,
    pub total_probability: i16,
    #[br(count = file_count)]
    pub items: Vec<SoundItem>,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq)]
pub struct SoundItem {
    pub file_name: DebugString,
    pub resource_id: i32,
    pub probability: i16,
    pub civilization: i16,
    pub icon_set: i16,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq)]
pub struct Graphic {
    pub name: DebugString,
    pub file_name: DebugString,
    pub particle_effect_name: DebugString,
    pub slp: i32,
    pub is_loaded: u8,
    pub old_color_flag: u8,
    pub layer: u8,
    pub player_color: i16,
    pub transparent_selection: u8,
    pub coordinates: [i16; 4],
    pub delta_count: i16,
    pub sound_id: i16,
    pub wwise_sound_id: u32,
    pub angle_sounds_used: u8,
    pub frame_count: i16,
    pub angle_count: i16,
    pub speed_multiplier: f32,
    pub frame_duration: f32,
    pub replay_delay: f32,
    pub sequence_type: u8,
    pub id: i16,
    pub mirroring_mode: u8,
    pub editor_flag: u8,
    #[br(count = delta_count)]
    pub deltas: Vec<GraphicDelta>,
    #[br(count = if angle_sounds_used != 0 { angle_count.max(0) as usize } else { 0 })]
    pub angle_sounds: Vec<GraphicAngleSound>,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq)]
pub struct GraphicDelta {
    pub graphic_id: i16,
    pub padding1: i16,
    pub sprite_ptr: i32,
    pub offset_x: i16,
    pub offset_y: i16,
    pub display_angle: i16,
    pub padding2: i16,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq)]
pub struct GraphicAngleSound {
    pub frame_num: i16,
    pub sound_id: i16,
    pub wwise_sound_id: u32,
    pub frame_num2: i16,
    pub sound_id2: i16,
    pub wwise_sound_id2: u32,
    pub frame_num3: i16,
    pub sound_id3: i16,
    pub wwise_sound_id3: u32,
}

/// genieutils' `SharedTerrain::TILE_TYPE_COUNT`.
const TILE_TYPES: usize = 19;
/// genieutils' `Terrain::getTerrainCount` for GV_C8 and later.
const TERRAINS: usize = 200;
/// genieutils' `Terrain::TERRAIN_UNITS_SIZE`.
const TERRAIN_UNITS: usize = 30;

#[binrw]
#[brw(little)]
#[br(import(v: Version))]
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainBlock {
    pub virtual_function_ptr: u32,
    pub map_pointer: u32,
    pub map_width: i32,
    pub map_height: i32,
    pub world_width: i32,
    pub world_height: i32,
    pub tile_sizes: [TileSize; TILE_TYPES],
    pub padding_ts: i16,
    #[br(count = TERRAINS)]
    pub terrains: Vec<Terrain>,
    pub map_min_x: f32,
    pub map_min_y: f32,
    pub map_max_x: f32,
    pub map_max_y: f32,
    pub map_max_x_plus1: f32,
    pub map_max_y_plus1: f32,
    pub terrains_used2: i16,
    pub borders_used: i16,
    pub max_terrain: i16,
    pub tile_width: i16,
    pub tile_height: i16,
    pub tile_half_height: i16,
    pub tile_half_width: i16,
    pub elev_height: i16,
    pub cur_row: i16,
    pub cur_col: i16,
    pub block_beg_row: i16,
    pub block_end_row: i16,
    pub block_beg_col: i16,
    pub block_end_col: i16,
    pub search_map_ptr: u32,
    pub search_map_rows_ptr: u32,
    pub any_frame_change: u8,
    pub map_visible_flag: u8,
    pub fog_flag: u8,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq, Default)]
pub struct TileSize {
    pub width: i16,
    pub height: i16,
    pub delta_y: i16,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FrameData {
    pub frame_count: i16,
    pub angle_count: i16,
    pub shape_id: i16,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq)]
pub struct Terrain {
    pub enabled: u8,
    pub random: u8,
    pub is_water: u8,
    pub hide_in_editor: u8,
    pub string_id: i32,
    pub name: DebugString,
    pub name2: DebugString,
    pub slp: i32,
    pub shape_ptr: i32,
    pub sound_id: i32,
    pub wwise_sound_id: u32,
    pub wwise_sound_stop_id: u32,
    pub blend_priority: i32,
    pub blend_type: i32,
    pub overlay_mask_name: DebugString,
    pub colors: [u8; 3],
    pub cliff_colors: [u8; 2],
    pub passable_terrain: u8,
    pub impassable_terrain: u8,
    pub is_animated: u8,
    pub animation_frames: i16,
    pub pause_frames: i16,
    pub interval: f32,
    pub pause_between_loops: f32,
    pub frame: i16,
    pub draw_frame: i16,
    pub animate_last: f32,
    pub frame_changed: u8,
    pub drawn: u8,
    pub elevation_graphics: [FrameData; TILE_TYPES],
    pub terrain_to_draw: i16,
    pub terrain_dimensions: [i16; 2],
    pub terrain_unit_masked_density: [i16; TERRAIN_UNITS],
    pub terrain_unit_id: [i16; TERRAIN_UNITS],
    pub terrain_unit_density: [i16; TERRAIN_UNITS],
    pub terrain_unit_centering: [u8; TERRAIN_UNITS],
    pub number_of_terrain_units_used: i16,
    pub phantom: i16,
}

/// The random maps: their heads, then each map again with its lists (genieutils' `io_all_`).
#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq)]
pub struct RandomMaps {
    pub map_count: u32,
    pub random_maps_ptr: i32,
    #[br(count = map_count)]
    pub heads: Vec<MapInfoHead>,
    #[br(count = map_count)]
    pub maps: Vec<MapInfo>,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq)]
pub struct MapInfoHead {
    pub map_id: i32,
    pub borders: [i32; 4],
    pub border_usage: i32,
    pub water_shape: i32,
    pub base_terrain: i32,
    pub land_coverage: i32,
    pub unused_id: i32,
    pub land_count: u32,
    pub lands_ptr: i32,
    pub terrain_count: u32,
    pub terrains_ptr: i32,
    pub unit_count: u32,
    pub units_ptr: i32,
    pub elevation_count: u32,
    pub elevations_ptr: i32,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq)]
pub struct MapInfo {
    pub borders: [i32; 4],
    pub border_usage: i32,
    pub water_shape: i32,
    pub base_terrain: i32,
    pub land_coverage: i32,
    pub unused_id: i32,
    pub land_count: u32,
    pub lands_ptr: i32,
    #[br(count = land_count)]
    pub lands: Vec<MapLand>,
    pub terrain_count: u32,
    pub terrains_ptr: i32,
    #[br(count = terrain_count)]
    pub terrains: Vec<MapTerrain>,
    pub unit_count: u32,
    pub units_ptr: i32,
    #[br(count = unit_count)]
    pub units: Vec<MapUnit>,
    pub elevation_count: u32,
    pub elevations_ptr: i32,
    #[br(count = elevation_count)]
    pub elevations: Vec<MapElevation>,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq)]
pub struct MapLand {
    pub land_id: i32,
    pub terrain: u32,
    pub land_spacing: i32,
    pub base_size: i32,
    pub zone: u8,
    pub placement_type: u8,
    pub padding1: i16,
    pub base_x: i32,
    pub base_y: i32,
    pub land_proportion: u8,
    pub by_player_flag: u8,
    pub padding2: i16,
    pub start_area_radius: i32,
    pub terrain_edge_fade: i32,
    pub clumpiness: i32,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq)]
pub struct MapTerrain {
    pub proportion: i32,
    pub terrain: i32,
    pub clump_count: i32,
    pub edge_spacing: i32,
    pub placement_terrain: i32,
    pub clumpiness: i32,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq)]
pub struct MapUnit {
    pub unit: i32,
    pub host_terrain: i32,
    pub group_placing: u8,
    pub scale_flag: u8,
    pub padding1: i16,
    pub objects_per_group: i32,
    pub fluctuation: i32,
    pub groups_per_player: i32,
    pub group_area: i32,
    pub player_id: i32,
    pub set_place_for_all_players: i32,
    pub min_distance_to_players: i32,
    pub max_distance_to_players: i32,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq)]
pub struct MapElevation {
    pub proportion: i32,
    pub terrain: i32,
    pub clump_count: i32,
    pub base_terrain: i32,
    pub base_elevation: i32,
    pub tile_spacing: i32,
}

/// A tech effect (genieutils' `Effect`, the "techage").
#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq)]
pub struct Effect {
    pub name: DebugString,
    pub command_count: i16,
    #[br(count = command_count)]
    pub commands: Vec<EffectCommand>,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq)]
pub struct EffectCommand {
    pub kind: u8,
    pub a: i16,
    pub b: i16,
    pub c: i16,
    pub d: f32,
}

/// The shared task list of a unit id (DE keeps the tasks in each unit's own list as well).
#[binrw]
#[brw(little)]
#[br(import(v: Version))]
#[derive(Clone, Debug, PartialEq)]
pub struct UnitHeader {
    pub exists: u8,
    #[br(if(exists != 0))]
    pub task_count: Option<i16>,
    #[br(count = task_count.unwrap_or(0), args { inner: (v,) })]
    pub tasks: Vec<Task>,
}

#[binrw]
#[brw(little)]
#[br(import(v: Version))]
#[derive(Clone, Debug, PartialEq)]
pub struct Task {
    pub task_type: i16,
    pub id: i16,
    pub is_default: u8,
    pub action_type: i16,
    pub class_id: i16,
    pub unit_id: i16,
    pub terrain_id: i16,
    pub resource_in: i16,
    pub resource_multiplier: i16,
    pub resource_out: i16,
    pub unused_resource: i16,
    pub work_value1: f32,
    pub work_value2: f32,
    pub work_range: f32,
    pub auto_search_targets: u8,
    pub search_wait_time: f32,
    pub enable_targeting: u8,
    pub combat_level_flag: u8,
    pub gather_type: i16,
    pub work_flag2: i16,
    pub target_diplomacy: u8,
    pub carry_check: u8,
    pub pick_for_construction: u8,
    pub moving_graphic_id: i16,
    pub proceeding_graphic_id: i16,
    pub working_graphic_id: i16,
    pub carrying_graphic_id: i16,
    pub resource_gathering_sound_id: i16,
    pub resource_deposit_sound_id: i16,
    pub wwise_resource_gathering_sound_id: u32,
    pub wwise_resource_deposit_sound_id: u32,
    #[br(if(v.at_least(28)))]
    pub enabled: Option<i16>,
}

#[binrw]
#[brw(little)]
#[br(import(v: Version))]
#[derive(Clone, Debug, PartialEq)]
pub struct Civ {
    pub player_type: u8,
    pub name: DebugString,
    pub resource_count: i16,
    pub tech_tree_id: i16,
    pub team_bonus_id: i16,
    #[br(count = resource_count)]
    pub resources: Vec<f32>,
    pub icon_set: u8,
    pub unit_count: i16,
    #[br(count = unit_count)]
    pub unit_pointers: Vec<i32>,
    #[br(parse_with = |r, e, _: ()| read_present(r, e, &unit_pointers, (v,)))]
    #[bw(write_with = |l, w, e, _: ()| write_present(l, w, e))]
    pub units: Vec<Option<Unit>>,
}

/// genieutils' unit types (`UT_*`).
pub mod unit_type {
    pub const FLAG: u8 = 20;
    pub const DEAD_FISH: u8 = 30;
    pub const BIRD: u8 = 40;
    pub const COMBATANT: u8 = 50;
    pub const PROJECTILE: u8 = 60;
    pub const CREATABLE: u8 = 70;
    pub const BUILDING: u8 = 80;
    pub const AOE_TREES: u8 = 90;
}

#[binrw]
#[brw(little)]
#[br(import(v: Version))]
#[derive(Clone, Debug, PartialEq)]
pub struct Unit {
    pub unit_type: u8,
    pub id: i16,
    #[br(if(v.at_least(15)))]
    pub language_dll_name: Option<i32>,
    #[br(if(v.at_least(15)))]
    pub language_dll_creation: Option<i32>,
    #[br(if(!v.at_least(15)))]
    pub language_dll_name_u16: Option<i16>,
    #[br(if(!v.at_least(15)))]
    pub language_dll_creation_u16: Option<i16>,
    pub class: i16,
    pub standing_graphic: [i16; 2],
    pub dying_graphic: i16,
    pub undead_graphic: i16,
    pub undead_mode: u8,
    pub hit_points: i16,
    pub line_of_sight: f32,
    pub garrison_capacity: u8,
    pub collision_size: [f32; 3],
    pub train_sound: i16,
    pub damage_sound: i16,
    pub dead_unit_id: i16,
    pub blood_unit_id: i16,
    pub sort_number: u8,
    pub can_be_built_on: u8,
    pub icon_id: i16,
    pub hide_in_editor: u8,
    pub old_portrait_pict: i16,
    pub enabled: u8,
    pub disabled: u8,
    pub placement_side_terrain: [i16; 2],
    pub placement_terrain: [i16; 2],
    pub clearance_size: [f32; 2],
    pub hill_mode: u8,
    pub fog_visibility: u8,
    pub terrain_restriction: i16,
    pub fly_mode: u8,
    pub resource_capacity: i16,
    pub resource_decay: f32,
    pub blast_defense_level: u8,
    pub combat_level: u8,
    pub interaction_mode: u8,
    pub minimap_mode: u8,
    pub interface_kind: u8,
    pub multiple_attribute_mode: f32,
    pub minimap_color: u8,
    pub language_dll_help: i32,
    pub language_dll_hot_key_text: i32,
    /// Before VER 8.7 (GV_C30) the hotkey of the (single) train location sits here.
    #[br(if(!v.at_least(30)))]
    pub hot_key_id: Option<i32>,
    pub recyclable: u8,
    pub enable_auto_gather: u8,
    pub create_doppelganger_on_death: u8,
    pub resource_gather_group: u8,
    pub occlusion_mode: u8,
    pub obstruction_type: u8,
    pub obstruction_class: u8,
    pub trait_: u8,
    pub civilization: u8,
    pub nothing: i16,
    pub selection_effect: u8,
    pub editor_selection_colour: u8,
    pub outline_size: [f32; 3],
    /// Two words genieutils writes as -2 and 0 and drops on read (GV_CK and later).
    pub unknown: [i32; 2],
    pub resource_storages: [ResourceStorage; 3],
    pub damage_graphic_count: u8,
    #[br(count = damage_graphic_count)]
    pub damage_graphics: Vec<DamageGraphic>,
    pub selection_sound: i16,
    pub dying_sound: i16,
    pub wwise_train_sound_id: u32,
    pub wwise_damage_sound_id: u32,
    pub wwise_selection_sound_id: u32,
    pub wwise_dying_sound_id: u32,
    pub old_attack_reaction: u8,
    pub convert_terrain: u8,
    pub name: DebugString,
    pub copy_id: i16,
    pub base_id: i16,
    #[br(if(unit_type >= unit_type::FLAG && unit_type != unit_type::AOE_TREES))]
    pub speed: Option<f32>,
    #[br(if(unit_type >= unit_type::DEAD_FISH && unit_type != unit_type::AOE_TREES))]
    pub dead_fish: Option<DeadFish>,
    #[br(if(unit_type >= unit_type::BIRD && unit_type != unit_type::AOE_TREES), args(v))]
    pub bird: Option<Bird>,
    #[br(if(unit_type >= unit_type::COMBATANT && unit_type != unit_type::AOE_TREES), args(v))]
    pub type50: Option<Type50>,
    #[br(if(unit_type == unit_type::PROJECTILE))]
    pub projectile: Option<Projectile>,
    #[br(if(unit_type >= unit_type::CREATABLE && unit_type != unit_type::AOE_TREES), args(v))]
    pub creatable: Option<Creatable>,
    #[br(if(unit_type == unit_type::BUILDING))]
    pub building: Option<Building>,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ResourceStorage {
    pub kind: i16,
    pub amount: f32,
    pub flag: u8,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq)]
pub struct DamageGraphic {
    pub graphic_id: i16,
    pub damage_percent: i16,
    pub apply_mode: u8,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq)]
pub struct DeadFish {
    pub walking_graphic: i16,
    pub running_graphic: i16,
    pub rotation_speed: f32,
    pub old_size_class: u8,
    pub tracking_unit: i16,
    pub tracking_unit_mode: u8,
    pub tracking_unit_density: f32,
    pub old_move_algorithm: u8,
    pub turn_radius: f32,
    pub turn_radius_speed: f32,
    pub max_yaw_per_second_moving: f32,
    pub stationary_yaw_revolution_time: f32,
    pub max_yaw_per_second_stationary: f32,
    pub min_collision_size_multiplier: f32,
}

/// The action part of a unit (genieutils' `Bird`).
#[binrw]
#[brw(little)]
#[br(import(v: Version))]
#[derive(Clone, Debug, PartialEq)]
pub struct Bird {
    pub default_task_id: i16,
    pub search_radius: f32,
    pub work_rate: f32,
    /// From VER 7.8 (GV_C21) the drop sites are counted; before, there are three.
    #[br(if(v.at_least(21)))]
    pub drop_site_count: Option<i16>,
    #[br(count = drop_site_count.map_or(3, |n| n.max(0) as usize))]
    pub drop_sites: Vec<i16>,
    pub task_swap_group: u8,
    pub attack_sound: i16,
    pub move_sound: i16,
    pub wwise_attack_sound_id: u32,
    pub wwise_move_sound_id: u32,
    pub run_pattern: u8,
    #[br(if(v.at_least(15)))]
    pub task_count: Option<i16>,
    #[br(count = task_count.unwrap_or(0), args { inner: (v,) })]
    pub tasks: Vec<Task>,
}

impl Bird {
    /// The unit's default task, from its own task list.
    pub fn default_task(&self) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == self.default_task_id)
    }
}

/// The combat part of a unit (genieutils' `Type50`).
#[binrw]
#[brw(little)]
#[br(import(v: Version))]
#[derive(Clone, Debug, PartialEq)]
pub struct Type50 {
    pub base_armor: i16,
    pub attack_count: i16,
    #[br(count = attack_count)]
    pub attacks: Vec<AttackOrArmor>,
    pub armour_count: i16,
    #[br(count = armour_count)]
    pub armours: Vec<AttackOrArmor>,
    pub defense_terrain_bonus: i16,
    #[br(if(v.at_least(16)))]
    pub bonus_damage_resistance: Option<f32>,
    pub max_range: f32,
    pub blast_width: f32,
    pub reload_time: f32,
    pub projectile_unit_id: i16,
    pub accuracy_percent: i16,
    pub break_off_combat: u8,
    pub frame_delay: i16,
    pub graphic_displacement: [f32; 3],
    pub blast_attack_level: u8,
    pub min_range: f32,
    pub accuracy_dispersion: f32,
    pub attack_graphic: i16,
    pub displayed_melee_armour: i16,
    pub displayed_attack: i16,
    pub displayed_range: f32,
    pub displayed_reload_time: f32,
    #[br(if(v.at_least(20)))]
    pub blast_damage: Option<f32>,
    #[br(if(v.at_least(22)))]
    pub damage_reflection: Option<f32>,
    #[br(if(v.at_least(22)))]
    pub friendly_fire_damage: Option<f32>,
    #[br(if(v.at_least(27)))]
    pub interrupt_frame: Option<i16>,
    #[br(if(v.at_least(27)))]
    pub garrison_firepower: Option<f32>,
    #[br(if(v.at_least(27)))]
    pub attack_graphic2: Option<i16>,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq)]
pub struct AttackOrArmor {
    pub class: i16,
    pub amount: i16,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq)]
pub struct Projectile {
    pub projectile_type: u8,
    pub smart_mode: u8,
    pub hit_mode: u8,
    pub vanish_mode: u8,
    pub area_effect_specials: u8,
    pub projectile_arc: f32,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ResourceCost {
    pub kind: i16,
    pub amount: i16,
    pub flag: i16,
}

#[binrw]
#[brw(little)]
#[br(import(v: Version))]
#[derive(Clone, Debug, PartialEq)]
pub struct TrainLocation {
    pub queue_time: i16,
    pub location_id: i16,
    pub button_id: u8,
    #[br(if(v.at_least(30)))]
    pub hot_key_id: Option<i32>,
}

#[binrw]
#[brw(little)]
#[br(import(v: Version))]
#[derive(Clone, Debug, PartialEq)]
pub struct Creatable {
    pub resource_costs: [ResourceCost; 3],
    /// From VER 8.6 (GV_C29) the train locations are counted; before, there is one.
    #[br(if(v.at_least(29)))]
    pub train_location_count: Option<i16>,
    #[br(count = train_location_count.map_or(1, |n| n.max(0) as usize), args { inner: (v,) })]
    pub train_locations: Vec<TrainLocation>,
    pub rear_attack_modifier: f32,
    pub flank_attack_modifier: f32,
    pub creatable_type: u8,
    pub hero_mode: u8,
    pub garrison_graphic: i32,
    pub spawning_graphic: i16,
    pub upgrade_graphic: i16,
    #[br(if(v.at_least(17)))]
    pub hero_glow_graphic: Option<i16>,
    #[br(if(v.at_least(26)))]
    pub idle_attack_graphic: Option<i16>,
    #[br(if(v.at_least(16)))]
    pub max_charge: Option<f32>,
    #[br(if(v.at_least(16)))]
    pub recharge_rate: Option<f32>,
    #[br(if(v.at_least(16)))]
    pub charge_event: Option<i16>,
    #[br(if(v.at_least(16)))]
    pub charge_type: Option<i16>,
    #[br(if(v.at_least(23)))]
    pub charge_target: Option<i16>,
    #[br(if(v.at_least(25)))]
    pub charge_projectile_unit: Option<i32>,
    #[br(if(v.at_least(25)))]
    pub attack_priority: Option<u8>,
    #[br(if(v.at_least(25)))]
    pub invulnerability_level: Option<f32>,
    #[br(if(v.at_least(24)))]
    pub button_icon_id: Option<i16>,
    #[br(if(v.at_least(24)))]
    pub button_short_tooltip_id: Option<i32>,
    #[br(if(v.at_least(24)))]
    pub button_extended_tooltip_id: Option<i32>,
    #[br(if(v.at_least(24)))]
    pub button_hotkey_action: Option<i16>,
    #[br(if(v.at_least(19)))]
    pub min_conversion_time_mod: Option<f32>,
    #[br(if(v.at_least(19)))]
    pub max_conversion_time_mod: Option<f32>,
    #[br(if(v.at_least(19)))]
    pub conversion_chance_mod: Option<f32>,
    pub total_projectiles: f32,
    pub max_total_projectiles: u8,
    pub projectile_spawning_area: [f32; 3],
    pub secondary_projectile_unit: i32,
    pub special_graphic: i32,
    pub special_ability: u8,
    pub displayed_pierce_armour: i16,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq, Default)]
pub struct BuildingAnnex {
    pub unit_id: i16,
    pub misplacement: [f32; 2],
}

#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq)]
pub struct Building {
    pub construction_graphic_id: i16,
    pub snow_graphic_id: i16,
    pub destruction_graphic_id: i16,
    pub destruction_rubble_graphic_id: i16,
    pub researching_graphic: i16,
    pub research_completed_graphic: i16,
    pub adjacent_mode: u8,
    pub graphics_angle: i16,
    pub disappears_when_built: u8,
    pub stack_unit_id: i16,
    pub foundation_terrain_id: i16,
    pub old_overlay_id: i16,
    pub tech_id: i16,
    pub can_burn: u8,
    pub annexes: [BuildingAnnex; 4],
    pub head_unit: i16,
    pub transform_unit: i16,
    pub transform_sound: i16,
    pub construction_sound: i16,
    pub wwise_transform_sound_id: u32,
    pub wwise_construction_sound_id: u32,
    pub garrison_type: u8,
    pub garrison_heal_rate: f32,
    pub garrison_repair_rate: f32,
    pub pile_unit: i16,
    pub looting_table: [u8; 6],
}

#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ResearchResourceCost {
    pub kind: i16,
    pub amount: i16,
    pub flag: u8,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq)]
pub struct ResearchLocation {
    pub location_id: i16,
    pub queue_time: i16,
    pub button_id: u8,
    pub hot_key_id: i32,
}

/// A tech (genieutils' `Tech`, `Research.cpp`).
#[binrw]
#[brw(little)]
#[br(import(v: Version))]
#[derive(Clone, Debug, PartialEq)]
pub struct Tech {
    pub required_techs: [i16; 6],
    pub resource_costs: [ResearchResourceCost; 3],
    pub required_tech_count: i16,
    pub civ: i16,
    pub full_tech_mode: i16,
    /// Before VER 8.8 (GV_C31) one research location's fields are spread over the tech.
    #[br(if(!v.at_least(31)))]
    pub location_id: Option<i16>,
    #[br(if(v.at_least(18)))]
    pub language_dll_name: Option<i32>,
    #[br(if(v.at_least(18)))]
    pub language_dll_description: Option<i32>,
    #[br(if(!v.at_least(18)))]
    pub language_dll_name_u16: Option<i16>,
    #[br(if(!v.at_least(18)))]
    pub language_dll_description_u16: Option<i16>,
    #[br(if(!v.at_least(31)))]
    pub queue_time: Option<i16>,
    pub effect_id: i16,
    pub kind: i16,
    pub icon_id: i16,
    #[br(if(!v.at_least(31)))]
    pub button_id: Option<u8>,
    pub language_dll_help: i32,
    pub language_dll_tech_tree: i32,
    #[br(if(!v.at_least(31)))]
    pub hot_key_id: Option<i32>,
    pub name: DebugString,
    #[br(if(v.at_least(15)))]
    pub repeatable: Option<u8>,
    #[br(if(v.at_least(31)))]
    pub research_location_count: Option<i16>,
    #[br(count = research_location_count.unwrap_or(0))]
    pub research_locations: Vec<ResearchLocation>,
}

/// genieutils' `TechTreeAge::getZoneCount` and `Common::getSlots` for AoK and later.
const ZONES: usize = 10;
/// genieutils' `BuildingConnection::AGES`.
const AGES: usize = 5;

#[binrw]
#[brw(little)]
#[br(import(v: Version))]
#[derive(Clone, Debug, PartialEq)]
pub struct TechTree {
    pub age_count: u8,
    pub building_count: u8,
    /// From VER 8.9 (GV_C32) the unit connections are counted in an i16.
    #[br(if(v.at_least(32)))]
    pub unit_count_wide: Option<i16>,
    #[br(if(!v.at_least(32)))]
    pub unit_count: Option<u8>,
    pub research_count: u8,
    pub total_unit_tech_groups: i32,
    #[br(count = age_count)]
    pub ages: Vec<TechTreeAge>,
    #[br(count = building_count)]
    pub buildings: Vec<BuildingConnection>,
    #[br(count = unit_count_wide.map_or(unit_count.unwrap_or(0) as usize, |n| n.max(0) as usize))]
    pub units: Vec<UnitConnection>,
    #[br(count = research_count)]
    pub researches: Vec<ResearchConnection>,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq)]
pub struct TechTreeCommon {
    pub slots_used: i32,
    pub unit_research: [i32; ZONES],
    pub mode: [i32; ZONES],
}

/// Three counted id lists: buildings, units, techs.
#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq)]
pub struct Connections {
    pub building_count: u8,
    #[br(count = building_count)]
    pub buildings: Vec<i32>,
    pub unit_count: u8,
    #[br(count = unit_count)]
    pub units: Vec<i32>,
    pub tech_count: u8,
    #[br(count = tech_count)]
    pub techs: Vec<i32>,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq)]
pub struct TechTreeAge {
    pub id: i32,
    pub status: u8,
    pub connections: Connections,
    pub common: TechTreeCommon,
    pub num_building_levels: u8,
    pub buildings_per_zone: [u8; ZONES],
    pub group_length_per_zone: [u8; ZONES],
    pub max_age_length: u8,
    pub line_mode: i32,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq)]
pub struct BuildingConnection {
    pub id: i32,
    pub status: u8,
    pub connections: Connections,
    pub common: TechTreeCommon,
    pub location_in_age: u8,
    pub units_techs_total: [u8; AGES],
    pub units_techs_first: [u8; AGES],
    pub line_mode: i32,
    pub enabling_research: i32,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq)]
pub struct UnitConnection {
    pub id: i32,
    pub status: u8,
    pub upper_building: i32,
    pub common: TechTreeCommon,
    pub vertical_line: i32,
    pub unit_count: u8,
    #[br(count = unit_count)]
    pub units: Vec<i32>,
    pub location_in_age: i32,
    pub required_research: i32,
    pub line_mode: i32,
    pub enabling_research: i32,
}

#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq)]
pub struct ResearchConnection {
    pub id: i32,
    pub status: u8,
    pub upper_building: i32,
    pub connections: Connections,
    pub common: TechTreeCommon,
    pub vertical_line: i32,
    pub location_in_age: i32,
    pub line_mode: i32,
}
