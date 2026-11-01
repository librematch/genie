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
use self::world_view::ORDER_WINDOW;
pub use self::world_view::{Ask, Battlefield, Searcher, Thing};
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::objects::masters::class;
use crate::objects::masters::ids;

/// `FUN_140c4c8e0`: the attack category of an object of master `master` and class `class`, by
/// which `bestUnitToAttack` weighs and narrows its choice; none (−1) for a class it does not
/// rank.
#[must_use]
pub const fn category(master: MasterId, class: ClassId) -> Option<AttackCategory> {
    match class {
        class::CLASS_0 | class::CLASS_23 | class::CLASS_26 | class::CLASS_36 | class::CLASS_44 => {
            if matches!(master, ids::ID_6 | ids::ID_7) {
                Some(AttackCategory::named(4))
            } else {
                Some(AttackCategory::named(3))
            }
        }
        class::TRADE_BOAT | class::FISHING_SHIP => Some(AttackCategory::named(11)),
        class::BUILDING
        | class::WALL
        | class::GATE
        | class::FARM
        | class::CLASS_51
        | class::TOWER => Some(AttackCategory::named(0)),
        class::VILLAGER | class::TRADE_CART | class::LIVESTOCK | class::KING => {
            Some(AttackCategory::named(7))
        }
        class::CLASS_6
        | class::CLASS_12
        | class::CLASS_24
        | class::CLASS_35
        | class::CLASS_45
        | class::CLASS_47
        | class::CLASS_50
        | class::CLASS_56
        | class::CLASS_57 => {
            if matches!(master, ids::ID_1744 | ids::ID_1746) {
                Some(AttackCategory::named(5))
            } else {
                Some(AttackCategory::named(1))
            }
        }
        class::SIEGE | class::CLASS_54 | class::CLASS_55 => {
            if matches!(
                master,
                ids::ID_35 | ids::ID_422 | ids::ID_548 | ids::ID_1258
            ) {
                Some(AttackCategory::named(5))
            } else {
                Some(AttackCategory::SIEGE)
            }
        }
        class::CLASS_17 | class::MONK | class::CLASS_43 => Some(AttackCategory::named(8)),
        class::TRANSPORT | class::CLASS_62 => Some(AttackCategory::named(10)),
        class::WARSHIP | class::CLASS_53 => Some(AttackCategory::named(9)),
        class::CLASS_28 | class::CLASS_46 => Some(AttackCategory::named(2)),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
