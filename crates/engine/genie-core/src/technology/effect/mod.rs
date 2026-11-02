//! A research's completion and the effects it gives: `FUN_1413cdcd0` (the technology done, its
//! effect, the technologies that depend on it, the player's statistics), the effects'
//! `TRIBE_Effects::do_effect` (`FUN_141462440`, then the base `FUN_140ec2ac0`) with every kind
//! for every party, and what they reach: the player's attribute modifier (slots 0x150, 0x158,
//! 0x160 -> slot 0x148, `FUN_1416121b0`: the hooks `FUN_141614f40` around `FUN_14165a000`, the
//! objects' slots 0x78, 0x80, 0x88 and the masters' 0x18, 0x20, 0x28 of
//! [`crate::objects::masters`]), the resource commands (slots 0x1b0, 0x1b8, the player's attribute set
//! and add), the technology changes (`'e'`, `'f'`, `'g'`, kind 8), the enables, upgrades,
//! spawns and cost entries. The objects' parts an upgrade, a spawn and the objects' own masters
//! need are the objects' routines, which this file leaves to them.

mod attribute_modifier;
use genie_model::technology::effect::change;
mod commands;
mod completion;
mod object_slots;
mod resources;
mod tech_change;
mod upgrade;

pub(crate) use self::attribute_modifier::{MasterSprites, follow_master};
pub use self::change::Change;
use self::object_slots::{CAPPED, ONE, ROUND, has_actions, within_table};
#[cfg(test)]
use self::object_slots::{POPULATION, RESOURCE_19, charge_after};
use crate::GenieError;
#[cfg(test)]
use crate::math::rng::Stream;
#[cfg(test)]
use crate::technology::EffectCommand;
#[cfg(test)]
use crate::world::World;

/// A tech effect finds an object it changes gone from the lists it was found in.
const OBJECT_GONE: GenieError =
    GenieError::unreachable("an object a tech effect changes gone from its list");

/// The attribute the dependents' update sets to 1 when none of them is left (`+0x138`).
const ATTRIBUTE_39: usize = 39;

#[cfg(test)]
mod tests;
