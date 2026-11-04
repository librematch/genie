//! The town bell's commands: `TOWN_BELL` (0x7f) rings a town centre's bell or ends it, and
//! the back-to-work command sends the owner's villagers back without the bell's flags.

use crate::GenieResult;
use crate::commands::Bell;
use crate::garrison::ContainerId;
use crate::garrison::ShelterReach;
use crate::garrison::Shelters;
use crate::math::fmath::below_or_unordered;
use crate::movement::footprint_gap_squared;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::GarrisonKinds;
use crate::objects::masters::MasterKind;
use crate::orders::Dispatch;
use crate::orders::town_bell::BellShelter;
use crate::players::PlayerId;
use crate::players::tracker::ListKey;
use crate::world::World;
use crate::world::layer::{ClassRoles, MasterRoles};
use crate::world::settings::OptionFlags;
use crate::world::slots::PlayerSlots;

impl World {
    /// `TOWN_BELL` (0x7f, `do_command`): the object `id` (none for −1, `FUN_140e6bad0`) of the
    /// commanding `player`, or of any player while players may order each other's
    /// ([`crate::world::settings::OptionFlags::CHEATS_ENABLED`], `FUN_1412ffe00`; the cheat
    /// `+0xb12` is not held), rings its owner's bell ([`Bell::Ring`],
    /// [`crate::world::World::ring_town_bell`]) or ends it
    /// ([`crate::world::World::end_town_bell`]). A special object's (`+0x30c` bit 0) UI flag is not
    /// simulation.
    pub(crate) fn town_bell(
        &mut self,
        player: PlayerId,
        id: ObjectId,
        bell: Bell,
    ) -> GenieResult<()> {
        let Some((owner, _)) = self.object(id) else {
            return Ok(());
        };
        if owner != player && !self.settings.flags.contains(OptionFlags::CHEATS_ENABLED) {
            return Ok(());
        }
        match bell {
            Bell::Ring => self.ring_town_bell(owner, id),
            Bell::End => self.end_town_bell(owner, id),
        }
    }

    /// `FUN_14163a650`: player `owner`'s town centre `tc` (master 109; any other object does
    /// nothing) rings: of its owner's live objects, in the list's order, those in state 2 not
    /// inside another (`FUN_141188da0`) within `GarrisonTuning::town_bell_reach` of it (`FUN_14118f8c0`; `COMISS`/`JC`: a
    /// NaN gap is out) are the units, a villager (class 4 of type 70) or master 1808, else the
    /// buildings (type 80) that take villagers
    /// ([`crate::objects::masters::GarrisonKinds::VILLAGERS`], `FUN_1411ccb60`), each with its
    /// count ([`crate::world::World::garrisoned`]) and its bell flag (`+0x9dc`) set (its sound,
    /// slot 0x4c0, is not simulation); then [`crate::world::World::shelter`] with the reach
    /// (`param_6` 1), no transports, not through the unit AIs.
    pub(crate) fn ring_town_bell(&mut self, owner: PlayerId, tc: ObjectId) -> GenieResult<()> {
        let Some((_, tc_unit)) = self.object(tc) else {
            return Ok(());
        };
        // The town centre, whose bell the command rings (`0x6109` raw).
        // The master that answers the bell besides the villagers (`0x710`).
        // The villagers' class.
        // The units' least type (`0x46`) and the buildings' (`'P'`).
        if !self
            .model
            .roles
            .of_master(tc_unit.life.master_id)
            .contains(MasterRoles::TOWN_CENTRE)
        {
            return Ok(());
        }
        let (at, radius) = self.ground(tc)?;
        let mut units = Vec::new();
        let mut shelters = Shelters::default();
        let mut list_index = ListIndex::FIRST;
        while self.players.player(owner)?.objects.holds(list_index) {
            let here = list_index;
            let unit = &self.players.live(owner, here)?;
            list_index = list_index.next();
            if unit.life.state != ObjectState::Alive || unit.life.inside.is_some() {
                continue;
            }
            let id = unit.id;
            let m = unit.master(self.master_tables(), owner)?;
            let (kind, class, master) = (m.kind, m.class, m.id);
            let takes_villagers = m.building.is_some_and(|building_master| {
                building_master
                    .garrison_kind
                    .contains(GarrisonKinds::VILLAGERS)
            });
            let near = |world: &Self| -> GenieResult<bool> {
                let (other_at, other_radius) = world.ground(id)?;
                let gap = footprint_gap_squared(other_at, other_radius, at, radius);
                // COMISS reach, gap; JC: a gap above it, or a NaN, is out
                Ok(!below_or_unordered(
                    world.model.tuning.garrison.town_bell_reach,
                    gap.get(),
                ))
            };
            if (self
                .model
                .roles
                .of_class(class)
                .contains(ClassRoles::VILLAGER)
                && kind == MasterKind::COMBAT
                && near(self)?)
                || (self
                    .model
                    .roles
                    .of_master(master)
                    .contains(MasterRoles::MULE_CART)
                    && near(self)?)
            {
                units.push(id);
            } else if kind == MasterKind::BUILDING && takes_villagers && near(self)? {
                shelters.buildings.push(ContainerId::new(id));
                shelters.counts.push(self.garrisoned(id)?);
                self.parts.set_town_bell_flag(id, BellShelter::Sheltered);
            }
        }
        self.shelter(
            owner,
            &units,
            &shelters,
            ShelterReach::TownBell,
            Dispatch::Now,
        )
    }

    /// `FUN_14163ab80`: player `owner`'s town centre `tc` (master 109; any other object does
    /// nothing) ends its bell. Each of its owner's live objects, in the list's order, of type 80
    /// in a state below 3 that takes villagers (`FUN_1411ccb60` with 1) and holds its bell flag
    /// (`+0x9dc`) drops the flag; within `GarrisonTuning::town_bell_reach` of the centre (`COMISS`/`JC`: a NaN gap is
    /// out) it lets each unit inside of type 70 or more that is a villager (class 4) or of master
    /// 1808 back to work, from the last in ([`crate::world::World::back_to_work`]; its sound, slot
    /// 0x4c0, is not simulation). Then every unit its owner's tracker files under class 4
    /// (`FUN_14104f8f0`) and master 1808 (`FUN_14104f960`), in that order, not inside another
    /// (`+0x2f0`) goes back to work from where it is.
    pub(crate) fn end_town_bell(&mut self, owner: PlayerId, tc: ObjectId) -> GenieResult<()> {
        let Some((_, tc_unit)) = self.object(tc) else {
            return Ok(());
        };
        if !self
            .model
            .roles
            .of_master(tc_unit.life.master_id)
            .contains(MasterRoles::TOWN_CENTRE)
        {
            return Ok(());
        }
        let (at, radius) = self.ground(tc)?;
        let mut list_index = ListIndex::FIRST;
        while self.players.player(owner)?.objects.holds(list_index) {
            let idx = list_index;
            list_index = list_index.next();
            let unit = &self.players.live(owner, idx)?;
            let master = unit.master(self.master_tables(), owner)?;
            let takes_villagers = master
                .building
                .is_some_and(|x| x.garrison_kind.contains(GarrisonKinds::VILLAGERS));
            if master.kind != MasterKind::BUILDING
                || ObjectState::Dying <= unit.life.state
                || !takes_villagers
                || !self.parts.town_bell_of(unit.id).flag.is_sheltered()
            {
                continue;
            }
            let id = unit.id;
            self.parts.set_town_bell_flag(id, BellShelter::Free);
            let (building_at, building_radius) = self.ground(id)?;
            // COMISS reach, gap; JC: a gap above it, or a NaN, lets none out
            let gap = footprint_gap_squared(building_at, building_radius, at, radius);
            if below_or_unordered(self.model.tuning.garrison.town_bell_reach, gap.get()) {
                continue;
            }
            self.workers_out(owner, idx)?;
        }
        self.workers_back(owner)
    }

    /// `ALL_BACK_TO_WORK` (`do_command` 0x8c; another player's object only while players may order
    /// each other's, [`crate::world::settings::OptionFlags::CHEATS_ENABLED`]; the cheat `+0xb12` is
    /// not held): `FUN_141617420` for the owner of object `id`, as the bell's end
    /// ([`crate::world::World::end_town_bell`]) without its flags: each of the owner's live
    /// objects, in the list's order, in state 2, not inside another (`+0x2f0`), of type 80, that
    /// takes villagers (`FUN_1411ccb60` with 1) and is within `GarrisonTuning::town_bell_reach` of
    /// the object (`COMISS`/`JC`: a NaN gap is out) lets its villagers and units of master 1808 out
    /// back to work, from the last in; then the units its owner's tracker files under class 4 and
    /// master 1808 not inside another go back from where they are. The game screen's flag
    /// (`+0x1874`) is not simulation.
    pub(crate) fn all_back_to_work(&mut self, player: PlayerId, id: ObjectId) -> GenieResult<()> {
        let Some((owner, _)) = self.object(id) else {
            return Ok(());
        };
        if owner != player && !self.settings.flags.contains(OptionFlags::CHEATS_ENABLED) {
            return Ok(());
        }
        let (at, radius) = self.ground(id)?;
        let mut list_index = ListIndex::FIRST;
        while self.players.player(owner)?.objects.holds(list_index) {
            let idx = list_index;
            list_index = list_index.next();
            let unit = &self.players.live(owner, idx)?;
            let master = unit.master(self.master_tables(), owner)?;
            let takes_villagers = master
                .building
                .is_some_and(|x| x.garrison_kind.contains(GarrisonKinds::VILLAGERS));
            if unit.life.state != ObjectState::Alive
                || unit.life.inside.is_some()
                || master.kind != MasterKind::BUILDING
                || !takes_villagers
            {
                continue;
            }
            let (building_at, building_radius) = self.ground(unit.id)?;
            // COMISS reach, gap; JC: a gap above it, or a NaN, lets none out
            let gap = footprint_gap_squared(building_at, building_radius, at, radius);
            if below_or_unordered(self.model.tuning.garrison.town_bell_reach, gap.get()) {
                continue;
            }
            self.workers_out(owner, idx)?;
        }
        self.workers_back(owner)
    }

    /// The building at `idx` of player `owner` lets each unit inside of type 70 or more that is a
    /// villager (class 4) or of master 1808 back to work, from the last in
    /// ([`crate::world::World::back_to_work`]; its sound, slot 0x4c0, is not simulation).
    fn workers_out(&mut self, owner: PlayerId, idx: ListIndex) -> GenieResult<()> {
        let mut live = self.players.live(owner, idx)?.garrison.len();
        while 0 < live {
            live -= 1;
            let Some(&inside) = self.players.live(owner, idx)?.garrison.get(live) else {
                continue;
            };
            // any player's unit; an id the world no longer holds is a null entry
            let Some((up, unit)) = self.object(inside) else {
                continue;
            };
            let um = unit.master(self.master_tables(), up)?;
            if MasterKind::COMBAT <= um.kind
                && (self
                    .model
                    .roles
                    .of_class(um.class)
                    .contains(ClassRoles::VILLAGER)
                    || self
                        .model
                        .roles
                        .of_master(um.id)
                        .contains(MasterRoles::MULE_CART))
            {
                let _ = self.back_to_work(Some((owner, idx)), inside)?;
            }
        }
        Ok(())
    }

    /// Every unit player `owner`'s tracker files under class 4 (`FUN_14104f8f0`) and master
    /// 1808 (`FUN_14104f960`), in that order, not inside another (`+0x2f0`) goes back to work
    /// from where it is.
    fn workers_back(&mut self, owner: PlayerId) -> GenieResult<()> {
        let tracker = &self.players.player(owner)?.tracker;
        let mut ids = tracker.class_list(self.model.slots.class.villager).to_vec();
        ids.extend_from_slice(tracker.list(ListKey::master(self.model.slots.master.mule_cart)));
        // a queued unit's place (−1) names no object
        for id in ids.into_iter().flatten() {
            // the tracker's keepers follow every object its player gains and loses: the world
            // holds each (the game reads it without a test)
            let Some((_, unit)) = self.object(id) else {
                continue;
            };
            if unit.life.inside.is_none() {
                let _ = self.back_to_work(None, id)?;
            }
        }
        Ok(())
    }
}
