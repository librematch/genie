//! `FORM_FORMATION` (0x17): the player's units form a formation.

use crate::GenieResult;
use crate::commands::UnitList;
use crate::groups::FormationKind;
use crate::players::PlayerId;
use crate::world::World;

/// `FORM_FORMATION` (0x17, `FUN_14161dba0`): the units the packet lists (the count at `+0x4`,
/// the ids from `+0xc`) are grouped by the order splitter, each group locks to the formation
/// type at `+0x8` ([`World::form_formation`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormFormationCommand {
    /// The player whose units form up.
    pub player: PlayerId,
    /// The units.
    pub units: UnitList,
    /// `+0x8`: the formation type the groups lock to.
    pub formation: FormationKind,
}

impl FormFormationCommand {
    /// The check (`FUN_140e54dd0`, STOP's twin) refuses a count past the packet's ids; the
    /// handler reads the first of them ([`World::read_units`], [`World::form_formation`]).
    pub(crate) fn run(&self, w: &mut World) -> GenieResult<()> {
        let Some(ids) = w.read_units(self.player, &self.units) else {
            return Ok(());
        };
        w.form_formation(self.player, ids, self.formation)
    }
}
