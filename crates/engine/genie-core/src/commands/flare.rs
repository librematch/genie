//! FLARE (0x73): a flare for the player's allies.

use crate::GenieResult;
use crate::map::Point;
use crate::objects::ObjectId;
use crate::players::PlayerId;
use crate::world::World;

/// FLARE (0x73, `0x140e7bfd0`): every player from 1 below `players` (`+0x10`, at most the
/// world's) gets a flare at `target`'s place (`+4`, −1 for none) or at `point` (`+8`, `+0xc`)
/// ([`World::flare`]).
#[derive(Clone, Debug, PartialEq)]
pub struct FlareCommand {
    /// The player who sent it.
    pub player: PlayerId,
    /// The object it is on, −1 for none.
    pub target: Option<ObjectId>,
    /// Its point without a target.
    pub point: Point,
    /// The players the command counts (`+0x10`).
    pub players: u8,
    /// `+0x11..`: one byte for each player the command counts, whether that player sees the
    /// flare (the interface's, not read by the slice).
    pub shown: Vec<bool>,
}

impl FlareCommand {
    /// Its handler ([`World::flare`]).
    pub(crate) fn run(&self, w: &mut World) -> GenieResult<()> {
        w.flare(self.target, self.point, self.players)
    }
}
