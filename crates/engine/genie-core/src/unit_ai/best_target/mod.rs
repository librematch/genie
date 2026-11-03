//! `UnitAIModule::bestUnitToAttack` (158041 `0x140c44680`) and what it keeps: the attack
//! categories (`FUN_140c4c8e0`) by which a unit AI's order history weighs its choice.

mod attack_category;
mod choice;
mod statistics;
mod values;
mod world_view;

pub use self::attack_category::{AttackCategory, ByCategory};
pub use self::choice::{Choice, best_unit_to_attack};
use self::statistics::EPSILON;
pub use self::statistics::{
    Bounded, Bounds, Candidate, Entry, STATISTICS, Statistics, order_weights,
};
pub use self::values::{EntryCount, KillTime, OrderWeight, TargetScore};
pub use self::world_view::{Ask, Battlefield, Searcher, Thing};
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;

/// `FUN_140c4c8e0`: the attack category of an object of master `master` and class `class`, by
/// which `bestUnitToAttack` weighs and narrows its choice; none (−1) for a class it does not
/// rank.
#[must_use]
pub fn category(master: MasterId, class: ClassId, roles: &Roles) -> Option<AttackCategory> {
    {
        let held = roles.of_class(class);
        if held.intersects(
            ClassRoles::ARCHERY
                | ClassRoles::CAVALRY_CANNON
                | ClassRoles::ELEPHANT_ARCHER
                | ClassRoles::CAVALRY_ARCHER
                | ClassRoles::ARCHERY_CANNON,
        ) {
            if roles
                .of_master(master)
                .intersects(MasterRoles::ELITE_SKIRMISHER | MasterRoles::SKIRMISHER)
            {
                Some(AttackCategory::named(4))
            } else {
                Some(AttackCategory::named(3))
            }
        } else if held.intersects(ClassRoles::TRADE_BOAT | ClassRoles::FISHING_SHIP) {
            Some(AttackCategory::named(11))
        } else if held.intersects(
            ClassRoles::BUILDING
                | ClassRoles::WALL
                | ClassRoles::GATE
                | ClassRoles::FARM
                | ClassRoles::PACKED_TREBUCHET
                | ClassRoles::TOWER,
        ) {
            Some(AttackCategory::named(0))
        } else if held.intersects(
            ClassRoles::VILLAGER
                | ClassRoles::TRADE_CART
                | ClassRoles::LIVESTOCK
                | ClassRoles::KING,
        ) {
            Some(AttackCategory::named(7))
        } else if held.intersects(
            ClassRoles::INFANTRY
                | ClassRoles::CAVALRY
                | ClassRoles::WAR_ELEPHANT
                | ClassRoles::PETARD
                | ClassRoles::TWO_HANDED_SWORDSMAN
                | ClassRoles::SCOUT_CAVALRY
                | ClassRoles::SPEARMAN
                | ClassRoles::RAIDER
                | ClassRoles::CAVALRY_RAIDER,
        ) {
            if roles
                .of_master(master)
                .intersects(MasterRoles::ARMORED_ELEPHANT | MasterRoles::SIEGE_ELEPHANT)
            {
                Some(AttackCategory::named(5))
            } else {
                Some(AttackCategory::named(1))
            }
        } else if held
            .intersects(ClassRoles::SIEGE | ClassRoles::UNPACKED_TREBUCHET | ClassRoles::SCORPION)
        {
            if roles.of_master(master).intersects(
                MasterRoles::BATTERING_RAM
                    | MasterRoles::CAPPED_RAM
                    | MasterRoles::SIEGE_RAM
                    | MasterRoles::BATTERING_RAM_1258,
            ) {
                Some(AttackCategory::named(5))
            } else {
                Some(AttackCategory::SIEGE)
            }
        } else if held
            .intersects(ClassRoles::HEALER | ClassRoles::MONK | ClassRoles::MONK_WITH_RELIC)
        {
            Some(AttackCategory::named(8))
        } else if held.intersects(ClassRoles::TRANSPORT | ClassRoles::TRANSPORT_CATEGORY) {
            Some(AttackCategory::named(10))
        } else if held.intersects(ClassRoles::WARSHIP | ClassRoles::BOARDING_SHIP) {
            Some(AttackCategory::named(9))
        } else if held.intersects(ClassRoles::PHALANX | ClassRoles::PIKEMAN) {
            Some(AttackCategory::named(2))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests;
