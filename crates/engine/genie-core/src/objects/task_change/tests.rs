//! A slot 0x50's swap of an action's sprite across a task change.

use crate::objects::Graphics;
use crate::objects::masters::TaskSprites;
use crate::objects::sprite::Graphic;
use crate::objects::task_change::{TaskChange, TaskSprite, swapped};
use crate::objects::{FacetCount, Frame, SpeedMultiplier, SpriteFlags, SpriteId};
use crate::world::Seconds;

/// The graphic of sprite `id`.
const fn graphic(id: i16) -> Graphic {
    Graphic {
        id: SpriteId::named(id),
        frames: Frame::of_data(10),
        facets: FacetCount::of_data(1),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::NONE,
    }
}

/// The graphics of sprites 1 to 4 and 11 to 13.
fn graphics() -> Graphics {
    Graphics::from([1, 2, 3, 4, 11, 12, 13].map(|id| (SpriteId::named(id), graphic(id))))
}

/// A task change from the sprites (1, 2, 3) to (11, 12, 13), the carrying ones 4 and 14.
const CHANGE: TaskChange = TaskChange {
    old: TaskSprites::of_data([1, 2, 3, 4]),
    next: TaskSprites::of_data([11, 12, 13, 14]),
};

/// Without tasks the sprite takes the other one; with them a sprite of the old task's slots
/// takes the new task's of the same slot, the first slot that holds it first.
#[test]
fn sprite_of_old_task_takes_the_new_task_sprite_of_its_slot() {
    let graphics = graphics();
    let both = [TaskSprite::Proceeding, TaskSprite::Working];
    let otherwise = Some(graphic(4));
    assert_eq!(
        [
            swapped(
                graphic(1),
                None,
                &[TaskSprite::Moving],
                otherwise,
                &graphics
            ),
            swapped(
                graphic(1),
                Some(CHANGE),
                &[TaskSprite::Moving],
                otherwise,
                &graphics
            ),
            swapped(graphic(2), Some(CHANGE), &both, otherwise, &graphics),
            swapped(graphic(3), Some(CHANGE), &both, otherwise, &graphics),
        ],
        [
            otherwise,
            Some(graphic(11)),
            Some(graphic(12)),
            Some(graphic(13))
        ]
    );
    let same = TaskChange {
        old: TaskSprites::of_data([1, 2, 2, 4]),
        ..CHANGE
    };
    assert_eq!(
        swapped(graphic(2), Some(same), &both, otherwise, &graphics),
        Some(graphic(12)),
        "the first slot"
    );
}

/// A sprite none of the slots holds takes the other one (the carrying sprite is none of them);
/// a new task's sprite the table lacks is none, and an old task's sprite the table lacks (a null
/// pointer) matches nothing.
#[test]
fn sprite_not_of_old_task_takes_other_one() {
    let graphics = graphics();
    let otherwise = Some(graphic(4));
    let all = [
        TaskSprite::Moving,
        TaskSprite::Proceeding,
        TaskSprite::Working,
    ];
    assert_eq!(
        swapped(graphic(4), Some(CHANGE), &all, None, &graphics),
        None,
        "the carrying sprite"
    );
    let unlinked_next = TaskChange {
        next: TaskSprites::of_data([21, 12, 13, 14]),
        ..CHANGE
    };
    assert_eq!(
        swapped(graphic(1), Some(unlinked_next), &all, otherwise, &graphics),
        None
    );
    let unlinked_old = TaskChange {
        old: TaskSprites::of_data([5, 2, 3, 4]),
        ..CHANGE
    };
    assert_eq!(
        swapped(graphic(5), Some(unlinked_old), &all, otherwise, &graphics),
        otherwise
    );
}
