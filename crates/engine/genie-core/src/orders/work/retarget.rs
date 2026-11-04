//! A worker's action already on the task the work found takes the new target itself
//! (`retake_task`): a Hunt or a Gather retargets, a Build or a Repair takes the target again.

use crate::construction::repair::REPAIR;
use crate::garrison::transport::TRANSPORT;
use crate::gathering::gather::GATHER;
use crate::gathering::hunt::HUNT;
use crate::map::Position;
use crate::math::rng::Stream;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::actions::Action;
use crate::objects::masters::ActionKind;
use crate::objects::masters::Task;
use crate::orders::work::{MasterSwap, TERRAIN_TASK};
use crate::players::PlayerId;
use crate::unit_ai::AiAction;
use crate::unit_ai::Answer;
use crate::unit_ai::Notifies;
use crate::unit_ai::OrderKind;
use crate::world::World;
use crate::world::layer::MasterRoles;
use crate::world::slots::PlayerSlotsExt;
use crate::{GenieError, GenieResult};

impl World {
    /// `LAB_1410706a8`: an action of the unit already on the `task` found takes the new target
    /// itself (its slot 9): a Hunt or Gather on the same task retargets, a Build or Repair on
    /// top takes the target again, a Transport on the same task works at the point
    /// ([`World::retake_transport`]); the base action types on top answer 0 (the factory
    /// follows).
    /// Taken when an action kept the work.
    pub(in crate::orders) fn retake_task(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        (target, point, notifies): (Option<ObjectId>, Position, Notifies),
        task: &Task,
        master: MasterSwap,
    ) -> GenieResult<Answer> {
        let unit = &self.players.live(player, idx)?;
        let head = self.parts.actions_of(unit.id).list.head().cloned();
        let top = head.as_ref().map(Action::kind);
        Ok(match head {
            Some(Action::Hunt(hunt_action))
                if task.action_kind == HUNT && hunt_action.task.id == task.id =>
            {
                self.retarget_hunt_work(
                    player,
                    idx,
                    (target.ok_or(TERRAIN_TASK)?, point, notifies),
                )?;
                Answer::Taken
            }
            Some(Action::Build(build)) if task.action_kind == ActionKind::BUILD => {
                if master == MasterSwap::Change {
                    return Err(GenieError::unsupported(
                        "a work command on the Build task of the unit's former master",
                    ));
                }
                let target = target.ok_or(TERRAIN_TASK)?;
                self.retake_build((player, idx), build, (target, point, notifies))?;
                Answer::Taken
            }
            Some(Action::Repair(repair)) if task.action_kind == REPAIR => {
                let target = target.ok_or(TERRAIN_TASK)?;
                self.retake_repair((player, idx), repair, (target, point, notifies))?;
                Answer::Taken
            }
            Some(Action::Gather(gather_action))
                if task.action_kind == GATHER && gather_action.task.id == task.id =>
            {
                self.retarget_gather_work(
                    player,
                    idx,
                    (target.ok_or(TERRAIN_TASK)?, point, notifies),
                )?
            }
            Some(Action::Transport(_)) if task.action_kind == TRANSPORT => {
                self.retake_transport((player, idx), (target, point), task, notifies)?
            }
            // the top action's slot 0x48 (FUN_140a977a0) on the same task: the base's
            // (FUN_140a7bf40: the MoveTo's, the Enter's, the missile's, the Fly's, the Make's, the
            // wait for a foundation's) answers 0, and the work goes on to the factory
            Some(other) if top == Some(task.action_kind) && top != Some(HUNT) => match other {
                Action::MoveTo(_)
                | Action::Enter(_)
                | Action::Missile(_)
                | Action::Fly(_)
                | Action::Make(_)
                | Action::WaitSolidify(_) => Answer::NotTaken,
                _ => {
                    return Err(GenieError::unsupported(
                        "a work command on the unit's current task",
                    )
                    .with("action", format_args!("{top:?}"))
                    .with("task", task.id.get())
                    .with("target", ObjectId::field(target)));
                }
            },
            _ => Answer::NotTaken,
        })
    }

    /// A Hunt on the work's task retargets (slot 9, [`World::retarget_hunt`]) and its unit AI
    /// hears the hunt order and action on the target at the work's point; a hunt sent to its
    /// player's master 84 is not modelled.
    pub(in crate::orders) fn retarget_hunt_work(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        (target, point, notifies): (ObjectId, Position, Notifies),
    ) -> GenieResult<()> {
        let task_target = self.task_target(target, 0)?;
        if self
            .model
            .roles
            .of_master(task_target.master.id)
            .contains(MasterRoles::MARKET)
            && task_target.player == player
        {
            return Err(GenieError::unsupported(
                "a hunt sent to its player's master 84",
            ));
        }
        let class = task_target.master.class;
        self.retarget_hunt(player, idx, target)?;
        let unit = self.players.live_mut(player, idx)?;
        if let Some(ai) = self.parts.unit_ai.get_mut(unit.id) {
            ai.notify_work(
                Some(OrderKind::Hunt),
                Some(AiAction::Hunt),
                Some((target, class)),
                point,
                notifies,
                self.streams.get_mut(Stream::Ai),
            )?;
            self.remember_order(player, idx, (Some(OrderKind::Hunt), Some(AiAction::Hunt)))?;
        }
        Ok(())
    }

    /// A Gather on the work's task retargets (slot 9, [`World::retarget_gather`]); when it took
    /// the target its unit AI hears the gather order and action on it at the work's point.
    /// Answers whether it took it; a gather sent to its player's master 84 is not modelled.
    pub(in crate::orders) fn retarget_gather_work(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        (target, point, notifies): (ObjectId, Position, Notifies),
    ) -> GenieResult<Answer> {
        let task_target = self.task_target(target, 0)?;
        if self
            .model
            .roles
            .of_master(task_target.master.id)
            .contains(MasterRoles::MARKET)
            && task_target.player == player
        {
            return Err(GenieError::unsupported(
                "a gather sent to its player's master 84",
            ));
        }
        let class = task_target.master.class;
        let took = self.retarget_gather(player, idx, target)?;
        if took == Answer::Taken {
            let unit = self.players.live_mut(player, idx)?;
            if let Some(ai) = self.parts.unit_ai.get_mut(unit.id) {
                ai.notify_work(
                    Some(OrderKind::Gather),
                    Some(AiAction::Gather),
                    Some((target, class)),
                    point,
                    notifies,
                    self.streams.get_mut(Stream::Ai),
                )?;
                self.remember_order(
                    player,
                    idx,
                    (Some(OrderKind::Gather), Some(AiAction::Gather)),
                )?;
            }
        }
        Ok(took)
    }
}
