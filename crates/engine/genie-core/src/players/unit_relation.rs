//! What a player's queries make of another player's objects as one type, its saved value, and
//! the tables by it.

use crate::players::ByPlayer;
use std::array::IntoIter as ArrayIter;
use std::iter::Zip;
use std::ops::{Index, IndexMut};
use std::slice::Iter as SliceIter;
use strum::FromRepr;

/// What a player's queries make of another player's objects: Gaia's, its own, an ally's, a
/// neutral's or an enemy's. `FUN_141676b80` writes it to `player+0x278` as a [`UnitDiplomacy`].
/// The visible-units query (`GetVisibleUnits`, `FUN_141288c00`) lists each object by it, in
/// this order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, FromRepr)]
#[repr(u8)]
pub enum UnitRelation {
    /// Gaia's objects (0).
    Gaia = 0,
    /// Its own objects (1).
    Own = 1,
    /// An ally's objects (2).
    Ally = 2,
    /// A neutral's objects (3).
    Neutral = 3,
    /// An enemy's objects (4).
    Enemy = 4,
}

impl UnitRelation {
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
        self as u8
    }

    /// Its bit in a query's mask of the relations it keeps (`1 << relation`).
    #[must_use]
    pub const fn bit(self) -> u8 {
        1 << self.get()
    }

    /// Its list among the five.
    const fn index(self) -> usize {
        self.get() as usize
    }
}

/// The bits of a shift's count that x86's `SHL` reads (`CL`'s low five).
const SHIFT_COUNT: u32 = 0x1f;

/// What a player's queries make of another player's objects as `player+0x278` holds it (the
/// save's `unit_diplomacy`): a dword, which `FUN_141676b80` writes from a [`UnitRelation`] and a
/// save can hold as any value. The visible-units query (`GetVisibleUnits`, `FUN_141288c00`)
/// keeps a record by its bit ([`UnitDiplomacy::bit`]) and lists it by its low byte
/// ([`UnitDiplomacy::list`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnitDiplomacy(i32);

impl UnitDiplomacy {
    /// Its bit in a query's mask (`MOV EAX, 1`, `SHL AL, CL`): 1 shifted within a byte by the
    /// value's low five bits, so none for a shift past 7.
    #[must_use]
    pub const fn bit(self) -> u8 {
        let shift = self.0 as u32 & SHIFT_COUNT;
        if shift < u8::BITS { 1 << shift } else { 0 }
    }

    /// The list a query files a record of it in (`MOVZX ESI, DL`): the relation of its low byte,
    /// none past the five lists, where the game writes past them.
    #[must_use]
    pub const fn list(self) -> Option<UnitRelation> {
        UnitRelation::from_repr(self.0 as u8)
    }
}

impl From<i32> for UnitDiplomacy {
    /// The value a save holds.
    fn from(value: i32) -> Self {
        Self(value)
    }
}

impl From<UnitRelation> for UnitDiplomacy {
    /// The value `FUN_141676b80` writes for `relation`.
    fn from(relation: UnitRelation) -> Self {
        Self(i32::from(relation.get()))
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

    /// Whether it keeps the objects of an owner its player holds as `diplomacy` (`TEST` of the
    /// mask's byte with [`UnitDiplomacy::bit`]).
    #[must_use]
    pub const fn keeps(self, diplomacy: UnitDiplomacy) -> bool {
        self.0 & diplomacy.bit() != 0
    }

    /// The byte, as the game's key hashes it.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// What a player's queries make of each player's objects (`unit_diplomacy[i]`, `player+0x278`),
/// by the other player, as the save holds it.
pub type UnitRelations = ByPlayer<UnitDiplomacy>;

/// A test's row of saved relations (`unit_diplomacy`): each value as the save holds it.
#[cfg(test)]
pub(crate) fn relations<const N: usize>(values: [i32; N]) -> UnitRelations {
    values.into_iter().map(UnitDiplomacy::from).collect()
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
    use crate::players::{ByRelation, UnitDiplomacy, UnitRelation};

    /// A mask keeps the relations it was made of, one bit each.
    #[test]
    fn relation_mask_keeps_the_relations_it_names() {
        let mask = RelationMask::of(&[UnitRelation::Gaia, UnitRelation::Enemy]);
        assert_eq!(mask.get(), 0x11);
        assert!(mask.keeps(UnitRelation::Enemy.into()));
        assert!(!mask.keeps(UnitRelation::Own.into()));
        assert!(
            UnitRelation::ALL
                .iter()
                .all(|&relation| RelationMask::ALL.keeps(relation.into()))
        );
    }

    /// 0 to 4 are the relations, in their lists' order, each with its bit.
    #[test]
    fn relation_has_its_list_and_bit() {
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

    /// Each relation's list is its value, and the value its writer gives lists it there.
    #[test]
    fn every_relation_value_maps_to_its_list_and_back() {
        for relation in UnitRelation::ALL {
            assert_eq!(UnitRelation::from_repr(relation.get()), Some(relation));
            assert_eq!(UnitDiplomacy::from(relation).list(), Some(relation));
            assert_eq!(UnitDiplomacy::from(relation).bit(), relation.bit());
        }
        assert_eq!(UnitRelation::from_repr(5), None);
    }

    /// A saved value's bit is 1 shifted within a byte by its low five bits (`SHL AL, CL`): 5
    /// has bit 5, 36 an enemy's bit, 8 and -1 (a shift of 31) none.
    #[test]
    fn saved_value_bit_shifts_within_byte_by_its_low_five_bits() {
        let bit = |value: i32| UnitDiplomacy::from(value).bit();
        assert_eq!(bit(4), 0x10);
        assert_eq!(bit(5), 0x20);
        assert_eq!(bit(7), 0x80);
        assert_eq!(bit(36), UnitRelation::Enemy.bit());
        assert_eq!(bit(8), 0);
        assert_eq!(bit(-1), 0);
    }

    /// A saved value lists by its low byte (`MOVZX`): 260 an enemy's, 256 Gaia's; 5, 36 and -1
    /// nowhere among the five.
    #[test]
    fn saved_value_lists_by_its_low_byte() {
        let list = |value: i32| UnitDiplomacy::from(value).list();
        assert_eq!(list(260), Some(UnitRelation::Enemy));
        assert_eq!(list(256), Some(UnitRelation::Gaia));
        assert_eq!(list(5), None);
        assert_eq!(list(36), None);
        assert_eq!(list(-1), None);
    }
}
