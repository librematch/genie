//! Building and repairing: the Build and Repair actions with each worker's part of its update,
//! the wait for a foundation, a building's own update and the gates.

pub mod build;
mod builder_count;
pub(crate) mod building_update;
mod footprint_waits;
pub(crate) mod gates;
pub mod repair;
pub mod wait_solidify;
mod wait_solidify_state;

pub use self::builder_count::BuilderCount;
pub use self::footprint_waits::FootprintWaits;
