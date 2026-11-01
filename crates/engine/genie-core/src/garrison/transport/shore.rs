//! The points ashore: a new point after none went ashore, the drop location
//! (`find_drop_location`) and the map as the Transport reads it ([`MapShore`]).

use crate::GenieError;
use crate::garrison::transport::{
    DEEP_WATER, DRAW_SCALE, DROP_GAP, DROP_TRIES, RETRIES, RETRY_SPREAD, RETRY_STEP, STACK_PLACE,
    Shore, WATER,
};
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::Position;
use crate::map::TerrainFlags;
use crate::map::Tile;
use crate::map::terrain::Terrain;
use crate::math::conversions::cvttss2si;
use crate::math::rng::Well512;

/// `FUN_140aa3ba0`'s new point once no unit went ashore: from the old point (x, y), draws of the
/// Action stream ("Unload": x, then y), each `r / 0xccc` tiles plus the old coordinate less
/// [`RETRY_SPREAD`], while the terrain a map read sees at the last draw's tile
/// ([`Terrain::read`], kept while the tile is off the `map`) is water (22, 23 or 1), at most
/// 100; the last draw becomes the point either way. It sails on (true) only when a draw left
/// the water with more than one draw to spare: the 99th and 100th never count.
pub(crate) fn retry_point(
    point: Point,
    map: GridSize,
    terrain: &Terrain,
    rng: &mut Well512,
) -> (Point, bool) {
    let (w, h) = (map.width(), map.height());
    let mut seen = DEEP_WATER;
    let mut at = point;
    let mut left = RETRIES;
    loop {
        let tries = left;
        left = tries - 1;
        if !matches!(seen.get().wrapping_sub(DEEP_WATER.get()), 0 | 1) && seen != WATER {
            break;
        }
        let x = (rng.rand15() / RETRY_STEP) as f32 + point.x() - RETRY_SPREAD;
        let y = (rng.rand15() / RETRY_STEP) as f32 + point.y() - RETRY_SPREAD;
        at = Point::new(x, y);
        let (tx, ty) = (cvttss2si(x), cvttss2si(y));
        if 0 <= tx && 0 <= ty && tx < w && ty < h {
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
    pub center: Point,
    /// The radii (x, y).
    pub radius: Footprint,
}

/// `FUN_141192d10`'s draws (`find_drop_location`) for a unit of master radii `unit_radius` about
/// `around`: up to 50 draws of the Object stream ("`find_drop_location`"). Each picks a side
/// (`r * 4 / 0x7fff`: east, west, north or south past the radii, the unit's own and
/// [`DROP_GAP`]), then a place along it (`r * 2 radius / 32767 - radius`, [`DRAW_SCALE`]); a draw
/// of `0x7fff` (side 4) keeps the place the last draw left in `last` (the caller's across its
/// calls; none yet is [`STACK_PLACE`]). The caller tests each place on the map
/// ([`DropDraws::next_on_map`]): the first in its zone that the unit's master may take and the
/// unit could stand on is the drop location.
pub(crate) struct DropDraws<'a> {
    around: Around,
    unit_radius: Footprint,
    map: GridSize,
    last: &'a mut Option<Point>,
    rng: &'a mut Well512,
    left: u16,
}

impl<'a> DropDraws<'a> {
    /// The draws for a unit of master radii `unit_radius` about `around` on a map of `map`
    /// tiles, the place the last draw left in `last`, drawing from `rng`.
    pub(crate) const fn new(
        around: Around,
        unit_radius: Footprint,
        map: GridSize,
        last: &'a mut Option<Point>,
        rng: &'a mut Well512,
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
    pub(crate) fn next_on_map(&mut self) -> Result<Option<Point>, GenieError> {
        let (w, h) = (self.map.width() as f32, self.map.height() as f32);
        while 0 < self.left {
            self.left -= 1;
            let (x, y) = {
                let p = self.draw()?;
                (p.x(), p.y())
            };
            if 0.0 > x || 0.0 > y || x >= w || y >= h {
                continue;
            }
            return Ok(Some(Point::new(x, y)));
        }
        Ok(None)
    }

    /// One draw: its side, then its place along it, which `last` keeps.
    fn draw(&mut self) -> Result<Point, GenieError> {
        let (cx, cy) = (self.around.center.x(), self.around.center.y());
        let (rx, ry) = (self.around.radius.x(), self.around.radius.y());
        let off_x = rx + (self.unit_radius.x() + DROP_GAP);
        let off_y = ry + (self.unit_radius.y() + DROP_GAP);
        let along = |r: u32, radius: Distance, centre: f32| {
            ((r as i32 as f32 * (radius + radius)) / DRAW_SCALE - radius) + Distance::new(centre)
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

/// A drop location's zone test (`FUN_141192d10`'s, for the Transport's unload): any for `zone`
/// −1 (`u32::MAX`); else the zone of the place's tile on the transport's zone map (`zone_map`:
/// `FUN_1411958c0` on its `CVTTSS2SI` tile, 0 below 0 or without a map), cut to its low byte,
/// is `zone` or touches it (`FUN_141414f70` on both low bytes; without a map none does).
///
/// # Errors
///
/// A zone past the adjacency table.
pub(crate) fn in_zone(
    at: Point,
    zone: u32,
    zone_map: Option<&crate::pathing::zones::ZoneMap>,
) -> Result<bool, GenieError> {
    let (x, y) = (at.x(), at.y());
    if zone == u32::MAX {
        return Ok(true);
    }
    let (tx, ty) = (cvttss2si(x), cvttss2si(y));
    let tile_zone = match zone_map {
        Some(m) if 0 <= tx && 0 <= ty => m.zone(Tile::new(tx, ty)),
        _ => crate::pathing::zones::ZoneId::OFF_MAP,
    };
    let low = tile_zone.byte();
    if u32::from(low) == zone {
        return Ok(true);
    }
    let Some(m) = zone_map else {
        return Ok(false);
    };
    m.adjacent(
        crate::pathing::zones::ZoneId::new(u16::from(low)),
        crate::pathing::zones::ZoneId::new((zone & 0xff) as u16),
    )
    .ok_or(GenieError::unreachable("a zone adjacency past its table"))
}

/// The map as a unit's Transport reads it ([`Shore`]): the map's terrain, the zone maps and the
/// planners, the unit's terrain restriction (its master's `+0x606`; none below 0) and its tile.
pub(crate) struct MapShore<'a> {
    pub(super) terrain: &'a Terrain,
    pub(super) zones: &'a mut crate::pathing::zones::Zones,
    pub(super) pathing: &'a mut crate::pathing::Pathing,
    pub(super) tiles: &'a crate::map::spatial::Tiles,
    pub(super) map: GridSize,
    pub(super) restriction: Option<crate::map::RestrictionId>,
    pub(super) tile: Tile,
}

impl<'a> MapShore<'a> {
    /// The map about a unit of terrain restriction `restriction` at `position`.
    ///
    /// # Errors
    ///
    /// A map without its terrain.
    pub(crate) fn new(
        spatial: &'a crate::map::spatial::Spatial,
        zones: &'a mut crate::pathing::zones::Zones,
        pathing: &'a mut crate::pathing::Pathing,
        restriction: Option<crate::map::RestrictionId>,
        position: Position,
    ) -> Result<Self, GenieError> {
        let terrain = spatial
            .terrain
            .as_ref()
            .ok_or(crate::pathing::zones::NO_TERRAIN)?;
        Ok(Self {
            terrain,
            zones,
            pathing,
            tiles: spatial.tiles(),
            map: spatial.map,
            restriction,
            tile: position.tile(),
        })
    }

    /// The unit's zone map (`FUN_1411931c0`: the map's for its restriction, `FUN_1414138d0`).
    pub(super) fn zone_map(
        &mut self,
    ) -> Result<Option<&crate::pathing::zones::ZoneMap>, GenieError> {
        let Some(r) = self.restriction else {
            return Ok(None);
        };
        let values = self
            .terrain
            .restriction(r)
            .ok_or(crate::pathing::zones::UNKNOWN_RESTRICTION)?;
        let (w, h) = (self.map.width(), self.map.height());
        Ok(Some(self.zones.of(
            GridSize::new(w, h),
            &self.terrain.kinds,
            values,
        )?))
    }
}

impl Shore for MapShore<'_> {
    fn zone(&mut self, tile: Tile) -> Result<crate::pathing::zones::ZoneId, GenieError> {
        if tile.x() < 0 || tile.y() < 0 {
            return Ok(crate::pathing::zones::ZoneId::OFF_MAP);
        }
        Ok(self
            .zone_map()?
            .map_or(crate::pathing::zones::ZoneId::OFF_MAP, |m| m.zone(tile)))
    }

    fn near(&mut self, zone: u8, reach: Distance) -> Result<bool, GenieError> {
        let tile = self.tile;
        Ok(self.zone_map()?.is_some_and(|m| {
            m.near(
                crate::pathing::zones::ZoneId::new(u16::from(zone)),
                tile,
                reach,
            )
            .is_some()
        }))
    }

    fn flags(&self, tile: Tile) -> TerrainFlags {
        self.terrain.flags_at(tile)
    }

    fn shore_point(&mut self, from: Point, to: Point) -> Option<Point> {
        let planner = self.pathing.planner(self.restriction?, self.tiles)?;
        planner.last_open_point(from, to)
    }
}
