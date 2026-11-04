//! A unit AI's attackers: the sorted set of the objects whose damage it took.

use crate::objects::ObjectId;

/// The attackers (`+0xc0`, count `+0xc8`): a set of object ids kept sorted (`FUN_140b43f30`),
/// which a damage notify adds to and the full update prunes; the save's `numberUnits` and
/// `tempInt`. A damage notify of a missile whose shooter is gone names none (its sender −1), which
/// joins the set as the game's −1 does until the update prunes it (the world holds no object −1).
/// The game sorts and searches the integers, so the set orders by [`ObjectId::field`]: none sorts
/// above the recyclable objects' negative ids.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Attackers(Vec<Option<ObjectId>>);

impl Attackers {
    /// The saved set (`tempInt`), in the save's order, each −1 none.
    #[must_use]
    pub fn of_saved(ids: &[i32]) -> Self {
        Self(ids.iter().map(|&id| ObjectId::of_field(id)).collect())
    }

    /// `FUN_140b43f30`'s binary search for `id` over the integers.
    fn search(&self, id: Option<ObjectId>) -> Result<usize, usize> {
        self.0
            .binary_search_by_key(&ObjectId::field(id), |&object_id| {
                ObjectId::field(object_id)
            })
    }

    /// `id` is in the set.
    #[must_use]
    pub fn contains(&self, id: Option<ObjectId>) -> bool {
        self.search(id).is_ok()
    }

    /// `FUN_140b43f30`: an id not in the set goes into its place (the array grows by `capacity *
    /// 103 / 64 + 1`, Vec's own growth here) and the call answers true, one already in it
    /// answers false.
    pub fn add(&mut self, id: Option<ObjectId>) -> bool {
        match self.search(id) {
            Ok(_) => false,
            Err(i) => {
                self.0.insert(i, id);
                true
            }
        }
    }

    /// `id` leaves the set (`FUN_140bf4cb0`); one not in it changes nothing.
    pub fn remove(&mut self, id: Option<ObjectId>) {
        if let Ok(i) = self.search(id) {
            let _ = self.0.remove(i);
        }
    }

    /// The first attacker in the set's order that is `gone` leaves it.
    pub fn remove_first(&mut self, mut gone: impl FnMut(Option<ObjectId>) -> bool) {
        if let Some(i) = self.0.iter().position(|&id| gone(id)) {
            let _ = self.0.remove(i);
        }
    }

    /// The attackers in the set's order.
    pub fn iter(&self) -> impl Iterator<Item = Option<ObjectId>> + '_ {
        self.0.iter().copied()
    }

    /// The set holds no attacker.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl genie_diff::Diff for Attackers {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

#[cfg(test)]
mod tests {
    use crate::test_ids::object_id;
    use crate::unit_ai::Attackers;

    /// The set sorts by the game's integers: none (−1) sorts above a negative id and below the
    /// others; an id joins once and leaves by its value.
    #[test]
    fn attackers_sort_by_the_game_integers() {
        let mut attackers = Attackers::default();
        assert!(attackers.add(Some(object_id(7))));
        assert!(attackers.add(None));
        assert!(attackers.add(Some(object_id(-2))));
        assert!(!attackers.add(Some(object_id(7))));
        assert_eq!(
            attackers.iter().collect::<Vec<_>>(),
            [Some(object_id(-2)), None, Some(object_id(7))]
        );
        assert!(attackers.contains(None));
        attackers.remove(None);
        assert!(!attackers.contains(None));
        attackers.remove_first(|id| id.is_some_and(|id| id.get() > 0));
        assert_eq!(attackers.iter().collect::<Vec<_>>(), [Some(object_id(-2))]);
        assert_eq!(
            Attackers::of_saved(&[-1, 4]).iter().collect::<Vec<_>>(),
            [None, Some(object_id(4))]
        );
    }
}
