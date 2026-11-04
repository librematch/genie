//! `ALL_BACK_TO_WORK` (0x8c): a town centre's villagers back to work.

use crate::GenieResult;
use crate::objects::ObjectId;
use crate::players::PlayerId;
use crate::world::World;

/// `ALL_BACK_TO_WORK` (0x8c, the object at `+4`): its owner's buildings near it let their
/// villagers out to work and the others go back ([`World::all_back_to_work`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AllBackToWorkCommand {
    /// The player.
    pub player: PlayerId,
    /// The object (a town centre).
    pub id: ObjectId,
}

impl AllBackToWorkCommand {
    /// Its handler ([`World::all_back_to_work`]).
    pub(crate) fn run(self, world: &mut World) -> GenieResult<()> {
        world.all_back_to_work(self.player, self.id)
    }
}
