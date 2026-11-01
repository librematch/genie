//! BUILD's builders: their task, the queue and the direct path, the wait for the foundation
//! and the build command.

use crate::GenieError;
use crate::commands::FOUNDATION_GONE;
use crate::construction::build::{BuildAction, BuildTask};
use crate::map::Distance;
use crate::map::Position;
use crate::math::rng::Stream;
use crate::movement::Speed;
use crate::movement::footprint_gap;
use crate::movement::move_to::{MoveToReads, WalkTo};
use crate::objects::HitPoints;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::lifecycle::Held;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::action_type;
use crate::objects::masters::master_type::MOVING;
use crate::objects::masters::player_master;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::unit_ai::ActionKind;
use crate::unit_ai::OrderKind;
use crate::unit_ai::task_finder::world::{TaskWorld, point_terrain, task_search, task_target_with};
use crate::unit_ai::task_finder::{Actor, TaskTarget};
use crate::unit_ai::world_view::Wait;
use crate::world::slots::PlayerSlots;
use crate::world::{STATIC_TYPES_END, World};

/// `work_master` returns a master the player has; one it lacks is a broken world.
const NO_WORK_MASTER: GenieError =
    GenieError::unreachable("a work master the player does not have");

impl World {
    /// BUILD's builders (the cost was paid with the foundation, [`World::found`]; the pending
    /// debit came before, [`World::debit_listed`]), the handler's last loop: each unit of the
    /// packet the world holds (`FUN_1415417a0`: any player's, asleep or not), in the packet's
    /// order, of a master of type 30 and up that moves (its slot 0xe8; COMISS: a NaN speed too),
    /// or any unit of a queued BUILD: its ignore list empties (`FUN_141197da0`); with a unit AI,
    /// a queued BUILD, or one that `commits` while the AI holds an order not ready
    /// (`FUN_140c4dfa0`), goes into its queue as the build order (slot 0x30 into slot 0x28:
    /// [`World::queue_build`], issued by the unit's own player); else its slot 0x128 on the
    /// foundation with 1: a static object's (types below 40, `FUN_141071bd0`) does nothing; a
    /// combat object's empties its unit AI's queue first ([`World::empty_queue_for_work`]); a unit's
    /// takes the Build ([`World::build_task`], [`World::build_directly`]), another player's
    /// whatever task its slot 0x7e8 finds on the foundation ([`World::order_work`]). A unit the world no
    /// longer holds is passed over. A committing BUILD's queue then takes the readiness pass with
    /// 0 (`FUN_140c42b60`, [`crate::orders::readiness`]): every queued order is ready.
    pub(crate) fn task_builders(
        &mut self,
        player: PlayerId,
        foundation: ObjectId,
        master: MasterId,
        position: Position,
        builders: &[Option<ObjectId>],
        (queued, commits): (bool, bool),
    ) -> Result<(), GenieError> {
        // the builders go through the order splitter, which counts the command
        self.groups.count_command();
        let site = (player, foundation, master);
        for &id in builders.iter().flatten() {
            let Some((q, unit)) = self.object(id) else {
                continue;
            };
            let m = unit.master(&self.masters, q)?;
            let speed = m.moving.map_or(Speed::ZERO, |s| s.speed);
            let kind = m.kind;
            let moves = MOVING <= kind && crate::math::fmath::below_or_unordered(0.0, speed.get());
            if !moves && !queued {
                continue;
            }
            let to_queue = unit
                .ai
                .as_ref()
                .is_some_and(|ai| queued || (commits && ai.has_unready_order()));
            let Some(crate::orders::Place::Live(q, idx)) = self.place(id) else {
                // a sleeping object is a static one without a unit AI: its slot 0x128 does
                // nothing
                continue;
            };
            self.direct_builder((q, idx), (id, kind), (site, position), (to_queue, commits))?;
        }
        Ok(())
    }

    /// One builder of [`World::task_builders`] at `idx` of player `q`, of master type `kind`:
    /// queued, started at once, handed the work of another player's foundation, or its ignore
    /// list cleared, by its type and owner.
    fn direct_builder(
        &mut self,
        (q, idx): (PlayerId, ListIndex),
        (id, kind): (ObjectId, MasterKind),
        (site, position): ((PlayerId, ObjectId, MasterId), Position),
        (to_queue, commits): (bool, bool),
    ) -> Result<(), GenieError> {
        let (player, foundation, _) = site;
        if to_queue {
            self.queue_builder((q, idx), site, position, commits)?;
        } else if STATIC_TYPES_END <= kind && q == player {
            let task = self.build_task(q, id, site, position)?;
            self.empty_queue_for_work(q, idx, kind)?;
            self.build_directly(q, idx, task, site, position)?;
        } else if STATIC_TYPES_END <= kind {
            self.empty_queue_for_work(q, idx, kind)?;
            // another player's unit: slot 0x128's slot 0x7e8 finds whatever task it has on
            // the foundation (a Build on an ally's), the work command's general path
            let work = crate::orders::Work {
                target: Some(foundation),
                point: Position::new(position.x(), position.y(), 0.0),
                carry_test: false,
                filter: None,
                notified: true,
            };
            self.order_work(q, idx, &work)?;
        } else if let Some(m) = self.players.live_mut(q, idx)?.body.mover_mut() {
            m.ignore_units.clear();
        }
        Ok(())
    }

    /// A builder that queues: its Build joins the queue, and under `commits` the readiness pass
    /// with 0 makes every queued order ready.
    fn queue_builder(
        &mut self,
        (q, idx): (PlayerId, ListIndex),
        site: (PlayerId, ObjectId, MasterId),
        position: Position,
        commits: bool,
    ) -> Result<(), GenieError> {
        self.queue_build(q, idx, site, position)?;
        if commits && let Some(ai) = self.players.live_mut(q, idx)?.ai.as_mut() {
            crate::orders::readiness(&mut ai.orders, false);
        }
        Ok(())
    }

    /// Slot 0x128's start with 1 on the unit at `idx` of player `q`, of master type `kind`: a
    /// combat object's and a building's (`FUN_141211b80`, types 70 and 80) set `+0xa14` and
    /// empty their unit AI's order queue (`FUN_140c5e010`); the action object's
    /// (`FUN_14106c590`, types 40 to 60) goes straight to the work.
    pub(super) fn empty_queue_for_work(
        &mut self,
        q: PlayerId,
        idx: ListIndex,
        kind: MasterKind,
    ) -> Result<(), GenieError> {
        if crate::objects::masters::master_type::COMBAT <= kind
            && let Some(ai) = self.players.live_mut(q, idx)?.ai.as_mut()
        {
            ai.orders.clear();
        }
        Ok(())
    }

    /// What slot 0x128 on `foundation` (of `master`, player `fp`'s, at `position`) takes for the
    /// builder `id` of player `q`: the master it builds with ([`Masters::work_master`], with
    /// `q`'s masters and attributes) and the Build action of that master's task. A builder
    /// whose action would be replaced, without a Build task for it or with one not enabled, is
    /// not modelled.
    ///
    /// [`Masters::work_master`]: crate::objects::masters::Masters::work_master
    pub(crate) fn build_task(
        &self,
        q: PlayerId,
        id: ObjectId,
        (fp, foundation, master): (PlayerId, ObjectId, MasterId),
        position: Position,
    ) -> Result<(MasterId, BuildAction), GenieError> {
        let masters = self.masters.of(q).ok_or(GenieError::unsupported(
            "BUILD tasks the builders: no master table",
        ))?;
        let site = player_master(&self.masters, fp, master).ok_or(GenieError::unsupported(
            "BUILD of a master the player does not have",
        ))?;
        let target = TaskTarget {
            id: foundation,
            master: site,
            player: fp,
            state: ObjectState::Foundation,
            held: Held::default(),
            hp: HitPoints::new(1.0),
            max_hp: HitPoints::of_master(site.hit_points),
            attackable: true,
            pasture: None,
            top_action: None,
        };
        let world = TaskWorld {
            players: &self.players,
            drop_sites: &self.drop_sites,
        };
        let owner = world.players.player(q)?;
        let unit = owner
            .objects
            .by_id(id)
            .ok_or(GenieError::unsupported("a builder the slice does not have"))?;
        if unit.actions.artifact().is_some()
            || unit.production.is_some()
            || unit.actions.fly().is_some()
        {
            return Err(GenieError::unsupported(
                "a builder with an action (its replacement)",
            ));
        }
        let actor = Actor {
            id,
            master: unit.master_in(Some(masters))?,
            player: q,
            held: unit.life.held(),
            attribute_95: owner.attributes.get(95).copied().unwrap_or(Amount::ZERO),
            civ: owner.civ,
        };
        // the work command's point: BUILD's x and y on the ground
        let on = crate::unit_ai::task_finder::TaskOn {
            target: Some(&target),
            general: Some(&target),
            terrain: self.task_terrain(Position::new(position.x(), position.y(), 0.0)),
        };
        let (with, task) =
            masters
                .work_master(&actor, &on, None, &world)?
                .ok_or(GenieError::unsupported(
                    "a builder with no task for its foundation",
                ))?;
        if task.action_type != action_type::BUILD {
            return Err(GenieError::unsupported(
                "a builder whose task for its foundation is not Build",
            ));
        }
        if !crate::unit_ai::task_finder::enabled(&owner.attributes, task.enablement) {
            return Err(GenieError::unsupported(
                "a builder whose Build task is not enabled",
            ));
        }
        let builder = masters.get(with).ok_or(NO_WORK_MASTER)?;
        let to = WalkTo {
            id: Some(foundation),
            at: position,
            gap: footprint_gap(unit.position(), builder.radius, position, site.radius),
            state: self.object_state(foundation),
        };
        let task = BuildTask::of(task, builder.work_rate + unit.work_rate_powerup);
        // the sprites its MoveTo's constructor takes for none (FUN_140aa0df0)
        let move_reads = MoveToReads::of(unit, builder, &self.graphics);
        Ok((with, BuildAction::start(to, task, &move_reads)?))
    }

    /// The queue path of a builder at `idx` of player `q` (slot 0x30 into slot 0x28): its
    /// ignore list empties (`FUN_141197da0`) and its unit AI's queue takes the build order on
    /// `foundation` (of `master`, player `fp`'s) at `position`, issued by `q`, priority 100, the
    /// point shown, ready, as the game's saves hold it ([`World::build_order`] runs it).
    ///
    /// [`World::build_order`]: crate::world::World::build_order
    pub(crate) fn queue_build(
        &mut self,
        q: PlayerId,
        idx: ListIndex,
        (fp, foundation, master): (PlayerId, ObjectId, MasterId),
        position: Position,
    ) -> Result<(), GenieError> {
        let unit = self.players.live_mut(q, idx)?;
        if let Some(m) = unit.body.mover_mut() {
            m.ignore_units.clear();
        }
        let state = unit.life.state;
        let order = crate::unit_ai::QueuedOrder {
            issuer: Some(crate::unit_ai::Issuer::Player(q)),
            order: OrderKind::Build,
            priority: crate::unit_ai::OrderPriority::COMMAND,
            target: Some(foundation),
            target_owner: Some(fp),
            point: Position::new(position.x(), position.y(), 0.0),
            range: Distance::ZERO,
            shown: position.ground(),
            ready: true,
            target_type: Some(crate::unit_ai::OrderTargetType::of_master(master)),
        };
        let ai = unit.ai.as_mut().ok_or(GenieError::unsupported(
            "a queued build order without a unit AI",
        ))?;
        let _ = ai.queue_order(state, order, false, false)?;
        Ok(())
    }

    /// The direct path of a builder at `idx` of player `q`, slot 0x128 on `foundation` (player
    /// `fp`'s) at `position` with what [`World::build_task`] found: its ignore list empties
    /// (`FUN_141197da0`), the town-bell record (slot 0x7e8 with task filter −1), then the work
    /// command (`FUN_14106ff70`): the master changes, the Build takes its foundation, the unit
    /// AI's notify, the action list clears and holds the Build.
    pub(crate) fn build_directly(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        (with, build): (MasterId, BuildAction),
        (fp, foundation, master): (PlayerId, ObjectId, MasterId),
        position: Position,
    ) -> Result<(), GenieError> {
        let site_class = self.master(fp, master)?.class;
        if let Some(m) = self.players.live_mut(player, idx)?.body.mover_mut() {
            m.ignore_units.clear();
        }
        // slot 0x128 -> slot 0x7e8 (task filter -1): the villager's town-bell record, its
        // flag cleared (FUN_1412067f0)
        self.town_bell_order(player, idx, foundation, None)?;
        // the work command (FUN_14106ff70): the master changes (slot 0x90, its action list
        // cleared); the Build action takes its foundation (FUN_140a7a230, slot 0x178)
        if self.players.live(player, idx)?.life.master_id != with {
            self.change_master(player, idx, with)?;
        }
        self.take(foundation, self.releaser(player, idx)?)?;
        // the work command's notify (slot 0x828): the build order and the foundation, with
        // the point slot 0x128 passes (BUILD's x and y, on the ground)
        let unit = self.players.live_mut(player, idx)?;
        if let Some(ai) = unit.ai.as_mut() {
            ai.notify_work(
                Some(OrderKind::Build),
                Some(ActionKind::Build),
                Some((foundation, site_class)),
                Position::new(position.x(), position.y(), 0.0),
                true,
                self.streams.get(Stream::Ai),
            )?;
            self.remember_order(
                player,
                idx,
                (Some(OrderKind::Build), Some(ActionKind::Build)),
            )?;
        }
        // the install (slot 0x808): the action list clears (its actions' teardowns), then
        // holds the Build
        self.clear_actions(player, idx)?;
        self.players
            .live_mut(player, idx)?
            .actions
            .set_build(Some(build));
        Ok(())
    }

    /// The unit's slot 0x388 (`FUN_141226fc0`) after its unit AI's update, with what it found
    /// there ([`Wait`]): slot 0x820's switch to the master its swap group gave (slot 0x90 with 0,
    /// [`World::change_master`]; a villager's candidates are of its own type, never a
    /// building's), the action list cleared (`FUN_140a98620`, [`World::clear_actions`]; a wait
    /// it held goes too, its teardown the base one's) and the new wait in it (slot 0x808,
    /// `FUN_14106e990`: the wait's slot 0x70 with no action and its start both do nothing).
    pub(crate) fn wait_for_foundation(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        wait: Wait,
    ) -> Result<(), GenieError> {
        if let Some(master) = wait.switch {
            self.change_master(player, idx, master)?;
        }
        self.clear_actions(player, idx)?;
        self.players
            .live_mut(player, idx)?
            .actions
            .set_wait_solidify(Some(crate::construction::wait_solidify::WaitSolidify::new(
                wait.task,
                wait.target,
            )));
        Ok(())
    }

    /// The unit's build command (slot 0x380, `FUN_14120d410`, forced) on `foundation`: nothing
    /// for a foundation the world does not hold (`FUN_1415417a0`); a Build on top takes it (its
    /// slot 0x48, [`World::build_take`]); without one its slot 0x820 with any action type and
    /// the switch ([`task_search`] on the foundation and its main building; the switch, slot
    /// 0x90 with 0, [`World::change_master`]) finds a task, and a Build action on it
    /// (`FUN_140aad4e0` with that task, whatever its type) replaces the unit's actions. True
    /// when it builds (slot 0x380's 1); false for the foundation gone or no task (its 0, which
    /// the Gather's `set_state(6)` ignores and `BuildObject` reads first,
    /// [`crate::unit_ai::UnitSide::builds`]).
    pub(crate) fn build_command(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        foundation: ObjectId,
    ) -> Result<bool, GenieError> {
        if self.object(foundation).is_none() {
            return Ok(false);
        }
        if self.players.live(player, idx)?.top_action_kind() == Some(action_type::BUILD) {
            let _ = self.build_take(player, idx, foundation)?;
            return Ok(true);
        }
        let gone = FOUNDATION_GONE;
        let (fp, f) = self.object(foundation).ok_or_else(|| gone.clone())?;
        let (site, f_at) = (f.master(&self.masters, fp)?, f.position());
        let find = |id: ObjectId| self.object(id);
        let t =
            task_target_with(foundation, 1, &find, &self.masters)?.ok_or_else(|| gone.clone())?;
        let general =
            task_target_with(foundation, 2, &find, &self.masters)?.ok_or_else(|| gone.clone())?;
        let masters = self
            .masters
            .get(player.index())
            .ok_or(GenieError::unsupported("a build order: no master table"))?;
        let world = TaskWorld {
            players: &self.players,
            drop_sites: &self.drop_sites,
        };
        let owner = world.players.player(player)?;
        let unit = &owner.objects[idx];
        let actor = Actor {
            id: unit.id,
            master: unit.master_in(Some(masters))?,
            player,
            held: unit.life.held(),
            attribute_95: owner.attributes.get(95).copied().unwrap_or(Amount::ZERO),
            civ: owner.civ,
        };
        // slot 0x820 with any action type (0xffff): whatever task it finds, the Build takes
        // (FUN_140aad4e0 on it), its type and its enablement unread
        // slot 0x820's point: the foundation's place
        let terrain = point_terrain(self.spatial.terrain.as_ref(), f_at);
        let Some((task, switch)) =
            task_search(masters, &actor, &t, &general, terrain, None, &world)?
        else {
            return Ok(false);
        };
        let builder = switch.unwrap_or(actor.master);
        let to = WalkTo {
            id: Some(foundation),
            at: f_at,
            gap: footprint_gap(unit.position(), builder.radius, f_at, site.radius),
            state: Some(f.life.state),
        };
        let move_reads = MoveToReads::of(unit, builder, &self.graphics);
        let build = BuildAction::start(
            to,
            BuildTask::of(&task, builder.work_rate + unit.work_rate_powerup),
            &move_reads,
        )?;
        if let Some(m) = switch.map(|m| m.id) {
            self.change_master(player, idx, m)?;
        }
        self.take(foundation, self.releaser(player, idx)?)?;
        self.clear_actions(player, idx)?;
        self.players
            .live_mut(player, idx)?
            .actions
            .set_build(Some(build));
        Ok(true)
    }
}
