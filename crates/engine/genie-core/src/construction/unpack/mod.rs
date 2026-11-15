//! A building's Unpack: `TRIBE_Action_Unpack` (action type 0x7d, vtable `0x142ed81a8`), which a
//! packed trebuchet (class 51) and `UNIT_ORDER` install on a building. It walks to its target or
//! its point within its range (state 11, its quiet `MoveTo`), then turns its building into its
//! master's transform (`+0x9ba`) and works (state 6): its timer runs up to the new master's
//! unpack time (its first train location's time) by the building's work rate, its sprite's frame
//! following it; once it got there, the building takes its standing sprite and its annexes, and
//! its unit AI is told to attack its target first. Its destructor before that turns the building
//! back.
//!
//! Its slots: the load `FUN_140ae59d0` ([`crate::world::load::top_actions`]), the destructor
//! (slot 0, `FUN_140ae67c0`, [`crate::world::World::unpack_torn_down`]), the start (slot 0x28,
//! `FUN_140ae68f0`), the update (slot 0x30, `FUN_140ae78f0`,
//! [`crate::objects::unit::Unit::unpack_part`]), the stop (slot 0x38, `FUN_140ae78d0`,
//! [`UnpackAction::halt`]) and `set_state` (slot 0x80, `FUN_140ae75d0`). The save (slot 0x20,
//! `FUN_140ae6d90`) writes the base, `+0x68`, `+0x6c`, `+0x70` and `+0x44`. Its other slots are
//! `RGE_Action`'s: its slot 0x68 is the base's change of target (`FUN_140a7a230`), and its slot
//! 0x40 takes no move (`FUN_140a78490`). The object stop reads its progress (`FUN_140ae6cd0`,
//! [`UnpackAction::progress`]).
//!
//! The constructor (`FUN_140ae58a0`), the start with 1 (`FUN_140ae68f0`), `set_state(11)` and the
//! installs are the tasks unpack-install-sites' and unpack-set-moving's: the load is its one
//! maker here.

mod unit;

#[cfg(test)]
mod tests;

use crate::GenieError;
use crate::GenieResult;
use crate::commands::QueueId;
use crate::construction::pack::{PackProgress, PackTime};
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Point;
use crate::map::Position;
use crate::math::conversions::cvttss2si;
use crate::math::fmath::{below_or_unordered, maxss};
use crate::math::image::unpack::PERCENT as UNPACK_PERCENT;
use crate::movement::move_to::MoveToAction;
use crate::objects::Frame;
use crate::objects::ObjectId;
use crate::objects::teardown::TornWalk;
use crate::world::PlayState;
use crate::world::Seconds;
use crate::world::WholeSeconds;
use derive_more::Display;
use genie_diff::Diff;

/// The progress of an Unpack in state 1 (`FUN_140ae6cd0`'s `MOV EAX, 0x64`): all of it.
const DONE: PackProgress = PackProgress::of_data(100);
/// The whole of an Unpack's progress, its percent (`DAT_14330ebcc`, `FUN_140ae6cd0`).
const PERCENT: f32 = UNPACK_PERCENT;

/// An Unpack's state (`+0x18`) with the data only that state holds: the constructor's 0, then
/// what `set_state` (slot 0x80, `FUN_140ae75d0`) writes. Every `set_state` drops the sub-actions
/// first (`FUN_140a98620` on `+0x50`), and only `set_state(11)` makes one, its quiet `MoveTo`.
#[derive(Clone, Copy, Debug, PartialEq, Diff, Display)]
#[display("{}", self.get())]
pub enum UnpackState {
    /// 0: made (the base constructor's, `FUN_140a73e90`), not started: its update answers 0.
    New,
    /// 1: done: its update answers 1 and the list deletes it.
    Done,
    /// 6: at work: each update adds the building's work to its timer.
    Working,
    /// 11: on its way to its target or its point.
    Moving {
        /// Its quiet `MoveTo` (`FUN_140aa0d30` or `FUN_140aa0df0`, then `FUN_140a7a170` with 1),
        /// none once the list's add deleted it.
        move_to: Option<MoveToAction>,
    },
    /// 13: failed: its update answers 1 as in state 1.
    Failed,
}

impl UnpackState {
    /// The state of a byte of the save or the game's memory, with the saved sub-action `walk`
    /// (state 11's `MoveTo`, the only one an Unpack makes).
    ///
    /// # Errors
    ///
    /// A byte that names no state of an Unpack, and a `MoveTo` in a state that does not walk.
    pub fn of_byte(value: u8, walk: Option<MoveToAction>) -> GenieResult<Self> {
        let state = match value {
            0 => Self::New,
            1 => Self::Done,
            6 => Self::Working,
            11 => return Ok(Self::Moving { move_to: walk }),
            13 => Self::Failed,
            _ => {
                return Err(GenieError::invalid_input("no state of an Unpack").with("state", value));
            }
        };
        if walk.is_some() {
            return Err(
                GenieError::invalid_input("a MoveTo of an Unpack that does not walk")
                    .with("state", value),
            );
        }
        Ok(state)
    }

    /// Its number.
    #[must_use]
    pub const fn get(self) -> u8 {
        match self {
            Self::New => 0,
            Self::Done => 1,
            Self::Working => 6,
            Self::Moving { .. } => 11,
            Self::Failed => 13,
        }
    }

    /// Its walk's `MoveTo` slot, none in a state that does not walk.
    pub const fn walk_mut(&mut self) -> Option<&mut Option<MoveToAction>> {
        match self {
            Self::Moving { move_to } => Some(move_to),
            _ => None,
        }
    }

    /// Its walk's `MoveTo`, none in any other state.
    #[must_use]
    pub const fn move_to(&self) -> Option<&MoveToAction> {
        match self {
            Self::Moving {
                move_to: Some(move_to_action),
            } => Some(move_to_action),
            _ => None,
        }
    }
}

/// An Unpack's status (`+0x6c`, `m_unpackStatus`): what its building went through.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, strum::FromRepr)]
#[repr(u8)]
pub enum UnpackStatus {
    /// 0: not at work yet (the constructor's and the load's first write), or its destructor
    /// turned the building back.
    Idle = 0,
    /// 1: at work (`set_state(6)`): the building took its master's transform, which the
    /// destructor undoes.
    Unpacking = 1,
    /// 2: the building took its standing sprite and its annexes (the update, before
    /// `set_state(1)`).
    Unpacked = 2,
}

impl UnpackStatus {
    /// The status of a byte of the save or the game's memory.
    ///
    /// # Errors
    ///
    /// A byte that names no status of an Unpack.
    pub fn of_byte(value: u8) -> GenieResult<Self> {
        Self::from_repr(value).ok_or_else(|| {
            GenieError::invalid_input("no status of an Unpack").with("status", value)
        })
    }

    /// Its number.
    #[must_use]
    pub const fn get(self) -> u8 {
        self as u8
    }

    /// Whether its destructor turns the building back (`FUN_140ae67c0`): in a played game
    /// (`FUN_140c896f0` with a world, `world+0x88` 0, [`PlayState::Playing`]) at work
    /// ([`UnpackStatus::Unpacking`]).
    #[must_use]
    pub fn is_undone_in(self, game_state: PlayState) -> bool {
        game_state == PlayState::Playing && self == Self::Unpacking
    }
}

impl Diff for UnpackStatus {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        differ.equal(self, other);
    }
}

/// `TRIBE_Action_Unpack`: its state (`+0x18`, its sub-action list `+0x50` in state 11), its target
/// (`+0x30`, the pointer `+0x20` the game's cache of the object the world's table holds of it),
/// its point (`+0x38`, `+0x3c`), its timer (`+0x44`), the queue id of the order that made it
/// (`+0x68`), its status (`+0x6c`) and its range (`+0x70`).
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct UnpackAction {
    /// `+0x18`.
    pub(crate) state: UnpackState,
    /// `+0x30`: the object it walks to and its unit AI attacks once the building unpacked; the
    /// update lets it go once the world loses it or it is past state 2.
    pub(crate) target: Option<ObjectId>,
    /// `+0x38`, `+0x3c`: the point it walks to without a target.
    pub(crate) point: Point,
    /// `+0x44`, `mTimer`: the work done, which `set_state(6)` starts at the unit's pack progress
    /// (`+0x88a`) and the update raises toward its master's unpack time: the pack time of the
    /// master it took ([`crate::construction::pack::pack_time`]).
    pub(crate) timer: Seconds,
    /// `+0x68`, `unique_id`: the queue id (`qId`, `+0x14`) of the `UNIT_ORDER` that made it, −1
    /// for the packed trebuchet's own.
    #[diff(eq)]
    pub(crate) unique_id: QueueId,
    /// `+0x6c`, `m_unpackStatus`.
    pub(crate) status: UnpackStatus,
    /// `+0x70`, `range`: the range its walk ends within, which its reach test measures.
    pub(crate) range: Distance,
}

/// Whether an Unpack's walk reached its target or its point ([`UnpackAction::reach_point`],
/// [`UnpackAction::reach_target`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub(crate) enum Reach {
    /// Within its range: the building unpacks (`set_state(6)`).
    Within,
    /// Out of its range, or a gap that is not a number.
    Out,
}

impl Reach {
    /// `COMISS range², gap²` (`0x140ae7a09` with `JC`, `0x140ae7b5d` with `JNC`): within when the
    /// squared `gap` is at most the squared `range`; a gap or a range that is not a number is out.
    fn of(range: Distance, gap: f32) -> Self {
        if below_or_unordered(range.squared().get(), gap) {
            Self::Out
        } else {
            Self::Within
        }
    }
}

impl UnpackAction {
    /// `set_state` (slot 0x80, `FUN_140ae75d0`) up to its case: its sub-actions go
    /// (`FUN_140a98620` on `+0x50`: its `MoveTo`'s teardown stops the unit, [`TornWalk::Stops`]),
    /// then the state. That is all of states 1 and 13; state 6 goes on with the transform
    /// ([`crate::objects::unit::Unit::unpack_set_working`]).
    pub(crate) fn set_bare(&mut self, state: UnpackState) -> TornWalk {
        let walk = TornWalk::from(self.state.move_to());
        self.state = state;
        walk
    }

    /// Its stop (slot 0x38, `FUN_140ae78d0`): done (`set_state(1)`); it answers 1, so the stop
    /// keeps the action list. Its status stays: at work, its destructor turns the building back.
    pub(crate) fn halt(&mut self) -> TornWalk {
        self.set_bare(UnpackState::Done)
    }

    /// Its update's work at 6 (`FUN_140ae78f0`): `work` is added to the timer (`ADDSS work,
    /// timer`), and [`PackTime::Reached`] once the timer is at the master's unpack `time`.
    pub(crate) fn work(&mut self, work: Seconds, time: WholeSeconds) -> PackTime {
        self.timer = work + self.timer;
        PackTime::of(self.timer, time)
    }

    /// The frame of its building's sprite of `frames` frames at its timer, toward the unpack time
    /// `time` (`FUN_140ae78f0`): `CVTTSS2SI` of (the frames times the timer, over the time), its
    /// low byte (`MOVZX EDI, BL`); none when that is not below the frames (`JGE`).
    pub(crate) fn frame(&self, time: WholeSeconds, frames: Frame) -> Option<Frame> {
        let time = Seconds::of_master(time).get();
        let shown = cvttss2si(frames.as_f32() * self.timer.get() / time) as u8;
        let frame = Frame::of_byte(shown);
        (frame < frames).then_some(frame)
    }

    /// `FUN_140ae6cd0`: its share done in percent, which the object stop keeps (`+0x88a`): all
    /// of it in state 1 ([`DONE`]); else `CVTTSS2SI` of its timer times 100 over its master's
    /// unpack `time`, the low half of the word (`MOV word ptr [R9], AX`).
    pub(crate) fn progress(&self, time: WholeSeconds) -> PackProgress {
        if self.state == UnpackState::Done {
            return DONE;
        }
        let share = self.timer.get() * PERCENT / Seconds::of_master(time).get();
        PackProgress::of_data(cvttss2si(share) as i16)
    }

    /// The reach test of its walk without a target (`0x140ae7a27` on), its unit at `at` of its
    /// master's radii `radius` (`+0x5d8`, `+0x5dc`): on each axis the gap to the point (`ANDPS`
    /// of the difference) less the radius, below 0 taken as 0 (`MAXSS 0, gap`: a NaN gap stays),
    /// squared and summed (y's first, `ADDSS`), against the range ([`Reach::of`]).
    pub(crate) fn reach_point(&self, at: Position, radius: Footprint) -> Reach {
        let dx = maxss(0.0, (self.point.x() - at.x()).abs() - radius.x());
        let dy = maxss(0.0, (self.point.y() - at.y()).abs() - radius.y());
        Reach::of(self.range, dy * dy + dx * dx)
    }

    /// The reach test of its walk to a target (`0x140ae79ee` on): the squared gap between the
    /// unit's and the target's footprints (`FUN_14118f8c0`, `gap`) against the range
    /// ([`Reach::of`]).
    pub(crate) fn reach_target(&self, gap: f32) -> Reach {
        Reach::of(self.range, gap)
    }
}
