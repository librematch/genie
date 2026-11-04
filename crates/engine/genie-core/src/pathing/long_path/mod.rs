//! The long pather (`FUN_140f9eb70`, run on the global at `0x1439e6910`): a best-first search over
//! the map's tiles from a unit's tile to a goal tile, which the reachability test of a point
//! (`FUN_140fa0370`, [`crate::movement::Mover::reaches`]) runs for a goal more than a tile from
//! the unit.
//!
//! A tile's cost is its distance walked so far plus its squared distance to the goal, both in
//! quarter tiles (a straight step costs 4, a diagonal 8, a change of direction 1 more); the open
//! tiles sit in a binary heap by cost, whose sift down the game runs only while the count's half
//! is above 1 and only to children below that half: the slice keeps both quirks. A tile is
//! entered when its terrain class is in the unit's mask, no static record blocks it (the
//! obstruction manager's word, [`crate::map::spatial::BLOCKED`]) and no tall object stands on it (the
//! map's count at `+0x10`, less the unit's own and the ignored units', `FUN_140f60440`); a
//! diagonal step also needs both tiles beside it so. The search ends at the goal tile or within
//! a tile of it (or, with `within_range`, within the range), after too many expansions, or when
//! the heap runs dry.
//!
//! With a target object the goal is its footprint's tiles (each of which the search enters past
//! its static record, not past a tall object), it counts as reached at once within the range of
//! its footprint (`FUN_14118f680`), and a unit already in range (`FUN_14118f8c0`) needs no search.
//!
//! The path the search writes when asked ([`path`], `FUN_140f9d770`) runs back from the tile it
//! ended on along the directions it entered each tile by; a search that ran out writes the path
//! to the tile it came nearest the goal on and answers no.
//!
//! Not modelled: an allied gate on the way (the tile's record of class 5, slot 0x678; another
//! player's gate is closed) and the search
//! beside a player's objects when the caller names one (argument 11, `FUN_14152f790`). The pather's flag at `+0x2db638` (terrain the unit
//! may cross anyway, `FUN_1413fc2b0`) is never set in this build: no instruction writes it.

use crate::map::Distance;
use crate::map::DistanceSquared;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Position;
use crate::map::spatial::{BLOCKED, Spatial, Tiles};
use crate::map::{Tile, TileRect};
use crate::math::fmath::BVector;
use crate::pathing::tile_path::Path;
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::world::index::ObjectsRef;
use crate::world::slots::CheckedIndexExt;
use crate::{GenieError, GenieResult};

mod cost;
mod step_dir;

use self::cost::{Heap, LongCost, SENTINEL, step_cost};
use self::step_dir::{STEPS, StepDir};
use crate::construction::gates::lets_pass;
use crate::map::TerrainMask;
use crate::math::fmath::sqrt;
use crate::math::image::long_path::{
    EPSILON as LONG_PATH_EPSILON, TILE_CENTRE as LONG_PATH_TILE_CENTRE,
};
use crate::players::PlayerSet;
use crate::world::layer::ClassRoles;
use crate::world::layer::generated::LongPathTuning;

/// The search expands at most this many tiles (`FUN_140f9eb70`: `20000 < n` breaks).
const MOST_EXPANSIONS: i32 = 20000;

/// A human player's search expands at most this many hundredths of the straight distance in
/// tiles (`DAT_143f0f358`, which the static initializer at `0x140167e90` sets to 7500, or to 10000
/// with `DAT_143f01832`, which no run of this build has set).
const HUMAN_PERCENT: i32 = 7500;

/// A computer player's search expands at most this many tiles per tile of straight distance
/// (`FUN_140f9eb70` after `FUN_141659cf0`).
const COMPUTER_FACTOR: i32 = 25;

/// The margin of the reached test within the range (`DAT_142eb0c60`).
const EPSILON: f32 = LONG_PATH_EPSILON;

/// A path point lies at its tile's centre: the tile plus this on both axes (`DAT_142e7f384`).
const TILE_CENTRE: f32 = LONG_PATH_TILE_CENTRE;

/// The record class whose tiles an allied unit may pass when its gate lets it (`0x500` in the
/// obstruction manager's word, bits 8 to 13).
const GATE_CLASS_BITS: u64 = 0x500;

/// What a search asks of an allied gate's tile: whether its gate lets the unit pass
/// ([`gate_lets_pass`] in a world; a test answers for itself).
pub type GateAsk<'env> = &'env dyn Fn(Tile) -> GenieResult<bool>;

/// `FUN_140f9eb70` on an allied gate's record at `tile`, for a unit of `player`: a building's
/// tile (the map tile's `+0x19` bits 0x60, [`crate::vision::visibility::Visibility::building_tile`])
/// whose anchor (`FUN_140f68f50`) holds an object of class 39 asks the first such object in the
/// anchor's object list (slot 0x678, [`crate::construction::gates::lets_pass`]); any other tile
/// is closed. Without the map's building links (no sight held) no tile is a building's.
///
/// # Errors
///
/// A master its owner's table does not hold.
pub fn gate_lets_pass(
    spatial: &Spatial,
    objects: ObjectsRef<'_>,
    player: PlayerId,
    tile: Tile,
) -> GenieResult<bool> {
    let roles = objects.roles();
    let Some(links) = spatial.sight.as_ref() else {
        return Ok(false);
    };
    let Some(anchor) = Some(tile)
        .filter(|&tile| links.building_tile(tile))
        .and_then(|tile| links.anchor_of(tile))
    else {
        return Ok(false);
    };
    for &id in spatial.tile_list(anchor) {
        // the ghosts and the unit out for its update are no gate
        let Some((owner, gate, master)) = objects.with_master(id)? else {
            continue;
        };
        if roles.of_class(master.class).contains(ClassRoles::GATE) {
            let layer = objects.building(id).ok_or_else(|| {
                GenieError::unreachable("a gate whose layer the view does not find")
                    .with("object", id)
            })?;
            return Ok(lets_pass(
                (gate, layer, master),
                owner,
                player,
                spatial,
                roles,
            ));
        }
    }
    Ok(false)
}

/// The target object of a search (`FUN_140f9eb70`'s argument 8).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LongTarget {
    /// Its tile.
    pub(crate) tile: Tile,
    /// The tiles of its footprint (the centre less the radius, the centre plus the radius less
    /// `LongPathTuning::edge_gap`, whole).
    pub(crate) rect: TileRect,
    /// The unit's footprint gap to it, squared (`FUN_14118f8c0`) and not (`FUN_14118f680`).
    pub(crate) gap_squared: DistanceSquared,
    /// The gap, not squared (`FUN_14118f680`).
    pub(crate) gap: Distance,
}

impl LongTarget {
    /// The target at `position` with radii `radius`, `gap` and `gap_squared` from the unit.
    #[must_use]
    pub fn new(
        position: Position,
        radius: Footprint,
        gap: Distance,
        gap_squared: DistanceSquared,
        long_path_tuning: LongPathTuning,
    ) -> Self {
        let (x, y) = (position.x(), position.y());
        Self {
            tile: Tile::new(x as i32, y as i32),
            rect: TileRect::new(
                (x - radius.x()) as i32,
                (y - radius.y()) as i32,
                ((x + radius.x()) - long_path_tuning.edge_gap) as i32,
                ((y + radius.y()) - long_path_tuning.edge_gap) as i32,
            ),
            gap_squared,
            gap,
        }
    }
}

/// What the search is asked: from one tile to another, within `range`.
#[derive(Clone, Debug, PartialEq)]
pub struct LongRequest {
    /// The start tile.
    pub(crate) from: Tile,
    /// The goal tile.
    pub(crate) to: Tile,
    /// How near counts as there.
    pub(crate) range: Distance,
    /// The target object, None for a point.
    pub(crate) target: Option<LongTarget>,
    /// The unit moves: its speed (slot 0x208, `FUN_141202c70`) is not 0, or its master is 42.
    pub(crate) moves: bool,
    /// The terrain classes the unit may stand on (its restriction's mask).
    pub(crate) mask: TerrainMask,
    /// The owner's type (`player+0x10`): a computer player (`FUN_141659cf0`) has a shorter
    /// budget.
    pub(crate) player_kind: PlayerKind,
    /// The tiles of the ignored units the search lowers the tall count of, once each
    /// (`FUN_140f60440`: the unit itself and its moving ignored units, not inside another, whose
    /// master has a height).
    pub(crate) lowered: Vec<Tile>,
    /// Argument 13: the goal counts as reached within the range (else within a tile).
    pub(crate) within_range: bool,
    /// Argument 11 is not −1: a blocked tile with no object of the player around counts as open.
    pub(crate) near_player: bool,
    /// The players the unit's player counts as allies (`FUN_141663dc0` with the ally flag): a
    /// gate's tile of another player is closed.
    pub(crate) allies: PlayerSet,
}

/// How a search ended ([`search`]).
enum Searched {
    /// An end outside the pather's map, or a unit that does not move: no, and no path.
    Refused,
    /// The goal within the range before the search: yes, and no path.
    InRange,
    /// The search ran: it reached the goal on `end` (`reached`), or ran out with `end` the tile
    /// it came nearest the goal on; `entered` holds the direction each tile was entered by.
    Ran {
        reached: bool,
        end: Tile,
        entered: Vec<Option<StepDir>>,
    },
}

/// `FUN_140f9eb70` for a goal point without a path to write: whether the unit reaches it.
pub fn reaches(
    tiles: &Tiles,
    map: GridSize,
    req: &LongRequest,
    gates: GateAsk<'_>,
    long_path_tuning: LongPathTuning,
) -> GenieResult<bool> {
    Ok(match search(tiles, map, req, gates, long_path_tuning)? {
        Searched::Refused => false,
        Searched::InRange => true,
        Searched::Ran { reached, .. } => reached,
    })
}

/// `FUN_140f9eb70` with a path to write (its argument 12): whether the unit reaches the goal, and
/// the path ([`trace`] from the tile the search ended on, at the unit's height `z`; empty when the
/// search did not run). A path the trace cannot write (a point equal to the one beside it) is
/// emptied and answers no.
///
/// # Errors
///
/// What the search does not model.
pub fn path(
    tiles: &Tiles,
    map: GridSize,
    (req, gates): (&LongRequest, GateAsk<'_>),
    z: f32,
    long_path_tuning: LongPathTuning,
) -> GenieResult<(bool, Path)> {
    Ok(match search(tiles, map, req, gates, long_path_tuning)? {
        Searched::Refused => (false, Path::default()),
        Searched::InRange => (true, Path::default()),
        Searched::Ran {
            reached,
            end,
            entered,
        } => trace(req.from, end, &entered, map, z)
            .map_or_else(|| (false, Path::default()), |path| (reached, path)),
    })
}

/// `FUN_140f9d770`: the path from `from` to `end`, traced back from `end` along the direction
/// each tile was entered by: a tile whose direction differs from the one after it on the way back
/// (the end's always) puts its centre (x + 0.5, `z`, y + 0.5, the paths' layout) in front
/// (`FUN_140f95a50`), the start tile's last. None when a point equals the one beside it
/// (`FUN_140f95060` refuses it).
fn trace(
    from: Tile,
    end: Tile,
    entered: &[Option<StepDir>],
    map: GridSize,
    z: f32,
) -> Option<Path> {
    // a tile never entered reads the game's 0xff: none
    let direction = |tile: Tile| entered.get(cell(map, tile)).copied().flatten();
    let centre = |tile: Tile| -> BVector {
        BVector::new(
            tile.x() as f32 + TILE_CENTRE,
            z,
            tile.y() as f32 + TILE_CENTRE,
        )
    };
    let mut path = Path::default();
    let mut at = end;
    let mut came = direction(at);
    let mut before = None;
    while at != from {
        if came != before && !path.insert(0, centre(at)) {
            return None;
        }
        // back along the step it was entered by (the game's 0xff indexes past the table)
        let step = came?.offset()?;
        at -= step;
        before = came;
        came = direction(at);
    }
    path.insert(0, centre(at)).then_some(path)
}

/// [`search`]'s answers before the search: an end off the pather's width (`+0x8`, the far edge
/// included) is refused; a target within range, or without a target a goal within a range
/// above `LongPathTuning::reached` (`gap2` the ends' squared gap), is in range; a unit that does not move is
/// refused. None: the search runs.
fn search_refused(
    map: GridSize,
    req: &LongRequest,
    gap2: i32,
    long_path_tuning: LongPathTuning,
) -> Option<Searched> {
    let (width, _) = (map.width(), map.height());
    let held = |value: i32| 0 <= value && value <= width;
    let (x0, y0) = (req.from.x(), req.from.y());
    let (x1, y1) = (req.to.x(), req.to.y());
    if !(held(x0) && held(y0) && held(x1) && held(y1)) {
        return Some(Searched::Refused);
    }
    if req
        .target
        .is_some_and(|long_target| long_target.gap_squared <= req.range.squared())
    {
        return Some(Searched::InRange);
    }
    if long_path_tuning.reached < req.range.get()
        && req.target.is_none()
        && (gap2 as f32) < req.range.squared().get()
    {
        return Some(Searched::InRange);
    }
    (!req.moves).then_some(Searched::Refused)
}

/// [`search`]'s goal test of the popped tile `distance` tiles from the goal: a target's gap within
/// the range ([`EPSILON`]) or the tile within `LongPathTuning::reached`; without a target, a search
/// to within range by `distance` against the range, else `distance` within
/// `LongPathTuning::reached`.
fn search_reached(req: &LongRequest, distance: f32, long_path_tuning: LongPathTuning) -> bool {
    match req.target {
        Some(long_target) => {
            long_target.gap - req.range < Distance::new(EPSILON)
                || distance < long_path_tuning.reached
        }
        None if req.within_range => distance - req.range.get() < EPSILON,
        None => distance < long_path_tuning.reached,
    }
}

/// What [`search`] reads of the tiles: the obstruction words, the tall objects less those the
/// request lowers (`lowered`, one each), and the unit's blocked terrain classes with and
/// without [`BLOCKED`], and what an allied gate answers.
struct Grid<'tiles> {
    tiles: &'tiles Tiles,
    gates: GateAsk<'tiles>,
    map: GridSize,
    lowered: Vec<u8>,
    blocked_terrain: u64,
    blocked: u64,
}

impl<'tiles> Grid<'tiles> {
    fn new(tiles: &'tiles Tiles, map: GridSize, req: &LongRequest, gates: GateAsk<'tiles>) -> Self {
        let blocked_terrain = req.mask.blocked();
        let mut lowered = vec![0u8; (map.width().max(0) * map.height().max(0)) as usize];
        for &tile in &req.lowered {
            if let Some(cell) = map.index(tile).and_then(|index| lowered.get_mut(index)) {
                *cell = cell.wrapping_add(1);
            }
        }
        Self {
            tiles,
            gates,
            map,
            lowered,
            blocked_terrain,
            blocked: blocked_terrain | BLOCKED,
        }
    }

    fn word(&self, tile: Tile) -> u64 {
        self.tiles.word(tile).unwrap_or(0)
    }

    fn tall(&self, tile: Tile) -> u8 {
        let Some(count) = self.tiles.tall(tile) else {
            return 0;
        };
        // the tiles hold the tile, so the map does
        count.wrapping_sub(
            self.map
                .index(tile)
                .and_then(|index| self.lowered.get(index))
                .copied()
                .unwrap_or(0),
        )
    }

    /// The step onto `next` (with the tiles `beside` a diagonal): its terrain and the beside tiles'
    /// passable, else it is skipped; then open when the tile, the beside tiles and their tall
    /// objects are clear, or, at the goal (`at_goal`) of a target, the target's own static
    /// record; another's gate is closed.
    ///
    /// # Errors
    ///
    /// What the gate's answer does not model, and a closed tile beside a player's objects
    /// (`FUN_14152f790`).
    fn step_open(
        &self,
        next: Tile,
        beside: Option<[Tile; 2]>,
        at_goal: bool,
        req: &LongRequest,
    ) -> GenieResult<bool> {
        let corners = |test: &dyn Fn(Tile) -> bool| {
            beside.is_none_or(|tiles| tiles.iter().all(|&tile| test(tile)))
        };
        let clear = |test: &dyn Fn(Tile) -> bool| test(next) && corners(test);
        if !clear(&|tile| self.word(tile) & self.blocked_terrain == 0) {
            return Ok(false);
        }
        let passes = || {
            corners(&|tile| self.word(tile) & self.blocked == 0)
                && self.tall(next) == 0
                && corners(&|tile| self.tall(tile) == 0)
        };
        let word = self.word(next);
        let open = if word & self.blocked == 0 {
            passes()
        } else if word & 0x3f00 == GATE_CLASS_BITS {
            // a gate's record: its owner (bits 16 to 22) an ally of the unit's player
            // (`FUN_141663dc0` with the ally flag) asks the gate on the tile's anchor (slot
            // 0x678), which opens the tile as an unblocked one; another's gate is closed
            let owner = PlayerId::from(((word >> 16) & 0x7f) as u8);
            req.allies.holds(owner) && (self.gates)(next)? && passes()
        } else {
            // the target's own tiles pass their static record
            at_goal && req.target.is_some() && passes()
        };
        if !open && req.near_player {
            return Err(GenieError::unsupported(
                "the long pather beside a player's objects (FUN_14152f790)",
            ));
        }
        Ok(open)
    }
}

/// The search's tables: the cost of each tile, the direction it was entered by this search and
/// the heap of the tiles to expand.
struct Front {
    cost: Vec<LongCost>,
    entered: Vec<Option<StepDir>>,
    heap: Heap,
}

/// The tile the search expands from: its place, its cost and the direction it was entered by.
struct Origin {
    at: Tile,
    cost: LongCost,
    came: Option<StepDir>,
}

/// What the search's steps share: the tiles, the request, the map's size and the goal.
struct Relax<'tiles> {
    grid: Grid<'tiles>,
    req: &'tiles LongRequest,
    map: GridSize,
    goal: Tile,
    rect: TileRect,
}

impl Relax<'_> {
    /// The step from `from` onto tile `to` in direction `dir`: skipped when the tile was reached
    /// as cheaply or is not open; else its cost and direction are kept and it joins the heap.
    /// Returns the tile when it is the goal.
    fn step(
        &self,
        front: &mut Front,
        from: &Origin,
        (to, dir): (Tile, StepDir),
    ) -> GenieResult<Option<Tile>> {
        let cell = cell(self.map, to);
        let new_cost = step_cost(from.cost, from.at, to, self.req.to);
        let seen = *front.entered.checked(cell)?;
        let old = *front.cost.checked(cell)?;
        if seen.is_some() && new_cost >= old {
            return Ok(None);
        }
        let at_goal = to == self.goal || self.rect.holds(to);
        if !self
            .grid
            .step_open(to, dir.beside_diagonal(to), at_goal, self.req)?
        {
            return Ok(None);
        }
        // the start's none (0xff) is no direction
        let mut long_cost = if from.came == Some(dir) {
            new_cost
        } else {
            new_cost.turned()
        };
        if at_goal {
            long_cost = LongCost::ZERO;
        }
        if seen.is_none() || long_cost < old {
            *front.cost.checked_mut(cell)? = long_cost;
            *front.entered.checked_mut(cell)? = Some(dir);
            front.heap.push(to, long_cost)?;
            if at_goal {
                return Ok(Some(to));
            }
        }
        Ok(None)
    }
}

/// `tile`'s place in the search's tables, row by row, unchecked (the game's tables are the
/// map's).
const fn cell(map: GridSize, tile: Tile) -> usize {
    (tile.y() * map.width() + tile.x()) as usize
}

/// The search of `FUN_140f9eb70` ([`reaches`], [`path`]).
fn search(
    tiles: &Tiles,
    map: GridSize,
    req: &LongRequest,
    gates: GateAsk<'_>,
    long_path_tuning: LongPathTuning,
) -> GenieResult<Searched> {
    let (width, height) = (map.width(), map.height());
    let (x0, y0) = (req.from.x(), req.from.y());
    let (x1, y1) = (req.to.x(), req.to.y());
    let (dx, dy) = (x1 - x0, y1 - y0);
    let gap2 = dy.wrapping_mul(dy).wrapping_add(dx.wrapping_mul(dx));
    if let Some(early) = search_refused(map, req, gap2, long_path_tuning) {
        return Ok(early);
    }
    // the goal: the target's tile and footprint, else the goal tile
    let (goal, rect) = req.target.map_or_else(
        || (req.to, TileRect::of(req.to)),
        |long_target| (long_target.tile, long_target.rect),
    );
    let cells = (width.max(0) * height.max(0)) as usize;
    let relax = Relax {
        grid: Grid::new(tiles, map, req, gates),
        req,
        map,
        goal,
        rect,
    };
    // a tile's cost (`+0x38420`) and, entered this search, the direction it was entered by
    // (`+0x20`, under the search's generation `+0x2db430`: an older one reads as never entered)
    let mut front = Front {
        cost: vec![LongCost::ZERO; cells],
        entered: vec![None; cells],
        heap: Heap { slots: Vec::new() },
    };
    let start_cost = LongCost::of_gap(dx, dy);
    *front.cost.checked_mut(cell(map, req.from))? = start_cost;
    front.heap.slots = vec![(Tile::new(0, 0), SENTINEL), (req.from, start_cost)];
    let straight = sqrt(gap2 as f32) as i32;
    let budget = if req.player_kind.is_computer() {
        straight.wrapping_mul(COMPUTER_FACTOR)
    } else {
        straight.wrapping_mul(HUMAN_PERCENT) / 100
    };
    // the popped tile nearest the goal (`local_52c`, its distance `local_4f8`: the start's own
    // in quarter tiles squared, a popped tile's a quarter of that)
    let mut nearest = (req.from, start_cost);
    let mut expansions = 0;
    while 0 < front.heap.count() {
        if budget < expansions || MOST_EXPANSIONS < expansions {
            break;
        }
        let (current, _) = *front.heap.slots.checked(1)?;
        let (cx, cy) = (current.x(), current.y());
        let (gx, gy) = (x1 - cx, y1 - cy);
        let distance = sqrt(gx.wrapping_mul(gx).wrapping_add(gy.wrapping_mul(gy)) as f32);
        if rect.holds(current) || search_reached(req, distance, long_path_tuning) {
            return Ok(Searched::Ran {
                reached: true,
                end: current,
                entered: front.entered,
            });
        }
        let near = LongCost::of_gap(x1 - cx, y1 - cy).quartered();
        if near < nearest.1 {
            nearest = (current, near);
        }
        front.heap.pop()?;
        let came = *front.entered.checked(cell(map, current))?;
        let back = StepDir::behind(came);
        let here = *front.cost.checked(cell(map, current))?;
        for (dir, step) in StepDir::ALL.into_iter().zip(STEPS) {
            let tile = current + step;
            if back.holds(dir) || !map.holds(tile) || tile == req.from {
                continue;
            }
            let from = Origin {
                at: current,
                cost: here,
                came,
            };
            if let Some(end) = relax.step(&mut front, &from, (tile, dir))? {
                return Ok(Searched::Ran {
                    reached: true,
                    end,
                    entered: front.entered,
                });
            }
        }
        expansions += 1;
    }
    // the nearest tile goes on the heap with cost 0, so the path runs to it
    Ok(Searched::Ran {
        reached: false,
        end: nearest.0,
        entered: front.entered,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::TerrainMask;
    use crate::testing::tuning;
    #[cfg(test)]
    use std::cell::RefCell;

    /// A map of `rows` ('#' a blocked tile, 'T' a tall object, 'G' player 4's gate record, '.'
    /// open), class 1 everywhere.
    fn tiles(rows: &[&str]) -> (Tiles, GridSize) {
        let (width, height) = (rows[0].len() as i32, rows.len() as i32);
        let mut classes = Vec::new();
        for text in rows {
            for character in text.chars() {
                classes.push(if character == '#' { 2 } else { 1 });
            }
        }
        let mut tiles = Tiles::new(GridSize::new(width, height), classes);
        for (y, text) in rows.iter().enumerate() {
            for (x, character) in text.chars().enumerate() {
                if character == 'T' {
                    tiles.add_tall_for_test(Tile::new(x as i32, y as i32));
                }
                if character == 'G' {
                    tiles.mark_for_test(
                        Tile::new(x as i32, y as i32),
                        BLOCKED | GATE_CLASS_BITS | 4 << 16,
                    );
                }
            }
        }
        (tiles, GridSize::new(width, height))
    }

    fn request(from: [i32; 2], to: [i32; 2]) -> LongRequest {
        LongRequest {
            from: Tile::from(from),
            to: Tile::from(to),
            range: Distance::new(3.0),
            target: None,
            moves: true,
            mask: TerrainMask::of_bits(1),
            player_kind: PlayerKind::Human,
            lowered: Vec::new(),
            within_range: false,
            near_player: false,
            allies: PlayerSet::of(PlayerId::new(1)),
        }
    }

    #[test]
    fn the_path_runs_from_the_start_over_its_turns_to_the_goal() {
        // around the wall: from (1, 2) up past its end and down to (8, 2); each point a tile's
        // centre at the unit's height, where the direction changes
        let (grid, map) = tiles(&[
            "..........",
            ".....#....",
            ".....#....",
            ".....#....",
            "..........",
        ]);
        let (reached, walk) = path(
            &grid,
            map,
            (&request([1, 2], [8, 2]), &|_| Ok(false)),
            0.25,
            tuning().long_path,
        )
        .unwrap();
        assert!(reached);
        let first = walk.points[0];
        let last = *walk.points.last().unwrap();
        assert_eq!(
            (first, last),
            (BVector::new(1.5, 0.25, 2.5), BVector::new(8.5, 0.25, 2.5))
        );
        // every stretch between two points is a straight or diagonal run of tiles
        for window in walk.points.windows(2) {
            let (dx, dy) = (window[1].x() - window[0].x(), window[1].y() - window[0].y());
            assert!(dx == 0.0 || dy == 0.0 || dx.abs() == dy.abs(), "{window:?}");
        }
        assert!(
            walk.points
                .iter()
                .all(|point| point.y() != 2.5 || point.x() < 5.0 || 6.0 < point.x())
        );
        // behind a closed wall the search runs out: the path to the tile nearest the goal
        let (tiles, map) = tiles(&[
            ".....#....",
            ".....#....",
            ".....#....",
            ".....#....",
            ".....#....",
        ]);
        let (reached, walk) = path(
            &tiles,
            map,
            (&request([1, 2], [8, 2]), &|_| Ok(false)),
            0.0,
            tuning().long_path,
        )
        .unwrap();
        assert!(!reached);
        assert_eq!(walk.points.first(), Some(&BVector::new(1.5, 0.0, 2.5)));
        assert_eq!(walk.points.last(), Some(&BVector::new(4.5, 0.0, 2.5)));
        // a goal within the range: yes, and no path
        let mut req = request([0, 1], [4, 1]);
        req.range = Distance::new(4.5);
        assert_eq!(
            path(&tiles, map, (&req, &|_| Ok(false)), 0.0, tuning().long_path).unwrap(),
            (true, Path::default())
        );
    }

    #[test]
    fn trace_puts_point_where_the_direction_changes() {
        // entered: (1, 0) from (0, 0) by step 3 ([1, 0]), (2, 0) by 3, (3, 1) by 4 ([1, 1])
        let width = 4;
        let mut entered = vec![None; 8];
        entered[1] = Some(StepDir::ALL[3]);
        entered[2] = Some(StepDir::ALL[3]);
        entered[width + 3] = Some(StepDir::ALL[4]);
        let path = trace(
            Tile::new(0, 0),
            Tile::new(3, 1),
            &entered,
            GridSize::new(width as i32, 2),
            0.0,
        )
        .unwrap();
        assert_eq!(
            path.points,
            [
                BVector::new(0.5, 0.0, 0.5),
                BVector::new(2.5, 0.0, 0.5),
                BVector::new(3.5, 0.0, 1.5)
            ]
        );
        // the end on the start: its centre only
        let path = trace(
            Tile::new(0, 0),
            Tile::new(0, 0),
            &entered,
            GridSize::new(width as i32, 2),
            0.0,
        )
        .unwrap();
        assert_eq!(path.points, [BVector::new(0.5, 0.0, 0.5)]);
    }

    #[test]
    fn goal_around_wall_is_reached_one_behind_closed_wall_not() {
        let (grid, map) = tiles(&[
            "..........",
            ".....#....",
            ".....#....",
            ".....#....",
            "..........",
        ]);
        assert_eq!(
            reaches(
                &grid,
                map,
                &request([1, 2], [8, 2]),
                &|_| Ok(false),
                tuning().long_path
            ),
            Ok(true)
        );
        let (tiles, map) = tiles(&[
            ".....#....",
            ".....#....",
            ".....#....",
            ".....#....",
            ".....#....",
        ]);
        assert_eq!(
            reaches(
                &tiles,
                map,
                &request([1, 2], [8, 2]),
                &|_| Ok(false),
                tuning().long_path
            ),
            Ok(false)
        );
    }

    /// The 4v4's scout 27272 (player 1) in update 4627 tests a goal across the wall of its enemy,
    /// player 4, whose gate's tiles hold their record: the gate is closed to it, unasked. An
    /// ally's gate asks the gate on its tile (slot 0x678): a no keeps it closed, a yes opens it
    /// as an unblocked tile, and what the gate cannot answer stops the search.
    #[test]
    fn an_allied_gate_asks_the_gate_and_an_enemy_is_closed() {
        let (tiles, map) = tiles(&[
            ".....#....",
            ".....#....",
            ".....G....",
            ".....#....",
            ".....#....",
        ]);
        let asked = RefCell::new(Vec::new());
        let asked = &asked;
        let ask = |answer: bool| {
            move |tile| {
                asked.borrow_mut().push(tile);
                Ok(answer)
            }
        };
        assert_eq!(
            reaches(
                &tiles,
                map,
                &request([1, 2], [8, 2]),
                &ask(true),
                tuning().long_path
            ),
            Ok(false)
        );
        assert_eq!(asked.borrow().len(), 0);
        let mut req = request([1, 2], [8, 2]);
        req.allies |= PlayerSet::of(PlayerId::new(4));
        assert_eq!(
            reaches(&tiles, map, &req, &ask(false), tuning().long_path),
            Ok(false)
        );
        assert!(asked.borrow().iter().all(|&tile| tile == Tile::new(5, 2)));
        assert_eq!(
            reaches(&tiles, map, &req, &ask(true), tuning().long_path),
            Ok(true)
        );
        let fails = |_| Err(GenieError::unreachable("the gate's master"));
        assert!(reaches(&tiles, map, &req, &fails, tuning().long_path).is_err());
    }

    #[test]
    fn diagonal_step_needs_both_tiles_beside_it() {
        // the only way through is a corner between two tall objects
        let (tiles, map) = tiles(&[
            "....T....",
            "....T....",
            "....T....",
            "...T.....",
            "...T.....",
        ]);
        assert_eq!(
            reaches(
                &tiles,
                map,
                &request([0, 0], [8, 4]),
                &|_| Ok(false),
                tuning().long_path
            ),
            Ok(false)
        );
    }

    #[test]
    fn goal_within_the_range_ends_before_the_search() {
        let (tiles, map) = tiles(&["..#..", "..#..", "..#.."]);
        let mut req = request([0, 1], [4, 1]);
        req.moves = false;
        assert_eq!(
            reaches(&tiles, map, &req, &|_| Ok(false), tuning().long_path),
            Ok(false)
        );
        req.range = Distance::new(4.5);
        assert_eq!(
            reaches(&tiles, map, &req, &|_| Ok(false), tuning().long_path),
            Ok(true)
        );
    }
}
