//! The members' moves to their slots (FD-036): the part of `BUnitGroup::update`
//! (`FUN_14103d910`, `0x14103e8e8`..`0x14103efba`) that brings each member of a formation to its
//! slot, with the member's steering (`FUN_141025fb0`) and the setter of a member's own move
//! (`FUN_141039f60`).

mod detour;
mod slot_steps;
mod to_slots;

#[cfg(test)]
use crate::groups::SpecificMove;
use crate::map::Distance;
use crate::math::image::group::{
    EPSILON as GROUP_EPSILON, ROUND_LENGTH as GROUP_ROUND_LENGTH,
    ROUND_RADIUS as GROUP_ROUND_RADIUS, ROUND_REACH as GROUP_ROUND_REACH,
    ROUND_STILL as GROUP_ROUND_STILL, ROUND_TIMER as GROUP_ROUND_TIMER,
};
use crate::movement::move_to::MOVE_TO as MOVEMENT_MOVE_TO;
use crate::objects::masters::ActionKind;
#[cfg(test)]
use crate::world::World;

/// A member this near its slot on the ground is in place (`DAT_142eb0c60`, `0x14102629e`).
const EPSILON: f32 = GROUP_EPSILON;
/// A member whose moves to its slot failed more than this many times in a row is moved aside
/// (`FUN_141025fb0`'s `0xa`).
const MOST_FAILURES: i32 = 10;
/// The action types and states the stop of a member without a slot passes over: a `MoveTo`
/// in the action list (slot 0x538 with 1), and a top action in state 4, 5 or 11 (slot 0x2b0).
const MOVE_TO: ActionKind = MOVEMENT_MOVE_TO;
const KEEPS_MOVING: [u8; 3] = [4, 5, 11];
/// The way round a record (`FUN_141025fb0` from `0x14102683b`): the short pather's radius is the
/// member's times this, and the position tests grow the records by as much (`DAT_142e7f384`).
const ROUND_RADIUS: f32 = GROUP_ROUND_RADIUS;
/// The short pather serves only a goal within this squared footprint gap (`DAT_14330ebcc`,
/// `FUN_140a847c0`).
const ROUND_REACH: f32 = GROUP_ROUND_REACH;
/// A direction to the slot this short on both axes is the group's forward (`DAT_142f49458`).
const ROUND_STILL: f32 = GROUP_ROUND_STILL;
/// The step aside's path may be at most this long (`DAT_142eb6e48`).
const ROUND_LENGTH: Distance = Distance::new(GROUP_ROUND_LENGTH);
/// The own move the way round gives a member: its timer and distance (`DAT_142eb1c98`).
const ROUND_TIMER: f32 = GROUP_ROUND_TIMER;

#[cfg(test)]
mod tests;
