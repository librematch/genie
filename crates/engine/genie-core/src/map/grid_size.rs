//! The size of a grid of tiles as one type ([`GridSize`]): its width and height, apart from a
//! tile on it.

use crate::map::Tile;

/// The size of a grid of tiles, its width (along x) and its height (along y), in tiles: the map's
/// (the obstruction manager's `+0xa8`, the pathing's), or an influence map's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GridSize(
    /// Its width and height.
    [i32; 2],
);

impl GridSize {
    /// The grid `width` tiles wide and `height` tiles high.
    #[must_use]
    pub const fn new(width: i32, height: i32) -> Self {
        Self([width, height])
    }

    /// Its width and height, in that order (a save's two integers).
    #[must_use]
    pub const fn get(self) -> [i32; 2] {
        self.0
    }

    /// Its width, along x.
    #[must_use]
    pub const fn width(self) -> i32 {
        self.0[0]
    }

    /// Its height, along y.
    #[must_use]
    pub const fn height(self) -> i32 {
        self.0[1]
    }

    /// Whether tile `t` lies on the grid: x from 0 to the width less 1, y from 0 to the height
    /// less 1.
    #[must_use]
    pub const fn holds(self, t: Tile) -> bool {
        0 <= t.x() && t.x() < self.0[0] && 0 <= t.y() && t.y() < self.0[1]
    }

    /// Tile `t`'s place in an array of the grid's tiles row by row (`y × width + x`); none off
    /// the grid.
    #[must_use]
    pub const fn index(self, t: Tile) -> Option<usize> {
        if self.holds(t) {
            Some((t.y() * self.0[0] + t.x()) as usize)
        } else {
            None
        }
    }
}

impl From<[i32; 2]> for GridSize {
    /// The size of two integers in their order (width, height), as a save holds them.
    fn from(c: [i32; 2]) -> Self {
        Self(c)
    }
}

impl genie_diff::Diff for GridSize {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::map::GridSize;
    use crate::map::Tile;

    /// A grid size keeps its width and height in their order.
    #[test]
    fn grid_size_keeps_its_width_and_height() {
        let s = GridSize::new(120, 80);
        assert_eq!((s.width(), s.height(), s.get()), (120, 80, [120, 80]));
        assert_eq!(GridSize::from([120, 80]), s);
        // a tile on it, row by row; one off any edge is not
        assert_eq!(s.index(Tile::new(0, 0)), Some(0));
        assert_eq!(s.index(Tile::new(119, 1)), Some(239));
        assert_eq!(s.index(Tile::new(119, 79)), Some(9599));
        for off in [(-1, 0), (0, -1), (120, 0), (0, 80)] {
            assert!(!s.holds(Tile::new(off.0, off.1)), "{off:?}");
            assert_eq!(s.index(Tile::new(off.0, off.1)), None);
        }
        assert_eq!(GridSize::default().get(), [0, 0]);
    }
}
