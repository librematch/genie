//! `GO_BACK_TO_WORK` (0x80) and `GO_BACK_TO_WORK2` (0x86): the player's buildings let their
//! villagers back out.

use crate::GenieResult;
use crate::commands::UnitList;
use crate::objects::ObjectId;
use crate::players::PlayerId;
use crate::world::World;

/// `GO_BACK_TO_WORK` (0x80): the player's building (`+4`) lets its villagers back out to work
/// ([`World::go_back_to_work`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GoBackToWorkCommand {
    /// The player.
    pub player: PlayerId,
    /// Its building.
    pub id: ObjectId,
}

impl GoBackToWorkCommand {
    /// `do_command`'s check leaves only the player's own building (or any with the setting
    /// `+0x1074`); the handler (`FUN_141625590`) runs on it ([`World::go_back_to_work`]).
    pub(crate) fn run(self, w: &mut World) -> GenieResult<()> {
        if w.players.get(self.player.index()).is_none() {
            return Ok(());
        }
        w.go_back_to_work(self.player, &[Some(self.id)])
    }
}

/// `GO_BACK_TO_WORK2` (0x86): `GO_BACK_TO_WORK` of the buildings the packet lists (the count at
/// `+4`, the ids from `+8`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoBackToWork2Command {
    /// The player.
    pub player: PlayerId,
    /// Its buildings.
    pub units: UnitList,
}

impl GoBackToWork2Command {
    /// `do_command`'s check refuses a count past the packet's ids; the handler reads the first
    /// of them ([`World::read_units`], [`World::go_back_to_work`]).
    pub(crate) fn run(&self, w: &mut World) -> GenieResult<()> {
        let Some(ids) = w.read_units(self.player, &self.units) else {
            return Ok(());
        };
        w.go_back_to_work(self.player, ids)
    }
}
