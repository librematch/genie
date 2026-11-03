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

use crate::math::image::game_status::ONE_UNIT_AGE as GAME_STATUS_ONE_UNIT_AGE;
use crate::objects::masters::ActionKind;
use crate::players::PlayerId;
use crate::players::Relation;
#[cfg(test)]
use crate::world::World;
use crate::world::WorldTime;

/// A player's game status (`game_status`, `+0x89`, saved): playing, won or lost. Every byte is
/// kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GameStatus {
    /// 0: the player still plays.
    #[default]
    Playing,
    /// 1: the player won.
    Won,
    /// 2: the player lost (or resigned).
    Lost,
    /// A value the game names nowhere, as the save or the game holds it.
    Other(Unnamed),
}

/// A raw value of [`GameStatus`] that names none of its variants: only the set's own
/// constructors make one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Unnamed(u8);

impl GameStatus {
    /// The value of raw `value`: its variant, else `Other`.
    const fn of_raw(value: u8) -> Self {
        match value {
            0 => Self::Playing,
            1 => Self::Won,
            2 => Self::Lost,
            other => Self::Other(Unnamed(other)),
        }
    }

    /// Its raw value.
    const fn raw(self) -> u8 {
        match self {
            Self::Playing => 0,
            Self::Won => 1,
            Self::Lost => 2,
            Self::Other(Unnamed(value)) => value,
        }
    }

    /// The status of the save's byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self::of_raw(value)
    }

    /// Whether the player's game ended: won or lost.
    #[must_use]
    pub const fn ended(self) -> bool {
        matches!(self, Self::Won | Self::Lost)
    }

    /// The byte, as the save holds it.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.raw()
    }
}

impl genie_diff::Diff for GameStatus {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.raw().diff(&other.raw(), d);
    }
}

/// The state of a player's victory conditions (`player+0x68`'s `+0x20`, the save's `victory`):
/// [`VictoryState::MET`] once they are met (`FUN_1414f35d0`). Every byte is kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VictoryState(u8);

impl VictoryState {
    /// 2: the conditions are met.
    pub const MET: Self = Self(2);

    /// The state of the save's byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// The byte, as the save holds it.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// A player's fealty (`fealty`, `+0x12f8`, saved): the player a defeated player's resources and
/// units would go to, when above 0. Every short is kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Fealty(i16);

impl Fealty {
    /// The fealty of the save's short `value`.
    #[must_use]
    pub const fn of_data(value: i16) -> Self {
        Self(value)
    }

    /// Whether it names a player: above 0.
    #[must_use]
    pub const fn names_player(self) -> bool {
        0 < self.0
    }

    /// The short, as the save holds it.
    #[must_use]
    pub const fn get(self) -> i16 {
        self.0
    }
}

/// A defeated player's objects stop once the game is this old, in milliseconds (`FUN_141639860`;
/// in a recording's playback the session check that would stop them earlier passes).
const STOP_AFTER_MS: WorldTime = WorldTime::from_millis(60_000);
/// The defeat's event weighs the players below this id (`FUN_141639860`'s `CMP R14D, 0x19`).
const DEFEAT_IDS: usize = 25;
/// The first of the attributes a defeat's event reads (300 + a player's id).
const KILLS_OF: usize = 300;
/// The action types whose stop keeps a value of the unit's (`+0x88a`, `FUN_14106fa60`).
const KEEPING_KINDS: [ActionKind; 2] = [ActionKind::PACK, ActionKind::UNPACK];

/// That attribute's bound (`DAT_14330eb8c`).
const ONE_UNIT_AGE: f32 = GAME_STATUS_ONE_UNIT_AGE;

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
        status: GameStatus,
        allied_victory: u8,
        relations: (Relation, Relation),
    ) {
        match status {
            GameStatus::Playing => {
                self.playing += 1;
                if self.first != PlayerId::GAIA && self.side != 0 {
                    // another side unless the two count each other's victory as their own
                    if allied_victory == 0
                        || !matches!(relations.0, Relation::Ally)
                        || !matches!(relations.1, Relation::Ally)
                    {
                        self.side = -1;
                    }
                } else {
                    self.side = 1;
                    self.first = i;
                }
            }
            GameStatus::Won => self.won += 1,
            GameStatus::Lost => self.lost += 1,
            GameStatus::Other(_) => {}
        }
    }
}

#[cfg(test)]
mod tests;
