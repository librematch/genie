//! A length in the pathing's fixed point ([`FixedDistance`]) and a squared one
//! ([`FixedDistanceSquared`]).

use crate::pathing::mesh::ONE;
use genie_diff::Diff;

/// A length in fixed point, in units of 2^-22 tile like [`crate::pathing::FixedPoint`]'s
/// coordinates: the short pather's radius, range and insets, its distances and the costs of
/// its paths.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Diff)]
pub struct FixedDistance(
    /// Its value, as the game's 32-bit word holds it.
    i32,
);

impl FixedDistance {
    /// No length.
    pub const ZERO: Self = Self(0);
    /// One tile ([`crate::pathing::mesh::ONE`]).
    pub const TILE: Self = Self(ONE);

    /// The length of `value` units.
    #[must_use]
    pub const fn new(value: i32) -> Self {
        Self(value)
    }

    /// Its value.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }

    /// The sum, wrapping as the game's 32-bit add does.
    #[must_use]
    pub const fn wrapping_add(self, other: Self) -> Self {
        Self(self.0.wrapping_add(other.0))
    }

    /// The difference, wrapping as the game's 32-bit subtract does.
    #[must_use]
    pub const fn wrapping_sub(self, other: Self) -> Self {
        Self(self.0.wrapping_sub(other.0))
    }
}

/// A squared length in fixed point, `d * d >> 22` in 64 bits, as the short pather compares them:
/// its goal's range, a point's distance to the goal's box, the escape circle's radius.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Diff)]
pub struct FixedDistanceSquared(
    /// Its value, as the game's 64-bit register holds it.
    i64,
);

impl FixedDistanceSquared {
    /// No length.
    pub const ZERO: Self = Self(0);

    /// The squared length of `value` units.
    #[must_use]
    pub const fn new(value: i64) -> Self {
        Self(value)
    }

    /// `fixed_distance` squared in 64 bits, `(fixed_distance * fixed_distance) >> 22`.
    #[must_use]
    pub fn of(fixed_distance: FixedDistance) -> Self {
        let v = i64::from(fixed_distance.0);
        Self((v * v) >> 22)
    }

    /// Its value.
    #[must_use]
    pub const fn get(self) -> i64 {
        self.0
    }
}
