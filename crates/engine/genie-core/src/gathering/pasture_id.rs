//! A pasture's id as its own type: the building whose livestock a unit is in or tends.

use crate::objects::{Role, RoleId};

/// The role of a pasture: the building a unit is in as its livestock (`parent_pasture_id`,
/// `+0x790`), the one a unit tends (`my_pasture_id`, `+0x794`) and the one a building's stack
/// unit made at its completion (`+0xcb0`).
#[derive(Clone, Copy, Debug)]
pub enum Pasture {}

impl Role for Pasture {
    const NAME: &'static str = "PastureId";
}

/// A pasture's id ([`Pasture`]).
pub type PastureId = RoleId<Pasture>;
