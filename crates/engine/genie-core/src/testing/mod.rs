//! What genie-core's tests build their game data from: each build's layer as the generated
//! fixture states it (FD-046-D08), so a test needs no Nickel, and the classes, masters and
//! player attributes the tests name.

pub mod ai;
pub mod attribute;
pub mod build_158041;
pub mod class;
pub mod ids;
pub mod picker;
pub mod scene;

use crate::combat::damage::HitLayer;
use crate::testing::build_158041::build_158041;
use crate::world::layer::Roles;
use crate::world::layer::Slots;
use crate::world::layer::generated::Tuning;
use genie_model::model::Model;
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

/// Build 158041's tuning values, which a test passes where the engine reads the model's (built
/// once).
#[must_use]
pub fn tuning() -> &'static Tuning {
    static TUNING: LazyLock<Tuning> = LazyLock::new(|| build_158041().tuning);
    &TUNING
}

/// A model of no tables with build 158041's roles, slots and tuning values (built once), which a
/// test passes where the engine reads the world's model.
#[must_use]
pub fn model() -> &'static Model {
    static MODEL: LazyLock<Model> = LazyLock::new(|| Model::default().with_layer(&build_158041()));
    &MODEL
}

/// Build 158041's roles for a view that lives as long as the test ([`roles`] once).
#[must_use]
pub fn static_roles() -> &'static Roles {
    static ROLES: LazyLock<Roles> = LazyLock::new(roles);
    &ROLES
}

/// What a test's hit reads of build 158041's layer: its roles, slots and damage tuning.
#[must_use]
pub fn hit_layer() -> HitLayer<'static> {
    HitLayer {
        roles: static_roles(),
        slots: slots(),
        damage_tuning: &tuning().damage,
    }
}
