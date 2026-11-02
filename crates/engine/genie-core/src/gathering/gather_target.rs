//! A Gather's target as its own type: the resource it works.

use crate::objects::{Role, RoleId};

/// The role of a Gather's target (`+0x20`): the resource it works (a tree, a mine, a bush, a
/// farm, a fish, a carcass). The work command's target the gather reads
/// ([`crate::gathering::gather::Resource`]) takes it, or the second target's role
/// ([`crate::gathering::DropSiteId`]) when a loaded gatherer is sent to its player's drop site.
#[derive(Clone, Copy, Debug)]
pub enum GatherTarget {}

impl Role for GatherTarget {
    const NAME: &'static str = "GatherTargetId";
}

/// A Gather's target's id ([`GatherTarget`]).
pub type GatherTargetId = RoleId<GatherTarget>;
