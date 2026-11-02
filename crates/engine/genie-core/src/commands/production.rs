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
use crate::objects::masters::MasterKind;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::cost_entries;
use crate::objects::masters::ids::VILLAGER_MALE;
use crate::objects::masters::{Masters, player_master};
use crate::objects::unit::Unit;
use crate::orders::NO_QUEUE;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute;
use crate::players::attribute::AttributeId;
use crate::players::attribute::QUEUED;
use crate::production::CostCount;
use crate::production::QueueButton;
use crate::production::QueuedUnits;
use crate::production::{Making, ROOM, costed, same_costs, train_time};
use crate::technology::TechId;
use crate::technology::{Attributes, factors};
use crate::world::GameMode;
use crate::world::Seconds;
use crate::world::WholeSeconds;
use crate::world::index::Place;
use crate::world::player::Player;
use crate::world::slots::CheckedIndex;
use crate::world::slots::PlayerSlots;
use crate::world::{COMPUTER_QUEUE_LIMIT, QUEUE_LIMIT, VILLAGER_ANY, World, villager};

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
        let limit = if owner.kind.is_computer() { 500 } else { 60 };
        let empty = Masters::default();
        let masters = self.masters.of(player).unwrap_or(&empty);
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
        let mut amount = i32::from(q.amount.get()).min(i32::from(total_room as i16));
        if 0 < amount {
            if unit.kind < MasterKind::COMBAT {
                return Ok(());
            }
            let Some(paid) = affordable(owner, masters, unit, amount)? else {
                return Ok(());
            };
            amount = paid;
        }
        // game mode 16: with attribute 6 at most 1 (COMISS, JC: a NaN is not) a military unit
        // (its key's +9) is queued one at a time, the player's +0x1378 its flag
        if self.settings.game_mode == GameMode::ONE_UNIT
            // The player's attribute game mode 16's QUEUE reads (6, `+0x30`), and its bound
            // (`DAT_14330eb8c`).
            && Attributes::get(owner, attribute::CURRENT_AGE)? <= Amount::new(ONE_UNIT_AGE)
            && self.tracker_key(player, unit_id).military
        {
            if owner.one_military_queued {
                return Ok(());
            }
            self.players.player_mut(player)?.one_military_queued = true;
            amount = 1;
        }
        let Some(counts) = self.shares(&found, (rooms, loads), unit, q.button, amount)? else {
            return Ok(());
        };
        for (&place, &count) in found.iter().zip(&counts) {
            if 0 < count {
                // the building's part works for its owner (`FUN_1411cbc40`'s `+0x138`)
                let Place::Live(p, i) = place else {
                    return Err(GenieError::unsupported(
                        "QUEUE at a sleeping object the unit trains at",
                    ));
                };
                let _ = self.queue_at(p, i, Some(unit_id), count, None, q.button)?;
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
    ) -> Result<Vec<Place>, GenieError> {
        let mut found = Vec::new();
        for place in self.lookup(ids).into_iter().flatten() {
            let (_, building) = self.at(place)?;
            let listed = self
                .parts
                .production
                .get(building.id)
                .is_some_and(|p| p.available_units.contains(&unit_id));
            if unit.available || listed {
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
    ) -> Result<Option<Vec<u16>>, GenieError> {
        let mut counts = vec![0u16; found.len()];
        let mut pick: Option<usize> = None;
        while 0 < amount {
            amount -= 1;
            let Some(k) = least_loaded(&rooms, &loads, pick) else {
                return Ok(None);
            };
            *rooms.checked_mut(k)? -= 1;
            *counts.checked_mut(k)? += 1;
            let at = self.at(*found.checked(k)?)?.1.life.master_id;
            let time = train_time(&unit.train_locations, at, button);
            *loads.checked_mut(k)? += Seconds::of_master(time);
            pick = Some(k);
        }
        Ok(Some(counts))
    }

    /// The object at `place`, with its player.
    fn at(&self, place: Place) -> Result<(PlayerId, &Unit), GenieError> {
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
            let total = self
                .parts
                .production
                .get(building.id)
                .map_or(0, |p| p.queue.total.get());
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
                Some((q, u)) => Ok(u.master(self.master_tables(), q)?.kind == MasterKind::BUILDING),
                None => Ok(false),
            }
        };
        let mut chosen = if is_building(r.building)? {
            r.building
        } else {
            None
        };
        if 1 < r.count {
            let limit = if self.players.player(r.player)?.kind.is_computer() {
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
        building: &Unit,
    ) -> Result<Seconds, GenieError> {
        let owner = self.players.player(p)?;
        let Some(production) = self.parts.production.get(building.id) else {
            return Ok(Seconds::ZERO);
        };
        let queue = &production.queue;
        let entries = queue.count.slots().min(queue.slots.len());
        let mut queued = Seconds::ZERO;
        for e in queue.slots.head(entries)? {
            if let Some(master) = e.master {
                let time = player_master(&self.masters, p, master)
                    .and_then(|m| m.train_locations.first())
                    .map_or(WholeSeconds::ZERO, |l| l.time);
                queued += Seconds::new((i32::from(e.count.get()) * i32::from(time.get())) as f32);
            } else if let Some(tech) = e.tech {
                // a queued research has its locations (the queue checked them)
                let (time, adjustment) = Some(tech.index())
                    .and_then(|t| owner.techs.get(t))
                    .and_then(|t| t.location(building.life.master_id))
                    .map_or((WholeSeconds::ZERO, Seconds::ZERO), |l| {
                        (l.time, l.adjustment)
                    });
                queued += (Seconds::of_master(time) + adjustment) * f32::from(e.count.get());
            }
        }
        if queue.slots.is_empty() {
            return Ok(queued);
        }
        let done = match production.action {
            Some(Making::Unit(m)) => m.progress,
            Some(Making::Research(r)) => r.progress,
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
        button: QueueButton,
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
            .parts
            .production
            .get(self.players.live(player, i)?.id)
            .map_or(0, |p| p.queue.total.get());
        if 14 < total {
            return Ok(false);
        }
        let n = QueuedUnits::of_data(if i32::from(count as i16) + i32::from(total) < 16 {
            count as i16
        } else {
            ROOM.get() - total
        });
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
        count: QueuedUnits,
        button: QueueButton,
    ) -> Result<bool, GenieError> {
        use crate::production::{as_entry_costs, master_costs, pay, population};
        let empty = Masters::default();
        let mut n = count.get();
        loop {
            let masters = self.masters.of(player).unwrap_or(&empty);
            if pay(
                masters,
                master,
                CostCount::new(f32::from(n)),
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
        owner.tracker.count_queued(QueuedUnits::of_data(n));
        for _ in 0..n {
            owner.tracker.place(Some(master), class);
        }
        let (building, id) = {
            let b = owner.objects.checked(i)?;
            (b.life.master_id, b.id)
        };
        let time = made
            .filter(|m| MasterKind::COMBAT <= m.kind)
            .map(|m| train_time(&m.train_locations, building, button));
        let production = self.production_or_new(id)?;
        if let Some(last) = production.queue.last()
            && last.master == Some(master)
            && last.button == button
            && same_costs(&last.costs, &costs)
        {
            let queue = &mut production.queue;
            let k = (queue.count.get() - 1) as usize;
            let slot = queue.slots.checked_mut(k)?;
            slot.count = slot.count.plus(QueuedUnits::of_data(n));
            queue.total = queue.total.plus(QueuedUnits::of_data(n));
            queue.changes = queue.changes.bumped();
            return Ok(true);
        }
        self.new_entry(
            player,
            i,
            (Some(master), None),
            QueuedUnits::of_data(n),
            button,
            time,
        )?;
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
        n: QueuedUnits,
        button: QueueButton,
        time: Option<WholeSeconds>,
    ) -> Result<(), GenieError> {
        let id = self.players.live(player, i)?.id;
        let queue = &self.parts.production.get(id).ok_or(NO_QUEUE)?.queue;
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
        let queue = &mut self.parts.production.get_mut(id).ok_or(NO_QUEUE)?.queue;
        if let Some(refill) = refill {
            queue.grow(&refill);
        }
        let k = queue.open(master, n, tech, button)?;
        let slot = queue.slots.checked_mut(k)?;
        slot.costs = costs;
        if let Some(time) = time {
            slot.train_time = time;
        }
        queue.total = queue.total.plus(n);
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
    ) -> Result<[(Option<AttributeId>, Amount); 3], GenieError> {
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
        let get = |k: AttributeId| Attributes::get(owner, k);
        owner
            .techs
            .checked(t)?
            .entry_costs(def, factors(tech, &get)?)
    }
}

/// [`World::queue`]'s building for its next unit, an index into `rooms` and `loads`: the last
/// one's `pick` first (none: the first with room), replaced by each with room and a load below
/// its (`COMISS`, `JBE`: a comparison with a NaN replaces nothing); with none, the one with the
/// most room, the first of equals; none without room.
pub(crate) fn least_loaded(rooms: &[i32], loads: &[Seconds], pick: Option<usize>) -> Option<usize> {
    let mut best = pick;
    for (k, (&room, &load)) in rooms.iter().zip(loads).enumerate() {
        if 0 < room && best.and_then(|b| loads.get(b)).is_none_or(|&b| b > load) {
            best = Some(k);
        }
    }
    if best.is_none() {
        for (k, &room) in rooms.iter().enumerate() {
            if 0 < room && best.and_then(|b| rooms.get(b)).is_none_or(|&b| b < room) {
                best = Some(k);
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
) -> Result<Option<i32>, GenieError> {
    let (costed, annex) = costed(masters, unit);
    let short = |count: i32| -> Result<bool, GenieError> {
        for (attribute, each, paid) in cost_entries(&costed.cost) {
            if (annex && !paid) || attribute == attribute::HEADROOM {
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
