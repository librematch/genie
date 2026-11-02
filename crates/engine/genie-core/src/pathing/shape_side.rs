//! A side of a shape in the obstruction manager's mesh.

use crate::GenieError;

/// A side of a shape's box in the mesh and the short pather (side s runs from corner s to
/// corner s + 1, [`crate::pathing::mesh::corner`]): its low y, high x, high y or low x edge,
/// counter-clockwise. A contact names the side its outline goes on along; a node of the short
/// pather the side it lies on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ShapeSide {
    /// 0: the low y edge, from corner 0 (x0, y0) to corner 1 (x1, y0).
    LowY = 0,
    /// 1: the high x edge, from corner 1 to corner 2 (x1, y1).
    HighX = 1,
    /// 2: the high y edge, from corner 2 to corner 3 (x0, y1).
    HighY = 2,
    /// 3: the low x edge, from corner 3 back to corner 0.
    LowX = 3,
}

impl ShapeSide {
    /// The four sides in their order.
    pub const ALL: [Self; 4] = [Self::LowY, Self::HighX, Self::HighY, Self::LowX];

    /// The side of a number's two low bits (the game's `& 3`).
    const fn of_low_bits(value: u32) -> Self {
        match value & 3 {
            0 => Self::LowY,
            1 => Self::HighX,
            2 => Self::HighY,
            _ => Self::LowX,
        }
    }

    /// The side a word of the game's mesh names (a contact's `w1`).
    ///
    /// # Errors
    ///
    /// A word past 3, which names no side.
    pub fn of_word(word: u32) -> Result<Self, GenieError> {
        if 3 < word {
            return Err(GenieError::invalid_input("a shape's side past 3").with("side", word));
        }
        Ok(Self::of_low_bits(word))
    }

    /// Its number.
    #[must_use]
    pub const fn get(self) -> u8 {
        self as u8
    }

    /// Its place in a shape's four side lists.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// The side after it, counter-clockwise (`(s + 1) & 3`).
    #[must_use]
    pub const fn next(self) -> Self {
        Self::of_low_bits(self as u32 + 1)
    }

    /// The side before it (`(s - 1) & 3`).
    #[must_use]
    pub const fn prev(self) -> Self {
        Self::of_low_bits(self as u32 + 3)
    }

    /// The side across from it (`(s - 2) & 3`).
    #[must_use]
    pub const fn opposite(self) -> Self {
        Self::of_low_bits(self as u32 + 2)
    }

    /// Whether it runs along y (sides 1 and 3, `s & 1`): its points differ in y.
    #[must_use]
    pub const fn along_y(self) -> bool {
        matches!(self, Self::HighX | Self::LowX)
    }

    /// The corner it starts at ([`crate::pathing::mesh::corner`]'s number).
    #[must_use]
    pub const fn first_corner(self) -> u32 {
        self as u32
    }
}

impl std::fmt::Display for ShapeSide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.get())
    }
}

#[cfg(test)]
mod tests {
    use crate::pathing::ShapeSide;

    /// The sides turn counter-clockwise: each next, previous and opposite side wraps round the
    /// four, and only 1 and 3 run along y.
    #[test]
    fn the_sides_turn_round_the_four() {
        let [low_y, high_x, high_y, low_x] = ShapeSide::ALL;
        assert_eq!(
            ShapeSide::ALL.map(ShapeSide::next),
            [high_x, high_y, low_x, low_y]
        );
        assert_eq!(
            ShapeSide::ALL.map(ShapeSide::prev),
            [low_x, low_y, high_x, high_y]
        );
        assert_eq!(
            ShapeSide::ALL.map(ShapeSide::opposite),
            [high_y, low_x, low_y, high_x]
        );
        assert_eq!(
            ShapeSide::ALL.map(ShapeSide::along_y),
            [false, true, false, true]
        );
        assert_eq!(ShapeSide::ALL.map(ShapeSide::get), [0, 1, 2, 3]);
        assert_eq!(
            (low_x.index(), low_x.first_corner(), low_x.to_string()),
            (3, 3, "3".to_owned())
        );
    }

    /// A word of 0 to 3 names a side; a larger one is an error.
    #[test]
    fn word_past_3_names_no_side() {
        assert_eq!(ShapeSide::of_word(2).unwrap(), ShapeSide::HighY);
        assert!(ShapeSide::of_word(4).is_err());
    }
}
