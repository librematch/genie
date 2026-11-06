//! A building's counts of the units inside it ([`GarrisonCounts`]).

use crate::garrison::InsideCount;
use crate::garrison::RaiderCountdown;
use crate::objects::masters::Master;
use crate::objects::masters::MasterKind;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use genie_diff::Diff;

/// What a building (type 80) counts of the units inside it as each enters (its slot 0x1c0,
/// `FUN_1411cb850`) and leaves (its slot 0x1c8, `FUN_1411d6ad0`), by the unit's own master
/// (`+0xa0`) and its countdown at that moment. The save holds neither count: the constructors
/// set both to 0, and the load's re-link enters each unit inside again
/// (`RGE_Static_Object::rehook`, slot 0x1a0 with 1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Diff)]
pub struct GarrisonCounts {
    /// `+0xba8`: the units of the villager's class (4) inside.
    pub(crate) villagers: InsideCount,
    /// `+0xc2c`: the units of type 70 that came in with their `raider_build_countdown`
    /// (`+0x9a0`) set, less those that left with it set (below 0 for a countdown that started
    /// inside); slot 0x498 leaves them out ([`crate::world::World::garrisoned`]).
    pub(crate) raiders: InsideCount,
}

impl GarrisonCounts {
    /// None inside: a new building's, and a loaded one's before its re-link.
    pub const NONE: Self = Self {
        villagers: InsideCount::ZERO,
        raiders: InsideCount::ZERO,
    };

    /// What a unit of master `master` whose countdown is `countdown` counts: a villager for the
    /// class 4 (the short −0xea8 the master holds), a raider for type 70 (`'F'`) with the
    /// countdown set (not 0).
    #[must_use]
    pub fn of_unit(master: &Master, countdown: RaiderCountdown, roles: &Roles) -> Self {
        let villager = roles.of_class(master.class).contains(ClassRoles::VILLAGER);
        let raider = master.kind == MasterKind::COMBAT && countdown.is_set();
        Self {
            villagers: InsideCount::new(i32::from(villager)),
            raiders: InsideCount::new(i32::from(raider)),
        }
    }

    /// The building's slot 0x1c0 counts in what a unit counts ([`GarrisonCounts::of_unit`]).
    pub(crate) const fn count_in(&mut self, unit: Self) {
        self.villagers = self.villagers.plus(unit.villagers);
        self.raiders = self.raiders.plus(unit.raiders);
    }

    /// The building's slot 0x1c8 counts out what a unit counts ([`GarrisonCounts::of_unit`]).
    pub(crate) const fn count_out(&mut self, unit: Self) {
        self.villagers = self.villagers.minus(unit.villagers);
        self.raiders = self.raiders.minus(unit.raiders);
    }
}
