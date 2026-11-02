//! A missile's target as its own type: the object it was fired at.

use crate::objects::{Role, RoleId};

/// The role of a missile's target (`+0x20`): the object it was fired at, the volley's aim
/// ([`crate::combat::volley::Aimed`]). The objects it meets on its way are any objects.
#[derive(Clone, Copy, Debug)]
pub enum MissileTarget {}

impl Role for MissileTarget {
    const NAME: &'static str = "MissileTargetId";
}

/// A missile's target's id ([`MissileTarget`]).
pub type MissileTargetId = RoleId<MissileTarget>;
