//! The points ashore: a new point after none went ashore, the drop location
//! (`find_drop_location`) and the map as the Transport reads it ([`MapShore`]).

use crate::garrison::transport::{
    DEEP_WATER, DRAW_SCALE, DROP_TRIES, RETRIES, RETRY_STEP, STACK_PLACE, WATER,
};
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::Position;
use crate::map::RestrictionId;
use crate::map::TerrainFlags;
use crate::map::Tile;
use crate::map::spatial::{Spatial, Tiles};
use crate::map::terrain::Terrain;
use crate::math::conversions::cvttss2si;
use crate::math::rng::Well512;
use crate::pathing::Pathing;
use crate::pathing::zones::ZoneId;
use crate::pathing::zones::{NO_TERRAIN, UNKNOWN_RESTRICTION, ZoneMap, Zones};
use crate::world::layer::generated::TilePathTuning;
use crate::world::layer::generated::TransportTuning;
use crate::{GenieError, GenieResult};

/// `FUN_140aa3ba0`'s new point once no unit went ashore: from the old point (x, y), draws of the
/// Action stream ("Unload": x, then y), each `r / 0xccc` tiles plus the old coordinate less
/// `TransportTuning::retry_spread`, while the terrain a map read sees at the last draw's tile
/// ([`Terrain::read`], kept while the tile is off the `map`) is water (22, 23 or 1), at most
/// 100; the last draw becomes the point either way. It sails on (true) only when a draw left
/// the water with more than one draw to spare: the 99th and 100th never count.
pub(crate) fn retry_point(
    point: Point,
    map: GridSize,
    terrain: &Terrain,
    rng: &mut Well512,
    transport_tuning: &TransportTuning,
) -> (Point, bool) {
    let (width, height) = (map.width(), map.height());
    let mut seen = DEEP_WATER;
    let mut at = point;
    let mut left = RETRIES;
    loop {
        let tries = left;
        left = tries - 1;
        if !matches!(seen.get().wrapping_sub(DEEP_WATER.get()), 0 | 1) && seen != WATER {
            break;
        }
        let x = (rng.rand15() / RETRY_STEP) as f32 + point.x() - transport_tuning.retry_spread;
        let y = (rng.rand15() / RETRY_STEP) as f32 + point.y() - transport_tuning.retry_spread;
        at = Point::new(x, y);
        let (tx, ty) = (cvttss2si(x), cvttss2si(y));
        if 0 <= tx && 0 <= ty && tx < width && ty < height {
            seen = terrain.read(Tile::new(tx, ty).short());
        }
        if left <= 0 {
            left = tries - 2;
            break;
        }
    }
    (at, 0 < left)
}

/// Where a drop location ([`DropDraws`]) puts a unit: about a centre within radii.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Around {
    /// The centre (x, y).
    pub(crate) center: Point,
    /// The radii (x, y).
    pub(crate) radius: Footprint,
}

/// `FUN_141192d10`'s draws (`find_drop_location`) for a unit of master radii `unit_radius` about
/// `around`: up to 50 draws of the Object stream ("`find_drop_location`"). Each picks a side
/// (`r * 4 / 0x7fff`: east, west, north or south past the radii, the unit's own and
/// `TransportTuning::drop_gap`), then a place along it (`r * 2 radius / 32767 - radius`, [`DRAW_SCALE`]); a draw
/// of `0x7fff` (side 4) keeps the place the last draw left in `last` (the caller's across its
/// calls; none yet is [`STACK_PLACE`]). The caller tests each place on the map
/// ([`DropDraws::next_on_map`]): the first in its zone that the unit's master may take and the
/// unit could stand on is the drop location.
pub(crate) struct DropDraws<'world> {
    around: Around,
    unit_radius: Footprint,
    map: GridSize,
    last: &'world mut Option<Point>,
    rng: &'world mut Well512,
    left: u16,
}

impl<'world> DropDraws<'world> {
    /// The draws for a unit of master radii `unit_radius` about `around` on a map of `map`
    /// tiles, the place the last draw left in `last`, drawing from `rng`.
    pub(crate) const fn new(
        around: Around,
        unit_radius: Footprint,
        map: GridSize,
        last: &'world mut Option<Point>,
        rng: &'world mut Well512,
    ) -> Self {
        Self {
            around,
            unit_radius,
            map,
            last,
            rng,
            left: DROP_TRIES,
        }
    }

    /// The next draw's place on the map (`COMISS`: a NaN coordinate passes); a place off it
    /// uses its draw up. None once the 50 draws are spent.
    ///
    /// # Errors
    ///
    /// [`STACK_PLACE`].
    pub(crate) fn next_on_map(
        &mut self,
        transport_tuning: &TransportTuning,
    ) -> GenieResult<Option<Point>> {
        let (width, height) = (self.map.width() as f32, self.map.height() as f32);
        while 0 < self.left {
            self.left -= 1;
            let (x, y) = {
                let point = self.draw(transport_tuning)?;
                (point.x(), point.y())
            };
            if 0.0 > x || 0.0 > y || x >= width || y >= height {
                continue;
            }
            return Ok(Some(Point::new(x, y)));
        }
        Ok(None)
    }

    /// One draw: its side, then its place along it, which `last` keeps.
    fn draw(&mut self, transport_tuning: &TransportTuning) -> GenieResult<Point> {
        let (cx, cy) = (self.around.center.x(), self.around.center.y());
        let (rx, ry) = (self.around.radius.x(), self.around.radius.y());
        let off_x = rx + (self.unit_radius.x() + transport_tuning.drop_gap);
        let off_y = ry + (self.unit_radius.y() + transport_tuning.drop_gap);
        let along = |draw: u32, radius: Distance, centre: f32| {
            ((draw as i32 as f32 * (radius + radius)) / DRAW_SCALE - radius) + Distance::new(centre)
        };
        let side = (self.rng.rand15() as i32 * 4) / 0x7fff;
        let place = match side {
            0 => Point::new(
                cx + off_x,
                (along(self.rng.rand15(), Distance::new(ry), cy)).get(),
            ),
            1 => Point::new(
                cx - off_x,
                (along(self.rng.rand15(), Distance::new(ry), cy)).get(),
            ),
            2 => {
                let y = cy + off_y;
                Point::new((along(self.rng.rand15(), Distance::new(rx), cx)).get(), y)
            }
            3 => {
                let y = cy - off_y;
                Point::new((along(self.rng.rand15(), Distance::new(rx), cx)).get(), y)
            }
            _ => self.last.ok_or(STACK_PLACE)?,
        };
        *self.last = Some(place);
        Ok(place)
    }
}

/// A drop location's zone test (`FUN_141192d10`'s, for the Transport's unload): any for no
/// `zone` (the game's −1, `u32::MAX`); else the zone of the place's tile on the transport's zone map (`zone_map`:
/// `FUN_1411958c0` on its `CVTTSS2SI` tile, 0 below 0 or without a map), cut to its low byte,
/// is `zone` or touches it (`FUN_141414f70` on both low bytes; without a map none does).
///
/// # Errors
///
/// A zone past the adjacency table.
pub(crate) fn in_zone(
    at: Point,
    zone: Option<ZoneId>,
    zone_map: Option<&ZoneMap>,
) -> GenieResult<bool> {
    let (x, y) = (at.x(), at.y());
    let Some(zone) = zone else {
        return Ok(true);
    };
    let (tx, ty) = (cvttss2si(x), cvttss2si(y));
    let tile_zone = match zone_map {
        Some(zone_map) if 0 <= tx && 0 <= ty => zone_map.zone(Tile::new(tx, ty)),
        _ => ZoneId::OFF_MAP,
    };
    let low = tile_zone.byte();
    if u16::from(low) == zone.get() {
        return Ok(true);
    }
    let Some(zone_map) = zone_map else {
        return Ok(false);
    };
    // the adjacency table covers its zones, and `adjacent` answers false for a zone past them
    zone_map
        .adjacent(
            ZoneId::new(u16::from(low)),
            ZoneId::new(u16::from(zone.byte())),
        )
        .ok_or(GenieError::unreachable("a zone adjacency past its table"))
}

/// The map as a unit's Transport reads it: the map's terrain, the zone maps and the
/// planners, the unit's terrain restriction (its master's `+0x606`; none below 0), its tile and
/// the tile paths' tuning values (the shore point's walk).
#[derive(Debug)]
pub struct MapShore<'world> {
    pub(super) terrain: &'world Terrain,
    pub(super) zones: &'world mut Zones,
    pub(super) pathing: &'world mut Pathing,
    pub(super) tiles: &'world Tiles,
    pub(super) map: GridSize,
    pub(super) restriction: Option<RestrictionId>,
    pub(super) tile: Tile,
    pub(super) tile_path_tuning: &'world TilePathTuning,
}

impl<'world> MapShore<'world> {
    /// The map about a unit of terrain restriction `restriction` at `position`.
    ///
    /// # Errors
    ///
    /// A map without its terrain.
    pub(crate) fn new(
        spatial: &'world Spatial,
        zones: &'world mut Zones,
        pathing: &'world mut Pathing,
        restriction: Option<RestrictionId>,
        position: Position,
        tile_path_tuning: &'world TilePathTuning,
    ) -> GenieResult<Self> {
        let terrain = spatial.terrain.as_ref().ok_or(NO_TERRAIN)?;
        Ok(Self {
            terrain,
            zones,
            pathing,
            tiles: spatial.tiles(),
            map: spatial.map,
            restriction,
            tile: position.tile(),
            tile_path_tuning,
        })
    }

    /// The unit's zone map (`FUN_1411931c0`: the map's for its restriction, `FUN_1414138d0`).
    pub(super) fn zone_map(&mut self) -> GenieResult<Option<&ZoneMap>> {
        let Some(restriction_id) = self.restriction else {
            return Ok(None);
        };
        let values = self
            .terrain
            .restriction(restriction_id)
            .ok_or(UNKNOWN_RESTRICTION)?;
        let (width, height) = (self.map.width(), self.map.height());
        Ok(Some(self.zones.of(
            GridSize::new(width, height),
            &self.terrain.kinds,
            values,
        )?))
    }

    /// `FUN_1411958c0`: the unit's zone (its restriction's zone map, `FUN_1411931c0`) at `tile`:
    /// 0 for a tile below 0, or without a zone map.
    ///
    /// # Errors
    ///
    /// A map the caller cannot read.
    pub(crate) fn zone(&mut self, tile: Tile) -> GenieResult<ZoneId> {
        if tile.x() < 0 || tile.y() < 0 {
            return Ok(ZoneId::OFF_MAP);
        }
        Ok(self
            .zone_map()?
            .map_or(ZoneId::OFF_MAP, |zone_map| zone_map.zone(tile)))
    }

    /// `FUN_1411a7570`: a tile of zone `zone` lies within `reach` of the unit's tile (its place
    /// cut by `CVTTSS2SI`, [`crate::pathing::zones::ZoneMap::near`]); no without a zone map.
    ///
    /// # Errors
    ///
    /// A map the caller cannot read.
    pub(crate) fn near(&mut self, zone: ZoneId, reach: Distance) -> GenieResult<bool> {
        let tile = self.tile;
        // the game passes the zone's low byte
        let zone = ZoneId::new(u16::from(zone.byte()));
        Ok(self
            .zone_map()?
            .is_some_and(|zone_map| zone_map.near(zone, tile, reach).is_some()))
    }

    /// The terrain table's flags of `tile`, its x and y shorts
    /// ([`crate::map::terrain::Terrain::flags_at`]).
    #[must_use]
    pub(crate) fn flags(&self, tile: Tile) -> TerrainFlags {
        self.terrain.flags_at(tile)
    }

    /// `FUN_140f7c390` with the planner of the unit's restriction (`FUN_140f7cb30`, none without a
    /// master): the last point of the planner's path from `from` to `to` whose tile has an edge
    /// ([`crate::pathing::tile_path::Planner::last_open_point`]); None without a planner or such a
    /// point.
    ///
    /// # Errors
    ///
    /// What the planner's searches fail on.
    pub(crate) fn shore_point(&mut self, from: Point, to: Point) -> GenieResult<Option<Point>> {
        let Some(restriction_id) = self.restriction else {
            return Ok(None);
        };
        let Some(planner) = self.pathing.planner(restriction_id, self.tiles)? else {
            return Ok(None);
        };
        planner.last_open_point(from, to, self.tile_path_tuning)
    }
}
