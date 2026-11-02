//! A kept attack, the garrison task and the work command and its task.

use crate::GenieError;
use crate::combat::AttackTargetId;
use crate::combat::attack::{ATTACK, Attack};
use crate::combat::missile::MISSILE;
use crate::construction::build::CART_TASK;
use crate::construction::repair::REPAIR;
use crate::construction::wait_solidify::WAIT_SOLIDIFY;
use crate::garrison::ENTER;
use crate::garrison::transport::TRANSPORT;
use crate::gathering::both_food;
use crate::gathering::gather::GATHER;
use crate::gathering::hunt::{HUNT, HuntTask};
use crate::gathering::make::MAKE;
use crate::map::Position;
use crate::map::TerrainId;
use crate::math::rng::Stream;
use crate::movement::Speed;
use crate::movement::fly::FLY;
use crate::movement::move_to::MOVE_TO;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::ActionKind;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::Task;
use crate::objects::masters::TaskGroup;
use crate::objects::masters::class;
use crate::objects::masters::ids::MARKET;
use crate::objects::masters::work_master;
use crate::orders::gather::HuntOrder;
use crate::orders::{ATTACK_TASK, Work};
use crate::ownership::artifact::ARTIFACT_TYPE;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::unit_ai::AiAction;
use crate::unit_ai::OrderKind;
use crate::unit_ai::task_finder::world::TaskWorld;
use crate::unit_ai::task_finder::world::point_terrain;
use crate::unit_ai::task_finder::{Actor, enabled};
use crate::unit_ai::task_finder::{TaskOn, find_task};
use crate::world::World;
use crate::world::slots::CheckedIndex;
use crate::world::slots::PlayerSlots;

/// A task of an object's action (Enter, Attack, Hunt, Build, Repair, Gather) that the task
/// finder gave a work command without a target, for the terrain at its point: the game starts
/// the action on −1, which the slice does not model.
const TERRAIN_TASK: GenieError =
    GenieError::unsupported("a work command's task of an object's action for a terrain");

/// The action list's factory for a task the slice does not model.
const UNMODELLED_FACTORY: GenieError =
    GenieError::unsupported("the action factory for a task the slice does not model");

/// Whether the unit's top action (`top`) is the Attack `a` on `target` (both none counts, as
/// the game compares −1 with −1).
fn attack_on(top: Option<ActionKind>, a: &Attack, target: Option<ObjectId>) -> bool {
    top == Some(ATTACK) && a.target.map(AttackTargetId::id) == target
}

impl World {
    /// The work command's check of the unit's top Attack (`FUN_14106ff70`): one on this very
    /// target, the filter none or 9, keeps it when its slot 9 (`FUN_140a841d0`, [`Attack::keeps`])
    /// does, and the unit AI hears of it as of a new one (slot 0x828: the attack order and
    /// action, the target, the retarget time). True when kept.
    ///
    /// [`Attack::keeps`]: crate::combat::attack::Attack::keeps
    pub(super) fn keep_attack(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: Option<ObjectId>,
        filter: Option<ActionKind>,
        point: Position,
        clears: bool,
    ) -> Result<bool, GenieError> {
        let unit = &self.players.live(player, idx)?;
        let top = self.parts.actions_of(unit.id).top_action_kind();
        let attacking = self
            .parts
            .actions_of(unit.id)
            .list
            .attack()
            .filter(|a| attack_on(top, a, target));
        let Some(a) = attacking.filter(|_| matches!(filter, None | Some(ATTACK))) else {
            return Ok(false);
        };
        if !a.keeps(target)? {
            return Ok(false);
        }
        self.tell_of_attack(player, idx, (target, point), clears)?;
        Ok(true)
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
        clears: bool,
    ) -> Result<(), GenieError> {
        let (tp, t) = self.object(target).ok_or(GenieError::unsupported(
            "an attack's target the slice does not hold",
        ))?;
        let target = t.id;
        let class = t.master(self.master_tables(), tp)?.class;
        let world_time = self.world_time;
        let unit = self.players.live_mut(player, idx)?;
        if let Some(ai) = self.parts.unit_ai.get_mut(unit.id) {
            ai.last_retarget_time = world_time;
            ai.notify_work(
                Some(OrderKind::Attack),
                Some(AiAction::Attack),
                Some((target, class)),
                point,
                clears,
                self.streams.get(Stream::Ai),
            )?;
            self.remember_order(
                player,
                idx,
                (Some(OrderKind::Attack), Some(AiAction::Attack)),
            )?;
        }
        Ok(())
    }

    /// The unit at `idx` of player `player` as the task finder's actor.
    ///
    /// # Errors
    ///
    /// A unit or a master the world does not hold.
    fn garrison_actor(&self, player: PlayerId, idx: ListIndex) -> Result<Actor<'_>, GenieError> {
        let owner = self.players.player(player)?;
        let unit = owner.live(idx)?;
        Ok(Actor {
            id: unit.id,
            master: unit.master(self.master_tables(), player)?,
            player,
            held: unit.life.held(),
            attribute_95: owner.attributes.get(95).copied().unwrap_or(Amount::ZERO),
            civ: owner.civ,
        })
    }

    /// The unit's own task list's finder with filter 3 (slot 2 of its master's `+0x788`,
    /// `FUN_1412784d0`: the garrison's, [`crate::unit_ai::task_finder::find_task`]) on `target` at
    /// the gather `point`: whether it finds a task, as a gather point on a building asks
    /// (`FUN_140ace9c0`).
    pub(crate) fn garrison_task_found(
        &self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
        point: Position,
    ) -> Result<bool, GenieError> {
        let actor = self.garrison_actor(player, idx)?;
        let master = actor.master;
        let world = TaskWorld {
            players: &self.players,
            drop_sites: &self.model.drop_sites,
        };
        let t = self.task_target(target, 1)?;
        let general = self.task_target(target, 2)?;
        Ok(find_task(
            master,
            &actor,
            Some(&t),
            Some(&general),
            self.task_terrain(point),
            Some(ENTER),
            &world,
        )?
        .is_some())
    }

    /// The work command's carried-resource test (`+0x12`, `FUN_14106ff70` with `param_8`) on the
    /// task found: a gather or hunt task (action type 5 or 0x6e) of another resource (`+0x20`)
    /// than the unit holds (`+0x308`; the food kinds 0 and 15 to 17 count as one) leaves the
    /// unit holding none of the task's (`FUN_14119d310` with 0); an attack task (7 or 9) of a unit
    /// that is no villager turns its unit AI's retargeting off (`+0x1d6`, `FUN_140c40ef0` with
    /// the test's 1).
    pub(super) fn carried_resource_test(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        task: &Task,
    ) -> Result<(), GenieError> {
        let class = self
            .players
            .live(player, idx)?
            .master(self.master_tables(), player)?
            .class;
        let unit = self.players.live_mut(player, idx)?;
        if matches!(task.action_kind, GATHER | HUNT) {
            let (kind, held) = (task.attribute_type, unit.life.attribute_type);
            if held != kind && !both_food(kind, held) {
                unit.life.attribute_type = kind;
                unit.life.amount = Amount::ZERO;
            }
        }
        if matches!(task.action_kind, ATTACK_TASK | ATTACK)
            && class != class::VILLAGER
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
    ) -> Result<(), GenieError> {
        let Work {
            target,
            point,
            carry_test,
            filter,
            notified,
        } = *order;
        let unit = &self.players.live(player, idx)?;
        if target == Some(unit.id) {
            return Ok(());
        }
        // FUN_140c64e50: a unit of a type below 70 always clears its AI's notifies
        let clears =
            notified || unit.master(self.master_tables(), player)?.kind < MasterKind::COMBAT;
        let found = self.work_task(player, idx, (target, point), filter)?;
        let Some((with, task)) = found else {
            if filter.is_some() {
                return Ok(());
            }
            // LAB_1410706a8 without a task: the top Attack on the target keeps it; else a unit
            // whose master moves (slot 0xe8, its speed `+0x740`) is told of a move (slot 0x828)
            // and moves there (slot 0x138), which is not modelled; one that does not, nothing
            if self.keep_attack(player, idx, target, filter, point, clears)? {
                return Ok(());
            }
            return self.work_move(player, idx, target, point, (clears, notified));
        };
        // a new master's task is another than the one the unit's action holds
        let same_master = with == self.players.live(player, idx)?.life.master_id;
        if !same_master {
            self.change_master(player, idx, with)?;
        }
        if !enabled(&self.players.player(player)?.attributes, task.enablement) {
            return Ok(());
        }
        // LAB_1410706a8: an action on the task found takes the new target itself (its slot 9)
        let top = self
            .parts
            .actions_of(self.players.live(player, idx)?.id)
            .top_action_kind();
        let kept = self.retake_task(player, idx, (target, point, clears), &task, same_master)?;
        if carry_test {
            self.carried_resource_test(player, idx, &task)?;
        }
        if self.keep_attack(player, idx, target, filter, point, clears)? {
            return Ok(());
        }
        // the top action of type 0x7e would hear the target through its slot 0x48: the slice
        // holds no action of that type. An Artifact on top ends a command of an Enter task here.
        if top == Some(ARTIFACT_TYPE) && task.action_kind == ENTER {
            return Ok(());
        }
        if kept {
            return Ok(());
        }
        self.start_task(player, idx, (target, point), &task, (clears, notified))
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
    ) -> Result<Option<(MasterId, Task)>, GenieError> {
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
            drop_sites: &self.model.drop_sites,
        };
        let (t, general) = match target {
            None => (None, None),
            Some(target) => (
                Some(self.task_target(target, 1)?),
                Some(self.task_target(target, 2)?),
            ),
        };
        let on = TaskOn {
            target: t.as_ref(),
            general: general.as_ref(),
            terrain: self.task_terrain(point),
        };
        Ok(work_master(masters, &actor, &on, filter, &world)?.map(|(with, task)| (with, *task)))
    }

    /// The terrain the task finder's general search reads at `point` (`FUN_1411a8ba0`,
    /// [`crate::unit_ai::task_finder::world::point_terrain`] on the map's terrain).
    pub(crate) fn task_terrain(&self, point: Position) -> Option<TerrainId> {
        point_terrain(self.spatial.terrain.as_ref(), point)
    }

    /// `LAB_1410706a8`: an action of the unit already on the `task` found takes the new target
    /// itself (its slot 9): a Hunt or Gather on the same task retargets, a Build or Repair on
    /// top takes the target again, a Transport on the same task works at the point
    /// ([`World::retake_transport`]); the base action types on top answer 0 (the factory
    /// follows).
    /// Answers whether an action kept the work.
    pub(super) fn retake_task(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        (target, point, clears): (Option<ObjectId>, Position, bool),
        task: &Task,
        same_master: bool,
    ) -> Result<bool, GenieError> {
        let unit = &self.players.live(player, idx)?;
        let top = self.parts.actions_of(unit.id).top_action_kind();
        Ok(match self.parts.actions_of(unit.id).list.hunt().copied() {
            Some(h) if task.action_kind == HUNT && h.task.id == task.id => {
                self.retarget_hunt_work(player, idx, (target.ok_or(TERRAIN_TASK)?, point, clears))?;
                true
            }
            _ if top == Some(task.action_kind) && task.action_kind == ActionKind::BUILD => {
                if !same_master {
                    return Err(GenieError::unsupported(
                        "a work command on the Build task of the unit's former master",
                    ));
                }
                self.retake_build(player, idx, target.ok_or(TERRAIN_TASK)?, point, clears)?;
                true
            }
            _ if top == Some(task.action_kind) && task.action_kind == REPAIR => {
                self.retake_repair(player, idx, target.ok_or(TERRAIN_TASK)?, point, clears)?;
                true
            }
            _ if top == Some(GATHER)
                && task.action_kind == GATHER
                && self
                    .parts
                    .actions_of(unit.id)
                    .list
                    .gather()
                    .is_some_and(|g| g.task.id == task.id) =>
            {
                self.retarget_gather_work(
                    player,
                    idx,
                    (target.ok_or(TERRAIN_TASK)?, point, clears),
                )?
            }
            _ if top == Some(TRANSPORT) && task.action_kind == TRANSPORT => {
                self.retake_transport((player, idx), (target, point), task, clears)?
            }
            // the top action's slot 0x48 (FUN_140a977a0) on the same task: the base's
            // (FUN_140a7bf40: the MoveTo's, the Enter's, the missile's, the Fly's, the Make's, the
            // wait for a foundation's) answers 0, and the work goes on to the factory
            _ if top == Some(task.action_kind) && top != Some(HUNT) => match top {
                Some(MOVE_TO | ENTER | MISSILE | FLY | MAKE | WAIT_SOLIDIFY) => false,
                _ => {
                    return Err(GenieError::unsupported(
                        "a work command on the unit's current task",
                    )
                    .with("action", format_args!("{top:?}"))
                    .with("task", task.id.get())
                    .with("target", ObjectId::field(target)));
                }
            },
            _ => false,
        })
    }

    /// A Hunt on the work's task retargets (slot 9, [`World::retarget_hunt`]) and its unit AI
    /// hears the hunt order and action on the target at the work's point; a hunt sent to its
    /// player's master 84 is not modelled.
    pub(super) fn retarget_hunt_work(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        (target, point, clears): (ObjectId, Position, bool),
    ) -> Result<(), GenieError> {
        let t = self.task_target(target, 0)?;
        if t.master.id == MARKET && t.player == player {
            return Err(GenieError::unsupported(
                "a hunt sent to its player's master 84",
            ));
        }
        let class = t.master.class;
        self.retarget_hunt(player, idx, target)?;
        let unit = self.players.live_mut(player, idx)?;
        if let Some(ai) = self.parts.unit_ai.get_mut(unit.id) {
            ai.notify_work(
                Some(OrderKind::Hunt),
                Some(AiAction::Hunt),
                Some((target, class)),
                point,
                clears,
                self.streams.get(Stream::Ai),
            )?;
            self.remember_order(player, idx, (Some(OrderKind::Hunt), Some(AiAction::Hunt)))?;
        }
        Ok(())
    }

    /// A Gather on the work's task retargets (slot 9, [`World::retarget_gather`]); when it took
    /// the target its unit AI hears the gather order and action on it at the work's point.
    /// Answers whether it took it; a gather sent to its player's master 84 is not modelled.
    pub(super) fn retarget_gather_work(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        (target, point, clears): (ObjectId, Position, bool),
    ) -> Result<bool, GenieError> {
        let t = self.task_target(target, 0)?;
        if t.master.id == MARKET && t.player == player {
            return Err(GenieError::unsupported(
                "a gather sent to its player's master 84",
            ));
        }
        let class = t.master.class;
        let took = self.retarget_gather(player, idx, target)?;
        if took {
            let unit = self.players.live_mut(player, idx)?;
            if let Some(ai) = self.parts.unit_ai.get_mut(unit.id) {
                ai.notify_work(
                    Some(OrderKind::Gather),
                    Some(AiAction::Gather),
                    Some((target, class)),
                    point,
                    clears,
                    self.streams.get(Stream::Ai),
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

    /// The action list's factory (slot 0x10) for the work's `task` on `target`: the Enter,
    /// Attack, Hunt, Build, Repair, Gather or Transport starts; a mule cart's task makes no
    /// action (the move without one, [`World::work_move`]).
    pub(super) fn start_task(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        (target, point): (Option<ObjectId>, Position),
        task: &Task,
        (clears, notified): (bool, bool),
    ) -> Result<(), GenieError> {
        match task.action_kind {
            // the factory makes no action of a mule cart's task: the move without one
            CART_TASK => self.work_move(player, idx, target, point, (clears, notified)),
            TRANSPORT => self.start_transport((player, idx), (target, point), task, clears),
            ENTER | ATTACK_TASK | HUNT | ActionKind::BUILD | REPAIR | GATHER => {
                let target = target.ok_or(TERRAIN_TASK)?;
                self.start_object_task(player, idx, (target, point), task, clears)
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
        clears: bool,
    ) -> Result<(), GenieError> {
        match task.action_kind {
            ENTER => self.start_enter(player, idx, target, task, point, clears),
            ATTACK_TASK => self.start_attack(player, idx, target, task, point, clears),
            HUNT => self.start_hunt(HuntOrder {
                player,
                index: idx,
                target,
                task: HuntTask {
                    id: task.id,
                    range: task.work_range,
                    resource: task.attribute_type,
                    sprites: task.sprites,
                    work_value: task.work_val1,
                    productivity: task.attribute_type2,
                    kind4: task.attribute_type4,
                },
                point,
                clears,
                notify: true,
            }),
            ActionKind::BUILD => self.start_build(player, idx, target, task, point, clears),
            REPAIR => self.start_repair(player, idx, target, task, point, clears),
            GATHER => self.start_gather(player, idx, target, task, Some((point, clears))),
            _ => Err(UNMODELLED_FACTORY),
        }
    }

    /// The work command without an action (`FUN_14106ff70`: no task, or one the action list's
    /// factory makes no action of, the mule cart's 0x99): a unit whose master moves (slot 0xe8,
    /// its speed `+0x740`) is told of a move (slot 0x828, its notifies cleared with `clears`) and,
    /// in state 2 or of class 51, moves to the target or the point (slot 0x138 with the unit's
    /// `+0xa14`, the work's `notified`); one that does not move does nothing.
    pub(super) fn work_move(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: Option<ObjectId>,
        point: Position,
        (clears, notified): (bool, bool),
    ) -> Result<(), GenieError> {
        let unit = &self.players.live(player, idx)?;
        let master = unit.master(self.master_tables(), player)?;
        if !master.moving.is_some_and(|m| Speed::ZERO < m.speed) {
            return Ok(());
        }
        // slot 0x828 with 1 (the move's order and action) for a unit in state 2, -1 for
        // another; then the move to the target (slot 0x138) for a unit in state 2 or of
        // class 51
        let (alive, class) = (unit.life.state == ObjectState::Alive, master.class);
        let named = match target {
            None => None,
            Some(target) => {
                let (tp, t) = self.object(target).ok_or(GenieError::unsupported(
                    "a work command's target the slice does not hold",
                ))?;
                Some((target, t.master(self.master_tables(), tp)?.class))
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
                clears,
                self.streams.get(Stream::Ai),
            )?;
            self.remember_order(player, idx, (order, action))?;
        }
        if alive || class == class::CLASS_51 {
            match target {
                None => self.move_point_order_with(player, idx, point, notified)?,
                Some(target) => {
                    let _ = self.move_to_object_with(player, idx, target, point, notified)?;
                }
            }
        }
        Ok(())
    }

    /// The unit's slot 0x820 (`FUN_14106c6a0`) on `target` (its place the point): the task its
    /// master's list gives for the target with `filter` (the list's slot 0x10, [`find_task`] in
    /// mode 0); without one, a master with a task swap group (`+0x7b8` not 0) tries each of its
    /// player's masters by id of the same class, type and group, its own too, with any filter;
    /// the first with a task gives it, and with `change` the unit takes that master (slot 0x90,
    /// [`World::change_master`], or [`World::transform_building`] for a building) though it be its
    /// own. None: no task. The terrain under the point, which only a task for a terrain reads, is
    /// not passed.
    ///
    /// [`find_task`]: crate::unit_ai::task_finder::find_task
    pub(crate) fn find_work_task(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
        filter: Option<ActionKind>,
        change: bool,
    ) -> Result<Option<Task>, GenieError> {
        let unit = &self.players.live(player, idx)?;
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
        let t = self.task_target(target, 1)?;
        let general = self.task_target(target, 2)?;
        // slot 0x820's point: the target's place
        let at = self
            .object(target)
            .map_or(Position::ORIGIN, |(_, u)| u.position());
        let terrain = self.task_terrain(at);
        let find = |list: &Master, filter: Option<ActionKind>| {
            find_task(
                list,
                &actor,
                Some(&t),
                Some(&general),
                terrain,
                filter,
                &world,
            )
            .map(Option::<&Task>::copied)
        };
        if let Some(task) = find(own, filter)? {
            return Ok(Some(task));
        }
        if own.task_by_group == TaskGroup::NONE {
            return Ok(None);
        }
        let mut found = None;
        // the master table exists: master_of found the unit's own in it
        for candidate in self.model.masters.checked(player.index())?.iter() {
            if candidate.class != own.class
                || candidate.kind != own.kind
                || candidate.task_by_group != own.task_by_group
            {
                continue;
            }
            if let Some(task) = find(candidate, None)? {
                found = Some((candidate.id, task));
                break;
            }
        }
        let Some((with, task)) = found else {
            return Ok(None);
        };
        if change {
            if self.master(player, with)?.kind == MasterKind::BUILDING {
                self.transform_building(player, idx, with)?;
            } else {
                self.change_master(player, idx, with)?;
            }
        }
        Ok(Some(task))
    }
}
