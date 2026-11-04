//! A trained unit's spawn: where the make action (`TRIBE_Action_Make_Obj`, `FUN_140ad0240`) puts
//! the unit its building made. `FUN_140ace630` asks the spawn point search (`FUN_1411d12d0`) for a
//! point beside the building, then the master's slot 0x40 makes the object there.
//!
//! The search walks the building's sides in segments, stepping by the unit's size, and tries
//! each spot (`FUN_1411c5b60`) through the master's placement test (slot 0x50, `0x141264db0`):
//! the map's edge, the tiles' terrain restriction, the player's explored tiles and the objects
//! whose boxes overlap the unit's. A spot whose candidate is blocked by an object that stays
//! where it is (a building, a dead tree, a relic, a resting animal ...) ends; another blocker
//! lets the spot try its next candidate.

mod gather_walk;
pub(crate) use self::gather_walk::FinderUse;
mod make_action;
mod near;
mod new_unit;
mod objects;
mod placement_code;
pub use self::near::{Near, NearFlags};
mod placement_test;
mod research;
mod search;
mod terrains;
mod world_map;

pub use self::placement_code::PlacementCode;
pub(crate) use self::placement_test::NOT_MOVING;
pub use self::placement_test::{Placing, ZoneTest};
pub(crate) use self::placement_test::{build_placement, placement};
use self::search::corner_tiles;
pub use self::search::spawn_point;
#[cfg(test)]
use self::search::{Spot, try_spot};
use self::terrains::hill_fails;
pub(crate) use self::terrains::placement_terrains;
pub use self::terrains::{BuildSite, SiteChecks};
pub(crate) use self::world_map::WorldMap;
#[cfg(test)]
use crate::GenieError;
use crate::combat::missile::grounded;
use crate::map::Footprint;
use crate::map::Point;
use crate::map::Position;
use crate::math::image::spawn::NONE as SPAWN_NONE;
use crate::objects::Facet;
#[cfg(test)]
use crate::objects::lifecycle::Lifecycle;
use crate::objects::masters::FlyMode;
use crate::objects::masters::MasterId;
use crate::objects::masters::player_master;
use crate::players::PlayerId;
use crate::world::World;

/// No gather point (`DAT_14330ec20`).
const NONE: f32 = SPAWN_NONE;

/// How a new plain object ([`World::plain_object`]) that moves faces.
///
/// [`World::plain_object`]: crate::world::World::plain_object
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Facing {
    /// Its drawn angle (the moving setup's, `FUN_141141640`).
    Drawn,
    /// The dead object's facet it remains of (`FUN_14119d490`, `FUN_14114d780`), None where the
    /// slice does not hold it.
    Dead(Option<Facet>),
}

/// The building a unit spawns from, as the search reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Site {
    /// The building's place.
    pub(crate) position: Position,
    /// Its master's radii.
    pub(crate) radius: Footprint,
    /// Its gather point (`+0xc34`, `+0xc38`), when it has one (`+0xc30`) that is not on itself
    /// (`+0xc40`).
    pub(crate) gather: Option<Point>,
    /// A wall at its corners (both tiles beside each: left top, right top, right bottom, left
    /// bottom), which closes the sides that meet there.
    pub(crate) walls: [bool; 4],
}

/// What the placement test found at a candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tested {
    /// Free: the unit may stand there.
    Free,
    /// Not free; `stays`: the object it reports blocks for good, which ends the spot.
    Blocked {
        /// The object it reports blocks for good, which ends the spot.
        stays: bool,
    },
}

impl World {
    /// A new object of player `player_id`'s `master` placed at `at` (the static `new_position`,
    /// `0x1411a58b0`, slot 0x60's `FUN_141193a10`): its height the ground's by the master's fly
    /// mode; without the map's elevations the height stays.
    pub(crate) fn on_ground(
        &self,
        player_id: PlayerId,
        master: MasterId,
        at: Position,
    ) -> Position {
        let fly_mode = player_master(&self.model.masters, player_id, master)
            .map_or(FlyMode::GROUND, |master| master.fly_mode);
        self.spatial
            .terrain
            .as_ref()
            .and_then(|terrain| terrain.ground(at.ground()).ok())
            .map_or(at, |ground| {
                Position::new(at.x(), at.y(), grounded(at.z(), ground, fly_mode))
            })
    }
}

#[cfg(test)]
mod tests;
