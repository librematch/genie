//! The tech tree's data: its units, buildings and unique units, and the queries over them (FD-046).

use crate::players::CivId;
use crate::players::world_player::{TreeMaster, TreeTech};
use crate::world::Age;
use crate::world::settings::Settings;
use serde::Deserialize;
use serde::Serialize;

/// One unit of a building in a civilisation's tech tree (`0x48` bytes).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TreeUnit {
    /// `+0x0`: the unit's master.
    pub id: TreeMaster,
    /// `+0x4`: the master it links to ([`TechTree::link`]).
    pub link: TreeMaster,
    /// `+0x8`: its age (2 Feudal, 3 Castle, 4 Imperial).
    pub age: Age,
    /// `+0xc`: the technology that brings it ([`TechTree::tech`]).
    pub tech: TreeTech,
}

/// One building of a civilisation's tech tree (`0x48` bytes): its master and its units.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TreeBuilding {
    /// `+0x0`: the building's master.
    pub id: TreeMaster,
    /// `+0x8` (count `+0x18`): its units, in order.
    pub units: Vec<TreeUnit>,
}

/// A civilisation's unique unit (its data's `+0xec`, `+0xf0`, `+0xf4`, `FUN_140e868f0`,
/// `FUN_140e868b0`, `FUN_140e86630`).
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UniqueUnit {
    /// The technology that makes it elite.
    pub elite_tech: TreeTech,
    /// The unit.
    pub unit: TreeMaster,
    /// The elite unit.
    pub elite: TreeMaster,
}

/// The game's tech tree table (`FUN_140c7e080`: the game object's `+0xa60`, or `+0xa68` for a
/// local player's civilisation in `FUN_140e9a7e0`'s list, 14 and 17) by civilisation index
/// (`0x20` bytes each: its buildings, count `+0x10`), and each civilisation's unique unit by its
/// index (`FUN_140e87930`, `civilizations.json`'s `unique_unit_upgrade_id`, `unique_unit_id`,
/// `elite_unique_unit_id`).
///
/// The game's data load (`FUN_140c81bd0`) reads `futuravailableunits.json` into `+0xa60` and
/// `paphosfutureavailableunits.json` into `+0xa68` (`FUN_1414f3720`): civilisation indices 0 to
/// the full tech tree's (`FUN_140c7e6f0`), each its name's buildings (`ID`) with their units
/// (`ID`, `RequiredUnitID`, `RequiredAge`, `RequiredTechID`, `FUN_1414f5c40`), the last
/// `FullTechCiv`'s. With the game's `+0x680` at 0, as genie and the tick oracle hold it, that is
/// index 0 alone: a player of civilisation 1 or more without the full tech tree finds no
/// civilisation in it, as in the empty table genie-oracle leaves; the full tech tree's index 0
/// is not loaded yet.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TechTree {
    /// The civilisations' buildings.
    pub civs: Vec<Vec<TreeBuilding>>,
    /// The civilisations' unique units.
    pub unique_units: Vec<Option<UniqueUnit>>,
}

/// What a tree lookup asks: the civilisation, the building, the unit, the ages the player
/// cannot reach (technologies 101 to 103 at state −1) and the game's options.
#[derive(Clone, Copy, Debug)]
pub struct TreeQuery<'a> {
    /// The civilisation index.
    pub civ: CivId,
    /// The building's master.
    pub building: TreeMaster,
    /// The unit's master.
    pub unit: TreeMaster,
    /// The Feudal, Castle and Imperial Ages out of reach.
    pub ages_out: [bool; 3],
    /// The game's options: the full tech tree and the last age.
    pub settings: &'a Settings,
    /// The civilisation the full tech tree falls back to (`FUN_140c7e6f0`).
    pub fallback: CivId,
}

/// What [`TechTree::find`] answers.
pub(in crate::players::world_player) enum Found<'a> {
    /// The table has no such civilisation (`FUN_1414f5340`'s and `FUN_1414f4c80`'s 0).
    NoCiv,
    /// The unit is out of the game's ages or of the player's (0).
    OutOfAges,
    /// The building's units, the first that is the unit, if any.
    Unit(Option<&'a TreeUnit>),
    /// No such building, also in the fallback civilisation (−1).
    NoBuilding,
}

impl TechTree {
    /// The tree lookups' common part (`FUN_1414f5970`, `FUN_1414f5340`, `FUN_1414f4c80`): the
    /// civilisation's first building that is `q.building`; its first unit that is `q.unit` gives
    /// the age (0 for none), out of the game when the last age less 1 is below it; then that
    /// unit's age 2 out with the Feudal Age, 3 with the Feudal or Castle and 4 with any. A
    /// building the civilisation lacks under the full tech tree tries the fallback civilisation
    /// once it is another.
    pub(in crate::players::world_player) fn find(&self, q: &TreeQuery<'_>) -> Found<'_> {
        let mut civ = q.civ;
        loop {
            let Some(buildings) = self.civs.get(civ.index()) else {
                return Found::NoCiv;
            };
            if let Some(b) = buildings.iter().find(|b| b.id == q.building) {
                let unit = b.units.iter().find(|u| u.id == q.unit);
                let age = unit.map_or(Age::Default, |u| u.age);
                if q.settings.last_age().before() < age {
                    return Found::OutOfAges;
                }
                let [feudal, castle, imperial] = q.ages_out;
                let out = unit.is_some_and(|u| {
                    (u.age == Age::Feudal && feudal)
                        || (u.age == Age::Castle && (feudal || castle))
                        || (u.age == Age::Imperial && (feudal || castle || imperial))
                });
                if out {
                    return Found::OutOfAges;
                }
                return Found::Unit(unit);
            }
            if civ == q.fallback || !q.settings.full_tech_tree {
                return Found::NoBuilding;
            }
            civ = q.fallback;
        }
    }

    /// `FUN_1414f5970`: the unit is in the civilisation's tree at the building, within the ages.
    #[must_use]
    pub fn has(&self, q: &TreeQuery<'_>) -> bool {
        matches!(self.find(q), Found::Unit(Some(_)))
    }

    /// `FUN_1414f5340`: the unit's link (`+0x4`); 0 out of the ages or without the civilisation,
    /// −1 without the building or the unit (a building found without the unit tries the
    /// fallback civilisation as one not found does).
    #[must_use]
    pub fn link(&self, q: &TreeQuery<'_>) -> TreeMaster {
        TreeMaster::of_data(self.field(q, |u| u.link.get()))
    }

    /// `FUN_1414f4c80`: the unit's technology (`+0xc`), as [`TechTree::link`].
    #[must_use]
    pub fn tech(&self, q: &TreeQuery<'_>) -> TreeTech {
        TreeTech::of_data(self.field(q, |u| u.tech.get()))
    }

    pub(in crate::players::world_player) fn field(
        &self,
        q: &TreeQuery<'_>,
        f: impl Fn(&TreeUnit) -> i32,
    ) -> i32 {
        match self.find(q) {
            Found::NoCiv | Found::OutOfAges => 0,
            Found::Unit(Some(u)) => f(u),
            Found::Unit(None) if q.civ != q.fallback && q.settings.full_tech_tree => {
                let again = TreeQuery {
                    civ: q.fallback,
                    ..*q
                };
                self.field(&again, f)
            }
            Found::Unit(None) | Found::NoBuilding => -1,
        }
    }
}
