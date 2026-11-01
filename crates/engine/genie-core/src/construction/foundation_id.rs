//! A foundation's id as its own type: the building a builder builds and its footprint clears.

use crate::objects::{Role, RoleId};

/// The role of a foundation: the building under construction that a Build works, BUILD's
/// builders are sent to and whose footprint its units are sent off.
#[derive(Clone, Copy, Debug)]
pub enum Foundation {}

impl Role for Foundation {
    const NAME: &'static str = "FoundationId";
}

/// A foundation's id ([`Foundation`]).
pub type FoundationId = RoleId<Foundation>;
