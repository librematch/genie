//! The world's objects by id as an update reaches them (FD-045-D19, D28): [`OthersRef`] to read
//! them (the owner's live list and the other players' lists), [`Others`] to change them while
//! the unit AI runs, and the world's own lookups for the update itself, which holds the world
//! ([`World::locate`], [`World::wake_sleeper`], [`World::take_out_own`]).
//!
//! The game reaches another object by its id (`FUN_1415417a0`, the table at `world+0x168`); a
//! caller asks for the owner's own object ([`OthersRef::own`]), another player's
//! ([`OthersRef::foreign`]) or anyone's ([`OthersRef::find`]), never for a list position.

use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectKey;
use crate::objects::Wake;
use crate::objects::lines::UnitLines;
use crate::objects::masters::Masters;
use crate::objects::masters::PlayersMasters;
use crate::objects::object_list::ObjectList;
use crate::objects::unit::{TAKEN_OUT, Unit};
use crate::players::PlayerId;
use crate::players::tracker::{Tracker, key};
use crate::unit_ai::world_view::Foreign;
use crate::world::World;
use crate::world::index::ListKind;
use crate::world::index::{ObjectIndex, Place};
use crate::world::layer::Roles;
use crate::world::player::Player;
use crate::{GenieError, GenieResult};
use std::cmp::Ordering;
use std::ptr::eq;
use std::sync::Arc;

/// The objects besides the one that updates, to read: its owner's other live objects in their
/// list's order and the other players' live and sleeping ones.
#[derive(Clone, Copy)]
pub(crate) struct OthersRef<'a> {
    /// The owner, as the update reads it (a captured unit's new one).
    player: PlayerId,
    /// The owner's live list (the updating object's slot holds its stand-in, [`TAKEN_OUT`]).
    own: &'a ObjectList,
    /// The other players' lists.
    foreign: Foreign<'a>,
    /// The world's table by id, which answers the view's lookups.
    index: &'a ObjectIndex,
}

impl<'a> OthersRef<'a> {
    /// Player `player`'s view: its live list `own` and the other players' `foreign`, whose
    /// objects the world's table `index` finds.
    pub(crate) const fn new(
        player: PlayerId,
        own: &'a ObjectList,
        foreign: Foreign<'a>,
        index: &'a ObjectIndex,
    ) -> Self {
        Self {
            player,
            own,
            foreign,
            index,
        }
    }

    /// Player `player`'s view of its live list `own` alone, no other player's.
    #[cfg(test)]
    pub(crate) fn alone(player: PlayerId, own: &'a ObjectList) -> Self {
        let foreign = Foreign {
            before: &[],
            after: &[],
            left: None,
        };
        Self::new(player, own, foreign, ObjectIndex::scanning())
    }

    /// Player `p`'s view of the world's `players`, whose objects the world's table `index` finds.
    ///
    /// # Errors
    ///
    /// When `players` holds no player `p`.
    pub(crate) fn of(
        players: &'a [Player],
        index: &'a ObjectIndex,
        p: PlayerId,
    ) -> GenieResult<Self> {
        let no_player = || {
            GenieError::unreachable("a view of a player the world does not hold").with("player", p)
        };
        let (before, rest) = players.split_at_checked(p.index()).ok_or_else(no_player)?;
        let (player, after) = rest.split_first().ok_or_else(no_player)?;
        let foreign = Foreign {
            before,
            after,
            left: None,
        };
        Ok(Self::new(p, &player.objects, foreign, index))
    }

    /// The view of player `new`, which took the updating object over in its update, with its
    /// live list `list`: this view's owner's list counts among the other players' (`left`).
    pub(crate) const fn changed_hands(self, new: PlayerId, list: &'a ObjectList) -> Self {
        Self {
            player: new,
            own: list,
            foreign: Foreign {
                left: Some((self.player, self.own)),
                ..self.foreign
            },
            index: self.index,
        }
    }

    /// The list among the view's parts that holds `place`, with its player: the slot of the
    /// player whose list is out holds the owner's own live list, or the list `left` names once
    /// the object changed hands; that player's sleeping list is not in the view.
    fn list_at(&self, place: Place) -> Option<(PlayerId, &'a ObjectList)> {
        let Foreign {
            before,
            after,
            left,
        } = self.foreign;
        let (q, list) = (place.player(), place.list());
        let list = match q.index().cmp(&before.len()) {
            Ordering::Less => side(before.get(q.index())?, list),
            Ordering::Greater => side(after.get(q.index() - before.len() - 1)?, list),
            Ordering::Equal if list == ListKind::Sleeping => return None,
            Ordering::Equal => left.map_or(self.own, |(_, l)| l),
        };
        Some((q, list))
    }

    /// Object `id` where the world's table files it, with its player and its list.
    fn held(&self, id: impl ObjectKey) -> Option<(PlayerId, &'a ObjectList, &'a Unit)> {
        let id = id.key()?;
        let place = self.index.filed(id)?;
        let (q, list) = self.list_at(place)?;
        let u = list.get(place.slot()).filter(|u| u.id == id)?;
        Some((q, list, u))
    }

    /// The owner's live object `id`.
    pub(crate) fn own(&self, id: impl ObjectKey) -> Option<&'a Unit> {
        if self.index.scans() {
            return self.own.by_id(id);
        }
        let (_, list, u) = self.held(id)?;
        eq(list, self.own).then_some(u)
    }

    /// The owner's live objects in their list's order, the updating one aside.
    pub(crate) fn own_iter(&self) -> impl Iterator<Item = &'a Unit> + use<'a> {
        self.own.iter().filter(|u| u.id != TAKEN_OUT)
    }

    /// The other players' live and sleeping objects with their players: the players in their
    /// order, each one's live list before its sleeping one, then the list `left` names.
    pub(crate) fn foreign_iter(&self) -> impl Iterator<Item = (PlayerId, &'a Unit)> + use<'a> {
        let Foreign {
            before,
            after,
            left,
        } = self.foreign;
        let skip = before.len() + 1;
        let before = numbered(before, 0);
        let after = numbered(after, skip);
        let lists = before
            .chain(after)
            .flat_map(|(q, p)| p.objects.iter().chain(&p.sleeping).map(move |u| (q, u)));
        let left = left.into_iter().flat_map(|(q, list)| {
            let list = list.iter().filter(|u| u.id != TAKEN_OUT);
            list.map(move |u| (q, u))
        });
        lists.chain(left)
    }

    /// Another player's live or sleeping object `id`, with that player
    /// ([`OthersRef::foreign_iter`]'s first).
    pub(crate) fn foreign(&self, id: impl ObjectKey) -> Option<(PlayerId, &'a Unit)> {
        if !self.index.scans() {
            let (q, _, u) = self.held(id)?;
            // the owner's slot is another player's only once the object changed hands
            let own = q.index() == self.foreign.before.len() && self.foreign.left.is_none();
            return (!own).then_some((q, u));
        }
        let Foreign {
            before,
            after,
            left,
        } = self.foreign;
        let skip = before.len() + 1;
        let before = numbered(before, 0);
        let after = numbered(after, skip);
        let mut players = before.chain(after);
        players
            .find_map(|(q, p)| {
                p.objects
                    .by_id(id)
                    .or_else(|| p.sleeping.by_id(id))
                    .map(|u| (q, u))
            })
            .or_else(|| left.and_then(|(q, list)| list.by_id(id).map(|u| (q, u))))
    }

    /// Every object the view holds: the owner's, then the other players'.
    pub(crate) fn every(&self) -> impl Iterator<Item = &'a Unit> + use<'a> {
        self.own_iter().chain(self.foreign_iter().map(|(_, u)| u))
    }

    /// Object `id` with its owner: the owner's own, else another player's.
    pub(crate) fn find(&self, id: impl ObjectKey) -> Option<(PlayerId, &'a Unit)> {
        self.own(id)
            .map(|u| (self.player, u))
            .or_else(|| self.foreign(id))
    }

    /// Object `id`, the owner's own or another player's.
    pub(crate) fn get(&self, id: impl ObjectKey) -> Option<&'a Unit> {
        self.find(id).map(|(_, u)| u)
    }

    /// The other players, whose data (diplomacy, attributes) the view's readers read.
    pub(crate) const fn players(&self) -> Foreign<'a> {
        self.foreign
    }
}

/// Player `player`'s list of `kind`.
const fn side(player: &Player, kind: ListKind) -> &ObjectList {
    match kind {
        ListKind::Sleeping => &player.sleeping,
        ListKind::Live => &player.objects,
    }
}

/// Players `players` in order, numbered from `first`: the slice's share of the world's players.
fn numbered(players: &[Player], first: usize) -> impl Iterator<Item = (PlayerId, &Player)> + Clone {
    PlayerId::each(first + players.len())
        .skip(first)
        .zip(players)
}

/// [`numbered`], to change.
fn numbered_mut(
    players: &mut [Player],
    first: usize,
) -> impl Iterator<Item = (PlayerId, &mut Player)> {
    PlayerId::each(first + players.len())
        .skip(first)
        .zip(players)
}

/// The objects besides the one that updates, to change: what [`OthersRef`] reads, with the
/// owner's other live objects and the other players to write.
pub(crate) struct Others<'a> {
    player: PlayerId,
    own: &'a mut ObjectList,
    /// The owner's sleeping list, which only a write by id reaches.
    own_sleeping: &'a mut ObjectList,
    /// The players before the owner and after it.
    before: &'a mut [Player],
    after: &'a mut [Player],
    /// The world's table by id, which answers the view's lookups and files its wakes.
    index: &'a mut ObjectIndex,
}

impl<'a> Others<'a> {
    /// The view of player `player`'s objects as one of them updates: the player's live list
    /// `own`, its sleeping list `own_sleeping` and the players `before` and `after` it, whose
    /// objects the world's table `index` finds.
    pub(crate) const fn new(
        player: PlayerId,
        (own, own_sleeping): (&'a mut ObjectList, &'a mut ObjectList),
        (before, after): (&'a mut [Player], &'a mut [Player]),
        index: &'a mut ObjectIndex,
    ) -> Self {
        Self {
            player,
            own,
            own_sleeping,
            before,
            after,
            index,
        }
    }

    /// The view to read.
    pub(crate) const fn view(&self) -> OthersRef<'_> {
        let foreign = Foreign {
            before: self.before,
            after: self.after,
            left: None,
        };
        OthersRef::new(self.player, self.own, foreign, self.index)
    }

    /// The live and sleeping lists of player `q` among the view's parts.
    fn lists_mut(&mut self, q: PlayerId) -> Option<(&mut ObjectList, &mut ObjectList)> {
        let own = self.before.len();
        let player = match q.index().cmp(&own) {
            Ordering::Less => self.before.get_mut(q.index())?,
            Ordering::Greater => self.after.get_mut(q.index() - own - 1)?,
            Ordering::Equal => return Some((&mut *self.own, &mut *self.own_sleeping)),
        };
        let Player {
            objects, sleeping, ..
        } = player;
        Some((objects, sleeping))
    }

    /// The live and sleeping lists of player `q` among the view's parts, to read.
    fn lists(&self, q: PlayerId) -> Option<(&ObjectList, &ObjectList)> {
        let own = self.before.len();
        let player = match q.index().cmp(&own) {
            Ordering::Less => self.before.get(q.index())?,
            Ordering::Greater => self.after.get(q.index() - own - 1)?,
            Ordering::Equal => return Some((&*self.own, &*self.own_sleeping)),
        };
        Some((&player.objects, &player.sleeping))
    }

    /// Object `id` of any list, live or sleeping, with its owner, to change: what the world's
    /// table by id (`FUN_1415417a0`) gives the game's call on it.
    pub(crate) fn get_mut(&mut self, id: impl ObjectKey) -> Option<(PlayerId, &mut Unit)> {
        let id = id.key()?;
        if !self.index.scans() {
            let place = self.index.filed(id)?;
            let (live, sleeping) = self.lists_mut(place.player())?;
            let list = if place.sleeping() { sleeping } else { live };
            let u = list.get_mut(place.slot()).filter(|u| u.id == id)?;
            return Some((place.player(), u));
        }
        let p = self.player;
        if let Some(u) = self.own.by_id_mut(id) {
            return Some((p, u));
        }
        if let Some(u) = self.own_sleeping.by_id_mut(id) {
            return Some((p, u));
        }
        let skip = self.before.len() + 1;
        let before = numbered_mut(self.before, 0);
        let after = numbered_mut(self.after, skip);
        before.chain(after).find_map(|(q, pl)| {
            let Player {
                objects, sleeping, ..
            } = pl;
            objects
                .by_id_mut(id)
                .or_else(|| sleeping.by_id_mut(id))
                .map(|u| (q, u))
        })
    }

    /// Object `id` of any list with its owner and whether it sleeps: what the world's table by
    /// id (`FUN_1415417a0`) gives, the owner's lists first.
    pub(crate) fn locate(&self, id: impl ObjectKey) -> Option<(PlayerId, &Unit, bool)> {
        let id = id.key()?;
        if !self.index.scans() {
            let place = self.index.filed(id)?;
            let (live, sleeping) = self.lists(place.player())?;
            let list = if place.sleeping() { sleeping } else { live };
            let u = list.get(place.slot()).filter(|u| u.id == id)?;
            return Some((place.player(), u, place.sleeping()));
        }
        let p = self.player;
        if let Some(u) = self.own.by_id(id) {
            return Some((p, u, false));
        }
        if let Some(u) = self.own_sleeping.by_id(id) {
            return Some((p, u, true));
        }
        let skip = self.before.len() + 1;
        let before = numbered(self.before, 0);
        let after = numbered(self.after, skip);
        before.chain(after).find_map(|(q, pl)| {
            let live = pl.objects.by_id(id).map(|u| (q, u, false));
            live.or_else(|| pl.sleeping.by_id(id).map(|u| (q, u, true)))
        })
    }

    /// The sleeping object `id` wakes (`FUN_14119deb0` with 0, [`World::wake`]): it leaves its
    /// player's sleeping list for the end of the live one, no longer flagged to sleep, and the
    /// remove and add (slots 0x2d8, 0x2d0) file it last in its player's tracker (`own_tracker`
    /// the owner's; `masters` and `lines` give the key). [`Wake::NotAsleep`] when no sleeping list
    /// holds it.
    ///
    /// [`World::wake`]: crate::world::World::wake
    pub(crate) fn wake(
        &mut self,
        id: ObjectId,
        own_tracker: &mut Tracker,
        (masters, lines): (&[Arc<Masters>], &UnitLines),
        roles: &Roles,
    ) -> Wake {
        let Self {
            player,
            own,
            own_sleeping,
            before,
            after,
            index,
        } = self;
        let place = if index.scans() {
            // the owner's sleeping list first, then the other players' in their order
            let skip = before.len() + 1;
            own_sleeping
                .position_of(id)
                .map(|i| Place::Sleeping(*player, i))
                .or_else(|| {
                    numbered(before, 0)
                        .chain(numbered(after, skip))
                        .find_map(|(q, pl)| {
                            pl.sleeping.position_of(id).map(|i| Place::Sleeping(q, i))
                        })
                })
        } else {
            index.filed(id).filter(|place| place.sleeping())
        };
        let Some(place) = place else {
            return Wake::NotAsleep;
        };
        let q = place.player();
        let (live, sleeping, tracker) = match q.index().cmp(&before.len()) {
            Ordering::Equal => (&mut **own, &mut **own_sleeping, own_tracker),
            Ordering::Less => {
                let Some(pl) = before.get_mut(q.index()) else {
                    return Wake::NotAsleep;
                };
                (&mut pl.objects, &mut pl.sleeping, &mut pl.tracker)
            }
            Ordering::Greater => {
                let Some(pl) = after.get_mut(q.index() - before.len() - 1) else {
                    return Wake::NotAsleep;
                };
                (&mut pl.objects, &mut pl.sleeping, &mut pl.tracker)
            }
        };
        if sleeping.get(place.slot()).is_none_or(|u| u.id != id) {
            return Wake::NotAsleep;
        }
        let Ok(mut woken) = index.swap_remove(sleeping, place) else {
            return Wake::NotAsleep;
        };
        woken.life.goto_sleep = false;
        let k = key(masters.of(q), lines, woken.life.master_id, roles);
        let _ = index.push(live, Place::Live(q, ListIndex::new(0)), woken);
        tracker.remove(id, k);
        tracker.add(id, k);
        Wake::Woke
    }
}

impl World {
    /// Object `id` of any list with its owner and whether it sleeps: what the world's table by
    /// id (`FUN_1415417a0`) gives.
    pub(crate) fn locate(&self, id: impl ObjectKey) -> Option<(PlayerId, &Unit, bool)> {
        self.index.locate(&self.players, id)
    }

    /// The sleeping object `id` wakes (`FUN_14119deb0` with 0): it leaves its player's sleeping
    /// list for the end of the live one, no longer flagged to sleep, and the remove and add
    /// (slots 0x2d8, 0x2d0) file it last in its player's tracker. [`Wake::NotAsleep`] when no
    /// sleeping list holds it.
    pub(crate) fn wake_sleeper(&mut self, id: ObjectId) -> GenieResult<Wake> {
        let Some(Place::Sleeping(p, i)) = self.place(id) else {
            return Ok(Wake::NotAsleep);
        };
        let mut woken = self.swap_remove_asleep(p, i)?;
        woken.unit.life.goto_sleep = false;
        let master = woken.unit.life.master_id;
        let _ = self.push_live(p, woken)?;
        self.track_relist(p, id, master)?;
        Ok(Wake::Woke)
    }

    /// Player `p`'s live object `id` out of its slot (its stand-in, [`TAKEN_OUT`], holds it),
    /// for a call on it that reaches the world as the updating object's do; it goes back with
    /// [`World::put_back_own`].
    ///
    /// # Errors
    ///
    /// A player the world does not hold.
    pub(crate) fn take_out_own(&mut self, p: PlayerId, id: ObjectId) -> GenieResult<Option<Unit>> {
        self.live_index(p, id)
            .map(|i| self.take_out(p, i))
            .transpose()
    }

    /// `unit`, out of player `p`'s live list, goes back to the slot its stand-in holds now (the
    /// list's removals move it as the game's do, [`ObjectList::put_back`]): that slot.
    ///
    /// # Errors
    ///
    /// Its stand-in left the list: the unit goes to the list's end; a stand-in itself (its
    /// object lost in a lend, [`crate::world::step::Step::lend`]) goes nowhere.
    pub(crate) fn put_back_own(&mut self, p: PlayerId, unit: Unit) -> GenieResult<ListIndex> {
        if unit.id == TAKEN_OUT {
            return Err(GenieError::unreachable(
                "a stand-in put back for its object",
            ));
        }
        match self.put_back(p, unit) {
            Ok(i) => Ok(i),
            // a removal moves a stand-in with the slot it holds (`ObjectList::outs`), and none
            // removes it
            Err(unit) => {
                let id = unit.id;
                let _ = self.push_live(p, *unit)?;
                Err(
                    GenieError::unreachable("an object's stand-in left its list")
                        .with("object", id),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::HitPoints;
    use crate::players::tracker::ListKey;
    use crate::test_ids::object_id;
    use crate::testing::ids;
    use crate::testing::roles;

    fn unit(id: i32) -> Unit {
        Unit {
            id: object_id(id),
            ..Unit::default()
        }
    }

    fn player(live: &[i32], sleeping: &[i32]) -> Player {
        Player {
            objects: live.iter().map(|&id| unit(id)).collect(),
            sleeping: sleeping.iter().map(|&id| unit(id)).collect(),
            ..Player::default()
        }
    }

    /// Player 1 updates object 11 (taken out); player 0 holds 1 live and 2 asleep, player 2
    /// holds 20 and, again, 12.
    fn world() -> (ObjectList, [Player; 1], [Player; 1]) {
        let mut own = ObjectList::from(vec![unit(10), Unit::taken_out(), unit(12)]);
        own[ListIndex::new(2)].life.hp = HitPoints::new(5.0);
        let mut theirs = player(&[20, 12], &[]);
        theirs.objects[ListIndex::new(1)].life.hp = HitPoints::new(7.0);
        (own, [player(&[1], &[2])], [theirs])
    }

    #[test]
    fn lookup_finds_the_owner_object_before_another_player_s() {
        let (own, before, after) = world();
        let foreign = Foreign {
            before: &before,
            after: &after,
            left: None,
        };
        let v = OthersRef::new(PlayerId::new(1), &own, foreign, ObjectIndex::scanning());
        assert_eq!(v.own(object_id(10)).map(|u| u.id.get()), Some(10));
        assert!(
            v.own(object_id(1)).is_none(),
            "another player's is not the owner's"
        );
        assert_eq!(
            v.find(object_id(12)).map(|(q, u)| (q, u.life.hp.get())),
            Some((PlayerId::new(1), 5.0))
        );
        assert_eq!(
            v.foreign(object_id(12)).map(|(q, u)| (q, u.life.hp.get())),
            Some((PlayerId::new(2), 7.0))
        );
        assert_eq!(
            v.find(object_id(2)).map(|(q, u)| (q, u.id.get())),
            Some((PlayerId::new(0), 2))
        );
        assert_eq!(
            v.find(object_id(20)).map(|(q, _)| q),
            Some(PlayerId::new(2))
        );
        assert_eq!(v.get(object_id(1)).map(|u| u.id.get()), Some(1));
        assert_eq!(
            v.players()
                .player(PlayerId::new(2))
                .map(|p| p.objects.len()),
            Some(2)
        );
        assert!(
            v.players().player(PlayerId::new(1)).is_none(),
            "the owner's data is the update's own"
        );
        let foreign: Vec<(PlayerId, i32)> =
            v.foreign_iter().map(|(q, u)| (q, u.id.get())).collect();
        assert_eq!(
            foreign,
            [
                (PlayerId::new(0), 1),
                (PlayerId::new(0), 2),
                (PlayerId::new(2), 20),
                (PlayerId::new(2), 12)
            ]
        );
        let every: Vec<ObjectId> = v.every().map(|u| u.id).collect();
        assert_eq!(
            every,
            [
                object_id(10),
                object_id(12),
                object_id(1),
                object_id(2),
                object_id(20),
                object_id(12)
            ]
        );
        assert!(
            v.get(object_id(11)).is_none(),
            "the updating object is out of its list"
        );
        assert!(v.find(TAKEN_OUT).is_none(), "nor is its stand-in");
    }

    #[test]
    fn the_owner_objects_iterate_in_list_order_without_the_one_that_updates() {
        let (mut own, mut before, mut after) = world();
        let mut asleep = ObjectList::from(vec![unit(13)]);
        let mut index = ObjectIndex::scanning().clone();
        let mut others = Others::new(
            PlayerId::new(1),
            (&mut own, &mut asleep),
            (&mut before, &mut after),
            &mut index,
        );
        let ids: Vec<ObjectId> = others.view().own_iter().map(|u| u.id).collect();
        assert_eq!(ids, [object_id(10), object_id(12)]);
        for (id, owner) in [(10, 1), (12, 1), (13, 1), (2, 0), (20, 2)] {
            let (q, u) = others.get_mut(object_id(id)).expect("any list's object");
            u.life.hp = HitPoints::new(4.0);
            assert_eq!(q, PlayerId::from(owner));
        }
        assert!(
            others.get_mut(object_id(11)).is_none(),
            "the updating object is out"
        );
        assert!(
            others.view().get(object_id(13)).is_none(),
            "a read leaves the owner's sleepers out"
        );
        assert_eq!(
            others.view().own(object_id(10)).map(|u| u.life.hp),
            Some(HitPoints::new(4.0))
        );
        assert_eq!(
            others.view().find(object_id(2)).map(|(_, u)| u.life.hp),
            Some(HitPoints::new(4.0))
        );
        assert_eq!(
            others.view().find(object_id(12)).map(|(_, u)| u.life.hp),
            Some(HitPoints::new(4.0))
        );
        assert_eq!(
            others.view().find(object_id(20)).map(|(q, _)| q),
            Some(PlayerId::new(2))
        );
        assert_eq!(others.view().foreign(object_id(10)).map(|(q, _)| q), None);
        assert_eq!(
            own[ListIndex::new(1)].id,
            TAKEN_OUT,
            "the stand-in stays as it was"
        );
    }

    #[test]
    fn lookup_by_id_reaches_every_list_and_says_whether_the_object_sleeps() {
        let (mut own, mut before, mut after) = world();
        let mut asleep = ObjectList::from(vec![unit(13)]);
        let mut index = ObjectIndex::scanning().clone();
        let others = Others::new(
            PlayerId::new(1),
            (&mut own, &mut asleep),
            (&mut before, &mut after),
            &mut index,
        );
        let at = |id| {
            others
                .locate(object_id(id))
                .map(|(q, u, sleeps)| (q, u.id.get(), sleeps))
        };
        assert_eq!(
            at(12),
            Some((PlayerId::new(1), 12, false)),
            "the owner's before another's"
        );
        assert_eq!(at(13), Some((PlayerId::new(1), 13, true)));
        assert_eq!(at(2), Some((PlayerId::new(0), 2, true)));
        assert_eq!(at(20), Some((PlayerId::new(2), 20, false)));
        assert_eq!(at(11), None, "the updating object is out");
        assert_eq!(at(TAKEN_OUT.get()), None);
    }

    /// A woken object (`FUN_14119deb0` with 0) leaves its sleeping list for the end of its
    /// player's live list, unflagged, and its player's tracker files it last.
    #[test]
    fn woken_object_moves_to_the_end_of_its_player_live_list_and_tracker() {
        use crate::players::tracker::{Tracker, key};
        let (mut own, mut before, mut after) = world();
        let mut asleep = ObjectList::from(vec![unit(13)]);
        asleep[ListIndex::new(0)].life.goto_sleep = true;
        let lines = UnitLines::default();
        let k = key(None, &lines, ids::ID_0, &roles());
        let mut tracker = Tracker::default();
        for id in [13, 10] {
            tracker.add(object_id(id), k);
        }
        for id in [2, 1] {
            before[0].tracker.add(object_id(id), k);
        }
        let mut index = ObjectIndex::scanning().clone();
        let mut others = Others::new(
            PlayerId::new(1),
            (&mut own, &mut asleep),
            (&mut before, &mut after),
            &mut index,
        );
        assert_eq!(
            others.wake(object_id(13), &mut tracker, (&[], &lines), &roles()),
            Wake::Woke
        );
        assert_eq!(
            others.wake(object_id(2), &mut tracker, (&[], &lines), &roles()),
            Wake::Woke
        );
        assert_eq!(
            others.wake(object_id(20), &mut tracker, (&[], &lines), &roles()),
            Wake::NotAsleep,
            "20 is awake"
        );
        assert_eq!(
            others.locate(object_id(13)).map(|(q, _, s)| (q, s)),
            Some((PlayerId::new(1), false))
        );
        assert_eq!(
            others.view().own(object_id(13)).map(|u| u.life.goto_sleep),
            Some(false)
        );
        let ids = |l: &ObjectList| l.iter().map(|u| u.id.get()).collect::<Vec<_>>();
        assert_eq!(
            (ids(&own), ids(&asleep)),
            (vec![10, TAKEN_OUT.get(), 12, 13], vec![])
        );
        assert_eq!(
            (ids(&before[0].objects), ids(&before[0].sleeping)),
            (vec![1, 2], vec![])
        );
        assert_eq!(
            (
                tracker.list(ListKey::of_master(k.master)),
                before[0].tracker.list(ListKey::of_master(k.master))
            ),
            (
                &[Some(object_id(10)), Some(object_id(13))][..],
                &[Some(object_id(1)), Some(object_id(2))][..]
            )
        );
    }

    #[test]
    fn captured_object_reads_its_new_owner_list_as_its_own() {
        let (own, before, after) = world();
        let foreign = Foreign {
            before: &before,
            after: &after,
            left: None,
        };
        let v = OthersRef::new(PlayerId::new(1), &own, foreign, ObjectIndex::scanning())
            .changed_hands(PlayerId::new(2), &after[0].objects);
        assert_eq!(v.own(object_id(20)).map(|u| u.id.get()), Some(20));
        assert_eq!(
            v.find(object_id(12)).map(|(q, u)| (q, u.life.hp.get())),
            Some((PlayerId::new(2), 7.0))
        );
        assert_eq!(
            v.find(object_id(10)).map(|(q, _)| q),
            Some(PlayerId::new(1)),
            "the old owner's list is left"
        );
        assert_eq!(v.own_iter().count(), 2);
        assert!(
            v.find(TAKEN_OUT).is_none(),
            "the left list's stand-in is no object"
        );
    }
}
