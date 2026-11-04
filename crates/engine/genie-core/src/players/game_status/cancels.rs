//! A building's queue cancelled: the whole queue, its units and researches refunded, and
//! `UNIT_ORDER`'s cancel of one, five or some.

use crate::math::fmath::below_or_unordered;
use crate::objects::ListIndex;
use crate::objects::masters::MasterId;
use crate::objects::masters::player_master;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::game_status::ONE_UNIT_AGE;
use crate::players::tracker::Key;
use crate::production::Batch;
use crate::production::QueueEntry;
use crate::production::QueuedUnits;
use crate::production::UnitIndex;
use crate::production::{Entry, Queue, QueueLength, population};
use crate::production::{Making, refund, refund_research};
use crate::technology::Attributes;
use crate::technology::CostPaid;
use crate::technology::ResearchState;
use crate::technology::TechId;
use crate::world::Seconds;
use crate::world::World;
use crate::world::layer::MasterRoles;
use crate::world::slots::{CheckedIndexExt, PlayerSlotsExt};
use crate::world::{GameMode, NO_ATTRIBUTE};
use crate::{GenieError, GenieResult};

/// A cancel finds the building without the production it cancels from.
const NO_PRODUCTION: GenieError =
    GenieError::unreachable("a cancelled queue entry of a building without production");

impl World {
    /// `FUN_1411cff00`, the cancel of the whole queue of the building at `idx` of player
    /// `player_id`, entry by entry. A unit entry: each unit gets its refund ([`refund`]) and the
    /// tracker its place back (the counter one less,
    /// [`crate::players::tracker::Tracker::unplace`]); attribute 80 lowers by the population times
    /// the count (slot 0xf8), and the tracker's counter by the count once more. A research entry,
    /// for a player with a tech tree: its count's refund ([`World::refund_researches`]). Then the
    /// interface's `+0xc02` (the local player's event 0x7d) is left; the entries go, the production
    /// list's action goes ([`World::end_production_action`]), the change counter grows and the
    /// total is 0.
    pub(crate) fn cancel_queue(&mut self, player_id: PlayerId, idx: ListIndex) -> GenieResult<()> {
        let Some(production) = self
            .parts
            .production
            .get(self.players.live(player_id, idx)?.id)
        else {
            return Ok(());
        };
        let entries = production.queue.entries().to_vec();
        for entry in &entries {
            if entry.tech.is_none() {
                self.cancel_units(player_id, entry, entry.count)?;
                self.players
                    .player_mut(player_id)?
                    .tracker
                    .count_queued(entry.count.negated());
            } else if !self.players.player(player_id)?.techs.is_empty() {
                self.refund_researches(player_id, idx, entry, entry.count)?;
            }
        }
        if let Some(production) = self
            .parts
            .production
            .get_mut(self.players.live(player_id, idx)?.id)
        {
            production.queue.count = QueueLength::ZERO;
        }
        self.end_production_action(player_id, idx)?;
        if let Some(production) = self
            .parts
            .production
            .get_mut(self.players.live(player_id, idx)?.id)
        {
            production.queue.changes = production.queue.changes.bumped();
            production.queue.total = QueuedUnits::ZERO;
        }
        Ok(())
    }

    /// `count` units of a unit entry `entry` of player `player_id` cancelled (`FUN_1411cff00`,
    /// `FUN_1411d61e0`): each its refund ([`refund`]) and its tracker place (the counter one
    /// less, [`crate::players::tracker::Tracker::unplace`]); then attribute 80 lowers by the population
    /// times the count (slot 0xf8 with −population × count).
    pub(super) fn cancel_units(
        &mut self,
        player_id: PlayerId,
        entry: &Entry,
        count: QueuedUnits,
    ) -> GenieResult<()> {
        use crate::production::population;
        let class = entry
            .master
            .map_or(Key::NONE, |master_id| {
                self.tracker_key(player_id, master_id)
            })
            .class;
        let master = entry
            .master
            .and_then(|master_id| player_master(&self.model.masters, player_id, master_id))
            .cloned();
        let owner = self.players.player_mut(player_id)?;
        for _ in 0..count.get().max(0) {
            if let Some(master) = &master {
                refund(master, &entry.costs, owner)?;
            }
            owner.tracker.count_queued(QueuedUnits::ONE.negated());
            owner.tracker.unplace(entry.master, class);
        }
        let pop = master.as_ref().map_or(Amount::ZERO, |master| {
            population(&master.storage, &self.model.roles)
        });
        owner.add_attribute(
            self.model.slots.attribute.queued,
            -pop * f32::from(count.get()),
        )
    }

    /// `count` researches of a research entry `entry` at the building at `idx` of player
    /// `player_id` cancelled (`FUN_1411cff00`, `FUN_1411d61e0`): the refund of what the entry paid
    /// times the count (`FUN_1413d15e0`, [`refund_research`]; the technology in state 1 with no
    /// work done), the costs the building paid back to its store (`FUN_1411c7a00`: each of the
    /// `.dat`'s cost entries paid by the building, flag 2, its amount once), the technology out of
    /// the building's disabled list (`FUN_1411c5250`) and its researches queued (`+0x58`) lower by
    /// the count.
    pub(super) fn refund_researches(
        &mut self,
        player_id: PlayerId,
        idx: ListIndex,
        entry: &Entry,
        count: QueuedUnits,
    ) -> GenieResult<()> {
        // the entry's technology is one of the tree: RESEARCH queued it, the load read it
        let tree = self.players.player(player_id)?.techs.len();
        let Some(tech) = entry.tech.map(TechId::index).filter(|&tech| tech < tree) else {
            return Ok(());
        };
        let owner = self.players.player_mut(player_id)?;
        refund_research(&entry.costs, count, owner)?;
        let state = owner.techs.checked_mut(tech)?;
        state.state = ResearchState::Available;
        state.research_done = Seconds::ZERO;
        let building = owner.objects.checked_mut(idx)?;
        if let Some(def) = self.model.techs.get(tech) {
            for cost in def
                .costs
                .iter()
                .filter(|cost| cost.paid == CostPaid::BUILDING)
            {
                // a cost entry of the building's own store names an attribute
                let attribute = cost.attribute.ok_or(NO_ATTRIBUTE)?;
                match self
                    .parts
                    .combat_mut(building.id)?
                    .store
                    .iter_mut()
                    .find(|stored| stored.0 == attribute)
                {
                    Some(stored) => stored.1 += Amount::new(cost.amount.as_f32()),
                    None => self
                        .parts
                        .combat_mut(building.id)?
                        .store
                        .push((attribute, Amount::new(cost.amount.as_f32() + 0.0))),
                }
            }
        }
        if let (Some(production), Some(tech)) =
            (self.parts.production.get_mut(building.id), entry.tech)
        {
            production.enable_tech(tech);
        }
        let pending = &mut owner.techs.checked_mut(tech)?.pending;
        *pending = pending.minus(count.get());
        Ok(())
    }

    /// `UNIT_ORDER`'s action 4 (`FUN_14161e370` case 4 -> `FUN_1411d66b0`) at the building at
    /// `idx` of player `player_id`: the entry of the `index`-th unit
    /// ([`crate::production::Queue::entry_of_unit`]; none past the last unit) is cancelled.
    /// A unit entry with [`Batch::Five`] (the command's `+0x1d`) gives up to five of its kind
    /// back ([`World::cancel_five`]); with [`Batch::One`], one unit ([`World::give_back_one`]). A research entry,
    /// for a player with a tech tree: for the first unit the list's first action (the research)
    /// loses its progress (`+0x70`) and the technology its work done; then the batch's
    /// researches go ([`World::cancel_some`] with the entry and its technology). Otherwise
    /// (one unit, or a research entry of a player without a tech tree, which gives nothing
    /// back) the entry's count, the total and the change counter lower, an emptied entry going
    /// ([`crate::production::Queue::close`]), and for the first unit (index 0) the production
    /// list's action goes ([`World::end_production_action`]; the interface's `+0xc02` and the
    /// local player's event 0x7d are left).
    pub(crate) fn cancel_queued(
        &mut self,
        player_id: PlayerId,
        idx: ListIndex,
        index: UnitIndex,
        batch: Batch,
    ) -> GenieResult<()> {
        let Some(production) = self
            .parts
            .production
            .get(self.players.live(player_id, idx)?.id)
        else {
            return Ok(());
        };
        let Some(slot_index) = production.queue.entry_of_unit(index) else {
            return Ok(());
        };
        let entry = *production.queue.slots.checked(slot_index)?;
        if entry.tech.is_none() {
            if batch == Batch::Five {
                return self.cancel_five(player_id, idx, index, slot_index);
            }
            self.give_back_one(player_id, &entry)?;
        } else if !self.players.player(player_id)?.techs.is_empty() {
            if index == UnitIndex::FIRST {
                let owner = self.players.player_mut(player_id)?;
                let id = owner.objects.checked(idx)?.id;
                if let Some(Some(action)) = self
                    .parts
                    .production
                    .get_mut(id)
                    .map(|production| production.action.as_mut())
                {
                    match action {
                        Making::Research(make_tech) => make_tech.progress = Seconds::ZERO,
                        // the game writes the make action's +0x70, its unique id
                        Making::Unit(make_obj) => make_obj.unique_id = 0,
                    }
                    if let Some(state) = entry
                        .tech
                        .and_then(|tech_id| owner.techs.get_mut(tech_id.index()))
                    {
                        state.research_done = Seconds::ZERO;
                    }
                }
            }
            let count = batch.units();
            return self.cancel_some(player_id, idx, (None, count), slot_index as i32, entry.tech);
        }
        let owner = self.players.player_mut(player_id)?;
        let queue = &mut self
            .parts
            .production
            .get_mut(owner.objects.checked(idx)?.id)
            .ok_or(NO_PRODUCTION)?
            .queue;
        let slot = queue.slots.checked_mut(slot_index)?;
        slot.count = slot.count.minus(QueuedUnits::ONE);
        queue.total = queue.total.minus(QueuedUnits::ONE);
        queue.changes = queue.changes.bumped();
        if queue.slots.checked(slot_index)?.count < QueuedUnits::ONE {
            queue.close(slot_index, QueueEntry::Train)?;
        }
        if index == UnitIndex::FIRST {
            self.end_production_action(player_id, idx)?;
        }
        Ok(())
    }

    /// `FUN_1411d66b0`'s one unit of entry `entry` given back to player `player_id`: its refund
    /// ([`refund`]), the tracker's counter one less and its place back, attribute 80 lower by
    /// its population (slot 0xf8).
    pub(super) fn give_back_one(&mut self, player_id: PlayerId, entry: &Entry) -> GenieResult<()> {
        let class = entry
            .master
            .map_or(Key::NONE, |master_id| {
                self.tracker_key(player_id, master_id)
            })
            .class;
        let master = entry
            .master
            .and_then(|master_id| player_master(&self.model.masters, player_id, master_id))
            .cloned();
        let owner = self.players.player_mut(player_id)?;
        if let Some(master) = &master {
            refund(master, &entry.costs, owner)?;
        }
        owner.tracker.count_queued(QueuedUnits::ONE.negated());
        owner.tracker.unplace(entry.master, class);
        let pop = master.as_ref().map_or(Amount::ZERO, |master| {
            population(&master.storage, &self.model.roles)
        });
        owner.add_attribute(self.model.slots.attribute.queued, -pop)
    }

    /// `FUN_1411d66b0`'s cancel of up to five of a kind, from the unit entry `entry` of the
    /// `index`-th unit of the queue of the building at `idx` of player `owner`: while the entry at
    /// `entry` is a unit entry with units of the first entry's master or of a villager's (83, 293:
    /// the two sexes count as one kind), at most what is left of five go ([`World::cancel_some`]
    /// with its master, from `index`). When the entry at `entry` is then a research's, or `entry`
    /// is past the last entry, `entry` steps back one; the loop ends once five went, the queue is
    /// empty, or the entry at `entry` is a research's, empty or of another kind. A step back from
    /// the first entry reads before the buffer in the game (heap memory the simulation does not
    /// hold): taken as no entry of the kind, which ends the loop.
    pub(super) fn cancel_five(
        &mut self,
        owner: PlayerId,
        idx: ListIndex,
        index: UnitIndex,
        entry: usize,
    ) -> GenieResult<()> {
        fn queue(world: &World, player_id: PlayerId, idx: ListIndex) -> GenieResult<&Queue> {
            Ok(&world
                .parts
                .production
                .get(world.players.live(player_id, idx)?.id)
                .ok_or(NO_PRODUCTION)?
                .queue)
        }
        let first = *queue(self, owner, idx)?.slots.checked(entry)?;
        let kind = first.master;
        let mut slot_index = entry as i32;
        let mut left = first.count.min(QueuedUnits::of_data(5));
        let mut master = kind;
        let mut gone: u16 = 0;
        let mut queue_length = queue(self, owner, idx)?.count;
        loop {
            if queue_length == QueueLength::ZERO {
                return Ok(());
            }
            let Some(slot) = usize::try_from(slot_index)
                .ok()
                .map(|index| {
                    queue(self, owner, idx).and_then(|queue| queue.slots.checked(index).copied())
                })
                .transpose()?
            else {
                return Ok(());
            };
            if slot.tech.is_some() || slot.count == QueuedUnits::ZERO {
                return Ok(());
            }
            // The villager's masters, whose two sexes a cancel of five counts as one kind
            // (`FUN_1411d66b0`, `FUN_1411d61e0`).
            if master != kind
                && !self
                    .model
                    .roles
                    .of_some_master(master)
                    .contains(MasterRoles::VILLAGER_MALE)
                && !self
                    .model
                    .roles
                    .of_some_master(master)
                    .contains(MasterRoles::VILLAGER_FEMALE)
            {
                return Ok(());
            }
            let take = if left <= slot.count { left } else { slot.count };
            self.cancel_some(owner, idx, (master, take), index.signed(), None)?;
            gone = gone.wrapping_add(take.get() as u16);
            let queue = queue(self, owner, idx)?;
            let at = *queue.slots.checked(slot_index as usize)?;
            queue_length = queue.count;
            if at.tech.is_some() || i32::from(queue_length.get()) - 1 < slot_index {
                slot_index -= 1;
            }
            if QueueLength::ZERO < queue_length {
                left = QueuedUnits::of_data(5 - gone as i16);
                // before the buffer: no master of the kind
                master = match usize::try_from(slot_index) {
                    Ok(index) => queue.slots.checked(index)?.master,
                    Err(_) => None,
                };
            } else {
                left = take;
            }
            if 4 < gone as i16 {
                return Ok(());
            }
        }
    }

    /// `FUN_1411d61e0`, the cancel of `count` of master `master` (or of technology `tech`, not −1)
    /// in the queue of the building at `idx` of player `player_id`, from entry `from` (held to the
    /// last). A research: the entry at `from` gives back what its researches paid
    /// ([`World::refund_researches`], its count at most the entry's). A unit: the first entry at
    /// `from` or before it of that master (none: nothing) gives back its units, at most its count
    /// ([`World::cancel_units`]), and the tracker's counter lowers by the count once more. The
    /// entry's count, the total and the change counter lower; an emptied entry goes
    /// ([`crate::production::Queue::close`]; a research's leaves the train times in their slots).
    /// For the first entry the production list's action goes ([`World::end_production_action`]; the
    /// interface's `+0xc02` and the local player's event 0x7d are left) unless a villager's entry
    /// went, the cancel did not start at the first unit, and a villager's entry with units followed
    /// it (the make action goes on for the other sex).
    pub(crate) fn cancel_some(
        &mut self,
        player_id: PlayerId,
        idx: ListIndex,
        (master, count): (Option<MasterId>, QueuedUnits),
        from: i32,
        tech: Option<TechId>,
    ) -> GenieResult<()> {
        let Some(production) = self
            .parts
            .production
            .get(self.players.live(player_id, idx)?.id)
        else {
            return Ok(());
        };
        let queue = &production.queue;
        if queue.count == QueueLength::ZERO {
            return Ok(());
        }
        let start = from.min(i32::from(queue.count.get()) - 1);
        if start < 0 {
            return Ok(());
        }
        let found = if tech.is_none() {
            (0..=start as usize).rev().find(|&index| {
                queue
                    .slots
                    .get(index)
                    .is_some_and(|entry| entry.master == master)
            })
        } else {
            Some(start as usize)
        };
        let Some(index) = found else {
            return Ok(());
        };
        let entry = *queue.slots.checked(index)?;
        let take = count.min(entry.count);
        if tech.is_none() {
            self.cancel_units(player_id, &entry, take)?;
            self.players
                .player_mut(player_id)?
                .tracker
                .count_queued(take.negated());
        } else {
            let with_tech = Entry { tech, ..entry };
            self.refund_researches(player_id, idx, &with_tech, take)?;
        }
        let queue = &mut self
            .parts
            .production
            .get_mut(self.players.live(player_id, idx)?.id)
            .ok_or(NO_PRODUCTION)?
            .queue;
        let slot = queue.slots.checked_mut(index)?;
        slot.count = slot.count.minus(take);
        queue.total = queue.total.minus(take);
        queue.changes = queue.changes.bumped();
        if QueuedUnits::ZERO < queue.slots.checked(index)?.count {
            return Ok(());
        }
        let villager = |master_id: Option<MasterId>| {
            self.model
                .roles
                .of_some_master(master_id)
                .contains(MasterRoles::VILLAGER_MALE)
                || self
                    .model
                    .roles
                    .of_some_master(master_id)
                    .contains(MasterRoles::VILLAGER_FEMALE)
        };
        // the game reads the next slot before the move; it matters for the first entry alone
        let pair = tech.is_none()
            && index == 0
            && villager(queue.slots.checked(0)?.master)
            && queue
                .slots
                .get(1)
                .is_some_and(|entry| villager(entry.master));
        queue.close(
            index,
            if tech.is_none() {
                QueueEntry::Train
            } else {
                QueueEntry::Research
            },
        )?;
        if index != 0 {
            return Ok(());
        }
        if tech.is_none() && start != 0 && pair && QueuedUnits::ZERO < queue.slots.checked(0)?.count
        {
            return Ok(());
        }
        self.end_production_action(player_id, idx)
    }

    /// `FUN_14161e370` case 4's tail after the cancel at the building at `idx` of player
    /// `player_id`: in game mode 16, with the player's attribute 6 at most 1 (`COMISS`, `JC`: a NaN
    /// is not), the master of the queue's slot at the unit index `index` (read as a slot's index,
    /// after the cancel) keys a military class ([`crate::players::tracker::Key::military`]): the
    /// player's flag of a military unit queued in that mode (`+0x1378`) clears. An index past the
    /// buffer reads heap memory the simulation does not hold: taken as no master.
    pub(crate) fn cancel_tail(
        &mut self,
        player_id: PlayerId,
        idx: ListIndex,
        index: UnitIndex,
    ) -> GenieResult<()> {
        if self.settings.game_mode != GameMode::OneUnit {
            return Ok(());
        }
        let attribute =
            // The player's attribute that mode reads (6, `+0x30`).
            Attributes::get(self.players.player(player_id)?, self.model.slots.attribute.current_age)?;
        if below_or_unordered(ONE_UNIT_AGE, attribute.get()) {
            return Ok(());
        }
        let master = self
            .parts
            .production
            .get(self.players.live(player_id, idx)?.id)
            .and_then(|production| production.queue.slots.get(index.signed() as usize))
            .and_then(|entry| entry.master);
        if let Some(master) = master
            && self.tracker_key(player_id, master).military
        {
            self.players.player_mut(player_id)?.one_military_queued = false;
        }
        Ok(())
    }
}
