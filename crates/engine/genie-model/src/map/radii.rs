//! An object's size as two types: its three radii ([`Radii`], x, y and z, as its master holds
//! them) and their part on the ground ([`Footprint`], x and y), apart from a place
//! ([`crate::map::Position`]) and a displacement ([`crate::map::Offset`]).

use serde::Deserialize;
use serde::Serialize;

/// An object's three radii in the map's order: x and y in tiles, z the height (a master's
/// `radius_x`, `radius_y`, `radius_z`, `+0x5d8`..`+0x5e0`). Each is half the object's size along
/// its axis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Radii(
    /// Its x, y and z.
    [f32; 3],
);

impl Radii {
    /// No size.
    pub const ZERO: Self = Self([0.0; 3]);

    /// The radii `x`, `y` and height `z`.
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

    /// Its part on the ground: x and y.
    #[must_use]
    pub const fn ground(self) -> Footprint {
        Footprint::new(self.0[0], self.0[1])
    }

    /// Each of the three is above 0 (a NaN is not): the object has a size along every axis.
    #[must_use]
    pub fn sized(self) -> bool {
        self.0.iter().all(|&r| 0.0 < r)
    }

    /// The radii with height `z`.
    #[must_use]
    pub const fn with_z(self, z: f32) -> Self {
        Self([self.0[0], self.0[1], z])
    }
}

impl From<[f32; 3]> for Radii {
    /// The radii of three floats in the map's order (x, y, z), as a save holds them.
    fn from(c: [f32; 3]) -> Self {
        Self(c)
    }
}

impl genie_diff::Diff for Radii {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

/// An object's radii on the ground: x and y in tiles, half its size along each axis (a master's
/// `radius_x` and `radius_y`, `+0x5d8`, `+0x5dc`). The area it covers is its place plus and minus
/// them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Footprint(
    /// Its x and y.
    [f32; 2],
);

impl Footprint {
    /// No size.
    pub const ZERO: Self = Self([0.0; 2]);

    /// The radii `x` and `y`.
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self([x, y])
    }

    /// Its x and y, in that order (a save's two floats).
    #[must_use]
    pub const fn get(self) -> [f32; 2] {
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

    /// Both are above 0 (a NaN is not): the object covers an area on the ground.
    #[must_use]
    pub fn sized(self) -> bool {
        self.0.iter().all(|&r| 0.0 < r)
    }

    /// The footprint with x `x`.
    #[must_use]
    pub const fn with_x(self, x: f32) -> Self {
        Self([x, self.0[1]])
    }

    /// The footprint with y `y`.
    #[must_use]
    pub const fn with_y(self, y: f32) -> Self {
        Self([self.0[0], y])
    }

    /// The three radii of this footprint and height `z`.
    #[must_use]
    pub const fn with_height(self, z: f32) -> Radii {
        Radii::new(self.0[0], self.0[1], z)
    }
}

impl From<[f32; 2]> for Footprint {
    /// The footprint of two floats in the map's order (x, y), as a save holds them.
    fn from(c: [f32; 2]) -> Self {
        Self(c)
    }
}

impl genie_diff::Diff for Footprint {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::map::{Footprint, Radii};

    /// Radii keep their components' bits in the map's order, and their ground part is x and y.
    #[test]
    fn radii_keep_their_bits_and_their_ground_part_is_both_axes() {
        let r = Radii::new(-0.0, 2.0, f32::from_bits(0x7fc0_0001));
        assert_eq!(
            r.get().map(f32::to_bits),
            [0x8000_0000, 2.0f32.to_bits(), 0x7fc0_0001]
        );
        assert_eq!(
            (r.x().to_bits(), r.y(), r.z().to_bits()),
            (0x8000_0000, 2.0, 0x7fc0_0001)
        );
        assert_eq!(
            r.ground().get().map(f32::to_bits),
            [0x8000_0000, 2.0f32.to_bits()]
        );
        assert_eq!(Radii::from([1.0, 2.0, 3.0]), Radii::new(1.0, 2.0, 3.0));
        assert_eq!(Radii::ZERO.with_z(3.0), Radii::new(0.0, 0.0, 3.0));
    }

    /// Radii are sized when each is above 0: a zero, a negative or a NaN one is not.
    #[test]
    fn radii_are_sized_when_each_is_above_zero() {
        assert!(Radii::new(0.5, 0.5, 2.0).sized());
        assert!(!Radii::new(0.5, 0.5, 0.0).sized());
        assert!(!Radii::new(-0.5, 0.5, 2.0).sized());
        assert!(!Radii::new(0.5, f32::NAN, 2.0).sized());
        assert!(Footprint::new(0.5, 0.5).sized());
        assert!(!Footprint::new(0.5, -0.0).sized());
        assert!(!Footprint::new(f32::NAN, 0.5).sized());
    }

    /// A footprint keeps its components' bits and takes a height to three radii.
    #[test]
    fn footprint_keeps_its_bits_and_takes_height() {
        let f = Footprint::new(-0.0, 0.5);
        assert_eq!(f.get().map(f32::to_bits), [0x8000_0000, 0.5f32.to_bits()]);
        assert_eq!((f.x().to_bits(), f.y()), (0x8000_0000, 0.5));
        assert_eq!(Footprint::from([1.0, 2.0]), Footprint::new(1.0, 2.0));
        assert_eq!(
            Footprint::ZERO.with_x(1.0).with_y(2.0),
            Footprint::new(1.0, 2.0)
        );
        assert_eq!(f.with_height(2.0), Radii::new(-0.0, 0.5, 2.0));
        assert_eq!(
            Radii::new(1.0, 2.0, 3.0).ground().with_height(3.0),
            Radii::new(1.0, 2.0, 3.0)
        );
    }
}
