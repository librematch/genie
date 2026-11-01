//! A building's queue cancelled: the whole queue, its units and researches refunded, and
//! `UNIT_ORDER`'s cancel of one, five or some.

use crate::GenieError;
use crate::objects::ListIndex;
use crate::objects::masters::MasterId;
use crate::objects::masters::ids::VILLAGER_FEMALE;
use crate::objects::masters::ids::VILLAGER_MALE;
use crate::objects::masters::player_master;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute;
use crate::players::attribute::QUEUED;
use crate::players::game_status::ONE_UNIT_AGE;
use crate::players::tracker::Key;
use crate::production::QueuedUnits;
use crate::production::UnitIndex;
use crate::production::{Making, refund, refund_research};
use crate::technology::CostPaid;
use crate::technology::ResearchState;
use crate::technology::TechId;
use crate::world::Seconds;
use crate::world::World;
use crate::world::slots::{CheckedIndex, PlayerSlots};

/// A cancel finds the building without the production it cancels from.
const NO_PRODUCTION: GenieError =
    GenieError::unreachable("a cancelled queue entry of a building without production");

impl World {
    /// `FUN_1411cff00`, the cancel of the whole queue of the building at `idx` of player `p`,
    /// entry by entry. A unit entry: each unit gets its refund ([`refund`]) and the tracker its
    /// place back (the counter one less, [`crate::players::tracker::Tracker::unplace`]); attribute 80
    /// lowers by the population times the count (slot 0xf8), and the tracker's counter by the
    /// count once more. A research entry, for a player with a tech tree: its count's refund
    /// ([`World::refund_researches`]). Then the interface's `+0xc02` (the local player's event
    /// 0x7d) is left; the entries go, the production list's action goes
    /// ([`World::end_production_action`]), the change counter grows and the total is 0.
    pub(crate) fn cancel_queue(&mut self, p: PlayerId, idx: ListIndex) -> Result<(), GenieError> {
        let Some(production) = self.players.live(p, idx)?.production.as_ref() else {
            return Ok(());
        };
        let entries = production.queue.entries().to_vec();
        for e in &entries {
            if e.tech.is_none() {
                self.cancel_units(p, e, e.count)?;
                self.players
                    .player_mut(p)?
                    .tracker
                    .count_queued(e.count.negated());
            } else if !self.players.player(p)?.techs.is_empty() {
                self.refund_researches(p, idx, e, e.count)?;
            }
        }
        if let Some(production) = self.players.live_mut(p, idx)?.production.as_mut() {
            production.queue.count = crate::production::QueueLength::ZERO;
        }
        self.end_production_action(p, idx)?;
        if let Some(production) = self.players.live_mut(p, idx)?.production.as_mut() {
            production.queue.changes = production.queue.changes.bumped();
            production.queue.total = QueuedUnits::ZERO;
        }
        Ok(())
    }

    /// `count` units of a unit entry `e` of player `p` cancelled (`FUN_1411cff00`,
    /// `FUN_1411d61e0`): each its refund ([`refund`]) and its tracker place (the counter one
    /// less, [`crate::players::tracker::Tracker::unplace`]); then attribute 80 lowers by the population
    /// times the count (slot 0xf8 with −population × count).
    pub(super) fn cancel_units(
        &mut self,
        p: PlayerId,
        e: &crate::production::Entry,
        count: QueuedUnits,
    ) -> Result<(), GenieError> {
        use crate::production::population;
        let class = e.master.map_or(Key::NONE, |m| self.tracker_key(p, m)).class;
        let master = e
            .master
            .and_then(|m| player_master(&self.masters, p, m))
            .cloned();
        let owner = self.players.player_mut(p)?;
        for _ in 0..count.get().max(0) {
            if let Some(m) = &master {
                refund(m, &e.costs, owner)?;
            }
            owner.tracker.count_queued(QueuedUnits::ONE.negated());
            owner.tracker.unplace(e.master, class);
        }
        let pop = master
            .as_ref()
            .map_or(Amount::ZERO, |m| population(&m.storage));
        owner.add_attribute(QUEUED, -pop * f32::from(count.get()))
    }

    /// `count` researches of a research entry `e` at the building at `idx` of player `p`
    /// cancelled (`FUN_1411cff00`, `FUN_1411d61e0`): the refund of what the entry paid times
    /// the count (`FUN_1413d15e0`, [`refund_research`]; the technology in state 1 with no work
    /// done), the costs the building paid back to its store (`FUN_1411c7a00`: each of the
    /// `.dat`'s cost entries paid by the building, flag 2, its amount once), the technology out of
    /// the building's disabled list (`FUN_1411c5250`) and its researches queued (`+0x58`) lower
    /// by the count.
    pub(super) fn refund_researches(
        &mut self,
        p: PlayerId,
        idx: ListIndex,
        e: &crate::production::Entry,
        count: QueuedUnits,
    ) -> Result<(), GenieError> {
        // the entry's technology is one of the tree: RESEARCH queued it, the load read it
        let tree = self.players.player(p)?.techs.len();
        let Some(t) = e.tech.map(TechId::index).filter(|&t| t < tree) else {
            return Ok(());
        };
        let owner = self.players.player_mut(p)?;
        refund_research(&e.costs, count, owner)?;
        let state = owner.techs.checked_mut(t)?;
        state.state = ResearchState::Available;
        state.research_done = Seconds::ZERO;
        let building = owner.objects.checked_mut(idx)?;
        if let Some(def) = self.techs.get(t) {
            for c in def.costs.iter().filter(|c| c.paid == CostPaid::BUILDING) {
                // a cost entry of the building's own store names an attribute
                let attribute = c.attribute.ok_or(crate::world::NO_ATTRIBUTE)?;
                match building.combat.store.iter_mut().find(|s| s.0 == attribute) {
                    Some(s) => s.1 += Amount::new(c.amount.as_f32()),
                    None => building
                        .combat
                        .store
                        .push((attribute, Amount::new(c.amount.as_f32() + 0.0))),
                }
            }
        }
        if let (Some(production), Some(tech)) = (building.production.as_mut(), e.tech) {
            production.enable_tech(tech);
        }
        let pending = &mut owner.techs.checked_mut(t)?.pending;
        *pending = pending.minus(count.get());
        Ok(())
    }

    /// `UNIT_ORDER`'s action 4 (`FUN_14161e370` case 4 -> `FUN_1411d66b0`) at the building at
    /// `idx` of player `p`: the entry of the `index`-th unit
    /// ([`crate::production::Queue::entry_of_unit`]; none past the last unit) is cancelled.
    /// A unit entry with `all` (the command's `+0x1d`) gives up to five of its kind back
    /// ([`World::cancel_five`]); without, one unit ([`World::give_back_one`]). A research entry,
    /// for a player with a tech tree: for the first unit the list's first action (the research)
    /// loses its progress (`+0x70`) and the technology its work done; then one research, five
    /// with `all`, goes ([`World::cancel_some`] with the entry and its technology). Otherwise
    /// (one unit, or a research entry of a player without a tech tree, which gives nothing
    /// back) the entry's count, the total and the change counter lower, an emptied entry going
    /// ([`crate::production::Queue::close`]), and for the first unit (index 0) the production
    /// list's action goes ([`World::end_production_action`]; the interface's `+0xc02` and the
    /// local player's event 0x7d are left).
    pub(crate) fn cancel_queued(
        &mut self,
        p: PlayerId,
        idx: ListIndex,
        index: UnitIndex,
        all: bool,
    ) -> Result<(), GenieError> {
        let Some(production) = self.players.live(p, idx)?.production.as_ref() else {
            return Ok(());
        };
        let Some(e) = production.queue.entry_of_unit(index) else {
            return Ok(());
        };
        let entry = *production.queue.slots.checked(e)?;
        if entry.tech.is_none() {
            if all {
                return self.cancel_five(p, idx, index, e);
            }
            self.give_back_one(p, &entry)?;
        } else if !self.players.player(p)?.techs.is_empty() {
            if index == UnitIndex::FIRST {
                let owner = self.players.player_mut(p)?;
                let unit = owner.objects.checked_mut(idx)?;
                if let Some(Some(action)) = unit.production.as_mut().map(|q| q.action.as_mut()) {
                    match action {
                        Making::Research(r) => r.progress = Seconds::ZERO,
                        // the game writes the make action's +0x70, its unique id
                        Making::Unit(m) => m.unique_id = 0,
                    }
                    if let Some(state) = entry.tech.and_then(|t| owner.techs.get_mut(t.index())) {
                        state.research_done = Seconds::ZERO;
                    }
                }
            }
            let count = QueuedUnits::of_data(i16::from(all) * 4 + 1);
            return self.cancel_some(p, idx, (None, count), e as i32, entry.tech);
        }
        let owner = self.players.player_mut(p)?;
        let queue = &mut owner
            .objects
            .checked_mut(idx)?
            .production
            .as_mut()
            .ok_or(NO_PRODUCTION)?
            .queue;
        let slot = queue.slots.checked_mut(e)?;
        slot.count = slot.count.minus(QueuedUnits::ONE);
        queue.total = queue.total.minus(QueuedUnits::ONE);
        queue.changes = queue.changes.bumped();
        if queue.slots.checked(e)?.count < QueuedUnits::ONE {
            queue.close(e, true)?;
        }
        if index == UnitIndex::FIRST {
            self.end_production_action(p, idx)?;
        }
        Ok(())
    }

    /// `FUN_1411d66b0`'s one unit of entry `entry` given back to player `p`: its refund
    /// ([`refund`]), the tracker's counter one less and its place back, attribute 80 lower by
    /// its population (slot 0xf8).
    pub(super) fn give_back_one(
        &mut self,
        p: PlayerId,
        entry: &crate::production::Entry,
    ) -> Result<(), GenieError> {
        let class = entry
            .master
            .map_or(Key::NONE, |m| self.tracker_key(p, m))
            .class;
        let master = entry
            .master
            .and_then(|m| player_master(&self.masters, p, m))
            .cloned();
        let owner = self.players.player_mut(p)?;
        if let Some(m) = &master {
            refund(m, &entry.costs, owner)?;
        }
        owner.tracker.count_queued(QueuedUnits::ONE.negated());
        owner.tracker.unplace(entry.master, class);
        let pop = master
            .as_ref()
            .map_or(Amount::ZERO, |m| crate::production::population(&m.storage));
        owner.add_attribute(QUEUED, -pop)
    }

    /// `FUN_1411d66b0`'s cancel of up to five of a kind, from the unit entry `e` of the
    /// `index`-th unit of the queue of the building at `idx` of player `p`: while the entry at
    /// `e` is a unit entry with units of the first entry's master or of a villager's (83, 293:
    /// the two sexes count as one kind), at most what is left of five go
    /// ([`World::cancel_some`] with its master, from `index`). When the entry at `e` is then a
    /// research's, or `e` is past the last entry, `e` steps back one; the loop ends once five
    /// went, the queue is empty, or the entry at `e` is a research's, empty or of another
    /// kind. A step back from the first entry reads before the buffer in the game (heap memory
    /// the simulation does not hold): taken as no entry of the kind, which ends the loop.
    pub(super) fn cancel_five(
        &mut self,
        p: PlayerId,
        idx: ListIndex,
        index: UnitIndex,
        e: usize,
    ) -> Result<(), GenieError> {
        fn queue(
            t: &World,
            p: PlayerId,
            idx: ListIndex,
        ) -> Result<&crate::production::Queue, GenieError> {
            Ok(&t
                .players
                .live(p, idx)?
                .production
                .as_ref()
                .ok_or(NO_PRODUCTION)?
                .queue)
        }
        let first = *queue(self, p, idx)?.slots.checked(e)?;
        let kind = first.master;
        let mut e = e as i32;
        let mut left = first.count.min(QueuedUnits::of_data(5));
        let mut master = kind;
        let mut gone: u16 = 0;
        let mut n = queue(self, p, idx)?.count;
        loop {
            if n == crate::production::QueueLength::ZERO {
                return Ok(());
            }
            let Some(slot) = usize::try_from(e)
                .ok()
                .map(|e| queue(self, p, idx).and_then(|q| q.slots.checked(e).copied()))
                .transpose()?
            else {
                return Ok(());
            };
            if slot.tech.is_some() || slot.count == QueuedUnits::ZERO {
                return Ok(());
            }
            // The villager's masters, whose two sexes a cancel of five counts as one kind
            // (`FUN_1411d66b0`, `FUN_1411d61e0`).
            if master != kind && master != Some(VILLAGER_MALE) && master != Some(VILLAGER_FEMALE) {
                return Ok(());
            }
            let take = if left <= slot.count { left } else { slot.count };
            self.cancel_some(p, idx, (master, take), index.signed(), None)?;
            gone = gone.wrapping_add(take.get() as u16);
            let q = queue(self, p, idx)?;
            let at = *q.slots.checked(e as usize)?;
            n = q.count;
            if at.tech.is_some() || i32::from(n.get()) - 1 < e {
                e -= 1;
            }
            if crate::production::QueueLength::ZERO < n {
                left = QueuedUnits::of_data(5 - gone as i16);
                // before the buffer: no master of the kind
                master = match usize::try_from(e) {
                    Ok(e) => q.slots.checked(e)?.master,
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

    /// `FUN_1411d61e0`, the cancel of `count` of master `master` (or of technology `tech`, not
    /// −1) in the queue of the building at `idx` of player `p`, from entry `from` (held to the
    /// last). A research: the entry at `from` gives back what its researches paid
    /// ([`World::refund_researches`], its count at most the entry's). A unit: the first entry
    /// at `from` or before it of that master (none: nothing) gives back its units, at most its
    /// count ([`World::cancel_units`]), and the tracker's counter lowers by the count once more.
    /// The entry's count, the total and the change counter lower; an emptied entry goes
    /// ([`crate::production::Queue::close`]; a research's leaves the train times in their
    /// slots). For the first entry the production list's action goes
    /// ([`World::end_production_action`]; the interface's `+0xc02` and the local player's event
    /// 0x7d are left) unless a villager's entry went, the cancel did not start at the first
    /// unit, and a villager's entry with units followed it (the make action goes on for the
    /// other sex).
    pub(crate) fn cancel_some(
        &mut self,
        p: PlayerId,
        idx: ListIndex,
        (master, count): (Option<MasterId>, QueuedUnits),
        from: i32,
        tech: Option<TechId>,
    ) -> Result<(), GenieError> {
        let Some(production) = self.players.live(p, idx)?.production.as_ref() else {
            return Ok(());
        };
        let q = &production.queue;
        if q.count == crate::production::QueueLength::ZERO {
            return Ok(());
        }
        let start = from.min(i32::from(q.count.get()) - 1);
        if start < 0 {
            return Ok(());
        }
        let found = if tech.is_none() {
            (0..=start as usize)
                .rev()
                .find(|&i| q.slots.get(i).is_some_and(|s| s.master == master))
        } else {
            Some(start as usize)
        };
        let Some(i) = found else {
            return Ok(());
        };
        let entry = *q.slots.checked(i)?;
        let take = count.min(entry.count);
        if tech.is_none() {
            self.cancel_units(p, &entry, take)?;
            self.players
                .player_mut(p)?
                .tracker
                .count_queued(take.negated());
        } else {
            let e = crate::production::Entry { tech, ..entry };
            self.refund_researches(p, idx, &e, take)?;
        }
        let queue = &mut self
            .players
            .live_mut(p, idx)?
            .production
            .as_mut()
            .ok_or(NO_PRODUCTION)?
            .queue;
        let slot = queue.slots.checked_mut(i)?;
        slot.count = slot.count.minus(take);
        queue.total = queue.total.minus(take);
        queue.changes = queue.changes.bumped();
        if QueuedUnits::ZERO < queue.slots.checked(i)?.count {
            return Ok(());
        }
        let villager = |m: Option<MasterId>| m == Some(VILLAGER_MALE) || m == Some(VILLAGER_FEMALE);
        // the game reads the next slot before the move; it matters for the first entry alone
        let pair = tech.is_none()
            && i == 0
            && villager(queue.slots.checked(0)?.master)
            && queue.slots.get(1).is_some_and(|s| villager(s.master));
        queue.close(i, tech.is_none())?;
        if i != 0 {
            return Ok(());
        }
        if tech.is_none() && start != 0 && pair && QueuedUnits::ZERO < queue.slots.checked(0)?.count
        {
            return Ok(());
        }
        self.end_production_action(p, idx)
    }

    /// `FUN_14161e370` case 4's tail after the cancel at the building at `idx` of player `p`:
    /// in game mode 16, with the player's attribute 6 at most 1 (`COMISS`, `JC`: a NaN is not),
    /// the master of the queue's slot at the unit index `index` (read as a slot's index, after
    /// the cancel) keys a military class ([`crate::players::tracker::Key::military`]): the player's flag
    /// of a military unit queued in that mode (`+0x1378`) clears. An index past the buffer
    /// reads heap memory the simulation does not hold: taken as no master.
    pub(crate) fn cancel_tail(
        &mut self,
        p: PlayerId,
        idx: ListIndex,
        index: UnitIndex,
    ) -> Result<(), GenieError> {
        if self.settings.game_mode != crate::world::GameMode::ONE_UNIT {
            return Ok(());
        }
        let attribute =
            // The player's attribute that mode reads (6, `+0x30`).
            crate::technology::Attributes::get(self.players.player(p)?, attribute::CURRENT_AGE)?;
        if crate::math::fmath::below_or_unordered(ONE_UNIT_AGE, attribute.get()) {
            return Ok(());
        }
        let master = self
            .players
            .live(p, idx)?
            .production
            .as_ref()
            .and_then(|q| q.queue.slots.get(index.signed() as usize))
            .and_then(|s| s.master);
        if let Some(master) = master
            && self.tracker_key(p, master).military
        {
            self.players.player_mut(p)?.one_military_queued = false;
        }
        Ok(())
    }
}
