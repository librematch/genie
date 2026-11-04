//! Where the world's objects are, as movement reads them: each live object's position, radii and
//! class, and the obstruction record the game keeps for it in its obstruction manager
//! (`DAT_143f0a088`, queried by `FUN_141051900`).
//!
//! The game files its records in a grid of levels ([`Grid`]); the slice keeps them by object id
//! and asks the grid only which records a query visits: a record filed in a cell outside the
//! query's tiles is not found, even when its grown box meets the query. The grid also orders a
//! query's hits, but the slice stops a walker at its first hit ([`Spatial::walk_hits`]), so it
//! needs no order yet.
//!
//! [`Tiles`] holds what the game keeps per tile: the obstruction manager's word (terrain class,
//! static records' marks) and the counts of records and objects whose centre is on the tile.

use crate::GenieResult;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::Position;
use crate::map::Rect;
use crate::map::Tile;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::BuildableOn;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::objects::masters::ObstructionClass;
use crate::objects::masters::ObstructionKind;
use crate::pathing::tile_path::PlanRect;
use crate::players::PlayerId;
use crate::players::{ByRelation, QueryRelation};
use crate::vision::registry::Record;
use crate::vision::visibility::Visibility;
use crate::world::Seconds;
use crate::world::WorldTime;
use crate::world::layer::Roles;
use crate::world::layer::generated::BestTargetTuning;
use crate::world::layer::generated::SpatialTuning;
use genie_diff::Diff;
use std::collections::BTreeMap;

mod footprint;
pub use self::footprint::{Movers, SpotPick};
mod grid;
mod objects;
mod pather;
mod record_id;
mod search;
mod sight;
mod tile_sight;

pub use self::grid::{TileRecords, WalkQuery};

pub use self::pather::{Hit, SmoothingWalls, tile_record_id};
use self::pather::{WalkTest, record_meets_segment};
#[cfg(test)]
use self::pather::{box_meets_segment, ccw};
pub use self::record_id::RecordId;
pub use self::search::{SearchLists, SearchQuery};
pub use self::tile_sight::TileSight;
use crate::groups::GroupId;
use crate::map::terrain::Terrain;
use crate::movement::explore::InfluenceMap;
#[cfg(test)]
use crate::objects::unit::TAKEN_OUT;
use crate::objects::{IdTable, WorkerCount};
use crate::players::{ByPlayer, Relations, UnitRelations};
use crate::vision::VisionMasks;
use crate::world::index::ObjectsRef;
use std::collections::BTreeSet;
use std::ops::RangeInclusive;

/// The caller's reachability check of a found object (slot 0x3d0 within a range): it paths over
/// the spatial index the search reads, which the check's queries write, so the search hands it
/// over for each call.
pub type Reach<'a> = dyn FnMut(&mut Spatial, ObjectId, Distance) -> GenieResult<bool> + 'a;

/// What `ClosestObject` keeps of the objects found (its arguments 2 to 6; each −1 for any): the
/// master class, the relation (0 itself or friends, 1 neutrals, 2 enemies, 3 itself), the state,
/// the master and the cap of workers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Filter {
    group: Option<ClassId>,
    relation: Option<QueryRelation>,
    state: Option<ObjectState>,
    master: Option<MasterId>,
    workers: WorkerCount,
    /// The relation passed over (argument 9, `FUN_140c32b50`; −1 none).
    spared: Option<QueryRelation>,
    /// Argument 8: only objects the unit reaches count (slot 0x3d0).
    reach: bool,
}

/// `ClosestObject`'s count (`FUN_140c488b0`, `DAT_143e6f92c`): the owners from 1 to 8 of the
/// records the lists read, every record marking its owner (`record+8`) before any test; Gaia
/// is not counted.
fn players_seen(found: &ByRelation<Vec<Record>>) -> u32 {
    let mut seen = [false; 9];
    for record in found.values().iter().flatten() {
        if let Some(s) = seen.get_mut(record.owner.index()) {
            *s = true;
        }
    }
    seen[1..].iter().map(|&s| u32::from(s)).sum()
}

/// What the walk reads of a moving object it meets: whether it moves (its velocity, `+0x868`, is
/// not 0: slot 0x5c8, `FUN_1411583d0`), its last move time (`+0x870`, slot 0x5e8) and its current
/// path level's pause time (`+0x858`'s `+0x34`, slot 0x528), none without a level, and when that
/// level's path was made. The walk reads it from the met unit itself, by id
/// ([`crate::world::index::ObjectsRef::gait`], [`crate::objects::unit::Unit::gait`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Gait {
    /// It moves: its velocity (`+0x868`) is not 0 (slot 0x5c8).
    pub(crate) moving: bool,
    /// Slot 0x2a8 (`FUN_141163100`): it waits to move (`+0x7f9`) and its top action is in a state
    /// other than 6 and 7.
    pub(crate) waits: bool,
    /// Its last move time (`+0x870`, slot 0x5e8).
    pub(crate) last_move: WorldTime,
    /// Its current path level's pause time (slot 0x528), none without a level.
    pub(crate) pause: Option<Seconds>,
    /// `+0x650`, `mLastHumanOrderTime`: a met unit's rank against a human walker.
    pub(crate) human_order: WorldTime,
    /// When its current path level's path was made (slot 0x528's `+0x18`, the path's `+8`), none
    /// without a level or a path: a drop-off walker's ignore list passes a worker whose path is
    /// this update's.
    pub(crate) path_made: Option<WorldTime>,
    /// Its unit group (`mGroupID`, `+0x6d0`), none for −1: a walker of the same group ranks it by
    /// the group's member order.
    pub(crate) group: Option<GroupId>,
}

/// One object as movement sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    /// x, y, z on the map.
    pub position: Position,
    /// The master's radii on the ground (master `+0x5d8`, `+0x5dc`).
    pub(crate) radius: Footprint,
    /// The master's class (`+0x410`, decoded): the motion step treats gates (39) apart.
    pub(crate) class: ClassId,
    /// The owning player.
    pub(crate) player: PlayerId,
    /// The master's id.
    pub(crate) master: MasterId,
    /// The master's `obstruction_type` (`+0x651`), whether or not the object has its record yet.
    pub(crate) obstruction_kind: ObstructionKind,
    /// The master's `can_be_built_on` (`+0x5e9`): a foundation clears such objects off its
    /// footprint.
    pub(crate) buildable_on: BuildableOn,
    /// The master's height radius (`+0x5e0`) is above 0: the tile counts it as tall.
    pub(crate) tall: bool,
    /// Its master moves ([`crate::objects::masters::Master::moves`]: a moving master whose speed,
    /// `+0x740`, is above 0): the footprint's occupants read it (`FUN_14123bb20`, the master's
    /// slot 0xe8), and the pather's target (`FUN_140f85580`, the object's top speed, slot
    /// 0x208, not 0 just when its master's speed is not). Its placement and a change of its
    /// master write it.
    pub(crate) moves: bool,
    /// How far from `position` the object may be, when its player's update is not modelled: it
    /// grows by its master's speed every tick ([`Spatial::grow_unknown`]). None for an object
    /// the slice updates.
    pub(crate) unknown: Option<f32>,
    /// The obstruction record, if the object has one: a foundation gets its record only when
    /// construction starts.
    pub(crate) obstruction: Option<Obstruction>,
}

/// An obstruction record's kind (its byte `+1`, the master's `obstruction_type`, master `+0x651`)
/// and the master's `obstruction_class` (`+0x652`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Diff)]
pub struct Obstruction {
    /// The record's kind (`obstruction_type`).
    pub(crate) kind: ObstructionKind,
    /// The master's `obstruction_class`.
    pub(crate) class: ObstructionClass,
}

impl Obstruction {
    /// `FUN_141056460`'s first test: a master of obstruction kind 0 or 12, or of class 0, gets
    /// neither a record nor a mesh shape.
    #[must_use]
    pub const fn recorded(self) -> bool {
        self.kind.recorded() && !matches!(self.class, ObstructionClass::NONE)
    }
}

impl Placed {
    /// The record's box `x0, y0, x1, y1` (`+0x20`..`+0x2c`): the position plus and minus the radii.
    #[must_use]
    pub fn bounds(&self) -> Rect {
        let (x, y) = (self.position.x(), self.position.y());
        let (rx, ry) = (self.radius.x(), self.radius.y());
        Rect::new(x - rx, y - ry, x + rx, y + ry)
    }
}

/// The tile bit a static record sets (`0x4000`, in `FUN_141056460`): the tile is blocked.
pub const BLOCKED: u64 = 0x4000;

/// The room of the hit list the obstruction queries' callers give (`local_1758`, `local_148`:
/// `0x19`), which the edge records (`FUN_141057290`) also respect.
const QUERY_HITS: usize = 0x19;

/// The most objects a tile's list holds (`FUN_140f5f0a0`'s `count < 0xfa`).
const MOST_ON_TILE: usize = 0xfa;

/// The obstruction manager's grid (`FUN_141054660`): its top cell is the largest power of two up
/// to twice the map's longer side (at most 4096 tiles), each level halves the cells' side
/// (level `k`'s cells are `top >> k` tiles wide, its cell shift `levels - k + 2`), down to cells
/// of 8 tiles. Records are filed in one cell each ([`Spatial::filed`]); a query visits on every
/// level the cells its tile range covers (`FUN_141051900`, `FUN_141052350`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Grid {
    /// How many levels (`+0x1ac`).
    levels: i32,
}

impl Grid {
    fn new(map: GridSize) -> Self {
        let (w, h) = (map.width(), map.height());
        let longer = w.max(h);
        let mut top = 0x1000;
        while longer * 2 < top {
            top >>= 1;
        }
        let mut levels = 0;
        while 7 < top {
            levels += 1;
            top >>= 1;
        }
        Self { levels }
    }

    /// The level table (`+0x1bc`): the finest level whose cells hold two tiles whose coordinates
    /// XOR, shifted right by 3, to `v` (the levels less its bit length, at least −1).
    fn level(self, v: i32) -> i32 {
        let bits = 32 - v.cast_unsigned().leading_zeros() as i32;
        (self.levels - 1 - bits).max(-1)
    }

    /// Level `level`'s cell shift (`+0x2c4`).
    const fn shift(self, level: i32) -> i32 {
        self.levels - level + 2
    }
}

/// What the game keeps per tile, row by row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tiles {
    size: GridSize,
    /// The obstruction manager's word (`DAT_143f0a088 + 0x98`): bits 0 to 7 the terrain's class,
    /// bits 8 to 13 a static record's obstruction class, [`BLOCKED`], bit 15 for class 5, bits 16
    /// to 22 the record's player, bits 23 and up a bit per player that marked the tile.
    words: Vec<u64>,
    /// Tall objects (a height radius above 0) whose centre is on the tile (the map's bytes at
    /// `+0x10`, `FUN_140f5f0a0`); the passability test queries the obstructions of a tile with two.
    tall: Vec<u8>,
    /// The objects whose centre is on the tile, in the tile's order (its list `+0x28`, count
    /// `+0x1b`): an object entering the tile goes last (`FUN_140f5f0a0`), one leaving it gives
    /// its slot to the last (`FUN_140f5f560`).
    lists: Vec<Vec<ObjectId>>,
}

impl Tiles {
    /// A map whose tiles hold `classes` (one terrain class bit per tile, row by row) and nothing
    /// else.
    ///
    /// # Panics
    ///
    /// When `classes` does not hold one class per tile of the map.
    pub fn new(size: GridSize, classes: Vec<u8>) -> Self {
        let n = classes.len();
        assert_eq!(
            n,
            (size.width().max(0) * size.height().max(0)) as usize,
            "one class per tile"
        );
        Self {
            size,
            words: classes.into_iter().map(u64::from).collect(),
            tall: vec![0; n],
            lists: vec![Vec::new(); n],
        }
    }

    /// The map's size in tiles.
    #[must_use]
    pub const fn size(&self) -> GridSize {
        self.size
    }

    /// `tile`'s word: its terrain class and the obstruction manager's marks.
    #[must_use]
    pub fn word(&self, tile: Tile) -> Option<u64> {
        self.size
            .index(tile)
            .and_then(|i| self.words.get(i).copied())
    }

    /// Whether the obstruction manager marks `tile` blocked ([`BLOCKED`]); a tile off the map is
    /// not.
    #[must_use]
    pub fn blocked(&self, tile: Tile) -> bool {
        self.word(tile).is_some_and(|w| w & BLOCKED != 0)
    }

    /// A test's map of `size`: every tile of class 0, blocked where `blocked` says.
    #[cfg(test)]
    pub(crate) fn with_blocked(size: GridSize, blocked: impl Fn(Tile) -> bool) -> Self {
        let mut tiles = Self::new(size, vec![0; (size.width() * size.height()) as usize]);
        for y in 0..size.height() {
            for x in 0..size.width() {
                if blocked(Tile::new(x, y)) {
                    tiles.words[(y * size.width() + x) as usize] |= BLOCKED;
                }
            }
        }
        tiles
    }

    /// How many tall objects stand on `tile`.
    #[must_use]
    pub fn tall(&self, tile: Tile) -> Option<u8> {
        self.size
            .index(tile)
            .and_then(|i| self.tall.get(i).copied())
    }

    /// How many objects stand on `tile`.
    #[must_use]
    pub fn objects(&self, tile: Tile) -> Option<u8> {
        self.size
            .index(tile)
            .and_then(|i| self.lists.get(i))
            .map(|l| l.len() as u8)
    }

    /// The objects on `tile` in the tile's order; none off the map.
    #[must_use]
    pub fn list(&self, tile: Tile) -> &[ObjectId] {
        self.size
            .index(tile)
            .and_then(|i| self.lists.get(i))
            .map_or(&[], Vec::as_slice)
    }

    /// The tile of a centre, if it is on the map.
    const fn tile_of(&self, position: Position) -> Option<usize> {
        self.size
            .index(Tile::new(position.x() as i32, position.y() as i32))
    }

    /// The object goes last on tile `i` (`FUN_140f5f0a0`). An empty tile's list takes it and
    /// the tall count becomes 1 for a `tall` object (else it stays, 0 on an empty tile); a tile
    /// of [`MOST_ON_TILE`] objects or more takes none and keeps its count; else the object goes
    /// last and the tall count is redone over the list, each object by `tall` (its master's
    /// height radius above 0 now).
    fn append(&mut self, i: usize, id: ObjectId, tall: impl Fn(ObjectId) -> bool) {
        let (Some(list), Some(count)) = (self.lists.get_mut(i), self.tall.get_mut(i)) else {
            return;
        };
        let n = list.len();
        if n == 0 {
            list.push(id);
            if tall(id) {
                *count = 1;
            }
            return;
        }
        if MOST_ON_TILE <= n {
            return;
        }
        list.push(id);
        self.recount(i, tall);
    }

    /// The object leaves tile `i` (`FUN_140f5f560`): one not on the list changes nothing; else
    /// the last object takes its slot and the tall count is redone over the list.
    fn take_off(&mut self, i: usize, id: ObjectId, tall: impl Fn(ObjectId) -> bool) {
        let Some(list) = self.lists.get_mut(i) else {
            return;
        };
        let Some(k) = list.iter().position(|&o| o == id) else {
            return;
        };
        let _ = list.swap_remove(k);
        self.recount(i, tall);
    }

    /// Tile `i`'s tall count over its list, each object by `tall` (a byte, as the game counts).
    fn recount(&mut self, i: usize, tall: impl Fn(ObjectId) -> bool) {
        let (Some(list), Some(count)) = (self.lists.get(i), self.tall.get_mut(i)) else {
            return;
        };
        *count = list.iter().filter(|&&o| tall(o)).count() as u8;
    }

    /// Marks (`bits`) on `tile`'s word, for a test's map.
    #[cfg(test)]
    pub(crate) fn mark_for_test(&mut self, tile: Tile, bits: u64) {
        let i = self.size.index(tile).expect("a tile on the map");
        self.words[i] |= bits;
    }

    /// A tall object on `tile`, for a test's map.
    #[cfg(test)]
    pub(crate) fn add_tall_for_test(&mut self, tile: Tile) {
        let i = self.size.index(tile).expect("a tile on the map");
        self.lists[i].push(TAKEN_OUT);
        self.tall[i] += 1;
    }

    /// The tiles a static record covers (`FUN_141056460`): from the box's low edge to its high
    /// edge less 0.001, held on the map.
    fn covered(&self, p: &Placed) -> (RangeInclusive<i32>, RangeInclusive<i32>) {
        let (x, y) = (p.position.x(), p.position.y());
        let (rx, ry) = (p.radius.x(), p.radius.y());
        let hold = |v: i32, size: i32| v.clamp(0, size - 1);
        let (w, h) = (self.size.width(), self.size.height());
        let xs = hold((x - rx) as i32, w)..=hold(((x + rx) - 0.001) as i32, w);
        let ys = hold((y - ry) as i32, h)..=hold(((y + ry) - 0.001) as i32, h);
        (xs, ys)
    }

    /// The word indices of the covered tiles, row by row: each is on the map.
    fn covered_indices(&self, p: &Placed) -> impl Iterator<Item = usize> + use<> {
        let width = self.size.width();
        let on_map = 0 < self.size.width() && 0 < self.size.height();
        let (xs, ys) = if on_map {
            self.covered(p)
        } else {
            (RangeInclusive::new(1, 0), RangeInclusive::new(1, 0))
        };
        ys.flat_map(move |y| xs.clone().map(move |x| (y * width + x) as usize))
    }

    /// The covered tiles as a rectangle, as the manager hands it to the planners' callback.
    fn covered_rect(&self, p: &Placed) -> PlanRect {
        let (xs, ys) = self.covered(p);
        let [x0, y0, x1, y1] = [*xs.start(), *ys.start(), *xs.end(), *ys.end()].map(|v| v as i16);
        PlanRect::new(x0, y0, x1, y1)
    }

    /// A static record marks the tiles it covers that no player marked yet.
    fn mark(&mut self, p: &Placed, o: Obstruction) {
        let player = p.player.index() as u64;
        let mut bits =
            ((player << 8 | u64::from(o.class.mark_bits()) | 0x40) << 8) | (0x80_0000 << player);
        if o.class == ObstructionClass::GATE {
            bits |= 0x8000;
        }
        for i in self.covered_indices(p) {
            if let Some(word) = self.words.get_mut(i)
                && *word < 0x80_0000
            {
                *word = (*word & !0xff_ff00) | bits;
            }
        }
    }

    /// A static record of object `me` leaves (`FUN_141055980`): tile by tile, row by row, over
    /// the tiles it covers, a tile its player marked keeps only its terrain class; but at a tile
    /// where another object (its centre on the tile) holds a static record (`static_record`:
    /// kind 0, 1, 2, 11 or 13), the rest stays as it is and the planners hear nothing (false).
    fn unmark(
        &mut self,
        p: &Placed,
        me: ObjectId,
        static_record: impl Fn(ObjectId) -> bool,
    ) -> bool {
        let own = 1u64 << p.player.get();
        for i in self.covered_indices(p) {
            if self
                .lists
                .get(i)
                .is_some_and(|l| l.iter().any(|&o| o != me && static_record(o)))
            {
                return false;
            }
            if let Some(word) = self.words.get_mut(i)
                && *word >> 23 == own
            {
                *word &= 0xff;
            }
        }
        true
    }
}

/// The obstruction manager's grid as a lookup (FD-030-D03): the cell each object's record is
/// filed in ([`Spatial::filed`], as (level, x, y)) and the ids filed in each cell, so a query
/// reads the cells it visits ([`Spatial::visited_records`]) instead of testing every object.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct RecordCells {
    /// The ids by cell, in (level, x, y) order: a query that visits more cells than are held
    /// walks one level's range of them.
    cells: BTreeMap<(i32, i32, i32), BTreeSet<ObjectId>>,
    filed: IdTable<(i32, i32, i32)>,
}

/// The world's objects by id, the map's size in tiles and what the game keeps per tile.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Spatial {
    /// The map's size in tiles.
    pub map: GridSize,
    /// The obstruction manager's ignore list (`+0x2c` set, `+0x30` count, `+0x38` ids) as the
    /// last `FUN_1410562e0` left it: the queries that set it before they ask
    /// ([`Spatial::set_manager_ignore`]) leave it to the ones that do not (the motion step's
    /// query of a level's waypoint). The game keeps a pointer to the caller's list; the slice
    /// keeps the ids it held then.
    manager_ignore: Vec<RecordId>,
    /// The obstruction manager's temporary tile records (`FUN_1410510c0`: kind 3, the tile's box,
    /// id [`tile_record_id`], on the list `+0x70` with the count `+0x78`, bit 0x80 in the tile's
    /// word), as (y, x): [`Spatial::put_tile_records`] puts them, and they stay for every query
    /// until [`Spatial::clear_tile_records`] removes them all.
    tile_records: BTreeSet<(i32, i32)>,
    /// The obstruction manager's buffer (`+0x57` set, `+0x58` the radius its queries grow the
    /// records by) as the last `FUN_1410562d0` left it, None when not set: the routines that
    /// set it reset it at their end but `FindShortPath`'s quick-only exit, and the queries that
    /// set none read what is left ([`Spatial::manager_grow`]).
    manager_buffer: Option<Distance>,
    /// The placed objects by id (T7: a dense column, walked in the ids' order).
    objects: IdTable<Placed>,
    /// The cell of each object's record ([`RecordCells`]), kept with `objects`
    /// ([`Spatial::refile`]).
    records: RecordCells,
    /// The objects inside others (`+0x2f0`): off the tiles' lists and the obstruction manager,
    /// without a record, at their container's x and y; a lookup by id still finds them.
    inside: IdTable<Placed>,
    tiles: Tiles,
    /// The tile rectangles (x0, y0, x1, y1) whose static marks changed, for the planners
    /// ([`Spatial::take_changes`]).
    changed: Vec<PlanRect>,
    /// The seconds since each frozen object froze.
    frozen_for: IdTable<Seconds>,
    /// What a stopped player's units may have done since: the fastest any of the players' unit
    /// masters (type 70) moves and the farthest any acts beyond its footprint (its weapon or work
    /// range), for [`Spatial::check_known`]. 0 when not set.
    pub(crate) fastest: f32,
    /// The farthest any of the players' units acts beyond its footprint.
    pub(crate) reach: Distance,
    /// Each player's scout map (`player+0x1370`, `mIsScoutMapEnabled` `+0x1368`), once made.
    scout_maps: BTreeMap<PlayerId, InfluenceMap>,
    /// `relation[i]` of each player (`player+0x260`), for the walk's query's allies.
    pub(crate) relations: ByPlayer<Relations>,
    /// What each player's queries make of the other players' objects (`player+0x278`, the
    /// save's `unit_diplomacy`): none for a saved value past the five relations.
    /// Loaded, not derived: the slice runs no diplomacy change (command 0x76), whose handler
    /// would write it with `relation`.
    pub(crate) unit_diplomacy: ByPlayer<UnitRelations>,
    /// Each player's masters with a task of action type 0x99, which a building's slot 0x4f0
    /// reads (`FUN_1411d4980` -> `FUN_1415c60b0`): a drop-off walker counts such a building
    /// among the workers near it.
    pub(crate) task_0x99: Vec<Vec<MasterId>>,
    /// What the players see, when the slice holds it: the objects' sight follows their tiles
    /// ([`Spatial::set_position`]).
    pub sight: Option<Visibility>,
    /// Each player's vision masks, visible (`+0x2a8`) and explored (`+0x2a0`), as the world's
    /// rebuild sets them ([`crate::vision`]); a player without them sees with its own bits.
    pub(crate) vision: ByPlayer<VisionMasks>,
    /// The map's terrain, when the slice holds it.
    pub terrain: Option<Terrain>,
    /// The unit AIs' search cache ([`SearchCache`]).
    search_cache: SearchCache,
    /// The obstruction manager's grid, from the map's size.
    grid: Grid,
}

/// The unit AIs' search cache (`FUN_140c63e20`): a search within `SpatialTuning::search_reuse_squared` (squared)
/// of the last one's place while the cache holds skips its query ("Already searched this update,
/// skipping second time.") and reads the registry's last answer, whichever unit's search made it;
/// a search that queries sets the place and holds the cache. Each unit AI's full update
/// (`0x140c66b10`, at its start) lets it go ([`Spatial::forget_search`]). Not saved: a slice
/// loaded from a save starts without one, as the game's load does.
#[derive(Clone, Debug, Default, PartialEq)]
struct SearchCache {
    holds: bool,
    at: Point,
    /// The registry's five output lists as the last query left them.
    last: Option<ByRelation<Vec<Record>>>,
}

impl SearchCache {
    /// A search that queries sets the place and holds the cache (`FUN_140c63e20`).
    const fn hold(&mut self, at: Point) {
        self.holds = true;
        self.at = at;
    }

    /// The registry's lists a query left (`DAT_143f69178`): the last answer the cache reads.
    fn remember(&mut self, lists: ByRelation<Vec<Record>>) {
        self.last = Some(lists);
    }
}

/// [`Spatial::closest_object`]'s per-object checks: the world's objects by id (herded livestock,
/// the masters a class-10 searcher passes, a farm's workers and pasture), the build's roles of
/// the classes it meets and the target search's tuning values.
#[derive(Clone, Copy)]
struct ClosestChecks<'a> {
    objects: ObjectsRef<'a>,
    roles: &'a Roles,
    best_target_tuning: &'a BestTargetTuning,
    spatial_tuning: &'a SpatialTuning,
}

#[cfg(test)]
mod tests;
