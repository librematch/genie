//! The unit AI's order slots, one per kind of order: defend, build, work, gather, hunt, follow,
//! enter, transport, unload, seek and explore; an order on a target and one taken at once.

use crate::GenieResult;
use crate::construction::FoundationId;
use crate::map::Distance;
use crate::map::Point;
use crate::map::Position;
use crate::map::Tile;
use crate::math::rng::Well512;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::WorkerCount;
use crate::objects::masters::ClassId;
use crate::objects::masters::GatherGroup;
use crate::players::PlayerKind;
use crate::unit_ai::AiAction;
use crate::unit_ai::AiCommand;
use crate::unit_ai::AiLayer;
use crate::unit_ai::Answer;
use crate::unit_ai::OrderForce;
use crate::unit_ai::OrderOutcome;
use crate::unit_ai::OrderSite;
use crate::unit_ai::Pace;
use crate::unit_ai::StopOrder;
use crate::unit_ai::TakenAs;
use crate::unit_ai::UnitAiKind;
use crate::unit_ai::attack_orders::OrderNow;
use crate::unit_ai::world_view::UnitView;
use crate::unit_ai::{
    AiMove, Effect, GatheringCommand, QueuedOrder, ResourceSearch, Seen, Surroundings, UnitAi,
    UnitCommand, civilian_kind,
};
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::layer::generated::UnitAiTuning;

impl UnitAi {
    /// Slot 0x170 (`FUN_140c4ccc0`), the defend of object `target` within `range`, `force`d or
    /// without an action value: on an object the world holds, the action value is the defend's
    /// (`0x259`, the last keeping the old), the target the object at its place
    /// (`FUN_140c64c10`), the desired distance `range` and the defend target the object.
    pub(super) fn defend_object(
        &mut self,
        target: ObjectId,
        range: Distance,
        force: OrderForce,
        at: &Surroundings,
        side: &UnitView<'_, '_>,
    ) -> GenieResult<Answer> {
        if force == OrderForce::Unforced && self.current_action.is_some() {
            return Ok(Answer::NotTaken);
        }
        let Some(unit_seen) = Self::world_object(target, at, side)? else {
            return Ok(Answer::NotTaken);
        };
        self.set_action(Some(AiAction::Defend));
        self.target_at(Some(target), Some(unit_seen.class), unit_seen.position);
        self.desired_distance = range;
        self.defend_target = Some(target);
        Ok(Answer::Taken)
    }

    /// Slot 0x178 (`FUN_140c4ce30`), the defend of `point` within `range`, `force`d or without an
    /// action value: the defend's action value (`0x259`, the last keeping the old), no target at
    /// the point at the unit's height (`FUN_140c64c10` with −1), the desired distance `range`
    /// and no defend target.
    pub(super) const fn defend_point(
        &mut self,
        point: Point,
        range: Distance,
        force: OrderForce,
        at: &Surroundings,
    ) -> Answer {
        if matches!(force, OrderForce::Unforced) && self.current_action.is_some() {
            return Answer::NotTaken;
        }
        self.set_action(Some(AiAction::Defend));
        self.target_at(None, None, point.with_height(at.position.z()));
        self.desired_distance = range;
        self.defend_target = None;
        Answer::Taken
    }

    /// `BuildObject` (slot 0xf0, `FUN_140c47d80`) on foundation `target`, `force`d or off the
    /// build action value (`0x25a`): the unit's build command (slot 0x380, forced as every
    /// caller passes it), which the caller runs after the update in its order
    /// ([`AiCommand::BuildObject`]); its 0 ([`UnitView::builds`]: the foundation gone, no task)
    /// leaves the AI as it was. Then the build action value (the last keeping the old) and the
    /// foundation as the target at its place with its class as the world holds it, else class −1
    /// at (0, 0, 0) (the target and class before becoming the last). Taken when it builds.
    ///
    /// # Errors
    ///
    /// A foundation the slice cannot read, and what the build command's task search does not
    /// model.
    pub(crate) fn build_object(
        &mut self,
        target: FoundationId,
        force: OrderForce,
        at: &Surroundings,
        side: &mut UnitView<'_, '_>,
        rng: &mut Well512,
    ) -> GenieResult<Answer> {
        if (force == OrderForce::Unforced && self.current_action == Some(AiAction::Build))
            || !side.builds(target)?
        {
            return Ok(Answer::NotTaken);
        }
        side.effect_now(Effect::Command(AiCommand::BuildObject(target)), self, rng)?;
        self.set_action(Some(AiAction::Build));
        self.take_held_target(Some(target.id()), at, side)?;
        Ok(Answer::Taken)
    }

    /// The work slots on object `target` (heal 0xe0, `FUN_140c4e010`; conversion 0xd8,
    /// `FUN_140c4c9f0`; repair 0xe8, `FUN_140c5e3a0`), `force`d or off their own action value
    /// `action` (`0x25b`, `0x25c`, `0x26a`): the unit's `command` ([`UnitView::command`]: slots
    /// 0x108, 0x370, 0x378), on its 1 the action value `action` (the last keeping the old) and the
    /// target with its class and place as the world then holds it, else class −1 at (0, 0, 0)
    /// (the target and class before becoming the last).
    pub(super) fn work_object(
        &mut self,
        target: ObjectId,
        force: OrderForce,
        (action, command): (AiAction, UnitCommand),
        (at, side): (&Surroundings, &mut UnitView<'_, '_>),
    ) -> GenieResult<Answer> {
        if force == OrderForce::Unforced && self.current_action == Some(action) {
            return Ok(Answer::NotTaken);
        }
        if side.command(command)? == Answer::NotTaken {
            return Ok(Answer::NotTaken);
        }
        self.set_action(Some(action));
        self.take_held_target(Some(target), at, side)?;
        Ok(Answer::Taken)
    }

    /// `GatherObject` (slot 0xd0, `FUN_140c4dd50`) on object `target`, `force`d or off the
    /// gather's action value (`0x261`): the unit's gather command with the force (slot 0x368,
    /// [`GatheringCommand::Gather`]); on its 1 the gather's action value (the last keeping the
    /// old) and the target as the world then holds it ([`UnitAi::take_held_target`]). Taken when
    /// the unit gathers.
    ///
    /// # Errors
    ///
    /// What the gather command does not model.
    pub(super) fn gather_object(
        &mut self,
        target: ObjectId,
        force: OrderForce,
        at: &Surroundings,
        side: &mut UnitView<'_, '_>,
        rng: &mut Well512,
    ) -> GenieResult<Answer> {
        if force == OrderForce::Unforced && self.current_action == Some(AiAction::Gather) {
            return Ok(Answer::NotTaken);
        }
        let command = GatheringCommand::Gather { target, force };
        if side.gathering_command(command, self, rng)? == Answer::NotTaken {
            return Ok(Answer::NotTaken);
        }
        self.set_action(Some(AiAction::Gather));
        self.take_held_target(Some(target), at, side)?;
        Ok(Answer::Taken)
    }

    /// `HuntObject` (slot 0xc8, `FUN_140c4e220`) on object `target` (none for −1), `force`d or off
    /// the hunt's action value (`0x265`; `"UAIMod::HuntObject failed because uncon is FALSE."`):
    /// the current action stops at once (slot 0xb0 with 1, [`UnitAi::stop_current`]), then the
    /// unit's hunt command with the force (slot 0x360, [`GatheringCommand::Hunt`]); on its 1 the
    /// hunt's action value (the last the stopped none) and the target as the world then holds it
    /// ([`UnitAi::take_held_target`]). Taken when the unit hunts; else the stop stays
    /// (`"UAIMod::HuntObject: returning FALSE."`).
    ///
    /// # Errors
    ///
    /// What the stop and the hunt command do not model.
    pub(super) fn hunt_object(
        &mut self,
        target: Option<ObjectId>,
        force: OrderForce,
        at: &Surroundings,
        side: &mut UnitView<'_, '_>,
        rng: &mut Well512,
    ) -> GenieResult<Answer> {
        if force == OrderForce::Unforced && self.current_action == Some(AiAction::Hunt) {
            return Ok(Answer::NotTaken);
        }
        self.stop_current(StopOrder::Keep, at, side, rng)?;
        let command = GatheringCommand::Hunt { target, force };
        if side.gathering_command(command, self, rng)? == Answer::NotTaken {
            return Ok(Answer::NotTaken);
        }
        self.set_action(Some(AiAction::Hunt));
        self.take_held_target(target, at, side)?;
        Ok(Answer::Taken)
    }

    /// The target a slot takes once the unit's command took it (`FUN_140c64be0`'s inline form
    /// after `FUN_1415417a0`): object `target` (none for −1) with its master's class and its place
    /// as the world holds it, else class −1 at (0, 0, 0); the target and class before become the
    /// last ones.
    ///
    /// # Errors
    ///
    /// The unit's own master the slice does not hold, for a target that is the unit, and what the
    /// side's lookup does not model ([`UnitView::seen_unit`]).
    pub(super) fn take_held_target(
        &mut self,
        target: Option<ObjectId>,
        at: &Surroundings,
        side: &UnitView<'_, '_>,
    ) -> GenieResult<()> {
        match Self::world_object(target, at, side)? {
            Some(unit_seen) => {
                self.set_target(target, unit_seen.class.or_none(), unit_seen.position);
            }
            None => self.set_target(target, None, Position::ORIGIN),
        }
        Ok(())
    }

    /// Slot 0x168 (`FUN_140c4dab0`, "GOT A FOLLOW ORDER"), the follow of object `target` within
    /// `range`, `force`d or without an action value: on an object the world holds, the follow's
    /// action value (`0x264`, the last keeping the old), the target at its place
    /// (`FUN_140c64c10`) and the desired distance `range`; when the footprints' squared gap
    /// (`FUN_14118f8c0`) exceeds `range` squared (`COMISS`/`JBE`: not a number stays), the AI
    /// moves to it (slot 0x138, [`UnitAi::move_to_object`]).
    pub(super) fn follow_object(
        &mut self,
        target: ObjectId,
        range: Distance,
        force: OrderForce,
        at: &Surroundings,
        side: &mut UnitView<'_, '_>,
        rng: &mut Well512,
    ) -> GenieResult<Answer> {
        if force == OrderForce::Unforced && self.current_action.is_some() {
            return Ok(Answer::NotTaken);
        }
        let Some(unit_seen) = Self::world_object(target, at, side)? else {
            return Ok(Answer::NotTaken);
        };
        self.set_action(Some(AiAction::Follow));
        self.target_at(Some(target), Some(unit_seen.class), unit_seen.position);
        self.desired_distance = range;
        if range.squared() < unit_seen.gap_squared {
            let seen = Seen {
                state: unit_seen.state,
                class: unit_seen.class,
                position: unit_seen.position,
                ..Seen::default()
            };
            self.move_to_object(
                target,
                (seen.state, seen.class, seen.position),
                range,
                side,
                rng,
            )?;
        }
        Ok(Answer::Taken)
    }

    /// Slot 0x110, the enter into object `target`, `force`d or without an action value (the
    /// base's `FUN_140c4d050`; the civilian's, `FUN_140c1c680`, first takes nothing the world
    /// does not hold): the unit's enter command (slot 0x3a0, [`UnitCommand::Enter`]), on its 1 the
    /// enter's action value (`0x269`, the last keeping the old) and the target at (−1, −1, −1)
    /// (`FUN_140c64c10`: its class when the world holds it).
    pub(super) fn enter_object(
        &mut self,
        target: ObjectId,
        force: OrderForce,
        (at, side): (&Surroundings, &mut UnitView<'_, '_>),
    ) -> GenieResult<Answer> {
        let held = Self::world_object(target, at, side)?;
        if self.ai_kind == UnitAiKind::Civilian && held.is_none() {
            return Ok(Answer::NotTaken);
        }
        if force == OrderForce::Unforced && self.current_action.is_some() {
            return Ok(Answer::NotTaken);
        }
        if side.command(UnitCommand::Enter(target))? == Answer::NotTaken {
            return Ok(Answer::NotTaken);
        }
        self.set_action(Some(AiAction::Enter));
        self.target_at(
            Some(target),
            held.map(|unit_seen| unit_seen.class),
            Position::from([-1.0; 3]),
        );
        Ok(Answer::Taken)
    }

    /// Slot 0x128 (`FUN_140c65400`), the transport to `point`, `force`d or without an action
    /// value: the unit's transport command (slot 0x3b8, [`UnitCommand::Transport`]), on its 1
    /// the transport's action value (`0x266`, the last keeping the old) and no target at the
    /// point (`FUN_140c64c10` with −1).
    pub(super) fn transport_to(
        &mut self,
        point: Position,
        force: OrderForce,
        side: &mut UnitView<'_, '_>,
    ) -> GenieResult<Answer> {
        if force == OrderForce::Unforced && self.current_action.is_some() {
            return Ok(Answer::NotTaken);
        }
        if side.command(UnitCommand::Transport(point))? == Answer::NotTaken {
            return Ok(Answer::NotTaken);
        }
        self.set_action(Some(AiAction::Transport));
        self.target_at(None, None, point);
        Ok(Answer::Taken)
    }

    /// Slots 0x118 (`FUN_140c660d0`) and, `over_wall`, 0x120 (`FUN_140c66330`): the unload at
    /// `point`, `force`d or without an action value: the unit's unload command at the point at
    /// its own height (slots 0x3a8 and 0x3b0, [`UnitCommand::Unload`]), on its 1 the unload's
    /// action value (`0x26d`; over a wall 700) with the last keeping the old, and the unit
    /// itself as the target, with its class and place.
    pub(super) fn unload_at(
        &mut self,
        point: Point,
        (force, over_wall): (OrderForce, bool),
        (at, side): (&Surroundings, &mut UnitView<'_, '_>),
    ) -> GenieResult<Answer> {
        if force == OrderForce::Unforced && self.current_action.is_some() {
            return Ok(Answer::NotTaken);
        }
        let to = point.with_height(at.position.z());
        if side.command(UnitCommand::Unload { to, over_wall })? == Answer::NotTaken {
            return Ok(Answer::NotTaken);
        }
        self.set_action(Some(if over_wall {
            AiAction::UnloadOverWall
        } else {
            AiAction::Unload
        }));
        self.set_target(Some(at.id), at.class, at.position);
        Ok(Answer::Taken)
    }

    /// Slot 0x180 (`FUN_140c64ab0`), seek and destroy object `target` from the tile `tile`,
    /// `force`d or without an action value: the unit's attack command on the target (slot 0x338,
    /// [`UnitAi::attack_command`]), on its 1 the seek's action value (`0x26e`, the last keeping
    /// the old) and the target at the tile (as floats) at height 1 (`FUN_140c64c10`).
    pub(super) fn seek_and_destroy(
        &mut self,
        target: ObjectId,
        tile: Tile,
        force: OrderForce,
        at: &Surroundings,
        side: &mut UnitView<'_, '_>,
        rng: &mut Well512,
    ) -> GenieResult<Answer> {
        let unit_ai_tuning = &at.unit_ai_tuning;
        if force == OrderForce::Unforced && self.current_action.is_some() {
            return Ok(Answer::NotTaken);
        }
        if self.attack_command(target, force, at, side, rng)? == Answer::NotTaken {
            return Ok(Answer::NotTaken);
        }
        self.set_action(Some(AiAction::SeekAndDestroy));
        let class = Self::world_object(target, at, side)?.map(|unit_seen| unit_seen.class);
        self.target_at(
            Some(target),
            class,
            Position::new(tile.x() as f32, tile.y() as f32, unit_ai_tuning.explore_z),
        );
        Ok(Answer::Taken)
    }

    /// Slot 0x188 (`FUN_140c4d9b0`), explore and destroy to the tile `tile`, `force`d or without
    /// an action value: the unit's explore there with `force` (slot 0x398, [`Effect::Explore`];
    /// unforced, an Explore on top keeps its point), which takes it, then the action value
    /// `0x26f` (the last keeping the old) and no target at the tile (as floats) at height 1.
    pub(super) fn explore_and_destroy(
        &mut self,
        tile: Tile,
        force: OrderForce,
        side: &mut UnitView<'_, '_>,
        rng: &mut Well512,
        unit_ai_tuning: &UnitAiTuning,
    ) -> GenieResult<Answer> {
        if force == OrderForce::Unforced && self.current_action.is_some() {
            return Ok(Answer::NotTaken);
        }
        let point = Point::new(tile.x() as f32, tile.y() as f32);
        side.effect_now(Effect::Explore { point, force }, self, rng)?;
        self.set_action(Some(AiAction::ExploreAndDestroy));
        self.target_at(None, None, point.with_height(unit_ai_tuning.explore_z));
        Ok(Answer::Taken)
    }

    /// Slot 0x230's case for the order on a target (`0x2db`, `0x325`): on a target the world holds,
    /// the unit's slot 0x128 on it at its place, then the tail with the order value the AI holds
    /// after it ("the order the work gave"), both at once on the unit ([`Effect::OrderOnTarget`],
    /// `World::order_on_target`), which ends the full update before its rest
    /// ([`UnitAi::ordered_on_target`]). On a gone target: without its type (`+0x30` −1), the move to
    /// the order's point within its range (slot 0x130, forced and walking) and, taken, the tail
    /// with the order on a target. With its type (the class of that master of the target's
    /// owner `+0x10`, `FUN_14152fe40`, [`UnitView::master_class`]; without an owner the type is
    /// the class), the unit AI's slot 0x1b0 gives a kind of resource
    /// ([`UnitAi::resource_kind`]) and slot 0x88 the closest object of it to the order's shown
    /// point (`+0x24`, `+0x28`), which the unit works (slot 0x7e8, [`AiCommand::WorkOn`]); either
    /// way the order is not taken (0).
    pub(super) fn order_on_target(
        &mut self,
        queued_order: &QueuedOrder,
        class: Option<ClassId>,
        at: &Surroundings,
        side: &mut UnitView<'_, '_>,
        ai: &mut Well512,
    ) -> GenieResult<OrderOutcome> {
        let roles = &at.roles;
        if Self::world_object(queued_order.target, at, side)?.is_some() {
            side.effect_now(Effect::OrderOnTarget(*queued_order), self, ai)?;
            self.ordered_on_target = true;
            return Ok(OrderOutcome::Taken);
        }
        if let Some(target_kind) = queued_order.target_kind {
            // the class of the target's type: its owner's master's, else the type itself
            let class = match queued_order.target_owner {
                None => target_kind.class(),
                Some(owner) => match side.master_class(owner, target_kind.master()) {
                    Some(class) => class,
                    None => return Ok(OrderOutcome::NotTaken),
                },
            };
            let Some(kind) = self.resource_kind(class, roles) else {
                return Ok(OrderOutcome::NotTaken);
            };
            // slot 0x88 (`FUN_140c4b0f0`): slot 0x80 from the order's shown point, without a
            // drop site, wood (0) for fewer than 2 workers, at any state
            let found = side.closest_resource(ResourceSearch {
                kind,
                cap: if kind == GatherGroup::WOOD {
                    WorkerCount::of_data(2)
                } else {
                    WorkerCount::NO_CAP
                },
                site: None,
                current: self.current_target,
                point: queued_order.shown,
                min_state: ObjectState::Foundation,
            })?;
            // the world holds what the search found: the move after a found object it does not
            // hold cannot run
            if let Some(gatherable) = found {
                side.effect_now(
                    Effect::Command(AiCommand::WorkOn {
                        target: gatherable.id,
                        point: gatherable.position,
                    }),
                    self,
                    ai,
                )?;
            }
            return Ok(OrderOutcome::NotTaken);
        }
        let Some(ai_move) = self.move_to(AiMove {
            point: queued_order.point,
            range: queued_order.range,
            force: OrderForce::Forced,
            pace: Pace::Walk,
        }) else {
            return Ok(OrderOutcome::NotTaken);
        };
        side.effect_now(Effect::Move(ai_move), self, ai)?;
        Ok(self.order_tail(
            Some(queued_order.order),
            queued_order.priority,
            class,
            TakenAs::Order,
            ai,
            roles,
        ))
    }

    /// Slot 0x1b0 by the unit AI's type: the kind of resource of a target class. The civilian's
    /// (`FUN_140c1c4f0`) and the building's (`FUN_140c1c460`, the same switch,
    /// [`civilian_kind`]); the fishing ship's (`FUN_140c1c580`): fish for classes 5, 31 and 33,
    /// gold for 63; the base's (`FUN_140c4cc00`) none. None for the byte 0xff.
    pub(super) fn resource_kind(&self, class: ClassId, roles: &Roles) -> Option<GatherGroup> {
        match self.ai_kind {
            UnitAiKind::Civilian | UnitAiKind::Building => civilian_kind(class, roles),
            UnitAiKind::Fishing => {
                let held = roles.of_class(class);
                if held.intersects(
                    ClassRoles::OCEAN_FISH | ClassRoles::DEEP_SEA_FISH | ClassRoles::SHORE_FISH,
                ) {
                    Some(GatherGroup::FISH)
                } else if held.contains(ClassRoles::GOLD_FISH) {
                    Some(GatherGroup::GOLD)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Slot 0x108 (`FUN_140c68a60`), with `force` (its argument 3) or without an action value:
    /// the unit's slot 0x388 on the foundation `target` ([`UnitView::wait_solidify`]); on its 1
    /// the action value becomes [`AiAction::WaitSolidify`], the old one the last. Taken when it
    /// waits.
    pub(super) fn wait_for_foundation(
        &mut self,
        target: ObjectId,
        force: OrderForce,
        side: &mut UnitView<'_, '_>,
    ) -> GenieResult<Answer> {
        if force == OrderForce::Unforced && self.current_action.is_some() {
            return Ok(Answer::NotTaken);
        }
        if side.wait_solidify(target)? == Answer::NotTaken {
            return Ok(Answer::NotTaken);
        }
        self.set_action(Some(AiAction::WaitSolidify));
        Ok(Answer::Taken)
    }

    /// Slot 0x100 (`FUN_140c4d910`), forced (its argument 4) or without an action value:
    /// the unit's explore to `tile`'s corner with `force` (slot 0x398, [`Effect::Explore`];
    /// unforced, an Explore on top keeps its point), which takes it; the AI then holds the
    /// explore's action value and the point without a target (`FUN_140c64c10` with −1, which
    /// leaves the target class), at height `UnitAiTuning::explore_z`. The unit AI's unit is a
    /// combat object (`TRIBE_Combat_Object` holds it), whose slot 0x398 always answers 1.
    pub(super) fn explore_at(
        &mut self,
        tile: Tile,
        force: OrderForce,
        side: &mut UnitView<'_, '_>,
        rng: &mut Well512,
        unit_ai_tuning: &UnitAiTuning,
    ) -> GenieResult<Answer> {
        if force == OrderForce::Unforced && self.current_action.is_some() {
            return Ok(Answer::NotTaken);
        }
        let point = tile.corner();
        side.effect_now(Effect::Explore { point, force }, self, rng)?;
        self.last_action = self.current_action.replace(AiAction::Explore);
        self.target_at(None, None, point.with_height(unit_ai_tuning.explore_z));
        Ok(Answer::Taken)
    }

    /// Slot 0x28 (`FUN_140c55c90`) with Immediate set and inFront clear: for a unit in state 2 or
    /// below and an order (not −1), [`UnitAi::order_now`] without keeping the current order.
    /// `place` is the unit's position (x, y) and master's class, for the stop; `ai` the AI
    /// stream, for the order's tail.
    ///
    /// # Errors
    ///
    /// An order whose slot reads the unit's surroundings, which this entry does not have.
    pub(crate) fn take_order_now(
        &mut self,
        unit_state: ObjectState,
        order: QueuedOrder,
        place: (Point, Option<ClassId>),
        ai: &mut Well512,
        side: &mut UnitView<'_, '_>,
        layer: AiLayer<'_>,
    ) -> GenieResult<()> {
        if ObjectState::Alive < unit_state {
            return Ok(());
        }
        let (position, class) = place;
        let now = OrderNow {
            order,
            keep_from: None,
        };
        self.order_now(now, OrderSite::Place(position, class), ai, side, layer)
    }

    /// Slot 0x28 (`FUN_140c55c90`) with Immediate set and inFront clear, from outside the unit
    /// AI's update with what it reads of its unit and the world (the surroundings `at`, the
    /// side, the owner's computer flag): for a unit in state 2 or below and an order (not −1),
    /// [`UnitAi::order_now`] without keeping the current order, the order run at once with
    /// them ([`UnitAi::process_order`]). `ai` is the AI stream.
    ///
    /// # Errors
    ///
    /// What the order's run does not model.
    pub(crate) fn take_order_now_with(
        &mut self,
        order: QueuedOrder,
        (at, side, player_kind): (&Surroundings, &mut UnitView<'_, '_>, PlayerKind),
        ai: &mut Well512,
    ) -> GenieResult<()> {
        let unit_ai_tuning = &at.unit_ai_tuning;
        let roles = &at.roles;
        if ObjectState::Alive < at.state {
            return Ok(());
        }
        let now = OrderNow {
            order,
            keep_from: None,
        };
        self.order_now(
            now,
            OrderSite::Surroundings(at, player_kind),
            ai,
            side,
            AiLayer {
                roles,
                slots: &at.slots,
                unit_ai_tuning,
            },
        )
    }
}

#[cfg(test)]
mod tests;
