//! What a player's queries make of another player's objects as one type, and the tables by it.

use crate::players::ByPlayer;
use std::array::IntoIter as ArrayIter;
use std::cmp::Ordering;
use std::iter::Zip;
use std::ops::{Index, IndexMut};
use std::slice::Iter as SliceIter;

/// What a player's queries make of another player's objects (`player+0x278`, the save's
/// `unit_diplomacy`, which `FUN_141676b80` writes from the relation): Gaia's, its own, an
/// ally's, a neutral's or an enemy's. The visible-units query (`GetVisibleUnits`,
/// `FUN_141288c00`) lists each object by it; a saved value past 4 lists nowhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnitRelation {
    /// Gaia's objects (0).
    Gaia,
    /// Its own objects (1).
    Own,
    /// An ally's objects (2).
    Ally,
    /// A neutral's objects (3).
    Neutral,
    /// An enemy's objects (4).
    Enemy,
    /// A value the game names nowhere, as the save or the game holds it.
    Other(Unnamed),
}

/// A raw value of [`UnitRelation`] that names none of its variants: only the set's own
/// constructors make one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Unnamed(u8);

impl UnitRelation {
    /// The value of raw `value`: its variant, else `Other`.
    const fn of_raw(value: u8) -> Self {
        match value {
            0 => Self::Gaia,
            1 => Self::Own,
            2 => Self::Ally,
            3 => Self::Neutral,
            4 => Self::Enemy,
            other => Self::Other(Unnamed(other)),
        }
    }

    /// Its raw value.
    const fn raw(self) -> u8 {
        match self {
            Self::Gaia => 0,
            Self::Own => 1,
            Self::Ally => 2,
            Self::Neutral => 3,
            Self::Enemy => 4,
            Self::Other(Unnamed(value)) => value,
        }
    }

    /// The relations in their lists' order.
    pub const ALL: [Self; 5] = [
        Self::Gaia,
        Self::Own,
        Self::Ally,
        Self::Neutral,
        Self::Enemy,
    ];

    /// Its value.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.raw()
    }

    /// A saved value: 0 to 4 a relation, any other none.
    #[must_use]
    pub const fn of_field(value: i32) -> Option<Self> {
        if 0 <= value && value <= Self::Enemy.raw() as i32 {
            Some(Self::of_raw(value as u8))
        } else {
            None
        }
    }

    /// Its bit in a query's mask of the relations it keeps (`1 << relation`).
    #[must_use]
    pub const fn bit(self) -> u8 {
        1 << self.raw()
    }

    /// Its list among the five.
    const fn index(self) -> usize {
        self.raw() as usize
    }
}

impl PartialOrd for UnitRelation {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for UnitRelation {
    /// The game's order: by the raw value.
    fn cmp(&self, other: &Self) -> Ordering {
        self.raw().cmp(&other.raw())
    }
}

/// The relations a visible-units query keeps (`FUN_141288c00`'s last argument): one bit each
/// ([`UnitRelation::bit`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RelationMask(u8);

impl RelationMask {
    /// Every bit: what a unit AI's search keeps (`0xff`).
    pub const ALL: Self = Self(0xff);

    /// The mask of `relations`.
    #[must_use]
    pub fn of(relations: &[UnitRelation]) -> Self {
        Self(
            relations
                .iter()
                .fold(0, |bits, relation| bits | relation.bit()),
        )
    }

    /// Whether it keeps `relation`.
    #[must_use]
    pub const fn keeps(self, relation: UnitRelation) -> bool {
        self.0 & relation.bit() != 0
    }

    /// The byte, as the game's key hashes it.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// What a player's queries make of each player's objects (`unit_diplomacy[i]`, `player+0x278`),
/// by the other player: none for a saved value past the five relations.
pub type UnitRelations = ByPlayer<Option<UnitRelation>>;

/// A test's row of saved relations (`unit_diplomacy`): each value as [`UnitRelation::of_field`]
/// reads it.
#[cfg(test)]
pub(crate) fn relations<const N: usize>(values: [i32; N]) -> UnitRelations {
    values.into_iter().map(UnitRelation::of_field).collect()
}

/// One value per [`UnitRelation`], in their order: the visible-units query's lists.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ByRelation<T>([T; 5]);

impl<T> ByRelation<T> {
    /// The table of the five values in the relations' order.
    #[must_use]
    pub const fn new(values: [T; 5]) -> Self {
        Self(values)
    }

    /// The values in the relations' order.
    #[must_use]
    pub const fn values(&self) -> &[T; 5] {
        &self.0
    }

    /// Each relation with its value, in their order.
    pub fn iter(&self) -> Zip<ArrayIter<UnitRelation, 5>, SliceIter<'_, T>> {
        UnitRelation::ALL.into_iter().zip(&self.0)
    }

    /// The table of `transform` of each value.
    pub fn map<U>(&self, transform: impl FnMut(&T) -> U) -> ByRelation<U> {
        ByRelation(self.0.each_ref().map(transform))
    }
}

impl<'table, T> IntoIterator for &'table ByRelation<T> {
    type Item = (UnitRelation, &'table T);
    type IntoIter = Zip<ArrayIter<UnitRelation, 5>, SliceIter<'table, T>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<T> Index<UnitRelation> for ByRelation<T> {
    type Output = T;

    fn index(&self, relation: UnitRelation) -> &T {
        &self.0[relation.index()]
    }
}

impl<T> IndexMut<UnitRelation> for ByRelation<T> {
    fn index_mut(&mut self, relation: UnitRelation) -> &mut T {
        &mut self.0[relation.index()]
    }
}

#[cfg(test)]
mod tests {
    use crate::players::RelationMask;
    use crate::players::{ByRelation, UnitRelation};

    /// A mask keeps the relations it was made of, one bit each.
    #[test]
    fn relation_mask_keeps_the_relations_it_names() {
        let mask = RelationMask::of(&[UnitRelation::Gaia, UnitRelation::Enemy]);
        assert_eq!(mask.get(), 0x11);
        assert!(mask.keeps(UnitRelation::Enemy));
        assert!(!mask.keeps(UnitRelation::Own));
        assert!(
            UnitRelation::ALL
                .iter()
                .all(|&relation| RelationMask::ALL.keeps(relation))
        );
    }

    /// 0 to 4 are the relations, in their lists' order, each with its bit; other values none.
    #[test]
    fn saved_value_is_relation_with_its_list_and_bit() {
        assert_eq!(UnitRelation::of_field(4), Some(UnitRelation::Enemy));
        assert_eq!(UnitRelation::of_field(5), None);
        assert_eq!(UnitRelation::of_field(-1), None);
        assert_eq!(UnitRelation::Ally.get(), 2);
        assert_eq!(UnitRelation::Neutral.bit(), 8);
        let mut lists = ByRelation::new([0, 1, 2, 3, 4]);
        lists[UnitRelation::Own] = 10;
        assert_eq!(lists[UnitRelation::Own], 10);
        assert_eq!(lists.values(), &[0, 10, 2, 3, 4]);
        let order: Vec<u8> = lists
            .iter()
            .rev()
            .map(|(unit_relation, _)| unit_relation.get())
            .collect();
        assert_eq!(order, [4, 3, 2, 1, 0]);
        assert_eq!(lists.map(|value| value * 2).values(), &[0, 20, 4, 6, 8]);
    }

    /// Each named value and one the set names nowhere go to their variant and back.
    #[test]
    fn every_unit_relation_value_maps_to_its_variant_and_back() {
        for value in [0, 1, 2, 3, 4, 5] {
            assert_eq!(UnitRelation::of_raw(value).raw(), value);
        }
        assert!(matches!(UnitRelation::of_raw(5), UnitRelation::Other(_)));
    }
}
