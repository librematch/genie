//! A search's request and goal: its ranges and the marks of the cells and tiles within them.

use crate::GenieResult;
use crate::map::Distance;
use crate::map::Point;
use crate::pathing::tile_path::{
    CellEdges, CellIndex, CellKind, GridLevel, HALF, ONE, PlanRect, PlanTile, Planner,
};
use crate::world::layer::generated::TilePathTuning;

/// A search's request: from a point to a goal with its ranges, as the planner's entry passes
/// them on (`FUN_140f7c530`'s arguments 3 to 11).
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    /// x, y on the map.
    pub from: Point,
    /// x, y of the goal on the map.
    pub to: Point,
    /// The unit's and its target's radii, and how near the goal counts as there (the path
    /// level's range).
    pub radii: Distance,
    /// How near the goal counts as there (the path level's range).
    pub range: Distance,
    /// Rectangles of tiles closed for this search.
    pub closed: Vec<PlanRect>,
    /// A human player's search: no early stop near the goal.
    pub full: bool,
}

/// The goal of a search: its point on the map and both ranges.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Goal {
    pub(super) at: Point,
    pub(super) radii: Distance,
    pub(super) range: Distance,
}

/// Whether a planner's pass sets a cell's mark (the goal's, [`CellKind::GOAL`], or a closed
/// rectangle's, [`CellEdges::CLOSED`]) or clears it (the routines' flag 1 or 0).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Mark {
    /// The mark goes on.
    Set,
    /// The mark goes off.
    Clear,
}

impl Mark {
    /// Whether the mark goes on.
    const fn is_set(self) -> bool {
        matches!(self, Self::Set)
    }
}

impl Request {
    pub(super) const fn goal(&self) -> Goal {
        Goal {
            at: self.to,
            radii: self.radii,
            range: self.range,
        }
    }
}

/// `(short)(int)coordinate`: the game's tile of a coordinate.
pub(super) const fn tile(coordinate: f32) -> i16 {
    coordinate as i32 as i16
}

impl Planner {
    /// `FUN_140f7ba80`: whether `cell` of `level` lies within the goal's ranges of (`x`, `y`):
    /// one of its passable (or lone) tiles, measured from the tile's corner.
    ///
    /// # Errors
    ///
    /// A tile off the grid.
    pub(super) fn in_range(
        &self,
        level: GridLevel,
        cell: PlanTile,
        goal: Goal,
    ) -> GenieResult<bool> {
        let Goal { at, radii, range } = goal;
        let (x, y) = (at.x(), at.y());
        let radii = radii.get();
        if !self.region.holds_at(level, cell) {
            return Ok(false);
        }
        let distance = range + Distance::new(HALF);
        let step = 1i16 << level.get();
        let off = |coordinate: f32, centre: f32| {
            let gap = (coordinate - (centre + -HALF)).abs();
            let gap = if gap <= radii { 0.0 } else { gap - radii };
            if gap == radii {
                if 0.0 <= gap - distance.get() {
                    gap - distance.get()
                } else {
                    0.0
                }
            } else {
                gap
            }
        };
        let first = cell.corner_at(level);
        let (x0, y0) = (first.x(), first.y());
        for tx in x0..=step - 1 + x0 {
            for ty in y0..=step - 1 + y0 {
                let cell = self.cell(GridLevel::TILES, PlanTile::new(tx, ty))?;
                if cell.edges.is_empty() && !cell.kind.contains(CellKind::LONE) {
                    continue;
                }
                let ex = off(f32::from(tx), x);
                if distance < Distance::new(ex) {
                    continue;
                }
                let ey = off(f32::from(ty), y);
                if ey <= distance.get() && ey * ey + ex * ex <= distance.squared().get() {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// The square of tiles the goal's ranges may reach (`FUN_140f7d540`, `FUN_140f7a990`): the
    /// goal ± the radii and the range (+ 1 when the range is below 1); the far sides grow by `far`.
    pub(super) fn goal_square(&mut self, goal: Goal, far: f32) {
        let Goal { at, radii, range } = goal;
        let (x, y) = (at.x(), at.y());
        let mut reach = (radii + range).get();
        if range < Distance::new(ONE) {
            reach += ONE;
        }
        let x0 = tile(x - reach).max(0);
        let y0 = tile(y - reach).max(0);
        let mut x1 = tile(reach + x + far);
        let mut y1 = tile(reach + y + far);
        let last = self.last_tile();
        if last.x() < x1 {
            x1 = last.x();
        }
        if last.y() < y1 {
            y1 = last.y();
        }
        self.region = PlanRect::new(x0, y0, x1, y1);
        self.goal = PlanTile::new(tile(x), tile(y));
    }

    /// Toggle a cell's goal mark; true when it changed.
    ///
    /// # Errors
    ///
    /// A cell off the grid.
    pub(super) fn toggle_goal(&mut self, cell_index: CellIndex, mark: Mark) -> GenieResult<bool> {
        let cell = self.cells.checked_mut(cell_index)?;
        let b1 = cell.kind;
        if mark.is_set() == b1.contains(CellKind::GOAL) {
            return Ok(false);
        }
        cell.kind = if mark.is_set() {
            b1.plus(CellKind::GOAL)
        } else {
            b1.minus(CellKind::GOAL)
        };
        Ok(true)
    }

    /// `FUN_140f7d540`: mark (or clear) the node cells within the goal's ranges; when none is, the
    /// coarsest node cell holding the goal's tile.
    ///
    /// # Errors
    ///
    /// A cell off the grid.
    pub(super) fn mark_goal(
        &mut self,
        goal: Goal,
        mark: Mark,
        tile_path_tuning: &TilePathTuning,
    ) -> GenieResult<()> {
        self.goal_square(goal, tile_path_tuning.far_side);
        self.goal_marked = false;
        let [x0, y0, x1, y1] = self.region.get();
        let mut marked = 0;
        for cx in (x0 >> 3)..=(x1 >> 3) {
            for cy in (y0 >> 3)..=(y1 >> 3) {
                marked += self.mark_cells(GridLevel::TOP, PlanTile::new(cx, cy), goal, mark)?;
            }
        }
        if marked != 0 {
            self.goal_marked = true;
            return Ok(());
        }
        for grid_level in GridLevel::ALL.into_iter().rev() {
            let cell_index = self.index(grid_level, self.goal.at_level(grid_level));
            if self
                .cells
                .checked(cell_index)?
                .kind
                .contains(CellKind::NODE)
            {
                if self.toggle_goal(cell_index, mark)? && mark.is_set() {
                    self.goal_marked = true;
                }
                return Ok(());
            }
        }
        Ok(())
    }

    /// `FUN_140f7d7a0`: the node cells under a cell within the goal's ranges, marked or cleared;
    /// returns how many changed.
    ///
    /// # Errors
    ///
    /// A cell off the grid.
    pub(super) fn mark_cells(
        &mut self,
        level: GridLevel,
        cell: PlanTile,
        goal: Goal,
        mark: Mark,
    ) -> GenieResult<i32> {
        if !self.region.holds_at(level, cell) {
            return Ok(0);
        }
        let cell_index = self.index(level, cell);
        if !self
            .cells
            .checked(cell_index)?
            .kind
            .contains(CellKind::NODE)
        {
            if level == GridLevel::TILES {
                return Ok(0);
            }
            let mut marked = 0;
            for plan_tile in cell.children() {
                marked += self.mark_cells(level.down(), plan_tile, goal, mark)?;
            }
            return Ok(marked);
        }
        Ok(i32::from(
            self.in_range(level, cell, goal)? && self.toggle_goal(cell_index, mark)?,
        ))
    }

    /// `FUN_140f7a990`: mark (or clear) the tiles within the goal's ranges; when none is, the
    /// goal's own tile.
    ///
    /// # Errors
    ///
    /// A cell off the grid.
    pub(super) fn mark_goal_tiles(&mut self, goal: Goal, mark: Mark) -> GenieResult<()> {
        self.goal_square(goal, 0.0);
        let [x0, y0, x1, y1] = self.region.get();
        let mut changed = false;
        for ty in y0..=y1 {
            for tx in x0..=x1 {
                let plan_tile = PlanTile::new(tx, ty);
                if self.in_range(GridLevel::TILES, plan_tile, goal)? {
                    let index = self.index(GridLevel::TILES, plan_tile);
                    changed |= self.toggle_goal(index, mark)?;
                }
            }
        }
        if !changed {
            let cell_index = self.index(GridLevel::TILES, self.goal);
            let _ = self.toggle_goal(cell_index, mark)?;
        }
        Ok(())
    }

    /// A search's closed rectangles of tiles closed or reopened, one after another
    /// ([`Planner::close_rect`]).
    ///
    /// # Errors
    ///
    /// A cell off the grid.
    pub(super) fn close_rects(&mut self, rects: &[PlanRect], mark: Mark) -> GenieResult<()> {
        for &plan_rect in rects {
            self.close_rect(plan_rect, mark)?;
        }
        Ok(())
    }

    /// `FUN_140f7be80`: close (or reopen) the node cells over a rectangle of tiles
    /// (`FUN_140f7e070`).
    ///
    /// # Errors
    ///
    /// A cell off the grid.
    pub(super) fn close_rect(&mut self, rect: PlanRect, mark: Mark) -> GenieResult<()> {
        let [x0, y0, x1, y1] = rect.get();
        let (lo_x, hi_x) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
        let (lo_y, hi_y) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
        let last = self.last_tile();
        let rx0 = lo_x.max(0);
        let ry0 = lo_y.max(0);
        let rx1 = if last.x() < hi_x { last.x() } else { hi_x };
        let ry1 = if last.y() < hi_y { last.y() } else { hi_y };
        self.region = PlanRect::new(rx0, ry0, rx1, ry1);
        for cx in (rx0 >> 3)..=(rx1 >> 3) {
            for cy in (ry0 >> 3)..=(ry1 >> 3) {
                self.close_cells(GridLevel::TOP, PlanTile::new(cx, cy), mark)?;
            }
        }
        Ok(())
    }

    /// `FUN_140f7e070`: the node cells under `cell` of `level` within the region closed or
    /// reopened; the last child is walked by the loop, the others by recursion.
    ///
    /// # Errors
    ///
    /// A cell off the grid.
    pub(super) fn close_cells(
        &mut self,
        mut level: GridLevel,
        mut cell: PlanTile,
        mark: Mark,
    ) -> GenieResult<()> {
        while !self.cell(level, cell)?.kind.contains(CellKind::NODE) {
            if level == GridLevel::TILES {
                return Ok(());
            }
            level = level.down();
            let [c0, c1, c2, c3] = cell.children();
            self.close_cells(level, c0, mark)?;
            self.close_cells(level, c1, mark)?;
            self.close_cells(level, c3, mark)?;
            cell = c2;
        }
        if self.region.holds_at(level, cell) {
            let node_cell = self.cell_mut(level, cell)?;
            let b0 = node_cell.edges;
            if mark.is_set() != b0.contains(CellEdges::CLOSED) {
                node_cell.edges = if mark.is_set() {
                    b0.plus(CellEdges::CLOSED)
                } else {
                    b0.minus(CellEdges::CLOSED)
                };
            }
        }
        Ok(())
    }
}
