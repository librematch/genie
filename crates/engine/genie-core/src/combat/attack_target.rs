//! An Attack's target as its own type: the object it attacks.

use crate::objects::{Role, RoleId};

/// The role of an Attack's target and its second target (`+0x20`, `+0x28`): the object it
/// attacks, whose hits it deals ([`crate::combat::damage::Victim`], what a hit reads and changes
/// of it). The work command's Attack takes its target as both.
#[derive(Clone, Copy, Debug)]
pub enum AttackTarget {}

impl Role for AttackTarget {
    const NAME: &'static str = "AttackTargetId";
}

/// An Attack's target's id ([`AttackTarget`]).
pub type AttackTargetId = RoleId<AttackTarget>;
