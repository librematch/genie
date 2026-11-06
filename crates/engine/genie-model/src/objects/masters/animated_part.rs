//! A master's animated layer: the RGE animated master's values (types 20 and up but 90, the
//! `.dat`'s speed; FD-046-D05).

use crate::movement::Speed;
use crate::objects::masters::Master;
use genie_diff::Diff;
use serde::Deserialize;
use serde::Serialize;

/// The animated layer of a master of type 20 and up but 90 (`RGE_Master_Animated_Object`, `+0x740`;
/// the `.dat`'s `speed`, read for the types 20 and up): its top speed. An animated object (20), a
/// doppelganger (25), a moving one (30) and every type above hold it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Diff, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct AnimatedPart {
    /// `+0x740`, `mMaxSpeed` (the `.dat`'s `speed`): the top speed before a sprite's factor, from
    /// which [`Master::move_stats`]'s speeds and [`crate::combat::missile::MissileStats::speed`]
    /// follow.
    pub max_speed: Speed,
}

impl AnimatedPart {
    /// What a master without the layer reads ([`Master::animated_part`]): a top speed of 0, the
    /// import's value for a unit without the speed.
    pub const ABSENT: Self = Self {
        max_speed: Speed::ZERO,
    };
}

impl Master {
    /// Its animated layer, or for a master of another type the values a master without it reads
    /// ([`AnimatedPart::ABSENT`]).
    #[must_use]
    pub fn animated_part(&self) -> &AnimatedPart {
        self.animated.as_ref().unwrap_or(&AnimatedPart::ABSENT)
    }
}

#[cfg(test)]
mod tests {
    use crate::movement::Speed;
    use crate::objects::masters::AnimatedPart;
    use crate::objects::masters::Master;
    use crate::objects::masters::MasterKind;

    /// The types that hold the animated layer among [10, 15, 20, 25, 30, 40, 70, 80, 90]: 90, the
    /// data's own type, does not.
    #[test]
    fn masters_hold_the_animated_layer_of_their_kind() {
        let layered = |kind| {
            Master::of_kind(MasterKind::of_data(kind))
                .animated
                .is_some()
        };
        assert_eq!(
            [10, 15, 20, 25, 30, 40, 70, 80, 90].map(layered),
            [false, false, true, true, true, true, true, true, false]
        );
    }

    /// A master without the layer reads a top speed of 0, as the import gave a unit without the
    /// speed.
    #[test]
    fn masters_without_the_animated_layer_read_no_speed() {
        let still = Master::of_kind(MasterKind::STATIC);
        assert_eq!(still.animated_part().max_speed, Speed::ZERO);
        assert_eq!(still.animated_part(), &AnimatedPart::default());
    }
}
