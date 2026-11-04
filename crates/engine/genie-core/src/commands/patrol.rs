//! PATROL (0x15): the player's units patrol through waypoints.

use crate::commands::UnitList;
use crate::map::Point;
use crate::orders::SavedPatrols;
use crate::players::PlayerId;
use crate::world::World;
use crate::{GenieError, GenieResult};

/// The waypoints of PATROL and `ATTACK_MOVE`: their count (`+0xb`) and the packet's ten slots
/// (x from `+0xc`, y from `+0x34`; the slots past the count are 0 in every recording).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Waypoints {
    /// How many of the slots the command uses.
    pub count: u8,
    /// The slots.
    pub points: [Point; 10],
}

impl Waypoints {
    /// Waypoints of `points` (at most ten; the slots past them 0).
    ///
    /// # Errors
    ///
    /// More than ten points.
    pub fn of(points: &[Point]) -> GenieResult<Self> {
        let mut slots = [Point::default(); 10];
        let too_many =
            || GenieError::invalid_input("more than ten waypoints").with("count", points.len());
        slots
            .get_mut(..points.len())
            .ok_or_else(too_many)?
            .copy_from_slice(points);
        Ok(Self {
            count: u8::try_from(points.len()).map_err(|_| too_many())?,
            points: slots,
        })
    }

    /// The waypoints the handlers read (`FUN_140e69cc0`, `FUN_140e66200`): the first `count`;
    /// None without 1 to 10, where they do nothing.
    #[must_use]
    pub fn used(&self) -> Option<&[Point]> {
        if (1..=10).contains(&self.count) {
            self.points.get(..usize::from(self.count))
        } else {
            None
        }
    }
}

/// PATROL (0x15, `FUN_140e69cc0`): the units the packet lists (the count at `+0x4`, the ids from
/// `+0x5c`) patrol through `waypoints` (1 to 10, their count at `+0xb`, x from `+0xc`, y from
/// `+0x34`); `patrols` (`+0x8`) drops their unit AIs' saved patrols or keeps them
/// ([`World::patrol`]).
#[derive(Clone, Debug, PartialEq)]
pub struct PatrolCommand {
    /// The player whose units patrol.
    pub player: PlayerId,
    /// The units.
    pub units: UnitList,
    /// The waypoints.
    pub waypoints: Waypoints,
    /// `+0x8`.
    pub patrols: SavedPatrols,
}

impl PatrolCommand {
    /// The check (`FUN_140e55590`) refuses a count past the packet's ids; the handler reads the
    /// first of them ([`World::read_units`]) and does nothing without 1 to 10 waypoints
    /// ([`Waypoints::used`], [`World::patrol`]).
    pub(crate) fn run(&self, world: &mut World) -> GenieResult<()> {
        let Some(ids) = world.read_units(self.player, &self.units) else {
            return Ok(());
        };
        let Some(waypoints) = self.waypoints.used() else {
            return Ok(());
        };
        world.patrol(self.player, ids, waypoints, self.patrols)
    }
}
