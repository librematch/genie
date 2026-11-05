//! The object slots that hand a work or a move on, by master type: slot 0x128 (a work from a
//! source), slot 0x140 (a work with nothing else) and the start of slot 0x138 (a move), with
//! what they keep of the unit: where its order comes from (`+0xa14`) and its command count
//! (`+0x888`).

use crate::GenieResult;
use crate::map::Position;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::masters::ActionKind;
use crate::objects::masters::MasterKind;
use crate::orders::Place;
use crate::orders::town_bell::BellShelter;
use crate::players::PlayerId;
use crate::unit_ai::Notifies;
use crate::world::World;
use crate::world::layer::ClassRoles;
use crate::world::slots::PlayerSlotsExt;
use genie_diff::Diff;

/// Where a unit's order comes from: a combat object's or a building's `+0xa14` (types 70 and 80,
/// not saved: [`crate::objects::layers::Combat::order_source`]). It is 1 from the constructor
/// (`FUN_1411ea960`) on; slot 0x128 (`FUN_141211b80`, [`World::give_work`]) holds it at its 6th
/// argument while its work runs and then sets 1 again, and the town bell's way back to work
/// (`FUN_1412081e0`, [`World::back_outside`]) sets 0 and sets 1 again only where the unit went,
/// so a failed way back leaves it 0. A direct order (1) empties the unit AI's order queue in slot
/// 0x128 and in slot 0x138 (`FUN_140c5e010`; `FUN_141216f10`), and the new action's notify
/// clears the unit AI's notifies (`FUN_140c64e50` through `FUN_141201680`); a queued one (0)
/// does neither.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Diff)]
pub enum OrderSource {
    /// A direct order (1).
    #[default]
    Direct,
    /// The unit AI's queued order (0).
    Queued,
}

impl OrderSource {
    /// Whether the work command's notify (`FUN_140c64e50`) of a unit of master type `kind` clears
    /// its unit AI's notifies: a type below 70 always (it has no `+0xa14`), another only with a
    /// direct order.
    #[must_use]
    pub fn notifies(self, kind: MasterKind) -> Notifies {
        if kind < MasterKind::COMBAT || self == Self::Direct {
            Notifies::Cleared
        } else {
            Notifies::Kept
        }
    }
}

/// What a unit's slot 0x7e8 (`FUN_141227190`) is given, directly or through slot 0x128
/// ([`World::give_work`]) or slot 0x140 ([`World::work_at`]): the target, the point (x, y, z), the
/// work command's retargeting byte (its 8th argument) and task filter (its 7th, −1 for any).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Work {
    pub target: Option<ObjectId>,
    pub point: Position,
    /// The work command's last byte (`FUN_14106ff70`'s `param_8`): set, a gather or hunt task of
    /// another resource drops the unit's load ([`World::carried_resource_test`]) and an attack
    /// task turns a non-villager's retargeting off (the unit AI's `+0x1d6`, `FUN_140c40ef0`).
    /// INTERACT passes its `+0x12`, a group its `+0x3dc`; every other caller 0.
    pub disable_retargeting: bool,
    pub filter: Option<ActionKind>,
}

impl Work {
    /// The work on `target` at `point` with the byte 0 and any task, as most callers give it.
    #[must_use]
    pub(crate) const fn on(target: Option<ObjectId>, point: Position) -> Self {
        Self {
            target,
            point,
            disable_retargeting: false,
            filter: None,
        }
    }
}

impl World {
    /// Slot 0x128 of object `id` with `work` from `source`: a static object's (types below 40,
    /// `FUN_141071bd0`) does nothing; an action object's (40 to 60, `FUN_14106c590`) hands the
    /// work on ([`World::counted_work`]); a combat object's or a building's (70 and 80,
    /// `FUN_141211b80`) does so with its `+0xa14` at `source` ([`World::with_order_source`]).
    /// Nothing for an object the world does not hold live.
    ///
    /// # Errors
    ///
    /// The unit's master, and what the work command does not model.
    pub(crate) fn give_work(
        &mut self,
        id: ObjectId,
        work: &Work,
        source: OrderSource,
    ) -> GenieResult<()> {
        let Some(Place::Live(player, idx)) = self.place(id) else {
            return Ok(());
        };
        let kind = self
            .players
            .live(player, idx)?
            .master(self.master_tables(), player)?
            .kind;
        if kind < MasterKind::ACTION {
            return Ok(());
        }
        let work = *work;
        self.in_work_frame(id, kind, source, |world| {
            world.counted_work(player, idx, &work)
        })
    }

    /// Slot 0x128's frame of object `id` of master type `kind` (40 and up) around `run`: an
    /// action object's (`FUN_14106c590`) has none; a combat object's or a building's
    /// (`FUN_141211b80`) holds its `+0xa14` at `source` ([`World::with_order_source`]).
    ///
    /// # Errors
    ///
    /// What `run` does not model.
    pub(crate) fn in_work_frame(
        &mut self,
        id: ObjectId,
        kind: MasterKind,
        source: OrderSource,
        run: impl FnOnce(&mut Self) -> GenieResult<()>,
    ) -> GenieResult<()> {
        if kind < MasterKind::COMBAT {
            return run(self);
        }
        self.with_order_source(id, source, run)
    }

    /// `FUN_141211b80`'s frame around the work `run` of the combat object or building `id`: its
    /// `+0xa14` takes `source`, a direct order empties its unit AI's order queue
    /// (`FUN_140c5e010`), the work runs, and `+0xa14` takes 1 again.
    ///
    /// # Errors
    ///
    /// What `run` does not model.
    pub(crate) fn with_order_source(
        &mut self,
        id: ObjectId,
        source: OrderSource,
        run: impl FnOnce(&mut Self) -> GenieResult<()>,
    ) -> GenieResult<()> {
        self.parts.combat_mut(id)?.order_source = source;
        if source == OrderSource::Direct
            && let Some(ai) = self.parts.unit_ai.get_mut(id)
        {
            ai.orders.clear();
        }
        run(self)?;
        self.set_order_source(id, OrderSource::Direct)
    }

    /// The `+0xa14` of object `id` takes `source`, while the world holds it.
    ///
    /// # Errors
    ///
    /// None (an unreachable error where the table lost the layer it took).
    pub(crate) fn set_order_source(
        &mut self,
        id: ObjectId,
        source: OrderSource,
    ) -> GenieResult<()> {
        if self.object(id).is_some() {
            self.parts.combat_mut(id)?.order_source = source;
        }
        Ok(())
    }

    /// `FUN_14106c590` and `FUN_141070c90` on the unit at `idx`: unless the work's target is the
    /// unit itself, its slot 0x7e8 ([`World::order_work`]), then its command count one more
    /// ([`World::count_command`]).
    ///
    /// # Errors
    ///
    /// What the work command does not model.
    pub(crate) fn counted_work(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        work: &Work,
    ) -> GenieResult<()> {
        let id = self.players.live(player, idx)?.id;
        if work.target == Some(id) {
            return Ok(());
        }
        self.order_work(player, idx, work)?;
        self.count_command(id)
    }

    /// Slot 0x140 of the unit at `idx` on `target` at `point`: a static object's (types below
    /// 40, `FUN_141072790`) does nothing; any other's (`FUN_141070c90`) hands on the work with the
    /// byte 0 and any task ([`World::counted_work`]), its `+0xa14` as it stands.
    ///
    /// # Errors
    ///
    /// The unit's master, and what the work command does not model.
    pub(crate) fn work_at(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: Option<ObjectId>,
        point: Position,
    ) -> GenieResult<()> {
        let kind = self
            .players
            .live(player, idx)?
            .master(self.master_tables(), player)?
            .kind;
        if kind < MasterKind::ACTION {
            return Ok(());
        }
        self.counted_work(player, idx, &Work::on(target, point))
    }

    /// The command count (`+0x888`) of object `id` one more, while the world holds it.
    ///
    /// # Errors
    ///
    /// None (an unreachable error where the table lost the layer it took).
    pub(crate) fn count_command(&mut self, id: ObjectId) -> GenieResult<()> {
        if self.object(id).is_some() {
            let actions = self.parts.actions_mut(id)?;
            actions.commands = actions.commands.next();
        }
        Ok(())
    }

    /// Slot 0x138's start on the unit at `idx`, before the move (`FUN_14106d340`): a combat
    /// object's or a building's (`FUN_141216f10`) villager is no longer under the town bell
    /// (`+0x9dc`), and with its `+0xa14` 1 its unit AI's order queue empties (`FUN_140c5e010`);
    /// an action object's starts nothing.
    ///
    /// # Errors
    ///
    /// The unit's master.
    pub(crate) fn move_start(&mut self, player: PlayerId, idx: ListIndex) -> GenieResult<()> {
        let unit = self.players.live(player, idx)?;
        let master = unit.master(self.master_tables(), player)?;
        if master.kind < MasterKind::COMBAT {
            return Ok(());
        }
        let id = unit.id;
        if self
            .model
            .roles
            .of_class(master.class)
            .contains(ClassRoles::VILLAGER)
        {
            self.parts.set_town_bell_flag(id, BellShelter::Free);
        }
        if self.parts.combat_of(id).order_source == OrderSource::Direct
            && let Some(ai) = self.parts.unit_ai.get_mut(id)
        {
            ai.orders.clear();
        }
        Ok(())
    }
}
