//! RESIGN (0x0b): a player resigns or drops.

use crate::GenieResult;
use crate::players::PlayerId;
use crate::world::World;

/// How a player leaves the game by RESIGN: it resigns, or it drops (the packet's `dropped`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Departure {
    /// It resigns (0).
    Resigned,
    /// It drops (1).
    Dropped,
}

/// RESIGN (0x0b, `FUN_140d25af0`): the comm player `player` resigns, or drops ([`Departure`])
/// ([`World::resign`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResignCommand {
    /// The comm player (the world player of the same number in the slice's games).
    pub player: PlayerId,
    /// A resignation or a drop from the game.
    pub departure: Departure,
}

impl ResignCommand {
    /// Its handler ([`World::resign`]).
    pub(crate) fn run(self, world: &mut World) -> GenieResult<()> {
        world.resign(self.player, self.departure)
    }
}
