//! The map and the players' sight from a save: the terrain, the objects as movement sees them,
//! the visible maps and resources.

use crate::GenieError;
use crate::map::Distance;
use crate::map::Point;
use crate::map::Position;
use crate::map::spatial::Spatial;
use crate::map::terrain::Terrain;
use crate::map::terrain::TileTerrain;
use crate::map::{GridSize, TileOffset};
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::ObstructionKind;
use crate::objects::masters::{MasterId, MasterKind};
use crate::ownership::CaptureKind;
use crate::players::PlayerId;
use crate::players::{PlayerKind, UnitRelation};
use crate::vision::TileCount;
use crate::vision::VisionBits;
use crate::vision::visibility::{
    BuildingSight, Extent, Seer, Visibility, VisibleMap, VisibleResource, VisibleResources,
    capture_kind,
};
use genie_import::dat::TerrainTable;
use genie_import::save::moving_speed;
use genie_import::save::placement;
use genie_model::layer::BuildLayer;
use genie_save::Header;
use genie_save::generated::world::VisibleResourceManager;

/// The map's terrain per tile from the save, with the `.dat`'s terrain records.
#[must_use]
pub fn map_terrain(header: &Header, terrain: &TerrainTable) -> Terrain {
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

/// The world's objects as movement sees them: every player's live and sleeping objects with their
/// positions and their masters' radii and class; the players' doppelgangers count on their tiles.
/// An object has an obstruction record when its master has an obstruction kind and all three radii,
/// unless it is a building marked desolid (a foundation no builder has started; the first builder's
/// Build state 7 adds its record). Checked against the game's obstruction manager, dumped by the
/// tick oracle.
///
/// # Errors
///
/// An object's id or master of −1, or what the sight does not model of the save.
pub fn spatial(
    header: &Header,
    terrain: &TerrainTable,
    layer: &BuildLayer,
) -> Result<Spatial, GenieError> {
    use crate::map::spatial::{Obstruction, Placed, Spatial, Tiles};
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
pub fn saved_sight(header: &Header) -> Result<Visibility, GenieError> {
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
pub fn saved_resources(m: &VisibleResourceManager) -> Result<VisibleResources, GenieError> {
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
                    .collect::<Result<Vec<_>, GenieError>>()
            })
            .collect::<Result<_, _>>()?,
    })
}

#[cfg(test)]
mod tests;
