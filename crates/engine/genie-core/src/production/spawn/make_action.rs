//! A building's production list and its make action (`TRIBE_Action_Make_Obj`).

use crate::GenieError;
use crate::gathering::QueueCount;
use crate::objects::ListIndex;
use crate::objects::UpdateCode;
use crate::objects::masters::action_type;
use crate::objects::masters::player_master;
use crate::orders::NO_QUEUE;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::players::attribute::IN_PRODUCTION;
use crate::players::attribute::QUEUED;
use crate::production::{
    MakeObj, MakeObjState, MakeTech, MakeTechState, Making, housed, population,
};
use crate::technology::ResearchState;
use crate::unit_ai::ActionKind;
use crate::unit_ai::Notify;
use crate::unit_ai::NotifyKind;
use crate::world::Seconds;
use crate::world::World;
use crate::world::slots::PlayerSlots;

/// A queue that is enabled and counts an entry, but holds none.
const EMPTY_QUEUE: GenieError =
    GenieError::unreachable("a production queue that counts an entry but holds none");

/// A building the make step runs for, whose action is no unit's Make.
const NO_MAKE_ACTION: GenieError =
    GenieError::unreachable("a make step of a building without a make action");

impl World {
    /// `TRIBE_Building_Object::update`'s part after its combat update (`0x1411ca3e2`) for the
    /// building at `i` of player `p`: past state 2 (dying or dead) it cancels its whole queue
    /// in every update ([`World::cancel_queue`], `FUN_1411cff00`: the units and researches
    /// given back, the change counter counting on); in state 2 or below its production step
    /// runs ([`World::produce`]). Its second action list (`+0xba0`: cleared first past state 2,
    /// else updated by `FUN_140acafd0` into `+0xb9d`) holds nothing the world models.
    ///
    /// # Errors
    ///
    /// A building the world does not hold, and the cancel's and the production's.
    pub(crate) fn building_production(
        &mut self,
        p: PlayerId,
        i: ListIndex,
        dt_seconds: Seconds,
    ) -> Result<(), GenieError> {
        if crate::objects::ObjectState::Alive < self.players.live(p, i)?.life.state {
            return self.cancel_queue(p, i);
        }
        self.produce(p, i, dt_seconds)
    }

    /// The building update's production step (`FUN_1411e7dc0`, after its combat update) for the
    /// building at `i` of player `p`. A disabled queue (`+0xbf6`) or an empty one does nothing.
    /// For a player not of type 3 a top action of the building's own (`FUN_141227a00`) that is a
    /// make, a research or of types 0x7b to 0x7d sets `+0xc01` and nothing more happens; else
    /// `+0xc01` clears. Without an action in its production list the first entry's comes: for a
    /// unit the population test first ([`housed`]): a housed building waits (its reason
    /// `+0xc02`, 0x20 or 4, and the events 0x7c and 0x7d are the interface's), a computer
    /// player's cancels its queue ([`World::cancel_queue`]); then the make action
    /// ([`MakeObj::for_entry`]; its constructor `FUN_140acb140` gives the building's unit AI the
    /// action 619, `FUN_140c64b50`; the headroom's sound and event 0xa9 are the interface's), or
    /// the research's ([`World::make_research`]). The list's add (`FUN_140a977e0`) starts it
    /// for a building in a state below 3 (slot 0x28): a make action of a master the player has
    /// (`FUN_140acf0a0`) adds its population to attribute 81 and takes state 6, one of a master
    /// it lacks stays in state 0 ("Canceling Train #1a"); a building in state 3 or more deletes
    /// it at once ([`World::end_production_action`]). Then the list's update (`FUN_140a992b0`,
    /// the first action's slot 0x30; an empty list answers done): the make action's
    /// ([`World::make_step`]) or the research's ([`World::research_step`]). Done (1) or failed
    /// (2, 3): after a research of the shared kind the technology joins the building's lists
    /// ([`crate::production::Production::note_shared_research`]); the action goes ([`World::end_production_action`])
    /// and the first entry lets one go: a unit's population leaves attribute 80
    /// ([`crate::production::Queue::release_first`]).
    pub(crate) fn produce(
        &mut self,
        p: PlayerId,
        i: ListIndex,
        dt_seconds: Seconds,
    ) -> Result<(), GenieError> {
        let unit = &self.players.live(p, i)?;
        let Some(production) = unit.production.as_ref() else {
            return Ok(());
        };
        if !production.queue.enabled
            || production.queue.count == crate::production::QueueLength::ZERO
        {
            return Ok(());
        }
        let busy = !self.players.player(p)?.computer
            && unit.top_action_kind().is_some_and(|k| {
                matches!(
                    k,
                    action_type::MAKE_OBJECT
                        | action_type::MAKE_TECH
                        | action_type::ACTION_123
                        | action_type::PACK
                        | action_type::UNPACK
                )
            });
        if let Some(production) = self.players.live_mut(p, i)?.production.as_mut() {
            production.queue.waiting = busy;
        }
        if busy {
            return Ok(());
        }
        if !self.begin_production(p, i)? {
            return Ok(());
        }
        let code = self.update_production_action(p, i, dt_seconds)?;
        if !matches!(code.get(), 1..=3) {
            return Ok(());
        }
        if code == UpdateCode::DONE {
            self.note_shared_research(p, i)?;
        }
        self.end_production_action(p, i)?;
        let Some(first) = self
            .players
            .live(p, i)?
            .production
            .as_ref()
            .and_then(|q| q.queue.first().copied())
        else {
            return Ok(());
        };
        if first.tech.is_none() {
            let pop = first
                .master
                .and_then(|m| player_master(&self.masters, p, m))
                .map_or(Amount::ZERO, |m| population(&m.storage));
            self.players.player_mut(p)?.add_attribute(QUEUED, -pop)?;
        }
        if let Some(production) = self.players.live_mut(p, i)?.production.as_mut() {
            production.queue.release_first();
        }
        Ok(())
    }

    /// The first queue entry's action starts when the list has none ([`World::produce`]): a unit
    /// waits housed (a computer player's entry is cancelled), else it starts. False when the
    /// production stops for this update.
    fn begin_production(&mut self, p: PlayerId, i: ListIndex) -> Result<bool, GenieError> {
        let production = self
            .players
            .live(p, i)?
            .production
            .as_ref()
            .ok_or(NO_QUEUE)?;
        if production.action.is_none() {
            let first = *production.queue.first().ok_or(EMPTY_QUEUE)?;
            let making = if first.tech.is_none() {
                let pop = first
                    .master
                    .and_then(|m| player_master(&self.masters, p, m))
                    .map_or(Amount::ZERO, |m| population(&m.storage));
                if housed(&self.players.player(p)?.attributes, pop)? {
                    if self.players.player(p)?.computer {
                        self.cancel_queue(p, i)?;
                        return Ok(false);
                    }
                    return Ok(false);
                }
                if let Some(ai) = self.players.live_mut(p, i)?.ai.as_mut() {
                    ai.set_action(Some(ActionKind::Make));
                }
                Making::Unit(MakeObj::for_entry(&first))
            } else {
                Self::make_research(&first)?
            };
            self.start_production_action(p, i, making)?;
        }
        Ok(true)
    }

    /// A finished shared-kind research of the building at `i` of player `p` is noted on its
    /// production list (`FUN_141616a40`'s shared technology), when the tree holds it.
    fn note_shared_research(&mut self, p: PlayerId, i: ListIndex) -> Result<(), GenieError> {
        if let Some(tech) = self
            .players
            .live(p, i)?
            .production
            .as_ref()
            .and_then(|q| q.queue.first())
            .and_then(|e| e.tech)
            && self
                .techs
                .get(tech.index())
                .is_some_and(|d| d.kind == crate::technology::TechType::SHARED)
            && tech.index() < self.players.player(p)?.techs.len()
            && let Some(production) = self.players.live_mut(p, i)?.production.as_mut()
        {
            production.note_shared_research(tech);
        }
        Ok(())
    }

    /// The list's add (`FUN_140a977e0`) of `making` to the production list of the building at
    /// `i` of player `p`: for a building in a state below 3 it joins the list and starts
    /// (slot 0x28 with 1). A make action (`FUN_140acf0a0`) of a master the player has adds its
    /// population to attribute 81 and takes state 6; one of a master it lacks stays in state 0
    /// ("Canceling Train #1a"). (The building's update makes it paid, so the payment test of an
    /// unpaid one never runs here.) A research (`0x140ad1d80`): its start test
    /// ([`crate::technology::TechState::start_test`] against what the first entry paid) puts the
    /// technology in state 2 at the building's master and the action in state 6, with no work
    /// done; a failed test (its refund, `FUN_1413d15e0`) is not modelled. A building in state 3
    /// or more deletes the action unstarted (its destructor runs, [`World::destroy_action`]).
    pub(super) fn start_production_action(
        &mut self,
        p: PlayerId,
        i: ListIndex,
        making: Making,
    ) -> Result<(), GenieError> {
        if crate::objects::ObjectState::Dying <= self.players.live(p, i)?.life.state {
            return self.destroy_action(p, i, making);
        }
        let mut making = making;
        match &mut making {
            Making::Unit(make) => {
                if let Some(m) = make.master.and_then(|m| player_master(&self.masters, p, m)) {
                    let pop = population(&m.storage);
                    self.players
                        .player_mut(p)?
                        .add_attribute(IN_PRODUCTION, pop)?;
                    make.state = MakeObjState::Making;
                }
            }
            Making::Research(r) => self.start_research(p, i, r)?,
        }
        if let Some(production) = self.players.live_mut(p, i)?.production.as_mut() {
            production.action = Some(making);
        }
        Ok(())
    }

    /// A research action's start (`0x140ad1d80`): see [`World::start_production_action`].
    fn start_research(
        &mut self,
        p: PlayerId,
        i: ListIndex,
        r: &mut MakeTech,
    ) -> Result<(), GenieError> {
        use crate::technology::Attributes;
        let (t, def) = self.research_tech(p, r.tech)?;
        let owner = self.players.player(p)?;
        let get = |k: AttributeId| owner.get(k);
        let factors = crate::technology::factors(r.tech, &get)?;
        let paid = owner.objects[i]
            .production
            .as_ref()
            .and_then(|q| q.queue.first())
            .map_or(crate::production::NO_COSTS, |e| e.costs);
        if !owner.techs[t].start_test(def, factors, &paid, &get)? {
            return Err(GenieError::unsupported(
                "a research whose start test fails (its refund)",
            ));
        }
        let master = owner.objects[i].life.master_id;
        let state = &mut self.players.player_mut(p)?.techs[t];
        state.state = ResearchState::Researching;
        state.building_master = Some(master);
        r.state = MakeTechState::Researching;
        r.progress = Seconds::ZERO;
        Ok(())
    }

    /// The production list's update (`FUN_140a992b0`) of the building at `i` of player `p`:
    /// the first action's slot 0x30, or 1 for an empty list.
    pub(super) fn update_production_action(
        &mut self,
        p: PlayerId,
        i: ListIndex,
        dt_seconds: Seconds,
    ) -> Result<UpdateCode, GenieError> {
        let action = self
            .players
            .live(p, i)?
            .production
            .as_ref()
            .and_then(|q| q.action);
        match action {
            None => Ok(UpdateCode::DONE),
            Some(Making::Unit(_)) => self.make_step(p, i, dt_seconds),
            Some(Making::Research(_)) => self.research_step(p, i, dt_seconds),
        }
    }

    /// The make action's update (`0x140ad0490`) at the building at `i` of player `p`: in state
    /// 1 it is done; in state 6 the building's work rate (its powerup, slot 0x6e8, plus its
    /// master's `work_rate`) times dt adds to its progress, and once its train time
    /// ([`MakeObj::time`]) is reached (`COMISS`, `JC`: a NaN waits) its unit spawns
    /// ([`World::spawn`], which a computer player's unit does not reach) and it is done; any
    /// other state waits. Not modelled: a computer player's handicaps (`FUN_141659cf0`, the
    /// difficulty's factors on the rate), the game's instant work (`+0xc14`,
    /// `FUN_140c884c0`, 0 as the tick oracle holds it), the spot search's failure (state 10's
    /// timer, `FUN_140ace630`).
    pub(super) fn make_step(
        &mut self,
        p: PlayerId,
        i: ListIndex,
        dt_seconds: Seconds,
    ) -> Result<UpdateCode, GenieError> {
        let building = &self.players.live(p, i)?;
        let Some(Some(Making::Unit(make))) = building.production.as_ref().map(|q| q.action) else {
            return Err(NO_MAKE_ACTION);
        };
        match make.state {
            MakeObjState::Done => return Ok(UpdateCode::DONE),
            MakeObjState::Making => {}
            MakeObjState::New | MakeObjState::NoSpot => return Ok(UpdateCode::UNDER_WAY),
        }
        let site = building.master(&self.masters, p)?;
        let rate = building.work_rate_powerup + site.work_rate;
        let made = make.master.and_then(|m| player_master(&self.masters, p, m));
        let time = Seconds::of_master(make.time(made, building.life.master_id)).get();
        let progress = Seconds::new(rate * dt_seconds) + make.progress;
        if let Some(Some(Making::Unit(m))) = self
            .players
            .live_mut(p, i)?
            .production
            .as_mut()
            .map(|q| q.action.as_mut())
        {
            m.progress = progress;
        }
        if crate::math::fmath::below_or_unordered(progress.get(), time) {
            return Ok(UpdateCode::UNDER_WAY);
        }
        let pop = made.map_or(Amount::ZERO, |m| population(&m.storage));
        self.spawn(
            p,
            i,
            make.master.ok_or(GenieError::invalid_input(
                "a make action that names no master",
            ))?,
            pop,
        )?;
        if let Some(Some(Making::Unit(m))) = self
            .players
            .live_mut(p, i)?
            .production
            .as_mut()
            .map(|q| q.action.as_mut())
        {
            m.state = MakeObjState::Done;
        }
        Ok(UpdateCode::DONE)
    }

    /// The production list's first action goes (`FUN_140a98e20`, and `FUN_140a98620` for the
    /// list of one the building's update makes) at the building at `i` of player `p`: its
    /// destructor ([`World::destroy_action`]).
    pub(crate) fn end_production_action(
        &mut self,
        p: PlayerId,
        i: ListIndex,
    ) -> Result<(), GenieError> {
        let Some(making) = self
            .players
            .live_mut(p, i)?
            .production
            .as_mut()
            .and_then(|q| q.action.take())
        else {
            return Ok(());
        };
        self.destroy_action(p, i, making)
    }

    /// A production action's destructor at the building at `i` of player `p`. The make
    /// action's (`FUN_140ace3f0`): in state 6 or 10, while the game is not over, its unit's
    /// population leaves attribute 81 and, made without the queue's payment (`+0x60` clear),
    /// the master's paid costs come back ([`crate::production::refund_master`]); the building's
    /// unit AI is told the make action completed (0x1fa with 0x26b). The research's: its
    /// destructor ([`World::research_destroyed`]).
    pub(super) fn destroy_action(
        &mut self,
        p: PlayerId,
        i: ListIndex,
        making: Making,
    ) -> Result<(), GenieError> {
        let make = match making {
            Making::Unit(make) => make,
            Making::Research(r) => return self.research_destroyed(p, i, &r),
        };
        if matches!(make.state, MakeObjState::Making | MakeObjState::NoSpot)
            && self.game_state == crate::world::PlayState::PLAYING
        {
            let made = make.master.and_then(|m| player_master(&self.masters, p, m));
            let pop = made.map_or(Amount::ZERO, |m| population(&m.storage));
            let refund = (!make.paid).then(|| made.cloned()).flatten();
            let owner = self.players.player_mut(p)?;
            owner.add_attribute(IN_PRODUCTION, -pop)?;
            if let Some(m) = refund {
                crate::production::refund_master(&m, QueueCount::ONE, owner)?;
            }
        }
        let building = self.players.live_mut(p, i)?;
        let (id, state) = (building.id, building.life.state);
        if let Some(ai) = building.ai.as_mut() {
            ai.post(
                Notify::own(
                    id,
                    NotifyKind::ACTION_COMPLETED,
                    [ActionKind::Make.value(), 0, 0],
                ),
                state,
            )?;
        }
        Ok(())
    }
}
