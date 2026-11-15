//! A building's Housing: `TRIBE_Action_Housing` (action type 0x7b, vtable `0x142ed7008`), which
//! `UNIT_ORDER`'s case 3 installs on a building ([`World::housing_order`]), and the work command's
//! factory on a task of its type (`FUN_140aca4b0` case 0x7b). Its start pays its
//! cost in wood; at work its timer runs down by the building's work rate from its owner's
//! attribute 62; once it ran out the building holds more headroom (attribute 4) and its owner
//! gains as much. Its stop, and its destructor while it is at work, give the cost back.
//!
//! Its slots: the constructor `FUN_140ac1b80`, the load `FUN_140ac1bc0`
//! ([`crate::world::load::top_actions`]), the destructor (slot 0, `0x140ac20f0`,
//! [`HousingAction::refund`]), the start (slot 0x28, `0x140ac2240`), the update (slot 0x30,
//! `0x140ac2a10`, [`crate::objects::unit::Unit::housing_part`]), the stop (slot 0x38,
//! `0x140ac2910`) and `set_state` (slot 0x80, `0x140ac2870`). The save (slot 0x20,
//! `0x140ac24e0`) writes the base and `+0x68`. It holds no target and makes no sub-action.
//!
//! [`World::housing_order`]: crate::world::World::housing_order

mod unit;

#[cfg(test)]
pub(crate) mod tests;

use crate::GenieError;
use crate::GenieResult;
use crate::commands::QueueId;
use crate::objects::teardown::Refund;
use crate::players::Amount;
use crate::players::attribute::AttributeId;
use crate::technology::Attributes;
use crate::world::Player;
use crate::world::Seconds;
use crate::world::layer::AttributeSlots;
use genie_diff::Diff;
use strum::FromRepr;

/// A Housing's state (`+0x18`): the constructor's 0, then what its own slots set (`set_state`,
/// slot 0x80, `0x140ac2870`). The other bytes no slot of it gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, FromRepr)]
#[repr(u8)]
pub enum HousingState {
    /// 0: made (the base constructor's, `FUN_140a73e90`), not started yet.
    New = 0,
    /// 1: done: its next update answers 1 and the list deletes it.
    Done = 1,
    /// 6: at work: each update takes the building's work off its timer.
    Working = 6,
    /// 13: failed (its owner could not pay): its next update ends it and answers 3.
    Failed = 13,
}

impl HousingState {
    /// The state of a byte of the save or the game's memory.
    ///
    /// # Errors
    ///
    /// A byte that names no state of a Housing.
    pub fn of_byte(value: u8) -> GenieResult<Self> {
        Self::from_repr(value)
            .ok_or_else(|| GenieError::invalid_input("no state of a Housing").with("state", value))
    }

    /// Its number.
    #[must_use]
    pub const fn get(self) -> u8 {
        self as u8
    }
}

impl Diff for HousingState {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        differ.equal(self, other);
    }
}

/// `TRIBE_Action_Housing`: its state (`+0x18`), its timer (`+0x44`) and the queue id of the order
/// that made it (`+0x68`).
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct HousingAction {
    /// `+0x18`.
    pub(crate) state: HousingState,
    /// `+0x44`: the work still to do. `set_state(6)` starts it at its owner's attribute 62
    /// ([`AttributeSlots::building_house_rate`]); each update at work takes the building's work
    /// off it.
    pub(crate) timer: Seconds,
    /// `+0x68`, `unique_id`: the queue id (`qId`, `+0x14`) of the `UNIT_ORDER` that made it. The
    /// constructor and the load write it, the save (slot 0x20) reads it.
    #[diff(eq)]
    pub(crate) unique_id: QueueId,
}

/// Whether a Housing's timer ran out in its update ([`HousingAction::work`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub(crate) enum WorkLeft {
    /// The timer is above 0, or not a number: the work goes on.
    Some,
    /// The timer is at most 0: the work is done.
    None,
}

impl HousingAction {
    /// The constructor (`FUN_140ac1b80`): the base action's (`FUN_140a73e90` with 1: state 0, no
    /// target, the timer 0), the order's queue id `unique_id` (`+0x68`) and the type 0x7b.
    #[must_use]
    pub const fn new(unique_id: QueueId) -> Self {
        Self {
            state: HousingState::New,
            timer: Seconds::ZERO,
            unique_id,
        }
    }

    /// `set_state` (slot 0x80, `0x140ac2870`) to 1 or 13: the state alone.
    const fn set_state(&mut self, state: HousingState) {
        self.state = state;
    }

    /// `set_state(6)` (slot 0x80, `0x140ac2870`): the state, and the timer at `house_rate`, its
    /// owner's attribute 62 (`player+0x100`'s `+0x1f0`, the low half of the decoded word).
    const fn set_working(&mut self, house_rate: Amount) {
        self.state = HousingState::Working;
        self.timer = Seconds::new(house_rate.get());
    }

    /// Its start as the list's add runs it (slot 0x28 with 1, `0x140ac2240`) for its `owner`, of
    /// the attribute slots `slots`, at the wood `cost` ([`HousingTuning::cost`]): an owner
    /// holding less wood (`COMISS cost, wood`, `JBE`: a NaN wood pays) fails it
    /// (`set_state(13)`); else the cost comes off its wood straight (`SUBSS`, with no escrow) and
    /// it goes to work (`set_state(6)`). The local or a computer player's statistics of the
    /// wood spent (`FUN_14097daf0`) are not held. Its restart (slot 0x28 with 0) does nothing
    /// ([`crate::objects::unit::Unit::restart_head`]).
    ///
    /// [`HousingTuning::cost`]: crate::world::layer::generated::HousingTuning::cost
    ///
    /// # Errors
    ///
    /// An owner without wood or without attribute 62.
    pub(crate) fn start(
        &mut self,
        owner: &mut Player,
        slots: &AttributeSlots,
        cost: Amount,
    ) -> GenieResult<()> {
        let wood = Attributes::get(owner, slots.wood)?;
        if cost > wood {
            self.set_state(HousingState::Failed);
            return Ok(());
        }
        owner.set_attribute(slots.wood, wood - cost)?;
        self.set_working(Attributes::get(owner, slots.building_house_rate)?);
        Ok(())
    }

    /// Its update's work at 6 (`0x140ac2a10`): `work` comes off the timer (`SUBSS`), and
    /// [`WorkLeft::None`] once the timer is at most 0 (`COMISS 0, timer`, `JC`: a NaN timer
    /// works on).
    pub(crate) fn work(&mut self, work: Seconds) -> WorkLeft {
        self.timer -= work;
        if self.timer <= Seconds::ZERO {
            WorkLeft::None
        } else {
            WorkLeft::Some
        }
    }

    /// Done: `set_state(1)`, as its update's end at 6 and 13 and its stop set it.
    pub(crate) const fn finish(&mut self) {
        self.set_state(HousingState::Done);
    }

    /// Its stop (slot 0x38, `0x140ac2910`) for its `owner`: done (`set_state(1)`), and the wood
    /// `cost` goes back to the owner's `wood` in any state ([`give_back`]): also to one that never
    /// paid it (state 0 or 13). It answers 1: the stop keeps the action list.
    ///
    /// # Errors
    ///
    /// An owner without wood.
    pub(crate) fn halt(
        &mut self,
        owner: &mut Player,
        wood: AttributeId,
        cost: Amount,
    ) -> GenieResult<()> {
        self.finish();
        give_back(owner, wood, cost)
    }

    /// What its destructor (slot 0, `0x140ac20f0`) gives back: at work (state 6) the cost, which
    /// its owner's wood takes while the game is played (`world+0x88` 0, through the game's
    /// `+0x5c0`), after `set_state(1)`; nothing in any other state. Then the base teardown
    /// (`FUN_140a73fc0`), which lets no target go.
    pub(crate) const fn refund(self) -> Refund {
        match self.state {
            HousingState::Working => Refund::HousingCost,
            HousingState::New | HousingState::Done | HousingState::Failed => Refund::Nothing,
        }
    }
}

/// The Housing's `cost` back to its `owner`'s `wood` (its destructor `0x140ac20f0` and its stop
/// `0x140ac2910`): the wood plus the cost (`ADDSS`, the wood first) written straight, with no
/// escrow.
///
/// # Errors
///
/// An owner without wood.
pub(crate) fn give_back(owner: &mut Player, wood: AttributeId, cost: Amount) -> GenieResult<()> {
    let held = Attributes::get(owner, wood)?;
    owner.set_attribute(wood, held + cost)
}
