//! The A* searches over the node cells and over the tiles: the nodes, the open list, the
//! estimates and the expansions.

use crate::map::Distance;
use crate::math::fmath::sqrt;
use crate::math::msvc::sort;
use crate::pathing::tile_path::PlanTile;
use crate::pathing::tile_path::{CellEdges, CellIndex, CellKind, GridLevel, NO_COST, Planner};
use crate::world::layer::generated::TilePathTuning;
use crate::world::slots::CheckedIndexExt;
use crate::{GenieError, GenieResult};
use std::slice::Iter;

/// The searches' four steps from a node, in their order: the edge, the step and the neighbour's
/// edge back.
const STEPS: [(CellEdges, i16, i16, CellEdges); 4] = [
    (CellEdges::W, -1, 0, CellEdges::E),
    (CellEdges::E, 1, 0, CellEdges::W),
    (CellEdges::N, 0, -1, CellEdges::S),
    (CellEdges::S, 0, 1, CellEdges::N),
];

/// A node the planner's pool does not hold. The pool hands out every [`NodeIndex`] as it
/// appends the node ([`Nodes::push`]), and a search reads only the nodes it made: the pool is
/// emptied after the search's path is traced ([`Planner::clear_nodes`]).
const NO_NODE: GenieError = GenieError::unreachable("a node the planner's pool does not hold");

/// A node's place in the planner's pool (`+0x428`, `FUN_140f79c90` hands them out in order).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) struct NodeIndex(usize);

/// The planner's node pool, read and changed by [`NodeIndex`]; a search's nodes in the order
/// it made them.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Nodes(Vec<Node>);

impl Nodes {
    /// Append `node`; its place.
    pub(super) fn push(&mut self, node: Node) -> NodeIndex {
        self.0.push(node);
        NodeIndex(self.0.len() - 1)
    }

    /// Node `node_index`.
    ///
    /// # Errors
    ///
    /// A node the pool does not hold ([`NO_NODE`]).
    pub(super) fn checked(&self, node_index: NodeIndex) -> GenieResult<&Node> {
        self.0
            .get(node_index.0)
            .ok_or_else(|| NO_NODE.with("node", node_index.0))
    }

    /// Node `node_index`, to change.
    ///
    /// # Errors
    ///
    /// A node the pool does not hold ([`NO_NODE`]).
    pub(super) fn checked_mut(&mut self, node_index: NodeIndex) -> GenieResult<&mut Node> {
        self.0
            .get_mut(node_index.0)
            .ok_or_else(|| NO_NODE.with("node", node_index.0))
    }

    /// Every node, in the order made.
    pub(super) fn iter(&self) -> Iter<'_, Node> {
        self.0.iter()
    }

    /// The pool emptied.
    pub(super) fn clear(&mut self) {
        self.0.clear();
    }
}

/// A search node (`0x28` bytes, from the planner's pool `+0x428`, `FUN_140f79c90`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Node {
    /// `+0`: the cell's place in the grid.
    pub(super) cell: CellIndex,
    /// `+8`, `+0xa`: the cell at its level.
    pub(super) at: PlanTile,
    /// `+0xc`, `+0xe`: the tile the cost is measured from.
    pub(super) entry: PlanTile,
    /// `+0x10`: cost so far plus the estimate to the goal; `+0x14`: the cost so far.
    pub(super) total: Distance,
    pub(super) cost: Distance,
    /// `+0x18`.
    pub(super) level: GridLevel,
    /// `+0x19`: the edge back to the parent (none for the start).
    pub(super) dir: CellEdges,
    /// `+0x1a`: the cost was measured from the grandparent.
    pub(super) shortcut: bool,
    /// `+0x20`.
    pub(super) parent: Option<NodeIndex>,
}

/// An open node with its cost so far plus the estimate ([`Node::total`]), which the open list
/// sorts by. A node's total is set as the pool hands the node out, before it is opened, and
/// never changes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct OpenNode {
    /// The node.
    pub(super) node: NodeIndex,
    /// Its total.
    pub(super) total: Distance,
}

/// The open nodes (`+0x480`): a bucket per whole cost, the lowest one sorted with its cheapest
/// node last (`FUN_140f9cba0`, `FUN_140f9cd40`).
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct OpenList {
    pub(super) buckets: Vec<Vec<OpenNode>>,
    /// `+0x18` with its flag `+0x1c`, `+0x20` with `+0x24`.
    pub(super) min: Option<i32>,
    pub(super) max: Option<i32>,
}

impl OpenList {
    /// `FUN_140f9c9f0`.
    pub(super) fn clear(&mut self) {
        for node_indexes in &mut self.buckets {
            node_indexes.clear();
        }
        self.min = None;
        self.max = None;
    }

    pub(super) const fn is_empty(&self) -> bool {
        self.min.is_none()
    }

    /// `FUN_140f9cba0`: into the lowest bucket before the first node from the back that costs
    /// more; into a new lowest or a higher bucket at its end.
    ///
    /// # Errors
    ///
    /// A node the pool does not hold.
    pub(super) fn push(&mut self, node: NodeIndex, nodes: &Nodes) -> GenieResult<()> {
        let distance = nodes.checked(node)?.total;
        let open_node = OpenNode {
            node,
            total: distance,
        };
        let slot = distance.get() as i32;
        let index = slot as usize;
        if self.buckets.len() <= index {
            self.buckets.resize_with(index + 1, Vec::new);
        }
        match self.min {
            Some(least) if slot == least => {
                let bucket = self.buckets.checked_mut(index)?;
                let at = bucket
                    .iter()
                    .rposition(|queued| distance < queued.total)
                    .map_or(0, |place| place + 1);
                bucket.insert(at, open_node);
                return Ok(());
            }
            Some(lowest) if lowest < slot => {}
            _ => self.min = Some(slot),
        }
        if self.max.is_none_or(|highest| highest < slot) {
            self.max = Some(slot);
        }
        self.buckets.checked_mut(index)?.push(open_node);
        Ok(())
    }

    /// `FUN_140f9cd40`: the last node of the lowest bucket; an emptied bucket hands over to the
    /// next one that holds nodes, which is sorted then.
    ///
    /// # Errors
    ///
    /// A bucket the list has not got, or what the sort of the next bucket fails on.
    pub(super) fn pop(&mut self) -> GenieResult<Option<NodeIndex>> {
        let Some(lowest) = self.min else {
            return Ok(None);
        };
        let bucket = self.buckets.checked_mut(lowest as usize)?;
        let Some(open_node) = bucket.pop() else {
            return Ok(None);
        };
        if bucket.is_empty() {
            let max = self.max.unwrap_or(lowest);
            if let Some((next, bucket)) = self
                .buckets
                .iter_mut()
                .enumerate()
                .skip(lowest as usize + 1)
                .take((max - lowest).max(0) as usize)
                .find(|(_, bucket)| !bucket.is_empty())
            {
                self.min = Some(next as i32);
                sort(bucket, &|left: OpenNode, right: OpenNode| {
                    left.total > right.total
                })?;
            } else {
                self.min = None;
                self.max = None;
            }
        }
        Ok(Some(open_node.node))
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
    pub(super) cost: Distance,
    /// The estimate to the goal.
    pub(super) estimate: Distance,
    /// The node it comes from.
    pub(super) parent: Option<NodeIndex>,
    /// The edge it was entered by (none for the start).
    pub(super) dir: CellEdges,
    /// The tile its cost is measured from.
    pub(super) entry: PlanTile,
}

impl Planner {
    /// `FUN_140f79c90`: a new node from the pool.
    pub(super) fn alloc(&mut self, new: TileNode) -> NodeIndex {
        let TileNode {
            at,
            level,
            cost,
            estimate: distance,
            parent,
            dir,
            entry,
        } = new;
        let cell = self.index(level, at);
        self.nodes.push(Node {
            cell,
            at,
            entry,
            total: cost + distance,
            cost,
            level,
            dir,
            shortcut: false,
            parent,
        })
    }

    /// The estimate from a tile to the goal's, in whole tiles (`FUN_140f764c0`, `FUN_140f76840`).
    pub(super) fn estimate_from(&self, plan_tile: PlanTile) -> Distance {
        let dx = i32::from(self.goal.x()) - i32::from(plan_tile.x());
        let dy = i32::from(self.goal.y()) - i32::from(plan_tile.y());
        Distance::new(sqrt((dy * dy + dx * dx) as f32))
    }

    /// The start's estimate (`FUN_140f78850`, `FUN_140f79250`): in f32 differences.
    pub(super) fn start_estimate(&self, plan_tile: PlanTile) -> Distance {
        let dx = f32::from(self.goal.x()) - f32::from(plan_tile.x());
        let dy = f32::from(self.goal.y()) - f32::from(plan_tile.y());
        Distance::new(sqrt(dy * dy + dx * dx))
    }

    /// Where a new node's cost is measured from (`FUN_140f764c0`, `FUN_140f76840`): a turn
    /// (a move across the parent's own move) measures from the grandparent, unless the parent
    /// already did; returns that node's cost so far, its tile and whether it was the grandparent.
    ///
    /// # Errors
    ///
    /// A parent or a grandparent the pool does not hold.
    pub(super) fn measured_from(
        &self,
        parent: NodeIndex,
        dir: CellEdges,
    ) -> GenieResult<(Distance, PlanTile, bool)> {
        let node = self.nodes.checked(parent)?;
        if let (Some(gp), false) = (node.parent, node.shortcut) {
            let turn = if dir == CellEdges::W || dir == CellEdges::E {
                node.dir == CellEdges::N || node.dir == CellEdges::S
            } else {
                node.dir == CellEdges::W || node.dir == CellEdges::E
            };
            if turn {
                let gp = self.nodes.checked(gp)?;
                return Ok((gp.cost, gp.entry, true));
            }
        }
        Ok((node.cost, node.entry, false))
    }

    /// `FUN_140f764c0`: the neighbour across edge `dir` (the neighbour's edge back) of a node, at
    /// the level where it is a node cell; Some when it lies in the goal.
    ///
    /// # Errors
    ///
    /// A cell off the grid, a node the pool does not hold.
    pub(super) fn expand(
        &mut self,
        level: GridLevel,
        at: PlanTile,
        cost: Distance,
        parent: NodeIndex,
        dir: CellEdges,
    ) -> GenieResult<Option<NodeIndex>> {
        use CellEdges as Ed;
        let cell_index = self.index(level, at);
        let cell = *self.cells.checked(cell_index)?;
        if !cell.edges.intersects(dir) {
            return Ok(None);
        }
        if !cell.kind.contains(CellKind::NODE) {
            if !cell.kind.contains(CellKind::MIXED) {
                return self.expand(level.up(), at.parent(), cost, parent, dir);
            }
            // the children on the edge `dir`: (2x, 2y) above and left, (2x + 1, 2y) above and
            // right, (2x, 2y + 1) left and below, (2x + 1, 2y + 1) right and below
            let sides = [Ed::N | Ed::W, Ed::N | Ed::E, Ed::W | Ed::S, Ed::E | Ed::S];
            for (child, side) in at.children().into_iter().zip(sides) {
                if dir.intersects(side)
                    && let Some(node_index) = self.expand(level.down(), child, cost, parent, dir)?
                {
                    return Ok(Some(node_index));
                }
            }
            return Ok(None);
        }
        if cell.edges.contains(CellEdges::CLOSED) {
            return Ok(None);
        }
        let centre = at.centre_at(level);
        let (g0, entry, shortcut) = match self.measured_from(parent, dir)? {
            (_, _, false) => (cost, self.nodes.checked(parent)?.entry, false),
            found => found,
        };
        let dx = i32::from(entry.x()) - i32::from(centre.x());
        let dy = i32::from(entry.y()) - i32::from(centre.y());
        let distance = Distance::new(sqrt((dy * dy + dx * dx) as f32));
        let estimate = self.estimate_from(centre);
        let node_index = self.alloc(TileNode {
            at,
            level,
            cost: distance + g0,
            estimate,
            parent: Some(parent),
            dir,
            entry: centre,
        });
        self.mark_open(node_index, cell_index, shortcut)
    }

    /// `FUN_140f76840`: the level-0 neighbour across edge `dir`; a turn's cost from the
    /// grandparent counts 1.1 times the grandparent's cost so far.
    ///
    /// # Errors
    ///
    /// A cell off the grid, a node the pool does not hold.
    pub(super) fn expand0(
        &mut self,
        at: PlanTile,
        cost: Distance,
        parent: NodeIndex,
        dir: CellEdges,
        tile_path_tuning: &TilePathTuning,
    ) -> GenieResult<Option<NodeIndex>> {
        let cell_index = self.index(GridLevel::TILES, at);
        let b0 = self.cells.checked(cell_index)?.edges;
        if !b0.intersects(dir) || b0.contains(CellEdges::CLOSED) {
            return Ok(None);
        }
        let (g0, entry, shortcut) = match self.measured_from(parent, dir)? {
            (gp, entry, true) => (gp * tile_path_tuning.shortcut_cost, entry, true),
            _ => (cost, self.nodes.checked(parent)?.entry, false),
        };
        let dx = i32::from(entry.x()) - i32::from(at.x());
        let dy = i32::from(entry.y()) - i32::from(at.y());
        let distance = Distance::new(sqrt((dy * dy + dx * dx) as f32));
        let estimate = self.estimate_from(at);
        let node_index = self.alloc(TileNode {
            at,
            level: GridLevel::TILES,
            cost: distance + g0,
            estimate,
            parent: Some(parent),
            dir,
            entry: at,
        });
        self.mark_open(node_index, cell_index, shortcut)
    }

    /// A new neighbour's node opened (the end of `FUN_140f764c0` and `FUN_140f76840`): into the
    /// open list, its shortcut kept, its cell marked open unless it is; Some when the cell lies
    /// in the goal.
    ///
    /// # Errors
    ///
    /// A cell off the grid, a node the pool does not hold.
    fn mark_open(
        &mut self,
        node_index: NodeIndex,
        cell_index: CellIndex,
        shortcut: bool,
    ) -> GenieResult<Option<NodeIndex>> {
        self.open.push(node_index, &self.nodes)?;
        self.nodes.checked_mut(node_index)?.shortcut = shortcut;
        let cell = self.cells.checked_mut(cell_index)?;
        if !cell.edges.contains(CellEdges::OPEN) {
            cell.edges = cell.edges.plus(CellEdges::OPEN);
        }
        Ok(cell.kind.contains(CellKind::GOAL).then_some(node_index))
    }

    /// Close a popped node and keep it as the best when its estimate is the lowest yet; its
    /// cell's edges before the close, None for a cell closed already.
    ///
    /// # Errors
    ///
    /// A cell off the grid, a node the pool does not hold.
    pub(super) fn close(&mut self, node_index: NodeIndex) -> GenieResult<Option<CellEdges>> {
        let node = *self.nodes.checked(node_index)?;
        let cell = self.cells.checked_mut(node.cell)?;
        let cell_edges = cell.edges;
        if cell_edges.contains(CellEdges::CLOSED) {
            return Ok(None);
        }
        cell.edges = cell_edges.plus(CellEdges::CLOSED);
        let distance = node.total - node.cost;
        if distance < self.best_cost {
            self.best_cost = distance;
            self.best = Some(node_index);
        }
        Ok(Some(cell_edges))
    }

    /// `FUN_140f78850`: A* over the node cells from the start's cell at `level`; Some node in the
    /// goal, None when the open list runs dry (or a computer player's search stops early, keeping
    /// the node it stopped at as the best).
    ///
    /// # Errors
    ///
    /// A cell off the grid, a node the pool does not hold, what the open list's sort fails on.
    pub(super) fn search_levels(
        &mut self,
        level: GridLevel,
        cell: PlanTile,
        start: PlanTile,
        full: bool,
        tile_path_tuning: &TilePathTuning,
    ) -> GenieResult<Option<NodeIndex>> {
        let first = self.cell(level, cell)?;
        if !first.kind.contains(CellKind::LONE) && first.edges.is_empty() {
            return Ok(None);
        }
        let h0 = self.start_estimate(start);
        let mut count: u32 = 0;
        let allocated = self.alloc(TileNode {
            at: cell,
            level,
            cost: Distance::ZERO,
            estimate: h0,
            parent: None,
            dir: CellEdges::NONE,
            entry: start,
        });
        let start_cell = self
            .cells
            .checked_mut(self.nodes.checked(allocated)?.cell)?;
        if start_cell.kind.contains(CellKind::GOAL) {
            return Ok(Some(allocated));
        }
        // the start's open mark, added without a test
        start_cell.edges = start_cell.edges.plus(CellEdges::OPEN);
        self.open.push(allocated, &self.nodes)?;
        self.best_cost = NO_COST;
        while !self.open.is_empty() {
            let Some(node_index) = self.open.pop()? else {
                return Ok(None);
            };
            count += 1;
            let Some(cell_edges) = self.close(node_index)? else {
                continue;
            };
            let node = *self.nodes.checked(node_index)?;
            let distance = node.total - node.cost;
            if !full
                && distance <= Distance::new(tile_path_tuning.early_left)
                && h0 <= Distance::new(tile_path_tuning.early_start)
                && (h0.get() as i32 * 10) as u32 <= count
            {
                self.best = Some(node_index);
                return Ok(None);
            }
            for (edge, dx, dy, back) in STEPS {
                if cell_edges.intersects(edge)
                    && let Some(found) = self.expand(
                        node.level,
                        node.at.stepped(dx, dy),
                        node.cost,
                        node_index,
                        back,
                    )?
                {
                    return Ok(Some(found));
                }
            }
        }
        Ok(None)
    }

    /// `FUN_140f79250`: A* over the tiles from the start's tile.
    ///
    /// # Errors
    ///
    /// A cell off the grid, a node the pool does not hold, what the open list's sort fails on.
    pub(super) fn search_tiles(
        &mut self,
        start: PlanTile,
        tile_path_tuning: &TilePathTuning,
    ) -> GenieResult<Option<NodeIndex>> {
        if self.cell(GridLevel::TILES, start)?.edges.is_empty() {
            return Ok(None);
        }
        let h0 = self.start_estimate(start);
        let allocated = self.alloc(TileNode {
            at: start,
            level: GridLevel::TILES,
            cost: Distance::ZERO,
            estimate: h0,
            parent: None,
            dir: CellEdges::NONE,
            entry: start,
        });
        let start_cell = self
            .cells
            .checked_mut(self.nodes.checked(allocated)?.cell)?;
        if start_cell.kind.contains(CellKind::GOAL) {
            return Ok(Some(allocated));
        }
        self.open.clear();
        // the start's open mark, added without a test
        start_cell.edges = start_cell.edges.plus(CellEdges::OPEN);
        self.open.push(allocated, &self.nodes)?;
        self.best_cost = NO_COST;
        while !self.open.is_empty() {
            let Some(node_index) = self.open.pop()? else {
                return Ok(None);
            };
            let Some(cell_edges) = self.close(node_index)? else {
                continue;
            };
            let node = *self.nodes.checked(node_index)?;
            for (edge, dx, dy, back) in STEPS {
                if cell_edges.intersects(edge)
                    && let Some(found) = self.expand0(
                        node.at.stepped(dx, dy),
                        node.cost,
                        node_index,
                        back,
                        tile_path_tuning,
                    )?
                {
                    return Ok(Some(found));
                }
            }
        }
        Ok(None)
    }
}
