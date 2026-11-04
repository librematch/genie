//! The world player's own update (`WorldPlayer::update`, `0x14163b100`, slot 0x100) and Gaia's
//! (`WorldPlayerGaia::update`, `0x141681cd0`, its slot 0x100). `BaseWorld::update` runs a
//! player's slot 0x100 after its groups (`0x1415518f0`); both end in `WorldPlayerBase::update`
//! (`0x141677450`: the plans, the objects, `Process_New_Tiles`), which [`World::tick_with`] runs
//! between [`World::update_world_player`] and [`World::world_player_tail`].
//!
//! Gaia's update only adds the update's seconds (`world+0x164`) to its own `update_time`
//! (`+0x1468`); the world player's `mUpdateTime` (`+0x7a4`) of Gaia stays 0. The world player's
//! update adds them to `mUpdateTime` and runs a loop once for each whole second of it
//! ([`World::world_player_second`]): the history, the explored share (attribute 22), the kill
//! ratio (attribute 44), the defeat check of a human or computer player and, while the player
//! plays, its resource trickles (attributes 230 to 233) and its mills' food (attribute 254).
//! After `WorldPlayerBase::update` its scout map takes the tiles its map explored
//! ([`World::world_player_tail`]).
//!
//! Not simulation, not held: the local player's farm and fish trap queues sent as commands once
//! after a load (`+0x1427`, `FUN_140e74d70`, `FUN_140e74e10`: GAME 0x10 and 0x13, which a
//! recording holds; a playback sends none, `FUN_141533c10`), the profiler (`+0x1448`), the nine
//! slots at `+0xeac` (a world time read, `FUN_1415300d0`, whose answer goes unused), the attack
//! alerts' list (`+0x798`, unsaved: `FUN_141612690` posts the spectator API's event,
//! `FUN_141778410`, and the `"?under_attack"` sound and chat; `FUN_1417787d0` reads the spectator
//! API), the statistics of the local or a computer player's income (`FUN_14097daf0`), the
//! resource panel's event (`FUN_140c898a0` 0x80 in the attribute add), the AI's turn and update
//! (`FUN_14154e310` on `world+0x7dc`, unsaved, and the AI's slot 0x28 at `+0x7a8`: a playback
//! runs neither, `FUN_140d2f9c0`'s `+1`). The units of a formation that came out at one gather
//! point (`+0x1428`, `+0x1438` above 1, `FUN_141614170`) are only listed (`FUN_14160c4f0`) under
//! the world's feature flags (`mUseFeatureFlags`, `DAT_143f01830`), 0 in every recording seen.
//!
//! 158041: the explored share `FUN_141285c50`, the human or computer test `FUN_141663f30`, the
//! world player's attribute add (slot 0xf8) `FUN_141616820` and its base's `FUN_14165d040`, the
//! scout map's set `FUN_140f4d5e0`.
//!
//! [`World::tick_with`]: crate::world::World::tick_with
//! [`World::update_world_player`]: crate::world::World::update_world_player
//! [`World::world_player_second`]: crate::world::World::world_player_second
//! [`World::world_player_tail`]: crate::world::World::world_player_tail

use crate::math::fmath::below_or_unordered;
use crate::objects::masters::ClassMask;
use crate::objects::masters::MasterId;
use crate::players::Amount;
use crate::players::attribute::AttributeId;
use crate::world::Seconds;
use crate::world::WorldTime;

mod counts;
mod income;
pub use self::income::ResourceShare;
mod tech_tree;
use genie_model::players::world_player::tree_ids;
mod update;

pub use self::tech_tree::{TechTree, TreeBuilding, TreeQuery, TreeUnit, UniqueUnit};
pub use self::tree_ids::{TreeMaster, TreeTech};
use crate::math::image::world_player::{
    MINUTE as WORLD_PLAYER_MINUTE, PERCENT as WORLD_PLAYER_PERCENT, SECOND as WORLD_PLAYER_SECOND,
};
use crate::players::CivId;
use crate::technology::TechId;
use crate::world::Mode680;
#[cfg(test)]
use crate::world::World;
use crate::world::layer::MasterSlots;

/// One second (`DAT_14330eb8c`): the loop's step.
const SECOND: f32 = WORLD_PLAYER_SECOND;
/// One hundred (`DAT_14330ebcc`): attribute 22 is a percentage.
const PERCENT: f32 = WORLD_PLAYER_PERCENT;
/// A minute's seconds (`DAT_14330ebc8`): the trickles and the mills' food are rates a minute.
const MINUTE: f32 = WORLD_PLAYER_MINUTE;
/// Until this world time (ms) the scout map takes only the explored tiles the player sees.
const EARLY_SCOUTING_MS: WorldTime = WorldTime::from_millis(5000);

/// The town centres' masters (`FUN_141658970`'s, `FUN_141658a30`'s and `FUN_1416597e0`'s keys
/// 0x6d and 0x1bc) in the order the counts read their lists: the build's `town_centre`, then
/// `town_centre_444`.
pub(crate) const fn town_centres(masters: MasterSlots) -> [MasterId; 2] {
    [masters.town_centre, masters.town_centre_444]
}
/// The classes that never keep their player (`0x48000800`: other 11, walls 27, flags 30).
const UNKEPT_LOW: ClassMask = ClassMask::new(0x4800_0800, 31);
/// The classes that never keep their player (`0x140204a000384006`: 1, 2, 14, 19, 20, 21, 37,
/// 39, 42, 49, 58, 60).
const UNKEPT_HIGH: ClassMask = ClassMask::new(0x1402_04a0_0038_4006, 61);
/// The ages' technologies (Feudal, Castle, Imperial: `0x3284`, `0x3304`, `0x3384` in the
/// player's technologies), whose state −1 puts their units out of the player's tree.
const AGES: [TechId; 3] = [TechId::FIRST_AGE, TechId::CASTLE_AGE, TechId::LAST_AGE];
/// `FUN_140c7e6f0`: the civilisation the full tech tree reads, by the game's tagged value at
/// `+0x680` ([`crate::world::World::mode_680`]): 18 for 1, 59 for 2, else 0.
const fn full_tree_civ(mode_680: Mode680) -> CivId {
    CivId::new(match mode_680.get() {
        1 => 0x12,
        2 => 0x3b,
        _ => 0,
    })
}

/// … with the store mode 0x20 (`0x14163b89d`).
const ALIVE_STORE_MODE: u8 = 0x20;

/// The timers that keep a player in the game without a living object (the defeat check's first
/// test): each counts at 0 or more; −1 is none.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VictoryTimes {
    /// The game's wonder countdown of the player (the game object's `+0x4ec[id]`,
    /// `FUN_140c88300`): the least time left of its wonders' actions (`TRIBE_Action_Wonder`,
    /// `FUN_140c91500`), −1 without one (`FUN_140c90040`).
    pub(crate) wonder: Seconds,
    /// `ruin_held_time` (`+0xdb0`, saved).
    pub(crate) ruins: Seconds,
    /// `artifact_held_time` (`+0xdb4`, saved).
    pub(crate) artifacts: Seconds,
}

/// A victory timer that does not run (−1).
const NO_TIMER: Seconds = Seconds::new(-1.0);

/// No timer runs.
impl Default for VictoryTimes {
    fn default() -> Self {
        Self {
            wonder: NO_TIMER,
            ruins: NO_TIMER,
            artifacts: NO_TIMER,
        }
    }
}

impl VictoryTimes {
    /// One of the timers runs (`COMISS` with 0, `JNC`: 0 or more, not NaN): the player stays in
    /// the game.
    #[must_use]
    pub fn running(&self) -> bool {
        [self.wonder, self.ruins, self.artifacts]
            .into_iter()
            .any(|seconds| !below_or_unordered(seconds.get(), 0.0))
    }
}

/// Attribute `attribute_id` of `attributes`, 0 past them (the game's array holds them all).
fn get(attributes: &[Amount], attribute_id: AttributeId) -> Amount {
    attributes
        .get(attribute_id.index())
        .copied()
        .unwrap_or(Amount::ZERO)
}

/// Attribute `attribute_id` of `attributes` takes `v`; none past them (the game's array holds them
/// all).
fn set(attributes: &mut [Amount], attribute_id: AttributeId, v: Amount) {
    if let Some(amount) = attributes.get_mut(attribute_id.index()) {
        *amount = v;
    }
}

#[cfg(test)]
mod tests;
