//! The direction of an axis walk of the short pather.

use crate::pathing::ShapeSide;

/// The direction of an axis ray or sweep (`FUN_140fa2d00`, `FUN_140fa8930`): toward side d of a
/// box, so 0 −y, 1 +x, 2 +y, 3 −x.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AxisDir {
    /// 0: toward the low y.
    MinusY = 0,
    /// 1: toward the high x.
    PlusX = 1,
    /// 2: toward the high y.
    PlusY = 2,
    /// 3: toward the low x.
    MinusX = 3,
}

impl AxisDir {
    /// The direction along x of a step `dx`: +x above 0, else −x.
    pub(super) const fn of_dx(dx: i32) -> Self {
        if 0 < dx { Self::PlusX } else { Self::MinusX }
    }

    /// The direction along y of a step `dy`: +y above 0, else −y.
    pub(super) const fn of_dy(dy: i32) -> Self {
        if 0 < dy { Self::PlusY } else { Self::MinusY }
    }

    /// The direction back (`(d - 2) & 3`).
    pub(super) const fn reverse(self) -> Self {
        match self {
            Self::MinusY => Self::PlusY,
            Self::PlusX => Self::MinusX,
            Self::PlusY => Self::MinusY,
            Self::MinusX => Self::PlusX,
        }
    }

    /// Whether it walks along x (1 and 3).
    pub(super) const fn along_x(self) -> bool {
        matches!(self, Self::PlusX | Self::MinusX)
    }

    /// The side of a box a walk in it meets: the one across from the side it heads for.
    pub(super) const fn facing(self) -> ShapeSide {
        match self {
            Self::MinusY => ShapeSide::HighY,
            Self::PlusX => ShapeSide::LowX,
            Self::PlusY => ShapeSide::LowY,
            Self::MinusX => ShapeSide::HighX,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::pathing::ShapeSide;
    use crate::pathing::short_path::axis_dir::AxisDir;

    /// A step's sign picks the direction (0 the negative one); the reverse and the side met are
    /// across from it.
    #[test]
    fn walk_meets_the_side_across_from_its_heading() {
        let all = [
            AxisDir::MinusY,
            AxisDir::PlusX,
            AxisDir::PlusY,
            AxisDir::MinusX,
        ];
        assert_eq!(
            [1, 0, -1].map(AxisDir::of_dx),
            [AxisDir::PlusX, AxisDir::MinusX, AxisDir::MinusX]
        );
        assert_eq!(
            [1, 0].map(AxisDir::of_dy),
            [AxisDir::PlusY, AxisDir::MinusY]
        );
        assert_eq!(all.map(AxisDir::reverse), [all[2], all[3], all[0], all[1]]);
        assert_eq!(all.map(AxisDir::along_x), [false, true, false, true]);
        assert_eq!(
            all.map(AxisDir::facing),
            ShapeSide::ALL.map(ShapeSide::opposite)
        );
    }
}
