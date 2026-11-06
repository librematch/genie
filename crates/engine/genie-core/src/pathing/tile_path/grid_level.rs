//! A level of a planner's grid ([`GridLevel`]).

/// A level of a planner's grid: 0 the tiles, each level up cells of twice the side, 3 the top
/// (8 × 8 tiles), as the planner's byte (a node's `+0x18`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GridLevel(
    /// The level, 0 to 3.
    u8,
);

impl GridLevel {
    /// Level 0: a cell per tile.
    pub const TILES: Self = Self(0);
    /// Level 3: a cell per 8 × 8 tiles, where the searches and the updates start.
    pub const TOP: Self = Self(3);
    /// The four levels, the tiles first.
    pub const ALL: [Self; 4] = [Self(0), Self(1), Self(2), Self(3)];

    /// Level `level`.
    #[must_use]
    pub const fn new(level: u8) -> Self {
        Self(level)
    }

    /// The level as a number, the shift from a tile to its cell.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// The level above.
    #[must_use]
    pub const fn up(self) -> Self {
        Self(self.0 + 1)
    }

    /// The level below, its cells' children.
    #[must_use]
    pub const fn down(self) -> Self {
        Self(self.0 - 1)
    }
}
