//! A technology's change (kind 8): its state, a forced research and its research time.

use crate::GenieError;
use crate::math::conversions::cvttss2si;
use crate::math::fmath::below_or_unordered;
use crate::math::image::effect::{
    NO_TIME as EFFECT_NO_TIME, STATE_AVAILABLE as EFFECT_STATE_AVAILABLE,
    STATE_RESEARCHED as EFFECT_STATE_RESEARCHED, STATE_UNAVAILABLE as EFFECT_STATE_UNAVAILABLE,
};
use crate::objects::masters::MasterId;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::production::LocationButton;
use crate::technology::ChangeMode;
use crate::technology::EffectId;
use crate::technology::ModifyLocation;
use crate::technology::ModifyMode;
use crate::technology::ResearchState;
use crate::technology::TechId;
use crate::technology::effect::{ONE, ROUND};
use crate::technology::{Attributes, Location, ResearchCap, factors};
use crate::world::Seconds;
use crate::world::World;
use crate::world::slots::{CheckedIndex, PlayerSlots};

/// The modes of `FUN_1415c3f20`'s technology change (kind 8) that change a cost, with the
/// resource and the cost modifier's mode (`DAT_142ed71b0`'s map: 0 to 3 set food, wood, stone,
/// gold; 0x4000 to 0x4003 add to them; 0xd to 0x10 multiply them, 0x11 every one).
pub(super) const fn tech_cost_mode(mode: ModifyMode) -> Option<(Option<AttributeId>, ChangeMode)> {
    let mode = mode.get();
    match mode {
        0..=3 => Some((Some(AttributeId::named(mode)), ChangeMode::Set)),
        0x4000..=0x4003 => Some((Some(AttributeId::named(mode - 0x4000)), ChangeMode::Add)),
        0xd..=0x10 => Some((Some(AttributeId::named(mode - 0xd)), ChangeMode::Scale)),
        0x11 => Some((None, ChangeMode::Scale)),
        _ => None,
    }
}

/// The modes that change a research time (`DAT_142f8f340`'s map: −1 sets, −2 adds, −3
/// multiplies).
pub(super) const fn tech_time_mode(mode: ModifyMode) -> Option<ChangeMode> {
    match mode.get() {
        -1 => Some(ChangeMode::Set),
        -2 => Some(ChangeMode::Add),
        -3 => Some(ChangeMode::Scale),
        _ => None,
    }
}

/// 1.0, 2.0 and 3.0 (`DAT_14330eb8c`, `DAT_142e8c150`, `DAT_142eb1c98`): a state change's values
/// (kind 8's mode 12): 0 disables, 1 makes it unavailable, 2 available and enabled, 3 researched.
pub(super) const STATE_UNAVAILABLE: f32 = EFFECT_STATE_UNAVAILABLE;
pub(super) const STATE_AVAILABLE: f32 = EFFECT_STATE_AVAILABLE;
pub(super) const STATE_RESEARCHED: f32 = EFFECT_STATE_RESEARCHED;
/// −1.0 (`DAT_14330ec20`): a research time without a `.dat` location, and a set's "none".
pub(super) const NO_TIME: f32 = EFFECT_NO_TIME;

/// A technology location a modify command names: its index and its building.
type LocationAt = (Option<usize>, Option<MasterId>);

impl World {
    /// `FUN_1415c3f20`'s technology change (kind 8, its `0x6f` case) for player `p`'s technology
    /// `tech` by `mode` and `v` at location `location` (below 1: every location; past the
    /// technology's locations a new one is added, a copy of the last with no building): a cost
    /// mode changes a cost ([`World::tech_cost`]); 4 sets the location's building to a master the
    /// player has, 5 its button; 9 the stacking, 10 the research cap; 12 the state; 18 the
    /// effect override (an already researched technology gives its new effect at once,
    /// `FUN_1413d4320`); the research time modes ([`tech_time_mode`]) change
    /// the time ([`World::research_time`]) and update the dependents. The icon (6), name (7),
    /// description (8) and hotkey (11) overrides are the interface's. A technology past the
    /// player's tree changes nothing (the game also reads its one past the end, not modelled).
    pub(super) fn modify_tech(
        &mut self,
        p: PlayerId,
        tech: TechId,
        mode: ModifyMode,
        v: f32,
        location: ModifyLocation,
    ) -> Result<(), GenieError> {
        let tree = self.players.player(p)?.techs.len();
        let Some(tech_id) = Some(tech).filter(|t| t.index() < tree) else {
            return Ok(());
        };
        let Some((at, building)) = self.tech_location(p, tech_id, location)? else {
            return Ok(());
        };
        if let Some((resource, cost_mode)) = tech_cost_mode(mode) {
            self.tech_cost(p, tech, resource, v, cost_mode)?;
            return Ok(());
        }
        if let Some(time_mode) = tech_time_mode(mode) {
            self.research_time(p, tech, v, time_mode, building)?;
            return self.update_dependents(p, tech_id);
        }
        self.set_tech_field(p, tech_id, (mode, v), at)
    }

    /// The location a modify command names of player `p`'s technology `t` (`location` above 0:
    /// the entry at that index, or a copy of the last one with no building): its index and its
    /// building. `None` for a technology without locations to copy.
    fn tech_location(
        &mut self,
        p: PlayerId,
        t: TechId,
        location: ModifyLocation,
    ) -> Result<Option<LocationAt>, GenieError> {
        let t = t.index();
        let (mut at, mut building) = (None, None);
        if let Some(i) = location.entry() {
            let state = self.players.player_mut(p)?.techs.checked_mut(t)?;
            if i < state.locations.len() {
                building = state.locations.checked(i)?.building;
                at = Some(i);
            } else {
                // the game copies the last entry; a technology without locations would read before
                // an empty list's start (a crash no recording shows)
                let Some(&last) = state.locations.last() else {
                    return Ok(None);
                };
                at = Some(state.locations.len());
                state.locations.push(Location {
                    building: None,
                    ..last
                });
            }
        }
        Ok(Some((at, building)))
    }

    /// The modify command's plain fields of player `p`'s technology `t` (`mode` 4 the building, 5
    /// the button, 9 the stacking, 10 the research cap, 12 the state, 18 the effect override,
    /// `FUN_1413d4320`),
    /// those of a location at `at` only (all without one).
    fn set_tech_field(
        &mut self,
        p: PlayerId,
        t: TechId,
        (mode, v): (ModifyMode, f32),
        at: Option<usize>,
    ) -> Result<(), GenieError> {
        let (tech, t) = (t, t.index());
        let short = cvttss2si(v) as i16;
        match mode {
            ModifyMode::Building => self.set_tech_building(p, tech, v, at)?,
            ModifyMode::Button => {
                let state = self.players.player_mut(p)?.techs.checked_mut(t)?;
                for (i, l) in state.locations.iter_mut().enumerate() {
                    if at.is_none_or(|a| a == i) {
                        l.button = LocationButton::of_data(cvttss2si(v) as u8);
                    }
                }
            }
            ModifyMode::Stacking => {
                self.players.player_mut(p)?.techs.checked_mut(t)?.stack = v != 0.0;
            }
            ModifyMode::ResearchCap => {
                self.players
                    .player_mut(p)?
                    .techs
                    .checked_mut(t)?
                    .research_cap = ResearchCap::of_data(short);
            }
            ModifyMode::State => self.set_tech_state(p, tech, v)?,
            ModifyMode::EffectOverride => {
                // `FUN_1413d4320`: a researched technology gives its new effect at once, with no
                // object
                let state = self.players.player_mut(p)?.techs.checked_mut(t)?;
                state.effect_override = EffectId::of_data(short);
                if state.state == ResearchState::Researched {
                    let effect = state.effect_override;
                    self.do_table_effect(p, effect, None)?;
                }
            }
            ModifyMode::Other(_) => {}
        }
        Ok(())
    }

    /// Mode 4: the building of player `p`'s technology `t`'s locations (those of `at`, all
    /// without one), when `v` names a master.
    fn set_tech_building(
        &mut self,
        p: PlayerId,
        t: TechId,
        v: f32,
        at: Option<usize>,
    ) -> Result<(), GenieError> {
        let t = t.index();
        let short = MasterId::of_data(cvttss2si(v) as i16);
        if let Some(short) = short
            && self
                .model
                .masters
                .get(p.index())
                .is_some_and(|m| m.get(short).is_some())
        {
            let state = self.players.player_mut(p)?.techs.checked_mut(t)?;
            for (i, l) in state.locations.iter_mut().enumerate() {
                if at.is_none_or(|a| a == i) {
                    l.building = Some(short);
                }
            }
        }
        Ok(())
    }

    /// Mode 12: the state of player `p`'s technology `t` (`v` 0 none, then unavailable,
    /// available, or researched, which researches it).
    fn set_tech_state(&mut self, p: PlayerId, t: TechId, v: f32) -> Result<(), GenieError> {
        let (tech, t) = (t, t.index());
        let state = self.players.player_mut(p)?.techs.checked_mut(t)?;
        if v == 0.0 {
            state.state = ResearchState::Disabled;
        } else if v == STATE_UNAVAILABLE {
            state.state = ResearchState::Unavailable;
        } else if v == STATE_AVAILABLE {
            state.state = ResearchState::Available;
            state.enabled = true;
        } else if v == STATE_RESEARCHED {
            let _ = self.force_research(p, tech)?;
        }
        Ok(())
    }

    /// `FUN_1413cdcd0` with `param_3` 1 (kind 8's state 3) for player `p`'s technology `t`: one in
    /// state 0 to 2 completes as [`World::complete_research`] (the civ's disabled technologies are
    /// not asked); any other gives its effect again and counts in attributes 0x15 and 0x63 (its
    /// listed costs) whatever its location, its state left.
    pub(super) fn force_research(&mut self, p: PlayerId, t: TechId) -> Result<bool, GenieError> {
        let (tech, t) = (t, t.index());
        if (ResearchState::Unavailable..=ResearchState::Researching)
            .contains(&self.players.player(p)?.techs.checked(t)?.state)
        {
            return self.complete_research(p, tech, None);
        }
        let def = self.model.techs.checked(t)?.clone();
        self.research_effect_known(p, tech)?;
        let effect = self.players.player(p)?.techs.checked(t)?.effect(&def);
        self.do_table_effect(p, effect, None)?;
        let owner = self.players.player(p)?;
        let get = |k: AttributeId| Attributes::get(owner, k);
        let factors = factors(tech, &get, self.model.slots)?;
        let listed = owner.techs.checked(t)?.listed_costs(&def, factors)?;
        let spent = listed.iter().map(|&(_, a)| i32::from(a.get())).sum::<i32>() as f32;
        let owner = self.players.player_mut(p)?;
        owner.add_attribute(
            self.model.slots.attribute.techs_researched,
            Amount::new(1.0),
        )?;
        owner.add_attribute(
            self.model.slots.attribute.research_spent,
            Amount::new(spent),
        )?;
        Ok(true)
    }

    /// The research time modifier (`'g'`, `FUN_1413d1210`) of player `p`'s technology `tech` by
    /// `v` in `mode`: each location (`location` −1; else its location of that building,
    /// `FUN_1413c8930`) changes its adjustment (`+0xc`) against the `.dat`'s time there (−1
    /// without one): mode 2 adds `(v − 1) × time` rounded half away from 1 (in 16 bits), mode 0
    /// sets it so the time is `v` (none for −1, which leaves 0), any other adds `v` (in 16 bits).
    /// A technology in state 1 whose work done reaches its new time completes
    /// ([`World::complete_research`]), but for a set to −1. The game walks a copy of the
    /// locations.
    pub(super) fn research_time(
        &mut self,
        p: PlayerId,
        tech: TechId,
        v: f32,
        mode: ChangeMode,
        location: Option<MasterId>,
    ) -> Result<(), GenieError> {
        let tree = self.players.player(p)?.techs.len();
        let Some(tech_id) = Some(tech).filter(|t| t.index() < tree) else {
            return Ok(());
        };
        let t = tech_id.index();
        let buildings: Vec<Option<MasterId>> = if location.is_none() {
            self.players
                .player(p)?
                .techs
                .checked(t)?
                .locations
                .iter()
                .map(|l| l.building)
                .collect()
        } else {
            vec![location]
        };
        for building in buildings {
            let time = self
                .model
                .techs
                .get(t)
                .and_then(|d| d.locations.iter().find(|&&(b, _)| b == building))
                .map_or(NO_TIME, |&(_, time)| Seconds::of_master(time).get());
            let state = self.players.player_mut(p)?.techs.checked_mut(t)?;
            let i = state
                .locations
                .iter()
                .position(|l| l.building == building)
                .unwrap_or(0);
            // a building comes from the locations (kind 8 adds one first), so only an empty list
            // finds none, which the game's walk of the copy never reaches
            let Some(entry) = state.locations.get_mut(i) else {
                continue;
            };
            let short = |x: f32| f32::from(cvttss2si(x) as i16);
            let adjustment = match mode {
                ChangeMode::Scale => {
                    let half = if below_or_unordered(v, ONE) {
                        -ROUND
                    } else {
                        ROUND
                    };
                    Seconds::new(short((v - ONE) * time + half)) + entry.adjustment
                }
                ChangeMode::Set => {
                    let s = if v == NO_TIME { 0 } else { cvttss2si(v) as i16 };
                    Seconds::new(f32::from(s) - time)
                }
                _ => Seconds::new(short(v)) + entry.adjustment,
            };
            entry.adjustment = adjustment;
            let done = state.state == ResearchState::Available
                && adjustment + Seconds::new(time) <= state.research_done
                && (mode != ChangeMode::Set || v != NO_TIME);
            if done {
                let _ = self.complete_research(p, tech_id, None)?;
            }
        }
        Ok(())
    }
}
