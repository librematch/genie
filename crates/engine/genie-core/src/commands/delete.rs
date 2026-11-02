//! A player's deletion of one of its objects: `CANCEL_BUILD` (0x6a, `TRIBE_Command::do_command`
//! case 0x6a), which the game's delete key sends.
//!
//! The command checks the object's owner and whether it may be deleted (`FUN_1416257c0`), then
//! runs its slot 0x8b8: a unit's (`FUN_14120f620`) takes its hit points (slot 0xd8), a building's
//! (`FUN_1411cd780` -> `FUN_1411ccbb0`) gives back what is left of a foundation's cost and takes
//! its hit points too. The object dies in its next update.

use crate::GenieError;
use crate::construction::build::held;
use crate::map::Tile;
use crate::objects::HitPoints;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::ClassId;
use crate::objects::masters::GhostMode;
use crate::objects::masters::Master;
use crate::objects::masters::MasterKind;
use crate::objects::masters::class;
use crate::objects::masters::ids::TOWN_CENTRE_621;
use crate::objects::masters::master_type::ACTION;
use crate::objects::masters::master_type::BUILDING;
use crate::objects::masters::master_type::COMBAT;
use crate::orders::object_gone;
use crate::ownership::CaptureKind;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::game_status::GameStatus;
use crate::world::Seconds;
use crate::world::World;
use crate::world::index::Place;
use crate::world::slots::PlayerSlots;

/// The classes no delete takes (`DAT_142f925c0`, and `FUN_14120f620`'s own test): livestock,
/// class 42, artifacts and class 60.
const UNDELETABLE: [ClassId; 4] = [
    class::LIVESTOCK,
    class::RELIC,
    class::ARTIFACT,
    class::CLASS_60,
];

/// The first master type a delete takes (`FUN_1416257c0`: above `0x45`).
const DELETABLE_TYPE: MasterKind = MasterKind::of_data(70);

/// `CANCEL_BUILD` (0x6a, `do_command` case 0x6a): the player deletes its object `id`
/// ([`World::delete`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeleteCommand {
    /// The player.
    pub player: PlayerId,
    /// The object.
    pub id: ObjectId,
}

impl DeleteCommand {
    /// Its handler ([`World::delete`]).
    pub(crate) fn run(self, w: &mut World) -> Result<(), GenieError> {
        w.delete(self.player, self.id)
    }
}

impl World {
    /// `CANCEL_BUILD` for `player` on object `id`: the object must exist and be the player's,
    /// unless the setting `+0x1074` lets players order each other's ([`crate::world::settings::Settings::cheats_enabled`])
    /// or its owner lost and the two are allies both ways (`FUN_14162ef10`). `FUN_1416257c0`
    /// then wants its deletion enabled (`+0x687`), a master of type 70 or more, a class other
    /// than [`UNDELETABLE`]'s and a capture flag (`+0x924`) other than 2; the object's slot 0x8b8
    /// follows ([`World::delete_unit`], [`World::delete_building`]). The bypass of the
    /// disable-verification cheat (`+0xb12`) is not held.
    ///
    /// # Errors
    ///
    /// What the deletes do not model.
    pub(crate) fn delete(&mut self, player: PlayerId, id: ObjectId) -> Result<(), GenieError> {
        let Some((owner, unit)) = self.object(id) else {
            return Ok(());
        };
        let mutual = |a: PlayerId, b: PlayerId| {
            self.spatial.allied_with(a, b) && self.spatial.allied_with(b, a)
        };
        // The buildings' master type, whose slot 0x8b8 is the building's.
        // The first master type whose slot 0xd0 is the action object's (`FUN_14106c420`), and the
        // TRIBE combat objects' type, whose slot 0xa0 is `FUN_14121cc40`.
        // A player's game status once it lost (`+0x89`).
        // A town centre's master whose delete also tells the world's `+0x820` (`FUN_1414615d0`).
        let lost_ally =
            self.players.player(owner)?.game_status == GameStatus::LOST && mutual(player, owner);
        if owner != player && !self.settings.cheats_enabled && !lost_ally {
            return Ok(());
        }
        let master = unit.master(self.master_tables(), owner)?;
        if unit.undeletable
            || master.kind < DELETABLE_TYPE
            || UNDELETABLE.contains(&master.class)
            || self.parts.combat_of(unit.id).capture_flag == CaptureKind::AGAIN
        {
            return Ok(());
        }
        if master.kind == BUILDING {
            self.delete_building(owner, id)
        } else {
            self.delete_unit(owner, id)
        }
    }

    /// A construction start's clearing (`FUN_1411cd0d0`) for the foundation `id`: the tiles from 5
    /// before its box to 5 past it (held on the map; rows, then columns), each tile's objects in
    /// its list's order: a desolid foundation (type 80, `+0xc60`) other than it whose owner sees
    /// and explored the tile ([`crate::map::spatial::Spatial::sees_and_explored`]) and whose box
    /// overlaps its own (on each axis the gap below the two radii summed) is removed
    /// ([`World::remove_building`]) and its `mROOAlreadyCalled` (`+0x686`) set.
    pub(crate) fn clear_foundations_under(&mut self, id: ObjectId) -> Result<(), GenieError> {
        const AROUND: i32 = 5;
        let Some(site) = self.spatial.get(id).copied() else {
            return Ok(());
        };
        let (x, y) = (site.position.x(), site.position.y());
        let (rx, ry) = (site.radius.x(), site.radius.y());
        let (w, h) = (self.spatial.map.width(), self.spatial.map.height());
        let (x0, y0) = (
            (((x - rx) as i32) - AROUND).max(0),
            (((y - ry) as i32) - AROUND).max(0),
        );
        let x1 = if ((x + rx) as i32 + AROUND) < w {
            (x + rx) as i32 + AROUND
        } else {
            w - 1
        };
        let y1 = if ((y + ry) as i32 + AROUND) < h {
            (y + ry) as i32 + AROUND
        } else {
            h - 1
        };
        let mut gone = Vec::new();
        for ty in y0..=y1 {
            for tx in x0..=x1 {
                for (oid, o) in self.spatial.objects_on(Tile::new(tx, ty)) {
                    if oid == id || gone.iter().any(|&(_, g)| g == oid) {
                        continue;
                    }
                    let Some((owner, u)) = self.object(oid) else {
                        continue;
                    };
                    let kind = u.master(self.master_tables(), owner)?.kind;
                    let desolid = self
                        .parts
                        .building_of(u.id)
                        .construction
                        .is_some_and(|c| c.desolid);
                    if kind != BUILDING
                        || !desolid
                        || !self.spatial.sees_and_explored(owner, Tile::new(tx, ty))
                    {
                        continue;
                    }
                    let dx = (o.position.x() - x).abs();
                    let dy = (o.position.y() - y).abs();
                    if dx < o.radius.x() + rx && dy < ry + o.radius.y() {
                        gone.push((owner, oid));
                    }
                }
            }
        }
        for (owner, oid) in gone {
            self.remove_building(owner, oid)?;
            // its record went with it (`+0x686` set)
            if let Some((_, u)) = self.object_mut(oid) {
                u.roo_called = true;
            }
        }
        Ok(())
    }

    /// A unit's slot 0x8b8 (`FUN_14120f620`): a unit of a class other than [`UNDELETABLE`]'s is
    /// marked deleted (`+0xa0c`) and its slot 0xd8 (`FUN_141211340`) spares class 42 and
    /// livestock holding some, else `FUN_14118f580`: a sleeping unit wakes to the end of its
    /// player's live list and its hit points go to 0. Not held: the deleted mark, the killer
    /// (`+0x690`, its owner) and the special-object list (`+0x30c` bit 0).
    fn delete_unit(&mut self, owner: PlayerId, id: ObjectId) -> Result<(), GenieError> {
        let (_, unit) = self.object(id).ok_or_else(|| object_gone(id))?;
        let class = unit.master(self.master_tables(), owner)?.class;
        if UNDELETABLE.contains(&class) {
            return Ok(());
        }
        // The class a unit's slot 0xd8 (`FUN_141211340`) spares, and the livestock it spares while it
        // holds some.
        if class == class::RELIC || (class == class::LIVESTOCK && Amount::ZERO < unit.life.amount) {
            return Ok(());
        }
        let i = self.wake(id)?;
        self.players.live_mut(owner, i)?.life.hp = HitPoints::ZERO;
        Ok(())
    }

    /// A building's slot 0x8b8 (`FUN_1411cd780`, then `FUN_1411ccbb0`, [`World::remove_building`]):
    /// it wakes; a foundation
    /// (below state 2) with at least 1 hit point gives its owner back the share of its paid cost
    /// (`+0x880`: each entry with a resource and the paid flag, slot 0xf8) its build time has
    /// still to go (`(build time - build points) / build time`, at most 1), all of it while no
    /// builder started it (`+0xc60`); then its hit points go to 0, and such a foundation is
    /// destroyed at once ([`World::destroy_foundation`]); master 621 clears its owner's treaty
    /// zones first ([`World::clear_treaty_zones`]). Not held: the deleted marks (`+0xa0c`, `+0xc`, `+0xb9c`), the
    /// semi-asleep list (`+0xc98`), the killer (`+0x690`) and a computer player's AI event (slot
    /// 0x380).
    fn delete_building(&mut self, owner: PlayerId, id: ObjectId) -> Result<(), GenieError> {
        let (_, unit) = self.object(id).ok_or_else(|| object_gone(id))?;
        let master = unit.master(self.master_tables(), owner)?;
        if master.id == TOWN_CENTRE_621 {
            self.clear_treaty_zones()?;
        }
        self.remove_building(owner, id)
    }

    /// `FUN_1411ccbb0`, the core of a building's delete ([`World::delete_building`]) and of a
    /// construction start's clearing ([`World::clear_foundations_under`]): the building wakes,
    /// a foundation's paid cost comes back by the share its build time has still to go, its hit
    /// points go to 0 and a desolid foundation is destroyed at once.
    pub(crate) fn remove_building(
        &mut self,
        owner: PlayerId,
        id: ObjectId,
    ) -> Result<(), GenieError> {
        let (_, unit) = self.object(id).ok_or(GenieError::unsupported(
            "a removed building the slice does not hold",
        ))?;
        let master = unit.master(self.master_tables(), owner)?;
        let cost = master.cost;
        let (state, hp) = (unit.life.state, unit.life.hp);
        let site = self.parts.building_of(unit.id).construction;
        let desolid = site.is_some_and(|c| c.desolid);
        let share = if state < ObjectState::Alive && HitPoints::new(1.0) <= hp {
            let c = site.ok_or(crate::construction::build::NO_CONSTRUCTION)?;
            let build_time = Seconds::of_master(c.build_time).get();
            (build_time - c.progress.get()) / build_time
        } else {
            0.0
        };
        // at most 1 (a NaN keeps the 1)
        let mut share = if share <= 1.0 { share } else { 1.0 };
        if desolid && HitPoints::ZERO < hp {
            share = 1.0;
        }
        if 0.0 < share {
            for (resource, amount, paid) in crate::objects::masters::cost_entries(&cost) {
                if paid {
                    self.players
                        .player_mut(owner)?
                        .add_attribute(resource, Amount::new(amount.as_f32() * share))?;
                }
            }
        }
        let i = self.wake(id)?;
        self.players.live_mut(owner, i)?.life.hp = HitPoints::ZERO;
        if state < ObjectState::Alive && desolid {
            self.destroy_foundation(owner, id)?;
        }
        Ok(())
    }

    /// A foundation's destroy (slot 0xd0, [`World::destroy`]) from its delete.
    fn destroy_foundation(&mut self, owner: PlayerId, id: ObjectId) -> Result<(), GenieError> {
        self.destroy(owner, id)
    }

    /// Slot 0xd0 of object `id` of player `owner`, by its master's type. A building's
    /// (`FUN_1411cfb30`): in state 2 what its storage gave its owner leaves
    /// ([`World::storage_leaves`]); a town-centre foundation (621) while its owner has no town
    /// centre (`FUN_1416597e0`, the player's object tracker for masters 109 and 444; the slice
    /// asks its live lists) clears its owner's treaty zones ([`World::clear_treaty_zones`]);
    /// then the action object's. The action object's (types 40 to
    /// 79 and the buildings, `FUN_14106c420`): its action list goes (`FUN_140a98620`,
    /// [`World::clear_actions`]), then the base destroy ([`World::base_destroy`]). A building's
    /// owner's watched list lets it go last (`FUN_141091310`). Not held: the semi-asleep flag
    /// (`+0xc98`, `FUN_14107db90`), `+0xca4` and the log lines.
    ///
    /// # Errors
    ///
    /// An object the slice does not hold, and what the parts do not model.
    pub(crate) fn destroy(&mut self, owner: PlayerId, id: ObjectId) -> Result<(), GenieError> {
        let (_, u) = self
            .object(id)
            .ok_or(crate::construction::build::NOT_HELD)?;
        let state = u.life.state;
        let master = u.master(self.master_tables(), owner)?.clone();
        if master.kind == BUILDING {
            if state == ObjectState::Alive {
                self.storage_leaves(owner, id, &master)?;
            }
            if master.id == TOWN_CENTRE_621 && !self.has_town_centre(owner) {
                self.clear_treaty_zones()?;
            }
        }
        if ACTION <= master.kind
            && let Some(Place::Live(p, i)) = self.place(id)
        {
            self.clear_actions(p, i)?;
        }
        self.base_destroy(owner, id)?;
        if master.kind == BUILDING {
            self.unwatch(owner, id)?;
        }
        Ok(())
    }

    /// `FUN_1414615d0` on the world's treaty monitor (`+0x820`) for a town centre's owner: its
    /// zones (`+0x18 + (owner − 1) × 0x24`: two rectangles and a flag) go to 0, and the game
    /// screen's list of them (`FUN_1414d2060`, `Game/ui/GameScreenPC.cpp`) loses its entries. Only
    /// a treaty fills the zones (`FUN_141461c00`), so without one they are 0 already.
    ///
    /// # Errors
    ///
    /// A game with a treaty ([`crate::world::settings::Settings::treaty_length`]), whose zones the slice
    /// does not hold.
    const fn clear_treaty_zones(&self) -> Result<(), GenieError> {
        if self.settings.treaty_length.holds() {
            return Err(GenieError::unsupported(
                "a deleted town centre 621 during a treaty (its owner's treaty zones)",
            ));
        }
        Ok(())
    }

    /// `FUN_1416597e0(owner, 0)`: the owner's tracker files a town centre the world holds
    /// ([`World::town_centre_at`]).
    fn has_town_centre(&self, owner: PlayerId) -> bool {
        self.town_centre_at(owner, 0).is_some()
    }

    /// The base destroy (`FUN_14118f1e0`) of object `id` of player `owner`: below state 7 its
    /// hit points go to 0, it wakes (`FUN_14119deb0` with 0) and goes to state 7 (slot 0xa0,
    /// [`World::set_state`]). An object inside another would leave it first (slot 0x1a8): not
    /// modelled. Not held: the special-object list (`+0x30c` bit 0, `FUN_141677380`) and the log
    /// line.
    pub(crate) fn base_destroy(&mut self, owner: PlayerId, id: ObjectId) -> Result<(), GenieError> {
        let (_, u) = self
            .object(id)
            .ok_or(crate::construction::build::NOT_HELD)?;
        if ObjectState::Removed <= u.life.state {
            return Ok(());
        }
        if u.life.inside.is_some() {
            return Err(GenieError::unsupported(
                "a destroyed object inside another (slot 0x1a8)",
            ));
        }
        let i = self.wake(id)?;
        self.players.live_mut(owner, i)?.life.hp = HitPoints::ZERO;
        self.set_state(owner, id, ObjectState::Removed)
    }

    /// Slot 0xa0 of object `id` of player `owner` to `new`, a state of 3 or more (2 for a
    /// building's completion), outside the
    /// object's own update, by its master's type: a building's (`FUN_1411dbb70`,
    /// [`World::building_set_state`]), a combat object's of type 70 (`FUN_14121cc40`,
    /// [`World::combat_set_state`]), else the base one ([`World::base_set_state`]).
    ///
    /// # Errors
    ///
    /// What the parts do not model.
    pub(crate) fn set_state(
        &mut self,
        owner: PlayerId,
        id: ObjectId,
        new: ObjectState,
    ) -> Result<(), GenieError> {
        let (_, u) = self.object(id).ok_or(GenieError::unsupported(
            "a set_state of an object the slice does not hold",
        ))?;
        let master = u.master(self.master_tables(), owner)?.clone();
        match master.kind {
            BUILDING => self.building_set_state(owner, id, new, &master),
            COMBAT => self.combat_set_state(owner, id, new, &master),
            _ => self.base_set_state(owner, id, new, &master),
        }
    }

    /// The building's `set_state` (`FUN_1411dbb70`) to a state of 3 or more: its annexes first
    /// (`+0xbc0`..`+0xbd8`, each a building: their own slot 0xa0 with the same state); a
    /// foundation (state 0) then runs the completion's first part ([`World::completion_part`],
    /// `FUN_1411c6bd0`) and, still in state 0 and going to a state from 1 to 6, its
    /// construction's start (`FUN_1411e5670`, [`World::construction_starts`]); a built building
    /// going to state 3 dies ([`World::built_building_dies`], which ends in the combat object's
    /// `set_state`); any other goes to the combat object's ([`World::combat_set_state`]).
    fn building_set_state(
        &mut self,
        owner: PlayerId,
        id: ObjectId,
        new: ObjectState,
        master: &Master,
    ) -> Result<(), GenieError> {
        let annexes = self.objects().annexes(id);
        for annex in annexes.ids() {
            if let Some((q, a)) = self.object(annex)
                && a.master(self.master_tables(), q)?.kind == BUILDING
            {
                // the annexes come from the spatial index's raw ids yet (T9's spatial keys)
                self.set_state(q, annex, new)?;
            }
        }
        let state = self
            .object(id)
            .map_or(ObjectState::Gone, |(_, u)| u.life.state);
        // the completion: its first part, its construction's start and the block for state 2
        if new == ObjectState::Alive && state == ObjectState::Foundation {
            return self.complete_building(id);
        }
        if state == ObjectState::Foundation {
            self.completion_part(owner, id, master)?;
            let still = self
                .object(id)
                .is_some_and(|(_, u)| u.life.state == ObjectState::Foundation);
            if still && (ObjectState::Unfinished..=ObjectState::Undead).contains(&new) {
                self.construction_starts(owner, id)?;
            }
        }
        if new == ObjectState::Dying && state == ObjectState::Alive {
            return self.built_building_dies(owner, id, master);
        }
        self.combat_set_state(owner, id, new, master)
    }

    /// The combat object's `set_state` (`FUN_14121cc40`) to a state of 3 or more: from below 3
    /// its death's part ([`crate::objects::lifecycle::death_parts::combat_dies`]); to state 3 from 2 its owner's attributes
    /// lose its paid cost ([`crate::objects::lifecycle::death_parts::refund_of`]: 98 and 164, a building 98 and 165, each at
    /// least 0); then the base `set_state` ([`World::base_set_state`]).
    fn combat_set_state(
        &mut self,
        owner: PlayerId,
        id: ObjectId,
        new: ObjectState,
        master: &Master,
    ) -> Result<(), GenieError> {
        // an object the slice does not hold fails in the base set_state
        let Some(place) = self.place(id) else {
            return self.base_set_state(owner, id, new, master);
        };
        let u = held(&mut self.players, place)?;
        let (state, main) = (u.life.state, self.parts.building_of(u.id).annex_of);
        if ObjectState::Dying <= new && state < ObjectState::Dying {
            crate::objects::lifecycle::death_parts::combat_dies(
                u,
                self.parts.building_of(id),
                master,
                &mut self.spatial,
                &mut self.pathing,
            )?;
            self.pathing.tiles_changed(self.spatial.take_changes());
        }
        if new == ObjectState::Dying && state == ObjectState::Alive {
            for (kind, amount) in crate::objects::lifecycle::death_parts::refund_of(master, main) {
                let player = self.players.player_mut(owner)?;
                player.take_at_least_zero(kind, amount)?;
            }
        }
        self.base_set_state(owner, id, new, master)
    }

    /// The base `set_state` (`FUN_14119d710`) to `new`, a state of 3 or more, outside the
    /// object's update: a lower state than its own changes nothing; from below 3 the storage of
    /// flags 2 and 16 leaves its owner (`FUN_1411a56c0` with 0) and, while the world's game state
    /// is not 2, a master of fog visibility 2 leaves its ghosts (`FUN_14118e990`,
    /// [`World::ghosts_at_death`], before the state changes); from below 4 to above 6 it leaves
    /// the visible-resource lists (`FUN_1411980b0`, [`crate::objects::lifecycle::death_parts::forget_resource`]); its sight
    /// follows ([`crate::map::spatial::Spatial::sight_state`]); from below 7 to 7 the storage of flag
    /// 32 leaves its owner; then the state, and a sleeping object wakes unless the state is 2 or
    /// 6. Not held: the statistics and the scenario's events.
    fn base_set_state(
        &mut self,
        owner: PlayerId,
        id: ObjectId,
        new: ObjectState,
        master: &Master,
    ) -> Result<(), GenieError> {
        let (_, u) = self.object(id).ok_or(GenieError::unsupported(
            "a set_state of an object the slice does not hold",
        ))?;
        let (old, position) = (u.life.state, u.position());
        if old == new || (new < old && new != ObjectState::Foundation) {
            return Ok(());
        }
        if ObjectState::Alive < new && old < ObjectState::Dying {
            self.leave_storage(owner, master, |flag| matches!(flag, 2 | 16))?;
            if master.ghost_on_death == GhostMode::LEAVES && self.game_state.leaves_ghosts() {
                self.ghosts_at_death(owner.index(), id)?;
            }
        }
        if old < ObjectState::DyingSprite && ObjectState::Undead < new {
            crate::objects::lifecycle::death_parts::forget_resource(
                &mut self.spatial,
                Some(master),
                id,
                position,
            )?;
        }
        self.spatial.sight_state(id, new)?;
        if old < ObjectState::Removed && new == ObjectState::Removed {
            self.leave_storage(owner, master, |flag| flag == 32)?;
        }
        if let Some((_, u)) = self.object_mut(id) {
            u.life.state = new;
        }
        if !matches!(new, ObjectState::Alive | ObjectState::Undead)
            && matches!(self.place(id), Some(Place::Sleeping(..)))
        {
            let _ = self.wake(id)?;
        }
        Ok(())
    }

    /// The storage of the master's entries of a flag `leaves` takes, which leaves its owner
    /// (`FUN_1411a56c0` with 0).
    fn leave_storage(
        &mut self,
        owner: PlayerId,
        master: &Master,
        leaves: impl Fn(u8) -> bool,
    ) -> Result<(), GenieError> {
        for &(kind, amount, flag) in &master.storage {
            if let Some(kind) = kind
                && leaves(flag)
            {
                self.players
                    .player_mut(owner)?
                    .add_attribute(kind, -amount)?;
            }
        }
        Ok(())
    }

    /// A built building's death (`FUN_1411dbb70` to state 3 from 2) outside its update: its
    /// part ([`crate::objects::lifecycle::death_parts::built_death`]), then the combat object's `set_state`; the count it
    /// lowers and its technology follow (as its own update's caller does them).
    fn built_building_dies(
        &mut self,
        owner: PlayerId,
        id: ObjectId,
        master: &Master,
    ) -> Result<(), GenieError> {
        let computer = self.players.player(owner)?.computer;
        let annex_of = self.parts.building_of(id).annex_of;
        let held = self
            .object_and_combat_mut(id)?
            .ok_or(crate::construction::build::NOT_HELD)?;
        let death =
            crate::objects::lifecycle::death_parts::built_death(held, annex_of, master, computer)?;
        for &(kind, amount) in &death.adds {
            self.players
                .player_mut(owner)?
                .add_attribute(kind, amount)?;
        }
        self.unwatch(owner, id)?;
        if let (Some(v), Some(at)) = (self.spatial.sight.as_mut(), death.unlink) {
            v.unlink_building(at, master.radius)?;
        }
        self.combat_set_state(owner, id, ObjectState::Dying, master)?;
        if let Some(kind) = death.building.count {
            self.players
                .player_mut(owner)?
                .take_at_least_zero(kind, Amount::new(1.0))?;
        }
        if let Some((m, tech)) = death.building.tech {
            self.withdraw_tech(owner, m, tech)?;
        }
        Ok(())
    }

    /// `FUN_14119deb0` with 0: an object asleep wakes to the end of its player's live list, a
    /// live one flagged to sleep drops the flag; its index in the live list.
    ///
    /// # Errors
    ///
    /// An object the world does not hold.
    pub(crate) fn wake(&mut self, id: ObjectId) -> Result<ListIndex, GenieError> {
        Ok(match self.place(id).ok_or_else(|| object_gone(id))? {
            Place::Live(p, i) => {
                self.players.live_mut(p, i)?.life.goto_sleep = false;
                i
            }
            Place::Sleeping(p, i) => {
                let mut woken = self.swap_remove_asleep(p, i)?;
                woken.unit.life.goto_sleep = false;
                let (id, master) = (woken.unit.id, woken.unit.life.master_id);
                let i = self.push_live(p, woken)?;
                // the remove and add (slots 0x2d8, 0x2d0) file it last in the tracker
                self.track_relist(p, id, master)?;
                i
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construction::build::Construction;
    use crate::map::spatial::Obstruction;
    use crate::objects::MasterHitPoints;
    use crate::objects::masters::ObstructionClass;
    use crate::objects::masters::ObstructionType;
    use crate::objects::masters::ids;
    use crate::objects::masters::{Master, Masters};
    use crate::objects::unit::Unit;
    use crate::players::attribute;
    use crate::test_ids::costs;
    use crate::test_ids::master_id;
    use crate::test_ids::object_id;
    use crate::test_ids::player_id;
    use crate::world::Millis;
    use crate::world::Player;
    use crate::world::Seconds;
    use crate::world::WholeSeconds;
    use crate::world::WorldTime;

    /// Gaia and two players: player 1's villager 10, house foundation 11 (build time 25, cost 25
    /// wood paid) and sheep 12, and player 2's villager 20.
    fn game(progress: f32, desolid: bool) -> World {
        let house = Unit {
            id: object_id(11),
            ..Unit::default()
        };
        let layer = crate::objects::layers::Building {
            construction: Some(Construction {
                progress: Seconds::new(progress),
                desolid,
                builders: crate::construction::BuilderCount::of_data(0),
                hit_points: MasterHitPoints::of_data(550),
                build_time: WholeSeconds::of_data(25),
                record: Obstruction {
                    kind: ObstructionType::TYPE_2,
                    class: ObstructionClass::NONE,
                },
                snow: false,
                sprite_override: false,
            }),
            ..Default::default()
        };
        let mut t = World::new(
            WorldTime::ZERO,
            crate::world::FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO; 3],
            vec![
                Player::default(),
                Player {
                    objects: vec![
                        Unit {
                            id: object_id(10),
                            ..Unit::default()
                        },
                        house,
                        Unit {
                            id: object_id(12),
                            ..Unit::default()
                        },
                    ]
                    .into(),
                    attributes: vec![Amount::ZERO; 10],
                    ..Player::default()
                },
                Player {
                    objects: vec![Unit {
                        id: object_id(20),
                        ..Unit::default()
                    }]
                    .into(),
                    ..Player::default()
                },
            ],
        )
        .with_buildings([(object_id(11), layer)]);
        for (p, objects) in [
            (1, [(0, 83), (1, 70), (2, 594)].as_slice()),
            (2, &[(0, 83)]),
        ] {
            for &(i, master) in objects {
                let u = &mut t.players[p].objects[ListIndex::new(i)];
                u.life.master_id = master_id(master);
                u.life.hp = HitPoints::new(25.0);
                u.pasture = None;
                u.my_pasture = None;
            }
        }
        t.players[1].objects[ListIndex::new(1)].life.state = ObjectState::Foundation;
        t.players[1].objects[ListIndex::new(2)].life.amount = Amount::new(100.0);
        let masters = Masters::new([
            Master {
                id: ids::VILLAGER_MALE,
                class: class::VILLAGER,
                kind: COMBAT,
                ..Master::default()
            },
            Master {
                id: ids::HOUSE,
                class: class::BUILDING,
                kind: BUILDING,
                cost: costs(&[(attribute::WOOD, 25, true), (attribute::HEADROOM, 1, false)]),
                ..Master::default()
            },
            Master {
                id: master_id(594),
                class: class::LIVESTOCK,
                kind: COMBAT,
                ..Master::default()
            },
        ]);
        t.masters = vec![masters.clone(), masters.clone(), masters];
        t
    }

    #[test]
    fn delete_takes_unit_hit_points_but_not_another_player_or_livestock() {
        let mut t = game(0.0, true);
        t.delete(player_id(1), object_id(10)).unwrap();
        t.delete(player_id(1), object_id(20)).unwrap();
        t.delete(player_id(1), object_id(12)).unwrap();
        let hp = |t: &World, p: usize, i: ListIndex| t.players[p].objects[i].life.hp;
        assert_eq!(
            (
                hp(&t, 1, ListIndex::new(0)),
                hp(&t, 2, ListIndex::new(0)),
                hp(&t, 1, ListIndex::new(2))
            ),
            (HitPoints::ZERO, HitPoints::new(25.0), HitPoints::new(25.0))
        );
        // the setting +0x1074 lets a player delete another's
        t.settings.cheats_enabled = true;
        t.delete(player_id(1), object_id(20)).unwrap();
        assert_eq!(hp(&t, 2, ListIndex::new(0)), HitPoints::ZERO);
        // a unit whose deletion is disabled stays
        let mut t = game(0.0, true);
        t.players[1].objects[ListIndex::new(0)].undeletable = true;
        t.delete(player_id(1), object_id(10)).unwrap();
        assert_eq!(hp(&t, 1, ListIndex::new(0)), HitPoints::new(25.0));
    }

    #[test]
    fn deleted_foundation_gives_back_what_its_build_time_has_still_to_go() {
        // no builder started it: all of its paid cost, and it is destroyed at once (state 7)
        let mut t = game(0.0, true);
        t.delete(player_id(1), object_id(11)).unwrap();
        let house = &t.players[1].objects[ListIndex::new(1)].life;
        assert_eq!(
            (t.players[1].attributes[1], house.hp, house.state),
            (Amount::new(25.0), HitPoints::ZERO, ObjectState::Removed)
        );
        // 10 of its 25 seconds of work done: 15/25 of the wood (in f32: 15.000001); it dies in
        // its own update
        let mut t = game(10.0, false);
        t.delete(player_id(1), object_id(11)).unwrap();
        let house = &t.players[1].objects[ListIndex::new(1)].life;
        let share = (25.0_f32 - 10.0) / 25.0;
        assert_eq!(
            (t.players[1].attributes[1], house.hp, house.state),
            (
                Amount::new(25.0 * share),
                HitPoints::ZERO,
                ObjectState::Foundation
            )
        );
    }

    /// A town-centre foundation (master 621) deleted, and destroyed at once: both clear its
    /// owner's treaty zones in the world's treaty monitor (`+0x820`, `FUN_1414615d0`, from
    /// `FUN_1411cd780` and `FUN_1411cfb30`), which only a treaty fills (`FUN_141461c00`): without
    /// one the delete is any building's; with one the zones are not held.
    #[test]
    fn deleted_town_centre_foundation_clears_no_zones_without_treaty() {
        let town_centre = |t: &mut World| {
            for m in &mut t.masters {
                let mut tc = m.get(ids::HOUSE).unwrap().clone();
                tc.id = TOWN_CENTRE_621;
                *m = Masters::new(m.iter().cloned().chain([tc]));
            }
            t.players[1].objects[ListIndex::new(1)].life.master_id = TOWN_CENTRE_621;
        };
        let mut t = game(0.0, true);
        town_centre(&mut t);
        t.delete(player_id(1), object_id(11)).unwrap();
        let tc = &t.players[1].objects[ListIndex::new(1)].life;
        assert_eq!(
            (t.players[1].attributes[1], tc.hp, tc.state),
            (Amount::new(25.0), HitPoints::ZERO, ObjectState::Removed)
        );
        let mut t = game(0.0, true);
        town_centre(&mut t);
        t.settings.treaty_length = crate::world::TreatyLength::of_minutes(10);
        assert_eq!(
            t.delete(player_id(1), object_id(11)),
            Err(GenieError::unsupported(
                "a deleted town centre 621 during a treaty (its owner's treaty zones)"
            ))
        );
    }
}

#[cfg(test)]
mod clearing_tests {
    use super::*;
    use crate::construction::MainBuildingId;
    use crate::map::Distance;
    use crate::map::Footprint;
    use crate::map::GridSize;
    use crate::map::Point;
    use crate::map::Position;
    use crate::map::Tile;
    use crate::map::spatial::{Placed, Spatial, Tiles};
    use crate::objects::lifecycle::{Class, Lifecycle};
    use crate::objects::masters::BuildableOn;
    use crate::objects::masters::ObstructionClass;
    use crate::objects::masters::ObstructionType;
    use crate::objects::masters::master_type;
    use crate::objects::masters::{Master, Masters};
    use crate::objects::masters::{MasterId, ids};
    use crate::objects::unit::Unit;
    use crate::players::attribute::{self, AttributeId};
    use crate::test_ids::costs;
    use crate::test_ids::master_id;
    use crate::test_ids::object_id;
    use crate::world::Millis;
    use crate::world::Player;
    use crate::world::WorldTime;

    /// A live object `id` of `master` at `x`, `y`, outside any pasture.
    fn object(id: i32, master: MasterId, x: f32, y: f32) -> Unit {
        Unit {
            id: object_id(id),
            place: crate::objects::layers::Static::at(Position::new(x, y, 0.0)),
            life: Lifecycle {
                master_id: master,
                ..Lifecycle::default()
            },
            pasture: None,
            my_pasture: None,
            ..Unit::default()
        }
    }

    /// Its place on the map, from its master.
    fn placed(u: &Unit, m: &Master, player: PlayerId) -> Placed {
        Placed {
            position: u.position(),
            radius: m.radius,
            class: m.class,
            player,
            master: m.id,
            obstruction_type: m.obstruction_type,
            buildable_on: m.buildable_on,
            tall: m.tall,
            moves: false,
            unknown: None,
            obstruction: None,
        }
    }

    /// The masters: a house (70), two decorations Gaia's map scatters (1351, 1358; type 10,
    /// buildable on), a dead villager (type 30, buildable on, its flag-32 storage of attribute
    /// 7), a pile (type 70, buildable on, tall, with a record) and a dead farm (type 80, class
    /// 11, buildable on).
    fn masters() -> Masters {
        let deco = |id| Master {
            id,
            kind: master_type::STATIC,
            class: class::CLASS_5,
            radius: Footprint::new(0.5, 0.5),
            buildable_on: BuildableOn::YES,
            ..Master::default()
        };
        Masters::new([
            Master {
                id: ids::HOUSE,
                kind: BUILDING,
                class: class::BUILDING,
                radius: Footprint::new(1.0, 1.0),
                ..Master::default()
            },
            deco(master_id(1351)),
            deco(master_id(1358)),
            Master {
                id: master_id(43),
                kind: master_type::MOVING,
                class: class::CLASS_11,
                radius: Footprint::new(0.2, 0.2),
                buildable_on: BuildableOn::YES,
                storage: vec![(Some(attribute::RELICS_HELD), Amount::new(1.0), 32)],
                ..Master::default()
            },
            Master {
                id: master_id(254),
                kind: COMBAT,
                class: class::CLASS_40,
                radius: Footprint::new(0.5, 0.5),
                height: Distance::new(1.0),
                tall: true,
                obstruction_type: ObstructionType::TYPE_2,
                buildable_on: BuildableOn::YES,
                ..Master::default()
            },
            Master {
                id: ids::ID_357,
                kind: BUILDING,
                class: class::CLASS_11,
                radius: Footprint::new(1.5, 1.5),
                buildable_on: BuildableOn::YES,
                building: Some(crate::objects::masters::BuildingMaster {
                    stack_unit: None,
                    foundation_terrain: None,
                    tech: None,
                    annexes: [None; 4],
                    head_unit: None,
                    transform_unit: None,
                    ..crate::objects::masters::BuildingMaster::default()
                }),
                ..Master::default()
            },
        ])
    }

    /// Gaia holding `gaia` (live) and `asleep`, player 1 holding `own`, on a 20 by 20 map.
    fn world(gaia: Vec<Unit>, asleep: Vec<Unit>, own: Vec<Unit>) -> World {
        let ms = masters();
        let mut t = World::new(
            WorldTime::ZERO,
            crate::world::FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO; 2],
            vec![
                Player {
                    objects: gaia.into(),
                    sleeping: asleep.into(),
                    attributes: vec![Amount::ZERO; 200],
                    ..Player::default()
                },
                Player {
                    objects: own.into(),
                    attributes: vec![Amount::ZERO; 200],
                    ..Player::default()
                },
            ],
        );
        t.spatial = Spatial::with_tiles(
            GridSize::new(20, 20),
            Tiles::new(GridSize::new(20, 20), vec![0; 400]),
        );
        for p in PlayerId::each(2) {
            let all: Vec<Unit> = t.players[p.index()]
                .objects
                .iter()
                .chain(&t.players[p.index()].sleeping)
                .cloned()
                .collect();
            for u in all {
                let m = ms.get(u.life.master_id).expect("a test master");
                let mut at = placed(&u, m, p);
                if m.obstruction_type == ObstructionType::TYPE_2 {
                    at.obstruction = Some(crate::map::spatial::Obstruction {
                        kind: ObstructionType::TYPE_2,
                        class: ObstructionClass::NONE,
                    });
                }
                t.spatial.insert(u.id, at);
            }
        }
        t.masters = vec![ms.clone(), ms];
        t
    }

    /// records/429859027's update 19 (and 161 more stops of census v5): a foundation's
    /// constructor clears two of Gaia's sleeping decorations off one tile (`FUN_1415c3620`): each
    /// in its tile's list order is destroyed (`FUN_14118f1e0`): its hit points go, it wakes to
    /// the end of Gaia's live list and goes to state 7.
    #[test]
    fn footprint_clearing_wakes_two_sleeping_decorations_in_their_tile_order() {
        let mut t = world(
            vec![object(1, master_id(1351), 3.5, 3.5)],
            vec![
                object(6084, master_id(1358), 10.6, 10.6),
                object(6083, master_id(1351), 10.4, 10.4),
            ],
            vec![],
        );
        assert_eq!(
            t.spatial.tiles().list(Tile::new(10, 10)),
            [object_id(6084), object_id(6083)]
        );
        t.clear_footprint(PlayerId::new(1), ids::HOUSE, Point::new(10.5, 10.5), false)
            .unwrap();
        let ids: Vec<ObjectId> = t.players[0].objects.iter().map(|u| u.id).collect();
        assert_eq!(ids, [object_id(1), object_id(6084), object_id(6083)]);
        assert_eq!(t.players[0].sleeping.len(), 0);
        for u in &t.players[0].objects.as_slice()[1..] {
            assert_eq!(
                (u.life.state, u.life.hp),
                (ObjectState::Removed, HitPoints::ZERO)
            );
        }
        assert_eq!(
            t.players[0].objects[ListIndex::new(0)].life.state,
            ObjectState::Alive
        );
    }

    /// records/428477858's update 1838: a foundation clears a dead villager (type 30) in state 5
    /// off its footprint: the base destroy takes it to state 7 and its storage of flag 32 leaves
    /// its owner (`FUN_14119d710`).
    #[test]
    fn cleared_corpse_goes_to_state_7_and_its_flag_32_storage_leaves() {
        let mut corpse = object(30, master_id(43), 10.5, 10.5);
        corpse.life.state = ObjectState::Dead;
        corpse.life.class = Class::Animated;
        let mut t = world(vec![], vec![], vec![corpse]);
        t.players[1].attributes[7] = Amount::new(3.0);
        t.clear_footprint(PlayerId::new(1), ids::HOUSE, Point::new(10.5, 10.5), false)
            .unwrap();
        let u = &t.players[1].objects[ListIndex::new(0)];
        assert_eq!(
            (u.life.state, u.life.hp),
            (ObjectState::Removed, HitPoints::ZERO)
        );
        assert_eq!(t.players[1].attributes[7], Amount::new(2.0));
    }

    /// A pile (type 70) a new object's clearing (`param_6`) destroys: its action list goes
    /// (`FUN_14106c420`), and the combat object's `set_state(7)` (`FUN_14121cc40`) drops its
    /// record and its height and sets `+0x686`; it gets no refund (only state 3 from 2 does).
    #[test]
    fn cleared_pile_loses_its_record_in_its_destroy() {
        let mut t = world(vec![object(40, master_id(254), 10.5, 10.5)], vec![], vec![]);
        t.players[0].attributes[98] = Amount::new(5.0);
        assert!(t.spatial.get(object_id(40)).unwrap().obstruction.is_some());
        t.clear_footprint(PlayerId::new(1), ids::HOUSE, Point::new(10.5, 10.5), true)
            .unwrap();
        let u = &t.players[0].objects[ListIndex::new(0)];
        assert_eq!(
            (u.life.state, u.life.hp, u.roo_called),
            (ObjectState::Removed, HitPoints::ZERO, true)
        );
        let p = t.spatial.get(object_id(40)).unwrap();
        assert_eq!((p.obstruction, p.tall), (None, false));
        assert_eq!(t.players[0].attributes[98], Amount::new(5.0));
    }

    /// A dead farm (`FARM_D`, a building of type 80) a new farm's constructor clears: the
    /// building's destroy (`FUN_1411cfb30`): its storage leaves in state 2, its action list goes,
    /// the base destroy takes it to state 7 through the building's and the combat object's
    /// `set_state`, and its owner's watched list lets it go.
    #[test]
    fn cleared_dead_farm_goes_through_the_building_destroy() {
        let mut t = world(vec![], vec![], vec![object(50, ids::ID_357, 10.5, 10.5)]);
        t.clear_footprint(PlayerId::new(1), ids::HOUSE, Point::new(10.5, 10.5), false)
            .unwrap();
        let u = &t.players[1].objects[ListIndex::new(0)];
        assert_eq!(
            (u.life.state, u.life.hp, u.roo_called),
            (ObjectState::Removed, HitPoints::ZERO, true)
        );
    }

    /// The base `set_state` (`FUN_14119d710`) of a decoration (type 10): the same or a lower
    /// state changes nothing; from below 3 its storage of flags 2 and 16 leaves its owner (that
    /// of flag 32 only at 7, a kind below 0 never); a sleeping object wakes to the end of the
    /// live list unless the state is 6; an object the world does not hold is not modelled.
    #[test]
    fn the_base_set_state_lets_its_storage_go_and_wakes_sleeper() {
        let mut t = world(
            vec![object(1, master_id(1351), 3.5, 3.5)],
            vec![
                object(2, master_id(1358), 5.5, 5.5),
                object(3, master_id(1358), 7.5, 7.5),
            ],
            vec![],
        );
        for m in [1351, 1358] {
            t.masters[0].get_mut(master_id(m)).unwrap().storage = vec![
                (Some(attribute::HEADROOM), Amount::new(2.0), 2),
                (Some(AttributeId::numbered(5)), Amount::new(3.0), 16),
                (Some(attribute::RELICS_HELD), Amount::new(1.0), 32),
                (None, Amount::new(9.0), 2),
            ];
        }
        let at = |t: &World| [4, 5, 7].map(|i| t.players[0].attributes[i]);
        t.players[0].attributes[4..8].copy_from_slice(&[
            Amount::new(10.0),
            Amount::new(10.0),
            Amount::new(10.0),
            Amount::new(10.0),
        ]);
        t.players[0].objects[ListIndex::new(0)].life.state = ObjectState::Alive;
        // the same state, then a lower one: nothing
        t.set_state(PlayerId::new(0), object_id(1), ObjectState::Alive)
            .unwrap();
        t.players[0].objects[ListIndex::new(0)].life.state = ObjectState::Dead;
        t.set_state(PlayerId::new(0), object_id(1), ObjectState::Dying)
            .unwrap();
        assert_eq!(
            t.players[0].objects[ListIndex::new(0)].life.state,
            ObjectState::Dead
        );
        assert_eq!(at(&t), crate::test_ids::amounts([10.0; 3]));
        // from 2 to 5: flags 2 and 16 leave, flag 32 stays; then to 7, flag 32 leaves
        t.players[0].objects[ListIndex::new(0)].life.state = ObjectState::Alive;
        t.set_state(PlayerId::new(0), object_id(1), ObjectState::Dead)
            .unwrap();
        assert_eq!(at(&t), crate::test_ids::amounts([8.0, 7.0, 10.0]));
        t.set_state(PlayerId::new(0), object_id(1), ObjectState::Removed)
            .unwrap();
        assert_eq!(at(&t), crate::test_ids::amounts([8.0, 7.0, 9.0]));
        assert_eq!(
            t.players[0].objects[ListIndex::new(0)].life.state,
            ObjectState::Removed
        );
        // a sleeper set to 6 sleeps on, one set to 7 wakes
        t.players[0].sleeping[ListIndex::new(0)].life.state = ObjectState::Dead;
        t.set_state(PlayerId::new(0), object_id(2), ObjectState::Undead)
            .unwrap();
        assert_eq!(t.players[0].sleeping.len(), 2);
        t.set_state(PlayerId::new(0), object_id(3), ObjectState::Removed)
            .unwrap();
        let ids: Vec<ObjectId> = t.players[0].objects.iter().map(|u| u.id).collect();
        assert_eq!(
            (ids, t.players[0].sleeping.len()),
            (vec![object_id(1), object_id(3)], 1)
        );
        assert!(
            t.set_state(PlayerId::new(0), object_id(99), ObjectState::Removed)
                .is_err()
        );
    }

    /// `FUN_14121cc40`'s refund at a death from state 2: a unit's paid cost leaves attributes 98
    /// and 164, a building's 98 and 165, the castle's and the wonder's 98 alone, an annex's none.
    #[test]
    fn death_refunds_its_paid_cost_by_its_type() {
        let unit = Master {
            kind: COMBAT,
            cost: costs(&[
                (attribute::FOOD, 50, true),
                (attribute::GOLD, 20, true),
                (attribute::WOOD, 5, false),
            ]),
            ..Master::default()
        };
        assert_eq!(
            crate::objects::lifecycle::death_parts::refund_of(&unit, None),
            crate::test_ids::adds([
                (attribute::TOTAL_VALUE, 70.0),
                (attribute::UNITS_VALUE, 70.0)
            ])
        );
        let house = Master {
            id: ids::HOUSE,
            kind: BUILDING,
            ..unit
        };
        assert_eq!(
            crate::objects::lifecycle::death_parts::refund_of(&house, None),
            crate::test_ids::adds([
                (attribute::TOTAL_VALUE, 70.0),
                (attribute::BUILDINGS_VALUE, 70.0)
            ])
        );
        assert_eq!(
            crate::objects::lifecycle::death_parts::refund_of(
                &house,
                Some(MainBuildingId::new(object_id(9)))
            ),
            []
        );
        let castle = Master {
            id: ids::CASTLE,
            ..house
        };
        assert_eq!(
            crate::objects::lifecycle::death_parts::refund_of(&castle, None),
            crate::test_ids::adds([(attribute::TOTAL_VALUE, 70.0)])
        );
    }
}
