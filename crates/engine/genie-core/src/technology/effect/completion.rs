//! A research's completion (`FUN_1413cdcd0`): its effect, the technologies that depend on
//! it, a built building's technologies, the cost modifier and the disable and enable commands.

use crate::math::conversions::cvttss2si;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::Availability;
use crate::objects::masters::CostAmount;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::player_master;
use crate::players::Amount;
use crate::players::CivId;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::players::history::EventKind;
use crate::production::LocationButton;
use crate::production::Research;
use crate::technology::ChangeMode;
use crate::technology::DisabledTech;
use crate::technology::ResearchState;
use crate::technology::SHARED_RESEARCH;
use crate::technology::TechId;
use crate::technology::TechMode;
use crate::technology::effect::ATTRIBUTE_39;
use crate::technology::{EffectCommand, ObjectsKind, Party};
use crate::technology::{Location, TechDef, TechKind, TechState, factors};
use crate::world::GameMode;
use crate::world::Seconds;
use crate::world::World;
use crate::world::layer::AttributeRoles;
use crate::world::layer::MasterRoles;
use crate::world::settings::OptionFlags;
use crate::world::slots::{CheckedIndexExt, PlayerSlotsExt};
use crate::{GenieError, GenieResult};
use std::sync::Arc;

/// The technologies `FUN_1413cce70` updates ([`World::update_dependents`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Dependents {
    /// Those that require a technology (its id).
    Of(TechId),
    /// Every technology of the player's tree (−1).
    Every,
}

/// An enable command finds the master it tested gone.
const NO_MASTER: GenieError =
    GenieError::unreachable("an enable command for a master the player lacks");

impl World {
    /// `FUN_1413cdcd0` for player `player_id`'s technology `tech_id` (`param_3` 0) researched at a
    /// building of master `building`: a technology not in state 0 to 2 is not done (the game's
    /// overrides are not held). Else it takes state 3, gives its effect (its override's, else its
    /// own; [`World::do_effect`]) and updates the technologies that depend on it
    /// ([`World::update_dependents`]); researched at a location with a button, it counts in
    /// attribute 0x15 and its listed costs (`FUN_1413d4a00`, the amounts summed) in attribute 0x63.
    /// Last, an age (101 to 103) adds its event to the player's history (`FUN_141560ae0` with 0, 1
    /// or 2, [`crate::players::history::History::add_event`]), which the AI's age time reads. Not
    /// held: the local player's statistics and achievements (`FUN_1409644a0` among them). Not
    /// modelled: a research paid in relics (`FUN_1413d0e70`), an effect the slice cannot run
    /// without its building ([`World::research_effect_known`]) and the civ's disabled technologies
    /// (`FUN_140ea93a0`; such a technology is never available to be researched).
    pub(crate) fn complete_research(
        &mut self,
        player_id: PlayerId,
        tech_id: TechId,
        building: Option<MasterId>,
    ) -> GenieResult<Research> {
        let (tech, index) = (tech_id, tech_id.index());
        let def = self.model.techs.checked(index)?.clone();
        let state = &self.players.player(player_id)?.techs.checked(index)?;
        if !(ResearchState::Unavailable..=ResearchState::Researching).contains(&state.state) {
            return Ok(Research::NotDone);
        }
        if self
            .model
            .roles
            .of_some_attribute(def.costs[0].attribute)
            .contains(AttributeRoles::RELICS_HELD)
        {
            return Err(GenieError::unsupported(
                "a research paid in relics (FUN_1413d0e70)",
            ));
        }
        self.research_effect_known(player_id, tech)?;
        self.players
            .player_mut(player_id)?
            .techs
            .checked_mut(index)?
            .state = ResearchState::Researched;
        let effect = self
            .players
            .player(player_id)?
            .techs
            .checked(index)?
            .effect(&def);
        self.do_table_effect(player_id, effect, None)?;
        self.update_dependents(player_id, Dependents::Of(tech))?;
        let owner = &self.players.player(player_id)?;
        let state = owner.techs.checked(index)?;
        let location = building.map_or_else(
            || state.locations.first(),
            |master_id| state.location(master_id),
        );
        if location.is_some_and(|location| {
            location.building.is_some() && location.button != LocationButton::NONE
        }) {
            let factors = factors(tech, *owner, self.model.slots)?;
            let listed = state.listed_costs(&def, factors)?;
            let spent = listed
                .iter()
                .map(|&(_, cost_amount)| i32::from(cost_amount.get()))
                .sum::<i32>() as f32;
            let owner = self.players.player_mut(player_id)?;
            owner.add_attribute(
                self.model.slots.attribute.techs_researched,
                Amount::new(1.0),
                &self.model.roles,
            );
            owner.add_attribute(
                self.model.slots.attribute.research_spent,
                Amount::new(spent),
                &self.model.roles,
            );
        }
        let age = match index {
            101 => Some(EventKind::FeudalAge),
            102 => Some(EventKind::CastleAge),
            103 => Some(EventKind::ImperialAge),
            _ => None,
        };
        if let Some(kind) = age {
            let now = self.world_time;
            self.players
                .player_mut(player_id)?
                .history
                .add_event(kind, 0.0, 0.0, 0, now);
        }
        Ok(Research::Completed)
    }

    /// Whether the effect player `player_id`'s technology `researched` gives (`FUN_1413ce9b0`,
    /// [`crate::technology::TechState::effect`]: its override's when it has one) runs here, with
    /// the research's building as the effect's object (`FUN_1413cdcd0`'s
    /// `param_4`, which the callers do not pass yet): an effect that reaches its object (an
    /// enable for one building, an upgrade of the object, the object's kinds 203 to 206, a
    /// disable of a building's own technology) would differ without it.
    pub(super) fn research_effect_known(
        &self,
        player_id: PlayerId,
        researched: TechId,
    ) -> GenieResult<()> {
        let index = researched.index();
        let def = self.model.techs.checked(index)?;
        let state = &self.players.player(player_id)?.techs.checked(index)?;
        let shared = |tech: DisabledTech| {
            // FUN_1413d1020 tests the whole id
            tech.whole()
                .and_then(|tech_id| self.model.techs.get(tech_id.index()))
                .is_some_and(|tech_def| tech_def.kind == TechKind::SHARED)
        };
        // the player's own enable for one building and upgrade of the object (kinds 2 and 3),
        // the disable of a shared technology, the object's kinds 203 to 206
        let commands = self
            .model
            .effects
            .get(state.effect(def))
            .map_or(&[][..], |effect| &effect.commands);
        let reaches_object = commands.iter().any(|effect_command| match *effect_command {
            EffectCommand::Enable {
                party: Party::Own,
                one,
                ..
            } => one,
            EffectCommand::Upgrade {
                party: Party::Own,
                mode,
                ..
            } => mode.one_object(),
            EffectCommand::TechDisable { tech } => shared(tech),
            EffectCommand::Objects(kind) => {
                matches!(kind, ObjectsKind::Object(_) | ObjectsKind::ObjectMaster)
            }
            _ => false,
        });
        if reaches_object {
            return Err(GenieError::unsupported(
                "a research's effect on its building (FUN_1413cdcd0's param_4, not passed)",
            ));
        }
        Ok(())
    }

    /// A completed building's technology `tech_id` for player `player_id` (the end of
    /// `FUN_1411dbb70`'s `set_state(2)`): [`World::complete_research`] with `param_3` 0 and no
    /// building, so the technology's first location decides its statistics; a technology beyond the
    /// player's tree is not researched.
    pub(crate) fn research_built(
        &mut self,
        player_id: PlayerId,
        tech_id: TechId,
    ) -> GenieResult<()> {
        let tree = self.players.player(player_id)?.techs.len();
        if tree == 0 || self.model.techs.len() != tree {
            return Err(GenieError::unsupported(
                "a completion's technology without the player's tech tree",
            ));
        }
        if tech_id.index() < tree {
            let _ = self.complete_research(player_id, tech_id, None)?;
        }
        Ok(())
    }

    /// After a built building of `master` died (`FUN_1411dbb70`): unless another of player
    /// `player_id`'s live objects of the master stands (state 2, `FUN_141166ab0`), the master's
    /// technology `tech` in a state above 0 turns to 0 and its dependents are updated
    /// (`FUN_1413d4ec0`, then `FUN_1413cce70`, [`World::update_dependents`]).
    pub(crate) fn withdraw_tech(
        &mut self,
        player_id: PlayerId,
        master: MasterId,
        tech: TechId,
    ) -> GenieResult<()> {
        let owner = &self.players.player(player_id)?;
        if owner
            .objects
            .iter()
            .any(|unit| unit.life.master_id == master && unit.life.state == ObjectState::Alive)
        {
            return Ok(());
        }
        let index = tech.index();
        if owner.techs.len() <= index {
            return Ok(());
        }
        if owner.techs.checked(index)?.state <= ResearchState::Unavailable {
            return Ok(());
        }
        self.players
            .player_mut(player_id)?
            .techs
            .checked_mut(index)?
            .state = ResearchState::Unavailable;
        self.update_dependents(player_id, Dependents::Of(tech))
    }

    /// `FUN_1413cce70` for player `player_id`'s technologies `dependents`: each technology that
    /// requires a technology, by id (`+0x38`'s lists), or with −1 each of the player's tree (its
    /// count `+0x20`), not of the shared kind: one in state 0 or 1 and not held by `+0x60` turns to
    /// −1 when it is another civ's (or its civ's under the full tech tree,
    /// [`crate::world::settings::OptionFlags::FULL_TECH_TREE`], while not of mode 1) or not for
    /// this game mode; else with fewer of its requirements researched than it needs it turns to 0
    /// (from above 0), with them all to 1 (from below 1) unless its research is done already (its
    /// work done reaching a location's time), which completes it ([`World::complete_research`]).
    /// Each such technology but one completed at its building, and each in state 2, counts; none
    /// counted sets attribute 39 to 1 where it is 0. The game's lists hold the dependents by id, as
    /// the `.dat`'s requirements give them.
    pub(crate) fn update_dependents(
        &mut self,
        player_id: PlayerId,
        dependents: Dependents,
    ) -> GenieResult<()> {
        let mut counted = 0;
        let owner = self.players.player(player_id)?;
        let (civ, tree) = (owner.civ, owner.techs.len());
        let walked = match dependents {
            Dependents::Of(_) => self.model.techs.len(),
            Dependents::Every => tree,
        };
        for index in 0..walked {
            counted += self.update_dependent(player_id, civ, index, dependents)?;
        }
        if counted == 0 {
            let owner = self.players.player_mut(player_id)?;
            if let Some(amount) = owner
                .attributes
                .get_mut(ATTRIBUTE_39)
                .filter(|amount| **amount == Amount::ZERO)
            {
                *amount = Amount::new(1.0);
            }
        }
        Ok(())
    }

    /// One technology `dependent` of [`World::update_dependents`]'s walk over `dependents`, for
    /// player `player_id` of civ `civ`: how much it adds to the count.
    fn update_dependent(
        &mut self,
        player_id: PlayerId,
        civ: CivId,
        dependent: usize,
        dependents: Dependents,
    ) -> GenieResult<i32> {
        let mut counted = 0;
        let def = self.model.techs.checked(dependent)?;
        let walked = match dependents {
            Dependents::Of(tech) => def.required.contains(&Some(tech)),
            Dependents::Every => true,
        };
        if !walked || def.kind == TechKind::SHARED {
            return Ok(counted);
        }
        let Some(state) = self.players.player(player_id)?.techs.get(dependent) else {
            return Ok(counted);
        };
        match state.state {
            ResearchState::Unavailable | ResearchState::Available => {}
            ResearchState::Researching => {
                counted += 1;
                return Ok(counted);
            }
            _ => return Ok(counted),
        }
        if state.enabled {
            return Ok(counted);
        }
        if self.dependent_disabled(def, civ)? {
            self.players
                .player_mut(player_id)?
                .techs
                .checked_mut(dependent)?
                .state = ResearchState::Disabled;
            return Ok(counted);
        }
        counted += 1;
        let techs = &self.players.player(player_id)?.techs;
        let met = def
            .required
            .iter()
            .filter(|&&required| {
                required.is_some_and(|tech_id| {
                    techs
                        .get(tech_id.index())
                        .is_some_and(|tech_state| tech_state.state == ResearchState::Researched)
                })
            })
            .count() as i16;
        let state = self
            .players
            .player_mut(player_id)?
            .techs
            .checked_mut(dependent)?;
        if met < def.required_count {
            if ResearchState::Unavailable < state.state {
                state.state = ResearchState::Unavailable;
            }
            return Ok(counted);
        }
        Ok(counted + self.finish_dependent(player_id, dependent)?)
    }

    /// Whether the technology `def` is out of reach for a player of civ `civ`: another civ's (or
    /// the civ's own under the full tech tree, unless of mode 1), or for other game modes
    /// (`FUN_1413cf2f0`).
    fn dependent_disabled(&self, def: &TechDef, civ: CivId) -> GenieResult<bool> {
        // another civ's: −1; the civ's own too under the full tech tree (the option +0x63)
        // unless of mode 1
        let other_civ = match def.civ {
            None => false,
            Some(own) if own == civ => {
                self.settings.flags.contains(OptionFlags::FULL_TECH_TREE)
                    && def.mode != TechMode::KEPT_IN_FULL_TREE
            }
            Some(_) => true,
        };
        Ok(other_civ || !self.settings.tech_mode_allows(def.mode)?)
    }

    /// The end of [`World::update_dependent`] for the technology `dependent` whose requirements are
    /// all met: the change to the count.
    fn finish_dependent(&mut self, player_id: PlayerId, dependent: usize) -> GenieResult<i32> {
        let mut counted = 0;
        let state = self
            .players
            .player_mut(player_id)?
            .techs
            .checked_mut(dependent)?;
        let reached = |location: &Location| {
            let at = location
                .building
                .and_then(|master_id| state.location(master_id))
                .copied()
                .unwrap_or(*location);
            Seconds::of_master(location.time) + at.adjustment <= state.research_done
        };
        // its research done already: it completes (`FUN_1413cdcd0` with `param_3` 0 and no
        // building), once per location reached without a building of its own (each
        // uncounting it; past the first the technology is done and the call does nothing),
        // else once for its building's, which leaves it uncounted (the walk keeps the count it
        // had before it, `0x1413cd3c8`)
        if let Some(building_master) = state.building_master {
            if state.location(building_master).is_some_and(reached) {
                let _ = self.complete_research(player_id, TechId::of_index(dependent)?, None)?;
                counted -= 1;
                return Ok(counted);
            }
        } else {
            let reached = state
                .locations
                .iter()
                .filter(|location| reached(location))
                .count();
            if 0 < reached {
                for _ in 0..reached {
                    let _ =
                        self.complete_research(player_id, TechId::of_index(dependent)?, None)?;
                    counted -= 1;
                }
                return Ok(counted);
            }
        }
        let state = self
            .players
            .player_mut(player_id)?
            .techs
            .checked_mut(dependent)?;
        if state.state < ResearchState::Available {
            state.state = ResearchState::Available;
        }
        Ok(counted)
    }

    /// The cost modifier (`'e'`, `FUN_1413d10f0`) of player `player_id`'s technology `tech`: each
    /// of its `.dat` cost entries that is paid and takes `resource` (any for −1) changes its
    /// adjustment (`+0x6`): mode 2 adds `(amount − 1) × the .dat's amount`, rounded half away from
    /// 1; mode 0 sets it so the cost is `amount`; any other adds `amount`. A named resource changes
    /// only its first entry. A technology out of the tree is let be.
    pub(super) fn tech_cost(
        &mut self,
        player_id: PlayerId,
        tech: TechId,
        resource: Option<AttributeId>,
        amount: f32,
        mode: ChangeMode,
    ) -> GenieResult<()> {
        let tree = self
            .players
            .player(player_id)?
            .techs
            .len()
            .min(self.model.techs.len());
        let Some(tech_index) = Some(tech.index()).filter(|&tech_index| tech_index < tree) else {
            return Ok(());
        };
        let costs = self.model.techs.checked(tech_index)?.costs;
        let adjustments = &mut self
            .players
            .player_mut(player_id)?
            .techs
            .checked_mut(tech_index)?
            .cost_adj;
        for (cost, adj) in costs.iter().zip(adjustments.iter_mut()) {
            if !cost.paid.is_paid()
                || resource.is_some_and(|attribute_id| cost.attribute != Some(attribute_id))
            {
                continue;
            }
            *adj = match mode {
                ChangeMode::Scale => {
                    let half = if amount < 1.0 { -0.5 } else { 0.5 };
                    let delta = (amount - 1.0) * cost.amount.as_f32() + half;
                    adj.plus(cvttss2si(delta) as i16)
                }
                // CVTTSS2SI's 32 bits, the low word added (0x1413d11a6, 0x1413d11be)
                ChangeMode::Set => {
                    CostAmount::of_data((cvttss2si(amount) as i16).wrapping_sub(cost.amount.get()))
                }
                _ => adj.plus(cvttss2si(amount) as i16),
            };
            if resource.is_some() {
                return Ok(());
            }
        }
        Ok(())
    }

    /// The disable command (`'f'`, `FUN_14163a500` with `param_3` 0) of player `player`'s
    /// technology `tech`: one of the shared kind (`FUN_1413d1020`) is a building's own, which a
    /// command without a building as its object lets be (with one, `FUN_1411c49a0` files it in the
    /// building's list `+0xd00`, which the slice does not hold); any other goes through
    /// `FUN_1413cdb00`: a technology of type 0x21 turns to 1 (it may be researched), any other but
    /// a researched one (state 3) to −1; a negative id does so to every technology.
    pub(super) fn disable_tech(
        &mut self,
        player: PlayerId,
        tech: DisabledTech,
        object: Option<ObjectId>,
    ) -> GenieResult<()> {
        let count = self
            .players
            .player(player)?
            .techs
            .len()
            .min(self.model.techs.len());
        let held = tech
            .whole()
            .map(TechId::index)
            .filter(|&tech_index| tech_index < count);
        if held.is_some_and(|tech_index| {
            self.model
                .techs
                .get(tech_index)
                .is_some_and(|tech_def| tech_def.kind == TechKind::SHARED)
        }) {
            let building = match object.and_then(|id| self.object(id)) {
                Some((player_id, unit)) => {
                    unit.master(self.master_tables(), player_id)?.kind == MasterKind::BUILDING
                }
                None => false,
            };
            if building {
                return Err(SHARED_RESEARCH);
            }
            return Ok(());
        }
        let disable = |defs: &[TechDef], states: &mut [TechState], tech_index: usize| {
            let (Some(def), Some(state)) = (defs.get(tech_index), states.get_mut(tech_index))
            else {
                return;
            };
            if def.kind == TechKind::REENABLED {
                state.state = ResearchState::Available;
            // A researched technology's state, which a disable command keeps (`FUN_1413cdb00`).
            } else if state.state != ResearchState::Researched {
                state.state = ResearchState::Disabled;
            }
        };
        // FUN_1413cdb00 takes the id's low 16 bits (MOVZX EDX, BX in FUN_14163a500)
        let short = tech.short();
        let states = &mut self.players.player_mut(player)?.techs;
        match usize::try_from(short)
            .ok()
            .filter(|&tech_index| tech_index < count)
        {
            Some(tech_index) => disable(&self.model.techs, states, tech_index),
            None if short < 0 => {
                for index in 0..count {
                    disable(&self.model.techs, states, index);
                }
            }
            None => {}
        }
        Ok(())
    }

    /// The enable command (`FUN_1416665b0`) of player `player` on its master `master` with `enable`
    /// (the command's low byte): a master the player does not have is let be (its id check
    /// against `+0x260` is taken to hold). Without `one` building (a third argument of 1) and
    /// its object, the master's slot 0x58 (`FUN_141126280`) sets `available` unless it is
    /// `disabled` (the byte, held as its test against 0). The sprites' loading on an enable
    /// (slot 0xa8) is not simulation. The enabling of master `0x26d` does nothing in game mode
    /// 11; not modelled: its enabling in another mode (it asks a settings flag) and a command for
    /// one building (`FUN_1411c69f0`, `FUN_1411c6a50`: the building's list, not held).
    pub(super) fn enable_master(
        &mut self,
        player: PlayerId,
        master: MasterId,
        enable: u8,
        one: bool,
        object: Option<ObjectId>,
    ) -> GenieResult<()> {
        if player_master(&self.model.masters, player, master).is_none() {
            return Ok(());
        }
        if enable == 1
            && self
                .model
                .roles
                .of_master(master)
                .contains(MasterRoles::MODE_TESTED)
        {
            // game mode 11 never enables it; any other asks a settings flag (their `+0x18`'s
            // `+0x7c`, bit 1, FUN_141324b60), which is not held
            if self.settings.game_mode == GameMode::SuddenDeath {
                return Ok(());
            }
            return Err(GenieError::unsupported(
                "the enabling of master 0x26d (the game's mode)",
            ));
        }
        if let (true, Some(id)) = (one, object) {
            let building = match self.object(id) {
                Some((player_id, unit)) => {
                    unit.master(self.master_tables(), player_id)?.kind == MasterKind::BUILDING
                }
                None => false,
            };
            if building {
                return Err(GenieError::unsupported(
                    "an enable command for one building",
                ));
            }
            return Ok(());
        }
        let master = Arc::make_mut(self.model.masters.checked_mut(player.index())?)
            .get_mut(master)
            .ok_or(NO_MASTER)?;
        if !master.disabled.is_disabled() {
            master.available = Availability::of_data(enable);
        }
        Ok(())
    }
}
