//! An action's slot 0x50 across a task change: the action's task and the new master's task that
//! takes its place, whose sprites an Attack and a `MoveTo` swap ([`TaskChange`]).

use crate::objects::Graphics;
use crate::objects::SpriteId;
use crate::objects::masters::TaskSprites;
use crate::objects::sprite::Graphic;

/// The tasks of a task change (`FUN_140a78150`, an upgrade's or a changed sprite's): the sprites
/// of the action's task (`old`) and of the new master's task that takes its place (`next`). An
/// action's slot 0x50 takes them as its third and fourth arguments; slot 0x90 with 1 and an
/// action without a task pass none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TaskChange {
    /// The action's task's sprites.
    pub(crate) old: TaskSprites,
    /// The sprites of the new master's task that takes its place.
    pub(crate) next: TaskSprites,
}

/// A task's sprite that a slot 0x50 compares with the one an action holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TaskSprite {
    /// `+0x40`, the sprite while it walks to its target.
    Moving,
    /// `+0x48`, the sprite while it comes on.
    Proceeding,
    /// `+0x50`, the sprite while it works.
    Working,
}

impl TaskSprite {
    /// This sprite of a task's `sprites`.
    const fn of(self, sprites: TaskSprites) -> Option<SpriteId> {
        match self {
            Self::Moving => sprites.moving,
            Self::Proceeding => sprites.proceeding,
            Self::Working => sprites.working,
        }
    }
}

/// What a slot 0x50 holds in place of the sprite `shown`: with `tasks`, the new task's sprite of
/// the first of `slots` whose old task's sprite `shown` is (none where the new task has none or
/// `graphics` lacks it, a null pointer); else `otherwise`. The game compares the pointers, so a
/// sprite `graphics` lacks matches nothing.
pub(crate) fn swapped(
    shown: Graphic,
    tasks: Option<TaskChange>,
    slots: &[TaskSprite],
    otherwise: Option<Graphic>,
    graphics: &Graphics,
) -> Option<Graphic> {
    let Some(change) = tasks else {
        return otherwise;
    };
    slots
        .iter()
        .find(|slot| graphics.resolve(slot.of(change.old)) == Some(shown))
        .map_or(otherwise, |slot| graphics.resolve(slot.of(change.next)))
}

#[cfg(test)]
mod tests;
