//! A task's two sound events ([`SoundEvents`]).

use genie_diff::Diff;

/// `+0x60` and `+0x78` of a task: the event ids (`mEventId`) of its gathering and its deposit
/// sound, as the task's load (`FUN_141062470`) leaves them. A terrain look task (159) names its
/// dying and its undead sprite with them ([`crate::combat::abilities`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Diff)]
pub struct SoundEvents {
    /// The gathering sound's event (`+0x60`).
    pub gathering: i32,
    /// The deposit sound's event (`+0x78`).
    pub deposit: i32,
}

impl SoundEvents {
    /// `gathering` and `deposit`.
    #[must_use]
    pub const fn new(gathering: i32, deposit: i32) -> Self {
        Self { gathering, deposit }
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::SoundEvents;

    /// The gathering event comes first, the deposit's second.
    #[test]
    fn sound_events_hold_the_gathering_then_the_deposit() {
        let s = SoundEvents::new(19598, -5);
        assert_eq!((s.gathering, s.deposit), (19598, -5));
    }
}
