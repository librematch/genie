//! A search's request and goal: its ranges and the marks of the cells and tiles within them.

use crate::map::Distance;
use crate::map::Point;
use crate::pathing::tile_path::{
    CellEdges, CellIndex, CellKind, FAR_SIDE, GridLevel, HALF, ONE, PlanRect, PlanTile, Planner,
};

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

impl Request {
    pub(super) const fn goal(&self) -> Goal {
        Goal {
            at: self.to,
            radii: self.radii,
            range: self.range,
        }
    }
}

/// `(short)(int)v`: the game's tile of a coordinate.
pub(super) const fn tile(v: f32) -> i16 {
    v as i32 as i16
}

impl Planner {
    /// `FUN_140f7ba80`: whether `cell` of `level` lies within the goal's ranges of (`x`, `y`):
    /// one of its passable (or lone) tiles, measured from the tile's corner.
    pub(super) fn in_range(&self, level: GridLevel, cell: PlanTile, goal: Goal) -> bool {
        let Goal { at, radii, range } = goal;
        let (x, y) = (at.x(), at.y());
        let radii = radii.get();
        if !self.region.holds_at(level, cell) {
            return false;
        }
        let r = range + Distance::new(HALF);
        let step = 1i16 << level.get();
        let off = |v: f32, centre: f32| {
            let d = (v - (centre + -HALF)).abs();
            let d = if d <= radii { 0.0 } else { d - radii };
            if d == radii {
                if 0.0 <= d - r.get() { d - r.get() } else { 0.0 }
            } else {
                d
            }
        };
        let first = cell.corner_at(level);
        let (x0, y0) = (first.x(), first.y());
        for tx in x0..=step - 1 + x0 {
            for ty in y0..=step - 1 + y0 {
                let c = self.cell(GridLevel::TILES, PlanTile::new(tx, ty));
                if c.edges.is_empty() && !c.kind.contains(CellKind::LONE) {
                    continue;
                }
                let ex = off(f32::from(tx), x);
                if r < Distance::new(ex) {
                    continue;
                }
                let ey = off(f32::from(ty), y);
                if ey <= r.get() && ey * ey + ex * ex <= r.squared().get() {
                    return true;
                }
            }
        }
        false
    }

    /// The square of tiles the goal's ranges may reach (`FUN_140f7d540`, `FUN_140f7a990`): the
    /// goal ± the radii and the range (+ 1 when the range is below 1); the far sides grow by `far`.
    pub(super) fn goal_square(&mut self, goal: Goal, far: f32) {
        let Goal { at, radii, range } = goal;
        let (x, y) = (at.x(), at.y());
        let mut r = (radii + range).get();
        if range < Distance::new(ONE) {
            r += ONE;
        }
        let x0 = tile(x - r).max(0);
        let y0 = tile(y - r).max(0);
        let mut x1 = tile(r + x + far);
        let mut y1 = tile(r + y + far);
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
    pub(super) fn toggle_goal(&mut self, i: CellIndex, set: bool) -> bool {
        let b1 = self.cells[i].kind;
        if set == b1.contains(CellKind::GOAL) {
            return false;
        }
        self.cells[i].kind = if set {
            b1.plus(CellKind::GOAL)
        } else {
            b1.minus(CellKind::GOAL)
        };
        true
    }

    /// `FUN_140f7d540`: mark (or clear) the node cells within the goal's ranges; when none is, the
    /// coarsest node cell holding the goal's tile.
    pub(super) fn mark_goal(&mut self, goal: Goal, set: bool) {
        self.goal_square(goal, FAR_SIDE);
        self.goal_marked = false;
        let [x0, y0, x1, y1] = self.region.get();
        let mut marked = 0;
        for cx in (x0 >> 3)..=(x1 >> 3) {
            for cy in (y0 >> 3)..=(y1 >> 3) {
                marked += self.mark_cells(GridLevel::TOP, PlanTile::new(cx, cy), goal, set);
            }
        }
        if marked != 0 {
            self.goal_marked = true;
            return;
        }
        let Some(i) = GridLevel::ALL.into_iter().rev().find_map(|l| {
            let i = self.index(l, self.goal.at_level(l));
            self.cells[i].kind.contains(CellKind::NODE).then_some(i)
        }) else {
            return;
        };
        if self.toggle_goal(i, set) && set {
            self.goal_marked = true;
        }
    }

    /// `FUN_140f7d7a0`: the node cells under a cell within the goal's ranges, marked or cleared;
    /// returns how many changed.
    pub(super) fn mark_cells(
        &mut self,
        level: GridLevel,
        cell: PlanTile,
        goal: Goal,
        set: bool,
    ) -> i32 {
        if !self.region.holds_at(level, cell) {
            return 0;
        }
        let i = self.index(level, cell);
        if !self.cells[i].kind.contains(CellKind::NODE) {
            if level == GridLevel::TILES {
                return 0;
            }
            return cell
                .children()
                .into_iter()
                .map(|c| self.mark_cells(level.down(), c, goal, set))
                .sum();
        }
        i32::from(self.in_range(level, cell, goal) && self.toggle_goal(i, set))
    }

    /// `FUN_140f7a990`: mark (or clear) the tiles within the goal's ranges; when none is, the
    /// goal's own tile.
    pub(super) fn mark_goal_tiles(&mut self, goal: Goal, set: bool) {
        self.goal_square(goal, 0.0);
        let [x0, y0, x1, y1] = self.region.get();
        let mut changed = false;
        for ty in y0..=y1 {
            for tx in x0..=x1 {
                let t = PlanTile::new(tx, ty);
                if self.in_range(GridLevel::TILES, t, goal) {
                    let i = self.index(GridLevel::TILES, t);
                    changed |= self.toggle_goal(i, set);
                }
            }
        }
        if !changed {
            let i = self.index(GridLevel::TILES, self.goal);
            let _ = self.toggle_goal(i, set);
        }
    }

    /// `FUN_140f7be80`: close (or reopen) the node cells over a rectangle of tiles
    /// (`FUN_140f7e070`).
    pub(super) fn close_rect(&mut self, rect: PlanRect, set: bool) {
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
                self.close_cells(GridLevel::TOP, PlanTile::new(cx, cy), set);
            }
        }
    }

    /// `FUN_140f7e070`: the node cells under `cell` of `level` within the region closed or
    /// reopened; the last child is walked by the loop, the others by recursion.
    pub(super) fn close_cells(&mut self, mut level: GridLevel, mut cell: PlanTile, set: bool) {
        while !self.cell(level, cell).kind.contains(CellKind::NODE) {
            if level == GridLevel::TILES {
                return;
            }
            level = level.down();
            let [c0, c1, c2, c3] = cell.children();
            self.close_cells(level, c0, set);
            self.close_cells(level, c1, set);
            self.close_cells(level, c3, set);
            cell = c2;
        }
        if self.region.holds_at(level, cell) {
            let i = self.index(level, cell);
            let b0 = self.cells[i].edges;
            if set != b0.contains(CellEdges::CLOSED) {
                self.cells[i].edges = if set {
                    b0.plus(CellEdges::CLOSED)
                } else {
                    b0.minus(CellEdges::CLOSED)
                };
            }
        }
    }
}
