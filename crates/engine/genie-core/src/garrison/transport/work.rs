//! The work command's Transport: a transport's task of type 12 (a beach, the only terrain task
//! in the game's data) makes or retakes it.

use crate::GenieResult;
use crate::garrison::transport::TransportTarget;
use crate::garrison::transport::{Changed, MapShore, Ship, TransportAction};
use crate::map::Position;
use crate::math::rng::Stream;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::actions::Action;
use crate::objects::masters::Task;
use crate::players::PlayerId;
use crate::unit_ai::AiAction;
use crate::unit_ai::Answer;
use crate::unit_ai::Notifies;
use crate::unit_ai::OrderKind;
use crate::world::World;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::slots::PlayerSlotsExt;

impl World {
    /// The work command's Transport (its task of action type 12, the factory `FUN_140aca4b0`
    /// case 0xc: [`TransportAction::on_task`] at the work's `point`); the unit AI's notify
    /// (`FUN_14106e4e0` kind 0xc: the transport order 0x2ca and action 0x266, the target with
    /// its class, none for a point, the work's point); the install (slot 0x808,
    /// [`World::set_only_action`]) clears the action list, then the list's add starts it
    /// ([`TransportAction::start`] with 1), or deletes it for a ship in state 3 or later.
    ///
    /// # Errors
    ///
    /// A map without its terrain, and what the actions' teardowns do not model.
    pub(crate) fn start_transport(
        &mut self,
        (player_id, list_index): (PlayerId, ListIndex),
        (target, point): (Option<ObjectId>, Position),
        task: &Task,
        notifies: Notifies,
    ) -> GenieResult<()> {
        self.notify_transport((player_id, list_index), (target, point), notifies)?;
        let transport_action = TransportAction::on_task(Some(*task), point);
        let Some(mut transport_action) =
            self.set_only_action(player_id, list_index, Some(transport_action))?
        else {
            return Ok(());
        };
        let move_to_tuning = self.model.tuning.move_to;
        let transport_tuning = self.model.tuning.transport;
        let changed = self.with_shore(
            player_id,
            list_index,
            transport_action.task,
            |ship, shore| transport_action.start(ship, shore, &transport_tuning, &move_to_tuning),
        )?;
        self.parts
            .actions_mut(self.players.live(player_id, list_index)?.id)?
            .list
            .set_transport(Some(transport_action));
        self.transport_changed(player_id, list_index, changed)
    }

    /// The stop's slot 0x38 of the Transport on top (slot 7, `FUN_140aa4620`: `set_state(2)`,
    /// [`TransportAction::stop`]) of player `player_id`'s object at `list_index`, the action
    /// `transport_action` as the stop took it: it goes back on the list, and what the state
    /// change leaves applies ([`World::transport_changed`]).
    ///
    /// # Errors
    ///
    /// A map without its terrain, and a sprite the table lacks.
    pub(crate) fn halt_transport(
        &mut self,
        player_id: PlayerId,
        list_index: ListIndex,
        mut transport_action: TransportAction,
    ) -> GenieResult<()> {
        let move_to_tuning = self.model.tuning.move_to;
        let transport_tuning = self.model.tuning.transport;
        let changed = self.with_shore(
            player_id,
            list_index,
            transport_action.task,
            |ship, shore| transport_action.stop(ship, shore, &transport_tuning, &move_to_tuning),
        );
        self.parts
            .actions_mut(self.players.live(player_id, list_index)?.id)?
            .list
            .set_transport(Some(transport_action));
        self.transport_changed(player_id, list_index, changed?)
    }

    /// The work command's retake (`LAB_1410706a8`) of the Transport on top on the `task` it
    /// found (its `+0x48`): the top action's slot 0x48 ([`TransportAction::work`] at the work's
    /// point, from the unit itself when the target is the unit), then the unit AI's notify as
    /// [`World::start_transport`]'s. A trade cart or boat (classes 19, 2) ordered to its own
    /// player's market (84) is never retaken. Taken when the Transport kept the work.
    ///
    /// # Errors
    ///
    /// A map without its terrain, and what the map reads give.
    ///
    /// [`World`]: crate::world::World
    pub(crate) fn retake_transport(
        &mut self,
        (player_id, list_index): (PlayerId, ListIndex),
        (target, point): (Option<ObjectId>, Position),
        task: &Task,
        notifies: Notifies,
    ) -> GenieResult<Answer> {
        let unit = self.players.live(player_id, list_index)?;
        // the Transport on top, on the task the work found
        let on_task = match self.parts.actions_of(unit.id).list.head() {
            Some(Action::Transport(transport_action))
                if transport_action.task.as_ref() == Some(task) =>
            {
                Some(*transport_action)
            }
            _ => None,
        };
        let Some(mut transport_action) = on_task else {
            return Ok(Answer::NotTaken);
        };
        if self.trade_at_own_market((player_id, list_index), target)? {
            return Ok(Answer::NotTaken);
        }
        let named = if target == Some(unit.id) {
            TransportTarget::Ship
        } else {
            TransportTarget::Other
        };
        let move_to_tuning = self.model.tuning.move_to;
        let transport_tuning = self.model.tuning.transport;
        let changed = self.with_shore(
            player_id,
            list_index,
            transport_action.task,
            |ship, shore| {
                transport_action.work(
                    point,
                    named,
                    ship,
                    shore,
                    &transport_tuning,
                    &move_to_tuning,
                )
            },
        )?;
        self.parts
            .actions_mut(self.players.live(player_id, list_index)?.id)?
            .list
            .set_transport(Some(transport_action));
        self.transport_changed(player_id, list_index, changed)?;
        self.notify_transport((player_id, list_index), (target, point), notifies)?;
        Ok(Answer::Taken)
    }

    /// `LAB_1410706a8`'s exception: the unit is a trade cart or boat (`FUN_141195400`,
    /// `FUN_141195490`) and the target its own player's market.
    fn trade_at_own_market(
        &self,
        (player_id, list_index): (PlayerId, ListIndex),
        target: Option<ObjectId>,
    ) -> GenieResult<bool> {
        let Some((tp, unit)) = self.object(target) else {
            return Ok(false);
        };
        // The market (`0x60f0` raw), whose own trade units the work command never retakes.
        let market = tp == player_id
            && self
                .model
                .roles
                .of_master(unit.master(self.master_tables(), tp)?.id)
                .contains(MasterRoles::MARKET);
        let class = self
            .players
            .live(player_id, list_index)?
            .master(self.master_tables(), player_id)?
            .class;
        Ok(market
            && self
                .model
                .roles
                .of_class(class)
                .contains(ClassRoles::TRADER))
    }

    /// The unit AI's notify of the work command's Transport (`FUN_14106e4e0` kind 0xc): the
    /// transport order and action, the target with its class (none for a point or a target the
    /// world does not hold) and the work's point.
    fn notify_transport(
        &mut self,
        (player_id, list_index): (PlayerId, ListIndex),
        (target, point): (Option<ObjectId>, Position),
        notifies: Notifies,
    ) -> GenieResult<()> {
        let named = match target.and_then(|id| Some((id, self.object(id)?))) {
            Some((target, (tp, unit))) => {
                Some((target, unit.master(self.master_tables(), tp)?.class))
            }
            None => None,
        };
        let id = self.players.live(player_id, list_index)?.id;
        if let Some(ai) = self.parts.unit_ai.get_mut(id) {
            ai.notify_work(
                Some(OrderKind::Transport),
                Some(AiAction::Transport),
                named,
                point,
                notifies,
                self.streams.get_mut(Stream::Ai),
            )?;
            self.remember_order(
                player_id,
                list_index,
                (Some(OrderKind::Transport), Some(AiAction::Transport)),
            )?;
        }
        Ok(())
    }

    /// `work` on the view of the Transport on `task` of player `player_id`'s object at `list_index`
    /// ([`Ship`]) and of the map ([`MapShore`] with the unit's terrain restriction).
    ///
    /// [`World`]: crate::world::World
    fn with_shore(
        &mut self,
        player_id: PlayerId,
        list_index: ListIndex,
        task: Option<Task>,
        work: impl FnOnce(&Ship<'_>, &mut MapShore<'_>) -> GenieResult<Changed>,
    ) -> GenieResult<Changed> {
        let ship = {
            let unit = self.players.live(player_id, list_index)?;
            let master = unit.master(self.master_tables(), player_id)?;
            Ship::of_unit(
                unit,
                self.top_speed(player_id, unit)?,
                master,
                self.spatial.map,
                (
                    &self.model.graphics,
                    self.parts.combat_of(unit.id).abilities.look.as_ref(),
                ),
                task.as_ref(),
                &self.model.roles,
            )
        };
        let restriction = self
            .players
            .live(player_id, list_index)?
            .master(self.master_tables(), player_id)?
            .terrain_restriction;
        let mut shore = MapShore::new(
            &self.spatial,
            &mut self.zones,
            &mut self.pathing,
            restriction,
            ship.position,
            &self.model.tuning.tile_path,
        )?;
        work(&ship, &mut shore)
    }
}
