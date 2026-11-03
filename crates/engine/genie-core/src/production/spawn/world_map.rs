//! The map as the placement test reads it from the world ([`WorldMap`]).

use crate::GenieError;
use crate::map::GridSize;
use crate::map::TerrainFlags;
use crate::map::TerrainId;
use crate::map::Tile;
use crate::movement::Speed;
use crate::objects::masters::MasterKind;
use crate::players::AllyTest;
use crate::players::Amount;
use crate::players::NO_RELATIONS;
use crate::players::PlayerId;
use crate::production::spawn::{Near, PlaceMap};
use crate::unit_ai::task_finder::allied as task_finder_allied;
use crate::vision::{VisionBits, VisionMasks};
use crate::world::World;
use crate::world::slots::PlayerSlots;

/// The map as the spawn's placement test reads it: the terrain, the players' sight and the
/// objects with what their units and masters say.
pub(crate) struct WorldMap<'a> {
    pub(crate) turns: &'a World,
}

impl PlaceMap for WorldMap<'_> {
    fn terrain_flags(&self, t: usize) -> Option<TerrainFlags> {
        self.turns
            .spatial
            .terrain
            .as_ref()
            .and_then(|m| m.terrains.get(t))
            .map(|k| k.flags)
    }

    fn terrain_count(&self) -> usize {
        self.turns
            .spatial
            .terrain
            .as_ref()
            .map_or(0, |m| m.terrains.len())
    }

    fn size(&self) -> GridSize {
        self.turns.spatial.map
    }

    fn terrain(&self, tile: Tile) -> Result<TerrainId, GenieError> {
        let t = self
            .turns
            .spatial
            .terrain
            .as_ref()
            .ok_or(GenieError::unsupported(
                "a placement test without the map's terrain",
            ))?;
        t.kind(tile).ok_or(GenieError::unsupported(
            "a placement test off the map's tiles",
        ))
    }

    fn explored(&self, player: PlayerId, tile: Tile) -> bool {
        // without the sight the spawn stops before (World::spawn)
        self.turns.spatial.explored(player, tile).unwrap_or(false)
    }

    fn visible(&self, player: PlayerId, tile: Tile) -> bool {
        self.turns.spatial.visible(player, tile)
    }

    fn vision(&self, player: PlayerId) -> VisionMasks {
        self.turns.spatial.vision(player)
    }

    fn elevation(&self, tile: Tile) -> Option<u8> {
        let t = self.turns.spatial.terrain.as_ref()?;
        let i = self.turns.spatial.map.index(tile)?;
        t.elevations.get(i).copied()
    }

    fn shape(&self, tile: Tile) -> Option<u8> {
        let t = self.turns.spatial.terrain.as_ref()?;
        let i = self.turns.spatial.map.index(tile)?;
        t.shapes.get(i).copied()
    }

    fn allied(&self, a: PlayerId, b: PlayerId, test: AllyTest) -> bool {
        let relations = self
            .turns
            .players
            .player(a)
            .map_or(&NO_RELATIONS, |p| &p.relations);
        task_finder_allied(relations, a, b, test)
    }

    fn objects(&self, tile: Tile) -> Result<Vec<Near>, GenieError> {
        let t = self.turns;
        t.spatial
            .objects_on(tile)
            .map(|(id, placed)| {
                let (owner, unit) = t.object(id).ok_or(GenieError::unsupported(
                    "an object near a spawn point the slice does not update",
                ))?;
                let master = unit.master(t.master_tables(), owner)?;
                Ok(Near {
                    id,
                    position: placed.position,
                    radius: placed.radius,
                    tall: placed.tall,
                    kind: master.kind,
                    class: placed.class,
                    master: placed.master,
                    buildable_on: placed.buildable_on,
                    obstruction_kind: placed.obstruction_kind,
                    auto_gather: master.auto_gather.is_on(),
                    state: unit.life.state,
                    desolid: t
                        .parts
                        .building_of(id)
                        .construction
                        .is_some_and(|c| c.desolid),
                    player: owner,
                    moving: master
                        .move_stats
                        .as_ref()
                        .is_some_and(|m| Speed::ZERO < m.speed),
                    fog_visibility: master.fog_visibility,
                    fog_mask: t.spatial.sight.as_ref().and_then(|v| {
                        let umv = v.seer(id).map_or(0, |s| s.umv);
                        (MasterKind::RGE_COMBAT <= master.kind && umv != 0).then(|| {
                            v.registry
                                .as_ref()
                                .and_then(|r| r.entry(id))
                                .map_or(VisionBits::NONE, |e| e.mask)
                        })
                    }),
                    holds: Amount::ZERO < unit.life.amount
                        && unit
                            .life
                            .attribute_kind
                            .is_some_and(|k| k.is_resource() || k.is_food_load()),
                    hp: unit.life.hp,
                    outline: master.outline.with_height(master.outline_height.get()),
                    forest_tile: t.spatial.terrain.as_ref().is_some_and(|ter| {
                        ter.kind(Tile::new(
                            placed.position.x() as i32,
                            placed.position.y() as i32,
                        ))
                        .and_then(|k| ter.terrains.get(k.index()))
                        .is_some_and(|k| k.forest)
                    }),
                })
            })
            .collect()
    }
}
