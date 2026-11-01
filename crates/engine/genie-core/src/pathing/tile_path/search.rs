//! The A* searches over the node cells and over the tiles: the nodes, the open list, the
//! estimates and the expansions.

use crate::pathing::tile_path::PlanTile;
use crate::pathing::tile_path::{
    CellEdges, CellKind, EARLY_LEFT, EARLY_START, GridLevel, NO_COST, Planner, SHORTCUT_COST,
};
use crate::{math::fmath, math::msvc};

/// The searches' four steps from a node, in their order: the edge, the step and the neighbour's
/// edge back.
const STEPS: [(CellEdges, i16, i16, CellEdges); 4] = [
    (CellEdges::W, -1, 0, CellEdges::E),
    (CellEdges::E, 1, 0, CellEdges::W),
    (CellEdges::N, 0, -1, CellEdges::S),
    (CellEdges::S, 0, 1, CellEdges::N),
];

/// A search node (`0x28` bytes, from the planner's pool `+0x428`, `FUN_140f79c90`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Node {
    /// `+0`: the cell's index.
    pub(super) cell: usize,
    /// `+8`, `+0xa`: the cell at its level.
    pub(super) at: PlanTile,
    /// `+0xc`, `+0xe`: the tile the cost is measured from.
    pub(super) entry: PlanTile,
    /// `+0x10`: cost so far plus the estimate to the goal; `+0x14`: the cost so far.
    pub(super) f: f32,
    pub(super) g: f32,
    /// `+0x18`.
    pub(super) level: GridLevel,
    /// `+0x19`: the edge back to the parent (none for the start).
    pub(super) dir: CellEdges,
    /// `+0x1a`: the cost was measured from the grandparent.
    pub(super) shortcut: bool,
    /// `+0x20`.
    pub(super) parent: Option<usize>,
}

/// The open nodes (`+0x480`): a bucket per whole cost, the lowest one sorted with its cheapest
/// node last (`FUN_140f9cba0`, `FUN_140f9cd40`).
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct OpenList {
    pub(super) buckets: Vec<Vec<usize>>,
    /// `+0x18` with its flag `+0x1c`, `+0x20` with `+0x24`.
    pub(super) min: Option<i32>,
    pub(super) max: Option<i32>,
}

impl OpenList {
    /// `FUN_140f9c9f0`.
    pub(super) fn clear(&mut self) {
        for b in &mut self.buckets {
            b.clear();
        }
        self.min = None;
        self.max = None;
    }

    pub(super) const fn is_empty(&self) -> bool {
        self.min.is_none()
    }

    /// `FUN_140f9cba0`: into the lowest bucket before the first node from the back that costs
    /// more; into a new lowest or a higher bucket at its end.
    pub(super) fn push(&mut self, node: usize, nodes: &[Node]) {
        let f = nodes[node].f;
        let b = f as i32;
        let index = b as usize;
        if self.buckets.len() <= index {
            self.buckets.resize_with(index + 1, Vec::new);
        }
        match self.min {
            Some(m) if b == m => {
                let bucket = &mut self.buckets[index];
                let at = (1..=bucket.len())
                    .rev()
                    .find(|&k| f < nodes[bucket[k - 1]].f)
                    .unwrap_or(0);
                bucket.insert(at, node);
                return;
            }
            Some(m) if m < b => {}
            _ => self.min = Some(b),
        }
        if self.max.is_none_or(|m| m < b) {
            self.max = Some(b);
        }
        self.buckets[index].push(node);
    }

    /// `FUN_140f9cd40`: the last node of the lowest bucket; an emptied bucket hands over to the
    /// next one that holds nodes, which is sorted then.
    pub(super) fn pop(&mut self, nodes: &[Node]) -> Option<usize> {
        let m = self.min?;
        let bucket = &mut self.buckets[m as usize];
        let node = bucket.pop()?;
        if bucket.is_empty() {
            let max = self.max.unwrap_or(m);
            if let Some(i) = (m + 1..=max).find(|&i| !self.buckets[i as usize].is_empty()) {
                self.min = Some(i);
                msvc::sort(&mut self.buckets[i as usize], &|a: usize, b: usize| {
                    nodes[a].f > nodes[b].f
                });
            } else {
                self.min = None;
                self.max = None;
            }
        }
        Some(node)
    }
}

/// A node from the pool ([`Planner::alloc`]'s arguments, `FUN_140f79c90`'s).
#[derive(Clone, Copy, Debug)]
pub(super) struct TileNode {
    /// The cell at its level.
    pub(super) at: PlanTile,
    /// The cell's level.
    pub(super) level: GridLevel,
    /// The cost so far.
    pub(super) cost: f32,
    /// The estimate to the goal.
    pub(super) estimate: f32,
    /// The node it comes from.
    pub(super) parent: Option<usize>,
    /// The edge it was entered by (none for the start).
    pub(super) dir: CellEdges,
    /// The tile its cost is measured from.
    pub(super) entry: PlanTile,
}

impl Planner {
    /// `FUN_140f79c90`: a new node from the pool.
    pub(super) fn alloc(&mut self, new: TileNode) -> usize {
        let TileNode {
            at,
            level,
            cost: g,
            estimate: h,
            parent,
            dir,
            entry,
        } = new;
        let cell = self.index(level, at);
        self.nodes.push(Node {
            cell,
            at,
            entry,
            f: g + h,
            g,
            level,
            dir,
            shortcut: false,
            parent,
        });
        self.nodes.len() - 1
    }

    /// The estimate from a tile to the goal's, in whole tiles (`FUN_140f764c0`, `FUN_140f76840`).
    pub(super) fn estimate_from(&self, t: PlanTile) -> f32 {
        let dx = i32::from(self.goal.x()) - i32::from(t.x());
        let dy = i32::from(self.goal.y()) - i32::from(t.y());
        fmath::sqrt((dy * dy + dx * dx) as f32)
    }

    /// The start's estimate (`FUN_140f78850`, `FUN_140f79250`): in f32 differences.
    pub(super) fn start_estimate(&self, t: PlanTile) -> f32 {
        let dx = f32::from(self.goal.x()) - f32::from(t.x());
        let dy = f32::from(self.goal.y()) - f32::from(t.y());
        fmath::sqrt(dy * dy + dx * dx)
    }

    /// Where a new node's cost is measured from (`FUN_140f764c0`, `FUN_140f76840`): a turn
    /// (a move across the parent's own move) measures from the grandparent, unless the parent
    /// already did; returns that node's cost so far, its tile and whether it was the grandparent.
    pub(super) fn measured_from(&self, parent: usize, dir: CellEdges) -> (f32, PlanTile, bool) {
        let p = &self.nodes[parent];
        if let (Some(gp), false) = (p.parent, p.shortcut) {
            let turn = if dir == CellEdges::W || dir == CellEdges::E {
                p.dir == CellEdges::N || p.dir == CellEdges::S
            } else {
                p.dir == CellEdges::W || p.dir == CellEdges::E
            };
            if turn {
                let gp = &self.nodes[gp];
                return (gp.g, gp.entry, true);
            }
        }
        (p.g, p.entry, false)
    }

    /// `FUN_140f764c0`: the neighbour across edge `dir` (the neighbour's edge back) of a node, at
    /// the level where it is a node cell; Some when it lies in the goal.
    pub(super) fn expand(
        &mut self,
        level: GridLevel,
        at: PlanTile,
        g: f32,
        parent: usize,
        dir: CellEdges,
    ) -> Option<usize> {
        use CellEdges as Ed;
        let i = self.index(level, at);
        let cell = self.cells[i];
        if !cell.edges.intersects(dir) {
            return None;
        }
        if !cell.kind.contains(CellKind::NODE) {
            if !cell.kind.contains(CellKind::MIXED) {
                return self.expand(level.up(), at.parent(), g, parent, dir);
            }
            // the children on the edge `dir`: (2x, 2y) above and left, (2x + 1, 2y) above and
            // right, (2x, 2y + 1) left and below, (2x + 1, 2y + 1) right and below
            let ([c0, c1, c2, c3], l) = (at.children(), level.down());
            if dir.intersects(Ed::N | Ed::W)
                && let Some(n) = self.expand(l, c0, g, parent, dir)
            {
                return Some(n);
            }
            if dir.intersects(Ed::N | Ed::E)
                && let Some(n) = self.expand(l, c1, g, parent, dir)
            {
                return Some(n);
            }
            if dir.intersects(Ed::W | Ed::S)
                && let Some(n) = self.expand(l, c2, g, parent, dir)
            {
                return Some(n);
            }
            if !dir.intersects(Ed::E | Ed::S) {
                return None;
            }
            return self.expand(l, c3, g, parent, dir);
        }
        if cell.edges.contains(CellEdges::CLOSED) {
            return None;
        }
        let centre = at.centre_at(level);
        let (g0, entry, shortcut) = match self.measured_from(parent, dir) {
            (_, _, false) => (g, self.nodes[parent].entry, false),
            found => found,
        };
        let dx = i32::from(entry.x()) - i32::from(centre.x());
        let dy = i32::from(entry.y()) - i32::from(centre.y());
        let d = fmath::sqrt((dy * dy + dx * dx) as f32);
        let h = self.estimate_from(centre);
        let n = self.alloc(TileNode {
            at,
            level,
            cost: d + g0,
            estimate: h,
            parent: Some(parent),
            dir,
            entry: centre,
        });
        self.open.push(n, &self.nodes);
        self.nodes[n].shortcut = shortcut;
        if !self.cells[i].edges.contains(CellEdges::OPEN) {
            self.cells[i].edges = self.cells[i].edges.plus(CellEdges::OPEN);
        }
        self.cells[i].kind.contains(CellKind::GOAL).then_some(n)
    }

    /// `FUN_140f76840`: the level-0 neighbour across edge `dir`; a turn's cost from the
    /// grandparent counts 1.1 times the grandparent's cost so far.
    pub(super) fn expand0(
        &mut self,
        at: PlanTile,
        g: f32,
        parent: usize,
        dir: CellEdges,
    ) -> Option<usize> {
        let i = self.index(GridLevel::TILES, at);
        let b0 = self.cells[i].edges;
        if !b0.intersects(dir) || b0.contains(CellEdges::CLOSED) {
            return None;
        }
        let (g0, entry, shortcut) = match self.measured_from(parent, dir) {
            (gp, entry, true) => (gp * SHORTCUT_COST, entry, true),
            _ => (g, self.nodes[parent].entry, false),
        };
        let dx = i32::from(entry.x()) - i32::from(at.x());
        let dy = i32::from(entry.y()) - i32::from(at.y());
        let d = fmath::sqrt((dy * dy + dx * dx) as f32);
        let h = self.estimate_from(at);
        let n = self.alloc(TileNode {
            at,
            level: GridLevel::TILES,
            cost: d + g0,
            estimate: h,
            parent: Some(parent),
            dir,
            entry: at,
        });
        self.open.push(n, &self.nodes);
        self.nodes[n].shortcut = shortcut;
        if !self.cells[i].edges.contains(CellEdges::OPEN) {
            self.cells[i].edges = self.cells[i].edges.plus(CellEdges::OPEN);
        }
        self.cells[i].kind.contains(CellKind::GOAL).then_some(n)
    }

    /// Close a popped node and keep it as the best when its estimate is the lowest yet; its
    /// cell's edges before the close, None for a cell closed already.
    pub(super) fn close(&mut self, n: usize) -> Option<CellEdges> {
        let cell = self.nodes[n].cell;
        let b = self.cells[cell].edges;
        if b.contains(CellEdges::CLOSED) {
            return None;
        }
        self.cells[cell].edges = b.plus(CellEdges::CLOSED);
        let h = self.nodes[n].f - self.nodes[n].g;
        if h < self.best_cost {
            self.best_cost = h;
            self.best = Some(n);
        }
        Some(b)
    }

    /// `FUN_140f78850`: A* over the node cells from the start's cell at `level`; Some node in the
    /// goal, None when the open list runs dry (or a computer player's search stops early, keeping
    /// the node it stopped at as the best).
    pub(super) fn search_levels(
        &mut self,
        level: GridLevel,
        cell: PlanTile,
        start: PlanTile,
        full: bool,
    ) -> Option<usize> {
        let start_cell = self.cell(level, cell);
        if !start_cell.kind.contains(CellKind::LONE) && start_cell.edges.is_empty() {
            return None;
        }
        let h0 = self.start_estimate(start);
        let mut count: u32 = 0;
        let n = self.alloc(TileNode {
            at: cell,
            level,
            cost: 0.0,
            estimate: h0,
            parent: None,
            dir: CellEdges::NONE,
            entry: start,
        });
        let cell = self.nodes[n].cell;
        if self.cells[cell].kind.contains(CellKind::GOAL) {
            return Some(n);
        }
        // the start's open mark, added without a test
        self.cells[cell].edges = self.cells[cell].edges.plus(CellEdges::OPEN);
        self.open.push(n, &self.nodes);
        self.best_cost = NO_COST;
        while !self.open.is_empty() {
            let n = self.open.pop(&self.nodes)?;
            count += 1;
            let Some(b) = self.close(n) else { continue };
            let node = self.nodes[n];
            let h = node.f - node.g;
            if !full && h <= EARLY_LEFT && h0 <= EARLY_START && (h0 as i32 * 10) as u32 <= count {
                self.best = Some(n);
                return None;
            }
            for (edge, dx, dy, back) in STEPS {
                if b.intersects(edge)
                    && let Some(found) =
                        self.expand(node.level, node.at.stepped(dx, dy), node.g, n, back)
                {
                    return Some(found);
                }
            }
        }
        None
    }

    /// `FUN_140f79250`: A* over the tiles from the start's tile.
    pub(super) fn search_tiles(&mut self, start: PlanTile) -> Option<usize> {
        if self.cell(GridLevel::TILES, start).edges.is_empty() {
            return None;
        }
        let h0 = self.start_estimate(start);
        let n = self.alloc(TileNode {
            at: start,
            level: GridLevel::TILES,
            cost: 0.0,
            estimate: h0,
            parent: None,
            dir: CellEdges::NONE,
            entry: start,
        });
        let cell = self.nodes[n].cell;
        if self.cells[cell].kind.contains(CellKind::GOAL) {
            return Some(n);
        }
        self.open.clear();
        // the start's open mark, added without a test
        self.cells[cell].edges = self.cells[cell].edges.plus(CellEdges::OPEN);
        self.open.push(n, &self.nodes);
        self.best_cost = NO_COST;
        while !self.open.is_empty() {
            let n = self.open.pop(&self.nodes)?;
            let Some(b) = self.close(n) else { continue };
            let node = self.nodes[n];
            for (edge, dx, dy, back) in STEPS {
                if b.intersects(edge)
                    && let Some(found) = self.expand0(node.at.stepped(dx, dy), node.g, n, back)
                {
                    return Some(found);
                }
            }
        }
        None
    }
}
