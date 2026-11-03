//! The random maps: their heads, lands, terrains, objects and elevations.

use binrw::binrw;

/// The random maps: their heads, then each map again with its lists (genieutils' `io_all_`).
#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RandomMaps {
    /// Number of random maps.
    pub map_count: u32,
    /// An in-memory pointer the game saved with the file.
    pub random_maps_ptr: i32,
    /// Each map's settings and list counts.
    #[br(count = map_count)]
    pub heads: Vec<MapInfoHead>,
    /// Each map again, now with its lists.
    #[br(count = map_count)]
    pub maps: Vec<MapInfo>,
}

/// A random map's settings and the counts of its lists, without the lists.
#[binrw]
#[brw(little)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapInfoHead {
    /// The map's id.
    pub map_id: i32,
    /// Border widths of the four map edges.
    pub borders: [i32; 4],
    /// How much of the borders is used; its scale is not known yet.
    pub border_usage: i32,
    /// Shape of the water; its values are not known yet.
    pub water_shape: i32,
    /// Terrain the map starts out as.
    pub base_terrain: i32,
    /// Share of the map covered by the lands, in percent.
    pub land_coverage: i32,
    /// An id the game does not use, as far as known.
    pub unused_id: i32,
    /// Number of lands.
    pub land_count: u32,
    /// An in-memory pointer the game saved with the file.
    pub lands_ptr: i32,
    /// Number of scattered terrains.
    pub terrain_count: u32,
    /// An in-memory pointer the game saved with the file.
    pub terrains_ptr: i32,
    /// Number of object groups.
    pub unit_count: u32,
    /// An in-memory pointer the game saved with the file.
    pub units_ptr: i32,
    /// Number of elevation entries.
    pub elevation_count: u32,
    /// An in-memory pointer the game saved with the file.
    pub elevations_ptr: i32,
}

/// A random map's settings with its lists of lands, terrains, objects and elevations.
#[binrw]
#[brw(little)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapInfo {
    /// Border widths of the four map edges.
    pub borders: [i32; 4],
    /// How much of the borders is used; its scale is not known yet.
    pub border_usage: i32,
    /// Shape of the water; its values are not known yet.
    pub water_shape: i32,
    /// Terrain the map starts out as.
    pub base_terrain: i32,
    /// Share of the map covered by the lands, in percent.
    pub land_coverage: i32,
    /// An id the game does not use, as far as known.
    pub unused_id: i32,
    /// Number of lands.
    pub land_count: u32,
    /// An in-memory pointer the game saved with the file.
    pub lands_ptr: i32,
    /// The lands.
    #[br(count = land_count)]
    pub lands: Vec<MapLand>,
    /// Number of scattered terrains.
    pub terrain_count: u32,
    /// An in-memory pointer the game saved with the file.
    pub terrains_ptr: i32,
    /// The scattered terrains.
    #[br(count = terrain_count)]
    pub terrains: Vec<MapTerrain>,
    /// Number of object groups.
    pub unit_count: u32,
    /// An in-memory pointer the game saved with the file.
    pub units_ptr: i32,
    /// The object groups.
    #[br(count = unit_count)]
    pub units: Vec<MapUnit>,
    /// Number of elevation entries.
    pub elevation_count: u32,
    /// An in-memory pointer the game saved with the file.
    pub elevations_ptr: i32,
    /// The elevation entries.
    #[br(count = elevation_count)]
    pub elevations: Vec<MapElevation>,
}

/// A land of a random map: a player's or a shared area grown from a base.
#[binrw]
#[brw(little)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapLand {
    /// The land's id.
    pub land_id: i32,
    /// Terrain of the land.
    pub terrain: u32,
    /// Distance kept to other lands.
    pub land_spacing: i32,
    /// Size of the land's base, the square it grows from.
    pub base_size: i32,
    /// Zone the land belongs to; lands of one zone may touch.
    pub zone: u8,
    /// How the land is placed; its values are not known yet.
    pub placement_kind: u8,
    /// Padding; kept to write the file back.
    pub padding1: i16,
    /// Fixed x position of the base.
    pub base_x: i32,
    /// Fixed y position of the base.
    pub base_y: i32,
    /// Share of the land area, in percent.
    pub land_proportion: u8,
    /// Whether every player gets one such land.
    pub by_player_flag: u8,
    /// Padding; kept to write the file back.
    pub padding2: i16,
    /// Radius of the starting area.
    pub start_area_radius: i32,
    /// How the terrain fades out at the land's edge, in percent.
    pub terrain_edge_fade: i32,
    /// How clumped the land grows.
    pub clumpiness: i32,
}

/// A terrain a random map scatters in clumps.
#[binrw]
#[brw(little)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapTerrain {
    /// Share of the map, in percent.
    pub proportion: i32,
    /// Terrain placed.
    pub terrain: i32,
    /// Number of clumps.
    pub clump_count: i32,
    /// Distance kept to other terrains.
    pub edge_spacing: i32,
    /// Terrain the clumps are placed on.
    pub placement_terrain: i32,
    /// How clumped the terrain grows.
    pub clumpiness: i32,
}

/// Objects a random map places in groups.
#[binrw]
#[brw(little)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapUnit {
    /// Unit placed.
    pub unit: i32,
    /// Terrain the objects are placed on.
    pub host_terrain: i32,
    /// How the objects of a group are placed; its values are not known yet.
    pub group_placing: u8,
    /// Whether the number of groups scales with the map size.
    pub scale_flag: u8,
    /// Padding; kept to write the file back.
    pub padding1: i16,
    /// Number of objects in a group.
    pub objects_per_group: i32,
    /// Random variation of `objects_per_group`.
    pub fluctuation: i32,
    /// Number of groups (per player, or in total).
    pub groups_per_player: i32,
    /// Area a group spreads over.
    pub group_area: i32,
    /// Owner of the objects: -1 gives every player their own, 0 is Gaia.
    pub player_id: i32,
    /// Whether every player gets a set of the objects.
    pub set_place_for_all_players: i32,
    /// Minimum distance to the players.
    pub min_distance_to_players: i32,
    /// Maximum distance to the players.
    pub max_distance_to_players: i32,
}

/// Hills a random map raises.
#[binrw]
#[brw(little)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapElevation {
    /// Share of the map, in percent.
    pub proportion: i32,
    /// Terrain raised.
    pub terrain: i32,
    /// Number of clumps.
    pub clump_count: i32,
    /// Terrain the elevation is placed on.
    pub base_terrain: i32,
    /// Elevation level.
    pub base_elevation: i32,
    /// Spacing between the elevated tiles.
    pub tile_spacing: i32,
}
