//! The world's treaty monitor (`world+0x820`, HD's `TreatyMode`, 0x160 bytes): a treaty's
//! countdown, and the no-build zones about the players' town centres while it holds. The save
//! holds none of it: a load makes it from the save's `treatyLength` byte
//! ([`World::load_treaty`]), and while it holds the players who would be enemies stand in
//! relation 4. The world's update runs it while the update has time ([`World::treaty_step`]):
//! its countdown, the zones once every player has a town centre, and its end at its second, by
//! game mode 16's Castle Age or, for a lengthened nomad treaty, once its zones stand; relation 3
//! comes back and the monitor goes.
//!
//! 158041: the constructor `FUN_141461190` (its zones' fill `FUN_141462030`), the starts
//! `FUN_141461620` and `FUN_141461710`, the end `FUN_141461310`, the update `FUN_1414618a0`,
//! the zones `FUN_141461c00` (`FUN_1414612b0` at a load), the no-build test `FUN_141461400`
//! and a town centre's foundation sweep `FUN_1411c7ae0`; the world's queries `FUN_1415e8e10`
//! and `FUN_1415c8150`.

mod update;
mod zones;

use crate::GenieError;
use crate::GenieResult;
use crate::map::TileRect;
use crate::map::WholeDistance;
use crate::players::PlayerId;
use crate::world::GameMode;
use crate::world::World;
use crate::world::WorldTime;

/// The seconds of a minute in the treaty's int arithmetic: the constructor's `IMUL` by `0x3c`
/// (`FUN_141461190`) and the load's signed division by it (`IMUL` by `0x88888889`, `SAR` 5,
/// `World::load_world` at `0x1415d4b91`).
const SECONDS_PER_MINUTE: i32 = 60;
/// The milliseconds of a second in the treaty's int arithmetic: the signed division of the world's
/// time by it (`IMUL` by `0x10624dd3`, `SAR` 6; `FUN_1414618a0`, `FUN_141461710`).
const MS_PER_SECOND: i32 = 1000;

/// A forced nomad treaty shorter than this many minutes lasts this long and is lengthened
/// (`FUN_141461190`'s `CMP EDX, 5`, a signed compare); the load's own bound is
/// [`update::LOADED_NOMAD_LEAST`].
const NOMAD_LEAST: TreatyMinutes = TreatyMinutes(5);
/// The zones' margin in tiles about a town centre (`FUN_141461190`'s `MOV [RCX+0xc], 0x14`).
const MARGIN: WholeDistance = WholeDistance::of_data(20);
/// The margin in game mode 16 (`FUN_141461190`'s `MOV [RBX+0xc], 6`).
const ONE_UNIT_MARGIN: WholeDistance = WholeDistance::of_data(6);
/// The margin of a forced nomad treaty outside game mode 16 (`FUN_141461190`'s
/// `MOV [RBX+0xc], 0xa`).
const NOMAD_MARGIN: WholeDistance = WholeDistance::of_data(10);

/// The monitor's zones (`+0x18`..`+0x15c`, nine of 0x24 bytes): one per player past Gaia, as
/// many as a world's players (Gaia and eight).
const ZONES: usize = 9;

/// The world's treaty monitor while a routine of it runs: the world's update and the load call
/// them only with one, and nothing they call lets it go.
const NO_MONITOR: GenieError =
    GenieError::unreachable("a treaty monitor's routine while the world holds no monitor");
/// A player past the monitor's nine zones: a world holds Gaia and eight players at most.
const NO_ZONE: GenieError = GenieError::unreachable("a player past the treaty monitor's zones");

/// The treaty's clock in whole seconds as the monitor's `int`s hold them: its end
/// (`mTreatyTimeInSeconds`, `+0x0`), its next message (`mNextMessageTimeInSeconds`, `+0x4`), the
/// world's time cut to its seconds, and the seconds left (`mTimeLeftInSeconds`, `+0x8`), the end
/// less the time. The game's int arithmetic on them wraps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TreatySeconds(i32);

impl TreatySeconds {
    /// No second: the seconds left once the treaty ends at its second (`FUN_1414618a0`).
    pub const ZERO: Self = Self(0);
    /// The seconds left of a treaty without time (−1, `FUN_141461620`'s store): game mode 16's.
    pub const UNTIMED: Self = Self(-1);

    /// The world's time `time` cut to its whole seconds: its milliseconds as an `int`, divided
    /// by 1000 toward zero (`FUN_1414618a0`, `FUN_141461710`).
    #[must_use]
    pub const fn at(time: WorldTime) -> Self {
        Self(time.millis() as i32 / MS_PER_SECOND)
    }

    /// It less `seconds` (`SUB`, wrapping): the seconds left of an end less the time's, a
    /// countdown's second before the end.
    #[must_use]
    pub const fn less(self, seconds: Self) -> Self {
        Self(self.0.wrapping_sub(seconds.0))
    }

    /// Its whole minutes, toward zero (the load's signed division by 60).
    #[must_use]
    pub const fn minutes(self) -> i32 {
        self.0 / SECONDS_PER_MINUTE
    }

    /// The int, as the monitor holds it.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

/// A treaty's length in whole minutes as the monitor's constructor takes it (`FUN_141461190`'s
/// `int`): the save's `treatyLength` byte, or −1 in game mode 16.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TreatyMinutes(i32);

impl TreatyMinutes {
    /// Game mode 16's length (−1, `World::load_world`'s `R13D` at `0x1415d4b32`): a treaty no
    /// time ends.
    pub const UNTIMED: Self = Self(-1);

    /// The length of the save's byte `byte` (`MOVZX`).
    #[must_use]
    pub const fn of_byte(byte: u8) -> Self {
        Self(byte as i32)
    }

    /// The treaty's end in seconds since the game's start (`IMUL` by `0x3c`, wrapping).
    #[must_use]
    pub const fn end(self) -> TreatySeconds {
        TreatySeconds(self.0.wrapping_mul(SECONDS_PER_MINUTE))
    }

    /// The int.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

/// The monitor's state (`mCurState`, `+0x10`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TreatyState {
    /// 0: made, not started.
    New,
    /// 1: the treaty holds ([`World::treaty_holds`]).
    Holds,
    /// 2: ended ([`World::end_treaty`]): relation 3 stands again.
    Ended,
    /// 3: ended in its update: the world's update lets it go.
    Done,
}

/// Whether the game forces a nomad treaty (the save's `gbForceNomadTreaty`, the global
/// `DAT_143f69470`): its treaty lasts at least five minutes and its zones' margin is 10.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NomadTreaty {
    /// The byte is not 0.
    Forced,
    /// The byte is 0.
    NotForced,
}

impl NomadTreaty {
    /// The choice of the save's byte `byte`.
    #[must_use]
    pub const fn of_byte(byte: u8) -> Self {
        if byte == 0 {
            Self::NotForced
        } else {
            Self::Forced
        }
    }
}

/// Whether the interface's game screen and its view stand (`DAT_143fb1d40` and its `+0xa48`),
/// which the zones' walk wants (`FUN_141461c00`). genie plays as a game client: the game makes
/// its screen after the world's load (`FUN_140f1e880`), so the load's walk finds none and every
/// update's walk finds it. The tick oracle, which has no screen, never draws a zone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GameScreen {
    /// The screen and its view stand: an update's walk.
    Shown,
    /// No screen yet: the load's walk.
    Absent,
}

/// Whether the monitor's zones stand for every player (`bordersAreDrawn`, `+0x14`): the zones'
/// walk found each player's first town centre ([`World::draw_treaty_zones`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Borders {
    /// The last walk found every player's first town centre.
    Drawn,
    /// No walk yet, a walk that missed one, or a treaty that ended.
    Undrawn,
}

/// A player's no-build zones (`+0x18 + (player − 1) × 0x24`): the tiles about its first and its
/// second town centre, and whether they are set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TreatyZone {
    /// `+0x0`..`+0xc` (HD's `borders`): about its first town centre, x0, y0, x1, y1.
    pub(crate) first: TileRect,
    /// `+0x10`..`+0x1c` (HD's `borders2`): about its second.
    pub(crate) second: TileRect,
    /// `+0x20`: set; the zones' walk passes the player over.
    pub(crate) set: bool,
}

/// The world's treaty monitor (`world+0x820`): its clock, its state and the players' zones.
/// Not held: `+0x15`, the forced nomad flag the constructor stores, which no routine of the
/// simulation reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreatyMonitor {
    /// `+0x0`, `mTreatyTimeInSeconds`: the treaty's end.
    pub(crate) end: TreatySeconds,
    /// `+0x4`, `mNextMessageTimeInSeconds`: the countdown's next message. The constructor leaves
    /// it and `+0x8` as the heap gave them, and the starts write them before a reader; genie
    /// starts both at 0.
    pub(crate) next_message: TreatySeconds,
    /// `+0x8`, `mTimeLeftInSeconds`: the seconds left ([`World::treaty_seconds_left`]).
    pub(crate) seconds_left: TreatySeconds,
    /// `+0xc`: the zones' margin in tiles about a town centre.
    pub(crate) margin: WholeDistance,
    /// `+0x10`, `mCurState`.
    pub(crate) state: TreatyState,
    /// `+0x14`, `bordersAreDrawn`.
    pub(crate) borders: Borders,
    /// `+0x16`: a forced nomad treaty shorter than five minutes took five; it ends once its
    /// zones stand.
    pub(crate) lengthened: bool,
    /// `+0x17`: game mode 16's Castle Age was queued (RESEARCH, `FUN_141630d10`); the next
    /// update ends the treaty.
    pub(crate) castle_age: bool,
    /// `+0x18`: each player's zones, player 1 first.
    pub(crate) zones: [TreatyZone; ZONES],
    /// `+0x15c`, `secondBordersEnabled`: a player with two town centres had its second zone
    /// set; every later player's walk then looks for its second town centre too.
    pub(crate) second_zones: bool,
}

impl TreatyMonitor {
    /// `FUN_141461190(minutes, forced)`: the treaty of `minutes` minutes in game mode
    /// `game_mode`. A forced nomad treaty shorter than five minutes (a signed compare) takes five
    /// and is lengthened. The zones' margin is 20 tiles, 6 in game mode 16 and 10 for a forced
    /// nomad treaty elsewhere. It is new, its zones undrawn, without game mode 16's Castle Age,
    /// and every zone 0 (`FUN_141462030` copies one zero entry into all nine).
    #[must_use]
    pub fn new(minutes: TreatyMinutes, nomad: NomadTreaty, game_mode: GameMode) -> Self {
        let forced = nomad == NomadTreaty::Forced;
        let lengthened = minutes < NOMAD_LEAST && forced;
        let minutes = if lengthened { NOMAD_LEAST } else { minutes };
        let margin = if game_mode == GameMode::OneUnit {
            ONE_UNIT_MARGIN
        } else if forced {
            NOMAD_MARGIN
        } else {
            MARGIN
        };
        Self {
            end: minutes.end(),
            next_message: TreatySeconds::ZERO,
            seconds_left: TreatySeconds::ZERO,
            margin,
            state: TreatyState::New,
            borders: Borders::Undrawn,
            lengthened,
            castle_age: false,
            zones: [TreatyZone::default(); ZONES],
            second_zones: false,
        }
    }

    /// The treaty holds: the monitor in its state 1 (`FUN_1415e8e10`'s `CMP [RAX + 0x10], 1`).
    #[must_use]
    pub fn holds(&self) -> bool {
        self.state == TreatyState::Holds
    }

    /// Player `player`'s zones (`+0x18 + (player − 1) × 0x24`).
    ///
    /// # Errors
    ///
    /// Gaia, or a player past the nine zones.
    pub(crate) fn zone(&self, player: PlayerId) -> GenieResult<&TreatyZone> {
        player
            .index()
            .checked_sub(1)
            .and_then(|index| self.zones.get(index))
            .ok_or_else(|| NO_ZONE.with("player", player))
    }

    /// Player `player`'s zones to change ([`TreatyMonitor::zone`]).
    ///
    /// # Errors
    ///
    /// Gaia, or a player past the nine zones.
    pub(crate) fn zone_mut(&mut self, player: PlayerId) -> GenieResult<&mut TreatyZone> {
        player
            .index()
            .checked_sub(1)
            .and_then(|index| self.zones.get_mut(index))
            .ok_or_else(|| NO_ZONE.with("player", player))
    }
}

/// `FUN_1415e8e10` over the world's treaty monitor `monitor` (`+0x820`, none for its null
/// pointer): a monitor stands and its treaty holds ([`TreatyMonitor::holds`]). A reader that
/// holds no world (the ally test, [`crate::unit_ai::task_finder::allied`]) takes the world's
/// monitor as it stands.
#[must_use]
pub fn treaty_holds(monitor: Option<&TreatyMonitor>) -> bool {
    monitor.is_some_and(TreatyMonitor::holds)
}

impl World {
    /// `FUN_1415e8e10`: the world's treaty holds, its monitor (`+0x820`) in its state 1
    /// ([`treaty_holds`]).
    #[must_use]
    pub fn treaty_holds(&self) -> bool {
        treaty_holds(self.treaty.as_ref())
    }

    /// `FUN_1415c8150` (HD's `World::TreatyTimeLeftInSeconds`): the seconds left of the treaty
    /// that holds (`+0x8`), 0 without one.
    #[must_use]
    pub fn treaty_seconds_left(&self) -> TreatySeconds {
        self.treaty
            .as_ref()
            .filter(|monitor| monitor.holds())
            .map_or(TreatySeconds::ZERO, |monitor| monitor.seconds_left)
    }

    /// The world's treaty monitor while one of its routines runs.
    ///
    /// # Errors
    ///
    /// A world without one ([`NO_MONITOR`]).
    pub(crate) fn treaty_monitor(&self) -> GenieResult<&TreatyMonitor> {
        self.treaty.as_ref().ok_or(NO_MONITOR)
    }

    /// The world's treaty monitor to change while one of its routines runs.
    ///
    /// # Errors
    ///
    /// A world without one ([`NO_MONITOR`]).
    pub(crate) fn treaty_monitor_mut(&mut self) -> GenieResult<&mut TreatyMonitor> {
        self.treaty.as_mut().ok_or(NO_MONITOR)
    }
}

#[cfg(test)]
mod tests;
