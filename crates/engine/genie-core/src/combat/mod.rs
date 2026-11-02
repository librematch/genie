//! Combat: attacks, hits and missiles. The Attack action, the damage a hit does, a combat
//! object's passive abilities, the missiles and their flight, a unit's volleys and strikes back,
//! and a hit's victims.

pub mod abilities;
pub mod attack;
mod attack_powerup;
mod attack_target;
use genie_model::combat::charge;
pub mod damage;
use genie_model::combat::firepower;
pub mod missile;
pub(crate) mod victim_reads;
pub(crate) mod victims;
pub(crate) mod volley;

pub use self::attack_powerup::AttackPowerup;
pub use self::attack_target::{AttackTarget, AttackTargetId};
pub use self::charge::{Charge, ChargeRate};
pub use self::firepower::{AttackCount, Firepower, MissileCount};
