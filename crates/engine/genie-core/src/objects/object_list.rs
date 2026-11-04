//! One of a player's object lists (FD-045-D11, D20): its objects in update order. The world's
//! table by id (`FUN_1415417a0`, [`crate::world::index`]) holds each object's slot; the world's
//! list operations change a list and file what moved, and a list's own lookup by id scans it.
//!
//! An object out for its update ([`ObjectList::take_out`]) leaves its stand-in ([`TAKEN_OUT`])
//! in its slot, and the list keeps that slot as its removals move the stand-in, so the object
//! goes back where the game's list holds it ([`ObjectList::put_back`]; `FUN_1411664a0`
//! swap-removes, the object's `+0x668` following its slot). A stand-in is no object: a lookup
//! by id never answers it.

use crate::objects::unit::{TAKEN_OUT, Unit};
use crate::objects::{ListIndex, ObjectId, ObjectKey, Places};
use crate::{GenieError, GenieResult};
use std::mem::replace;
use std::ops::{Index, IndexMut};
use std::slice::{Iter, IterMut};
use std::vec::IntoIter as VecIntoIter;

/// A player's live or sleeping objects in their list's order.
#[derive(Clone, Debug, Default)]
pub struct ObjectList {
    units: Vec<Unit>,
    /// The objects out of the list ([`ObjectList::take_out`]): each one's id and the slot its
    /// stand-in holds now.
    outs: Vec<(ObjectId, ListIndex)>,
}

/// A slot the list does not hold.
fn no_slot(list_index: ListIndex) -> GenieError {
    GenieError::unreachable("an object slot the list does not hold").with("index", list_index)
}

impl ObjectList {
    /// The slot of object `id`, if the list holds it (not while it is out for its update), the
    /// first one first: a scan of the list (the world's table by id answers a lookup in the
    /// world, [`crate::world::World::place`]).
    #[must_use]
    pub fn position_of(&self, id: impl ObjectKey) -> Option<ListIndex> {
        let id = id.key()?;
        if id == TAKEN_OUT {
            return None;
        }
        self.units
            .iter()
            .position(|unit| unit.id == id)
            .map(ListIndex::new)
    }

    /// The object whose stand-in holds slot `list_index` now, if one does.
    #[must_use]
    pub fn out_at(&self, list_index: ListIndex) -> Option<ObjectId> {
        self.outs.iter().find(|o| o.1 == list_index).map(|o| o.0)
    }

    /// The object in slot `list_index`, if the list holds the slot.
    #[must_use]
    pub fn get(&self, list_index: ListIndex) -> Option<&Unit> {
        self.units.get(list_index.get())
    }

    /// The object in slot `list_index` to change, if the list holds the slot.
    pub fn get_mut(&mut self, list_index: ListIndex) -> Option<&mut Unit> {
        self.units.get_mut(list_index.get())
    }

    /// The object in slot `list_index`, as an error where the slot is missing: the slot comes from
    /// the code's own loop or search, so a miss is an invariant broken.
    ///
    /// # Errors
    ///
    /// The list holds no slot `list_index`.
    pub fn checked(&self, list_index: ListIndex) -> GenieResult<&Unit> {
        self.get(list_index).ok_or_else(|| no_slot(list_index))
    }

    /// The object in slot `list_index` to change, as [`ObjectList::checked`].
    ///
    /// # Errors
    ///
    /// The list holds no slot `list_index`.
    pub fn checked_mut(&mut self, list_index: ListIndex) -> GenieResult<&mut Unit> {
        self.get_mut(list_index).ok_or_else(|| no_slot(list_index))
    }

    /// The list's slots, first to last, as many as it holds now: a loop over them may change
    /// the list (the iterator holds no borrow of it).
    #[must_use]
    pub const fn places(&self) -> Places {
        Places::of(self.units.len())
    }

    /// The slot an object pushed now takes, one past the last.
    #[must_use]
    pub const fn next_place(&self) -> ListIndex {
        ListIndex::new(self.units.len())
    }

    /// The last slot, if the list holds any.
    #[must_use]
    pub const fn last_place(&self) -> Option<ListIndex> {
        match self.units.len().checked_sub(1) {
            Some(i) => Some(ListIndex::new(i)),
            None => None,
        }
    }

    /// Whether the list holds slot `list_index` (its length is past it).
    #[must_use]
    pub const fn holds(&self, list_index: ListIndex) -> bool {
        list_index.get() < self.units.len()
    }

    /// How many slots the list holds.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.units.len()
    }

    /// Whether the list holds no slot.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.units.is_empty()
    }

    /// The objects in the list's order.
    pub fn iter(&self) -> Iter<'_, Unit> {
        self.units.iter()
    }

    /// The objects in the list's order, to change.
    pub fn iter_mut(&mut self) -> IterMut<'_, Unit> {
        self.units.iter_mut()
    }

    /// Each object with its slot, in the list's order.
    pub fn indexed(&self) -> impl Iterator<Item = (ListIndex, &Unit)> {
        self.units
            .iter()
            .enumerate()
            .map(|(i, unit)| (ListIndex::new(i), unit))
    }

    /// The first object, if any.
    #[must_use]
    pub fn first(&self) -> Option<&Unit> {
        self.units.first()
    }

    /// The last object, if any.
    #[must_use]
    pub fn last(&self) -> Option<&Unit> {
        self.units.last()
    }

    /// The last object to change, if any.
    pub fn last_mut(&mut self) -> Option<&mut Unit> {
        self.units.last_mut()
    }

    /// The objects as a slice, in the list's order.
    #[must_use]
    pub fn as_slice(&self) -> &[Unit] {
        &self.units
    }

    /// Object `id`, if the list holds it.
    #[must_use]
    pub fn by_id(&self, id: impl ObjectKey) -> Option<&Unit> {
        self.position_of(id)
            .and_then(|list_index| self.get(list_index))
    }

    /// Object `id` to change, if the list holds it.
    pub fn by_id_mut(&mut self, id: impl ObjectKey) -> Option<&mut Unit> {
        self.position_of(id)
            .and_then(|list_index| self.units.get_mut(list_index.get()))
    }

    /// The object in slot `list_index` leaves its slot for a while (its update, a call on it); its
    /// stand-in holds the slot, which the list's removals keep track of.
    ///
    /// # Errors
    ///
    /// The list holds no slot `list_index`.
    pub fn take_out(&mut self, list_index: ListIndex) -> GenieResult<Unit> {
        let slot = self
            .units
            .get_mut(list_index.get())
            .ok_or_else(|| no_slot(list_index))?;
        let unit = replace(slot, Unit::taken_out());
        self.outs.push((unit.id, list_index));
        Ok(unit)
    }

    /// `unit`, which [`ObjectList::take_out`] took, goes back to the slot its stand-in holds now:
    /// that slot.
    ///
    /// # Errors
    ///
    /// The unit, when its stand-in left the list (or it was never taken out).
    pub fn put_back(&mut self, unit: Unit) -> Result<ListIndex, Box<Unit>> {
        let Some(n) = self.outs.iter().rposition(|&(id, _)| id == unit.id) else {
            return Err(Box::new(unit));
        };
        let (_, slot) = self.outs.swap_remove(n);
        let Some(held) = self.units.get_mut(slot.get()) else {
            return Err(Box::new(unit));
        };
        *held = unit;
        Ok(slot)
    }

    /// `unit` goes to the end of the list.
    pub fn push(&mut self, unit: Unit) {
        self.units.push(unit);
    }

    /// The objects of `units` go to the end of the list, in their order.
    pub fn extend(&mut self, units: impl IntoIterator<Item = Unit>) {
        for unit in units {
            self.push(unit);
        }
    }

    /// The object in slot `list_index` leaves the list; the last one takes its slot.
    ///
    /// # Errors
    ///
    /// The list holds no slot `list_index`.
    pub fn swap_remove(&mut self, list_index: ListIndex) -> GenieResult<Unit> {
        if !self.holds(list_index) {
            return Err(no_slot(list_index));
        }
        let last = ListIndex::new(self.units.len().wrapping_sub(1));
        let gone = self.units.swap_remove(list_index.get());
        self.forget(gone.id, list_index);
        if self
            .get(list_index)
            .is_some_and(|unit| unit.id == TAKEN_OUT)
        {
            self.moved_out(last, list_index);
        }
        Ok(gone)
    }

    /// Object `id` leaves the list; the last one takes its slot. None when the list does not
    /// hold it.
    pub fn swap_remove_id(&mut self, id: ObjectId) -> Option<Unit> {
        let list_index = self.position_of(id)?;
        self.swap_remove(list_index).ok()
    }

    /// The object in slot `list_index` leaves the list; the ones after it move up.
    ///
    /// # Errors
    ///
    /// The list holds no slot `list_index`.
    pub fn remove(&mut self, list_index: ListIndex) -> GenieResult<Unit> {
        if !self.holds(list_index) {
            return Err(no_slot(list_index));
        }
        let gone = self.units.remove(list_index.get());
        self.forget(gone.id, list_index);
        for o in self.outs.iter_mut().filter(|o| o.1 > list_index) {
            o.1 = ListIndex::new(o.1.get() - 1);
        }
        Ok(gone)
    }

    /// The last object leaves the list, if it holds one.
    pub fn pop(&mut self) -> Option<Unit> {
        let gone = self.units.pop()?;
        self.forget(gone.id, self.next_place());
        Some(gone)
    }

    /// The objects from slot `len` on leave the list.
    pub fn truncate(&mut self, len: usize) {
        while len < self.units.len() {
            let _ = self.pop();
        }
    }

    /// The objects `keep` answers false for leave the list; the others keep their order.
    pub fn retain(&mut self, mut keep: impl FnMut(&Unit) -> bool) {
        let kept: Vec<bool> = self.units.iter().map(&mut keep).collect();
        // each slot's place once the objects before it that go have gone
        let mut slot = Vec::with_capacity(kept.len());
        let mut n = 0;
        for &k in &kept {
            slot.push(n);
            n += usize::from(k);
        }
        // a stand-in's slot the list does not hold (it cannot be: the stand-ins are in the list)
        // goes with its record
        self.outs = self
            .outs
            .iter()
            .filter_map(|&(id, list_index)| {
                let moved = slot
                    .get(list_index.get())
                    .copied()
                    .filter(|_| kept.get(list_index.get()) == Some(&true))?;
                Some((id, ListIndex::new(moved)))
            })
            .collect();
        let mut keeps = kept.into_iter();
        self.units.retain(|_| keeps.next().unwrap_or(false));
    }

    /// Every object leaves the list.
    pub fn clear(&mut self) {
        self.units.clear();
        self.outs.clear();
    }

    /// Object `id` left slot `list_index`: a stand-in's object stays out for good.
    fn forget(&mut self, id: ObjectId, list_index: ListIndex) {
        if id == TAKEN_OUT {
            self.outs.retain(|o| o.1 != list_index);
        }
    }

    /// The stand-in in slot `from` moved to slot `to`.
    fn moved_out(&mut self, from: ListIndex, to: ListIndex) {
        for o in self.outs.iter_mut().filter(|o| o.1 == from) {
            o.1 = to;
        }
    }
}

impl From<Vec<Unit>> for ObjectList {
    fn from(units: Vec<Unit>) -> Self {
        Self {
            units,
            outs: Vec::new(),
        }
    }
}

impl FromIterator<Unit> for ObjectList {
    fn from_iter<I: IntoIterator<Item = Unit>>(units: I) -> Self {
        Self::from(units.into_iter().collect::<Vec<_>>())
    }
}

impl Index<ListIndex> for ObjectList {
    type Output = Unit;

    fn index(&self, list_index: ListIndex) -> &Unit {
        &self.units[list_index.get()]
    }
}

impl IndexMut<ListIndex> for ObjectList {
    fn index_mut(&mut self, list_index: ListIndex) -> &mut Unit {
        &mut self.units[list_index.get()]
    }
}

impl IntoIterator for ObjectList {
    type Item = Unit;
    type IntoIter = VecIntoIter<Unit>;

    fn into_iter(self) -> Self::IntoIter {
        self.units.into_iter()
    }
}

impl<'a> IntoIterator for &'a ObjectList {
    type Item = &'a Unit;
    type IntoIter = Iter<'a, Unit>;

    fn into_iter(self) -> Self::IntoIter {
        self.units.iter()
    }
}

impl<'a> IntoIterator for &'a mut ObjectList {
    type Item = &'a mut Unit;
    type IntoIter = IterMut<'a, Unit>;

    fn into_iter(self) -> Self::IntoIter {
        self.units.iter_mut()
    }
}

impl PartialEq for ObjectList {
    fn eq(&self, other: &Self) -> bool {
        self.units == other.units
    }
}

impl PartialEq<Vec<Unit>> for ObjectList {
    fn eq(&self, other: &Vec<Unit>) -> bool {
        &self.units == other
    }
}

impl<const N: usize> PartialEq<[Unit; N]> for ObjectList {
    fn eq(&self, other: &[Unit; N]) -> bool {
        self.units == other
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::HitPoints;
    use crate::test_ids::object_id;

    fn unit(id: i32) -> Unit {
        Unit {
            id: object_id(id),
            ..Unit::default()
        }
    }

    fn list(ids: &[i32]) -> ObjectList {
        ids.iter().map(|&id| unit(id)).collect()
    }

    /// Every id's slot as a scan of the list finds it.
    fn indexed_as_scanned(object_list: &ObjectList) -> bool {
        object_list.iter().all(|u| {
            object_list.position_of(u.id)
                == object_list
                    .iter()
                    .position(|v| v.id == u.id)
                    .map(ListIndex::new)
        })
    }

    #[test]
    fn lookup_by_id_finds_the_object_slot() {
        let mut object_list = list(&[5, 7, 9]);
        assert_eq!(
            object_list.position_of(object_id(7)),
            Some(ListIndex::new(1))
        );
        assert_eq!(object_list.by_id(object_id(9)).map(|u| u.id.get()), Some(9));
        assert!(object_list.by_id(object_id(8)).is_none());
        if let Some(u) = object_list.by_id_mut(object_id(5)) {
            u.life.hp = HitPoints::new(3.0);
        }
        assert_eq!(object_list[ListIndex::new(0)].life.hp, HitPoints::new(3.0));
        object_list.push(unit(11));
        assert_eq!(
            object_list.position_of(object_id(11)),
            Some(ListIndex::new(3))
        );
        object_list.extend([unit(12), unit(13)]);
        assert_eq!(
            object_list.position_of(object_id(13)),
            Some(ListIndex::new(5))
        );
        assert!(indexed_as_scanned(&object_list));
    }

    #[test]
    fn list_places_are_counted_once_and_checked() {
        let mut object_list = list(&[4, 5, 6]);
        let places: Vec<_> = object_list.places().map(ListIndex::get).collect();
        assert_eq!(places, [0, 1, 2]);
        assert_eq!(
            (object_list.next_place(), object_list.last_place()),
            (ListIndex::new(3), Some(ListIndex::new(2)))
        );
        // the loop holds no borrow of the list: it may change it
        for i in object_list.places().rev() {
            object_list.checked_mut(i).unwrap().life.hp = HitPoints::new(1.0);
            let _ = object_list.swap_remove(i).unwrap();
        }
        assert!(object_list.is_empty());
        assert_eq!(
            (object_list.next_place(), object_list.last_place()),
            (ListIndex::FIRST, None)
        );
        assert!(object_list.checked(ListIndex::FIRST).is_err());
        assert!(object_list.checked_mut(ListIndex::FIRST).is_err());
        assert!(object_list.last_mut().is_none());
        object_list.push(unit(9));
        assert_eq!(
            object_list.checked(ListIndex::FIRST).unwrap().id,
            object_id(9)
        );
        if let Some(u) = object_list.last_mut() {
            u.life.hp = HitPoints::new(2.0);
        }
        assert_eq!(object_list[ListIndex::FIRST].life.hp, HitPoints::new(2.0));
        assert!(object_list.holds(ListIndex::FIRST) && !object_list.holds(ListIndex::new(1)));
        let indexed: Vec<_> = object_list
            .indexed()
            .map(|(list_index, u)| (list_index, u.id))
            .collect();
        assert_eq!(indexed, [(ListIndex::FIRST, object_id(9))]);
    }

    #[test]
    fn the_list_removals_keep_every_slot() {
        let mut object_list = list(&[1, 2, 3, 4, 5, 6]);
        assert_eq!(
            object_list.swap_remove(ListIndex::new(1)).unwrap().id,
            object_id(2),
            "the last takes its slot"
        );
        assert_eq!(
            (
                object_list.position_of(object_id(6)),
                object_list.position_of(object_id(2))
            ),
            (Some(ListIndex::new(1)), None)
        );
        assert_eq!(
            object_list.remove(ListIndex::new(0)).unwrap().id,
            object_id(1),
            "the ones after it move up"
        );
        assert_eq!(
            object_list.position_of(object_id(6)),
            Some(ListIndex::new(0))
        );
        assert_eq!(object_list.pop().map(|u| u.id.get()), Some(5));
        assert!(object_list.by_id(object_id(5)).is_none());
        object_list.truncate(2);
        assert_eq!(
            object_list.iter().map(|u| u.id.get()).collect::<Vec<_>>(),
            [6, 3]
        );
        assert!(object_list.by_id(object_id(4)).is_none());
        object_list.retain(|unit| unit.id != object_id(6));
        assert_eq!(
            object_list.position_of(object_id(3)),
            Some(ListIndex::new(0))
        );
        let last = object_list.swap_remove(ListIndex::new(0)).unwrap();
        assert_eq!((last.id.get(), object_list.is_empty()), (3, true));
        object_list.clear();
        assert!(object_list.by_id(object_id(3)).is_none());
        assert!(indexed_as_scanned(&list(&[8, 9])));
    }

    #[test]
    fn an_object_out_for_its_update_is_not_in_its_list() {
        let mut object_list = list(&[1, 2]);
        let out = object_list.take_out(ListIndex::new(0)).unwrap();
        assert!(
            object_list.by_id(object_id(1)).is_none(),
            "its slot holds the stand-in"
        );
        assert!(
            object_list.by_id(TAKEN_OUT).is_none(),
            "the stand-in is no object"
        );
        assert_eq!(object_list.put_back(out).ok(), Some(ListIndex::new(0)));
        assert_eq!(
            object_list.position_of(object_id(1)),
            Some(ListIndex::new(0)),
            "put back, it is found again"
        );
    }

    fn ids(object_list: &ObjectList) -> Vec<i32> {
        object_list.iter().map(|unit| unit.id.get()).collect()
    }

    #[test]
    fn an_object_taken_out_goes_back_where_its_stand_in_is() {
        let mut object_list = list(&[1, 2, 3]);
        let out = object_list.take_out(ListIndex::new(2)).unwrap();
        assert_eq!(out.id, object_id(3));
        assert!(
            object_list.by_id(object_id(3)).is_none(),
            "its slot holds the stand-in"
        );
        assert_eq!(
            object_list.swap_remove(ListIndex::new(0)).unwrap().id,
            object_id(1),
            "the stand-in, last, takes slot 0"
        );
        assert_eq!(object_list.put_back(out).ok(), Some(ListIndex::new(0)));
        assert_eq!(ids(&object_list), [3, 2]);
        assert_eq!(
            object_list.position_of(object_id(3)),
            Some(ListIndex::new(0))
        );
        assert!(indexed_as_scanned(&object_list));
    }

    #[test]
    fn nested_take_outs_go_back_each_to_its_own_slot() {
        let mut object_list = list(&[1, 2, 3, 4, 5]);
        let a = object_list.take_out(ListIndex::new(1)).unwrap();
        let take_out = object_list.take_out(ListIndex::new(3)).unwrap();
        assert_eq!(
            object_list.remove(ListIndex::new(0)).unwrap().id,
            object_id(1),
            "both stand-ins move up"
        );
        assert_eq!(object_list.put_back(take_out).ok(), Some(ListIndex::new(2)));
        object_list.retain(|unit| unit.id != object_id(3));
        assert_eq!(
            object_list.put_back(a).ok(),
            Some(ListIndex::new(0)),
            "the stand-in kept its place"
        );
        assert_eq!(ids(&object_list), [2, 4, 5]);
        assert!(indexed_as_scanned(&object_list));
    }

    #[test]
    fn stand_in_that_left_its_list_takes_nothing_back() {
        let mut l = list(&[1, 2]);
        let out = l.take_out(ListIndex::new(1)).unwrap();
        assert_eq!(l.pop().map(|u| u.id), Some(TAKEN_OUT));
        assert_eq!(l.put_back(out).ok(), None);
        let out = l.take_out(ListIndex::new(0)).unwrap();
        assert_eq!(l.swap_remove(ListIndex::new(0)).unwrap().id, TAKEN_OUT);
        assert_eq!(l.put_back(out).ok(), None);
        assert_eq!(l.put_back(unit(9)).ok(), None, "an object never taken out");
        assert!(l.is_empty());
        let mut object_list = list(&[1, 2]);
        let out = object_list.take_out(ListIndex::new(0)).unwrap();
        assert_eq!(object_list.remove(ListIndex::new(0)).unwrap().id, TAKEN_OUT);
        object_list.retain(|_| true);
        assert_eq!(object_list.put_back(out).ok(), None);
        let out = object_list.take_out(ListIndex::new(0)).unwrap();
        object_list.clear();
        assert_eq!(object_list.put_back(out).ok(), None);
    }

    #[test]
    fn two_stand_ins_leave_the_lookup_answering() {
        let mut object_list = list(&[1, 2, 3]);
        let (a, b) = (
            object_list.take_out(ListIndex::new(0)).unwrap(),
            object_list.take_out(ListIndex::new(2)).unwrap(),
        );
        assert_eq!(
            object_list.position_of(object_id(2)),
            Some(ListIndex::new(1))
        );
        assert_eq!(
            (object_list.put_back(a).ok(), object_list.put_back(b).ok()),
            (Some(ListIndex::new(0)), Some(ListIndex::new(2)))
        );
        assert!(indexed_as_scanned(&object_list));
    }

    #[test]
    fn list_that_held_an_id_twice_finds_its_first_slot() {
        let mut object_list = list(&[0, 4, 0]);
        assert_eq!(
            object_list.position_of(object_id(0)),
            Some(ListIndex::new(0))
        );
        let _ = object_list.swap_remove(ListIndex::new(0)).unwrap();
        assert_eq!(
            object_list.position_of(object_id(0)),
            Some(ListIndex::new(0)),
            "the other one, which took the slot"
        );
        let _ = object_list.remove(ListIndex::new(0)).unwrap();
        assert_eq!(object_list.position_of(object_id(0)), None);
        assert_eq!(
            object_list.position_of(object_id(4)),
            Some(ListIndex::new(0))
        );
        object_list[ListIndex::new(0)].id = object_id(6);
        assert_eq!(
            object_list.position_of(object_id(6)),
            Some(ListIndex::new(0)),
            "an id written in place counts"
        );
    }

    #[test]
    fn lists_compare_by_their_objects_in_order() {
        let mut a = list(&[1, 2]);
        let b = list(&[1, 2]);
        assert_eq!(a, b);
        assert_eq!(a, vec![unit(1), unit(2)]);
        assert_eq!(a, [unit(1), unit(2)]);
        let _ = a.swap_remove(ListIndex::new(0)).unwrap();
        assert_ne!(a, b);
        let ids: Vec<ObjectId> = b.into_iter().map(|u| u.id).collect();
        assert_eq!(ids, [object_id(1), object_id(2)]);
        let mut object_list = list(&[3]);
        for u in &mut object_list {
            u.life.hp = HitPoints::new(2.0);
        }
        assert_eq!(
            (&object_list)
                .into_iter()
                .map(|u| u.life.hp.get())
                .sum::<f32>(),
            2.0
        );
    }
}
