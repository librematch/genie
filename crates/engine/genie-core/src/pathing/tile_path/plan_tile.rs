//! The planner's places as its shorts: a tile or a cell of a level ([`PlanTile`]) and a
//! rectangle of tiles ([`PlanRect`]).

use crate::map::Point;
use crate::map::Tile;
use crate::pathing::tile_path::GridLevel;

/// A place on the tile planner's grid as the planner's shorts (x, y): at level 0 a tile, at
/// level `l` a cell of 2^l × 2^l tiles (`+8`, `+0xa` of a node; the goal's tile `+0x410`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PlanTile(
    /// Its x and y.
    [i16; 2],
);

impl PlanTile {
    /// The place (`x`, `y`).
    #[must_use]
    pub const fn new(x: i16, y: i16) -> Self {
        Self([x, y])
    }

    /// The tile of a point, each coordinate `(short)(int)v` as the planner cuts it.
    #[must_use]
    pub const fn of_point(point: Point) -> Self {
        Self([point.x() as i32 as i16, point.y() as i32 as i16])
    }

    /// Its x and y.
    #[must_use]
    pub const fn get(self) -> [i16; 2] {
        self.0
    }

    /// Its x.
    #[must_use]
    pub const fn x(self) -> i16 {
        self.0[0]
    }

    /// Its y.
    #[must_use]
    pub const fn y(self) -> i16 {
        self.0[1]
    }

    /// The place `dx`, `dy` from it.
    #[must_use]
    pub const fn stepped(self, dx: i16, dy: i16) -> Self {
        Self([self.0[0] + dx, self.0[1] + dy])
    }

    /// The cell holding a tile at `level`: x and y shifted right by the level.
    #[must_use]
    pub const fn at_level(self, level: GridLevel) -> Self {
        let l = level.get();
        Self([self.0[0] >> l, self.0[1] >> l])
    }

    /// The cell holding a cell a level up: x and y shifted right by one.
    #[must_use]
    pub const fn parent(self) -> Self {
        Self([self.0[0] >> 1, self.0[1] >> 1])
    }

    /// A cell's four children a level down, in the game's order: (2x, 2y), (2x + 1, 2y),
    /// (2x, 2y + 1), (2x + 1, 2y + 1), the doubling wrapping as the game's shorts.
    #[must_use]
    pub const fn children(self) -> [Self; 4] {
        let (x2, y2) = (self.0[0].wrapping_mul(2), self.0[1].wrapping_mul(2));
        [
            Self([x2, y2]),
            Self([x2 + 1, y2]),
            Self([x2, y2 + 1]),
            Self([x2 + 1, y2 + 1]),
        ]
    }

    /// A cell's first tile at `level`: x and y shifted left.
    #[must_use]
    pub const fn corner_at(self, level: GridLevel) -> Self {
        let l = level.get();
        Self([self.0[0] << l, self.0[1] << l])
    }

    /// A cell's centre tile at `level` (its first tile plus half its side); the tile itself at
    /// level 0.
    #[must_use]
    pub const fn centre_at(self, level: GridLevel) -> Self {
        let l = level.get();
        if l == 0 {
            return self;
        }
        let half = 1i16 << (l - 1);
        Self([(self.0[0] << l) + half, (self.0[1] << l) + half])
    }

    /// The tile at level 0, widened.
    #[must_use]
    pub fn tile(self) -> Tile {
        Tile::new(i32::from(self.0[0]), i32::from(self.0[1]))
    }
}

/// A rectangle of the planner's tiles from x0, y0 to x1, y1, both corners included, as its
/// shorts: the tiles an update or the goal's marks cover (`+0x408`..`+0x40e`), the tiles whose
/// static marks changed, a search's closed tiles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PlanRect(
    /// x0, y0, x1, y1.
    [i16; 4],
);

impl PlanRect {
    /// The rectangle from (`x0`, `y0`) to (`x1`, `y1`).
    #[must_use]
    pub const fn new(x0: i16, y0: i16, x1: i16, y1: i16) -> Self {
        Self([x0, y0, x1, y1])
    }

    /// x0, y0, x1, y1.
    #[must_use]
    pub const fn get(self) -> [i16; 4] {
        self.0
    }

    /// Whether it holds `cell` of `level`: the corners shifted to the level, both included.
    #[must_use]
    pub const fn holds_at(self, level: GridLevel, cell: PlanTile) -> bool {
        let [x0, y0, x1, y1] = self.0;
        let l = level.get();
        x0 >> l <= cell.x() && y0 >> l <= cell.y() && cell.x() <= x1 >> l && cell.y() <= y1 >> l
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_tile_steps_shifts_and_widens() {
        let plan_tile = PlanTile::new(13, -2);
        assert_eq!(
            (plan_tile.x(), plan_tile.y(), plan_tile.get()),
            (13, -2, [13, -2])
        );
        assert_eq!(plan_tile.stepped(-1, 3), PlanTile::new(12, 1));
        assert_eq!(
            PlanTile::new(13, 6).at_level(GridLevel::new(2)),
            PlanTile::new(3, 1)
        );
        assert_eq!(PlanTile::new(13, 6).parent(), PlanTile::new(6, 3));
        assert_eq!(plan_tile.tile(), Tile::new(13, -2));
        assert_eq!(
            PlanTile::of_point(Point::new(7.9, -0.5)),
            PlanTile::new(7, 0)
        );
    }

    #[test]
    fn cell_children_go_in_the_game_order_and_its_centre_by_level() {
        assert_eq!(
            PlanTile::new(3, 1).children(),
            [
                PlanTile::new(6, 2),
                PlanTile::new(7, 2),
                PlanTile::new(6, 3),
                PlanTile::new(7, 3)
            ]
        );
        // the doubling wraps as a short
        assert_eq!(
            PlanTile::new(0x4000, 0).children()[0],
            PlanTile::new(-0x8000, 0)
        );
        let plan_tile = PlanTile::new(3, 1);
        let two = GridLevel::new(2);
        assert_eq!(plan_tile.corner_at(two), PlanTile::new(12, 4));
        assert_eq!(plan_tile.centre_at(two), PlanTile::new(14, 6));
        assert_eq!(plan_tile.centre_at(GridLevel::TILES), plan_tile);
    }

    #[test]
    fn rect_holds_cell_of_level_by_its_shifted_corners() {
        let plan_rect = PlanRect::new(8, 4, 23, 15);
        assert_eq!(plan_rect.get(), [8, 4, 23, 15]);
        assert!(plan_rect.holds_at(GridLevel::TILES, PlanTile::new(8, 15)));
        assert!(!plan_rect.holds_at(GridLevel::TILES, PlanTile::new(7, 15)));
        // level 3: x 1..=2, y 0..=1
        assert!(plan_rect.holds_at(GridLevel::TOP, PlanTile::new(2, 1)));
        assert!(!plan_rect.holds_at(GridLevel::TOP, PlanTile::new(3, 1)));
    }
}
