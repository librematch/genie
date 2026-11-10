//! A unit's move, to a point or aside, a work order, its reach and a point's move order.

use crate::gathering::PastureId;
use crate::map::Distance;
use crate::map::Point;
use crate::map::Position;
use crate::map::spatial::Placed;
use crate::math::rng::Stream;
use crate::movement::Speed;
use crate::movement::ai_moves::MoveSprites;
use crate::movement::ai_moves::MoveWith;
use crate::movement::footprint_gap;
use crate::movement::move_to::{MoveToAction, point_sprite};
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::Master;
use crate::objects::masters::MasterKind;
use crate::objects::masters::OwnerMasters;
use crate::objects::masters::PlayersMastersExt;
use crate::objects::sprite::Cargo;
use crate::objects::sprite::Graphic;
use crate::orders::Dispatch;
use crate::orders::OrderFlags;
use crate::orders::ReadyOrders;
use crate::orders::{MoveOrder, Work, ignore_unit, packs, readiness, work_reach};
use crate::players::AllyTest;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::unit_ai::AiAction;
use crate::unit_ai::AiLayer;
use crate::unit_ai::Answer;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::OrderForce;
use crate::unit_ai::OrderKind;
use crate::unit_ai::QueueEnd;
use crate::unit_ai::task_finder::Diplomacy;
use crate::unit_ai::task_finder::world::TaskWorld;
use crate::unit_ai::{AiMove, Issuer, Notify, OrderPriority, OrderTargetKind, QueuedOrder};
use crate::world::World;
use crate::world::index::Place;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::slots::PlayerSlotsExt;
use crate::{GenieError, GenieResult};

/// The Pack (`TRIBE_Action_Pack`, `FUN_140ad4ff0`) or Unpack (`TRIBE_Action_Unpack`,
/// `FUN_140ae58a0`) a trebuchet's work order or attack command makes, neither held.
pub(crate) const PACK_OR_UNPACK: GenieError =
    GenieError::unsupported("a Pack or Unpack of class 54 or 51 (FUN_140ad4ff0, FUN_140ae58a0)");
/// An order's target without a place on the map.
const ORDER_TARGET_UNPLACED: GenieError = GenieError::unsupported("a target without a place");
/// A unit a move order moves without movement state.
pub(crate) const MOVED_UNMOVING: GenieError =
    GenieError::unsupported("a moved unit without movement state");

/// `FUN_1410862c0` of the unit at `me` of master `master` against the object at `them`
/// ([`work_reach`] of the footprint gap): within its reach, and too close.
pub(crate) fn placed_reach(me: &Placed, them: &Placed, master: &Master) -> (bool, bool) {
    let gap = footprint_gap(me.position, me.radius, them.position, them.radius);
    work_reach(
        gap,
        master.combat_part().weapon_range,
        master.combat_part().min_range,
    )
}

/// What a work order leaves of the pasture its unit tends ([`World::work_pasture`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum PastureAfter {
    /// The order ends there: no livestock was given.
    Ends,
    /// The unit tends this pasture, or none (`FUN_141189ca0` with −1: it leaves the one it
    /// tends).
    Tends(Option<PastureId>),
}

impl World {
    /// One unit of MOVE: an object that does not move ignores an order not queued. The unit's
    /// ignore list empties (`FUN_141197da0`), it waits to move (slot 0x2a0 with 1) and lists the
    /// order's `others` (`FUN_14118c790`). An order that is queued, or neither direct nor to a
    /// unit AI with every queued order ready (`FUN_140c4dfa0`), goes to its unit AI's queue as
    /// order `0x2c6` at its point (slot 0x30, or in a grid slot 0x28: range 0, priority 100, the
    /// order's point shown, ready when direct); a unit without a unit AI then does nothing.
    /// Otherwise the unit takes the move itself (slot 0x138) to the point, or to its grid `cell`.
    pub(super) fn move_unit(
        &mut self,
        player: PlayerId,
        id: ObjectId,
        others: &[ObjectId],
        cell: Option<Point>,
        order: &MoveOrder,
    ) -> GenieResult<()> {
        let point2 = cell.unwrap_or(order.point);
        let Some(place) = self.place(id) else {
            return Ok(());
        };
        let Place::Live(owner, idx) = place else {
            return Err(GenieError::unsupported(
                "a move of a unit the slice does not hold live",
            ));
        };
        // the command's check (`FUN_140e553f0`, [`World::checked_units`]) dropped another
        // player's unit (its id -1) unless the game setting `+0x1074` lets players order each
        // other's units ([`crate::world::settings::Settings::cheats_enabled`]): such a unit takes the
        // move as its own player's, the issuer only queuing it
        let issuer = player;
        let player = owner;
        let master = self
            .players
            .live(player, idx)?
            .master(self.master_tables(), player)?;
        // slot 0xe8: the master's speed (`+0x740`), before a sprite's factor
        let speed = master.animated_part().max_speed;
        if master.kind < MasterKind::MOVING || speed <= Speed::ZERO {
            if order.flags.contains(OrderFlags::QUEUED) {
                return Err(GenieError::unsupported(
                    "a queued move of an object that does not move",
                ));
            }
            return Ok(());
        }
        let target = self
            .object(order.target)
            .map(|(tp, unit)| (unit.life.master_id, tp));
        let unit = self.players.live_mut(player, idx)?;
        // FUN_141197da0, then slot 0x2a0 with 1: it waits to move
        let mover = self.parts.mover_mut(unit.id).ok_or(MOVED_UNMOVING)?;
        mover.ignore_units.clear();
        mover.waiting = 1;
        for &other in others {
            ignore_unit(&mut mover.ignore_units, other);
        }
        let takes_it = !order.flags.contains(OrderFlags::QUEUED)
            && self.parts.unit_ai.get(unit.id).is_none_or(|ai| {
                order.flags.contains(OrderFlags::DIRECT) || !ai.has_unready_order()
            });
        if !takes_it {
            let state = unit.life.state;
            let Some(ai) = self.parts.unit_ai.get_mut(unit.id) else {
                return Ok(());
            };
            let (target_kind, target_owner) =
                target.map_or((None, None), |(master_id, player_id)| {
                    (Some(OrderTargetKind::of_master(master_id)), Some(player_id))
                });
            let queued = QueuedOrder {
                issuer: Some(Issuer::Player(issuer)),
                order: OrderKind::Move,
                priority: OrderPriority::COMMAND,
                target: if target.is_some() { order.target } else { None },
                target_owner,
                point: point2.with_height(0.0),
                range: Distance::ZERO,
                shown: order.point,
                ready: order.flags.contains(OrderFlags::DIRECT),
                target_kind,
            };
            let _ = ai.queue_order(state, queued, Dispatch::Queued, QueueEnd::Back)?;
            if !order.flags.contains(OrderFlags::QUEUED)
                && !order.flags.contains(OrderFlags::DIRECT)
            {
                readiness(&mut ai.orders, ReadyOrders::Drop);
                // the pass's tail: the unit's stop (its slot 0x148)
                self.object_stop(player, idx)?;
            }
            return Ok(());
        }
        // slot 0x138's start (FUN_141216f10), then FUN_14106d340
        self.move_start(player, idx)?;
        if order.target == Some(id) {
            return Ok(());
        }
        if order.target.is_some() {
            return Err(GenieError::unsupported("a move that follows an object"));
        }
        let order = &MoveOrder {
            point: point2,
            ..*order
        };
        let taken = self.move_to_point(player, idx, order.point.with_height(0.0))?;
        // the order handler's notify: a unit that took the move as a MoveTo gets the move order
        // and action, its target forgotten (FUN_140c4dcb0); every unit then the point
        // (FUN_140c64be0); in a grid every unit gets the order, the action and its cell, its
        // target kept; slot 0x2d8's walk and show targets are not held
        let unit = self.players.live_mut(player, idx)?;
        if let Some(ai) = self.parts.unit_ai.get_mut(unit.id) {
            let ai_stream = self.streams.get_mut(Stream::Ai);
            match cell {
                None => ai.notify_move(taken == Answer::NotTaken, None, order.point, ai_stream),
                Some(point) => ai.notify_grid_move(point, ai_stream),
            }
        }
        Ok(())
    }

    /// `FUN_14106d340` on a move of the unit at `idx` to `point`: a unit with a unit AI
    /// (`FUN_140c5dc10` clears its notifies with the world time) lets its head action take the
    /// point ([`World::head_takes_move`]) unless it stops after a kill (`FUN_140c41c10`); else a
    /// `MoveTo` to the point replaces its actions and its command count goes one up
    /// ([`World::count_command`]). Taken when the action took it.
    pub(crate) fn move_to_point(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        point: Position,
    ) -> GenieResult<Answer> {
        // a unit with a unit AI (FUN_140c5dc10 clears its notifies with the world time) lets its
        // head action's slot 0x40 take the point unless it stops after a kill (FUN_140c41c10)
        let world_time = self.world_time;
        let class = self
            .players
            .live(player, idx)?
            .master(self.master_tables(), player)?
            .class;
        let unit = self.players.live_mut(player, idx)?;
        let place = (unit.position().ground(), Some(class));
        if self.parts.unit_ai.contains(unit.id) {
            // a completion's stop (slot 0xb0 with 1): the unit's stopAction at once
            let (roles, slots) = (self.model.roles.clone(), self.model.slots);
            let unit_ai_tuning = self.model.tuning.unit_ai;
            self.run_ai_calls((player, idx), |ai, side, rng| {
                let layer = AiLayer {
                    roles: &roles,
                    slots: &slots,
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
        let taken = if self.parts.unit_ai.contains(unit.id) && !stop_after_kill {
            self.head_takes_move(player, idx, None, point)?
        } else {
            Answer::NotTaken
        };
        if taken == Answer::NotTaken {
            // the MoveTo to the point (FUN_140aa0d30: range 0, the walking sprite) and its
            // install (slot 0x808: the MoveTo's slot 0x70, FUN_140a78050, does nothing; the list
            // clears, then the start)
            self.clear_actions(player, idx)?;
            let sprite = self.point_move_sprite(player, idx)?;
            let top = self.top_speed(player, self.players.live(player, idx)?)?;
            let unit = self.players.live_mut(player, idx)?;
            let move_to = MoveToAction::to_point(point, Distance::ZERO, sprite, top);
            if move_to.shows_standing() {
                unit.shows_standing_sprite(
                    self.parts.motion.layer_mut(unit.id),
                    OwnerMasters::new(&self.parts.own_master, self.model.masters.of(player)),
                    &self.model.graphics,
                    self.streams.get_mut(Stream::Sprite),
                    self.model.tuning.sprite,
                )?;
            }
            let id = unit.id;
            self.parts.actions_mut(id)?.list.set_move_to(Some(move_to));
            self.count_command(id)?;
        }
        Ok(taken)
    }

    /// The sprite (`+0x58`) of the unit at `idx`'s `MoveTo` to a point whose maker gives none
    /// ([`point_sprite`] with the unit's terrain look; a trade cart's goods are not held, so it
    /// carries none).
    ///
    /// [`point_sprite`]: crate::movement::move_to::point_sprite
    pub(crate) fn point_move_sprite(
        &self,
        player: PlayerId,
        idx: ListIndex,
    ) -> GenieResult<Option<Graphic>> {
        let unit = &self.players.live(player, idx)?;
        let master = unit.master(self.master_tables(), player)?;
        let look = self.parts.combat_of(unit.id).abilities.look.as_ref();
        Ok(point_sprite(master, look, Cargo::Empty, &self.model.roles)
            .and_then(|id| self.model.graphics.get(id).copied()))
    }

    /// The unit's slot 0x340 from its unit AI's move off a footprint (slot 0x148,
    /// [`crate::objects::unit::Unit::ai_move_with`]); then what its actions let go: a counted builder
    /// leaves its building ([`World::leave_builders`]), the targets go ([`World::release`]).
    pub(crate) fn move_aside(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        ai_move: AiMove,
    ) -> GenieResult<()> {
        let unit = &self.players.live(player, idx)?;
        let class = unit.master(self.master_tables(), player)?.class;
        let by = self.releaser(player, idx)?;
        let id = self.players.live(player, idx)?.id;
        let top_speed = self.top_speed_terms(player, unit)?;
        let (mut releases, mut builder_leaves) = (Vec::new(), Vec::new());
        let sails = self.players.live_mut(player, idx)?.ai_move_with(
            (self.parts.actions.of_mut(id)?, self.parts.unread.get(id)),
            self.parts.motion.layer_mut(id),
            ai_move,
            (class, player),
            (&mut releases, &mut builder_leaves, by),
            (
                MoveSprites {
                    masters: OwnerMasters::new(
                        &self.parts.own_master,
                        self.model.masters.of(player),
                    ),
                    graphics: &self.model.graphics,
                    look: self.parts.combat.of(id).abilities.look.as_ref(),
                    roles: &self.model.roles,
                    sprite_tuning: self.model.tuning.sprite,
                    move_to_tuning: &self.model.tuning.move_to,
                    top_speed,
                },
                self.streams.get_mut(Stream::Sprite),
            ),
        )?;
        if let MoveWith::Sails(transport_action) = sails {
            self.transport_move((player, idx), transport_action, ai_move.point)?;
        }
        for id in builder_leaves {
            self.leave_builders(Some(id))?;
        }
        for (id, by) in releases {
            self.release(id, by)?;
        }
        Ok(())
    }

    /// Slot 0x7e8 of a unit on a target object: an action object's (types 40 to 60) is the work
    /// command (`FUN_14106ff70`, [`World::work_command`]). A combat object's or a building's
    /// (`FUN_141227190`): a unit in state 3 or later, or of
    /// class 54 under its own Pack or Unpack (`0x7d`, `0x7c`), ignores it. Against a target the
    /// unit is not allied with (`FUN_141663d90` with allies) [`work_reach`] (`FUN_1410862c0`)
    /// decides: class 54 out of reach packs (`FUN_140ad4ff0`), class 51 in reach unpacks
    /// (`FUN_140ae58a0`), each through slot 0x808; else a target below the least range is the
    /// unit's notify [`NotifyKind::TOO_CLOSE`] to its own unit AI (slot 0x328) and nothing more.
    /// Without a target the world holds (none, or one gone) the work command goes to the point, and
    /// a computer's class 54 packs instead. Neither action is held. A villager drops its town-bell
    /// flag (`FUN_1412067f0`); then the work command. A villager on a pasture (1893, 1897) whose
    /// livestock's workers (slot 0x780,
    /// [`ObjectsRef::livestock_workers`](crate::world::objects_ref::ObjectsRef::livestock_workers)) are below its owner's
    /// [`AttributeSlots::pasture_workers`](crate::world::layer::AttributeSlots::pasture_workers)
    /// (COMISS, JBE) works the livestock [`World::pasture_pick`] gives at its place with the
    /// order's flags (slot 0x7e8 again) and then tends the pasture; none given ends the order
    /// there; a full pasture has its unit AI take a search (slot 0x38: 0x202 with the hunt action,
    /// class 49 and −1). Any other unit leaves the pasture it tends ([`World::tend_pasture`],
    /// `FUN_141189ca0`).
    pub(crate) fn order_work(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        order: &Work,
    ) -> GenieResult<()> {
        let unit = &self.players.live(player, idx)?;
        let master = unit.master(self.master_tables(), player)?;
        // an action object's slot 0x7e8 is the work command itself (FUN_14106ff70)
        if master.kind < MasterKind::COMBAT {
            return self.work_command(player, idx, order);
        }
        // the class-54 test of its top action comes first in the game, but either way the order
        // is ignored
        if ObjectState::Alive < unit.life.state {
            return Ok(());
        }
        let class = master.class;
        let target = self.object(order.target).map(|(tp, _)| tp);
        let reach = self.order_reach(player, unit.id, order, target, master)?;
        let packs = packs(
            class,
            reach,
            target.is_none(),
            self.players.player(player)?.kind,
            &self.model.roles,
        );
        // a class 54 under an action the slice does not hold may be under its Pack or Unpack
        if packs
            || (self
                .model
                .roles
                .of_class(class)
                .contains(ClassRoles::UNPACKED_TREBUCHET)
                && self.parts.unread.contains(unit.id))
        {
            return Err(PACK_OR_UNPACK);
        }
        if reach.is_some_and(|(_, too_close)| too_close) {
            let unit = self.players.live_mut(player, idx)?;
            unit.post_own(
                &mut self.parts.unit_ai,
                NotifyKind::TOO_CLOSE,
                [ObjectId::field(order.target), -1, -1],
            )?;
            return Ok(());
        }
        if target.is_none() {
            // no target, or one the world does not hold (slot 0x7e8 with none): the work command
            // at the point (FUN_1412067f0's town bell reads no target)
            let order = Work {
                target: None,
                ..*order
            };
            self.work_command(player, idx, &order)?;
            // FUN_141189ca0 with −1: the unit leaves the pasture it tends
            return self.tend_pasture(player, idx, None);
        }
        self.work_on_held(player, idx, order)
    }

    /// [`World::order_work`] on a target the world holds: livestock (class 58) takes the enter
    /// command (slot 0x3a0), and where it enters nothing the work command goes to the target's
    /// place without a target; the town bell (`FUN_1412067f0`, which reads no target), the work
    /// command and the pasture ([`World::work_pasture`]).
    fn work_on_held(&mut self, player: PlayerId, idx: ListIndex, order: &Work) -> GenieResult<()> {
        let master = self
            .players
            .live(player, idx)?
            .master(self.master_tables(), player)?;
        let mut order = *order;
        if self
            .model
            .roles
            .of_class(master.class)
            .contains(ClassRoles::LIVESTOCK)
            && let Some(target) = order.target
            && self.enter_command(player, idx, target, OrderForce::Unforced)? == Answer::NotTaken
        {
            let at = self
                .spatial
                .get_known(target)?
                .ok_or(ORDER_TARGET_UNPLACED)?
                .position;
            order.point = order.point.with_x(at.x()).with_y(at.y());
            order.target = None;
        }
        if let Some(target) = order.target {
            self.town_bell_order(player, idx, target, order.filter)?;
        }
        self.work_command(player, idx, &order)?;
        match self.work_pasture(player, idx, &order)? {
            PastureAfter::Ends => Ok(()),
            PastureAfter::Tends(pasture) => self.tend_pasture(player, idx, pasture),
        }
    }

    /// [`World::order_work`]'s reach against `target` (the world's player holding the order's
    /// target) when the unit `id` is not allied with it: [`work_reach`] of the gap between the
    /// two's places; none against an ally's target or none.
    pub(super) fn order_reach(
        &self,
        player: PlayerId,
        id: ObjectId,
        order: &Work,
        target: Option<PlayerId>,
        master: &Master,
    ) -> GenieResult<Option<(bool, bool)>> {
        let world = TaskWorld {
            players: &self.players,
            treaty: self.treaty.as_ref(),
            drop_sites: &self.model.drop_sites,
        };
        Ok(match target {
            Some(tp) if !world.allied(player, tp, AllyTest::WithRelation4) => {
                let (me, them) = (
                    self.spatial
                        .get(id)
                        .ok_or(GenieError::unsupported("an ordered unit without a place"))?,
                    self.spatial
                        .get_known(order.target)?
                        .ok_or(ORDER_TARGET_UNPLACED)?,
                );
                Some(placed_reach(me, them, master))
            }
            _ => None,
        })
    }

    /// [`World::order_work`]'s pasture after the work command ([`PastureAfter`]). A villager ordered
    /// onto a pasture (1893, 1897) with room for workers works the livestock
    /// [`World::pasture_pick`] gives and tends the pasture; a full one has its unit AI take a
    /// search.
    pub(super) fn work_pasture(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        order: &Work,
    ) -> GenieResult<PastureAfter> {
        use crate::technology::Attributes;
        let unit = &self.players.live(player, idx)?;
        let master = unit.master(self.master_tables(), player)?;
        let Some(on) = order.target else {
            return Ok(PastureAfter::Tends(None));
        };
        let target = self.task_target(on, 0)?;
        let pasture = self
            .model
            .roles
            .of_class(master.class)
            .contains(ClassRoles::VILLAGER)
            && target.master.kind == MasterKind::BUILDING
            && self
                .model
                .roles
                .of_master(target.master.id)
                .intersects(MasterRoles::PASTURE_MANGROVE | MasterRoles::PASTURE);
        if !pasture {
            return Ok(PastureAfter::Tends(None));
        }
        let on = PastureId::new(on);
        let room = Attributes::get(
            self.players.player(player)?,
            self.model.slots.attribute.pasture_workers,
        )?;
        let workers = self.objects().livestock_workers(on).as_f32();
        // COMISS room with the workers, JBE: a NaN is full
        if room > Amount::new(workers) {
            let Some((pick, point)) = self.pasture_pick(on, player, idx)? else {
                return Ok(PastureAfter::Ends);
            };
            let on_it = Work {
                target: Some(pick),
                point,
                ..*order
            };
            self.order_work(player, idx, &on_it)?;
            return Ok(PastureAfter::Tends(Some(on)));
        }
        // its unit AI's slot 0x38 takes a search (0x202 with the hunt action, class 49 and −1,
        // from and to the unit)
        let unit = self.players.live_mut(player, idx)?;
        let (id, state) = (unit.id, unit.life.state);
        if let Some(ai) = self.parts.unit_ai.get_mut(unit.id) {
            let search = Notify::own(
                id,
                NotifyKind::REQUIRES_SEARCH,
                [
                    AiAction::Hunt.value(),
                    i32::from(self.model.slots.class.farm.get()),
                    -1,
                ],
            );
            let _ = ai.accept(search, state);
        }
        Ok(PastureAfter::Tends(None))
    }

    /// Slot 0x138 without a target: its start ([`World::move_start`]: a villager's town-bell
    /// flag, the order queue by the unit's `+0xa14`), then the move to the point
    /// ([`World::move_to_point`], `FUN_14106d340`). Taken when an action of the unit took it.
    ///
    /// # Errors
    ///
    /// What the move does not model.
    pub(crate) fn move_point_order(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        point: Position,
    ) -> GenieResult<Answer> {
        self.move_start(player, idx)?;
        self.move_to_point(player, idx, point)
    }
}
