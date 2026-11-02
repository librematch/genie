//! The map's terrain per tile (`RGE_Map`'s tiles, 0x40 bytes each: `+0x9` the terrain, `+0xa` the
//! terrain to restore, `+0xc` the layer) and what the `.dat` says of each terrain, as far as the
//! slice reads them: a completed building's snow check and its foundation terrain
//! (`FUN_1411dbb70`, `FUN_1411ddd90`).
//!
//! The obstruction manager keeps its own copy of the terrain classes ([`crate::map::spatial::Tiles`]),
//! which a terrain change does not update; the map's render caches (`FUN_140f741b0`'s blends and
//! minimap) are not held.

use crate::GenieError;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::Position;
use crate::map::Restriction;
use crate::map::TerrainFlags;
use crate::map::TerrainId;
use crate::map::TerrainMask;
use crate::map::Tile;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::objects::masters::class;
use crate::objects::masters::ids;
use crate::objects::masters::ids::TOWN_CENTRE;
use crate::world::slots::CheckedIndex;

/// A foundation's tile off the map after the clamp to it.
const TILE_OFF_MAP: GenieError =
    GenieError::unreachable("a foundation tile off the map after the clamp");

/// What the `.dat` says of a terrain (its record at map `+0x6238`, 0x1f0 bytes each).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerrainKind {
    /// The record's first byte (`enabled`) or its `terrain_to_draw` (`+0xf8`) of 0 or more: the
    /// map's terrain reads (`FUN_140f690e0`, `FUN_140f69000`) see the terrain, else terrain 0.
    pub shown: bool,
    /// The `.dat`'s `is_water` byte, which the map's load copies to the terrain table
    /// (`FUN_1413fc340`, `DAT_143cc7b40 + 0x2c`).
    pub flags: TerrainFlags,
    /// A forest (`FUN_140f5efb0`): the densities of its terrain units of class 15 (Gaia's
    /// masters) sum to more than 249.
    pub forest: bool,
}

/// The terrain a completed building leaves on snow (`FUN_1411ddd90`): its foundation 27 becomes 6
/// with the layer 32, and it skips tiles of 36.
const FOUNDATION: TerrainId = TerrainId::named(27);
const SNOW_FOUNDATION: TerrainId = TerrainId::named(6);
const SNOW_LAYER: TerrainId = TerrainId::named(32);
const SNOW_SKIPPED: TerrainId = TerrainId::named(36);

/// The class whose foundation terrain always goes into the terrain (`FUN_1411ddd90`).
const CLASS_14: ClassId = class::CLASS_14;
/// Terrains a foundation never covers (`FUN_1411ddd90`: 47 and 69).
const UNCOVERED: [TerrainId; 2] = [TerrainId::named(47), TerrainId::named(69)];

/// The terrains foundation 27 covers, and the layers it covers them under (the bits of
/// `0x500001209`; the layer test lists their complement, `0x2ffffedf6` and above 34).
const fn plain(t: TerrainId) -> bool {
    matches!(t.get(), 0 | 3 | 9 | 12 | 32 | 34)
}

/// `FUN_1413fc100` and `FUN_1413fc220`: mangrove shallows or forest (54, 55) or a rice farm's
/// terrain (63 to 67), where a farm is a rice farm.
#[must_use]
pub const fn is_rice_terrain(t: TerrainId) -> bool {
    matches!(t.get(), 54 | 55 | 63..=67)
}

/// `FUN_1413fc120`: the terrain table entry's first byte, the terrain's id, is a farm's: 7, 8,
/// 29 to 31, 63 to 67 and 117 to 121.
#[must_use]
pub const fn is_farm_terrain(t: TerrainId) -> bool {
    matches!(t.get(), 7 | 8 | 29..=31 | 63..=67 | 117..=121)
}

/// A tile's terrain as the map holds it (`+0x9`, `+0xa`, `+0xc`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileTerrain {
    /// Its terrain (`+0x9`).
    pub kind: TerrainId,
    /// The terrain to restore (`+0xa`), none for the byte 255.
    pub restore: Option<TerrainId>,
    /// Its layer (`+0xc`), none for the short −1.
    pub layer: Option<TerrainId>,
}

impl TileTerrain {
    /// A tile of the save's three fields: the terrain and the terrain to restore as bytes (255
    /// none), the layer as a short (below 0 none).
    #[must_use]
    pub const fn of_save(kind: u8, restore: u8, layer: i16) -> Self {
        Self {
            kind: TerrainId::named(kind),
            restore: if restore == NO_RESTORE {
                None
            } else {
                Some(TerrainId::named(restore))
            },
            layer: TerrainId::of_data(layer),
        }
    }
}

/// The byte of a tile's terrain to restore (`+0xa`) that holds none.
const NO_RESTORE: u8 = 255;
/// The terrain a layer of none (the short −1) reads as: its low byte.
const LOW_BYTE_OF_NONE: TerrainId = TerrainId::named(0xff);

/// The map's terrain and the `.dat`'s terrain records.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Terrain {
    size: GridSize,
    /// Per tile, row by row: the terrain (`+0x9`).
    pub kinds: Vec<TerrainId>,
    /// Per tile: the terrain to restore (`+0xa`, the byte 255 for none).
    pub restore: Vec<Option<TerrainId>>,
    /// Per tile: the layer (`+0xc`, the short −1 for none).
    pub layers: Vec<Option<TerrainId>>,
    /// Per terrain id, the `.dat`'s records.
    pub terrains: Vec<TerrainKind>,
    /// Each terrain restriction's value per terrain (`world+0x788`, the `.dat`'s
    /// `passable_buildable_dmg_multiplier`).
    pub restrictions: Vec<Restriction>,
    /// Per tile, its elevation (`+0x14`, the save's `t->m_elevation`); empty when not known.
    pub elevations: Vec<u8>,
    /// Per tile, its shape (`+0x8`, not saved): what the map's load (`FUN_140f733f0` over the
    /// whole map) makes of the elevations about it ([`Terrain::set_elevations`]).
    pub shapes: Vec<u8>,
}

/// A tile's side (`DAT_14330eb8c`), which the slope shares measure against.
const TILE: f32 = crate::math::image::terrain::TILE;

/// `FUN_140f69470` (HD's `RGE_Map::get_tile_type`): the shape of the tile of elevation `e` from
/// its neighbours' (`n`orth is `y − 1`, `e`ast `x + 1`; a neighbour off the map counts as the
/// tile's own): a side or corner one step up decides it, in the routine's order; 0 is flat.
#[allow(clippy::many_single_char_names)]
fn tile_shape(e: u8, [n, e_, w, s]: [u8; 4], [ne, sw, nw, se]: [u8; 4]) -> u8 {
    let (up, down) = (i32::from(e) + 1, i32::from(e) - 1);
    let is = |v: u8, w: i32| i32::from(v) == w;
    if is(n, up) && is(e_, up) {
        14
    } else if is(w, up) && is(s, up) {
        13
    } else if is(n, up) && is(w, up) {
        16
    } else if is(e_, up) && is(s, up) {
        15
    } else if is(n, up) {
        6
    } else if is(e_, up) {
        8
    } else if is(w, up) {
        5
    } else if is(s, up) {
        7
    } else if is(ne, up) {
        if is(w, down) && is(s, down) { 2 } else { 10 }
    } else if is(sw, up) {
        if is(n, down) && is(e_, down) { 1 } else { 9 }
    } else if is(nw, up) {
        if is(e_, down) && is(s, down) { 3 } else { 11 }
    } else if is(se, up) {
        if is(n, down) && is(w, down) { 4 } else { 12 }
    } else {
        0
    }
}

/// The static `new_position`'s slope share (`0x1411a58b0`, the switch on the tile's shape): how
/// far up the tile's slope the point `fx`, `fy` (its place within the tile) lies. The arms follow
/// the switch's cases one by one.
#[allow(clippy::match_same_arms)]
fn slope_share(shape: u8, fx: f32, fy: f32) -> f32 {
    let sum = fy + fx;
    match shape {
        1 if TILE <= sum => TILE - fx,
        1 | 7 => fy,
        2 if TILE <= sum => TILE - fy,
        2 | 8 => fx,
        3 if fx <= fy => TILE - fy,
        3 => TILE - fx,
        4 if fx <= fy => fx,
        4 => fy,
        5 => TILE - fx,
        6 => TILE - fy,
        9 if fx - fy < 0.0 => -(fx - fy),
        10 if 0.0 < fx - fy => fx - fy,
        11 if sum < TILE => TILE - sum,
        12 if TILE < sum => sum - TILE,
        13 if sum < TILE => TILE - fx,
        13 => fy,
        14 if sum < TILE => TILE - fy,
        14 => fx,
        15 if fx <= fy => fy,
        15 => fx,
        16 if fx <= fy => TILE - fx,
        16 => TILE - fy,
        _ => 0.0,
    }
}

/// What a completed building's foundation reads of it (`FUN_1411ddd90`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Foundation {
    /// The building's place.
    pub position: Position,
    /// Its master's `clearance_size` (`+0x5fc`).
    pub clearance: Footprint,
    /// The foundation terrain, the master's `+0x988` byte.
    pub terrain: TerrainId,
    /// The building's `snow_flag` (`+0xc61`).
    pub snow: crate::map::Snow,
    /// The master's id and class, its `defense_terrain_bonus` (`+0x820`).
    pub master: MasterId,
    /// The master's class.
    pub class: ClassId,
    /// The master's `defense_terrain_bonus` (`+0x820`).
    pub terrain_bonus: crate::objects::masters::RestrictionField,
    /// The game's setting `FUN_140ec0cd0` reads at `+0x680` ([`crate::world::World::mode_680`]):
    /// at 1 a foundation without a snow layer goes into the layer, not the terrain.
    pub mode_680: crate::world::Mode680,
}

/// The class bit of a terrain's flags (the `.dat`'s `is_water` byte) in the terrain
/// restrictions' masks (the load's table `FUN_141054630` reads): none for no flag, 1 for water
/// ([`TerrainFlags::ANY_WATER`]), 2 for the shore ([`TerrainFlags::SHORE`]), else land
/// ([`TerrainFlags::LAND`]) as 4.
#[must_use]
pub const fn terrain_class(flags: TerrainFlags) -> TerrainMask {
    TerrainMask::of_bits(if flags.is_empty() {
        0
    } else if flags.intersects(TerrainFlags::ANY_WATER) {
        1
    } else if flags.intersects(TerrainFlags::SHORE) {
        2
    } else {
        (flags.bits() >> 3) & 4
    })
}

impl Terrain {
    /// Restriction `restriction`'s values, one a terrain; none for a restriction the table lacks.
    #[must_use]
    pub fn restriction(&self, restriction: crate::map::RestrictionId) -> Option<&Restriction> {
        self.restrictions.get(restriction.index())
    }

    /// The terrain classes restriction `restriction` lets an object stand on
    /// (`FUN_141054630`'s table): the class bits ([`terrain_class`]) of the terrains whose value
    /// in it is above 0; none for a restriction the table lacks.
    #[must_use]
    pub fn restriction_mask(&self, restriction: crate::map::RestrictionId) -> TerrainMask {
        self.restrictions
            .get(restriction.index())
            .map_or(TerrainMask::NONE, |values| {
                values
                    .values()
                    .iter()
                    .enumerate()
                    .filter(|&(_, &v)| 0.0 < v)
                    .fold(TerrainMask::NONE, |m, (t, _)| {
                        m.with(
                            self.terrains
                                .get(t)
                                .map_or(TerrainMask::NONE, |k| terrain_class(k.flags)),
                        )
                    })
            })
    }

    /// The map's terrain from its tiles, the terrain table and the restrictions.
    #[must_use]
    pub fn new(
        size: GridSize,
        tiles: Vec<TileTerrain>,
        terrains: Vec<TerrainKind>,
        restrictions: Vec<Restriction>,
    ) -> Self {
        Self {
            size,
            kinds: tiles.iter().map(|t| t.kind).collect(),
            restore: tiles.iter().map(|t| t.restore).collect(),
            layers: tiles.iter().map(|t| t.layer).collect(),
            terrains,
            restrictions,
            elevations: Vec::new(),
            shapes: Vec::new(),
        }
    }

    /// The tiles' elevations, row by row, and the shapes the map's load makes of them
    /// (`FUN_140f733f0`: [`tile_shape`] on every tile).
    pub fn set_elevations(&mut self, elevations: Vec<u8>) {
        self.elevations = elevations;
        let at = |x: i32, y: i32, own: u8| {
            self.size
                .index(Tile::new(x, y))
                .and_then(|i| self.elevations.get(i).copied())
                .unwrap_or(own)
        };
        let mut shapes = Vec::with_capacity(self.elevations.len());
        for y in 0..self.size.height() {
            for x in 0..self.size.width() {
                let Some(e) = self
                    .size
                    .index(Tile::new(x, y))
                    .and_then(|i| self.elevations.get(i).copied())
                else {
                    shapes.push(0);
                    continue;
                };
                let (last_column, last_row) = (self.size.width() - 1, self.size.height() - 1);
                // FUN_140f69470's reads: a neighbour past the map's edge is the tile itself
                let n = if 0 < y { at(x, y - 1, e) } else { e };
                let east = if x < last_column { at(x + 1, y, e) } else { e };
                let west = if 0 < x { at(x - 1, y, e) } else { e };
                let s = if y < last_row { at(x, y + 1, e) } else { e };
                let ne = if 0 < y && x < last_column {
                    at(x + 1, y - 1, e)
                } else {
                    e
                };
                let sw = if 0 < x && y < last_row {
                    at(x - 1, y + 1, e)
                } else {
                    e
                };
                let nw = if 0 < y && 0 < x {
                    at(x - 1, y - 1, e)
                } else {
                    e
                };
                let se = if y < last_row && x < last_column {
                    at(x + 1, y + 1, e)
                } else {
                    e
                };
                shapes.push(tile_shape(e, [n, east, west, s], [ne, sw, nw, se]));
            }
        }
        self.shapes = shapes;
    }

    /// The map's size in tiles.
    #[must_use]
    pub const fn size(&self) -> GridSize {
        self.size
    }

    /// A tile's terrain (the map tile's `+0x9`), None off the map.
    #[must_use]
    pub fn kind(&self, tile: Tile) -> Option<TerrainId> {
        self.size
            .index(tile)
            .and_then(|i| self.kinds.get(i).copied())
    }

    /// The ground's height under (`x`, `y`) on the map (the static `new_position`'s,
    /// `0x1411a58b0`, which slot 0x60 takes): the tile's elevation plus its slope's share by the
    /// tile's shape ([`slope_share`] of the place within the tile).
    pub fn ground(&self, at: Point) -> Result<f32, GenieError> {
        let (x, y) = (at.x(), at.y());
        let (tx, ty) = (x as i32, y as i32);
        let i = self
            .size
            .index(Tile::new(tx, ty))
            .filter(|&i| i < self.elevations.len() && i < self.shapes.len())
            .ok_or(GenieError::unsupported(
                "the ground's height where the elevations are not known",
            ))?;
        let (fx, fy) = (x - tx as f32, y - ty as f32);
        Ok(f32::from(*self.elevations.checked(i)?) + slope_share(*self.shapes.checked(i)?, fx, fy))
    }

    /// The terrain defense of an object on `tile` whose master has `defense_terrain_bonus`
    /// `bonus` (`FUN_141082530`): the restriction's value (`world+0x788`) for the tile's terrain
    /// to restore, else its terrain (the tile's `+0xa`, else `+0x9`); 0 counts as 1, and an
    /// object off the tiles has 1. None for a restriction or terrain the data has not got.
    #[must_use]
    pub fn defense(
        &self,
        bonus: crate::objects::masters::RestrictionField,
        tile: Tile,
    ) -> Option<f32> {
        let Some(i) = self.size.index(tile) else {
            return Some(1.0);
        };
        let t = match *self.restore.get(i)? {
            None => *self.kinds.get(i)?,
            Some(t) => t,
        };
        let v = self.restriction(bonus.restriction()?)?.value(t)?;
        Some(if v == 0.0 { 1.0 } else { v })
    }

    /// The terrain a map read sees of `t` (`FUN_140f690e0`): `t` when its record exists and it is
    /// shown, else 0.
    fn seen(&self, t: TerrainId) -> TerrainId {
        match self.terrains.get(t.index()) {
            Some(k) if k.shown => t,
            _ => TerrainId::named(0),
        }
    }

    /// `FUN_1413fc280`: the terrain table's snow bit; a terrain beyond the records has none.
    fn snowy(&self, t: TerrainId) -> bool {
        self.terrains
            .get(t.index())
            .is_some_and(|k| k.flags.contains(TerrainFlags::SNOW))
    }

    /// `FUN_1413fbee0` (`FUN_1413fbf30` on a place's tile) with `FUN_1413fc0f0`: the terrain a map
    /// read sees on `tile` (terrain 0 off the map) is a beach ([`TerrainFlags::BEACH`]), where
    /// land units and ships meet.
    #[must_use]
    pub fn beach(&self, tile: Tile) -> bool {
        let t = self
            .size
            .index(tile)
            .and_then(|i| self.kinds.get(i))
            .map_or(TerrainId::named(0), |&k| self.seen(k));
        self.terrains
            .get(t.index())
            .is_some_and(|k| k.flags.contains(TerrainFlags::BEACH))
    }

    /// `FUN_1413fbee0`: the terrain table's flags (the `.dat`'s `is_water` byte) of the terrain a
    /// map read sees on `tile` (`FUN_140f690e0` on shorts, [`Tile::short`]: terrain 0 off the
    /// map, or for a terrain beyond the records or hidden); none for a terrain the records lack.
    /// [`TerrainFlags::ANY_WATER`] is water (`FUN_1413fc2b0`), [`TerrainFlags::SHORE`] the
    /// shore (`FUN_1413fc270`).
    #[must_use]
    pub fn flags_at(&self, tile: Tile) -> TerrainFlags {
        let t = self.read(tile);
        self.terrains
            .get(t.index())
            .map_or(TerrainFlags::NONE, |k| k.flags)
    }

    /// The terrain the general task search reads at a point (`FUN_1411a8ba0`): none for x or y
    /// exactly −1 (`UCOMISS`: a NaN is not), else the tile of each coordinate's `CVTTSS2SI`
    /// truncated to 16 bits; none for a tile off the map, else the terrain a map read sees there
    /// ([`Terrain::read`]).
    #[must_use]
    pub fn at_task_point(&self, at: Point) -> Option<TerrainId> {
        let (x, y) = (at.x(), at.y());
        #[allow(clippy::float_cmp)]
        if x == -1.0 || y == -1.0 {
            return None;
        }
        let short = |v: f32| i32::from(crate::math::conversions::cvttss2si(v) as i16);
        let tile = Tile::new(short(x), short(y));
        self.size.holds(tile).then(|| self.read(tile))
    }

    /// `FUN_140f690e0`: the terrain a map read sees on `tile` (its x and y shorts,
    /// [`Tile::short`]): terrain 0 off the map, or for a terrain beyond the records or hidden.
    #[must_use]
    pub fn read(&self, tile: Tile) -> TerrainId {
        self.size
            .index(tile)
            .and_then(|i| self.kinds.get(i))
            .map_or(TerrainId::named(0), |&k| self.seen(k))
    }

    /// The snow check of a completed building on `tile` (`FUN_1411dbb70`'s "pick snow"):
    /// the tile's terrain (`FUN_1413fbee0`) or its layer read as a terrain (`FUN_1413fbf90`, the
    /// layer's low byte) is snow. Its decal (`FUN_1413b7bf0`, which may draw from the Object
    /// stream between a push and a pop of its state, `FUN_140811270`/`FUN_140811210`) is not
    /// held.
    pub fn snow_under(&self, tile: Tile) -> Result<bool, GenieError> {
        let i = self
            .size
            .index(tile)
            .ok_or(GenieError::unsupported("a building off the map's tiles"))?;
        let kind = self.seen(*self.kinds.checked(i)?);
        // a layer of none reads as its short's low byte, 255: a terrain beyond the records
        let layer = self.seen(self.layers.checked(i)?.unwrap_or(LOW_BYTE_OF_NONE));
        Ok(self.snowy(kind) || self.snowy(layer))
    }

    /// [`Terrain::cover`]'s test for the foundation terrain: tile `i` of terrain `t` takes it
    /// over a plain or farm terrain whose layer is empty, plain or a farm too.
    fn foundation_covers(&self, i: usize, t: TerrainId) -> bool {
        let Some(&l) = self.layers.get(i) else {
            return false;
        };
        let farm = |t: TerrainId| t.get() < 200 && is_farm_terrain(t);
        (plain(t) || farm(t)) && l.is_none_or(|l| plain(l) || farm(l))
    }

    /// A completed building's foundation terrain (`FUN_1411ddd90`) over its position plus and
    /// minus its clearance (clamped to the map, `FUN_1409a47d0`). A tile of a terrain its
    /// `defense_terrain_bonus` scales (neither 0 nor 1) keeps it to restore; then, but on 47 and
    /// 69, foundation 27 covers the tiles of terrains 0, 3, 9, 12, 32, 34 or a farm's whose
    /// layer is none, one of those or a farm's; on snow it covers every tile but 36's with
    /// terrain 6 and layer 32; any other foundation covers the tiles of other terrains. A covered
    /// tile's layer goes to the snow layer (or none, `FUN_140f640f0`), then its terrain to the
    /// foundation's (`FUN_140f64170`), or with no snow layer and setting `+0x680` at 1 its layer
    /// (a farm's terrain and a building of class 14 always take the terrain). The town centre
    /// (109) passes over the four tiles of its gap, which keep their terrain to restore. Masters
    /// 357 and 1188 go into the terrain as a farm's terrain does. The deferred covering of farm
    /// terrains under `DAT_143f69454` (0 when probed) is not modelled; the local player's own
    /// farm's view (`FUN_140f640a0`) is not held.
    ///
    /// # Panics
    ///
    /// Never: the tiles it covers are held on the map.
    pub fn cover(&mut self, f: &Foundation) -> Result<(), GenieError> {
        /// `DAT_142eb1e9c`.
        const INSIDE: f32 = crate::math::image::terrain::INSIDE;
        let (x, y) = (f.position.x(), f.position.y());
        let (cx, cy) = (f.clearance.x(), f.clearance.y());
        let (mut x0, mut y0) = ((x - cx) as i32, (y - cy) as i32);
        let (mut x1, mut y1) = (((x + cx) - INSIDE) as i32, ((y + cy) - INSIDE) as i32);
        if x1 < x0 {
            std::mem::swap(&mut x0, &mut x1);
        }
        if y1 < y0 {
            std::mem::swap(&mut y0, &mut y1);
        }
        let (w, h) = (self.size.width() - 1, self.size.height() - 1);
        if x0 > w || y0 > h || x1 < 0 || y1 < 0 {
            return Err(GenieError::unsupported("a foundation terrain off the map"));
        }
        let (x0, y0, x1, y1) = (x0.max(0), y0.max(0), x1.min(w), y1.min(h));
        // the town centre's (109) gap: the clamped rectangle's first two columns of its third
        // and fourth rows
        let gap = |tx: i32, ty: i32| {
            // The town centre's master (109), whose foundation terrain leaves its gap (`FUN_1411ddd90`).
            f.master == TOWN_CENTRE && (tx == x0 || tx == x0 + 1) && (ty == y0 + 2 || ty == y0 + 3)
        };
        let (mut wanted, mut kind, mut layer) = (f.terrain, f.terrain, None);
        if f.terrain == FOUNDATION && f.snow.on_snow() {
            (wanted, kind, layer) = (SNOW_SKIPPED, SNOW_FOUNDATION, Some(SNOW_LAYER));
        }
        let bonus = f
            .terrain_bonus
            .restriction()
            .map(|r| {
                self.restriction(r).ok_or(GenieError::unsupported(
                    "a terrain bonus the data file does not have",
                ))
            })
            .transpose()?
            .cloned();
        // a farm's terrain, masters 357 and 1188 (taken as farms, `DAT_143f69454` 0 leaving
        // nothing deferred), or a building of class 14 always goes into the terrain
        let into_terrain = is_farm_terrain(wanted)
            || matches!(f.master, ids::ID_357 | ids::ID_1188)
            || f.class == CLASS_14;
        for ty in y0..=y1 {
            for tx in x0..=x1 {
                let i = self.size.index(Tile::new(tx, ty)).ok_or(TILE_OFF_MAP)?;
                let t = *self.kinds.checked(i)?;
                if let Some(values) = bonus.as_ref() {
                    let v = values.value(t).unwrap_or(0.0);
                    if v != 0.0 && v != 1.0 {
                        *self.restore.checked_mut(i)? = Some(t);
                    }
                }
                if UNCOVERED.contains(&t) {
                    continue;
                }
                let covered = if wanted == FOUNDATION {
                    self.foundation_covers(i, t)
                } else {
                    wanted != t
                };
                if !covered || gap(tx, ty) {
                    continue;
                }
                let cell = self.layers.checked_mut(i)?;
                *cell = layer;
                if into_terrain || layer.is_some() || !f.mode_680.is_one() {
                    *self.kinds.checked_mut(i)? = kind;
                } else {
                    *cell = Some(kind);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::masters::RestrictionField;
    use crate::test_ids::master_id;

    /// Terrains of ids `k`.
    fn ids(k: &[u8]) -> Vec<TerrainId> {
        k.iter().copied().map(TerrainId::named).collect()
    }

    /// Layers of shorts `l`, −1 none.
    fn layers_of(l: &[i16]) -> Vec<Option<TerrainId>> {
        l.iter().copied().map(TerrainId::of_data).collect()
    }

    fn map(kinds: Vec<u8>, layers: Vec<i16>) -> Terrain {
        let n = kinds.len();
        let mut terrains = vec![
            TerrainKind {
                shown: true,
                flags: TerrainFlags::LAND,
                forest: false,
            };
            40
        ];
        terrains[32].flags = TerrainFlags::LAND | TerrainFlags::SNOW;
        Terrain {
            size: GridSize::new(2, n as i32 / 2),
            kinds: ids(&kinds),
            restore: vec![None; n],
            layers: layers_of(&layers),
            terrains,
            restrictions: vec![Restriction::new(vec![0.0; 40]); 5],
            elevations: vec![0; n],
            shapes: vec![0; n],
        }
    }

    /// `FUN_141082530`: the defense of the restriction's value for the tile's terrain to restore,
    /// else its terrain; 0 counts as 1, a tile off the map has 1, and a restriction or terrain
    /// the data lacks has none.
    #[test]
    fn tile_defense_is_its_restriction_value_for_its_terrain() {
        let mut t = map(vec![0, 1, 2, 3], vec![0; 4]);
        let mut values = vec![0.0; 40];
        values[1] = 1.25;
        values[3] = 0.75;
        t.restrictions[2] = Restriction::new(values);
        t.restore[3] = Some(TerrainId::named(1));
        assert_eq!(
            t.defense(RestrictionField::of_data(2), Tile::new(1, 0)),
            Some(1.25)
        );
        assert_eq!(
            t.defense(RestrictionField::of_data(2), Tile::new(0, 0)),
            Some(1.0)
        );
        assert_eq!(
            t.defense(RestrictionField::of_data(2), Tile::new(1, 1)),
            Some(1.25)
        );
        assert_eq!(
            t.defense(RestrictionField::of_data(2), Tile::new(5, 5)),
            Some(1.0)
        );
        assert_eq!(
            t.defense(RestrictionField::of_data(9), Tile::new(1, 0)),
            None
        );
        assert_eq!(t.defense(RestrictionField::NONE, Tile::new(1, 0)), None);
    }

    /// `FUN_1411a8ba0`'s terrain at a point: x or y exactly −1 reads none, a NaN reads tile 0
    /// (`CVTTSS2SI` gives `0x80000000`, whose low 16 bits are 0), a tile off the map none.
    #[test]
    fn the_task_search_reads_the_terrain_at_point_on_the_map() {
        let t = map(vec![0, 1, 2, 3], vec![0; 4]);
        assert_eq!(
            t.at_task_point(Point::new(1.5, 1.25)),
            Some(TerrainId::named(3))
        );
        assert_eq!(
            t.at_task_point(Point::new(0.0, 0.99)),
            Some(TerrainId::named(0))
        );
        assert_eq!(t.at_task_point(Point::new(-1.0, 1.0)), None);
        assert_eq!(t.at_task_point(Point::new(1.0, -1.0)), None);
        assert_eq!(
            t.at_task_point(Point::new(-0.5, 1.0)),
            Some(TerrainId::named(2))
        );
        assert_eq!(
            t.at_task_point(Point::new(f32::NAN, 1.0)),
            Some(TerrainId::named(2))
        );
        assert_eq!(t.at_task_point(Point::new(2.0, 0.0)), None);
        assert_eq!(t.at_task_point(Point::new(0.0, 2.0)), None);
        assert_eq!(t.at_task_point(Point::new(-1.5, 0.0)), None);
        // 65536.5 truncates to tile 0 in 16 bits
        assert_eq!(
            t.at_task_point(Point::new(65_536.5, 1.0)),
            Some(TerrainId::named(2))
        );
    }

    #[test]
    fn the_ground_is_the_tile_elevation_plus_its_slope_share() {
        // two columns, four rows: (0, 3) is one up, so (0, 2) slopes up to the south (shape 7)
        // and (1, 2) up to the south-west corner (9); (1, 3) slopes up to the west (5)
        let mut t = map(vec![0; 8], vec![-1; 8]);
        t.set_elevations(vec![1, 1, 1, 1, 1, 1, 2, 1]);
        assert_eq!(t.shapes, [0, 0, 0, 0, 7, 9, 0, 5]);
        assert_eq!(t.ground(Point::new(0.5, 0.5)), Ok(1.0));
        assert_eq!(t.ground(Point::new(0.5, 2.25)), Ok(1.25));
        assert_eq!(t.ground(Point::new(1.25, 2.75)), Ok(1.5));
        assert_eq!(t.ground(Point::new(1.75, 2.25)), Ok(1.0));
        let mut unknown = map(vec![0; 8], vec![-1; 8]);
        unknown.elevations.clear();
        assert!(unknown.ground(Point::new(0.5, 0.5)).is_err());
    }

    #[test]
    fn tile_flags_are_its_seen_terrain_s() {
        // water 1 at (0, 0), a beach 2 at (1, 0), a hidden 3 and 45, beyond the records, at
        // (0, 1) and (1, 1): both read as terrain 0, shallows
        let mut t = map(vec![1, 2, 3, 45], vec![-1; 4]);
        t.terrains[0].flags = TerrainFlags::SHALLOWS;
        t.terrains[1].flags = TerrainFlags::WATER;
        t.terrains[2].flags = TerrainFlags::BEACH;
        t.terrains[3].shown = false;
        assert_eq!(
            [t.flags_at(Tile::new(0, 0)), t.flags_at(Tile::new(1, 0))],
            [TerrainFlags::WATER, TerrainFlags::BEACH]
        );
        assert_eq!(
            [t.flags_at(Tile::new(0, 1)), t.flags_at(Tile::new(1, 1))],
            [TerrainFlags::SHALLOWS; 2]
        );
        assert_eq!(
            [
                t.read(Tile::new(0, 0)),
                t.read(Tile::new(0, 1)),
                t.read(Tile::new(1, 1)),
                t.read(Tile::new(5, 5))
            ],
            [1, 0, 0, 0].map(TerrainId::named)
        );
        // off the map: terrain 0
        assert_eq!(
            [t.flags_at(Tile::new(-1, 0)), t.flags_at(Tile::new(2, 0))],
            [TerrainFlags::SHALLOWS; 2]
        );
        // without terrain 0's record, none
        t.terrains.clear();
        assert_eq!(t.flags_at(Tile::new(0, 1)), TerrainFlags::NONE);
    }

    #[test]
    fn tile_shape_follows_its_sides_then_its_corners_one_step_up() {
        let flat = [1; 4];
        assert_eq!(tile_shape(1, flat, flat), 0);
        assert_eq!(tile_shape(1, [2, 2, 1, 1], flat), 14);
        assert_eq!(tile_shape(1, [2, 1, 1, 1], flat), 6);
        assert_eq!(tile_shape(1, [1, 1, 1, 2], flat), 7);
        // a corner up with the far sides down is a saddle (2), else a corner slope (10)
        assert_eq!(tile_shape(1, [1, 1, 0, 0], [2, 1, 1, 1]), 2);
        assert_eq!(tile_shape(1, flat, [2, 1, 1, 1]), 10);
        assert_eq!(tile_shape(0, flat.map(|_| 0), [0, 0, 0, 1]), 12);
    }

    #[test]
    fn slope_share_is_how_far_up_the_tile_the_point_lies() {
        assert_eq!(slope_share(5, 0.25, 0.5), 0.75);
        assert_eq!(slope_share(6, 0.25, 0.5), 0.5);
        assert_eq!(slope_share(9, 0.25, 0.5), 0.25);
        assert_eq!(slope_share(9, 0.5, 0.25), 0.0);
        assert_eq!(slope_share(11, 0.25, 0.25), 0.5);
        assert_eq!(slope_share(13, 0.75, 0.5), 0.5);
        assert_eq!(slope_share(16, 0.25, 0.5), 0.75);
        assert_eq!(slope_share(0, 0.25, 0.5), 0.0);
        // every case's arms: the point below and above the tile's diagonal, or left and right
        // of its other one
        let (low, high, left) = ((0.25, 0.25), (0.75, 0.5), (0.25, 0.5));
        let right = (0.75, 0.25);
        let cases = [
            (1, high, 0.25),
            (1, left, 0.5),
            (7, left, 0.5),
            (2, high, 0.5),
            (2, left, 0.25),
            (8, left, 0.25),
            (3, left, 0.5),
            (3, right, 0.25),
            (4, left, 0.25),
            (4, high, 0.5),
            (10, right, 0.5),
            (10, left, 0.0),
            (11, high, 0.0),
            (12, high, 0.25),
            (12, low, 0.0),
            (13, low, 0.75),
            (14, low, 0.75),
            (14, high, 0.75),
            (15, left, 0.5),
            (15, high, 0.75),
            (16, high, 0.5),
            (17, low, 0.0),
        ];
        for (shape, (fx, fy), share) in cases {
            assert_eq!(slope_share(shape, fx, fy), share, "{shape} {fx} {fy}");
        }
    }

    /// MarvinBlue/426048731's house 10026 at (104, 38): its tiles 103..104 x 37..38 were snow (32)
    /// and grass 9 with a snow layer; the save after 1115 holds terrain 6 with layer 32 on each.
    #[test]
    fn house_completed_on_snow_leaves_terrain_6_under_snow_layer() {
        let mut t = map(vec![32, 32, 32, 9], vec![-1, -1, -1, 32]);
        assert!(t.snow_under(Tile::new(0, 0)).unwrap());
        assert!(t.snow_under(Tile::new(1, 1)).unwrap());
        t.cover(&Foundation {
            position: Position::new(1.0, 1.0, 0.0),
            clearance: Footprint::new(1.0, 1.0),
            terrain: FOUNDATION,
            snow: crate::map::Snow::Completed,
            master: ids::HOUSE,
            class: class::BUILDING,
            terrain_bonus: RestrictionField::of_data(4),
            mode_680: crate::world::Mode680::ZERO,
        })
        .unwrap();
        assert_eq!(t.kinds, ids(&[6, 6, 6, 6]));
        assert_eq!(t.layers, layers_of(&[32, 32, 32, 32]));
        assert_eq!(t.restore, [None; 4]);
    }

    /// The town centre's foundation (master 109, terrain 27; records/428759682's 8768 at
    /// (54, 147), its clearance 2): `FUN_1411ddd90` passes over the four tiles of its gap, the
    /// first two columns of the clamped rectangle's third and fourth rows, which keep their
    /// terrain (and still keep it to restore).
    #[test]
    fn the_town_centre_foundation_leaves_its_gap() {
        let mut t = map(vec![0; 2 * 4], vec![-1; 2 * 4]);
        t.size = GridSize::new(4, 4);
        t.kinds = ids(&[0; 16]);
        t.layers = vec![None; 16];
        t.restore = vec![None; 16];
        let mut values = vec![0.0; 40];
        values[0] = 0.5;
        t.restrictions[4] = Restriction::new(values);
        t.cover(&Foundation {
            position: Position::new(2.0, 2.0, 0.0),
            clearance: Footprint::new(2.0, 2.0),
            terrain: FOUNDATION,
            snow: crate::map::Snow::None,
            master: TOWN_CENTRE,
            class: class::BUILDING,
            terrain_bonus: RestrictionField::of_data(4),
            mode_680: crate::world::Mode680::ZERO,
        })
        .unwrap();
        #[rustfmt::skip]
        let covered = [
            27, 27, 27, 27,
            27, 27, 27, 27,
            0, 0, 27, 27,
            0, 0, 27, 27,
        ];
        assert_eq!(t.kinds, ids(&covered));
        assert_eq!(t.restore, [Some(TerrainId::named(0)); 16]);
    }

    #[test]
    fn foundation_27_covers_grass_but_not_terrain_outside_its_list() {
        let mut t = map(vec![0, 2, 9, 47], vec![-1, -1, -1, -1]);
        assert!(!t.snow_under(Tile::new(0, 0)).unwrap());
        t.cover(&Foundation {
            position: Position::new(1.0, 1.0, 0.0),
            clearance: Footprint::new(1.0, 1.0),
            terrain: FOUNDATION,
            snow: crate::map::Snow::None,
            master: ids::HOUSE,
            class: class::BUILDING,
            terrain_bonus: RestrictionField::NONE,
            mode_680: crate::world::Mode680::ZERO,
        })
        .unwrap();
        assert_eq!(t.kinds, ids(&[27, 2, 27, 47]));
    }

    /// A farm's foundation (terrain 29, then its stages 30 and 31) covers every tile of another
    /// terrain but 47 and 69, into the terrain even where setting `+0x680` at 1 would put another
    /// foundation into the layer (records/425906646's farm 31311 from update 8654).
    #[test]
    fn farm_foundation_lays_its_terrain_into_the_terrain() {
        let mut t = map(vec![0, 3, 29, 47], vec![-1, -1, -1, -1]);
        let farm = Foundation {
            position: Position::new(1.0, 1.0, 0.0),
            clearance: Footprint::new(1.0, 1.0),
            terrain: TerrainId::named(29),
            snow: crate::map::Snow::None,
            master: ids::FARM,
            class: class::FARM,
            terrain_bonus: RestrictionField::NONE,
            mode_680: crate::world::Mode680::ONE,
        };
        t.cover(&farm).unwrap();
        assert_eq!(
            (t.kinds.clone(), t.layers.clone()),
            (ids(&[29, 29, 29, 47]), vec![None; 4])
        );
        t.cover(&Foundation {
            terrain: TerrainId::named(30),
            ..farm
        })
        .unwrap();
        assert_eq!(t.kinds, ids(&[30, 30, 30, 47]));
    }

    /// Masters 357 and 1188 (428408213's 19365 in update 1069) go into the terrain as a farm's
    /// terrain does, though their foundation is not a farm's and setting `+0x680` is 1; any other
    /// building of the same foundation goes into the layer there.
    #[test]
    fn masters_357_and_1188_lay_their_terrain_into_the_terrain() {
        let site = |master| Foundation {
            position: Position::new(1.0, 1.0, 0.0),
            clearance: Footprint::new(1.0, 1.0),
            terrain: TerrainId::named(14),
            snow: crate::map::Snow::None,
            master,
            class: class::BUILDING,
            terrain_bonus: RestrictionField::NONE,
            mode_680: crate::world::Mode680::ONE,
        };
        for master in [master_id(357), master_id(1188)] {
            let mut t = map(vec![0, 3, 29, 47], vec![-1, -1, -1, -1]);
            t.cover(&site(master)).unwrap();
            assert_eq!(
                (t.kinds.clone(), t.layers.clone()),
                (ids(&[14, 14, 14, 47]), vec![None; 4])
            );
        }
        let mut t = map(vec![0, 3, 29, 47], vec![-1, -1, -1, -1]);
        t.cover(&site(ids::HOUSE)).unwrap();
        assert_eq!(t.kinds, ids(&[0, 3, 29, 47]));
        assert_eq!(t.layers, layers_of(&[14, 14, 14, -1]));
    }
}
