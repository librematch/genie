//! A group's update (FD-036): `BUnitGroup::update` (`FUN_14103d910`), which the world runs for
//! the groups a player owns before the player's objects, and the deletion of a group whose update
//! returns 0 (the world list's unlink, then `BUnitGroup`'s destructor, `FUN_140fe1920`).

use crate::GenieError;
use crate::groups::GroupCommandKind;
use crate::groups::GroupFlags;
use crate::groups::MotionFlags;
use crate::groups::UnitGroup;
use crate::map::Distance;
use crate::movement::PathFlags;
use crate::objects::masters::ActionType;
use crate::world::Millis;

mod attack;
mod awake;
mod checks;
mod entry;
mod moves;
mod release;
mod send;

pub use self::awake::IDLE_SPAN;

/// The group's current command (`mCurrentCommand`, `+0x3a8`) without one.
const NO_COMMAND: GroupCommandKind = GroupCommandKind::NONE;
/// The group commands the update tells apart: an attack, a build, a repair, a relic's pick-up
/// and its delivery.
const ATTACK: GroupCommandKind = GroupCommandKind::ATTACK;
const BUILD: GroupCommandKind =
    GroupCommandKind::of_action(crate::objects::masters::action_type::BUILD);
const REPAIR: GroupCommandKind = GroupCommandKind::of_action(crate::construction::repair::REPAIR);
const PICK_UP_RELIC: GroupCommandKind = GroupCommandKind::named(0x84);
const DELIVER_RELIC: GroupCommandKind = GroupCommandKind::named(0x88);
/// The action types of the tasks that pick up and deliver relics (`FUN_1415c60b0`'s third
/// argument, as the commands).
const RELIC_PICKUP_ACTION: ActionType = ActionType::named(0x84);
const RELIC_DELIVERY_ACTION: ActionType = ActionType::named(0x88);
/// The nearest member's target starts this far (`FUN_141007ca0`, `DAT_14330ebf4`).
const FARTHEST: f32 = crate::math::image::group::FARTHEST;
/// A follow (`mCurrentCommand` 2).
const FOLLOW: GroupCommandKind = GroupCommandKind::FOLLOW;
/// A turn of the motion above this lays the formation out anew (`DAT_142eb6e44`, π/4).
const TURN_LIMIT: f32 = crate::math::image::facing::QUARTER_PI;
/// How far ahead of the group its front is looked for (`DAT_14330ebcc`).
const FRONT_AHEAD: Distance = Distance::new(crate::math::image::movement::LOOK_AHEAD);
/// A total spacing above this lays the formation out anew on a changed count
/// (`DAT_142ec9eb4`).
const RELAYOUT_SPACING: Distance = Distance::new(crate::math::image::missile::VOLLEY_REACH_MARGIN);
/// The share of the lead's line of sight a follow keeps within (`DAT_142e7f384`).
const FOLLOW_SHARE: f32 = crate::math::image::group::FRONT_SHARE;
/// A follow's target this near the top path's end (squared, `DAT_14330eb8c`) keeps the move.
const CLOSE: f32 = crate::math::image::group::CLOSE;
/// The differentiator groups' members the update lists for the members' moves, at most this
/// many (`DAT_143f15860`: `0x8b < n` ends the list).
const DIFFERENTIATORS: usize = 0x8c;
/// The idle timer past which an idle group draws it anew (the immediate `0xbb8`).
pub const IDLE_LIMIT: Millis = Millis::new(3000);
/// A trade cart's standing and walking sprites are its trade task's while it carries goods
/// (`FUN_14106a940`: its carried attributes, `+0x7a0`, which the slice does not hold).
const TRADE_CART_SPRITES: GenieError =
    GenieError::unsupported("a trade cart's standing or walking sprite (its goods, FUN_14106a940)");

/// The sprites slots 0x660, 0x658 and 0x650 show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shown {
    /// Slot 0x660's standing sprite.
    Standing,
    /// Slot 0x658's walking sprite.
    Walking,
    /// Slot 0x650's running sprite.
    Running,
}

/// The update of the other commands on a target (`FUN_141040bd0`), and the attack formation's
/// refill (`FUN_1410163f0`): not modelled.
const OTHER_COMMANDS: GenieError = GenieError::unsupported(
    "a group command's update other than a move, a follow or an attack (FUN_141040bd0)",
);

impl UnitGroup {
    /// `IsIdle` (`FUN_141017ad0`): idle with [`GroupFlags::MEMBERS_SENT`] (`cConsiderIdle`), or on
    /// a move (command 1) while it patrols ([`UnitGroup::patrols`]); else not while its motion
    /// moves or waits to move; else idle.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        if self.flags.contains(GroupFlags::MEMBERS_SENT) {
            return true;
        }
        if self.current_command == GroupCommandKind::MOVE && self.patrols() {
            return true;
        }
        !(self.motion.moving() || self.motion.waits_to_move())
    }

    /// The group patrols: its top path's flags have bit 0 (`FUN_141017c40`, the path of
    /// `FUN_141046500`) or its [`GroupFlags::PATROL`] (`FUN_141017c30`).
    #[must_use]
    pub fn patrols(&self) -> bool {
        crate::groups::formation::top_path(&self.motion.paths)
            .is_some_and(|p| p.path_flags.contains(PathFlags::LOOPS))
            || self.flags.contains(GroupFlags::PATROL)
    }

    /// The group's vtable slot 8 after its update (`BUnitGroup::updateEnd`, `FUN_141040b50`): a
    /// group whose motion moved ([`MotionFlags::MOVED`]) adds its new position to the front of its
    /// stored positions ([`crate::groups::PointLine::add_at_front`]); the motion's flags lose
    /// [`MotionFlags::MOVED`], and [`MotionFlags::NEW`] with it when set, which sets the group's
    /// [`GroupFlags::RELAYOUT`] (`FUN_14104a240`).
    pub fn update_end(&mut self) -> Result<(), GenieError> {
        let g = self;
        if g.motion.flags.contains(MotionFlags::MOVED) {
            let [point, forward, up, right] = g.motion.new_position;
            g.stored_positions.add_at_front(crate::groups::LinePoint {
                point,
                forward,
                up,
                right,
            })?;
        }
        let flags = g.motion.flags;
        g.motion.flags.remove(MotionFlags::MOVED);
        if flags.contains(MotionFlags::NEW) {
            g.motion.flags.remove(MotionFlags::MOVED | MotionFlags::NEW);
            g.flags |= GroupFlags::RELAYOUT;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
