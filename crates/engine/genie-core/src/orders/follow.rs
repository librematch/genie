//! FOLLOW and GUARD, a move to an object and the head action's move.

use crate::groups::LeavingWalk;
use crate::map::Distance;
use crate::map::Point;
use crate::map::Position;
use crate::math::fmath::below_or_unordered;
use crate::math::rng::Stream;
use crate::movement::ai_moves::MoveSprites;
use crate::movement::ai_moves::PointMove;
use crate::movement::move_to::ToObject;
use crate::movement::move_to::{
    MoveToAction, TARGET_LACKING, holds_object, object_range, object_sprite,
};
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::actions::Action;
use crate::objects::masters::MasterKind;
use crate::objects::masters::OwnerMasters;
use crate::objects::masters::PlayersMastersExt;
use crate::orders::ControlKey;
use crate::orders::Kept;
use crate::orders::moves::MOVED_UNMOVING;
use crate::orders::{Followed, follow_axis, follow_spread, ignore_unit};
use crate::players::PlayerId;
use crate::unit_ai::AiAction;
use crate::unit_ai::AiLayer;
use crate::unit_ai::Answer;
use crate::unit_ai::Issuer;
use crate::unit_ai::OrderKind;
use crate::unit_ai::{OrderPriority, OrderTargetKind, QueuedOrder};
use crate::world::World;
use crate::world::index::Place;
use crate::world::layer::ClassRoles;
use crate::world::slots::CheckedIndexExt;
use crate::world::slots::PlayerSlotsExt;
use crate::world::unread::UNREAD_ACTION;
use crate::{GenieError, GenieResult};

impl World {
    /// FOLLOW (0x14): the command's check (`FUN_140e54c50`, [`World::command_check`]), then
    /// `FUN_140e66a60`: a target the world does not hold does nothing; the first 60 ids (500 for
    /// a computer player) go, gone ones null (`FUN_140e6b750`), to `FUN_14161c920`. Its unit AI
    /// resets (`FUN_141631040`) are the caller's ([`crate::world`]). The splitter
    /// (`FUN_140fd9ae0`, no Ctrl) parts the units; a group it forms is not modelled (a new
    /// group, `FUN_14152efc0`, follows through `FUN_141007320`; a kept one through
    /// `FUN_140fd9f40`). The command count goes on. Several loose units leave their unit groups
    /// (`FUN_14102c160` with −1) and, when the box of those that move is narrow enough
    /// ([`follow_spread`]), each moves to its offset from their mean added to the target's place
    /// ([`World::follow_offset`]); else each follows the target ([`World::follow_unit`]).
    pub(crate) fn follow(
        &mut self,
        player: PlayerId,
        units: &[Option<ObjectId>],
        target: ObjectId,
    ) -> GenieResult<()> {
        let Some(ids) = self.command_check(player, units)? else {
            return Ok(());
        };
        let Some((tp, unit)) = self.object(target) else {
            return Ok(());
        };
        let tm = unit.master(self.master_tables(), tp)?;
        let followed = Followed {
            id: target,
            place: unit.position().ground(),
            class: (0.0 < tm.radius.x() || 0.0 < tm.radius.y()).then_some(tm.class),
        };
        let cap = if self.players.player(player)?.kind.is_computer() {
            500
        } else {
            60
        };
        let ids = ids.split_at(ids.len().min(cap)).0;
        let split = self.split(ids, Kept::All, ControlKey::Up)?;
        if !split.groups.is_empty() {
            return Err(GenieError::unsupported(
                "a FOLLOW whose units the splitter groups (FUN_141007320, FUN_140fd9f40)",
            ));
        }
        self.groups.count_command();
        let loose = split.loose;
        if 1 < loose.len() {
            let mut points = Vec::with_capacity(loose.len());
            for &id in &loose {
                self.leave_group_of(id, None)?;
                if let Some(at) = self.follower(id)? {
                    points.push(at);
                }
            }
            if let Some(mean) = follow_spread(&points, &self.model.tuning.order) {
                for index in 0..loose.len() {
                    self.follow_offset(&loose, index, followed, mean)?;
                }
                return Ok(());
            }
        }
        for index in 0..loose.len() {
            self.follow_unit(&loose, index, followed)?;
        }
        Ok(())
    }

    /// GUARD (0x13) after its check and its unit AI resets (the caller's, [`crate::world`]):
    /// `FUN_140e67620`'s first 60 units (500 for a computer player) go, gone ones null
    /// (`FUN_140e6b750`), to `FUN_14161dee0`: each unit the world holds leaves the unit group the
    /// world holds of its link (`FUN_14102c160` with −1 and 0), and its unit AI takes the defend
    /// order (`0x2bd`) on `target` at once (slot 0x30 -> slot 0x28 with Immediate set and inFront
    /// clear, [`crate::unit_ai::UnitAi::take_order_now_with`], run outside the unit's update by
    /// [`World::run_unit_ai`]): issued by the unit's owner, on the target with its master as the
    /// target type and its owner, at the target's place, within `OrderTuning::guard_range`, priority 100,
    /// ready (slot 0x30's last argument, 1); slot 0x30 shows the order at its x and y.
    pub(crate) fn guard(
        &mut self,
        player: PlayerId,
        units: &[Option<ObjectId>],
        target: ObjectId,
    ) -> GenieResult<()> {
        let Some((tp, unit)) = self.object(target) else {
            return Ok(());
        };
        let tm = unit.master(self.master_tables(), tp)?;
        let (x, y, z) = (
            unit.position().x(),
            unit.position().y(),
            unit.position().z(),
        );
        let order = QueuedOrder {
            issuer: None,
            order: OrderKind::Defend,
            priority: OrderPriority::COMMAND,
            target: Some(target),
            target_owner: Some(tp),
            point: Position::new(x, y, z),
            range: Distance::new(self.model.tuning.order.guard_range),
            shown: Point::new(x, y),
            ready: true,
            target_kind: Some(OrderTargetKind::of_master(tm.id)),
        };
        let cap = if self.players.player(player)?.kind.is_computer() {
            500
        } else {
            60
        };
        for &id in units.iter().take(cap).flatten() {
            let Some(place) = self.place(id) else {
                continue;
            };
            self.leave_group_of(id, None)?;
            let (player, idx) = match place {
                Place::Live(owner, slot) => (owner, slot),
                // only slot 0x180 puts an object to sleep, one of a type below 30 without a
                // unit AI
                Place::Sleeping(player_id, list_index)
                    if !self
                        .parts
                        .unit_ai
                        .contains(self.players.asleep(player_id, list_index)?.id) =>
                {
                    continue;
                }
                Place::Sleeping(..) => {
                    return Err(GenieError::unsupported(
                        "a GUARD of a sleeping object with a unit AI",
                    ));
                }
            };
            if !self
                .parts
                .unit_ai
                .contains(self.players.live(player, idx)?.id)
            {
                continue;
            }
            let order = QueuedOrder {
                issuer: Some(Issuer::Player(player)),
                ..order
            };
            self.run_unit_ai((player, idx), |ai, at, side, player_kind, rng| {
                ai.take_order_now_with(order, (at, side, player_kind), rng)
            })?;
        }
        Ok(())
    }

    /// Unit `id` leaves the unit group the world holds of its link (`FUN_14102c160` with `target`
    /// and 0, [`World::remove_unit`]); a unit the world does not hold or in no group, nothing.
    pub(crate) fn leave_group_of(
        &mut self,
        id: ObjectId,
        target: Option<ObjectId>,
    ) -> GenieResult<()> {
        if let Some(link) = self.object(id).and_then(|(_, unit)| unit.group.id) {
            let _ = self.remove_unit(link, id, target, LeavingWalk::Stands)?;
        }
        Ok(())
    }

    /// FOLLOW's test of a unit (`FUN_14161c920`): one the world holds, of a type of 30 or more
    /// whose master moves (slot 0xe8 above 0; `COMISS`, `JNC`: a NaN speed moves), gives its
    /// place on the ground.
    pub(super) fn follower(&self, id: ObjectId) -> GenieResult<Option<Point>> {
        let Some((player_id, unit)) = self.object(id) else {
            return Ok(None);
        };
        let master = unit.master(self.master_tables(), player_id)?;
        // slot 0xe8: the master's speed (`+0x740`), before a sprite's factor
        let speed = master.animated_part().max_speed;
        let moves = MasterKind::MOVING <= master.kind && below_or_unordered(0.0, speed.get());
        Ok(moves.then_some(unit.position().ground()))
    }

    /// A follower's start (`FUN_14161c920`, both loops): its ignore list empties
    /// (`FUN_141197da0`), it waits to move (slot 0x2a0 with 1) and lists the other loose units
    /// ([`ignore_unit`]); its place in the live list. A sleeping object is of a type below 30
    /// (only slot 0x180 puts one to sleep), which [`World::follower`] passed over.
    pub(super) fn follower_start(
        &mut self,
        loose: &[ObjectId],
        loose_index: usize,
    ) -> GenieResult<(PlayerId, ListIndex)> {
        let Some(Place::Live(player_id, idx)) = self.place(*loose.checked(loose_index)?) else {
            return Err(GenieError::unsupported(
                "a FOLLOW of a sleeping object of a type of 30 or more",
            ));
        };
        let mover = self
            .parts
            .mover_mut(self.players.live(player_id, idx)?.id)
            .ok_or(GenieError::unsupported(
                "a followed unit without movement state",
            ))?;
        mover.ignore_units.clear();
        mover.waiting = 1;
        for (index, &other) in loose.iter().enumerate() {
            if index != loose_index {
                ignore_unit(&mut mover.ignore_units, other);
            }
        }
        Ok((player_id, idx))
    }

    /// FOLLOW's loose unit `loose[loose_index]` (`FUN_14161c920`'s loop for units that do not move
    /// at their offsets): it leaves its unit group (`FUN_14102c160` with the target's id); one that
    /// does not move takes nothing ([`World::follower`]); else it starts
    /// ([`World::follower_start`]) and moves to the target at its place (slot 0x138,
    /// [`World::move_to_object`], z 0). Its unit AI takes the follow order and action
    /// (`FUN_140c64b60` with 1, `FUN_140c64b50`) only when no action of its took the move; then the
    /// target with its class (none without a footprint) at its place (`FUN_140c64be0`). Slot
    /// 0x2d8's walk and show targets are not held.
    pub(super) fn follow_unit(
        &mut self,
        loose: &[ObjectId],
        loose_index: usize,
        followed: Followed,
    ) -> GenieResult<()> {
        self.leave_group_of(*loose.checked(loose_index)?, Some(followed.id))?;
        if self.follower(*loose.checked(loose_index)?)?.is_none() {
            return Ok(());
        }
        let (player_id, idx) = self.follower_start(loose, loose_index)?;
        let (x, y) = (followed.place.x(), followed.place.y());
        let taken = self.move_to_object(player_id, idx, followed.id, Position::new(x, y, 0.0))?;
        let rng = self.streams.get_mut(Stream::Ai);
        if let Some(ai) = self
            .parts
            .unit_ai
            .get_mut(self.players.live(player_id, idx)?.id)
        {
            if taken == Answer::NotTaken {
                ai.set_order(Some(OrderKind::Follow), rng);
                ai.set_action(Some(AiAction::Follow));
            }
            let (object_id, class) = followed.class.map_or((None, None), |class_id| {
                (Some(followed.id), class_id.or_none())
            });
            ai.set_target(object_id, class, Position::new(x, y, 0.0));
        }
        Ok(())
    }

    /// FOLLOW's loose unit `loose[loose_index]` at its offset (`FUN_14161c920`'s loop after the
    /// box): one that moves ([`World::follower`]) starts ([`World::follower_start`]) and goes to
    /// its place less `mean` plus the target's, held on the map ([`follow_axis`]): slot 0x138 (z 0)
    /// to the target there when its master has a footprint, else to the point
    /// ([`World::move_point_order`]). Its unit AI takes the move order and action
    /// (`FUN_140c64b60` with 1, `FUN_140c64b50`) and the target (none without a footprint) at that
    /// place (`FUN_140c64be0`). Slot 0x2d8's walk and show targets are not held.
    pub(super) fn follow_offset(
        &mut self,
        loose: &[ObjectId],
        loose_index: usize,
        followed: Followed,
        mean: Point,
    ) -> GenieResult<()> {
        let Some(follower) = self.follower(*loose.checked(loose_index)?)? else {
            return Ok(());
        };
        let (x, y) = (follower.x(), follower.y());
        let (player_id, idx) = self.follower_start(loose, loose_index)?;
        let map = self.spatial.map;
        let at = Position::new(
            follow_axis(
                x - mean.x() + followed.place.x(),
                map.width(),
                &self.model.tuning.group,
            ),
            follow_axis(
                y - mean.y() + followed.place.y(),
                map.height(),
                &self.model.tuning.group,
            ),
            0.0,
        );
        match followed.class {
            Some(_) => {
                let _ = self.move_to_object(player_id, idx, followed.id, at)?;
            }
            None => {
                let _ = self.move_point_order(player_id, idx, at)?;
            }
        }
        let rng = self.streams.get_mut(Stream::Ai);
        if let Some(ai) = self
            .parts
            .unit_ai
            .get_mut(self.players.live(player_id, idx)?.id)
        {
            ai.set_order(Some(OrderKind::Move), rng);
            ai.set_action(Some(AiAction::Move));
            let (object_id, class) = followed.class.map_or((None, None), |class_id| {
                (Some(followed.id), class_id.or_none())
            });
            ai.set_target(object_id, class, at);
        }
        Ok(())
    }

    /// Slot 0x138 to `target`: its start ([`World::move_start`]: a villager's town-bell flag, the
    /// order queue by the unit's `+0xa14`); then `FUN_14106d340`: unless the unit is its own
    /// target, its unit AI's notifies go with the world time (`FUN_140c5dc10`) and, but after a
    /// kill it stops on (`FUN_140c41c10`), its current action's slot 0x40 may take the target and
    /// `point` (a Hunt's, [`World::hunt_take_move`]; an Artifact's, `FUN_140aaca30`, would: not
    /// modelled; a `MoveTo`'s or an Explore's never); else a `MoveTo` to the target
    /// (`FUN_140aa0df0`: no range, so slot 0x268's, [`crate::movement::move_to::object_range`]; the
    /// walking sprite) takes the target it holds
    /// (slot 0x178, [`crate::movement::move_to::holds_object`]) and replaces its actions (slot 0x808, the
    /// old ones' teardowns let their targets go, [`World::clear_actions`]; then its start,
    /// [`MoveToAction::to_object`]) and its command count goes one up
    /// ([`World::count_command`]).
    ///
    /// [`MoveToAction::to_object`]: crate::movement::move_to::MoveToAction::to_object
    pub(crate) fn move_to_object(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
        point: Position,
    ) -> GenieResult<Answer> {
        let world_time = self.world_time;
        self.move_start(player, idx)?;
        let class = self
            .players
            .live(player, idx)?
            .master(self.master_tables(), player)?
            .class;
        let unit = self.players.live_mut(player, idx)?;
        if target == unit.id {
            return Ok(Answer::NotTaken);
        }
        let place = (unit.position().ground(), Some(class));
        if self.parts.unit_ai.contains(unit.id) {
            // a completion's stop (slot 0xb0 with 1): the unit's stopAction at once
            let (roles, slots) = (self.model.roles.clone(), self.model.slots);
            let unit_ai_tuning = self.model.tuning.unit_ai;
            self.run_ai_calls((player, idx), |ai, side, rng| {
                let layer = AiLayer {
                    roles: &roles,
                    slots,
                    unit_ai_tuning: &unit_ai_tuning,
                };
                ai.clear_notifies_at(world_time.since_start(), place, side, rng, layer)
            })?;
        }
        let unit = self.players.live_mut(player, idx)?;
        let stop_after_kill = self
            .parts
            .unit_ai
            .get(unit.id)
            .is_some_and(|ai| ai.stop_after_kill);
        if self.parts.unit_ai.contains(unit.id)
            && !stop_after_kill
            && self.head_takes_move(player, idx, Some(target), point)? == Answer::Taken
        {
            return Ok(Answer::Taken);
        }
        let unit = &self.players.live(player, idx)?;
        if self.parts.unread.contains(unit.id) {
            return Err(GenieError::unsupported(
                "a move to an object of a unit with an action",
            ));
        }
        // FUN_140aa0df0 makes the MoveTo first, which takes its target (slot 0x178, quiet 0);
        // slot 0x808 then clears the list (the old actions' teardowns) and starts it
        let livestock = self
            .model
            .roles
            .of_class(unit.master(self.master_tables(), player)?.class)
            .contains(ClassRoles::LIVESTOCK);
        let (tp, target_unit) = self.object(target).ok_or(TARGET_LACKING)?;
        let target_state = target_unit.life.state;
        let target_master = target_unit.master(self.master_tables(), tp)?.id;
        if holds_object(livestock, target_master, target_state, &self.model.roles) {
            self.take(target, self.releaser(player, idx)?)?;
        }
        self.clear_actions(player, idx)?;
        self.start_object_move(player, idx, (target, target_state), livestock)?;
        let id = self.players.live(player, idx)?.id;
        self.count_command(id)?;
        Ok(Answer::NotTaken)
    }

    /// [`World::move_to_object`]'s new `MoveTo` toward `target` (in state `target_state`), once the
    /// unit's actions are cleared: its range and sprite, the action's start and the standing
    /// sprite it may show; no action took the move.
    ///
    /// # Errors
    ///
    /// What the start does not model.
    fn start_object_move(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        (target, target_state): (ObjectId, ObjectState),
        livestock: bool,
    ) -> GenieResult<()> {
        let master = self
            .players
            .live(player, idx)?
            .master(self.master_tables(), player)?;
        let range = object_range(Distance::ZERO, Some(master), &self.model.tuning.order);
        let sprite = object_sprite(master).and_then(|id| self.model.graphics.get(id).copied());
        let top = self.top_speed(player, self.players.live(player, idx)?)?;
        let unit = self.players.live_mut(player, idx)?;
        let here = unit.position();
        let mover = self.parts.mover_of(unit.id).ok_or(MOVED_UNMOVING)?;
        let move_to = MoveToAction::to_object(
            &self.spatial,
            ToObject {
                me: here,
                mover,
                top,
                target,
                state: target_state,
                range,
                sprite,
                livestock,
            },
            &self.model,
        )?;
        if move_to.shows_standing() {
            unit.shows_standing_sprite(
                self.parts.motion.layer_mut(unit.id),
                OwnerMasters::new(&self.parts.own_master, self.model.masters.of(player)),
                &self.model.graphics,
                self.streams.get_mut(Stream::Sprite),
                self.model.tuning.sprite,
            )?;
        }
        self.parts
            .actions_mut(unit.id)?
            .list
            .set_move_to(Some(move_to));
        Ok(())
    }

    /// The head action's slot 0x40 (`FUN_140a97730`) on a move of the unit at `idx` to `point`,
    /// toward the object `target` if any: taken when the action takes the move. Without a target
    /// it is the unit's own [`crate::objects::unit::Unit::head_takes_point`], whose released target goes
    /// here ([`World::release`]). Toward an object, an empty list takes nothing, nor does the
    /// base slot (`FUN_140a78490`) of a `MoveTo`, an Explore, a Build (0x140aade60), an Attack,
    /// an Enter, a Fly, a missile, a Make or a wait for a foundation. The Artifact's
    /// (`FUN_140aaca30`) always takes it ([`World::artifact_take_object`]); the Hunt's
    /// (`0x140ac52c0`, [`World::hunt_take_move`]) and the Gather's (`0x140a8b740`,
    /// [`World::gather_take_move`]) decide themselves.
    pub(super) fn head_takes_move(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: Option<ObjectId>,
        point: Position,
    ) -> GenieResult<Answer> {
        let unit = self.players.live_mut(player, idx)?;
        let Some(target) = target else {
            return self.point_taken(player, idx, point);
        };
        if self.parts.unread.contains(unit.id) {
            return Err(UNREAD_ACTION);
        }
        match self.parts.actions_of(unit.id).list.head() {
            None
            | Some(
                Action::MoveTo(_)
                | Action::Explore(_)
                | Action::Build(_)
                | Action::Attack(_)
                | Action::Enter(_)
                | Action::Fly(_)
                | Action::Missile(_)
                | Action::Make(_)
                | Action::WaitSolidify(_),
            ) => Ok(Answer::NotTaken),
            Some(Action::Artifact(_)) => {
                self.artifact_take_object(player, idx, target)?;
                Ok(Answer::Taken)
            }
            Some(Action::Hunt(_)) => self.hunt_take_move(player, idx, target, point),
            Some(Action::Gather(_)) => self.gather_take_move(player, idx, target, point),
            // the Repair's slot 0x40 (FUN_140ad9700) walks to the point whatever the target, and
            // the Transport's (0x140aa3e40) sails there
            Some(Action::Repair(_) | Action::Transport(_)) => self.point_taken(player, idx, point),
        }
    }

    /// The head action's slot 0x40 on a move to `point` ([`crate::objects::unit::Unit::head_takes_point`]):
    /// the targets it let go are released and a counted repairer leaves its target's repairers.
    pub(super) fn point_taken(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        point: Position,
    ) -> GenieResult<Answer> {
        let by = self.releaser(player, idx)?;
        let id = self.players.live(player, idx)?.id;
        let top_speed = self.top_speed_terms(player, self.players.live(player, idx)?)?;
        let (released, left) = match self.players.live_mut(player, idx)?.head_takes_point(
            (self.parts.actions.of_mut(id)?, self.parts.unread.get(id)),
            self.parts.motion.layer_mut(id),
            point,
            player,
            MoveSprites {
                masters: OwnerMasters::new(&self.parts.own_master, self.model.masters.of(player)),
                graphics: &self.model.graphics,
                look: self.parts.combat.of(id).abilities.look.as_ref(),
                roles: &self.model.roles,
                sprite_tuning: self.model.tuning.sprite,
                move_to_tuning: &self.model.tuning.move_to,
                top_speed,
            },
        )? {
            PointMove::Declined => return Ok(Answer::NotTaken),
            PointMove::Taken(released) => (released, None),
            PointMove::Repairs(released, left) => (released, left),
            PointMove::Sails(transport_action) => {
                self.transport_move((player, idx), transport_action, point)?;
                return Ok(Answer::Taken);
            }
        };
        if let Some(object_id) = left {
            self.leave_builders(Some(object_id))?;
        }
        if let Some(object_id) = released {
            self.release(object_id, by)?;
        }
        Ok(Answer::Taken)
    }
}
