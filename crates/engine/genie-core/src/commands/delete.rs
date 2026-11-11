//! A player's deletion of one of its objects: `CANCEL_BUILD` (0x6a, `TRIBE_Command::do_command`
//! case 0x6a), which the game's delete key sends.
//!
//! The command checks the object's owner and whether it may be deleted (`FUN_1416257c0`), then
//! runs its slot 0x8b8: a unit's (`FUN_14120f620`) takes its hit points (slot 0xd8), a building's
//! (`FUN_1411cd780` -> `FUN_1411ccbb0`) gives back what is left of a foundation's cost and takes
//! its hit points too. The object dies in its next update.

use crate::construction::build::held;
use crate::construction::build::{NO_CONSTRUCTION, NOT_HELD};
use crate::map::Tile;
use crate::objects::HitPoints;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::lifecycle::death_parts::{
    built_death, combat_dies, forget_resource, refund_of, snow_leaves,
};
use crate::objects::masters::GhostMode;
use crate::objects::masters::Master;
use crate::objects::masters::MasterKind;
use crate::objects::masters::cost_entries;
use crate::objects::unit::UnitFlags;
use crate::orders::object_gone;
use crate::ownership::CaptureKind;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::game_status::GameStatus;
use crate::world::Seconds;
use crate::world::World;
use crate::world::index::Place;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::settings::OptionFlags;
use crate::world::slots::PlayerSlotsExt;
use crate::{GenieError, GenieResult};

/// An object inside another that the base destroy (`FUN_14118f1e0`) would let out first (slot
/// 0x1a8), which the slice does not model.
pub(crate) const DESTROYED_INSIDE: GenieError =
    GenieError::unsupported("a destroyed object inside another (slot 0x1a8)");
/// A `set_state` of an object the slice does not hold.
const SET_STATE_NOT_HELD: GenieError =
    GenieError::unsupported("a set_state of an object the slice does not hold");

/// The first master type a delete takes (`FUN_1416257c0`: above `0x45`).
const DELETABLE_KIND: MasterKind = MasterKind::of_data(70);

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
    pub(crate) fn run(self, world: &mut World) -> GenieResult<()> {
        world.delete(self.player, self.id)
    }
}

impl World {
    /// `CANCEL_BUILD` for `player` on object `id`: the object must exist and be the player's,
    /// unless the setting `+0x1074` lets players order each other's
    /// ([`crate::world::settings::OptionFlags::CHEATS_ENABLED`]) or its owner lost and the two are
    /// allies both ways (`FUN_14162ef10`). `FUN_1416257c0` then wants its deletion enabled
    /// (`+0x687`), a master of type 70 or more, a class other than
    /// [`ClassRoles::UNDELETABLE`](crate::world::layer::ClassRoles::UNDELETABLE)'s and a capture
    /// flag (`+0x924`) other than 2; the object's slot 0x8b8 follows ([`World::delete_unit`],
    /// [`World::delete_building`]). The bypass of the disable-verification cheat (`+0xb12`) is not
    /// held.
    ///
    /// # Errors
    ///
    /// What the deletes do not model.
    pub(crate) fn delete(&mut self, player: PlayerId, id: ObjectId) -> GenieResult<()> {
        let Some((owner, unit)) = self.object(id) else {
            return Ok(());
        };
        let mutual = |one: PlayerId, other: PlayerId| {
            let treaty = self.treaty.as_ref();
            self.spatial.allied_with(treaty, one, other)
                && self.spatial.allied_with(treaty, other, one)
        };
        // The buildings' master type, whose slot 0x8b8 is the building's.
        // The first master type whose slot 0xd0 is the action object's (`FUN_14106c420`), and the
        // TRIBE combat objects' type, whose slot 0xa0 is `FUN_14121cc40`.
        // A player's game status once it lost (`+0x89`).
        // A town centre's master whose delete also tells the world's `+0x820` (`FUN_1414615d0`).
        let lost_ally =
            self.players.player(owner)?.game_status == GameStatus::Lost && mutual(player, owner);
        if owner != player
            && !self.settings.flags.contains(OptionFlags::CHEATS_ENABLED)
            && !lost_ally
        {
            return Ok(());
        }
        let master = unit.master(self.master_tables(), owner)?;
        if unit.flags.contains(UnitFlags::UNDELETABLE)
            || master.kind < DELETABLE_KIND
            || self
                .model
                .roles
                .of_class(master.class)
                .contains(ClassRoles::UNDELETABLE)
            || self.spatial.capture_of(unit.id) == CaptureKind::Again
        {
            return Ok(());
        }
        if master.kind == MasterKind::BUILDING {
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
    pub(crate) fn clear_foundations_under(&mut self, id: ObjectId) -> GenieResult<()> {
        const AROUND: i32 = 5;
        let Some(site) = self.spatial.get(id).copied() else {
            return Ok(());
        };
        let (x, y) = (site.position.x(), site.position.y());
        let (rx, ry) = (site.radius.x(), site.radius.y());
        let (width, height) = (self.spatial.map.width(), self.spatial.map.height());
        let (x0, y0) = (
            (((x - rx) as i32) - AROUND).max(0),
            (((y - ry) as i32) - AROUND).max(0),
        );
        let x1 = if ((x + rx) as i32 + AROUND) < width {
            (x + rx) as i32 + AROUND
        } else {
            width - 1
        };
        let y1 = if ((y + ry) as i32 + AROUND) < height {
            (y + ry) as i32 + AROUND
        } else {
            height - 1
        };
        let mut gone = Vec::new();
        for ty in y0..=y1 {
            for tx in x0..=x1 {
                for (oid, placed) in self.spatial.objects_on(Tile::new(tx, ty)) {
                    if oid == id || gone.iter().any(|&(_, object_id)| object_id == oid) {
                        continue;
                    }
                    let Some((owner, oid_unit)) = self.object(oid) else {
                        continue;
                    };
                    let kind = oid_unit.master(self.master_tables(), owner)?.kind;
                    let desolid = self
                        .parts
                        .building_of(oid_unit.id)
                        .construction
                        .is_some_and(|construction| construction.desolid);
                    if kind != MasterKind::BUILDING
                        || !desolid
                        || !self.spatial.sees_and_explored(owner, Tile::new(tx, ty))
                    {
                        continue;
                    }
                    let dx = (placed.position.x() - x).abs();
                    let dy = (placed.position.y() - y).abs();
                    if dx < placed.radius.x() + rx && dy < ry + placed.radius.y() {
                        gone.push((owner, oid));
                    }
                }
            }
        }
        for (owner, oid) in gone {
            self.remove_building(owner, oid)?;
            // its record went with it (`+0x686` set)
            if let Some((_, unit)) = self.object_mut(oid) {
                unit.flags.set(UnitFlags::ROO_CALLED, true);
            }
        }
        Ok(())
    }

    /// A unit's slot 0x8b8 (`FUN_14120f620`): a unit of a class other than
    /// [`ClassRoles::UNDELETABLE`](crate::world::layer::ClassRoles::UNDELETABLE)'s is marked
    /// deleted (`+0xa0c`) and its slot 0xd8 (`FUN_141211340`) spares class 42 and livestock holding
    /// some, else `FUN_14118f580`: a sleeping unit wakes to the end of its player's live list and
    /// its hit points go to 0. Not held: the deleted mark, the killer (`+0x690`, its owner) and the
    /// special-object list (`+0x30c` bit 0).
    fn delete_unit(&mut self, owner: PlayerId, id: ObjectId) -> GenieResult<()> {
        let (_, unit) = self.object(id).ok_or_else(|| object_gone(id))?;
        let class = unit.master(self.master_tables(), owner)?.class;
        if self
            .model
            .roles
            .of_class(class)
            .contains(ClassRoles::UNDELETABLE)
        {
            return Ok(());
        }
        // The class a unit's slot 0xd8 (`FUN_141211340`) spares, and the livestock it spares while it
        // holds some.
        if self.model.roles.of_class(class).contains(ClassRoles::RELIC)
            || (self
                .model
                .roles
                .of_class(class)
                .contains(ClassRoles::LIVESTOCK)
                && Amount::ZERO < unit.life.amount)
        {
            return Ok(());
        }
        let list_index = self.wake(id)?;
        self.players.live_mut(owner, list_index)?.life.hp = HitPoints::ZERO;
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
    fn delete_building(&mut self, owner: PlayerId, id: ObjectId) -> GenieResult<()> {
        let (_, unit) = self.object(id).ok_or_else(|| object_gone(id))?;
        let master = unit.master(self.master_tables(), owner)?;
        if self
            .model
            .roles
            .of_master(master.id)
            .contains(MasterRoles::TOWN_CENTRE_621)
        {
            self.clear_treaty_zones(owner)?;
        }
        self.remove_building(owner, id)
    }

    /// `FUN_1411ccbb0`, the core of a building's delete ([`World::delete_building`]) and of a
    /// construction start's clearing ([`World::clear_foundations_under`]): the building wakes,
    /// a foundation's paid cost comes back by the share its build time has still to go (slot
    /// 0xf8 with 0 and 1, [`World::add_player_attribute`]), its hit points go to 0 and a desolid
    /// foundation is destroyed at once.
    pub(crate) fn remove_building(&mut self, owner: PlayerId, id: ObjectId) -> GenieResult<()> {
        let (_, unit) = self.object(id).ok_or(GenieError::unsupported(
            "a removed building the slice does not hold",
        ))?;
        let master = unit.master(self.master_tables(), owner)?;
        let cost = master.creatable_part().cost;
        let (state, hp) = (unit.life.state, unit.life.hp);
        let site = self.parts.building_of(unit.id).construction;
        let desolid = site.is_some_and(|construction| construction.desolid);
        let share = if state < ObjectState::Alive && HitPoints::new(1.0) <= hp {
            let construction = site.ok_or(NO_CONSTRUCTION)?;
            let build_time = Seconds::of_master(construction.build_time).get();
            (build_time - construction.progress.get()) / build_time
        } else {
            0.0
        };
        // at most 1 (a NaN keeps the 1)
        let mut share = if share <= 1.0 { share } else { 1.0 };
        if desolid && HitPoints::ZERO < hp {
            share = 1.0;
        }
        if 0.0 < share {
            for (resource, amount, paid) in cost_entries(&cost) {
                if paid {
                    self.add_player_attribute(
                        owner,
                        resource,
                        Amount::new(amount.as_f32() * share),
                    )?;
                }
            }
        }
        let list_index = self.wake(id)?;
        self.players.live_mut(owner, list_index)?.life.hp = HitPoints::ZERO;
        if state < ObjectState::Alive && desolid {
            self.destroy_foundation(owner, id)?;
        }
        Ok(())
    }

    /// A foundation's destroy (slot 0xd0, [`World::destroy`]) from its delete.
    fn destroy_foundation(&mut self, owner: PlayerId, id: ObjectId) -> GenieResult<()> {
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
    pub(crate) fn destroy(&mut self, owner: PlayerId, id: ObjectId) -> GenieResult<()> {
        let (_, unit) = self.object(id).ok_or(NOT_HELD)?;
        let state = unit.life.state;
        let master = unit.master(self.master_tables(), owner)?.clone();
        if master.kind == MasterKind::BUILDING {
            if state == ObjectState::Alive {
                self.storage_leaves(owner, id, &master)?;
            }
            if self
                .model
                .roles
                .of_master(master.id)
                .contains(MasterRoles::TOWN_CENTRE_621)
                && !self.has_town_centre(owner)
            {
                self.clear_treaty_zones(owner)?;
            }
        }
        if MasterKind::ACTION <= master.kind
            && let Some(Place::Live(player_id, list_index)) = self.place(id)
        {
            self.clear_actions(player_id, list_index)?;
        }
        self.base_destroy(owner, id)?;
        if master.kind == MasterKind::BUILDING {
            self.unwatch(owner, id)?;
        }
        Ok(())
    }

    /// The town centre's delete and destroy (`FUN_1411cd780`, `FUN_1411cfb30`) on the world's
    /// treaty monitor (`+0x820`) for the town centre's owner `owner`: a monitor in any state
    /// clears the owner's first zone (`FUN_1414615d0`, [`TreatyMonitor::clear_zone`]); without
    /// one (a null pointer) nothing happens.
    ///
    /// # Errors
    ///
    /// The clear's: Gaia's town centre, and an owner past the monitor's zones.
    ///
    /// [`TreatyMonitor::clear_zone`]: crate::players::treaty::TreatyMonitor::clear_zone
    fn clear_treaty_zones(&mut self, owner: PlayerId) -> GenieResult<()> {
        self.treaty
            .as_mut()
            .map_or(Ok(()), |monitor| monitor.clear_zone(owner))
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
    pub(crate) fn base_destroy(&mut self, owner: PlayerId, id: ObjectId) -> GenieResult<()> {
        let (_, unit) = self.object(id).ok_or(NOT_HELD)?;
        if ObjectState::Removed <= unit.life.state {
            return Ok(());
        }
        if unit.life.inside.is_some() {
            return Err(DESTROYED_INSIDE);
        }
        let list_index = self.wake(id)?;
        self.players.live_mut(owner, list_index)?.life.hp = HitPoints::ZERO;
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
    ) -> GenieResult<()> {
        let (_, unit) = self.object(id).ok_or(SET_STATE_NOT_HELD)?;
        let master = unit.master(self.master_tables(), owner)?.clone();
        match master.kind {
            MasterKind::BUILDING => self.building_set_state(owner, id, new, &master),
            MasterKind::COMBAT => self.combat_set_state(owner, id, new, &master),
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
    ) -> GenieResult<()> {
        let annexes = self.objects().annexes(id);
        for annex in annexes.ids() {
            if let Some((player_id, annex_unit)) = self.object(annex)
                && annex_unit.master(self.master_tables(), player_id)?.kind == MasterKind::BUILDING
            {
                // the annexes come from the spatial index's raw ids yet (T9's spatial keys)
                self.set_state(player_id, annex, new)?;
            }
        }
        let state = self
            .object(id)
            .map_or(ObjectState::Gone, |(_, unit)| unit.life.state);
        // the completion: its first part, its construction's start and the block for state 2
        if new == ObjectState::Alive && state == ObjectState::Foundation {
            return self.complete_building(id);
        }
        if state == ObjectState::Foundation {
            self.completion_part(owner, id, master)?;
            let still = self
                .object(id)
                .is_some_and(|(_, unit)| unit.life.state == ObjectState::Foundation);
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
    ) -> GenieResult<()> {
        // an object the slice does not hold fails in the base set_state
        let Some(place) = self.place(id) else {
            return self.base_set_state(owner, id, new, master);
        };
        let unit = held(&mut self.players, place)?;
        let (state, main) = (unit.life.state, self.parts.building_of(unit.id).annex_of);
        if ObjectState::Dying <= new && state < ObjectState::Dying {
            combat_dies(
                unit,
                self.parts.building_of(id),
                master,
                &mut self.spatial,
                &mut self.pathing,
                &self.model.roles,
                self.model.tuning.visibility,
            )?;
            self.pathing.tiles_changed(self.spatial.take_changes());
        }
        if new == ObjectState::Dying && state == ObjectState::Alive {
            for (kind, amount) in refund_of(master, main, &self.model.roles, self.model.slots) {
                let player = self.players.player_mut(owner)?;
                player.take_at_least_zero(kind, amount, &self.model.roles);
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
    ) -> GenieResult<()> {
        let (_, unit) = self.object(id).ok_or(SET_STATE_NOT_HELD)?;
        let (old, position) = (unit.life.state, unit.position());
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
            forget_resource(&mut self.spatial, Some(master), id, position)?;
        }
        self.spatial
            .sight_state(id, new, &self.model.roles, self.model.tuning.visibility)?;
        if old < ObjectState::Removed && new == ObjectState::Removed {
            self.leave_storage(owner, master, |flag| flag == 32)?;
        }
        if let Some((_, unit)) = self.object_mut(id) {
            unit.life.state = new;
        }
        if !matches!(new, ObjectState::Alive | ObjectState::Undead)
            && matches!(self.place(id), Some(Place::Sleeping(..)))
        {
            let _ = self.wake(id)?;
        }
        Ok(())
    }

    /// The storage of the master's entries of a flag `leaves` takes, which leaves its owner
    /// (`FUN_1411a56c0` with 0: slot 0xf8 with 0 and 1, [`World::add_player_attribute`]).
    fn leave_storage(
        &mut self,
        owner: PlayerId,
        master: &Master,
        leaves: impl Fn(u8) -> bool,
    ) -> GenieResult<()> {
        for &(kind, amount, flag) in &master.storage {
            if let Some(kind) = kind
                && leaves(flag)
            {
                self.add_player_attribute(owner, kind, -amount)?;
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
    ) -> GenieResult<()> {
        let player_kind = self.players.player(owner)?.kind;
        let annex_of = self.parts.building_of(id).annex_of;
        let (roles, slots) = (self.model.roles.clone(), self.model.slots);
        let held = self.object_and_combat_mut(id)?.ok_or(NOT_HELD)?;
        let death = built_death(held, annex_of, master, player_kind, &roles, slots)?;
        for &(kind, amount) in &death.adds {
            self.add_player_attribute(owner, kind, amount)?;
        }
        self.unwatch(owner, id)?;
        if let (Some(visibility), Some(at)) = (self.spatial.sight.as_mut(), death.unlink) {
            visibility.unlink_building(at, master.radius, self.model.tuning.visibility)?;
        }
        snow_leaves(&mut self.parts.building, id);
        self.combat_set_state(owner, id, ObjectState::Dying, master)?;
        if let Some(kind) = death.building.count {
            self.players
                .player_mut(owner)?
                .take_at_least_zero(kind, Amount::new(1.0), &roles);
        }
        if let Some((master_id, tech)) = death.building.tech {
            self.withdraw_tech(owner, master_id, tech)?;
        }
        Ok(())
    }

    /// `FUN_14119deb0` with 0: an object asleep wakes to the end of its player's live list, a
    /// live one flagged to sleep drops the flag; its index in the live list.
    ///
    /// # Errors
    ///
    /// An object the world does not hold.
    pub(crate) fn wake(&mut self, id: ObjectId) -> GenieResult<ListIndex> {
        Ok(match self.place(id).ok_or_else(|| object_gone(id))? {
            Place::Live(owner, list_index) => {
                self.players.live_mut(owner, list_index)?.life.goto_sleep = false;
                list_index
            }
            Place::Sleeping(player_id, slot) => {
                let mut woken = self.swap_remove_asleep(player_id, slot)?;
                woken.unit.life.goto_sleep = false;
                let (id, master) = (woken.unit.id, woken.unit.life.master_id);
                let list_index = self.push_live(player_id, woken)?;
                // the remove and add (slots 0x2d8, 0x2d0) file it last in the tracker
                self.track_relist(player_id, id, master)?;
                list_index
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(test)]
    use crate::construction::BuilderCount;
    use crate::construction::build::Construction;
    #[cfg(test)]
    use crate::map::Distance;
    use crate::map::GridSize;
    use crate::map::Snow;
    use crate::map::TileRect;
    use crate::map::spatial::Obstruction;
    use crate::objects::MasterHitPoints;
    #[cfg(test)]
    use crate::objects::layers::Building;
    use crate::objects::masters::BuildingMaster;
    use crate::objects::masters::CreatablePart;
    use crate::objects::masters::ObstructionClass;
    use crate::objects::masters::ObstructionKind;
    use crate::objects::masters::{Master, Masters};
    use crate::objects::unit::Unit;
    use crate::players::tracker::Key;
    use crate::players::treaty::{
        NomadTreaty, TreatyMinutes, TreatyMonitor, TreatyState, TreatyZone,
    };
    use crate::test_ids::costs;
    use crate::test_ids::master_id;
    use crate::test_ids::object_id;
    use crate::test_ids::player_id;
    use crate::testing::attribute;
    use crate::testing::class;
    use crate::testing::ids;
    use crate::testing::ids::TOWN_CENTRE_621;
    use crate::vision::visibility::Seer;
    use crate::vision::visibility::Visibility;
    use crate::world::FrameSeed;
    use crate::world::GameMode;
    use crate::world::Millis;
    use crate::world::Player;
    use crate::world::Seconds;
    use crate::world::WholeSeconds;
    use crate::world::WorldTime;
    use std::sync::Arc;

    /// Gaia and two players: player 1's villager 10, house foundation 11 (build time 25, cost 25
    /// wood paid) and sheep 12, and player 2's villager 20.
    fn game(progress: f32, desolid: bool) -> World {
        let house = Unit {
            id: object_id(11),
            ..Unit::default()
        };
        let layer = Building {
            construction: Some(Construction {
                progress: Seconds::new(progress),
                desolid,
                builders: BuilderCount::of_data(0),
                hit_points: MasterHitPoints::of_data(550),
                build_time: WholeSeconds::of_data(25),
                record: Obstruction {
                    kind: ObstructionKind::KIND_2,
                    class: ObstructionClass::NONE,
                },
                snow: Snow::None,
                sprite_override: false,
            }),
            ..Default::default()
        };
        let mut world = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(1),
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
        for (player, objects) in [
            (1, [(0, 83), (1, 70), (2, 594)].as_slice()),
            (2, &[(0, 83)]),
        ] {
            for &(index, master) in objects {
                let unit = &mut world.players[player].objects[ListIndex::new(index)];
                unit.life.master_id = master_id(master);
                unit.life.hp = HitPoints::new(25.0);
                unit.pasture = None;
                unit.my_pasture = None;
            }
        }
        world.players[1].objects[ListIndex::new(1)].life.state = ObjectState::Foundation;
        world.players[1].objects[ListIndex::new(2)].life.amount = Amount::new(100.0);
        let masters = Masters::new([
            Master {
                id: ids::VILLAGER_MALE,
                class: class::VILLAGER,
                kind: MasterKind::COMBAT,
                ..Master::default()
            },
            Master {
                id: ids::HOUSE,
                class: class::BUILDING,
                kind: MasterKind::BUILDING,
                creatable: Some(CreatablePart {
                    cost: costs(&[(attribute::WOOD, 25, true), (attribute::HEADROOM, 1, false)]),
                    ..CreatablePart::default()
                }),
                ..Master::default()
            },
            Master {
                id: master_id(594),
                class: class::LIVESTOCK,
                kind: MasterKind::COMBAT,
                ..Master::default()
            },
        ]);
        world.model.masters = vec![
            Arc::new(masters.clone()),
            Arc::new(masters.clone()),
            Arc::new(masters),
        ];
        world
    }

    #[test]
    fn delete_takes_unit_hit_points_but_not_another_player_or_livestock() {
        let mut scene = game(0.0, true);
        scene.delete(player_id(1), object_id(10)).unwrap();
        scene.delete(player_id(1), object_id(20)).unwrap();
        scene.delete(player_id(1), object_id(12)).unwrap();
        let hp = |world: &World, player: usize, list_index: ListIndex| {
            world.players[player].objects[list_index].life.hp
        };
        assert_eq!(
            (
                hp(&scene, 1, ListIndex::new(0)),
                hp(&scene, 2, ListIndex::new(0)),
                hp(&scene, 1, ListIndex::new(2))
            ),
            (HitPoints::ZERO, HitPoints::new(25.0), HitPoints::new(25.0))
        );
        // the setting +0x1074 lets a player delete another's
        scene.settings.flags.set(OptionFlags::CHEATS_ENABLED, true);
        scene.delete(player_id(1), object_id(20)).unwrap();
        assert_eq!(hp(&scene, 2, ListIndex::new(0)), HitPoints::ZERO);
        // a unit whose deletion is disabled stays
        let mut world = game(0.0, true);
        world.players[1].objects[ListIndex::new(0)]
            .flags
            .set(UnitFlags::UNDELETABLE, true);
        world.delete(player_id(1), object_id(10)).unwrap();
        assert_eq!(hp(&world, 1, ListIndex::new(0)), HitPoints::new(25.0));
        // so does one whose capture flag (`+0x924`, its seer's) is 2
        let mut world = game(0.0, true);
        let mut visibility = Visibility::new(
            GridSize::new(10, 10),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            0,
            Vec::new(),
        );
        visibility.add_seer(
            object_id(10),
            Seer {
                player: player_id(1),
                master: ids::VILLAGER_MALE,
                los: Distance::new(4.0),
                los_powerup: Distance::ZERO,
                state: ObjectState::Alive,
                explored: true,
                building: None,
                umv: 0,
                capture: CaptureKind::Again,
                kind: MasterKind::COMBAT,
            },
        );
        world.spatial.sight = Some(visibility);
        world.delete(player_id(1), object_id(10)).unwrap();
        assert_eq!(hp(&world, 1, ListIndex::new(0)), HitPoints::new(25.0));
    }

    /// A built building's death (`FUN_1411dbb70` to state 3 from 2) takes back a completion's
    /// snow (1) with its decal; a farm's (2) stays for its death spawn.
    #[test]
    fn built_house_death_loses_its_completion_snow() {
        let snow_after = |snow| {
            let mut world = game(25.0, false);
            for masters in &mut world.model.masters {
                Arc::make_mut(masters).get_mut(ids::HOUSE).unwrap().building =
                    Some(BuildingMaster::default());
            }
            world.players[1].objects[ListIndex::new(1)].life.state = ObjectState::Alive;
            // the refund's attributes (98 and 165)
            world.players[1].attributes = vec![Amount::ZERO; 200];
            let construction = world
                .parts
                .building
                .get_mut(object_id(11))
                .and_then(|building| building.construction.as_mut())
                .unwrap();
            construction.snow = snow;
            world
                .set_state(player_id(1), object_id(11), ObjectState::Dying)
                .unwrap();
            (
                world.players[1].objects[ListIndex::new(1)].life.state,
                world
                    .parts
                    .building_of(object_id(11))
                    .construction
                    .as_ref()
                    .map(|construction| construction.snow),
            )
        };
        assert_eq!(
            [Snow::None, Snow::Completed, Snow::Kept].map(snow_after),
            [
                (ObjectState::Dying, Some(Snow::None)),
                (ObjectState::Dying, Some(Snow::None)),
                (ObjectState::Dying, Some(Snow::Kept)),
            ]
        );
    }

    #[test]
    fn deleted_foundation_gives_back_what_its_build_time_has_still_to_go() {
        // no builder started it: all of its paid cost, and it is destroyed at once (state 7)
        let mut world = game(0.0, true);
        world.delete(player_id(1), object_id(11)).unwrap();
        let house = &world.players[1].objects[ListIndex::new(1)].life;
        assert_eq!(
            (world.players[1].attributes[1], house.hp, house.state),
            (Amount::new(25.0), HitPoints::ZERO, ObjectState::Removed)
        );
        // 10 of its 25 seconds of work done: 15/25 of the wood (in f32: 15.000001); it dies in
        // its own update
        let mut world = game(10.0, false);
        world.delete(player_id(1), object_id(11)).unwrap();
        let house = &world.players[1].objects[ListIndex::new(1)].life;
        let share = (25.0_f32 - 10.0) / 25.0;
        assert_eq!(
            (world.players[1].attributes[1], house.hp, house.state),
            (
                Amount::new(25.0 * share),
                HitPoints::ZERO,
                ObjectState::Foundation
            )
        );
    }

    /// A town-centre foundation (master 621) deleted, and destroyed at once: without a treaty
    /// monitor (`+0x820`) the delete is any building's.
    #[test]
    fn deleted_town_centre_foundation_without_monitor_is_any_building() {
        let mut world = game(0.0, true);
        as_town_centre(&mut world);
        world.delete(player_id(1), object_id(11)).unwrap();
        let foundation = &world.players[1].objects[ListIndex::new(1)].life;
        assert_eq!(
            (
                world.players[1].attributes[1],
                foundation.hp,
                foundation.state
            ),
            (Amount::new(25.0), HitPoints::ZERO, ObjectState::Removed)
        );
    }

    /// The delete of a town-centre foundation (`FUN_1411cd780`) and its destroy while its owner
    /// has no town centre (`FUN_1411cfb30`) each clear the owner's first zone and its flag in the
    /// world's treaty monitor (`FUN_1414615d0`), whatever the monitor's state; its second zone
    /// and the other players' zones stay. While the owner holds a town centre
    /// (`FUN_1416597e0(owner, 0)`) the destroy leaves the zones, and the delete, which asks no
    /// such thing, clears them still.
    #[test]
    fn deleted_town_centre_foundation_clears_its_owner_first_zone() {
        let drawn = TreatyZone {
            first: TileRect::new(1, 2, 30, 40),
            second: TileRect::new(5, 6, 70, 80),
            set: true,
        };
        let zoned = |progress: f32, state: TreatyState| {
            let mut world = game(progress, false);
            as_town_centre(&mut world);
            let mut monitor = TreatyMonitor::new(
                TreatyMinutes::of_byte(10),
                NomadTreaty::NotForced,
                GameMode::RandomMap,
            );
            monitor.state = state;
            monitor.zones[..2].fill(drawn);
            world.treaty = Some(monitor);
            world
        };
        let cleared = TreatyZone {
            first: TileRect::default(),
            set: false,
            ..drawn
        };
        let zones = |world: &World| world.treaty.as_ref().unwrap().zones[..2].to_vec();
        let mut world = zoned(0.0, TreatyState::Holds);
        world.delete(player_id(1), object_id(11)).unwrap();
        // the foundation, not desolid, stays: only the delete cleared
        assert_eq!(
            world.players[1].objects[ListIndex::new(1)].life.state,
            ObjectState::Foundation
        );
        assert_eq!(zones(&world), [cleared, drawn]);
        let mut world = zoned(0.0, TreatyState::Ended);
        world.destroy(player_id(1), object_id(11)).unwrap();
        assert_eq!(zones(&world), [cleared, drawn]);
        let mut world = zoned(0.0, TreatyState::Holds);
        add_town_centre(&mut world);
        world.destroy(player_id(1), object_id(11)).unwrap();
        assert_eq!(zones(&world), [drawn, drawn]);
        let mut world = zoned(0.0, TreatyState::Holds);
        add_town_centre(&mut world);
        world.delete(player_id(1), object_id(11)).unwrap();
        assert_eq!(zones(&world), [cleared, drawn]);
    }

    /// Player 1's built town centre 13 (master 109), filed in its tracker's done list of master
    /// 109: its owner holds a town centre.
    fn add_town_centre(world: &mut World) {
        let mut town_centre = Unit {
            id: object_id(13),
            ..Unit::default()
        };
        town_centre.life.master_id = ids::TOWN_CENTRE;
        world.players[1].objects.push(town_centre);
        world.reindex();
        world.players[1].tracker.add(
            object_id(13),
            Key {
                master: Some(ids::TOWN_CENTRE),
                class: Some(class::BUILDING),
                pending: false,
                military: false,
            },
        );
        assert_eq!(world.town_centre_at(player_id(1), 0), Some(object_id(13)));
    }

    /// Master 621 for player 1's house foundation 11.
    fn as_town_centre(world: &mut World) {
        for masters in &mut world.model.masters {
            let mut town_centre = masters.get(ids::HOUSE).unwrap().clone();
            town_centre.id = TOWN_CENTRE_621;
            *masters = Arc::new(Masters::new(masters.iter().cloned().chain([town_centre])));
        }
        world.players[1].objects[ListIndex::new(1)].life.master_id = TOWN_CENTRE_621;
    }
}

#[cfg(test)]
mod clearing_tests {
    use super::*;
    use crate::construction::MainBuildingId;
    use crate::construction::build::Clearer;
    use crate::map::Distance;
    use crate::map::Footprint;
    use crate::map::GridSize;
    use crate::map::Point;
    use crate::map::Position;
    use crate::map::Tile;
    #[cfg(test)]
    use crate::map::spatial::Obstruction;
    use crate::map::spatial::{Placed, Spatial, Tiles};
    #[cfg(test)]
    use crate::objects::layers::Static;
    use crate::objects::lifecycle::{Class, Lifecycle};
    use crate::objects::masters::BuildableOn;
    #[cfg(test)]
    use crate::objects::masters::BuildingMaster;
    use crate::objects::masters::CreatablePart;
    use crate::objects::masters::MasterId;
    use crate::objects::masters::MasterKind;
    use crate::objects::masters::ObstructionClass;
    use crate::objects::masters::ObstructionKind;
    use crate::objects::masters::{Master, Masters};
    use crate::objects::unit::Unit;
    use crate::players::attribute::AttributeId;
    use crate::test_ids::costs;
    use crate::test_ids::master_id;
    use crate::test_ids::object_id;
    #[cfg(test)]
    use crate::test_ids::{adds, amounts};
    use crate::testing::attribute;
    use crate::testing::class;
    use crate::testing::ids;
    use crate::testing::roles;
    use crate::testing::slots;
    #[cfg(test)]
    use crate::world::FrameSeed;
    use crate::world::Millis;
    use crate::world::Player;
    use crate::world::WorldTime;
    use std::sync::Arc;

    /// A live object `id` of `master` at `x`, `y`, outside any pasture.
    fn object(id: i32, master: MasterId, x: f32, y: f32) -> Unit {
        Unit {
            id: object_id(id),
            place: Static::at(Position::new(x, y, 0.0)),
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
    fn placed(unit: &Unit, master: &Master, player: PlayerId) -> Placed {
        Placed {
            position: unit.position(),
            radius: master.radius,
            class: master.class,
            player,
            master: master.id,
            obstruction_kind: master.obstruction_kind,
            buildable_on: master.buildable_on,
            tall: master.is_tall(),
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
            kind: MasterKind::STATIC,
            class: class::CLASS_5,
            radius: Footprint::new(0.5, 0.5),
            buildable_on: BuildableOn::YES,
            ..Master::default()
        };
        Masters::new([
            Master {
                id: ids::HOUSE,
                kind: MasterKind::BUILDING,
                class: class::BUILDING,
                radius: Footprint::new(1.0, 1.0),
                ..Master::default()
            },
            deco(master_id(1351)),
            deco(master_id(1358)),
            Master {
                id: master_id(43),
                kind: MasterKind::MOVING,
                class: class::CLASS_11,
                radius: Footprint::new(0.2, 0.2),
                buildable_on: BuildableOn::YES,
                storage: vec![(Some(attribute::RELICS_HELD), Amount::new(1.0), 32)],
                ..Master::default()
            },
            Master {
                id: master_id(254),
                kind: MasterKind::COMBAT,
                class: class::CLASS_40,
                radius: Footprint::new(0.5, 0.5),
                height: Distance::new(1.0),
                obstruction_kind: ObstructionKind::KIND_2,
                buildable_on: BuildableOn::YES,
                ..Master::default()
            },
            Master {
                id: ids::ID_357,
                kind: MasterKind::BUILDING,
                class: class::CLASS_11,
                radius: Footprint::new(1.5, 1.5),
                buildable_on: BuildableOn::YES,
                building: Some(BuildingMaster {
                    stack_unit: None,
                    foundation_terrain: None,
                    tech: None,
                    annexes: [None; 4],
                    head_unit: None,
                    transform_unit: None,
                    ..BuildingMaster::default()
                }),
                ..Master::default()
            },
        ])
    }

    /// Gaia holding `gaia` (live) and `asleep`, player 1 holding `own`, on a 20 by 20 map.
    fn world(gaia: Vec<Unit>, asleep: Vec<Unit>, own: Vec<Unit>) -> World {
        let ms = masters();
        let mut world = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(1),
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
        world.spatial = Spatial::with_tiles(
            GridSize::new(20, 20),
            Tiles::new(GridSize::new(20, 20), vec![0; 400]),
        );
        for player_id in PlayerId::each(2) {
            let all: Vec<Unit> = world.players[player_id.index()]
                .objects
                .iter()
                .chain(&world.players[player_id.index()].sleeping)
                .cloned()
                .collect();
            for unit in all {
                let master = ms.get(unit.life.master_id).expect("a test master");
                let mut at = placed(&unit, master, player_id);
                if master.obstruction_kind == ObstructionKind::KIND_2 {
                    at.obstruction = Some(Obstruction {
                        kind: ObstructionKind::KIND_2,
                        class: ObstructionClass::NONE,
                    });
                }
                world.spatial.insert(unit.id, at);
            }
        }
        world.model.masters = vec![Arc::new(ms.clone()), Arc::new(ms)];
        world
    }

    /// records/429859027's update 19 (and 161 more stops of census v5): a foundation's
    /// constructor clears two of Gaia's sleeping decorations off one tile (`FUN_1415c3620`): each
    /// in its tile's list order is destroyed (`FUN_14118f1e0`): its hit points go, it wakes to
    /// the end of Gaia's live list and goes to state 7.
    #[test]
    fn footprint_clearing_wakes_two_sleeping_decorations_in_their_tile_order() {
        let mut world = world(
            vec![object(1, master_id(1351), 3.5, 3.5)],
            vec![
                object(6084, master_id(1358), 10.6, 10.6),
                object(6083, master_id(1351), 10.4, 10.4),
            ],
            vec![],
        );
        assert_eq!(
            world.spatial.tiles().list(Tile::new(10, 10)),
            [object_id(6084), object_id(6083)]
        );
        world
            .clear_footprint(
                PlayerId::new(1),
                ids::HOUSE,
                Point::new(10.5, 10.5),
                Clearer::Constructor,
            )
            .unwrap();
        let ids: Vec<ObjectId> = world.players[0]
            .objects
            .iter()
            .map(|unit| unit.id)
            .collect();
        assert_eq!(ids, [object_id(1), object_id(6084), object_id(6083)]);
        assert_eq!(world.players[0].sleeping.len(), 0);
        for unit in &world.players[0].objects.as_slice()[1..] {
            assert_eq!(
                (unit.life.state, unit.life.hp),
                (ObjectState::Removed, HitPoints::ZERO)
            );
        }
        assert_eq!(
            world.players[0].objects[ListIndex::new(0)].life.state,
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
        let mut world = world(vec![], vec![], vec![corpse]);
        world.players[1].attributes[7] = Amount::new(3.0);
        world
            .clear_footprint(
                PlayerId::new(1),
                ids::HOUSE,
                Point::new(10.5, 10.5),
                Clearer::Constructor,
            )
            .unwrap();
        let unit = &world.players[1].objects[ListIndex::new(0)];
        assert_eq!(
            (unit.life.state, unit.life.hp),
            (ObjectState::Removed, HitPoints::ZERO)
        );
        assert_eq!(world.players[1].attributes[7], Amount::new(2.0));
    }

    /// A pile (type 70) a new object's clearing (`param_6`) destroys: its action list goes
    /// (`FUN_14106c420`), and the combat object's `set_state(7)` (`FUN_14121cc40`) drops its
    /// record and its height and sets `+0x686`; it gets no refund (only state 3 from 2 does).
    #[test]
    fn cleared_pile_loses_its_record_in_its_destroy() {
        let mut world = world(vec![object(40, master_id(254), 10.5, 10.5)], vec![], vec![]);
        world.players[0].attributes[98] = Amount::new(5.0);
        assert!(
            world
                .spatial
                .get(object_id(40))
                .unwrap()
                .obstruction
                .is_some()
        );
        world
            .clear_footprint(
                PlayerId::new(1),
                ids::HOUSE,
                Point::new(10.5, 10.5),
                Clearer::Builder,
            )
            .unwrap();
        let unit = &world.players[0].objects[ListIndex::new(0)];
        assert_eq!(
            (
                unit.life.state,
                unit.life.hp,
                unit.flags.contains(UnitFlags::ROO_CALLED)
            ),
            (ObjectState::Removed, HitPoints::ZERO, true)
        );
        let placed = world.spatial.get(object_id(40)).unwrap();
        assert_eq!((placed.obstruction, placed.tall), (None, false));
        assert_eq!(world.players[0].attributes[98], Amount::new(5.0));
    }

    /// A dead farm (`FARM_D`, a building of type 80) a new farm's constructor clears: the
    /// building's destroy (`FUN_1411cfb30`): its storage leaves in state 2, its action list goes,
    /// the base destroy takes it to state 7 through the building's and the combat object's
    /// `set_state`, and its owner's watched list lets it go.
    #[test]
    fn cleared_dead_farm_goes_through_the_building_destroy() {
        let mut world = world(vec![], vec![], vec![object(50, ids::ID_357, 10.5, 10.5)]);
        world
            .clear_footprint(
                PlayerId::new(1),
                ids::HOUSE,
                Point::new(10.5, 10.5),
                Clearer::Constructor,
            )
            .unwrap();
        let unit = &world.players[1].objects[ListIndex::new(0)];
        assert_eq!(
            (
                unit.life.state,
                unit.life.hp,
                unit.flags.contains(UnitFlags::ROO_CALLED)
            ),
            (ObjectState::Removed, HitPoints::ZERO, true)
        );
    }

    /// The base `set_state` (`FUN_14119d710`) of a decoration (type 10): the same or a lower
    /// state changes nothing; from below 3 its storage of flags 2 and 16 leaves its owner (that
    /// of flag 32 only at 7, a kind below 0 never); a sleeping object wakes to the end of the
    /// live list unless the state is 6; an object the world does not hold is not modelled.
    #[test]
    fn the_base_set_state_lets_its_storage_go_and_wakes_sleeper() {
        let mut world = world(
            vec![object(1, master_id(1351), 3.5, 3.5)],
            vec![
                object(2, master_id(1358), 5.5, 5.5),
                object(3, master_id(1358), 7.5, 7.5),
            ],
            vec![],
        );
        for master in [1351, 1358] {
            Arc::make_mut(&mut world.model.masters[0])
                .get_mut(master_id(master))
                .unwrap()
                .storage = vec![
                (Some(attribute::HEADROOM), Amount::new(2.0), 2),
                (Some(AttributeId::named(5)), Amount::new(3.0), 16),
                (Some(attribute::RELICS_HELD), Amount::new(1.0), 32),
                (None, Amount::new(9.0), 2),
            ];
        }
        let at = |world: &World| [4, 5, 7].map(|attribute| world.players[0].attributes[attribute]);
        world.players[0].attributes[4..8].copy_from_slice(&[
            Amount::new(10.0),
            Amount::new(10.0),
            Amount::new(10.0),
            Amount::new(10.0),
        ]);
        world.players[0].objects[ListIndex::new(0)].life.state = ObjectState::Alive;
        // the same state, then a lower one: nothing
        world
            .set_state(PlayerId::new(0), object_id(1), ObjectState::Alive)
            .unwrap();
        world.players[0].objects[ListIndex::new(0)].life.state = ObjectState::Dead;
        world
            .set_state(PlayerId::new(0), object_id(1), ObjectState::Dying)
            .unwrap();
        assert_eq!(
            world.players[0].objects[ListIndex::new(0)].life.state,
            ObjectState::Dead
        );
        assert_eq!(at(&world), amounts([10.0; 3]));
        // from 2 to 5: flags 2 and 16 leave, flag 32 stays; then to 7, flag 32 leaves
        world.players[0].objects[ListIndex::new(0)].life.state = ObjectState::Alive;
        world
            .set_state(PlayerId::new(0), object_id(1), ObjectState::Dead)
            .unwrap();
        assert_eq!(at(&world), amounts([8.0, 7.0, 10.0]));
        world
            .set_state(PlayerId::new(0), object_id(1), ObjectState::Removed)
            .unwrap();
        assert_eq!(at(&world), amounts([8.0, 7.0, 9.0]));
        assert_eq!(
            world.players[0].objects[ListIndex::new(0)].life.state,
            ObjectState::Removed
        );
        // a sleeper set to 6 sleeps on, one set to 7 wakes
        world.players[0].sleeping[ListIndex::new(0)].life.state = ObjectState::Dead;
        world
            .set_state(PlayerId::new(0), object_id(2), ObjectState::Undead)
            .unwrap();
        assert_eq!(world.players[0].sleeping.len(), 2);
        world
            .set_state(PlayerId::new(0), object_id(3), ObjectState::Removed)
            .unwrap();
        let ids: Vec<ObjectId> = world.players[0]
            .objects
            .iter()
            .map(|unit| unit.id)
            .collect();
        assert_eq!(
            (ids, world.players[0].sleeping.len()),
            (vec![object_id(1), object_id(3)], 1)
        );
        assert!(
            world
                .set_state(PlayerId::new(0), object_id(99), ObjectState::Removed)
                .is_err()
        );
    }

    /// `FUN_14121cc40`'s refund at a death from state 2: a unit's paid cost leaves attributes 98
    /// and 164, a building's 98 and 165, the castle's and the wonder's 98 alone, an annex's none.
    #[test]
    fn death_refunds_its_paid_cost_by_its_kind() {
        let unit = Master {
            kind: MasterKind::COMBAT,
            creatable: Some(CreatablePart {
                cost: costs(&[
                    (attribute::FOOD, 50, true),
                    (attribute::GOLD, 20, true),
                    (attribute::WOOD, 5, false),
                ]),
                ..CreatablePart::default()
            }),
            ..Master::default()
        };
        assert_eq!(
            refund_of(&unit, None, &roles(), slots()),
            adds([
                (attribute::TOTAL_VALUE, 70.0),
                (attribute::UNITS_VALUE, 70.0)
            ])
        );
        let house = Master {
            id: ids::HOUSE,
            kind: MasterKind::BUILDING,
            ..unit
        };
        assert_eq!(
            refund_of(&house, None, &roles(), slots()),
            adds([
                (attribute::TOTAL_VALUE, 70.0),
                (attribute::BUILDINGS_VALUE, 70.0)
            ])
        );
        assert_eq!(
            refund_of(
                &house,
                Some(MainBuildingId::new(object_id(9))),
                &roles(),
                slots()
            ),
            []
        );
        let castle = Master {
            id: ids::CASTLE,
            ..house
        };
        assert_eq!(
            refund_of(&castle, None, &roles(), slots()),
            adds([(attribute::TOTAL_VALUE, 70.0)])
        );
    }
}
