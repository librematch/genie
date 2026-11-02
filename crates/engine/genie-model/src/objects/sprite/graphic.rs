//! A sprite's definition and the speed it gives a mover (FD-046: the model's part of the sprites).

use crate::movement::Speed;
use crate::objects::SpeedMultiplier;
use crate::objects::SpriteFlags;
use crate::objects::SpriteId;
use crate::objects::{FacetCount, Frame};
use crate::world::Seconds;
use genie_diff::Diff;

/// What the clock reads of a sprite (the `.dat` graphic, memory `+0x8`..`+0x1a`).
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Graphic {
    /// The graphic's id.
    pub id: SpriteId,
    /// `+0x8`, `frame_count`.
    pub frames: Frame,
    /// `+0xa`, `angle_count`: the facets.
    pub facets: FacetCount,
    /// `+0xc`, `speed_multiplier`: at least 1e-7, it scales the frame duration.
    pub speed_multiplier: SpeedMultiplier,
    /// `+0x10`, `frame_duration`.
    pub frame_duration: Seconds,
    /// `+0x14`, `replay_delay`.
    pub replay_delay: Seconds,
    /// `+0x1a`, `sequence_type`.
    pub flags: SpriteFlags,
}

impl Graphic {
    /// Whether it plays its frames (its sequence type's bit [`SpriteFlags::ANIMATED`]).
    #[must_use]
    pub const fn animated(&self) -> bool {
        self.flags.contains(SpriteFlags::ANIMATED)
    }

    /// Whether it plays its frames backwards (its sequence type's bit
    /// [`SpriteFlags::BACKWARDS`]).
    #[must_use]
    pub const fn backwards(&self) -> bool {
        self.flags.contains(SpriteFlags::BACKWARDS)
    }

    /// The first frame: the last one for a sprite played backwards.
    #[must_use]
    pub fn first_frame(&self) -> Frame {
        if self.backwards() {
            self.frames - Frame::ONE
        } else {
            Frame::ZERO
        }
    }
}

/// A moving object's top speed (slot 65, `FUN_14114d1d0`) with the sprite `shown`: its master's
/// speed `base` (slot 0x1d0's `+0x740`), times the sprite's speed factor (`+0xc`) when that is
/// above 0 (`COMISS`, `JBE`: a NaN is not). The game reads it from the current sprite (`+0x140`)
/// each time; genie derives `MoveStats::speed` from it where the sprite or the base changes
/// (`new_sprite`).
#[must_use]
pub fn top_speed(base: Speed, shown: Option<&Graphic>) -> Speed {
    shown.map_or(base, |g| g.speed_multiplier.applied_to(base))
}
