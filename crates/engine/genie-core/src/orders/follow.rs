//! FOLLOW and GUARD, a move to an object and the head action's move.

use crate::GenieError;
use crate::gathering::hunt::HUNT;
use crate::map::Distance;
use crate::map::Point;
use crate::map::Position;
use crate::math::rng::Stream;
use crate::movement::Speed;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::action_type;
use crate::objects::masters::class;
use crate::objects::masters::master_type::MOVING;
use crate::orders::{Followed, GUARD_RANGE, follow_axis, follow_spread, ignore_unit};
use crate::players::PlayerId;
use crate::unit_ai::ActionKind;
use crate::unit_ai::Issuer;
use crate::unit_ai::OrderKind;
use crate::world::World;
use crate::world::index::Place;
use crate::world::slots::CheckedIndex;
use crate::world::slots::PlayerSlots;

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
    ) -> Result<(), GenieError> {
        let Some(ids) = self.command_check(player, units)? else {
            return Ok(());
        };
        let Some((tp, t)) = self.object(target) else {
            return Ok(());
        };
        let tm = t.master(self.master_tables(), tp)?;
        let followed = Followed {
            id: target,
            place: t.position().ground(),
            class: (0.0 < tm.radius.x() || 0.0 < tm.radius.y()).then_some(tm.class),
        };
        let cap = if self.players.player(player)?.computer {
            500
        } else {
            60
        };
        let ids = ids.split_at(ids.len().min(cap)).0;
        let split = self.split(ids, &|_| true, false)?;
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
            if let Some(mean) = follow_spread(&points) {
                for i in 0..loose.len() {
                    self.follow_offset(&loose, i, followed, mean)?;
                }
                return Ok(());
            }
        }
        for i in 0..loose.len() {
            self.follow_unit(&loose, i, followed)?;
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
    /// target type and its owner, at the target's place, within [`GUARD_RANGE`], priority 100,
    /// ready (slot 0x30's last argument, 1); slot 0x30 shows the order at its x and y.
    pub(crate) fn guard(
        &mut self,
        player: PlayerId,
        units: &[Option<ObjectId>],
        target: ObjectId,
    ) -> Result<(), GenieError> {
        let Some((tp, t)) = self.object(target) else {
            return Ok(());
        };
        let tm = t.master(self.master_tables(), tp)?;
        let (x, y, z) = (t.position().x(), t.position().y(), t.position().z());
        let order = crate::unit_ai::QueuedOrder {
            issuer: None,
            order: OrderKind::Defend,
            priority: crate::unit_ai::OrderPriority::COMMAND,
            target: Some(target),
            target_owner: Some(tp),
            point: Position::new(x, y, z),
            range: Distance::new(GUARD_RANGE),
            shown: Point::new(x, y),
            ready: true,
            target_type: Some(crate::unit_ai::OrderTargetType::of_master(tm.id)),
        };
        let cap = if self.players.player(player)?.computer {
            500
        } else {
            60
        };
        for &id in units.iter().take(cap).flatten() {
            let Some(place) = self.place(id) else {
                continue;
            };
            self.leave_group_of(id, None)?;
            let (p, idx) = match place {
                Place::Live(p, i) => (p, i),
                // only slot 0x180 puts an object to sleep, one of a type below 30 without a
                // unit AI
                Place::Sleeping(p, i)
                    if !self.parts.unit_ai.contains(self.players.asleep(p, i)?.id) =>
                {
                    continue;
                }
                Place::Sleeping(..) => {
                    return Err(GenieError::unsupported(
                        "a GUARD of a sleeping object with a unit AI",
                    ));
                }
            };
            if !self.parts.unit_ai.contains(self.players.live(p, idx)?.id) {
                continue;
            }
            let order = crate::unit_ai::QueuedOrder {
                issuer: Some(Issuer::Player(p)),
                ..order
            };
            self.run_unit_ai((p, idx), |ai, at, side, computer, rng| {
                ai.take_order_now_with(order, (at, side, computer), rng)
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
    ) -> Result<(), GenieError> {
        if let Some(link) = self.object(id).and_then(|(_, u)| u.group.id) {
            let _ = self.remove_unit(link, id, target, false)?;
        }
        Ok(())
    }

    /// FOLLOW's test of a unit (`FUN_14161c920`): one the world holds, of a type of 30 or more
    /// whose master moves (slot 0xe8 above 0; `COMISS`, `JNC`: a NaN speed moves), gives its
    /// place on the ground.
    pub(super) fn follower(&self, id: ObjectId) -> Result<Option<Point>, GenieError> {
        let Some((p, u)) = self.object(id) else {
            return Ok(None);
        };
        let master = u.master(self.master_tables(), p)?;
        let speed = master.moving.as_ref().map_or(Speed::ZERO, |m| m.speed);
        let moves =
            MOVING <= master.kind && crate::math::fmath::below_or_unordered(0.0, speed.get());
        Ok(moves.then_some(u.position().ground()))
    }

    /// A follower's start (`FUN_14161c920`, both loops): its ignore list empties
    /// (`FUN_141197da0`), it waits to move (slot 0x2a0 with 1) and lists the other loose units
    /// ([`ignore_unit`]); its place in the live list. A sleeping object is of a type below 30
    /// (only slot 0x180 puts one to sleep), which [`World::follower`] passed over.
    pub(super) fn follower_start(
        &mut self,
        loose: &[ObjectId],
        i: usize,
    ) -> Result<(PlayerId, ListIndex), GenieError> {
        let Some(Place::Live(p, idx)) = self.place(*loose.checked(i)?) else {
            return Err(GenieError::unsupported(
                "a FOLLOW of a sleeping object of a type of 30 or more",
            ));
        };
        let mover =
            self.parts
                .mover_mut(self.players.live(p, idx)?.id)
                .ok_or(GenieError::unsupported(
                    "a followed unit without movement state",
                ))?;
        mover.ignore_units.clear();
        mover.waiting = 1;
        for (k, &other) in loose.iter().enumerate() {
            if k != i {
                ignore_unit(&mut mover.ignore_units, other);
            }
        }
        Ok((p, idx))
    }

    /// FOLLOW's loose unit `loose[i]` (`FUN_14161c920`'s loop for units that do not move at
    /// their offsets): it leaves its unit group (`FUN_14102c160` with the target's id); one that
    /// does not move takes nothing ([`World::follower`]); else it starts
    /// ([`World::follower_start`]) and moves to the target at its place (slot 0x138,
    /// [`World::move_to_object_with`], z 0). Its unit AI takes the follow order and action
    /// (`FUN_140c64b60` with 1, `FUN_140c64b50`) only when no action of its took the move; then
    /// the target with its class (none without a footprint) at its place (`FUN_140c64be0`).
    /// Slot 0x2d8's walk and show targets and `+0x888` are not held.
    pub(super) fn follow_unit(
        &mut self,
        loose: &[ObjectId],
        i: usize,
        followed: Followed,
    ) -> Result<(), GenieError> {
        self.leave_group_of(*loose.checked(i)?, Some(followed.id))?;
        if self.follower(*loose.checked(i)?)?.is_none() {
            return Ok(());
        }
        let (p, idx) = self.follower_start(loose, i)?;
        let (x, y) = (followed.place.x(), followed.place.y());
        let taken =
            self.move_to_object_with(p, idx, followed.id, Position::new(x, y, 0.0), true)?;
        let rng = self.streams.get(Stream::Ai);
        if let Some(ai) = self.parts.unit_ai.get_mut(self.players.live(p, idx)?.id) {
            if !taken {
                ai.set_order(Some(OrderKind::Follow), rng);
                ai.set_action(Some(ActionKind::Follow));
            }
            let (t, class) = followed
                .class
                .map_or((None, None), |c| (Some(followed.id), c.or_none()));
            ai.set_target(t, class, Position::new(x, y, 0.0));
        }
        Ok(())
    }

    /// FOLLOW's loose unit `loose[i]` at its offset (`FUN_14161c920`'s loop after the box): one
    /// that moves ([`World::follower`]) starts ([`World::follower_start`]) and goes to its place
    /// less `mean` plus the target's, held on the map ([`follow_axis`]): slot 0x138 (z 0) to the
    /// target there when its master has a footprint, else to the point
    /// ([`World::move_point_order_with`]). Its unit AI takes the move order and action
    /// (`FUN_140c64b60` with 1, `FUN_140c64b50`) and the target (none without a footprint) at
    /// that place (`FUN_140c64be0`). Slot 0x2d8's walk and show targets are not held.
    pub(super) fn follow_offset(
        &mut self,
        loose: &[ObjectId],
        i: usize,
        followed: Followed,
        mean: Point,
    ) -> Result<(), GenieError> {
        let Some(follower) = self.follower(*loose.checked(i)?)? else {
            return Ok(());
        };
        let (x, y) = (follower.x(), follower.y());
        let (p, idx) = self.follower_start(loose, i)?;
        let map = self.spatial.map;
        let at = Position::new(
            follow_axis(x - mean.x() + followed.place.x(), map.width()),
            follow_axis(y - mean.y() + followed.place.y(), map.height()),
            0.0,
        );
        match followed.class {
            Some(_) => {
                let _ = self.move_to_object_with(p, idx, followed.id, at, true)?;
            }
            None => self.move_point_order_with(p, idx, at, true)?,
        }
        let rng = self.streams.get(Stream::Ai);
        if let Some(ai) = self.parts.unit_ai.get_mut(self.players.live(p, idx)?.id) {
            ai.set_order(Some(OrderKind::Move), rng);
            ai.set_action(Some(ActionKind::Move));
            let (t, class) = followed
                .class
                .map_or((None, None), |c| (Some(followed.id), c.or_none()));
            ai.set_target(t, class, at);
        }
        Ok(())
    }

    /// Slot 0x138 to `target` outside slot 0x128's work command (the unit's `+0xa14` 1):
    /// [`World::move_to_object_with`].
    pub(crate) fn move_to_object(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
        point: Position,
    ) -> Result<(), GenieError> {
        self.move_to_object_with(player, idx, target, point, true)
            .map(|_| ())
    }

    /// Slot 0x138 to `target` (`FUN_141216f10`): a villager is no longer under the town bell
    /// (`+0x9dc`); with `+0xa14` (`a14`: 1 but inside slot 0x128's work command with 0) its unit
    /// AI's order queue empties (`FUN_140c5e010`); then `FUN_14106d340`: unless the unit is its own
    /// target, its unit AI's notifies go with the world time (`FUN_140c5dc10`) and, but after a
    /// kill it stops on (`FUN_140c41c10`), its current action's slot 0x40 may take the target and
    /// `point` (a Hunt's, [`World::hunt_take_move`]; an Artifact's, `FUN_140aaca30`, would: not
    /// modelled; a `MoveTo`'s or an Explore's never); else a `MoveTo` to the target
    /// (`FUN_140aa0df0`: no range, so slot 0x268's, [`crate::movement::move_to::object_range`]; the
    /// walking sprite) takes the target it holds
    /// (slot 0x178, [`crate::movement::move_to::holds_object`]) and replaces its actions (slot 0x808, the
    /// old ones' teardowns let their targets go, [`World::clear_actions`]; then its start,
    /// [`MoveToAction::to_object`]). Its `command_flag` (`+0x888`) is not held.
    ///
    /// [`MoveToAction::to_object`]: crate::movement::move_to::MoveToAction::to_object
    pub(crate) fn move_to_object_with(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
        point: Position,
        a14: bool,
    ) -> Result<bool, GenieError> {
        let world_time = self.world_time;
        let class = self
            .players
            .live(player, idx)?
            .master(self.master_tables(), player)?
            .class;
        let villager = class == class::VILLAGER;
        let unit = self.players.live_mut(player, idx)?;
        if villager {
            self.parts.set_town_bell_flag(unit.id, false);
        }
        if a14 && let Some(ai) = self.parts.unit_ai.get_mut(unit.id) {
            ai.orders.clear();
        }
        if target == unit.id {
            return Ok(false);
        }
        let place = (unit.position().ground(), Some(class));
        if self.parts.unit_ai.contains(unit.id) {
            // a completion's stop (slot 0xb0 with 1): the unit's stopAction at once
            self.run_ai_calls((player, idx), |ai, side, rng| {
                ai.clear_notifies_at(world_time.since_start(), place, side, rng)
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
            && self.head_takes_move(player, idx, Some(target), point)?
        {
            return Ok(true);
        }
        let unit = &self.players.live(player, idx)?;
        if self.parts.actions_of(unit.id).unmodelled_action.is_some() {
            return Err(GenieError::unsupported(
                "a move to an object of a unit with an action",
            ));
        }
        // FUN_140aa0df0 makes the MoveTo first, which takes its target (slot 0x178, quiet 0);
        // slot 0x808 then clears the list (the old actions' teardowns) and starts it
        let livestock = unit.master(self.master_tables(), player)?.class == class::LIVESTOCK;
        let (tp, t) = self
            .object(target)
            .ok_or(crate::movement::move_to::TARGET_LACKING)?;
        let t_state = t.life.state;
        let t_master = t.master(self.master_tables(), tp)?.id;
        if crate::movement::move_to::holds_object(livestock, t_master, t_state) {
            self.take(target, self.releaser(player, idx)?)?;
        }
        self.clear_actions(player, idx)?;
        self.start_object_move(player, idx, (target, t_state), livestock)
    }

    /// [`World::move_to_object_with`]'s new `MoveTo` toward `target` (in state `t_state`), once the
    /// unit's actions are cleared: its range and sprite, the action's start and the standing
    /// sprite it may show. Always false: no action took the move.
    ///
    /// # Errors
    ///
    /// What the start does not model.
    fn start_object_move(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        (target, t_state): (ObjectId, crate::objects::ObjectState),
        livestock: bool,
    ) -> Result<bool, GenieError> {
        let master = self
            .players
            .live(player, idx)?
            .master(self.master_tables(), player)?;
        let range = crate::movement::move_to::object_range(Distance::ZERO, Some(master));
        let sprite = crate::movement::move_to::object_sprite(master)
            .and_then(|id| self.graphics.get(id).copied());
        let unit = self.players.live_mut(player, idx)?;
        let here = unit.position();
        let mover = self
            .parts
            .mover_mut(unit.id)
            .ok_or(GenieError::unsupported(
                "a moved unit without movement state",
            ))?;
        let move_to = crate::movement::move_to::MoveToAction::to_object(
            &self.spatial,
            here,
            mover,
            (target, t_state),
            range,
            (sprite, livestock),
        )?;
        if move_to.shows_standing() {
            unit.shows_standing_sprite(
                self.parts.motion.layer_mut(unit.id),
                crate::objects::masters::OwnerMasters::new(
                    &self.parts.own_master,
                    self.masters.of(player),
                ),
                &self.graphics,
                self.streams.get(Stream::Sprite),
            )?;
        }
        self.parts
            .actions_mut(unit.id)?
            .list
            .set_move_to(Some(move_to));
        Ok(false)
    }

    /// The head action's slot 0x40 (`FUN_140a97730`) on a move of the unit at `idx` to `point`,
    /// toward the object `target` if any: true when the action takes the move. Without a target
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
    ) -> Result<bool, GenieError> {
        let unit = self.players.live_mut(player, idx)?;
        let Some(target) = target else {
            return self.point_taken(player, idx, point);
        };
        if self.parts.actions_of(unit.id).unmodelled_action.is_some() {
            return Err(GenieError::unsupported(
                "slot 0x40 of an action the slice does not hold",
            ));
        }
        match self.parts.actions_of(unit.id).top_action_kind() {
            None
            | Some(
                crate::movement::move_to::MOVE_TO
                | crate::movement::explore::EXPLORE
                | action_type::BUILD
                | crate::combat::attack::ATTACK
                | crate::garrison::ENTER
                | crate::movement::fly::FLY
                | crate::combat::missile::MISSILE
                | crate::gathering::make::MAKE
                | crate::construction::wait_solidify::WAIT_SOLIDIFY,
            ) => Ok(false),
            Some(crate::ownership::artifact::ARTIFACT_TYPE) => {
                self.artifact_take_object(player, idx, target)?;
                Ok(true)
            }
            Some(HUNT) => self.hunt_take_move(player, idx, target, point),
            Some(crate::gathering::gather::GATHER) => {
                self.gather_take_move(player, idx, target, point)
            }
            // the Repair's slot 0x40 (FUN_140ad9700) walks to the point whatever the target, and
            // the Transport's (0x140aa3e40) sails there
            Some(crate::construction::repair::REPAIR | crate::garrison::transport::TRANSPORT) => {
                self.point_taken(player, idx, point)
            }
            Some(other) => Err(GenieError::unreachable(
                "a top action of a type that top_action never holds",
            )
            .with("type", other)),
        }
    }

    /// The head action's slot 0x40 on a move to `point` ([`crate::objects::unit::Unit::head_takes_point`]):
    /// the targets it let go are released and a counted repairer leaves its target's repairers.
    pub(super) fn point_taken(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        point: Position,
    ) -> Result<bool, GenieError> {
        let by = self.releaser(player, idx)?;
        let id = self.players.live(player, idx)?.id;
        let (released, left) = match self.players.live_mut(player, idx)?.head_takes_point(
            self.parts.actions.of_mut(id)?,
            self.parts.motion.layer_mut(id),
            point,
            player,
            crate::objects::masters::OwnerMasters::new(
                &self.parts.own_master,
                self.masters.of(player),
            ),
            (
                &self.graphics,
                self.parts.combat.of(id).abilities.look.as_ref(),
            ),
        )? {
            crate::movement::ai_moves::PointMove::Declined => return Ok(false),
            crate::movement::ai_moves::PointMove::Taken(released) => (released, None),
            crate::movement::ai_moves::PointMove::Repairs(released, left) => (released, left),
            crate::movement::ai_moves::PointMove::Sails => {
                self.transport_move(player, idx, point)?;
                return Ok(true);
            }
        };
        if let Some(t) = left {
            self.leave_builders(Some(t))?;
        }
        if let Some(t) = released {
            self.release(t, by)?;
        }
        Ok(true)
    }
}
