//! The task a group's member takes on a target (FD-036): the unit's slot 0x130
//! (`RGE_Action_Object::slot38`, `FUN_14106b8b0`), which `BUnitGroup::command` asks of its
//! first member that can act to choose the command a group's order runs.

use crate::GenieError;
use crate::groups::CommandArgs;
use crate::groups::GroupId;
use crate::map::Distance;
use crate::map::Point;
use crate::math::image::group::MEMBER_MIN_HP;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::masters::ActionKind;
use crate::objects::masters::MasterKind;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::TaskGroup;
use crate::objects::masters::{Master, Masters, Task};
use crate::players::Amount;
use crate::unit_ai::task_finder::TaskOn;
use crate::unit_ai::task_finder::TaskTarget;
use crate::unit_ai::task_finder::world::TaskWorld;
use crate::unit_ai::task_finder::{Actor, find_task};
use crate::world::World;
use crate::world::slots::PlayerSlots;

/// The task whose target range is the target's owner's attribute 90 (`+0x2d0` of its attribute
/// table), `TRIBE_Action_Repair`'s neighbour 0x69 in the command table.
const OWNER_RANGE_TASK: ActionKind = ActionKind::named(0x69);
/// The task `slot38` leaves the range of to the unit's own slot 0x230 (the attack path never
/// reads it).
const OWN_RANGE_TASK: ActionKind = ActionKind::named(0x68);
/// The player attribute that bounds a [`OWNER_RANGE_TASK`]'s range.
const OWNER_RANGE_ATTRIBUTE: usize = 90;
/// The task `FUN_1415c60b0` looks for in a unit's master before it asks the player's transport
/// commands (`FUN_14160e090`).
const BOARD_TASK: ActionKind = ActionKind::named(0x84);
/// The same for `FUN_14160e140`.
const UNBOARD_TASK: ActionKind = ActionKind::named(0x88);
/// A master of this type or above (`0x27` below) may hold the board tasks.
const FIRST_ACTING_KIND: MasterKind = MasterKind::ACTION;

/// What a member's slot 0x130 gives a group's command: the task's action type (the command the
/// group runs) and how near the target the work is done.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TargetTask {
    /// The task's action type (`+0xe`), the command.
    pub(crate) command: ActionKind,
    /// The range the work is done at (`*param_6`): the task's (`+0x30`), the target owner's
    /// attribute 90 for [`OWNER_RANGE_TASK`]. None for [`OWN_RANGE_TASK`], where the routine
    /// asks the unit's slot 0x230 for it and `BUnitGroup::command` never reads it.
    pub(crate) range: Option<Distance>,
}

/// What slot 0x130 is asked: the target, the point (x, y; z is 0), the order's task filter
/// (`-1` for any) and its queued and direct bytes, which only the board tasks' commands read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TaskQuery {
    /// The object the command is on.
    pub(crate) target: ObjectId,
    /// The order's point.
    pub(crate) point: Point,
    /// The order's task filter (`+0x16`), `-1` for any.
    pub(crate) filter: Option<ActionKind>,
    /// The order is queued.
    pub(crate) queued: bool,
    /// A unit with a unit AI takes the order at once.
    pub(crate) direct: bool,
}

impl World {
    /// The task of group `group` on `target` as `BUnitGroup::command` asks for it: slot 0x130
    /// ([`World::member_task`]) of each member the world holds with at least
    /// [`MEMBER_MIN_HP`] hit points, in order, until one answers a task.
    ///
    /// # Errors
    ///
    /// What [`World::member_task`] does not model.
    pub(crate) fn group_task(
        &self,
        group: GroupId,
        target: ObjectId,
        point: Point,
        order: CommandArgs,
    ) -> Result<Option<TargetTask>, GenieError> {
        let query = TaskQuery {
            target,
            point,
            filter: order.filter,
            queued: order.queued,
            direct: order.direct,
        };
        for m in &self.groups.held(group)?.members {
            let Some((_, u)) = self.object(m.id) else {
                continue;
            };
            let Some(member) = Some(m.id) else {
                continue;
            };
            if u.life.hp < HitPoints::new(MEMBER_MIN_HP) {
                continue;
            }
            if let Some(task) = self.member_task(member, &query)? {
                return Ok(Some(task));
            }
        }
        Ok(None)
    }

    /// Slot 0x130 of the member `member` (`FUN_14106b8b0`) for `query`: none when the target is
    /// the unit itself. The unit's own master's task list (`find_task`, mode 0, the `query`'s
    /// filter) gives the task; without one, a master with a task swap group (`+0x7b8`) tries
    /// its player's masters in id order of its class, type and group, with the same filter, and
    /// the first with a task gives it. Without any, a master that holds a board task (`0x84`)
    /// or an unboard task (`0x88`) asks the player's commands for them, which is not modelled.
    ///
    /// # Errors
    ///
    /// A member or a target the world does not hold live, and a master with a board or an
    /// unboard task (the player's commands `FUN_14160e090` and `FUN_14160e140`).
    pub(crate) fn member_task(
        &self,
        member: ObjectId,
        query: &TaskQuery,
    ) -> Result<Option<TargetTask>, GenieError> {
        if member == query.target {
            return Ok(None);
        }
        let (player, unit) = self.object(member).ok_or_else(|| {
            GenieError::unreachable("a member the group's task search found is not live")
                .with("member", member.get())
        })?;
        let own = unit.master(self.master_tables(), player)?;
        let owner = &self.players.player(player)?;
        let actor = Actor {
            id: unit.id,
            master: own,
            player,
            held: unit.life.held(),
            attribute_95: owner.attributes.get(95).copied().unwrap_or(Amount::ZERO),
            civ: owner.civ,
        };
        let world = TaskWorld {
            players: &self.players,
            drop_sites: &self.model.drop_sites,
        };
        let target = query.target;
        let t = self.task_target(target, 1)?;
        let general = self.task_target(target, 2)?;
        let terrain = self.task_terrain(query.point.with_height(0.0));
        let find = |list: &Master| {
            find_task(
                list,
                &actor,
                &TaskOn {
                    target: Some(&t),
                    general: Some(&general),
                    terrain,
                },
                query.filter,
                &world,
                self.model.slots,
                &self.model.roles,
            )
            .map(Option::<&Task>::copied)
        };
        let mut found = find(own)?;
        if found.is_none() && own.task_by_group != TaskGroup::NONE {
            let masters = self.model.masters.of(player);
            for candidate in masters.into_iter().flat_map(Masters::iter) {
                if candidate.class != own.class
                    || candidate.kind != own.kind
                    || candidate.task_by_group != own.task_by_group
                {
                    continue;
                }
                found = find(candidate)?;
                if found.is_some() {
                    break;
                }
            }
        }
        match found {
            Some(task) => self.task_range(&task, &t).map(Some),
            None if holds_board_task(own) => Err(GenieError::unsupported(
                "a group's command on a target (its task, slot 0x130)",
            )),
            None => Ok(None),
        }
    }

    /// The answer of slot 0x130 for the `task` found on `target`: its command and range.
    pub(super) fn task_range(
        &self,
        task: &Task,
        target: &TaskTarget<'_>,
    ) -> Result<TargetTask, GenieError> {
        let range = match task.action_kind {
            OWN_RANGE_TASK => None,
            OWNER_RANGE_TASK => Some(Distance::new(
                self.players
                    .get(target.player.index())
                    .and_then(|p| p.attributes.get(OWNER_RANGE_ATTRIBUTE))
                    .copied()
                    .ok_or_else(|| {
                        GenieError::unreachable("a player without the attribute of a task's range")
                            .with("player", target.player)
                            .with("attribute", OWNER_RANGE_ATTRIBUTE)
                    })?
                    .get(),
            )),
            _ => Some(task.work_range),
        };
        Ok(TargetTask {
            command: task.action_kind,
            range,
        })
    }
}

/// `FUN_1415c60b0` for the board and the unboard task: a master of a type above 0x27 holds a
/// task of that action type.
fn holds_board_task(master: &Master) -> bool {
    FIRST_ACTING_KIND <= master.kind
        && master
            .tasks
            .iter()
            .any(|t| matches!(t.action_kind, BOARD_TASK | UNBOARD_TASK))
}
