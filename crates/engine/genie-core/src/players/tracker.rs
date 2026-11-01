//! The player's object tracker (`player+0x18`, the "Object Tracker"): lists of the player's
//! object ids by key, in the order the ids came in. A key is a master (the one its key master
//! stands for, [`key`]) or a class + 900; a building waits under its key + 90000 (its class +
//! 90900) until its completion moves it. The tracker is not saved: the load files every object
//! of the player's lists in order (live, sleeping, then the ghosts), each building completed
//! (`build_pts` at least its build time) at the end of its own load.
//!
//! What files and moves ids, and where genie-core runs it:
//! - the player's add (slot 0x2d0, `FUN_14165cc80` -> `FUN_14104e7f0`, [`Tracker::add`]): a new
//!   object, a woken one, one going to sleep (after its removal), the new owner of one changing
//!   hands, and the load;
//! - the player's remove (slot 0x2d8, `FUN_141668040` -> `FUN_14104fff0`, [`Tracker::remove`]):
//!   an object leaving the world, sleep and wake, the old owner;
//! - a building's completion's first part (`FUN_1411c6bd0` -> `FUN_14104eac0`,
//!   [`Tracker::complete`]), a building's load and its owner change (`FUN_1411df160`,
//!   `FUN_1411cd860`);
//! - a master change (`FUN_1411a6a70` -> `FUN_14104fb20`, [`Tracker::promote`]);
//! - an upgrade to master 0x67f or 0x6a1 (`FUN_141657d10` -> `FUN_14104fd10`,
//!   [`Tracker::move_all`]).
//!
//! - the production queue's places: a queued unit files a −1 at the end of its master's pending
//!   list and its class's (`FUN_141050230`, [`Tracker::place`]), its spawn or cancel drops the
//!   last of each (`FUN_14104f330`, [`Tracker::unplace`]); the counter `+0x60`
//!   (`FUN_14104fb00`, [`Tracker::count_queued`]) follows the same writers.
//!
//! Not held: the ghosts (they file under the ghosts' master 243) and the counters at `+0x64` to
//! `+0x6c`. A reader of the done lists ([`Tracker::list`], [`Tracker::class_list`]) needs none of
//! them.
//!
//! 158041: the key `FUN_141658430`, a master's list `FUN_14104f960` (an empty one, `+0x40`, for
//! a key it lacks: `FUN_14104dd80`, `FUN_141050ae0`), a class's `FUN_14104f8f0`, the lists'
//! erase `FUN_140a63bd0` (order kept).

use crate::GenieError;
use crate::objects::ObjectId;
use crate::objects::lines::UnitLines;
use crate::objects::masters::MasterId;
use crate::objects::masters::Masters;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::master_type::BUILDING;
use crate::objects::masters::{ClassId, class};
use crate::players::PlayerId;
use crate::production::QueuedUnits;
use crate::world::Seconds;
use crate::world::slots::PlayerSlots;

/// A pending key is its done key + this (a master's + 90000, a class's + 90900).
const PENDING: i32 = 90_000;
/// A class's key is the class + this.
const CLASS_KEY: i32 = 900;

/// A list's key in the tracker (`FUN_141658430`'s integers): a key master's id, a class + 900,
/// each + 90000 while pending. The game keeps them as one integer, so a key master of 900 to
/// 999 shares a class's list (the game logs it), and a master the player lacks files under −1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ListKey(i32);

impl ListKey {
    /// The list of no key master (−1): [`Key::NONE`]'s.
    pub const NONE: Self = Self(-1);

    /// The done list of key master `master`.
    #[must_use]
    pub fn master(master: MasterId) -> Self {
        Self(i32::from(master.get()))
    }

    /// The done list of key master `master`, [`ListKey::NONE`] for none.
    #[must_use]
    pub fn of_master(master: Option<MasterId>) -> Self {
        master.map_or(Self::NONE, Self::master)
    }

    /// The done list of class `class` (the class + 900).
    #[must_use]
    pub fn class(class: ClassId) -> Self {
        Self(i32::from(class.get()) + CLASS_KEY)
    }

    /// The pending list of the same key (+ 90000).
    #[must_use]
    pub const fn pending(self) -> Self {
        Self(self.0 + PENDING)
    }

    /// Whether this is a pending list's key (90000 or more).
    #[must_use]
    pub const fn is_pending(self) -> bool {
        self.0 >= PENDING
    }

    /// The game's integer.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

/// The keys run below this master id (`FUN_141658430`: 9000 or more is none).
const TRACKED_MASTERS: i16 = 9000;
/// The classes a key holds below this (`FUN_141658430`).
const CLASSES: i16 = 100;
/// The classes that count as done from the start (packed and unpacked siege, 51 and 54).
const NEVER_PENDING: [ClassId; 2] = [class::CLASS_51, class::CLASS_54];
/// The classes whose key carries the military byte (`+9`, `FUN_141658430`'s switch).
const MILITARY: [ClassId; 25] = [
    class::CLASS_0,
    class::CLASS_6,
    class::CLASS_12,
    class::CLASS_17,
    class::MONK,
    class::TRANSPORT,
    class::WARSHIP,
    class::CLASS_23,
    class::CLASS_24,
    class::CLASS_25,
    class::CLASS_26,
    class::CLASS_28,
    class::CLASS_35,
    class::CLASS_36,
    class::CLASS_43,
    class::CLASS_44,
    class::CLASS_45,
    class::CLASS_46,
    class::CLASS_47,
    class::CLASS_50,
    class::TOWER,
    class::CLASS_53,
    class::CLASS_56,
    class::CLASS_57,
    class::KING,
];

/// The key `FUN_141658430` gives a master (12 bytes: the key master, its class, two bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key {
    /// `+0`: the key master (−1 for none).
    pub master: Option<MasterId>,
    /// `+4`: the key master's class (−1 for none).
    pub class: Option<ClassId>,
    /// `+8`: the object waits in the pending lists until its completion.
    pub pending: bool,
    /// `+9`: the key master's class is a military one.
    pub military: bool,
}

impl Key {
    /// The key of a master the player lacks (`FUN_141658430`'s −1, −1, 0).
    pub const NONE: Self = Self {
        master: None,
        class: None,
        pending: false,
        military: false,
    };

    /// The key master's list ([`ListKey::NONE`] for none), pending while the key waits.
    #[must_use]
    pub fn master_list(self) -> ListKey {
        self.waiting(ListKey::of_master(self.master))
    }

    /// The class's list (none for a key without a class), pending while the key waits.
    #[must_use]
    pub fn class_list(self) -> Option<ListKey> {
        self.class.map(|c| self.waiting(ListKey::class(c)))
    }

    /// `list`, pending while the key waits.
    const fn waiting(self, list: ListKey) -> ListKey {
        if self.pending { list.pending() } else { list }
    }
}

/// `FUN_141658430`: the key of player's master `master` (`masters`, the player's table; None
/// gives [`Key::NONE`]). A building (type 80) keys by its stack unit (`+0x986`) when it names
/// one (a stack unit the player lacks, or out of 0 to 8999, gives none) and waits in the pending
/// lists; an annex (a head unit, `+0x9b8`, and no annex of its own) files under no class. Any
/// other master keys by its line's first master ([`UnitLines::base`]). The key master must be one
/// the player has, below 9000; its class below 100 is the key's class, and classes 51 and 54 do
/// not wait.
#[must_use]
pub fn key(masters: Option<&Masters>, lines: &UnitLines, master: MasterId) -> Key {
    let get = |id: MasterId| masters?.get(id).filter(|_| id.get() < TRACKED_MASTERS);
    let Some(m) = get(master) else {
        return Key::NONE;
    };
    let mut out = Key::NONE;
    let mut classed = true;
    // The buildings' type, which wait in the pending lists.
    let id = if m.kind == BUILDING {
        let building = m.building.as_ref();
        if building.is_some_and(|b| b.head_unit.is_some() && b.annexes[0].is_none()) {
            classed = false;
        }
        out.pending = true;
        Some(building.and_then(|b| b.stack_unit).unwrap_or(master))
    } else {
        lines.base(master)
    };
    let Some(k) = id.and_then(get) else {
        return Key::NONE;
    };
    out.master = id;
    if classed && (0..CLASSES).contains(&k.class.get()) {
        out.class = Some(k.class);
        if NEVER_PENDING.contains(&k.class) {
            out.pending = false;
            out.military = false;
        }
        if MILITARY.contains(&k.class) {
            out.military = true;
        }
    }
    out
}

/// A player's tracker: the lists by key ([`key`]'s master, a class + 900, each + 90000 while
/// pending), each in the order its ids came in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tracker {
    lists: std::collections::BTreeMap<ListKey, Vec<Option<ObjectId>>>,
    /// `+0x60` (`FUN_14104fb00`): what the production queues' writers count, units queued and
    /// spawned or cancelled (a cancel of several counts them twice, as the game does).
    pub queued: i32,
}

impl Tracker {
    /// The list of key `key` (a key the tracker lacks: the empty one, `+0x40`); a queued unit's
    /// place holds none (−1, [`Tracker::place`]).
    #[must_use]
    pub fn list(&self, key: ListKey) -> &[Option<ObjectId>] {
        self.lists.get(&key).map_or(&[], Vec::as_slice)
    }

    /// `FUN_14104f8f0`: the done objects filed under class `class` (key class + 900).
    #[must_use]
    pub fn class_list(&self, class: ClassId) -> &[Option<ObjectId>] {
        self.list(ListKey::class(class))
    }

    fn push(&mut self, key: ListKey, id: Option<ObjectId>) {
        self.lists.entry(key).or_default().push(id);
    }

    /// `FUN_140a63bd0` on the first `id` of list `key`: the ids after it move up.
    fn erase(&mut self, key: ListKey, id: ObjectId) {
        if let Some(list) = self.lists.get_mut(&key)
            && let Some(k) = list.iter().position(|&x| x == Some(id))
        {
            let _ = list.remove(k);
        }
    }

    fn holds(&self, key: ListKey, id: ObjectId) -> bool {
        self.list(key).contains(&Some(id))
    }

    /// `FUN_14104e7f0`: `id` joins the end of its key master's list and of its class's (when it
    /// has one), each + 90000 while pending. (The game logs a key master of 900 to 999, which a
    /// class's list uses.)
    pub fn add(&mut self, id: ObjectId, key: Key) {
        self.push(key.master_list(), Some(id));
        if let Some(class) = key.class_list() {
            self.push(class, Some(id));
        }
    }

    /// `FUN_14104eac0`: a pending `id` leaves its pending lists for the end of its done ones
    /// (the master's, then the class's), unless already there; a key that does not wait
    /// changes nothing.
    pub fn complete(&mut self, id: ObjectId, key: Key) {
        if !key.pending {
            return;
        }
        let done = Key {
            pending: false,
            ..key
        };
        self.erase(key.master_list(), id);
        if !self.holds(done.master_list(), id) {
            self.push(done.master_list(), Some(id));
        }
        if let (Some(waiting), Some(class)) = (key.class_list(), done.class_list()) {
            self.erase(waiting, id);
            if !self.holds(class, id) {
                self.push(class, Some(id));
            }
        }
    }

    /// `FUN_14104fff0`: `id` leaves its class's lists (the pending one too while its key waits)
    /// and its key master's (the pending one first). (A tracker in teardown, its `+0xb0`, is
    /// not held.)
    pub fn remove(&mut self, id: ObjectId, key: Key) {
        let done = Key {
            pending: false,
            ..key
        };
        if let Some(class) = done.class_list() {
            self.erase(class, id);
            if key.pending {
                self.erase(class.pending(), id);
            }
        }
        if key.pending {
            self.erase(key.master_list(), id);
        }
        self.erase(done.master_list(), id);
    }

    /// `FUN_14104fb00`: the counter of queued units (`+0x60`) changes by `n`.
    pub fn count_queued(&mut self, n: QueuedUnits) {
        self.queued += i32::from(n.get());
    }

    /// `FUN_141050230`: a queued unit of master `master` (its [`key`]'s class `class`, −1 for
    /// none) files a −1 (none) at the end of the master's pending list (+ 90000, the master
    /// itself, not its key master) and of the class's (+ 90900).
    pub fn place(&mut self, master: Option<MasterId>, class: Option<ClassId>) {
        self.push(ListKey::of_master(master).pending(), None);
        if let Some(class) = class {
            self.push(ListKey::class(class).pending(), None);
        }
    }

    /// `FUN_14104f330`: a queued unit's spawn or cancel drops the last id of the same lists
    /// (`FUN_140a63bd0` at the end; an empty list stays empty).
    pub fn unplace(&mut self, master: Option<MasterId>, class: Option<ClassId>) {
        let mut drop_last = |key: ListKey| {
            if let Some(list) = self.lists.get_mut(&key) {
                let _ = list.pop();
            }
        };
        drop_last(ListKey::of_master(master).pending());
        if let Some(class) = class {
            drop_last(ListKey::class(class).pending());
        }
    }

    /// `FUN_14104fb20`: an object changes masters. Out of its old key master's done list it goes
    /// to the end of the new one's, and from the old class's done list to the end of the new
    /// one's (a key without a class has none); one the old done list does not hold stays where
    /// it is (the game logs "Skipping promotion on untracked object.").
    pub fn promote(&mut self, id: ObjectId, old: Key, new: Key) {
        let old_list = ListKey::of_master(old.master);
        if !self.holds(old_list, id) {
            return;
        }
        self.erase(old_list, id);
        self.push(ListKey::of_master(new.master), Some(id));
        if let Some(class) = old.class {
            self.erase(ListKey::class(class), id);
        }
        if let Some(class) = new.class {
            self.push(ListKey::class(class), Some(id));
        }
    }

    /// `FUN_14104fd10`: every id of list `old` the world holds (`class_of` gives its master's
    /// class) goes, in order, to the end of list `new`, out of the done list of `old_class` (the
    /// class of [`key`] of master `old`, −1 for none: key 899) and to the end of its own class's;
    /// list `old` then empties. Below the pending keys the same runs for the pending keys of
    /// both (each + 90000), whose class [`key`] gives none (a pending id goes to its class's done
    /// list). Its one caller (`FUN_141657d10`, the upgrades to masters 0x67f and 0x6a1) passes two masters;
    /// the same key twice would loop for ever in the game.
    pub fn move_all(
        &mut self,
        old: ListKey,
        new: ListKey,
        old_class: Option<ClassId>,
        class_of: &dyn Fn(ObjectId) -> Result<Option<ClassId>, GenieError>,
    ) -> Result<(), GenieError> {
        if let Some(ids) = self.lists.get(&old).cloned() {
            for id in ids {
                // a queued unit's place (−1) and a negative id hold no object of a class
                let Some(id) = id.filter(|id| 0 <= id.get()) else {
                    continue;
                };
                let Some(class) = class_of(id)? else {
                    continue;
                };
                self.push(new, Some(id));
                if let Some(old_class) = old_class {
                    self.erase(ListKey::class(old_class), id);
                }
                self.push(ListKey::class(class), Some(id));
            }
            if let Some(list) = self.lists.get_mut(&old) {
                list.clear();
            }
        }
        if !old.is_pending() {
            self.move_all(old.pending(), new.pending(), Key::NONE.class, class_of)?;
        }
        Ok(())
    }
}

impl crate::world::World {
    /// [`key`] of player `p`'s master `master`.
    #[must_use]
    pub fn tracker_key(&self, p: PlayerId, master: MasterId) -> Key {
        key(self.masters.of(p), &self.lines, master)
    }

    /// The load's filing: each player's objects as its lists hold them, the live ones then the
    /// sleeping ones, each added at the end of its own load (`FUN_14119df60`); a building's load
    /// (`FUN_1411df160`) then places each unit of its queue's entries of a master above 0
    /// ([`Tracker::place`], one count each) and, a building whose build points (`+0xc50`) reach
    /// its build time (`COMISS`/`JC`: a NaN does not), completes it at its end.
    ///
    /// # Errors
    ///
    /// A player the world does not hold.
    pub fn load_trackers(&mut self) -> Result<(), GenieError> {
        for p in PlayerId::each(self.players.len()) {
            let mut tracker = Tracker::default();
            let player = self.players.player(p)?;
            for u in player.objects.iter().chain(&player.sleeping) {
                let k = self.tracker_key(p, u.life.master_id);
                tracker.add(u.id, k);
                for e in u.production.iter().flat_map(|q| q.queue.entries()) {
                    if let Some(master) = e.master.filter(|m| 0 < m.get())
                        && QueuedUnits::ZERO < e.count
                    {
                        let class = self.tracker_key(p, master).class;
                        for _ in 0..e.count.get() {
                            tracker.place(Some(master), class);
                            tracker.count_queued(QueuedUnits::ONE);
                        }
                    }
                }
                let building = u.master(&self.masters, p)?.kind == BUILDING;
                if building
                    && u.construction
                        .is_some_and(|c| Seconds::of_master(c.build_time) <= c.progress)
                {
                    tracker.complete(u.id, k);
                }
            }
            self.players.player_mut(p)?.tracker = tracker;
        }
        Ok(())
    }

    /// The player's add (slot 0x2d0, `FUN_14165cc80`'s tracker part) of its object `id` of
    /// master `master`.
    pub(crate) fn track_add(
        &mut self,
        p: PlayerId,
        id: ObjectId,
        master: MasterId,
    ) -> Result<(), GenieError> {
        let k = self.tracker_key(p, master);
        self.players.player_mut(p)?.tracker.add(id, k);
        Ok(())
    }

    /// The player's remove (slot 0x2d8, `FUN_141668040`'s tracker part) of its object `id` of
    /// master `master`.
    pub(crate) fn track_remove(
        &mut self,
        p: PlayerId,
        id: ObjectId,
        master: MasterId,
    ) -> Result<(), GenieError> {
        let k = self.tracker_key(p, master);
        self.players.player_mut(p)?.tracker.remove(id, k);
        Ok(())
    }

    /// An object moving between the player's live and sleeping lists (its sleep, the static
    /// object's update; its wake, `FUN_14119deb0`): the player's remove (slot 0x2d8), then its
    /// add (slot 0x2d0), which files it last.
    pub(crate) fn track_relist(
        &mut self,
        p: PlayerId,
        id: ObjectId,
        master: MasterId,
    ) -> Result<(), GenieError> {
        let k = self.tracker_key(p, master);
        let tracker = &mut self.players.player_mut(p)?.tracker;
        tracker.remove(id, k);
        tracker.add(id, k);
        Ok(())
    }

    /// A building's completion's first part (`FUN_1411c6bd0`'s tracker part).
    pub(crate) fn track_complete(
        &mut self,
        p: PlayerId,
        id: ObjectId,
        master: MasterId,
    ) -> Result<(), GenieError> {
        let k = self.tracker_key(p, master);
        self.players.player_mut(p)?.tracker.complete(id, k);
        Ok(())
    }

    /// A master change (`FUN_1411a6a70`'s tracker part) of player `p`'s object `id` from master
    /// `old` to `new`.
    pub(crate) fn track_promote(
        &mut self,
        p: PlayerId,
        id: ObjectId,
        old: MasterId,
        new: MasterId,
    ) -> Result<(), GenieError> {
        let (old, new) = (self.tracker_key(p, old), self.tracker_key(p, new));
        self.players.player_mut(p)?.tracker.promote(id, old, new);
        Ok(())
    }

    /// `FUN_14104fd10` on player `p`'s tracker from master `from`'s list to `to`'s
    /// ([`Tracker::move_all`]): an object's class is its master's, the world's object
    /// (`FUN_1415417a0`) of any player.
    pub(crate) fn track_move_all(
        &mut self,
        p: PlayerId,
        from: MasterId,
        to: MasterId,
    ) -> Result<(), GenieError> {
        let old_class = self.tracker_key(p, from).class;
        let mut tracker = std::mem::take(&mut self.players.player_mut(p)?.tracker);
        let class_of = |id: ObjectId| -> Result<Option<ClassId>, GenieError> {
            let Some((q, u)) = self.object(id) else {
                return Ok(None);
            };
            Ok(Some(u.master(&self.masters, q)?.class))
        };
        let moved = tracker.move_all(
            ListKey::master(from),
            ListKey::master(to),
            old_class,
            &class_of,
        );
        self.players.player_mut(p)?.tracker = tracker;
        moved
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::MasterHitPoints;
    use crate::objects::lines::UnitLine;
    use crate::objects::masters::ObstructionClass;
    use crate::objects::masters::ObstructionType;
    use crate::objects::masters::ids;
    use crate::objects::masters::master_type;
    use crate::objects::masters::{BuildingMaster, Master};
    use crate::test_ids::master_id;
    use crate::test_ids::object_id;
    use crate::test_ids::opt_master;
    use crate::world::Millis;
    use crate::world::WholeSeconds;
    use crate::world::WorldTime;

    fn building(id: i16, class: ClassId, stack: i16, head: i16, annex: i16) -> Master {
        Master {
            id: master_id(id),
            kind: BUILDING,
            class,
            building: Some(BuildingMaster {
                stack_unit: opt_master(stack),
                head_unit: opt_master(head),
                annexes: [opt_master(annex), None, None, None],
                ..BuildingMaster::default()
            }),
            ..Master::default()
        }
    }

    fn unit(id: MasterId, class: ClassId) -> Master {
        Master {
            id,
            kind: master_type::COMBAT,
            class,
            ..Master::default()
        }
    }

    fn masters() -> Masters {
        Masters::new([
            unit(master_id(4), class::CLASS_0),
            unit(master_id(24), class::CLASS_0),
            unit(ids::VILLAGER_MALE, class::VILLAGER),
            unit(ids::BUILDER, class::VILLAGER),
            building(79, class::TOWER, -1, -1, -1),
            building(234, class::TOWER, -1, -1, -1),
            building(109, class::BUILDING, -1, -1, 618),
            building(618, class::BUILDING, -1, 109, -1),
            building(1189, class::BUILDING, 79, -1, -1),
            building(1190, class::BUILDING, 9999, -1, -1),
            building(42, class::CLASS_54, -1, -1, -1),
            building(43, ClassId::of_master(130), -1, -1, -1),
            unit(ids::ID_331, class::CLASS_54),
        ])
    }

    /// Key master `id`'s done list.
    fn list_of(id: i16) -> ListKey {
        ListKey::master(master_id(id))
    }

    /// A list's key is the game's integer: a master's id, a class + 900, each + 90000 pending.
    #[test]
    fn a_list_key_is_the_games_integer() {
        assert_eq!(
            (list_of(109).get(), list_of(109).pending().get()),
            (109, 90_109)
        );
        assert_eq!(
            (
                ListKey::class(class::BUILDING).get(),
                ListKey::class(class::BUILDING).pending().get()
            ),
            (903, 90_903)
        );
        assert_eq!(ListKey::NONE.pending().get(), 89_999);
        assert!(list_of(109).pending().is_pending() && !ListKey::NONE.pending().is_pending());
        // a key master of 900 to 999 shares a class's list
        assert_eq!(list_of(903), ListKey::class(class::BUILDING));
    }

    /// A waiting key files under its pending lists, a done one under its done lists.
    #[test]
    fn a_keys_lists_are_pending_while_it_waits() {
        let tc = Key {
            master: Some(master_id(109)),
            class: Some(class::BUILDING),
            pending: true,
            military: false,
        };
        assert_eq!(
            (tc.master_list(), tc.class_list()),
            (
                list_of(109).pending(),
                Some(ListKey::class(class::BUILDING).pending())
            )
        );
        let done = Key {
            pending: false,
            ..tc
        };
        assert_eq!(
            (done.master_list(), done.class_list()),
            (list_of(109), Some(ListKey::class(class::BUILDING)))
        );
        assert_eq!(
            (Key::NONE.master_list(), Key::NONE.class_list()),
            (ListKey::NONE, None)
        );
    }

    fn lines() -> UnitLines {
        UnitLines::build(
            2000,
            &[UnitLine {
                line: Some(-299),
                chain: Some(vec![4, 24, 492]),
                ..UnitLine::default()
            }],
        )
    }

    /// `FUN_141050230` files a −1 under the queued master itself (+ 90000, the crossbowman's
    /// own 24, not its key master 4) and its key's class (+ 90900); `FUN_14104f330` drops the
    /// last of each, whatever it is, and leaves an empty list alone.
    #[test]
    fn a_queued_unit_places_a_minus_one_and_its_spawn_drops_the_last() {
        let mut t = Tracker::default();
        t.place(Some(master_id(24)), Some(class::CLASS_0));
        t.place(Some(master_id(24)), Some(class::CLASS_0));
        t.place(Some(ids::VILLAGER_MALE), None);
        assert_eq!(t.list(list_of(24).pending()), [None, None]);
        assert_eq!(
            t.list(ListKey::class(class::CLASS_0).pending()),
            [None, None]
        );
        assert_eq!(t.list(list_of(83).pending()), [None]);
        t.lists
            .get_mut(&ListKey::class(class::CLASS_0).pending())
            .unwrap()
            .push(Some(object_id(5)));
        t.unplace(Some(master_id(24)), Some(class::CLASS_0));
        assert_eq!(t.list(list_of(24).pending()), [None]);
        assert_eq!(
            t.list(ListKey::class(class::CLASS_0).pending()),
            [None, None]
        );
        t.unplace(Some(ids::VILLAGER_MALE), None);
        t.unplace(Some(ids::VILLAGER_MALE), None);
        assert_eq!(t.list(list_of(83).pending()), []);
    }

    #[test]
    fn a_unit_keys_by_its_lines_first_master_and_a_building_by_its_stack_unit() {
        let (m, l) = (masters(), lines());
        let k = |id| key(Some(&m), &l, id);
        // the crossbowman (24) files under the archer (4), class 0 a military one
        assert_eq!(
            k(master_id(24)),
            Key {
                master: Some(master_id(4)),
                class: Some(class::CLASS_0),
                pending: false,
                military: true
            }
        );
        assert_eq!(
            k(ids::VILLAGER_MALE),
            Key {
                master: Some(master_id(83)),
                class: Some(class::VILLAGER),
                pending: false,
                military: false
            }
        );
        // a building waits; a stack unit is its key master (with its class)
        assert_eq!(
            k(ids::TOWN_CENTRE),
            Key {
                master: Some(master_id(109)),
                class: Some(class::BUILDING),
                pending: true,
                military: false
            }
        );
        assert_eq!(
            k(master_id(1189)),
            Key {
                master: Some(master_id(79)),
                class: Some(class::TOWER),
                pending: true,
                military: true
            }
        );
        // an annex files under no class; a stack unit out of range gives none
        assert_eq!(
            k(master_id(618)),
            Key {
                master: Some(master_id(618)),
                class: None,
                pending: true,
                military: false
            }
        );
        assert_eq!(k(master_id(1190)), Key::NONE);
        // classes 51 and 54 do not wait; a class of 100 or more is none and the building waits
        assert_eq!(
            (k(ids::ID_42).pending, k(ids::ID_42).class),
            (false, Some(class::CLASS_54))
        );
        assert_eq!(
            (k(master_id(43)).pending, k(master_id(43)).class),
            (true, None)
        );
        assert_eq!(
            (k(ids::ID_331).pending, k(ids::ID_331).class),
            (false, Some(class::CLASS_54))
        );
        // a master the player lacks, a line whose first master it lacks, no table
        assert_eq!(k(ids::ID_7), Key::NONE);
        assert_eq!(k(master_id(492)), Key::NONE);
        assert_eq!(key(None, &l, ids::VILLAGER_MALE), Key::NONE);
    }

    #[test]
    fn a_building_waits_until_its_completion_moves_it_to_the_end_of_the_done_lists() {
        let (m, l) = (masters(), lines());
        let tc = key(Some(&m), &l, ids::TOWN_CENTRE);
        let mut t = Tracker::default();
        t.add(object_id(10), tc);
        t.add(object_id(11), tc);
        assert_eq!(
            (t.list(list_of(109)), t.list(list_of(109).pending())),
            (&[][..], &[Some(object_id(10)), Some(object_id(11))][..])
        );
        assert_eq!(
            t.list(ListKey::class(class::BUILDING).pending()),
            [Some(object_id(10)), Some(object_id(11))]
        );
        t.complete(object_id(11), tc);
        t.complete(object_id(10), tc);
        assert_eq!(
            (t.list(list_of(109)), t.list(list_of(109).pending())),
            (&[Some(object_id(11)), Some(object_id(10))][..], &[][..])
        );
        assert_eq!(
            t.class_list(class::BUILDING),
            [Some(object_id(11)), Some(object_id(10))]
        );
        // a second completion leaves the order as it is
        t.complete(object_id(11), tc);
        assert_eq!(
            t.list(list_of(109)),
            [Some(object_id(11)), Some(object_id(10))]
        );
        // a key that does not wait changes nothing
        let villager = key(Some(&m), &l, ids::VILLAGER_MALE);
        t.add(object_id(20), villager);
        t.complete(object_id(20), villager);
        assert_eq!(t.class_list(class::VILLAGER), [Some(object_id(20))]);
    }

    #[test]
    fn a_removal_keeps_the_others_order() {
        let (m, l) = (masters(), lines());
        let v = key(Some(&m), &l, ids::VILLAGER_MALE);
        let mut t = Tracker::default();
        for id in [1, 2, 3, 4] {
            t.add(object_id(id), v);
        }
        t.remove(object_id(2), v);
        assert_eq!(
            (t.list(list_of(83)), t.class_list(class::VILLAGER)),
            (
                &[Some(object_id(1)), Some(object_id(3)), Some(object_id(4))][..],
                &[Some(object_id(1)), Some(object_id(3)), Some(object_id(4))][..]
            )
        );
        // a pending building leaves both its lists, done or not
        let tc = key(Some(&m), &l, ids::TOWN_CENTRE);
        t.add(object_id(30), tc);
        t.add(object_id(31), tc);
        t.complete(object_id(31), tc);
        t.remove(object_id(30), tc);
        t.remove(object_id(31), tc);
        assert_eq!(
            (
                t.list(list_of(109)),
                t.list(list_of(109).pending()),
                t.class_list(class::BUILDING)
            ),
            (&[][..], &[][..], &[][..])
        );
    }

    #[test]
    fn a_promotion_moves_an_object_to_the_end_of_its_new_lists() {
        let (m, l) = (masters(), lines());
        let (villager, builder) = (
            key(Some(&m), &l, ids::VILLAGER_MALE),
            key(Some(&m), &l, ids::BUILDER),
        );
        let mut t = Tracker::default();
        for id in [1, 2, 3] {
            t.add(object_id(id), villager);
        }
        // villager 1 becomes a builder: class 4's list takes it last
        t.promote(object_id(1), villager, builder);
        assert_eq!(
            (t.list(list_of(83)), t.list(list_of(118))),
            (
                &[Some(object_id(2)), Some(object_id(3))][..],
                &[Some(object_id(1))][..]
            )
        );
        assert_eq!(
            t.class_list(class::VILLAGER),
            [Some(object_id(2)), Some(object_id(3)), Some(object_id(1))]
        );
        // an archer upgraded in its line stays under the archer, last
        let archer = key(Some(&m), &l, master_id(4));
        let crossbow = key(Some(&m), &l, master_id(24));
        t.add(object_id(7), archer);
        t.add(object_id(8), archer);
        t.promote(object_id(7), archer, crossbow);
        assert_eq!(
            (t.list(list_of(4)), t.class_list(class::CLASS_0)),
            (
                &[Some(object_id(8)), Some(object_id(7))][..],
                &[Some(object_id(8)), Some(object_id(7))][..]
            )
        );
        // an object the old list does not hold is skipped
        t.promote(object_id(99), villager, builder);
        assert_eq!(t.list(list_of(118)), [Some(object_id(1))]);
    }

    fn world() -> crate::world::World {
        use crate::objects::unit::Unit;
        use crate::world::{Player, World};
        let unit = |id: i32, master: MasterId, progress: Option<f32>| {
            let mut u = Unit {
                id: object_id(id),
                construction: progress.map(|progress| crate::construction::build::Construction {
                    progress: Seconds::new(progress),
                    desolid: false,
                    builders: crate::construction::BuilderCount::of_data(0),
                    hit_points: MasterHitPoints::of_data(2400),
                    build_time: WholeSeconds::of_data(150),
                    record: crate::map::spatial::Obstruction {
                        kind: ObstructionType::NONE,
                        class: ObstructionClass::NONE,
                    },
                    snow: false,
                    sprite_override: false,
                }),
                ..Unit::default()
            };
            u.life.master_id = master;
            u
        };
        let player = Player {
            objects: vec![
                unit(10, ids::TOWN_CENTRE, Some(150.0)),
                unit(11, ids::VILLAGER_MALE, None),
                unit(12, ids::TOWN_CENTRE, Some(149.9)),
                unit(13, ids::BUILDER, None),
                unit(14, ids::TOWN_CENTRE, Some(f32::NAN)),
            ]
            .into(),
            sleeping: vec![unit(15, ids::VILLAGER_MALE, None)].into(),
            ..Player::default()
        };
        let mut t = World::new(
            WorldTime::ZERO,
            crate::world::FrameSeed::of_data(0),
            PlayerId::new(0),
            vec![Millis::ZERO; 2],
            vec![Player::default(), player],
        );
        t.masters = vec![masters(), masters()];
        t.lines = lines();
        t
    }

    #[test]
    fn the_load_files_the_live_objects_then_the_sleeping_ones_and_completes_the_built() {
        let mut t = world();
        t.load_trackers().unwrap();
        let tracker = &t.players[1].tracker;
        // build points at the build time complete; below it, or NaN, the building waits
        assert_eq!(
            (
                tracker.list(list_of(109)),
                tracker.list(list_of(109).pending())
            ),
            (
                &[Some(object_id(10))][..],
                &[Some(object_id(12)), Some(object_id(14))][..]
            )
        );
        assert_eq!(
            tracker.class_list(class::VILLAGER),
            [
                Some(object_id(11)),
                Some(object_id(13)),
                Some(object_id(15))
            ]
        );
        assert_eq!(
            (tracker.list(list_of(83)), tracker.list(list_of(118))),
            (
                &[Some(object_id(11)), Some(object_id(15))][..],
                &[Some(object_id(13))][..]
            )
        );
        assert_eq!(t.players[0].tracker, Tracker::default());
    }

    #[test]
    fn a_relisted_object_goes_last_and_a_completed_one_joins_its_done_lists() {
        let mut t = world();
        t.load_trackers().unwrap();
        t.track_relist(PlayerId::new(1), object_id(11), ids::VILLAGER_MALE)
            .unwrap();
        assert_eq!(
            t.players[1].tracker.class_list(class::VILLAGER),
            [
                Some(object_id(13)),
                Some(object_id(15)),
                Some(object_id(11))
            ]
        );
        t.track_complete(PlayerId::new(1), object_id(12), ids::TOWN_CENTRE)
            .unwrap();
        assert_eq!(
            t.players[1].tracker.list(list_of(109)),
            [Some(object_id(10)), Some(object_id(12))]
        );
        t.track_promote(
            PlayerId::new(1),
            object_id(15),
            master_id(83),
            master_id(118),
        )
        .unwrap();
        assert_eq!(
            (
                t.players[1].tracker.list(list_of(118)),
                t.players[1].tracker.class_list(class::VILLAGER)
            ),
            (
                &[Some(object_id(13)), Some(object_id(15))][..],
                &[
                    Some(object_id(13)),
                    Some(object_id(11)),
                    Some(object_id(15))
                ][..]
            )
        );
        t.track_remove(PlayerId::new(1), object_id(13), ids::BUILDER)
            .unwrap();
        t.track_add(PlayerId::new(1), object_id(16), ids::VILLAGER_MALE)
            .unwrap();
        assert_eq!(
            t.players[1].tracker.class_list(class::VILLAGER),
            [
                Some(object_id(11)),
                Some(object_id(15)),
                Some(object_id(16))
            ]
        );
    }

    #[test]
    fn a_move_of_all_reads_each_objects_class_from_the_world() {
        let mut t = world();
        t.load_trackers().unwrap();
        // the villagers' list (83) moves to 118's; their class (4) stays theirs
        t.track_move_all(PlayerId::new(1), master_id(83), master_id(118))
            .unwrap();
        let tracker = &t.players[1].tracker;
        assert_eq!(
            (tracker.list(list_of(83)), tracker.list(list_of(118))),
            (
                &[][..],
                &[
                    Some(object_id(13)),
                    Some(object_id(11)),
                    Some(object_id(15))
                ][..]
            )
        );
        assert_eq!(
            tracker.class_list(class::VILLAGER),
            [
                Some(object_id(13)),
                Some(object_id(11)),
                Some(object_id(15))
            ]
        );
    }

    #[test]
    fn a_move_of_all_takes_the_list_and_its_pending_one_and_empties_them() {
        let mut t = Tracker::default();
        t.push(list_of(500), Some(object_id(1)));
        t.push(list_of(500), None);
        t.push(list_of(500), Some(object_id(2)));
        t.push(list_of(500), Some(object_id(3)));
        t.push(ListKey::class(class::CLASS_5), Some(object_id(1)));
        t.push(ListKey::class(class::CLASS_5), Some(object_id(3)));
        t.push(list_of(500).pending(), Some(object_id(4)));
        t.push(list_of(500).pending(), None);
        // 2 is gone from the world; the objects are of class 6 now
        let class_of = |id: ObjectId| Ok((id != object_id(2)).then_some(class::CLASS_6));
        t.move_all(list_of(500), list_of(600), Some(class::CLASS_5), &class_of)
            .unwrap();
        assert_eq!(
            (t.list(list_of(500)), t.list(list_of(500).pending())),
            (&[][..], &[][..])
        );
        assert_eq!(
            (t.list(list_of(600)), t.list(list_of(600).pending())),
            (
                &[Some(object_id(1)), Some(object_id(3))][..],
                &[Some(object_id(4))][..]
            )
        );
        // the pending id goes to its class's done list too
        assert_eq!(
            (t.class_list(class::CLASS_5), t.class_list(class::CLASS_6)),
            (
                &[][..],
                &[Some(object_id(1)), Some(object_id(3)), Some(object_id(4))][..]
            )
        );
    }
}
