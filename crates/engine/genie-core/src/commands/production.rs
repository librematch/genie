//! QUEUE: a building's production queue, its load, its units and technologies, and a new
//! entry's costs.

use crate::commands::ONE_UNIT_AGE;
use crate::commands::queue::QueueCommand;
use crate::commands::research::ResearchCommand;
use crate::math::rng::Stream;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::PlayersMastersExt;
use crate::objects::masters::cost_entries;
use crate::objects::masters::{Masters, player_master};
use crate::objects::unit::Unit;
use crate::orders::NO_QUEUE;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::production::CostCount;
use crate::production::Payment;
use crate::production::QueueButton;
use crate::production::QueueOutcome;
use crate::production::QueuedUnits;
use crate::production::{Making, ROOM, costed, same_costs, train_time};
use crate::technology::TechId;
use crate::technology::{Attributes, factors};
use crate::world::GameMode;
use crate::world::Seconds;
use crate::world::WholeSeconds;
use crate::world::index::Place;
use crate::world::layer::AttributeRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::player::Player;
use crate::world::slots::CheckedIndexExt;
use crate::world::slots::PlayerSlotsExt;
use crate::world::{COMPUTER_QUEUE_LIMIT, QUEUE_LIMIT, World, villager};
use crate::{GenieError, GenieResult};

impl World {
    /// QUEUE, the player's part (`FUN_141614450`) after the packet's check of its buildings
    /// (`do_command`'s case 0x81, [`World::command_check`]: an id the world lacks, or another
    /// player's without cheats, goes; a human's packet left with none is refused). Nothing for a
    /// master the player lacks (`FUN_141659d20`). Of the first 60 buildings (500 for a computer
    /// player) those the master is available to (`+0x5f0`) or that list it themselves (`+0xd30`,
    /// `FUN_1411c8fd0`) take part, at most 60: each where the unit trains ([`World::trains_at`],
    /// `FUN_141263610`) has its room (15, a computer player's 2, less its queued total `+0xbf4`;
    /// the sum of those above 0 holds the amount) and its load ([`World::production_load`]); any
    /// other has room −1 and takes none. The amount is lowered while the player cannot pay that
    /// many: each cost entry of the costed master ([`crate::production::costed`]; an annex's paid
    /// ones) but the population's needs its amount times the count (`COMISS`, `JA`: a NaN passes);
    /// at one still short, or for a master below type 70, nothing is queued. In game mode 16, while
    /// the player's attribute 6 is at most 1, a military master (its key's `+9`,
    /// [`crate::players::tracker::Key::military`]) is queued one at a time: with the player's flag
    /// (`+0x1378`) set nothing is, else the flag sets and the amount is 1 (even with no room). Then
    /// unit by unit the building with room and the least load takes it (the last one's pick first,
    /// replaced by one whose load is below: `COMISS`, `JBE`, a NaN never wins; with none at the
    /// start, the most room), its room one less and its load the unit's train time there
    /// ([`crate::production::train_time`]); last, each building's part ([`World::queue_at`]) with
    /// its count, the packet's master and button. The limited units' tests before the mode 16 check
    /// (`FUN_1416116c0`, `FUN_141611520`: an amount above the master's `+0x5f0` lowered, none at 1)
    /// read the `+0x5f1` bits 1 and 2 of the unit or its linked master; genie's masters keep
    /// `+0x5f1` as a boolean (the `.dat` holds 0 for every master), so they pass
    /// ([`World::trainable_count`]).
    ///
    /// # Errors
    ///
    /// A sleeping object the unit trains at, and what the parts do not model.
    pub(crate) fn queue(
        &mut self,
        queue_command: &QueueCommand,
        buildings: &[Option<ObjectId>],
    ) -> GenieResult<()> {
        let (player, unit_id) = (queue_command.player, queue_command.unit);
        let Some(ids) = self.command_check(player, buildings)? else {
            return Ok(());
        };
        let owner = self.players.player(player)?;
        let limit = if owner.kind.is_computer() { 500 } else { 60 };
        let empty = Masters::default();
        let masters = self.model.masters.of(player).unwrap_or(&empty);
        let Some(unit) = masters.get(unit_id) else {
            return Ok(());
        };
        let found = self.taking_part(ids.get(..limit).unwrap_or(&ids), unit, unit_id)?;
        if found.is_empty() {
            return Ok(());
        }
        let per = if owner.kind.is_computer() {
            COMPUTER_QUEUE_LIMIT
        } else {
            QUEUE_LIMIT
        } as i32;
        let (rooms, loads, total_room) = self.rooms_and_loads(&found, unit, per)?;
        let mut amount = i32::from(queue_command.amount.get()).min(i32::from(total_room as i16));
        if 0 < amount {
            if unit.kind < MasterKind::COMBAT {
                return Ok(());
            }
            let Some(paid) = affordable(owner, masters, unit, amount, &self.model.roles)? else {
                return Ok(());
            };
            amount = paid;
        }
        // game mode 16: with attribute 6 at most 1 (COMISS, JC: a NaN is not) a military unit
        // (its key's +9) is queued one at a time, the player's +0x1378 its flag
        if self.settings.game_mode == GameMode::OneUnit
            // The player's attribute game mode 16's QUEUE reads (6, `+0x30`), and its bound
            // (`DAT_14330eb8c`).
            && Attributes::get(owner, self.model.slots.attribute.current_age)? <= Amount::new(ONE_UNIT_AGE)
            && self.tracker_key(player, unit_id).military
        {
            if owner.one_military_queued {
                return Ok(());
            }
            self.players.player_mut(player)?.one_military_queued = true;
            amount = 1;
        }
        let Some(counts) =
            self.shares(&found, (rooms, loads), unit, queue_command.button, amount)?
        else {
            return Ok(());
        };
        for (&place, &count) in found.iter().zip(&counts) {
            if 0 < count {
                // the building's part works for its owner (`FUN_1411cbc40`'s `+0x138`)
                let Place::Live(player_id, list_index) = place else {
                    return Err(GenieError::unsupported(
                        "QUEUE at a sleeping object the unit trains at",
                    ));
                };
                let _ = self.queue_at(
                    player_id,
                    list_index,
                    Some(unit_id),
                    count,
                    None,
                    queue_command.button,
                )?;
            }
        }
        Ok(())
    }

    /// [`World::queue`]'s buildings that take part among `ids`: those `unit` (of id `unit_id`)
    /// is available to or that list it themselves, at most 60.
    fn taking_part(
        &self,
        ids: &[Option<ObjectId>],
        unit: &Master,
        unit_id: MasterId,
    ) -> GenieResult<Vec<Place>> {
        let mut found = Vec::new();
        for place in self.lookup(ids).into_iter().flatten() {
            let (_, building) = self.at(place)?;
            let listed = self
                .parts
                .production
                .get(building.id)
                .is_some_and(|production| production.available_units.contains(&unit_id));
            if unit.available.is_available() || listed {
                found.push(place);
            }
        }
        found.truncate(60);
        Ok(found)
    }

    /// [`World::queue`]'s count of `amount` units of `unit` per building of `found`: unit by
    /// unit the building with room and the least load takes it ([`least_loaded`]), its room one
    /// less and its load the unit's train time there for `button`. None when no building has
    /// room for a unit: nothing is queued then.
    fn shares(
        &self,
        found: &[Place],
        (mut rooms, mut loads): (Vec<i32>, Vec<Seconds>),
        unit: &Master,
        button: QueueButton,
        mut amount: i32,
    ) -> GenieResult<Option<Vec<u16>>> {
        let mut counts = vec![0u16; found.len()];
        let mut pick: Option<usize> = None;
        while 0 < amount {
            amount -= 1;
            let Some(least) = least_loaded(&rooms, &loads, pick) else {
                return Ok(None);
            };
            *rooms.checked_mut(least)? -= 1;
            *counts.checked_mut(least)? += 1;
            let at = self.at(*found.checked(least)?)?.1.life.master_id;
            let time = train_time(&unit.creatable_part().train_locations, at, button);
            *loads.checked_mut(least)? += Seconds::of_master(time);
            pick = Some(least);
        }
        Ok(Some(counts))
    }

    /// The object at `place`, with its player.
    fn at(&self, place: Place) -> GenieResult<(PlayerId, &Unit)> {
        Ok(match place {
            Place::Live(owner, slot) => (owner, self.players.live(owner, slot)?),
            Place::Sleeping(player_id, list_index) => {
                (player_id, self.players.asleep(player_id, list_index)?)
            }
        })
    }

    /// [`World::queue`]'s rooms and loads of the `found` buildings for `unit`, each with `per`
    /// places: a building the unit does not train at keeps room −1 and load 0; the sum of the
    /// rooms (0 at least) comes third.
    fn rooms_and_loads(
        &self,
        found: &[Place],
        unit: &Master,
        per: i32,
    ) -> GenieResult<(Vec<i32>, Vec<Seconds>, i32)> {
        let mut rooms = Vec::with_capacity(found.len());
        let mut loads = Vec::with_capacity(found.len());
        let mut total_room = 0i32;
        for &place in found {
            let (player_id, building) = self.at(place)?;
            // a building the unit does not train at keeps room −1 and load 0
            if !self.trains_at(unit, building.life.master_id) {
                rooms.push(-1);
                loads.push(Seconds::ZERO);
                continue;
            }
            let total = self
                .parts
                .production
                .get(building.id)
                .map_or(0, |production| production.queue.total.get());
            let room = per - i32::from(total);
            total_room += room.max(0);
            rooms.push(room);
            loads.push(self.production_load(player_id, building)?);
        }
        Ok((rooms, loads, total_room))
    }

    /// RESEARCH's building (`0x140e7e270`): the packet's (`+4`, a building or none); with more
    /// than one listed (`+0x8` as a short; at most 60, 500 for a computer player,
    /// `FUN_140e6b250`), each listed one the world holds (`+0x11..`, `FUN_140e6bad0`) whose load
    /// ([`World::production_load`]) is below the one taken (`COMISS`, `JBE`: a NaN keeps it)
    /// takes its place, of `buildings`, the listed ones the check left (none for the others).
    /// None for none.
    pub(crate) fn least_loaded(
        &self,
        research_command: &ResearchCommand,
        buildings: &[Option<ObjectId>],
    ) -> GenieResult<Option<ObjectId>> {
        let load = |id: ObjectId| -> GenieResult<Seconds> {
            self.object(id)
                .map_or(Ok(Seconds::ZERO), |(player_id, unit)| {
                    self.production_load(player_id, unit)
                })
        };
        let is_building = |id: Option<ObjectId>| -> GenieResult<bool> {
            match self.object(id) {
                Some((player_id, unit)) => {
                    Ok(unit.master(self.master_tables(), player_id)?.kind == MasterKind::BUILDING)
                }
                None => Ok(false),
            }
        };
        let mut chosen = if is_building(research_command.building)? {
            research_command.building
        } else {
            None
        };
        if 1 < research_command.count {
            let limit = if self
                .players
                .player(research_command.player)?
                .kind
                .is_computer()
            {
                500
            } else {
                60
            };
            let count = usize::from(research_command.count as u16).min(limit);
            for id in buildings.iter().take(count).flatten().copied() {
                if self.object(id).is_none() {
                    continue;
                }
                let heavier = match chosen {
                    Some(object_id) => load(object_id)? > load(id)?,
                    None => true,
                };
                if heavier {
                    chosen = Some(id);
                }
            }
        }
        Ok(chosen)
    }

    /// `FUN_1411d0e90`: the seconds of work `building` (live or sleeping) of player `player_id` has
    /// queued: each queue entry's count times its time (a unit's master's first train time,
    /// `+0x898`, in integers; a research's location at the building, `FUN_1413cbd20`, its time plus
    /// its adjustment, as floats), less the work done of its production list's action (a make's
    /// `+0x6c`, a research's `+0x70`).
    pub(crate) fn production_load(
        &self,
        player_id: PlayerId,
        building: &Unit,
    ) -> GenieResult<Seconds> {
        let owner = self.players.player(player_id)?;
        let Some(production) = self.parts.production.get(building.id) else {
            return Ok(Seconds::ZERO);
        };
        let queue = &production.queue;
        let entries = queue.count.slots().min(queue.slots.len());
        let mut queued = Seconds::ZERO;
        for entry in queue.slots.head(entries)? {
            if let Some(master) = entry.master {
                let time = player_master(&self.model.masters, player_id, master)
                    .and_then(|master| master.creatable_part().train_locations.first())
                    .map_or(WholeSeconds::ZERO, |train_location| train_location.time);
                queued +=
                    Seconds::new((i32::from(entry.count.get()) * i32::from(time.get())) as f32);
            } else if let Some(tech) = entry.tech {
                // a queued research has its locations (the queue checked them)
                let (time, adjustment) = Some(tech.index())
                    .and_then(|index| owner.techs.get(index))
                    .and_then(|tech_state| tech_state.location(building.life.master_id))
                    .map_or((WholeSeconds::ZERO, Seconds::ZERO), |location| {
                        (location.time, location.adjustment)
                    });
                queued += (Seconds::of_master(time) + adjustment) * f32::from(entry.count.get());
            }
        }
        if queue.slots.is_empty() {
            return Ok(queued);
        }
        let done = match production.action {
            Some(Making::Unit(make_obj)) => make_obj.progress,
            Some(Making::Research(make_tech)) => make_tech.progress,
            None => Seconds::ZERO,
        };
        Ok(queued - (Seconds::ZERO + done))
    }

    /// `FUN_1411cbc40`, QUEUE's and RESEARCH's building part, at the building at `list_index` of
    /// `player`: `count` of master `unit`, or with `tech` other than −1 that technology (`unit`
    /// −1), with the train button `button`. The generic villager (0x712) is the villager
    /// (0x53), whose sex the Action stream draws for each: the routine queues the rest first,
    /// one by one, then this one (the last drawn is queued first). A building whose total
    /// (`+0xbf4`) is above 14 refuses (its event 0x84 is the interface's); the count is held
    /// to the room of 15. Units: [`World::queue_units`]; researches: [`World::queue_techs`].
    /// Queued when any was.
    pub(crate) fn queue_at(
        &mut self,
        player: PlayerId,
        list_index: ListIndex,
        unit: Option<MasterId>,
        count: u16,
        tech: Option<TechId>,
        button: QueueButton,
    ) -> GenieResult<QueueOutcome> {
        let mut master = if self
            .model
            .roles
            .of_some_master(unit)
            .contains(MasterRoles::VILLAGER_ANY)
        {
            Some(self.model.slots.master.villager_male)
        } else {
            unit
        };
        let mut count = count;
        if self
            .model
            .roles
            .of_some_master(master)
            .contains(MasterRoles::VILLAGER_MALE)
        {
            master = Some(villager(
                self.streams.get_mut(Stream::Action).raw64(),
                self.model.slots.master,
            ));
            if 0 < count.wrapping_sub(1) as i16 {
                let _ = self.queue_at(
                    player,
                    list_index,
                    Some(self.model.slots.master.villager_male),
                    count - 1,
                    None,
                    button,
                )?;
            }
            count = 1;
        }
        let total = self
            .parts
            .production
            .get(self.players.live(player, list_index)?.id)
            .map_or(0, |production| production.queue.total.get());
        if 14 < total {
            return Ok(QueueOutcome::NoneQueued);
        }
        let queued_units =
            QueuedUnits::of_data(if i32::from(count as i16) + i32::from(total) < 16 {
                count as i16
            } else {
                ROOM.get() - total
            });
        if let Some(tech) = tech {
            self.queue_techs(player, list_index, (master, tech), queued_units, button)
        } else {
            let master = master.ok_or(GenieError::invalid_input(
                "a unit queue entry that names no master",
            ))?;
            self.queue_units(player, list_index, master, queued_units, button)
        }
    }

    /// `FUN_1411cbc40`'s unit branch: `count` of `master` paid at once (`FUN_14162fb60`,
    /// [`crate::production::pay`]), the count lowered while the player cannot (at one, nothing
    /// is queued); attribute 80 (the queued units) grows by the master's population times the
    /// count (slot 0xf8); the player's tracker counts them and places each
    /// ([`crate::players::tracker::Tracker::place`]; `FUN_140a4f570` per unit is the local player's
    /// statistics). They join the last entry when it is of the same master and button and its
    /// costs are the ones a unit costs now (`FUN_1411c81a0`,
    /// [`crate::production::master_costs`]); else a new entry ([`World::new_entry`]) records
    /// them, with the master's train time at the building and button
    /// ([`crate::production::train_time`]) for a combat master.
    pub(crate) fn queue_units(
        &mut self,
        player: PlayerId,
        list_index: ListIndex,
        master: MasterId,
        count: QueuedUnits,
        button: QueueButton,
    ) -> GenieResult<QueueOutcome> {
        use crate::production::{as_entry_costs, master_costs, pay, population};
        let empty = Masters::default();
        let mut count = count.get();
        loop {
            let masters = self.model.masters.of(player).unwrap_or(&empty);
            if pay(
                masters,
                master,
                CostCount::new(f32::from(count)),
                self.players.player_mut(player)?,
                &self.model.roles,
                self.model.slots,
            )? == Payment::Paid
            {
                break;
            }
            if count < 2 {
                return Ok(QueueOutcome::NoneQueued);
            }
            count -= 1;
        }
        let class = self.tracker_key(player, master).class;
        let masters = self.model.masters.of(player).unwrap_or(&empty);
        let made = masters.get(master);
        let pop = made.map_or(Amount::ZERO, |master| {
            population(&master.storage, &self.model.roles)
        });
        let owner = self.players.player_mut(player)?;
        owner.add_attribute(
            self.model.slots.attribute.queued,
            pop * f32::from(count),
            &self.model.roles,
        );
        let costs = as_entry_costs(master_costs(
            masters,
            master,
            &*owner,
            &self.model.roles,
            self.model.slots,
        )?);
        owner.tracker.count_queued(QueuedUnits::of_data(count));
        for _ in 0..count {
            owner.tracker.place(Some(master), class);
        }
        let (building, id) = {
            let unit = owner.objects.checked(list_index)?;
            (unit.life.master_id, unit.id)
        };
        let time = made
            .filter(|master| MasterKind::COMBAT <= master.kind)
            .map(|master| train_time(&master.creatable_part().train_locations, building, button));
        let production = self.production_or_new(id)?;
        if let Some(last) = production.queue.last()
            && last.master == Some(master)
            && last.button == button
            && same_costs(&last.costs, &costs)
        {
            let queue = &mut production.queue;
            let last = (queue.count.get() - 1) as usize;
            let slot = queue.slots.checked_mut(last)?;
            slot.count = slot.count.plus(QueuedUnits::of_data(count));
            queue.total = queue.total.plus(QueuedUnits::of_data(count));
            queue.changes = queue.changes.bumped();
            return Ok(QueueOutcome::Queued);
        }
        self.new_entry(
            player,
            list_index,
            (Some(master), None),
            QueuedUnits::of_data(count),
            button,
            time,
        )?;
        Ok(QueueOutcome::Queued)
    }

    /// `FUN_1411cbc40`'s new entry at the building at `list_index` of `player`: a full buffer grows
    /// by ten first, each old slot's costs filled anew ([`World::entry_costs`]); the slot after the
    /// last takes the master, the count, the technology, the button and the next serial, its costs
    /// [`World::entry_costs`], and the train time `time` when given (a research and a master that
    /// is not a combat master leave the slot's); the total, the entries and the change counter
    /// grow.
    pub(super) fn new_entry(
        &mut self,
        player: PlayerId,
        list_index: ListIndex,
        (master, tech): (Option<MasterId>, Option<TechId>),
        queued_units: QueuedUnits,
        button: QueueButton,
        time: Option<WholeSeconds>,
    ) -> GenieResult<()> {
        let id = self.players.live(player, list_index)?.id;
        let queue = &self.parts.production.get(id).ok_or(NO_QUEUE)?.queue;
        let refill = if queue.full() {
            Some(
                queue
                    .slots
                    .iter()
                    .map(|entry| self.entry_costs(player, entry.tech, entry.master))
                    .collect::<Result<Vec<_>, _>>()?,
            )
        } else {
            None
        };
        let costs = self.entry_costs(player, tech, master)?;
        let queue = &mut self.parts.production.get_mut(id).ok_or(NO_QUEUE)?.queue;
        if let Some(refill) = refill {
            queue.grow(&refill);
        }
        let open = queue.open(master, queued_units, tech, button)?;
        let slot = queue.slots.checked_mut(open)?;
        slot.costs = costs;
        if let Some(time) = time {
            slot.train_time = time;
        }
        queue.total = queue.total.plus(queued_units);
        queue.count = queue.count.grown();
        queue.changes = queue.changes.bumped();
        Ok(())
    }

    /// `FUN_1411c3190`: the costs a queue entry of `master` (for a master other than −1:
    /// [`crate::production::master_costs`]) or of technology `tech` records, from (−1, 0). A
    /// technology the tree has not got (`FUN_1413cc6c0`), or none, records none; one in state 2
    /// or 4 what [`crate::technology::TechState::entry_costs`] gives.
    pub(crate) fn entry_costs(
        &self,
        player: PlayerId,
        tech: Option<TechId>,
        master: Option<MasterId>,
    ) -> GenieResult<[(Option<AttributeId>, Amount); 3]> {
        use crate::production::{NO_COSTS, as_entry_costs, master_costs};
        let owner = self.players.player(player)?;
        if let Some(master) = master {
            let empty = Masters::default();
            let masters = self.model.masters.of(player).unwrap_or(&empty);
            return Ok(as_entry_costs(master_costs(
                masters,
                master,
                owner,
                &self.model.roles,
                self.model.slots,
            )?));
        }
        let Some((tech, tech_index)) = tech
            .map(|tech| (tech, tech.index()))
            .filter(|&(_, tech_index)| tech_index < owner.techs.len())
        else {
            return Ok(NO_COSTS);
        };
        let Some(def) = self.model.techs.get(tech_index) else {
            return Ok(NO_COSTS);
        };
        owner
            .techs
            .checked(tech_index)?
            .entry_costs(def, factors(tech, owner, self.model.slots)?)
    }
}

/// [`World::queue`]'s building for its next unit, an index into `rooms` and `loads`: the last
/// one's `pick` first (none: the first with room), replaced by each with room and a load below
/// its (`COMISS`, `JBE`: a comparison with a NaN replaces nothing); with none, the one with the
/// most room, the first of equals; none without room.
pub(crate) fn least_loaded(rooms: &[i32], loads: &[Seconds], pick: Option<usize>) -> Option<usize> {
    let mut best = pick;
    for (index, (&room, &load)) in rooms.iter().zip(loads).enumerate() {
        if 0 < room
            && best
                .and_then(|index| loads.get(index))
                .is_none_or(|&seconds| seconds > load)
        {
            best = Some(index);
        }
    }
    if best.is_none() {
        for (index, &room) in rooms.iter().enumerate() {
            if 0 < room
                && best
                    .and_then(|index| rooms.get(index))
                    .is_none_or(|&best_room| best_room < room)
            {
                best = Some(index);
            }
        }
    }
    best
}

/// [`World::queue`]'s amount of `unit` the player `owner` can pay, from `amount` down: each cost
/// entry of the costed master ([`crate::production::costed`] of `masters`; an annex's paid ones)
/// but the population's needs its amount times the count (`COMISS`, `JA`: a NaN passes). None
/// when one unit is still short.
fn affordable(
    owner: &Player,
    masters: &Masters,
    unit: &Master,
    mut amount: i32,
    roles: &Roles,
) -> GenieResult<Option<i32>> {
    let (costed, annex) = costed(masters, unit);
    let short = |count: i32| -> GenieResult<bool> {
        for (attribute, each, paid) in cost_entries(&costed.creatable_part().cost) {
            if (annex && !paid)
                || roles
                    .of_attribute(attribute)
                    .contains(AttributeRoles::HEADROOM)
            {
                continue;
            }
            let held = Attributes::get(owner, attribute)?;
            if held < Amount::new(each.as_f32() * count as f32) {
                return Ok(true);
            }
        }
        Ok(false)
    };
    while short(amount)? {
        if amount <= 1 {
            return Ok(None);
        }
        amount -= 1;
    }
    Ok(Some(amount))
}
