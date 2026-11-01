//! A drop site's id as its own type: the object a worker carries its load to.

use crate::objects::{Role, RoleId};

/// The role of a drop site: the object a gatherer, a hunter or a builder carries its load to
/// (the action's second target, `+0x28`), the one a hunter leads its own livestock to and the
/// one a unit AI sends a load elsewhere to. Not the drop-site table's row
/// ([`crate::unit_ai::task_finder::DropSite`]), which names a master.
#[derive(Clone, Copy, Debug)]
pub enum DropSite {}

impl Role for DropSite {
    const NAME: &'static str = "DropSiteId";
}

/// A drop site's id ([`DropSite`]).
pub type DropSiteId = RoleId<DropSite>;
