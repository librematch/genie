//! Training units and researching technologies: a building's production queue, the actions that
//! work it off, and a trained unit's spawn.
//!
//! This file holds the queue QUEUE and RESEARCH fill (`TRIBE_Building_Object` `+0xbe8` to `+0xc01`,
//! `+0xcd8`) and the action of its production list (`+0xbf8`) that works the first entry off: the
//! make action (`TRIBE_Action_Make_Obj`, update `0x140ad0490`) or the research's
//! (`TRIBE_Action_Make_Tech`). The building's update makes and runs that action after its combat
//! update (`FUN_1411e7dc0`, [`crate::production::spawn`]); QUEUE's building part is `FUN_1411cbc40`
//! ([`crate::commands`]); the cancels are `FUN_1411d66b0`, `FUN_1411d61e0` and `FUN_1411cff00`
//! ([`crate::players::game_status`]).
//!
//! The queue is a buffer of 0x30-byte slots (`+0xbe8`, room for `+0xbf0`), the first `+0xbf2`
//! of them its entries: a master or a technology and how many of it. An entry that empties goes:
//! the entries after it move down a slot, field by field (the serial `+0x20` stays where it was),
//! and the slot past the new last one keeps what it held. The slots past the count are never
//! cleared: a field a new entry does not write (a research's train time) keeps an old entry's
//! value, and the buffer's regrowth by ten copies them with the rest.

use crate::GenieError;
use crate::objects::ActionState;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObjectFlags;
use crate::objects::masters::{Master, TrainLocation};
use crate::players::Amount;
use crate::players::attribute::AttributeId;
use crate::technology::TechId;
use crate::world::Seconds;
use crate::world::WholeSeconds;
use crate::world::slots::CheckedIndex;
use genie_diff::Diff;

mod batch;
use genie_model::production::button;
mod cost_count;
mod costs;
use genie_model::production::hotkey;
mod make_obj_state;
mod make_tech_state;
mod queue_counts;
pub mod spawn;
mod unit_index;

pub use self::batch::Batch;
pub use self::button::{LocationButton, QueueButton};
pub use self::cost_count::CostCount;
pub use self::costs::{
    as_entry_costs, cost_test, costed, master_costs, pay, refund, refund_master, refund_research,
    refund_tech,
};
pub use self::hotkey::HotkeyId;
pub use self::make_obj_state::MakeObjState;
pub use self::make_tech_state::MakeTechState;
pub use self::queue_counts::{EntrySerial, QueueChanges, QueueLength, QueuedUnits};
pub use self::unit_index::UnitIndex;
use crate::world::NO_ATTRIBUTE;
use crate::world::layer::AttributeRoles;
use crate::world::layer::Roles;
use crate::world::layer::Slots;

/// One slot of a building's queue (`+0xbe8`, 0x30 bytes; the save's `mProductionQueue[i]`). A
/// slot the buffer's growth adds is zeroed (`calloc`), as [`Entry::default`] is.
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct Entry {
    /// `+0x0`, `mMasterUnitId`: the master it makes; â1 for a research.
    pub master: Option<MasterId>,
    /// `+0x2`, `mUnitCount`: how many.
    pub count: QueuedUnits,
    /// `+0x4`, `mTechId`: the technology it researches; â1 for a unit.
    pub tech: Option<TechId>,
    /// `+0x8`, `mCosts`: what each of its units or researches paid, three (attribute, amount)
    /// pairs (`FUN_1411c3190`), which a cancel gives back.
    pub costs: [(Option<AttributeId>, Amount); 3],
    /// `+0x20`: the building's serial (`+0xcd8`) when the entry came; not saved (a load numbers
    /// the slots it reads from 0).
    #[diff(unsaved)]
    pub serial: EntrySerial,
    /// `+0x28`, `mButtonLocationId`: the train button QUEUE named, â1 for none.
    pub button: QueueButton,
    /// `+0x2c`, `mTrainTime`: the unit's train time at the building and button
    /// ([`train_time`]); a research entry does not write it.
    pub train_time: WholeSeconds,
}

impl Default for Entry {
    /// A zeroed slot: master 0 (a master, as the game reads the field), no count, technology 0,
    /// costs of attribute 0 (food) and nothing, serial 0, button 0, train time 0.
    fn default() -> Self {
        Self {
            master: Some(MasterId::default()),
            count: QueuedUnits::ZERO,
            tech: Some(TechId::default()),
            // the zeroed field's attribute 0, food
            costs: [(Some(AttributeId::named(0)), Amount::ZERO); 3],
            serial: EntrySerial::FIRST,
            button: QueueButton::default(),
            train_time: WholeSeconds::ZERO,
        }
    }
}

/// A queue entry's costs before `FUN_1411c3190` fills them: no attribute, nothing (`0xffffffff`
/// in each pair's eight bytes).
pub const NO_COSTS: [(Option<AttributeId>, Amount); 3] = [(None, Amount::ZERO); 3];

/// How many slots the buffer grows by when it is full (`FUN_1411cbc40`).
pub const GROWTH: i16 = 10;

/// The units and researches a building holds queued at most (`+0xbf4`, `FUN_1411cbc40`).
pub const ROOM: QueuedUnits = QueuedUnits::of_data(15);

/// The queue of a building.
#[derive(Clone, Debug, PartialEq, Diff)]
pub struct Queue {
    /// `+0xbe8`, its length `+0xbf0`: the buffer's slots, the entries first.
    pub slots: Vec<Entry>,
    /// `+0xbf2`, `mProductionQueueCount`: how many slots are entries.
    pub count: QueueLength,
    /// `+0xbf4`, `mProductionQueueTotalUnits`: the entries' counts summed.
    pub total: QueuedUnits,
    /// `+0xbf6`, `mProductionQueueEnabled`: the building's update works the queue; a new
    /// building's constructor sets it, and nothing in the simulation clears it.
    pub enabled: bool,
    /// `+0xc00`: grows by one with every change of the entries (a byte, wrapping).
    /// Not saved.
    #[diff(unsaved)]
    pub changes: QueueChanges,
    /// `+0xc01`: the last update made nothing because the building's own top action is a make,
    /// a research or of types 0x7b to 0x7d (`FUN_141227a00`) for a player not of type 3.
    /// Not saved.
    #[diff(unsaved)]
    pub waiting: bool,
    /// `+0xcd8`: the serial the next entry takes (not saved; a load leaves it at the slots it
    /// read).
    #[diff(unsaved)]
    pub next_serial: EntrySerial,
}

impl Default for Queue {
    /// A new building's queue: no slots, enabled.
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            count: QueueLength::ZERO,
            total: QueuedUnits::ZERO,
            enabled: true,
            changes: QueueChanges::NONE,
            waiting: false,
            next_serial: EntrySerial::FIRST,
        }
    }
}

/// Two entries' costs are the same (`FUN_1411cbc40`'s join): each pair's attribute as an
/// integer and its amount as a float (`UCOMISS`, `JP`: a NaN differs).
#[must_use]
pub fn same_costs(
    a: &[(Option<AttributeId>, Amount); 3],
    b: &[(Option<AttributeId>, Amount); 3],
) -> bool {
    a.iter().zip(b).all(|(x, y)| x.0 == y.0 && x.1 == y.1)
}

impl Queue {
    /// The entries (the first `+0xbf2` slots).
    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        let n = self.count.slots().min(self.slots.len());
        self.slots.get(..n).unwrap_or_default()
    }

    /// The first entry, if any.
    #[must_use]
    pub fn first(&self) -> Option<&Entry> {
        self.entries().first()
    }

    /// The last entry, if any.
    #[must_use]
    pub fn last(&self) -> Option<&Entry> {
        self.entries().last()
    }

    /// `FUN_1411d66b0`'s walk: the entry of the `index`-th unit, counting each entry's units in
    /// turn; None for an empty queue or an index past the last unit.
    #[must_use]
    pub fn entry_of_unit(&self, index: UnitIndex) -> Option<usize> {
        let entries = self.entries();
        if entries.is_empty() {
            return None;
        }
        let (mut e, mut k) = (0usize, 0i32);
        for _ in 0..index.get() {
            k += 1;
            if i32::from(entries.get(e)?.count.get()) <= k {
                e += 1;
                k = 0;
                if entries.len() <= e {
                    return None;
                }
            }
        }
        Some(e)
    }

    /// An emptied entry `i` goes (its caller lowered `+0xbf2` first in the game; here this
    /// does): each later entry moves down a slot, its master, count, technology, costs and
    /// button, and its train time when `time` (`FUN_1411d61e0`'s research branch does not move
    /// it); the serials stay, and the slot of the old last entry keeps what it held.
    ///
    /// # Errors
    ///
    /// A slot the buffer does not hold (the count past the buffer).
    pub fn close(&mut self, i: usize, time: bool) -> Result<(), GenieError> {
        self.count = self.count.shrunk();
        let mut j = i;
        while (j as i32) < i32::from(self.count.get()) {
            let next = *self.slots.checked(j + 1)?;
            let slot = self.slots.checked_mut(j)?;
            slot.master = next.master;
            slot.count = next.count;
            slot.tech = next.tech;
            slot.costs = next.costs;
            slot.button = next.button;
            if time {
                slot.train_time = next.train_time;
            }
            j += 1;
        }
        Ok(())
    }

    /// `FUN_1411cbc40`'s growth of a full buffer (`+0xbf0` at most `+0xbf2`): ten zeroed slots
    /// more; each old slot keeps its master, count, technology, button, train time and serial,
    /// and takes its costs from `costs`, one per old slot (`FUN_1411c3190` filled them anew from
    /// (â1, 0) with the slot's technology and master, as they stand now: [`Queue::full`]).
    pub fn grow(&mut self, costs: &[[(Option<AttributeId>, Amount); 3]]) {
        let old = self.slots.len();
        let mut slots = vec![Entry::default(); old + GROWTH as usize];
        for ((new, slot), &costs) in slots.iter_mut().zip(&self.slots).zip(costs) {
            *new = Entry { costs, ..*slot };
        }
        self.slots = slots;
    }

    /// The buffer is full: a new entry grows it first (`+0xbf0` at most `+0xbf2`).
    #[must_use]
    pub fn full(&self) -> bool {
        self.slots.len() as i32 <= i32::from(self.count.get())
    }

    /// `FUN_1411cbc40`'s new entry in the slot after the last (the buffer grown first): its
    /// master, count, technology, button and serial (`+0xcd8`, which grows), and `costs`; its
    /// train time is the caller's. The count, the total and the change counter are the
    /// caller's too. Returns the slot.
    ///
    /// # Errors
    ///
    /// A buffer without the slot after the last (not grown).
    pub fn open(
        &mut self,
        master: Option<MasterId>,
        count: QueuedUnits,
        tech: Option<TechId>,
        button: QueueButton,
    ) -> Result<usize, GenieError> {
        let i = self.count.slots();
        let slot = self.slots.checked_mut(i)?;
        let serial = self.next_serial;
        self.next_serial = serial.next();
        slot.master = master;
        slot.count = count;
        slot.tech = tech;
        slot.button = button;
        slot.serial = serial;
        slot.costs = NO_COSTS;
        Ok(i)
    }

    /// `FUN_1411e7dc0`'s release of the first entry's unit or research once its action is done
    /// or failed: its count, the total and the change counter; an emptied first entry goes
    /// ([`Queue::close`], the train times moved).
    ///
    /// # Errors
    ///
    /// A count past the buffer.
    pub fn release_first(&mut self) -> Result<(), GenieError> {
        if self.count == QueueLength::ZERO {
            return Ok(());
        }
        let first = self.slots.checked_mut(0)?;
        first.count = first.count.minus(QueuedUnits::ONE);
        let emptied = first.count < QueuedUnits::ONE;
        self.total = self.total.minus(QueuedUnits::ONE);
        self.changes = self.changes.bumped();
        if emptied {
            self.close(0, true)?;
        }
        Ok(())
    }
}

/// `FUN_1411cbc40`'s train time for a unit of a master with train locations `locations`
/// queued at a building of master `building` with button `button` (and `FUN_140acf280`'s for a
/// master without flag 0x80): with button â1 the first location's time; else the location of
/// that building whose button is the button's low byte, else the first location's; 0 without
/// locations.
#[must_use]
pub fn train_time(
    locations: &[TrainLocation],
    building: MasterId,
    button: QueueButton,
) -> WholeSeconds {
    let first = locations.first().map_or(WholeSeconds::ZERO, |l| l.time);
    if button == QueueButton::NONE {
        return first;
    }
    locations
        .iter()
        .find(|l| l.location == Some(building) && button.names(l.button))
        .map_or(first, |l| l.time)
}

/// `TRIBE_Action_Make_Obj` (type 0x66, 0x80 bytes): the make action of a building's first
/// queue entry (the save's `tribe_action_make_obj`).
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct MakeObj {
    /// `+0x18`: its state.
    pub state: MakeObjState,
    /// `+0x60`: the queue paid for its unit (`FUN_140acb140`'s `param_5` 0), so its destructor
    /// gives nothing back; clear, the destructor refunds the master's paid costs
    /// (`FUN_141630770`).
    pub paid: bool,
    /// `+0x68`, `obj_id`: the master it makes.
    pub master: Option<MasterId>,
    /// `+0x6c`, `work_done`: the seconds of work done.
    pub progress: Seconds,
    /// `+0x70`, `unique_id` (â1 from the building's update).
    pub unique_id: i32,
    /// `+0x78`, `mButtonLocationId`.
    pub button: QueueButton,
    /// `+0x7c`, `mTrainTime`: the train time of the entry it was made for.
    pub train_time: WholeSeconds,
}

impl MakeObj {
    /// The building's update's make action (`FUN_140acb140` with the first entry's master, â1,
    /// 0, its train time and button): paid, nothing done, state 0 until the list starts it.
    #[must_use]
    pub const fn for_entry(entry: &Entry) -> Self {
        Self {
            state: MakeObjState::New,
            paid: true,
            master: entry.master,
            progress: Seconds::ZERO,
            unique_id: -1,
            button: entry.button,
            train_time: entry.train_time,
        }
    }

    /// `FUN_140acf280`: the seconds its unit takes. A master with flag 0x80 (`+0x654`) takes
    /// the entry's train time; any other its train location at the building of master
    /// `building` and the action's button ([`train_time`]); 0 for a master that is not a
    /// combat master (`TRIBE_Master_Combat_Object`, type 70 and up).
    #[must_use]
    pub fn time(&self, master: Option<&Master>, building: MasterId) -> WholeSeconds {
        // The first master type that is a `TRIBE_Master_Combat_Object` (the dynamic casts of
        // `FUN_1411cbc40` and `FUN_140acf280`).
        let Some(m) = master.filter(|m| MasterKind::COMBAT <= m.kind) else {
            return WholeSeconds::ZERO;
        };
        if m.object_flags.contains(ObjectFlags::OWN_TRAIN_TIME) {
            return self.train_time;
        }
        train_time(&m.train_locations, building, self.button)
    }
}

/// `TRIBE_Action_Make_Tech` (type 0x67, 0x78 bytes): the research of a building's first queue
/// entry (the save's `tribe_action_make_tech`).
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct MakeTech {
    /// `+0x18`: its state.
    pub state: MakeTechState,
    /// `+0x60`: `FUN_140ad0700`'s `param_5` flipped (the building's update passes 0): set, its
    /// destructor gives nothing back (`FUN_1413ccc00` runs only while it is clear).
    pub paid: bool,
    /// `+0x68`, `tech_id`.
    pub tech: TechId,
    /// `+0x6c`, `unique_id`: the entry's master (â1).
    pub master: i32,
    /// `+0x70`, `m_researchProgress`: the seconds of work done.
    pub progress: Seconds,
}

/// What a building's production list (`+0xbf8`) holds: the building's update makes at most one
/// action in it.
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub enum Making {
    /// A unit's make action.
    Unit(MakeObj),
    /// A research.
    Research(MakeTech),
}

impl Making {
    /// Its state (`+0x18`).
    #[must_use]
    pub const fn state(&self) -> ActionState {
        ActionState::new(match self {
            Self::Unit(m) => m.state.get(),
            Self::Research(r) => r.state.get(),
        })
    }
}

/// A building's queue, its production list's action and its lists of technologies.
#[derive(Clone, Debug, Default, PartialEq, Diff)]
pub struct Production {
    /// The queue.
    pub queue: Queue,
    /// The production list's action, if it has one.
    pub action: Option<Making>,
    /// `+0xd00`, `localDisabledTechs`: technologies the building may not research again; a
    /// research of the shared kind joins it (`FUN_1411e7dc0`), a cancel of a technology drops
    /// it (`FUN_1411c5250`).
    pub disabled_techs: Vec<TechId>,
    /// `+0xd18`, `localResearchedTechs`: the technologies of the shared kind this building
    /// researched (`FUN_1411e7dc0`).
    pub researched_techs: Vec<TechId>,
    /// `+0xd30`, `localAvailableUnits`: masters QUEUE takes at this building though they are
    /// not available to the player (`FUN_1411c8fd0`).
    pub available_units: Vec<MasterId>,
}

impl Production {
    /// `FUN_1411c5250`: technology `tech` leaves the building's disabled list, every time it
    /// is there (`std::remove` and `erase`, the order of the rest kept).
    pub fn enable_tech(&mut self, tech: TechId) {
        self.disabled_techs.retain(|&t| t != tech);
    }

    /// `FUN_1411e7dc0` after a research of the shared kind: the technology joins the
    /// building's researched list, and its disabled list, each unless it holds it; once the
    /// researched list holds it the disabled one is left alone.
    pub fn note_shared_research(&mut self, tech: TechId) {
        if self.researched_techs.contains(&tech) {
            return;
        }
        self.researched_techs.push(tech);
        if !self.disabled_techs.contains(&tech) {
            self.disabled_techs.push(tech);
        }
    }
}

/// The population a master's unit takes (`FUN_14163b0b0`): minus its storage of attribute 4,
/// the population headroom, over the master's three storage entries.
#[must_use]
pub fn population(storage: &[(Option<AttributeId>, Amount, u8)], roles: &Roles) -> Amount {
    storage
        .iter()
        // The population headroom (attribute 4), the population (11), the population cap (32), the
        // units queued (80) and the units in production (81).
        // The population (attribute 11).
        .filter(|&&(kind, _, _)| {
            roles
                .of_some_attribute(kind)
                .contains(AttributeRoles::HEADROOM)
        })
        .fold(Amount::ZERO, |pop, &(_, amount, _)| pop - amount)
}

/// `FUN_1411e7dc0`'s test before it makes a unit's make action: the population with the units in
/// production and this one stays within the cap (`COMISS`, `JBE`: a NaN passes), and the
/// headroom covers them. A building that fails it waits (housed).
pub fn housed(attributes: &[Amount], pop: Amount, slots: Slots) -> Result<bool, GenieError> {
    let a = |i: AttributeId| attributes.get(i.index()).copied().ok_or(NO_ATTRIBUTE);
    let in_production = a(slots.attribute.in_production)?;
    Ok(
        a(slots.attribute.population_cap)? < a(slots.attribute.population)? + in_production + pop
            || (a(slots.attribute.headroom)? - in_production) - pop < Amount::ZERO,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gathering::QueueCount;
    use crate::objects::masters::CostAmount;
    use crate::objects::masters::CostSlot;
    use crate::objects::masters::MasterKind;
    use crate::objects::masters::Masters;
    use crate::players::attribute::AttributeId;
    #[cfg(test)]
    use crate::technology::Attributes;
    use crate::technology::CostPaid;
    use crate::technology::ResearchState;
    #[cfg(test)]
    use crate::test_ids::amounts;
    use crate::test_ids::costs;
    use crate::test_ids::master_id;
    use crate::test_ids::opt_master;
    use crate::test_ids::tech_id;
    use crate::testing::attribute;
    use crate::testing::ids;
    use crate::testing::roles;
    use crate::testing::slots;

    fn unit(master: MasterId, count: i16, food: f32) -> Entry {
        Entry {
            master: Some(master),
            count: QueuedUnits::of_data(count),
            tech: None,
            costs: [
                (Some(attribute::FOOD), Amount::new(food)),
                (None, Amount::ZERO),
                (None, Amount::ZERO),
            ],
            serial: EntrySerial::new(0),
            button: QueueButton::NONE,
            train_time: WholeSeconds::of_data(25),
        }
    }

    /// A zeroed slot (the buffer's growth, `calloc`) holds master 0 and technology 0, as the
    /// game's save writes a slot past the entries: not the â1 of a research's master.
    #[test]
    fn zeroed_slot_holds_master_0() {
        let e = Entry::default();
        assert_eq!(
            (
                MasterId::field(e.master),
                e.count.get(),
                TechId::field(e.tech)
            ),
            (0, 0, 0)
        );
        assert_eq!(
            (e.costs, e.button, e.train_time),
            (
                [(Some(attribute::FOOD), Amount::ZERO); 3],
                QueueButton::default(),
                WholeSeconds::ZERO
            )
        );
    }

    fn queue(entries: &[Entry]) -> Queue {
        let mut slots = vec![Entry::default(); 10];
        slots[..entries.len()].copy_from_slice(entries);
        for (k, s) in slots.iter_mut().enumerate() {
            s.serial = EntrySerial::new(k as i64);
        }
        Queue {
            slots,
            count: QueueLength::of_data(entries.len() as i16),
            total: entries.iter().map(|e| e.count).sum(),
            next_serial: EntrySerial::new(10),
            ..Queue::default()
        }
    }

    /// `FUN_1411d66b0` walks the units entry by entry: index 4 of [3 archers, 2 villagers] is
    /// the second villager, index 5 none.
    #[test]
    fn unit_index_walks_the_entries_counts() {
        let q = queue(&[
            unit(master_id(4), 3, 25.0),
            unit(ids::VILLAGER_MALE, 2, 50.0),
        ]);
        let at: Vec<_> = (0..6)
            .map(|i| q.entry_of_unit(UnitIndex::of_data(i)))
            .collect();
        assert_eq!(at, [Some(0), Some(0), Some(0), Some(1), Some(1), None]);
        assert_eq!(Queue::default().entry_of_unit(UnitIndex::of_data(0)), None);
    }

    /// An emptied entry goes: the later ones move down field by field, the serials stay, the
    /// old last slot keeps its copy; without `time` the train times stay as they were.
    #[test]
    fn closing_an_entry_moves_the_later_ones_down_but_not_their_serials() {
        let mut q = queue(&[
            unit(master_id(4), 0, 25.0),
            unit(ids::VILLAGER_MALE, 2, 50.0),
            unit(master_id(5), 1, 30.0),
        ]);
        q.slots[2].train_time = WholeSeconds::of_data(40);
        q.close(0, true).unwrap();
        assert_eq!(q.count.get(), 2);
        assert_eq!(
            q.slots[..3]
                .iter()
                .map(|s| (s.master, s.count.get(), s.serial.get(), s.train_time.get()))
                .collect::<Vec<_>>(),
            [
                (Some(ids::VILLAGER_MALE), 2, 0, 25),
                (Some(master_id(5)), 1, 1, 40),
                (Some(master_id(5)), 1, 2, 40)
            ]
        );
        // a research's cancel (FUN_1411d61e0) leaves the train times in their slots
        let mut q = queue(&[
            unit(master_id(4), 0, 25.0),
            unit(ids::VILLAGER_MALE, 2, 50.0),
        ]);
        q.slots[0].train_time = WholeSeconds::of_data(7);
        q.close(0, false).unwrap();
        assert_eq!(
            (q.slots[0].master, q.slots[0].train_time),
            (Some(ids::VILLAGER_MALE), WholeSeconds::of_data(7))
        );
    }

    /// The regrowth by ten keeps every old slot but its costs, which it fills anew.
    #[test]
    fn full_buffer_grows_by_ten_and_fills_the_old_costs_anew() {
        let mut q = queue(&[unit(ids::VILLAGER_MALE, 1, 50.0)]);
        q.slots[3] = unit(ids::ID_7, 0, 9.0);
        assert!(!q.full());
        // each old slot's own costs: its place's attribute, its place as the amount
        let refill: Vec<_> = (0..q.slots.len())
            .map(|i| {
                [
                    (AttributeId::of_index(i), Amount::new(i as f32)),
                    NO_COSTS[1],
                    NO_COSTS[2],
                ]
            })
            .collect();
        q.grow(&refill);
        assert_eq!(q.slots.len(), 20);
        assert_eq!(q.slots[0].costs[0], (Some(attribute::FOOD), Amount::ZERO));
        assert_eq!(
            (q.slots[3].master, q.slots[3].costs[0]),
            (Some(ids::ID_7), (Some(attribute::GOLD), Amount::new(3.0)))
        );
        assert_eq!(q.slots[15], Entry::default());
        // a new entry takes the next serial and no costs
        let i = q
            .open(
                Some(master_id(5)),
                QueuedUnits::of_data(2),
                None,
                QueueButton::of_data(3),
            )
            .unwrap();
        assert_eq!(
            (i, q.slots[1].serial.get(), q.slots[1].costs),
            (1, 10, NO_COSTS)
        );
        assert_eq!(q.next_serial, EntrySerial::new(11));
    }

    #[test]
    fn the_first_entry_lets_one_unit_go_and_then_itself() {
        let mut q = queue(&[
            unit(ids::VILLAGER_MALE, 2, 50.0),
            unit(master_id(4), 1, 25.0),
        ]);
        q.release_first().unwrap();
        assert_eq!(
            (
                q.count.get(),
                q.total.get(),
                q.changes.get(),
                q.slots[0].count.get()
            ),
            (2, 2, 1, 1)
        );
        q.release_first().unwrap();
        assert_eq!(
            (
                q.count.get(),
                q.total.get(),
                q.changes.get(),
                q.slots[0].master
            ),
            (1, 1, 2, Some(master_id(4)))
        );
    }

    /// `FUN_1411cbc40`'s join compares the amounts as floats: a NaN never joins.
    #[test]
    fn costs_join_only_when_equal_and_not_nan() {
        let a = [
            (Some(attribute::FOOD), Amount::new(50.0)),
            (None, Amount::ZERO),
            (None, Amount::ZERO),
        ];
        assert!(same_costs(&a, &a));
        assert!(!same_costs(
            &a,
            &[
                (Some(attribute::WOOD), Amount::new(50.0)),
                (None, Amount::ZERO),
                (None, Amount::ZERO)
            ]
        ));
        let nan = [
            (Some(attribute::FOOD), Amount::new(f32::NAN)),
            (None, Amount::ZERO),
            (None, Amount::ZERO),
        ];
        assert!(!same_costs(&nan, &nan));
    }

    /// A villager at the town centre (109) with button 1 trains in 25 seconds there; another
    /// building or button takes the first location's time; no locations, 0.
    #[test]
    fn train_time_is_the_building_and_button_else_the_first() {
        let at = |location, time, button| TrainLocation {
            location,
            time,
            button,
            hotkey: HotkeyId::of_data(0),
        };
        let locations = [
            at(
                Some(ids::CASTLE),
                WholeSeconds::of_data(30),
                LocationButton::of_data(2),
            ),
            at(
                Some(ids::TOWN_CENTRE),
                WholeSeconds::of_data(25),
                LocationButton::of_data(1),
            ),
        ];
        assert_eq!(
            train_time(&locations, ids::TOWN_CENTRE, QueueButton::of_data(1)),
            WholeSeconds::of_data(25)
        );
        assert_eq!(
            train_time(&locations, ids::TOWN_CENTRE, QueueButton::of_data(2)),
            WholeSeconds::of_data(30)
        );
        assert_eq!(
            train_time(&locations, ids::TOWN_CENTRE, QueueButton::NONE),
            WholeSeconds::of_data(30)
        );
        assert_eq!(
            train_time(&locations, ids::TOWN_CENTRE, QueueButton::of_data(0x101)),
            WholeSeconds::of_data(25)
        );
        assert_eq!(
            train_time(&[], ids::TOWN_CENTRE, QueueButton::of_data(1)),
            WholeSeconds::ZERO
        );
        let mut m = Master {
            kind: MasterKind::COMBAT,
            train_locations: locations.to_vec(),
            ..Master::default()
        };
        let mut make = MakeObj::for_entry(&Entry {
            master: Some(ids::VILLAGER_MALE),
            button: QueueButton::of_data(1),
            train_time: WholeSeconds::of_data(99),
            ..Entry::default()
        });
        assert_eq!(
            make.time(Some(&m), ids::TOWN_CENTRE),
            WholeSeconds::of_data(25)
        );
        m.object_flags = ObjectFlags::OWN_TRAIN_TIME;
        assert_eq!(
            make.time(Some(&m), ids::TOWN_CENTRE),
            WholeSeconds::of_data(99)
        );
        m.kind = MasterKind::MISSILE;
        assert_eq!(make.time(Some(&m), ids::TOWN_CENTRE), WholeSeconds::ZERO);
        make.button = QueueButton::NONE;
        assert_eq!(
            (make.time(None, ids::TOWN_CENTRE), make.paid, make.unique_id),
            (WholeSeconds::ZERO, true, -1)
        );
    }

    /// A villager stores â1 headroom, +1 population and +1 of attribute 19.
    #[test]
    fn villager_takes_one_population() {
        assert_eq!(
            population(
                &[
                    (Some(attribute::HEADROOM), Amount::new(-1.0), 2),
                    (Some(attribute::POPULATION), Amount::new(1.0), 2),
                    (Some(attribute::UNITS_OWNED), Amount::new(1.0), 1)
                ],
                &roles()
            ),
            Amount::new(1.0)
        );
        let mut a = vec![Amount::ZERO; 82];
        (a[4], a[11], a[32], a[81]) = (
            Amount::new(6.0),
            Amount::new(4.0),
            Amount::new(5.0),
            Amount::ZERO,
        );
        assert!(!housed(&a, Amount::new(1.0), slots()).unwrap());
        a[81] = Amount::new(1.0);
        assert!(housed(&a, Amount::new(1.0), slots()).unwrap());
        a[32] = Amount::new(f32::NAN);
        a[4] = Amount::new(10.0);
        assert!(!housed(&a, Amount::new(1.0), slots()).unwrap());
    }

    struct Held(Vec<Amount>);

    impl Attributes for Held {
        fn get(&self, i: AttributeId) -> Result<Amount, GenieError> {
            Ok(self.0[i.index()])
        }
        fn set(&mut self, i: AttributeId, v: Amount) -> Result<(), GenieError> {
            self.0[i.index()] = v;
            Ok(())
        }
    }

    /// `FUN_1413ccc00` with `param_3` 1: a technology in state 2 or 4 gives back each paid cost
    /// entry's amount plus its adjustment, scaled by the research cost modifier (attribute 85)
    /// above 0 and truncated to 16 bits (not an age's), and is in state 1 with no work done; in
    /// another state nothing happens.
    #[test]
    fn cancelled_research_gives_back_its_paid_costs() {
        use crate::technology::{Cost, TechDef, TechState};
        let def = TechDef {
            costs: [
                Cost {
                    attribute: Some(attribute::FOOD),
                    amount: CostAmount::of_data(100),
                    paid: CostPaid::PLAYER,
                },
                Cost {
                    attribute: Some(attribute::WOOD),
                    amount: CostAmount::of_data(50),
                    paid: CostPaid::UNPAID,
                },
                Cost {
                    attribute: None,
                    amount: CostAmount::of_data(5),
                    paid: CostPaid::PLAYER,
                },
            ],
            ..TechDef::default()
        };
        let state = |state| TechState {
            state,
            research_done: Seconds::new(7.5),
            cost_adj: [CostAmount::of_data(-10), CostAmount::ZERO, CostAmount::ZERO],
            ..TechState::default()
        };
        let refunded = |state: &mut TechState, tech, modifier| {
            let mut held = Held(vec![Amount::ZERO; 90]);
            held.0[attribute::RESEARCH_COST_MODIFIER.index()] = modifier;
            let r = refund_tech(state, &def, tech, &mut held, slots());
            (r, held.0[0], held.0[1])
        };
        let mut s = state(ResearchState::Researching);
        assert_eq!(
            refunded(&mut s, tech_id(22), Amount::ZERO),
            (Ok(true), Amount::new(90.0), Amount::ZERO)
        );
        assert_eq!(
            (s.state, s.research_done),
            (ResearchState::Available, Seconds::ZERO)
        );
        let mut s = state(ResearchState::Queued);
        assert_eq!(
            refunded(&mut s, tech_id(22), Amount::new(0.75)),
            (Ok(true), Amount::new(67.0), Amount::ZERO)
        );
        let mut s = state(ResearchState::Queued);
        assert_eq!(
            refunded(&mut s, tech_id(101), Amount::new(0.75)),
            (Ok(true), Amount::new(90.0), Amount::ZERO)
        );
        // the scaled amount wraps at 16 bits: 40000 is -25536
        let mut s = TechState {
            cost_adj: [
                CostAmount::of_data(19_900),
                CostAmount::ZERO,
                CostAmount::ZERO,
            ],
            ..state(ResearchState::Researching)
        };
        assert_eq!(
            refunded(&mut s, tech_id(22), Amount::new(2.0)),
            (Ok(true), Amount::new(-25_536.0), Amount::ZERO)
        );
        let mut s = state(ResearchState::Researched);
        assert_eq!(
            refunded(&mut s, tech_id(22), Amount::ZERO),
            (Ok(false), Amount::ZERO, Amount::ZERO)
        );
        assert_eq!(
            (s.state, s.research_done),
            (ResearchState::Researched, Seconds::new(7.5))
        );
    }

    /// `FUN_141630530` gives back what the entry paid of each paid cost (the last pair naming
    /// it), not what the master costs now; an unpaid cost and a master below type 70 give
    /// nothing.
    #[test]
    fn refund_gives_back_what_the_entry_paid() {
        let m = Master {
            kind: MasterKind::COMBAT,
            cost: costs(&[
                (attribute::FOOD, 60, true),
                (attribute::GOLD, 20, true),
                (attribute::HEADROOM, 1, false),
            ]),
            ..Master::default()
        };
        let mut held = Held(vec![Amount::new(1.0); 5]);
        refund(
            &m,
            &[
                (Some(attribute::FOOD), Amount::new(50.0)),
                (Some(attribute::FOOD), Amount::new(45.0)),
                (Some(attribute::HEADROOM), Amount::new(1.0)),
            ],
            &mut held,
        )
        .unwrap();
        assert_eq!(held.0, amounts([46.0, 1.0, 1.0, 1.0, 1.0]));
        let low = Master {
            kind: MasterKind::MISSILE,
            ..m.clone()
        };
        refund(
            &low,
            &[
                (Some(attribute::FOOD), Amount::new(50.0)),
                (None, Amount::ZERO),
                (None, Amount::ZERO),
            ],
            &mut held,
        )
        .unwrap();
        assert_eq!(held.0[0], Amount::new(46.0));
        refund_research(
            &[
                (Some(attribute::GOLD), Amount::new(50.0)),
                (None, Amount::ZERO),
                (Some(attribute::FOOD), Amount::new(0.5)),
            ],
            QueuedUnits::of_data(3),
            &mut held,
        )
        .unwrap();
        assert_eq!(
            (held.0[3], held.0[0]),
            (Amount::new(151.0), Amount::new(47.5))
        );
        refund_master(&m, QueueCount::ONE, &mut held).unwrap();
        assert_eq!(
            (held.0[0], held.0[3], held.0[4]),
            (Amount::new(107.5), Amount::new(171.0), Amount::new(1.0))
        );
    }

    fn villager(cost: &[(AttributeId, i16, bool)]) -> Master {
        Master {
            id: ids::VILLAGER_MALE,
            kind: MasterKind::COMBAT,
            cost: costs(cost),
            ..Master::default()
        }
    }

    /// 100 food, headroom 5, population 4, cap 10, nothing queued.
    fn held() -> Held {
        let mut a = vec![Amount::ZERO; 82];
        (a[0], a[4], a[11], a[32]) = (
            Amount::new(100.0),
            Amount::new(5.0),
            Amount::new(4.0),
            Amount::new(10.0),
        );
        Held(a)
    }

    /// The game's villager costs food, an empty slot and the population (the `.dat`'s
    /// `resource_costs`): a queue entry's costs keep the empty slot in its place
    /// (MarvinBlue/426048731's 9149 at update 25: (0, 50), (â1, 0), (4, 1)).
    #[test]
    fn queue_entry_keeps_the_empty_cost_slot_in_its_place() {
        let mut m = villager(&[]);
        m.cost = [
            CostSlot::new(attribute::FOOD, CostAmount::of_data(50), true),
            CostSlot::default(),
            CostSlot::new(attribute::HEADROOM, CostAmount::of_data(1), false),
        ];
        let ms = Masters::new([m]);
        assert_eq!(
            master_costs(&ms, ids::VILLAGER_MALE, &held(), &roles(), slots())
                .unwrap()
                .map(|(a, n)| (a, n.get())),
            [
                (Some(attribute::FOOD), 50),
                (None, 0),
                (Some(attribute::HEADROOM), 1)
            ]
        );
    }

    /// `FUN_141618db0`: two villagers' 100 food are held, three's are not; the population's
    /// entry asks the queued units plus the population plus the need to stay within the cap
    /// or the headroom's sum, the smaller, only with `param_5` 0.
    #[test]
    fn the_cost_test_reads_each_entry_and_the_population_only_when_asked() {
        let ms = Masters::new([villager(&[
            (attribute::FOOD, 50, true),
            (attribute::HEADROOM, 1, false),
        ])]);
        let mut h = held();
        assert!(
            cost_test(
                &ms,
                ids::VILLAGER_MALE,
                CostCount::new(2.0),
                true,
                &h,
                &roles(),
                slots()
            )
            .unwrap()
        );
        assert!(
            !cost_test(
                &ms,
                ids::VILLAGER_MALE,
                CostCount::new(3.0),
                true,
                &h,
                &roles(),
                slots()
            )
            .unwrap()
        );
        // 4 queued: 4 + 4 + 1 is the headroom's 9; one more queued is not
        h.0[80] = Amount::new(4.0);
        assert!(
            cost_test(
                &ms,
                ids::VILLAGER_MALE,
                CostCount::ONE,
                true,
                &h,
                &roles(),
                slots()
            )
            .unwrap()
        );
        h.0[80] = Amount::new(5.0);
        assert!(
            !cost_test(
                &ms,
                ids::VILLAGER_MALE,
                CostCount::ONE,
                true,
                &h,
                &roles(),
                slots()
            )
            .unwrap()
        );
        assert!(
            cost_test(
                &ms,
                ids::VILLAGER_MALE,
                CostCount::ONE,
                false,
                &h,
                &roles(),
                slots()
            )
            .unwrap()
        );
        // a population at the cap
        let mut h = held();
        h.0[32] = Amount::new(4.0);
        assert!(
            !cost_test(
                &ms,
                ids::VILLAGER_MALE,
                CostCount::ONE,
                true,
                &h,
                &roles(),
                slots()
            )
            .unwrap()
        );
        // a NaN need passes, a master the player lacks or below type 70 fails
        assert!(
            cost_test(
                &ms,
                ids::VILLAGER_MALE,
                CostCount::new(f32::NAN),
                false,
                &h,
                &roles(),
                slots()
            )
            .unwrap()
        );
        assert!(
            !cost_test(
                &ms,
                ids::MARKET,
                CostCount::ONE,
                false,
                &h,
                &roles(),
                slots()
            )
            .unwrap()
        );
        let low = Masters::new([Master {
            kind: MasterKind::MISSILE,
            ..villager(&[])
        }]);
        assert!(
            !cost_test(
                &low,
                ids::VILLAGER_MALE,
                CostCount::ONE,
                false,
                &h,
                &roles(),
                slots()
            )
            .unwrap()
        );
    }

    /// `FUN_14162fb60` pays the paid entries times the count in one subtraction.
    #[test]
    fn payment_takes_the_paid_entries_times_the_count() {
        let ms = Masters::new([villager(&[
            (attribute::FOOD, 50, true),
            (attribute::HEADROOM, 1, false),
            (attribute::WOOD, 7, true),
        ])]);
        let mut h = held();
        h.0[1] = Amount::new(30.0);
        assert!(
            pay(
                &ms,
                ids::VILLAGER_MALE,
                CostCount::new(2.0),
                &mut h,
                &roles(),
                slots()
            )
            .unwrap()
        );
        assert_eq!(
            (h.0[0], h.0[1], h.0[4]),
            (Amount::ZERO, Amount::new(16.0), Amount::new(5.0))
        );
        assert!(
            !pay(
                &ms,
                ids::VILLAGER_MALE,
                CostCount::ONE,
                &mut h,
                &roles(),
                slots()
            )
            .unwrap()
        );
        assert_eq!(h.0[0], Amount::ZERO);
    }

    /// An annex (master 2, head 3 whose stack unit is 4) is tested by its stack unit's paid
    /// costs, while `FUN_14162ef60` lists its own: what it lacks of food and stone goes to the
    /// third entry as gold; a gold entry first takes it on its amount; an amount below 1 lists
    /// no attribute.
    #[test]
    fn listed_costs_move_what_the_player_lacks_to_gold() {
        use crate::objects::masters::BuildingMaster;
        let building = |id: i16, head: i16, stack: i16, cost: &[(AttributeId, i16, bool)]| Master {
            id: master_id(id),
            kind: MasterKind::BUILDING,
            cost: costs(cost),
            building: Some(BuildingMaster {
                head_unit: opt_master(head),
                stack_unit: opt_master(stack),
                ..BuildingMaster::default()
            }),
            ..Master::default()
        };
        let ms = Masters::new([
            building(
                2,
                3,
                -1,
                &[(attribute::FOOD, 50, false), (attribute::STONE, 30, false)],
            ),
            building(3, -1, 4, &[]),
            building(
                4,
                -1,
                -1,
                &[(attribute::WOOD, 10, true), (attribute::FOOD, 500, false)],
            ),
        ]);
        let mut h = held();
        (h.0[0], h.0[1], h.0[2]) = (Amount::new(20.0), Amount::new(100.0), Amount::new(5.5));
        assert_eq!(
            costed(&ms, ms.get(master_id(2)).unwrap()).0.id,
            master_id(4)
        );
        assert_eq!(
            master_costs(&ms, master_id(2), &h, &roles(), slots())
                .unwrap()
                .map(|(a, n)| (a, n.get())),
            [
                (Some(attribute::FOOD), 20),
                (Some(attribute::STONE), 5),
                (Some(attribute::GOLD), 55)
            ]
        );
        // a gold entry first takes the difference on what it holds
        let ms = Masters::new([
            building(
                2,
                3,
                -1,
                &[
                    (attribute::GOLD, 50, false),
                    (attribute::FOOD, 30, false),
                    (attribute::WOOD, 0, false),
                ],
            ),
            building(3, -1, 4, &[]),
            building(4, -1, -1, &[(attribute::WOOD, 10, true)]),
        ]);
        (h.0[3], h.0[0]) = (Amount::new(20.0), Amount::new(10.0));
        assert_eq!(
            master_costs(&ms, master_id(2), &h, &roles(), slots())
                .unwrap()
                .map(|(a, n)| (a, n.get())),
            [
                (Some(attribute::GOLD), 70),
                (Some(attribute::FOOD), 10),
                (None, 0)
            ]
        );
        // a unit the player cannot pay for one more of moves nothing
        let ms = Masters::new([villager(&[(attribute::GOLD, 50, true)])]);
        assert_eq!(
            master_costs(&ms, ids::VILLAGER_MALE, &h, &roles(), slots())
                .unwrap()
                .map(|(a, n)| (a, n.get())),
            [(Some(attribute::GOLD), 50), (None, 0), (None, 0)]
        );
        assert_eq!(
            master_costs(&ms, ids::ID_7, &h, &roles(), slots())
                .unwrap()
                .map(|(a, n)| (a, n.get())),
            [(None, 0); 3]
        );
        assert_eq!(
            as_entry_costs([
                (Some(attribute::GOLD), CostAmount::of_data(50)),
                (None, CostAmount::of_data(0)),
                (None, CostAmount::of_data(-2))
            ])
            .map(|(a, n)| (a, n.get())),
            [(Some(attribute::GOLD), 50.0), (None, 0.0), (None, -2.0)]
        );
    }

    #[test]
    fn shared_research_joins_both_lists_once_and_cancel_drops_it() {
        let mut p = Production::default();
        p.note_shared_research(tech_id(7));
        p.note_shared_research(tech_id(7));
        assert_eq!(
            (p.researched_techs.as_slice(), p.disabled_techs.as_slice()),
            (&[tech_id(7)][..], &[tech_id(7)][..])
        );
        p.disabled_techs.extend([tech_id(3), tech_id(7)]);
        p.enable_tech(tech_id(7));
        assert_eq!(p.disabled_techs, [tech_id(3)]);
        assert!(p.queue.enabled);
    }
}
