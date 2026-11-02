//! A Repair's target as its own type: the object it repairs.

use crate::objects::{Role, RoleId};

/// The role of a Repair's target (`+0x30`): the building, ship or siege weapon it repairs
/// ([`crate::construction::repair::Repairee`], what its update reads of it). Its repairers count
/// in it, or in its main building for an annex.
#[derive(Clone, Copy, Debug)]
pub enum RepairTarget {}

impl Role for RepairTarget {
    const NAME: &'static str = "RepairTargetId";
}

/// A Repair's target's id ([`RepairTarget`]).
pub type RepairTargetId = RoleId<RepairTarget>;
