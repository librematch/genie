//! A computer player's AI module (`player+0x7a8`, `TribeMainDecisionAIModule`, 0x28fb8 bytes).
//!
//! The simulation never runs the module's own update in a playback, but it reads the module's
//! state: the world player's notification hooks write it and the hear logic (`FUN_14162e1b0`)
//! reads it. This folder holds that state, one table entry per player, as plain columns of the
//! sub-modules the hooks reach: the information module (`module+0xbb28`) and the tactical
//! module (`module+0x25f88`).
//!
//! The save holds a module only for a player whose AI was created, and the load rebuilds the
//! parts it does not save to the constructor's values ([`InfoModule::explorer`] none,
//! [`InfoModule::search`] empty).

mod explored;
mod search;
mod unit_ai_side;

pub(crate) use self::search::SearchUnit;

use crate::map::GridSize;
use crate::map::Tile;
use crate::movement::explore::InfluenceMap;
use crate::objects::ObjectId;
use crate::players::PlayerId;

/// The number of slots [`TacticalModule::slot`] reads (`FUN_140c09de0`): an index from here on
/// gives -1.
pub const SLOTS: usize = 0x1ff;

/// The information module (`TribeInformationAIModule`, `module+0xbb28`): what the AI knows of
/// the map.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InfoModule {
    /// The map's size (`+0x10518`, `+0x1051c`, saved).
    pub size: GridSize,
    /// The path map (`+0x105c8`, an `InfluenceMap`, saved): a byte per tile. The explored-tile
    /// hook writes 0; the explorer's search writes 1 for a tile its unit cannot path to and 0
    /// for the tile it picks.
    pub path_map: InfluenceMap,
    /// The unit the AI sent to explore (`+0x19848`, not saved: the load makes it -1).
    pub explorer: Option<ObjectId>,
    /// The punt search's scores (`+0x19484`, 16 records of 3 words, saved): record `4 * column +
    /// row` of the map's 4 by 4 quadrants gives its first two words ([`InfoModule::punt`]).
    pub(crate) quadrants: [[i32; 3]; 16],
    /// The explorer's points still to try (`+0x19828`, a stack of tile pairs, not saved).
    pub search: Vec<Tile>,
}

/// The tactical module (`TribeTacticalAIModule`, `module+0x25f88`): what the AI's groups do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TacticalModule {
    /// The words from `+0x1f8` (saved): [`TacticalModule::slot`] gives one. The slots the save
    /// does not hold are -1 (the load's `memset`).
    pub(crate) slot_times: Vec<i32>,
}

impl TacticalModule {
    /// `FUN_140c09de0`: slot `index`, -1 from [`SLOTS`] on or past the saved words.
    #[must_use]
    pub fn slot(&self, index: u32) -> i32 {
        let index = index as usize;
        if SLOTS <= index {
            return -1;
        }
        self.slot_times.get(index).copied().unwrap_or(-1)
    }
}

/// One player's AI module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiModule {
    /// The information module.
    pub info: InfoModule,
    /// The tactical module.
    pub(crate) tactical: TacticalModule,
}

/// The players' AI modules, by player: `None` for a player without one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AiModules {
    modules: Vec<Option<AiModule>>,
}

impl AiModules {
    /// Player `player` holds `module`.
    pub fn set(&mut self, player: PlayerId, module: AiModule) {
        let i = player.index();
        if let Some(slot) = self.modules.get_mut(i) {
            *slot = Some(module);
        } else {
            self.modules.resize(i, None);
            self.modules.push(Some(module));
        }
    }

    /// Player `player`'s module, out of the table until [`AiModules::put_back`] (a search that
    /// reads the rest of the world holds it meanwhile).
    pub fn take(&mut self, player: PlayerId) -> Option<AiModule> {
        self.modules.get_mut(player.index()).and_then(Option::take)
    }

    /// Player `player`'s module taken out by [`AiModules::take`] goes back.
    pub fn put_back(&mut self, player: PlayerId, module: AiModule) {
        self.set(player, module);
    }

    /// Player `player`'s module.
    #[must_use]
    pub fn get(&self, player: PlayerId) -> Option<&AiModule> {
        self.modules.get(player.index()).and_then(Option::as_ref)
    }

    /// Player `player`'s module, to change.
    pub fn get_mut(&mut self, player: PlayerId) -> Option<&mut AiModule> {
        self.modules
            .get_mut(player.index())
            .and_then(Option::as_mut)
    }
}

#[cfg(test)]
mod tests;
