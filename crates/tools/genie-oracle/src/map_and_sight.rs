//! The map and the players' sight: the terrain and its differences, the objects as movement
//! sees them, the visible maps and resources against the game's.

use genie_core::map::Distance;
use genie_core::map::terrain::TileTerrain;
use genie_core::map::{GridSize, TileOffset};
use genie_core::vision::TileCount;

use crate::masters::{MasterTable, TerrainTable};
use crate::{Error, game_state, header_objects, moving_speed, placement, saved_ghosts};
use genie_core::map::Point;
use genie_core::map::Position;
use genie_core::map::spatial::Spatial;
use genie_core::map::terrain::Terrain;
use genie_core::objects::ObjectId;
use genie_core::objects::ObjectState;
use genie_core::objects::masters::ObstructionKind;
use genie_core::objects::masters::{MasterId, MasterKind};
use genie_core::ownership::CaptureKind;
use genie_core::players::PlayerId;
use genie_core::players::{PlayerKind, UnitRelation};
use genie_core::vision::VisionBits;
use genie_core::vision::ghost::Ghost;
use genie_core::vision::visibility::{
    BuildingSight, Extent, Seer, Visibility, VisibleMap, VisibleResource, VisibleResources,
    capture_kind,
};
use genie_core::world::GameState;
use genie_core::world::parts::Object;
use genie_model::layer::BuildLayer;
use genie_rec::de::chapters;
use genie_save::Header;
use genie_save::generated::world::VisibleResourceManager;

/// The world's objects as movement sees them: every player's live and sleeping objects with their
/// positions and their masters' radii and class; the players' doppelgangers count on their tiles. An object has an obstruction record when its
/// master has an obstruction kind and all three radii, unless it is a building marked desolid (a
/// foundation no builder has started; the first builder's Build state 7 adds its record). Checked
/// against the game's obstruction manager, dumped by the tick oracle.
/// The map's terrain per tile from the save, with the `.dat`'s terrain records.
pub(super) fn map_terrain(header: &Header, terrain: &TerrainTable) -> Terrain {
    let map = &header.world.base_world.map;
    let tiles = map
        .rows
        .iter()
        .flat_map(|r| {
            r.tiles
                .iter()
                .map(|t| TileTerrain::of_save(t.terrain_kind, t.restore_terrain_kind, t.layer))
        })
        .collect();
    let mut t = Terrain::new(
        GridSize::new(map.map_width, map.map_height),
        tiles,
        terrain.kinds().to_vec(),
        terrain.passable().to_vec(),
    );
    t.set_elevations(
        map.rows
            .iter()
            .flat_map(|r| r.tiles.iter().map(|t| t.elevation))
            .collect(),
    );
    t
}

/// The differences between the slice's map terrain and a save's, tile by tile.
pub fn terrain_differences(slice: &Terrain, save: &[u8]) -> Result<Vec<String>, Error> {
    let chapter = chapters(save)?[0];
    Ok(header_terrain_differences(slice, &chapter.header()?))
}

/// [`terrain_differences`] of a read header.
pub(super) fn header_terrain_differences(slice: &Terrain, header: &Header) -> Vec<String> {
    let (width, tiles) = saved_terrain(header);
    tile_differences(slice, width, &tiles)
}

/// A save's terrain per tile, row by row: the terrain, the terrain to restore, the layer; and the
/// map's width.
pub(super) fn saved_terrain(header: &Header) -> (usize, Vec<(u8, u8, i16)>) {
    let map = &header.world.base_world.map;
    let tiles = map
        .rows
        .iter()
        .flat_map(|r| {
            r.tiles
                .iter()
                .map(|t| (t.terrain_kind, t.restore_terrain_kind, t.layer))
        })
        .collect();
    (map.map_width as usize, tiles)
}

/// The tiles whose terrain differs between the slice and the game's.
pub(super) fn tile_differences(
    slice: &Terrain,
    width: usize,
    tiles: &[(u8, u8, i16)],
) -> Vec<String> {
    let mut out = Vec::new();
    for (i, &theirs) in tiles.iter().enumerate() {
        let ours = TileTerrain {
            kind: slice.kinds[i],
            restore: slice.restore[i],
            layer: slice.layers[i],
        };
        let theirs = TileTerrain::of_save(theirs.0, theirs.1, theirs.2);
        if ours != theirs {
            let (x, y) = (i % width, i / width);
            out.push(format!(
                "terrain tile ({x}, {y}): slice {ours:?} game {theirs:?}"
            ));
        }
    }
    out
}

/// What the forced replay reads of the game's save after an update: every player's object rows,
/// the sight, the map's terrain and the state the forcing takes. Parsing a save takes most of
/// the replay's time, so [`forced_run`] builds these on worker threads ahead of the replay.
///
/// [`forced_run`]: crate::forced_replay::forced_run
pub(super) struct SavedStep {
    pub(super) objects: Vec<Vec<Object>>,
    /// Each player's ghosts (type 25) in its list's order.
    pub(super) ghosts: Vec<Vec<Ghost>>,
    pub(super) seen: Visibility,
    pub(super) width: usize,
    pub(super) terrain: Vec<(u8, u8, i16)>,
    pub(super) game: GameState,
}

impl SavedStep {
    pub(super) fn of(save: &[u8], masters: Option<&MasterTable>) -> Result<Self, Error> {
        let chapter = chapters(save)?[0];
        let header = chapter.header()?;
        let objects = (0..header.players().len() as u32)
            .map(|id| header_objects(&header, id, masters))
            .collect::<Result<_, _>>()?;
        let (width, terrain) = saved_terrain(&header);
        Ok(Self {
            objects,
            ghosts: header
                .players()
                .iter()
                .map(saved_ghosts)
                .collect::<Result<_, _>>()
                .map_err(Error::Unmodelled)?,
            seen: saved_sight(&header).map_err(Error::Unmodelled)?,
            width,
            terrain,
            game: game_state(&header, masters)?,
        })
    }
}

pub(super) fn spatial(
    header: &Header,
    terrain: &TerrainTable,
    layer: &BuildLayer,
) -> Result<Spatial, genie_core::GenieError> {
    use genie_core::map::spatial::{Obstruction, Placed, Spatial, Tiles};
    let roles = &layer.roles;
    let map = &header.world.base_world.map;
    let classes = map
        .rows
        .iter()
        .flat_map(|r| r.tiles.iter().map(|t| terrain.class(t.terrain_kind)))
        .collect();
    let tiles = Tiles::new(GridSize::new(map.map_width, map.map_height), classes);
    let mut out = Spatial::with_tiles(GridSize::new(map.map_width, map.map_height), tiles);
    out.terrain = Some(map_terrain(header, terrain));
    // what each player's queries make of the others' objects (player +0x278, the load's
    // unit_diplomacy)
    out.unit_diplomacy = header
        .players()
        .iter()
        .map(|p| {
            p.world_player()
                .base
                .unit_diplomacy
                .iter()
                .map(|&k| UnitRelation::of_field(k))
                .collect()
        })
        .collect();
    let mut sight = saved_sight(header)?;
    for (player, section) in PlayerId::each(header.players().len()).zip(header.players()) {
        let all = section.live_objects().chain(section.sleeping_objects());
        for o in all {
            let Some(s) = o.static_part() else { continue };
            let Some(m) = placement(s, section.objects()) else {
                continue;
            };
            let master = MasterId::try_from(s.master_obj)?;
            let id = ObjectId::try_from(s.id)?;
            let building = o.tribe_building_object.as_deref();
            // a building's place links its tiles (FUN_1411e6780), an annex's does not, and a
            // foundation made desolid lost them (FUN_1411cf770); so do a combat unit's that does
            // not move and is more than a tile wide (FUN_14108c320). A building keeps its
            // footprint's UMV entry (+0x904)
            let place = Position::new(s.tx, s.ty, s.tz);
            let footprint = m.radius.ground();
            let speed = moving_speed(o, s, section.objects());
            let links = building.map_or_else(
                || {
                    o.combat().is_some()
                        && speed == 0.0
                        && (0.5 < m.radius.x() || 0.5 < m.radius.y())
                },
                |b| b.mupt_linked_object_references_linked_owner_id == -1 && b.desolid_flag == 0,
            );
            let umv = match (links, building.is_some()) {
                (true, true) => sight.link_building(place, footprint, layer.tuning.visibility)?,
                (true, false) => {
                    let _ = sight.link_building(place, footprint, layer.tuning.visibility)?;
                    0
                }
                (false, true) => sight.building_entry(place, footprint, layer.tuning.visibility),
                (false, false) => 0,
            };
            sight.add_seer(
                id,
                Seer {
                    player,
                    master,
                    los: m.los,
                    state: ObjectState::of_byte(s.object_state)?,
                    // a foundation made in the game explores first when its construction starts
                    explored: building.is_none_or(|b| b.built != 0 || b.build_pts != 0.0),
                    building: building.map(|b| BuildingSight::of_saved(b.built, b.build_pts)),
                    umv: umv as u8,
                    // the save's +0x924 (a map's objects keep what the map gave them, as
                    // 426048706's Gaia petards their 0); the setup's rule without a combat part
                    capture: o.combat().map_or_else(
                        || Ok(capture_kind(player, m.class, master, roles)),
                        |c| CaptureKind::try_from(c.rge_combat_object.capture_flag),
                    )?,
                    // the object's type: the save's factory types 70 and 80, any other below
                    kind: if o.tribe_combat_object.is_some() {
                        MasterKind::COMBAT
                    } else if building.is_some() {
                        MasterKind::BUILDING
                    } else {
                        MasterKind::of_data(0)
                    },
                },
            );
            let desolid = o
                .tribe_building_object
                .as_deref()
                .is_some_and(|b| b.desolid_flag != 0);
            let solid = m.radius.sized() && !desolid;
            let obstruction =
                (m.obstruction_kind != ObstructionKind::NONE && solid).then_some(Obstruction {
                    kind: m.obstruction_kind,
                    class: m.obstruction_class,
                });
            out.insert(
                ObjectId::try_from(s.id)?,
                Placed {
                    position: Position::new(s.tx, s.ty, s.tz),
                    radius: m.radius.ground(),
                    class: m.class,
                    player,
                    master,
                    obstruction_kind: m.obstruction_kind,
                    buildable_on: m.buildable_on,
                    tall: 0.0 < m.radius.z(),
                    moves: 0.0 < moving_speed(o, s, section.objects()),
                    unknown: None,
                    obstruction,
                },
            );
            // an object inside another left its tile's list and the obstruction manager
            // (FUN_1411921d0)
            if s.inside_obj != -1 {
                out.go_inside(id, Position::new(s.tx, s.ty, s.tz));
            }
        }
        for ghost in section.objects().doppleganger_objects.objects() {
            if let Some(s) = ghost.static_part() {
                out.insert_ghost(ObjectId::try_from(s.id)?, Position::new(s.tx, s.ty, s.tz));
            }
        }
    }
    out.sight = Some(sight);
    Ok(out)
}

/// What the players see in a save: each player's visible map, the map's unified map and UMV list
/// and which players are computers; no tile links and no objects' sight.
pub(super) fn saved_sight(header: &Header) -> Result<Visibility, genie_core::GenieError> {
    let map = &header.world.base_world.map;
    let n = (map.map_width.max(0) * map.map_height.max(0)) as usize;
    let players = header.players();
    let maps = players
        .iter()
        .map(|p| {
            let vm = &p.objects().rge_visible_map;
            VisibleMap {
                counts: vm.visible_map.clone().unwrap_or_else(|| vec![-1; n]),
                explored: TileCount::new(vm.number_tiles_explored_value),
            }
        })
        .collect();
    let umv = &map.umv_data_list;
    let extents = umv
        .umv_data_guid
        .iter()
        .zip(&umv.umv_data_list)
        .map(|(&guid, e)| Extent {
            guid,
            offsets: e
                .p
                .iter()
                .map(|&v| TileOffset::new(v as u32 as i32, (v >> 32) as u32 as i32))
                .collect(),
            capacity: e.n + 1,
        })
        .collect();
    let kinds = players
        .iter()
        .map(|p| PlayerKind::try_from(p.world_player().kind))
        .collect::<Result<_, _>>()?;
    let mut sight = Visibility::new(
        GridSize::new(map.map_width, map.map_height),
        maps,
        map.unified_vis_map
            .unified_visible_map
            .as_ref()
            .map_or_else(
                || vec![VisionBits::NONE; n],
                |words| {
                    words
                        .iter()
                        .copied()
                        .map(VisionBits::from_bits_retain)
                        .collect()
                },
            ),
        extents,
        umv.umv_data_list_size,
        kinds,
    );
    sight.resources = players
        .iter()
        .map(|p| saved_resources(&p.objects().visible_resource_manager))
        .collect::<Result<_, _>>()?;
    Ok(sight)
}

/// A player's saved `Visible_Resource_Manager` as [`VisibleResources`] holds it.
pub(super) fn saved_resources(
    m: &VisibleResourceManager,
) -> Result<VisibleResources, genie_core::GenieError> {
    Ok(VisibleResources {
        capacities: m
            .list_sizes
            .iter()
            .flatten()
            .map(|s| s.vr_list_size)
            .collect(),
        lists: m
            .lists
            .iter()
            .flatten()
            .map(|l| {
                l.visible_resource_rec
                    .iter()
                    .map(|r| {
                        Ok(VisibleResource {
                            id: ObjectId::try_from(r.object_id)?,
                            distance: Distance::new(r.distance),
                            zone: r.zone,
                            at: Point::new(r.pos_x, r.pos_y),
                        })
                    })
                    .collect::<Result<Vec<_>, genie_core::GenieError>>()
            })
            .collect::<Result<_, _>>()?,
    })
}

/// The players' visible maps and the unified map of a save, as [`Visibility`] holds them.
pub fn saved_visibility(save: &[u8]) -> Result<Visibility, Error> {
    let chapter = chapters(save)?[0];
    let header = chapter.header()?;
    saved_sight(&header).map_err(Error::Unmodelled)
}

/// Where player `player`'s sight in `slice` differs from `game`'s: its explored count, each tile's
/// count and its two bits of the unified map.
#[must_use]
pub fn visibility_differences(slice: &Visibility, game: &Visibility, player: usize) -> Vec<String> {
    let mut out = Vec::new();
    let (Some(a), Some(b)) = (slice.maps.get(player), game.maps.get(player)) else {
        return vec![format!("player {player} has no visible map")];
    };
    if a.explored != b.explored {
        out.push(format!(
            "player {player} explored {} tiles, the game {}",
            a.explored.get(),
            b.explored.get()
        ));
    }
    out.extend(resource_differences(slice, game, player));
    let w = slice.size.width().max(1) as usize;
    let bits = PlayerId::try_from(player).map_or(VisionBits::NONE, |p| {
        VisionBits::visible(p) | VisionBits::explored(p)
    });
    let unified_same = || {
        slice
            .unified
            .iter()
            .zip(&game.unified)
            .all(|(&u, &v)| u & bits == v & bits)
    };
    // the usual case, every tile the same, without a look at each tile's place
    if a.counts == b.counts && unified_same() {
        return out;
    }
    for (i, (&c, &g)) in a.counts.iter().zip(&b.counts).enumerate() {
        let (u, v) = (slice.unified[i] & bits, game.unified[i] & bits);
        if c == g && u == v {
            continue;
        }
        let (x, y) = (i % w, i / w);
        if c != g {
            out.push(format!(
                "player {player} tile ({x}, {y}) count {c}, the game {g}"
            ));
        }
        if u != v {
            out.push(format!(
                "player {player} tile ({x}, {y}) unified bits {u:#x}, the game {v:#x}"
            ));
        }
    }
    out
}

/// Where player `player`'s visible-resource lists in `slice` differ from `game`'s: their count,
/// each list's capacity and its first record that differs (every field, the floats by their
/// bits), or its length.
pub(super) fn resource_differences(
    slice: &Visibility,
    game: &Visibility,
    player: usize,
) -> Vec<String> {
    let (Some(a), Some(b)) = (slice.resources.get(player), game.resources.get(player)) else {
        return vec![format!("player {player} has no visible-resource lists")];
    };
    let mut out = Vec::new();
    if a.lists.len() != b.lists.len() {
        out.push(format!(
            "player {player} holds {} visible-resource lists, the game {}",
            a.lists.len(),
            b.lists.len()
        ));
    }
    if a.capacities != b.capacities {
        out.push(format!(
            "player {player} visible-resource capacities {:?}, the game {:?}",
            a.capacities, b.capacities
        ));
    }
    let bits = |r: &VisibleResource| {
        (
            r.id,
            r.distance.get().to_bits(),
            r.zone,
            r.at.x().to_bits(),
            r.at.y().to_bits(),
        )
    };
    for (g, (x, y)) in a.lists.iter().zip(&b.lists).enumerate() {
        if let Some(k) = x.iter().zip(y).position(|(r, s)| bits(r) != bits(s)) {
            out.push(format!(
                "player {player} visible resources {g}: record {k} {:?}, the game {:?}",
                x[k], y[k]
            ));
        } else if x.len() != y.len() {
            out.push(format!(
                "player {player} visible resources {g}: {} records, the game {}",
                x.len(),
                y.len()
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests;
