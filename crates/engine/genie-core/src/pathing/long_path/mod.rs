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

use crate::GenieError;
use crate::map::Distance;
use crate::map::DistanceSquared;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Position;
use crate::map::spatial::{BLOCKED, Tiles};
use crate::map::{Tile, TileOffset, TileRect};
use crate::math::fmath::BVector;
use crate::pathing::tile_path::Path;
use crate::players::PlayerId;
use crate::world::slots::CheckedIndex;

/// A heap pop finds the heap empty where the caller checked its top.
const EMPTY_HEAP: GenieError = GenieError::unreachable("a heap pop without a top");

/// The search expands at most this many tiles (`FUN_140f9eb70`: `20000 < n` breaks).
const MOST_EXPANSIONS: i32 = 20000;

/// A human player's search expands at most this many hundredths of the straight distance in
/// tiles (`DAT_143f0f358`, which the static initializer at `0x140167e90` sets to 7500, or to 10000
/// with `DAT_143f01832`, which no run of this build has set).
const HUMAN_PERCENT: i32 = 7500;

/// A computer player's search expands at most this many tiles per tile of straight distance
/// (`FUN_140f9eb70` after `FUN_141659cf0`).
const COMPUTER_FACTOR: i32 = 25;

/// The heap's slot 0 (`+0x119424`), below every cost: a sift up stops there.
const SENTINEL: i32 = 0x88ca_6c00_u32 as i32;

/// The goal counts as reached within this many tiles, and a range above it ends the search
/// before it starts when the goal lies within the range (`DAT_14330eb8c`).
const REACHED: f32 = crate::math::image::long_path::REACHED;

/// The margin of the reached test within the range (`DAT_142eb0c60`).
const EPSILON: f32 = crate::math::image::long_path::EPSILON;

/// A target's footprint ends this short of its far edges (`DAT_142eb99cc`).
const EDGE_GAP: f32 = crate::math::image::long_path::EDGE_GAP;

/// A path point lies at its tile's centre: the tile plus this on both axes (`DAT_142e7f384`).
const TILE_CENTRE: f32 = crate::math::image::long_path::TILE_CENTRE;

/// The record class whose tiles an allied unit may pass when its gate lets it (`0x500` in the
/// obstruction manager's word, bits 8 to 13).
const GATE_CLASS_BITS: u64 = 0x500;

/// The tiles one step away in the order the search tries them (the switch of `FUN_140f9eb70`).
const STEPS: [TileOffset; 8] = TileOffset::table([
    [-1, -1],
    [0, -1],
    [1, -1],
    [1, 0],
    [1, 1],
    [0, 1],
    [-1, 1],
    [-1, 0],
]);

/// A step's direction (`FUN_140f9eb70`'s switch): 0 to 7, the place of its tile in [`STEPS`],
/// the order the search tries the eight tiles around one; the even ones are diagonal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StepDir(u8);

impl StepDir {
    /// The eight in the order the search tries them.
    const ALL: [Self; 8] = [
        Self(0),
        Self(1),
        Self(2),
        Self(3),
        Self(4),
        Self(5),
        Self(6),
        Self(7),
    ];

    /// The step's tile offset ([`STEPS`]).
    fn offset(self) -> Option<TileOffset> {
        STEPS.get(usize::from(self.0)).copied()
    }

    /// The directions a tile entered by a step in direction `came` does not try (the table at
    /// `DAT_142f43a28`: the three that turn back, bits `came + 3` to `came + 5`; none for the
    /// start, the game's `came` 8), as a bit each.
    fn behind(came: Option<Self>) -> u8 {
        came.map_or(0, |c| (0x38u8).rotate_left(u32::from(c.0)))
    }

    /// Whether `back` ([`StepDir::behind`]) holds this direction.
    const fn in_set(self, back: u8) -> bool {
        back & (1 << self.0) != 0
    }

    /// The tiles beside a diagonal step (directions 0, 2, 4, 6) onto `n`, which the step needs
    /// as clear as the tile.
    const fn beside_diagonal(self, n: Tile) -> Option<[Tile; 2]> {
        let (nx, ny) = (n.x(), n.y());
        match self.0 {
            0 => Some([Tile::new(nx + 1, ny), Tile::new(nx, ny + 1)]),
            2 => Some([Tile::new(nx, ny + 1), Tile::new(nx - 1, ny)]),
            4 => Some([Tile::new(nx - 1, ny), Tile::new(nx, ny - 1)]),
            6 => Some([Tile::new(nx, ny - 1), Tile::new(nx + 1, ny)]),
            _ => None,
        }
    }
}

/// The target object of a search (`FUN_140f9eb70`'s argument 8).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LongTarget {
    /// Its tile.
    pub tile: Tile,
    /// The tiles of its footprint (the centre less the radius, the centre plus the radius less
    /// `EDGE_GAP`, whole).
    pub rect: TileRect,
    /// The unit's footprint gap to it, squared (`FUN_14118f8c0`) and not (`FUN_14118f680`).
    pub gap_squared: DistanceSquared,
    /// The gap, not squared (`FUN_14118f680`).
    pub gap: Distance,
}

impl LongTarget {
    /// The target at `position` with radii `radius`, `gap` and `gap_squared` from the unit.
    #[must_use]
    pub fn new(
        position: Position,
        radius: Footprint,
        gap: Distance,
        gap_squared: DistanceSquared,
    ) -> Self {
        let (x, y) = (position.x(), position.y());
        Self {
            tile: Tile::new(x as i32, y as i32),
            rect: TileRect::new(
                (x - radius.x()) as i32,
                (y - radius.y()) as i32,
                ((x + radius.x()) - EDGE_GAP) as i32,
                ((y + radius.y()) - EDGE_GAP) as i32,
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
    pub from: Tile,
    /// The goal tile.
    pub to: Tile,
    /// How near counts as there.
    pub range: Distance,
    /// The target object, None for a point.
    pub target: Option<LongTarget>,
    /// The unit moves: its speed (slot 0x208, `FUN_141202c70`) is not 0, or its master is 42.
    pub moves: bool,
    /// The terrain classes the unit may stand on (its restriction's mask).
    pub mask: crate::map::TerrainMask,
    /// The owner is a computer player (`FUN_141659cf0`): a shorter budget.
    pub computer: bool,
    /// The tiles of the ignored units the search lowers the tall count of, once each
    /// (`FUN_140f60440`: the unit itself and its moving ignored units, not inside another, whose
    /// master has a height).
    pub lowered: Vec<Tile>,
    /// Argument 13: the goal counts as reached within the range (else within a tile).
    pub within_range: bool,
    /// Argument 11 is not −1: a blocked tile with no object of the player around counts as open.
    pub near_player: bool,
    /// The players the unit's player counts as allies (`FUN_141663dc0` with the ally flag): a
    /// gate's tile of another player is closed.
    pub allies: crate::players::PlayerSet,
}

/// The open tiles: a binary heap from slot 1, slot 0 the sentinel (`+0x119420`, count
/// `+0x2db428`).
struct Heap {
    slots: Vec<(Tile, i32)>,
}

impl Heap {
    const fn count(&self) -> i32 {
        self.slots.len() as i32 - 1
    }

    /// `FUN_140fa0bd0` on a slot just filled: it rises while its cost is at most its parent's.
    fn push(&mut self, at: Tile, cost: i32) -> Result<(), GenieError> {
        self.slots.push((at, cost));
        let mut i = self.slots.len() - 1;
        let mut parent = i >> 1;
        if self.slots.checked(parent)?.1 < cost {
            return Ok(());
        }
        loop {
            *self.slots.checked_mut(i)? = *self.slots.checked_mut(parent)?;
            i /= 2;
            parent /= 2;
            if self.slots.checked(parent)?.1 < cost {
                break;
            }
        }
        *self.slots.checked_mut(i)? = (at, cost);
        Ok(())
    }

    /// The pop after the top's expansion check (`FUN_140f9eb70`): the last slot moves to the top
    /// and sinks, the game's way: only while the count's half is above 1, only to a child below
    /// that half, past a child whose cost is not above it.
    fn pop(&mut self) -> Result<(), GenieError> {
        let last = self.slots.pop().ok_or(EMPTY_HEAP)?;
        let count = self.count();
        if count < 1 {
            return Ok(());
        }
        *self.slots.checked_mut(1)? = last;
        let half = count / 2;
        if half <= 1 {
            return Ok(());
        }
        let mut i = 1usize;
        loop {
            let mut child = 2 * i;
            if (child as i32) < count
                && self.slots.checked(child + 1)?.1 < self.slots.checked(child)?.1
            {
                child += 1;
            }
            if last.1 < self.slots.checked(child)?.1 {
                break;
            }
            *self.slots.checked_mut(i)? = *self.slots.checked_mut(child)?;
            i = child;
            if half <= child as i32 {
                break;
            }
        }
        if i != 1 {
            *self.slots.checked_mut(i)? = last;
        }
        Ok(())
    }
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
pub fn reaches(tiles: &Tiles, map: GridSize, req: &LongRequest) -> Result<bool, GenieError> {
    Ok(match search(tiles, map, req)? {
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
    req: &LongRequest,
    z: f32,
) -> Result<(bool, Path), GenieError> {
    Ok(match search(tiles, map, req)? {
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
    let direction = |t: Tile| entered.get(cell(map, t)).copied().flatten();
    let centre = |t: Tile| -> BVector {
        BVector::new(t.x() as f32 + TILE_CENTRE, z, t.y() as f32 + TILE_CENTRE)
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
        at = at.stepped_back(step);
        before = came;
        came = direction(at);
    }
    path.insert(0, centre(at)).then_some(path)
}

/// [`search`]'s answers before the search: an end off the pather's width (`+0x8`, the far edge
/// included) is refused; a target within range, or without a target a goal within a range
/// above [`REACHED`] (`gap2` the ends' squared gap), is in range; a unit that does not move is
/// refused. None: the search runs.
fn search_refused(map: GridSize, req: &LongRequest, gap2: i32) -> Option<Searched> {
    let (w, _) = (map.width(), map.height());
    let held = |v: i32| 0 <= v && v <= w;
    let (x0, y0) = (req.from.x(), req.from.y());
    let (x1, y1) = (req.to.x(), req.to.y());
    if !(held(x0) && held(y0) && held(x1) && held(y1)) {
        return Some(Searched::Refused);
    }
    if req
        .target
        .is_some_and(|t| t.gap_squared <= req.range.squared())
    {
        return Some(Searched::InRange);
    }
    if REACHED < req.range.get()
        && req.target.is_none()
        && (gap2 as f32) < req.range.squared().get()
    {
        return Some(Searched::InRange);
    }
    (!req.moves).then_some(Searched::Refused)
}

/// [`search`]'s goal test of the popped tile `d` tiles from the goal: a target's gap within the
/// range ([`EPSILON`]) or the tile within [`REACHED`]; without a target, a search to within
/// range by `d` against the range, else `d` within [`REACHED`].
fn search_reached(req: &LongRequest, d: f32) -> bool {
    match req.target {
        Some(t) => t.gap - req.range < Distance::new(EPSILON) || d < REACHED,
        None if req.within_range => d - req.range.get() < EPSILON,
        None => d < REACHED,
    }
}

/// [`search`]'s cost of the step from `from` (its cost `here`) to `to`: the step's length in
/// quarter tiles squared over 4, and the estimate to the goal (`goal`, quarter tiles squared
/// per axis) moved from `from`'s to `to`'s, in the game's wrapping arithmetic.
const fn step_cost(here: i32, from: Tile, to: Tile, goal: Tile) -> i32 {
    const fn quad(a: i32) -> i32 {
        (a * 4).wrapping_mul(a * 4)
    }
    let walked = quad(to.y() - from.y()).wrapping_add(quad(to.x() - from.x())) >> 2;
    walked
        .wrapping_add(here)
        .wrapping_sub(quad(goal.y() - from.y()))
        .wrapping_sub(quad(goal.x() - from.x()))
        .wrapping_add(quad(goal.y() - to.y()))
        .wrapping_add(quad(goal.x() - to.x()))
}

/// What [`search`] reads of the tiles: the obstruction words, the tall objects less those the
/// request lowers (`lowered`, one each), and the unit's blocked terrain classes with and
/// without [`BLOCKED`].
struct Grid<'a> {
    tiles: &'a Tiles,
    map: GridSize,
    lowered: Vec<u8>,
    blocked_terrain: u64,
    blocked: u64,
}

impl<'a> Grid<'a> {
    fn new(tiles: &'a Tiles, map: GridSize, req: &LongRequest) -> Self {
        let blocked_terrain = req.mask.blocked();
        let mut lowered = vec![0u8; (map.width().max(0) * map.height().max(0)) as usize];
        for &t in &req.lowered {
            if let Some(cell) = map.index(t).and_then(|i| lowered.get_mut(i)) {
                *cell = cell.wrapping_add(1);
            }
        }
        Self {
            tiles,
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
        let Some(t) = self.tiles.tall(tile) else {
            return 0;
        };
        // the tiles hold the tile, so the map does
        t.wrapping_sub(
            self.map
                .index(tile)
                .and_then(|i| self.lowered.get(i))
                .copied()
                .unwrap_or(0),
        )
    }

    /// The step onto `n` (with the tiles `beside` a diagonal): its terrain and the beside tiles'
    /// passable, else it is skipped; then open when the tile, the beside tiles and their tall
    /// objects are clear, or, at the goal (`at_goal`) of a target, the target's own static
    /// record; another's gate is closed.
    ///
    /// # Errors
    ///
    /// An allied gate's tile (slot 0x678) and a closed tile beside a player's objects
    /// (`FUN_14152f790`).
    fn step_open(
        &self,
        n: Tile,
        beside: Option<[Tile; 2]>,
        at_goal: bool,
        req: &LongRequest,
    ) -> Result<bool, GenieError> {
        let corners =
            |test: &dyn Fn(Tile) -> bool| beside.is_none_or(|b| b.iter().all(|&t| test(t)));
        let clear = |test: &dyn Fn(Tile) -> bool| test(n) && corners(test);
        if !clear(&|t| self.word(t) & self.blocked_terrain == 0) {
            return Ok(false);
        }
        let passes = || {
            corners(&|t| self.word(t) & self.blocked == 0)
                && self.tall(n) == 0
                && corners(&|t| self.tall(t) == 0)
        };
        let word = self.word(n);
        let open = if word & self.blocked == 0 {
            passes()
        } else if word & 0x3f00 == GATE_CLASS_BITS {
            // a gate's record: its owner (bits 16 to 22) an ally of the unit's player
            // (`FUN_141663dc0` with the ally flag) asks the gate on the tile's anchor (slot
            // 0x678); another's gate is closed
            let owner = PlayerId::from(((word >> 16) & 0x7f) as u8);
            if req.allies.holds(owner) {
                return Err(GenieError::unsupported(
                    "the long pather at an allied gate's tile (slot 0x678)",
                ));
            }
            false
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
    cost: Vec<i32>,
    entered: Vec<Option<StepDir>>,
    heap: Heap,
}

/// The tile the search expands from: its place, its cost and the direction it was entered by.
struct Origin {
    at: Tile,
    cost: i32,
    came: Option<StepDir>,
}

/// What the search's steps share: the tiles, the request, the map's size and the goal.
struct Relax<'a> {
    grid: Grid<'a>,
    req: &'a LongRequest,
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
    ) -> Result<Option<Tile>, GenieError> {
        let i = cell(self.map, to);
        let new_cost = step_cost(from.cost, from.at, to, self.req.to);
        let seen = *front.entered.checked(i)?;
        let old = *front.cost.checked(i)?;
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
        let mut c = if from.came == Some(dir) {
            new_cost
        } else {
            new_cost.wrapping_add(1)
        };
        if at_goal {
            c = 0;
        }
        if seen.is_none() || c < old {
            *front.cost.checked_mut(i)? = c;
            *front.entered.checked_mut(i)? = Some(dir);
            front.heap.push(to, c)?;
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
fn search(tiles: &Tiles, map: GridSize, req: &LongRequest) -> Result<Searched, GenieError> {
    let (w, h) = (map.width(), map.height());
    let (x0, y0) = (req.from.x(), req.from.y());
    let (x1, y1) = (req.to.x(), req.to.y());
    let (dx, dy) = (x1 - x0, y1 - y0);
    let gap2 = dy.wrapping_mul(dy).wrapping_add(dx.wrapping_mul(dx));
    if let Some(early) = search_refused(map, req, gap2) {
        return Ok(early);
    }
    // the goal: the target's tile and footprint, else the goal tile
    let (goal, rect) = req
        .target
        .map_or_else(|| (req.to, TileRect::of(req.to)), |t| (t.tile, t.rect));
    let n = (w.max(0) * h.max(0)) as usize;
    let relax = Relax {
        grid: Grid::new(tiles, map, req),
        req,
        map,
        goal,
        rect,
    };
    let quad = |a: i32| (a * 4).wrapping_mul(a * 4);
    // a tile's cost (`+0x38420`) and, entered this search, the direction it was entered by
    // (`+0x20`, under the search's generation `+0x2db430`: an older one reads as never entered)
    let mut front = Front {
        cost: vec![0i32; n],
        entered: vec![None; n],
        heap: Heap { slots: Vec::new() },
    };
    let start_cost = quad(dy).wrapping_add(quad(dx));
    *front.cost.checked_mut(cell(map, req.from))? = start_cost;
    front.heap.slots = vec![(Tile::new(0, 0), SENTINEL), (req.from, start_cost)];
    let straight = crate::math::fmath::sqrt(gap2 as f32) as i32;
    let budget = if req.computer {
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
        let (c, _) = *front.heap.slots.checked(1)?;
        let (cx, cy) = (c.x(), c.y());
        let (gx, gy) = (x1 - cx, y1 - cy);
        let d =
            crate::math::fmath::sqrt(gx.wrapping_mul(gx).wrapping_add(gy.wrapping_mul(gy)) as f32);
        if rect.holds(c) || search_reached(req, d) {
            return Ok(Searched::Ran {
                reached: true,
                end: c,
                entered: front.entered,
            });
        }
        let near = quad(y1 - cy).wrapping_add(quad(x1 - cx)) >> 2;
        if near < nearest.1 {
            nearest = (c, near);
        }
        front.heap.pop()?;
        let came = *front.entered.checked(cell(map, c))?;
        let back = StepDir::behind(came);
        let here = *front.cost.checked(cell(map, c))?;
        for (dir, step) in StepDir::ALL.into_iter().zip(STEPS) {
            let n = c.stepped(step);
            if dir.in_set(back) || !map.holds(n) || n == req.from {
                continue;
            }
            let from = Origin {
                at: c,
                cost: here,
                came,
            };
            if let Some(end) = relax.step(&mut front, &from, (n, dir))? {
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

    /// A map of `rows` ('#' a blocked tile, 'T' a tall object, 'G' player 4's gate record, '.'
    /// open), class 1 everywhere.
    fn tiles(rows: &[&str]) -> (Tiles, GridSize) {
        let (w, h) = (rows[0].len() as i32, rows.len() as i32);
        let mut classes = Vec::new();
        for r in rows {
            for c in r.chars() {
                classes.push(if c == '#' { 2 } else { 1 });
            }
        }
        let mut t = Tiles::new(GridSize::new(w, h), classes);
        for (y, r) in rows.iter().enumerate() {
            for (x, c) in r.chars().enumerate() {
                if c == 'T' {
                    t.add_tall_for_test(Tile::new(x as i32, y as i32));
                }
                if c == 'G' {
                    t.mark_for_test(
                        Tile::new(x as i32, y as i32),
                        BLOCKED | GATE_CLASS_BITS | 4 << 16,
                    );
                }
            }
        }
        (t, GridSize::new(w, h))
    }

    fn request(from: [i32; 2], to: [i32; 2]) -> LongRequest {
        LongRequest {
            from: Tile::from(from),
            to: Tile::from(to),
            range: Distance::new(3.0),
            target: None,
            moves: true,
            mask: TerrainMask::of_bits(1),
            computer: false,
            lowered: Vec::new(),
            within_range: false,
            near_player: false,
            allies: crate::players::PlayerSet::of(PlayerId::new(1)),
        }
    }

    #[test]
    fn the_path_runs_from_the_start_over_its_turns_to_the_goal() {
        // around the wall: from (1, 2) up past its end and down to (8, 2); each point a tile's
        // centre at the unit's height, where the direction changes
        let (t, map) = tiles(&[
            "..........",
            ".....#....",
            ".....#....",
            ".....#....",
            "..........",
        ]);
        let (reached, walk) = path(&t, map, &request([1, 2], [8, 2]), 0.25).unwrap();
        assert!(reached);
        let first = walk.points[0];
        let last = *walk.points.last().unwrap();
        assert_eq!(
            (first, last),
            (BVector::new(1.5, 0.25, 2.5), BVector::new(8.5, 0.25, 2.5))
        );
        // every stretch between two points is a straight or diagonal run of tiles
        for w in walk.points.windows(2) {
            let (dx, dy) = (w[1].x() - w[0].x(), w[1].y() - w[0].y());
            assert!(dx == 0.0 || dy == 0.0 || dx.abs() == dy.abs(), "{w:?}");
        }
        assert!(
            walk.points
                .iter()
                .all(|p| p.y() != 2.5 || p.x() < 5.0 || 6.0 < p.x())
        );
        // behind a closed wall the search runs out: the path to the tile nearest the goal
        let (t, map) = tiles(&[
            ".....#....",
            ".....#....",
            ".....#....",
            ".....#....",
            ".....#....",
        ]);
        let (reached, walk) = path(&t, map, &request([1, 2], [8, 2]), 0.0).unwrap();
        assert!(!reached);
        assert_eq!(walk.points.first(), Some(&BVector::new(1.5, 0.0, 2.5)));
        assert_eq!(walk.points.last(), Some(&BVector::new(4.5, 0.0, 2.5)));
        // a goal within the range: yes, and no path
        let mut req = request([0, 1], [4, 1]);
        req.range = Distance::new(4.5);
        assert_eq!(path(&t, map, &req, 0.0).unwrap(), (true, Path::default()));
    }

    #[test]
    fn a_trace_puts_a_point_where_the_direction_changes() {
        // entered: (1, 0) from (0, 0) by step 3 ([1, 0]), (2, 0) by 3, (3, 1) by 4 ([1, 1])
        let w = 4;
        let mut entered = vec![None; 8];
        entered[1] = Some(StepDir(3));
        entered[2] = Some(StepDir(3));
        entered[w + 3] = Some(StepDir(4));
        let p = trace(
            Tile::new(0, 0),
            Tile::new(3, 1),
            &entered,
            GridSize::new(w as i32, 2),
            0.0,
        )
        .unwrap();
        assert_eq!(
            p.points,
            [
                BVector::new(0.5, 0.0, 0.5),
                BVector::new(2.5, 0.0, 0.5),
                BVector::new(3.5, 0.0, 1.5)
            ]
        );
        // the end on the start: its centre only
        let p = trace(
            Tile::new(0, 0),
            Tile::new(0, 0),
            &entered,
            GridSize::new(w as i32, 2),
            0.0,
        )
        .unwrap();
        assert_eq!(p.points, [BVector::new(0.5, 0.0, 0.5)]);
    }

    /// The three directions that turn back from each (`DAT_142f43a28`), none from the start.
    #[test]
    fn the_table_of_turning_back() {
        let table: Vec<u8> = StepDir::ALL
            .into_iter()
            .map(Some)
            .chain([None])
            .map(StepDir::behind)
            .collect();
        assert_eq!(
            table,
            [0x38, 0x70, 0xe0, 0xc1, 0x83, 0x07, 0x0e, 0x1c, 0x00]
        );
        assert!(StepDir(3).in_set(0x38) && !StepDir(2).in_set(0x38));
    }

    /// The even directions are diagonal and need the two tiles beside them; each direction's
    /// tile is its place in the table.
    #[test]
    fn a_diagonal_step_needs_the_tiles_beside_it() {
        let n = Tile::new(5, 5);
        assert_eq!(
            StepDir(0).beside_diagonal(n),
            Some([Tile::new(6, 5), Tile::new(5, 6)])
        );
        assert_eq!(StepDir(1).beside_diagonal(n), None);
        assert_eq!(StepDir::ALL.map(StepDir::offset), STEPS.map(Some));
    }

    #[test]
    fn a_goal_around_a_wall_is_reached_one_behind_a_closed_wall_not() {
        let (t, map) = tiles(&[
            "..........",
            ".....#....",
            ".....#....",
            ".....#....",
            "..........",
        ]);
        assert_eq!(reaches(&t, map, &request([1, 2], [8, 2])), Ok(true));
        let (t, map) = tiles(&[
            ".....#....",
            ".....#....",
            ".....#....",
            ".....#....",
            ".....#....",
        ]);
        assert_eq!(reaches(&t, map, &request([1, 2], [8, 2])), Ok(false));
    }

    /// The 4v4's scout 27272 (player 1) in update 4627 tests a goal across the wall of its enemy,
    /// player 4, whose gate's tiles hold their record: the gate is closed to it; an ally's gate
    /// asks the gate (slot 0x678, not modelled).
    #[test]
    fn an_enemy_gate_is_closed() {
        let (t, map) = tiles(&[
            ".....#....",
            ".....#....",
            ".....G....",
            ".....#....",
            ".....#....",
        ]);
        assert_eq!(reaches(&t, map, &request([1, 2], [8, 2])), Ok(false));
        let mut req = request([1, 2], [8, 2]);
        req.allies |= crate::players::PlayerSet::of(PlayerId::new(4));
        assert!(reaches(&t, map, &req).is_err());
    }

    #[test]
    fn a_diagonal_step_needs_both_tiles_beside_it() {
        // the only way through is a corner between two tall objects
        let (t, map) = tiles(&[
            "....T....",
            "....T....",
            "....T....",
            "...T.....",
            "...T.....",
        ]);
        assert_eq!(reaches(&t, map, &request([0, 0], [8, 4])), Ok(false));
    }

    #[test]
    fn a_goal_within_the_range_ends_before_the_search() {
        let (t, map) = tiles(&["..#..", "..#..", "..#.."]);
        let mut req = request([0, 1], [4, 1]);
        req.moves = false;
        assert_eq!(reaches(&t, map, &req), Ok(false));
        req.range = Distance::new(4.5);
        assert_eq!(reaches(&t, map, &req), Ok(true));
    }

    #[test]
    fn the_heap_sinks_the_games_way() {
        let mut h = Heap {
            slots: vec![(Tile::new(0, 0), SENTINEL)],
        };
        for (i, c) in [5, 3, 8, 1, 9, 2].into_iter().enumerate() {
            h.push(Tile::new(i as i32, 0), c).unwrap();
        }
        assert_eq!(h.slots[1].1, 1);
        h.pop().unwrap();
        // count 5, half 2: no sinking past the top's children below 2
        let costs: Vec<i32> = h.slots[1..].iter().map(|s| s.1).collect();
        assert_eq!(costs, [2, 3, 8, 5, 9]);
    }
}
