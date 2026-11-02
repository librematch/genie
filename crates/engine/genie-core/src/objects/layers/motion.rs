//! The layers between an object's place and its action layer (FD-045-D58): the animated
//! object's (types 20 to 29) and the moving object's (30 and up), which holds the animated one,
//! as the object's [`Motion`] in the world's table, and [`MotionMut`], the one a routine changes
//! beside the object's row.

use crate::movement::Mover;
use crate::movement::Speed;
use genie_diff::Diff;

/// `RGE_Animated_Object`'s own field (types 20 and up): `mCurrentSpeed` (`+0x7b8`), the speed the
/// object's sprite clock runs at. Its constructor (`FUN_141070cf0`) sets it to 0; a moving
/// unit's velocity sets it as it walks, and a stop clears it. The static layer below it is the
/// object's row's ([`Static`](crate::objects::layers::Static), `Unit::place`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Diff)]
pub struct Animated {
    /// `mCurrentSpeed` (`+0x7b8`).
    pub speed: Speed,
}

impl Animated {
    /// An animated layer at `speed`.
    #[must_use]
    pub const fn new(speed: Speed) -> Self {
        Self { speed }
    }
}

/// The layers of an object between its static layer (the row's place, [`Static`](crate::objects::layers::Static)) and its action
/// layer, by its type (FD-045-D58): the animated object (types 20 to 29), or the moving object
/// (30 and up), which holds the animated one. A static object (types below 20) has none: the
/// world's table ([`crate::world::parts::Parts::motion`]) holds no entry for it.
#[derive(Clone, Debug, PartialEq, Diff)]
#[expect(
    clippy::large_enum_variant,
    reason = "nearly every object of the table moves; a box per mover would cost more than the few animated ones save"
)]
pub enum Motion {
    /// An animated object that does not move (types 20 to 29).
    Animated(Animated),
    /// A moving object (types 30 and up), its animated layer inside its mover.
    Moving(Mover),
}

impl Motion {
    /// The layers of an object with `animated` and `mover` (each None where its type has none):
    /// a mover holds the animated layer (`animated`, else the mover's own), else the animated
    /// layer stands alone, else the object is static (none).
    #[must_use]
    pub fn of(animated: Option<Animated>, mover: Option<Mover>) -> Option<Self> {
        match (animated, mover) {
            (a, Some(mut m)) => {
                if let Some(a) = a {
                    m.animated = a;
                }
                Some(Self::Moving(m))
            }
            (Some(a), None) => Some(Self::Animated(a)),
            (None, None) => None,
        }
    }

    /// The moving layer, if the object moves.
    #[must_use]
    pub const fn mover(&self) -> Option<&Mover> {
        match self {
            Self::Moving(m) => Some(m),
            Self::Animated(_) => None,
        }
    }

    /// The moving layer to change, if the object moves.
    pub const fn mover_mut(&mut self) -> Option<&mut Mover> {
        match self {
            Self::Moving(m) => Some(m),
            Self::Animated(_) => None,
        }
    }

    /// The animated layer: its own, or its mover's.
    #[must_use]
    pub const fn animated(&self) -> &Animated {
        match self {
            Self::Animated(a) | Self::Moving(Mover { animated: a, .. }) => a,
        }
    }

    /// The animated layer to change.
    pub const fn animated_mut(&mut self) -> &mut Animated {
        match self {
            Self::Animated(a) | Self::Moving(Mover { animated: a, .. }) => a,
        }
    }
}

#[cfg(test)]
impl Motion {
    /// A test's own layer, apart from a world, to change through [`MotionMut`].
    pub(crate) const fn lent(&mut self) -> MotionMut<'_> {
        MotionMut(Some(self))
    }
}

/// An object's motion layer to change beside its row, none for a static object (the world's
/// table lends it, [`crate::world::parts::Parts::motion_mut`]): what a routine that stops,
/// shows or speeds its object reaches of the layers above its place.
#[derive(Debug)]
pub struct MotionMut<'a>(Option<&'a mut Motion>);

impl<'a> MotionMut<'a> {
    /// The layer `motion` to change (none for a static object).
    #[must_use]
    pub const fn new(motion: Option<&'a mut Motion>) -> Self {
        Self(motion)
    }

    /// The moving layer, if the object moves.
    #[must_use]
    pub fn mover(&self) -> Option<&Mover> {
        self.0.as_deref().and_then(Motion::mover)
    }

    /// The moving layer to change, if the object moves.
    pub fn mover_mut(&mut self) -> Option<&mut Mover> {
        self.0.as_deref_mut().and_then(Motion::mover_mut)
    }

    /// The movement's values to change (its top speed among them), if the object moves.
    pub fn move_stats_mut(&mut self) -> Option<&mut crate::movement::MoveStats> {
        self.mover_mut().map(|m| &mut m.stats)
    }

    /// The animation speed (`+0x7b8`), none for a static object.
    #[must_use]
    pub fn anim_speed(&self) -> Option<Speed> {
        self.0.as_deref().map(|m| m.animated().speed)
    }

    /// Sets the animation speed (`+0x7b8`) of an object that has one; a static object has none.
    pub fn set_anim_speed(&mut self, speed: Speed) {
        if let Some(m) = self.0.as_deref_mut() {
            m.animated_mut().speed = speed;
        }
    }

    /// The same layer for a call that ends before this one is used again.
    pub fn reborrow(&mut self) -> MotionMut<'_> {
        MotionMut(self.0.as_deref_mut())
    }

    /// The mover for as long as the layer was lent, none for an object that does not move.
    #[must_use]
    pub fn into_mover(self) -> Option<&'a mut Mover> {
        self.0.and_then(Motion::mover_mut)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mover() -> Mover {
        crate::movement::tests::builder_9177()
    }

    #[test]
    fn a_motion_holds_the_layers_its_type_has() {
        let a = Animated::new(Speed::new(0.5));
        assert_eq!(Motion::of(None, None), None, "a static object has none");
        assert_eq!(
            Motion::of(Some(a), None).map(|m| m.animated().speed),
            Some(Speed::new(0.5))
        );
        let moving = Motion::of(Some(a), Some(mover())).unwrap();
        assert_eq!(
            moving.mover().map(|m| m.animated.speed),
            Some(Speed::new(0.5))
        );
        assert_eq!(moving.animated().speed, Speed::new(0.5));
        let mut own = mover();
        own.animated.speed = Speed::new(0.25);
        assert_eq!(
            Motion::of(None, Some(own)).map(|m| m.animated().speed),
            Some(Speed::new(0.25)),
            "a mover always has an animated layer, its own without another"
        );
    }

    #[test]
    fn only_a_moving_object_has_a_mover() {
        let mut flag = Motion::Animated(Animated::default());
        flag.animated_mut().speed = Speed::new(1.0);
        assert_eq!(
            (flag.animated().speed, flag.mover()),
            (Speed::new(1.0), None)
        );
        let mut walker = Motion::Moving(mover());
        walker.animated_mut().speed = Speed::new(2.0);
        assert_eq!(
            walker.mover_mut().map(|m| m.animated.speed),
            Some(Speed::new(2.0))
        );
    }
}
