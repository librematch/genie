//! `SEEK_SHELTER` (0x2c): the player's villagers seek shelter.

use crate::GenieResult;
use crate::commands::UnitList;
use crate::players::PlayerId;
use crate::world::World;

/// `SEEK_SHELTER` (0x2c, `FUN_140e6a3a0`): the villagers the packet lists (the count at `+0x4`,
/// the ids from `+0x8`) seek shelter in the player's buildings ([`World::seek_shelter`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeekShelterCommand {
    /// The player whose units seek shelter.
    pub player: PlayerId,
    /// The units.
    pub units: UnitList,
}

impl SeekShelterCommand {
    /// The check (`FUN_140e558b0`) refuses a count past the packet's ids; the handler reads
    /// the first of them ([`World::read_units`], [`World::seek_shelter`]).
    pub(crate) fn run(&self, world: &mut World) -> GenieResult<()> {
        let Some(ids) = world.read_units(self.player, &self.units) else {
            return Ok(());
        };
        world.seek_shelter(self.player, ids)
    }
}
