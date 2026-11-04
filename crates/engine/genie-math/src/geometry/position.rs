//! A place on the map as one type ([`Position`]): x, y and z, apart from the game's vectors
//! (x, height, y, [`crate::fmath::BVector`]).

use crate::fmath::BVector;
use crate::geometry::Point;
use crate::geometry::Tile;

/// A place on the map: x and y in tiles, z its height. An object holds its place so (`+0x3c8`,
/// `+0x4c8`, `+0x5a8`), and so do the places the systems pass on; the game's vector routines
/// take it in their own order ([`Position::vector`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Position(
    /// Its x, y and z.
    [f32; 3],
);

impl Position {
    /// The place at the origin.
    pub const ORIGIN: Self = Self([0.0; 3]);

    /// The place at `x`, `y` and height `z`.
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self([x, y, z])
    }

    /// Its x, y and z, in that order (a save's three floats).
    #[must_use]
    pub const fn get(self) -> [f32; 3] {
        self.0
    }

    /// Its x.
    #[must_use]
    pub const fn x(self) -> f32 {
        self.0[0]
    }

    /// Its y.
    #[must_use]
    pub const fn y(self) -> f32 {
        self.0[1]
    }

    /// Its height.
    #[must_use]
    pub const fn z(self) -> f32 {
        self.0[2]
    }

    /// Its x and y: the place on the ground.
    #[must_use]
    pub const fn ground(self) -> Point {
        Point::new(self.0[0], self.0[1])
    }

    /// The tile its x and y lie in, each cut by `CVTTSS2SI` ([`Point::tile`]).
    #[must_use]
    pub fn tile(self) -> Tile {
        self.ground().tile()
    }

    /// The place with x `x`.
    #[must_use]
    pub const fn with_x(self, x: f32) -> Self {
        Self([x, self.0[1], self.0[2]])
    }

    /// The place with y `y`.
    #[must_use]
    pub const fn with_y(self, y: f32) -> Self {
        Self([self.0[0], y, self.0[2]])
    }

    /// The place with height `z`.
    #[must_use]
    pub const fn with_z(self, z: f32) -> Self {
        Self([self.0[0], self.0[1], z])
    }

    /// The game's vector of this place: x, height, y.
    #[must_use]
    pub const fn vector(self) -> BVector {
        let [x, y, z] = self.0;
        BVector::new(x, z, y)
    }
}

impl From<[f32; 3]> for Position {
    /// The place of three floats in the map's order (x, y, z), as a save holds them.
    fn from(c: [f32; 3]) -> Self {
        Self(c)
    }
}

impl genie_diff::Diff for Position {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

#[cfg(test)]
mod tests {
    use crate::fmath::BVector;
    use crate::geometry::Position;
    #[cfg(test)]
    use crate::geometry::Tile;

    /// A place's tile is its ground point's: x and y cut toward zero, the height left out.
    #[test]
    fn place_tile_is_its_ground_point_s() {
        assert_eq!(Position::new(3.9, -0.5, 7.0).tile(), Tile::new(3, 0));
    }

    /// A place's x, y and z keep their bits, and its vector swaps y and the height.
    #[test]
    fn place_keeps_its_bits_and_its_vector_swaps_the_height_and_y() {
        let position = Position::new(-0.0, 2.0, f32::from_bits(0x7fc0_0001));
        assert_eq!(
            (position.x().to_bits(), position.y(), position.z().to_bits()),
            (0x8000_0000, 2.0, 0x7fc0_0001)
        );
        assert_eq!(
            position.ground().get().map(f32::to_bits),
            [0x8000_0000, 2.0f32.to_bits()]
        );
        let vector = position.vector();
        assert_eq!(
            (vector.x().to_bits(), vector.height().to_bits(), vector.y()),
            (0x8000_0000, 0x7fc0_0001, 2.0)
        );
        assert_eq!(
            vector.position().get().map(f32::to_bits),
            position.get().map(f32::to_bits)
        );
        assert_eq!(
            BVector::new(1.0, 3.0, 2.0).position(),
            Position::new(1.0, 2.0, 3.0)
        );
        assert_eq!(
            Position::from([1.0, 2.0, 3.0]),
            Position::new(1.0, 2.0, 3.0)
        );
        assert_eq!(
            Position::new(1.0, 2.0, 3.0)
                .with_x(4.0)
                .with_y(5.0)
                .with_z(6.0),
            Position::new(4.0, 5.0, 6.0)
        );
    }
}
