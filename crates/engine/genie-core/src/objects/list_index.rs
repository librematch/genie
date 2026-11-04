//! An object's place in its player's list ([`ListIndex`]) and a list's places ([`Places`]).

use std::fmt::{Display, Error, Formatter};
use std::iter::FusedIterator;
use std::ops::Range;

/// An object's place in its player's live or sleeping list, counted from 0 (the object's
/// `+0x668` follows it: `FUN_1411664a0` swap-removes, the last object taking the place).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ListIndex(
    /// The place.
    usize,
);

impl ListIndex {
    /// The list's first place.
    pub const FIRST: Self = Self(0);

    /// Place `i`.
    #[must_use]
    pub const fn new(i: usize) -> Self {
        Self(i)
    }

    /// The place as a number.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }

    /// The place after it.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

/// The places of a list of a length counted once, first to last
/// ([`crate::objects::object_list::ObjectList::places`]): the game's loops over a list whose
/// objects may change under them by a count taken at the start.
#[derive(Clone, Debug)]
pub struct Places(Range<usize>);

impl Places {
    /// The places of a list of `len` objects.
    #[must_use]
    pub const fn of(len: usize) -> Self {
        Self(0..len)
    }
}

impl Iterator for Places {
    type Item = ListIndex;

    fn next(&mut self) -> Option<ListIndex> {
        self.0.next().map(ListIndex)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl DoubleEndedIterator for Places {
    fn next_back(&mut self) -> Option<ListIndex> {
        self.0.next_back().map(ListIndex)
    }
}

impl ExactSizeIterator for Places {}

impl FusedIterator for Places {}

impl Display for ListIndex {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        self.0.fmt(f)
    }
}

impl genie_diff::Diff for ListIndex {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_index_counts_from_the_first_place() {
        let i = ListIndex::FIRST;
        assert_eq!((i.get(), i.next().get()), (0, 1));
        assert_eq!(ListIndex::new(7).next(), ListIndex::new(8));
        assert!(ListIndex::new(2) < ListIndex::new(3));
        assert_eq!(ListIndex::new(5).to_string(), "5");
    }

    #[test]
    fn list_of_three_has_three_places_either_way() {
        let places: Vec<_> = Places::of(3).map(ListIndex::get).collect();
        assert_eq!(places, [0, 1, 2]);
        let back: Vec<_> = Places::of(3).rev().map(ListIndex::get).collect();
        assert_eq!(back, [2, 1, 0]);
        assert_eq!(Places::of(4).len(), 4);
        assert_eq!(Places::of(0).next(), None);
    }

    /// The places stay at their end once walked, from either end.
    #[test]
    fn places_stay_at_their_end() {
        let mut places = Places::of(2);
        assert_eq!(places.by_ref().count(), 2);
        assert_eq!((places.next(), places.next_back()), (None, None));
    }
}
