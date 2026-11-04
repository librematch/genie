//! A group's orders (FD-036): `BUnitGroup::command` (`FUN_14100efd0`), `commandMoveTo`
//! (`FUN_141010a80`) and `stopAllUnits` (`FUN_14103a0d0`), the HD PDB's names.

use crate::groups::CommandArgs;
use crate::groups::FormationKind;
use crate::groups::GroupCommandKind;
use crate::groups::GroupFlags;
use crate::groups::GroupId;
use crate::groups::GroupMove;
use crate::groups::MotionFlags;
use crate::groups::MovePath;
use crate::groups::MoveReason;
use crate::groups::QueueRun;
use crate::groups::attack::AttackOrder;
use crate::groups::formation::top_path;
use crate::groups::{GroupCommand, GroupTarget, SpecificMove, UnitGroup};
use crate::map::Distance;
use crate::map::Point;
use crate::map::WholeDistance;
use crate::math::fmath::BVector;
use crate::math::fmath::{at_most_or_unordered, below_or_unordered, sqrt};
use crate::math::image::group::EPSILON as GROUP_EPSILON;
use crate::movement::PathData;
use crate::movement::PathFlags;
use crate::movement::PathState;
use crate::movement::PlanTries;
use crate::movement::Speed;
use crate::movement::WaypointIndex;
use crate::movement::WaypointLevel;
use crate::objects::masters::ActionKind;
use crate::objects::masters::MasterKind;
use crate::objects::{ObjectId, ObjectKey};
use crate::pathing::tile_path::Path;
use crate::world::Seconds;
use crate::world::World;
use crate::world::WorldTime;
use crate::world::index::Place;
use crate::world::layer::generated::GroupTuning;
use crate::world::slots::{CheckedIndex, PlayerSlots};
use crate::{GenieError, GenieResult};

/// A group whose top speed is below this does not move (`commandMoveTo`, `DAT_142eb0c60`); nor is
/// a path shorter than it one.
const EPSILON: f32 = GROUP_EPSILON;
/// The group's command while it moves (`mCurrentCommand` 1).
pub const MOVE_COMMAND: GroupCommandKind = GroupCommandKind::MOVE;

/// The queued commands on a target `checkQueuedOrders` hands to `FUN_14100fc50` when the world
/// holds the target (its switch's cases 3 and `0x2713`).
const TARGET_COMMANDS: [GroupCommandKind; 11] =
    GroupCommandKind::table([3, 5, 0xc, 0x65, 0x69, 0x6a, 0x6e, 0x6f, 0x84, 0x88, 0x2713]);
/// The queued attacks `checkQueuedOrders` hands to `FUN_14100f7f0` when the world holds the
/// target.
pub(super) const ATTACK_COMMANDS: [GroupCommandKind; 4] =
    GroupCommandKind::table([7, 9, 0x68, 0x7d]);

bitflags::bitflags! {
    /// `commandMoveTo`'s flags `param_6` to `param_11`.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct MoveFlags: u8 {
        /// `param_6`: a patrol's leg, whose path loops.
        const LEG = 1;
        /// `param_7`: the group forgets its target (`mCurrentTarget`).
        const FORGET_TARGET = 1 << 1;
        /// `param_8`.
        const HOLD = 1 << 2;
        /// `param_9`: the move goes into the order queue.
        const QUEUED = 1 << 3;
        /// `param_10`: the move runs though orders are queued.
        const DIRECT = 1 << 4;
        /// `param_11`.
        const KEEP_FLAG = 1 << 5;
    }
}

/// What `commandMoveTo` is told besides the target and the point: the range (`param_5`) and its
/// flags.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MoveTo {
    /// `param_5`: the path level's range.
    pub(crate) range: Distance,
    /// `param_6` to `param_11`.
    pub(crate) flags: MoveFlags,
}

impl UnitGroup {
    /// `queueOrder` (`FUN_14102b640`): `command` at the queue's end (`+0x420`, its count
    /// `+0x450`): its command, target, point, range, approved byte, retargeting byte and the 16
    /// bits after them.
    pub fn queue_command(&mut self, command: GroupCommand) {
        self.command_queue.push(command);
    }

    /// `FUN_14100dd10`: the queue's approved commands go and the others turn approved.
    pub fn approve_queued(&mut self) {
        self.command_queue.retain(|c| c.approved == 0);
        for c in &mut self.command_queue {
            c.approved = 1;
        }
    }

    /// `FUN_14100f6b0`: [`GroupFlags::MEMBERS_SENT`] clears; with [`MoveFlags::FORGET_TARGET`] the
    /// target goes (-1, type -1, its place, range and radius 0, its retargeting byte 0); the rams
    /// are not re-added (`FUN_14102b730`, gated by `DAT_143f01837`, which nothing writes); every
    /// member's seconds since attacking are `f32::MAX`; the waypoints (`+0x398`, count `+0x3a0`)
    /// take (`at`'s x, the desired height, its y). The array's growth (5, then twice) fails only
    /// without memory.
    pub fn add_waypoint(&mut self, at: Point, how: MoveFlags) {
        self.flags &= !GroupFlags::MEMBERS_SENT;
        if how.contains(MoveFlags::FORGET_TARGET) {
            let t = &mut self.current_target;
            t.target = GroupTarget::None;
            t.original_position = BVector::ZERO;
            t.range = Distance::ZERO;
            t.target_radius = Distance::ZERO;
            t.disable_retargeting = 0;
        }
        for m in &mut self.members {
            m.seconds_since_attacking = Seconds::MAX;
        }
        self.waypoints
            .push(BVector::new(at.x(), self.desired_height, at.y()));
    }
}

/// Which members `stopAllUnits` ([`World::stop_all_units`]) stops: those with an action (slot
/// 0x530 not -1), or every one (its flag).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StopScope {
    /// The members with an action.
    Busy,
    /// Every member.
    All,
}

impl World {
    /// `allHordeUnits` (`FUN_141008e50`): no member the world holds is of a master above type 69
    /// with a formation category (`+0x8bc`).
    ///
    /// # Errors
    ///
    /// A member of a master the slice does not have.
    ///
    /// When the world does not hold `group`: the game's routine gets the group itself, from a
    /// caller that holds it.
    pub fn all_horde_units(&self, group: GroupId) -> GenieResult<bool> {
        let g = self.groups.held(group)?;
        self.horde_units_only(g)
    }

    /// [`World::all_horde_units`] of group `g`.
    ///
    /// # Errors
    ///
    /// A member of a master the slice does not have.
    pub fn horde_units_only(&self, g: &UnitGroup) -> GenieResult<bool> {
        for m in &g.members {
            if let Some((p, u)) = self.object(m.id) {
                let master = u.master(self.master_tables(), p)?;
                if MasterKind::COMBAT <= master.kind
                    && master.creatable_part().formation_category.grouped()
                {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    /// [`World::command_move_to`]'s queue: a queued move joins the group's command queue (direct
    /// as its flag; a group without a command that is told directly hibernates) and is not taken
    /// (`Some(NotBegun)`); an indirect move while commands are queued joins the queue too, which is
    /// approved and run ([`World::check_queued_orders`], `Some(Begun)`). None: the move runs now.
    ///
    /// # Errors
    ///
    /// A queued command the queue cannot run.
    ///
    /// When the world does not hold `group`.
    fn queue_move(
        &mut self,
        group: GroupId,
        point: Point,
        how: MoveTo,
    ) -> GenieResult<Option<GroupMove>> {
        let g = self.groups.held_mut(group)?;
        let at = BVector::new(point.x(), 0.0, point.y());
        if how.flags.contains(MoveFlags::QUEUED) {
            g.queue_command(GroupCommand {
                command: MOVE_COMMAND,
                target_id: None,
                range: how.range,
                point: at,
                approved: u8::from(how.flags.contains(MoveFlags::DIRECT)),
                disable_retargeting: 0,
                approved_2: 0xffff,
            });
            if g.current_command == GroupCommandKind::NONE
                && how.flags.contains(MoveFlags::DIRECT)
                && g.hibernating != 1
            {
                g.hibernating = 1;
            }
            return Ok(Some(GroupMove::NotBegun));
        }
        if !how.flags.contains(MoveFlags::DIRECT) && !g.command_queue.is_empty() {
            g.queue_command(GroupCommand {
                command: MOVE_COMMAND,
                target_id: None,
                range: how.range,
                point: at,
                approved: 0,
                disable_retargeting: 0,
                approved_2: 0xffff,
            });
            g.approve_queued();
            self.check_queued_orders(group)?;
            return Ok(Some(GroupMove::Begun));
        }
        Ok(None)
    }

    /// `commandMoveTo` (`FUN_141010a80`) of group `group` to `point` (`target` -1 for none): flags
    /// bits 5 and 12 clear; a changed target resets the attack plan's entries (`FUN_140fd76a0` on
    /// `+0x3b0`, `mCurrentAttackPlan`, which no save holds, every load empties and only a group's
    /// attack fills, which the slice does not run: empty here). A queued move goes into the command
    /// queue ([`UnitGroup::queue_command`], approved with [`MoveFlags::DIRECT`]) and, for a group
    /// without a command, [`MoveFlags::DIRECT`] makes it hibernate (`+0x13c` 1); not begun. A move
    /// behind queued commands without [`MoveFlags::DIRECT`] joins them unapproved, the approved
    /// ones go ([`UnitGroup::approve_queued`]) and the queue runs ([`World::check_queued_orders`]);
    /// begun. Else a group whose last command was not queued empties its command queue
    /// ([`UnitGroup::clear_command_queue`]); [`MoveFlags::FORGET_TARGET`] clears its target
    /// (`mCurrentTarget`: -1 and type -1, its place, range, radius and retargeting byte 0); its
    /// rams are not re-added (`readdRams`, `FUN_14102b730`, gated by `DAT_143f01837`, which nothing
    /// writes). Each member's seconds since attacking are `f32::MAX`. A horde moves as one
    /// (`commandHorde`, not modelled). A changed target, or a patrol's path (the top path's
    /// [`PathFlags::LOOPS`], [`crate::groups::formation::top_path`], or [`GroupFlags::PATROL`])
    /// when neither [`MoveFlags::LEG`] nor [`MoveFlags::HOLD`], sets
    /// [`GroupFlags::RELAYOUT_AT_TURN`] and [`GroupFlags::PLACE_AGAIN`]. Below [`EPSILON`] of top
    /// speed nothing moves (not begun). The path (a new one, flags [`PathFlags::NEW`], made now)
    /// runs from the group's place through its waypoints (which it empties) to the point at the
    /// group's desired height, without [`PathFlags::LOOPS`] and with [`PathFlags::BACK_AND_FORTH`];
    /// a patrol's leg ([`MoveFlags::LEG`]) loops instead and goes back and forth when the point is
    /// at least `GroupTuning::patrol_leg_reach` from the group on the ground (`COMISS`/`JBE` at
    /// 0x141010de3: a NaN distance too). [`GroupFlags::PATROL`] ends set only with
    /// [`MoveFlags::HOLD`], which also clears the path's [`PathFlags::LOOPS`]
    /// ([`MoveFlags::KEEP_FLAG`]'s [`GroupFlags::PATROL`] is cleared again after it, and a leg
    /// leaves it to [`MoveFlags::HOLD`] alone). Measured, under two points or at most [`EPSILON`]
    /// of length (`COMISS`/`JC` at 0x141010e2a: a NaN length keeps it) it is none. The group's path
    /// data resets ([`PathData`]'s `FUN_140f98e80`) and takes it (`FUN_140f9b800`: range, target,
    /// waypoint 1, state 2) as waypoint level 2 (that level even without a path); the members stop
    /// ([`World::stop_all_units`]) and the unit AIs of those the world's table holds, sleeping ones
    /// too, drop their notifies at the world's time (`FUN_140c5dc10`); the target's place is the
    /// point, the command [`MOVE_COMMAND`]. With [`MotionFlags::NEW`] clear and no patrol the
    /// formation follows (`UpdateDesiredPositions`: a column without a path or when the player's
    /// `mMinColumnDistance` is below its length, `COMISS`/`JNC` at 0x141010fc8, a NaN too; else a
    /// line). [`GroupFlags::TURN`] is set. Begun.
    ///
    /// # Errors
    ///
    /// A horde, a queued command the queue cannot run ([`World::check_queued_orders`]),
    /// or what the members' stop and the formation do not model.
    ///
    /// When the world does not hold `group`: the game's routine gets the group itself, from a
    /// caller that holds it.
    pub fn command_move_to(
        &mut self,
        group: GroupId,
        target: Option<ObjectId>,
        point: Point,
        how: MoveTo,
    ) -> GenieResult<GroupMove> {
        let world_time = self.world_time;
        let g = self.groups.held_mut(group)?;
        g.flags &= !(GroupFlags::MEMBERS_SENT | GroupFlags::FOLLOWED);
        // the game compares the two integers: a target of none changes a group with one
        let changed = target != g.current_target.target.key();
        if let Some(done) = self.queue_move(group, point, how)? {
            return Ok(done);
        }
        let g = self.groups.held_mut(group)?;
        if g.replaces_queue != 0 {
            g.clear_command_queue();
        }
        if how.flags.contains(MoveFlags::FORGET_TARGET) {
            let t = &mut g.current_target;
            t.target = GroupTarget::None;
            t.original_position = BVector::ZERO;
            t.range = Distance::ZERO;
            t.target_radius = Distance::ZERO;
            t.disable_retargeting = 0;
        }
        for m in &mut g.members {
            m.seconds_since_attacking = Seconds::MAX;
        }
        if self.all_horde_units(group)? {
            return Err(GenieError::unsupported(
                "a horde's move (commandHorde, FUN_141010180)",
            ));
        }
        let Some((patrol, length)) = self.set_move_level(group, point, how, (target, changed))?
        else {
            return Ok(GroupMove::NotBegun);
        };
        self.stop_all_units(group, StopScope::Busy)?;
        let ids: Vec<ObjectId> = self
            .groups
            .held(group)?
            .members
            .iter()
            .map(|m| m.id)
            .collect();
        for id in ids {
            // FUN_1415417a0 finds a sleeping member too
            if let Some(ai) = self.parts.unit_ai.get_mut(id) {
                ai.clear_notifies(world_time.since_start())?;
            }
        }
        let owner = self.groups.held(group)?.owner_player()?;
        let min_column = self
            .players
            .get(owner.index())
            .map_or(WholeDistance::ZERO, |p| p.formations.min_column_distance);
        let g = self.groups.held_mut(group)?;
        g.current_target.original_position = g
            .current_target
            .original_position
            .with_x(point.x())
            .with_y(point.y());
        g.current_command = MOVE_COMMAND;
        if !g.motion.flags.contains(MotionFlags::NEW) && !patrol {
            let column =
                length.is_none_or(|l| below_or_unordered(min_column.distance().get(), l.get()));
            self.update_desired_positions(
                group,
                Some(if column {
                    FormationKind::Column
                } else {
                    FormationKind::Line
                }),
            )?;
        }
        let g = self.groups.held_mut(group)?;
        g.flags |= GroupFlags::TURN;
        Ok(GroupMove::Begun)
    }

    /// [`World::command_move_to`]'s path and level: the group's walked level takes the path from
    /// its place through its waypoints to `point`. Its (patrol, length), None for a group whose
    /// top speed is below the tolerance.
    fn set_move_level(
        &mut self,
        group: GroupId,
        point: Point,
        how: MoveTo,
        (target, changed): (Option<ObjectId>, bool),
    ) -> GenieResult<Option<(bool, Option<Distance>)>> {
        let world_time = self.world_time;
        let g = self.groups.held_mut(group)?;
        // FUN_141046500: the top ancestor's path, else the walked level's
        let patrol = top_path(&g.motion.paths)
            .is_some_and(|l| l.path_flags.contains(PathFlags::LOOPS))
            || g.flags.contains(GroupFlags::PATROL);
        if (!how.flags.contains(MoveFlags::HOLD) && !how.flags.contains(MoveFlags::LEG) && patrol)
            || changed
        {
            g.flags |= GroupFlags::RELAYOUT_AT_TURN | GroupFlags::PLACE_AGAIN;
        }
        // the top speed below the tolerance (`COMISS`/`JA` at 0x141010cc3)
        if g.motion.maximum_velocity_forward < Speed::new(EPSILON) {
            return Ok(None);
        }
        let mut path = Path::default();
        let _ = path.push(g.motion.position[0]);
        for &w in &g.waypoints {
            let _ = path.push(w);
        }
        g.waypoints.clear();
        let at = BVector::new(point.x(), g.desired_height, point.y());
        let _ = path.push(at);
        let mut path_flags = move_path_flags(
            g.motion.position[0],
            at,
            how.flags.contains(MoveFlags::LEG),
            &self.model.tuning.group,
        );
        // `MoveFlags::KEEP_FLAG` sets PATROL (0x141010dc1) off a leg, which the hold's test then decides
        // again
        if how.flags.contains(MoveFlags::HOLD) {
            path_flags &= !PathFlags::LOOPS;
            g.flags |= GroupFlags::PATROL;
        } else {
            g.flags &= !GroupFlags::PATROL;
        }
        path.measure();
        let path = (2 <= path.points.len() && below_or_unordered(EPSILON, path.length.get()))
            .then_some(path);
        if path.is_none() {
            g.flags &= !GroupFlags::PATROL;
        }
        // FUN_140f98e80: the level and its parents go, its repath time stays
        let repath_time = g
            .motion
            .paths
            .first()
            .map_or(Seconds::ZERO, |l| l.repath_time);
        g.motion.paths.truncate(1);
        let length = path.as_ref().map(|p| p.length);
        let level = move_level(path, path_flags, repath_time, world_time, how.range, target);
        if g.motion.paths.is_empty() {
            g.motion.paths.push(level);
        } else {
            *g.motion.paths.checked_mut(0)? = level;
        }
        Ok(Some((patrol, length)))
    }

    /// `checkQueuedOrders` (`FUN_14100d460`) of group `group`: with no queued command a hibernating
    /// group wakes (`+0x13c` 0; flags `0x4c000` when it was 1). Else the last command replaced none
    /// (`+0x13d` 0) and the approved commands run in order (unapproved ones wait): each takes the
    /// run's first command if none was yet and leaves the queue; a move (1), or a command on a
    /// target the world's table does not hold, adds its point as a waypoint while more than one
    /// command is queued ([`UnitGroup::add_waypoint`]); a command on a held target (3, 5, 0xc,
    /// 0x65, 0x69, 0x6a, 0x6e, 0x6f, 0x84, 0x88, 0x2713: `FUN_14100fc50`; 7, 9, 0x68, 0x7d: the
    /// attack, `FUN_14100f7f0`) ends the run; any other is dropped. The run also ends when the next
    /// command differs from its first (the queue's end counts as -1). Then, when an approved
    /// command ran and none ended the run, the group moves to the last command of the run
    /// ([`World::command_move_to`] with its range, the target forgotten, [`MoveFlags::DIRECT`]);
    /// the hibernating group wakes as above.
    ///
    /// # Errors
    ///
    /// A run whose last command the game frees before its move (it reads a null command: the
    /// run's commands equal an unapproved one behind them), and what the move does not model.
    ///
    /// When the world does not hold `group`: the game's routine gets the group itself, from a
    /// caller that holds it.
    pub fn check_queued_orders(&mut self, group: GroupId) -> GenieResult<()> {
        let g = self.groups.held_mut(group)?;
        if !g.command_queue.is_empty() {
            g.replaces_queue = 0;
            self.run_queued_commands(group)?;
        }
        let g = self.groups.held_mut(group)?;
        if g.hibernating != 0 {
            if g.hibernating == 1 {
                g.flags |= GroupFlags::RELAYOUT_AT_TURN
                    | GroupFlags::PLACE_AGAIN
                    | GroupFlags::LEAD_TO_TARGET;
            }
            g.hibernating = 0;
        }
        Ok(())
    }

    /// [`World::check_queued_orders`]' run over the queue and its move to the run's last command
    /// (`bVar2`: a command on a held target ends the run without the move).
    fn run_queued_commands(&mut self, group: GroupId) -> GenieResult<()> {
        let (mut first, mut kept, mut ran, mut running) =
            (GroupCommandKind::NONE, None, false, true);
        let mut i = 0;
        while running {
            let g = self.groups.held(group)?;
            let Some(&node) = g.command_queue.get(i) else {
                break;
            };
            if node.approved == 0 {
                i += 1;
                continue;
            }
            ran = true;
            if first == GroupCommandKind::NONE {
                first = node.command;
            }
            let count = g.command_queue.len();
            running = self.run_queued_node(group, node, count)? == QueueRun::GoesOn;
            let g = self.groups.held_mut(group)?;
            let next = g
                .command_queue
                .get(i + 1)
                .map_or(GroupCommandKind::NONE, |n| n.command);
            let _ = g.command_queue.remove(i);
            let ends = first != GroupCommandKind::NONE && next != first;
            if ends || g.command_queue.is_empty() {
                kept = Some(node);
            }
            if ends {
                break;
            }
        }
        if ran && running {
            let last = kept.ok_or(GenieError::unsupported(
                "checkQueuedOrders' move to a command it freed (a null command in the game)",
            ))?;
            let _ = self.command_move_to(
                group,
                None,
                last.point.ground(),
                MoveTo {
                    range: last.range,
                    flags: MoveFlags::FORGET_TARGET | MoveFlags::DIRECT,
                },
            )?;
        }
        Ok(())
    }

    /// One approved command of [`World::check_queued_orders`]' run, `count` commands being queued:
    /// a command on a held target is the group's command (the attack, `FUN_14100f7f0`) and ends the
    /// run ([`QueueRun::Ends`]); a move, or a command on a target the world does not hold, adds its
    /// point as a waypoint while more than one is queued; any other is dropped.
    ///
    /// # Errors
    ///
    /// What the general command and the attack do not model.
    fn run_queued_node(
        &mut self,
        group: GroupId,
        node: GroupCommand,
        count: usize,
    ) -> GenieResult<QueueRun> {
        // the target with its place, when the world holds it
        let held = node
            .target_id
            .and_then(|t| self.world_object(t).map(|o| (t, o.position())));
        match (node.command, held) {
            (c, Some((target, at))) if TARGET_COMMANDS.contains(&c) => {
                let order = CommandArgs {
                    queued: false,
                    direct: true,
                    disable_retargeting: false,
                    filter: None,
                };
                let _ = self.general_command(group, (c, target), at.ground(), node.range, order)?;
                Ok(QueueRun::Ends)
            }
            (c, Some((target, _))) if ATTACK_COMMANDS.contains(&c) => {
                let order = AttackOrder {
                    disable_retargeting: node.disable_retargeting != 0,
                    filter: ActionKind::of_data(node.approved_2.cast_signed()),
                };
                let _ = self.command_attack(group, target, order)?;
                Ok(QueueRun::Ends)
            }
            (c, _)
                if (c == MOVE_COMMAND
                    || TARGET_COMMANDS.contains(&c)
                    || ATTACK_COMMANDS.contains(&c))
                    && 1 < count =>
            {
                self.groups
                    .held_mut(group)?
                    .add_waypoint(node.point.position().ground(), MoveFlags::empty());
                Ok(QueueRun::GoesOn)
            }
            _ => Ok(QueueRun::GoesOn),
        }
    }

    /// `stopAllUnits` (`FUN_14103a0d0`) of group `group`: each member the world holds with an
    /// action (slot 0x530, `actionType`, not -1), or every one with `all`, stops (slot 0x148,
    /// [`World::object_stop`]; slot 0x560: its movement stands and its path level resets) and its
    /// own move clears (active 0, path and reason -1, the rest 0).
    ///
    /// # Errors
    ///
    /// A member with an action the slice does not model or a
    /// sleeping one, or what the stop does not model.
    ///
    /// When the world does not hold `group`: the game's routine gets the group itself, from a
    /// caller that holds it.
    pub fn stop_all_units(&mut self, group: GroupId, scope: StopScope) -> GenieResult<()> {
        let ids: Vec<ObjectId> = self
            .groups
            .held(group)?
            .members
            .iter()
            .map(|m| m.id)
            .collect();
        for (k, id) in ids.into_iter().enumerate() {
            self.stop_listed_unit(group, (k, id), scope)?;
        }
        Ok(())
    }

    /// Whether [`World::stop_all_units`] stops the member at `place`: one with an action takes
    /// the stop, one without only with [`StopScope::All`] (slot 0x530: the top action's type, -1
    /// without one).
    fn stop_applies(&self, place: Place, scope: StopScope) -> GenieResult<bool> {
        let u = match place {
            Place::Live(p, i) => self.players.live(p, i)?,
            Place::Sleeping(p, i) => self.players.asleep(p, i)?,
        };
        if self.parts.unread.contains(u.id) {
            return Err(GenieError::unsupported(
                "stopAllUnits of a member with an action the slice does not model",
            ));
        }
        Ok(scope == StopScope::All || self.parts.actions_of(u.id).top_action_kind().is_some())
    }

    /// [`World::stop_all_units`]' stop of the member `id` at index `k` of group `group`.
    fn stop_listed_unit(
        &mut self,
        group: GroupId,
        (k, id): (usize, ObjectId),
        scope: StopScope,
    ) -> GenieResult<()> {
        // FUN_1415417a0: a sleeping member too
        let Some(place) = self.place(id) else {
            return Ok(());
        };
        if !self.stop_applies(place, scope)? {
            return Ok(());
        }
        let Place::Live(p, idx) = place else {
            return Err(GenieError::unsupported(
                "stopAllUnits' stop of a sleeping member (the stop takes the live list)",
            ));
        };
        self.object_stop(p, idx)?;
        if let Some(m) = self.parts.mover_mut(self.players.live(p, idx)?.id) {
            m.stand()?;
        }
        let g = self.groups.held_mut(group)?;
        g.members.checked_mut(k)?.specific_move = SpecificMove {
            path_id: MovePath::NONE,
            reason: MoveReason::None,
            ..SpecificMove::default()
        };
        Ok(())
    }
}

/// [`World::command_move_to`]'s path flags: a patrol leg sets [`PathFlags::LOOPS`] and, when the
/// group's place `from` lies at least `GroupTuning::patrol_leg_reach` from `to` on the ground
/// (`COMISS`/`JBE`: a NaN too), [`PathFlags::BACK_AND_FORTH`]; any other move clears
/// [`PathFlags::LOOPS`] and sets [`PathFlags::BACK_AND_FORTH`].
fn move_path_flags(from: BVector, to: BVector, leg: bool, group_tuning: &GroupTuning) -> PathFlags {
    if leg {
        let mut d = from - to;
        d = d.with_height(0.0);
        let length = sqrt(d.x() * d.x() + 0.0 + d.y() * d.y());
        if at_most_or_unordered(group_tuning.patrol_leg_reach, length) {
            // A new path's flags (`FUN_14152eea0`'s construction).
            // The plans a path level may still try (`FUN_140f9b800`, `FUN_140f98e80`).
            PathFlags::NEW | PathFlags::LOOPS | PathFlags::BACK_AND_FORTH
        } else {
            (PathFlags::NEW | PathFlags::LOOPS) & !PathFlags::BACK_AND_FORTH
        }
    } else {
        (PathFlags::NEW & !PathFlags::LOOPS) | PathFlags::BACK_AND_FORTH
    }
}

/// [`World::command_move_to`]'s new path level (`FUN_140f98e80`'s reset keeping the repath time,
/// then `FUN_140f9b800`): with a path its points, flags, length, creation time, range, target,
/// waypoint 1 and state 2; waypoint level 2 either way.
fn move_level(
    path: Option<Path>,
    path_flags: PathFlags,
    repath_time: Seconds,
    world_time: WorldTime,
    range: Distance,
    target: Option<ObjectId>,
) -> PathData {
    let mut level = PathData {
        repath_time,
        ..PathData::without_path()
    };
    if let Some(p) = path {
        // FUN_140f9b800
        level.points = p.points;
        level.has_path = true;
        level.path_flags = path_flags;
        level.length = p.length;
        level.created = world_time;
        level.range = range;
        level.target = target;
        level.waypoint = WaypointIndex::SECOND;
        level.state = PathState::WantsPath;
        level.continue_counter = PlanTries::FULL;
    }
    level.level = WaypointLevel::Top;
    level
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::groups::MotionFlags;
    use crate::groups::{GroupCommand, Member, UnitGroups};
    use crate::objects::ListIndex;
    use crate::objects::masters::CreatablePart;
    use crate::objects::masters::FormationClass;
    use crate::objects::masters::{Master, Masters};
    use crate::objects::unit::Unit;
    use crate::players::PlayerId;
    use crate::test_ids::group_id;
    use crate::test_ids::master_id;
    use crate::test_ids::object_id;
    #[cfg(test)]
    use crate::unit_ai::UnitAi;
    #[cfg(test)]
    use crate::world::FrameSeed;
    use crate::world::Millis;
    use crate::world::Player;
    use std::sync::Arc;

    /// Player 1's unit 7 (master 4: type 70, category 1, no action, no unit AI) in group 1 at
    /// the origin, its top speed 1, its [`MotionFlags::NEW`] set (no formation follows), at world
    /// time 50.
    fn moving_world() -> World {
        let mut u = Unit {
            id: object_id(7),
            ..Unit::default()
        };
        u.life.master_id = master_id(4);
        let players = vec![
            Player::default(),
            Player {
                objects: vec![u].into(),
                ..Player::default()
            },
        ];
        let mut t = World::new(
            WorldTime::from_millis(50),
            FrameSeed::of_data(0),
            PlayerId::new(0),
            vec![Millis::ZERO, Millis::ZERO],
            players,
        );
        let master = Master {
            id: master_id(4),
            kind: MasterKind::COMBAT,
            creatable: Some(CreatablePart {
                formation_category: FormationClass::of_data(1),
                ..CreatablePart::default()
            }),
            ..Master::default()
        };
        t.model.masters = vec![Masters::new([master]); 2]
            .into_iter()
            .map(Arc::new)
            .collect();
        let mut g = UnitGroup::new(group_id(1));
        g.owner = Some(PlayerId::new(1));
        g.members = vec![Member {
            id: object_id(7),
            ..Member::default()
        }];
        g.motion.flags = MotionFlags::NEW;
        g.motion.maximum_velocity_forward = Speed::new(1.0);
        t.groups = UnitGroups {
            groups: vec![g],
            ..UnitGroups::default()
        };
        t
    }

    fn command(command: i32, target: i32, x: f32, z: f32, approved: u8) -> GroupCommand {
        GroupCommand {
            command: GroupCommandKind::named(command),
            target_id: ObjectId::of_field(target),
            range: Distance::new(0.5),
            point: BVector::new(x, 0.0, z),
            approved,
            disable_retargeting: 0,
            approved_2: 0xffff,
        }
    }

    /// `FUN_141010a80` with `param_9`: the move joins the command queue (`queueOrder`, approved
    /// with [`MoveFlags::DIRECT`]) and nothing moves; a group without a command hibernates for a
    /// direct one.
    #[test]
    fn queued_move_joins_the_command_queue() {
        let mut t = moving_world();
        let how = MoveTo {
            range: Distance::new(0.5),
            flags: MoveFlags::QUEUED | MoveFlags::DIRECT,
        };
        assert_eq!(
            t.command_move_to(group_id(1), None, Point::new(3.0, 4.0), how)
                .unwrap(),
            GroupMove::NotBegun
        );
        let g = t.groups.get(group_id(1)).unwrap();
        assert_eq!(g.command_queue, [command(1, -1, 3.0, 4.0, 1)]);
        assert_eq!(
            (g.hibernating, g.current_command),
            (1, GroupCommandKind::NONE)
        );
        assert!(!g.motion.paths[0].has_path);
    }

    /// `FUN_14100dd10` drops the approved commands and approves the others.
    #[test]
    fn approving_the_queue_drops_the_approved_commands() {
        let mut g = UnitGroup::new(group_id(1));
        g.command_queue = vec![command(1, -1, 1.0, 1.0, 1), command(9, 5, 2.0, 2.0, 0)];
        g.approve_queued();
        assert_eq!(g.command_queue, [command(9, 5, 2.0, 2.0, 1)]);
    }

    /// `FUN_14100d460`: an unapproved command waits; of the approved moves each but the last
    /// adds its point as a waypoint while more than one command is queued, and the group moves
    /// through them to the last; a command on a target the world does not hold counts as a
    /// move, one on a held target is not modelled.
    #[test]
    fn the_queued_moves_run_through_their_points() {
        let mut t = moving_world();
        t.groups.get_mut(group_id(1)).unwrap().command_queue = vec![
            command(9, -1, 9.0, 9.0, 0),
            command(1, -1, 5.0, 5.0, 1),
            command(3, 99, 8.0, 6.0, 1),
        ];
        // 3 differs from the run's first (1): the run ends at the move to (5, 5)
        t.check_queued_orders(group_id(1)).unwrap();
        let g = t.groups.get(group_id(1)).unwrap();
        assert_eq!(
            g.command_queue,
            [command(9, -1, 9.0, 9.0, 0), command(3, 99, 8.0, 6.0, 1)]
        );
        assert_eq!(
            g.motion.paths[0].points,
            [BVector::ZERO, BVector::new(5.0, 0.0, 5.0)]
        );
        assert_eq!((g.current_command, g.replaces_queue), (MOVE_COMMAND, 0));
        // two moves: the first is a waypoint, the group goes through it to the second
        let mut t = moving_world();
        t.groups.get_mut(group_id(1)).unwrap().command_queue =
            vec![command(1, -1, 5.0, 5.0, 1), command(1, -1, 8.0, 8.0, 1)];
        t.check_queued_orders(group_id(1)).unwrap();
        let g = t.groups.get(group_id(1)).unwrap();
        assert!(g.command_queue.is_empty() && g.waypoints.is_empty());
        assert_eq!(
            g.motion.paths[0].points,
            [
                BVector::ZERO,
                BVector::new(5.0, 0.0, 5.0),
                BVector::new(8.0, 0.0, 8.0)
            ]
        );
        // a general command on a target the world holds is the group's command and ends the run
        let mut t = moving_world();
        t.groups.get_mut(group_id(1)).unwrap().command_queue = vec![command(3, 7, 1.0, 1.0, 1)];
        t.check_queued_orders(group_id(1)).unwrap();
        let g = t.groups.get(group_id(1)).unwrap();
        assert_eq!(g.command_queue, []);
        assert_eq!(
            (g.current_command, g.current_target.target),
            (
                GroupCommandKind::named(3),
                GroupTarget::Object(object_id(7))
            )
        );
        // nothing queued: a hibernating group wakes
        let mut t = moving_world();
        t.groups.get_mut(group_id(1)).unwrap().hibernating = 1;
        t.check_queued_orders(group_id(1)).unwrap();
        let g = t.groups.get(group_id(1)).unwrap();
        let marked =
            GroupFlags::RELAYOUT_AT_TURN | GroupFlags::PLACE_AGAIN | GroupFlags::LEAD_TO_TARGET;
        assert_eq!((g.hibernating, g.flags.contains(marked)), (0, true));
    }

    /// A move without [`MoveFlags::DIRECT`] behind queued commands joins them unapproved; the
    /// approved ones go and the queue runs: the group moves to the new point.
    #[test]
    fn move_behind_queued_commands_runs_the_queue() {
        let mut t = moving_world();
        t.groups.get_mut(group_id(1)).unwrap().command_queue = vec![command(1, -1, 5.0, 5.0, 1)];
        let how = MoveTo {
            range: Distance::new(0.5),
            ..MoveTo::default()
        };
        assert_eq!(
            t.command_move_to(group_id(1), None, Point::new(2.0, 3.0), how)
                .unwrap(),
            GroupMove::Begun
        );
        let g = t.groups.get(group_id(1)).unwrap();
        assert_eq!(g.command_queue, Vec::<GroupCommand>::new());
        assert_eq!(
            g.motion.paths[0].points,
            [BVector::ZERO, BVector::new(2.0, 0.0, 3.0)]
        );
    }

    /// `param_8` (hold) clears the path's [`PathFlags::LOOPS`] and leaves [`GroupFlags::PATROL`]
    /// set; a patrol is read on the top ancestor's path (`FUN_141046500`), which sets
    /// [`GroupFlags::RELAYOUT_AT_TURN`] and [`GroupFlags::PLACE_AGAIN`]; a sleeping
    /// member's unit AI drops its notifies (`FUN_140c5dc10` at the world's time); a NaN length
    /// keeps the path (`COMISS`/`JC`).
    #[test]
    fn held_move_keeps_flags_bit_11_and_patrol_is_the_top_path_s() {
        let mut t = moving_world();
        let hold = MoveTo {
            flags: MoveFlags::HOLD,
            ..MoveTo::default()
        };
        assert_eq!(
            t.command_move_to(group_id(1), None, Point::new(2.0, 3.0), hold)
                .unwrap(),
            GroupMove::Begun
        );
        let g = t.groups.get(group_id(1)).unwrap();
        assert_eq!(
            (
                g.flags & GroupFlags::PATROL,
                g.motion.paths[0].path_flags & PathFlags::LOOPS
            ),
            (GroupFlags::PATROL, PathFlags::NONE)
        );
        let mut t = moving_world();
        let mut walked = PathData::without_path();
        walked.has_path = true;
        let mut top = PathData::without_path();
        top.has_path = true;
        top.path_flags = PathFlags::LOOPS;
        t.groups.get_mut(group_id(1)).unwrap().motion.paths = vec![walked, top];
        let mut asleep = t.remove_live(PlayerId::new(1), ListIndex::new(0)).unwrap();
        asleep.parts.unit_ai = Some(UnitAi::default());
        let _ = t.push_asleep(PlayerId::new(1), asleep).unwrap();
        assert_eq!(
            t.command_move_to(group_id(1), None, Point::new(2.0, 3.0), MoveTo::default())
                .unwrap(),
            GroupMove::Begun
        );
        let g = t.groups.get(group_id(1)).unwrap();
        assert!(
            g.flags
                .contains(GroupFlags::RELAYOUT_AT_TURN | GroupFlags::PLACE_AGAIN)
        );
        let ai = t
            .parts
            .unit_ai
            .get(t.players[1].sleeping[ListIndex::new(0)].id)
            .unwrap();
        assert_eq!(ai.last_update_time, Millis::new(50));
        let mut t = moving_world();
        assert_eq!(
            t.command_move_to(
                group_id(1),
                None,
                Point::new(f32::NAN, 3.0),
                MoveTo::default()
            )
            .unwrap(),
            GroupMove::Begun
        );
        assert!(t.groups.get(group_id(1)).unwrap().motion.paths[0].has_path);
    }

    /// `param_6` (a patrol's leg): the path loops ([`PathFlags::LOOPS`]) and goes back and forth
    /// ([`PathFlags::BACK_AND_FORTH`]) from 5 tiles on the ground (`COMISS`/`JBE`: a NaN distance
    /// too); [`GroupFlags::PATROL`] clears, [`GroupFlags::RELAYOUT_AT_TURN`] and
    /// [`GroupFlags::PLACE_AGAIN`] stay off for a leg on a patrol.
    #[test]
    fn patrol_leg_loops_and_turns_back_from_five_tiles() {
        let leg = MoveTo {
            flags: MoveFlags::LEG | MoveFlags::FORGET_TARGET | MoveFlags::DIRECT,
            ..MoveTo::default()
        };
        for (point, flags) in [
            (Point::new(4.9, 0.0), 7 & !2),
            (Point::new(3.0, 4.0), 7),
            (Point::new(f32::NAN, 0.0), 7),
        ] {
            let mut t = moving_world();
            t.groups.get_mut(group_id(1)).unwrap().flags = GroupFlags::PATROL;
            assert_eq!(
                t.command_move_to(group_id(1), None, point, leg).unwrap(),
                GroupMove::Begun
            );
            let g = t.groups.get(group_id(1)).unwrap();
            assert_eq!(
                (g.motion.paths[0].path_flags, g.flags & GroupFlags::PATROL),
                (PathFlags::from_bits_retain(flags), GroupFlags::NONE),
                "{point:?}"
            );
        }
        // the group patrols already: a leg leaves RELAYOUT_AT_TURN and PLACE_AGAIN off
        let mut t = moving_world();
        let mut top = PathData::without_path();
        top.has_path = true;
        top.path_flags = PathFlags::LOOPS;
        t.groups.get_mut(group_id(1)).unwrap().motion.paths = vec![top];
        assert_eq!(
            t.command_move_to(group_id(1), None, Point::new(8.0, 0.0), leg)
                .unwrap(),
            GroupMove::Begun
        );
        assert_eq!(
            t.groups.get(group_id(1)).unwrap().flags
                & (GroupFlags::RELAYOUT_AT_TURN | GroupFlags::PLACE_AGAIN),
            GroupFlags::NONE
        );
    }
}
