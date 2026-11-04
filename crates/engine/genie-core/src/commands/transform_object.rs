//! `TRANSFORM_OBJECT` (0x29): an object of the player takes another master.

use crate::GenieResult;
use crate::objects::ObjectId;
use crate::objects::masters::MasterId;
use crate::players::PlayerId;
use crate::world::World;

/// `TRANSFORM_OBJECT` (0x29, `RGE_Command::do_command`'s case): the player's object `id` takes
/// the master `master` ([`World::transform_object`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransformObjectCommand {
    /// The player.
    pub player: PlayerId,
    /// The object (`+8`).
    pub id: ObjectId,
    /// The new master (`+4`).
    pub master: MasterId,
}

impl TransformObjectCommand {
    /// Its handler ([`World::transform_object`]).
    pub(crate) fn run(self, world: &mut World) -> GenieResult<()> {
        world.transform_object(self.player, self.id, self.master)
    }
}
