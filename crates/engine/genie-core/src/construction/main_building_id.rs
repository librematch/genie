//! A main building's id as its own type: the building whose annexes are its parts.

use crate::GenieError;
use crate::objects::{Role, RoleId};

/// An annex whose main building the world does not hold.
pub(crate) const MAIN_BUILDING_GONE: GenieError =
    GenieError::unsupported("an annex whose main building is gone");

/// The role of a main building: the building an annex belongs to (the annex's `+0xbb8`, the
/// save's linked owner), which takes a hit on the annex, pays for its repair and keeps its
/// annexes' list (`+0xbc0`).
#[derive(Clone, Copy, Debug)]
pub enum MainBuilding {}

impl Role for MainBuilding {
    const NAME: &'static str = "MainBuildingId";
}

/// A main building's id ([`MainBuilding`]).
pub type MainBuildingId = RoleId<MainBuilding>;
