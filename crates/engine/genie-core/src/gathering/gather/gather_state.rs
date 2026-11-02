//! A Gather's state: the steps of its update from the walk to the drop.

use crate::GenieError;

/// A Gather's state (`+0x18`, set by `set_state`, `0x140a8cd40`): its update switches on it.
/// The bytes without a state (5, 9, 10, 12, 14, 16 to 0xf1 and above 0xf3) have no case.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GatherState {
    /// 0: made, not started (the constructor's).
    New = 0,
    /// 1: done; the list removes it.
    Done = 1,
    /// 2: idle, without a target, a drop site or a point; its update does nothing.
    Idle = 2,
    /// 3: without a target or a load, it asks the unit AI for another (notify 0x202) and asks
    /// again every 10 seconds.
    Search = 3,
    /// 4: walking to the target.
    Walk = 4,
    /// 6: facing it.
    Face = 6,
    /// 7: gathering from it.
    Work = 7,
    /// 8: taking the load to its drop site.
    Drop = 8,
    /// 11: walking to its point, to search from there (a farm another farmer works).
    WalkBack = 11,
    /// 13 ("Failed"), the state every action shares.
    Failed = 13,
    /// 15: walking to the point the unit was moved to (slot 0x40).
    Moved = 15,
    /// 0xf2: a farmer walking back to where it gathered (`mGatherX`, `mGatherY`) to reseed its
    /// farm there.
    ReseedWalk = 0xf2,
    /// 0xf3: a farmer without its farm (or at one another farmer works) waits a second, its
    /// unit AI asked for another.
    FarmWait = 0xf3,
}

impl GatherState {
    /// The state of a byte of the save or the game's memory.
    ///
    /// # Errors
    ///
    /// A byte that names no state of a Gather.
    pub fn of_byte(value: u8) -> Result<Self, GenieError> {
        Ok(match value {
            0 => Self::New,
            1 => Self::Done,
            2 => Self::Idle,
            3 => Self::Search,
            4 => Self::Walk,
            6 => Self::Face,
            7 => Self::Work,
            8 => Self::Drop,
            11 => Self::WalkBack,
            13 => Self::Failed,
            15 => Self::Moved,
            0xf2 => Self::ReseedWalk,
            0xf3 => Self::FarmWait,
            _ => {
                return Err(GenieError::invalid_input("no state of a Gather").with("state", value));
            }
        })
    }

    /// Its number.
    #[must_use]
    pub const fn get(self) -> u8 {
        self as u8
    }
}

impl std::fmt::Display for GatherState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.get())
    }
}

impl genie_diff::Diff for GatherState {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::gathering::gather::GatherState;

    /// Each state reads back from its byte; the bytes without a state are errors.
    #[test]
    fn gather_states_read_back_from_their_bytes() {
        for b in 0..=255_u8 {
            match GatherState::of_byte(b) {
                Ok(s) => assert_eq!(s.get(), b),
                Err(_) => assert!(
                    matches!(b, 5 | 9 | 10 | 12 | 14 | 16..=0xf1 | 0xf4..),
                    "{b}"
                ),
            }
        }
        assert_eq!(GatherState::FarmWait.to_string(), "243");
    }
}
