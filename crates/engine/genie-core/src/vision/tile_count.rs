//! A count of tiles a player explored ([`TileCount`]).

use std::ops::AddAssign;

/// A count of tiles a player explored: the visible map's `numberTilesExploredValue` (`+0x28`)
/// and what one exploration adds to it ([`crate::vision::visibility::Visibility::explore`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TileCount(i32);

impl TileCount {
    /// No tile.
    pub const ZERO: Self = Self(0);
    /// One tile.
    pub const ONE: Self = Self(1);

    /// A count of `n` tiles.
    #[must_use]
    pub const fn new(n: i32) -> Self {
        Self(n)
    }

    /// The count.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

impl AddAssign for TileCount {
    /// The game's integer add.
    fn add_assign(&mut self, other: Self) {
        self.0 += other.0;
    }
}

#[cfg(test)]
mod tests {
    use crate::vision::TileCount;

    /// A count adds tiles one by one and reads back.
    #[test]
    fn count_adds_tiles() {
        let mut n = TileCount::ZERO;
        n += TileCount::ONE;
        n += TileCount::new(2);
        assert_eq!(n.get(), 3);
        assert!(TileCount::ZERO < n);
    }
}
