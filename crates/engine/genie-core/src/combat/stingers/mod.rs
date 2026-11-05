//! An object's stingers (`mActiveStingers`, `+0xb58`): what its attackers' hits hand it
//! ([`hand_off`](fn@hand_off), `FUN_14107bdf0`) from their masters' stinger tasks
//! ([`stinger_tasks`]), and the sums the stinger update rebuilds from them in each update
//! ([`World::object_stingers`](crate::world::World::object_stingers), `FUN_1412092e0`).
//!
//! The game keeps an object's stinger tasks in `+0x938`: slot 277 (`FUN_14107ddb0`) refills
//! them whenever the object's master changes (its constructor and load, a master change
//! `FUN_14107cb70`, TRIBE's slot 277 for an effect's master). genie derives them from the
//! master where the hand-off reads them, as it does the aura groups ([`crate::combat::aura`]).

mod hand_off;
mod update;

pub(crate) use self::hand_off::{Handed, hand_off};

use crate::combat::ClassOrMaster;
use crate::combat::powerup::PowerupAttribute;
use crate::math::conversions::cvttss2si;
use crate::objects::ObjectId;
use crate::objects::masters::ActionKind;
use crate::objects::masters::Master;
use crate::objects::masters::TaskFlags;
use crate::players::attribute::AttributeId;
use crate::world::Millis;
use crate::world::WorldTime;

/// The type of a stinger task (157, `0x9d`): a hit of its object hands the target, or the object
/// itself, a stinger (slot 277's `FUN_14107ddb0`).
pub(crate) const STINGER: ActionKind = ActionKind::named(0x9d);

/// A stinger's value (`+4`, a float: its task's `work_val1`): what it adds to its kind's sum,
/// or with [`TaskFlags::STINGER_SCALED`] its share of the master's value.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct StingerValue(f32);

impl StingerValue {
    /// The float `value`.
    #[must_use]
    pub const fn new(value: f32) -> Self {
        Self(value)
    }

    /// The float.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl genie_diff::Diff for StingerValue {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

/// How long a task's stinger lasts (slot 277's entry `+0x10`: its task's `work_val2` as whole
/// seconds, `CVTTSS2SI`); below 0 for good.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StingerSeconds(i32);

impl StingerSeconds {
    /// The whole seconds `value`.
    #[must_use]
    pub const fn new(value: i32) -> Self {
        Self(value)
    }

    /// The end of a stinger handed at `now` (`FUN_14107bdf0`): `now` plus the seconds in
    /// milliseconds (32-bit wrapping arithmetic), or for good (`-1`, the largest time) below 0.
    #[must_use]
    pub const fn end(self, now: WorldTime) -> WorldTime {
        if self.0 < 0 {
            WorldTime::from_millis(u32::MAX)
        } else {
            now.after(Millis::new(self.0.wrapping_mul(1000).cast_unsigned()))
        }
    }
}

/// Who a task's stinger goes to (slot 277's entry `+0x14`, a byte: its task's `work_range` is 0,
/// `UCOMISS`, which a NaN is not).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StingerReceiver {
    /// The object the hit struck.
    Target,
    /// The attacker itself (a TRIBE combat object only: `__RTDynamicCast`).
    Attacker,
}

/// A stinger task as slot 277 keeps it (`FUN_14107ddb0`, an entry of `+0x938`, 40 bytes): one
/// of the master's tasks of type [`STINGER`], in their order. Its `+0x20` (the task's proceeding
/// sprite, `+0x48`) goes with the stinger to the renderer (`+0x9f8`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StingerTask {
    /// `+0x0`: the objects it stings ([`ClassOrMaster::of_task`]).
    pub(crate) target: ClassOrMaster,
    /// `+0x4`: the attacker's player's attribute that enables it (`attribute_type2`, `+0x22`),
    /// none for always.
    pub(crate) enablement: Option<AttributeId>,
    /// `+0x8`: what the stinger changes (`search_wait_time`, `+0x1c`, `CVTTSS2SI`).
    pub(crate) kind: PowerupAttribute,
    /// `+0xc`: its value (`work_val1`, `+0x28`).
    pub(crate) value: StingerValue,
    /// `+0x10`: how long it lasts (`work_val2`, `+0x2c`, `CVTTSS2SI`).
    pub(crate) seconds: StingerSeconds,
    /// `+0x14`: who gets it (`work_range`, `+0x30`, at 0 the attacker).
    pub(crate) receiver: StingerReceiver,
    /// `+0x18`: its flags (`+0x15`).
    pub(crate) flags: TaskFlags,
}

/// Slot 277's stinger tasks of an object of `master` (`FUN_14107ddb0`): its tasks of type
/// [`STINGER`], in their order.
#[must_use]
pub(crate) fn stinger_tasks(master: &Master) -> Vec<StingerTask> {
    master
        .action_part()
        .tasks
        .iter()
        .filter(|task| task.action_kind == STINGER)
        .map(|task| StingerTask {
            target: ClassOrMaster::of_task(task),
            enablement: task.attribute_kind2,
            kind: PowerupAttribute::of_task(task.wait_time),
            value: StingerValue::new(task.work_val1.get()),
            seconds: StingerSeconds::new(cvttss2si(task.work_val2.get())),
            receiver: if task.work_range.get() == 0.0 {
                StingerReceiver::Attacker
            } else {
                StingerReceiver::Target
            },
            flags: task.combat_level_flag,
        })
        .collect()
}

/// An active stinger (an entry of `mActiveStingers`, `+0xb58`, 32 bytes, saved). Its `+0x18`,
/// the sprite pointer of the process that handed it, only the renderer reads (`+0x9f8`); the
/// load reads all 32 bytes raw (`0x141224521`), so a loaded stinger's pointer is another
/// process's.
#[derive(Clone, Copy, Debug, PartialEq, genie_diff::Diff)]
pub struct ActiveStinger {
    /// `+0x0`: the world time it ends at (unsigned; the largest for good).
    pub(crate) end: WorldTime,
    /// `+0x4`: its value.
    pub(crate) value: StingerValue,
    /// `+0x8`: what it changes.
    pub(crate) kind: PowerupAttribute,
    /// `+0xc`: its task's target, which [`TaskFlags::STINGER_UNIQUE`] counts by with the kind.
    pub(crate) key: ClassOrMaster,
    /// `+0x10`: the object whose hit handed it.
    pub(crate) attacker: Option<ObjectId>,
    /// `+0x14`: its task's flags (the low byte of a 32-bit field).
    pub(crate) flags: TaskFlags,
}

/// The size of a saved stinger (`+0xb58`'s entries).
pub const SAVED_STINGER: usize = 32;

impl ActiveStinger {
    /// The stinger a hit of `attacker` at `now` hands for `task` (`FUN_14107bdf0`).
    #[must_use]
    pub(crate) const fn handed(task: &StingerTask, attacker: ObjectId, now: WorldTime) -> Self {
        Self {
            end: task.seconds.end(now),
            value: task.value,
            kind: task.kind,
            key: task.target,
            attacker: Some(attacker),
            flags: task.flags,
        }
    }

    /// A saved stinger (the load's 32 raw bytes, `FUN_14121ec90`), little-endian.
    #[must_use]
    pub fn of_save(raw: &[u8; SAVED_STINGER]) -> Self {
        let word = |at: usize| {
            let mut bytes = [0; 4];
            bytes.copy_from_slice(&raw[at..at + 4]);
            bytes
        };
        Self {
            end: WorldTime::from_millis(u32::from_le_bytes(word(0))),
            value: StingerValue::new(f32::from_le_bytes(word(4))),
            kind: PowerupAttribute::of_data(i32::from_le_bytes(word(8))),
            key: ClassOrMaster::of_data(i32::from_le_bytes(word(0xc))),
            attacker: ObjectId::of_field(i32::from_le_bytes(word(0x10))),
            flags: TaskFlags::from_bits_retain(raw[0x14]),
        }
    }
}

#[cfg(test)]
mod tests;
