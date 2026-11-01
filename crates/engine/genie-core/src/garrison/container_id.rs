//! A container's id as its own type: the object a unit is inside, enters or leaves.

use crate::objects::{Role, RoleId};

/// The role of a container: the building, ship or siege engine a unit is inside (its
/// `inside_obj`, `+0x2f0`), enters, is let out of or unloaded from.
#[derive(Clone, Copy, Debug)]
pub enum Container {}

impl Role for Container {
    const NAME: &'static str = "ContainerId";
}

/// A container's id ([`Container`]).
pub type ContainerId = RoleId<Container>;
