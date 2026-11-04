//! QUEUE (0x81): units of one master at the player's buildings.

use crate::GenieResult;
use crate::commands::UnitList;
use crate::objects::masters::MasterId;
use crate::players::PlayerId;
use crate::production::QueueButton;
use crate::production::QueuedUnits;
use crate::world::World;

/// QUEUE (0x81, the player's queue `FUN_141614450`, the building's `FUN_1411cbc40`): `amount`
/// units of master `unit` at one building with the train button `button` ([`World::queue`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueueCommand {
    /// The player who queues.
    pub player: PlayerId,
    /// The buildings the packet lists (the count at `+0x4`, the ids from `+0x14`).
    pub buildings: UnitList,
    /// `+0xa`, the formatter's `bId`: the buildings' master, which the handler does not read.
    pub building_master: MasterId,
    /// The master to train (`+0xc`; 0x712 for the villager).
    pub unit: MasterId,
    /// How many (`+0xe`, which the handler takes as an int).
    pub amount: QueuedUnits,
    /// `button_location_id`: the train button, −1 for none.
    pub button: QueueButton,
}

impl QueueCommand {
    /// `do_command`'s check refuses a player the world lacks and a count past the packet's ids
    /// ([`World::read_units`]); then the queue at the buildings ([`World::queue`]).
    pub(crate) fn run(&self, world: &mut World) -> GenieResult<()> {
        let Some(listed) = world.read_units(self.player, &self.buildings) else {
            return Ok(());
        };
        world.queue(self, listed)
    }
}
