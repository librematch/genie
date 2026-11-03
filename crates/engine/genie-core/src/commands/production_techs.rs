//! QUEUE's technologies: a building's researches, paid and queued, joining its last entry or a
//! new one.

use crate::GenieError;
use crate::objects::ListIndex;
use crate::objects::masters::MasterId;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::production::QueueButton;
use crate::production::QueuedUnits;
use crate::production::{Queue, as_entry_costs, same_costs};
use crate::technology::Repeat;
use crate::technology::RepeatLimit;
use crate::technology::TechId;
use crate::technology::{Attributes, TechDef, factors};
use crate::world::World;
use crate::world::layer::Slots;
use crate::world::player::Player;
use crate::world::slots::CheckedIndex;
use crate::world::slots::PlayerSlots;
use std::mem::take;

impl World {
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
        count: QueuedUnits,
        button: QueueButton,
    ) -> Result<bool, GenieError> {
        let t = tech.index();
        let def = self.model.techs.get(t).ok_or(GenieError::unsupported(
            "a technology the data file has not got",
        ))?;
        let owner = self.players.player_mut(player)?;
        let state = owner.techs.checked(t)?;
        let done = state
            .pending
            .get()
            .wrapping_add(state.times_researched.get());
        let cap = state.research_cap.limit();
        let mut n = count.get();
        if state.stack && i32::from(cap) - i32::from(done) < i32::from(n) {
            n = cap.wrapping_sub(done);
        }
        // FUN_14162fd90's repeat flag: the player's +0x648 (not held) or FUN_1413d1050
        let repeat = if state.repeatable(def, RepeatLimit::Own) {
            Repeat::Allowed
        } else {
            Repeat::Denied
        };
        let building = owner.objects.checked(i)?.life.master_id;
        n = pay_researches(owner, (def, tech), repeat, building, n, self.model.slots)?;
        if n <= 0 {
            return Ok(false);
        }
        let get = |k: AttributeId| Attributes::get(&*owner, k);
        let listed = owner
            .techs
            .checked(t)?
            .listed_costs(def, factors(tech, &get, self.model.slots)?)?;
        let costs = as_entry_costs(listed);
        let id = owner.objects.checked(i)?.id;
        let production = self.production_or_new(id)?;
        if join_last(&mut production.queue, tech, &costs, n)? {
            let state = self.players.player_mut(player)?.techs.checked_mut(t)?;
            state.pending = state.pending.plus(n);
            return Ok(true);
        }
        self.new_entry(
            player,
            i,
            (master, Some(tech)),
            QueuedUnits::of_data(n),
            button,
            None,
        )?;
        let pending = &mut self
            .players
            .player_mut(player)?
            .techs
            .checked_mut(t)?
            .pending;
        *pending = pending.plus(n);
        Ok(true)
    }
}

/// [`World::queue_techs`]'s payment (`FUN_14162fd90`) of up to `n` researches of `tech` (its
/// definition `def`) at a building of master `building`: each is paid until one cannot be; the
/// count paid.
fn pay_researches(
    owner: &mut Player,
    (def, tech): (&TechDef, TechId),
    repeat: Repeat,
    building: MasterId,
    n: i16,
    slots: Slots,
) -> Result<i16, GenieError> {
    let t = tech.index();
    let mut paid = 0i16;
    while paid < n {
        let get = |k: AttributeId| Attributes::get(&*owner, k);
        let factors = factors(tech, &get, slots)?;
        let mut state = take(owner.techs.checked_mut(t)?);
        let ok = state.pay(def, factors, repeat, building, owner, slots);
        *owner.techs.checked_mut(t)? = state;
        if !ok? {
            return Ok(paid);
        }
        paid += 1;
    }
    Ok(n)
}

/// [`World::queue_techs`]'s `n` researches of `tech` join the queue's last entry when it is of
/// that technology and its costs are `costs` (`FUN_1411c81a0` -> `FUN_1413d4a00`): its count,
/// the total and the change counter grow. Whether they joined.
fn join_last(
    queue: &mut Queue,
    tech: TechId,
    costs: &[(Option<AttributeId>, Amount); 3],
    n: i16,
) -> Result<bool, GenieError> {
    let Some(last) = queue.last() else {
        return Ok(false);
    };
    if last.tech != Some(tech) || !same_costs(&last.costs, costs) {
        return Ok(false);
    }
    let k = (queue.count.get() - 1) as usize;
    let slot = queue.slots.checked_mut(k)?;
    slot.count = slot.count.plus(QueuedUnits::of_data(n));
    queue.total = queue.total.plus(QueuedUnits::of_data(n));
    queue.changes = queue.changes.bumped();
    Ok(true)
}
