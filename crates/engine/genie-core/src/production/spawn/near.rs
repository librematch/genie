//! An object near a candidate ([`Near`]), as the placement test and the blocker test read it.

use crate::map::Footprint;
use crate::map::Position;
use crate::map::Radii;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::BuildableOn;
use crate::objects::masters::ClassId;
use crate::objects::masters::FogVisibility;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObstructionKind;
use crate::players::PlayerId;
use crate::vision::VisionBits;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;

bitflags::bitflags! {
    /// What the placement test reads of a near object as yes or no ([`Near`]).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct NearFlags: u8 {
        /// Its master's height radius is above 0.
        const TALL = 1;
        /// Its master's `enable_auto_gather` (`+0x681`).
        const AUTO_GATHER = 1 << 1;
        /// A building's `desolid_flag` (`+0xc60`).
        const DESOLID = 1 << 2;
        /// Its master moves (speed above 0).
        const MOVING = 1 << 3;
        /// It holds an amount (`+0x310` above 0) of food, wood, stone or gold (its attribute,
        /// `+0x308`, through `FUN_140fdd470` below 4: 0 to 3 and the meats 15 to 17).
        const HOLDS = 1 << 4;
        /// The terrain of its tile (`+0x2e8`, taken as the tile it stands on) is a forest
        /// ([`crate::map::terrain::TerrainKind::forest`]).
        const FOREST_TILE = 1 << 5;
    }
}

/// An object near a candidate, as the placement test and the blocker test read it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Near {
    /// The object.
    pub id: ObjectId,
    /// Its place.
    pub position: Position,
    /// Its master's radii on the ground and whether its height radius is above 0.
    pub radius: Footprint,
    /// Its height, auto-gather, desolid, moving, holding and forest reads.
    pub flags: NearFlags,
    /// Its master's type (`+0x8`), class, id, `can_be_built_on` (`+0x5e9`), `obstruction_type`
    /// (`+0x651`) and `enable_auto_gather` (`+0x681`).
    pub kind: MasterKind,
    /// Its master's class.
    pub class: ClassId,
    /// Its master.
    pub master: MasterId,
    /// Its master's `can_be_built_on` (`+0x5e9`).
    pub buildable_on: BuildableOn,
    /// Its master's `obstruction_type` (`+0x651`).
    pub obstruction_kind: ObstructionKind,
    /// Its state (`+0x640`) and, for a building, its `desolid_flag` (`+0xc60`).
    pub state: ObjectState,
    /// Its owner.
    pub player: PlayerId,
    /// Its master's `fog_visibility` (`+0x605`).
    pub fog_visibility: FogVisibility,
    /// A building's visibility mask (`+0x8f8`, the registry's) when its footprint has a UMV entry
    /// (`+0x904` not 0); None otherwise.
    pub fog_mask: Option<VisionBits>,
    /// Its hit points.
    pub hp: HitPoints,
    /// Its master's outline radii and height (`+0x688`, `+0x68c`, `+0x690`), BUILD's box for
    /// obstruction types 11 to 13.
    pub outline: Radii,
}

impl Near {
    /// The blocker tests of `FUN_1411c5b60`: a building (`FUN_1411942c0`), a tree past state 2
    /// (`FUN_141195660`), the relic (`FUN_141194e80`), and `FUN_141194170`'s master with
    /// `enable_auto_gather` or of a class that stays from state 2 on (5, 9, 10, 29, 31, 33, 38,
    /// 40, 41, 58, 61) or from state 3 on (7, 8, 15, 32, 48, 63).
    #[must_use]
    pub fn stays(&self, roles: &Roles) -> bool {
        if self.kind == MasterKind::BUILDING
            || (roles.of_class(self.class).contains(ClassRoles::TREE)
                && ObjectState::Alive < self.state)
            || roles.of_master(self.master).contains(MasterRoles::RELIC)
        {
            return true;
        }
        if self.flags.contains(NearFlags::AUTO_GATHER) {
            return true;
        }
        {
            let held = roles.of_class(self.class);
            if held.intersects(
                ClassRoles::OCEAN_FISH
                    | ClassRoles::PREY
                    | ClassRoles::PREDATOR
                    | ClassRoles::DOMESTIC_ANIMAL
                    | ClassRoles::DEEP_SEA_FISH
                    | ClassRoles::SHORE_FISH
                    | ClassRoles::BIRD
                    | ClassRoles::SALVAGE_PILE
                    | ClassRoles::RESOURCE_PILE
                    | ClassRoles::LIVESTOCK
                    | ClassRoles::CONTROLLED_ANIMAL,
            ) {
                ObjectState::Unfinished < self.state
            } else if held.intersects(
                ClassRoles::FORAGE_BUSH
                    | ClassRoles::STONE_MINE
                    | ClassRoles::TREE
                    | ClassRoles::GOLD_MINE
                    | ClassRoles::ORE_MINE
                    | ClassRoles::GOLD_FISH,
            ) {
                ObjectState::Alive < self.state
            } else {
                false
            }
        }
    }
}
