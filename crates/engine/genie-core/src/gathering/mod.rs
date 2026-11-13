//! Gathering resources: the Gather and Hunt actions with each gatherer's part of its update, a
//! farm's food, the searches for a resource, the drop sites and drop-offs, the mill's and the dock's
//! queues and the pastures.

use crate::objects::lifecycle::Held;
use crate::players::Amount;
use crate::players::PlayerKind;
use crate::players::attribute::AttributeId;
use crate::world::settings::Settings;

mod action_end;
pub(crate) mod closest_resource;
mod decay_timer;
pub(crate) mod drop_off;
pub mod drop_site_id;
pub(crate) mod drop_sites;
pub mod farm_queue;
mod farm_stops;
pub mod gather;
pub mod gather_search;
mod gather_target;
pub mod hunt;
mod hunt_target;
pub mod make;
mod make_state;
pub(crate) mod mule_cart;
pub mod pasture;
mod pasture_id;
mod queue_count;
mod salvage_amounts;
#[cfg(test)]
pub(crate) mod tests;

pub use self::action_end::ActionEnd;
pub use self::decay_timer::DecayTimer;
pub use self::drop_site_id::DropSiteId;
pub use self::farm_stops::{FarmStops, StopIndex};
pub use self::gather_target::{GatherTarget, GatherTargetId};
pub use self::hunt_target::{HuntTarget, HuntTargetId};
pub use self::pasture_id::{Pasture, PastureId};
pub use self::queue_count::QueueCount;
pub use self::salvage_amounts::SalvageAmounts;

/// Both loads are food (0, or the foods a unit carries, 15 to 17; [`AttributeId::is_food_load`]):
/// a gatherer or a hunter keeps a load of one for another. None is no food.
pub(crate) fn both_food(load: Option<AttributeId>, other_load: Option<AttributeId>) -> bool {
    load.is_some_and(AttributeId::is_food_load) && other_load.is_some_and(AttributeId::is_food_load)
}

/// What a gatherer or a hunter holds as its Gather or its Hunt begins, for the task's resource
/// `kind`: the first part of their slot 0x28 (`first_in_stack`), which `0x140a8b3a0` and
/// `0x140ac49b0` (there only on a target that holds something) inline alike. A load of the
/// task's kind stays. A load of another food ([`both_food`]) takes the task's kind and keeps its
/// amount. A load of another resource takes the task's kind with nothing held where the game
/// settings' test says so (`FUN_141302150`, [`Settings::carry_rule`]), or where the owner of
/// `player_kind` is a computer player (`FUN_141659cf0`) that carries nothing (`UCOMISS` with
/// `JP`: a NaN is not nothing); else it stays. `FUN_14119d310` writes the kind and the amount.
pub(crate) fn load_for_task(
    held: Held,
    kind: Option<AttributeId>,
    settings: &Settings,
    player_kind: PlayerKind,
) -> Held {
    if held.kind == kind {
        return held;
    }
    if both_food(kind, held.kind) {
        return Held { kind, ..held };
    }
    let carries_nothing = player_kind.is_computer() && held.amount == Amount::ZERO;
    if settings.carry_rule() || carries_nothing {
        return Held {
            kind,
            amount: Amount::ZERO,
        };
    }
    held
}
