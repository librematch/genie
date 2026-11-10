//! A tile of the map as one type ([`Tile`]): x and y in whole tiles, apart from a point on the
//! ground ([`Point`]) and a step from one tile to another ([`TileOffset`]).

use crate::geometry::Point;
use std::ops::{Add, Sub, SubAssign};

/// A tile of the map, x and y in whole tiles: the tile a place's x and y fall in (each cut to a
/// whole number the way its routine cuts it), a search's or a path's tile, a landing's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Tile(
    /// Its x and y.
    [i32; 2],
);

impl Tile {
    /// The tile at `x`, `y`.
    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self([x, y])
    }

    /// Its x and y, in that order (a save's two integers).
    #[must_use]
    pub const fn get(self) -> [i32; 2] {
        self.0
    }

    /// Its x.
    #[must_use]
    pub const fn x(self) -> i32 {
        self.0[0]
    }

    /// Its y.
    #[must_use]
    pub const fn y(self) -> i32 {
        self.0[1]
    }

    /// The four tiles beside it in the order the floods try them: left, right, then down and up
    /// (x − 1, x + 1, y − 1, y + 1).
    #[must_use]
    pub const fn beside(self) -> [Self; 4] {
        let [x, y] = self.0;
        [
            Self([x - 1, y]),
            Self([x + 1, y]),
            Self([x, y - 1]),
            Self([x, y + 1]),
        ]
    }

    /// The point at its low corner: x and y as floats.
    #[must_use]
    pub const fn corner(self) -> Point {
        Point::new(self.0[0] as f32, self.0[1] as f32)
    }

    /// The tile a routine on shorts reads (`FUN_140f690e0`'s): x and y cut to their low 16 bits,
    /// signed.
    #[must_use]
    pub const fn short(self) -> Self {
        Self([self.0[0] as i16 as i32, self.0[1] as i16 as i32])
    }
}

/// A box of whole tiles from x0, y0 to x1, y1, both corners included: a target's footprint's
/// tiles, a query's cells.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TileRect(
    /// Its x0, y0, x1 and y1.
    [i32; 4],
);

impl TileRect {
    /// The box from tile (`x0`, `y0`) to tile (`x1`, `y1`).
    #[must_use]
    pub const fn new(x0: i32, y0: i32, x1: i32, y1: i32) -> Self {
        Self([x0, y0, x1, y1])
    }

    /// The box of the one tile `tile`.
    #[must_use]
    pub const fn of(tile: Tile) -> Self {
        Self([tile.x(), tile.y(), tile.x(), tile.y()])
    }

    /// Its x0, y0, x1 and y1, in that order.
    #[must_use]
    pub const fn get(self) -> [i32; 4] {
        self.0
    }

    /// Its first corner's x.
    #[must_use]
    pub const fn x0(self) -> i32 {
        self.0[0]
    }

    /// Its first corner's y.
    #[must_use]
    pub const fn y0(self) -> i32 {
        self.0[1]
    }

    /// Its second corner's x.
    #[must_use]
    pub const fn x1(self) -> i32 {
        self.0[2]
    }

    /// Its second corner's y.
    #[must_use]
    pub const fn y1(self) -> i32 {
        self.0[3]
    }

    /// Whether it holds tile `tile`: `x0 <= x <= x1` and `y0 <= y <= y1`.
    #[must_use]
    pub const fn holds(self, tile: Tile) -> bool {
        self.0[0] <= tile.x()
            && tile.x() <= self.0[2]
            && self.0[1] <= tile.y()
            && tile.y() <= self.0[3]
    }

    /// Whether it and `other` share a tile, edges included: `x0 <= other.x1`,
    /// `other.x0 <= x1`, then the same on y.
    #[must_use]
    pub const fn overlaps(self, other: Self) -> bool {
        self.0[0] <= other.0[2]
            && other.0[0] <= self.0[2]
            && self.0[1] <= other.0[3]
            && other.0[1] <= self.0[3]
    }
}

impl From<[i32; 4]> for TileRect {
    /// The box of four integers in their order (x0, y0, x1, y1).
    fn from(corners: [i32; 4]) -> Self {
        Self(corners)
    }
}

impl From<[i32; 2]> for Tile {
    /// The tile of two integers in the map's order (x, y), as a save holds them.
    fn from(components: [i32; 2]) -> Self {
        Self(components)
    }
}

impl Add<TileOffset> for Tile {
    type Output = Self;

    /// The tile `by` steps this one to: its x plus the step's, its y plus the step's.
    fn add(self, by: TileOffset) -> Self {
        Self([self.0[0] + by.dx(), self.0[1] + by.dy()])
    }
}

impl Sub<TileOffset> for Tile {
    type Output = Self;

    /// The tile `by` stepped to this one from: its x less the step's, its y less the step's.
    fn sub(self, by: TileOffset) -> Self {
        Self([self.0[0] - by.dx(), self.0[1] - by.dy()])
    }
}

impl SubAssign<TileOffset> for Tile {
    /// The tile steps back by `by` (`-`).
    fn sub_assign(&mut self, by: TileOffset) {
        *self = *self - by;
    }
}

impl genie_diff::Diff for Tile {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

/// A step from one tile to another, x and y in whole tiles: a direction of a search's table, a
/// building's footprint offset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TileOffset(
    /// Its x and y.
    [i32; 2],
);

impl TileOffset {
    /// The step by `dx` and `dy`.
    #[must_use]
    pub const fn new(dx: i32, dy: i32) -> Self {
        Self([dx, dy])
    }

    /// The steps of a table of x and y pairs, in its order.
    #[must_use]
    pub const fn table<const N: usize>(pairs: [[i32; 2]; N]) -> [Self; N] {
        let mut out = [Self([0; 2]); N];
        let mut index = 0;
        while index < N {
            out[index] = Self(pairs[index]);
            index += 1;
        }
        out
    }

    /// Its x.
    #[must_use]
    pub const fn dx(self) -> i32 {
        self.0[0]
    }

    /// Its y.
    #[must_use]
    pub const fn dy(self) -> i32 {
        self.0[1]
    }
}

#[cfg(test)]
mod tests {
    use crate::geometry::Point;
    use crate::geometry::{Tile, TileOffset, TileRect};

    /// A tile keeps its x and y, orders by x then y, and steps by an offset (`+`, `-`, `-=`).
    #[test]
    fn tile_keeps_its_place_orders_by_column_then_row_and_steps() {
        let tile = Tile::new(3, -4);
        assert_eq!((tile.x(), tile.y(), tile.get()), (3, -4, [3, -4]));
        assert_eq!(Tile::from([3, -4]), tile);
        assert!(Tile::new(2, 9) < Tile::new(3, 0) && Tile::new(3, 0) < Tile::new(3, 1));
        assert_eq!(tile + TileOffset::new(-1, 1), Tile::new(2, -3));
        assert_eq!(tile - TileOffset::new(-1, 1), Tile::new(4, -5));
        let mut back = tile;
        back -= TileOffset::new(-1, 1);
        assert_eq!(back, Tile::new(4, -5));
        assert_eq!(
            tile.beside(),
            [
                Tile::new(2, -4),
                Tile::new(4, -4),
                Tile::new(3, -5),
                Tile::new(3, -3)
            ]
        );
        assert_eq!(
            TileOffset::table([[1, 0], [0, -1]]),
            [TileOffset::new(1, 0), TileOffset::new(0, -1)]
        );
        let tile_offset = TileOffset::new(5, 6);
        assert_eq!((tile_offset.dx(), tile_offset.dy()), (5, 6));
        // a routine on shorts reads the low 16 bits, signed
        assert_eq!(tile.short(), tile);
        assert_eq!(tile.corner(), Point::new(3.0, -4.0));
        assert_eq!(Tile::new(0x1_0005, 0x8000).short(), Tile::new(5, -0x8000));
    }

    /// A tile box holds the tiles between its corners, the corners included.
    #[test]
    fn tile_box_holds_the_tiles_between_its_corners() {
        let tile_rect = TileRect::new(1, 2, 3, 4);
        assert_eq!(
            (
                tile_rect.x0(),
                tile_rect.y0(),
                tile_rect.x1(),
                tile_rect.y1(),
                tile_rect.get()
            ),
            (1, 2, 3, 4, [1, 2, 3, 4])
        );
        assert!(
            tile_rect.holds(Tile::new(1, 2))
                && tile_rect.holds(Tile::new(3, 4))
                && tile_rect.holds(Tile::new(2, 3))
        );
        assert!(!tile_rect.holds(Tile::new(0, 3)) && !tile_rect.holds(Tile::new(2, 5)));
        assert_eq!(TileRect::of(Tile::new(5, 6)), TileRect::new(5, 6, 5, 6));
        assert_eq!(TileRect::from([1, 2, 3, 4]), tile_rect);
    }

    /// Two tile boxes overlap while they share a tile on both axes, a shared edge included.
    #[test]
    fn tile_boxes_overlap_while_they_share_tile() {
        let tile_rect = TileRect::new(10, 10, 20, 20);
        for other in [
            TileRect::new(20, 20, 30, 30),
            TileRect::new(0, 0, 10, 10),
            TileRect::new(12, 0, 14, 40),
            TileRect::new(0, 15, 40, 15),
        ] {
            assert!(
                tile_rect.overlaps(other) && other.overlaps(tile_rect),
                "{other:?}"
            );
        }
        for other in [
            TileRect::new(21, 10, 30, 20),
            TileRect::new(0, 10, 9, 20),
            TileRect::new(10, 21, 20, 30),
            TileRect::new(10, 0, 20, 9),
        ] {
            assert!(
                !tile_rect.overlaps(other) && !other.overlaps(tile_rect),
                "{other:?}"
            );
        }
    }
}
