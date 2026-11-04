//! A group's command (FD-036): `BUnitGroup::command` (`FUN_14100efd0`), which chooses by the
//! task a member has on the target how the group's order runs.

use crate::groups::GroupCommand;
use crate::groups::GroupCommandKind;
use crate::groups::GroupFlags;
use crate::groups::GroupId;
use crate::groups::GroupMove;
use crate::groups::MoveFlags;
use crate::groups::StopScope;
use crate::groups::attack::AttackOrder;
use crate::groups::command::{ATTACK_COMMANDS, MOVE_COMMAND, MoveTo};
use crate::groups::task::TargetTask;
use crate::groups::{GroupTarget, TargetData};
use crate::map::Distance;
use crate::map::Point;
use crate::math::fmath::BVector;
use crate::objects::ObjectId;
use crate::objects::masters::ActionKind;
use crate::world::Seconds;
use crate::world::World;
use crate::{GenieError, GenieResult};

/// The tasks `BUnitGroup::command` runs through the general command (`FUN_14100fc50`): the cases
/// of its switch besides the attack path's.
const GENERAL_COMMANDS: [GroupCommandKind; 11] =
    GroupCommandKind::table([3, 5, 0xc, 0x65, 0x69, 0x6a, 0x6e, 0x6f, 0x84, 0x88, 0x99]);
/// What `BUnitGroup::command` is told besides the group, the target and the point: the order's
/// queued (`param_5`) and direct (`param_6`) bytes, its seventh byte (`param_7`), which the
/// group keeps as its target's `mDisableRetargeting`, and its task filter (`param_8`, `-1` for
/// any).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandArgs {
    /// `param_5`: the command is queued.
    pub queued: bool,
    /// `param_6`: a unit with a unit AI takes it at once.
    pub direct: bool,
    /// `param_7`.
    pub disable_retargeting: bool,
    /// `param_8`.
    pub filter: Option<ActionKind>,
}

impl World {
    /// `BUnitGroup::command` (`FUN_14100efd0`) of group `group` on `target` (-1 for none, or an
    /// object the world does not hold: the game passes a null object) at `point`, with the order's
    /// bytes ([`CommandArgs`]). Its rams are not re-added (`readdRams`, `FUN_14102b730`:
    /// `DAT_143f01837`, which nothing writes, is 0), [`GroupFlags::MEMBERS_SENT`] clears, and the
    /// first member the world holds with at least a hit point names the task on the target (slot
    /// 0x130, [`World::member_task`]: none without a target). A group of horde units moves as a
    /// horde (`commandHorde`, `FUN_141010180`, not modelled); any other notes whether the command
    /// was queued (`+0x13d`, [`crate::groups::UnitGroup::replaces_queue`]) and runs the command by
    /// the task's action type: without a task, or for 1, a move ([`World::command_move_to`] with
    /// its target forgotten); for 1, 7, 9, `0x68` and `0x7d` the attack path
    /// ([`World::command_attack`], or a queue entry); the other tasks' general command
    /// ([`World::general_command`]) runs. A move begun stops the members (`stopAllUnits` with 0,
    /// [`World::stop_all_units`]), and [`GroupFlags::JOINED`] clears. Begun when the move began.
    ///
    /// # Errors
    ///
    /// A horde, and what the paths and the general command do not model.
    pub fn group_command(
        &mut self,
        group: GroupId,
        target: Option<ObjectId>,
        point: Point,
        order: CommandArgs,
    ) -> GenieResult<GroupMove> {
        let unit_group = self.groups.held_mut(group)?;
        unit_group.flags &= !GroupFlags::MEMBERS_SENT;
        let target = target.filter(|&object_id| self.world_object(object_id).is_some());
        let task = match target {
            Some(object_id) => self.group_task(group, object_id, point, order)?,
            None => None,
        };
        if self.all_horde_units(group)? {
            return Err(GenieError::unsupported(
                "a horde's command (commandHorde, FUN_141010180)",
            ));
        }
        let unit_group = self.groups.held_mut(group)?;
        unit_group.replaces_queue = u8::from(!order.queued);
        let command = task.map(|target_task| GroupCommandKind::of_action(target_task.command));
        let mut moved = GroupMove::NotBegun;
        if command.is_none_or(|group_command_kind| group_command_kind == MOVE_COMMAND) {
            let mut flags = MoveFlags::FORGET_TARGET;
            flags.set(MoveFlags::QUEUED, order.queued);
            flags.set(MoveFlags::DIRECT, order.direct);
            moved = self.command_move_to(
                group,
                target,
                point,
                MoveTo {
                    flags,
                    ..MoveTo::default()
                },
            )?;
        }
        if let (Some(task), Some(held)) = (task, target) {
            let command = GroupCommandKind::of_action(task.command);
            if ATTACK_COMMANDS.contains(&command) {
                moved = self.attack_command(group, held, point, (task, order), moved)?;
            } else if GENERAL_COMMANDS.contains(&command) {
                moved = self.general_command(
                    group,
                    (command, held),
                    point,
                    task.range.unwrap_or(Distance::ZERO),
                    order,
                )?;
            }
        }
        if moved == GroupMove::Begun {
            self.stop_all_units(group, StopScope::Busy)?;
        }
        let unit_group = self.groups.held_mut(group)?;
        unit_group.flags &= !GroupFlags::JOINED;
        Ok(moved)
    }

    /// The general command (`FUN_14100fc50`) of group `group`: `command` on `target` at `point`
    /// within `range`, for the tasks that are no attack. Not queued, not direct and with commands
    /// queued, it joins the queue unapproved, the approved ones go and the queue runs (begun);
    /// queued, it joins the queue (approved when direct), and a group without a command that is
    /// told directly hibernates (not begun). Else a group whose last command was not queued empties
    /// its queue, its target's data resets (the range too), each member's seconds since
    /// attacking are `f32::MAX`, and the group moves to the point ([`World::command_move_to`]
    /// with `range`, the target forgotten, direct). A move begun without a target sets the
    /// target data to the point (range, no radius); with one it raises the categories' attack
    /// ranges ([`World::raise_attack_ranges`]) and sets the target (kind 1, the place it last
    /// moved to, the range, the radii's length, no retargeting, certain action -1). Either
    /// way the group is in `command`. Begun when the move began.
    ///
    /// # Errors
    ///
    /// What the queue, the move and the target's ranges do not model.
    pub(super) fn general_command(
        &mut self,
        group: GroupId,
        (command, target): (GroupCommandKind, ObjectId),
        point: Point,
        range: Distance,
        order: CommandArgs,
    ) -> GenieResult<GroupMove> {
        let unit_group = self.groups.held_mut(group)?;
        let entry = |approved: u8| GroupCommand {
            command,
            target_id: Some(target),
            range,
            point: BVector::new(point.x(), 0.0, point.y()),
            approved,
            disable_retargeting: 0,
            approved_2: u16::MAX,
        };
        if order.queued {
            unit_group.queue_command(entry(u8::from(order.direct)));
            if unit_group.current_command == GroupCommandKind::NONE
                && order.direct
                && unit_group.hibernating != 1
            {
                unit_group.hibernating = 1;
            }
            return Ok(GroupMove::NotBegun);
        }
        if !order.direct && !unit_group.command_queue.is_empty() {
            unit_group.queue_command(entry(0));
            unit_group.approve_queued();
            self.check_queued_orders(group)?;
            return Ok(GroupMove::Begun);
        }
        if unit_group.replaces_queue != 0 {
            unit_group.clear_command_queue();
        }
        unit_group.forget_target_data();
        unit_group.current_target.range = Distance::ZERO;
        for member in &mut unit_group.members {
            member.seconds_since_attacking = Seconds::MAX;
        }
        let moved = self.command_move_to(
            group,
            Some(target),
            point,
            MoveTo {
                range,
                flags: MoveFlags::FORGET_TARGET | MoveFlags::DIRECT,
            },
        )?;
        if moved == GroupMove::NotBegun {
            return Ok(GroupMove::NotBegun);
        }
        self.raise_attack_ranges(group, target)?;
        let at = self.moved_place_of(target)?;
        let radius = self.target_radius(target)?;
        let unit_group = self.groups.held_mut(group)?;
        unit_group.current_target = TargetData {
            target: GroupTarget::Object(target),
            original_position: at,
            range,
            target_radius: radius,
            disable_retargeting: 0,
            certain_action: None,
        };
        unit_group.current_command = command;
        Ok(GroupMove::Begun)
    }

    /// `BUnitGroup::command`'s attack tasks (1, 7, 9, `0x68`, `0x7d`) on `target`, `moved` what
    /// task 1's move before it returned: a command not queued either joins the command queue
    /// (not direct, with commands queued: the queue is approved and run, begun) or runs
    /// [`World::command_attack`] after the queue empties when the last command was not queued; a
    /// queued one joins the queue (approved when direct), a group without a command that is told
    /// directly hibernating, and answers what the move before it did.
    fn attack_command(
        &mut self,
        group: GroupId,
        target: ObjectId,
        point: Point,
        (task, order): (TargetTask, CommandArgs),
        moved: GroupMove,
    ) -> GenieResult<GroupMove> {
        let unit_group = self.groups.held_mut(group)?;
        let entry = |approved: u8| GroupCommand {
            command: GroupCommandKind::of_action(task.command),
            target_id: Some(target),
            range: task.range.unwrap_or(Distance::ZERO),
            point: BVector::new(point.x(), 0.0, point.y()),
            approved,
            disable_retargeting: u8::from(order.disable_retargeting),
            approved_2: ActionKind::field(order.filter).cast_unsigned(),
        };
        if order.queued {
            unit_group.queue_command(entry(u8::from(order.direct)));
            if unit_group.current_command == GroupCommandKind::NONE
                && order.direct
                && unit_group.hibernating != 1
            {
                unit_group.hibernating = 1;
            }
            return Ok(moved);
        }
        if !order.direct && !unit_group.command_queue.is_empty() {
            unit_group.queue_command(entry(0));
            unit_group.approve_queued();
            self.check_queued_orders(group)?;
            return Ok(GroupMove::Begun);
        }
        if unit_group.replaces_queue != 0 {
            unit_group.clear_command_queue();
        }
        self.command_attack(
            group,
            target,
            AttackOrder {
                disable_retargeting: order.disable_retargeting,
                filter: order.filter,
            },
        )
    }
}
