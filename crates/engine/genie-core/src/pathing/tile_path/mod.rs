//! The tile pathfinder: a planner per terrain restriction in use (the world's list at
//! `world + 0x140`, made by `FUN_140f7d310`), which searches a four-level grid of the map's tiles
//! for the path a unit's top path level asks for (`FUN_140f7c530`, called from the motion step's
//! state-2 branch).
//!
//! The grid (`+0x3d8`, two bytes per cell) holds level 0, one cell per tile, rounded up to whole
//! level-3 cells of 8 × 8 tiles, and three coarser levels, each cell covering 2 × 2 cells of the
//! level below. Byte 0 of a cell holds the edges a unit can cross ([`N`], [`W`], [`E`], [`S`]) and,
//! during a search, [`OPEN`] and [`CLOSED`]; byte 1 the children that lie off the map (its low
//! nibble), [`GOAL`], [`NODE`] (the cell is uniform and searched as one node), [`MIXED`] (its
//! children are searched instead) and [`LONE`] (a passable tile with no passable neighbour).
//!
//! A search runs A* over the coarsest node cells, from the start's cell to any cell marked as
//! the goal, with the open nodes in buckets by their whole cost ([`OpenList`]). The planner lives
//! in the world, not in the save: the slice builds it from the map's terrain, the restriction's
//! passability and the obstruction manager's blocked tiles, as the game does at load.

use crate::map::Distance;
use crate::math::fmath::{self, BVector};
use genie_diff::Diff;

mod goal_marks;
mod grid;
mod path_trace;
mod plan_tile;
mod planning;
mod search;
mod smoothing;

pub use self::goal_marks::Request;
pub use self::grid::{Blocked, Planner};
pub use self::plan_tile::{PlanRect, PlanTile};
use self::search::{Node, OpenList};
#[cfg(test)]
use self::smoothing::straighten;
use self::smoothing::{SHORTCUT_REACH, TILE_SEARCH_LENGTH};
pub use self::smoothing::{SegmentTest, smooth};

/// Byte 0: the edges to the cells above (y − 1), left, right and below (y + 1).
pub const N: u8 = 1;
/// The edge to the cell left.
pub const W: u8 = 2;
/// The edge to the cell right.
pub const E: u8 = 4;
/// The edge to the cell below (y + 1).
pub const S: u8 = 8;
/// Byte 0 during a search: a node of the cell was opened, or closed.
pub const OPEN: u8 = 0x10;
/// A node of the cell was closed.
pub const CLOSED: u8 = 0x20;
/// Byte 1: the goal's cell.
pub const GOAL: u8 = 0x10;
/// Byte 1: the cell is uniform and searched as one node.
pub const NODE: u8 = 0x20;
/// Byte 1: the cell's children are searched instead of it.
pub const MIXED: u8 = 0x40;
/// Byte 1: a passable tile with no passable neighbour.
pub const LONE: u8 = 0x80;

/// The cost the best node starts from in each search (`0x497423f0`).
const NO_COST: f32 = 999_999.0;
/// The goal's own tile counts when the goal is this near it (`DAT_142eb99cc`).
const GOAL_GAP: f32 = crate::math::image::tile_path::GOAL_GAP;
/// The planner's half tile (`DAT_142e7f384`) and its −0.5 (`DAT_142eb6e58`).
const HALF: f32 = 0.5;
/// A goal's range below 1 grows the marked square by 1 (`DAT_14330eb8c`).
const ONE: f32 = crate::math::image::tile_path::ONE;
/// The hierarchical search marks 0.1 more on the square's far sides (`DAT_142eb5690`).
const FAR_SIDE: f32 = crate::math::image::tile_path::FAR_SIDE;
/// A computer player's search stops early near the goal (`DAT_14330ebb8`, `DAT_142eb0f4c`).
const EARLY_LEFT: f32 = crate::math::image::tile_path::EARLY_LEFT;
const EARLY_START: f32 = crate::math::image::tile_path::EARLY_START;
/// The level-0 search costs a turn's shortcut from the node before at 1.1 times its cost
/// (`DAT_142e86c14`).
const SHORTCUT_COST: f32 = crate::math::image::tile_path::SHORTCUT_COST;

/// What a search returns (its result byte).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Found {
    /// 0: the start or the goal lies off the map.
    OffMap,
    /// 1: the path reaches the goal.
    Goal,
    /// 2: the path ends at the node nearest the goal.
    Nearest,
    /// 3: no node at all.
    Nothing,
}

impl Found {
    /// The search's result as the game's code (0 to 3).
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::OffMap => 0,
            Self::Goal => 1,
            Self::Nearest => 2,
            Self::Nothing => 3,
        }
    }
}

/// A path's points and its length, as the game's path data holds them (`+0x438`..`+0x440`, `+4`).
#[derive(Clone, Debug, Default, PartialEq, Diff)]
pub struct Path {
    /// The points.
    pub points: Vec<BVector>,
    /// The length.
    pub length: Distance,
}

impl Path {
    /// Insert `p` at `index` (`FUN_140f95060`) unless it equals the point before or the point at
    /// `index`; false when it does or `index` lies beyond the end.
    pub fn insert(&mut self, index: usize, p: BVector) -> bool {
        let n = self.points.len();
        if n < index
            || (0 < index && self.points.get(index - 1) == Some(&p))
            || (index < n && self.points.get(index) == Some(&p))
        {
            return false;
        }
        self.points.insert(index, p);
        true
    }

    /// Append `p` (`FUN_140f95a10`).
    pub fn push(&mut self, p: BVector) -> bool {
        self.insert(self.points.len(), p)
    }

    /// The length along the points in 3D (`FUN_140f95a60`).
    pub fn measure(&mut self) {
        let mut total = 0.0;
        for w in self.points.windows(2) {
            let [a, b] = w else {
                continue;
            };
            let d = fmath::sub(*b, *a);
            total += fmath::sqrt(d.x() * d.x() + d.height() * d.height() + d.y() * d.y());
        }
        self.length = Distance::new(total);
    }

    /// Append `other`'s points to these (`FUN_140f95c00`); the length is `other`'s, or measured
    /// anew when `other`'s is at most 1e-7 or unordered (`COMISS`, `JBE` at 0x140f95d66).
    pub fn append(&mut self, other: &Self) {
        for &p in &other.points {
            let _ = self.push(p);
        }
        if fmath::at_most_or_unordered(other.length.get(), 1e-7) {
            self.measure();
        } else {
            self.length = other.length;
        }
    }
}

#[cfg(test)]
mod tests;
