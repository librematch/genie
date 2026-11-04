//! `ATTACK_MOVE` (0x21): the player's units attack-move through waypoints.

use crate::GenieResult;
use crate::commands::{UnitList, Waypoints};
use crate::orders::SavedPatrols;
use crate::orders::Sweep;
use crate::players::PlayerId;
use crate::world::World;

/// `ATTACK_MOVE` (0x21, `FUN_140e66200`): the units the packet lists (the count at `+0x4`, the
/// ids from `+0x5c`) attack-move through `waypoints` (1 to 10, their count at `+0xb`, x from
/// `+0xc`, y from `+0x34`); `reset` (`+0x8`) drops their unit AIs' saved patrols, `queued`
/// (`+0x9`) and `ignore_pending` (`+0xa`) send the order through the unit AIs' queues
/// ([`World::attack_move`]).
#[derive(Clone, Debug, PartialEq)]
pub struct AttackMoveCommand {
    /// The player whose units attack-move.
    pub player: PlayerId,
    /// The units.
    pub units: UnitList,
    /// The waypoints.
    pub waypoints: Waypoints,
    /// `+0x8`.
    pub patrols: SavedPatrols,
    /// `+0x9`.
    pub queued: bool,
    /// `+0xa`.
    pub ignore_pending: bool,
}

impl AttackMoveCommand {
    /// The check (`FUN_140e54490`) refuses a count past the packet's ids; the handler reads the
    /// first of them ([`World::read_units`]); without 1 to 10 waypoints it does nothing
    /// ([`Waypoints::used`]), else it runs [`World::attack_move`] with the sweep of an attack
    /// move (no patrol).
    pub(crate) fn run(&self, w: &mut World) -> GenieResult<()> {
        let Some(ids) = w.read_units(self.player, &self.units) else {
            return Ok(());
        };
        let Some(waypoints) = self.waypoints.used() else {
            return Ok(());
        };
        let sweep = Sweep {
            patrol: false,
            patrols: self.patrols,
            queued: self.queued,
            ignore_pending: self.ignore_pending,
        };
        w.attack_move(self.player, ids, waypoints, sweep)
    }
}
