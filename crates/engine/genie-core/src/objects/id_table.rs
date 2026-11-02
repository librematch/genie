//! A table of values by object id: a dense column per sign of the id, so that a lookup indexes
//! and a walk takes the ids in a sorted map's order, the most negative first (the order the
//! side tables kept as `BTreeMap`s).

use std::collections::{BTreeMap, btree_map};

use crate::GenieError;
use crate::objects::ObjectId;
use std::mem::take as mem_take;

/// The most ids on either side of 0 a column holds; an id beyond (a stand-in such as
/// [`ObjectId::UNSAVED`]) goes to the sorted rest.
const DENSE: usize = 1 << 22;

/// Values by object id. The world's ids count up from 0 and its recycled ids down from −2, so
/// two dense columns hold them: `ahead[id]` and `behind[−id − 2]`; the ids beyond either
/// column (the stand-ins) are kept sorted apart. No object's id is −1. Two tables are equal
/// when they hold the same ids and values, as two maps are.
#[derive(Clone, Debug)]
pub struct IdTable<T> {
    /// The values of ids 0 and up, by id.
    ahead: Column<T>,
    /// The values of ids −2 and down, by `−id − 2`.
    behind: Column<T>,
    /// The values of ids below `behind`'s reach.
    far_below: BTreeMap<ObjectId, T>,
    /// The values of ids past `ahead`'s reach.
    far_above: BTreeMap<ObjectId, T>,
    /// How many values the table holds.
    len: usize,
}

/// One dense column: a value per index and a bit per index that holds one, so that a walk
/// skips 64 empty indices at a time.
#[derive(Clone, Debug)]
struct Column<T> {
    /// The values by index.
    values: Vec<Option<T>>,
    /// Bit `i % 64` of word `i / 64`: index `i` holds a value.
    held: Vec<u64>,
}

impl<T> Default for Column<T> {
    fn default() -> Self {
        Self {
            values: Vec::new(),
            held: Vec::new(),
        }
    }
}

impl<T> Column<T> {
    /// The value at `i`.
    fn get(&self, i: usize) -> Option<&T> {
        self.values.get(i)?.as_ref()
    }

    /// The value at `i` to change.
    fn get_mut(&mut self, i: usize) -> Option<&mut T> {
        self.values.get_mut(i)?.as_mut()
    }

    /// `value` at `i`, the column grown to hold it; the value it held.
    fn put(&mut self, i: usize, value: T) -> Option<T> {
        if self.values.len() <= i {
            self.values.resize_with(i + 1, || None);
            self.held.resize(i / 64 + 1, 0);
        }
        if let Some(word) = self.held.get_mut(i / 64) {
            *word |= 1 << (i % 64);
        }
        self.values.get_mut(i).and_then(|v| v.replace(value))
    }

    /// The value at `i` taken out.
    fn take(&mut self, i: usize) -> Option<T> {
        if let Some(word) = self.held.get_mut(i / 64) {
            *word &= !(1 << (i % 64));
        }
        self.values.get_mut(i).and_then(Option::take)
    }
}

/// The indices whose bit is set in a column's words, rising or (`down`) falling.
#[derive(Clone, Debug)]
struct Bits<'a> {
    /// The words.
    words: &'a [u64],
    /// The word being walked; `words.len()` (rising) or none left (falling) at the end.
    word: usize,
    /// Its bits not walked yet.
    left: u64,
    /// Falling.
    down: bool,
}

impl<'a> Bits<'a> {
    /// The set bits of `words`, rising.
    fn up(words: &'a [u64]) -> Self {
        Self {
            words,
            word: 0,
            left: words.first().copied().unwrap_or(0),
            down: false,
        }
    }

    /// The set bits of `words`, falling.
    fn down(words: &'a [u64]) -> Self {
        let word = words.len().saturating_sub(1);
        Self {
            words,
            word,
            left: words.get(word).copied().unwrap_or(0),
            down: true,
        }
    }
}

impl Iterator for Bits<'_> {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        while self.left == 0 {
            if self.down {
                self.word = self.word.checked_sub(1)?;
            } else {
                self.word += 1;
            }
            self.left = *self.words.get(self.word)?;
        }
        let bit = if self.down {
            self.left.ilog2()
        } else {
            self.left.trailing_zeros()
        };
        self.left &= !(1 << bit);
        Some(self.word * 64 + bit as usize)
    }
}

impl<T> Default for IdTable<T> {
    fn default() -> Self {
        Self {
            ahead: Column::default(),
            behind: Column::default(),
            far_below: BTreeMap::new(),
            far_above: BTreeMap::new(),
            len: 0,
        }
    }
}

/// Where an id's value lies.
enum Slot {
    /// `ahead[i]`.
    Ahead(usize),
    /// `behind[i]`.
    Behind(usize),
    /// In `far_below`.
    FarBelow,
    /// In `far_above`.
    FarAbove,
}

/// The slot of `id`.
fn slot(id: ObjectId) -> Slot {
    let v = id.get();
    if v >= 0 {
        usize::try_from(v)
            .ok()
            .filter(|&i| i < DENSE)
            .map_or(Slot::FarAbove, Slot::Ahead)
    } else {
        // −2 is behind[0]; no object's id is −1
        usize::try_from(-i64::from(v) - 2)
            .ok()
            .filter(|&i| i < DENSE)
            .map_or(Slot::FarBelow, Slot::Behind)
    }
}

/// The id of `ahead[i]`.
fn ahead_id(i: usize) -> Option<ObjectId> {
    ObjectId::of_field(i32::try_from(i).ok()?)
}

/// The id of `behind[i]`.
fn behind_id(i: usize) -> Option<ObjectId> {
    ObjectId::of_field(-i32::try_from(i).ok()? - 2)
}

impl<T> IdTable<T> {
    /// An empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// How many values it holds.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether it holds none.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The value of `id`.
    #[must_use]
    pub fn get(&self, id: ObjectId) -> Option<&T> {
        match slot(id) {
            Slot::Ahead(i) => self.ahead.get(i),
            Slot::Behind(i) => self.behind.get(i),
            Slot::FarBelow => self.far_below.get(&id),
            Slot::FarAbove => self.far_above.get(&id),
        }
    }

    /// The value of `id` to change.
    pub fn get_mut(&mut self, id: ObjectId) -> Option<&mut T> {
        match slot(id) {
            Slot::Ahead(i) => self.ahead.get_mut(i),
            Slot::Behind(i) => self.behind.get_mut(i),
            Slot::FarBelow => self.far_below.get_mut(&id),
            Slot::FarAbove => self.far_above.get_mut(&id),
        }
    }

    /// Whether it holds a value of `id`.
    #[must_use]
    pub fn contains_key(&self, id: ObjectId) -> bool {
        self.get(id).is_some()
    }

    /// `value` for `id`; the value it held before.
    pub fn insert(&mut self, id: ObjectId, value: T) -> Option<T> {
        let old = match slot(id) {
            Slot::Ahead(i) => self.ahead.put(i, value),
            Slot::Behind(i) => self.behind.put(i, value),
            Slot::FarBelow => self.far_below.insert(id, value),
            Slot::FarAbove => self.far_above.insert(id, value),
        };
        if old.is_none() {
            self.len += 1;
        }
        old
    }

    /// The value of `id`, `make`'s first when the table holds none (a map's `entry`).
    ///
    /// # Errors
    ///
    /// A value the table does not find right after it put it there (an invariant broken).
    pub fn get_or_insert_with(
        &mut self,
        id: ObjectId,
        make: impl FnOnce() -> T,
    ) -> Result<&mut T, GenieError> {
        if !self.contains_key(id) {
            let _ = self.insert(id, make());
        }
        self.get_mut(id).ok_or_else(|| {
            GenieError::unreachable("an id table's value missing right after its insert")
                .with("id", id)
        })
    }

    /// Every value taken out.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// The value of `id` taken out.
    pub fn remove(&mut self, id: ObjectId) -> Option<T> {
        let old = match slot(id) {
            Slot::Ahead(i) => self.ahead.take(i),
            Slot::Behind(i) => self.behind.take(i),
            Slot::FarBelow => self.far_below.remove(&id),
            Slot::FarAbove => self.far_above.remove(&id),
        };
        if old.is_some() {
            self.len -= 1;
        }
        old
    }

    /// Every id and its value, the most negative id first.
    #[must_use]
    pub fn iter(&self) -> Iter<'_, T> {
        Iter {
            far_below: self.far_below.iter(),
            behind: (&self.behind.values, Bits::down(&self.behind.held)),
            ahead: (&self.ahead.values, Bits::up(&self.ahead.held)),
            far_above: self.far_above.iter(),
        }
    }

    /// Every id and its value to change, the most negative id first.
    pub fn iter_mut(&mut self) -> IterMut<'_, T> {
        IterMut {
            far_below: self.far_below.iter_mut(),
            behind: Falling {
                values: &mut self.behind.values,
                bits: Bits::down(&self.behind.held),
            },
            ahead: Rising {
                values: &mut self.ahead.values,
                at: 0,
                bits: Bits::up(&self.ahead.held),
            },
            far_above: self.far_above.iter_mut(),
        }
    }

    /// Every id, the most negative first.
    pub fn keys(&self) -> impl Iterator<Item = ObjectId> + '_ {
        self.iter().map(|(id, _)| id)
    }

    /// Every value, in its id's order.
    pub fn values(&self) -> impl Iterator<Item = &T> {
        self.iter().map(|(_, v)| v)
    }

    /// Every value to change, in its id's order.
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.iter_mut().map(|(_, v)| v)
    }
}

/// [`IdTable::iter`]'s walk.
#[derive(Clone, Debug)]
pub struct Iter<'a, T> {
    /// The ids below the columns.
    far_below: btree_map::Iter<'a, ObjectId, T>,
    /// The negative column, falling indices (rising ids).
    behind: (&'a [Option<T>], Bits<'a>),
    /// The non-negative column, rising.
    ahead: (&'a [Option<T>], Bits<'a>),
    /// The ids past the columns.
    far_above: btree_map::Iter<'a, ObjectId, T>,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = (ObjectId, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some((&id, v)) = self.far_below.next() {
            return Some((id, v));
        }
        for i in self.behind.1.by_ref() {
            if let (Some(id), Some(Some(v))) = (behind_id(i), self.behind.0.get(i)) {
                return Some((id, v));
            }
        }
        for i in self.ahead.1.by_ref() {
            if let (Some(id), Some(Some(v))) = (ahead_id(i), self.ahead.0.get(i)) {
                return Some((id, v));
            }
        }
        self.far_above.next().map(|(&id, v)| (id, v))
    }
}

impl<'a, T> IntoIterator for &'a IdTable<T> {
    type Item = (ObjectId, &'a T);
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Iter<'a, T> {
        self.iter()
    }
}

/// The negative column's values to change, falling indices: each taken off the slice's end.
#[derive(Debug)]
struct Falling<'a, T> {
    /// The values not passed yet (indices below the last one walked).
    values: &'a mut [Option<T>],
    /// The held indices, falling.
    bits: Bits<'a>,
}

/// The non-negative column's values to change, rising: each taken off the slice's front.
#[derive(Debug)]
struct Rising<'a, T> {
    /// The values not passed yet, from index `at` on.
    values: &'a mut [Option<T>],
    /// The index of `values[0]`.
    at: usize,
    /// The held indices, rising.
    bits: Bits<'a>,
}

/// [`IdTable::iter_mut`]'s walk.
#[derive(Debug)]
pub struct IterMut<'a, T> {
    /// The ids below the columns.
    far_below: btree_map::IterMut<'a, ObjectId, T>,
    /// The negative column.
    behind: Falling<'a, T>,
    /// The non-negative column.
    ahead: Rising<'a, T>,
    /// The ids past the columns.
    far_above: btree_map::IterMut<'a, ObjectId, T>,
}

impl<'a, T> Iterator for IterMut<'a, T> {
    type Item = (ObjectId, &'a mut T);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some((&id, v)) = self.far_below.next() {
            return Some((id, v));
        }
        for i in self.behind.bits.by_ref() {
            // a held index lies below the values not passed yet, so the split finds it
            let (head, tail) = mem_take(&mut self.behind.values).split_at_mut_checked(i)?;
            self.behind.values = head;
            if let (Some(id), Some(Some(v))) = (behind_id(i), tail.first_mut()) {
                return Some((id, v));
            }
        }
        for i in self.ahead.bits.by_ref() {
            let rest = mem_take(&mut self.ahead.values);
            let (head, tail) = rest.split_at_mut_checked(i.checked_sub(self.ahead.at)? + 1)?;
            self.ahead.values = tail;
            self.ahead.at = i + 1;
            if let (Some(id), Some(Some(v))) = (ahead_id(i), head.last_mut()) {
                return Some((id, v));
            }
        }
        self.far_above.next().map(|(&id, v)| (id, v))
    }
}

impl<'a, T> IntoIterator for &'a mut IdTable<T> {
    type Item = (ObjectId, &'a mut T);
    type IntoIter = IterMut<'a, T>;

    fn into_iter(self) -> IterMut<'a, T> {
        self.iter_mut()
    }
}

impl<T: PartialEq> PartialEq for IdTable<T> {
    fn eq(&self, other: &Self) -> bool {
        self.len == other.len && self.iter().eq(other.iter())
    }
}

impl<T: Eq> Eq for IdTable<T> {}

impl<T> FromIterator<(ObjectId, T)> for IdTable<T> {
    fn from_iter<I: IntoIterator<Item = (ObjectId, T)>>(iter: I) -> Self {
        let mut table = Self::new();
        for (id, value) in iter {
            let _ = table.insert(id, value);
        }
        table
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::objects::ObjectId;
    use crate::objects::id_table::IdTable;
    #[cfg(test)]
    use std::iter::once;

    /// Object `value`, which the test knows is an id.
    fn id(value: i32) -> ObjectId {
        ObjectId::try_from(value).unwrap()
    }

    /// The table walks its ids as a sorted map does, the most negative first, the stand-ins
    /// beyond the columns included, through any order of inserts, overwrites and removes; a
    /// walk to change takes the same order.
    #[test]
    fn the_table_walks_its_ids_as_sorted_map() {
        let mut table = IdTable::new();
        let mut map = BTreeMap::new();
        let ids = [
            5,
            -2,
            0,
            -7,
            100,
            63,
            64,
            -66,
            -65,
            i32::MIN,
            i32::MIN + 1,
            i32::MAX,
            -3,
            1,
        ];
        let mut seed = 7_u32;
        for (n, &v) in ids.iter().cycle().take(400).enumerate() {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            if seed >> 30 == 0 {
                assert_eq!(table.remove(id(v)), map.remove(&id(v)));
            } else {
                assert_eq!(table.insert(id(v), n), map.insert(id(v), n));
            }
            assert_eq!(table.len(), map.len());
            assert!(
                table
                    .iter()
                    .map(|(k, &x)| (k, x))
                    .eq(map.iter().map(|(&k, &x)| (k, x)))
            );
            let walked: Vec<ObjectId> = table.iter_mut().map(|(k, _)| k).collect();
            assert!(walked.into_iter().eq(map.keys().copied()));
        }
        for &v in &ids {
            assert_eq!(table.get(id(v)), map.get(&id(v)));
            assert_eq!(table.contains_key(id(v)), map.contains_key(&id(v)));
        }
    }

    /// A value changes in place, through a lookup or a walk; a removed id is gone and the table
    /// empty after the last.
    #[test]
    fn value_changes_in_place_and_goes_with_its_id() {
        let mut table: IdTable<i32> = [(id(3), 1), (id(-4), 2), (id(i32::MAX), 9)]
            .into_iter()
            .collect();
        *table.get_mut(id(-4)).unwrap() += 5;
        for v in table.values_mut() {
            *v *= 10;
        }
        assert_eq!(table.values().copied().collect::<Vec<_>>(), [70, 10, 90]);
        assert_eq!(
            table.keys().collect::<Vec<_>>(),
            [id(-4), id(3), id(i32::MAX)]
        );
        assert_eq!(table.remove(id(3)), Some(10));
        assert_eq!(table.remove(id(3)), None);
        assert_eq!(table.get_mut(id(9)), None);
        assert_eq!(table.get_or_insert_with(id(-4), || 0), Ok(&mut 70));
        assert_eq!(table.get_or_insert_with(id(8), || 4), Ok(&mut 4));
        assert_eq!(table.remove(id(8)), Some(4));
        assert_eq!(table.remove(id(-4)), Some(70));
        assert_eq!(table.remove(id(i32::MAX)), Some(90));
        assert!(table.is_empty());
        assert_eq!((&table).into_iter().count(), 0);
        let mut full: IdTable<i32> = [(id(1), 1), (id(-9), 2)].into_iter().collect();
        full.clear();
        assert_eq!(full, IdTable::new());
    }

    /// Two tables with the same ids and values are equal, whatever their columns once held.
    #[test]
    fn tables_with_the_same_values_are_equal() {
        let mut grown: IdTable<i32> = [(id(900), 1), (id(-900), 2), (id(4), 3)]
            .into_iter()
            .collect();
        let _ = grown.remove(id(900));
        let _ = grown.remove(id(-900));
        let small: IdTable<i32> = once((id(4), 3)).collect();
        assert_eq!(grown, small);
        assert_ne!(grown, IdTable::new());
    }
}
