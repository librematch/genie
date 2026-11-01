//! A research's completion (`FUN_1413cdcd0`): its effect, the technologies that depend on
//! it, a built building's technologies, the cost modifier and the disable and enable commands.

use crate::GenieError;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::MasterId;
use crate::objects::masters::{master_type, player_master};
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::players::attribute::RESEARCH_SPENT;
use crate::players::attribute::TECHS_RESEARCHED;
use crate::production::LocationButton;
use crate::technology::ChangeMode;
use crate::technology::DisabledTech;
use crate::technology::EffectId;
use crate::technology::ResearchState;
use crate::technology::TechId;
use crate::technology::effect::{ATTRIBUTE_39, MODE_TESTED_MASTER, MODE_WITHOUT_0X26D};
use crate::technology::{EffectCommand, ObjectsKind, Party};
use crate::world::Seconds;
use crate::world::World;
use crate::world::slots::{CheckedIndex, PlayerSlots};

/// An enable command finds the master it tested gone.
const NO_MASTER: GenieError =
    GenieError::unreachable("an enable command for a master the player lacks");

impl World {
    /// `FUN_1413cdcd0` for player `p`'s technology `t` (`param_3` 0) researched at a building of
    /// master `building`: a technology not in state 0 to 2 is not done (the game's overrides
    /// are not held). Else it takes state 3, gives its effect (its override's, else its own;
    /// [`World::do_effect`]) and updates the technologies that depend on it
    /// ([`World::update_dependents`]); researched at a location with a button, it counts in
    /// attribute 0x15 and its listed costs (`FUN_1413d4a00`, the amounts summed) in attribute
    /// 0x63. Last, an age (101 to 103) adds its event to the player's history (`FUN_141560ae0`
    /// with 0, 1 or 2, [`crate::players::history::History::add_event`]), which the AI's age time reads.
    /// Not held: the local player's statistics and achievements (`FUN_1409644a0` among them).
    /// Not modelled: a research paid in relics (`FUN_1413d0e70`), an effect the slice cannot run without its
    /// building ([`World::research_effect_known`]) and the civ's disabled technologies
    /// (`FUN_140ea93a0`; such a technology is never available to be researched).
    pub(crate) fn complete_research(
        &mut self,
        p: PlayerId,
        t: TechId,
        building: Option<MasterId>,
    ) -> Result<bool, GenieError> {
        use crate::technology::Attributes;
        let (tech, t) = (t, t.index());
        let def = self.techs.checked(t)?.clone();
        let state = &self.players.player(p)?.techs.checked(t)?;
        if !(ResearchState::Unavailable..=ResearchState::Researching).contains(&state.state) {
            return Ok(false);
        }
        if def.costs[0].attribute == Some(crate::players::attribute::RELICS_HELD) {
            return Err(GenieError::unsupported(
                "a research paid in relics (FUN_1413d0e70)",
            ));
        }
        self.research_effect_known(p, tech)?;
        self.players.player_mut(p)?.techs.checked_mut(t)?.state = ResearchState::Researched;
        self.do_effect(p, &def.effect, None)?;
        self.update_dependents(p, tech)?;
        let owner = &self.players.player(p)?;
        let state = owner.techs.checked(t)?;
        let location = building.map_or_else(|| state.locations.first(), |b| state.location(b));
        if location.is_some_and(|l| l.building.is_some() && l.button != LocationButton::NONE) {
            let get = |k: AttributeId| owner.get(k);
            let factors = crate::technology::factors(tech, &get)?;
            let listed = state.listed_costs(&def, factors)?;
            let spent = listed.iter().map(|&(_, a)| i32::from(a)).sum::<i32>() as f32;
            let owner = self.players.player_mut(p)?;
            owner.add_attribute(TECHS_RESEARCHED, Amount::new(1.0))?;
            owner.add_attribute(RESEARCH_SPENT, Amount::new(spent))?;
        }
        let age = match t {
            101 => Some(crate::players::history::EventKind::FEUDAL_AGE),
            102 => Some(crate::players::history::EventKind::CASTLE_AGE),
            103 => Some(crate::players::history::EventKind::IMPERIAL_AGE),
            _ => None,
        };
        if let Some(kind) = age {
            let now = self.world_time;
            self.players
                .player_mut(p)?
                .history
                .add_event(kind, 0.0, 0.0, 0, now);
        }
        Ok(true)
    }

    /// Whether player `p`'s technology `t` gives an effect this file runs (`FUN_1413ce9b0`): its
    /// override's when it has one (the world's effect table `+0x128`, not held: only its own
    /// effect is known), with the research's building as the effect's object (`FUN_1413cdcd0`'s
    /// `param_4`, which the callers do not pass yet): an effect that reaches its object (an
    /// enable for one building, an upgrade of the object, the object's kinds 203 to 206, a
    /// disable of a building's own technology) would differ without it.
    pub(super) fn research_effect_known(&self, p: PlayerId, t: TechId) -> Result<(), GenieError> {
        let t = t.index();
        let def = self.techs.checked(t)?;
        let state = &self.players.player(p)?.techs.checked(t)?;
        let shared = |tech: DisabledTech| {
            // FUN_1413d1020 tests the whole id
            tech.whole()
                .and_then(|d| self.techs.get(d.index()))
                .is_some_and(|d| d.kind == crate::technology::TechType::SHARED)
        };
        // the player's own enable for one building and upgrade of the object (kinds 2 and 3),
        // the disable of a shared technology, the object's kinds 203 to 206
        let reaches_object = def.effect.iter().any(|c| match *c {
            EffectCommand::Enable {
                party: Party::Own,
                one,
                ..
            } => one,
            EffectCommand::Upgrade {
                party: Party::Own,
                mode,
                ..
            } => mode == 2,
            EffectCommand::TechDisable { tech } => shared(tech),
            EffectCommand::Objects(kind) => {
                matches!(kind, ObjectsKind::Object(_) | ObjectsKind::ObjectMaster)
            }
            _ => false,
        });
        if state.effect_override != EffectId::NONE && state.effect_override != def.effect_id {
            return Err(GenieError::unsupported("a technology's effect override"));
        }
        if reaches_object {
            return Err(GenieError::unsupported(
                "a research's effect on its building (FUN_1413cdcd0's param_4, not passed)",
            ));
        }
        Ok(())
    }

    /// A completed building's technology `t` for player `p` (the end of `FUN_1411dbb70`'s
    /// `set_state(2)`): [`World::complete_research`] with `param_3` 0 and no building, so the
    /// technology's first location decides its statistics; a technology beyond the player's tree
    /// is not researched.
    pub(crate) fn research_built(&mut self, p: PlayerId, t: TechId) -> Result<(), GenieError> {
        let tree = self.players.player(p)?.techs.len();
        if tree == 0 || self.techs.len() != tree {
            return Err(GenieError::unsupported(
                "a completion's technology without the player's tech tree",
            ));
        }
        if t.index() < tree {
            let _ = self.complete_research(p, t, None)?;
        }
        Ok(())
    }

    /// After a built building of `master` died (`FUN_1411dbb70`): unless another of player `p`'s
    /// live objects of the master stands (state 2, `FUN_141166ab0`), the master's technology
    /// `tech` in a state above 0 turns to 0 and its dependents are updated (`FUN_1413d4ec0`, then
    /// `FUN_1413cce70`, [`World::update_dependents`]).
    pub(crate) fn withdraw_tech(
        &mut self,
        p: PlayerId,
        master: MasterId,
        tech: TechId,
    ) -> Result<(), GenieError> {
        let owner = &self.players.player(p)?;
        if owner
            .objects
            .iter()
            .any(|u| u.life.master_id == master && u.life.state == ObjectState::Alive)
        {
            return Ok(());
        }
        let t = tech.index();
        if owner.techs.len() <= t {
            return Ok(());
        }
        if owner.techs.checked(t)?.state <= ResearchState::Unavailable {
            return Ok(());
        }
        self.players.player_mut(p)?.techs.checked_mut(t)?.state = ResearchState::Unavailable;
        self.update_dependents(p, tech)
    }

    /// `FUN_1413cce70` for player `p`'s technology `t`: each technology that requires it, by id
    /// (`+0x38`'s lists), not of the shared kind: one in state 0 or 1 and not held by
    /// `+0x60` turns to −1 when it is another civ's (or its civ's under the full tech tree,
    /// [`crate::world::settings::Settings::full_tech_tree`], while not of mode 1) or not for this game
    /// mode; else with fewer of
    /// its requirements researched than it needs it turns to 0 (from above 0), with them all to
    /// 1 (from below 1) unless its research is done already (its work done reaching a
    /// location's time), which completes it ([`World::complete_research`]). Each such technology, and each in
    /// state 2, counts; none counted sets attribute 39 to 1 where it is 0. The game's lists hold
    /// the dependents by id, as the `.dat`'s requirements give them.
    pub(crate) fn update_dependents(
        &mut self,
        p: PlayerId,
        tech: TechId,
    ) -> Result<(), GenieError> {
        let mut counted = 0;
        let civ = self.players.player(p)?.civ;
        for d in 0..self.techs.len() {
            counted += self.update_dependent(p, civ, (d, tech))?;
        }
        if counted == 0 {
            let owner = self.players.player_mut(p)?;
            if let Some(v) = owner
                .attributes
                .get_mut(ATTRIBUTE_39)
                .filter(|v| **v == Amount::ZERO)
            {
                *v = Amount::new(1.0);
            }
        }
        Ok(())
    }

    /// One technology `d` of [`World::update_dependents`]'s walk, for player `p` of civ `civ`
    /// and the changed technology `t`: how much it adds to the count.
    fn update_dependent(
        &mut self,
        p: PlayerId,
        civ: crate::players::CivId,
        (d, t): (usize, TechId),
    ) -> Result<i32, GenieError> {
        let mut counted = 0;
        let def = self.techs.checked(d)?;
        if !def.required.contains(&Some(t)) || def.kind == crate::technology::TechType::SHARED {
            return Ok(counted);
        }
        let Some(state) = self.players.player(p)?.techs.get(d) else {
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
            self.players.player_mut(p)?.techs.checked_mut(d)?.state = ResearchState::Disabled;
            return Ok(counted);
        }
        counted += 1;
        let techs = &self.players.player(p)?.techs;
        let met = def
            .required
            .iter()
            .filter(|&&r| {
                r.is_some_and(|r| {
                    techs
                        .get(r.index())
                        .is_some_and(|s| s.state == ResearchState::Researched)
                })
            })
            .count() as i16;
        let state = self.players.player_mut(p)?.techs.checked_mut(d)?;
        if met < def.required_count {
            if ResearchState::Unavailable < state.state {
                state.state = ResearchState::Unavailable;
            }
            return Ok(counted);
        }
        Ok(counted + self.finish_dependent(p, d)?)
    }

    /// Whether the technology `def` is out of reach for a player of civ `civ`: another civ's (or
    /// the civ's own under the full tech tree, unless of mode 1), or for other game modes
    /// (`FUN_1413cf2f0`).
    fn dependent_disabled(
        &self,
        def: &crate::technology::TechDef,
        civ: crate::players::CivId,
    ) -> Result<bool, GenieError> {
        // another civ's: −1; the civ's own too under the full tech tree (the option +0x63)
        // unless of mode 1
        let other_civ = match def.civ {
            None => false,
            Some(own) if own == civ => self.settings.full_tech_tree && def.mode != 1,
            Some(_) => true,
        };
        Ok(other_civ || !self.settings.tech_mode_allows(def.mode)?)
    }

    /// The end of [`World::update_dependent`] for the technology `d` whose requirements are all
    /// met: the change to the count.
    fn finish_dependent(&mut self, p: PlayerId, d: usize) -> Result<i32, GenieError> {
        let mut counted = 0;
        let state = self.players.player_mut(p)?.techs.checked_mut(d)?;
        let reached = |l: &crate::technology::Location| {
            let at = l
                .building
                .and_then(|b| state.location(b))
                .copied()
                .unwrap_or(*l);
            Seconds::of_master(l.time) + at.adjustment <= state.research_done
        };
        // its research done already: it completes (`FUN_1413cdcd0` with `param_3` 0 and no
        // building), once per location reached without a building of its own (each
        // uncounting it; past the first the technology is done and the call does nothing),
        // else once for its building's
        if let Some(building_master) = state.building_master {
            if state.location(building_master).is_some_and(reached) {
                let _ = self.complete_research(p, TechId::of_index(d)?, None)?;
                return Ok(counted);
            }
        } else {
            let reached = state.locations.iter().filter(|l| reached(l)).count();
            if 0 < reached {
                for _ in 0..reached {
                    let _ = self.complete_research(p, TechId::of_index(d)?, None)?;
                    counted -= 1;
                }
                return Ok(counted);
            }
        }
        let state = self.players.player_mut(p)?.techs.checked_mut(d)?;
        if state.state < ResearchState::Available {
            state.state = ResearchState::Available;
        }
        Ok(counted)
    }

    /// The cost modifier (`'e'`, `FUN_1413d10f0`) of player `p`'s technology `tech`: each of its
    /// `.dat` cost entries that is paid and takes `resource` (any for −1) changes its adjustment
    /// (`+0x6`): mode 2 adds `(amount − 1) × the .dat's amount`, rounded half away from 1; mode
    /// 0 sets it so the cost is `amount`; any other adds `amount`. A named resource changes only
    /// its first entry. A technology out of the tree is let be.
    pub(super) fn tech_cost(
        &mut self,
        p: PlayerId,
        tech: TechId,
        resource: Option<AttributeId>,
        amount: f32,
        mode: ChangeMode,
    ) -> Result<(), GenieError> {
        let tree = self.players.player(p)?.techs.len().min(self.techs.len());
        let Some(t) = Some(tech.index()).filter(|&t| t < tree) else {
            return Ok(());
        };
        let costs = self.techs.checked(t)?.costs;
        let adjustments = &mut self.players.player_mut(p)?.techs.checked_mut(t)?.cost_adj;
        for (cost, adj) in costs.iter().zip(adjustments.iter_mut()) {
            if !cost.paid.is_paid() || resource.is_some_and(|r| cost.attribute != Some(r)) {
                continue;
            }
            *adj = match mode {
                ChangeMode::SCALE => {
                    let half = if amount < 1.0 { -0.5 } else { 0.5 };
                    let delta = (amount - 1.0) * f32::from(cost.amount) + half;
                    adj.wrapping_add(crate::math::conversions::cvttss2si(delta) as i16)
                }
                // CVTTSS2SI's 32 bits, the low word added (0x1413d11a6, 0x1413d11be)
                ChangeMode::SET => {
                    (crate::math::conversions::cvttss2si(amount) as i16).wrapping_sub(cost.amount)
                }
                _ => adj.wrapping_add(crate::math::conversions::cvttss2si(amount) as i16),
            };
            if resource.is_some() {
                return Ok(());
            }
        }
        Ok(())
    }

    /// The disable command (`'f'`, `FUN_14163a500` with `param_3` 0) of player `p`'s technology
    /// `tech`: one of the shared kind (`FUN_1413d1020`) is a building's own, which a command
    /// without a building as its object lets be (with one, `FUN_1411c49a0` files it in the
    /// building's list `+0xd00`, which the slice does not hold); any other goes through
    /// `FUN_1413cdb00`: a technology of type 0x21 turns to 1 (it may be researched), any other
    /// but a researched one (state 3) to −1; a negative id does so to every technology.
    pub(super) fn disable_tech(
        &mut self,
        p: PlayerId,
        tech: DisabledTech,
        object: Option<ObjectId>,
    ) -> Result<(), GenieError> {
        let count = self.players.player(p)?.techs.len().min(self.techs.len());
        let held = tech.whole().map(TechId::index).filter(|&t| t < count);
        if held.is_some_and(|t| {
            self.techs
                .get(t)
                .is_some_and(|d| d.kind == crate::technology::TechType::SHARED)
        }) {
            let building = match object.and_then(|id| self.object(id)) {
                Some((q, u)) => u.master(&self.masters, q)?.kind == master_type::BUILDING,
                None => false,
            };
            if building {
                return Err(GenieError::unsupported(
                    "a research of the shared kind (FUN_1411c49a0)",
                ));
            }
            return Ok(());
        }
        let disable = |defs: &[crate::technology::TechDef],
                       states: &mut [crate::technology::TechState],
                       t: usize| {
            let (Some(def), Some(state)) = (defs.get(t), states.get_mut(t)) else {
                return;
            };
            if def.kind == crate::technology::TechType::REENABLED {
                state.state = ResearchState::Available;
            // A researched technology's state, which a disable command keeps (`FUN_1413cdb00`).
            } else if state.state != ResearchState::Researched {
                state.state = ResearchState::Disabled;
            }
        };
        // FUN_1413cdb00 takes the id's low 16 bits (MOVZX EDX, BX in FUN_14163a500)
        let short = tech.short();
        let states = &mut self.players.player_mut(p)?.techs;
        match usize::try_from(short).ok().filter(|&t| t < count) {
            Some(t) => disable(&self.techs, states, t),
            None if short < 0 => {
                for t in 0..count {
                    disable(&self.techs, states, t);
                }
            }
            None => {}
        }
        Ok(())
    }

    /// The enable command (`FUN_1416665b0`) of player `p` on its master `master` with `enable`
    /// (the command's low byte): a master the player does not have is let be (its id check
    /// against `+0x260` is taken to hold). Without `one` building (a third argument of 1) and
    /// its object, the master's slot 0x58 (`FUN_141126280`) sets `available` unless it is
    /// `disabled` (the byte, held as its test against 0). The sprites' loading on an enable
    /// (slot 0xa8) is not simulation. The enabling of master `0x26d` does nothing in game mode
    /// 11; not modelled: its enabling in another mode (it asks a settings flag) and a command for
    /// one building (`FUN_1411c69f0`, `FUN_1411c6a50`: the building's list, not held).
    pub(super) fn enable_master(
        &mut self,
        p: PlayerId,
        master: MasterId,
        enable: u8,
        one: bool,
        object: Option<ObjectId>,
    ) -> Result<(), GenieError> {
        if player_master(&self.masters, p, master).is_none() {
            return Ok(());
        }
        if enable == 1 && master == MODE_TESTED_MASTER {
            // game mode 11 never enables it; any other asks a settings flag (their `+0x18`'s
            // `+0x7c`, bit 1, FUN_141324b60), which is not held
            if self.settings.game_mode == MODE_WITHOUT_0X26D {
                return Ok(());
            }
            return Err(GenieError::unsupported(
                "the enabling of master 0x26d (the game's mode)",
            ));
        }
        if let (true, Some(id)) = (one, object) {
            let building = match self.object(id) {
                Some((q, u)) => u.master(&self.masters, q)?.kind == master_type::BUILDING,
                None => false,
            };
            if building {
                return Err(GenieError::unsupported(
                    "an enable command for one building",
                ));
            }
            return Ok(());
        }
        let m = self
            .masters
            .checked_mut(p.index())?
            .get_mut(master)
            .ok_or(NO_MASTER)?;
        if !m.disabled {
            m.available = enable != 0;
        }
        Ok(())
    }
}
