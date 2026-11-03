//! The research action (`TRIBE_Action_Make_Tech`): its technology, its update, its end and
//! its destructor.

use crate::GenieError;
use crate::math::conversions::cvttss2si;
use crate::objects::ListIndex;
use crate::objects::UpdateCode;
use crate::objects::masters::MasterId;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::production::{Entry, Production, refund_tech};
use crate::production::{MakeTech, MakeTechState, Making};
use crate::technology::ResearchState;
use crate::technology::TechId;
use crate::technology::{Location, ResearchCount, TechDef, TechKind, is_age};
use crate::unit_ai::AiAction;
use crate::unit_ai::Notify;
use crate::unit_ai::NotifyKind;
use crate::world::PlayState;
use crate::world::Seconds;
use crate::world::World;
use crate::world::slots::CheckedIndex;
use crate::world::slots::PlayerSlots;
use std::mem::take;

/// A building the research step runs for, whose action is no Research.
const NO_RESEARCH_ACTION: GenieError =
    GenieError::unreachable("a research step of a building without a research action");

impl World {
    /// The research's action (`TRIBE_Action_Make_Tech`, constructor `FUN_140ad0700` with the
    /// first entry's technology and master and 0, so paid; state 0, nothing done) for player
    /// `p`: its effect's first preparation (`FUN_1413d1530`) loads graphics and marks the
    /// masters an upgrade turns into (`+0x330`: the save does not hold the mark, and its
    /// readers, the object search `FUN_1411650e0` and the masters' slot 0xc0, are not
    /// modelled); the human player's sound is the interface's. Not held: the start time
    /// (`+0x74`).
    pub(super) fn make_research(first: &Entry) -> Result<Making, GenieError> {
        let tech = first.tech.ok_or(GenieError::missing_data(
            "a research entry without a technology",
        ))?;
        Ok(Making::Research(MakeTech {
            state: MakeTechState::New,
            paid: true,
            tech,
            master: i32::from(MasterId::field(first.master)),
            progress: Seconds::ZERO,
        }))
    }

    /// The technology `tech` of a research at player `p`'s building: its index in the player's
    /// tree and the `.dat`'s record (RESEARCH queued only technologies of both).
    pub(super) fn research_tech(
        &self,
        p: PlayerId,
        tech: TechId,
    ) -> Result<(usize, &TechDef), GenieError> {
        let tree = self.players.player(p)?.techs.len();
        let t = Some(tech.index())
            .filter(|&t| t < tree)
            .ok_or(GenieError::unsupported(
                "a research of a technology the tree has not got",
            ))?;
        let def = self.model.techs.get(t).ok_or(GenieError::unsupported(
            "a technology the data file has not got",
        ))?;
        Ok((t, def))
    }

    /// The research's update (slot 0x30, `0x140ad2a20`) at the building at `i` of player `p`:
    /// in state 1 it is done, in a state other than 6 it waits; in state 6 the building's work rate (its powerup and its master's rate) times dt adds to
    /// the progress, 32767 once the technology was researched 0x100 times, which the
    /// technology's work done takes; a repeatable technology or one of the shared kind turns to
    /// state 2 first. The research is done once the progress reaches the location's time,
    /// times the research time modifier (attribute 86, above 0 and not an age), plus its
    /// adjustment ([`World::research_done`]). Not held: the human player's sound; not
    /// modelled: a computer player's handicaps and the game's instant work (`FUN_140c884c0`,
    /// 0 as the tick oracle holds it).
    pub(super) fn research_step(
        &mut self,
        p: PlayerId,
        i: ListIndex,
        dt_seconds: Seconds,
    ) -> Result<UpdateCode, GenieError> {
        use crate::technology::Attributes;
        if self.players.player(p)?.kind.is_computer() {
            return Err(GenieError::unsupported(
                "a computer player's research (its handicaps)",
            ));
        }
        let owner = self.players.player(p)?;
        let building = owner.objects.checked(i)?;
        let Some(Some(Making::Research(r))) =
            self.parts.production.get(building.id).map(|q| q.action)
        else {
            return Err(NO_RESEARCH_ACTION);
        };
        match r.state {
            MakeTechState::Done => return Ok(UpdateCode::DONE),
            MakeTechState::Researching => {}
            MakeTechState::New => return Ok(UpdateCode::UNDER_WAY),
        }
        let (t, def) = self.research_tech(p, r.tech)?;
        let master = building.life.master_id;
        let rate = self.parts.combat_of(building.id).work_rate_powerup
            + self.master(p, master)?.action_part().work_rate;
        let state = owner.techs.checked(t)?;
        let repeatable = state.repeatable(def, false);
        let shared = def.kind == TechKind::SHARED;
        let time_modifier = owner.get(self.model.slots.attribute.research_time_modifier)?;
        let location = state
            .location(master)
            .copied()
            .ok_or(GenieError::unsupported(
                "a research at a technology without locations",
            ))?;
        let times = state.times_researched.get();
        let owner = self.players.player_mut(p)?;
        let state = owner.techs.checked_mut(t)?;
        if state.state != ResearchState::Researching && (repeatable || shared) {
            state.state = ResearchState::Researching;
        }
        let mut progress = r.progress + Seconds::new(rate * dt_seconds);
        if 0xff < times {
            progress = Seconds::new(32767.0);
        }
        state.research_done = progress;
        let id = owner.objects.checked(i)?.id;
        set_progress(self.parts.production.get_mut(id), progress);
        if research_time(&location, r.tech, time_modifier) <= progress {
            if 0xff < times {
                return Err(GenieError::unsupported(
                    "a research done 0x100 times (set_state 1)",
                ));
            }
            self.research_done(p, i, r.tech, master, repeatable)?;
            return Ok(UpdateCode::DONE);
        }
        Ok(UpdateCode::UNDER_WAY)
    }

    /// The research's end in its update (`0x140ad2a20`) once its progress reaches its time:
    /// `FUN_1413d1740` (with `repeat`, the player's `+0x648` or [`TechState::repeatable`]): a
    /// technology in state 2 (or with `repeat`) whose work done reaches the `.dat`'s time at the
    /// building (â1 without), times the research time modifier (attribute 86, above 0, not an
    /// age), plus the location's adjustment, is done ([`World::complete_research`]); else, or
    /// without `repeat`, it is left. Then the action takes state 1 (the event 100 and the
    /// computer players' slot 0x3a8 are not held) and the building's unit AI is told 0x1fa with
    /// the action value 0x26c. Last, attribute 269 (the Spanish's 300) names an effect of the
    /// world's table ([`Model::effects`](genie_model::model::Model::effects), slot 0x10) the player gets with the building as its
    /// object ([`World::do_effect`]).
    ///
    /// [`TechState::repeatable`]: crate::technology::TechState::repeatable
    pub(super) fn research_done(
        &mut self,
        p: PlayerId,
        i: ListIndex,
        t: TechId,
        master: MasterId,
        repeat: bool,
    ) -> Result<(), GenieError> {
        use crate::technology::Attributes;
        let (tech, t) = (t, t.index());
        let owner = self.players.player(p)?;
        let state = owner.techs.checked(t)?;
        let def = self.model.techs.checked(t)?;
        if state.state == ResearchState::Researching || repeat {
            let mut time = def
                .locations
                .iter()
                .find(|l| l.0 == Some(master))
                .map_or(-1.0, |l| Seconds::of_master(l.1).get());
            let modifier = owner.get(self.model.slots.attribute.research_time_modifier)?;
            if !is_age(tech) && Amount::ZERO < modifier {
                time *= modifier.get();
            }
            let adjustment = state
                .location(master)
                .map_or(Seconds::ZERO, |l| l.adjustment);
            if repeat || Seconds::new(time) + adjustment <= state.research_done {
                let _ = self.complete_research(p, tech, Some(master))?;
            }
        }
        let building = self.players.live_mut(p, i)?;
        let (id, state) = (building.id, building.life.state);
        if let Some(Some(Making::Research(r))) =
            self.parts.production.get_mut(id).map(|q| q.action.as_mut())
        {
            r.state = MakeTechState::Done;
        }
        if let Some(ai) = self.parts.unit_ai.get_mut(building.id) {
            ai.post(
                Notify::own(
                    id,
                    NotifyKind::ACTION_COMPLETED,
                    [AiAction::Research.value(), 0, 0],
                ),
                state,
            )?;
        }
        // attribute 269, cut by CVTTSS2SI and its low short above 0, names an effect of the
        // world's table the player gets with the building as its object (its slot 0x10,
        // `TRIBE_Effects::do_effect`, which skips an id the table does not hold)
        let reward = cvttss2si(
            (self
                .players
                .player(p)?
                .get(self.model.slots.attribute.world_effect)?)
            .get(),
        ) as i16;
        if 0 < reward
            && let Some(effect) = usize::try_from(reward)
                .ok()
                .and_then(|e| self.model.effects.get(e))
                .cloned()
        {
            self.do_effect(p, &effect, Some(id))?;
        }
        Ok(())
    }

    /// [`World::research_destroyed`]'s part for a research not done while the game is not
    /// over: an unpaid one (`+0x60` clear) gives technology `t`'s costs back
    /// ([`crate::production::refund_tech`]), and the building's unit AI is told the research
    /// failed (0x1f9 with 0x26c).
    fn research_failed(
        &mut self,
        p: PlayerId,
        i: ListIndex,
        r: &MakeTech,
        t: usize,
    ) -> Result<(), GenieError> {
        if !r.paid {
            let def = self.model.techs.checked(t)?;
            let owner = self.players.player_mut(p)?;
            let mut state = take(owner.techs.checked_mut(t)?);
            let done = refund_tech(&mut state, def, r.tech, owner, self.model.slots);
            *owner.techs.checked_mut(t)? = state;
            let _ = done?;
        }
        let building = self.players.live_mut(p, i)?;
        let (id, state) = (building.id, building.life.state);
        if let Some(ai) = self.parts.unit_ai.get_mut(building.id) {
            ai.post(
                Notify::own(
                    id,
                    NotifyKind::ACTION_FAILED,
                    [AiAction::Research.value(), 0, 0],
                ),
                state,
            )?;
        }
        Ok(())
    }

    /// The research action's destructor (`FUN_140ad0be0`) at the building at `i` of player
    /// `p`. Done (state 1), the human player's sound is the interface's; else, while the game is
    /// not over, a research made unpaid (`+0x60` clear) gives its technology's costs back
    /// ([`crate::production::refund_tech`]) and the building's unit AI is told the research
    /// failed (0x1f9 with 0x26c). Then a technology of the shared kind (`FUN_1413d1020`) is in
    /// state 1 again, neither researched nor queued; another that stacks below its cap (`+0x56`,
    /// 0x100 above 0x7fff; the player's `+0x648`, which would count too, is not held) takes the
    /// action's progress as its work done and, done (the progress at least the `.dat`'s time at
    /// the building times the research time modifier, above 0 and not an age, plus the
    /// location's adjustment; `COMISS`, `SETNC`), counts one more researched and one less
    /// queued, in state 1 while one more stays below the cap and it may be queued again (its
    /// `+0xe` set and its effect no upgrade), else in state 3; not done, state 1.
    pub(super) fn research_destroyed(
        &mut self,
        p: PlayerId,
        i: ListIndex,
        r: &MakeTech,
    ) -> Result<(), GenieError> {
        use crate::technology::Attributes;
        let (t, _) = self.research_tech(p, r.tech)?;
        if r.state != MakeTechState::Done && self.game_state == PlayState::PLAYING {
            self.research_failed(p, i, r, t)?;
        }
        let def = self.model.techs.checked(t)?;
        let owner = self.players.player(p)?;
        let state = owner.techs.checked(t)?;
        if def.kind == TechKind::SHARED {
            let state = self.players.player_mut(p)?.techs.checked_mut(t)?;
            state.state = ResearchState::Available;
            state.times_researched = ResearchCount::ZERO;
            state.pending = ResearchCount::ZERO;
            return Ok(());
        }
        let cap = state.research_cap.limit();
        if !(state.stack && state.times_researched.get() < cap) {
            return Ok(());
        }
        let once = !state.allow_multiple || def.upgrades();
        let master = owner.objects.checked(i)?.life.master_id;
        let mut time = def
            .locations
            .iter()
            .find(|l| l.0 == Some(master))
            .map_or(-1.0, |l| Seconds::of_master(l.1).get());
        let adjustment = state
            .location(master)
            .map_or(Seconds::ZERO, |l| l.adjustment);
        let modifier = owner.get(self.model.slots.attribute.research_time_modifier)?;
        if !is_age(r.tech) && Amount::ZERO < modifier {
            time *= modifier.get();
        }
        let done = adjustment + Seconds::new(time) <= r.progress;
        let state = self.players.player_mut(p)?.techs.checked_mut(t)?;
        state.research_done = r.progress;
        if done {
            state.times_researched = state.times_researched.plus(1);
            state.pending = state.pending.minus(1);
            state.state = if i32::from(state.times_researched.get()) < i32::from(cap) && !once {
                ResearchState::Available
            } else {
                ResearchState::Researched
            };
        } else {
            state.state = ResearchState::Available;
        }
        Ok(())
    }
}

/// The research action of a building's `production` takes the progress `progress`.
fn set_progress(production: Option<&mut Production>, progress: Seconds) {
    if let Some(Some(Making::Research(r))) = production.map(|q| q.action.as_mut()) {
        r.progress = progress;
    }
}

/// The progress a research of `tech` at `location` is done at: the location's time, times the
/// research time modifier `time_modifier` (attribute 86, above 0 and not an age), plus its
/// adjustment.
fn research_time(location: &Location, tech: TechId, time_modifier: Amount) -> Seconds {
    let mut time = Seconds::of_master(location.time).get();
    if !is_age(tech) && Amount::ZERO < time_modifier {
        time *= time_modifier.get();
    }
    location.adjustment + Seconds::new(time)
}
