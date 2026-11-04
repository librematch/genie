//! The explorer's search for an unexplored tile (`FUN_140b71990`, "closestUnexploredTile") and
//! its helpers on the information module: the path map's reads and the stack of points to try.
//!
//! The path map holds a byte per tile: [`UNEXPLORED`] (-1, the module's start), [`EXPLORED`]
//! (the explored-tile hook's 0), [`PUNTED`] (a tile its unit cannot path to) and [`DEAD_END`]
//! (a tile no search reached). A civilian takes a tile no search touched, any other unit one that
//! is not explored.
//!
//! The search floods from a tile over the map: the first tile that the player does not see now
//! and that counts takes the unit's pick, when the unit reaches it. A tile it cannot reach is
//! punted, and the search then starts again from the unit's own place; a search that already
//! starts there picks a corner of a quadrant by the scores the save holds and draws from the
//! unsynced stream.

use crate::map::Distance;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::Tile;
use crate::math::conversions::cvttss2si;
use crate::math::fmath::{cos, sin};
use crate::math::image::facing::QUARTER_PI;
use crate::math::rng::Well512;
use crate::movement::scout_area::ScoutArea;
use crate::players::ai_module::InfoModule;
use crate::{GenieError, GenieResult};

/// A path map's byte of a tile no search has touched (-1).
pub const UNEXPLORED: u8 = 0xff;
/// The byte of an explored tile (the hook's, and a tile the search picked).
pub const EXPLORED: u8 = 0;
/// The byte of a tile its unit cannot path to.
pub const PUNTED: u8 = 1;
/// The byte of a tile that no search reached.
pub const DEAD_END: u8 = 2;

/// A tile in the map that the path map does not hold (the game reads what follows it).
const OFF_THE_GRID: GenieError = GenieError::unreachable("a tile the AI's path map does not hold");

/// What the search reads of the explorer.
#[derive(Clone, Copy, Debug)]
pub struct SearchUnit {
    /// Its place on the ground.
    pub(crate) position: Point,
    /// Its master's class is the civilian's (4).
    pub(crate) civilian: bool,
}

/// How a flood from one tile ended.
enum Flood {
    /// A tile (or a quadrant's corner) the unit goes to.
    Found(Tile),
    /// The search starts again from the unit's place.
    Again,
    /// The flood reached every tile it could.
    Exhausted,
}

/// `!=` of two floats as the game's `UCOMISS` and `JNZ`/`JP` see it: a NaN differs.
fn differs(left: f32, right: f32) -> bool {
    left != right
}

impl InfoModule {
    /// The byte of `tile` (`+0x105c8`).
    fn cell(&self, tile: Tile) -> GenieResult<u8> {
        self.path_map.get(tile).ok_or(OFF_THE_GRID)
    }

    /// `FUN_140b90270` ("tileUncovered"): the tile is on the map and neither unexplored (-1) nor
    /// a dead end (2).
    ///
    /// # Errors
    ///
    /// A tile of the map that the path map does not hold.
    pub fn tile_uncovered(&self, tile: Tile) -> GenieResult<bool> {
        if !self.size.holds(tile) {
            return Ok(false);
        }
        let cell = self.cell(tile)?;
        Ok(cell != UNEXPLORED && cell != DEAD_END)
    }

    /// `FUN_140b96860`: the stack's top, (-1, -1) for none.
    #[must_use]
    pub fn top_point(&self) -> Tile {
        self.search.last().copied().unwrap_or(Tile::new(-1, -1))
    }

    /// `FUN_140b97220`: the stack's top goes and is given, (-1, -1) for none.
    pub fn pop_point(&mut self) -> Tile {
        self.search.pop().unwrap_or(Tile::new(-1, -1))
    }

    /// `FUN_140b7d920`: the point held on the map (the module's size), and whether it moved.
    const fn clamp(&self, tile: Tile) -> (Tile, bool) {
        let (mut x, mut y) = (tile.x(), tile.y());
        let mut moved = false;
        if x < 0 {
            x = 0;
            moved = true;
        } else if self.size.width() <= x {
            x = self.size.width() - 1;
            moved = true;
        }
        if y < 0 {
            y = 0;
            moved = true;
        } else if self.size.height() <= y {
            y = self.size.height() - 1;
            moved = true;
        }
        (Tile::new(x, y), moved)
    }

    /// `FUN_140b96cf0`: the eight points of a ring of `radius` about `centre`, every eighth of a
    /// turn from 0, go onto the stack when they lie on the map.
    pub fn push_ring(&mut self, radius: Distance, centre: Point) {
        let (cx, cy) = (cvttss2si(centre.x()) as f32, cvttss2si(centre.y()) as f32);
        for index in 0..8 {
            let angle = index as f32 * QUARTER_PI;
            let (tile, moved) = self.clamp(Tile::new(
                cvttss2si((cos(angle) * radius + Distance::new(cx)).get()),
                cvttss2si((sin(angle) * radius + Distance::new(cy)).get()),
            ));
            if !moved {
                self.search.push(tile);
            }
        }
    }

    /// `FUN_140b71990` for `unit`, starting at `start`: the tile its pick goes to, None when the
    /// flood reaches no tile that counts ([`ScoutArea`]: `map` is the map's size, `rng` the
    /// unsynced stream).
    ///
    /// # Errors
    ///
    /// A tile of the map that the path map does not hold, or what the area's reads do not model.
    pub(crate) fn closest_unexplored_tile(
        &mut self,
        unit: &SearchUnit,
        start: Point,
        area: &mut ScoutArea<'_>,
        (map, rng): (GridSize, &mut Well512),
    ) -> GenieResult<Option<Tile>> {
        let own = area.own_zone().byte();
        let mut start = start;
        loop {
            match self.flood(unit, start, own, area, (map, &mut *rng))? {
                Flood::Found(tile) => return Ok(Some(tile)),
                Flood::Again => start = unit.position,
                Flood::Exhausted => {
                    self.mark_dead_ends(own, area, map)?;
                    return Ok(None);
                }
            }
        }
    }

    /// One flood of the search: from the start tile (held on the module's map), each tile in
    /// the order the flood met them.
    fn flood(
        &mut self,
        unit: &SearchUnit,
        start: Point,
        own: u8,
        area: &mut ScoutArea<'_>,
        (map, rng): (GridSize, &mut Well512),
    ) -> GenieResult<Flood> {
        let (first, _) = self.clamp(start.tile());
        let mut visited = vec![false; (map.width().max(0) * map.height().max(0)) as usize];
        let mut queue = vec![first];
        let mut index = 0;
        while let Some(&tile) = queue.get(index) {
            index += 1;
            let cell = self.cell(tile)?;
            // a civilian takes a tile no search touched, another unit any that is not explored
            let wanted = if unit.civilian {
                cell == UNEXPLORED
            } else {
                cell != EXPLORED
            };
            if area.visible_now(tile) || !wanted {
                push_neighbours(&mut queue, &mut visited, tile, map);
                continue;
            }
            if area.zone(tile).byte() != own || !area.clear(tile)? {
                continue;
            }
            if area.reaches(tile)? {
                self.path_map.set(tile, EXPLORED);
                return Ok(Flood::Found(tile));
            }
            self.path_map.set(tile, PUNTED);
            if differs(start.x(), unit.position.x()) || differs(start.y(), unit.position.y()) {
                return Ok(Flood::Again);
            }
            return Ok(Flood::Found(self.punt(rng)));
        }
        Ok(Flood::Exhausted)
    }

    /// The punt search's pick: the quadrant of the map's 4 by 4 whose record scores the least
    /// (`first + draw % 100 + second * 25`, a draw for each record from the first column down),
    /// and one of its corners by a draw.
    fn punt(&self, rng: &mut Well512) -> Tile {
        let (width, height) = (self.size.width(), self.size.height());
        let mut best = -1_i32;
        let mut corner = Tile::new(0, 0);
        for (index, record) in self.quadrants.iter().enumerate() {
            let (column, row) = ((index / 4) as i32, (index % 4) as i32);
            let draw = (rng.rand15() as i32) % 100;
            let score = record[0]
                .wrapping_add(draw)
                .wrapping_add(record[1].wrapping_mul(0x19));
            if best == -1 || score < best {
                corner = Tile::new(width * column / 4, row * height / 4);
                best = score;
            }
        }
        let far = Tile::new(width / 4 - 1 + corner.x(), height / 4 - 1 + corner.y());
        match rng.rand15() & 3 {
            0 => corner,
            1 => Tile::new(corner.x(), far.y()),
            2 => far,
            _ => Tile::new(far.x(), corner.y()),
        }
    }

    /// `FUN_140b71490`, after a flood that found nothing: every tile of the map that is neither
    /// unexplored nor a dead end, lies in the unit's zone and is not seen now becomes a dead end.
    fn mark_dead_ends(&mut self, own: u8, area: &ScoutArea<'_>, map: GridSize) -> GenieResult<()> {
        for y in 0..map.height() {
            for x in 0..map.width() {
                let tile = Tile::new(x, y);
                let cell = self.cell(tile)?;
                if cell != UNEXPLORED
                    && cell != DEAD_END
                    && area.zone(tile).byte() == own
                    && !area.visible_now(tile)
                {
                    self.path_map.set(tile, DEAD_END);
                }
            }
        }
        Ok(())
    }
}

/// `FUN_140b6c2d0`: the tile's neighbours on the map that the flood has not met join the queue
/// (left, right, up, down).
fn push_neighbours(queue: &mut Vec<Tile>, visited: &mut [bool], at: Tile, map: GridSize) {
    for tile in at.beside() {
        let (nx, ny) = (tile.x(), tile.y());
        if nx < 0 || ny < 0 || map.width() <= nx || map.height() <= ny {
            continue;
        }
        let index = (ny * map.width() + nx) as usize;
        if let Some(met) = visited.get_mut(index)
            && !*met
        {
            *met = true;
            queue.push(tile);
        }
    }
}
