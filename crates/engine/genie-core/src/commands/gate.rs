//! GATE (0x72): a player's gate lock.

use crate::GenieResult;
use crate::objects::ObjectId;
use crate::players::PlayerId;
use crate::world::World;

/// GATE (0x72, `do_command` case 0x72): the player's building `id` toggles its gate lock
/// ([`World::gate`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GateCommand {
    /// The player.
    pub player: PlayerId,
    /// Its building.
    pub id: ObjectId,
}

impl GateCommand {
    /// Its handler ([`World::gate`]).
    pub(crate) fn run(self, w: &mut World) -> GenieResult<()> {
        w.gate(self.player, self.id)
    }
}
