//! A line of sight in whole tiles ([`SightTiles`]), as the visible map's routines and the
//! resource search take it.

use crate::map::Distance;
use crate::math::conversions::cvttss2si;
use derive_more::Display;

/// A line of sight in whole tiles: a seer's sight truncated toward zero (`CVTTSS2SI`: a NaN or
/// one out of range is `i32::MIN`), the radius of the circle the explore tables hold
/// ([`SightTiles::row`]). A sight of 0 or less sees nothing ([`SightTiles::sees`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Display)]
pub struct SightTiles(i32);

impl SightTiles {
    /// No sight: a seer the visible map does not hold.
    pub const NONE: Self = Self(0);

    /// `tiles` whole tiles.
    #[must_use]
    pub const fn new(tiles: i32) -> Self {
        Self(tiles)
    }

    /// The whole tiles of `sight`, truncated toward zero as the game converts it (`CVTTSS2SI`).
    #[must_use]
    pub fn of(sight: Distance) -> Self {
        Self(cvttss2si(sight.get()))
    }

    /// The tiles, as the game's integer arithmetic takes them.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }

    /// Above 0: the sight explores a circle (or a footprint).
    #[must_use]
    pub const fn sees(self) -> bool {
        0 < self.0
    }

    /// The explore table's row of this circle: the tiles, at least 0 and at most 20 (the tables'
    /// last row).
    #[must_use]
    pub fn row(self) -> usize {
        // the clamp leaves 0..=20, which a usize holds
        self.0.clamp(0, 20) as usize
    }
}

impl genie_diff::Diff for SightTiles {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::map::Distance;
    use crate::vision::SightTiles;

    /// A sight converts as `CVTTSS2SI` does (toward zero, a NaN to `i32::MIN`), sees above 0,
    /// and its table row is clamped to 0..=20.
    #[test]
    fn sight_in_tiles_truncates_sees_above_zero_and_clamps_its_row() {
        assert_eq!(SightTiles::of(Distance::new(4.9)), SightTiles::new(4));
        assert_eq!(SightTiles::of(Distance::new(-0.5)), SightTiles::new(0));
        assert_eq!(SightTiles::of(Distance::new(f32::NAN)).get(), i32::MIN);
        assert!(SightTiles::new(1).sees());
        assert!(!SightTiles::NONE.sees() && !SightTiles::new(-3).sees());
        assert_eq!(
            [-3, 0, 7, 20, 25].map(|t| SightTiles::new(t).row()),
            [0, 0, 7, 20, 20]
        );
        assert_eq!(SightTiles::new(7).to_string(), "7");
    }
}
