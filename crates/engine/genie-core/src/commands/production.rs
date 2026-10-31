//! QUEUE: a building's production queue, its load, its units and technologies, and a new
//! entry's costs.

use crate::GenieError;
use crate::commands::ONE_UNIT_AGE;
use crate::commands::queue::QueueCommand;
use crate::commands::research::ResearchCommand;
use crate::math::rng::Stream;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::ids::VILLAGER_MALE;
use crate::objects::masters::master_type::BUILDING;
use crate::objects::masters::master_type::COMBAT;
use crate::objects::masters::{Masters, player_master};
use crate::orders::NO_QUEUE;
use crate::orders::Place;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute;
use crate::players::attribute::AttributeId;
use crate::players::attribute::QUEUED;
use crate::production::Production;
use crate::technology::TechId;
use crate::world::Seconds;
use crate::world::slots::PlayerSlots;
use crate::world::{
    COMPUTER_QUEUE_LIMIT, ONE_UNIT_GAME, QUEUE_LIMIT, VILLAGER_ANY, World, villager,
};

impl World {
    /// QUEUE, the player's part (`FUN_141614450`) after the packet's check of its buildings
    /// (`do_command`'s case 0x81, [`World::command_check`]: an id the world lacks, or another
    /// player's without cheats, goes; a human's packet left with none is refused). Nothing for
    /// a master the player lacks (`FUN_141659d20`). Of the first 60 buildings (500 for a computer
    /// player) those the master is available to (`+0x5f0`) or that list it themselves (`+0xd30`,
    /// `FUN_1411c8fd0`) take part, at most 60: each where the unit trains ([`World::trains_at`],
    /// `FUN_141263610`) has its room (15, a computer player's 2, less its queued total
    /// `+0xbf4`; the sum of those above 0 holds the amount) and its load
    /// ([`World::production_load`]); any other has room −1 and takes none. The amount is lowered while the player cannot pay that many:
    /// each cost entry of the costed master ([`crate::production::costed`]; an annex's paid
    /// ones) but the population's needs its amount times the count (`COMISS`, `JA`: a NaN
    /// passes); at one still short, or for a master below type 70, nothing is queued. In game
    /// mode 16, while the player's attribute 6 is at most 1, a military master (its key's `+9`,
    /// [`crate::players::tracker::Key::military`]) is queued one at a time: with the player's flag
    /// (`+0x1378`) set nothing is, else the flag sets and the amount is 1 (even with no room).
    /// Then unit by unit the building with room and the least load takes it (the last one's
    /// pick first, replaced by one whose load is below: `COMISS`, `JBE`, a NaN never wins; with
    /// none at the start, the most room), its room one less and its load the unit's train time
    /// there ([`crate::production::train_time`]); last, each building's part
    /// ([`World::queue_at`]) with its count, the packet's master and button. The limited units'
    /// tests before the mode 16 check (`FUN_1416116c0`, `FUN_141611520`: an amount above the
    /// master's `+0x5f0` lowered, none at 1) read the `+0x5f1` bits 1 and 2 of the unit or its
    /// linked master; genie's masters keep `+0x5f1` as a boolean (the `.dat` holds 0 for every
    /// master), so they pass ([`World::trainable_count`]).
    ///
    /// # Errors
    ///
    /// A sleeping object the unit trains at, and what the parts do not model.
    pub(crate) fn queue(
        &mut self,
        q: &QueueCommand,
        buildings: &[Option<ObjectId>],
    ) -> Result<(), GenieError> {
        let (player, unit_id) = (q.player, q.unit);
        let Some(ids) = self.command_check(player, buildings)? else {
            return Ok(());
        };
        let owner = self.players.player(player)?;
        let limit = if owner.computer { 500 } else { 60 };
        let empty = Masters::default();
        let masters = self.masters.of(player).unwrap_or(&empty);
        let Some(unit) = masters.get(unit_id) else {
            return Ok(());
        };
        let mut found = Vec::new();
        for place in self
            .lookup(&ids[..ids.len().min(limit)])
            .into_iter()
            .flatten()
        {
            let (_, building) = self.at(place)?;
            let listed = building
                .production
                .as_ref()
                .is_some_and(|p| p.available_units.contains(&unit_id));
            if unit.available || listed {
                found.push(place);
            }
        }
        found.truncate(60);
        if found.is_empty() {
            return Ok(());
        }
        let per = if owner.computer {
            COMPUTER_QUEUE_LIMIT
        } else {
            QUEUE_LIMIT
        } as i32;
        let (mut rooms, mut loads, total_room) = self.rooms_and_loads(&found, unit, per)?;
        let mut amount = i32::from(q.amount).min(i32::from(total_room as i16));
        if 0 < amount {
            if unit.kind < COMBAT {
                return Ok(());
            }
            let (costed, annex) = crate::production::costed(masters, unit);
            let short = |count: i32| -> Result<bool, GenieError> {
                for (attribute, each, paid) in crate::objects::masters::cost_entries(&costed.cost) {
                    if (annex && !paid) || attribute == attribute::HEADROOM {
                        continue;
                    }
                    let held = crate::technology::Attributes::get(owner, attribute)?;
                    if held < Amount::new(f32::from(each) * count as f32) {
                        return Ok(true);
                    }
                }
                Ok(false)
            };
            while short(amount)? {
                if amount <= 1 {
                    return Ok(());
                }
                amount -= 1;
            }
        }
        // game mode 16: with attribute 6 at most 1 (COMISS, JC: a NaN is not) a military unit
        // (its key's +9) is queued one at a time, the player's +0x1378 its flag
        if self.settings.game_mode == ONE_UNIT_GAME
            // The player's attribute game mode 16's QUEUE reads (6, `+0x30`), and its bound
            // (`DAT_14330eb8c`).
            && crate::technology::Attributes::get(owner, attribute::CURRENT_AGE)? <= Amount::new(ONE_UNIT_AGE)
            && self.tracker_key(player, unit_id).military
        {
            if owner.one_military_queued {
                return Ok(());
            }
            self.players.player_mut(player)?.one_military_queued = true;
            amount = 1;
        }
        let mut counts = vec![0u16; found.len()];
        let mut pick: Option<usize> = None;
        while 0 < amount {
            amount -= 1;
            let Some(k) = least_loaded(&rooms, &loads, pick) else {
                return Ok(());
            };
            rooms[k] -= 1;
            counts[k] += 1;
            let at = self.at(found[k])?.1.life.master_id;
            let time = crate::production::train_time(&unit.train_locations, at, q.button);
            loads[k] += Seconds::of_master(time);
            pick = Some(k);
        }
        for (k, &place) in found.iter().enumerate() {
            if 0 < counts[k] {
                // the building's part works for its owner (`FUN_1411cbc40`'s `+0x138`)
                let Place::Live(p, i) = place else {
                    return Err(GenieError::unsupported(
                        "QUEUE at a sleeping object the unit trains at",
                    ));
                };
                let _ = self.queue_at(p, i, Some(unit_id), counts[k], None, q.button)?;
            }
        }
        Ok(())
    }

    /// The object at `place`, with its player.
    fn at(&self, place: Place) -> Result<(PlayerId, &crate::objects::unit::Unit), GenieError> {
        Ok(match place {
            Place::Live(p, i) => (p, self.players.live(p, i)?),
            Place::Sleeping(p, i) => (p, self.players.asleep(p, i)?),
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
    ) -> Result<(Vec<i32>, Vec<Seconds>, i32), GenieError> {
        let mut rooms = Vec::with_capacity(found.len());
        let mut loads = Vec::with_capacity(found.len());
        let mut total_room = 0i32;
        for &place in found {
            let (p, building) = self.at(place)?;
            // a building the unit does not train at keeps room −1 and load 0
            if !self.trains_at(unit, building.life.master_id) {
                rooms.push(-1);
                loads.push(Seconds::ZERO);
                continue;
            }
            let total = building.production.as_ref().map_or(0, |p| p.queue.total);
            let room = per - i32::from(total);
            total_room += room.max(0);
            rooms.push(room);
            loads.push(self.production_load(p, building)?);
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
        r: &ResearchCommand,
        buildings: &[Option<ObjectId>],
    ) -> Result<Option<ObjectId>, GenieError> {
        let load = |id: ObjectId| -> Result<Seconds, GenieError> {
            self.object(id)
                .map_or(Ok(Seconds::ZERO), |(q, u)| self.production_load(q, u))
        };
        let is_building = |id: Option<ObjectId>| -> Result<bool, GenieError> {
            match self.object(id) {
                Some((q, u)) => Ok(u.master(&self.masters, q)?.kind == BUILDING),
                None => Ok(false),
            }
        };
        let mut chosen = if is_building(r.building)? {
            r.building
        } else {
            None
        };
        if 1 < r.count {
            let limit = if self.players.player(r.player)?.computer {
                500
            } else {
                60
            };
            let n = usize::from(r.count as u16).min(limit);
            for id in buildings.iter().take(n).flatten().copied() {
                if self.object(id).is_none() {
                    continue;
                }
                let heavier = match chosen {
                    Some(c) => load(c)? > load(id)?,
                    None => true,
                };
                if heavier {
                    chosen = Some(id);
                }
            }
        }
        Ok(chosen)
    }

    /// `FUN_1411d0e90`: the seconds of work `building` (live or sleeping) of player `p` has queued: each
    /// queue entry's count times its time (a unit's master's first train time, `+0x898`, in
    /// integers; a research's location at the building, `FUN_1413cbd20`, its time plus its
    /// adjustment, as floats), less the work done of its production list's action (a make's
    /// `+0x6c`, a research's `+0x70`).
    pub(crate) fn production_load(
        &self,
        p: PlayerId,
        building: &crate::objects::unit::Unit,
    ) -> Result<Seconds, GenieError> {
        let owner = self.players.player(p)?;
        let Some(production) = building.production.as_ref() else {
            return Ok(Seconds::ZERO);
        };
        let queue = &production.queue;
        let entries = usize::try_from(queue.count)
            .unwrap_or(0)
            .min(queue.slots.len());
        let mut queued = Seconds::ZERO;
        for e in &queue.slots[..entries] {
            if let Some(master) = e.master {
                let time = player_master(&self.masters, p, master)
                    .and_then(|m| m.train_locations.first())
                    .map_or(0, |l| l.time);
                queued += Seconds::new((i32::from(e.count) * i32::from(time)) as f32);
            } else if let Some(tech) = e.tech {
                // a queued research has its locations (the queue checked them)
                let (time, adjustment) = Some(tech.index())
                    .and_then(|t| owner.techs.get(t))
                    .and_then(|t| t.location(building.life.master_id))
                    .map_or((0, Seconds::ZERO), |l| (l.time as i16, l.adjustment));
                queued += (Seconds::of_master(time) + adjustment) * f32::from(e.count);
            }
        }
        if queue.slots.is_empty() {
            return Ok(queued);
        }
        let done = match production.action {
            Some(crate::production::Making::Unit(m)) => m.progress,
            Some(crate::production::Making::Research(r)) => r.progress,
            None => Seconds::ZERO,
        };
        Ok(queued - (Seconds::ZERO + done))
    }

    /// `FUN_1411cbc40`, QUEUE's and RESEARCH's building part, at the building at `i` of
    /// `player`: `count` of master `unit`, or with `tech` other than −1 that technology (`unit`
    /// −1), with the train button `button`. The generic villager (0x712) is the villager
    /// (0x53), whose sex the Action stream draws for each: the routine queues the rest first,
    /// one by one, then this one (the last drawn is queued first). A building whose total
    /// (`+0xbf4`) is above 14 refuses (its event 0x84 is the interface's); the count is held
    /// to the room of 15. Units: [`World::queue_units`]; researches: [`World::queue_techs`].
    /// True when any was queued.
    pub(crate) fn queue_at(
        &mut self,
        player: PlayerId,
        i: ListIndex,
        unit: Option<MasterId>,
        count: u16,
        tech: Option<TechId>,
        button: i32,
    ) -> Result<bool, GenieError> {
        let mut master = if unit == Some(VILLAGER_ANY) {
            Some(VILLAGER_MALE)
        } else {
            unit
        };
        let mut count = count;
        if master == Some(VILLAGER_MALE) {
            master = Some(villager(self.streams.get(Stream::Action).raw64()));
            if 0 < count.wrapping_sub(1) as i16 {
                let _ = self.queue_at(player, i, Some(VILLAGER_MALE), count - 1, None, button)?;
            }
            count = 1;
        }
        let total = self
            .players
            .live(player, i)?
            .production
            .as_ref()
            .map_or(0, |p| p.queue.total);
        if 14 < total {
            return Ok(false);
        }
        let n = if i32::from(count as i16) + i32::from(total) < 16 {
            count as i16
        } else {
            crate::production::ROOM - total
        };
        if let Some(tech) = tech {
            self.queue_techs(player, i, (master, tech), n, button)
        } else {
            let master = master.ok_or(GenieError::invalid_input(
                "a unit queue entry that names no master",
            ))?;
            self.queue_units(player, i, master, n, button)
        }
    }

    /// `FUN_1411cbc40`'s unit branch: `n` of `master` paid at once (`FUN_14162fb60`,
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
        i: ListIndex,
        master: MasterId,
        count: i16,
        button: i32,
    ) -> Result<bool, GenieError> {
        use crate::production::{as_entry_costs, master_costs, pay, population};
        let empty = Masters::default();
        let mut n = count;
        loop {
            let masters = self.masters.of(player).unwrap_or(&empty);
            if pay(
                masters,
                master,
                f32::from(n),
                self.players.player_mut(player)?,
            )? {
                break;
            }
            if n < 2 {
                return Ok(false);
            }
            n -= 1;
        }
        let class = self.tracker_key(player, master).class;
        let masters = self.masters.of(player).unwrap_or(&empty);
        let made = masters.get(master);
        let pop = made.map_or(Amount::ZERO, |m| population(&m.storage));
        let owner = self.players.player_mut(player)?;
        owner.add_attribute(QUEUED, pop * f32::from(n))?;
        let costs = as_entry_costs(master_costs(masters, master, &*owner)?);
        owner.tracker.count_queued(n);
        for _ in 0..n {
            owner.tracker.place(Some(master), class);
        }
        let building = owner.objects[i].life.master_id;
        let production = owner.objects[i]
            .production
            .get_or_insert_with(Production::default);
        if let Some(last) = production.queue.last()
            && last.master == Some(master)
            && last.button == button
            && crate::production::same_costs(&last.costs, &costs)
        {
            let queue = &mut production.queue;
            let k = (queue.count - 1) as usize;
            queue.slots[k].count = queue.slots[k].count.wrapping_add(n);
            queue.total = queue.total.wrapping_add(n);
            queue.changes = queue.changes.wrapping_add(1);
            return Ok(true);
        }
        let time = made
            .filter(|m| COMBAT <= m.kind)
            .map(|m| crate::production::train_time(&m.train_locations, building, button));
        self.new_entry(player, i, (Some(master), None), n, button, time)?;
        Ok(true)
    }

    /// `FUN_1411cbc40`'s technology branch: the count is held to the technology's cap less its
    /// researches queued and done when it stacks (`+0x40`; the cap `+0x56`, 0x100 below 0; the
    /// player's `+0x648` is not held, 0 for every player probed); each research is paid
    /// (`FUN_14162fd90`: [`crate::technology::TechState::pay`] with the repeat test) until one cannot
    /// be. The paid ones join the last entry when it is of the same technology and its costs
    /// are the ones it lists now (`FUN_1411c81a0` -> `FUN_1413d4a00`,
    /// [`crate::technology::TechState::listed_costs`]); else a new entry ([`World::new_entry`]); the
    /// technology's researches queued (`+0x58`) grow by them.
    pub(super) fn queue_techs(
        &mut self,
        player: PlayerId,
        i: ListIndex,
        (master, tech): (Option<MasterId>, TechId),
        count: i16,
        button: i32,
    ) -> Result<bool, GenieError> {
        let t = tech.index();
        let def = self.techs.get(t).ok_or(GenieError::unsupported(
            "a technology the data file has not got",
        ))?;
        let owner = self.players.player_mut(player)?;
        let state = &owner.techs[t];
        let done = state.pending.wrapping_add(state.times_researched);
        let cap = if 0 <= state.research_cap {
            state.research_cap
        } else {
            0x100
        };
        let mut n = count;
        if state.stack && i32::from(cap) - i32::from(done) < i32::from(n) {
            n = cap.wrapping_sub(done);
        }
        // FUN_14162fd90's repeat flag: the player's +0x648 (not held) or FUN_1413d1050
        let repeat = state.repeatable(def, false);
        let building = owner.objects[i].life.master_id;
        let mut paid = 0i16;
        while paid < n {
            let get = |k: AttributeId| crate::technology::Attributes::get(&*owner, k);
            let factors = crate::technology::factors(tech, &get)?;
            let mut state = std::mem::take(&mut owner.techs[t]);
            let ok = state.pay(def, factors, repeat, building, owner);
            owner.techs[t] = state;
            if !ok? {
                n = paid;
                break;
            }
            paid += 1;
        }
        if n <= 0 {
            return Ok(false);
        }
        let get = |k: AttributeId| crate::technology::Attributes::get(&*owner, k);
        let listed = owner.techs[t].listed_costs(def, crate::technology::factors(tech, &get)?)?;
        let costs = crate::production::as_entry_costs(listed);
        let production = owner.objects[i]
            .production
            .get_or_insert_with(Production::default);
        if let Some(last) = production.queue.last()
            && last.tech == Some(tech)
            && crate::production::same_costs(&last.costs, &costs)
        {
            let queue = &mut production.queue;
            let k = (queue.count - 1) as usize;
            queue.slots[k].count = queue.slots[k].count.wrapping_add(n);
            queue.total = queue.total.wrapping_add(n);
            queue.changes = queue.changes.wrapping_add(1);
            owner.techs[t].pending = owner.techs[t].pending.wrapping_add(n);
            return Ok(true);
        }
        self.new_entry(player, i, (master, Some(tech)), n, button, None)?;
        let pending = &mut self.players.player_mut(player)?.techs[t].pending;
        *pending = pending.wrapping_add(n);
        Ok(true)
    }

    /// `FUN_1411cbc40`'s new entry at the building at `i` of `player`: a full buffer grows by
    /// ten first, each old slot's costs filled anew ([`World::entry_costs`]); the slot after
    /// the last takes the master, the count, the technology, the button and the next serial,
    /// its costs [`World::entry_costs`], and the train time `time` when given (a research and a
    /// master that is not a combat master leave the slot's); the total, the entries and the
    /// change counter grow.
    pub(super) fn new_entry(
        &mut self,
        player: PlayerId,
        i: ListIndex,
        (master, tech): (Option<MasterId>, Option<TechId>),
        n: i16,
        button: i32,
        time: Option<i16>,
    ) -> Result<(), GenieError> {
        let queue = &self
            .players
            .live(player, i)?
            .production
            .as_ref()
            .ok_or(NO_QUEUE)?
            .queue;
        let refill = if queue.full() {
            Some(
                queue
                    .slots
                    .iter()
                    .map(|s| self.entry_costs(player, s.tech, s.master))
                    .collect::<Result<Vec<_>, _>>()?,
            )
        } else {
            None
        };
        let costs = self.entry_costs(player, tech, master)?;
        let queue = &mut self
            .players
            .live_mut(player, i)?
            .production
            .as_mut()
            .ok_or(NO_QUEUE)?
            .queue;
        if let Some(refill) = refill {
            queue.grow(&refill);
        }
        let k = queue.open(master, n, tech, button);
        queue.slots[k].costs = costs;
        if let Some(time) = time {
            queue.slots[k].train_time = time;
        }
        queue.total = queue.total.wrapping_add(n);
        queue.count += 1;
        queue.changes = queue.changes.wrapping_add(1);
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
    ) -> Result<[(Option<AttributeId>, f32); 3], GenieError> {
        use crate::production::{NO_COSTS, as_entry_costs, master_costs};
        let owner = self.players.player(player)?;
        if let Some(master) = master {
            let empty = Masters::default();
            let masters = self.masters.of(player).unwrap_or(&empty);
            return Ok(as_entry_costs(master_costs(masters, master, owner)?));
        }
        let Some((tech, t)) = tech
            .map(|tech| (tech, tech.index()))
            .filter(|&(_, t)| t < owner.techs.len())
        else {
            return Ok(NO_COSTS);
        };
        let Some(def) = self.techs.get(t) else {
            return Ok(NO_COSTS);
        };
        let get = |k: AttributeId| crate::technology::Attributes::get(owner, k);
        owner.techs[t].entry_costs(def, crate::technology::factors(tech, &get)?)
    }
}

/// [`World::queue`]'s building for its next unit, an index into `rooms` and `loads`: the last
/// one's `pick` first (none: the first with room), replaced by each with room and a load below
/// its (`COMISS`, `JBE`: a comparison with a NaN replaces nothing); with none, the one with the
/// most room, the first of equals; none without room.
pub(crate) fn least_loaded(rooms: &[i32], loads: &[Seconds], pick: Option<usize>) -> Option<usize> {
    let mut best = pick;
    for k in 0..rooms.len() {
        if 0 < rooms[k] && best.is_none_or(|b| loads[b] > loads[k]) {
            best = Some(k);
        }
    }
    if best.is_none() {
        for k in 0..rooms.len() {
            if 0 < rooms[k] && best.is_none_or(|b| rooms[b] < rooms[k]) {
                best = Some(k);
            }
        }
    }
    best
}
