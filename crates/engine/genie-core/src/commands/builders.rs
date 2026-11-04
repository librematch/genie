//! BUILD's builders: their task, the queue and the direct path, the wait for the foundation
//! and the build command.

use crate::GenieError;
use crate::commands::FOUNDATION_GONE;
use crate::construction::FoundationId;
use crate::construction::build::{BuildAction, BuildTask};
use crate::construction::wait_solidify::WaitSolidify;
use crate::map::Distance;
use crate::map::Position;
use crate::math::fmath::below_or_unordered;
use crate::math::rng::Stream;
use crate::movement::Speed;
use crate::movement::footprint_gap;
use crate::movement::move_to::{MoveToReads, WalkTo};
use crate::objects::HitPoints;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::actions::Action;
use crate::objects::lifecycle::Held;
use crate::objects::masters::ActionKind;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::player_master;
use crate::objects::masters::work_master;
use crate::objects::masters::{MasterTables, OwnerMasters};
use crate::orders::Dispatch;
use crate::orders::OrderSource;
use crate::orders::ReadyOrders;
use crate::orders::{Work, readiness};
use crate::players::Amount;
use crate::players::PlayerId;
use crate::unit_ai::AiAction;
use crate::unit_ai::Answer;
use crate::unit_ai::Notifies;
use crate::unit_ai::OrderKind;
use crate::unit_ai::QueueEnd;
use crate::unit_ai::task_finder::world::{TaskWorld, point_terrain, task_search, task_target_with};
use crate::unit_ai::task_finder::{Actor, TaskTarget};
use crate::unit_ai::task_finder::{TaskOn, enabled};
use crate::unit_ai::world_view::Wait;
use crate::unit_ai::{Issuer, OrderPriority, OrderTargetKind, QueuedOrder};
use crate::world::index::Place;
use crate::world::slots::PlayerSlots;
use crate::world::{STATIC_KINDS_END, World};

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
        foundation: FoundationId,
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
            let m = unit.master(self.master_tables(), q)?;
            let speed = m.move_stats.map_or(Speed::ZERO, |s| s.speed);
            let kind = m.kind;
            let moves = MasterKind::MOVING <= kind && below_or_unordered(0.0, speed.get());
            if !moves && !queued {
                continue;
            }
            let to_queue = self
                .parts
                .unit_ai
                .get(unit.id)
                .is_some_and(|ai| queued || (commits && ai.has_unready_order()));
            let Some(Place::Live(q, idx)) = self.place(id) else {
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
        (site, position): ((PlayerId, FoundationId, MasterId), Position),
        (to_queue, commits): (bool, bool),
    ) -> Result<(), GenieError> {
        let (player, foundation, _) = site;
        if to_queue {
            self.queue_builder((q, idx), site, position, commits)?;
        } else if STATIC_KINDS_END <= kind && q == player {
            let task = self.build_task(q, id, site, position)?;
            self.empty_queue_for_work(q, idx, kind)?;
            self.build_directly(q, idx, task, site, position)?;
        } else if STATIC_KINDS_END <= kind {
            self.empty_queue_for_work(q, idx, kind)?;
            // another player's unit: slot 0x128's slot 0x7e8 finds whatever task it has on
            // the foundation (a Build on an ally's), the work command's general path
            let work = Work {
                target: Some(foundation.id()),
                point: Position::new(position.x(), position.y(), 0.0),
                carry_test: false,
                filter: None,
                source: OrderSource::Direct,
            };
            self.order_work(q, idx, &work)?;
        } else if let Some(m) = self.parts.mover_mut(self.players.live(q, idx)?.id) {
            m.ignore_units.clear();
        }
        Ok(())
    }

    /// A builder that queues: its Build joins the queue, and under `commits` the readiness pass
    /// with 0 makes every queued order ready.
    fn queue_builder(
        &mut self,
        (q, idx): (PlayerId, ListIndex),
        site: (PlayerId, FoundationId, MasterId),
        position: Position,
        commits: bool,
    ) -> Result<(), GenieError> {
        self.queue_build(q, idx, site, position)?;
        if commits && let Some(ai) = self.parts.unit_ai.get_mut(self.players.live(q, idx)?.id) {
            readiness(&mut ai.orders, ReadyOrders::Keep);
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
        if MasterKind::COMBAT <= kind
            && let Some(ai) = self.parts.unit_ai.get_mut(self.players.live(q, idx)?.id)
        {
            ai.orders.clear();
        }
        Ok(())
    }

    /// What slot 0x128 on `foundation` (of `master`, player `fp`'s, at `position`) takes for the
    /// builder `id` of player `q`: the master it builds with ([`work_master`], with
    /// `q`'s masters and attributes) and the Build action of that master's task. A builder
    /// whose action would be replaced, without a Build task for it or with one not enabled, is
    /// not modelled.
    pub(crate) fn build_task(
        &self,
        q: PlayerId,
        id: ObjectId,
        (fp, foundation, master): (PlayerId, FoundationId, MasterId),
        position: Position,
    ) -> Result<(MasterId, BuildAction), GenieError> {
        let masters = self.model.masters.of(q).ok_or(GenieError::unsupported(
            "BUILD tasks the builders: no master table",
        ))?;
        let site = player_master(&self.model.masters, fp, master).ok_or(
            GenieError::unsupported("BUILD of a master the player does not have"),
        )?;
        let target = TaskTarget {
            id: foundation.id(),
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
            drop_sites: &self.model.drop_sites,
        };
        let owner = world.players.player(q)?;
        let unit = self
            .live_object(q, id)
            .ok_or(GenieError::unsupported("a builder the slice does not have"))?;
        if self.parts.actions_of(unit.id).list.artifact().is_some()
            || self.parts.production.contains(id)
            || self.parts.actions_of(unit.id).list.fly().is_some()
        {
            return Err(GenieError::unsupported(
                "a builder with an action (its replacement)",
            ));
        }
        let actor = Actor {
            id,
            master: unit.master_in(OwnerMasters::new(&self.parts.own_master, Some(masters)))?,
            player: q,
            held: unit.life.held(),
            attribute_95: owner.attributes.get(95).copied().unwrap_or(Amount::ZERO),
            civ: owner.civ,
        };
        // the work command's point: BUILD's x and y on the ground
        let on = TaskOn {
            target: Some(&target),
            general: Some(&target),
            terrain: self.task_terrain(Position::new(position.x(), position.y(), 0.0)),
        };
        let (builder, task) = work_master(
            masters,
            &actor,
            &on,
            None,
            &world,
            self.model.slots,
            &self.model.roles,
        )?
        .ok_or(GenieError::unsupported(
            "a builder with no task for its foundation",
        ))?;
        if task.action_kind != ActionKind::BUILD {
            return Err(GenieError::unsupported(
                "a builder whose task for its foundation is not Build",
            ));
        }
        if !enabled(&owner.attributes, task.enablement) {
            return Err(GenieError::unsupported(
                "a builder whose Build task is not enabled",
            ));
        }
        let to = WalkTo {
            id: Some(foundation.id()),
            at: position,
            gap: footprint_gap(unit.position(), builder.radius, position, site.radius),
            state: self.object_state(foundation),
        };
        let task = BuildTask::of(
            task,
            builder.action_part().work_rate + self.parts.combat_of(unit.id).work_rate_powerup,
        );
        // the sprites its MoveTo's constructor takes for none (FUN_140aa0df0)
        let move_reads = MoveToReads::of(
            (unit, self.parts.mover_of(unit.id)),
            builder,
            (
                &self.model.graphics,
                self.parts.combat_of(unit.id).abilities.look.as_ref(),
            ),
            &self.model.roles,
        );
        Ok((
            builder.id,
            BuildAction::start(to, task, &move_reads, &self.model.tuning.move_to)?,
        ))
    }

    /// The queue path of a builder at `idx` of player `q` (slot 0x30 into slot 0x28): its
    /// ignore list empties (`FUN_141197da0`) and its unit AI's queue takes the build order on
    /// `foundation` (of `master`, player `fp`'s) at `position`, issued by `q`, priority 100, the
    /// point shown, ready, as the game's saves hold it (its build order runs it).
    pub(crate) fn queue_build(
        &mut self,
        q: PlayerId,
        idx: ListIndex,
        (fp, foundation, master): (PlayerId, FoundationId, MasterId),
        position: Position,
    ) -> Result<(), GenieError> {
        let unit = self.players.live_mut(q, idx)?;
        if let Some(m) = self.parts.mover_mut(unit.id) {
            m.ignore_units.clear();
        }
        let state = unit.life.state;
        let order = QueuedOrder {
            issuer: Some(Issuer::Player(q)),
            order: OrderKind::Build,
            priority: OrderPriority::COMMAND,
            target: Some(foundation.id()),
            target_owner: Some(fp),
            point: Position::new(position.x(), position.y(), 0.0),
            range: Distance::ZERO,
            shown: position.ground(),
            ready: true,
            target_kind: Some(OrderTargetKind::of_master(master)),
        };
        let ai = self
            .parts
            .unit_ai
            .get_mut(unit.id)
            .ok_or(GenieError::unsupported(
                "a queued build order without a unit AI",
            ))?;
        let _ = ai.queue_order(state, order, Dispatch::Queued, QueueEnd::Back)?;
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
        (fp, foundation, master): (PlayerId, FoundationId, MasterId),
        position: Position,
    ) -> Result<(), GenieError> {
        let site_class = self.master(fp, master)?.class;
        if let Some(m) = self.parts.mover_mut(self.players.live(player, idx)?.id) {
            m.ignore_units.clear();
        }
        // slot 0x128 -> slot 0x7e8 (task filter -1): the villager's town-bell record, its
        // flag cleared (FUN_1412067f0)
        self.town_bell_order(player, idx, foundation.id(), None)?;
        // the work command (FUN_14106ff70): the master changes (slot 0x90, its action list
        // cleared); the Build action takes its foundation (FUN_140a7a230, slot 0x178)
        if self.players.live(player, idx)?.life.master_id != with {
            self.change_master(player, idx, with)?;
        }
        self.take(foundation.id(), self.releaser(player, idx)?)?;
        // the work command's notify (slot 0x828): the build order and the foundation, with
        // the point slot 0x128 passes (BUILD's x and y, on the ground)
        let unit = self.players.live_mut(player, idx)?;
        if let Some(ai) = self.parts.unit_ai.get_mut(unit.id) {
            ai.notify_work(
                Some(OrderKind::Build),
                Some(AiAction::Build),
                Some((foundation.id(), site_class)),
                Position::new(position.x(), position.y(), 0.0),
                Notifies::Cleared,
                self.streams.get(Stream::Ai),
            )?;
            self.remember_order(player, idx, (Some(OrderKind::Build), Some(AiAction::Build)))?;
        }
        // the install (slot 0x808): the action list clears (its actions' teardowns), then
        // holds the Build
        self.clear_actions(player, idx)?;
        self.parts
            .actions_mut(self.players.live(player, idx)?.id)?
            .list
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
        self.parts
            .actions_mut(self.players.live(player, idx)?.id)?
            .list
            .set_wait_solidify(Some(WaitSolidify::new(wait.task, wait.target)));
        Ok(())
    }

    /// The unit's build command (slot 0x380, `FUN_14120d410`, forced) on `foundation`: nothing for
    /// a foundation the world does not hold (`FUN_1415417a0`); a Build on top takes it (its slot
    /// 0x48, [`World::build_take`]); without one its slot 0x820 with any action type and the switch
    /// ([`task_search`] on the foundation and its main building; the switch, slot 0x90 with 0,
    /// [`World::change_master`]) finds a task, and a Build action on it (`FUN_140aad4e0` with that
    /// task, whatever its type) replaces the unit's actions. [`Answer::Taken`] when it builds (slot
    /// 0x380's 1); [`Answer::NotTaken`] for the foundation gone or no task (its 0, which the
    /// Gather's `set_state(6)` ignores and `BuildObject` reads first,
    /// [`crate::unit_ai::world_view::UnitView::builds`]).
    pub(crate) fn build_command(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        foundation: FoundationId,
    ) -> Result<Answer, GenieError> {
        if self.object(foundation).is_none() {
            return Ok(Answer::NotTaken);
        }
        if let Some(Action::Build(build)) = self
            .parts
            .actions_of(self.players.live(player, idx)?.id)
            .list
            .head()
        {
            let _ = self.build_take((player, idx), *build, foundation.id())?;
            return Ok(Answer::Taken);
        }
        let gone = FOUNDATION_GONE;
        let (fp, f) = self.object(foundation).ok_or_else(|| gone.clone())?;
        let (site, foundation_at) = (f.master(self.master_tables(), fp)?, f.position());
        let find = |id: ObjectId| self.object(id);
        let t = task_target_with(
            foundation.id(),
            1,
            &find,
            &self.parts,
            MasterTables::new(&self.model.masters, &self.parts.own_master),
        )?
        .ok_or_else(|| gone.clone())?;
        let general = task_target_with(
            foundation.id(),
            2,
            &find,
            &self.parts,
            MasterTables::new(&self.model.masters, &self.parts.own_master),
        )?
        .ok_or_else(|| gone.clone())?;
        let masters = self
            .model
            .masters
            .get(player.index())
            .ok_or(GenieError::unsupported("a build order: no master table"))?;
        let world = TaskWorld {
            players: &self.players,
            drop_sites: &self.model.drop_sites,
        };
        let owner = world.players.player(player)?;
        let unit = owner.objects.checked(idx)?;
        let actor = Actor {
            id: unit.id,
            master: unit.master_in(OwnerMasters::new(&self.parts.own_master, Some(masters)))?,
            player,
            held: unit.life.held(),
            attribute_95: owner.attributes.get(95).copied().unwrap_or(Amount::ZERO),
            civ: owner.civ,
        };
        // slot 0x820 with any action type (0xffff): whatever task it finds, the Build takes
        // (FUN_140aad4e0 on it), its type and its enablement unread
        // slot 0x820's point: the foundation's place
        let terrain = point_terrain(self.spatial.terrain.as_ref(), foundation_at);
        let Some((task, switch)) = task_search(
            masters,
            &actor,
            &TaskOn {
                target: Some(&t),
                general: Some(&general),
                terrain,
            },
            None,
            &world,
            self.model.slots,
            &self.model.roles,
        )?
        else {
            return Ok(Answer::NotTaken);
        };
        let builder = switch.unwrap_or(actor.master);
        let to = WalkTo {
            id: Some(foundation.id()),
            at: foundation_at,
            gap: footprint_gap(unit.position(), builder.radius, foundation_at, site.radius),
            state: Some(f.life.state),
        };
        let move_reads = MoveToReads::of(
            (unit, self.parts.mover_of(unit.id)),
            builder,
            (
                &self.model.graphics,
                self.parts.combat_of(unit.id).abilities.look.as_ref(),
            ),
            &self.model.roles,
        );
        let build = BuildAction::start(
            to,
            BuildTask::of(
                &task,
                builder.action_part().work_rate + self.parts.combat_of(unit.id).work_rate_powerup,
            ),
            &move_reads,
            &self.model.tuning.move_to,
        )?;
        if let Some(m) = switch.map(|m| m.id) {
            self.change_master(player, idx, m)?;
        }
        self.take(foundation.id(), self.releaser(player, idx)?)?;
        self.clear_actions(player, idx)?;
        self.parts
            .actions
            .of_mut(self.players.live(player, idx)?.id)?
            .list
            .set_build(Some(build));
        Ok(Answer::Taken)
    }
}
