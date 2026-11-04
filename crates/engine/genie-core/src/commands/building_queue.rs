//! `BUILDING_COMMAND_QUEUE` (0x2d): a step the player's objects queue.

use crate::commands::UnitList;
use crate::map::Point;
use crate::objects::ObjectId;
use crate::players::PlayerId;
use crate::world::World;
use crate::{GenieError, GenieResult};

/// `BUILDING_COMMAND_QUEUE` (0x2d, `0x140e64310`): the objects the packet lists (the count at
/// `+0x10`, the ids from `+0x18`) queue a step to `target` (−1 for none) or `point`
/// ([`World::command_queue`]).
#[derive(Clone, Debug, PartialEq)]
pub struct BuildingQueueCommand {
    /// The player.
    pub player: PlayerId,
    /// The objects of the packet.
    pub units: UnitList,
    /// The object the step is on, −1 for none.
    pub target: Option<ObjectId>,
    /// Its point.
    pub point: Point,
}

impl BuildingQueueCommand {
    /// The check (`FUN_140e54910`) refuses a count past the packet's ids but `0xffff`, which it
    /// reads as the last commanded units; the handler reads the first of the packet's ids
    /// ([`World::read_units`], [`World::command_queue`]). With the count `0xffff` the handler
    /// reads its 60 (or 500) ids past the end of the packet.
    pub(crate) fn run(&self, w: &mut World) -> GenieResult<()> {
        if self.units.names_last_commanded() {
            return Err(GenieError::invalid_input(
                "a BUILDING_COMMAND_QUEUE of the last commanded units reads past its packet",
            ));
        }
        let Some(ids) = w.read_units(self.player, &self.units) else {
            return Ok(());
        };
        w.command_queue(self.player, ids, self.target, self.point)
    }
}
