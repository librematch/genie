//! The motion part's reads and writes (FD-045-D58): an object's animated layer and its mover, by
//! id in the world's table ([`Parts::motion`]), beside the row's place.

use crate::movement::MoveStats;
use crate::movement::Mover;
use crate::movement::Speed;
use crate::objects::IdTable;
use crate::objects::ObjectId;
use crate::objects::layers::{Motion, MotionMut};
use crate::world::parts::{PartTable, Parts};

/// A part table read with at most one entry lent out to change ([`PartTable::lend`]): the
/// others read as the table's, the lent one as none (its object is out for its update, as its
/// row is out of its list).
#[derive(Debug)]
pub struct Lent<'a, T> {
    /// The parts before the lent one's slot (all of them where none is lent).
    before: &'a [T],
    /// The parts after the lent one's slot.
    after: &'a [T],
    /// The slot of the part lent out, none where none is.
    out: Option<usize>,
    /// Each object's slot.
    slots: &'a IdTable<usize>,
}

impl<T> Clone for Lent<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Lent<'_, T> {}

impl<'a, T> Lent<'a, T> {
    /// Object `id`'s part, none for an object without one or the one lent out.
    #[must_use]
    pub fn get(&self, id: ObjectId) -> Option<&'a T> {
        let slot = *self.slots.get(id)?;
        match self.out {
            None => self.before.get(slot),
            Some(out) if slot < out => self.before.get(slot),
            Some(out) if slot == out => None,
            Some(out) => self.after.get(slot - out - 1),
        }
    }
}

impl<T> PartTable<T> {
    /// The table to read, none of its parts lent out.
    #[must_use]
    pub fn unlent(&self) -> Lent<'_, T> {
        Lent {
            before: self.parts(),
            after: &[],
            out: None,
            slots: self.slots(),
        }
    }

    /// Object `id`'s part lent out to change (none for an object without one) and the table's
    /// others to read beside it.
    pub(crate) fn lend(&mut self, id: ObjectId) -> (Option<&mut T>, Lent<'_, T>) {
        let (parts, slots) = self.parts_and_slots_mut();
        let Some(&slot) = slots.get(id) else {
            return (
                None,
                Lent {
                    before: parts,
                    after: &[],
                    out: None,
                    slots,
                },
            );
        };
        let (before, rest) = parts.split_at_mut(slot);
        match rest.split_first_mut() {
            Some((one, after)) => (
                Some(one),
                Lent {
                    before,
                    after,
                    out: Some(slot),
                    slots,
                },
            ),
            None => (
                None,
                Lent {
                    before,
                    after: &[],
                    out: None,
                    slots,
                },
            ),
        }
    }
}

impl PartTable<Motion> {
    /// Object `id`'s mover, none for an object that does not move (or that the table does not
    /// hold: a static object).
    #[must_use]
    pub fn mover(&self, id: ObjectId) -> Option<&Mover> {
        self.get(id).and_then(Motion::mover)
    }

    /// Object `id`'s mover to change, none for an object that does not move.
    pub(crate) fn mover_mut(&mut self, id: ObjectId) -> Option<&mut Mover> {
        self.get_mut(id).and_then(Motion::mover_mut)
    }

    /// Object `id`'s motion layer to change ([`MotionMut`], none for a static object), beside
    /// borrows of the world's other part tables.
    pub(crate) fn layer_mut(&mut self, id: ObjectId) -> MotionMut<'_> {
        MotionMut::new(self.get_mut(id))
    }

    /// Object `id`'s movement values to change, none for an object that does not move.
    pub(crate) fn move_stats_mut(&mut self, id: ObjectId) -> Option<&mut MoveStats> {
        self.mover_mut(id).map(|m| &mut m.stats)
    }
}

impl Parts {
    /// Object `id`'s mover, none for an object that does not move (or that the table does not
    /// hold: a static object).
    #[must_use]
    pub fn mover_of(&self, id: ObjectId) -> Option<&Mover> {
        self.motion.mover(id)
    }

    /// Object `id`'s mover to change, none for an object that does not move.
    pub(crate) fn mover_mut(&mut self, id: ObjectId) -> Option<&mut Mover> {
        self.motion.mover_mut(id)
    }

    /// Object `id`'s motion layer to change beside its row ([`MotionMut`]), none for a static
    /// object.
    pub(crate) fn motion_mut(&mut self, id: ObjectId) -> MotionMut<'_> {
        self.motion.layer_mut(id)
    }

    /// Object `id`'s building layer to change (put in where the table holds none,
    /// [`Parts::building_mut`]) and its mover beside it (none for an object that does not move).
    ///
    /// # Errors
    ///
    /// None (an unreachable error where the table lost the layer it took).
    pub(crate) fn building_and_mover_mut(
        &mut self,
        id: ObjectId,
    ) -> Result<(&mut crate::objects::layers::Building, Option<&mut Mover>), crate::GenieError>
    {
        let building = self
            .building
            .get_or_insert_with(id, || crate::objects::layers::Building::NONE)?;
        Ok((building, self.motion.mover_mut(id)))
    }

    /// Object `id`'s combat layer to change (a new object's put in where the table holds none,
    /// [`Parts::combat_mut`]) and its motion layer beside it ([`MotionMut`]).
    ///
    /// # Errors
    ///
    /// None (an unreachable error where the table lost the layer it took).
    pub(crate) fn combat_and_motion_mut(
        &mut self,
        id: ObjectId,
    ) -> Result<(&mut crate::objects::layers::Combat, MotionMut<'_>), crate::GenieError> {
        let combat = self
            .combat
            .get_or_insert_with(id, crate::objects::layers::Combat::default)?;
        Ok((combat, self.motion.layer_mut(id)))
    }

    /// Object `id`'s motion layer lent out of the table to change ([`MotionMut`], none for a
    /// static object) and the world's objects by id beside it (`players`, `index` and
    /// `masters`, the other parts' tables), which read the others' motion.
    pub(crate) fn lend_motion<'a>(
        &'a mut self,
        id: ObjectId,
        (players, index, masters): (
            &'a [crate::world::Player],
            &'a crate::world::index::ObjectIndex,
            &'a [crate::objects::masters::Masters],
        ),
    ) -> (MotionMut<'a>, crate::world::index::ObjectsRef<'a>) {
        let (motion, motions) = self.motion.lend(id);
        (
            MotionMut::new(motion),
            crate::world::index::ObjectsRef::of_tables(
                players,
                index,
                crate::objects::masters::MasterTables::new(masters, &self.own_master),
                (
                    &self.building,
                    &self.combat,
                    &self.actions,
                    &self.unread,
                    motions,
                ),
            ),
        )
    }

    /// Object `id`'s mover lent out of the motion table to change (none for an object that does
    /// not move) and the world's objects by id beside it ([`Parts::lend_motion`]): its walk
    /// reads the others' gaits while it moves.
    pub(crate) fn lend_mover<'a>(
        &'a mut self,
        id: ObjectId,
        tables: (
            &'a [crate::world::Player],
            &'a crate::world::index::ObjectIndex,
            &'a [crate::objects::masters::Masters],
        ),
    ) -> (Option<&'a mut Mover>, crate::world::index::ObjectsRef<'a>) {
        let (motion, by_id) = self.lend_motion(id, tables);
        (motion.into_mover(), by_id)
    }

    /// Object `id`'s movement values to change (its top speed among them), none for an object
    /// that does not move.
    pub(crate) fn move_stats_mut(&mut self, id: ObjectId) -> Option<&mut MoveStats> {
        self.motion.move_stats_mut(id)
    }

    /// Object `id`'s animation speed (`+0x7b8`), none for a static object.
    #[must_use]
    pub fn anim_speed_of(&self, id: ObjectId) -> Option<Speed> {
        self.motion.get(id).map(|m| m.animated().speed)
    }

    /// Object `id`'s animation speed (`+0x7b8`) becomes `speed`; a static object has none.
    pub(crate) fn set_anim_speed(&mut self, id: ObjectId, speed: Speed) {
        if let Some(m) = self.motion.get_mut(id) {
            m.animated_mut().speed = speed;
        }
    }

    /// Object `id`'s mover goes and its animated layer stays (a caller that works on the mover
    /// apart from its object puts it back with [`Parts::put_mover`]); none for an object that
    /// does not move.
    pub(crate) fn take_mover(&mut self, id: ObjectId) -> Option<Mover> {
        let motion = self.motion.get_mut(id)?;
        let animated = *motion.animated();
        match std::mem::replace(motion, Motion::Animated(animated)) {
            Motion::Moving(m) => Some(m),
            animated @ Motion::Animated(_) => {
                *motion = animated;
                None
            }
        }
    }

    /// A mover taken with [`Parts::take_mover`] comes back to object `id`, with its animated
    /// layer as it is now (an object without one takes the mover's own); none leaves the
    /// animated layer alone. The caller holds the object: its row is borrowed from the world's
    /// lists, which this borrow leaves alone.
    pub(crate) fn put_mover(&mut self, id: ObjectId, mover: Option<Mover>) {
        let animated = self.motion.get(id).map(|m| *m.animated());
        match Motion::of(animated, mover) {
            Some(motion) => {
                let _ = self.motion.insert(id, motion);
            }
            None => {
                let _ = self.motion.remove(id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::movement::Speed;
    use crate::objects::layers::{Animated, Motion};
    use crate::test_ids::object_id;
    use crate::world::parts::Parts;

    #[test]
    fn taken_mover_leaves_the_animated_layer_and_comes_back_with_it() {
        let id = object_id(7);
        let mut parts = Parts::default();
        let mut walker = crate::movement::tests::builder_9177();
        walker.animated = Animated::new(Speed::new(1.0));
        let _ = parts.motion.insert(id, Motion::Moving(walker));
        let taken = parts.take_mover(id);
        assert!(taken.is_some());
        assert_eq!(
            parts.motion.get(id),
            Some(&Motion::Animated(Animated::new(Speed::new(1.0))))
        );
        parts.set_anim_speed(id, Speed::ZERO);
        parts.put_mover(id, taken);
        assert_eq!(
            parts.mover_of(id).map(|m| m.animated.speed),
            Some(Speed::ZERO)
        );
        let still = object_id(8);
        assert_eq!(parts.take_mover(still), None);
        parts.put_mover(still, None);
        assert_eq!(parts.motion.get(still), None, "a static object has none");
    }

    #[test]
    fn only_an_object_of_the_table_has_an_animation_speed() {
        let mut parts = Parts::default();
        parts.set_anim_speed(object_id(1), Speed::new(1.0));
        assert_eq!(parts.anim_speed_of(object_id(1)), None);
        let _ = parts
            .motion
            .insert(object_id(2), Motion::Animated(Animated::default()));
        parts.set_anim_speed(object_id(2), Speed::new(1.0));
        assert_eq!(parts.anim_speed_of(object_id(2)), Some(Speed::new(1.0)));
        assert_eq!(parts.mover_of(object_id(2)), None);
    }
}
