//! A task's two sound events ([`SoundEvents`]) and a sound event's id ([`SoundEventId`]).

use genie_diff::Diff;

/// A sound event by its id (`mEventId`, the audio engine's event): what a task's sounds name
/// (`+0x60`, `+0x78`). The `.dat`'s event 0 with sound files is the default event.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Diff)]
pub struct SoundEventId(i32);

impl SoundEventId {
    /// The event of id `id` (the game's 32-bit integer).
    #[must_use]
    pub const fn new(id: i32) -> Self {
        Self(id)
    }

    /// The id, as the task holds it.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

/// `+0x60` and `+0x78` of a task: the event ids of its gathering and its deposit sound, as the
/// task's load (`FUN_141062470`) leaves them. A terrain look task (159) names its dying and its
/// undead sprite with their ids ([`crate::combat::abilities`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Diff)]
pub struct SoundEvents {
    /// The gathering sound's event (`+0x60`).
    pub gathering: SoundEventId,
    /// The deposit sound's event (`+0x78`).
    pub deposit: SoundEventId,
}

impl SoundEvents {
    /// The events of ids `gathering` and `deposit`.
    #[must_use]
    pub const fn new(gathering: i32, deposit: i32) -> Self {
        Self {
            gathering: SoundEventId::new(gathering),
            deposit: SoundEventId::new(deposit),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::{SoundEventId, SoundEvents};

    /// The gathering event comes first, the deposit's second.
    #[test]
    fn sound_events_hold_the_gathering_then_the_deposit() {
        let s = SoundEvents::new(19598, -5);
        assert_eq!(
            (s.gathering, s.deposit),
            (SoundEventId::new(19598), SoundEventId::new(-5))
        );
        assert_eq!((s.gathering.get(), s.deposit.get()), (19598, -5));
    }
}
