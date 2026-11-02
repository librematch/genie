//! The enter command and the Enter's construction, install and start, at once or with the
//! transport's move.

use crate::GenieError;
use crate::construction::MainBuildingId;
use crate::garrison::ContainerId;
use crate::garrison::EnterState;
use crate::garrison::meeting::Meeting;
use crate::garrison::{
    ENTER, ENTER_SLEEPING, EnterAction, EnterTarget, UNPLACED, UNREACHED, WalkStart, Walking,
    enter_view, ship_reaches,
};
use crate::map::Distance;
use crate::map::Point;
use crate::map::Position;
use crate::movement::move_to::MoveToReads;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::masters::ClassId;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::Task;
use crate::objects::masters::master_type::BUILDING;
use crate::orders::GARRISON;
use crate::orders::object_gone;
use crate::players::PlayerId;
use crate::unit_ai::ActionKind;
use crate::unit_ai::OrderKind;
use crate::world::World;
use crate::world::slots::PlayerSlots;

impl World {
    /// The work command's Enter (its action type 3, [`World::new_enter`]), the unit AI hearing of
    /// it (slot 0x828: the enter order and action, the target with its class and the work's
    /// point), then its install ([`World::install_enter`]).
    pub(crate) fn start_enter(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
        task: &Task,
        point: Position,
        clears: bool,
    ) -> Result<(), GenieError> {
        let (action, target, class) = self.new_enter(player, idx, target, task)?;
        if let Some(ai) = self
            .parts
            .unit_ai
            .get_mut(self.players.live(player, idx)?.id)
        {
            ai.notify_work(
                Some(OrderKind::Enter),
                Some(ActionKind::Enter),
                Some((target.id(), class)),
                point,
                clears,
                self.streams.get(crate::math::rng::Stream::Ai),
            )?;
            self.remember_order(
                player,
                idx,
                (Some(OrderKind::Enter), Some(ActionKind::Enter)),
            )?;
        }
        let failed = self.install_enter(player, idx, target, action, task)?;
        self.post_own_failure(player, idx, failed)
    }

    /// Slot 0x3a0 (`FUN_1410847e0`, the enter command) of the unit at `idx` of `player` on
    /// `target`, `forced` its third argument: a target the world does not hold is false;
    /// unforced, a unit entering the target already (its top action an Enter on it) is true.
    /// Slot 0x820 ([`World::find_work_task`]) looks for a task on the target with the garrison
    /// filter (3) and the switch (the unit takes the master that gave it, slot 0x90): none is
    /// false; else its Enter ([`World::enter_with`]): true.
    ///
    /// # Errors
    ///
    /// What the Enter does not model.
    pub(crate) fn enter_command(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
        forced: bool,
    ) -> Result<bool, GenieError> {
        if self.object(target).is_none() {
            return Ok(false);
        }
        let unit = self.players.live(player, idx)?;
        let top = self.parts.actions_of(unit.id).top_action_kind();
        if !forced
            && top == Some(ENTER)
            && self.parts.actions_of(unit.id).top_target() == Some(target)
        {
            return Ok(true);
        }
        let Some(task) = self.find_work_task(player, idx, target, Some(GARRISON), true)? else {
            return Ok(false);
        };
        let under_artifact = top == Some(crate::ownership::artifact::ARTIFACT_TYPE);
        let failed = self.enter_with(player, idx, target, &task, under_artifact)?;
        self.post_own_failure(player, idx, failed)?;
        Ok(true)
    }

    /// The notify an Enter's install gave the unit's own unit AI ([`World::install_enter_as`]),
    /// posted to it (slot 0x328, with the unit's state).
    fn post_own_failure(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        failed: Option<crate::unit_ai::Notify>,
    ) -> Result<(), GenieError> {
        let unit = self.players.live_mut(player, idx)?;
        let state = unit.life.state;
        match (failed, self.parts.unit_ai.get_mut(unit.id)) {
            (Some(n), Some(ai)) => ai.post(n, state),
            _ => Ok(()),
        }
    }

    /// The unit's enter command its unit AI gave ([`crate::unit_ai::world_view::EnterOrder`]),
    /// while the AI runs: the master switch (slot 0x90 with 0, [`World::change_master`]), then
    /// the Enter ([`World::enter_with`]). Returns the notify the Enter's install gives the unit's
    /// own AI, which its side holds until it returns.
    ///
    /// # Errors
    ///
    /// What the master change and the Enter do not model.
    pub(crate) fn enter_order(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        order: crate::unit_ai::world_view::EnterOrder,
    ) -> Result<Option<crate::unit_ai::Notify>, GenieError> {
        if let Some(master) = order.switch {
            self.change_master(player, idx, master)?;
        }
        self.enter_with(player, idx, order.target, &order.task, order.under_artifact)
    }

    /// The enter command's Enter on `task` (`FUN_140a855d0`, [`World::new_enter`]): under an
    /// Artifact on top (`under_artifact`, read before the task search) it would become the
    /// Artifact's sub-action, which is not modelled; else the action list empties and the Enter
    /// is installed (slot 0x808, [`World::install_enter`]). Returns the notify the install gives
    /// the unit's own AI ([`World::install_enter_as`]).
    ///
    /// # Errors
    ///
    /// What the Enter does not model.
    pub(crate) fn enter_with(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
        task: &Task,
        under_artifact: bool,
    ) -> Result<Option<crate::unit_ai::Notify>, GenieError> {
        let (action, target, _) = self.new_enter(player, idx, target, task)?;
        self.install_enter_as(player, idx, target, action, task, under_artifact)
    }

    /// The Enter's constructor (`FUN_140a855d0`) on `target` with `task`: its target, an annex
    /// its main building; the point [`meeting`] gives at the target's height
    /// ([`World::enter_meeting`]); it takes the target (slot 0x178). With the target and its
    /// class.
    ///
    /// [`meeting`]: crate::garrison::meeting::meeting_point
    pub(super) fn new_enter(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
        task: &Task,
    ) -> Result<(EnterAction, ContainerId, ClassId), GenieError> {
        let (tp, t) = self
            .object(target)
            .ok_or(GenieError::unsupported("an Enter whose building is gone"))?;
        let target = ContainerId::new(if t.master(self.master_tables(), tp)?.kind == BUILDING {
            self.parts
                .building_of(t.id)
                .annex_of
                .map_or(target, MainBuildingId::id)
        } else {
            target
        });
        let (tp, t) = self.object(target).ok_or_else(|| object_gone(target))?;
        let tm = t.master(self.master_tables(), tp)?;
        let (class, height) = (tm.class, t.position().z());
        let (at, _) = self.enter_meeting(player, idx, target)?;
        let action = EnterAction::new(target, at.with_height(height), task);
        self.take(target.id(), self.releaser(player, idx)?)?;
        Ok((action, target, class))
    }

    /// The Enter's install (slot 0x808): the action list empties and the Enter starts (slot 5,
    /// `FUN_140a86f20`): a unit inside an object fails it (`set_state(13)`, its notify for the
    /// unit's own AI the answer, [`World::install_enter_as`]); a target without room
    /// (`FUN_140a864d0`, `set_state(11)`) is not modelled; else its `set_state(4)`
    /// ([`EnterAction::walk_again`]): a unit in reach enters at once ([`World::enter_now`]),
    /// else it walks, a transport to the point first ([`World::move_transport`]).
    pub(super) fn install_enter(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ContainerId,
        action: EnterAction,
        task: &Task,
    ) -> Result<Option<crate::unit_ai::Notify>, GenieError> {
        self.install_enter_as(player, idx, target, action, task, false)
    }

    /// [`World::install_enter`], or with `sub` the Enter as the sub-action of the Artifact on top
    /// (`FUN_1410847e0`): the Artifact's sub-action goes (`FUN_140a98e20`,
    /// [`crate::ownership::artifact::Artifact::hold_enter`]: an Enter lets its target go, slot 0x180
    /// ([`World::release`]), and a walking `MoveTo`'s teardown stops the unit) and the Enter
    /// starts in its list (`FUN_140a977e0`), the Artifact's update running it from then on.
    /// Returns the notify a unit inside an object posts its own unit AI (0x1f9 with the Enter's
    /// action value), which the caller posts: the unit AI that gave the enter command holds it
    /// out of the unit while it runs.
    pub(super) fn install_enter_as(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ContainerId,
        mut action: EnterAction,
        task: &Task,
        sub: bool,
    ) -> Result<Option<crate::unit_ai::Notify>, GenieError> {
        let id = self.players.live(player, idx)?.id;
        if sub {
            self.enter_under_artifact(player, idx, id, action)?;
        } else {
            self.clear_actions(player, idx)?;
        }
        if self.players.live(player, idx)?.life.inside.is_some() {
            return self.enter_from_inside(player, idx, action, sub).map(Some);
        }
        let (tp, t) = self.object(target).ok_or_else(|| object_gone(target))?;
        if !self.has_room(tp, t, Some(id))? {
            return Err(GenieError::unsupported(
                "an Enter into a building without room (state 11)",
            ));
        }
        let (t, start) = self.entering(player, idx, target, task.work_range)?;
        let walking = self.enter_walk_again(player, idx, &mut action, &t, start)?;
        match walking {
            // the enter's set_state(1) reads the Enter as the unit's; a sub-action goes back
            Walking::Enter => {
                self.parts
                    .actions
                    .of_mut(self.players.live(player, idx)?.id)?
                    .list
                    .set_enter(Some(action));
                let entered = self.enter_now(player, idx, target);
                if sub {
                    let actions = self.parts.actions.of_mut(id)?;
                    let e = actions.list.take_enter();
                    if let (Some(e), Some(art)) = (e, actions.list.artifact_mut()) {
                        art.sub = Some(crate::ownership::artifact::ArtifactSub::Enter { enter: e });
                    }
                }
                entered.map(|()| None)
            }
            Walking::Walk(to) => {
                self.parts.actions.of_mut(id)?.place_enter(action, sub);
                to.map_or(Ok(()), |to| self.move_transport(Some(target), to))
                    .map(|()| None)
            }
        }
    }

    /// [`World::install_enter_as`] under the Artifact on top of unit `idx` of `player` (its id
    /// `id`): the Artifact's sub-action goes for the Enter `action`
    /// ([`crate::ownership::artifact::Artifact::hold_enter`]): an old Enter lets its target go
    /// (slot 0x180, [`World::release`]), then a walking `MoveTo`'s teardown stops the unit.
    fn enter_under_artifact(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        id: ObjectId,
        action: EnterAction,
    ) -> Result<(), GenieError> {
        let by = self.releaser(player, idx)?;
        let art =
            self.parts
                .actions
                .of_mut(id)?
                .list
                .artifact_mut()
                .ok_or(GenieError::unreachable(
                    "an Artifact on top without its action",
                ))?;
        let gone = art.hold_enter(action);
        // the old Enter's teardown lets its target go before its MoveTo's stops the unit
        if let Some(t) = gone.released {
            self.release(t, by)?;
        }
        let unit = self.players.live(player, idx)?;
        if gone.stops
            && let Some(m) = self.parts.mover_mut(unit.id)
        {
            m.stop()?;
        }
        Ok(())
    }

    /// [`World::install_enter_as`] for unit `idx` of `player` inside an object: the Enter
    /// `action` fails at once (`set_state(13)`): the unit takes its master's standing sprite
    /// (slot 0x68 with `+0x470`) and, its `+0x60` 0 in a new Enter, posts notify 0x1f9 with the
    /// enter's action value to its unit AI, which this returns; the update returns 3 then.
    fn enter_from_inside(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        mut action: EnterAction,
        sub: bool,
    ) -> Result<crate::unit_ai::Notify, GenieError> {
        action.state = EnterState::Failed;
        let masters = crate::objects::masters::OwnerMasters::new(
            &self.parts.own_master,
            self.masters.of(player),
        );
        let unit = self.players.live_mut(player, idx)?;
        self.parts.actions.of_mut(unit.id)?.place_enter(action, sub);
        unit.shows_standing_sprite(
            self.parts.motion.layer_mut(unit.id),
            masters,
            &self.graphics,
            self.streams.get(crate::math::rng::Stream::Sprite),
        )?;
        Ok(crate::unit_ai::Notify::own(
            unit.id,
            crate::unit_ai::NotifyKind::ACTION_FAILED,
            [ActionKind::Enter.value(), 0, 0],
        ))
    }

    /// What the Enter's start reads of unit `idx` of `player` and its target ([`enter_view`],
    /// room by slot 0x198).
    ///
    /// # Errors
    ///
    /// A target the world does not place, and [`enter_view`]'s.
    pub(super) fn entering(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ContainerId,
        range: Distance,
    ) -> Result<(EnterTarget, WalkStart), GenieError> {
        let by_id = crate::world::index::ObjectsRef::new(
            &self.players,
            &self.index,
            &self.masters,
            &self.parts,
        );
        let id = self.players.live(player, idx)?.id;
        let (tp, t) = self.object(target).ok_or(UNPLACED)?;
        let tm = t.master(self.master_tables(), tp)?.clone();
        let room = self.has_room(tp, t, Some(id))?;
        let t = placed_object(&self.players, self.place(target))?;
        let unit = self.players.live(player, idx)?;
        enter_view(
            &mut self.spatial,
            &self.masters,
            (unit, self.parts.mover_of(unit.id)),
            ((t, self.parts.mover_of(t.id)), &tm, room),
            range,
            by_id,
        )
    }

    /// The start's `set_state(4)` ([`EnterAction::walk_again`]) of unit `idx` of `player`, with
    /// what [`meeting`] reads of the world ([`Meeting`]).
    ///
    /// # Errors
    ///
    /// The walk again's.
    ///
    /// [`meeting`]: crate::garrison::meeting::meeting_point
    pub(super) fn enter_walk_again(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        action: &mut EnterAction,
        t: &EnterTarget,
        start: WalkStart,
    ) -> Result<Walking, GenieError> {
        let by_id = crate::world::index::ObjectsRef::new(
            &self.players,
            &self.index,
            &self.masters,
            &self.parts,
        );
        let (tp, target) = self.object(action.target).ok_or(UNPLACED)?;
        let master = target.master(self.master_tables(), tp)?.clone();
        let target = placed_object(&self.players, self.place(action.target))?;
        let unit = self.players.live(player, idx)?;
        let reads = MoveToReads::of(
            (unit, self.parts.mover_of(unit.id)),
            unit.master(self.master_tables(), player)?,
            (
                &self.graphics,
                self.parts.combat_of(unit.id).abilities.look.as_ref(),
            ),
        );
        let meeting = Meeting {
            spatial: &mut self.spatial,
            zones: &mut self.zones,
            target,
            master: &master,
            room: t.room,
            unit,
            mover: self.parts.mover_of(unit.id),
            target_actions: self.parts.actions_of(target.id),
            target_unread: self.parts.unread.get(target.id),
            target_mover: self.parts.mover_of(target.id),
        };
        action.walk_again(t, start, meeting, &reads, by_id)
    }

    /// `FUN_140a85f30` ([`meeting`], [`Meeting::point`]) for unit `idx` of `player` and its
    /// target, as the constructor reads it: room by slot 0x198.
    ///
    /// # Errors
    ///
    /// A target the world does not place, and the meeting's.
    ///
    /// [`meeting`]: crate::garrison::meeting::meeting_point
    pub(super) fn enter_meeting(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ContainerId,
    ) -> Result<(Point, u8), GenieError> {
        let by_id = crate::world::index::ObjectsRef::new(
            &self.players,
            &self.index,
            &self.masters,
            &self.parts,
        );
        let id = self.players.live(player, idx)?.id;
        let (tp, t) = self.object(target).ok_or(UNPLACED)?;
        let master = t.master(self.master_tables(), tp)?.clone();
        let room = self.has_room(tp, t, Some(id))?;
        let target = placed_object(&self.players, self.place(target))?;
        let unit = self.players.live(player, idx)?;
        Meeting {
            spatial: &mut self.spatial,
            zones: &mut self.zones,
            target,
            master: &master,
            room,
            unit,
            mover: self.parts.mover_of(unit.id),
            target_actions: self.parts.actions_of(target.id),
            target_unread: self.parts.unread.get(target.id),
            target_mover: self.parts.mover_of(target.id),
        }
        .point(by_id)
    }

    /// The transport's part of the Enter's `set_state(4)` with code 1: its slot 0x3d8
    /// (`FUN_14114ffc0`) asks whether it reaches within 1 of the point's tile (the point
    /// truncated, [`crate::movement::Mover::reaches`]); then its slot 0x340 (`FUN_141216c50`:
    /// [`World::move_aside`], range 0, forced, running) moves it to the point at its height. One
    /// that cannot reach it is [`UNREACHED`].
    pub(crate) fn move_transport(
        &mut self,
        transport: Option<ContainerId>,
        to: Position,
    ) -> Result<(), GenieError> {
        let by_id = crate::world::index::ObjectsRef::new(
            &self.players,
            &self.index,
            &self.masters,
            &self.parts,
        );
        let Some(crate::world::index::Place::Live(tp, ti)) = self.place(transport) else {
            return Err(UNREACHED);
        };
        let ship = self.players.live(tp, ti)?;
        let mut plan = crate::movement::Planning {
            pathing: &mut self.pathing,
            now: self.world_time,
            computer: self.players.player(tp)?.kind.is_computer(),
            object: self.streams.get(crate::math::rng::Stream::Object),
            groups: &self.groups,
        };
        if !ship_reaches(
            (
                ship,
                self.parts.actions_of(ship.id),
                self.parts.mover_of(ship.id),
            ),
            &mut self.spatial,
            &mut plan,
            to,
            by_id,
        )? {
            return Err(UNREACHED);
        }
        self.ship_move(transport, to)
    }

    /// The transport's slot 0x340 (`FUN_141216c50`, [`World::move_aside`]: range 0, forced,
    /// running) to `to`, where it meets an entering unit.
    pub(crate) fn ship_move(
        &mut self,
        ship: Option<ContainerId>,
        to: Position,
    ) -> Result<(), GenieError> {
        let Some(crate::world::index::Place::Live(p, i)) = self.place(ship) else {
            return Err(UNREACHED);
        };
        let walk = crate::unit_ai::AiMove {
            point: to,
            range: Distance::ZERO,
            force: true,
            walk: false,
        };
        self.move_aside(p, i, walk)
    }

    /// The enter at the Enter's start ([`crate::objects::unit::Unit::enter_building`] into its
    /// target, any player's live object, then the start's own `set_state(1)`), on the unit's
    /// step ([`World::run_on_step`]).
    pub(super) fn enter_now(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ContainerId,
    ) -> Result<(), GenieError> {
        let inside = self
            .players
            .iter()
            .flat_map(|p| p.objects.iter().chain(&p.sleeping))
            .filter(|u| u.life.inside == Some(target))
            .count();
        let Some(crate::world::index::Place::Live(..)) = self.place(target) else {
            return Err(ENTER_SLEEPING);
        };
        self.run_on_step(player, idx, |unit, actions, c| {
            if !unit.enter_building(actions, c, target, inside)? {
                return Err(GenieError::unsupported(
                    "an Enter in reach that did not get in",
                ));
            }
            unit.entered(actions, c)
        })
    }

    /// The unit at `idx` of player `player` goes into `target` at once, its slot 0x1a0 forced
    /// (`FUN_141212e10` with 1: `FUN_1411921d0` passes over the room test, slot 0x198;
    /// [`crate::objects::unit::Unit::go_inside`] on the unit's step): no load given, no notify,
    /// no state change. The building's list takes it and its volleys its share.
    pub(crate) fn enter_forced(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ContainerId,
    ) -> Result<(), GenieError> {
        let Some(crate::world::index::Place::Live(tp, ti)) = self.place(target) else {
            return Err(ENTER_SLEEPING);
        };
        let me = self
            .players
            .live(player, idx)?
            .master(self.master_tables(), player)?
            .clone();
        let bm = self
            .players
            .live(tp, ti)?
            .master(self.master_tables(), tp)?
            .clone();
        self.run_on_step(player, idx, |unit, actions, c| {
            unit.go_inside(actions, c, target, &me, &bm)
        })
    }
}

/// The live or sleeping object at `place` among `players` ([`World::place`]).
///
/// # Errors
///
/// [`UNPLACED`] for an object the world does not place.
fn placed_object(
    players: &[crate::world::Player],
    place: Option<crate::world::index::Place>,
) -> Result<&crate::objects::unit::Unit, GenieError> {
    match place {
        Some(crate::world::index::Place::Live(p, i)) => players.live(p, i),
        Some(crate::world::index::Place::Sleeping(p, i)) => players.asleep(p, i),
        None => Err(UNPLACED),
    }
}
