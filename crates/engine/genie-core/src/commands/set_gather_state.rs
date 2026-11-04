//! SET GATHER STATE (0x82): the gather point of a player's buildings is the building itself.

use crate::GenieResult;
use crate::commands::GatherPointState;
use crate::commands::UnitList;
use crate::players::PlayerId;
use crate::world::World;

/// SET GATHER STATE (0x82, `FUN_140e727e0`; the formatter's `SetGatherPointSelf`): each of the
/// objects the packet lists (the count at `+0x4`, the state byte at `+0x8`, the ids from `+0x9`)
/// that takes a gather point has itself for it with that state, or none with a state of 0
/// ([`World::set_gather_state`]). The computer player sends it: 0 to clear, 1, or 0xff.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetGatherStateCommand {
    /// The player whose objects take it.
    pub player: PlayerId,
    /// The objects of the packet.
    pub units: UnitList,
    /// The state the objects' gather point takes (`+0xc30`); none (the packet's 0) clears it.
    pub state: Option<GatherPointState>,
}

impl SetGatherStateCommand {
    /// The check refuses a count past the packet's ids; the handler reads the first of them
    /// ([`World::read_units`], [`World::set_gather_state`]).
    pub(crate) fn run(&self, w: &mut World) -> GenieResult<()> {
        let Some(ids) = w.read_units(self.player, &self.units) else {
            return Ok(());
        };
        w.set_gather_state(self.player, self.state, ids)
    }
}
