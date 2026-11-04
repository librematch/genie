//! A task's four sprites ([`TaskSprites`]).

use crate::objects::SpriteId;
use genie_diff::Diff;
use serde::Deserialize;
use serde::Serialize;

/// A task's moving, proceeding, working and carrying sprites (`+0x40`..`+0x58`, the `.dat`'s
/// graphic ids), each None for −1.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Diff, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TaskSprites {
    /// `+0x40`: the sprite while it walks to its target.
    pub moving: Option<SpriteId>,
    /// `+0x48`: the sprite while it comes on (an attack's swing, a repair's work).
    pub proceeding: Option<SpriteId>,
    /// `+0x50`: the sprite while it works.
    pub working: Option<SpriteId>,
    /// `+0x58`: the sprite while it carries what it gathered.
    pub carrying: Option<SpriteId>,
}

impl TaskSprites {
    /// A task without sprites (each −1).
    pub const NONE: Self = Self::of_data([-1; 4]);

    /// The four in the data's order: moving, proceeding, working, carrying.
    #[must_use]
    pub const fn ids(self) -> [Option<SpriteId>; 4] {
        [self.moving, self.proceeding, self.working, self.carrying]
    }

    /// The sprites of the save's or the `.dat`'s four ids in their order (−1 or below none).
    #[must_use]
    pub const fn of_data([moving, proceeding, working, carrying]: [i16; 4]) -> Self {
        Self {
            moving: SpriteId::of_data(moving),
            proceeding: SpriteId::of_data(proceeding),
            working: SpriteId::of_data(working),
            carrying: SpriteId::of_data(carrying),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::SpriteId;
    use crate::objects::masters::TaskSprites;

    /// The four ids keep their order; −1 is none.
    #[test]
    fn task_sprites_keep_their_order() {
        let task_sprites = TaskSprites::of_data([1, -1, 3, 4]);
        assert_eq!(
            (
                task_sprites.moving,
                task_sprites.proceeding,
                task_sprites.working,
                task_sprites.carrying
            ),
            (
                Some(SpriteId::named(1)),
                None,
                Some(SpriteId::named(3)),
                Some(SpriteId::named(4))
            )
        );
        assert_eq!(TaskSprites::of_data([-1; 4]), TaskSprites::default());
        assert_eq!(TaskSprites::NONE, TaskSprites::default());
        assert_eq!(
            task_sprites.ids(),
            [
                task_sprites.moving,
                task_sprites.proceeding,
                task_sprites.working,
                task_sprites.carrying
            ],
            "the data's order"
        );
    }
}
