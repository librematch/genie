//! The work command's notify (the unit's slot 0x828) of the action it gave the unit: the unit AI
//! takes the order and action of the action's type and the target.

use crate::GenieResult;
use crate::combat::attack::ATTACK;
use crate::map::Position;
use crate::math::rng::Stream;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ClassId;
use crate::players::PlayerId;
use crate::unit_ai::Notifies;
use crate::unit_ai::work_orders;
use crate::world::World;
use crate::world::slots::PlayerSlotsExt;

impl World {
    /// The target the work notify names for the work command's `target` (`R14`, the object the
    /// command found), with its master's class: none for a point and for a target the world
    /// does not hold, for which the command holds a null object.
    ///
    /// # Errors
    ///
    /// A target of a master its owner lacks.
    pub(crate) fn told_target(
        &self,
        target: Option<ObjectId>,
    ) -> GenieResult<Option<(ObjectId, ClassId)>> {
        let Some((id, (owner, object))) = target.and_then(|id| Some((id, self.object(id)?))) else {
            return Ok(None);
        };
        Ok(Some((
            id,
            object.master(self.master_tables(), owner)?.class,
        )))
    }

    /// The work notify (the unit's slot 0x828) of object `idx` of player `player` for its new
    /// action of type `kind`, on the target `told` with its class (none: −1) at `point`: its unit
    /// AI takes the order and action of the type ([`work_orders`], [`UnitAi::notify_work`]; none
    /// for a type the switch does not name), and an Attack sets its last retarget time to the
    /// world's (`FUN_140c41bd0`); then it remembers the order on the target
    /// ([`World::remember_order`]). An object without a unit AI hears nothing.
    ///
    /// The slot is the unit's class's. A `TRIBE_Combat_Object` (type 70) or a
    /// `TRIBE_Building_Object` (type 80) holds `FUN_14121c890`. It handles the types `0x7a`,
    /// `0x84`, `0x87` and `0x88` itself (order `0x2db`, action `0x277`), and `0x95`, the Shepherd
    /// (order `0x2dc`, action `0x278`). For these it sets the target twice (`FUN_140c64be0`), so
    /// the target becomes its own last target too. genie's action list holds no action of these
    /// types, so that branch cannot run here (the task tribe-work-notify-slot828). Every other
    /// type goes to `RGE_Combat_Object`'s slot 0x828 (`FUN_14106e4e0`), which the other classes
    /// hold.
    ///
    /// [`UnitAi::notify_work`]: crate::unit_ai::UnitAi::notify_work
    ///
    /// # Errors
    ///
    /// An object the world lacks, the notify's and the order's record's.
    pub(crate) fn notify_new_action(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        kind: ActionKind,
        told: Option<(ObjectId, ClassId)>,
        point: Position,
        notifies: Notifies,
    ) -> GenieResult<()> {
        let id = self.players.live(player, idx)?.id;
        let (order, action) = work_orders(kind);
        let world_time = self.world_time;
        if let Some(ai) = self.parts.unit_ai.get_mut(id) {
            if kind == ATTACK {
                ai.last_retarget_time = world_time;
            }
            ai.notify_work(
                order,
                action,
                told,
                point,
                notifies,
                self.streams.get_mut(Stream::Ai),
            )?;
            self.remember_order(player, idx, (order, action))?;
        }
        Ok(())
    }
}
