//! QUEUE's technologies: a building's researches, paid and queued, joining its last entry or a
//! new one.

use crate::GenieResult;
use crate::objects::ListIndex;
use crate::objects::masters::MasterId;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::production::EntryJoin;
use crate::production::Payment;
use crate::production::QueueButton;
use crate::production::QueueOutcome;
use crate::production::QueuedUnits;
use crate::production::{Queue, as_entry_costs, same_costs};
use crate::technology::Repeat;
use crate::technology::RepeatLimit;
use crate::technology::TechId;
use crate::technology::UNKNOWN_TECH;
use crate::technology::{TechDef, factors};
use crate::world::World;
use crate::world::layer::Slots;
use crate::world::player::Player;
use crate::world::slots::CheckedIndexExt;
use crate::world::slots::PlayerSlotsExt;
use std::mem::take;

impl World {
    /// `FUN_1411cbc40`'s technology branch: the count is held to the technology's cap less its
    /// researches queued and done when it stacks (`+0x40`; the cap `+0x56`, 0x100 below 0; the
    /// player's `+0x648` is not held, 0 for every player probed); each research is paid
    /// (`FUN_14162fd90`: [`crate::technology::TechState::pay`] with the repeat test) until one
    /// cannot be. The paid ones join the last entry when it is of the same technology and its costs
    /// are the ones it lists now (`FUN_1411c81a0` -> `FUN_1413d4a00`,
    /// [`crate::technology::TechState::listed_costs`]); else a new entry ([`World::new_entry`]);
    /// the technology's researches queued (`+0x58`) grow by them.
    pub(super) fn queue_techs(
        &mut self,
        player: PlayerId,
        list_index: ListIndex,
        (master, tech): (Option<MasterId>, TechId),
        count: QueuedUnits,
        button: QueueButton,
    ) -> GenieResult<QueueOutcome> {
        let index = tech.index();
        let def = self.model.techs.get(index).ok_or(UNKNOWN_TECH)?;
        let owner = self.players.player_mut(player)?;
        let state = owner.techs.checked(index)?;
        let done = state
            .pending
            .get()
            .wrapping_add(state.times_researched.get());
        let cap = state.research_cap.limit();
        let mut count = count.get();
        if state.stack && i32::from(cap) - i32::from(done) < i32::from(count) {
            count = cap.wrapping_sub(done);
        }
        // FUN_14162fd90's repeat flag: the player's +0x648 (not held) or FUN_1413d1050
        let repeat = if state.repeatable(def, RepeatLimit::Own) {
            Repeat::Allowed
        } else {
            Repeat::Denied
        };
        let building = owner.objects.checked(list_index)?.life.master_id;
        count = pay_researches(
            owner,
            (def, tech),
            repeat,
            building,
            count,
            self.model.slots,
        )?;
        if count <= 0 {
            return Ok(QueueOutcome::NoneQueued);
        }
        let listed = owner
            .techs
            .checked(index)?
            .listed_costs(def, factors(tech, &*owner, self.model.slots)?)?;
        let costs = as_entry_costs(listed);
        let id = owner.objects.checked(list_index)?.id;
        let production = self.production_or_new(id)?;
        if join_last(&mut production.queue, tech, &costs, count)? == EntryJoin::Joined {
            let state = self.players.player_mut(player)?.techs.checked_mut(index)?;
            state.pending = state.pending.plus(count);
            return Ok(QueueOutcome::Queued);
        }
        self.new_entry(
            player,
            list_index,
            (master, Some(tech)),
            QueuedUnits::of_data(count),
            button,
            None,
        )?;
        let pending = &mut self
            .players
            .player_mut(player)?
            .techs
            .checked_mut(index)?
            .pending;
        *pending = pending.plus(count);
        Ok(QueueOutcome::Queued)
    }
}

/// [`World::queue_techs`]'s payment (`FUN_14162fd90`) of up to `count` researches of `tech` (its
/// definition `def`) at a building of master `building`: each is paid until one cannot be; the
/// count paid.
fn pay_researches(
    owner: &mut Player,
    (def, tech): (&TechDef, TechId),
    repeat: Repeat,
    building: MasterId,
    count: i16,
    slots: Slots,
) -> GenieResult<i16> {
    let index = tech.index();
    let mut paid = 0i16;
    while paid < count {
        let factors = factors(tech, &*owner, slots)?;
        let mut state = take(owner.techs.checked_mut(index)?);
        let ok = state.pay(def, factors, repeat, building, owner, slots);
        *owner.techs.checked_mut(index)? = state;
        if ok? == Payment::Unpaid {
            return Ok(paid);
        }
        paid += 1;
    }
    Ok(count)
}

/// [`World::queue_techs`]'s `count` researches of `tech` join the queue's last entry when it is of
/// that technology and its costs are `costs` (`FUN_1411c81a0` -> `FUN_1413d4a00`): its count,
/// the total and the change counter grow. Whether they joined.
fn join_last(
    queue: &mut Queue,
    tech: TechId,
    costs: &[(Option<AttributeId>, Amount); 3],
    count: i16,
) -> GenieResult<EntryJoin> {
    let Some(last) = queue.last() else {
        return Ok(EntryJoin::NewEntry);
    };
    if last.tech != Some(tech) || !same_costs(&last.costs, costs) {
        return Ok(EntryJoin::NewEntry);
    }
    let last = (queue.count.get() - 1) as usize;
    let slot = queue.slots.checked_mut(last)?;
    slot.count = slot.count.plus(QueuedUnits::of_data(count));
    queue.total = queue.total.plus(QueuedUnits::of_data(count));
    queue.changes = queue.changes.bumped();
    Ok(EntryJoin::Joined)
}
