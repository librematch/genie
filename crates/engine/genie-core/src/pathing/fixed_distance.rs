//! A length in the pathing's fixed point ([`FixedDistance`]).

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
    pub const TILE: Self = Self(crate::pathing::mesh::ONE);

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
