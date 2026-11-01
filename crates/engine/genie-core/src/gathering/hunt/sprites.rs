//! The hunt's sprites: standing, walking and the task's; an asked drop-off and the kind of
//! what a unit carries.

use crate::gathering::hunt::HuntTask;
use crate::objects::SpriteId;
use crate::objects::lifecycle::Held;
use crate::objects::sprite::Graphic;
use crate::players::attribute::{self, AttributeId};

/// `FUN_140ac5210`, the sprite a hunter stands with: the task's carrying sprite while it holds a
/// whole unit or more of the task's resource and the sprite exists, else its master's standing
/// sprite (`standing`).
pub(crate) fn stand_sprite(
    task: &HuntTask,
    standing: Option<SpriteId>,
    held: Held,
    graphics: &crate::objects::Graphics,
) -> Option<Graphic> {
    let g = |id: Option<SpriteId>| id.and_then(|s| graphics.get(s).copied());
    let carrying = task.sprites.carrying;
    if task.resource.is_some()
        && task.resource == held.kind
        && 1 <= held.amount.get() as i32
        && g(carrying).is_some()
    {
        g(carrying)
    } else {
        g(standing)
    }
}

/// Slot 0x78 (`FUN_140a78060`, the base action's; `UNIT_ORDER`'s drop-off) of an action whose
/// task takes `resource` (`+0x20`, any with none) for a unit carrying some of `held_kind`
/// (`+0x310` above 0, the caller's test): a load of the task's resource (compared by
/// [`resource_kind`]) asks the caller for `set_state(8)` (true); a load of another resource has
/// the caller's unit AI take notify 0x21c from and to the unit (its slot 0x38,
/// [`crate::unit_ai::NotifyKind::OTHER_LOAD`]) and returns false.
pub(crate) fn asked_drop_off(
    resource: Option<AttributeId>,
    held_kind: Option<AttributeId>,
    attributes: usize,
) -> bool {
    resource.is_none()
        || resource_kind(held_kind, attributes) == resource_kind(resource, attributes)
}

/// The player's resource of what a unit carries (`FUN_140fdd4f0`): the load's resource
/// ([`AttributeId::load_resource`], none for none), then food for none or for a resource beyond
/// the player's `attributes` (`FUN_140efe5b0`).
pub(crate) fn resource_kind(kind: Option<AttributeId>, attributes: usize) -> AttributeId {
    kind.and_then(AttributeId::load_resource)
        .filter(|k| k.index() < attributes)
        .unwrap_or(attribute::FOOD)
}

/// The hunt's sprites as graphics: the task's moving, working and carrying sprites, and the
/// master's standing sprite, which slot 0x68 takes for none.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HuntSprites {
    /// The moving sprite.
    pub moving: Option<Graphic>,
    /// The working sprite.
    pub working: Option<Graphic>,
    /// The carrying sprite.
    pub carrying: Option<Graphic>,
    /// The master's standing sprite.
    pub standing: Option<Graphic>,
}

impl HuntTask {
    /// The hunt's view of master task `t`.
    #[must_use]
    pub const fn of(t: &crate::objects::masters::Task) -> Self {
        Self {
            id: t.id,
            range: t.work_range,
            resource: t.attribute_type,
            sprites: t.sprites,
            work_value: t.work_val1,
            productivity: t.attribute_type2,
            kind4: t.attribute_type4,
        }
    }

    /// [`walk_sprite`]'s id from the task's sprites (`FUN_140ac51d0`), which the hunt's Attack
    /// takes for its `MoveTo` (`FUN_140a7c210`'s argument 4).
    #[must_use]
    pub fn walk_sprite_id(&self, held: Held) -> Option<SpriteId> {
        let carrying =
            self.resource.is_some() && held.kind == self.resource && 1 <= held.amount.get() as i32;
        if carrying && self.sprites.carrying.is_some() {
            self.sprites.carrying
        } else {
            self.sprites.moving
        }
    }
}

/// `FUN_140ac51d0`: the sprite a hunt's `MoveTo` shows (its `set_state(4)`, slot 0x68 with
/// `+0x58`): the task's carrying sprite while the unit carries at least 1 of the task's
/// resource, else its moving one; None for none.
#[must_use]
pub fn walk_sprite(
    resource: Option<AttributeId>,
    held: Held,
    sprites: &HuntSprites,
) -> Option<Graphic> {
    let carrying = resource.is_some() && held.kind == resource && 1 <= held.amount.get() as i32;
    match (carrying, sprites.carrying) {
        (true, Some(g)) => Some(g),
        _ => sprites.moving,
    }
}
