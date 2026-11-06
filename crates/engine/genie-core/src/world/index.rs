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

use crate::GenieResult;
#[cfg(test)]
use crate::construction::MainBuildingId;
#[cfg(test)]
use crate::map::spatial::Gait;
#[cfg(test)]
use crate::movement::tests::unit_with_gait;
use crate::objects::IdTable;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectKey;
#[cfg(test)]
use crate::objects::masters::Masters;
use crate::objects::object_list::ObjectList;
use crate::objects::unit::{TAKEN_OUT, Unit};
use crate::players::PlayerId;
#[cfg(test)]
use crate::testing::static_roles;
#[cfg(test)]
use crate::testing::tuning;
use crate::world::World;
#[cfg(test)]
use crate::world::layer::Roles;
use crate::world::objects_ref::ObjectsRef;
use crate::world::parts::Object;
#[cfg(test)]
use crate::world::parts::Parts;
use crate::world::player::Player;
use crate::world::slots::PlayerSlotsExt;
#[cfg(test)]
use std::sync::Arc;

/// Which of a player's object lists: the live one or the sleeping one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ListKind {
    /// The live list (`+0x30`).
    Live,
    /// The sleeping list.
    Sleeping,
}

/// Where an object is: its player and its index in the live or the sleeping list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Place {
    Live(PlayerId, ListIndex),
    Sleeping(PlayerId, ListIndex),
}

impl Place {
    /// Slot `list_index` of player `player_id`'s list of `kind`.
    pub(crate) const fn of(player_id: PlayerId, kind: ListKind, list_index: ListIndex) -> Self {
        match kind {
            ListKind::Sleeping => Self::Sleeping(player_id, list_index),
            ListKind::Live => Self::Live(player_id, list_index),
        }
    }

    /// The list the slot is in.
    pub(crate) const fn list(self) -> ListKind {
        match self {
            Self::Live(..) => ListKind::Live,
            Self::Sleeping(..) => ListKind::Sleeping,
        }
    }

    /// The object's player (`+0x138`'s number).
    pub(crate) const fn player(self) -> PlayerId {
        match self {
            Self::Live(player_id, _) | Self::Sleeping(player_id, _) => player_id,
        }
    }

    /// The slot in its list.
    pub(crate) const fn slot(self) -> ListIndex {
        match self {
            Self::Live(_, list_index) | Self::Sleeping(_, list_index) => list_index,
        }
    }

    /// Whether the slot is in a sleeping list.
    pub(crate) const fn sleeping(self) -> bool {
        matches!(self, Self::Sleeping(..))
    }

    /// Slot `list_index` of the same list.
    const fn at(self, list_index: ListIndex) -> Self {
        Self::of(self.player(), self.list(), list_index)
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
        for (player_id, player) in PlayerId::each(players.len()).zip(players) {
            for (list_index, unit) in player.objects.indexed() {
                index.file(unit.id, Place::Live(player_id, list_index));
            }
            for (list_index, unit) in player.sleeping.indexed() {
                index.file(unit.id, Place::Sleeping(player_id, list_index));
            }
        }
        index
    }

    /// A table that never answers, so that every lookup scans: the view of lists no world holds
    /// (a test's).
    #[cfg(test)]
    pub(crate) const fn scanning() -> &'static Self {
        // A constant that holds a `Vec` is not promoted: the inline `const` lends it for
        // `'static`.
        const {
            &Self {
                places: IdTable::new(),
                twice: true,
            }
        }
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
        list(player, place.list())
            .get(place.slot())
            .is_some_and(|unit| unit.id == id)
            .then_some(place)
    }

    /// Object `id` among `players`' lists with its owner and whether it sleeps.
    pub(crate) fn locate<'world>(
        &self,
        players: &'world [Player],
        id: impl ObjectKey,
    ) -> Option<(PlayerId, &'world Unit, bool)> {
        let place = self.place(players, id)?;
        let unit = players.at(place).ok()?;
        Some((place.player(), unit, place.sleeping()))
    }

    /// `unit` goes to the end of `list`, the list of `at`'s player and kind; returns its slot.
    pub(crate) fn push(&mut self, list: &mut ObjectList, at: Place, unit: Unit) -> ListIndex {
        let list_index = list.next_place();
        let id = unit.id;
        list.push(unit);
        self.file(id, at.at(list_index));
        list_index
    }

    /// The object at `place` leaves `list` (the list there); the last one takes its slot.
    ///
    /// # Errors
    ///
    /// The list holds no such slot.
    pub(crate) fn swap_remove(&mut self, list: &mut ObjectList, place: Place) -> GenieResult<Unit> {
        let list_index = place.slot();
        let out = list.out_at(list_index);
        let gone = list.swap_remove(list_index)?;
        self.left(gone.id, out, place);
        if let Some(moved) = list.get(list_index).map(|unit| unit.id) {
            self.refile(moved, place);
        }
        Ok(gone)
    }

    /// The object at `place` leaves `list` (the list there); the ones after it move up.
    ///
    /// # Errors
    ///
    /// The list holds no such slot.
    pub(crate) fn remove(&mut self, list: &mut ObjectList, place: Place) -> GenieResult<Unit> {
        let slot = place.slot();
        let out = list.out_at(slot);
        let gone = list.remove(slot)?;
        self.left(gone.id, out, place);
        for (list_index, unit) in list.indexed().skip(slot.get()) {
            self.refile(unit.id, place.at(list_index));
        }
        Ok(gone)
    }

    /// `unit`, which `list` (player `player_id`'s live list) let out, goes back to the slot its
    /// stand-in holds now: that slot.
    ///
    /// # Errors
    ///
    /// The unit, when its stand-in left the list (or it was never taken out).
    pub(crate) fn put_back(
        &mut self,
        list: &mut ObjectList,
        player_id: PlayerId,
        unit: Unit,
    ) -> Result<ListIndex, Box<Unit>> {
        let id = unit.id;
        let list_index = list.put_back(unit)?;
        self.refile(id, Place::Live(player_id, list_index));
        Ok(list_index)
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

/// A test's objects by id without a world ([`ObjectsRef`]): one player's live list of units, each
/// built for the gait a walk reads of it.
#[cfg(test)]
#[derive(Clone, Debug, Default)]
pub(crate) struct TestObjects {
    players: Vec<Player>,
    index: ObjectIndex,
    masters: Vec<Arc<Masters>>,
    parts: Parts,
}

#[cfg(test)]
impl TestObjects {
    /// Unit `id` gives `gait` ([`crate::movement::tests::unit_with_gait`]), its building layer
    /// kept; none leaves no unit of the id (nor its layer).
    pub(crate) fn set_gait(&mut self, id: ObjectId, gait: Option<Gait>) {
        if self.players.is_empty() {
            self.players.push(Player::default());
        }
        let Some(player) = self.players.first_mut() else {
            return;
        };
        let made = gait.map(|gait| {
            let object = unit_with_gait(id, gait);
            let _ = self
                .parts
                .motion
                .insert(id, object.parts.motion.expect("a walker's motion part"));
            object.unit
        });
        if made.is_none() {
            let _ = self.parts.building.remove(id);
            let _ = self.parts.motion.remove(id);
        }
        player.objects = player
            .objects
            .iter()
            .filter(|unit| unit.id != id)
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
        annexed.annex_of = Some(MainBuildingId::new(main));
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
        self.view_with(static_roles())
    }

    /// The units by id under the layer's `roles` in place of build 158041's.
    pub(crate) fn view_with<'world>(&'world self, roles: &'world Roles) -> ObjectsRef<'world> {
        ObjectsRef::new(
            &self.players,
            &self.index,
            &self.masters,
            &self.parts,
            roles,
            tuning(),
        )
    }
}

#[cfg(test)]
impl World {
    /// A test's link of `annex` to the building `main`: the annex's `+0xbb8` and the building's
    /// first empty slot, as far as the world holds each.
    pub(crate) fn link_test_annex(&mut self, annex: ObjectId, main: ObjectId) {
        if let Ok(Some((_, annex_building))) = self.object_and_layer_mut(annex) {
            annex_building.annex_of = Some(MainBuildingId::new(main));
        }
        if let Ok(Some((_, building))) = self.object_and_layer_mut(main) {
            let slot = building.annexes.ids().count();
            let _ = building.annexes.link(slot, annex);
        }
    }
}

/// [`ObjectIndex::place`] by the lists themselves, the first slot of the first player first.
fn scan(players: &[Player], id: ObjectId) -> Option<Place> {
    PlayerId::each(players.len())
        .zip(players)
        .find_map(|(player_id, player)| {
            if let Some(list_index) = player.objects.position_of(id) {
                return Some(Place::Live(player_id, list_index));
            }
            let list_index = player.sleeping.position_of(id)?;
            Some(Place::Sleeping(player_id, list_index))
        })
}

/// A player's list of `kind`.
const fn list(player: &Player, kind: ListKind) -> &ObjectList {
    match kind {
        ListKind::Sleeping => &player.sleeping,
        ListKind::Live => &player.objects,
    }
}

impl World {
    /// The objects by id, to read ([`ObjectsRef`]).
    #[must_use]
    pub fn objects(&self) -> ObjectsRef<'_> {
        ObjectsRef::new(
            &self.players,
            &self.index,
            &self.model.masters,
            &self.parts,
            &self.model.roles,
            &self.model.tuning,
        )
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

    /// Object `id`'s slot in player `owner`'s live list, if it is there.
    pub(crate) fn live_index(&self, owner: PlayerId, id: impl ObjectKey) -> Option<ListIndex> {
        match self.place(id)? {
            Place::Live(player_id, list_index) if player_id == owner => Some(list_index),
            _ => None,
        }
    }

    /// Player `player_id`'s live object `id`, if its live list holds it.
    pub(crate) fn live_object(&self, player_id: PlayerId, id: impl ObjectKey) -> Option<&Unit> {
        let list_index = self.live_index(player_id, id)?;
        self.players.live(player_id, list_index).ok()
    }

    /// Player `player_id`'s live object `id` to change, if its live list holds it.
    pub(crate) fn live_object_mut(
        &mut self,
        player_id: PlayerId,
        id: impl ObjectKey,
    ) -> Option<&mut Unit> {
        let list_index = self.live_index(player_id, id)?;
        self.players.live_mut(player_id, list_index).ok()
    }

    /// `object` goes to the end of player `player_id`'s live list, its parts into the world's
    /// tables; returns its slot.
    ///
    /// # Errors
    ///
    /// A player the world does not hold.
    pub fn push_live(
        &mut self,
        player_id: PlayerId,
        object: impl Into<Object>,
    ) -> GenieResult<ListIndex> {
        let Object { unit, parts } = object.into();
        let list = &mut self.players.player_mut(player_id)?.objects;
        self.parts.file(unit.id, parts);
        Ok(self
            .index
            .push(list, Place::Live(player_id, ListIndex::new(0)), unit))
    }

    /// `object` goes to the end of player `player_id`'s sleeping list, its parts into the world's
    /// tables; returns its slot.
    ///
    /// # Errors
    ///
    /// A player the world does not hold.
    pub fn push_asleep(
        &mut self,
        player_id: PlayerId,
        object: impl Into<Object>,
    ) -> GenieResult<ListIndex> {
        let Object { unit, parts } = object.into();
        let list = &mut self.players.player_mut(player_id)?.sleeping;
        self.parts.file(unit.id, parts);
        Ok(self
            .index
            .push(list, Place::Sleeping(player_id, ListIndex::new(0)), unit))
    }

    /// The objects of `objects` go to the end of player `player_id`'s live list, in their order.
    ///
    /// # Errors
    ///
    /// A player the world does not hold.
    pub fn extend_live<O: Into<Object>>(
        &mut self,
        player_id: PlayerId,
        objects: impl IntoIterator<Item = O>,
    ) -> GenieResult<()> {
        for object in objects {
            let _ = self.push_live(player_id, object)?;
        }
        Ok(())
    }

    /// The object in slot `list_index` of player `player_id`'s live list leaves it; the last one
    /// takes its slot.
    ///
    /// # Errors
    ///
    /// A player or a slot the world does not hold.
    pub fn swap_remove_live(
        &mut self,
        player_id: PlayerId,
        list_index: ListIndex,
    ) -> GenieResult<Object> {
        let list = &mut self.players.player_mut(player_id)?.objects;
        let unit = self
            .index
            .swap_remove(list, Place::Live(player_id, list_index))?;
        Ok(self.whole_from(unit))
    }

    /// The object in slot `list_index` of player `player_id`'s sleeping list leaves it; the last
    /// one takes its slot.
    ///
    /// # Errors
    ///
    /// A player or a slot the world does not hold.
    pub fn swap_remove_asleep(
        &mut self,
        player_id: PlayerId,
        list_index: ListIndex,
    ) -> GenieResult<Object> {
        let list = &mut self.players.player_mut(player_id)?.sleeping;
        let unit = self
            .index
            .swap_remove(list, Place::Sleeping(player_id, list_index))?;
        Ok(self.whole_from(unit))
    }

    /// The object in slot `list_index` of player `player_id`'s live list leaves it; the ones after
    /// it move up.
    ///
    /// # Errors
    ///
    /// A player or a slot the world does not hold.
    pub fn remove_live(
        &mut self,
        player_id: PlayerId,
        list_index: ListIndex,
    ) -> GenieResult<Object> {
        let list = &mut self.players.player_mut(player_id)?.objects;
        let unit = self
            .index
            .remove(list, Place::Live(player_id, list_index))?;
        Ok(self.whole_from(unit))
    }

    /// `unit`, which left its list, with its parts taken out of the world's tables.
    fn whole_from(&mut self, unit: Unit) -> Object {
        let parts = self.parts.take(unit.id);
        Object { unit, parts }
    }

    /// The object in slot `list_index` of player `player_id`'s live list leaves its slot for a
    /// while (its update, a call on it); its stand-in holds the slot, which the list's removals
    /// keep track of. The table keeps its place, which answers none while the stand-in holds it.
    ///
    /// # Errors
    ///
    /// A player or a slot the world does not hold.
    pub fn take_out(&mut self, player_id: PlayerId, list_index: ListIndex) -> GenieResult<Unit> {
        self.players
            .player_mut(player_id)?
            .objects
            .take_out(list_index)
    }

    /// `unit`, which [`World::take_out`] took from player `player_id`'s live list, goes back to the
    /// slot its stand-in holds now: that slot.
    ///
    /// # Errors
    ///
    /// The unit, when its stand-in left the list (or it was never taken out), or the player is
    /// not the world's.
    pub fn put_back(&mut self, player_id: PlayerId, unit: Unit) -> Result<ListIndex, Box<Unit>> {
        let Some(player) = self.players.get_mut(player_id.index()) else {
            return Err(Box::new(unit));
        };
        self.index.put_back(&mut player.objects, player_id, unit)
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

    /// A world whose live lists hold `live` (one list a player) and nothing asleep.
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

    fn live(player: u8, index: usize) -> Place {
        Place::Live(PlayerId::new(player), ListIndex::new(index))
    }

    fn asleep(player: u8, index: usize) -> Place {
        Place::Sleeping(PlayerId::new(player), ListIndex::new(index))
    }

    /// The walk's view by id: each unit's gait as the unit gives it ([`Unit::gait`]); none for
    /// an id the world does not hold, a unit without movement state, and a unit out for its
    /// update.
    #[test]
    fn the_objects_view_reads_each_unit_gait_by_id() {
        use crate::map::spatial::Gait;
        let mut world = world(&[&[1, 2]]);
        let walking = Gait {
            moving: true,
            last_move: WorldTime::from_millis(70),
            ..Gait::default()
        };
        let player_id = PlayerId::new(0);
        let _ = world
            .push_live(player_id, unit_with_gait(object_id(3), walking))
            .unwrap();
        assert_eq!(world.objects().gait(object_id(3)), Some(walking));
        assert!(world.objects().moving(object_id(3)));
        assert_eq!(
            world.objects().gait(object_id(1)),
            None,
            "no movement state"
        );
        assert!(!world.objects().moving(object_id(9)), "not held");
        let list_index = world.live_index(player_id, object_id(3)).unwrap();
        let out = world.take_out(player_id, list_index).unwrap();
        assert_eq!(
            world.objects().gait(object_id(3)),
            None,
            "out for its update"
        );
        let _ = world.put_back(player_id, out).unwrap();
        assert!(world.objects().moving(object_id(3)));
    }

    #[test]
    fn the_table_finds_each_object_of_the_players_lists() {
        let mut world = world(&[&[1, 2], &[3]]);
        assert_eq!(world.place(object_id(2)), Some(live(0, 1)));
        assert_eq!(world.place(object_id(3)), Some(live(1, 0)));
        assert_eq!(world.place(object_id(9)), None);
        let list_index = world.push_asleep(PlayerId::new(1), unit(4)).unwrap();
        assert_eq!(
            (list_index, world.place(object_id(4))),
            (ListIndex::new(0), Some(asleep(1, 0)))
        );
        assert_eq!(
            world.live_index(PlayerId::new(1), object_id(4)),
            None,
            "asleep, not live"
        );
        assert_eq!(
            world.live_index(PlayerId::new(0), object_id(3)),
            None,
            "another player's"
        );
        assert_eq!(
            world.live_index(PlayerId::new(1), object_id(3)),
            Some(ListIndex::new(0))
        );
        assert_eq!(
            world
                .locate(object_id(4))
                .map(|(player_id, unit, sleeping)| (player_id, unit.id, sleeping)),
            Some((PlayerId::new(1), object_id(4), true))
        );
    }

    #[test]
    fn swap_remove_files_the_object_that_took_the_slot() {
        let mut world = world(&[&[1, 2, 3]]);
        let gone = world
            .swap_remove_live(PlayerId::new(0), ListIndex::new(0))
            .unwrap();
        assert_eq!(gone.unit.id, object_id(1));
        assert_eq!(world.place(object_id(1)), None);
        assert_eq!(world.place(object_id(3)), Some(live(0, 0)));
        assert_eq!(world.place(object_id(2)), Some(live(0, 1)));
        let gone = world
            .swap_remove_live(PlayerId::new(0), ListIndex::new(1))
            .unwrap();
        assert_eq!(
            (gone.unit.id, world.place(object_id(3))),
            (object_id(2), Some(live(0, 0)))
        );
        assert!(
            world
                .swap_remove_live(PlayerId::new(0), ListIndex::new(4))
                .is_err()
        );
        assert!(
            world
                .swap_remove_live(PlayerId::new(5), ListIndex::new(0))
                .is_err()
        );
    }

    #[test]
    fn remove_moves_the_later_objects_up_in_the_table() {
        let mut world = world(&[&[1, 2, 3]]);
        let gone = world
            .remove_live(PlayerId::new(0), ListIndex::new(0))
            .unwrap();
        assert_eq!(gone.unit.id, object_id(1));
        assert_eq!(
            (world.place(object_id(2)), world.place(object_id(3))),
            (Some(live(0, 0)), Some(live(0, 1)))
        );
    }

    #[test]
    fn an_object_out_for_its_update_is_none_until_it_goes_back() {
        let mut world = world(&[&[1, 2, 3]]);
        let player_id = PlayerId::new(0);
        let out = world.take_out(player_id, ListIndex::new(0)).unwrap();
        assert_eq!(
            world.place(object_id(1)),
            None,
            "its stand-in holds the slot"
        );
        // another object leaves: the last one takes its slot
        let _ = world
            .swap_remove_live(player_id, ListIndex::new(1))
            .unwrap();
        assert_eq!(world.place(object_id(3)), Some(live(0, 1)));
        assert_eq!(world.put_back(player_id, out).ok(), Some(ListIndex::new(0)));
        assert_eq!(world.place(object_id(1)), Some(live(0, 0)));
        // a stand-in that moves with a swap remove: its object goes back where it went
        let out = world.take_out(player_id, ListIndex::new(1)).unwrap();
        let _ = world
            .swap_remove_live(player_id, ListIndex::new(0))
            .unwrap();
        assert_eq!(world.put_back(player_id, out).ok(), Some(ListIndex::new(0)));
        assert_eq!(world.place(object_id(3)), Some(live(0, 0)));
    }

    #[test]
    fn an_object_whose_stand_in_left_leaves_the_table_for_good() {
        let mut world = world(&[&[1, 2]]);
        let player_id = PlayerId::new(0);
        let out = world.take_out(player_id, ListIndex::new(0)).unwrap();
        assert_eq!(
            world
                .swap_remove_live(player_id, ListIndex::new(0))
                .unwrap()
                .unit
                .id,
            TAKEN_OUT
        );
        assert!(world.put_back(player_id, out).is_err());
        // its id comes again (a recycled missile's): filed once, the table still answers
        let _ = world.push_live(player_id, unit(1)).unwrap();
        assert!(!world.index.scans());
        assert_eq!(world.place(object_id(1)), Some(live(0, 1)));
    }

    #[test]
    fn lists_that_hold_an_id_twice_answer_by_scanning() {
        let world = world(&[&[5], &[0, 5]]);
        assert!(world.index.scans());
        assert_eq!(
            world.place(object_id(5)),
            Some(live(0, 0)),
            "the first player's"
        );
        assert_eq!(world.place(object_id(0)), Some(live(1, 0)));
    }

    #[test]
    fn list_changed_in_place_answers_once_reindexed() {
        let mut world = world(&[&[1, 2]]);
        world.players[0].objects.push(unit(7));
        assert_eq!(
            world.place(object_id(7)),
            None,
            "the table does not know a raw push"
        );
        world.reindex();
        assert_eq!(world.place(object_id(7)), Some(live(0, 2)));
    }
}
