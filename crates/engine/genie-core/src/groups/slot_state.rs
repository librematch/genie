//! A formation slot's state: its member steers to it, reaches it, stops at it or stands in it.

use crate::groups::motion::Passing;
use crate::{GenieError, GenieResult};
use derive_more::Display;

/// A formation slot's state (`mState`, `+0x2c`), which a member's steer to its slot
/// (`FUN_141025fb0`) sets and switches on. The values without a state have no case.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Display)]
#[display("{}", self.get())]
pub enum SlotState {
    /// 0: its member steers toward it (the layout's, a detour's, a free member's of a moving
    /// group; a zeroed slot's).
    #[default]
    Steering = 0,
    /// 1: its member's step reaches it (`FUN_141046130`'s 1); once the slot's time is past, it
    /// stops.
    Arriving = 1,
    /// 2: its member's step passed it, or its time ran out while arriving: the member stops and is
    /// tried in it.
    Stopping = 2,
    /// 3: its member stands in it.
    InPlace = 3,
}

impl SlotState {
    /// The state of a value of the save or the game's memory.
    ///
    /// # Errors
    ///
    /// A value that names no state of a formation slot.
    pub fn of_int(value: i32) -> GenieResult<Self> {
        Ok(match value {
            0 => Self::Steering,
            1 => Self::Arriving,
            2 => Self::Stopping,
            3 => Self::InPlace,
            _ => {
                return Err(
                    GenieError::invalid_input("no state of a formation slot").with("state", value)
                );
            }
        })
    }

    /// Its number.
    #[must_use]
    pub const fn get(self) -> i32 {
        self as i32
    }

    /// The state a step's [`Passing`] gives the slot: the game stores `FUN_141046130`'s answer.
    #[must_use]
    pub(in crate::groups) const fn of_passing(passing: Passing) -> Self {
        match passing {
            Passing::Short => Self::Steering,
            Passing::Reaches => Self::Arriving,
            Passing::Passes => Self::Stopping,
        }
    }
}

impl genie_diff::Diff for SlotState {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::groups::SlotState;
    use crate::groups::motion::Passing;

    /// Each state reads back from its value, a step's answer gives its state by number; the
    /// values without a state are errors.
    #[test]
    fn its_states_read_back_from_their_values() {
        for v in 0..=3 {
            assert_eq!(SlotState::of_int(v).map(SlotState::get), Ok(v));
        }
        for v in [i32::MIN, -1, 4, 256] {
            assert!(SlotState::of_int(v).is_err(), "{v}");
        }
        assert_eq!(SlotState::of_passing(Passing::Short), SlotState::Steering);
        assert_eq!(SlotState::of_passing(Passing::Reaches), SlotState::Arriving);
        assert_eq!(SlotState::of_passing(Passing::Passes), SlotState::Stopping);
        assert_eq!(SlotState::InPlace.to_string(), "3");
    }
}
