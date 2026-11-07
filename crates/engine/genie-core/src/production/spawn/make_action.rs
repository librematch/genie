//! A building's production list and its make action (`TRIBE_Action_Make_Obj`).

use crate::gathering::QueueCount;
use crate::math::fmath::below_or_unordered;
use crate::objects::ListIndex;
use crate::objects::ObjectState;
use crate::objects::UpdateCode;
use crate::objects::masters::ActionKind;
use crate::objects::masters::player_master;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::production::Entry;
use crate::production::ProductionStart;
use crate::production::spawn::new_unit::SpawnOutcome;
use crate::production::{
    MakeObj, MakeObjState, MakeTech, MakeTechState, Making, housed, population,
};
use crate::production::{NO_COSTS, QueueLength, refund_master};
use crate::technology::ResearchState;
use crate::technology::{TechKind, factors};
use crate::unit_ai::AiAction;
use crate::unit_ai::Notify;
use crate::unit_ai::NotifyKind;
use crate::world::PlayState;
use crate::world::Seconds;
use crate::world::World;
use crate::world::slots::CheckedIndexExt;
use crate::world::slots::PlayerSlotsExt;
use crate::{GenieError, GenieResult};

/// A queue that is enabled and counts an entry, but holds none.
const EMPTY_QUEUE: GenieError =
    GenieError::unreachable("a production queue that counts an entry but holds none");

impl World {
    /// `TRIBE_Building_Object::update`'s part after its combat update (`0x1411ca3e2`) for the
    /// building at `list_index` of player `player_id`: past state 2 (dying or dead) it cancels its
    /// whole queue in every update ([`World::cancel_queue`], `FUN_1411cff00`: the units and
    /// researches given back, the change counter counting on); in state 2 or below its production
    /// step runs ([`World::produce`]). Its second action list (`+0xba0`: cleared first past state
    /// 2, else updated by `FUN_140acafd0` into `+0xb9d`) holds nothing the world models.
    ///
    /// # Errors
    ///
    /// A building the world does not hold, and the cancel's and the production's.
    pub(crate) fn building_production(
        &mut self,
        player_id: PlayerId,
        list_index: ListIndex,
        dt_seconds: Seconds,
    ) -> GenieResult<()> {
        if ObjectState::Alive < self.players.live(player_id, list_index)?.life.state {
            return self.cancel_queue(player_id, list_index);
        }
        self.produce(player_id, list_index, dt_seconds)
    }

    /// The building update's production step (`FUN_1411e7dc0`, after its combat update) for the
    /// building at `list_index` of player `player_id`. A disabled queue (`+0xbf6`) or an empty one
    /// does nothing. For a player not of type 3 a top action of the building's own
    /// (`FUN_141227a00`) that is a make, a research or of types 0x7b to 0x7d sets `+0xc01` and
    /// nothing more happens; else `+0xc01` clears. Without an action in its production list the
    /// first entry's comes: for a unit the population test first ([`housed`]): a housed building
    /// waits (its reason `+0xc02`, 0x20 or 4, and the events 0x7c and 0x7d are the interface's), a
    /// computer player's cancels its queue ([`World::cancel_queue`]); then the make action
    /// ([`MakeObj::for_entry`]; its constructor `FUN_140acb140` gives the building's unit AI the
    /// action 619, `FUN_140c64b50`; the headroom's sound and event 0xa9 are the interface's), or
    /// the research's ([`World::make_research`]). The list's add (`FUN_140a977e0`) starts it for a
    /// building in a state below 3 (slot 0x28): a make action of a master the player has
    /// (`FUN_140acf0a0`) adds its population to attribute 81 and takes state 6, one of a master it
    /// lacks stays in state 0 ("Canceling Train #1a"); a building in state 3 or more deletes it at
    /// once ([`World::end_production_action`]). Then the list's update (`FUN_140a992b0`, the first
    /// action's slot 0x30; an empty list answers done): the make action's ([`World::make_step`]) or
    /// the research's ([`World::research_step`]). Done (1) or failed (2, 3): after a research of
    /// the shared kind the technology joins the building's lists
    /// ([`crate::production::Production::note_shared_research`]); the action goes
    /// ([`World::end_production_action`]) and the first entry lets one go: a unit's population
    /// leaves attribute 80 ([`crate::production::Queue::release_first`]).
    pub(crate) fn produce(
        &mut self,
        player_id: PlayerId,
        list_index: ListIndex,
        dt_seconds: Seconds,
    ) -> GenieResult<()> {
        let unit = &self.players.live(player_id, list_index)?;
        let Some(production) = self.parts.production.get(unit.id) else {
            return Ok(());
        };
        if !production.queue.enabled || production.queue.count == QueueLength::ZERO {
            return Ok(());
        }
        // the first entry's action starts when the list has none
        let unstarted = production
            .action
            .is_none()
            .then(|| production.queue.first().copied());
        let busy = !self.players.player(player_id)?.kind.is_computer()
            && self
                .parts
                .actions_of(unit.id)
                .top_action_kind()
                .is_some_and(|action_kind| {
                    matches!(
                        action_kind,
                        ActionKind::MAKE_OBJECT
                            | ActionKind::MAKE_TECH
                            | ActionKind::ACTION_123
                            | ActionKind::PACK
                            | ActionKind::UNPACK
                    )
                });
        if let Some(production) = self
            .parts
            .production
            .get_mut(self.players.live(player_id, list_index)?.id)
        {
            production.queue.waiting = busy;
        }
        if busy {
            return Ok(());
        }
        if let Some(first) = unstarted
            // the count says an entry, and the load and the queue's writers keep it within the
            // slots (`Queue::entries`)
            && self.begin_production(player_id, list_index, first.ok_or(EMPTY_QUEUE)?)? == ProductionStart::Stops
        {
            return Ok(());
        }
        let code = self.update_production_action(player_id, list_index, dt_seconds)?;
        if !matches!(code.get(), 1..=3) {
            return Ok(());
        }
        if code == UpdateCode::Done {
            self.note_shared_research(player_id, list_index)?;
        }
        self.end_production_action(player_id, list_index)?;
        self.release_first_entry(player_id, list_index)
    }

    /// After its action ends the first queue entry of the building at `list_index` of player
    /// `player_id` lets one go ([`World::produce`]): a unit's population leaves the queued
    /// attribute (80, the slot `queued`), then the entry
    /// ([`crate::production::Queue::release_first`]).
    fn release_first_entry(
        &mut self,
        player_id: PlayerId,
        list_index: ListIndex,
    ) -> GenieResult<()> {
        let Some(first) = self
            .parts
            .production
            .get(self.players.live(player_id, list_index)?.id)
            .and_then(|production| production.queue.first().copied())
        else {
            return Ok(());
        };
        if first.tech.is_none() {
            let pop = first
                .master
                .and_then(|master_id| player_master(&self.model.masters, player_id, master_id))
                .map_or(Amount::ZERO, |master| {
                    population(&master.storage, &self.model.roles)
                });
            self.players
                .player_mut(player_id)?
                .add_attribute(self.model.slots.attribute.queued, -pop);
        }
        if let Some(production) = self
            .parts
            .production
            .get_mut(self.players.live(player_id, list_index)?.id)
        {
            production.queue.release_first()?;
        }
        Ok(())
    }

    /// The first queue entry `first`'s action starts, the list holding none ([`World::produce`]):
    /// a unit waits housed (a computer player's entry is cancelled), else it starts. Stops when
    /// the production stops for this update.
    fn begin_production(
        &mut self,
        player_id: PlayerId,
        list_index: ListIndex,
        first: Entry,
    ) -> GenieResult<ProductionStart> {
        let making = if first.tech.is_none() {
            let pop = first
                .master
                .and_then(|master_id| player_master(&self.model.masters, player_id, master_id))
                .map_or(Amount::ZERO, |master| {
                    population(&master.storage, &self.model.roles)
                });
            if housed(
                &self.players.player(player_id)?.attributes,
                pop,
                self.model.slots,
            )? {
                if self.players.player(player_id)?.kind.is_computer() {
                    self.cancel_queue(player_id, list_index)?;
                    return Ok(ProductionStart::Stops);
                }
                return Ok(ProductionStart::Stops);
            }
            if let Some(ai) = self
                .parts
                .unit_ai
                .get_mut(self.players.live(player_id, list_index)?.id)
            {
                ai.set_action(Some(AiAction::Make));
            }
            Making::Unit(MakeObj::for_entry(&first))
        } else {
            Self::make_research(&first)?
        };
        self.start_production_action(player_id, list_index, making)?;
        Ok(ProductionStart::Goes)
    }

    /// A finished shared-kind research of the building at `list_index` of player `player_id` is
    /// noted on its production list (`FUN_141616a40`'s shared technology), when the tree holds it.
    fn note_shared_research(
        &mut self,
        player_id: PlayerId,
        list_index: ListIndex,
    ) -> GenieResult<()> {
        if let Some(tech) = self
            .parts
            .production
            .get(self.players.live(player_id, list_index)?.id)
            .and_then(|production| production.queue.first())
            .and_then(|entry| entry.tech)
            && self
                .model
                .techs
                .get(tech.index())
                .is_some_and(|tech_def| tech_def.kind == TechKind::SHARED)
            && tech.index() < self.players.player(player_id)?.techs.len()
            && let Some(production) = self
                .parts
                .production
                .get_mut(self.players.live(player_id, list_index)?.id)
        {
            production.note_shared_research(tech);
        }
        Ok(())
    }

    /// The list's add (`FUN_140a977e0`) of `making` to the production list of the building at
    /// `list_index` of player `player_id`: for a building in a state below 3 it joins the list and
    /// starts (slot 0x28 with 1). A make action (`FUN_140acf0a0`) of a master the player has adds
    /// its population to attribute 81 and takes state 6; one of a master it lacks stays in state 0
    /// ("Canceling Train #1a"). (The building's update makes it paid, so the payment test of an
    /// unpaid one never runs here.) A research (`0x140ad1d80`): its start test
    /// ([`crate::technology::TechState::start_test`] against what the first entry paid) puts the
    /// technology in state 2 at the building's master and the action in state 6, with no work done;
    /// a failed test (its refund, `FUN_1413d15e0`) is not modelled. A building in state 3 or more
    /// deletes the action unstarted (its destructor runs, [`World::destroy_action`]).
    pub(super) fn start_production_action(
        &mut self,
        player_id: PlayerId,
        list_index: ListIndex,
        making: Making,
    ) -> GenieResult<()> {
        if ObjectState::Dying <= self.players.live(player_id, list_index)?.life.state {
            return self.destroy_action(player_id, list_index, making);
        }
        let mut making = making;
        match &mut making {
            Making::Unit(make) => {
                if let Some(master) = make
                    .master
                    .and_then(|master_id| player_master(&self.model.masters, player_id, master_id))
                {
                    let pop = population(&master.storage, &self.model.roles);
                    self.players
                        .player_mut(player_id)?
                        .add_attribute(self.model.slots.attribute.in_production, pop);
                    make.state = MakeObjState::Making;
                }
            }
            Making::Research(make_tech) => self.start_research(player_id, list_index, make_tech)?,
        }
        if let Some(production) = self
            .parts
            .production
            .get_mut(self.players.live(player_id, list_index)?.id)
        {
            production.action = Some(making);
        }
        Ok(())
    }

    /// A research action's start (`0x140ad1d80`): see [`World::start_production_action`].
    fn start_research(
        &mut self,
        player_id: PlayerId,
        list_index: ListIndex,
        make_tech: &mut MakeTech,
    ) -> GenieResult<()> {
        let (tech_index, def) = self.research_tech(player_id, make_tech.tech)?;
        let owner = self.players.player(player_id)?;
        let factors = factors(make_tech.tech, owner, self.model.slots)?;
        let paid = self
            .parts
            .production
            .get(owner.objects.checked(list_index)?.id)
            .and_then(|production| production.queue.first())
            .map_or(NO_COSTS, |entry| entry.costs);
        if !owner
            .techs
            .checked(tech_index)?
            .start_test(def, factors, &paid, owner)?
        {
            return Err(GenieError::unsupported(
                "a research whose start test fails (its refund)",
            ));
        }
        let master = owner.objects.checked(list_index)?.life.master_id;
        let state = self
            .players
            .player_mut(player_id)?
            .techs
            .checked_mut(tech_index)?;
        state.state = ResearchState::Researching;
        state.building_master = Some(master);
        make_tech.state = MakeTechState::Researching;
        make_tech.progress = Seconds::ZERO;
        Ok(())
    }

    /// The production list's update (`FUN_140a992b0`) of the building at `list_index` of player
    /// `player_id`: the first action's slot 0x30, or 1 for an empty list.
    pub(super) fn update_production_action(
        &mut self,
        player_id: PlayerId,
        list_index: ListIndex,
        dt_seconds: Seconds,
    ) -> GenieResult<UpdateCode> {
        let action = self
            .parts
            .production
            .get(self.players.live(player_id, list_index)?.id)
            .and_then(|production| production.action);
        match action {
            None => Ok(UpdateCode::Done),
            Some(Making::Unit(make)) => self.make_step((player_id, list_index), make, dt_seconds),
            Some(Making::Research(make_tech)) => {
                self.research_step((player_id, list_index), make_tech, dt_seconds)
            }
        }
    }

    /// The make action's update (`0x140ad0490`) at the building at `list_index` of player
    /// `player_id`: in state 1 it is done; in state 6 the building's work rate (its powerup, slot
    /// 0x6e8, plus its master's `work_rate`) times dt adds to its progress, and once its train time
    /// ([`MakeObj::time`]) is reached (`COMISS`, `JC`: a NaN waits) its unit spawns
    /// ([`World::spawn`], which a computer player's unit does not reach) and it is done; in state
    /// 10 its timer loses dt (`SUBSS`), and at 0 or below (`COMISS`, `JC`: a NaN waits) the spawn
    /// tries again; any other state waits. A spawn without a spot waits for one
    /// ([`MakeObj::wait_for_spot`]). Not modelled: a computer player's handicaps
    /// (`FUN_141659cf0`, the difficulty's factors on the rate), the game's instant work
    /// (`+0xc14`, `FUN_140c884c0`, 0 as the tick oracle holds it).
    pub(super) fn make_step(
        &mut self,
        (player_id, list_index): (PlayerId, ListIndex),
        make: MakeObj,
        dt_seconds: Seconds,
    ) -> GenieResult<UpdateCode> {
        let waits = match make.state {
            MakeObjState::Done => return Ok(UpdateCode::Done),
            MakeObjState::New => return Ok(UpdateCode::UnderWay),
            MakeObjState::Making => {
                let (progress, time) =
                    self.make_progress(player_id, list_index, make, dt_seconds)?;
                below_or_unordered(progress.get(), time.get())
            }
            MakeObjState::NoSpot => {
                let timer = make.timer - dt_seconds;
                if let Some(make_obj) = self.make_obj_mut(player_id, list_index)? {
                    make_obj.timer = timer;
                }
                below_or_unordered(0.0, timer.get())
            }
        };
        if waits {
            return Ok(UpdateCode::UnderWay);
        }
        let made = make
            .master
            .and_then(|master_id| player_master(&self.model.masters, player_id, master_id));
        let pop = made.map_or(Amount::ZERO, |master| {
            population(&master.storage, &self.model.roles)
        });
        let outcome = self.spawn(
            player_id,
            list_index,
            make.master.ok_or(GenieError::invalid_input(
                "a make action that names no master",
            ))?,
            pop,
        )?;
        let make_obj = self.make_obj_mut(player_id, list_index)?;
        Ok(match outcome {
            SpawnOutcome::Made => {
                if let Some(make_obj) = make_obj {
                    make_obj.state = MakeObjState::Done;
                }
                UpdateCode::Done
            }
            SpawnOutcome::NoSpot => {
                if let Some(make_obj) = make_obj {
                    make_obj.wait_for_spot();
                }
                UpdateCode::UnderWay
            }
        })
    }

    /// State 6 of the make action's update (`0x140ad0490`) at the building at `list_index` of
    /// player `player_id`: the building's work rate times dt adds to the action's progress; the
    /// new progress and the train time.
    fn make_progress(
        &mut self,
        player_id: PlayerId,
        list_index: ListIndex,
        make: MakeObj,
        dt_seconds: Seconds,
    ) -> GenieResult<(Seconds, Seconds)> {
        let building = &self.players.live(player_id, list_index)?;
        let site = building.master(self.master_tables(), player_id)?;
        let rate =
            self.parts.combat_of(building.id).work_rate_powerup + site.action_part().work_rate;
        let made = make
            .master
            .and_then(|master_id| player_master(&self.model.masters, player_id, master_id));
        let time = Seconds::of_master(make.time(made, building.life.master_id));
        let progress = Seconds::new(rate * dt_seconds) + make.progress;
        if let Some(make_obj) = self.make_obj_mut(player_id, list_index)? {
            make_obj.progress = progress;
        }
        Ok((progress, time))
    }

    /// The make action of the building at `list_index` of player `player_id`, when its
    /// production list's action is one.
    fn make_obj_mut(
        &mut self,
        player_id: PlayerId,
        list_index: ListIndex,
    ) -> GenieResult<Option<&mut MakeObj>> {
        let id = self.players.live(player_id, list_index)?.id;
        Ok(self.parts.production.get_mut(id).and_then(|production| {
            match production.action.as_mut() {
                Some(Making::Unit(make_obj)) => Some(make_obj),
                Some(Making::Research(_)) | None => None,
            }
        }))
    }

    /// The production list's first action goes (`FUN_140a98e20`, and `FUN_140a98620` for the list
    /// of one the building's update makes) at the building at `list_index` of player `player_id`:
    /// its destructor ([`World::destroy_action`]).
    pub(crate) fn end_production_action(
        &mut self,
        player_id: PlayerId,
        list_index: ListIndex,
    ) -> GenieResult<()> {
        let Some(making) = self
            .parts
            .production
            .get_mut(self.players.live(player_id, list_index)?.id)
            .and_then(|production| production.action.take())
        else {
            return Ok(());
        };
        self.destroy_action(player_id, list_index, making)
    }

    /// A production action's destructor at the building at `list_index` of player `player_id`. The
    /// make action's (`FUN_140ace3f0`): in state 6 or 10, while the game is not over, its unit's
    /// population leaves attribute 81 and, made without the queue's payment (`+0x60` clear), the
    /// master's paid costs come back ([`crate::production::refund_master`]); the building's unit AI
    /// is told the make action completed (0x1fa with 0x26b). The research's: its destructor
    /// ([`World::research_destroyed`]).
    pub(super) fn destroy_action(
        &mut self,
        player_id: PlayerId,
        list_index: ListIndex,
        making: Making,
    ) -> GenieResult<()> {
        let make = match making {
            Making::Unit(make) => make,
            Making::Research(make_tech) => {
                return self.research_destroyed(player_id, list_index, &make_tech);
            }
        };
        if matches!(make.state, MakeObjState::Making | MakeObjState::NoSpot)
            && self.game_state == PlayState::Playing
        {
            let made = make
                .master
                .and_then(|master_id| player_master(&self.model.masters, player_id, master_id));
            let pop = made.map_or(Amount::ZERO, |master| {
                population(&master.storage, &self.model.roles)
            });
            let refund = (!make.paid).then(|| made.cloned()).flatten();
            let owner = self.players.player_mut(player_id)?;
            owner.add_attribute(self.model.slots.attribute.in_production, -pop);
            if let Some(master) = refund {
                refund_master(&master, QueueCount::ONE, owner)?;
            }
        }
        let building = self.players.live_mut(player_id, list_index)?;
        let (id, state) = (building.id, building.life.state);
        if let Some(ai) = self.parts.unit_ai.get_mut(building.id) {
            ai.post(
                Notify::own(
                    id,
                    NotifyKind::ACTION_COMPLETED,
                    [AiAction::Make.value(), 0, 0],
                ),
                state,
            )?;
        }
        Ok(())
    }
}
