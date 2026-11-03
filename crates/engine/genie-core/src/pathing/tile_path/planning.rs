//! The planner's entry (`FUN_140f7c530`): the search over the levels, the shortcut near the
//! start and the search on the tiles.

use crate::GenieError;
use crate::map::Distance;
use crate::map::Point;
use crate::math::fmath::BVector;
use crate::math::fmath::sqrt;
use crate::pathing::tile_path::goal_marks::Mark;
use crate::pathing::tile_path::{
    CellKind, Found, GridLevel, Path, PlanTile, Planner, Request, SegmentTest, smooth,
};
use crate::world::layer::generated::TilePathTuning;

impl Planner {
    /// `FUN_140f77f20`: the search over the levels: from the coarsest node cell that holds the
    /// start, toward the node cells within the goal's ranges; the path enters each node cell at
    /// the tile nearest the line toward the next crossing.
    ///
    /// # Errors
    ///
    /// What the open list's sort fails on.
    pub fn search_over_levels(
        &mut self,
        req: &Request,
        out: &mut Path,
        tile_path_tuning: &TilePathTuning,
    ) -> Result<Found, GenieError> {
        for &r in &req.closed {
            self.close_rect(r, Mark::Set);
        }
        let start = PlanTile::of_point(req.from);
        let goal = PlanTile::of_point(req.to);
        if !self.on_map(start) || !self.on_map(goal) {
            return Ok(Found::OffMap);
        }
        self.open.clear();
        self.mark_goal(req.goal(), Mark::Set, tile_path_tuning);
        self.best = None;
        let mut found = (None, Found::Nearest);
        for level in GridLevel::ALL.into_iter().rev() {
            let cell = start.at_level(level);
            if level == GridLevel::TILES || self.cell(level, cell).kind.contains(CellKind::NODE) {
                found = match self.search_levels(level, cell, start, req.full, tile_path_tuning)? {
                    None => (self.best, Found::Nearest),
                    Some(n) if self.goal_marked => (Some(n), Found::Goal),
                    Some(n) => (Some(n), Found::Nearest),
                };
                break;
            }
        }
        out.points.clear();
        self.points.clear();
        let result = match found {
            (None, _) => Found::Nothing,
            (Some(n), result) => {
                self.last = goal;
                let (to_x, to_y) = (req.to.x(), req.to.y());
                let mut end = BVector::new(to_x, 0.0, to_y);
                let toward =
                    result != Found::Goal || self.cell(GridLevel::TILES, goal).edges.is_empty();
                if toward {
                    let c = self.tile_toward(n, goal);
                    let dy = f32::from(goal.y()) - f32::from(c.y());
                    let dx = f32::from(goal.x()) - f32::from(c.x());
                    if result != Found::Goal
                        || req.radii.get() + tile_path_tuning.goal_gap < sqrt(dy * dy + dx * dx)
                    {
                        end = Self::centre(c);
                    }
                }
                self.points.push(end);
                self.trace_back(n, start);
                self.points
                    .push(BVector::new(req.from.x(), 0.0, req.from.y()));
                result
            }
        };
        for &p in self.points.iter().rev() {
            let _ = out.push(p);
        }
        self.mark_goal(req.goal(), Mark::Clear, tile_path_tuning);
        self.clear_nodes();
        for &r in &req.closed {
            self.close_rect(r, Mark::Clear);
        }
        Ok(result)
    }

    /// `FUN_140f7d160`: the tile or one of its four neighbours (right, below, left, above) has an
    /// edge.
    pub(super) fn near_passable(&self, t: PlanTile) -> bool {
        let edges =
            |t: PlanTile| self.on_map(t) && !self.cell(GridLevel::TILES, t).edges.is_empty();
        edges(t)
            || edges(t.stepped(1, 0))
            || edges(t.stepped(0, 1))
            || edges(t.stepped(-1, 0))
            || edges(t.stepped(0, -1))
    }

    /// The planner's entry (`FUN_140f7c530`): the search over the levels; for a human player's
    /// path to the goal under 35 tiles between passable tiles also the search on the tiles, else
    /// a shorter first stretch to the last point within 10 tiles; both smoothed (`blocked`, the
    /// obstruction manager's segment test for the unit, None without a unit); the shorter, or
    /// the one with the better result, appended to `out`. `target_box` is the target's flag the
    /// game reads at `+0x658` (a goal tile without edges then counts if a neighbour has some).
    pub fn plan<E: From<GenieError>>(
        &mut self,
        req: &Request,
        target_box: bool,
        out: &mut Path,
        walls: Option<&mut dyn SegmentTest<E>>,
        tile_path_tuning: &TilePathTuning,
    ) -> Result<Found, E> {
        let (w, h) = (self.size.width() as f32, self.size.height() as f32);
        if req.to.x() < 0.0 || w <= req.to.x() || req.to.y() < 0.0 || h <= req.to.y() {
            return Ok(Found::OffMap);
        }
        let mut levels = Path::default();
        let mut tiles = Path::default();
        let first = self.search_over_levels(req, &mut levels, tile_path_tuning)?;
        let mut second = None;
        let goal = PlanTile::of_point(req.to);
        // the start's cell is read only once the search over the levels reached the goal
        if req.range == Distance::ZERO
            && req.full
            && first == Found::Goal
            && !self
                .cell(GridLevel::TILES, PlanTile::of_point(req.from))
                .edges
                .is_empty()
            && (!self.cell(GridLevel::TILES, goal).edges.is_empty()
                || (target_box && self.near_passable(goal)))
            && {
                levels.measure();
                levels.length < Distance::new(tile_path_tuning.tile_search_length)
            }
        {
            second = Some(self.search_on_tiles(req, &mut tiles, tile_path_tuning)?);
        }
        if req.full && second != Some(Found::Goal) && !levels.points.is_empty() {
            self.shorter_start(req, &mut levels, tile_path_tuning)?;
        }
        if let Some(walls) = walls {
            let smoothed = |r: Found| !matches!(r, Found::OffMap | Found::Nothing);
            if smoothed(first) {
                smooth(&mut levels, walls, tile_path_tuning)?;
            }
            if second.is_some_and(smoothed) {
                smooth(&mut tiles, walls, tile_path_tuning)?;
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
    ) -> Result<Option<Point>, GenieError> {
        let req = Request {
            from,
            to,
            radii: Distance::ZERO,
            range: Distance::ZERO,
            closed: Vec::new(),
            full: false,
        };
        let mut path = Path::default();
        let found = self.plan::<GenieError>(&req, false, &mut path, None, tile_path_tuning)?;
        if found == Found::OffMap {
            return Ok(None);
        }
        Ok(path
            .points
            .iter()
            .rev()
            .find(|p| {
                !self
                    .cell(GridLevel::TILES, PlanTile::of_point(p.ground()))
                    .edges
                    .is_empty()
            })
            .map(|p| p.ground()))
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
    ) -> Result<(), GenieError> {
        let n = levels.points.len();
        let Some(&p0) = levels.points.first() else {
            return Ok(());
        };
        let mut i = 1;
        if n <= 1 {
            return Ok(());
        }
        loop {
            let Some(&pi) = levels.points.get(i) else {
                return Ok(());
            };
            let d = (p0.height() - pi.height()) * (p0.height() - pi.height())
                + (p0.x() - pi.x()) * (p0.x() - pi.x())
                + (p0.y() - pi.y()) * (p0.y() - pi.y());
            if tile_path_tuning.shortcut_reach < d {
                i -= 1;
                break;
            }
            i += 1;
            if n <= i {
                break;
            }
        }
        if i <= 1 || i == n {
            return Ok(());
        }
        let Some(&pt) = levels.points.get(i) else {
            return Ok(());
        };
        let mut shortcut = Path::default();
        let to_point = Request {
            to: pt.ground(),
            ..req.clone()
        };
        if self.search_on_tiles(&to_point, &mut shortcut, tile_path_tuning)? == Found::Goal {
            for &p in levels.points.iter().skip(i + 1) {
                let _ = shortcut.push(p);
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
    /// What the open list's sort fails on.
    pub fn search_on_tiles(
        &mut self,
        req: &Request,
        out: &mut Path,
        tile_path_tuning: &TilePathTuning,
    ) -> Result<Found, GenieError> {
        for &r in &req.closed {
            self.close_rect(r, Mark::Set);
        }
        let start = PlanTile::of_point(req.from);
        let goal = PlanTile::of_point(req.to);
        if !self.on_map(start) || !self.on_map(goal) {
            return Ok(Found::OffMap);
        }
        self.mark_goal_tiles(req.goal(), Mark::Set);
        self.best = None;
        let found = match self.search_tiles(start, tile_path_tuning)? {
            None => (self.best, Found::Nearest),
            Some(n) => (Some(n), Found::Goal),
        };
        out.points.clear();
        self.points.clear();
        let result = match found {
            (None, _) => Found::Nothing,
            (Some(n), result) => {
                self.last = goal;
                let end = if result == Found::Goal
                    && !self.cell(GridLevel::TILES, goal).edges.is_empty()
                {
                    let (to_x, to_y) = (req.to.x(), req.to.y());
                    BVector::new(to_x, 0.0, to_y)
                } else {
                    Self::centre(self.tile_toward(n, goal))
                };
                self.points.push(end);
                self.trace_back(n, start);
                self.points
                    .push(BVector::new(req.from.x(), 0.0, req.from.y()));
                result
            }
        };
        for &p in self.points.iter().rev() {
            let _ = out.push(p);
        }
        self.mark_goal_tiles(req.goal(), Mark::Clear);
        self.clear_nodes();
        for &r in &req.closed {
            self.close_rect(r, Mark::Clear);
        }
        Ok(result)
    }
}
