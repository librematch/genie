//! The planner's entry (`FUN_140f7c530`): the search over the levels, the shortcut near the
//! start and the search on the tiles.

use crate::GenieResult;
use crate::map::Distance;
use crate::map::Point;
use crate::map::spatial::SmoothingWalls;
use crate::math::fmath::BVector;
use crate::math::fmath::sqrt;
use crate::pathing::tile_path::goal_marks::Mark;
use crate::pathing::tile_path::{
    CellKind, Found, GridLevel, NodeIndex, Path, PlanTile, Planner, Request, smooth,
};
use crate::world::layer::generated::TilePathTuning;

impl Planner {
    /// `FUN_140f77f20`: the search over the levels: from the coarsest node cell that holds the
    /// start, toward the node cells within the goal's ranges; the path enters each node cell at
    /// the tile nearest the line toward the next crossing.
    ///
    /// # Errors
    ///
    /// A cell off the grid, a node the pool does not hold, what the open list's sort fails on.
    pub fn search_over_levels(
        &mut self,
        req: &Request,
        out: &mut Path,
        tile_path_tuning: &TilePathTuning,
    ) -> GenieResult<Found> {
        self.close_rects(&req.closed, Mark::Set)?;
        let start = PlanTile::of_point(req.from);
        let goal = PlanTile::of_point(req.to);
        if !self.on_map(start) || !self.on_map(goal) {
            return Ok(Found::OffMap);
        }
        self.open.clear();
        self.mark_goal(req.goal(), Mark::Set, tile_path_tuning)?;
        self.best = None;
        let mut found = (None, Found::Nearest);
        for level in GridLevel::ALL.into_iter().rev() {
            let cell = start.at_level(level);
            if level == GridLevel::TILES || self.cell(level, cell)?.kind.contains(CellKind::NODE) {
                found = match self.search_levels(level, cell, start, req.full, tile_path_tuning)? {
                    None => (self.best, Found::Nearest),
                    Some(node) if self.goal_marked => (Some(node), Found::Goal),
                    Some(node_index) => (Some(node_index), Found::Nearest),
                };
                break;
            }
        }
        out.points.clear();
        self.points.clear();
        let result = match found {
            (None, _) => Found::Nothing,
            (Some(node_index), result) => {
                self.trace_over_levels(req, node_index, result, tile_path_tuning)?;
                result
            }
        };
        for &point in self.points.iter().rev() {
            let _ = out.push(point);
        }
        self.mark_goal(req.goal(), Mark::Clear, tile_path_tuning)?;
        self.clear_nodes()?;
        self.close_rects(&req.closed, Mark::Clear)?;
        Ok(result)
    }

    /// `FUN_140f77f20`'s path from the node it found (`result`, the goal or the nearest): its end
    /// at the goal's point, or at the node cell's tile nearest the goal's tile when the search
    /// fell short of the goal, or the goal's tile has no edges and that tile lies beyond the
    /// radii and the goal's gap; then the crossings back to the start, and the start's point.
    ///
    /// # Errors
    ///
    /// A cell off the grid, a node the pool does not hold.
    fn trace_over_levels(
        &mut self,
        req: &Request,
        node_index: NodeIndex,
        result: Found,
        tile_path_tuning: &TilePathTuning,
    ) -> GenieResult<()> {
        let start = PlanTile::of_point(req.from);
        let goal = PlanTile::of_point(req.to);
        self.last = goal;
        let (to_x, to_y) = (req.to.x(), req.to.y());
        let mut end = BVector::new(to_x, 0.0, to_y);
        let toward = result != Found::Goal || self.cell(GridLevel::TILES, goal)?.edges.is_empty();
        if toward {
            let plan_tile = self.tile_toward(node_index, goal)?;
            let dy = f32::from(goal.y()) - f32::from(plan_tile.y());
            let dx = f32::from(goal.x()) - f32::from(plan_tile.x());
            if result != Found::Goal
                || req.radii.get() + tile_path_tuning.goal_gap < sqrt(dy * dy + dx * dx)
            {
                end = Self::centre(plan_tile);
            }
        }
        self.points.push(end);
        self.trace_back(node_index, start)?;
        self.points
            .push(BVector::new(req.from.x(), 0.0, req.from.y()));
        Ok(())
    }

    /// `FUN_140f7d160`: the tile or one of its four neighbours (right, below, left, above) has an
    /// edge.
    ///
    /// # Errors
    ///
    /// A tile off the grid.
    pub(super) fn near_passable(&self, tile: PlanTile) -> GenieResult<bool> {
        let edges = |plan_tile: PlanTile| -> GenieResult<bool> {
            Ok(self.on_map(plan_tile) && !self.cell(GridLevel::TILES, plan_tile)?.edges.is_empty())
        };
        Ok(edges(tile)?
            || edges(tile.stepped(1, 0))?
            || edges(tile.stepped(0, 1))?
            || edges(tile.stepped(-1, 0))?
            || edges(tile.stepped(0, -1))?)
    }

    /// The planner's entry (`FUN_140f7c530`): the search over the levels; for a human player's
    /// path to the goal under 35 tiles between passable tiles also the search on the tiles, else
    /// a shorter first stretch to the last point within 10 tiles; both smoothed (`walls`, the
    /// obstruction manager's segment test for the unit, None without a unit); the shorter, or
    /// the one with the better result, appended to `out`. `target_box` is the target's flag the
    /// game reads at `+0x658` (a goal tile without edges then counts if a neighbour has some).
    ///
    /// # Errors
    ///
    /// What the planner's searches fail on.
    pub fn plan(
        &mut self,
        req: &Request,
        target_box: bool,
        out: &mut Path,
        walls: Option<&mut SmoothingWalls<'_>>,
        tile_path_tuning: &TilePathTuning,
    ) -> GenieResult<Found> {
        let (width, height) = (self.size.width() as f32, self.size.height() as f32);
        if req.to.x() < 0.0 || width <= req.to.x() || req.to.y() < 0.0 || height <= req.to.y() {
            return Ok(Found::OffMap);
        }
        let mut levels = Path::default();
        let mut tiles = Path::default();
        let first = self.search_over_levels(req, &mut levels, tile_path_tuning)?;
        let mut second = None;
        if self.wants_tile_search(req, first, target_box, &mut levels, tile_path_tuning)? {
            second = Some(self.search_on_tiles(req, &mut tiles, tile_path_tuning)?);
        }
        if req.full && second != Some(Found::Goal) && !levels.points.is_empty() {
            self.shorter_start(req, &mut levels, tile_path_tuning)?;
        }
        if let Some(walls) = walls {
            let smoothed = |found: Found| !matches!(found, Found::OffMap | Found::Nothing);
            if smoothed(first) {
                smooth(&mut levels, walls, tile_path_tuning);
            }
            if second.is_some_and(smoothed) {
                smooth(&mut tiles, walls, tile_path_tuning);
            }
        }
        levels.measure();
        tiles.measure();
        // no second search reads as the game's 0, off the map
        let (chosen, result) =
            if second.unwrap_or(Found::OffMap) != first || levels.length < tiles.length {
                (&levels, first)
            } else {
                (&tiles, second.unwrap_or(first))
            };
        out.append(chosen);
        Ok(result)
    }

    /// Whether the entry searches on the tiles too (`FUN_140f7c530`): a human player's search
    /// with no range whose search over the levels (`first`) reached the goal, from a start tile
    /// with edges to a goal tile with edges (or one beside it for a target's box), along a path
    /// over the levels shorter than the tile search's length, which it measures.
    ///
    /// # Errors
    ///
    /// A tile off the grid.
    fn wants_tile_search(
        &self,
        req: &Request,
        first: Found,
        target_box: bool,
        levels: &mut Path,
        tile_path_tuning: &TilePathTuning,
    ) -> GenieResult<bool> {
        let goal = PlanTile::of_point(req.to);
        // the start's cell is read only once the search over the levels reached the goal
        Ok(req.range == Distance::ZERO
            && req.full
            && first == Found::Goal
            && !self
                .cell(GridLevel::TILES, PlanTile::of_point(req.from))?
                .edges
                .is_empty()
            && (!self.cell(GridLevel::TILES, goal)?.edges.is_empty()
                || (target_box && self.near_passable(goal)?))
            && {
                levels.measure();
                levels.length < Distance::new(tile_path_tuning.tile_search_length)
            })
    }

    /// `FUN_140f7c390`: the planner's path from `from` to `to` ([`Planner::plan`] with no radii,
    /// no range, no closed tiles, no tile search, no target and no smoothing, in a path the
    /// planner's pool lends, `+0x238` and `+0x278`); when it found one (a result other than 0),
    /// the last of its points whose tile has an edge (byte 0 of its level-0 cell, the tile of the
    /// point's `(short)(int)` coordinates), searched from the end; None without one.
    ///
    /// # Errors
    ///
    /// What the planner's searches fail on.
    pub fn last_open_point(
        &mut self,
        from: Point,
        to: Point,
        tile_path_tuning: &TilePathTuning,
    ) -> GenieResult<Option<Point>> {
        let req = Request {
            from,
            to,
            radii: Distance::ZERO,
            range: Distance::ZERO,
            closed: Vec::new(),
            full: false,
        };
        let mut path = Path::default();
        let found = self.plan(&req, false, &mut path, None, tile_path_tuning)?;
        if found == Found::OffMap {
            return Ok(None);
        }
        for point in path.points.iter().rev() {
            if !self
                .cell(GridLevel::TILES, PlanTile::of_point(point.ground()))?
                .edges
                .is_empty()
            {
                return Ok(Some(point.ground()));
            }
        }
        Ok(None)
    }

    /// `FUN_140f7c530`'s shortcut: the last point of the path within 10 tiles of its start (past
    /// the second, short of the end) is searched for on the tiles; when that reaches it, the path
    /// becomes that search's path and the rest after the point.
    ///
    /// # Errors
    ///
    /// What the search on the tiles fails on.
    pub(super) fn shorter_start(
        &mut self,
        req: &Request,
        levels: &mut Path,
        tile_path_tuning: &TilePathTuning,
    ) -> GenieResult<()> {
        let count = levels.points.len();
        let Some(&p0) = levels.points.first() else {
            return Ok(());
        };
        let mut index = 1;
        if count <= 1 {
            return Ok(());
        }
        loop {
            let Some(&pi) = levels.points.get(index) else {
                return Ok(());
            };
            let distance = (p0.height() - pi.height()) * (p0.height() - pi.height())
                + (p0.x() - pi.x()) * (p0.x() - pi.x())
                + (p0.y() - pi.y()) * (p0.y() - pi.y());
            if tile_path_tuning.shortcut_reach < distance {
                index -= 1;
                break;
            }
            index += 1;
            if count <= index {
                break;
            }
        }
        if index <= 1 || index == count {
            return Ok(());
        }
        let Some(&pt) = levels.points.get(index) else {
            return Ok(());
        };
        let mut shortcut = Path::default();
        let to_point = Request {
            to: pt.ground(),
            ..req.clone()
        };
        if self.search_on_tiles(&to_point, &mut shortcut, tile_path_tuning)? == Found::Goal {
            for &point in levels.points.iter().skip(index + 1) {
                let _ = shortcut.push(point);
            }
            // the game empties the path before it appends the shortcut (0x140f7c940)
            levels.points.clear();
            levels.append(&shortcut);
        }
        Ok(())
    }

    /// `FUN_140f78ba0`: the search on level 0, tile by tile, toward the tiles within the goal's
    /// ranges.
    ///
    /// # Errors
    ///
    /// A cell off the grid, a node the pool does not hold, what the open list's sort fails on.
    pub fn search_on_tiles(
        &mut self,
        req: &Request,
        out: &mut Path,
        tile_path_tuning: &TilePathTuning,
    ) -> GenieResult<Found> {
        self.close_rects(&req.closed, Mark::Set)?;
        let start = PlanTile::of_point(req.from);
        let goal = PlanTile::of_point(req.to);
        if !self.on_map(start) || !self.on_map(goal) {
            return Ok(Found::OffMap);
        }
        self.mark_goal_tiles(req.goal(), Mark::Set)?;
        self.best = None;
        let found = match self.search_tiles(start, tile_path_tuning)? {
            None => (self.best, Found::Nearest),
            Some(node_index) => (Some(node_index), Found::Goal),
        };
        out.points.clear();
        self.points.clear();
        let result = match found {
            (None, _) => Found::Nothing,
            (Some(node_index), result) => {
                self.last = goal;
                let end = if result == Found::Goal
                    && !self.cell(GridLevel::TILES, goal)?.edges.is_empty()
                {
                    let (to_x, to_y) = (req.to.x(), req.to.y());
                    BVector::new(to_x, 0.0, to_y)
                } else {
                    Self::centre(self.tile_toward(node_index, goal)?)
                };
                self.points.push(end);
                self.trace_back(node_index, start)?;
                self.points
                    .push(BVector::new(req.from.x(), 0.0, req.from.y()));
                result
            }
        };
        for &point in self.points.iter().rev() {
            let _ = out.push(point);
        }
        self.mark_goal_tiles(req.goal(), Mark::Clear)?;
        self.clear_nodes()?;
        self.close_rects(&req.closed, Mark::Clear)?;
        Ok(result)
    }
}
