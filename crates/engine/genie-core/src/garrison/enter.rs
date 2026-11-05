//! The enter command and the Enter's construction, install and start, at once or with the
//! transport's move.

use crate::construction::MainBuildingId;
use crate::garrison::ContainerId;
use crate::garrison::EnterOutcome;
use crate::garrison::EnterState;
use crate::garrison::meeting::Meeting;
use crate::garrison::{
    ENTER, ENTER_SLEEPING, EnterAction, EnterTarget, UNPLACED, UNREACHED, WalkStart, Walking,
    enter_view, ship_reaches,
};
use crate::map::Distance;
use crate::map::Point;
use crate::map::Position;
use crate::math::rng::Stream;
use crate::movement::Planning;
use crate::movement::move_to::MoveToReads;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::OwnerMasters;
use crate::objects::masters::PlayersMastersExt;
use crate::objects::masters::Task;
use crate::objects::unit::Unit;
use crate::orders::GARRISON;
use crate::orders::MasterSwap;
use crate::orders::object_gone;
use crate::ownership::artifact::{ARTIFACT_KIND, ArtifactSub};
use crate::players::PlayerId;
use crate::unit_ai::AiAction;
use crate::unit_ai::Answer;
use crate::unit_ai::Notifies;
use crate::unit_ai::OrderForce;
use crate::unit_ai::OrderKind;
use crate::unit_ai::Pace;
use crate::unit_ai::world_view::EnterOrder;
use crate::unit_ai::{AiMove, Notify, NotifyKind};
use crate::world::Player;
use crate::world::World;
use crate::world::index::Place;
use crate::world::objects_ref::ObjectsRef;
use crate::world::slots::PlayerSlotsExt;
use crate::{GenieError, GenieResult};

/// Where a unit's new Enter goes: its own action list, or under the Artifact on top as its
/// sub-action (`FUN_1410847e0`; the top action read before the task search).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum EnterSlot {
    /// The unit's own action.
    Own,
    /// The Artifact's sub-action.
    UnderArtifact,
}

impl EnterSlot {
    /// The slot under the top action `top`.
    pub(crate) fn under(top: Option<ActionKind>) -> Self {
        if top == Some(ARTIFACT_KIND) {
            Self::UnderArtifact
        } else {
            Self::Own
        }
    }
}

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
        notifies: Notifies,
    ) -> GenieResult<()> {
        let (action, target, class) = self.new_enter(player, idx, target, task)?;
        if let Some(ai) = self
            .parts
            .unit_ai
            .get_mut(self.players.live(player, idx)?.id)
        {
            ai.notify_work(
                Some(OrderKind::Enter),
                Some(AiAction::Enter),
                Some((target.id(), class)),
                point,
                notifies,
                self.streams.get_mut(Stream::Ai),
            )?;
            self.remember_order(player, idx, (Some(OrderKind::Enter), Some(AiAction::Enter)))?;
        }
        let failed = self.install_enter(player, idx, target, action, task)?;
        self.post_own_failure(player, idx, failed)
    }

    /// Slot 0x3a0 (`FUN_1410847e0`, the enter command) of the unit at `idx` of `player` on
    /// `target`, `force` its third argument: a target the world does not hold is not taken;
    /// unforced, a unit entering the target already (its top action an Enter on it) is taken.
    /// Slot 0x820 ([`World::find_work_task`]) looks for a task on the target with the garrison
    /// filter (3) and the switch (the unit takes the master that gave it, slot 0x90): none is
    /// not taken; else its Enter ([`World::enter_with`]): taken.
    ///
    /// # Errors
    ///
    /// What the Enter does not model.
    pub(crate) fn enter_command(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
        force: OrderForce,
    ) -> GenieResult<Answer> {
        if self.object(target).is_none() {
            return Ok(Answer::NotTaken);
        }
        let unit = self.players.live(player, idx)?;
        let top = self.parts.actions_of(unit.id).top_action_kind();
        if force == OrderForce::Unforced
            && top == Some(ENTER)
            && self.parts.actions_of(unit.id).top_target() == Some(target)
        {
            return Ok(Answer::Taken);
        }
        let Some(task) =
            self.find_work_task(player, idx, target, Some(GARRISON), MasterSwap::Change)?
        else {
            return Ok(Answer::NotTaken);
        };
        let slot = EnterSlot::under(top);
        let failed = self.enter_with(player, idx, target, &task, slot)?;
        self.post_own_failure(player, idx, failed)?;
        Ok(Answer::Taken)
    }

    /// The notify an Enter's install gave the unit's own unit AI ([`World::install_enter_as`]),
    /// posted to it (slot 0x328, with the unit's state).
    fn post_own_failure(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        failed: Option<Notify>,
    ) -> GenieResult<()> {
        let unit = self.players.live_mut(player, idx)?;
        let state = unit.life.state;
        match (failed, self.parts.unit_ai.get_mut(unit.id)) {
            (Some(notify), Some(ai)) => ai.post(notify, state),
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
        order: EnterOrder,
    ) -> GenieResult<Option<Notify>> {
        if let Some(master) = order.switch {
            self.change_master(player, idx, master)?;
        }
        self.enter_with(player, idx, order.target, &order.task, order.slot)
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
        slot: EnterSlot,
    ) -> GenieResult<Option<Notify>> {
        let (action, target, _) = self.new_enter(player, idx, target, task)?;
        self.install_enter_as(player, idx, target, action, task, slot)
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
    ) -> GenieResult<(EnterAction, ContainerId, ClassId)> {
        let (tp, unit) = self
            .object(target)
            .ok_or(GenieError::unsupported("an Enter whose building is gone"))?;
        let target = ContainerId::new(
            if unit.master(self.master_tables(), tp)?.kind == MasterKind::BUILDING {
                self.parts
                    .building_of(unit.id)
                    .annex_of
                    .map_or(target, MainBuildingId::id)
            } else {
                target
            },
        );
        let (tp, unit) = self.object(target).ok_or_else(|| object_gone(target))?;
        let tm = unit.master(self.master_tables(), tp)?;
        let (class, height) = (tm.class, unit.position().z());
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
    ) -> GenieResult<Option<Notify>> {
        self.install_enter_as(player, idx, target, action, task, EnterSlot::Own)
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
        slot: EnterSlot,
    ) -> GenieResult<Option<Notify>> {
        let id = self.players.live(player, idx)?.id;
        if slot == EnterSlot::UnderArtifact {
            self.enter_under_artifact(player, idx, id, action)?;
        } else {
            self.clear_actions(player, idx)?;
        }
        if self.players.live(player, idx)?.life.inside.is_some() {
            return self.enter_from_inside(player, idx, action, slot).map(Some);
        }
        let (tp, unit) = self.object(target).ok_or_else(|| object_gone(target))?;
        if !self.has_room(tp, unit, Some(id))? {
            return Err(GenieError::unsupported(
                "an Enter into a building without room (state 11)",
            ));
        }
        let (enter_target, start) = self.entering(player, idx, target, task.work_range)?;
        let walking = self.enter_walk_again(player, idx, &mut action, &enter_target, start)?;
        match walking {
            // the enter's set_state(1) reads the Enter as the unit's; a sub-action goes back
            Walking::Enter => {
                self.parts
                    .actions
                    .of_mut(self.players.live(player, idx)?.id)?
                    .list
                    .set_enter(Some(action));
                let entered = self.enter_now(player, idx, target);
                if slot == EnterSlot::UnderArtifact {
                    let actions = self.parts.actions.of_mut(id)?;
                    let take_enter = actions.list.take_enter();
                    if let (Some(enter_action), Some(art)) =
                        (take_enter, actions.list.artifact_mut())
                    {
                        art.sub = Some(ArtifactSub::Enter {
                            enter: enter_action,
                        });
                    }
                }
                entered.map(|()| None)
            }
            Walking::Walk(to) => {
                self.parts.actions.of_mut(id)?.place_enter(action, slot);
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
    ) -> GenieResult<()> {
        let by = self.releaser(player, idx)?;
        // `EnterSlot::under` read the Artifact on top before the task search, which takes no
        // action of the unit
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
        if let Some(object_id) = gone.released {
            self.release(object_id, by)?;
        }
        let unit = self.players.live(player, idx)?;
        if gone.stops
            && let Some(mover) = self.parts.mover_mut(unit.id)
        {
            mover.stop()?;
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
        slot: EnterSlot,
    ) -> GenieResult<Notify> {
        action.state = EnterState::Failed;
        let masters = OwnerMasters::new(&self.parts.own_master, self.model.masters.of(player));
        let unit = self.players.live_mut(player, idx)?;
        self.parts
            .actions
            .of_mut(unit.id)?
            .place_enter(action, slot);
        unit.shows_standing_sprite(
            self.parts.motion.layer_mut(unit.id),
            masters,
            &self.model.graphics,
            self.streams.get_mut(Stream::Sprite),
            self.model.tuning.sprite,
        )?;
        Ok(Notify::own(
            unit.id,
            NotifyKind::ACTION_FAILED,
            [AiAction::Enter.value(), 0, 0],
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
    ) -> GenieResult<(EnterTarget, WalkStart)> {
        let by_id = ObjectsRef::new(
            &self.players,
            &self.index,
            &self.model.masters,
            &self.parts,
            &self.model.roles,
            &self.model.tuning,
        );
        let id = self.players.live(player, idx)?.id;
        let (tp, target_unit) = self.object(target).ok_or(UNPLACED)?;
        let tm = target_unit.master(self.master_tables(), tp)?.clone();
        let room = self.has_room(tp, target_unit, Some(id))?;
        let placed_object = placed_object(&self.players, self.place(target))?;
        let target_speed = self.top_speed(tp, placed_object)?;
        let unit = self.players.live(player, idx)?;
        enter_view(
            &mut self.spatial,
            (unit, self.parts.mover_of(unit.id)),
            (
                (
                    placed_object,
                    self.parts.mover_of(placed_object.id),
                    target_speed,
                ),
                &tm,
                room,
            ),
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
        enter_target: &EnterTarget,
        start: WalkStart,
    ) -> GenieResult<Walking> {
        let by_id = ObjectsRef::new(
            &self.players,
            &self.index,
            &self.model.masters,
            &self.parts,
            &self.model.roles,
            &self.model.tuning,
        );
        let (tp, target) = self.object(action.target).ok_or(UNPLACED)?;
        let master = target.master(self.master_tables(), tp)?.clone();
        let target = placed_object(&self.players, self.place(action.target))?;
        let target_speed = self.top_speed(tp, target)?;
        let unit = self.players.live(player, idx)?;
        let reads = MoveToReads::of(
            (unit, self.top_speed(player, unit)?),
            unit.master(self.master_tables(), player)?,
            (
                &self.model.graphics,
                self.parts.combat_of(unit.id).abilities.look.as_ref(),
            ),
            &self.model.roles,
        );
        let meeting = Meeting {
            spatial: &mut self.spatial,
            zones: &mut self.zones,
            target,
            master: &master,
            room: enter_target.room,
            unit,
            mover: self.parts.mover_of(unit.id),
            target_actions: self.parts.actions_of(target.id),
            target_unread: self.parts.unread.get(target.id),
            target_speed,
        };
        action.walk_again(enter_target, start, meeting, &reads, by_id)
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
    ) -> GenieResult<(Point, u8)> {
        let by_id = ObjectsRef::new(
            &self.players,
            &self.index,
            &self.model.masters,
            &self.parts,
            &self.model.roles,
            &self.model.tuning,
        );
        let id = self.players.live(player, idx)?.id;
        let (tp, target_unit) = self.object(target).ok_or(UNPLACED)?;
        let master = target_unit.master(self.master_tables(), tp)?.clone();
        let room = self.has_room(tp, target_unit, Some(id))?;
        let target = placed_object(&self.players, self.place(target))?;
        let target_speed = self.top_speed(tp, target)?;
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
            target_speed,
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
    ) -> GenieResult<()> {
        let by_id = ObjectsRef::new(
            &self.players,
            &self.index,
            &self.model.masters,
            &self.parts,
            &self.model.roles,
            &self.model.tuning,
        );
        let Some(Place::Live(tp, ti)) = self.place(transport) else {
            return Err(UNREACHED);
        };
        let ship = self.players.live(tp, ti)?;
        let top_speed = self.top_speed_terms(tp, ship)?;
        let mut plan = Planning {
            pathing: &mut self.pathing,
            now: self.world_time,
            player_kind: self.players.player(tp)?.kind,
            object: self.streams.get_mut(Stream::Object),
            groups: &self.groups,
        };
        if !ship_reaches(
            (
                ship,
                self.parts.actions_of(ship.id),
                (self.parts.mover_of(ship.id), top_speed),
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
    pub(crate) fn ship_move(&mut self, ship: Option<ContainerId>, to: Position) -> GenieResult<()> {
        let Some(Place::Live(player_id, list_index)) = self.place(ship) else {
            return Err(UNREACHED);
        };
        let walk = AiMove {
            point: to,
            range: Distance::ZERO,
            force: OrderForce::Forced,
            pace: Pace::Run,
        };
        self.move_aside(player_id, list_index, walk)
    }

    /// The enter at the Enter's start ([`crate::objects::unit::Unit::enter_building`] into its
    /// target, any player's live object, then the start's own `set_state(1)`), on the unit's
    /// step ([`World::run_on_step`]).
    pub(super) fn enter_now(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ContainerId,
    ) -> GenieResult<()> {
        let inside = self
            .players
            .iter()
            .flat_map(|player| player.objects.iter().chain(&player.sleeping))
            .filter(|target_unit| target_unit.life.inside == Some(target))
            .count();
        let Some(Place::Live(..)) = self.place(target) else {
            return Err(ENTER_SLEEPING);
        };
        let roles = self.model.roles.clone();
        self.run_on_step(player, idx, |unit, actions, step| {
            if unit.enter_building(actions, step, target, inside, &roles)? == EnterOutcome::NoRoom {
                return Err(GenieError::unsupported(
                    "an Enter in reach that did not get in",
                ));
            }
            unit.entered(actions, step)
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
    ) -> GenieResult<()> {
        let Some(Place::Live(tp, ti)) = self.place(target) else {
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
        let roles = self.model.roles.clone();
        self.run_on_step(player, idx, |unit, actions, step| {
            unit.go_inside(actions, step, target, &me, &bm, &roles)
        })
    }
}

/// The live or sleeping object at `place` among `players` ([`World::place`]).
///
/// # Errors
///
/// [`UNPLACED`] for an object the world does not place.
fn placed_object(players: &[Player], place: Option<Place>) -> GenieResult<&Unit> {
    match place {
        Some(Place::Live(owner, slot)) => players.live(owner, slot),
        Some(Place::Sleeping(player_id, list_index)) => players.asleep(player_id, list_index),
        None => Err(UNPLACED),
    }
}
