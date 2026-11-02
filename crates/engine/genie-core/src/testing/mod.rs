//! What genie-core's tests build their game data from: each build's layer as the generated
//! fixture states it (FD-046-D08), so a test needs no Nickel, and the classes the tests name.

pub mod build_158041;
pub mod class;

use crate::testing::build_158041::build_158041;
use crate::world::layer::Roles;
use crate::world::layer::Slots;
use std::sync::LazyLock;

/// Build 158041's roles, which a test passes where the engine reads the layer's.
#[must_use]
pub fn roles() -> Roles {
    build_158041().roles
}

/// Build 158041's slots, which a test passes where the engine reads the layer's.
#[must_use]
pub fn slots() -> Slots {
    build_158041().slots
}

/// Build 158041's roles for a view that lives as long as the test ([`roles`] once).
#[must_use]
pub fn static_roles() -> &'static Roles {
    static ROLES: LazyLock<Roles> = LazyLock::new(roles);
    &ROLES
}
