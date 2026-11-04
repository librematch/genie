//! Gathering resources: the Gather and Hunt actions with each gatherer's part of its update, a
//! farm's food, the searches for a resource, the drop sites and drop-offs, the mill's and the dock's
//! queues and the pastures.

use crate::players::attribute::AttributeId;

mod action_end;
pub(crate) mod closest_resource;
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

pub use self::action_end::ActionEnd;
pub use self::drop_site_id::DropSiteId;
pub use self::farm_stops::{FarmStops, StopIndex};
pub use self::gather_target::{GatherTarget, GatherTargetId};
pub use self::hunt_target::{HuntTarget, HuntTargetId};
pub use self::pasture_id::{Pasture, PastureId};
pub use self::queue_count::QueueCount;

/// Both loads are food (0, or the foods a unit carries, 15 to 17; [`AttributeId::is_food_load`]):
/// a gatherer or a hunter keeps a load of one for another. None is no food.
pub(crate) fn both_food(load: Option<AttributeId>, other_load: Option<AttributeId>) -> bool {
    load.is_some_and(AttributeId::is_food_load) && other_load.is_some_and(AttributeId::is_food_load)
}
