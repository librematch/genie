//! A terrain restriction's planner ([`Planner`]) and its four-level grid: built, read and
//! rebuilt over a changed rectangle.

use crate::GenieError;
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
use crate::world::slots::CheckedIndex;

/// A terrain restriction's planner (`0x4a8` bytes, built by `FUN_140f74f90`).
#[derive(Clone, Debug, PartialEq)]
pub struct Planner {
    /// `+0x400`, `+0x402`: the map's size (as shorts).
    pub(super) size: GridSize,
    /// Per level: `+0x3f0` its width, `+0x3f8` its height (as shorts), `+0x3e0` its first cell.
    pub(super) levels: [GridSize; 4],
    pub(super) offsets: [i32; 4],
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
    /// Whether two planners hold the same grid: the map's size, the levels, the cells, the
    /// restriction's passability and the terrain. What a search leaves (the region, the goal, the
    /// last tile, the goal's marks, the nodes, the open list, the best node and the points) is
    /// not compared: a planner built afresh has searched nothing.
    #[must_use]
    pub fn same_grid(&self, other: &Self) -> bool {
        self.size == other.size
            && self.levels == other.levels
            && self.offsets == other.offsets
            && self.cells == other.cells
            && self.passable == other.passable
            && self.terrain == other.terrain
    }

    /// Build the grid (`FUN_140f74f90`): every cell's off-map children (`FUN_140f7da90`), then
    /// every tile's edges and the levels above them (`FUN_140f7e4b0` over the whole map).
    /// `terrain` holds each tile's terrain row by row, `passable` the restriction's value per
    /// terrain (above 0 passes).
    #[must_use]
    pub fn new(
        size: GridSize,
        terrain: Vec<TerrainId>,
        passable: Restriction,
        tiles: &Tiles,
    ) -> Self {
        let (w, h) = (size.width() as i16, size.height() as i16);
        let w0 = (w.wrapping_add(7)) & !7;
        let h0 = (h.wrapping_add(7)) & !7;
        let levels = [0, 1, 2, 3].map(|l| GridSize::new(i32::from(w0 >> l), i32::from(h0 >> l)));
        let area = |l: usize| levels[l].width() * levels[l].height();
        let offsets = [0, area(0), area(0) + area(1), area(0) + area(1) + area(2)];
        let total = (offsets[3] + area(3)).max(0) as usize;
        let mut planner = Self {
            size: GridSize::new(i32::from(w), i32::from(h)),
            levels,
            offsets,
            cells: Cells::cleared(total),
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
        for x in (0..w0 >> 3).rev() {
            for y in (0..h0 >> 3).rev() {
                planner.init_cell(GridLevel::TOP, PlanTile::new(x, y));
            }
        }
        planner.update(PlanRect::new(0, 0, w, h), tiles);
        planner
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
    #[must_use]
    pub fn level(&self, level: GridLevel) -> &[Cell] {
        let start = CellIndex::new(self.offsets[level.index()] as usize);
        let size = self.levels[level.index()];
        let n = (size.width() * size.height()) as usize;
        self.cells.run(start, n)
    }

    /// Whether the tile level's cell of `tile` has no edges, so no tile path leaves it (the
    /// builder's flood for more work, [`crate::gathering::gather_search::reached`]).
    ///
    /// # Errors
    ///
    /// A tile off the planner's map.
    pub fn tile_closed(&self, tile: Tile) -> Result<bool, GenieError> {
        let width = i64::from(self.level_size(GridLevel::TILES).width());
        let cell = (width * i64::from(tile.y()) + i64::from(tile.x())) as usize;
        Ok(self.level(GridLevel::TILES).checked(cell)?.edges.is_empty())
    }

    /// The size of `level`'s grid in cells.
    #[must_use]
    pub const fn level_size(&self, level: GridLevel) -> GridSize {
        self.levels[level.index()]
    }

    /// The cell `at` of `level`'s place in the cells.
    pub(super) fn index(&self, level: GridLevel, at: PlanTile) -> CellIndex {
        let l = level.index();
        let i = i64::from(self.levels[l].width()) * i64::from(at.y())
            + i64::from(at.x())
            + i64::from(self.offsets[l]);
        CellIndex::new(usize::try_from(i).expect("a cell outside the planner's grid"))
    }

    pub(super) fn cell(&self, level: GridLevel, at: PlanTile) -> Cell {
        self.cells[self.index(level, at)]
    }

    /// `FUN_140f7da90`: clear a cell and mark its children that lie off the map (its kind holds
    /// [`CellKind::OFF_MAP`] when all of it does).
    pub(super) fn init_cell(&mut self, level: GridLevel, at: PlanTile) {
        let i = self.index(level, at);
        self.cells[i] = Cell::EMPTY;
        if level == GridLevel::TILES {
            if self.size.width() <= i32::from(at.x()) || self.size.height() <= i32::from(at.y()) {
                self.cells[i].kind = CellKind::OFF_MAP;
            }
        } else {
            for (bit, child) in at.children().into_iter().enumerate() {
                self.init_cell(level.down(), child);
                if self
                    .cell(level.down(), child)
                    .kind
                    .contains(CellKind::OFF_MAP)
                {
                    self.cells[i].kind = self.cells[i].kind.plus(CellKind::child_off_map(bit));
                }
            }
        }
    }

    /// `FUN_140f799b0` (through the passability lambda, `0x140f7b110`): 1 when the tile is
    /// blocked, else a bit per passable neighbour (2 left, 4 right, 8 above, 0x10 below).
    pub(super) fn adjacency(&self, tile: Tile, tiles: &Tiles) -> u32 {
        let (w, h) = (self.size.width(), self.size.height());
        let passes = |t: Tile| {
            let kind = self.terrain[(t.y() * w + t.x()) as usize];
            self.passable.passes(kind) && !tiles.blocked(t)
        };
        if !passes(tile) {
            return 1;
        }
        let (x, y) = (tile.x(), tile.y());
        let [left, right, up, down] = tile.beside();
        let mut bits = 0;
        if 0 < x && passes(left) {
            bits = 2;
        }
        if x < w - 1 && passes(right) {
            bits |= 4;
        }
        if 0 < y && passes(up) {
            bits |= 8;
        }
        if y < h - 1 && passes(down) {
            bits |= 0x10;
        }
        bits
    }

    /// Rebuild the cells over a changed rectangle of tiles and its border (`FUN_140f7e4b0`, which
    /// the obstruction manager calls back through `FUN_140f7d4b0` when tiles change).
    pub fn update(&mut self, rect: PlanRect, tiles: &Tiles) {
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
                let _ = self.rebuild(GridLevel::TOP, PlanTile::new(cx, cy), tiles);
            }
        }
    }

    /// `FUN_140f7e5e0`: a cell's edges and kind from its tile (level 0) or its children; returns
    /// the cell.
    pub(super) fn rebuild(&mut self, level: GridLevel, at: PlanTile, tiles: &Tiles) -> Cell {
        let i = self.index(level, at);
        let old = self.cells[i].kind;
        let mut low = old.off_map();
        if level == GridLevel::TILES {
            self.cells[i].kind = low;
            if old.contains(CellKind::LONE) {
                self.cells[i].kind = low.plus(CellKind::LONE);
            }
            if self.region.holds_at(GridLevel::TILES, at) {
                let a = self.adjacency(at.tile(), tiles);
                self.cells[i].edges = CellEdges::NONE;
                if a & 1 == 0 {
                    let mut b = CellEdges::NONE;
                    if a & 2 != 0 {
                        b = CellEdges::W;
                    }
                    if a & 4 != 0 {
                        b |= CellEdges::E;
                    }
                    if a & 8 != 0 {
                        b |= CellEdges::N;
                    }
                    if a & 0x10 != 0 {
                        b |= CellEdges::S;
                    }
                    self.cells[i].edges = b;
                    if b.is_empty() {
                        self.cells[i].kind |= CellKind::LONE | CellKind::NODE;
                    } else {
                        self.cells[i].kind &= !CellKind::LONE;
                    }
                }
            }
            return self.cells[i];
        }
        let l = level.down();
        let children = at.children();
        let mut child = [Cell::EMPTY; 4];
        for (k, &c) in children.iter().enumerate() {
            if low.is_empty() || !self.cells[i].kind.intersects(CellKind::child_off_map(k)) {
                child[k] = self.rebuild(l, c, tiles);
            }
        }
        let [d0, d1, d2, d3] = child.map(|c| c.edges);
        low = self.cells[i].kind.off_map();
        // the game adds the four disjoint edges
        let dirs = ((d3 | d2) & CellEdges::S)
            .plus((d3 | d1) & CellEdges::E)
            .plus((d2 | d0) & CellEdges::W)
            .plus((d1 | d0) & CellEdges::N);
        let complex = child
            .iter()
            .any(|c| c.kind.intersects(CellKind::MIXED | CellKind::LONE))
            || self.splits(child.map(|c| c.edges), dirs);
        if complex {
            low = low.plus(CellKind::MIXED);
            for (k, &at) in children.iter().enumerate() {
                if !child[k].kind.intersects(CellKind::NODE | CellKind::MIXED) {
                    let c = self.index(l, at);
                    self.cells[c].kind = self.cells[c].kind.plus(CellKind::NODE);
                }
            }
        } else if level == GridLevel::TOP {
            low = low.plus(CellKind::NODE);
        }
        self.cells[i] = Cell {
            edges: dirs,
            kind: low,
        };
        self.cells[i]
    }

    /// Whether four children with edges `d` fall apart as one cell (`FUN_140f7e5e0`'s labels):
    /// three links among the ring of children, no edge out but a passable child, or children with
    /// edges that join none of the others.
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
        for b in labels {
            if b != 0 {
                if seen == 0 || seen == b {
                    seen = b;
                } else {
                    return true;
                }
            }
        }
        false
    }
}
