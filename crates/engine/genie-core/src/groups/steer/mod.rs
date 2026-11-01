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
use crate::objects::masters::ActionType;
#[cfg(test)]
use crate::world::World;

/// A member this near its slot on the ground is in place (`DAT_142eb0c60`, `0x14102629e`).
const EPSILON: f32 = crate::math::image::group::EPSILON;
/// The members' obstruction tests grow the records by the member's radius times this
/// (`DAT_142e7f384`).
const CLEARANCE: Distance = Distance::new(crate::math::image::group::CLEARANCE);
/// A member whose moves to its slot failed more than this many times in a row is moved aside
/// (`FUN_141025fb0`'s `0xa`).
const MOST_FAILURES: i32 = 10;
/// The timer of the move aside after too many failures (`DAT_142eb5690`).
const ASIDE_TIMER: f32 = crate::math::image::group::ASIDE_TIMER;
/// A member within this many radii of its slot, or this far, walks at its top speed
/// (`DAT_14330ebb8`, loaded once at `0x1410279d1` for both).
const SLOT_REACH: f32 = crate::math::image::group::SLOT_REACH;
/// Nearer its slot than [`SLOT_REACH`], a member catches up at its top speed times
/// [`CATCH_UP`], less the nearer it is within this (`DAT_142eb1c98`).
const CATCH_UP_SPAN: f32 = crate::math::image::group::CATCH_UP_SPAN;
/// The catching up's share of the top speed (`DAT_142ed66b4`).
const CATCH_UP: f32 = crate::math::image::group::CATCH_UP;
/// A moving object whose top speed is 0 and whose radius on either axis is above this links its
/// tiles at a new place (`FUN_14108c320`, `DAT_142e7f384`).
const LINKED_RADIUS: f32 = crate::math::image::group::LINKED_RADIUS;
/// The action types and states the stop of a member without a slot passes over: a `MoveTo`
/// in the action list (slot 0x538 with 1), and a top action in state 4, 5 or 11 (slot 0x2b0).
const MOVE_TO: ActionType = crate::movement::move_to::MOVE_TO;
const KEEPS_MOVING: [u8; 3] = [4, 5, 11];
/// The way round a record (`FUN_141025fb0` from `0x14102683b`): the short pather's radius is the
/// member's times this, and the position tests grow the records by as much (`DAT_142e7f384`).
const ROUND_RADIUS: f32 = crate::math::image::group::ROUND_RADIUS;
/// The short pather serves only a goal within this squared footprint gap (`DAT_14330ebcc`,
/// `FUN_140a847c0`).
const ROUND_REACH: f32 = crate::math::image::group::ROUND_REACH;
/// A direction to the slot this short on both axes is the group's forward (`DAT_142f49458`).
const ROUND_STILL: f32 = crate::math::image::group::ROUND_STILL;
/// The step aside's path may be at most this long (`DAT_142eb6e48`).
const ROUND_LENGTH: Distance = Distance::new(crate::math::image::group::ROUND_LENGTH);
/// The own move the way round gives a member: its timer and distance (`DAT_142eb1c98`).
const ROUND_TIMER: f32 = crate::math::image::group::ROUND_TIMER;

#[cfg(test)]
mod tests;
