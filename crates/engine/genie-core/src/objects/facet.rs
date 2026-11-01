//! A sprite's facets: their count and the facet an object faces.

/// A graphic's facet count (`angle_count`, `+0xa`, a short): how many directions it draws.
/// Every value is kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FacetCount(i16);

impl FacetCount {
    /// None: a sprite the object lacks.
    pub const NONE: Self = Self(0);
    /// The facets most routines count at least (`FUN_14114d780`, `FUN_14119d0c0`).
    const LEAST: i32 = 8;

    /// The count of the data's short `value`.
    #[must_use]
    pub const fn of_data(value: i16) -> Self {
        Self(value)
    }

    /// Whether it has facets: above 0.
    #[must_use]
    pub const fn any(self) -> bool {
        0 < self.0
    }

    /// Whether it has more than one facet (a drawn first facet needs several).
    #[must_use]
    pub const fn several(self) -> bool {
        1 < self.0
    }

    /// The facets most routines count: at least 8.
    #[must_use]
    pub fn at_least_eight(self) -> i32 {
        i32::from(self.0).max(Self::LEAST)
    }

    /// The count as an int, as a draw multiplies it.
    #[must_use]
    pub fn counted(self) -> i32 {
        i32::from(self.0)
    }

    /// The short, as the data holds it.
    #[must_use]
    pub const fn get(self) -> i16 {
        self.0
    }
}

impl genie_diff::Diff for FacetCount {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

/// A facet an object faces (a byte: `+0xe`), counted in its sprite's facets from the diagonal.
/// Every byte is kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Facet(u8);

impl Facet {
    /// The facet of byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// The byte.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl genie_diff::Diff for Facet {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::{Facet, FacetCount};

    /// A count has facets above 0, several above 1, and counts at least 8.
    #[test]
    fn a_facet_count_counts_at_least_eight() {
        assert!(!FacetCount::of_data(0).any() && FacetCount::of_data(1).any());
        assert!(!FacetCount::of_data(1).several() && FacetCount::of_data(2).several());
        assert_eq!(FacetCount::of_data(4).at_least_eight(), 8);
        assert_eq!(FacetCount::of_data(16).at_least_eight(), 16);
        assert_eq!(FacetCount::of_data(-1).at_least_eight(), 8);
        assert_eq!(
            (
                FacetCount::of_data(16).counted(),
                FacetCount::of_data(5).get()
            ),
            (16, 5)
        );
        assert_eq!(Facet::of_data(3).get(), 3);
    }
}
