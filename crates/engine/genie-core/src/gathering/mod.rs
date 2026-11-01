//! Gathering resources: the Gather and Hunt actions with each gatherer's part of its update, a
//! farm's food, the searches for a resource, the drop sites and drop-offs, the mill's and the dock's
//! queues and the pastures.

use crate::players::attribute::AttributeId;

pub(crate) mod closest_resource;
pub(crate) mod drop_off;
pub(crate) mod drop_sites;
pub mod farm_queue;
pub mod gather;
pub mod gather_search;
pub mod hunt;
pub mod make;
mod make_state;
pub(crate) mod mule_cart;
pub mod pasture;
mod queue_count;

pub use self::queue_count::QueueCount;

/// Both loads are food (0, or the foods a unit carries, 15 to 17; [`AttributeId::is_food_load`]):
/// a gatherer or a hunter keeps a load of one for another. None is no food.
pub(crate) fn both_food(a: Option<AttributeId>, b: Option<AttributeId>) -> bool {
    a.is_some_and(AttributeId::is_food_load) && b.is_some_and(AttributeId::is_food_load)
}
