//! Units inside other objects: the garrison order, the Enter action, the unloads and the
//! Transport action.
//!
//! This file holds `UNIT_ORDER`'s garrison (action 5 on a building, `FUN_14161e370` ->
//! `FUN_141621220`), which sends each unit to a building that takes it in, nearest first, each
//! building up to its capacity, and gives each building's units, through the order splitter, the
//! garrison work on it (slot 0x7e8 with task filter 3); and a building's test whether it takes a
//! unit in (`FUN_14120eb10`).

use crate::GenieError;
use crate::vision::SightTiles;

mod class_flags;
mod container_id;
mod counts;
mod damage_rate;
mod enter;
pub(crate) use self::enter::EnterSlot;
mod enter_state;
mod firepower;
mod holder;
mod inside_count;
mod let_out;
pub(crate) use self::let_out::HOUSED_NOT_LIVE;
mod meeting;
mod nested;
mod outcomes;
mod raider_countdown;
mod relics;
pub(crate) use self::relics::Kept;
mod scope;
mod shelter;
mod takes_in;
pub mod transport;
pub(crate) mod unit;
mod unload;
mod walk_in;

pub use self::class_flags::ClassFlags;
pub use self::container_id::{Container, ContainerId};
pub use self::counts::GarrisonCounts;
pub use self::damage_rate::DamageRate;
pub use self::enter_state::EnterState;
pub use self::firepower::{attack_rate, firepower, share};
pub use self::inside_count::InsideCount;
use self::meeting::{HALF_TILE, UNPLACED};
pub use self::meeting::{LandingWalk, MeetAt, MeetingTarget, Walls, meet_at, walled_off};
pub(crate) use self::meeting::{UNREACHED, enter_view, ship_reaches};
pub use self::nested::NestedCount;
pub use self::outcomes::{EnterOutcome, LetOut, WorkReturn};
pub use self::raider_countdown::RaiderCountdown;
pub use self::scope::{GarrisonScope, ShelterReach};
pub(crate) use self::shelter::Shelters;
use self::takes_in::has_garrison_task;
pub use self::takes_in::{GarrisonSwitch, class_has_flags, garrison_kind_takes, is_garrison_task};
use self::unload::NO_POINT;
#[cfg(test)]
use self::unload::unload_group_takes;
pub use self::walk_in::{ENTER, EnterAction, EnterKind, EnterTarget, WalkStart, Walking};
pub(crate) use self::walk_in::{EnterReads, NO_SPEED, SIEGE_UNLOAD};
use crate::math::image::garrison::{CLASS_FLAGS as GARRISON_CLASS_FLAGS, FAR as GARRISON_FAR};
#[cfg(test)]
use crate::movement::move_to::MoveTo;
#[cfg(test)]
use crate::objects::masters::{Master, Masters, Task};
#[cfg(test)]
use crate::orders::GARRISON;
#[cfg(test)]
use crate::world::World;

/// The squared footprint gap past which a garrison order sends no unit (`FUN_141621220` with 0:
/// no limit).
const FAR: f32 = GARRISON_FAR;
/// The circle a building sees while a unit is inside it (`FUN_141212e10`, `FUN_141213120`:
/// slots 0x1d0 and 0x1d8 with 5).
pub(crate) const INSIDE_SIGHT: SightTiles = SightTiles::new(5);
/// An enter into an object the world holds asleep.
const ENTER_SLEEPING: GenieError = GenieError::unsupported("an enter into a sleeping object");
/// An enter without its owner's masters.
pub(crate) const NO_MASTERS: GenieError = GenieError::unsupported("an enter without the masters");
/// An entering unit without a place on the map.
pub(crate) const ENTERING_UNPLACED: GenieError =
    GenieError::unsupported("an entering unit without a place");

/// The ids a command's check reads (`FUN_140e558b0`): a human player's first 60, a computer
/// player's first 500.
const HUMAN_IDS: usize = 60;
const COMPUTER_IDS: usize = 500;

/// The class-flags table `FUN_1411205e0` reads (`DAT_1439e37b0`, a u32 per class; its count,
/// `DAT_1439e2cf8`, is 64), as the image holds it (nothing writes it).
const CLASS_FLAGS: [u32; 64] = GARRISON_CLASS_FLAGS;

#[cfg(test)]
mod counts_tests;
#[cfg(test)]
mod nested_tests;
#[cfg(test)]
mod relics_tests;
#[cfg(test)]
mod siege_tests;
#[cfg(test)]
pub(crate) mod tests;
