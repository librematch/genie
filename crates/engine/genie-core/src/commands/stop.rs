//! STOP (0x01): the player's units stop.

use crate::GenieResult;
use crate::commands::UnitList;
use crate::players::PlayerId;
use crate::world::World;

/// STOP (0x01, `FUN_14161e200`): the units the packet lists (the count at `+0x4`, the ids from
/// `+0x8`) stop ([`World::stop_command`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StopCommand {
    /// The player whose units stop.
    pub player: PlayerId,
    /// The units.
    pub units: UnitList,
}

impl StopCommand {
    /// The check (`FUN_140e55a30`) refuses a count past the packet's ids; the handler reads the
    /// first of them ([`World::read_units`], [`World::stop_command`]).
    pub(crate) fn run(&self, w: &mut World) -> GenieResult<()> {
        let Some(ids) = w.read_units(self.player, &self.units) else {
            return Ok(());
        };
        w.stop_command(self.player, ids)
    }
}
