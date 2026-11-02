//! The work command's Transport: a transport's task of type 12 (a beach, the only terrain task
//! in the game's data) makes or retakes it.

use crate::GenieError;
use crate::garrison::transport::{Changed, MapShore, Ship, TRANSPORT, TransportAction};
use crate::map::Position;
use crate::math::rng::Stream;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::masters::ClassId;
use crate::objects::masters::class;
use crate::objects::masters::ids::MARKET;
use crate::objects::masters::{Task, TaskId};
use crate::players::PlayerId;
use crate::unit_ai::ActionKind;
use crate::unit_ai::OrderKind;
use crate::world::World;
use crate::world::slots::PlayerSlots;

/// The trade cart's and the trade boat's classes (`FUN_141195400`, `FUN_141195490`).
const TRADE_CLASSES: [ClassId; 2] = [class::TRADE_CART, class::TRADE_BOAT];

impl World {
    /// The work command's Transport (its task of action type 12, the factory `FUN_140aca4b0`
    /// case 0xc: [`TransportAction::on_task`] at the work's `point`); the unit AI's notify
    /// (`FUN_14106e4e0` kind 0xc: the transport order 0x2ca and action 0x266, the target with
    /// its class, none for a point, the work's point); the install (slot 0x808, `FUN_14106e990`)
    /// clears the action list, then the list's add starts it ([`TransportAction::start`] with 1).
    ///
    /// # Errors
    ///
    /// A map without its terrain, and what the actions' teardowns do not model.
    pub(crate) fn start_transport(
        &mut self,
        (p, i): (PlayerId, ListIndex),
        (target, point): (Option<ObjectId>, Position),
        task: &Task,
        clears: bool,
    ) -> Result<(), GenieError> {
        self.notify_transport((p, i), (target, point), clears)?;
        self.clear_actions(p, i)?;
        let mut t = TransportAction::on_task(Some(task.id), point);
        let changed = self.with_shore(p, i, t.task, |ship, shore| t.start(true, ship, shore))?;
        self.parts
            .actions_mut(self.players.live(p, i)?.id)?
            .list
            .set_transport(Some(t));
        self.transport_changed(p, i, changed)
    }

    /// The work command's retake (`LAB_1410706a8`) of the Transport on top on the `task` it
    /// found (its `+0x48`): the top action's slot 0x48 ([`TransportAction::work`] at the work's
    /// point, from the unit itself when the target is the unit), then the unit AI's notify as
    /// [`World::start_transport`]'s. A trade cart or boat (classes 19, 2) ordered to its own
    /// player's market (84) is never retaken. Answers whether the Transport kept the work.
    ///
    /// # Errors
    ///
    /// A map without its terrain, and what the map reads give.
    ///
    /// [`World`]: crate::world::World
    pub(crate) fn retake_transport(
        &mut self,
        (p, i): (PlayerId, ListIndex),
        (target, point): (Option<ObjectId>, Position),
        task: &Task,
        clears: bool,
    ) -> Result<bool, GenieError> {
        let unit = self.players.live(p, i)?;
        let on_task = self.parts.actions_of(unit.id).top_action_kind() == Some(TRANSPORT)
            && self
                .parts
                .actions_of(unit.id)
                .list
                .transport()
                .copied()
                .and_then(|t| t.task)
                == Some(task.id);
        if !on_task || self.trade_at_own_market((p, i), target)? {
            return Ok(false);
        }
        let by_self = target == Some(unit.id);
        let mut t = self
            .parts
            .actions_of(unit.id)
            .list
            .transport()
            .copied()
            .ok_or(GenieError::unreachable(
                "a Transport on top without its action",
            ))?;
        let changed = self.with_shore(p, i, t.task, |ship, shore| {
            t.work(point, by_self, ship, shore)
        })?;
        self.parts
            .actions_mut(self.players.live(p, i)?.id)?
            .list
            .set_transport(Some(t));
        self.transport_changed(p, i, changed)?;
        self.notify_transport((p, i), (target, point), clears)?;
        Ok(true)
    }

    /// `LAB_1410706a8`'s exception: the unit is a trade cart or boat (`FUN_141195400`,
    /// `FUN_141195490`) and the target its own player's market.
    fn trade_at_own_market(
        &self,
        (p, i): (PlayerId, ListIndex),
        target: Option<ObjectId>,
    ) -> Result<bool, GenieError> {
        let Some((tp, t)) = self.object(target) else {
            return Ok(false);
        };
        // The market (`0x60f0` raw), whose own trade units the work command never retakes.
        let market = tp == p && t.master(self.master_tables(), tp)?.id == MARKET;
        let class = self
            .players
            .live(p, i)?
            .master(self.master_tables(), p)?
            .class;
        Ok(market && TRADE_CLASSES.contains(&class))
    }

    /// The unit AI's notify of the work command's Transport (`FUN_14106e4e0` kind 0xc): the
    /// transport order and action, the target with its class (none for a point or a target the
    /// world does not hold) and the work's point.
    fn notify_transport(
        &mut self,
        (p, i): (PlayerId, ListIndex),
        (target, point): (Option<ObjectId>, Position),
        clears: bool,
    ) -> Result<(), GenieError> {
        let named = match target.and_then(|id| Some((id, self.object(id)?))) {
            Some((target, (tp, t))) => Some((target, t.master(self.master_tables(), tp)?.class)),
            None => None,
        };
        let id = self.players.live(p, i)?.id;
        if let Some(ai) = self.parts.unit_ai.get_mut(id) {
            ai.notify_work(
                Some(OrderKind::Transport),
                Some(ActionKind::Transport),
                named,
                point,
                clears,
                self.streams.get(Stream::Ai),
            )?;
            self.remember_order(
                p,
                i,
                (Some(OrderKind::Transport), Some(ActionKind::Transport)),
            )?;
        }
        Ok(())
    }

    /// `f` on the view of the Transport on `task` of player `p`'s object at `i` ([`World::ship`])
    /// and of the map ([`MapShore`] with the unit's terrain restriction).
    ///
    /// [`World`]: crate::world::World
    fn with_shore(
        &mut self,
        p: PlayerId,
        i: ListIndex,
        task: Option<TaskId>,
        f: impl FnOnce(&Ship<'_>, &mut MapShore<'_>) -> Result<Changed, GenieError>,
    ) -> Result<Changed, GenieError> {
        let ship = {
            let unit = self.players.live(p, i)?;
            let master = unit.master(self.master_tables(), p)?;
            Ship::of_unit(
                (unit, self.parts.mover_of(unit.id)),
                master,
                self.spatial.map,
                (
                    &self.graphics,
                    self.parts.combat_of(unit.id).abilities.look.as_ref(),
                ),
                task,
            )?
        };
        let restriction = self
            .players
            .live(p, i)?
            .master(self.master_tables(), p)?
            .terrain_restriction;
        let mut shore = MapShore::new(
            &self.spatial,
            &mut self.zones,
            &mut self.pathing,
            restriction,
            ship.position,
        )?;
        f(&ship, &mut shore)
    }
}
