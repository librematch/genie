//! The game's 3-vectors (`BVector`): sums, products, length, angle, line distance and
//! rotation.

use crate::fmath::{acos, cos, sin, sqrt};
use crate::geometry::Point;
use crate::geometry::Position;
use std::f32::consts::{FRAC_PI_2, PI};
use std::ops::Neg;

/// A vector in the game's order (`BVector`): x, height, y. The map's y is the third component,
/// so a place on the map (x, y, z) is the vector (x, z, y) ([`Position::vector`],
/// [`BVector::position`]).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct BVector(
    /// Its components in the game's order: x, height, y.
    [f32; 3],
);

impl BVector {
    /// The zero vector.
    pub const ZERO: Self = Self([0.0; 3]);

    /// The vector of components `x`, `height` and `y`, in the game's order.
    #[must_use]
    pub const fn new(x: f32, height: f32, y: f32) -> Self {
        Self([x, height, y])
    }

    /// Its components in the game's order: x, height, y.
    #[must_use]
    pub const fn get(self) -> [f32; 3] {
        self.0
    }

    /// The place on the map of this vector (x, height, y): x, y, z.
    #[must_use]
    pub const fn position(self) -> Position {
        let [x, height, y] = self.0;
        Position::new(x, y, height)
    }

    /// Its x, the map's x.
    #[must_use]
    pub const fn x(self) -> f32 {
        self.0[0]
    }

    /// Its height, the map's z.
    #[must_use]
    pub const fn height(self) -> f32 {
        self.0[1]
    }

    /// Its y, the map's y.
    #[must_use]
    pub const fn y(self) -> f32 {
        self.0[2]
    }

    /// Its place on the ground: x and y.
    #[must_use]
    pub const fn ground(self) -> Point {
        Point::new(self.0[0], self.0[2])
    }

    /// The vector with x `x`.
    #[must_use]
    pub const fn with_x(self, x: f32) -> Self {
        Self([x, self.0[1], self.0[2]])
    }

    /// The vector with height `height`.
    #[must_use]
    pub const fn with_height(self, height: f32) -> Self {
        Self([self.0[0], height, self.0[2]])
    }

    /// The vector with y `y`.
    #[must_use]
    pub const fn with_y(self, y: f32) -> Self {
        Self([self.0[0], self.0[1], y])
    }
}

impl From<[f32; 3]> for BVector {
    /// The vector of three floats in the game's order (x, height, y), as a save holds them.
    fn from(c: [f32; 3]) -> Self {
        Self(c)
    }
}

impl Neg for BVector {
    type Output = Self;

    /// Each component negated (its sign bit flipped, a NaN's too).
    fn neg(self) -> Self {
        let [x, height, y] = self.0;
        Self([-x, -height, -y])
    }
}

impl genie_diff::Diff for BVector {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

/// `a - b` (`FUN_14087e560`).
#[must_use]
pub fn sub(a: BVector, b: BVector) -> BVector {
    let ([a0, a1, a2], [b0, b1, b2]) = (a.0, b.0);
    BVector([a0 - b0, a1 - b1, a2 - b2])
}

/// `a + b` (`FUN_14087e5e0`).
#[must_use]
pub fn add(a: BVector, b: BVector) -> BVector {
    let ([a0, a1, a2], [b0, b1, b2]) = (a.0, b.0);
    BVector([b0 + a0, b1 + a1, b2 + a2])
}

/// `s * v` (`FUN_14087e4a0`).
#[must_use]
pub fn scale(s: f32, v: BVector) -> BVector {
    let [v0, v1, v2] = v.0;
    BVector([s * v0, s * v1, s * v2])
}

/// `a · b`, summed left to right.
#[must_use]
pub fn dot(a: BVector, b: BVector) -> f32 {
    let ([a0, a1, a2], [b0, b1, b2]) = (a.0, b.0);
    a0 * b0 + a1 * b1 + a2 * b2
}

/// `a × b` (`FUN_14087e4d0`), each component in the game's operand order.
#[must_use]
pub fn cross(a: BVector, b: BVector) -> BVector {
    let ([a0, a1, a2], [b0, b1, b2]) = (a.0, b.0);
    BVector([a1 * b2 - a2 * b1, b0 * a2 - a0 * b2, a0 * b1 - b0 * a1])
}

/// `|v|`, the squares summed x, height, y.
#[must_use]
pub fn length(v: BVector) -> f32 {
    let [v0, v1, v2] = v.0;
    sqrt(v0 * v0 + v1 * v1 + v2 * v2)
}

/// `v / |v|` (`FUN_141027d00`).
#[must_use]
pub fn normalize(v: BVector) -> BVector {
    let l = length(v);
    let [v0, v1, v2] = v.0;
    BVector([v0 / l, v1 / l, v2 / l])
}

/// The angle between two unit vectors (`FUN_14087e650`): π/2 when their dot product is below
/// 1e-7 in size, 0 from 1 up, π from -1 down, else its arc cosine (a NaN dot included).
#[must_use]
pub fn angle_between(a: BVector, b: BVector) -> f32 {
    let d = dot(a, b);
    if d.abs() < 1e-7 {
        FRAC_PI_2
    } else if d >= 1.0 {
        0.0
    } else if d <= -1.0 {
        PI
    } else {
        // COMISS -1, d; JC at 0x14087e6ed: an unordered dot goes on to acosf
        acos(d)
    }
}

/// The distance of `p` from the line through `a` along `dir` (`FUN_14087e710`): the cross
/// product of `p - a` and `dir`, its length over `dir`'s, each sum in the game's order.
#[must_use]
pub fn line_distance(p: BVector, a: BVector, dir: BVector) -> f32 {
    let ([p0, p1, p2], [a0, a1, a2], [d0, d1, d2]) = (p.0, a.0, dir.0);
    let x = (p1 - a1) * d2 - (p2 - a2) * d1;
    let z = (p0 - a0) * d1 - (p1 - a1) * d0;
    let y = (p2 - a2) * d0 - (p0 - a0) * d2;
    sqrt((y * y + x * x + z * z) / (d1 * d1 + d0 * d0 + d2 * d2))
}

/// A rotation: the 3×3 part of the game's 3×4 matrix, rows as it stores them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rotation([[f32; 3]; 3]);

/// The rotation by `angle` about the unit `axis` (`FUN_14087e350`).
#[must_use]
pub fn rotation(angle: f32, axis: BVector) -> Rotation {
    let (c, s) = (cos(angle), sin(angle));
    let t = 1.0 - c;
    let [x, y, z] = axis.0;
    let xy = y * x * t;
    let xz = z * x * t;
    let yz = z * y * t;
    Rotation([
        [x * x * t + c, xy - z * s, y * s + xz],
        [z * s + xy, y * y * t + c, yz - x * s],
        [xz - y * s, x * s + yz, z * z * t + c],
    ])
}

/// `m · v` (`FUN_14087e2c0`), each row's products summed in the game's order.
#[must_use]
pub fn rotate(m: &Rotation, v: BVector) -> BVector {
    let [r0, r1, r2] = m.0;
    let [v0, v1, v2] = v.0;
    BVector([
        v1 * r0[1] + v0 * r0[0] + v2 * r0[2],
        v0 * r1[0] + v1 * r1[1] + v2 * r1[2],
        v0 * r2[0] + v1 * r2[1] + v2 * r2[2],
    ])
}

#[cfg(test)]
mod tests {
    use crate::fmath::BVector;
    use crate::geometry::Position;

    /// A vector keeps its components' bits in the game's order (x, height, y): its accessors,
    /// its place on the map (x, y, z) and back, its ground, the `with_` copies and the save's
    /// three floats.
    #[test]
    fn vector_keeps_the_game_order_and_its_bits() {
        let v = BVector::new(-0.0, 2.0, f32::from_bits(0x7fc0_0001));
        assert_eq!(
            (v.x().to_bits(), v.height(), v.y().to_bits()),
            (0x8000_0000, 2.0, 0x7fc0_0001)
        );
        assert_eq!(
            v.get().map(f32::to_bits),
            [0x8000_0000, 2.0f32.to_bits(), 0x7fc0_0001]
        );
        assert_eq!(
            v.ground().get().map(f32::to_bits),
            [0x8000_0000, 0x7fc0_0001]
        );
        let p = v.position();
        assert_eq!(
            (p.x().to_bits(), p.y().to_bits(), p.z()),
            (0x8000_0000, 0x7fc0_0001, 2.0)
        );
        assert_eq!(
            p.vector().get().map(f32::to_bits),
            v.get().map(f32::to_bits)
        );
        assert_eq!(BVector::from([1.0, 2.0, 3.0]), BVector::new(1.0, 2.0, 3.0));
        assert_eq!(
            BVector::ZERO.with_x(1.0).with_height(2.0).with_y(3.0),
            BVector::new(1.0, 2.0, 3.0)
        );
        assert_eq!(
            Position::new(1.0, 2.0, 3.0).vector(),
            BVector::new(1.0, 3.0, 2.0)
        );
    }

    /// Negation flips every component's sign bit, a zero's and a NaN's too (`-c`, not
    /// `-1 * c`, which would keep a NaN's sign).
    #[test]
    fn negated_vector_flips_each_sign_bit() {
        let v = -BVector::new(0.0, -2.0, f32::from_bits(0x7fc0_0001));
        assert_eq!(
            v.get().map(f32::to_bits),
            [0x8000_0000, 2.0f32.to_bits(), 0xffc0_0001]
        );
    }
}
