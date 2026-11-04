//! What the explorer's searches read of the world for one unit ([`ScoutArea`]): the tiles its
//! player explored and sees, the zones of its restriction, its stand and reach tests and its gap
//! to a tile.

use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Position;
use crate::map::Tile;
use crate::map::spatial::Spatial;
use crate::map::spatial::TileSight;
use crate::math::fmath::sqrt;
use crate::math::rng::Stream;
use crate::movement::{Me, Mover, Planning};
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterTables;
use crate::objects::unit::Unit;
use crate::pathing::zones::{UNKNOWN_RESTRICTION, ZoneId, ZoneMap};
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::world::World;
use crate::world::index::ObjectsRef;
use crate::{GenieError, GenieResult};

/// What the auto scout's searches ([`crate::movement::explore`]) and the AI's explorer
/// ([`crate::players::ai_module::InfoModule::closest_unexplored_tile`]) read of the world for
/// one unit.
pub(crate) struct ScoutArea<'world> {
    pub(crate) spatial: &'world mut Spatial,
    /// The world's objects, whose gaits the stand and reach tests read.
    pub(crate) by_id: ObjectsRef<'world>,
    pub(crate) zones: &'world ZoneMap,
    pub(crate) plan: Planning<'world>,
    pub(crate) mover: &'world Mover,
    /// The scout's place (its row's static layer), which the search does not move.
    pub(crate) at: Position,
    pub(crate) me: Me,
    pub(crate) player: PlayerId,
    pub(crate) master: MasterId,
    pub(crate) radius: Footprint,
    /// The reach of slot 0x3d8: the line of sight less a tile.
    pub(crate) reach: Distance,
    /// The search reads the player's scout map for the tiles seen (`FUN_140c4b130` with one).
    pub(crate) scouted: bool,
}

impl<'world> ScoutArea<'world> {
    /// The searches' reads for `unit` of player `player` (`me` its actions' state, `player_kind`
    /// its owner's kind), with the map's size and the unit's line of sight: the zones of its
    /// mover's restriction (`FUN_1411931c0`), its master's footprint, and its reach, the line of
    /// sight less the explore tuning's margin. The scout map is not read until the caller sets
    /// [`ScoutArea::scouted`].
    ///
    /// # Errors
    ///
    /// A unit without its master or movement state, a map without its terrain, or a restriction
    /// the terrain has not got.
    pub(crate) fn of(
        world: &'world mut World,
        unit: &Unit,
        me: Me,
        player: PlayerId,
        player_kind: PlayerKind,
    ) -> GenieResult<(Self, GridSize, Distance)> {
        let master = unit.master(
            MasterTables::new(&world.model.masters, &world.parts.own_master),
            player,
        )?;
        let mover = world
            .parts
            .mover_of(unit.id)
            .ok_or(GenieError::unsupported(
                "an auto scout without movement state",
            ))?;
        let map = world.spatial.map;
        let terrain = world
            .spatial
            .terrain
            .as_ref()
            .ok_or(GenieError::unsupported(
                "the map's zones without its terrain",
            ))?;
        let values = terrain
            .restriction(mover.stats.restriction)
            .ok_or(UNKNOWN_RESTRICTION)?;
        let zones = world.zones.of(map, &terrain.kinds, values)?;
        let area = ScoutArea {
            spatial: &mut world.spatial,
            by_id: ObjectsRef::new(
                &world.players,
                &world.index,
                &world.model.masters,
                &world.parts,
                &world.model.roles,
                &world.model.tuning,
            ),
            zones,
            plan: Planning {
                pathing: &mut world.pathing,
                now: world.world_time,
                player_kind,
                object: world.streams.get_mut(Stream::Object),
                groups: &world.groups,
            },
            mover,
            at: unit.position(),
            me,
            player,
            master: master.id,
            radius: master.radius,
            reach: master.line_of_sight - Distance::new(world.model.tuning.explore.los_margin),
            scouted: false,
        };
        Ok((area, map, master.line_of_sight))
    }

    /// The player saw the tile or sees it (`FUN_141285350` not 0); with the scout map
    /// ([`ScoutArea::scouted`]) the map's 1.
    ///
    /// # Errors
    ///
    /// A tile off the scout map, or a test without the players' sight.
    pub(crate) fn explored(&self, tile: Tile) -> GenieResult<bool> {
        if self.scouted {
            return self
                .spatial
                .scout_map(self.player)
                .and_then(|influence_map| influence_map.get(tile))
                .map(|value| value == 1)
                .ok_or(GenieError::unsupported("a tile off the player's scout map"));
        }
        self.spatial.explored(self.player, tile)
    }

    /// The player sees the tile now (`FUN_141285350` gives 0xf).
    pub(crate) fn visible_now(&self, tile: Tile) -> bool {
        self.spatial
            .scout_sight(self.player, tile)
            .is_some_and(TileSight::seen)
    }

    /// The tile's zone for the unit's restriction (`FUN_1411958c0`).
    pub(crate) fn zone(&self, tile: Tile) -> ZoneId {
        self.zones.zone(tile)
    }

    /// The unit's own zone (`FUN_14118eea0`): its place's tile's.
    pub(crate) fn own_zone(&self) -> ZoneId {
        let (x, y) = (self.at.x(), self.at.y());
        self.zones.zone(Tile::new(x as i32, y as i32))
    }

    /// The unit could stand at the tile's corner (slot 0x278,
    /// [`crate::map::spatial::Spatial::point_clear`]).
    ///
    /// # Errors
    ///
    /// What the point query does not model.
    pub(crate) fn clear(&mut self, tile: Tile) -> GenieResult<bool> {
        let mask = self.mover.stats.restriction_mask;
        self.spatial.point_clear(
            self.me.id,
            tile.corner(),
            Distance::new(self.radius.x()),
            mask,
            self.by_id,
        )
    }

    /// The unit reaches the tile's corner within its line of sight less a tile (slot 0x3d8,
    /// [`crate::movement::Mover::reaches`]).
    ///
    /// # Errors
    ///
    /// What the reach test does not model.
    pub(crate) fn reaches(&mut self, tile: Tile) -> GenieResult<bool> {
        self.mover.reaches(
            (self.at, &self.me),
            (self.spatial, self.by_id),
            &mut self.plan,
            tile.corner(),
            self.reach,
            self.master,
        )
    }

    /// `FUN_14118faf0`: the gap from the unit's footprint to the tile on each axis (at least 0),
    /// then its length.
    pub(crate) fn distance(&self, tile: Tile) -> f32 {
        let gap = |delta: f32, radius: f32| {
            let beyond = delta.abs() - radius;
            if beyond <= 0.0 { 0.0 } else { beyond }
        };
        let gx = gap(tile.x() as f32 - self.at.x(), self.radius.x());
        let gy = gap(tile.y() as f32 - self.at.y(), self.radius.y());
        sqrt(gy * gy + gx * gx)
    }
}
