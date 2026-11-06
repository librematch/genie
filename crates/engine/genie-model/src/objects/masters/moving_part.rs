//! A master's moving layer: the RGE moving master's values (types 30 and up but 90, the `.dat`'s
//! type-30 section; FD-046-D05).

use crate::objects::SpriteId;
use crate::objects::masters::Master;
use crate::objects::masters::Trail;
use crate::world::Seconds;
use genie_diff::Diff;
use serde::Deserialize;
use serde::Serialize;

/// The moving layer of a master of type 30 and up but 90 (`RGE_Master_Moving_Object`,
/// `+0x748`..`+0x764`; the `.dat`'s type-30 section, genieutils' `DeadFish`): its walking and
/// running sprites, its turn speed and its trail. A moving object (30) and every type above hold
/// it; what its objects move with is [`Master::move_stats`], which these values and the base's
/// follow.
#[derive(Clone, Copy, Debug, Default, PartialEq, Diff, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct MovingPart {
    /// Its walking sprite (`+0x748`, `move_sprite_id`; the unit's slot 0x7c8) and its running
    /// one (`+0x750`, `run_sprite_id`; slot 0x7d0), −1 for none.
    pub move_sprite: Option<SpriteId>,
    /// Its running sprite (`+0x750`), −1 for none.
    pub run_sprite: Option<SpriteId>,
    /// `+0x758`, `turn_speed` (the `.dat`'s `rotation_speed`), which
    /// [`crate::movement::MoveStats::turn_time`] follows.
    pub turn_speed: Seconds,
    /// A moving master's trail (`+0x75e`..`+0x764`).
    pub trail: Trail,
}

impl MovingPart {
    /// What a master without the layer reads ([`Master::moving_part`]): no sprites, no turn speed
    /// and no trail, the import's values for a unit without the type-30 section.
    pub const ABSENT: Self = Self {
        move_sprite: None,
        run_sprite: None,
        turn_speed: Seconds::ZERO,
        trail: Trail::NONE,
    };
}

impl Master {
    /// Its moving layer, or for a master of another type the values a master without it reads
    /// ([`MovingPart::ABSENT`]).
    #[must_use]
    pub fn moving_part(&self) -> &MovingPart {
        self.moving.as_ref().unwrap_or(&MovingPart::ABSENT)
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::Master;
    use crate::objects::masters::MasterKind;
    use crate::objects::masters::MovingPart;
    use crate::world::Seconds;

    /// The types that hold the moving layer among [10, 20, 25, 30, 40, 60, 70, 80, 90]: 90, the
    /// data's own type, does not.
    #[test]
    fn masters_hold_the_moving_layer_of_their_kind() {
        let layered = |kind| Master::of_kind(MasterKind::of_data(kind)).moving.is_some();
        assert_eq!(
            [10, 20, 25, 30, 40, 60, 70, 80, 90].map(layered),
            [false, false, false, true, true, true, true, true, false]
        );
    }

    /// A master without the layer reads no sprites, no turn speed and no trail, as the import
    /// gave a unit without the type-30 section.
    #[test]
    fn masters_without_the_moving_layer_read_no_sprites() {
        let animated = Master::of_kind(MasterKind::ANIMATED);
        let part = animated.moving_part();
        assert_eq!((part.move_sprite, part.run_sprite), (None, None));
        assert_eq!(part.turn_speed, Seconds::ZERO);
        assert_eq!(part, &MovingPart::default());
    }
}
