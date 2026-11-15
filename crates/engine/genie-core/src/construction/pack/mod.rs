//! A building's Pack: `TRIBE_Action_Pack` (action type 0x7c, vtable `0x142ed7978`), which a
//! trebuchet (class 54) and `UNIT_ORDER`'s case 1 install on a building. At work its timer runs
//! up to its master's pack time (its first train location's time) by the building's work rate,
//! its sprite's frame following it; once it got there, the building turns into its master's
//! transform (`+0x9ba`), its unit AI told to attack its target first. Its destructor before that
//! puts the building's standing sprite and its annexes back.
//!
//! Its slots: the load `FUN_140ad5090` ([`crate::world::load::top_actions`]), the destructor
//! (slot 0, `0x140ad6070` over `FUN_140ad5370`, [`crate::world::World::pack_torn_down`]), the
//! start (slot 0x28, `0x140ad6150`, [`crate::objects::unit::Unit::start_pack`]), the update (slot
//! 0x30, `0x140ad69d0`, [`crate::objects::unit::Unit::pack_part`]), the stop (slot 0x38,
//! `0x140ad69b0`, [`PackAction::halt`]) and `set_state` (slot 0x80, `0x140ad68e0`,
//! [`crate::objects::unit::Unit::set_pack_state`]). The save (slot 0x20, `0x140ad6230`) writes the
//! base, `+0x6c`, `+0x68` and `+0x44`. Its other slots are `RGE_Action`'s: its slot 0x68 is the
//! base's change of target (`FUN_140a7a230`), and it makes no sub-action. The object stop reads
//! its progress (`FUN_140ad6170`, [`PackAction::progress`]).
//!
//! Its makers are the load and the constructor (`FUN_140ad4ff0`,
//! [`crate::world::World::construct_pack`]), which a trebuchet's work order installs
//! ([`crate::world::World::install_pack`]), as does the unit's slot 0x340 on an AI's move
//! ([`crate::world::World::install_pack_to_move`]); `UNIT_ORDER`'s case 1 is the task
//! pack-unit-order.

mod install;
mod unit;

#[cfg(test)]
mod tests;

use crate::GenieError;
use crate::GenieResult;
use crate::commands::QueueId;
use crate::math::conversions::cvttss2si;
use crate::math::image::pack::PERCENT as PACK_PERCENT;
use crate::objects::Frame;
use crate::objects::ObjectId;
use crate::objects::masters::Master;
use crate::world::Seconds;
use crate::world::WholeSeconds;
use genie_diff::Diff;
use strum::FromRepr;

/// A Pack or an Unpack whose master has no building part, which reads `+0x9ba` and `+0x990` of a
/// building master: every master that packs or unpacks is a building (type 80).
pub(crate) const PACK_NOT_BUILDING: GenieError =
    GenieError::unreachable("a Pack or Unpack of a master without its building part");

/// The progress of a Pack in state 1 (`FUN_140ad6170`'s `MOV EAX, 0x64`): all of it. The
/// Unpack's progress has an immediate of its own with the same value (`construction::unpack`'s
/// `DONE`).
const DONE: PackProgress = PackProgress(100);
/// The whole of a Pack's progress, its percent (`DAT_14330ebcc`): `FUN_140ad6170`'s and
/// `FUN_140ad60b0`'s (`DIVSS` at `0x140ad613e`).
const PERCENT: f32 = PACK_PERCENT;

/// A Pack's state (`+0x18`): the constructor's 0, then what `set_state` (slot 0x80,
/// `0x140ad68e0`) writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, FromRepr)]
#[repr(u8)]
pub enum PackState {
    /// 0: made (the base constructor's, `FUN_140a73e90`), not started: its update answers 0.
    New = 0,
    /// 1: done: its update answers 1 and the list deletes it.
    Done = 1,
    /// 6: at work: each update adds the building's work to its timer.
    Working = 6,
    /// 13: failed: its update answers 1 as in state 1. No slot of the Pack's writes it.
    Failed = 13,
}

impl PackState {
    /// The state of a byte of the save or the game's memory.
    ///
    /// # Errors
    ///
    /// A byte that names no state of a Pack.
    pub fn of_byte(value: u8) -> GenieResult<Self> {
        Self::from_repr(value)
            .ok_or_else(|| GenieError::invalid_input("no state of a Pack").with("state", value))
    }

    /// Its number.
    #[must_use]
    pub const fn get(self) -> u8 {
        self as u8
    }
}

impl Diff for PackState {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        differ.equal(self, other);
    }
}

/// A Pack's status (`+0x68`, `m_packStatus`): what its building went through.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, FromRepr)]
#[repr(u8)]
pub enum PackStatus {
    /// 0: not at work yet (the constructor's and the load's first write), or its destructor
    /// put the building back.
    Idle = 0,
    /// 1: at work (`set_state(6)`): the building took its construction sprite and lost its
    /// annexes, which the destructor puts back.
    Packing = 1,
    /// 2: the building turns into its master's transform (the update, before slot 0x90).
    Transformed = 2,
}

impl PackStatus {
    /// The status of a byte of the save or the game's memory.
    ///
    /// # Errors
    ///
    /// A byte that names no status of a Pack.
    pub fn of_byte(value: u8) -> GenieResult<Self> {
        Self::from_repr(value)
            .ok_or_else(|| GenieError::invalid_input("no status of a Pack").with("status", value))
    }

    /// Its number.
    #[must_use]
    pub const fn get(self) -> u8 {
        self as u8
    }
}

impl Diff for PackStatus {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        differ.equal(self, other);
    }
}

/// An action object's pack progress (`+0x88a`, a short the save does not hold): the share of its
/// Pack or Unpack done in percent, which its stop keeps (`FUN_14106fa60`) for the next one to
/// start from (`FUN_140ad60b0`, the Unpack's `0x140ae591c` and `0x140ae78a4`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PackProgress(i16);

impl PackProgress {
    /// None (0): the load's (`FUN_14106ec00`), a new object's and a stop's under any other
    /// action.
    pub const NONE: Self = Self(0);

    /// The progress of the short `value`.
    #[must_use]
    pub const fn of_data(value: i16) -> Self {
        Self(value)
    }

    /// The short.
    #[must_use]
    pub const fn get(self) -> i16 {
        self.0
    }
}

impl Diff for PackProgress {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

/// `TRIBE_Action_Pack`: its state (`+0x18`), its target (`+0x30`, the pointer `+0x20` the
/// game's cache of the object the world's table holds of it), its timer (`+0x44`), its status
/// (`+0x68`) and the queue id of the order that made it (`+0x6c`).
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct PackAction {
    /// `+0x18`.
    pub(crate) state: PackState,
    /// `+0x30`: the object its unit AI attacks once the building transformed; the trebuchet's
    /// order takes it (slot 0x68), and the update lets it go once the world loses it or it is
    /// past state 2.
    pub(crate) target: Option<ObjectId>,
    /// `+0x44`, `mTimer`: the work done, which the update raises toward its master's pack time
    /// ([`pack_time`]).
    pub(crate) timer: Seconds,
    /// `+0x68`, `m_packStatus`.
    pub(crate) status: PackStatus,
    /// `+0x6c`, `unique_id`: the queue id (`qId`, `+0x14`) of the `UNIT_ORDER` that made it, −1
    /// for the trebuchet's own.
    #[diff(eq)]
    pub(crate) unique_id: QueueId,
}

/// Whether a Pack's or an Unpack's timer reached its master's pack time in its update
/// ([`PackAction::work`], [`crate::construction::unpack::UnpackAction::work`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub(crate) enum PackTime {
    /// The timer is below the pack time, or not a number: the work goes on.
    Left,
    /// The timer is at the pack time or past it.
    Reached,
}

impl PackTime {
    /// `COMISS timer, time` with `JC` (the Pack's `0x140ad6cd9`, `0x140ad5430`; the Unpack's
    /// `0x140ae7d23`): reached when `timer` is at least `time`; a timer or a time that is not a
    /// number has not reached it.
    pub(crate) fn of(timer: Seconds, time: WholeSeconds) -> Self {
        if Seconds::of_master(time) <= timer {
            Self::Reached
        } else {
            Self::Left
        }
    }
}

/// A master's pack time: the train time of its first train location (`+0x898`'s first entry,
/// `+0x2`), 0 without one (`FUN_140ad60b0`, `FUN_140ad6170`, `0x140ad69d0`, `FUN_140ad5370`).
pub(crate) fn pack_time(master: &Master) -> WholeSeconds {
    master
        .creatable_part()
        .train_locations
        .first()
        .map_or(WholeSeconds::ZERO, |location| location.time)
}

impl PackAction {
    /// The constructor's own part (`FUN_140ad4ff0`, after the base `FUN_140a73e90` with 1): made
    /// (state 0) with no target, its status 0, its `unique_id` (`+0x6c`), the type 0x7c, its
    /// timer 0 and then the share `progress` of its unit's master's pack `time`
    /// ([`PackAction::take_progress`]).
    pub(crate) fn new(unique_id: QueueId, time: WholeSeconds, progress: PackProgress) -> Self {
        let mut pack = Self {
            state: PackState::New,
            target: None,
            timer: Seconds::ZERO,
            status: PackStatus::Idle,
            unique_id,
        };
        pack.take_progress(time, progress);
        pack
    }

    /// `FUN_140ad60b0`: the timer (`+0x44`) takes the share `progress` (its unit's `+0x88a`, a
    /// signed short) of the pack `time` (a signed short): their `i32` product (`MOVSX`, `IMUL`)
    /// as a float (`CVTDQ2PS`) over [`PERCENT`] (`DIVSS`). The product of two shorts never
    /// overflows.
    pub(crate) fn take_progress(&mut self, time: WholeSeconds, progress: PackProgress) {
        let share = i32::from(time.get()) * i32::from(progress.get());
        self.timer = Seconds::new(share as f32 / PERCENT);
    }

    /// `set_state(6)`'s writes of the Pack itself (`0x140ad68e0`): at work, its status 1, its
    /// timer the share `progress` of the pack `time` ([`PackAction::take_progress`]). The game
    /// writes the timer 0 first (`0x140ad6959`), which `FUN_140ad60b0` writes over at once.
    pub(crate) fn go_to_work(&mut self, time: WholeSeconds, progress: PackProgress) {
        self.state = PackState::Working;
        self.status = PackStatus::Packing;
        self.take_progress(time, progress);
    }

    /// `set_state` (slot 0x80, `0x140ad68e0`) to 1: the state alone, as the stop and the update
    /// write it on the Pack they hold ([`crate::objects::unit::Unit::set_pack_state`] is the
    /// whole routine on the list's Pack).
    const fn finish(&mut self) {
        self.state = PackState::Done;
    }

    /// Its stop (slot 0x38, `0x140ad69b0`): done (`set_state(1)`); it answers 1, so the stop
    /// keeps the action list. Its status stays: at work, its destructor puts the building back.
    pub(crate) const fn halt(&mut self) {
        self.finish();
    }

    /// Its update's work at 6 (`0x140ad69d0`): `work` is added to the timer (`ADDSS work,
    /// timer`), and [`PackTime::Reached`] once the timer is at the master's pack `time`.
    pub(crate) fn work(&mut self, work: Seconds, time: WholeSeconds) -> PackTime {
        self.timer = work + self.timer;
        PackTime::of(self.timer, time)
    }

    /// The frame of its building's sprite of `frames` frames at its timer, toward the pack time
    /// `time` (`0x140ad69d0`): `CVTTSS2SI` of (the time less the timer, times the frames, over the
    /// time), its low byte (`MOVZX EDI, BL`); none when that is not below the frames (`JGE`).
    pub(crate) fn frame(&self, time: WholeSeconds, frames: Frame) -> Option<Frame> {
        let time = Seconds::of_master(time).get();
        let shown = cvttss2si((time - self.timer.get()) * frames.as_f32() / time) as u8;
        let frame = Frame::of_byte(shown);
        (frame < frames).then_some(frame)
    }

    /// `FUN_140ad6170`: its share done in percent, which the object stop keeps (`+0x88a`): all
    /// of it in state 1 ([`DONE`]); else `CVTTSS2SI` of its timer times 100 over its master's pack
    /// `time`, the low half of the word (`MOV word ptr [R9], AX`).
    pub(crate) fn progress(&self, time: WholeSeconds) -> PackProgress {
        if self.state == PackState::Done {
            return DONE;
        }
        let share = self.timer.get() * PERCENT / Seconds::of_master(time).get();
        PackProgress(cvttss2si(share) as i16)
    }
}
