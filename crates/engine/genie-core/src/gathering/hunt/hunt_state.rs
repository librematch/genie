//! A Hunt's state: the steps of its update from the attack to the drop.

use crate::GenieError;

/// A Hunt's state (`+0x18`, set by `set_state`): its update (`0x140ac7a00`) switches on it.
/// The bytes without a state (5, 9, 14 and above 15) have no case.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HuntState {
    /// 0: made, not started (the constructor's).
    New = 0,
    /// 1: done; the list removes it.
    Done = 1,
    /// 2: done with its target, it waits (a tree's target class would search again).
    Idle = 2,
    /// 3: searching: it told its unit AI to look for the next target (notify 0x202), again
    /// every 10 seconds.
    Search = 3,
    /// 4: walking to a dead target.
    Walk = 4,
    /// 6: facing it.
    Face = 6,
    /// 7: gathering from it.
    Gather = 7,
    /// 8: taking the load to its drop site.
    Drop = 8,
    /// 10 (`0xa`): leading its own livestock to the drop site, it waits for the animal to stop
    /// there.
    Lead = 10,
    /// 11 (`0xb`): walking to the point after the target went on the way to it; its end
    /// searches.
    WalkSearch = 11,
    /// 12 (`0xc`): attacking it (an Attack sub-action).
    Attack = 12,
    /// 13 ("Failed"), the state every action shares.
    Failed = 13,
    /// 15 (`0xf`): walking to a point a move gave it (its slot 0x40).
    WalkPoint = 15,
}

impl HuntState {
    /// The state of a byte of the save or the game's memory.
    ///
    /// # Errors
    ///
    /// A byte that names no state of a Hunt.
    pub fn of_byte(value: u8) -> Result<Self, GenieError> {
        Ok(match value {
            0 => Self::New,
            1 => Self::Done,
            2 => Self::Idle,
            3 => Self::Search,
            4 => Self::Walk,
            6 => Self::Face,
            7 => Self::Gather,
            8 => Self::Drop,
            10 => Self::Lead,
            11 => Self::WalkSearch,
            12 => Self::Attack,
            13 => Self::Failed,
            15 => Self::WalkPoint,
            _ => {
                return Err(GenieError::invalid_input("no state of a Hunt").with("state", value));
            }
        })
    }

    /// Its number.
    #[must_use]
    pub const fn get(self) -> u8 {
        self as u8
    }
}

impl std::fmt::Display for HuntState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.get())
    }
}

impl genie_diff::Diff for HuntState {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::gathering::hunt::HuntState;

    /// Each state reads back from its byte; the bytes without a state are errors.
    #[test]
    fn hunt_states_read_back_from_their_bytes() {
        for b in 0..=255_u8 {
            match HuntState::of_byte(b) {
                Ok(s) => assert_eq!(s.get(), b),
                Err(_) => assert!(matches!(b, 5 | 9 | 14 | 16..), "{b}"),
            }
        }
        assert_eq!(HuntState::Attack.to_string(), "12");
    }
}
