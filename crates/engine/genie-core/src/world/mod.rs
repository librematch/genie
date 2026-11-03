//! The world: its state and its tick. [`World`] holds the players with their objects, the map,
//! the pathing state, the random streams and the masters, and [`World::tick_with`] runs one
//! update of it.
//!
//! 158041 `BaseWorld::update` (`0x14154fc70`), in order: advance the world time by dt,
//! `SeedAll(frame seed)`, the player loop (each player's time slot `world+0x18c[p]` gains dt; only the
//! turn player `world+0x188` runs the full unit-AI update), `s1 = World.rand15`, `SeedAll(s1)`, zero
//! the turn player's slot and pass the turn on, then the next frame seed `= World.rand15`. Besides
//! those two, a unit group's idle timer draws from the World stream
//! ([`World::reset_idle_timer`]); triggers and XS scripts, which draw more, are not run.
//!
//! A player's update walks its live list in order (`RGE_Object_List::update`, `0x141166550`). Each
//! object moves by its velocity ([`Mover::step_velocity`]), runs its static state step
//! ([`crate::objects::lifecycle::Lifecycle`]), then its actions (the livestock default action, a builder's
//! Build action with its walk), then its unit AI. An object whose update returns 1 leaves: the
//! last object takes its slot and updates there in the same tick.
//!
//! The module's files: the world as the header save holds it (`setup`), its clock (`clock`), one
//! world update
//! (`world_update`), one object's step (`object_step`) with what it reads and writes (`step`),
//! a player (`player`) and the checked reach into its lists (`slots`), the game options (`settings`), the checkpoint's state block
//! (`checkpoint`) and the forced replay (`forced`).
//!
//! [`Mover::step_velocity`]: crate::movement::Mover::step_velocity

mod action_parts;
pub mod checkpoint;
pub mod index;
pub(crate) mod lets;
pub mod motions;
pub(crate) mod others;
pub mod parts;
#[cfg(test)]
mod parts_fixtures;
pub(crate) mod player;
pub(crate) mod slots;
pub(crate) mod step;
pub(crate) mod unread;
pub(crate) mod whole;

use crate::GenieError;
use crate::map::TerrainId;
use crate::map::spatial::Spatial;
use crate::math::rng::Streams;
use crate::objects::ObjectId;
use crate::objects::masters::ActionKind;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::pathing::Pathing;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
pub use crate::players::history::HISTORY_PERIOD;
use crate::technology::TechId;
pub(crate) use crate::world::player::NO_ATTRIBUTE;
pub use crate::world::player::{AiSettings, Formations, Player};
use genie_diff::Diff;
pub use genie_model::layer;
use genie_model::layer::MasterSlots;
pub use genie_model::model::Model;

mod after_step;
mod ai_values;
use genie_model::world::clock;
mod forced;
mod frame_seed;
mod mode_680;
mod object_step;
use genie_model::world::options;
mod play_state;
pub mod schedule;
pub use genie_model::world::settings;
mod setup;
mod tick;
mod world_update;

pub use self::ai_values::{LureDestination, Percent, TargetingMode, VillagerGarrison};
pub use self::clock::{Millis, Seconds, WholeSeconds, WorldTime};
pub use self::forced::GameState;
pub use self::frame_seed::FrameSeed;
pub use self::mode_680::Mode680;
pub use self::options::{
    Age, ContentMode, GameMode, ResourceLevel, SubModeFlags, TreatyLength, Visibility,
};
pub use self::play_state::PlayState;
pub use self::tick::Tick;
pub use self::unread::Unread;
use crate::groups::UnitGroups;
use crate::math::image::turn::{
    GATE_EPSILON as TURN_GATE_EPSILON, SPEED_EPSILON as TURN_SPEED_EPSILON,
};
use crate::objects::RelicCount;
use crate::pathing::zones::Zones;
use crate::players::Team;
use crate::players::ai_module::AiModules;
use crate::players::history::HistoryUpdates;
use crate::players::market::Market;

/// What every building holds of a gate: `mGateLocked` (`+0xc7c`; a lock also sets the
/// building's last move time, [`Mover::last_move`], slot 0x670, `FUN_1411dba30`), `mFirstUpdate`
/// (`+0xc80`) and `mCloseTimer` (`+0xc84`) ([`Unit::gate_update`]).
///
/// [`Mover::last_move`]: crate::movement::Mover::last_move
/// [`Unit::gate_update`]: crate::objects::unit::Unit::gate_update
#[derive(Clone, Copy, Debug, Default, PartialEq, Diff)]
pub struct Gate {
    /// `mGateLocked`: the gate is locked.
    pub locked: bool,
    /// `mFirstUpdate`: a new building's is set; a gate's first update in state 2 takes its
    /// annexes' places as its endpoints (`mEndpoint1`, `mEndpoint2`, not held) and clears it.
    pub first_update: bool,
    /// `mCloseTimer`: the seconds an open gate with nothing in it waits before it shuts.
    pub close_timer: Seconds,
    /// The building has a saved open or shut master (`+0xc58`, the save's
    /// `has_open_closed_master_object`) or its original owner's masters (`mOriginalOwner`,
    /// `+0xc88`): a gate's toggle then takes another master, not modelled. A master of its own
    /// (`+0x6c0`, which a foundation holds until its first builder starts) the slice does not
    /// hold.
    pub other_masters: bool,
}

/// A walk's unit in the way (notify 0x215) that the walk's owner does not pass on.
pub(crate) const IN_THE_WAY: GenieError =
    GenieError::unsupported("a walk's unit in the way (notify 0x215) the action does not pass on");

/// An attack's target without a place.
pub(crate) const TARGET_UNPLACED: GenieError =
    GenieError::unsupported("an attack's target without a place");
/// An attacker without a place.
pub(crate) const ATTACKER_UNPLACED: GenieError =
    GenieError::unsupported("an attacker without a place");
/// A hunter without movement state.
pub(crate) const HUNTER_UNMOVING: GenieError =
    GenieError::unsupported("a hunter without movement state");
/// An attacker without its standing sprite.
pub(crate) const NO_STANDING_SPRITE: GenieError =
    GenieError::unsupported("an attacker without its standing sprite");
/// A walker without movement state.
pub(crate) const WALKER_UNMOVING: GenieError =
    GenieError::unsupported("a walker without movement state");
/// An attacker without movement state.
pub(crate) const ATTACKER_UNMOVING: GenieError =
    GenieError::unsupported("an attacker without movement state");
/// A unit AI's unit of a master the slice does not have.
pub(crate) const UNIT_AI_MASTER: GenieError =
    GenieError::unsupported("a unit AI's unit of a master the slice does not have");
/// A hunter of a master the slice does not have.
pub(crate) const HUNTER_MASTER: GenieError =
    GenieError::unsupported("a hunter of a master the slice does not have");
/// An object of a master the slice does not have.
pub(crate) const OBJECT_MASTER: GenieError =
    GenieError::unsupported("an object of a master the slice does not have");
/// An attacker of a master the slice does not have.
pub(crate) const ATTACKER_MASTER: GenieError =
    GenieError::unsupported("an attacker of a master the slice does not have");

/// A gate's height (its master's `radius_z`) above this shuts it (`DAT_142eb0c60`); a unit
/// slower than it counts in a gate only when it is a player's.
pub(crate) const GATE_EPSILON: f32 = TURN_GATE_EPSILON;
/// The seconds an open gate with nothing in it waits before it shuts (`FUN_1411e7730`'s
/// immediate `0x3f000000`).
pub(crate) const GATE_CLOSE_DELAY: Seconds = Seconds::new(0.5);
/// The most records a gate's query lists (`FUN_1411d0570`'s list of 10) and the most units it
/// counts (25).
pub(crate) const GATE_QUERY_RECORDS: usize = 10;
pub(crate) const GATE_COUNT_CAP: usize = 25;

/// The building top actions that keep RESEARCH from a building (`FUN_141227a00`: 0x7b to 0x7d).
pub(crate) const BLOCKS_RESEARCH: [ActionKind; 3] =
    [ActionKind::ACTION_123, ActionKind::PACK, ActionKind::UNPACK];
/// The first master type whose slot 0x128 (a work at an object) does something: the action
/// objects' (`FUN_14106c590`) and up; below it `FUN_141071bd0` returns at once.
pub(crate) const STATIC_TYPES_END: MasterKind = MasterKind::ACTION;

/// The foundation terrain a building's death covers with terrain 1 (`FUN_1411dbb70`: `0x1c`).
pub(crate) const RESTORED_FOUNDATION: TerrainId = TerrainId::named(28);

/// What a built building's death leaves its owner's update to do after it
/// ([`Unit::built_building_died`]): the count attribute it lowers, and its master with the master's
/// technology.
///
/// [`Unit::built_building_died`]: crate::objects::unit::Unit::built_building_died
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BuildingDeath {
    pub(crate) count: Option<AttributeId>,
    pub(crate) tech: Option<(MasterId, TechId)>,
}

/// The attribute naming a building master that changes the terrain around its buildings
/// (`FUN_1411c39b0`, the player's attribute 255).
pub(crate) const BUILDING_TERRAIN: usize = 255;

/// A building's queue holds at most 15 units (`FUN_1411cbc40`).
pub(crate) const QUEUE_LIMIT: usize = 15;
/// A computer player's QUEUE fills a building's queue to at most 2 (`FUN_141614450`).
pub(crate) const COMPUTER_QUEUE_LIMIT: usize = 2;
/// The Hunt's search timer: it tells its unit AI again every 10 seconds (`0xc1200000`, in
/// `0x140ac60e0` and `0x140ac7a00`).
pub(crate) const SEARCH_AGAIN: Seconds = Seconds::new(-10.0);
/// A velocity longer than this sets the animation speed (`DAT_142eb0c60`).
pub(crate) const SPEED_EPSILON: f32 = TURN_SPEED_EPSILON;

/// A queued villager's master: male when the 64-bit Action draw times 100 (wrapping) is below 50
/// modulo 100 (`FUN_1411cbc40`: the build's `villager_male`, 0x53, else its `villager_female`,
/// 0x125).
pub(crate) const fn villager(draw: u64, masters: MasterSlots) -> MasterId {
    if draw.wrapping_mul(100) % 100 < 50 {
        masters.villager_male
    } else {
        masters.villager_female
    }
}

/// World state the slice carries from tick to tick. All of it is in the header save.
#[derive(Clone, Debug, PartialEq)]
pub struct World {
    /// The world's time (`world_time`).
    pub world_time: WorldTime,
    /// `world+0x7d0`.
    pub frame_seed: FrameSeed,
    /// `world+0x188`, `player_turn` in the save.
    pub player_turn: PlayerId,
    /// `world+0x18c[p]`, the time each player accumulated since its last turn.
    pub player_time: Vec<Millis>,
    /// Every player, Gaia first.
    pub players: Vec<Player>,
    /// The table by id (`FUN_1415417a0`): each object's place, written by the world's list
    /// operations ([`index`]); a list changed in place indexes again ([`World::reindex`]).
    pub index: index::ObjectIndex,
    /// The parts only some objects hold, each in a table by id ([`parts`], FD-045-D51): their
    /// one writer the world's list operations and [`World::set_part`].
    pub parts: parts::Parts,
    /// The updates run so far.
    pub ticks: Tick,
    /// `mNextObjectId`, the id the next new object gets.
    pub next_object_id: ObjectId,
    /// The game data the world plays on (FD-046-D04): the masters per player, the
    /// technologies and effects, the tech tree, the unit lines, the drop sites and the graphics,
    /// each table shared with the stock model until its first write.
    pub model: Model,
    /// Every object's place on the map.
    pub spatial: Spatial,
    /// The planners and the mesh.
    pub pathing: Pathing,
    /// The game options the simulation reads ([`crate::world::settings::Settings`]), from the header.
    pub settings: settings::Settings,
    /// The game object's tagged value at `+0x680` (`DAT_143f07a58`, read through
    /// `FUN_140ec0cd0`; at 1 a boar does not wander and a hit may do no damage). `GamePC::start4`
    /// (`FUN_140f29b60`) compares it with the loaded world's `+0x6b4` and reloads the data when
    /// they differ. It is not the options' content mode: MarvinBlue/426048731 saves
    /// `mSelectedContentMode` 2 and the tick oracle reads this as 0. Its writer is not located
    /// and the save does not hold it; genie-oracle leaves it at 0.
    pub mode_680: Mode680,
    /// The Empire Wars start's pass is due ([`crate::orders::start_work`]): the world's `+0x6c6` is
    /// clear in game mode 13, which a game's start has.
    pub start_work_due: bool,
    /// The world's `+0x844`: the relics (master 0x11d) an effect's spawn made (`FUN_1416136f0`),
    /// which the relic victory's countdown reads (not modelled, [`crate::players::game_status`]); not
    /// saved here, 0 at a load.
    pub spawned_relics: RelicCount,
    /// The world's game state (`+0x88`, the save's `tempGameState`): 1 once the game is over.
    pub game_state: PlayState,
    /// The scenario's `victory_conquest` (`world+0x7e0`'s `+0x2c0`, `FUN_140ca4d20`): the game
    /// ends when one side is left.
    pub victory_conquest: bool,
    /// The map's zone maps (`map+0x5e28`), made when first asked for.
    pub zones: Zones,
    /// The world's ids and pools for the objects of recyclable masters ([`Recycling`]).
    pub recycling: Recycling,
    /// The computer players' AI modules (`player+0x7a8`), by player.
    pub ai: AiModules,
    pub(crate) streams: Streams,
    /// The market's prices (`+0x868`.., saved).
    pub market: Market,
    /// The world's unit groups ([`crate::groups`]).
    pub groups: UnitGroups,
    /// Each comm player's last commanded units (the command processor's list at
    /// `+(0xd + n) * 0x18`, not saved): the own list of an INTERACT, WORK, MOVE or RETREAT
    /// whose check let it run, which a count of `0xffff` names
    /// ([`crate::commands::UnitList::LAST_COMMANDED`]).
    pub last_commanded: Vec<Vec<Option<ObjectId>>>,
    /// The current update's time in seconds (`world+0x164`, set before the commands run).
    pub dt_seconds: Seconds,
    /// The current update's time in milliseconds (`world+0x7c`, set before the commands run).
    pub dt: Millis,
    /// The history period (`world+0x8b8`, `time_slice`): the `.dat`'s, which the world reads
    /// after its constructor's [`HISTORY_PERIOD`] ([`crate::players::history::History::update`]).
    pub history_period: HistoryUpdates,
    /// Each player's `mAllowCapturingGaiaUnits` (`player+0x670`, saved; false for a player it
    /// lacks): a computer player takes Gaia's objects over by seeing them in the game modes
    /// outside the standard ones ([`crate::vision::visibility::sight_taker`]).
    pub allow_gaia_capture: Vec<bool>,
    /// `mLocalPlayerIndex` (`world+0x6b0`, saved): the player whose view the game shows, a
    /// recording's point of view. Its masters stand for a building's in a train location's test
    /// (`FUN_141263610` through `FUN_140c88440`, [`crate::players::world_player`]).
    pub local_player: PlayerId,
    /// `allianceLocked` (`world+0xc`, saved): GAME's diplomacy and allied victory change nothing
    /// while it is not 0.
    pub alliance_locked: i32,
    /// The world's teams (`+0x984`..`+0x9a0`, `playerTeams[i]`, saved as bytes): player i's
    /// team at i − 1, 1 alone ([`World::resolve_teams`]).
    pub teams: [Team; 8],
    /// The game's `+0xc14` (`FUN_140c884c0`, saved as a byte): the player of the quick-build
    /// cheat (GAME 4), 0 without; while it is not 0 every work is done at once, which the work
    /// actions do not model (they take it as 0).
    pub instant_work: i32,
}

/// The objects of recyclable masters (`+0x680`, the `.dat`'s `recyclable`): a new one takes the
/// world's next reusable id (`FUN_1415301b0`: the value, then one less), which the world's
/// constructor starts at −2 (`FUN_14152f270` stores `0xfffffffe` at `+0x14c`), so no object's
/// id is −1. A scenario object takes its own id instead (`+0x150` with the flag `+0x154`,
/// `FUN_141541b80`), which is 0 or more and so never a reusable one. One that leaves its list
/// goes to the world's pool for its type (`RGE_Static_Object::recycle_out_of_game`,
/// `FUN_141546960`), from which the next of that type comes back, the last in first
/// (`FUN_141546870`), keeping its id (`recycle_in_to_game` gives none). The pools are not saved:
/// a game loaded from a save starts them empty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Recycling {
    /// `next_reusable_object_id`.
    pub next_id: ObjectId,
    /// The pool of type 60 (`world+0x1d8`): each missile's id and its flying flag (`+0x994`,
    /// which the object keeps), the last in last.
    pub missiles: Vec<(ObjectId, bool)>,
    /// The pool of type 25 (`world+0x1e0`): the ghosts' ids, the last in last.
    pub ghosts: Vec<ObjectId>,
}

#[cfg(test)]
pub(crate) mod tests;
