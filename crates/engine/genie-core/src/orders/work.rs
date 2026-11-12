//! A kept attack, the garrison task and the work command and its task.

use crate::combat::AttackTargetId;
use crate::combat::attack::{ATTACK, Attack};
use crate::construction::build::CART_TASK;
use crate::construction::repair::REPAIR;
use crate::garrison::ENTER;
use crate::garrison::transport::TRANSPORT;
use crate::gathering::both_food;
use crate::gathering::gather::GATHER;
use crate::gathering::hunt::{HUNT, HuntTask};
use crate::map::Position;
use crate::map::TerrainId;
use crate::math::fmath::at_most_or_unordered;
use crate::math::rng::Stream;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::ActionKind;
use crate::objects::masters::MasterId;
use crate::objects::masters::Task;
use crate::objects::masters::player_masters;
use crate::objects::masters::work_master;
use crate::orders::ActionsOnChange;
use crate::orders::gather::HuntOrder;
use crate::orders::{ATTACK_TASK, Work};
use crate::ownership::artifact::ARTIFACT_KIND;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::unit_ai::AiAction;
use crate::unit_ai::Answer;
use crate::unit_ai::Notifies;
use crate::unit_ai::OrderKind;
use crate::unit_ai::task_finder::TaskOn;
use crate::unit_ai::task_finder::world::TaskWorld;
use crate::unit_ai::task_finder::world::point_terrain;
use crate::unit_ai::task_finder::world::task_search;
use crate::unit_ai::task_finder::{Actor, enabled};
use crate::world::World;
use crate::world::layer::ClassRoles;
use crate::world::slots::PlayerSlotsExt;
use crate::{GenieError, GenieResult};

mod garrison;
mod retarget;
#[cfg(test)]
mod tests;

/// A task of an object's action (Enter, Attack, Hunt, Build, Repair, Gather) that the task
/// finder gave a work command without a target, for the terrain at its point: the game starts
/// the action on −1, which the slice does not model.
const TERRAIN_TASK: GenieError =
    GenieError::unsupported("a work command's task of an object's action for a terrain");

/// The action list's factory for a task the slice does not model.
const UNMODELLED_FACTORY: GenieError =
    GenieError::unsupported("the action factory for a task the slice does not model");

/// Whether the unit's top action (`top`) is the Attack `attack` on `target` (both none counts, as
/// the game compares −1 with −1).
fn attack_on(top: Option<ActionKind>, attack: &Attack, target: Option<ObjectId>) -> bool {
    top == Some(ATTACK) && attack.target.map(AttackTargetId::id) == target
}

/// Whether a unit changes its master to the one whose task it found (1) or keeps its own (0):
/// the task search's ([`World::find_work_task`]), and the work command's before it retakes the
/// task ([`World::retake_task`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum MasterSwap {
    /// The unit takes the master.
    Change,
    /// The unit keeps its own.
    Keep,
}

impl World {
    /// The work command's check of the unit's top Attack (`FUN_14106ff70`): one on this very
    /// target, the filter none or 9, keeps it when its slot 9 (`FUN_140a841d0`, [`Attack::keeps`])
    /// does, and the unit AI hears of it as of a new one (slot 0x828: the attack order and
    /// action, the target, the retarget time). Taken when kept.
    ///
    /// [`Attack::keeps`]: crate::combat::attack::Attack::keeps
    pub(super) fn keep_attack(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: Option<ObjectId>,
        filter: Option<ActionKind>,
        point: Position,
        notifies: Notifies,
    ) -> GenieResult<Answer> {
        let unit = &self.players.live(player, idx)?;
        let top = self.parts.actions_of(unit.id).top_action_kind();
        let attacking = self
            .parts
            .actions_of(unit.id)
            .list
            .attack()
            .filter(|attack| attack_on(top, attack, target));
        let Some(attack) = attacking.filter(|_| matches!(filter, None | Some(ATTACK))) else {
            return Ok(Answer::NotTaken);
        };
        if !attack.keeps(target)? {
            return Ok(Answer::NotTaken);
        }
        self.tell_of_attack(player, idx, (target, point), notifies)?;
        Ok(Answer::Taken)
    }

    /// The unit AI hears of the kept Attack as of a new one ([`World::keep_attack`]'s last part).
    ///
    /// # Errors
    ///
    /// A target or master the slice does not hold, or the notify's.
    fn tell_of_attack(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        (target, point): (Option<ObjectId>, Position),
        notifies: Notifies,
    ) -> GenieResult<()> {
        let (tp, target_unit) = self.object(target).ok_or(GenieError::unsupported(
            "an attack's target the slice does not hold",
        ))?;
        let target = target_unit.id;
        let class = target_unit.master(self.master_tables(), tp)?.class;
        let world_time = self.world_time;
        let unit = self.players.live_mut(player, idx)?;
        if let Some(ai) = self.parts.unit_ai.get_mut(unit.id) {
            ai.last_retarget_time = world_time;
            ai.notify_work(
                Some(OrderKind::Attack),
                Some(AiAction::Attack),
                Some((target, class)),
                point,
                notifies,
                self.streams.get_mut(Stream::Ai),
            )?;
            self.remember_order(
                player,
                idx,
                (Some(OrderKind::Attack), Some(AiAction::Attack)),
            )?;
        }
        Ok(())
    }

    /// The work command's retargeting byte set ([`Work::disable_retargeting`], `FUN_14106ff70`'s
    /// `param_8`) on the task found: a gather or hunt task (action type 5 or 0x6e) of another
    /// resource (`+0x20`) than the unit holds (`+0x308`; the food kinds 0 and 15 to 17 count as
    /// one) leaves the unit holding none of the task's (`FUN_14119d310` with 0); an attack task (7
    /// or 9) of a unit that is no villager turns its unit AI's retargeting off (`+0x1d6`,
    /// `FUN_140c40ef0` with the byte, 1).
    pub(super) fn carried_resource_test(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        task: &Task,
    ) -> GenieResult<()> {
        let class = self
            .players
            .live(player, idx)?
            .master(self.master_tables(), player)?
            .class;
        let unit = self.players.live_mut(player, idx)?;
        if matches!(task.action_kind, GATHER | HUNT) {
            let (kind, held) = (task.attribute_kind, unit.life.attribute_kind);
            if held != kind && !both_food(kind, held) {
                unit.life.attribute_kind = kind;
                unit.life.amount = Amount::ZERO;
            }
        }
        if matches!(task.action_kind, ATTACK_TASK | ATTACK)
            && !self
                .model
                .roles
                .of_class(class)
                .contains(ClassRoles::VILLAGER)
            && let Some(ai) = self.parts.unit_ai.get_mut(unit.id)
        {
            ai.retarget_disabled = 1;
        }
        Ok(())
    }

    /// The work command (slot 37, `FUN_14106ff70`) of a unit on a target object: the master and
    /// task of [`crate::objects::masters::work_master`] (the master changes, slot 0x90), the task's
    /// requirement, then the task's action from the action list's factory (slot 0x10), the unit
    /// AI's notify (slot 0x828) and the install (slot 0x808).
    pub(super) fn work_command(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        order: &Work,
    ) -> GenieResult<()> {
        let Work {
            target,
            point,
            disable_retargeting,
            filter,
        } = *order;
        let unit = &self.players.live(player, idx)?;
        if target == Some(unit.id) {
            return Ok(());
        }
        // FUN_140c64e50 on the new action's notify: the unit's `+0xa14` and type
        let notifies = self
            .parts
            .combat_of(unit.id)
            .order_source
            .notifies(unit.master(self.master_tables(), player)?.kind);
        let found = self.work_task(player, idx, (target, point), filter)?;
        let Some((with, task)) = found else {
            if filter.is_some() {
                return Ok(());
            }
            // LAB_1410706a8 without a task: the top Attack on the target keeps it; else a unit
            // whose master moves (slot 0xe8, its speed `+0x740`) is told of a move (slot 0x828)
            // and moves there (slot 0x138), which is not modelled; one that does not, nothing
            if self.keep_attack(player, idx, target, filter, point, notifies)? == Answer::Taken {
                return Ok(());
            }
            return self.work_move(player, idx, target, point, notifies);
        };
        // a new master's task is another than the one the unit's action holds
        let master = if with == self.players.live(player, idx)?.life.master_id {
            MasterSwap::Keep
        } else {
            MasterSwap::Change
        };
        if master == MasterSwap::Change {
            self.change_master(player, idx, with, ActionsOnChange::Clear)?;
        }
        if !enabled(&self.players.player(player)?.attributes, task.enablement) {
            return Ok(());
        }
        // LAB_1410706a8: an action on the task found takes the new target itself (its slot 9)
        let top = self
            .parts
            .actions_of(self.players.live(player, idx)?.id)
            .top_action_kind();
        let kept = self.retake_task(player, idx, (target, point, notifies), &task, master)?;
        if disable_retargeting {
            self.carried_resource_test(player, idx, &task)?;
        }
        if self.keep_attack(player, idx, target, filter, point, notifies)? == Answer::Taken {
            return Ok(());
        }
        // the top action of type 0x7e would hear the target through its slot 0x48: the slice
        // holds no action of that type. An Artifact on top ends a command of an Enter task here.
        if top == Some(ARTIFACT_KIND) && task.action_kind == ENTER {
            return Ok(());
        }
        if kept == Answer::Taken {
            return Ok(());
        }
        self.start_task(player, idx, (target, point), &task, notifies)
    }

    /// The work command's master and task for the unit at (`player`, `idx`) on `target` (none
    /// for a point) at `point` ([`crate::objects::masters::work_master`] under the task
    /// `filter`, the terrain under the point for a task for a terrain): the master with the task
    /// found.
    pub(super) fn work_task(
        &self,
        player: PlayerId,
        idx: ListIndex,
        (target, point): (Option<ObjectId>, Position),
        filter: Option<ActionKind>,
    ) -> GenieResult<Option<(MasterId, Task)>> {
        let unit = &self.players.live(player, idx)?;
        let masters = self
            .model
            .masters
            .get(player.index())
            .ok_or(GenieError::unsupported("a work command: no master table"))?;
        let owner = &self.players.player(player)?;
        let actor = Actor {
            id: unit.id,
            master: unit.master(self.master_tables(), player)?,
            player,
            held: unit.life.held(),
            attribute_95: owner.attributes.get(95).copied().unwrap_or(Amount::ZERO),
            civ: owner.civ,
        };
        let world = TaskWorld {
            players: &self.players,
            treaty: self.treaty.as_ref(),
            drop_sites: &self.model.drop_sites,
        };
        let (task_target, general) = match target {
            None => (None, None),
            Some(target) => (
                Some(self.task_target(target, 1)?),
                Some(self.task_target(target, 2)?),
            ),
        };
        let on = TaskOn {
            target: task_target.as_ref(),
            general: general.as_ref(),
            terrain: self.task_terrain(point),
        };
        Ok(work_master(
            masters,
            &actor,
            &on,
            filter,
            &world,
            self.model.slots,
            &self.model.roles,
        )?
        .map(|(with, task)| (with.id, *task)))
    }

    /// The terrain the task finder's general search reads at `point` (`FUN_1411a8ba0`,
    /// [`crate::unit_ai::task_finder::world::point_terrain`] on the map's terrain).
    pub(crate) fn task_terrain(&self, point: Position) -> Option<TerrainId> {
        point_terrain(self.spatial.terrain.as_ref(), point)
    }

    /// The action list's factory (slot 0x10) for the work's `task` on `target`: the Enter,
    /// Attack, Hunt, Build, Repair, Gather or Transport starts; a mule cart's task makes no
    /// action (the move without one, [`World::work_move`]).
    pub(super) fn start_task(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        (target, point): (Option<ObjectId>, Position),
        task: &Task,
        notifies: Notifies,
    ) -> GenieResult<()> {
        match task.action_kind {
            // the factory makes no action of a mule cart's task: the move without one
            CART_TASK => self.work_move(player, idx, target, point, notifies),
            TRANSPORT => self.start_transport((player, idx), (target, point), task, notifies),
            ENTER | ATTACK_TASK | HUNT | ActionKind::BUILD | REPAIR | GATHER => {
                let target = target.ok_or(TERRAIN_TASK)?;
                self.start_object_task(player, idx, (target, point), task, notifies)
            }
            _ => Err(UNMODELLED_FACTORY),
        }
    }

    /// [`World::start_task`]'s actions on an object: the Enter, Attack, Hunt, Build, Repair or
    /// Gather on `target` starts.
    fn start_object_task(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        (target, point): (ObjectId, Position),
        task: &Task,
        notifies: Notifies,
    ) -> GenieResult<()> {
        match task.action_kind {
            ENTER => self.start_enter(player, idx, target, task, point, notifies),
            ATTACK_TASK => self.start_attack(player, idx, target, task, point, notifies),
            HUNT => self.start_hunt(HuntOrder {
                player,
                index: idx,
                target,
                task: HuntTask::of(task),
                point,
                notifies,
                notify: true,
            }),
            ActionKind::BUILD => self.start_build(player, idx, target, task, point, notifies),
            REPAIR => self.start_repair(player, idx, target, task, point, notifies),
            GATHER => self.start_gather(player, idx, target, task, Some((point, notifies))),
            _ => Err(UNMODELLED_FACTORY),
        }
    }

    /// The work command without an action (`FUN_14106ff70`: no task, or one the action list's
    /// factory makes no action of, the mule cart's 0x99): a unit whose master moves (slot 0xe8,
    /// its speed `+0x740`) is told of a move (slot 0x828, its notifies cleared with
    /// [`Notifies::Cleared`]) and, in state 2 or of class 51, moves to the target or the point
    /// (slot 0x138, [`World::move_point_order`], [`World::move_to_object`]); one that does not
    /// move does nothing.
    pub(super) fn work_move(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: Option<ObjectId>,
        point: Position,
        notifies: Notifies,
    ) -> GenieResult<()> {
        let unit = &self.players.live(player, idx)?;
        let master = unit.master(self.master_tables(), player)?;
        // slot 0xe8: the master's speed (`+0x740`), before a sprite's factor; not above 0
        // (`COMISS`: a NaN too) ends here
        if at_most_or_unordered(master.animated_part().max_speed.get(), 0.0) {
            return Ok(());
        }
        // slot 0x828 with 1 (the move's order and action) for a unit in state 2, -1 for
        // another; then the move to the target (slot 0x138) for a unit in state 2 or of
        // class 51
        let (alive, class) = (unit.life.state == ObjectState::Alive, master.class);
        let named = match target {
            None => None,
            Some(target) => {
                let (tp, target_unit) = self.object(target).ok_or(GenieError::unsupported(
                    "a work command's target the slice does not hold",
                ))?;
                Some((target, target_unit.master(self.master_tables(), tp)?.class))
            }
        };
        let (order, action) = if alive {
            (Some(OrderKind::Move), Some(AiAction::Move))
        } else {
            (None, None)
        };
        let unit = self.players.live_mut(player, idx)?;
        if let Some(ai) = self.parts.unit_ai.get_mut(unit.id) {
            ai.notify_work(
                order,
                action,
                named,
                point,
                notifies,
                self.streams.get_mut(Stream::Ai),
            )?;
            self.remember_order(player, idx, (order, action))?;
        }
        if alive
            || self
                .model
                .roles
                .of_class(class)
                .contains(ClassRoles::PACKED_TREBUCHET)
        {
            match target {
                None => {
                    let _ = self.move_point_order(player, idx, point)?;
                }
                Some(target) => {
                    let _ = self.move_to_object(player, idx, target, point)?;
                }
            }
        }
        Ok(())
    }

    /// The unit's slot 0x820 (`RGE_Action_Object::getTask`, `FUN_14106c6a0`) on `target` at
    /// `point`, the x, y and z its caller passes (the target's place, or −1 each for the Build's
    /// drop-off): [`task_search`] with `filter` on the target, the target its general search reads
    /// and the terrain under the point ([`World::task_terrain`]). Its own master's list answers
    /// with the filter (the list's slot 0x10 in mode 0); without a task, a master with a task swap
    /// group (`+0x7b8` not 0) asks its player's masters by id of its class, type and group, its
    /// own too, with any filter (`0xffff`). With [`MasterSwap::Change`], the unit takes the master
    /// whose list answered there, though it be its own (slot 0x90 with 0: [`World::transform`],
    /// [`ActionsOnChange::Clear`]). None: no task.
    ///
    /// # Errors
    ///
    /// A unit or a target the world does not hold, a player without a masters table, and the
    /// task finder's and the master change's.
    pub(crate) fn find_work_task(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
        point: Position,
        filter: Option<ActionKind>,
        swap: MasterSwap,
    ) -> GenieResult<Option<Task>> {
        let unit = &self.players.live(player, idx)?;
        let owner = &self.players.player(player)?;
        let actor = Actor {
            id: unit.id,
            master: unit.master(self.master_tables(), player)?,
            player,
            held: unit.life.held(),
            attribute_95: owner.attributes.get(95).copied().unwrap_or(Amount::ZERO),
            civ: owner.civ,
        };
        let world = TaskWorld {
            players: &self.players,
            treaty: self.treaty.as_ref(),
            drop_sites: &self.model.drop_sites,
        };
        let task_target = self.task_target(target, 1)?;
        let general = self.task_target(target, 2)?;
        let on = TaskOn {
            target: Some(&task_target),
            general: Some(&general),
            terrain: self.task_terrain(point),
        };
        let found = task_search(
            player_masters(&self.model.masters, player)?,
            &actor,
            &on,
            filter,
            &world,
            self.model.slots,
            &self.model.roles,
        )?;
        let Some((task, swapped)) = found else {
            return Ok(None);
        };
        if swap == MasterSwap::Change
            && let Some(new) = swapped.map(|master| master.id)
        {
            self.transform(player, idx, new, ActionsOnChange::Clear)?;
        }
        Ok(Some(task))
    }
}
