//! A shooter's id as its own type: the object a missile was fired from.

use crate::objects::{Role, RoleId};

/// The role of a shooter: the object a missile was fired from (the missile's `+0x998`,
/// `fired_from_id`, and its action's second target `+0x28`), whose damage notify the hit sends
/// and which a kill credits. The game keeps the id of a shooter that is gone.
#[derive(Clone, Copy, Debug)]
pub enum Shooter {}

impl Role for Shooter {
    const NAME: &'static str = "ShooterId";
}

/// A shooter's id ([`Shooter`]).
pub type ShooterId = RoleId<Shooter>;
