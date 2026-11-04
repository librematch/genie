//! `AUTO_SCOUT` (0x26): the player's units scout by themselves.

use crate::GenieResult;
use crate::commands::UnitList;
use crate::players::PlayerId;
use crate::world::World;

/// `AUTO_SCOUT` (0x26, `FUN_140e666f0`): each unit the packet lists (the count at `+0x4`, the
/// ids from `+0x8`) takes the auto scout order, or the stop order when it scouts already
/// ([`World::auto_scout`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutoScoutCommand {
    /// The player.
    pub player: PlayerId,
    /// Its units.
    pub units: UnitList,
}

impl AutoScoutCommand {
    /// The check (`FUN_140e54790`) refuses a count past the packet's ids; the handler reads
    /// the first of them ([`World::read_units`], [`World::auto_scout`]).
    pub(crate) fn run(&self, world: &mut World) -> GenieResult<()> {
        let Some(ids) = world.read_units(self.player, &self.units) else {
            return Ok(());
        };
        world.auto_scout(self.player, ids)
    }
}
