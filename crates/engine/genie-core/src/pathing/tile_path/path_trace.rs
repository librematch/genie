//! The path's points: the tiles a node's path crosses into and aims at, traced back from the
//! goal.

use crate::GenieResult;
use crate::math::fmath::BVector;
use crate::pathing::tile_path::{
    Cell, CellEdges, CellKind, GridLevel, HALF, NodeIndex, PlanTile, Planner,
};

impl Planner {
    /// `FUN_140f7b9e0`: the steps from a child cell to the goal's tile at that level (`to`),
    /// counted only where the goal lies right of or below it; `0xffff` for a cell off the map or
    /// without edges.
    ///
    /// # Errors
    ///
    /// A cell off the grid.
    pub(super) fn steps_to(
        &self,
        to: PlanTile,
        level: GridLevel,
        cell: PlanTile,
    ) -> GenieResult<u16> {
        let dy = (to.y() - cell.y()).max(0);
        let dx = (to.x() - cell.x()).max(0);
        let cell = self.cell(level, cell)?;
        Ok(
            if !cell.kind.contains(CellKind::OFF_MAP) && !cell.edges.is_empty() {
                (dy + dx) as u16
            } else {
                0xffff
            },
        )
    }

    /// `FUN_140f77d00`: the tile of a node cell nearest the goal's tile, child by child down to
    /// level 0 (ties to the earlier child).
    ///
    /// # Errors
    ///
    /// A cell off the grid, a node the pool does not hold.
    pub(super) fn tile_toward(
        &self,
        node_index: NodeIndex,
        goal: PlanTile,
    ) -> GenieResult<PlanTile> {
        let node = self.nodes.checked(node_index)?;
        let mut at = node.at;
        for grid_level in (1..=node.level.get()).rev().map(GridLevel::new) {
            let cl = grid_level.down();
            // the children in the order (2x, 2y), (2x, 2y + 1), (2x + 1, 2y), (2x + 1, 2y + 1)
            let [c0, c1, c2, c3] = at.children();
            let to = goal.at_level(grid_level.down());
            let distances = [
                self.steps_to(to, cl, c0)?,
                self.steps_to(to, cl, c2)?,
                self.steps_to(to, cl, c1)?,
                self.steps_to(to, cl, c3)?,
            ];
            let mut best = if distances[0] < 9999 {
                distances[0]
            } else {
                9999
            };
            let mut which: i32 = if 9998 < distances[0] { -1 } else { 0 };
            for (index, &dk) in distances.iter().enumerate().skip(1).take(2) {
                if -1 < dk as i16 && (dk as i16) < best as i16 {
                    which = index as i32;
                    best = dk;
                }
            }
            at = if -1 < distances[3] as i16 && (distances[3] as i16) < best as i16 {
                c3
            } else {
                match which {
                    1 => c2,
                    2 => c1,
                    _ => c0,
                }
            };
        }
        Ok(at)
    }

    /// The scan along a boundary for the tile nearest `at` that has edge `dir`: first from `at` up
    /// to `hi`, then down to `lo` while nearer than the one found (`FUN_140f775a0`,
    /// `FUN_140f794f0`).
    ///
    /// # Errors
    ///
    /// What `tile_at` fails on.
    #[expect(
        clippy::unused_self,
        reason = "a part of the tile planner's members `FUN_140f775a0` and `FUN_140f794f0`; the scan reads nothing of the planner"
    )]
    pub(super) fn nearest_on_boundary(
        &self,
        at: i16,
        lo: i16,
        hi: i16,
        dir: CellEdges,
        tile_at: impl Fn(i16) -> GenieResult<Cell>,
    ) -> GenieResult<Option<i16>> {
        let mut off: i16 = 9999;
        let mut found = None;
        let mut scan = at;
        while scan <= hi {
            if tile_at(scan)?.edges.intersects(dir) {
                off = scan - at;
                found = Some(scan);
                break;
            }
            scan += 1;
        }
        let mut at = at;
        while lo <= at && 0 < off {
            if tile_at(at)?.edges.intersects(dir) {
                return Ok(Some(at));
            }
            at -= 1;
            off -= 1;
        }
        Ok(found)
    }

    /// `FUN_140f775a0`: the tile where a node's path crosses into its parent: on the node's edge
    /// toward the parent, nearest the middle of the smaller of the two cells.
    ///
    /// # Errors
    ///
    /// A cell off the grid, a node the pool does not hold.
    pub(super) fn crossing(&self, node_index: NodeIndex) -> GenieResult<Option<PlanTile>> {
        let node = *self.nodes.checked(node_index)?;
        let Some(parent) = node.parent else {
            return Ok(None);
        };
        let parent = *self.nodes.checked(parent)?;
        // the levels as the shifts from a cell to its tiles
        let (smaller, shift) = if parent.level < node.level {
            (parent.at, parent.level.get())
        } else {
            (node.at, node.level.get())
        };
        let nl = node.level.get();
        let (x, y) = (smaller.x(), smaller.y());
        let (nx, ny) = (node.at.x(), node.at.y());
        let dir = node.dir;
        let span = |coordinate: i16| {
            let lo = coordinate << shift;
            if 0 < shift {
                (lo, (1 << (shift - 1)) + lo, (1 << shift) - 1 + lo)
            } else {
                (lo, lo, lo)
            }
        };
        let (mut ox, mut oy) = if dir == CellEdges::N || dir == CellEdges::S {
            let (lo, mid, hi) = span(x);
            let row = if dir == CellEdges::N {
                ny << nl
            } else {
                ((ny + 1) << nl) - 1
            };
            let x = if 0 < shift {
                self.nearest_on_boundary(mid, lo, hi, dir, |coordinate| {
                    self.cell(GridLevel::TILES, PlanTile::new(coordinate, row))
                })?
                .unwrap_or(mid)
            } else {
                mid
            };
            (x, row)
        } else if dir == CellEdges::W || dir == CellEdges::E {
            let (lo, mid, hi) = span(y);
            let col = if dir == CellEdges::W {
                nx << nl
            } else {
                ((nx + 1) << nl) - 1
            };
            let y = if 0 < shift {
                self.nearest_on_boundary(mid, lo, hi, dir, |coordinate| {
                    self.cell(GridLevel::TILES, PlanTile::new(col, coordinate))
                })?
                .unwrap_or(mid)
            } else {
                mid
            };
            (col, y)
        } else {
            return Ok(None);
        };
        let last = self.last_tile();
        if last.x() < ox {
            ox = last.x();
        }
        if last.y() < oy {
            oy = last.y();
        }
        Ok(Some(PlanTile::new(ox, oy)))
    }

    /// `FUN_140f794f0`: the tile where the path from the last point `from` toward the look-ahead
    /// `toward` crosses a node's edge into its parent, moved along the edge to the nearest tile
    /// with that edge; -1 along the edge when none has it.
    ///
    /// # Errors
    ///
    /// A cell off the grid, a node the pool does not hold.
    pub(super) fn aim(
        &self,
        node_index: NodeIndex,
        toward: PlanTile,
        from: PlanTile,
    ) -> GenieResult<Option<PlanTile>> {
        let node = *self.nodes.checked(node_index)?;
        let Some(parent) = node.parent else {
            return Ok(None);
        };
        let parent = *self.nodes.checked(parent)?;
        // the levels as the shifts from a cell to its tiles
        let (smaller, bl) = if parent.level < node.level {
            (parent.at, parent.level.get())
        } else {
            (node.at, node.level.get())
        };
        let nl = node.level.get();
        let (bx, by) = (smaller.x(), smaller.y());
        let (nx, ny) = (node.at.x(), node.at.y());
        let (tx, ty, lx, ly) = (toward.x(), toward.y(), from.x(), from.y());
        let dir = node.dir;
        let (dxl, dyl) = (tx.wrapping_sub(lx), ty.wrapping_sub(ly));
        // where the line meets the edge, `steps` tiles from the last point along the other axis
        let along = |base: i16, steps: i16, num: i16, den: i16| {
            if steps == 0 || den == 0 {
                base
            } else {
                (i32::from(base) + (i32::from(steps) * i32::from(num)) / i32::from(den)) as i16
            }
        };
        let (lo, hi, at, fixed) = if dir == CellEdges::N || dir == CellEdges::S {
            let lo = bx << bl;
            let hi = ((bx + 1) << bl) - 1;
            let bottom = ((ny + 1) << nl) - 1;
            let top = ny << nl;
            let last = self.last_tile();
            let hi = hi.min(last.x());
            let bottom = bottom.min(last.y());
            if dir == CellEdges::N {
                let steps = if top <= ly { top - ly } else { 0 };
                (lo, hi, along(lx, steps, dxl, dyl), top)
            } else if ly <= bottom {
                (lo, hi, along(lx, bottom - ly, dxl, dyl), bottom)
            } else {
                (lo, hi, lx, bottom)
            }
        } else {
            let left = nx << nl;
            let right = ((nx + 1) << nl) - 1;
            let lo = by << bl;
            let hi = ((by + 1) << bl) - 1;
            if dir == CellEdges::W {
                let steps = if left <= lx { left - lx } else { 0 };
                (lo, hi, along(ly, steps, dyl, dxl), left)
            } else if lx <= right {
                (lo, hi, along(ly, right - lx, dyl, dxl), right)
            } else {
                (lo, hi, ly, right)
            }
        };
        let passes = |coordinate: i16| -> GenieResult<bool> {
            let cell = if dir == CellEdges::N || dir == CellEdges::S {
                self.cell(GridLevel::TILES, PlanTile::new(coordinate, fixed))?
            } else {
                self.cell(GridLevel::TILES, PlanTile::new(fixed, coordinate))?
            };
            Ok(cell.edges.intersects(dir))
        };
        let result = scan_edge(at, lo, hi, passes)?;
        Ok(Some(if dir == CellEdges::N || dir == CellEdges::S {
            PlanTile::new(result, fixed)
        } else {
            PlanTile::new(fixed, result)
        }))
    }

    /// Whether the map holds tile `plan_tile`.
    pub(super) const fn on_map(&self, plan_tile: PlanTile) -> bool {
        let last = self.last_tile();
        0 <= plan_tile.x()
            && plan_tile.x() <= last.x()
            && 0 <= plan_tile.y()
            && plan_tile.y() <= last.y()
    }

    /// A tile's centre as a path point.
    pub(super) fn centre(plan_tile: PlanTile) -> BVector {
        BVector::new(
            f32::from(plan_tile.x()) + HALF,
            0.0,
            f32::from(plan_tile.y()) + HALF,
        )
    }

    /// The path's points from the goal's node back to the start (the loops of `FUN_140f77f20` and
    /// `FUN_140f78ba0`): per node the tile where the path enters its parent, aimed from the last
    /// point toward the crossing two nodes on (the start's tile when there is none).
    ///
    /// # Errors
    ///
    /// A cell off the grid, a node the pool does not hold.
    pub(super) fn trace_back(&mut self, goal: NodeIndex, start: PlanTile) -> GenieResult<()> {
        let mut node = goal;
        loop {
            let parent = self.nodes.checked(node)?.parent;
            let grandparent = parent
                .map(|node_index| self.nodes.checked(node_index))
                .transpose()?
                .and_then(|parent_node| parent_node.parent);
            let toward = grandparent
                .map(|gp| self.crossing(gp))
                .transpose()?
                .flatten()
                .unwrap_or(start);
            if let Some(at) = self.aim(node, toward, self.last)? {
                self.points.push(Self::centre(at));
                self.last = at;
            }
            match parent {
                Some(node_index) => node = node_index,
                None => return Ok(()),
            }
        }
    }

    /// The searched cells' open and closed marks cleared and the pool emptied, after the goal's
    /// marks.
    ///
    /// # Errors
    ///
    /// A node's cell off the grid.
    pub(super) fn clear_nodes(&mut self) -> GenieResult<()> {
        for node in self.nodes.iter() {
            // the game keeps the byte's low nibble, the four edges
            self.cells.checked_mut(node.cell)?.edges &= CellEdges::EDGES;
        }
        self.nodes.clear();
        Ok(())
    }
}

/// The scan along a node's edge for the tile nearest the aimed one, `at` (`FUN_140f794f0`):
/// from `lo` up when `at` lies at or before it, from `hi` down when at or past it; else from
/// `at` up to `hi` for the first tile that `passes`, then from `at` down to `lo` while nearer
/// than that one. −1 when no tile passes.
///
/// # Errors
///
/// What `passes` fails on.
fn scan_edge(
    at: i16,
    lo: i16,
    hi: i16,
    passes: impl Fn(i16) -> GenieResult<bool>,
) -> GenieResult<i16> {
    if at <= lo {
        return first_passing(lo..=hi, passes);
    }
    if hi <= at {
        return first_passing((lo..=hi).rev(), passes);
    }
    let mut off: i16 = 9999;
    let mut found = -1;
    let mut scan = at;
    while scan <= hi {
        if passes(scan)? {
            off = scan - at;
            found = scan;
            break;
        }
        scan += 1;
    }
    let mut cur = at;
    loop {
        if off < 1 {
            return Ok(found);
        }
        if passes(cur)? {
            return Ok(cur);
        }
        cur -= 1;
        off -= 1;
        if cur < lo {
            return Ok(found);
        }
    }
}

/// The first of `coordinates` that `passes`, −1 for none.
///
/// # Errors
///
/// What `passes` fails on.
fn first_passing(
    coordinates: impl IntoIterator<Item = i16>,
    passes: impl Fn(i16) -> GenieResult<bool>,
) -> GenieResult<i16> {
    for coordinate in coordinates {
        if passes(coordinate)? {
            return Ok(coordinate);
        }
    }
    Ok(-1)
}
