//! An order's processing: the combat and work orders, the top target and the attack slots.

use crate::GenieResult;
use crate::construction::FoundationId;
use crate::map::Point;
use crate::math::rng::Well512;
use crate::movement::footprint_gap_squared;
use crate::objects::ObjectId;
use crate::objects::ObjectKey;
use crate::objects::masters::ClassId;
use crate::players::PlayerKind;
use crate::unit_ai::AiAction;
use crate::unit_ai::AiLayer;
use crate::unit_ai::Answer;
use crate::unit_ai::OrderForce;
use crate::unit_ai::OrderKind;
use crate::unit_ai::OrderOutcome;
use crate::unit_ai::OrderPriority;
use crate::unit_ai::StopOrder;
use crate::unit_ai::TargetRules;
use crate::unit_ai::UnitAiKind;
use crate::unit_ai::world_view::UnitView;
use crate::unit_ai::{
    Effect, LiveTop, OUTSIDE_ORDER, QueuedOrder, Surroundings, UnitAi, UnitCommand, UnitSeen,
    order_move,
};
use crate::world::UNIT_AI_MASTER;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;

/// How [`UnitAi::process_order`]'s case took its order: as the order itself, or as an attack
/// (`bVar2`: the attack move's attack), which the tail ([`UnitAi::order_tail`]) makes the
/// attack order (700).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TakenAs {
    /// As the order.
    Order,
    /// As an attack.
    Attack,
}

/// Where an order runs ([`UnitAi::order_now`], [`UnitAi::process_order`]): beside the unit AI's
/// update, with the unit's surroundings and its owner's type, or from a command outside it
/// ([`UnitAi::take_order_now`]), with only the unit's place.
#[derive(Clone, Copy, Debug)]
pub(crate) enum OrderSite<'at> {
    /// The unit's surroundings and its owner's type.
    Surroundings(&'at Surroundings, PlayerKind),
    /// The unit's position (x, y) and its master's class.
    Place(Point, Option<ClassId>),
}

impl<'at> OrderSite<'at> {
    /// The unit's surroundings and its owner's type, which only [`OrderSite::Surroundings`]
    /// holds.
    pub(crate) const fn world(self) -> Option<(&'at Surroundings, PlayerKind)> {
        match self {
            Self::Surroundings(at, player_kind) => Some((at, player_kind)),
            Self::Place(..) => None,
        }
    }

    /// The unit's position (x, y) and its master's class, which the stop reads.
    pub(crate) const fn place(self) -> (Point, Option<ClassId>) {
        match self {
            Self::Surroundings(at, _) => (at.position.ground(), at.class),
            Self::Place(position, class) => (position, class),
        }
    }
}

impl UnitAi {
    /// Slot 0x230 (`FUN_140c5c170`, every type's) on the order `queued_order`: one not ready
    /// returns 0xe. Each case runs its slot, forced (its force argument 1); a slot that does not
    /// take the order returns 0 (the queue drops it), one that does goes to the tail
    /// ([`UnitAi::order_tail`]), which makes it the AI's order with its priority, and 1:
    /// - 700, the attack: `AttackObject` by the type ([`UnitAi::attack_slot`], any target, the
    ///   group flag set).
    /// - `0x2bd`, the defend: a unit of master 128 takes none; without a target slot 0x178 at the
    ///   order's point ([`UnitAi::defend_point`]), else slot 0x170 on the target
    ///   ([`UnitAi::defend_object`]), within the order's range.
    /// - `0x2be`, the build: slot 0xf0 on the foundation and the tail, which the caller runs
    ///   ([`Effect::OrderOnTarget`], `World::build_order`).
    /// - `0x2bf`, `0x2c0`, `0x2ce`, the heal, conversion and repair: slots 0xe0, 0xd8 and 0xe8
    ///   ([`UnitAi::work_object`]) through the unit's commands ([`UnitCommand`]).
    /// - `0x2c1`, the explore, `0x2da`, the scout, and `0x321`, the auto scout: slot 0x100 to the
    ///   order's point cut to tiles (`CVTTSS2SI`, [`UnitAi::explore_at`]); whatever it gave, the
    ///   scout then holds the action value `0x276` and the auto scout `0x2bd`, the one before the
    ///   last.
    /// - `0x2c2`, the stop: the target goes (`FUN_140c5e1e0`), the action stops (slot 0xb0 with
    ///   1), and the tail ends the order.
    /// - `0x2c3`, run away: slot 0x160 with 1 and no unit ([`UnitAi::run_away`]).
    /// - `0x2c5`, the gather: slot 0xd0 ([`UnitAi::gather_object`]).
    /// - `0x2c6`, the move, and `0x2d4`, the group gather: slot 0x130 to the order's point within
    ///   its range, forced and walking (a move's unit then shows its points, slot 0x2d8, not
    ///   held).
    /// - `0x2c7`, the patrol ("I don't know what to do with patrol orders"), and an order no case
    ///   names: 0.
    /// - `0x2c8`, the follow: slot 0x168 on the target within the order's range
    ///   ([`UnitAi::follow_object`]).
    /// - `0x2c9`, the hunt, and `0x322`, the auto boar luring: slot 0xc8 ([`UnitAi::hunt_object`]);
    ///   whatever it gave, the luring then holds the action value `0x2be`, the one before the
    ///   last.
    /// - `0x2ca`, the transport: slot 0x128 to the order's point ([`UnitAi::transport_to`]).
    /// - `0x2cd`, the enter: slot 0x110 on the target ([`UnitAi::enter_object`]).
    /// - `0x2d1` and 800, the unloads: slots 0x118 and 0x120 at the order's point
    ///   ([`UnitAi::unload_at`]).
    /// - `0x2d2`, seek and destroy: slot 0x180 on the target at the order's point cut to tiles
    ///   ([`UnitAi::seek_and_destroy`]).
    /// - `0x2d3`, explore and destroy: slot 0x188 to the order's point cut to tiles
    ///   ([`UnitAi::explore_and_destroy`]).
    /// - `0x2d8`, the wait: the action value `0x274` and the order `0x2d8` (leaving a patrol
    ///   order as the tail does).
    /// - `0x2d9`, the attack roundup: slot 0xc0 on the target ([`UnitAi::attack_roundup`]); the
    ///   tail makes the order 700.
    /// - `0x2db` and `0x325`, the order on a target ([`UnitAi::order_on_target`]).
    /// - `0x2dd`, the attack move: a unit of class 51 or 54 with a target runs slot 0xb8 on it (the
    ///   tail makes the order 700), any other slot 0x130 to the order's point.
    /// - `0x2de`, the foundation's: slot 0x108 on the target
    ///   ([`UnitAi::wait_for_foundation`]); whatever it gave, the action value `0x27a`, the one
    ///   before the last.
    ///
    /// `site` is the unit's surroundings and its owner's type, which the full update and the
    /// queue between turns pass; [`UnitAi::take_order_now`] passes only the unit's place (its x
    /// and y and its master's class, [`OrderSite::Place`]), as its own order (`AUTO_SCOUT`'s
    /// stop or auto scout order) is the one that runs.
    #[expect(
        clippy::too_many_lines,
        reason = "slot 0x230 (`FUN_140c5c170`) ported as one function, one case per order in the game's order"
    )]
    pub(crate) fn process_order(
        &mut self,
        queued_order: &QueuedOrder,
        site: OrderSite<'_>,
        ai: &mut Well512,
        side: &mut UnitView<'_, '_>,
        layer: AiLayer<'_>,
    ) -> GenieResult<OrderOutcome> {
        let AiLayer {
            roles,
            unit_ai_tuning,
            ..
        } = layer;
        if !queued_order.ready {
            return Ok(OrderOutcome::NotReady);
        }
        let world = site.world();
        let place = site.place();
        let (_, class) = place;
        // `bVar2`: the attack move's attack, whose tail makes the order 700
        let mut taken_as = TakenAs::Order;
        let taken = match queued_order.order {
            OrderKind::Attack
            | OrderKind::Defend
            | OrderKind::RunAway
            | OrderKind::SeekAndDestroy
            | OrderKind::AttackRoundup
            | OrderKind::AttackMove => {
                self.combat_order(queued_order, world, ai, side, &mut taken_as, roles)?
            }
            OrderKind::Build
            | OrderKind::Heal
            | OrderKind::Convert
            | OrderKind::Repair
            | OrderKind::Gather
            | OrderKind::Hunt
            | OrderKind::AutoBoarLuring
            | OrderKind::Foundation => self.work_order(queued_order, world, side, ai)?,
            OrderKind::Follow
            | OrderKind::Transport
            | OrderKind::Enter
            | OrderKind::Unload
            | OrderKind::UnloadOverWall => self.object_move_order(queued_order, world, side, ai)?,
            OrderKind::Explore => self.explore_at(
                queued_order.point.tile(),
                OrderForce::Forced,
                side,
                ai,
                unit_ai_tuning,
            )?,
            OrderKind::Stop => {
                self.remove_target();
                self.stop_with(StopOrder::Keep, place, side, ai, roles)?;
                Answer::Taken
            }
            OrderKind::Move | OrderKind::GroupGather => {
                match self.move_to(order_move(queued_order)) {
                    Some(ai_move) => {
                        side.effect_now(Effect::Move(ai_move), self, ai)?;
                        Answer::Taken
                    }
                    None => Answer::NotTaken,
                }
            }
            OrderKind::ExploreAndDestroy => self.explore_and_destroy(
                queued_order.point.tile(),
                OrderForce::Forced,
                side,
                ai,
                unit_ai_tuning,
            )?,
            OrderKind::Wait => {
                // case 0x2d8: the wait's action value, leaving a patrol order, then the order
                // itself; the tail finds it the AI's already
                self.current_action = Some(AiAction::Wait);
                self.leave_patrol_timers(ai);
                self.current_order = Some(OrderKind::Wait);
                Answer::Taken
            }
            OrderKind::Scout => {
                let explored = self.explore_at(
                    queued_order.point.tile(),
                    OrderForce::Forced,
                    side,
                    ai,
                    unit_ai_tuning,
                )?;
                self.set_action(Some(AiAction::Scout));
                explored
            }
            OrderKind::OnTarget | OrderKind::OnTarget2 => {
                let (at, _) = world.ok_or(OUTSIDE_ORDER)?;
                return self.order_on_target(queued_order, class, at, side, ai);
            }
            OrderKind::AutoScout => {
                let explored = self.explore_at(
                    queued_order.point.tile(),
                    OrderForce::Forced,
                    side,
                    ai,
                    unit_ai_tuning,
                )?;
                self.set_action(Some(AiAction::AutoScout));
                explored
            }
            // the patrol order (0x2c7) and the orders the switch has no case for
            _ => Answer::NotTaken,
        };
        if taken == Answer::NotTaken {
            return Ok(OrderOutcome::NotTaken);
        }
        Ok(self.order_tail(
            Some(queued_order.order),
            queued_order.priority,
            class,
            taken_as,
            ai,
            roles,
        ))
    }

    /// [`UnitAi::process_order`]'s combat cases: the attack, the defence (a master that cannot
    /// defend takes none: the switch's `return 0`, which is the tail's "not taken"), the run away,
    /// seek and destroy, the attack roundup and the attack move (its attack [`TakenAs::Attack`]).
    pub(super) fn combat_order(
        &mut self,
        queued_order: &QueuedOrder,
        world: Option<(&Surroundings, PlayerKind)>,
        ai: &mut Well512,
        side: &mut UnitView<'_, '_>,
        taken_as: &mut TakenAs,
        roles: &Roles,
    ) -> GenieResult<Answer> {
        let (at, player_kind) = world.ok_or(OUTSIDE_ORDER)?;
        // an order on no target (−1): each case's lookup finds nothing, so it takes no order
        Ok(match (queued_order.order, queued_order.target) {
            (OrderKind::Attack, Some(target)) => {
                self.attack_slot(target, TargetRules::Any, at, side, ai)?
            }
            (OrderKind::Defend, target) => {
                if roles.of_master(at.master).contains(MasterRoles::DEFENDLESS) {
                    return Ok(Answer::NotTaken);
                }
                match target {
                    None => self.defend_point(
                        queued_order.point.ground(),
                        queued_order.range,
                        OrderForce::Forced,
                        at,
                    ),
                    Some(object_id) => self.defend_object(
                        object_id,
                        queued_order.range,
                        OrderForce::Forced,
                        at,
                        side,
                    )?,
                }
            }
            (OrderKind::RunAway, _) => {
                self.run_away(OrderForce::Forced, at, player_kind, ai, side)?
            }
            (OrderKind::SeekAndDestroy, Some(target)) => {
                let tiles = queued_order.point.tile();
                self.seek_and_destroy(target, tiles, OrderForce::Forced, at, side, ai)?
            }
            (OrderKind::AttackRoundup, Some(object)) => {
                self.attack_roundup(object, at, side, ai)?
            }
            (OrderKind::AttackMove, target) => {
                if let Some(object_id) = target
                    && roles
                        .of_some_class(at.class)
                        .contains(ClassRoles::ATTACK_MOVE_ATTACKS)
                {
                    *taken_as = TakenAs::Attack;
                    self.attack_slot(object_id, TargetRules::Any, at, side, ai)?
                } else {
                    match self.move_to(order_move(queued_order)) {
                        Some(ai_move) => {
                            side.effect_now(Effect::Move(ai_move), self, ai)?;
                            Answer::Taken
                        }
                        None => Answer::NotTaken,
                    }
                }
            }
            _ => Answer::NotTaken,
        })
    }

    /// [`UnitAi::process_order`]'s work cases: the build (slot 0xf0, `FUN_140c47d80`, with 1 on
    /// the target; the tail makes the order the AI's with its priority), the heal, convert and
    /// repair, the gather, the hunt and auto boar luring (the luring then holds the action value
    /// `0x2be`) and the foundation's wait (then the action value `0x27a`).
    pub(super) fn work_order(
        &mut self,
        queued_order: &QueuedOrder,
        world: Option<(&Surroundings, PlayerKind)>,
        side: &mut UnitView<'_, '_>,
        rng: &mut Well512,
    ) -> GenieResult<Answer> {
        let (at, _) = world.ok_or(OUTSIDE_ORDER)?;
        // an order on no target (−1): each case's lookup finds nothing, so it takes no order;
        // the hunt's stops the unit first (`hunt_object`)
        Ok(match (queued_order.order, queued_order.target) {
            (OrderKind::Build, Some(target)) => {
                self.build_object(FoundationId::new(target), OrderForce::Forced, at, side, rng)?
            }
            (OrderKind::Heal | OrderKind::Convert | OrderKind::Repair, Some(target)) => {
                let (action, command) = match queued_order.order {
                    OrderKind::Heal => (AiAction::Heal, UnitCommand::Heal(target)),
                    OrderKind::Convert => (AiAction::Convert, UnitCommand::Convert(target)),
                    _ => (AiAction::Repair, UnitCommand::Repair(target)),
                };
                self.work_object(target, OrderForce::Forced, (action, command), (at, side))?
            }
            (OrderKind::Gather, Some(object)) => {
                self.gather_object(object, OrderForce::Forced, at, side, rng)?
            }
            (OrderKind::Hunt | OrderKind::AutoBoarLuring, target) => {
                let hunts = self.hunt_object(target, OrderForce::Forced, at, side, rng)?;
                if queued_order.order == OrderKind::AutoBoarLuring {
                    self.set_action(Some(AiAction::BoarLuring));
                }
                hunts
            }
            (OrderKind::Foundation, target) => {
                let waits = match target {
                    Some(object_id) => {
                        self.wait_for_foundation(object_id, OrderForce::Forced, side)?
                    }
                    None => Answer::NotTaken,
                };
                self.set_action(Some(AiAction::WaitSolidify));
                waits
            }
            _ => Answer::NotTaken,
        })
    }

    /// [`UnitAi::process_order`]'s moves onto an object or a point of one: the follow, the
    /// transport, the enter and the two unloads.
    pub(super) fn object_move_order(
        &mut self,
        queued_order: &QueuedOrder,
        world: Option<(&Surroundings, PlayerKind)>,
        side: &mut UnitView<'_, '_>,
        rng: &mut Well512,
    ) -> GenieResult<Answer> {
        let (at, _) = world.ok_or(OUTSIDE_ORDER)?;
        // an order on no target (−1): its lookup finds nothing, so it takes no order
        Ok(match (queued_order.order, queued_order.target) {
            (OrderKind::Follow, Some(target)) => self.follow_object(
                target,
                queued_order.range,
                OrderForce::Forced,
                at,
                side,
                rng,
            )?,
            (OrderKind::Transport, _) => {
                self.transport_to(queued_order.point, OrderForce::Forced, side)?
            }
            (OrderKind::Enter, Some(object_id)) => {
                self.enter_object(object_id, OrderForce::Forced, (at, side))?
            }
            (OrderKind::Unload | OrderKind::UnloadOverWall, _) => {
                let over_wall = queued_order.order == OrderKind::UnloadOverWall;
                self.unload_at(
                    queued_order.point.ground(),
                    (OrderForce::Forced, over_wall),
                    (at, side),
                )?
            }
            _ => Answer::NotTaken,
        })
    }

    /// The order's tail (`LAB_140c5d432`) once its case took order value `order` of priority
    /// `priority`: the stop (`0x2c2`) leaves a patrol order ([`UnitAi::leave_patrol_timers`]) and
    /// ends the order and its priority (−1). The attack roundup (`0x2d9`) and the attack move's
    /// attack ([`TakenAs::Attack`]) leave a patrol order and become the attack order (700); the
    /// attack order of a unit of class 18 (`class`, its master's) leaves a patrol order, its target
    /// goes (`+0x48` −1, its class and point kept) and the order is −1. Any other becomes the
    /// order, leaving a patrol order for another (`LAB_140c5d611`). The priority is the order's but
    /// for the stop. Returns 1.
    pub(crate) fn order_tail(
        &mut self,
        order: Option<OrderKind>,
        priority: OrderPriority,
        class: Option<ClassId>,
        taken_as: TakenAs,
        ai: &mut Well512,
        roles: &Roles,
    ) -> OrderOutcome {
        if order == Some(OrderKind::Stop) {
            self.leave_patrol_timers(ai);
            self.current_order = None;
            self.order_priority = OrderPriority::NONE;
            return OrderOutcome::Taken;
        }
        let new = if order == Some(OrderKind::AttackRoundup) || taken_as == TakenAs::Attack {
            self.leave_patrol_timers(ai);
            Some(OrderKind::Attack)
        } else if order == Some(OrderKind::Attack)
            && roles.of_some_class(class).contains(ClassRoles::MONK)
        {
            self.leave_patrol_timers(ai);
            self.current_target = None;
            None
        } else {
            if self.current_order != order {
                self.leave_patrol_timers(ai);
            }
            order
        };
        self.current_order = new;
        self.order_priority = priority;
        OrderOutcome::Taken
    }

    /// Slot 400 as the full update stands: the target of the unit's top action now
    /// ([`UnitView::top_target_now`]: the update's stops and commands ran at once), its type and
    /// the footprints' squared gap (`FUN_14118f8c0`), as the update read it at its start
    /// ([`Surroundings::top_target`]) while it is that object, else as the world holds it
    /// ([`UnitAi::world_object`]).
    ///
    /// # Errors
    ///
    /// An object the side does not hold.
    pub(super) fn top_target_live(
        at: &Surroundings,
        side: &UnitView<'_, '_>,
    ) -> GenieResult<Option<LiveTop>> {
        let Some(id) = side.top_target_now() else {
            return Ok(None);
        };
        if let Some(top_target) = at.top_target.filter(|top_target| top_target.id == id) {
            return Ok(Some(LiveTop {
                id,
                kind: top_target.kind,
                gap_squared: footprint_gap_squared(
                    at.position,
                    at.size,
                    top_target.position,
                    top_target.radius,
                ),
            }));
        }
        Ok(Self::world_object(id, at, side)?.map(|unit_seen| LiveTop {
            id,
            kind: unit_seen.kind,
            gap_squared: unit_seen.gap_squared,
        }))
    }

    /// Slot 400 within the unit's range (slot 0x1e8, `FUN_141086330`): its top action's target
    /// as the update stands ([`UnitAi::top_target_live`]) when the footprints' squared gap is at
    /// most the weapon range squared and at least the least range squared (`COMISS`/`JC` each:
    /// a NaN is not in range).
    ///
    /// # Errors
    ///
    /// An object the side does not hold.
    pub(super) fn top_target_in_range(
        at: &Surroundings,
        side: &UnitView<'_, '_>,
    ) -> GenieResult<Option<ObjectId>> {
        let Some(live_top) = Self::top_target_live(at, side)? else {
            return Ok(None);
        };
        let gap = live_top.gap_squared;
        let in_range = gap <= at.weapon_range.squared() && at.min_range.squared() <= gap;
        Ok(in_range.then_some(live_top.id))
    }

    /// The object `id` as the world holds it (`FUN_1415417a0`): the unit itself (`at`, its own
    /// target of no one), among the objects the update names ([`Surroundings::unit`]), else as
    /// the unit's side finds it ([`UnitView::seen_unit`]); None when the world does not hold it
    /// (or for none, as the game's table finds nothing for −1).
    pub(super) fn world_object(
        id: impl ObjectKey,
        at: &Surroundings,
        side: &UnitView<'_, '_>,
    ) -> GenieResult<Option<UnitSeen>> {
        let Some(id) = id.key() else {
            return Ok(None);
        };
        if id == at.id {
            return Ok(Some(UnitSeen {
                id,
                class: at.class.ok_or(UNIT_AI_MASTER)?,
                owner: at.player,
                combat_level: at.combat_level,
                state: at.state,
                target: None,
                gap_squared: footprint_gap_squared(at.position, at.size, at.position, at.size),
                position: at.position,
                hp: at.hp,
                master: at.master,
                kind: at.kind,
                main: None,
                allied: true,
            }));
        }
        at.unit(id)
            .map_or_else(|| side.seen_unit(id), |unit_seen| Ok(Some(*unit_seen)))
    }

    /// `AttackObject` (slot 0xb8) forced, by the unit AI's type, on `target` by `rules`: the
    /// civilian's (`FUN_140c17370`, [`UnitAi::civilian_attack`]: an animal or a tree it hunts, any
    /// other it attacks by its own rules); the soldier's (`FUN_140c17550`) the base's, then a
    /// member of a group the world holds stops moving ([`UnitView::group_attack_stop`]); the
    /// others' the base's ([`UnitAi::attack_object`]). The priest's (`FUN_140c17490`) asks its slot
    /// 0x1b8 (`FUN_140c1af00`) whether to convert the target first: not modelled, the base's stops
    /// at the priest's slot 0xa0, which is not either.
    pub(super) fn attack_slot(
        &mut self,
        target: ObjectId,
        rules: TargetRules,
        at: &Surroundings,
        side: &mut UnitView<'_, '_>,
        rng: &mut Well512,
    ) -> GenieResult<Answer> {
        match self.ai_kind {
            UnitAiKind::Civilian => self.civilian_attack(target, at, side, rng),
            UnitAiKind::Soldier => {
                if self.attack_object(target, OrderForce::Forced, rules, at, side, rng)?
                    == Answer::NotTaken
                {
                    return Ok(Answer::NotTaken);
                }
                if at.group.is_some() {
                    side.group_attack_stop()?;
                }
                Ok(Answer::Taken)
            }
            _ => self.attack_object(target, OrderForce::Forced, rules, at, side, rng),
        }
    }

    /// `attackRoundupObject` (slot 0xc0, `FUN_140c43aa0`) on `target`: the AI's own target
    /// ("I'm already attacking that target") or one the world does not hold takes nothing; then
    /// slot 0xa0 by its own rules ([`UnitAi::attack_check`]) must let it attack the target; with an
    /// action value the unit must reach the target within its range (slot 0x3d0,
    /// [`UnitView::reaches`]: "can't path to new target"). Then the unit's attack command, forced
    /// ([`UnitAi::attack_command`]), and on success the action is the attack (600, the last action
    /// keeping the old) and the target its class and place (`FUN_140c64c10`). Unlike
    /// `AttackObject`, `+0x1d5` stays.
    pub(super) fn attack_roundup(
        &mut self,
        target: ObjectId,
        at: &Surroundings,
        side: &mut UnitView<'_, '_>,
        rng: &mut Well512,
    ) -> GenieResult<Answer> {
        if self.current_target == Some(target) {
            return Ok(Answer::NotTaken);
        }
        let Some((class, position)) = self.attack_check(target, TargetRules::Own, at, side)? else {
            return Ok(Answer::NotTaken);
        };
        if self.current_action.is_some()
            && !side.reaches(target, at.weapon_range, self.current_target)?
        {
            return Ok(Answer::NotTaken);
        }
        if self.attack_command(target, OrderForce::Forced, at, side, rng)? == Answer::NotTaken {
            return Ok(Answer::NotTaken);
        }
        self.set_action(Some(AiAction::Attack));
        self.target_at(Some(target), Some(class), position);
        Ok(Answer::Taken)
    }
}
