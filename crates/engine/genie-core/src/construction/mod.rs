//! Building and repairing: the Build and Repair actions with each worker's part of its update,
//! the wait for a foundation, a building's own update and the gates.

mod annexes;
pub mod build;
mod builder_count;
pub(crate) mod building_update;
mod footprint_waits;
mod foundation_id;
pub(crate) mod gates;
mod main_building_id;
mod outcomes;
pub mod repair;
mod repair_target;
pub(crate) mod speedup;
#[cfg(test)]
mod speedup_tests;
pub mod wait_solidify;
mod wait_solidify_state;

pub use self::annexes::Annexes;
pub use self::builder_count::BuilderCount;
pub use self::footprint_waits::FootprintWaits;
pub use self::foundation_id::{Foundation, FoundationId};
pub(crate) use self::main_building_id::MAIN_BUILDING_GONE;
pub use self::main_building_id::{MainBuilding, MainBuildingId};
pub use self::outcomes::{BuildProgress, ConstructionStart};
pub use self::repair_target::{RepairTarget, RepairTargetId};
