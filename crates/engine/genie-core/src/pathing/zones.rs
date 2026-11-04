//! The map's zones (`RGE_Zone_Map`, made by `FUN_141410680`, labelled by `FUN_141413190`): every
//! tile gets the number of the patch of tiles it belongs to, a patch being the tiles a 4-way
//! flood reaches over tiles that are, like its first, passable or not for a terrain restriction
//! (its value for the tile's terrain, map tile `+0x9`, above 0). Patches are numbered in the order
//! of their first tile, row by row; past 511 the numbers start again at 0 (`FUN_141413190`: the
//! 513th patch is 0, the next 1, ...). Each map also caches its zones' tile counts, holds their
//! number and which zones touch (`FUN_141412a80`).
//!
//! The map keeps one zone map per passability pattern (`map+0x5e28`, `FUN_1414138d0`: a map is
//! shared by the restrictions whose values begin with its own signs), made the first time a unit
//! of the pattern asks for its zone (`FUN_1411931c0`, cached on the unit at `+0x698`). The zone
//! maps are saved (`FUN_141414790`, read by `FUN_1414110f0`): a game loaded from a save holds
//! them as they were made, and a map is not made again when the terrain changes later.

use crate::map::Distance;
use crate::map::Restriction;
use crate::map::TerrainId;
use crate::map::{GridSize, Tile, TileOffset};
use crate::math::fmath::{at_most_or_unordered, sqrt};
use crate::world::slots::CheckedIndexExt;
use crate::{GenieError, GenieResult};

/// The number after the last a patch takes; the next patch takes 0 again.
const PATCHES: u16 = 0x200;
/// What a tile not yet reached holds: the patches' number ([`PATCHES`]).
const UNREACHED: ZoneId = ZoneId::new(PATCHES);

/// The flood's neighbours of a tile in the order it takes them (`FUN_141413340`): left, up
/// (y less 1), right, down.
const FLOOD_STEPS: [TileOffset; 4] = TileOffset::table([[-1, 0], [0, -1], [1, 0], [0, 1]]);

/// The zones a map caches a tile count for (`+0x000`, 0x800 bytes of ints).
const COUNTED: usize = 0x200;

/// A terrain restriction the slice's data lacks, whose zone map the slice cannot make.
pub const UNKNOWN_RESTRICTION: GenieError =
    GenieError::unsupported("a restriction the slice's data has not got");

/// The map's zones of a world without its terrain.
pub const NO_TERRAIN: GenieError = GenieError::unsupported("the map's zones without its terrain");

/// A tile's zone (`RGE_Zone_Map`'s `+0x808` shorts): the number of the patch it belongs to,
/// 0 to 511 (0 off the map); a patch not yet reached by the labelling holds 512.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ZoneId(u16);

impl ZoneId {
    /// A tile's zone off the map (`FUN_141413890`), and the first patch's.
    pub const OFF_MAP: Self = Self(0);

    /// The zone of number `value`.
    #[must_use]
    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    /// Its number.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }

    /// Its place in the per-zone tables.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// Its low byte, which the explore's and the transport's byte compares read.
    #[must_use]
    pub const fn byte(self) -> u8 {
        self.0.to_le_bytes()[0]
    }
}

/// The zones of one passability pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZoneMap {
    size: GridSize,
    /// The pattern (`+0x818`, the restriction's values the map was made for): for each terrain,
    /// the value is above 0.
    pub passable: Vec<bool>,
    /// Each tile's zone (`+0x808`, row by row; `+0x810` the rows).
    zones: Vec<ZoneId>,
    /// Each zone's tile count (`+0x000`, `numberTilesInZoneValue`), cached the first time a flood
    /// ends on the zone while its count is below 1 (`FUN_141413bb0`).
    tiles: Vec<i32>,
    /// The zones' number (`+0x838`, `mNumberZones`): the patches the last labelling made, 0x200
    /// once the numbering started again.
    count: i32,
    /// Which zones touch (`+0x840`, `count` x `count` bytes as `FUN_141412a80` built them, row by
    /// the zone of a tile, column by the zone of a tile among its eight neighbours or itself).
    touching: Vec<bool>,
    /// For two zones that do not touch, the first zone that touches both (`+0x848`, `count` x
    /// `count` shorts as `FUN_141412a80` built them). `None` where the game's table holds memory
    /// it never wrote: the allocation is twice the `count * count` bytes it clears.
    through: Vec<Option<ZoneId>>,
}

impl ZoneMap {
    /// The zone map of the map's terrain (`kinds`, row by row) for a restriction whose value per
    /// terrain is `values` (`FUN_141410680`): its tile counts cleared, every tile allocated 0, the
    /// labelling (`FUN_141413190`) and the zones' adjacency (`FUN_141412a80`). A terrain the
    /// restriction has no value for counts as impassable.
    #[must_use]
    pub fn new(size: GridSize, kinds: &[TerrainId], values: &Restriction) -> Self {
        let count = (size.width().max(0) * size.height().max(0)) as usize;
        let mut map = Self {
            size,
            passable: values.values().iter().map(|&value| 0.0 < value).collect(),
            zones: vec![ZoneId::OFF_MAP; count],
            tiles: vec![0; COUNTED],
            count: 0,
            touching: Vec::new(),
            through: Vec::new(),
        };
        map.label(kinds);
        map.connect();
        map
    }

    /// A zone map as a save holds it (`FUN_141410870`): its tile counts, zones, restriction values
    /// and zone count read, then the zones' adjacency built (`FUN_141412a80`).
    #[must_use]
    pub fn from_saved(
        size: GridSize,
        tiles: &[u32],
        zones: &[u16],
        values: &Restriction,
        count: u32,
    ) -> Self {
        let mut counts = vec![0; COUNTED];
        for (count, &tile) in counts.iter_mut().zip(tiles) {
            *count = tile as i32;
        }
        let mut map = Self {
            size,
            passable: values.values().iter().map(|&value| 0.0 < value).collect(),
            zones: zones.iter().copied().map(ZoneId::new).collect(),
            tiles: counts,
            count: count as i32,
            touching: Vec::new(),
            through: Vec::new(),
        };
        map.connect();
        map
    }

    /// Whether the restriction is above 0 on a tile's terrain (`+0x818` indexed by map tile
    /// `+0x9`).
    fn open(&self, kinds: &[TerrainId], index: usize) -> bool {
        kinds
            .get(index)
            .and_then(|&terrain_id| self.passable.get(terrain_id.index()))
            .copied()
            .unwrap_or(false)
    }

    /// `FUN_141413190`: the zones numbered again from the terrain `kinds`. The game fills and scans
    /// with its outer loop over the width as rows and its inner loop over the height as columns,
    /// the scan guarded by row < height and column < width: on a square map that is every tile, row
    /// by row. On a map higher than it is wide, the fill (row `row` at `row * width`) misses the
    /// last tiles, which keep their value, and only the width x width square starts patches. On a
    /// wider map the game's fill reads rows past its row table; the slice fills the rows the table
    /// has. Each patch's flood (`FUN_141413340`) ends with its zone's tile count cached
    /// (`FUN_141413bb0`); the counts are not cleared, so a count cached before stays.
    fn label(&mut self, kinds: &[TerrainId]) {
        let (width, height) = (self.size.width(), self.size.height());
        for row in 0..width {
            for index in 0..height {
                if let Some(z) = self.zones.get_mut((row * width + index) as usize) {
                    *z = UNREACHED;
                }
            }
        }
        let mut next: i32 = 0;
        let mut wrapped = false;
        for y in 0..width {
            for x in 0..height {
                if !(y < height && x < width) {
                    continue;
                }
                let start = (y * width + x) as usize;
                let Some(cell) = self.zones.get_mut(start) else {
                    continue;
                };
                if *cell != UNREACHED {
                    continue;
                }
                if 0x1ff < next {
                    wrapped = true;
                }
                let zone = if next < i32::from(PATCHES) { next } else { 0 };
                *cell = ZoneId::new(zone as u16);
                let kind = self.open(kinds, start);
                self.flood(kinds, Tile::new(x, y), kind, ZoneId::new(zone as u16));
                next = zone + 1;
            }
        }
        self.count = if wrapped { i32::from(PATCHES) } else { next };
    }

    /// `FUN_141413340`: from tile `from` the zone goes to every tile not yet reached that is,
    /// like it, passable or not (`kind`), over the left, upper, right and lower neighbours. The
    /// game keeps a FIFO queue of pool nodes (`+0x850`), the slice a stack: both reach the same
    /// 4-connected tiles. Then the zone's tile count (`FUN_141413bb0`).
    fn flood(&mut self, kinds: &[TerrainId], from: Tile, kind: bool, zone: ZoneId) {
        let mut stack = vec![from];
        while let Some(current) = stack.pop() {
            for step in FLOOD_STEPS {
                let tile = current + step;
                let Some(index) = self.size.index(tile) else {
                    continue;
                };
                let open = self.open(kinds, index) == kind;
                if let Some(cell) = self.zones.get_mut(index)
                    && *cell == UNREACHED
                    && open
                {
                    *cell = zone;
                    stack.push(tile);
                }
            }
        }
        let _ = self.count_tiles(zone);
    }

    /// `FUN_141413bb0`: a zone's tile count, counted over the whole map and cached when the cache
    /// holds less than 1.
    fn count_tiles(&mut self, zone: ZoneId) -> i32 {
        let Some(cached) = self.tiles.get_mut(zone.index()) else {
            return 0;
        };
        if *cached < 1 {
            *cached = self.zones.iter().filter(|&&z| z == zone).count() as i32;
        }
        *cached
    }

    /// `FUN_141412a80`: which zones touch, and for two that do not, a zone that touches both.
    /// For each tile of zone `z` (not above the count) and each of its eight neighbours and
    /// itself that lies on the map, `touching[z * count + zone]` is set; the neighbour's row is
    /// tested against the width (the game's quirk; square maps agree), and a row below the map
    /// reads zone 0. Then, for each pair of zones that do not touch, the first zone touching
    /// both. Writes past the tables (a zone at the count) are the game's out-of-bounds stores;
    /// the slice skips them.
    fn connect(&mut self) {
        let (width, height) = (self.size.width(), self.size.height());
        let count = self.count.max(0);
        let size = (count * count) as usize;
        self.touching = vec![false; size];
        // the clear covers `size` bytes of the `2 * size`: whole entries below `size / 2`
        self.through = (0..size)
            .map(|index| (index < size / 2).then_some(ZoneId::OFF_MAP))
            .collect();
        for y in 0..height {
            for x in 0..width {
                let z = self
                    .zones
                    .get((y * width + x) as usize)
                    .map_or(0, |&z| i32::from(z.get()));
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let (nx, ny) = (x + dx, y + dy);
                        if !(0 <= nx && nx < width && 0 <= ny && ny < width && z <= count) {
                            continue;
                        }
                        let neighbour = if ny < height {
                            self.zones
                                .get((ny * width + nx) as usize)
                                .map_or(0, |&z| i32::from(z.get()))
                        } else {
                            0
                        };
                        if let Some(touch) = self.touching.get_mut((neighbour + z * count) as usize)
                        {
                            *touch = true;
                        }
                    }
                }
            }
        }
        let count = count as usize;
        let touches = |from: usize, to: usize| {
            self.touching
                .get(from * count + to)
                .copied()
                .unwrap_or(false)
        };
        for zone in 0..count {
            for index in 0..count {
                if touches(zone, index) {
                    continue;
                }
                if let Some(via) = (0..count).find(|&via| touches(zone, via) && touches(index, via))
                    && let Some(slot) = self.through.get_mut(zone * count + index)
                {
                    *slot = Some(ZoneId::new(via as u16));
                }
            }
        }
    }

    /// A tile's zone (`FUN_141413890`): 0 off the map.
    #[must_use]
    pub fn zone(&self, tile: Tile) -> ZoneId {
        self.size
            .index(tile)
            .and_then(|index| self.zones.get(index))
            .copied()
            .unwrap_or(ZoneId::OFF_MAP)
    }

    /// `FUN_141414c00`: the first tile of zone `zone` within `reach` of tile `at`, column by
    /// column (x), each from its least y: the box `reach` cut to whole tiles (`CVTTSS2SI`) spans
    /// about `at`, kept on the map, and a tile counts when the root of its squared steps from
    /// `at` (an integer sum as a float, `FUN_14099ef70`) is not above `reach` (`COMISS`/`JA`: a
    /// NaN reach takes every tile of the box).
    #[must_use]
    pub fn near(&self, zone: ZoneId, at: Tile, reach: Distance) -> Option<Tile> {
        use crate::math::conversions::cvttss2si;
        let (x, y) = (at.x(), at.y());
        let whole_reach = cvttss2si(reach.get());
        // CMOVNS and CMOVLE on the wrapped sums
        let (x0, y0) = (
            x.wrapping_sub(whole_reach).max(0),
            y.wrapping_sub(whole_reach).max(0),
        );
        let x1 = x.wrapping_add(whole_reach).min(self.size.width() - 1);
        let y1 = y.wrapping_add(whole_reach).min(self.size.height() - 1);
        for tx in x0..=x1 {
            let dx = x.wrapping_sub(tx);
            for ty in y0..=y1 {
                let dy = y.wrapping_sub(ty);
                let steps = dy.wrapping_mul(dy).wrapping_add(dx.wrapping_mul(dx));
                let distance = sqrt(steps as f32);
                let tile = Tile::new(tx, ty);
                if at_most_or_unordered(distance, reach.get()) && self.zone(tile) == zone {
                    return Some(tile);
                }
            }
        }
        None
    }

    /// The zones' number (`+0x838`).
    #[must_use]
    pub const fn count(&self) -> i32 {
        self.count
    }

    /// A zone's cached tile count (`+0x000`); 0 past the cached zones.
    #[must_use]
    pub fn tiles(&self, zone: ZoneId) -> i32 {
        self.tiles.get(zone.index()).copied().unwrap_or(0)
    }

    /// Whether two zones touch (`FUN_141414f70`): no when either is above the count, else the
    /// table at `from * count + to`. `None` where that is past the table, which the game reads
    /// beyond: a zone at the count, or a count a relabel raised above the one the table was
    /// built for.
    #[must_use]
    pub fn adjacent(&self, first: ZoneId, second: ZoneId) -> Option<bool> {
        let (from, to) = (i32::from(first.get()), i32::from(second.get()));
        if self.count < from || self.count < to {
            return Some(false);
        }
        self.touching
            .get((self.count * from + to) as usize)
            .copied()
    }

    /// The zone between two that do not touch (`+0x848` at `from * count + to`, no reader in build
    /// 158041 but its builder): `None` where the game's table holds memory it never wrote, and
    /// past the table.
    #[must_use]
    pub fn via(&self, from: ZoneId, to: ZoneId) -> Option<ZoneId> {
        let at = self.count * i32::from(from.get()) + i32::from(to.get());
        self.through.get(at as usize).copied().flatten()
    }
}

/// The map's zone maps, one per passability pattern, made when first asked for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Zones {
    maps: Vec<ZoneMap>,
}

/// Whether the zone maps' lookup ([`Zones::find`]) labels a found map again from the terrain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Relabel {
    /// It labels it again (`FUN_141413190`).
    Again,
    /// It keeps its labels.
    Kept,
}

impl Zones {
    /// The zone maps a save holds, in its order.
    #[must_use]
    pub const fn from_saved(maps: Vec<ZoneMap>) -> Self {
        Self { maps }
    }

    /// The zone maps, in the order they were made.
    #[must_use]
    pub fn maps(&self) -> &[ZoneMap] {
        &self.maps
    }

    /// The index of the zone map of a restriction whose value per terrain is `values`
    /// (`FUN_1414138d0`): the first map whose own values are no more than the query's and agree
    /// with the query's first ones in their signs (`0 < value`; a map of no values matches any
    /// query), labelled again from the terrain `kinds` with [`Relabel::Again`] (`FUN_141413190`,
    /// only the random map's terrain generator asks); else a new map of the terrain `kinds`,
    /// appended.
    pub fn find(
        &mut self,
        size: GridSize,
        kinds: &[TerrainId],
        values: &Restriction,
        relabel: Relabel,
    ) -> usize {
        let query = values.values();
        let found = self.maps.iter().position(|zone_map| {
            zone_map.passable.len() <= query.len()
                && zone_map
                    .passable
                    .iter()
                    .zip(query)
                    .all(|(&passable, &value)| passable == (0.0 < value))
        });
        if let Some(index) = found {
            if relabel == Relabel::Again
                && let Some(map) = self.maps.get_mut(index)
            {
                map.label(kinds);
            }
            return index;
        }
        self.maps.push(ZoneMap::new(size, kinds, values));
        self.maps.len() - 1
    }

    /// The zone map at `index`.
    ///
    /// # Errors
    ///
    /// An index no zone map has.
    pub fn map(&self, index: usize) -> GenieResult<&ZoneMap> {
        self.maps.checked(index)
    }

    /// The zone map of a restriction whose value per terrain is `values`, as a unit asks for it
    /// (`FUN_1411931c0`, `FUN_1414138d0` without the relabel).
    ///
    /// # Errors
    ///
    /// A zone map the table lost between its search and its read.
    pub fn of(
        &mut self,
        size: GridSize,
        kinds: &[TerrainId],
        values: &Restriction,
    ) -> GenieResult<&ZoneMap> {
        let index = self.find(size, kinds, values, Relabel::Kept);
        self.maps.checked(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Terrains of ids `kinds`, row by row.
    fn ids(kinds: &[u8]) -> Vec<TerrainId> {
        kinds.iter().copied().map(TerrainId::named).collect()
    }

    /// The 3 x 3 map with a wall of terrain 1 (impassable) between two passable patches.
    #[rustfmt::skip]
    const WALL: [u8; 9] = [
        0, 1, 0,
        0, 1, 0,
        0, 1, 1,
    ];

    /// A zone's byte is its number's low byte, which the byte compares read.
    #[test]
    fn zone_byte_is_its_low_byte() {
        let z = ZoneId::new(0x1ff);
        assert_eq!((z.get(), z.index(), z.byte()), (0x1ff, 0x1ff, 0xff));
        assert_eq!(UNREACHED.byte(), 0);
    }

    #[test]
    fn patches_are_numbered_by_their_first_tile_row_by_row() {
        let z = ZoneMap::new(
            GridSize::new(3, 3),
            &ids(&WALL),
            &Restriction::new(vec![1.0, 0.0]),
        );
        assert_eq!(
            [
                z.zone(Tile::new(0, 0)),
                z.zone(Tile::new(1, 0)),
                z.zone(Tile::new(2, 0))
            ]
            .map(ZoneId::get),
            [0, 1, 2]
        );
        assert_eq!(
            [
                z.zone(Tile::new(0, 2)),
                z.zone(Tile::new(1, 2)),
                z.zone(Tile::new(2, 2))
            ]
            .map(ZoneId::get),
            [0, 1, 1]
        );
        assert_eq!(z.zone(Tile::new(2, 1)), ZoneId::new(2));
        assert_eq!(z.zone(Tile::new(-1, 0)), ZoneId::OFF_MAP);
        assert_eq!(z.count(), 3);
    }

    #[test]
    fn zone_near_tile_is_searched_column_by_column_within_the_reach() {
        // a 5 x 5 map: terrain 1 (impassable, zone 1) in column 4 and at (3, 0); the rest zone 0
        #[rustfmt::skip]
        let kinds = [
            0, 0, 0, 1, 1,
            0, 0, 0, 0, 1,
            0, 0, 0, 0, 1,
            0, 0, 0, 0, 1,
            0, 0, 0, 0, 1,
        ];
        let z = ZoneMap::new(
            GridSize::new(5, 5),
            &ids(&kinds),
            &Restriction::new(vec![1.0, 0.0]),
        );
        assert_eq!(
            [
                z.zone(Tile::new(3, 0)),
                z.zone(Tile::new(4, 2)),
                z.zone(Tile::new(2, 2))
            ]
            .map(ZoneId::get),
            [1, 1, 0]
        );
        // from (2, 2) within 2: column 3 comes before column 4, and (3, 0) is sqrt 5 away;
        // column 4's (4, 2) is 2 away
        assert_eq!(
            z.near(ZoneId::new(1), Tile::new(2, 2), Distance::new(2.0)),
            Some(Tile::new(4, 2))
        );
        // within 2.5 (cut to 2 tiles) (3, 0) counts and its column comes first
        assert_eq!(
            z.near(ZoneId::new(1), Tile::new(2, 2), Distance::new(2.5)),
            Some(Tile::new(3, 0))
        );
        // the box stays on the map: from (0, 0) within 1 no zone-1 tile
        assert_eq!(
            z.near(ZoneId::new(1), Tile::new(0, 0), Distance::new(1.0)),
            None
        );
        // the unit's own tile counts, and x before y: (0, 0) itself
        assert_eq!(
            z.near(ZoneId::OFF_MAP, Tile::new(0, 0), Distance::new(1.0)),
            Some(Tile::new(0, 0))
        );
        // a NaN reach is 0 tiles (CVTTSS2SI's i32::MIN wraps the box away) and takes no distance
        // test
        assert_eq!(
            z.near(ZoneId::OFF_MAP, Tile::new(2, 2), Distance::new(f32::NAN)),
            None
        );
    }

    #[test]
    fn the_flood_takes_four_neighbours_only() {
        // tiles touching at a corner only are patches of their own, passable or not
        let kinds = [0, 1, 1, 0];
        let z = ZoneMap::new(
            GridSize::new(2, 2),
            &ids(&kinds),
            &Restriction::new(vec![1.0, 0.0]),
        );
        assert_eq!(
            [z.zone(Tile::new(0, 0)), z.zone(Tile::new(1, 0))].map(ZoneId::get),
            [0, 1]
        );
        assert_eq!(
            [z.zone(Tile::new(0, 1)), z.zone(Tile::new(1, 1))].map(ZoneId::get),
            [2, 3]
        );
    }

    /// A square map of `side` x `side` tiles alternating passable and not: every tile is a patch.
    fn checkerboard(side: i32) -> Vec<u8> {
        (0..side * side)
            .map(|tile| ((tile % side + tile / side) % 2) as u8)
            .collect()
    }

    #[test]
    fn numbers_start_again_after_511_and_the_count_stays_512() {
        // 23 x 23 = 529 patches: the 513th (tile 512, at 6, 22) is 0 again, the next 1
        let z = ZoneMap::new(
            GridSize::new(23, 23),
            &ids(&checkerboard(23)),
            &Restriction::new(vec![1.0, 0.0]),
        );
        assert_eq!(
            [
                z.zone(Tile::new(5, 22)),
                z.zone(Tile::new(6, 22)),
                z.zone(Tile::new(7, 22))
            ]
            .map(ZoneId::get),
            [511, 0, 1]
        );
        assert_eq!(z.count(), 512);
    }

    #[test]
    fn each_flood_caches_its_zone_tile_count_once() {
        let z = ZoneMap::new(
            GridSize::new(3, 3),
            &ids(&WALL),
            &Restriction::new(vec![1.0, 0.0]),
        );
        assert_eq!(z.tiles(ZoneId::OFF_MAP), 3);
        assert_eq!(z.tiles(ZoneId::new(1)), 4);
        assert_eq!(z.tiles(ZoneId::new(2)), 2);
        assert_eq!(z.tiles(ZoneId::new(3)), 0);
        // past 511 a zone's second patch finds its count cached by the first: zone 0's tiles
        // are two (tiles 0 and 512), its count stays the first patch's 1
        let z = ZoneMap::new(
            GridSize::new(23, 23),
            &ids(&checkerboard(23)),
            &Restriction::new(vec![1.0, 0.0]),
        );
        assert_eq!(z.tiles(ZoneId::OFF_MAP), 1);
    }

    #[test]
    fn map_wider_than_it_is_high_labels_the_square_of_its_height() {
        // 2 wide, 3 high: the game fills and scans rows over the width and columns over the
        // height, so tile (1, 2) keeps the 0 it was allocated with, and (0, 2), impassable beside
        // the passable patch above, is filled but never starts a patch: it keeps 0x200
        let kinds = [0, 0, 0, 0, 1, 0];
        let z = ZoneMap::new(
            GridSize::new(2, 3),
            &ids(&kinds),
            &Restriction::new(vec![1.0, 0.0]),
        );
        assert_eq!(
            [
                z.zone(Tile::new(0, 0)),
                z.zone(Tile::new(1, 0)),
                z.zone(Tile::new(0, 1)),
                z.zone(Tile::new(1, 1))
            ]
            .map(ZoneId::get),
            [0; 4]
        );
        assert_eq!(
            [z.zone(Tile::new(0, 2)), z.zone(Tile::new(1, 2))].map(ZoneId::get),
            [PATCHES, 0]
        );
        assert_eq!(z.count(), 1);
        // the game's 3 wide, 2 high case writes past its row table; the slice labels the rest
        let z = ZoneMap::new(
            GridSize::new(3, 2),
            &ids(&[0; 6]),
            &Restriction::new(vec![1.0]),
        );
        assert_eq!(z.zone(Tile::new(0, 0)), ZoneId::OFF_MAP);
    }

    #[test]
    fn zones_touching_at_tile_or_corner_are_adjacent() {
        let z = ZoneMap::new(
            GridSize::new(3, 3),
            &ids(&WALL),
            &Restriction::new(vec![1.0, 0.0]),
        );
        assert_eq!(z.adjacent(ZoneId::new(0), ZoneId::new(1)), Some(true));
        assert_eq!(z.adjacent(ZoneId::new(1), ZoneId::new(2)), Some(true));
        assert_eq!(z.adjacent(ZoneId::new(0), ZoneId::new(0)), Some(true));
        assert_eq!(z.adjacent(ZoneId::new(0), ZoneId::new(2)), Some(false));
        assert_eq!(z.adjacent(ZoneId::new(2), ZoneId::new(0)), Some(false));
        // FUN_141414f70: past the count it answers no; at the count it reads past the table
        assert_eq!(z.adjacent(ZoneId::new(4), ZoneId::new(0)), Some(false));
        assert_eq!(z.adjacent(ZoneId::new(3), ZoneId::new(0)), None);
        let z = ZoneMap::new(
            GridSize::new(2, 2),
            &ids(&[0, 1, 1, 0]),
            &Restriction::new(vec![1.0, 0.0]),
        );
        assert_eq!(z.adjacent(ZoneId::new(0), ZoneId::new(3)), Some(true));
        assert_eq!(z.adjacent(ZoneId::new(1), ZoneId::new(2)), Some(true));
    }

    #[test]
    fn two_zones_apart_go_through_the_first_zone_touching_both() {
        let z = ZoneMap::new(
            GridSize::new(3, 3),
            &ids(&WALL),
            &Restriction::new(vec![1.0, 0.0]),
        );
        assert_eq!(z.via(ZoneId::new(0), ZoneId::new(2)), Some(ZoneId::new(1)));
        assert_eq!(z.via(ZoneId::new(2), ZoneId::new(0)), Some(ZoneId::new(1)));
        // the table's first n * n bytes start 0 (entries 0 to 3 and the low byte of 4), the
        // rest is not initialised: adjacent pairs keep that
        assert_eq!(z.via(ZoneId::new(0), ZoneId::new(0)), Some(ZoneId::new(0)));
        assert_eq!(z.via(ZoneId::new(1), ZoneId::new(1)), None);
        assert_eq!(z.via(ZoneId::new(2), ZoneId::new(2)), None);
    }

    #[test]
    fn saved_map_keeps_its_counts_and_builds_its_adjacency() {
        let made = ZoneMap::new(
            GridSize::new(3, 3),
            &ids(&WALL),
            &Restriction::new(vec![1.0, 0.0]),
        );
        let mut tiles = vec![0u32; 512];
        tiles[..3].copy_from_slice(&[3, 4, 2]);
        let zones: Vec<u16> = (0..9)
            .map(|tile| made.zone(Tile::new(tile % 3, tile / 3)).get())
            .collect();
        let saved = ZoneMap::from_saved(
            GridSize::new(3, 3),
            &tiles,
            &zones,
            &Restriction::new(vec![1.0, 0.0]),
            3,
        );
        assert_eq!(saved, made);
    }

    #[test]
    fn restrictions_of_one_pattern_share_their_map() {
        let mut zones = Zones::default();
        let kinds = [0, 1];
        let _ = zones
            .of(
                GridSize::new(2, 1),
                &ids(&kinds),
                &Restriction::new(vec![1.0, 0.0]),
            )
            .unwrap();
        let _ = zones
            .of(
                GridSize::new(2, 1),
                &ids(&kinds),
                &Restriction::new(vec![0.5, -1.0]),
            )
            .unwrap();
        let _ = zones
            .of(
                GridSize::new(2, 1),
                &ids(&kinds),
                &Restriction::new(vec![0.0, 1.0]),
            )
            .unwrap();
        assert_eq!(zones.maps.len(), 2);
    }

    #[test]
    fn map_matches_query_whose_values_begin_with_its_pattern() {
        let mut zones = Zones::default();
        let kinds = [0, 1, 1, 0];
        assert_eq!(
            zones.find(
                GridSize::new(2, 2),
                &ids(&kinds),
                &Restriction::new(vec![1.0, 0.0]),
                Relabel::Kept
            ),
            0
        );
        // the map's two values are a prefix of the query's three
        assert_eq!(
            zones.find(
                GridSize::new(2, 2),
                &ids(&kinds),
                &Restriction::new(vec![1.0, 0.0, 1.0]),
                Relabel::Kept
            ),
            0
        );
        // a query shorter than the map's values does not match it
        assert_eq!(
            zones.find(
                GridSize::new(2, 2),
                &ids(&kinds),
                &Restriction::new(vec![1.0]),
                Relabel::Kept
            ),
            1
        );
        // a map of no values matches every query
        let mut empty = Zones::default();
        assert_eq!(
            empty.find(
                GridSize::new(2, 2),
                &ids(&kinds),
                &Restriction::new(vec![]),
                Relabel::Kept
            ),
            0
        );
        assert_eq!(
            empty.find(
                GridSize::new(2, 2),
                &ids(&kinds),
                &Restriction::new(vec![0.0, 1.0]),
                Relabel::Kept
            ),
            0
        );
    }

    #[test]
    fn relabel_numbers_the_terrain_again_and_keeps_the_rest() {
        let mut zones = Zones::default();
        let zone = zones.find(
            GridSize::new(3, 3),
            &ids(&WALL),
            &Restriction::new(vec![1.0, 0.0]),
            Relabel::Kept,
        );
        let before = zones.map(zone).unwrap().clone();
        // the wall is gone: one patch, the count 1; the tile counts and the adjacency stay
        let zone = zones.find(
            GridSize::new(3, 3),
            &ids(&[0; 9]),
            &Restriction::new(vec![1.0, 0.0]),
            Relabel::Again,
        );
        let after = zones.map(zone).unwrap();
        assert_eq!(after.zone(Tile::new(2, 2)), ZoneId::OFF_MAP);
        assert_eq!(after.count(), 1);
        assert_eq!(after.tiles(ZoneId::new(1)), before.tiles(ZoneId::new(1)));
        assert_eq!(after.adjacent(ZoneId::new(0), ZoneId::new(0)), Some(true));
        // the 3 x 3 table stays: at the new count 1, (0, 1) reads its entry 1, (0, 1) of before
        assert_eq!(after.adjacent(ZoneId::new(0), ZoneId::new(1)), Some(true));
        // without the relabel flag the map is not numbered again
        let zone = zones.find(
            GridSize::new(3, 3),
            &ids(&WALL),
            &Restriction::new(vec![1.0, 0.0]),
            Relabel::Kept,
        );
        assert_eq!(
            zones.map(zone).unwrap().zone(Tile::new(1, 0)),
            ZoneId::OFF_MAP
        );
    }
}
