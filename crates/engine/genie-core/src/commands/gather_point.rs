//! SET GATHER POINT (0x78): the gather point of the player's buildings.

use crate::GenieResult;
use crate::commands::UnitList;
use crate::map::Point;
use crate::objects::ObjectId;
use crate::objects::masters::MasterId;
use crate::players::PlayerId;
use crate::world::World;

/// SET GATHER POINT (0x78, `FUN_140e7e9a0`): each of the player's objects the packet lists (the
/// count at `+0x4`, the ids from `+0x19`) that takes a gather point (its master garrisons,
/// `+0x5d4`, or its player trains a unit there, `FUN_141625a20`'s count with no object,
/// [`World::trainable_count`]) takes `point` and `target` with its master, or with `own` itself
/// (`FUN_1411c7ff0`); a target of −1 at (−1, −1) clears it, and its unit AI's command queue
/// empties (slot 0x210). The ids of other players' objects are
/// left out (the game setting `+0x1074` that lets them is not modelled), as is the exists byte's
/// value 0xff (`+0xc30`, which keeps the target).
#[derive(Clone, Debug, PartialEq)]
pub struct GatherPointCommand {
    /// The player whose objects take it.
    pub player: PlayerId,
    /// The objects of the packet.
    pub units: UnitList,
    /// The point.
    pub point: Point,
    /// The object the point is on, −1 for none.
    pub target: Option<ObjectId>,
    /// That object's master.
    pub target_master: Option<MasterId>,
    /// The building itself is the gather point (`+0x18`).
    pub own: bool,
}

impl GatherPointCommand {
    /// `do_command`'s check refuses a count past the packet's ids; the handler reads the first
    /// of them ([`World::read_units`], [`World::gather_point`]).
    pub(crate) fn run(&self, w: &mut World) -> GenieResult<()> {
        let Some(ids) = w.read_units(self.player, &self.units) else {
            return Ok(());
        };
        w.gather_point(self, ids)
    }
}
