//! A terrain restriction's planner ([`Planner`]) and its four-level grid: built, read and
//! rebuilt over a changed rectangle.

use crate::GenieResult;
use crate::map::Distance;
use crate::map::GridSize;
use crate::map::Restriction;
use crate::map::TerrainId;
use crate::map::Tile;
use crate::map::spatial::Tiles;
use crate::math::fmath::BVector;
use crate::pathing::tile_path::{
    Cell, CellEdges, CellIndex, CellKind, Cells, GridLevel, NO_COST, NodeIndex, Nodes, OpenList,
    PlanRect, PlanTile,
};
use crate::world::slots::CheckedIndexExt;

/// A terrain restriction's planner (`0x4a8` bytes, built by `FUN_140f74f90`).
#[derive(Clone, Debug, PartialEq)]
pub struct Planner {
    /// `+0x400`, `+0x402`: the map's size (as shorts). The levels' sizes (`+0x3f0`, `+0x3f8`)
    /// and first cells (`+0x3e0`) follow from it ([`Planner::level_size`]).
    pub(super) size: GridSize,
    pub(super) cells: Cells,
    /// `+0x3c0`: the restriction's passability per terrain.
    pub(super) passable: Restriction,
    /// Each tile's terrain (the map's tile `+9`), row by row.
    pub(super) terrain: Vec<TerrainId>,
    /// `+0x408`..`+0x40e`: the tiles an update or the goal's marks cover.
    pub(super) region: PlanRect,
    /// `+0x410`, `+0x412`: the goal's tile.
    pub(super) goal: PlanTile,
    /// `+0x414`, `+0x416`: the tile the path was last built to.
    pub(super) last: PlanTile,
    /// `+0x478`: the goal's marks found a node cell.
    pub(super) goal_marked: bool,
    pub(super) nodes: Nodes,
    pub(super) open: OpenList,
    /// `+0x470` and `+0xb8`: the node nearest the goal and its estimate.
    pub(super) best: Option<NodeIndex>,
    pub(super) best_cost: Distance,
    /// `+0x448`: the path's points from the goal back.
    pub(super) points: Vec<BVector>,
}

impl Planner {
    /// Whether two planners hold the same grid: the map's size, the cells, the restriction's
    /// passability and the terrain. What a search leaves (the region, the goal, the last tile,
    /// the goal's marks, the nodes, the open list, the best node and the points) is not
    /// compared: a planner built afresh has searched nothing.
    #[must_use]
    pub fn same_grid(&self, other: &Self) -> bool {
        self.size == other.size
            && self.cells == other.cells
            && self.passable == other.passable
            && self.terrain == other.terrain
    }

    /// Build the grid (`FUN_140f74f90`): every cell's off-map children (`FUN_140f7da90`), then
    /// every tile's edges and the levels above them (`FUN_140f7e4b0` over the whole map).
    /// `terrain` holds each tile's terrain row by row, `passable` the restriction's value per
    /// terrain (above 0 passes).
    ///
    /// # Errors
    ///
    /// A cell or a terrain the build reads off the grid, which its walk over the map's cells
    /// never does.
    pub fn new(
        size: GridSize,
        terrain: Vec<TerrainId>,
        passable: Restriction,
        tiles: &Tiles,
    ) -> GenieResult<Self> {
        let (width, height) = (size.width() as i16, size.height() as i16);
        let mut planner = Self {
            size: GridSize::new(i32::from(width), i32::from(height)),
            cells: Cells::default(),
            passable,
            terrain,
            region: PlanRect::default(),
            goal: PlanTile::default(),
            last: PlanTile::default(),
            goal_marked: false,
            nodes: Nodes::default(),
            open: OpenList::default(),
            best: None,
            best_cost: NO_COST,
            points: Vec::new(),
        };
        let total = planner.first_cell(GridLevel::TOP) + planner.level_area(GridLevel::TOP);
        planner.cells = Cells::cleared(total.max(0) as usize);
        let top = planner.level_size(GridLevel::TOP);
        for x in (0..top.width() as i16).rev() {
            for y in (0..top.height() as i16).rev() {
                planner.init_cell(GridLevel::TOP, PlanTile::new(x, y))?;
            }
        }
        planner.update(PlanRect::new(0, 0, width, height), tiles)?;
        Ok(planner)
    }

    /// The map's last tile (its width and height less one) as the planner's shorts.
    pub(super) const fn last_tile(&self) -> PlanTile {
        PlanTile::new(self.size.width() as i16 - 1, self.size.height() as i16 - 1)
    }

    /// The restriction's passability per terrain the planner was made for.
    #[must_use]
    pub const fn passable(&self) -> &Restriction {
        &self.passable
    }

    /// The grid of `level`, row by row.
    ///
    /// # Errors
    ///
    /// A level whose cells the grid does not hold (none of the four).
    pub fn level(&self, level: GridLevel) -> GenieResult<&[Cell]> {
        let count = self.level_area(level).max(0) as usize;
        self.cells
            .run(CellIndex::new(self.first_cell(level)), count)
    }

    /// Whether the tile level's cell of `tile` has no edges, so no tile path leaves it (the
    /// builder's flood for more work, [`crate::gathering::gather_search::reached`]).
    ///
    /// # Errors
    ///
    /// A tile off the planner's map.
    pub fn tile_closed(&self, tile: Tile) -> GenieResult<bool> {
        let width = i64::from(self.level_size(GridLevel::TILES).width());
        let cell = (width * i64::from(tile.y()) + i64::from(tile.x())) as usize;
        Ok(self
            .level(GridLevel::TILES)?
            .checked(cell)?
            .edges
            .is_empty())
    }

    /// The size of `level`'s grid in cells (`+0x3f0`, `+0x3f8`): the map's as the planner's
    /// shorts, rounded up to whole top cells of 8 × 8 tiles, then shifted down by the level.
    #[must_use]
    pub const fn level_size(&self, level: GridLevel) -> GridSize {
        let width = (self.size.width() as i16).wrapping_add(7) & !7;
        let height = (self.size.height() as i16).wrapping_add(7) & !7;
        let shift = level.get();
        GridSize::new((width >> shift) as i32, (height >> shift) as i32)
    }

    /// The number of `level`'s cells.
    const fn level_area(&self, level: GridLevel) -> i64 {
        let size = self.level_size(level);
        size.width() as i64 * size.height() as i64
    }

    /// `level`'s first cell in the grid (`+0x3e0`): the cells of the levels below it.
    fn first_cell(&self, level: GridLevel) -> i64 {
        GridLevel::ALL
            .into_iter()
            .take_while(|&below| below < level)
            .map(|below| self.level_area(below))
            .sum()
    }

    /// The cell `at` of `level`'s place in the cells: the level's first cell, plus its width
    /// times y, plus x.
    pub(super) fn index(&self, level: GridLevel, at: PlanTile) -> CellIndex {
        CellIndex::new(
            i64::from(self.level_size(level).width()) * i64::from(at.y())
                + i64::from(at.x())
                + self.first_cell(level),
        )
    }

    /// The cell `at` of `level`.
    ///
    /// # Errors
    ///
    /// A place off the grid.
    pub(super) fn cell(&self, level: GridLevel, at: PlanTile) -> GenieResult<Cell> {
        self.cells.checked(self.index(level, at)).copied()
    }

    /// The cell `at` of `level`, to change.
    ///
    /// # Errors
    ///
    /// A place off the grid.
    pub(super) fn cell_mut(&mut self, level: GridLevel, at: PlanTile) -> GenieResult<&mut Cell> {
        let cell_index = self.index(level, at);
        self.cells.checked_mut(cell_index)
    }

    /// `FUN_140f7da90`: clear a cell and mark its children that lie off the map (its kind holds
    /// [`CellKind::OFF_MAP`] when all of it does).
    ///
    /// # Errors
    ///
    /// A place off the grid.
    pub(super) fn init_cell(&mut self, level: GridLevel, at: PlanTile) -> GenieResult<()> {
        *self.cell_mut(level, at)? = Cell::EMPTY;
        if level == GridLevel::TILES {
            if self.size.width() <= i32::from(at.x()) || self.size.height() <= i32::from(at.y()) {
                self.cell_mut(level, at)?.kind = CellKind::OFF_MAP;
            }
        } else {
            for (bit, child) in at.children().into_iter().enumerate() {
                self.init_cell(level.down(), child)?;
                if self
                    .cell(level.down(), child)?
                    .kind
                    .contains(CellKind::OFF_MAP)
                {
                    let cell = self.cell_mut(level, at)?;
                    cell.kind = cell.kind.plus(CellKind::child_off_map(bit));
                }
            }
        }
        Ok(())
    }

    /// `FUN_140f799b0` (through the passability lambda, `0x140f7b110`): 1 when the tile is
    /// blocked, else a bit per passable neighbour (2 left, 4 right, 8 above, 0x10 below).
    ///
    /// # Errors
    ///
    /// A tile off the map, whose terrain the planner has not got.
    pub(super) fn adjacency(&self, tile: Tile, tiles: &Tiles) -> GenieResult<u32> {
        let (width, height) = (self.size.width(), self.size.height());
        let passes = |tile: Tile| -> GenieResult<bool> {
            let kind = *self
                .terrain
                .checked((tile.y() * width + tile.x()) as usize)
                .map_err(|error| error.with("x", tile.x()).with("y", tile.y()))?;
            Ok(self.passable.passes(kind) && !tiles.blocked(tile))
        };
        if !passes(tile)? {
            return Ok(1);
        }
        let (x, y) = (tile.x(), tile.y());
        let [left, right, up, down] = tile.beside();
        let mut bits = 0;
        if 0 < x && passes(left)? {
            bits = 2;
        }
        if x < width - 1 && passes(right)? {
            bits |= 4;
        }
        if 0 < y && passes(up)? {
            bits |= 8;
        }
        if y < height - 1 && passes(down)? {
            bits |= 0x10;
        }
        Ok(bits)
    }

    /// Rebuild the cells over a changed rectangle of tiles and its border (`FUN_140f7e4b0`, which
    /// the obstruction manager calls back through `FUN_140f7d4b0` when tiles change).
    ///
    /// # Errors
    ///
    /// A cell or a terrain the rebuild reads off the grid, which its walk over the map's cells
    /// never does.
    pub fn update(&mut self, rect: PlanRect, tiles: &Tiles) -> GenieResult<()> {
        let [x0, y0, x1, y1] = rect.get();
        let (lo_x, hi_x) = (x0.min(x1), x0.max(x1).wrapping_add(1));
        let (lo_y, hi_y) = (y0.min(y1), y0.max(y1).wrapping_add(1));
        let last = self.last_tile();
        let rx0 = (lo_x - 1).max(0);
        let ry0 = (lo_y - 1).max(0);
        let rx1 = if last.x() < hi_x { last.x() } else { hi_x };
        let ry1 = if last.y() < hi_y { last.y() } else { hi_y };
        self.region = PlanRect::new(rx0, ry0, rx1, ry1);
        for cx in (rx0 >> 3)..=(rx1 >> 3) {
            for cy in (ry0 >> 3)..=(ry1 >> 3) {
                let _ = self.rebuild(GridLevel::TOP, PlanTile::new(cx, cy), tiles)?;
            }
        }
        Ok(())
    }

    /// `FUN_140f7e5e0`: a cell's edges and kind from its tile (level 0) or its children; returns
    /// the cell.
    ///
    /// # Errors
    ///
    /// A cell or a terrain off the grid.
    pub(super) fn rebuild(
        &mut self,
        level: GridLevel,
        at: PlanTile,
        tiles: &Tiles,
    ) -> GenieResult<Cell> {
        if level == GridLevel::TILES {
            return self.rebuild_tile(at, tiles);
        }
        let old = self.cell(level, at)?.kind;
        let mut low = old.off_map();
        let grid_level = level.down();
        let children = at.children();
        let mut child = [Cell::EMPTY; 4];
        for (bit, (&plan_tile, rebuilt)) in children.iter().zip(&mut child).enumerate() {
            if low.is_empty() || !old.intersects(CellKind::child_off_map(bit)) {
                *rebuilt = self.rebuild(grid_level, plan_tile, tiles)?;
            }
        }
        let [d0, d1, d2, d3] = child.map(|cell| cell.edges);
        // the game adds the four disjoint edges
        let dirs = ((d3 | d2) & CellEdges::S)
            .plus((d3 | d1) & CellEdges::E)
            .plus((d2 | d0) & CellEdges::W)
            .plus((d1 | d0) & CellEdges::N);
        let complex = child
            .iter()
            .any(|cell| cell.kind.intersects(CellKind::MIXED | CellKind::LONE))
            || self.splits(child.map(|cell| cell.edges), dirs);
        if complex {
            low = low.plus(CellKind::MIXED);
            for (&plan_tile, rebuilt) in children.iter().zip(&child) {
                if !rebuilt.kind.intersects(CellKind::NODE | CellKind::MIXED) {
                    let cell = self.cell_mut(grid_level, plan_tile)?;
                    cell.kind = cell.kind.plus(CellKind::NODE);
                }
            }
        } else if level == GridLevel::TOP {
            low = low.plus(CellKind::NODE);
        }
        let cell = Cell {
            edges: dirs,
            kind: low,
        };
        *self.cell_mut(level, at)? = cell;
        Ok(cell)
    }

    /// `FUN_140f7e5e0` at level 0: a tile's kind keeps its off-map and lone marks; within the
    /// region its edges are its passable neighbours' (none for a blocked tile), and a passable
    /// tile with none is a lone node.
    ///
    /// # Errors
    ///
    /// A cell or a terrain off the grid.
    fn rebuild_tile(&mut self, at: PlanTile, tiles: &Tiles) -> GenieResult<Cell> {
        // the neighbours are read from the terrain and the blocked tiles, not the cells
        let adjacency = if self.region.holds_at(GridLevel::TILES, at) {
            Some(self.adjacency(at.tile(), tiles)?)
        } else {
            None
        };
        let cell = self.cell_mut(GridLevel::TILES, at)?;
        let old = cell.kind;
        let low = old.off_map();
        cell.kind = low;
        if old.contains(CellKind::LONE) {
            cell.kind = low.plus(CellKind::LONE);
        }
        if let Some(adjacency) = adjacency {
            cell.edges = CellEdges::NONE;
            if adjacency & 1 == 0 {
                let mut cell_edges = CellEdges::NONE;
                if adjacency & 2 != 0 {
                    cell_edges = CellEdges::W;
                }
                if adjacency & 4 != 0 {
                    cell_edges |= CellEdges::E;
                }
                if adjacency & 8 != 0 {
                    cell_edges |= CellEdges::N;
                }
                if adjacency & 0x10 != 0 {
                    cell_edges |= CellEdges::S;
                }
                cell.edges = cell_edges;
                if cell_edges.is_empty() {
                    cell.kind |= CellKind::LONE | CellKind::NODE;
                } else {
                    cell.kind &= !CellKind::LONE;
                }
            }
        }
        Ok(*cell)
    }

    /// Whether four children with edges `d0` to `d3` fall apart as one cell (`FUN_140f7e5e0`'s
    /// labels): three links among the ring of children, no edge out but a passable child, or
    /// children with edges that join none of the others.
    pub(super) fn splits(&self, [d0, d1, d2, d3]: [CellEdges; 4], dirs: CellEdges) -> bool {
        let link01 = d0.contains(CellEdges::E);
        let mut b4: u8 = if d1.is_empty() { 0 } else { 2 };
        if link01 {
            b4 = u8::from(!d0.is_empty());
        }
        let mut links: u8 = u8::from(link01) + 1;
        let mut b15 = b4;
        if !d1.contains(CellEdges::S) {
            links = u8::from(link01);
            b15 = if d3.is_empty() { 0 } else { 4 };
        }
        let mut links2 = links + 1;
        let mut b10 = b15;
        if !d3.contains(CellEdges::W) {
            links2 = links;
            b10 = if d2.is_empty() { 0 } else { 3 };
        }
        let mut links3 = links2 + 1;
        let mut b9 = b10;
        if !d2.contains(CellEdges::N) {
            b9 = u8::from(!d0.is_empty());
            links3 = links2;
        }
        if link01 {
            b4 = b9;
        }
        if d1.contains(CellEdges::S) {
            b15 = b4;
        }
        let labels = [b9, b4, b10, b15];
        if links3 == 3 || (dirs.is_empty() && labels != [0; 4]) {
            return true;
        }
        let mut seen = 0;
        for label in labels {
            if label != 0 {
                if seen == 0 || seen == label {
                    seen = label;
                } else {
                    return true;
                }
            }
        }
        false
    }
}
