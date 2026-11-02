//! A Hunt's target as its own type: the animal it hunts or leads.

use crate::objects::{Role, RoleId};

/// The role of a Hunt's target (`+0x20`): the animal it hunts, or the livestock of its own
/// player it leads to a drop site. The work command's target the hunt reads
/// ([`crate::gathering::hunt::Quarry`]) takes it.
#[derive(Clone, Copy, Debug)]
pub enum HuntTarget {}

impl Role for HuntTarget {
    const NAME: &'static str = "HuntTargetId";
}

/// A Hunt's target's id ([`HuntTarget`]).
pub type HuntTargetId = RoleId<HuntTarget>;
