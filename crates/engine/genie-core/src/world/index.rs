//! The world's table by id (`FUN_1415417a0`: `world+0x168` for ids 0 and up, `+0x178` for the
//! recycled ones, FD-045-D49): where each object is, a player's live or sleeping slot.
//!
//! Its one writer is its own list operations ([`ObjectIndex::push`], [`ObjectIndex::swap_remove`],
//! [`ObjectIndex::remove`], [`ObjectIndex::put_back`]), which change the list they are given and
//! file what moved; the world's ([`World::push_live`], [`World::swap_remove_live`] and the rest)
//! and the update's view ([`crate::world::others::Others`]) call them. A lookup checks that the
//! slot still holds the id, so an object out for its update (its stand-in in the slot) answers
//! none, as the list's own lookup did. A list changed in place (a test's, the import's) indexes
//! again with [`World::reindex`].

use crate::GenieError;
use crate::objects::IdTable;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectKey;
use crate::objects::masters::Master;
use crate::objects::object_list::ObjectList;
use crate::objects::unit::{TAKEN_OUT, Unit};
use crate::players::PlayerId;
use crate::world::World;
use crate::world::parts::Object;
use crate::world::player::Player;
use crate::world::slots::PlayerSlots;

/// Where an object is: its player and its index in the live or the sleeping list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Place {
    Live(PlayerId, ListIndex),
    Sleeping(PlayerId, ListIndex),
}

impl Place {
    /// Slot `i` of player `p`'s sleeping list, or else its live one.
    pub(crate) const fn of(p: PlayerId, sleeping: bool, i: ListIndex) -> Self {
        if sleeping {
            Self::Sleeping(p, i)
        } else {
            Self::Live(p, i)
        }
    }

    /// The object's player (`+0x138`'s number).
    pub(crate) const fn player(self) -> PlayerId {
        match self {
            Self::Live(p, _) | Self::Sleeping(p, _) => p,
        }
    }

    /// The slot in its list.
    pub(crate) const fn slot(self) -> ListIndex {
        match self {
            Self::Live(_, i) | Self::Sleeping(_, i) => i,
        }
    }

    /// Whether the slot is in a sleeping list.
    pub(crate) const fn sleeping(self) -> bool {
        matches!(self, Self::Sleeping(..))
    }

    /// Slot `i` of the same list.
    const fn at(self, i: ListIndex) -> Self {
        Self::of(self.player(), self.sleeping(), i)
    }
}

/// The world's table by id: each object's place.
#[derive(Clone, Debug, Default)]
pub struct ObjectIndex {
    places: IdTable<Place>,
    /// The lists held one id in two slots (a test's default units): the table does not answer,
    /// a scan of the lists does, the first player's first.
    twice: bool,
}

impl PartialEq for ObjectIndex {
    /// The table follows the lists, which a world's comparison compares.
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl ObjectIndex {
    /// The table of `players`' lists, each player's live list before its sleeping one.
    #[must_use]
    pub fn of(players: &[Player]) -> Self {
        let mut index = Self::default();
        for (p, player) in PlayerId::each(players.len()).zip(players) {
            for (i, u) in player.objects.indexed() {
                index.file(u.id, Place::Live(p, i));
            }
            for (i, u) in player.sleeping.indexed() {
                index.file(u.id, Place::Sleeping(p, i));
            }
        }
        index
    }

    /// A table that never answers, so that every lookup scans: the view of lists no world holds
    /// (a test's).
    #[cfg(test)]
    pub(crate) fn scanning() -> &'static Self {
        static SCANNING: std::sync::LazyLock<ObjectIndex> =
            std::sync::LazyLock::new(|| ObjectIndex {
                places: IdTable::new(),
                twice: true,
            });
        &SCANNING
    }

    /// Whether a lookup scans the lists (they held one id twice).
    pub(crate) const fn scans(&self) -> bool {
        self.twice
    }

    /// Object `id`'s place as filed, unchecked; none when the table scans ([`ObjectIndex::scans`]).
    /// The caller checks that the slot holds the id.
    pub(crate) fn filed(&self, id: impl ObjectKey) -> Option<Place> {
        if self.twice {
            return None;
        }
        self.places.get(id.key()?).copied()
    }

    /// Where object `id` is among `players`' lists (the world's, which the table follows); none
    /// for an id they do not hold or an object out for its update. The caller that holds the
    /// world's parts apart asks here, the others [`World::place`].
    pub(crate) fn place(&self, players: &[Player], id: impl ObjectKey) -> Option<Place> {
        let id = id.key()?;
        if self.twice {
            return scan(players, id);
        }
        let place = self.filed(id)?;
        let player = players.get(place.player().index())?;
        list(player, place.sleeping())
            .get(place.slot())
            .is_some_and(|u| u.id == id)
            .then_some(place)
    }

    /// Object `id` among `players`' lists with its owner and whether it sleeps.
    pub(crate) fn locate<'a>(
        &self,
        players: &'a [Player],
        id: impl ObjectKey,
    ) -> Option<(PlayerId, &'a Unit, bool)> {
        let place = self.place(players, id)?;
        let u = players.at(place).ok()?;
        Some((place.player(), u, place.sleeping()))
    }

    /// `unit` goes to the end of `list`, the list of `at`'s player and kind; returns its slot.
    pub(crate) fn push(&mut self, list: &mut ObjectList, at: Place, unit: Unit) -> ListIndex {
        let i = list.next_place();
        let id = unit.id;
        list.push(unit);
        self.file(id, at.at(i));
        i
    }

    /// The object at `place` leaves `list` (the list there); the last one takes its slot.
    ///
    /// # Errors
    ///
    /// The list holds no such slot.
    pub(crate) fn swap_remove(
        &mut self,
        list: &mut ObjectList,
        place: Place,
    ) -> Result<Unit, GenieError> {
        let i = place.slot();
        let out = list.out_at(i);
        let gone = list.swap_remove(i)?;
        self.left(gone.id, out, place);
        if let Some(moved) = list.get(i).map(|u| u.id) {
            self.refile(moved, place);
        }
        Ok(gone)
    }

    /// The object at `place` leaves `list` (the list there); the ones after it move up.
    ///
    /// # Errors
    ///
    /// The list holds no such slot.
    pub(crate) fn remove(
        &mut self,
        list: &mut ObjectList,
        place: Place,
    ) -> Result<Unit, GenieError> {
        let i = place.slot();
        let out = list.out_at(i);
        let gone = list.remove(i)?;
        self.left(gone.id, out, place);
        for (k, u) in list.indexed().skip(i.get()) {
            self.refile(u.id, place.at(k));
        }
        Ok(gone)
    }

    /// `unit`, which `list` (player `p`'s live list) let out, goes back to the slot its stand-in
    /// holds now: that slot.
    ///
    /// # Errors
    ///
    /// The unit, when its stand-in left the list (or it was never taken out).
    pub(crate) fn put_back(
        &mut self,
        list: &mut ObjectList,
        p: PlayerId,
        unit: Unit,
    ) -> Result<ListIndex, Box<Unit>> {
        let id = unit.id;
        let i = list.put_back(unit)?;
        self.refile(id, Place::Live(p, i));
        Ok(i)
    }

    /// Object `id` takes `place`, unless the table holds it already (then the lists hold it
    /// twice); a stand-in is no object.
    fn file(&mut self, id: ObjectId, place: Place) {
        if id == TAKEN_OUT {
            return;
        }
        if self.places.contains_key(id) {
            self.twice = true;
        } else {
            let _ = self.places.insert(id, place);
        }
    }

    /// Object `id` moved to `place` (a list's swap, a put back).
    fn refile(&mut self, id: ObjectId, place: Place) {
        if id != TAKEN_OUT {
            let _ = self.places.insert(id, place);
        }
    }

    /// The object of `gone`, which left slot `place`: a stand-in's object (`out`, the list's
    /// record of it) stays out for good and leaves the table wherever it was filed.
    fn left(&mut self, gone: ObjectId, out: Option<ObjectId>, place: Place) {
        match out {
            Some(id) if gone == TAKEN_OUT => {
                let _ = self.places.remove(id);
            }
            _ => {
                if self.places.get(gone) == Some(&place) {
                    let _ = self.places.remove(gone);
                }
            }
        }
    }
}

/// The part tables [`ObjectsRef::of_tables`] reads: the building, combat and action layers, the
/// objects the load could not read whole and the motion layers (one of them lent out).
pub type PartTables<'a> = (
    &'a crate::world::parts::PartTable<crate::objects::layers::Building>,
    &'a crate::world::parts::PartTable<crate::objects::layers::Combat>,
    &'a crate::world::parts::PartTable<crate::objects::layers::Action>,
    &'a crate::world::parts::PartTable<crate::world::Unread>,
    crate::world::motions::Lent<'a, crate::objects::layers::Motion>,
);

/// The world's objects by id, to read, beside the parts a caller holds apart (a walk holds the
/// spatial index and the planners): the players' lists, the table by id, the players' masters
/// and the objects' parts. An object out for its update is none.
#[derive(Clone, Copy, Debug)]
pub struct ObjectsRef<'a> {
    players: &'a [Player],
    index: &'a ObjectIndex,
    masters: crate::objects::masters::MasterTables<'a>,
    buildings: &'a crate::world::parts::PartTable<crate::objects::layers::Building>,
    combats: &'a crate::world::parts::PartTable<crate::objects::layers::Combat>,
    actions: &'a crate::world::parts::PartTable<crate::objects::layers::Action>,
    unread: &'a crate::world::parts::PartTable<crate::world::Unread>,
    motions: crate::world::motions::Lent<'a, crate::objects::layers::Motion>,
}

impl<'a> ObjectsRef<'a> {
    /// The objects of `players`, which the world's table `index` finds, with the players'
    /// `masters` and the objects' `parts` (their own masters, their building, combat and motion
    /// layers).
    #[must_use]
    pub fn new(
        players: &'a [Player],
        index: &'a ObjectIndex,
        masters: &'a [crate::objects::masters::Masters],
        parts: &'a crate::world::parts::Parts,
    ) -> Self {
        Self::of_tables(
            players,
            index,
            crate::objects::masters::MasterTables::new(masters, &parts.own_master),
            (
                &parts.building,
                &parts.combat,
                &parts.actions,
                &parts.unread,
                parts.motion.unlent(),
            ),
        )
    }

    /// [`ObjectsRef::new`] of the part tables it reads, for a caller that holds another part
    /// table to change: the world's `masters` (the players' and the objects' own), the building
    /// layers `buildings`, the combat layers `combats`, the action layers `actions`, the objects
    /// the load could not read whole `unread` and the motion layers `motions` (one of them lent
    /// out to a walk,
    /// [`crate::world::parts::PartTable::lend`]).
    #[must_use]
    pub const fn of_tables(
        players: &'a [Player],
        index: &'a ObjectIndex,
        masters: crate::objects::masters::MasterTables<'a>,
        (buildings, combats, actions, unread, motions): PartTables<'a>,
    ) -> Self {
        Self {
            players,
            index,
            masters,
            buildings,
            combats,
            actions,
            unread,
            motions,
        }
    }

    /// Object `id`'s mover; none for an object that does not move, one the world does not hold
    /// or one out for its update.
    #[must_use]
    pub fn mover_of(&self, id: impl ObjectKey) -> Option<&'a crate::movement::Mover> {
        let u = self.get(id)?;
        self.motions
            .get(u.id)
            .and_then(crate::objects::layers::Motion::mover)
    }

    /// Object `id`'s combat layer, an object out for its update's too: a new object's where
    /// the table holds none ([`crate::world::parts::Parts::combat_of`]).
    #[must_use]
    pub fn combat_of(&self, id: ObjectId) -> &'a crate::objects::layers::Combat {
        self.combats.of(id)
    }

    /// The world's masters these objects read: the players' tables and the objects' own.
    #[must_use]
    pub const fn masters(&self) -> crate::objects::masters::MasterTables<'a> {
        self.masters
    }

    /// No objects: a test's walk that meets none by id.
    #[cfg(test)]
    pub(crate) fn empty() -> ObjectsRef<'static> {
        ObjectsRef::new(
            &[],
            ObjectIndex::scanning(),
            &[],
            crate::world::parts::no_parts(),
        )
    }

    /// Object `id`, live or sleeping.
    #[must_use]
    pub fn get(&self, id: impl ObjectKey) -> Option<&'a Unit> {
        self.index.locate(self.players, id).map(|(_, u, _)| u)
    }

    /// Object `id`, live or sleeping, with its owner and its master ([`Unit::master`]); none
    /// for an object the world does not hold or one out for its update.
    ///
    /// # Errors
    ///
    /// A master its owner's table does not hold.
    pub fn with_master(
        &self,
        id: impl ObjectKey,
    ) -> Result<Option<(PlayerId, &'a Unit, &'a Master)>, GenieError> {
        let Some((p, u, _)) = self.index.locate(self.players, id) else {
            return Ok(None);
        };
        Ok(Some((p, u, u.master(self.masters, p)?)))
    }

    /// What a walk reads of the moving object `id` ([`Unit::gait`]); none for an object without
    /// movement state, one the world does not hold or one out for its update.
    #[must_use]
    pub fn gait(&self, id: impl ObjectKey) -> Option<crate::map::spatial::Gait> {
        let u = self.get(id)?;
        u.gait(
            self.actions.of(u.id),
            self.motions
                .get(u.id)
                .and_then(crate::objects::layers::Motion::mover),
        )
    }

    /// The building layer of object `id` ([`crate::world::parts::Parts::building_of`]); none
    /// for an object the world does not hold or one out for its update.
    #[must_use]
    pub fn building(&self, id: impl ObjectKey) -> Option<&'a crate::objects::layers::Building> {
        Some(self.buildings.of(self.get(id)?.id))
    }

    /// The action layer of object `id` ([`crate::world::parts::Parts::actions_of`]); none for
    /// an object the world does not hold or one out for its update.
    #[must_use]
    pub fn actions(&self, id: impl ObjectKey) -> Option<&'a crate::objects::layers::Action> {
        Some(self.actions.of(self.get(id)?.id))
    }

    /// Why the load could not read object `id` whole ([`crate::world::Unread`]), none for one it
    /// read. The updating object's too: its entry stays in the table while its row is out.
    #[must_use]
    pub fn unread(&self, id: ObjectId) -> Option<&'a crate::world::Unread> {
        self.unread.get(id)
    }

    /// The main building of the annex `id` (its `+0xbb8`); none for an object that is no annex,
    /// one the world does not hold or one out for its update.
    #[must_use]
    pub fn annex_of(&self, id: impl ObjectKey) -> Option<crate::construction::MainBuildingId> {
        self.buildings.get(self.get(id)?.id)?.annex_of
    }

    /// The annexes of the building `main` ([`crate::construction::Annexes`]): none for an
    /// object the world does not hold or one out for its update, which reads its own.
    #[must_use]
    pub fn annexes(&self, main: impl ObjectKey) -> crate::construction::Annexes {
        self.get(main)
            .and_then(|u| self.buildings.get(u.id))
            .map_or(crate::construction::Annexes::NONE, |b| b.annexes)
    }

    /// The obstruction query's ignore list for `ids` (−1 for none), as `FUN_14114e8a0` adds each
    /// object: its id, then a building's annexes (`+0xbc0`..`+0xbd8`, in slot order).
    #[must_use]
    pub fn query_ignore(&self, ids: &[Option<ObjectId>]) -> Vec<crate::map::spatial::RecordId> {
        let mut ignore = Vec::new();
        for &id in ids.iter().flatten() {
            ignore.push(crate::map::spatial::RecordId::from(id));
            ignore.extend(
                self.annexes(id)
                    .ids()
                    .map(crate::map::spatial::RecordId::from),
            );
        }
        ignore
    }

    /// Whether object `id` moves now ([`crate::map::spatial::Gait::moving`]).
    #[must_use]
    pub fn moving(&self, id: impl ObjectKey) -> bool {
        self.gait(id).is_some_and(|g| g.moving)
    }
}

/// A test's objects by id without a world ([`ObjectsRef`]): one player's live list of units, each
/// built for the gait a walk reads of it.
#[cfg(test)]
#[derive(Clone, Debug, Default)]
pub(crate) struct TestObjects {
    players: Vec<Player>,
    index: ObjectIndex,
    masters: Vec<crate::objects::masters::Masters>,
    parts: crate::world::parts::Parts,
}

#[cfg(test)]
impl TestObjects {
    /// Unit `id` gives `gait` ([`crate::movement::tests::unit_with_gait`]), its building layer
    /// kept; none leaves no unit of the id (nor its layer).
    pub(crate) fn set_gait(&mut self, id: ObjectId, gait: Option<crate::map::spatial::Gait>) {
        if self.players.is_empty() {
            self.players.push(Player::default());
        }
        let Some(player) = self.players.first_mut() else {
            return;
        };
        let made = gait.map(|g| {
            let o = crate::movement::tests::unit_with_gait(id, g);
            let _ = self
                .parts
                .motion
                .insert(id, o.parts.motion.expect("a walker's motion part"));
            o.unit
        });
        if made.is_none() {
            let _ = self.parts.building.remove(id);
            let _ = self.parts.motion.remove(id);
        }
        player.objects = player
            .objects
            .iter()
            .filter(|u| u.id != id)
            .cloned()
            .chain(made)
            .collect();
        self.index = ObjectIndex::of(&self.players);
    }

    /// A test's link of `annex` to the building `main`: the annex's `+0xbb8` and the building's
    /// first empty slot, each a unit of its own (without movement state) if none yet.
    pub(crate) fn link_annex(&mut self, annex: ObjectId, main: ObjectId) {
        if self.players.is_empty() {
            self.players.push(Player::default());
        }
        let Some(player) = self.players.first_mut() else {
            return;
        };
        for id in [annex, main] {
            if player.objects.position_of(id).is_none() {
                player.objects.push(Unit {
                    id,
                    ..Unit::default()
                });
            }
        }
        let annexed = self
            .parts
            .building_mut(annex)
            .expect("a test's annex layer");
        annexed.annex_of = Some(crate::construction::MainBuildingId::new(main));
        let building = self
            .parts
            .building_mut(main)
            .expect("a test's building layer");
        let slot = building.annexes.ids().count();
        let _ = building.annexes.link(slot, annex);
        self.index = ObjectIndex::of(&self.players);
    }

    /// The units by id.
    pub(crate) fn view(&self) -> ObjectsRef<'_> {
        ObjectsRef::new(&self.players, &self.index, &self.masters, &self.parts)
    }
}

#[cfg(test)]
impl World {
    /// A test's link of `annex` to the building `main`: the annex's `+0xbb8` and the building's
    /// first empty slot, as far as the world holds each.
    pub(crate) fn link_test_annex(&mut self, annex: ObjectId, main: ObjectId) {
        if let Ok(Some((_, a))) = self.object_and_layer_mut(annex) {
            a.annex_of = Some(crate::construction::MainBuildingId::new(main));
        }
        if let Ok(Some((_, m))) = self.object_and_layer_mut(main) {
            let slot = m.annexes.ids().count();
            let _ = m.annexes.link(slot, annex);
        }
    }
}

/// [`ObjectIndex::place`] by the lists themselves, the first slot of the first player first.
fn scan(players: &[Player], id: ObjectId) -> Option<Place> {
    PlayerId::each(players.len())
        .zip(players)
        .find_map(|(p, player)| {
            if let Some(i) = player.objects.position_of(id) {
                return Some(Place::Live(p, i));
            }
            let i = player.sleeping.position_of(id)?;
            Some(Place::Sleeping(p, i))
        })
}

/// A player's list, by the place's kind.
const fn list(player: &Player, sleeping: bool) -> &ObjectList {
    if sleeping {
        &player.sleeping
    } else {
        &player.objects
    }
}

impl World {
    /// The objects by id, to read ([`ObjectsRef`]).
    #[must_use]
    pub fn objects(&self) -> ObjectsRef<'_> {
        ObjectsRef::new(&self.players, &self.index, &self.masters, &self.parts)
    }

    /// The table by id again from the players' lists (after a list changed in place).
    pub fn reindex(&mut self) {
        self.index = ObjectIndex::of(&self.players);
    }

    /// Where object `id` is, live or sleeping; none for an id the world does not hold or an
    /// object out for its update.
    pub(crate) fn place(&self, id: impl ObjectKey) -> Option<Place> {
        self.index.place(&self.players, id)
    }

    /// Object `id`'s slot in player `p`'s live list, if it is there.
    pub(crate) fn live_index(&self, p: PlayerId, id: impl ObjectKey) -> Option<ListIndex> {
        match self.place(id)? {
            Place::Live(q, i) if q == p => Some(i),
            _ => None,
        }
    }

    /// Player `p`'s live object `id`, if its live list holds it.
    pub(crate) fn live_object(&self, p: PlayerId, id: impl ObjectKey) -> Option<&Unit> {
        let i = self.live_index(p, id)?;
        self.players.live(p, i).ok()
    }

    /// Player `p`'s live object `id` to change, if its live list holds it.
    pub(crate) fn live_object_mut(&mut self, p: PlayerId, id: impl ObjectKey) -> Option<&mut Unit> {
        let i = self.live_index(p, id)?;
        self.players.live_mut(p, i).ok()
    }

    /// `object` goes to the end of player `p`'s live list, its parts into the world's tables;
    /// returns its slot.
    ///
    /// # Errors
    ///
    /// A player the world does not hold.
    pub fn push_live(
        &mut self,
        p: PlayerId,
        object: impl Into<Object>,
    ) -> Result<ListIndex, GenieError> {
        let Object { unit, parts } = object.into();
        let list = &mut self.players.player_mut(p)?.objects;
        self.parts.file(unit.id, parts);
        Ok(self
            .index
            .push(list, Place::Live(p, ListIndex::new(0)), unit))
    }

    /// `object` goes to the end of player `p`'s sleeping list, its parts into the world's
    /// tables; returns its slot.
    ///
    /// # Errors
    ///
    /// A player the world does not hold.
    pub fn push_asleep(
        &mut self,
        p: PlayerId,
        object: impl Into<Object>,
    ) -> Result<ListIndex, GenieError> {
        let Object { unit, parts } = object.into();
        let list = &mut self.players.player_mut(p)?.sleeping;
        self.parts.file(unit.id, parts);
        Ok(self
            .index
            .push(list, Place::Sleeping(p, ListIndex::new(0)), unit))
    }

    /// The objects of `objects` go to the end of player `p`'s live list, in their order.
    ///
    /// # Errors
    ///
    /// A player the world does not hold.
    pub fn extend_live<O: Into<Object>>(
        &mut self,
        p: PlayerId,
        objects: impl IntoIterator<Item = O>,
    ) -> Result<(), GenieError> {
        for object in objects {
            let _ = self.push_live(p, object)?;
        }
        Ok(())
    }

    /// The object in slot `i` of player `p`'s live list leaves it; the last one takes its slot.
    ///
    /// # Errors
    ///
    /// A player or a slot the world does not hold.
    pub fn swap_remove_live(&mut self, p: PlayerId, i: ListIndex) -> Result<Object, GenieError> {
        let list = &mut self.players.player_mut(p)?.objects;
        let unit = self.index.swap_remove(list, Place::Live(p, i))?;
        Ok(self.whole_from(unit))
    }

    /// The object in slot `i` of player `p`'s sleeping list leaves it; the last one takes its
    /// slot.
    ///
    /// # Errors
    ///
    /// A player or a slot the world does not hold.
    pub fn swap_remove_asleep(&mut self, p: PlayerId, i: ListIndex) -> Result<Object, GenieError> {
        let list = &mut self.players.player_mut(p)?.sleeping;
        let unit = self.index.swap_remove(list, Place::Sleeping(p, i))?;
        Ok(self.whole_from(unit))
    }

    /// The object in slot `i` of player `p`'s live list leaves it; the ones after it move up.
    ///
    /// # Errors
    ///
    /// A player or a slot the world does not hold.
    pub fn remove_live(&mut self, p: PlayerId, i: ListIndex) -> Result<Object, GenieError> {
        let list = &mut self.players.player_mut(p)?.objects;
        let unit = self.index.remove(list, Place::Live(p, i))?;
        Ok(self.whole_from(unit))
    }

    /// `unit`, which left its list, with its parts taken out of the world's tables.
    fn whole_from(&mut self, unit: Unit) -> Object {
        let parts = self.parts.take(unit.id);
        Object { unit, parts }
    }

    /// The object in slot `i` of player `p`'s live list leaves its slot for a while (its update,
    /// a call on it); its stand-in holds the slot, which the list's removals keep track of. The
    /// table keeps its place, which answers none while the stand-in holds it.
    ///
    /// # Errors
    ///
    /// A player or a slot the world does not hold.
    pub fn take_out(&mut self, p: PlayerId, i: ListIndex) -> Result<Unit, GenieError> {
        self.players.player_mut(p)?.objects.take_out(i)
    }

    /// `unit`, which [`World::take_out`] took from player `p`'s live list, goes back to the slot
    /// its stand-in holds now: that slot.
    ///
    /// # Errors
    ///
    /// The unit, when its stand-in left the list (or it was never taken out), or the player is
    /// not the world's.
    pub fn put_back(&mut self, p: PlayerId, unit: Unit) -> Result<ListIndex, Box<Unit>> {
        let Some(player) = self.players.get_mut(p.index()) else {
            return Err(Box::new(unit));
        };
        self.index.put_back(&mut player.objects, p, unit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_ids::object_id;
    use crate::world::FrameSeed;
    use crate::world::Millis;
    use crate::world::WorldTime;

    fn unit(id: i32) -> Unit {
        Unit {
            id: object_id(id),
            ..Unit::default()
        }
    }

    /// A world of `n` players whose live lists hold `live` (by player) and nothing asleep.
    fn world(live: &[&[i32]]) -> World {
        let players = live
            .iter()
            .map(|ids| Player {
                objects: ids.iter().map(|&id| unit(id)).collect(),
                ..Player::default()
            })
            .collect();
        World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO; live.len()],
            players,
        )
    }

    fn live(p: u8, i: usize) -> Place {
        Place::Live(PlayerId::new(p), ListIndex::new(i))
    }

    fn asleep(p: u8, i: usize) -> Place {
        Place::Sleeping(PlayerId::new(p), ListIndex::new(i))
    }

    /// The walk's view by id: each unit's gait as the unit gives it ([`Unit::gait`]); none for
    /// an id the world does not hold, a unit without movement state, and a unit out for its
    /// update.
    #[test]
    fn the_objects_view_reads_each_unit_gait_by_id() {
        use crate::map::spatial::Gait;
        let mut t = world(&[&[1, 2]]);
        let walking = Gait {
            moving: true,
            last_move: WorldTime::from_millis(70),
            ..Gait::default()
        };
        let p = PlayerId::new(0);
        let _ = t
            .push_live(
                p,
                crate::movement::tests::unit_with_gait(object_id(3), walking),
            )
            .unwrap();
        assert_eq!(t.objects().gait(object_id(3)), Some(walking));
        assert!(t.objects().moving(object_id(3)));
        assert_eq!(t.objects().gait(object_id(1)), None, "no movement state");
        assert!(!t.objects().moving(object_id(9)), "not held");
        let i = t.live_index(p, object_id(3)).unwrap();
        let out = t.take_out(p, i).unwrap();
        assert_eq!(t.objects().gait(object_id(3)), None, "out for its update");
        let _ = t.put_back(p, out).unwrap();
        assert!(t.objects().moving(object_id(3)));
    }

    #[test]
    fn the_table_finds_each_object_of_the_players_lists() {
        let mut t = world(&[&[1, 2], &[3]]);
        assert_eq!(t.place(object_id(2)), Some(live(0, 1)));
        assert_eq!(t.place(object_id(3)), Some(live(1, 0)));
        assert_eq!(t.place(object_id(9)), None);
        let i = t.push_asleep(PlayerId::new(1), unit(4)).unwrap();
        assert_eq!(
            (i, t.place(object_id(4))),
            (ListIndex::new(0), Some(asleep(1, 0)))
        );
        assert_eq!(
            t.live_index(PlayerId::new(1), object_id(4)),
            None,
            "asleep, not live"
        );
        assert_eq!(
            t.live_index(PlayerId::new(0), object_id(3)),
            None,
            "another player's"
        );
        assert_eq!(
            t.live_index(PlayerId::new(1), object_id(3)),
            Some(ListIndex::new(0))
        );
        assert_eq!(
            t.locate(object_id(4)).map(|(p, u, s)| (p, u.id, s)),
            Some((PlayerId::new(1), object_id(4), true))
        );
    }

    #[test]
    fn swap_remove_files_the_object_that_took_the_slot() {
        let mut t = world(&[&[1, 2, 3]]);
        let gone = t
            .swap_remove_live(PlayerId::new(0), ListIndex::new(0))
            .unwrap();
        assert_eq!(gone.unit.id, object_id(1));
        assert_eq!(t.place(object_id(1)), None);
        assert_eq!(t.place(object_id(3)), Some(live(0, 0)));
        assert_eq!(t.place(object_id(2)), Some(live(0, 1)));
        let gone = t
            .swap_remove_live(PlayerId::new(0), ListIndex::new(1))
            .unwrap();
        assert_eq!(
            (gone.unit.id, t.place(object_id(3))),
            (object_id(2), Some(live(0, 0)))
        );
        assert!(
            t.swap_remove_live(PlayerId::new(0), ListIndex::new(4))
                .is_err()
        );
        assert!(
            t.swap_remove_live(PlayerId::new(5), ListIndex::new(0))
                .is_err()
        );
    }

    #[test]
    fn remove_moves_the_later_objects_up_in_the_table() {
        let mut t = world(&[&[1, 2, 3]]);
        let gone = t.remove_live(PlayerId::new(0), ListIndex::new(0)).unwrap();
        assert_eq!(gone.unit.id, object_id(1));
        assert_eq!(
            (t.place(object_id(2)), t.place(object_id(3))),
            (Some(live(0, 0)), Some(live(0, 1)))
        );
    }

    #[test]
    fn an_object_out_for_its_update_is_none_until_it_goes_back() {
        let mut t = world(&[&[1, 2, 3]]);
        let p = PlayerId::new(0);
        let out = t.take_out(p, ListIndex::new(0)).unwrap();
        assert_eq!(t.place(object_id(1)), None, "its stand-in holds the slot");
        // another object leaves: the last one takes its slot
        let _ = t.swap_remove_live(p, ListIndex::new(1)).unwrap();
        assert_eq!(t.place(object_id(3)), Some(live(0, 1)));
        assert_eq!(t.put_back(p, out).ok(), Some(ListIndex::new(0)));
        assert_eq!(t.place(object_id(1)), Some(live(0, 0)));
        // a stand-in that moves with a swap remove: its object goes back where it went
        let out = t.take_out(p, ListIndex::new(1)).unwrap();
        let _ = t.swap_remove_live(p, ListIndex::new(0)).unwrap();
        assert_eq!(t.put_back(p, out).ok(), Some(ListIndex::new(0)));
        assert_eq!(t.place(object_id(3)), Some(live(0, 0)));
    }

    #[test]
    fn an_object_whose_stand_in_left_leaves_the_table_for_good() {
        let mut t = world(&[&[1, 2]]);
        let p = PlayerId::new(0);
        let out = t.take_out(p, ListIndex::new(0)).unwrap();
        assert_eq!(
            t.swap_remove_live(p, ListIndex::new(0)).unwrap().unit.id,
            TAKEN_OUT
        );
        assert!(t.put_back(p, out).is_err());
        // its id comes again (a recycled missile's): filed once, the table still answers
        let _ = t.push_live(p, unit(1)).unwrap();
        assert!(!t.index.scans());
        assert_eq!(t.place(object_id(1)), Some(live(0, 1)));
    }

    #[test]
    fn lists_that_hold_an_id_twice_answer_by_scanning() {
        let t = world(&[&[5], &[0, 5]]);
        assert!(t.index.scans());
        assert_eq!(
            t.place(object_id(5)),
            Some(live(0, 0)),
            "the first player's"
        );
        assert_eq!(t.place(object_id(0)), Some(live(1, 0)));
    }

    #[test]
    fn list_changed_in_place_answers_once_reindexed() {
        let mut t = world(&[&[1, 2]]);
        t.players[0].objects.push(unit(7));
        assert_eq!(
            t.place(object_id(7)),
            None,
            "the table does not know a raw push"
        );
        t.reindex();
        assert_eq!(t.place(object_id(7)), Some(live(0, 2)));
    }
}
