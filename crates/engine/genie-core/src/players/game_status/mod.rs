//! The players' game status (`+0x89`: playing, won, lost) and what changes it: RESIGN
//! (`FUN_140d25af0`), the world's victory check after every update (`TRIBE_World` slot 0x68,
//! `FUN_1415c9480`, whose tail `FUN_14152e3d0` decides who won), and the status setter
//! (`WorldPlayer` slot 1, `FUN_141639860`) with a defeated player's stop of its objects
//! (`FUN_141613ed0`): its villagers, fishing boats and explorers stop and stand, its buildings
//! stop and give back their queues, and every unit AI's orders go.
//!
//! 158041: the game over test `FUN_14152f810`, a player's own victory `FUN_14165d720`, the queue's
//! cancel `FUN_1411cff00`, the make action's destructor `FUN_140ace3f0`.

mod cancels;
mod stops;
mod victory;

use crate::objects::masters::ActionType;
use crate::objects::masters::MasterId;
use crate::players::PlayerId;
#[cfg(test)]
use crate::world::World;
use crate::world::WorldTime;

/// The player still plays.
pub const PLAYING: u8 = 0;
/// The player won.
pub const WON: u8 = 1;
/// The player lost (or resigned).
pub const LOST: u8 = 2;

/// The game mode whose status changes shrink the map (battle royale, 12).
const BATTLE_ROYALE: i32 = 12;
/// A defeated player's objects stop once the game is this old, in milliseconds (`FUN_141639860`;
/// in a recording's playback the session check that would stop them earlier passes).
const STOP_AFTER_MS: WorldTime = WorldTime::from_millis(60_000);
/// The defeat's event weighs the players below this id (`FUN_141639860`'s `CMP R14D, 0x19`).
const DEFEAT_IDS: usize = 25;
/// The first of the attributes a defeat's event reads (300 + a player's id).
const KILLS_OF: usize = 300;
/// The action types whose stop keeps a value of the unit's (`+0x88a`, `FUN_14106fa60`).
const KEEPING_TYPES: [ActionType; 2] = [
    crate::objects::masters::action_type::PACK,
    crate::objects::masters::action_type::UNPACK,
];
/// The buildings a stop leaves alone (`FUN_1411e6290`).
const UNSTOPPED_BUILDINGS: [MasterId; 4] = [
    MasterId::named(50),
    MasterId::named(199),
    MasterId::named(276),
    MasterId::named(1187),
];
/// The game mode whose QUEUE trains one military unit at a time early on (16, the settings'
/// `+0x34`).
const ONE_UNIT_MODE: i32 = 16;
/// That attribute's bound (`DAT_14330eb8c`).
const ONE_UNIT_AGE: f32 = crate::math::image::game_status::ONE_UNIT_AGE;

/// What the world's checks count of the players (Gaia aside): those still playing and whether
/// they form one side (0 none, 1 one, −1 more), and those who won and lost.
#[derive(Clone, Copy, Debug, Default)]
struct Count {
    playing: u32,
    first: PlayerId,
    side: i32,
    won: u32,
    lost: u32,
}

impl Count {
    /// Player `i` of `status`, with the allied victory flag (`+0x2d0`, tested for 0) and the
    /// relations both ways to the first player still playing (`relation` 0: allied;
    /// `FUN_141667f70`).
    fn add(
        &mut self,
        i: PlayerId,
        status: u8,
        allied_victory: u8,
        relations: (crate::players::Relation, crate::players::Relation),
    ) {
        match status {
            PLAYING => {
                self.playing += 1;
                if self.first != PlayerId::GAIA && self.side != 0 {
                    // another side unless the two count each other's victory as their own
                    if allied_victory == 0
                        || !matches!(relations.0, crate::players::Relation::ALLY)
                        || !matches!(relations.1, crate::players::Relation::ALLY)
                    {
                        self.side = -1;
                    }
                } else {
                    self.side = 1;
                    self.first = i;
                }
            }
            WON => self.won += 1,
            LOST => self.lost += 1,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests;
