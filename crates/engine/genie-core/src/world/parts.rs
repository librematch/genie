//! The parts only some objects hold, each in a table by id on the world (FD-045-D51), so that an
//! object's row holds only what every object has.
//!
//! The bookkeeping holds by construction: a table's [`PartTable::insert`] and
//! [`PartTable::remove`] are the world's alone, and its list operations (the one writer of the
//! table by id, FD-045-D49) are their writers. An object enters the world as one value with its
//! parts ([`Object`]) and a removal for good returns it with them, so a move between lists or
//! players carries its parts and no part outlives its object or misses it; taking an object out
//! for its update leaves its parts in the tables. [`World::check_parts`] tells when a table holds
//! an id the world does not. A comparison reads an object whole too ([`ObjectView`]), so no
//! part escapes the comparison with the game's save.

use crate::GenieError;
use crate::commands::GatherPoint;
use crate::objects::IdTable;
use crate::objects::ObjectId;
use crate::objects::masters::Master;
use crate::objects::unit::Unit;
use crate::world::World;
use genie_diff::{Diff, Differ};
use std::borrow::Cow;

/// One part's table: the parts packed in a dense vector in the order they came, each id's slot
/// in a table by id. A removal moves the last part into the gap.
#[derive(Clone, Debug)]
pub struct PartTable<T> {
    /// The parts, packed.
    parts: Vec<T>,
    /// The object of each part, by the part's slot.
    ids: Vec<ObjectId>,
    /// Each object's slot in `parts`.
    slots: IdTable<usize>,
}

impl<T> Default for PartTable<T> {
    fn default() -> Self {
        Self {
            parts: Vec::new(),
            ids: Vec::new(),
            slots: IdTable::new(),
        }
    }
}

impl<T> PartTable<T> {
    /// Object `id`'s part, if it holds one.
    #[must_use]
    pub fn get(&self, id: ObjectId) -> Option<&T> {
        self.parts.get(*self.slots.get(id)?)
    }

    /// Object `id`'s part to change, if it holds one.
    pub fn get_mut(&mut self, id: ObjectId) -> Option<&mut T> {
        self.parts.get_mut(*self.slots.get(id)?)
    }

    /// Whether object `id` holds the part.
    #[must_use]
    pub fn contains(&self, id: ObjectId) -> bool {
        self.slots.contains_key(id)
    }

    /// How many objects hold the part.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.parts.len()
    }

    /// Whether no object holds the part.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    /// The objects that hold the part and their parts, packed (in the order they came, but for
    /// the removals' moves).
    pub fn iter(&self) -> impl Iterator<Item = (ObjectId, &T)> {
        self.ids.iter().copied().zip(&self.parts)
    }

    /// Object `id` holds `part` (the world's list operations and [`World::set_part`] alone);
    /// the part it held before, if any.
    pub(in crate::world) fn insert(&mut self, id: ObjectId, part: T) -> Option<T> {
        if let Some(held) = self.get_mut(id) {
            return Some(std::mem::replace(held, part));
        }
        let _ = self.slots.insert(id, self.parts.len());
        self.parts.push(part);
        self.ids.push(id);
        None
    }

    /// Object `id`'s part taken out of the table (the world's list operations and
    /// [`World::set_part`] alone).
    pub(in crate::world) fn remove(&mut self, id: ObjectId) -> Option<T> {
        let slot = self.slots.remove(id)?;
        let part = self.parts.swap_remove(slot);
        let _ = self.ids.swap_remove(slot);
        if let Some(&moved) = self.ids.get(slot) {
            let _ = self.slots.insert(moved, slot);
        }
        Some(part)
    }
}

impl<T: PartialEq> PartialEq for PartTable<T> {
    /// Two tables are equal when the same objects hold equal parts, in any order.
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().all(|(id, p)| other.get(id) == Some(p))
    }
}

/// The parts the world keeps by id: one table per part.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Parts {
    /// `+0x6b8`: the copy of an old owner's death-spawn master an ownership change took
    /// ([`crate::ownership::change::change_hands`]), which the object's death spawn makes its
    /// dead object of (`FUN_1411a5090`).
    pub dead_master: PartTable<Master>,
    /// A building's gather point (`+0xc30`..`+0xc44`, the save's `gather_point_*`), which its
    /// trained units go to: SET GATHER POINT's and SET GATHER STATE's.
    pub gather_point: PartTable<GatherPoint>,
}

/// One object's parts as a value, outside the world's tables: what it enters the world with and
/// what a removal for good returns. Each part compares as the field it was on the object.
#[derive(Clone, Debug, Default, PartialEq, Diff)]
pub struct ObjectParts {
    /// Its dead master ([`Parts::dead_master`]).
    #[diff(gap = "the import does not read a saved dead master")]
    pub dead_master: Option<Master>,
    /// Its gather point ([`Parts::gather_point`]).
    #[diff(path = "building.gather_point")]
    pub gather_point: Option<GatherPoint>,
}

/// An object as one value: its row ([`Unit`], what the lists hold) and its parts. The world takes
/// an object in and gives it back whole; a [`Unit`] alone is an object without parts.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Object {
    /// Its row.
    pub unit: Unit,
    /// Its parts.
    pub parts: ObjectParts,
}

impl Diff for Object {
    /// As its view ([`ObjectView`]).
    fn diff(&self, other: &Self, d: &mut Differ) {
        self.view().diff(&other.view(), d);
    }
}

impl Object {
    /// The object as a comparison reads it, its parts its own.
    #[must_use]
    pub fn view(&self) -> ObjectView<'_> {
        self.into()
    }
}

/// An object as a comparison reads it: its row and its parts, the parts lent by an [`Object`]
/// (the game's save, imported) or copied out of the world's tables ([`World::object_view`]).
#[derive(Clone, Debug)]
pub struct ObjectView<'a> {
    /// Its row.
    pub unit: &'a Unit,
    /// Its parts.
    pub parts: Cow<'a, ObjectParts>,
}

impl Diff for ObjectView<'_> {
    /// The row's fields, then the parts', each at the path it had on the object.
    fn diff(&self, other: &Self, d: &mut Differ) {
        self.unit.diff(other.unit, d);
        self.parts.as_ref().diff(other.parts.as_ref(), d);
    }
}

impl<'a> From<&'a Object> for ObjectView<'a> {
    fn from(o: &'a Object) -> Self {
        ObjectView {
            unit: &o.unit,
            parts: Cow::Borrowed(&o.parts),
        }
    }
}

impl From<Unit> for Object {
    fn from(unit: Unit) -> Self {
        Self {
            unit,
            parts: ObjectParts::default(),
        }
    }
}

impl Parts {
    /// Object `id`'s parts filed in the tables (it enters the world).
    pub(in crate::world) fn file(&mut self, id: ObjectId, parts: ObjectParts) {
        let ObjectParts {
            dead_master,
            gather_point,
        } = parts;
        if let Some(m) = dead_master {
            let _ = self.dead_master.insert(id, m);
        }
        if let Some(g) = gather_point {
            let _ = self.gather_point.insert(id, g);
        }
    }

    /// Object `id`'s parts taken out of the tables (it leaves the world).
    pub(in crate::world) fn take(&mut self, id: ObjectId) -> ObjectParts {
        ObjectParts {
            dead_master: self.dead_master.remove(id),
            gather_point: self.gather_point.remove(id),
        }
    }

    /// Object `id`'s parts, copied (a comparison's whole object).
    #[must_use]
    pub fn of(&self, id: ObjectId) -> ObjectParts {
        ObjectParts {
            dead_master: self.dead_master.get(id).cloned(),
            gather_point: self.gather_point.get(id).copied(),
        }
    }

    /// Every object id a table holds, each once per table that holds it.
    fn ids(&self) -> impl Iterator<Item = ObjectId> + '_ {
        self.dead_master
            .iter()
            .map(|(id, _)| id)
            .chain(self.gather_point.iter().map(|(id, _)| id))
    }
}

/// A part of an object: which table [`World::set_part`] writes.
#[derive(Clone, Debug, PartialEq)]
pub enum Part {
    /// The dead master ([`Parts::dead_master`]), boxed: a master is about 700 bytes.
    DeadMaster(Box<Master>),
    /// The gather point ([`Parts::gather_point`]), none clearing it.
    GatherPoint(Option<GatherPoint>),
}

impl World {
    /// Object `id` holds `part` now, in its life (the routine that makes the part). The world
    /// must hold the object: a part of an object it does not hold would outlive it.
    ///
    /// # Errors
    ///
    /// An object the world does not hold.
    pub fn set_part(&mut self, id: ObjectId, part: Part) -> Result<(), GenieError> {
        if self.index.filed(id).is_none() && !self.index.scans() {
            return Err(
                GenieError::unreachable("a part of an object the world does not hold")
                    .with("object", id),
            );
        }
        match part {
            Part::DeadMaster(m) => {
                let _ = self.parts.dead_master.insert(id, *m);
            }
            Part::GatherPoint(Some(g)) => {
                let _ = self.parts.gather_point.insert(id, g);
            }
            Part::GatherPoint(None) => {
                let _ = self.parts.gather_point.remove(id);
            }
        }
        Ok(())
    }

    /// A loaded object's saved parts go into the tables (the import, after the world's lists
    /// stand). The world must hold the object.
    ///
    /// # Errors
    ///
    /// An object the world does not hold.
    pub fn load_parts(&mut self, id: ObjectId, parts: ObjectParts) -> Result<(), GenieError> {
        if self.index.filed(id).is_none() && !self.index.scans() {
            return Err(GenieError::unreachable(
                "a loaded part of an object the world does not hold",
            )
            .with("object", id));
        }
        self.parts.file(id, parts);
        Ok(())
    }

    /// Object `unit` of the world's lists with its parts from the world's tables, as a
    /// comparison reads it.
    #[must_use]
    pub fn object_view<'a>(&'a self, unit: &'a Unit) -> ObjectView<'a> {
        ObjectView {
            unit,
            parts: Cow::Owned(self.parts.of(unit.id)),
        }
    }

    /// Object `unit` of the world's lists with its parts from the world's tables, as one value
    /// apart from the world (a run's result).
    #[must_use]
    pub fn object_of(&self, unit: &Unit) -> Object {
        Object {
            unit: unit.clone(),
            parts: self.parts.of(unit.id),
        }
    }

    /// The parts' bookkeeping: every id a table holds is an object the world files (live,
    /// sleeping, or out for its update).
    ///
    /// # Errors
    ///
    /// The first id a table holds that the world does not.
    pub fn check_parts(&self) -> Result<(), GenieError> {
        if self.index.scans() {
            return Ok(());
        }
        self.parts
            .ids()
            .find(|&id| self.index.filed(id).is_none())
            .map_or(Ok(()), |id| {
                Err(
                    GenieError::unreachable("a part whose object left the world")
                        .with("object", id),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_ids::object_id;

    /// The table finds each object's part by id; a removal moves the last part into the gap and
    /// its slot follows; a second insert replaces; two tables of the same parts in another
    /// order are equal.
    #[test]
    fn a_part_table_packs_its_parts_and_finds_them_by_id() {
        let mut t = PartTable::default();
        assert_eq!(t.insert(object_id(5), 'a'), None);
        assert_eq!(t.insert(object_id(-3), 'b'), None);
        assert_eq!(t.insert(object_id(9), 'c'), None);
        assert_eq!(t.insert(object_id(9), 'd'), Some('c'));
        assert_eq!((t.len(), t.get(object_id(-3))), (3, Some(&'b')));
        assert_eq!(t.remove(object_id(5)), Some('a'));
        assert_eq!(t.remove(object_id(5)), None);
        assert_eq!(t.get(object_id(9)), Some(&'d'));
        assert_eq!(
            t.iter().collect::<Vec<_>>(),
            [(object_id(9), &'d'), (object_id(-3), &'b')]
        );
        *t.get_mut(object_id(-3)).unwrap() = 'e';
        let mut u = PartTable::default();
        let _ = u.insert(object_id(-3), 'e');
        let _ = u.insert(object_id(9), 'd');
        assert_eq!(t, u);
        assert!(!t.contains(object_id(5)) && !t.is_empty());
    }

    /// The world files an object's parts with it: a part made in its life only for an object
    /// it files; the lists' removal returns the object with its parts, which leave the tables,
    /// and a push files them again; the world's view and its whole object carry them, and an
    /// object without them differs at the part's old path; a table entry whose object the
    /// world does not file fails the check.
    #[test]
    fn the_world_files_an_object_s_parts_with_it() {
        use crate::objects::ListIndex;
        use crate::players::PlayerId;
        use genie_diff::{Scope, differences};
        let id = object_id(5);
        let mut t = World::new(
            crate::world::WorldTime::ZERO,
            crate::world::FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![crate::world::Millis::ZERO],
            vec![crate::world::player::Player {
                objects: vec![Unit {
                    id,
                    ..Unit::default()
                }]
                .into(),
                ..Default::default()
            }],
        );
        let dead = Master::default();
        assert!(
            t.set_part(object_id(6), Part::DeadMaster(Box::new(dead.clone())))
                .is_err()
        );
        t.set_part(id, Part::DeadMaster(Box::new(dead.clone())))
            .unwrap();
        t.check_parts().unwrap();
        let unit = t.players[0].objects.by_id(id).unwrap();
        let view = t.object_view(unit);
        assert_eq!(view.parts.dead_master.as_ref(), Some(&dead));
        let whole = t.object_of(unit);
        assert_eq!(differences(&view, &whole.view(), Scope::All), []);
        let bare = Object::from(unit.clone());
        let paths: Vec<String> = differences(&view, &bare.view(), Scope::SavedWithGaps)
            .into_iter()
            .map(|d| d.path)
            .collect();
        assert_eq!(paths, ["dead_master"]);
        let gone = t
            .swap_remove_live(PlayerId::new(0), ListIndex::FIRST)
            .unwrap();
        assert_eq!(gone.parts.dead_master.as_ref(), Some(&dead));
        assert!(t.parts.dead_master.is_empty());
        t.check_parts().unwrap();
        let _ = t.push_live(PlayerId::new(0), gone).unwrap();
        assert!(t.parts.dead_master.contains(id));
        let _ = t.parts.dead_master.insert(object_id(9), dead);
        assert!(t.check_parts().is_err());
    }

    /// A loaded object's parts go into the tables only for an object the world holds; the
    /// gather point's part is set and cleared in the object's life, and its removal from the
    /// lists carries it.
    #[test]
    fn a_loaded_object_s_parts_go_into_the_tables() {
        use crate::objects::ListIndex;
        use crate::players::PlayerId;
        let id = object_id(5);
        let mut t = World::new(
            crate::world::WorldTime::ZERO,
            crate::world::FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![crate::world::Millis::ZERO],
            vec![crate::world::player::Player {
                objects: vec![Unit {
                    id,
                    ..Unit::default()
                }]
                .into(),
                ..Default::default()
            }],
        );
        let gather = gather_point(3.5);
        let parts = ObjectParts {
            gather_point: Some(gather),
            ..ObjectParts::default()
        };
        assert!(t.load_parts(object_id(6), parts.clone()).is_err());
        assert!(t.parts.gather_point.is_empty());
        t.load_parts(id, parts).unwrap();
        assert_eq!(t.parts.gather_point.get(id), Some(&gather));
        t.check_parts().unwrap();
        t.set_part(id, Part::GatherPoint(None)).unwrap();
        assert!(t.parts.gather_point.is_empty());
        t.set_part(id, Part::GatherPoint(Some(gather))).unwrap();
        let gone = t
            .swap_remove_live(PlayerId::new(0), ListIndex::FIRST)
            .unwrap();
        assert_eq!(gone.parts.gather_point, Some(gather));
        assert!(t.parts.gather_point.is_empty());
    }

    /// A gather point at `x` on no target.
    fn gather_point(x: f32) -> GatherPoint {
        GatherPoint {
            state: crate::commands::GatherPointState::POINT,
            point: crate::map::Position::new(x, 1.0, 0.0),
            target: None,
            target_master: None,
        }
    }

    /// A whole object compares its row's fields and its parts' at the paths they had on the
    /// object: the dead master is a known gap (left out of the saved scope), the gather point
    /// compares at the building layer's path, a row's field keeps its own path.
    #[test]
    fn a_whole_object_compares_its_parts_at_their_old_paths() {
        use genie_diff::{Scope, differences};
        let a = Object::from(Unit {
            id: object_id(1),
            ..Unit::default()
        });
        let mut b = a.clone();
        b.parts.dead_master = Some(Master::default());
        b.unit.life.hp = crate::objects::HitPoints::new(7.0);
        b.parts.gather_point = Some(gather_point(2.0));
        let paths = |scope| {
            differences(&a, &b, scope)
                .into_iter()
                .map(|d| d.path)
                .collect::<Vec<_>>()
        };
        assert_eq!(paths(Scope::Saved), ["life.hp", "building.gather_point"]);
        assert_eq!(
            paths(Scope::SavedWithGaps),
            ["life.hp", "dead_master", "building.gather_point"]
        );
    }
}
