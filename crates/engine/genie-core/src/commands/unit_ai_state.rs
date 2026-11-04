//! `UNIT_AI_STATE` (0x12): the stance of the player's units.

use crate::GenieResult;
use crate::commands::UnitList;
use crate::players::PlayerId;
use crate::unit_ai::UnitStance;
use crate::world::World;

/// `UNIT_AI_STATE` (0x12, `FUN_140e6a7c0`): the unit AIs of the units the packet lists (the
/// count at `+0x4`, the ids from `+0xc`) take the stance `state` ([`World::unit_ai_state`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitAiStateCommand {
    /// The player whose units take it.
    pub player: PlayerId,
    /// The units.
    pub units: UnitList,
    /// The stance.
    pub state: UnitStance,
}

impl UnitAiStateCommand {
    /// The check (`FUN_140e55d30`) refuses a count past the packet's ids; the handler reads
    /// the first of them ([`World::read_units`], [`World::unit_ai_state`]).
    pub(crate) fn run(&self, world: &mut World) -> GenieResult<()> {
        let Some(ids) = world.read_units(self.player, &self.units) else {
            return Ok(());
        };
        world.unit_ai_state(self.player, ids, self.state)
    }
}
