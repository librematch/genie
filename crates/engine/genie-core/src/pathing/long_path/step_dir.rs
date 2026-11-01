//! The long pather's step directions ([`StepDir`], the eight tiles around one in the order the
//! search tries them) and a set of them ([`StepDirs`]).

use crate::map::{Tile, TileOffset};

/// The tiles one step away in the order the search tries them (the switch of `FUN_140f9eb70`).
pub(super) const STEPS: [TileOffset; 8] = TileOffset::table([
    [-1, -1],
    [0, -1],
    [1, -1],
    [1, 0],
    [1, 1],
    [0, 1],
    [-1, 1],
    [-1, 0],
]);

/// A step's direction (`FUN_140f9eb70`'s switch): 0 to 7, the place of its tile in [`STEPS`],
/// the order the search tries the eight tiles around one; the even ones are diagonal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct StepDir(u8);

impl StepDir {
    /// The eight in the order the search tries them.
    pub(super) const ALL: [Self; 8] = [
        Self(0),
        Self(1),
        Self(2),
        Self(3),
        Self(4),
        Self(5),
        Self(6),
        Self(7),
    ];

    /// The step's tile offset ([`STEPS`]).
    pub(super) fn offset(self) -> Option<TileOffset> {
        STEPS.get(usize::from(self.0)).copied()
    }

    /// The directions a tile entered by a step in direction `came` does not try (the table at
    /// `DAT_142f43a28`: the three that turn back, bits `came + 3` to `came + 5`; none for the
    /// start, the game's `came` 8).
    pub(super) fn behind(came: Option<Self>) -> StepDirs {
        StepDirs(came.map_or(0, |c| (0x38u8).rotate_left(u32::from(c.0))))
    }

    /// The tiles beside a diagonal step (directions 0, 2, 4, 6) onto `n`, which the step needs
    /// as clear as the tile.
    pub(super) const fn beside_diagonal(self, n: Tile) -> Option<[Tile; 2]> {
        let (nx, ny) = (n.x(), n.y());
        match self.0 {
            0 => Some([Tile::new(nx + 1, ny), Tile::new(nx, ny + 1)]),
            2 => Some([Tile::new(nx, ny + 1), Tile::new(nx - 1, ny)]),
            4 => Some([Tile::new(nx - 1, ny), Tile::new(nx, ny - 1)]),
            6 => Some([Tile::new(nx, ny - 1), Tile::new(nx + 1, ny)]),
            _ => None,
        }
    }
}

/// A set of step directions, a bit each ([`StepDir::behind`], the table at `DAT_142f43a28`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct StepDirs(u8);

impl StepDirs {
    /// Whether it holds direction `d`.
    pub(super) const fn holds(self, d: StepDir) -> bool {
        self.0 & (1 << d.0) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three directions that turn back from each (`DAT_142f43a28`), none from the start.
    #[test]
    fn the_table_of_turning_back() {
        let table: Vec<u8> = StepDir::ALL
            .into_iter()
            .map(Some)
            .chain([None])
            .map(|c| StepDir::behind(c).0)
            .collect();
        assert_eq!(
            table,
            [0x38, 0x70, 0xe0, 0xc1, 0x83, 0x07, 0x0e, 0x1c, 0x00]
        );
        assert!(StepDirs(0x38).holds(StepDir(3)) && !StepDirs(0x38).holds(StepDir(2)));
    }

    /// The even directions are diagonal and need the two tiles beside them; each direction's
    /// tile is its place in the table.
    #[test]
    fn a_diagonal_step_needs_the_tiles_beside_it() {
        let n = Tile::new(5, 5);
        assert_eq!(
            StepDir(0).beside_diagonal(n),
            Some([Tile::new(6, 5), Tile::new(5, 6)])
        );
        assert_eq!(StepDir(1).beside_diagonal(n), None);
        assert_eq!(StepDir::ALL.map(StepDir::offset), STEPS.map(Some));
    }
}
