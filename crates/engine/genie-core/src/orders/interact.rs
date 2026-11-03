//! An interact order's unit, an order on a target, MOVE and its groups.

use crate::GenieError;
use crate::groups::FormationKind;
use crate::groups::common_group;
use crate::groups::{CommandArgs, GroupId};
use crate::map::Distance;
use crate::map::Point;
use crate::math::rng::Stream;
use crate::movement::Speed;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::masters::ActionKind;
use crate::objects::masters::MasterKind;
use crate::orders::Dispatch;
use crate::orders::{
    Interact, LISTED, MoveOrder, Place, Split, Work, grid_point, ignore_unit, offset_point,
    readiness, set_differentiator_groups,
};
use crate::players::PlayerId;
use crate::unit_ai::OrderKind;
use crate::unit_ai::QueueEnd;
use crate::unit_ai::{Issuer, OrderPriority, OrderTargetType, QueuedOrder};
use crate::world::World;
use crate::world::layer::ClassRoles;
use crate::world::slots::PlayerSlots;

/// What the loop over the splitter's groups takes of its order ([`World::order_groups`]).
struct GroupsOrder {
    /// The order's target object, none for a point.
    target: Option<ObjectId>,
    /// The order's point.
    point: Point,
    /// The command each group takes at the end.
    args: CommandArgs,
    /// The command a queued order's new group first takes on its first unit's target.
    first: FirstTarget,
    /// Ctrl locks the new group's formation to a line.
    lock_line: bool,
}

/// How a queued order's first command on its first unit's target differs from the order's: its
/// task filter, and the retargeting byte the handler makes of the unit AI's (`+0x1d6` is 0:
/// MOVE passes it as is, INTERACT its opposite).
struct FirstTarget {
    filter: Option<ActionKind>,
    disable_retargeting: fn(bool) -> bool,
}

impl World {
    /// One unit of INTERACT: an object that does not move takes the order at once (slot 0x128,
    /// [`World::order_at_once`]) and, the order queued, goes on as a mover does. Its ignore list empties
    /// (`FUN_141197da0`), it waits to move (slot 0x2a0 with 1) and lists the order's other units
    /// (`FUN_14118c790`). With a centroid its point is its offset from it added to the order's,
    /// kept `EDGE` inside the map. A unit without a unit AI, or of an order that is not queued and
    /// either direct (`+0x14`) or to a unit AI with every queued order ready (`FUN_140c4dfa0`),
    /// takes the order itself (slot 0x128); any other unit AI queues order `0x2db` on the target at
    /// the unit's point (slot 0x28, or without a centroid slot 0x30, which shows the point it
    /// queues: priority 100, the order's point shown, ready when direct); after an order neither
    /// queued nor direct the readiness pass with 1 and the unit's stop ([`readiness`]).
    pub(super) fn interact_unit(
        &mut self,
        player: PlayerId,
        id: ObjectId,
        others: &[ObjectId],
        centroid: Option<Point>,
        order: &Interact,
    ) -> Result<(), GenieError> {
        let Some(place) = self.place(id) else {
            return Ok(());
        };
        let Place::Live(p, idx) = place else {
            return Err(GenieError::unsupported(
                "an order to a unit the slice does not hold live",
            ));
        };
        // the command's check (`FUN_140e55250`, [`World::checked_units`]) dropped another
        // player's unit (its id -1) unless the game setting `+0x1074` lets players order each
        // other's units ([`crate::world::settings::Settings::cheats_enabled`]); the lookup itself has
        // no owner test (`FUN_140e6b750`): such a unit takes the order as its own player's, the
        // issuer only queuing it
        let issuer = player;
        let player = p;
        let master = self
            .players
            .live(player, idx)?
            .master(self.master_tables(), player)?;
        let speed = master.move_stats.as_ref().map_or(Speed::ZERO, |m| m.speed);
        let still = master.kind < MasterKind::MOVING
            || (!self
                .model
                .roles
                .of_class(master.class)
                .contains(ClassRoles::UNPACKED_TREBUCHET)
                && speed <= Speed::ZERO);
        if still {
            // an object that does not move takes slot 0x128 at once with 1 (no carried-resource
            // test, any task) at the order's point, z 0; a queued order then goes on as a mover's
            self.order_at_once(player, idx, order.target, order.point)?;
            if !order.queued {
                return Ok(());
            }
        }
        let target = self
            .object(order.target)
            .map(|(tp, t)| (t.life.master_id, tp));
        let map = self.spatial.map;
        let unit = self.players.live_mut(player, idx)?;
        // its ignore list empties, it waits to move and lists the others; an object that does
        // not move holds no movement state, which only a walk reads
        match self.parts.mover_mut(unit.id) {
            Some(mover) => {
                mover.ignore_units.clear();
                mover.waiting = 1;
                for &other in others {
                    ignore_unit(&mut mover.ignore_units, other);
                }
            }
            None if still => {}
            None => {
                return Err(GenieError::unsupported(
                    "an ordered unit without movement state",
                ));
            }
        }
        let point = centroid.map_or_else(
            || order.point,
            |c| {
                offset_point(
                    unit.position().ground(),
                    c,
                    order.point,
                    map,
                    &self.model.tuning.order,
                )
            },
        );
        let takes_it = self
            .parts
            .unit_ai
            .get(unit.id)
            .is_none_or(|ai| !order.queued && (order.direct || !ai.has_unready_order()));
        if takes_it {
            // slot 0x128 with 1 (FUN_141211b80): `+0xa14` is 1 and a unit AI's order queue
            // empties (FUN_140c5e010); FUN_14106c590 passes the order on unless the target is the
            // unit itself (and counts `+0x888`, not held)
            if let Some(ai) = self.parts.unit_ai.get_mut(unit.id) {
                ai.orders.clear();
            }
            if order.target == Some(id) {
                return Ok(());
            }
            let at = point.with_height(0.0);
            let work = Work {
                target: order.target,
                point: at,
                carry_test: order.carry_test,
                filter: order.filter,
                notified: true,
            };
            return self.order_work(player, idx, &work);
        }
        let (target_type, target_owner) = target.map_or((None, None), |(m, o)| {
            (Some(OrderTargetType::of_master(m)), Some(o))
        });
        let queued = QueuedOrder {
            issuer: Some(Issuer::Player(issuer)),
            order: OrderKind::OnTarget,
            priority: OrderPriority::COMMAND,
            target: if target.is_some() { order.target } else { None },
            target_owner,
            point: point.with_height(0.0),
            range: Distance::ZERO,
            shown: order.point,
            ready: order.direct,
            target_type,
        };
        let state = unit.life.state;
        let ai = self
            .parts
            .unit_ai
            .get_mut(unit.id)
            .ok_or(GenieError::unreachable("unreachable: no unit AI"))?;
        let _ = ai.queue_order(state, queued, Dispatch::Queued, QueueEnd::Back)?;
        if !order.queued && !order.direct {
            readiness(&mut ai.orders, true);
            // the pass's tail: the unit's stop (its slot 0x148)
            self.object_stop(player, idx)?;
        }
        Ok(())
    }

    /// Slot 0x128 with 1 (`FUN_141211b80`) on `target` at `point` (z 0), no carried-resource test
    /// and any task: `+0xa14` is 1 and a unit AI's order queue empties (`FUN_140c5e010`);
    /// `FUN_14106c590` passes the order on to slot 0x7e8 unless the target is the unit itself
    /// (its `+0x888` count is not held).
    pub(super) fn order_at_once(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: Option<ObjectId>,
        point: Point,
    ) -> Result<(), GenieError> {
        let unit = self.players.live_mut(player, idx)?;
        if let Some(ai) = self.parts.unit_ai.get_mut(unit.id) {
            ai.orders.clear();
        }
        if target == Some(unit.id) {
            return Ok(());
        }
        let work = Work {
            target,
            point: point.with_height(0.0),
            carry_test: false,
            filter: None,
            notified: true,
        };
        self.order_work(player, idx, &work)
    }

    /// Slot 0x128 (`FUN_141211b80`) of object `id` with `work`: `+0xa14` takes `work.notified`
    /// while the command runs, and with it set the unit AI's order queue empties
    /// (`FUN_140c5e010`); `FUN_14106c590` passes the order on to slot 0x7e8 unless the target
    /// is the unit itself (its `+0x888` count is not held). Nothing for an object the world
    /// does not hold live.
    ///
    /// # Errors
    ///
    /// What the work command does not model.
    pub(crate) fn give_work(&mut self, id: ObjectId, work: &Work) -> Result<(), GenieError> {
        let Some(Place::Live(player, idx)) = self.place(id) else {
            return Ok(());
        };
        if work.notified
            && let Some(ai) = self.parts.unit_ai.get_mut(id)
        {
            ai.orders.clear();
        }
        if work.target == Some(id) {
            return Ok(());
        }
        self.order_work(player, idx, work)
    }

    /// The unit AI's order on a target (`FUN_140c5c170` case `0x2db`, from its queue): the unit's
    /// slot 0x128 on the target at its place with 0 — its `+0xa14` clear while the work command
    /// runs, so the new action's notify leaves the AI's notifies, and its order queue stays —
    /// with no carried-resource test and no filter; `FUN_14106c590` passes it on unless the target
    /// is the unit itself (its `+0x888` count is not held). Then the tail at `LAB_140c5d432` with
    /// the order the work gave the AI ([`crate::unit_ai::UnitAi::order_tail`]: the order's
    /// priority; a stop order ends, a class-18 unit's attack order forgets its target and ends).
    /// The unit AI runs the case on a target that is gone itself.
    pub(crate) fn order_on_target(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        order: QueuedOrder,
    ) -> Result<(), GenieError> {
        let (_, t) = self.object(order.target).ok_or(GenieError::unsupported(
            "a queued order on a target that is gone",
        ))?;
        let point = t.position();
        if order.target != Some(self.players.live(player, idx)?.id) {
            let work = Work {
                target: order.target,
                point,
                carry_test: false,
                filter: None,
                notified: false,
            };
            self.order_work(player, idx, &work)?;
        }
        let class = self
            .players
            .live(player, idx)?
            .master(self.master_tables(), player)?
            .class;
        let ai = self
            .parts
            .unit_ai
            .get_mut(self.players.live(player, idx)?.id)
            .ok_or(GenieError::unsupported(
                "an order on a target without a unit AI",
            ))?;
        // the tail (`LAB_140c5d432`) with the order the work left the AI
        let _ = ai.order_tail(
            ai.current_order,
            order.priority,
            Some(class),
            false,
            self.streams.get(Stream::Ai),
            &self.model.roles,
        );
        Ok(())
    }

    /// MOVE (`FUN_140e67800`) to its units, after their unit AIs reset. One unit takes the order
    /// at the point (the loop of `LAB_140e6838c`). Several go through the splitter
    /// ([`World::loose_units`]); those of a human player whose box ([`centroid`]'s) is narrow
    /// enough each take a cell of a grid around the point ([`grid_point`], the loop after the
    /// box), any others the point itself, in the splitter's order, each listing the others in its
    /// ignore list ([`World::move_unit`]).
    ///
    /// [`centroid`]: crate::orders::points::centroid
    pub(crate) fn move_order(
        &mut self,
        player: PlayerId,
        units: &[Option<ObjectId>],
        order: &MoveOrder,
    ) -> Result<(), GenieError> {
        // the groups take the command count, which then counts up whatever the units do
        let loose = match units {
            [] => {
                self.groups.count_command();
                return Ok(());
            }
            &[id] if !order.control => {
                self.groups.count_command();
                // a gone unit (the check's none) leaves no group and takes no move
                let Some(id) = id else {
                    return Ok(());
                };
                self.leave_groups(player, &[id], order.target)?;
                return self.move_unit(player, id, &[], None, order);
            }
            _ => {
                let loose = self
                    .split(units, &|_| true, order.control)
                    .and_then(|mut s| self.move_groups(&mut s, order).map(|()| s.loose));
                self.groups.count_command();
                let loose = loose?;
                self.leave_groups(player, &loose, order.target)?;
                loose
            }
        };
        // FUN_140d61870: a player whose lobby entry is of type 4, which a human's is not
        if 1 < loose.len() && self.players.player(player)?.kind.is_computer() {
            return Err(GenieError::unsupported(
                "a move of several units of a computer player (FUN_140d61870)",
            ));
        }
        let narrow = 1 < loose.len() && self.order_spread(&loose, order.queued)?.is_some();
        let map = self.spatial.map;
        for (i, &id) in loose.iter().enumerate() {
            let others: Vec<ObjectId> = loose
                .iter()
                .enumerate()
                .filter(|&(j, _)| j != i)
                .map(|(_, &u)| u)
                .collect();
            let cell = narrow
                .then(|| grid_point(i, loose.len(), order.point, map, &self.model.tuning.order));
            self.move_unit(player, id, &others, cell, order)?;
        }
        Ok(())
    }

    /// MOVE's groups (`FUN_140e67800`'s loop over the splitter's groups): see
    /// [`World::order_groups`], with Ctrl's formation lock and no task filter.
    pub(super) fn move_groups(
        &mut self,
        split: &mut Split,
        order: &MoveOrder,
    ) -> Result<(), GenieError> {
        let groups = GroupsOrder {
            target: order.target,
            point: order.point,
            args: CommandArgs {
                queued: order.queued,
                direct: order.direct,
                disable_retargeting: order.control,
                filter: None,
            },
            first: FirstTarget {
                filter: None,
                disable_retargeting: |retargets| retargets,
            },
            lock_line: order.control,
        };
        self.order_groups(split, &groups)
    }

    /// INTERACT's groups (`FUN_140e68b90`'s loop over the splitter's groups): see
    /// [`World::order_groups`]; the order's bytes `+0x12` and `+0x16` go to the group's command
    /// and a queued order's first unit's target takes the opposite retargeting byte.
    pub(super) fn interact_groups(
        &mut self,
        split: &mut Split,
        order: &Interact,
    ) -> Result<(), GenieError> {
        let groups = GroupsOrder {
            target: order.target,
            point: order.point,
            args: CommandArgs {
                queued: order.queued,
                direct: order.direct,
                disable_retargeting: order.carry_test,
                filter: order.filter,
            },
            first: FirstTarget {
                filter: order.filter,
                disable_retargeting: |retargets| !retargets,
            },
            lock_line: false,
        };
        self.order_groups(split, &groups)
    }

    /// The loop over the splitter's groups that MOVE and INTERACT share: the group all of a
    /// split group's units are in (`FUN_14152ece0`, [`crate::groups::common_group`]) takes the
    /// order when it holds just them, the splitter keeping it ([`World::keep_group`], with the
    /// target the world holds); one the world does not hold takes nothing; else a new group of
    /// the split's owner (`FUN_14152efc0`) takes them ([`World::add_to_group`]), its statistics
    /// ([`World::group_statistics`]) and its place ([`World::set_initial_position`]). Each listed
    /// group (60 at most) takes the command count as its differentiator, with Ctrl held locks
    /// its formation to a line ([`World::lock_formation`] with 2) and takes the command
    /// ([`World::group_command`]); a new group of a queued order first takes a command on its
    /// first unit's target ([`World::first_unit_target`]), not queued and direct. Then every
    /// listed group lists them all as its differentiator groups (`setDifferentiatorGroups`,
    /// `FUN_1410395a0`).
    fn order_groups(&mut self, split: &mut Split, order: &GroupsOrder) -> Result<(), GenieError> {
        let mut listed: Vec<GroupId> = Vec::new();
        for units in split.groups.clone() {
            let units = &units[..];
            let common = common_group(units, |id| self.object(id).map(|(_, u)| u.group));
            // the units' common group, kept when it holds them all
            let kept = match common {
                None => None,
                Some(common) => match self.groups.get(common) {
                    None => continue,
                    Some(g) => (g.members.len() == units.len()).then_some(common),
                },
            };
            let new = kept.is_none();
            let id = if let Some(common) = kept {
                // FUN_140e6b820's target: the order's object while the world holds it
                let held = order.target.filter(|&t| self.world_object(t).is_some());
                self.keep_group(split, common, held)?;
                common
            } else {
                let owner = split.owner;
                let id = self.groups.create(owner);
                for &unit in units {
                    let _ = self.add_to_group(id, unit)?;
                }
                self.group_statistics(id, false)?;
                self.set_initial_position(id)?;
                id
            };
            if listed.len() < LISTED {
                listed.push(id);
            }
            let commands = self.groups.commands;
            if let Some(g) = self.groups.get_mut(id) {
                g.differentiator_id = commands;
            }
            if order.lock_line {
                self.lock_formation(id, FormationKind::LINE)?;
            }
            // a queued order's new group first takes its first unit's target, not queued and
            // direct
            if new
                && order.args.queued
                && let Some((target, at, retargets)) = self.first_unit_target(units)
            {
                let args = CommandArgs {
                    queued: false,
                    direct: true,
                    disable_retargeting: (order.first.disable_retargeting)(retargets),
                    filter: order.first.filter,
                };
                let _ = self.group_command(id, Some(target), at, args)?;
            }
            let _ = self.group_command(id, order.target, order.point, order.args)?;
        }
        for &id in &listed {
            if let Some(g) = self.groups.get_mut(id) {
                set_differentiator_groups(g, &listed);
            }
        }
        Ok(())
    }

    /// The target a queued MOVE's new group takes first (`FUN_140e67800` at
    /// `0x140e67b5d`..`0x140e67cb1`): the first of `units` the world holds, when it has a unit
    /// AI, names its target (`+0x48`, `FUN_140c4cc40`); the object the world holds for it
    /// (`FUN_1415417a0`) gives its id and its place (x, y). The unit AI's retarget flag
    /// (`FUN_140c4ee90`: `+0x1d6` is 0) is the command's seventh argument, as the game passes it.
    pub(super) fn first_unit_target(&self, units: &[ObjectId]) -> Option<(ObjectId, Point, bool)> {
        let (_, first) = units.iter().find_map(|&id| self.object(id))?;
        let ai = self.parts.unit_ai.get(first.id)?;
        let t = self.world_object(ai.current_target)?;
        Some((t.id(), t.position().ground(), ai.retarget_disabled == 0))
    }
}
