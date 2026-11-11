//! The placement test's terrains (`FUN_141123f80`): the hill mode, the placement and side
//! terrains and the tiles beside a box.

use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::Restriction;
use crate::map::TerrainFlags;
use crate::map::TerrainId;
use crate::map::Tile;
use crate::map::TileRect;
use crate::objects::ObjectId;
use crate::objects::masters::ClassId;
use crate::objects::masters::HillMode;
use crate::objects::masters::PlacementTerrains;
use crate::players::PlayerId;
use crate::production::spawn::WorldMap;
use crate::{GenieError, GenieResult};

/// A hill mode's test where the map's elevations are not known.
const NO_ELEVATIONS: GenieError =
    GenieError::unsupported("a hill mode where the elevations are not known");

bitflags::bitflags! {
    /// What BUILD's placement test runs ([`BuildSite`]).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct SiteChecks: u8 {
        /// A human player (`p6`: the explored test).
        const HUMAN = 1;
        /// `p13` 1: `FUN_141123f80`'s and `FUN_141264db0`'s own scans run, the latter passing over
        /// the units (type 70); BUILD's runs neither.
        const SCANS = 1 << 1;
        /// The world's zone mode (`+0x820`, `FUN_1415e8e10`).
        const ZONES = 1 << 2;
    }
}

/// What BUILD's placement test reads of the building master and the game.
#[derive(Clone, Copy, Debug)]
pub struct BuildSite<'world> {
    /// The player who places it.
    pub(crate) player: PlayerId,
    /// What the test runs.
    pub(crate) checks: SiteChecks,
    /// The master's radii the test measures by (`p9`): its clearance radii (`+0x5fc`, `+0x600`)
    /// for BUILD, its size radii (`+0x5d8`, `+0x5dc`) for the Build's test of its foundation.
    pub(crate) radius: Footprint,
    /// The master's height radius (`+0x5e0`) is above 0, which the static scans need.
    pub(crate) tall: bool,
    /// The master's class.
    pub(crate) class: ClassId,
    /// The master's `obstruction_type` is neither 0 nor 12 (`FUN_1411205c0`).
    pub(crate) obstructs: bool,
    /// The master's terrain restriction's value per terrain.
    pub(crate) restriction: &'world Restriction,
    /// `hill_mode` (`+0x604`, `p7`).
    pub(crate) hill_mode: HillMode,
    /// The placement terrains (`+0x5f2`..`+0x5f8`, [`placement_terrains`]).
    pub(crate) placement: PlacementTerrains,
    /// The objects the scan passes over (`p12` and, for a building, its annexes, `FUN_14123d090`):
    /// none for BUILD, a reseeding gatherer's old target ([`crate::world::World::reseed`]).
    pub(crate) own: &'world [ObjectId],
}

/// The base elevation of the hill mode's test ([`hill_fails`]): `FUN_141123f80` reads the
/// elevation (`+0x14`) of the box's first column's last row, `corner`, before its tiles. Only mode
/// 3 measures by it, so for the others the port reads none.
///
/// # Errors
///
/// Mode 3 where the map's elevations are not known.
pub(super) fn hill_base(
    mode: HillMode,
    map: &WorldMap<'_>,
    corner: Tile,
) -> GenieResult<Option<u8>> {
    if mode == HillMode::ONE_STEP {
        Ok(Some(map.elevation(corner).ok_or(NO_ELEVATIONS)?))
    } else {
        Ok(None)
    }
}

/// The hill mode's test of a tile of the box (`FUN_141123f80`'s `p7` part, the master's
/// `hill_mode` `+0x604`): mode 1 takes flat ground and the four side slopes (shapes 0 and 5 to
/// 8), mode 2 flat ground alone, mode 3 a tile of the box's first column or last row within one
/// step of the `base` elevation; the other modes take any. True when the tile fails (code 3).
pub(super) fn hill_fails(
    mode: HillMode,
    map: &WorldMap<'_>,
    tile: Tile,
    corner: Tile,
    base: Option<u8>,
) -> GenieResult<bool> {
    let shape = || {
        map.shape(tile).ok_or(GenieError::unsupported(
            "a hill mode where the tiles' shapes are not known",
        ))
    };
    Ok(match mode {
        HillMode::SIDE_SLOPES => !matches!(shape()?, 0 | 5..=8),
        HillMode::FLAT => shape()? != 0,
        HillMode::ONE_STEP if tile.x() == corner.x() || tile.y() == corner.y() => {
            let elevation = i32::from(map.elevation(tile).ok_or(NO_ELEVATIONS)?);
            let base_elevation = i32::from(base.unwrap_or(0));
            elevation < base_elevation - 1 || base_elevation + 1 < elevation
        }
        _ => false,
    })
}

/// `FUN_141123f80`'s placement terrains (`placement`: the master's side terrains `+0x5f2`,
/// `+0x5f4`, then its terrains `+0x5f6`, `+0x5f8`; −1 for none) for a box of tiles `tile_rect` (x0,
/// y0, x1, y1) around point `point`: false where they refuse it (the test's code 1). With a
/// terrain, the point's tile must be of one of the two, or share shallow water or shallows
/// ([`TerrainFlags::SHALLOW_WATER`], [`TerrainFlags::SHALLOWS`]: the terrains' `.dat` flags,
/// [`WorldMap::terrain_flags`]) with them, and the box shrinks to that tile. With a side terrain,
/// one of the tiles beside the box (the row below, the column right, the row above and the column
/// left) must be of a side terrain, or of a terrain with ice or beach ([`TerrainFlags::ICE`],
/// [`TerrainFlags::BEACH`]) when a side terrain has it. A terrain of −1 beside one of 0 or more
/// (its flags read past the table) is not modelled.
///
/// # Errors
///
/// Where the map cannot tell.
pub(crate) fn placement_terrains(
    placement: PlacementTerrains,
    map: &WorldMap<'_>,
    point: Point,
    bounds: TileRect,
) -> GenieResult<bool> {
    let PlacementTerrains {
        beside: [side1, side2],
        on: [t1, t2],
    } = placement;
    let mut tile_rect = bounds;
    if t1.is_some() || t2.is_some() {
        let at = Tile::new(point.x() as i32, point.y() as i32);
        if !point_terrain_allows(map, [t1, t2], at)? {
            return Ok(false);
        }
        tile_rect = TileRect::of(at);
    }
    if side1.is_none() && side2.is_none() {
        return Ok(true);
    }
    let sides = side_terrains(map, [side1, side2])?;
    for tile in tiles_beside(tile_rect, map.size()) {
        if sides.contains(&map.terrain(tile)?) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// [`placement_terrains`]'s flags of terrain `terrain_id` (the `.dat`'s,
/// [`WorldMap::terrain_flags`]).
///
/// # Errors
///
/// A terrain of −1 or beyond the terrains (its flags read past the table), not modelled.
pub(super) fn placement_flags(
    map: &WorldMap<'_>,
    terrain: Option<TerrainId>,
) -> GenieResult<TerrainFlags> {
    terrain
        .and_then(|terrain_id| map.terrain_flags(terrain_id.index()))
        .ok_or(GenieError::unsupported(
            "placement terrains with one of -1 or beyond the terrains",
        ))
}

/// [`placement_terrains`]'s test of the point's tile `at` against the master's terrains
/// `terrains`: of one of the two, or sharing shallow water or shallows with them.
///
/// # Errors
///
/// Where the map cannot tell, and a terrain whose flags it does not hold.
pub(super) fn point_terrain_allows(
    map: &WorldMap<'_>,
    [t1, t2]: [Option<TerrainId>; 2],
    at: Tile,
) -> GenieResult<bool> {
    let tile = map.terrain(at)?;
    if Some(tile) == t1 || Some(tile) == t2 {
        return Ok(true);
    }
    let (f1, f2, ft) = (
        placement_flags(map, t1)?,
        placement_flags(map, t2)?,
        placement_flags(map, Some(tile))?,
    );
    let shares = |flag| (f1.contains(flag) || f2.contains(flag)) && ft.contains(flag);
    Ok(shares(TerrainFlags::SHALLOW_WATER) || shares(TerrainFlags::SHALLOWS))
}

/// [`placement_terrains`]'s side terrains: the master's own (those of 0 or more), then for ice
/// and then beach, when a side terrain has it, every other terrain with it in the terrains'
/// order.
///
/// # Errors
///
/// A terrain whose flags the map does not hold.
pub(super) fn side_terrains(
    map: &WorldMap<'_>,
    [side1, side2]: [Option<TerrainId>; 2],
) -> GenieResult<Vec<TerrainId>> {
    let (f1, f2) = (placement_flags(map, side1)?, placement_flags(map, side2)?);
    let mut sides: Vec<TerrainId> = [side1, side2].into_iter().flatten().collect();
    for flag in [TerrainFlags::ICE, TerrainFlags::BEACH] {
        if f1.contains(flag) || f2.contains(flag) {
            for index in 0..map.terrain_count() {
                let terrain_id = TerrainId::named(u8::try_from(index).unwrap_or(u8::MAX));
                let other = Some(terrain_id) != side1 && Some(terrain_id) != side2;
                if placement_flags(map, Some(terrain_id))?.contains(flag) && other {
                    sides.push(terrain_id);
                }
            }
        }
    }
    Ok(sides)
}

/// [`placement_terrains`]'s tiles beside the box `tile_rect` (x0, y0, x1, y1) on a map of `size`:
/// the row below, the column right, the row above (right to left) and the column left (top to
/// bottom), each where the map holds it.
pub(super) fn tiles_beside(tile_rect: TileRect, map: GridSize) -> Vec<Tile> {
    let (width, height) = (map.width(), map.height());
    let (x0, y0, x1, y1) = (
        tile_rect.x0(),
        tile_rect.y0(),
        tile_rect.x1(),
        tile_rect.y1(),
    );
    let mut beside = Vec::new();
    if 0 < y0 {
        beside.extend((x0..=x1).map(|x| Tile::new(x, y0 - 1)));
    }
    if x1 + 1 < width {
        beside.extend((y0..=y1).map(|y| Tile::new(x1 + 1, y)));
    }
    if y1 + 1 < height {
        beside.extend((x0..=x1).rev().map(|x| Tile::new(x, y1 + 1)));
    }
    if 0 < x0 {
        beside.extend((y0..=y1).rev().map(|y| Tile::new(x0 - 1, y)));
    }
    beside
}
