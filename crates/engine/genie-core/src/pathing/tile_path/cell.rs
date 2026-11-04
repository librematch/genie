//! A planner grid's cell ([`Cell`]): its edges with the search's marks ([`CellEdges`]) and its
//! kind ([`CellKind`]), the grid's two bytes; the grid ([`Cells`]) by a cell's place
//! ([`CellIndex`]).

use std::ops::{Index, IndexMut};

bitflags::bitflags! {
    /// A cell's byte 0: the edges a unit can cross to the cells beside it and, during a search,
    /// whether a node of the cell was opened or closed. A node's edge back to its parent is one
    /// of the four edges. Every bit is kept (`_ = !0`).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct CellEdges: u8 {
        /// The edge to the cell above (y − 1).
        const N = 1;
        /// The edge to the cell left.
        const W = 1 << 1;
        /// The edge to the cell right.
        const E = 1 << 2;
        /// The edge to the cell below (y + 1).
        const S = 1 << 3;
        /// During a search: a node of the cell was opened.
        const OPEN = 1 << 4;
        /// During a search: a node of the cell was closed.
        const CLOSED = 1 << 5;
        /// Any other bit.
        const _ = !0;
    }
}

bitflags::bitflags! {
    /// A cell's byte 1: the children that lie off the map (its low nibble, a bit per child),
    /// the goal's mark and what the search takes the cell for. Every bit is kept (`_ = !0`).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct CellKind: u8 {
        /// All four children lie off the map (a level-0 cell: the tile does).
        const OFF_MAP = 0xf;
        /// The goal's cell.
        const GOAL = 1 << 4;
        /// The cell is uniform and searched as one node.
        const NODE = 1 << 5;
        /// The cell's children are searched instead of it.
        const MIXED = 1 << 6;
        /// A passable tile with no passable neighbour.
        const LONE = 1 << 7;
        /// Any other bit.
        const _ = !0;
    }
}

impl CellEdges {
    /// No edge, no mark.
    pub const NONE: Self = Self::empty();
    /// The four edges.
    pub const EDGES: Self = Self::N.union(Self::W).union(Self::E).union(Self::S);

    /// The game's byte add of `flag` (`b + flag`): a mark the caller knows is clear, or the
    /// start's open mark, which the game adds without a test.
    #[must_use]
    pub const fn plus(self, flag: Self) -> Self {
        Self::from_bits_retain(self.bits().wrapping_add(flag.bits()))
    }

    /// The game's byte subtract of `flag` (`b - flag`): a mark the caller knows is set.
    #[must_use]
    pub const fn minus(self, flag: Self) -> Self {
        Self::from_bits_retain(self.bits().wrapping_sub(flag.bits()))
    }
}

impl CellKind {
    /// No mark.
    pub const NONE: Self = Self::empty();

    /// Child `k`'s off-map bit (`1 << k`).
    #[must_use]
    pub const fn child_off_map(k: usize) -> Self {
        Self::from_bits_retain(1 << k)
    }

    /// The low nibble: the children that lie off the map.
    #[must_use]
    pub const fn off_map(self) -> Self {
        self.intersection(Self::OFF_MAP)
    }

    /// The game's byte add of `flag` (`b + flag`), as [`CellEdges::plus`].
    #[must_use]
    pub const fn plus(self, flag: Self) -> Self {
        Self::from_bits_retain(self.bits().wrapping_add(flag.bits()))
    }

    /// The game's byte subtract of `flag` (`b - flag`), as [`CellEdges::minus`].
    #[must_use]
    pub const fn minus(self, flag: Self) -> Self {
        Self::from_bits_retain(self.bits().wrapping_sub(flag.bits()))
    }
}

/// A cell of a planner's grid (`+0x3d8`, two bytes): its edges and the search's marks, and its
/// kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Cell {
    /// Byte 0.
    pub(crate) edges: CellEdges,
    /// Byte 1.
    pub(crate) kind: CellKind,
}

impl Cell {
    /// A cleared cell.
    pub const EMPTY: Self = Self {
        edges: CellEdges::NONE,
        kind: CellKind::NONE,
    };

    /// The cell of the game's two bytes.
    #[must_use]
    pub const fn from_bytes([edges, kind]: [u8; 2]) -> Self {
        Self {
            edges: CellEdges::from_bits_retain(edges),
            kind: CellKind::from_bits_retain(kind),
        }
    }

    /// The game's two bytes.
    #[must_use]
    pub const fn bytes(self) -> [u8; 2] {
        [self.edges.bits(), self.kind.bits()]
    }
}

/// A cell's place in a planner's grid: the levels one after another, the tiles first, each
/// row by row (`+0x3d8`'s index, a node's `+0`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) struct CellIndex(
    /// The place.
    usize,
);

impl CellIndex {
    /// Place `i`.
    #[must_use]
    pub(super) const fn new(i: usize) -> Self {
        Self(i)
    }
}

/// A planner's grid (`+0x3d8`): every level's cells, read and changed by [`CellIndex`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Cells(Vec<Cell>);

impl Cells {
    /// `total` cleared cells.
    #[must_use]
    pub(super) fn cleared(total: usize) -> Self {
        Self(vec![Cell::EMPTY; total])
    }

    /// The `n` cells from place `start` on (a level's).
    #[must_use]
    pub(super) fn run(&self, start: CellIndex, n: usize) -> &[Cell] {
        &self.0[start.0..start.0 + n]
    }
}

impl Index<CellIndex> for Cells {
    type Output = Cell;

    fn index(&self, i: CellIndex) -> &Cell {
        &self.0[i.0]
    }
}

impl IndexMut<CellIndex> for Cells {
    fn index_mut(&mut self, i: CellIndex) -> &mut Cell {
        &mut self.0[i.0]
    }
}
