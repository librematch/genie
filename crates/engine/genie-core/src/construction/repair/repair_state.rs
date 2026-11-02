//! A Repair's state: the steps of its update from the walk to the work, each walk with its
//! `MoveTo`.

use crate::GenieError;
use crate::movement::move_to::MoveToAction;
use genie_diff::Diff;

/// A Repair's state (`+0x18`, set by `set_state`, `0x140ad9c60`) with the data only that state
/// holds (FD-045-D62): every `set_state` drops the sub-actions first, and the two walks make
/// their quiet `MoveTo`, none once the `MoveTo`'s update dropped it, until the next
/// `set_state`. Its update (`0x140adaa20`) and `set_state` switch on it. The update has no case
/// for the other bytes (0, 5, 8, 9, 12 and above 13), and no step modelled here sets them.
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub enum RepairState {
    /// 1: done; the target is let go and the list removes it.
    Done,
    /// 2: halted (a failed repair's end), the standing sprite.
    Halted,
    /// 3: idle; the unit AI is told to search (notify 0x202).
    Idle,
    /// 4: on the way to the target (the constructor's).
    Move {
        /// Its quiet `MoveTo` to the target.
        move_to: Option<MoveToAction>,
    },
    /// 6: turning toward the target.
    Face,
    /// 7: repairing, counted among the target's repairers.
    Work,
    /// 10: waiting a second for the way to the target.
    Wait,
    /// 11: on the way to its point.
    WalkPoint {
        /// Its quiet `MoveTo` to the point.
        move_to: Option<MoveToAction>,
    },
    /// 13 ("Failed"): the target cannot be reached; the update tells the unit AI and halts.
    Failed,
}

impl RepairState {
    /// The state of a byte of the save or the game's memory, with the saved sub-action `walk`
    /// (a walk's `MoveTo`, the walks' only).
    ///
    /// # Errors
    ///
    /// A byte that names no state of a Repair, and a `MoveTo` in a state that does not walk.
    pub fn of_byte(value: u8, walk: Option<MoveToAction>) -> Result<Self, GenieError> {
        let state = match value {
            1 => Self::Done,
            2 => Self::Halted,
            3 => Self::Idle,
            4 => return Ok(Self::Move { move_to: walk }),
            6 => Self::Face,
            7 => Self::Work,
            10 => Self::Wait,
            11 => return Ok(Self::WalkPoint { move_to: walk }),
            13 => Self::Failed,
            _ => {
                return Err(GenieError::invalid_input("no state of a Repair").with("state", value));
            }
        };
        if walk.is_some() {
            return Err(
                GenieError::invalid_input("a MoveTo of a Repair that does not walk")
                    .with("state", value),
            );
        }
        Ok(state)
    }

    /// Its number.
    #[must_use]
    pub const fn get(self) -> u8 {
        match self {
            Self::Done => 1,
            Self::Halted => 2,
            Self::Idle => 3,
            Self::Move { .. } => 4,
            Self::Face => 6,
            Self::Work => 7,
            Self::Wait => 10,
            Self::WalkPoint { .. } => 11,
            Self::Failed => 13,
        }
    }

    /// Its walk's `MoveTo` slot, none in a state that does not walk.
    pub const fn walk_mut(&mut self) -> Option<&mut Option<MoveToAction>> {
        match self {
            Self::Move { move_to } | Self::WalkPoint { move_to } => Some(move_to),
            _ => None,
        }
    }

    /// Its walk's `MoveTo`, none in any other state.
    #[must_use]
    pub const fn move_to(&self) -> Option<&MoveToAction> {
        match self {
            Self::Move { move_to: Some(m) } | Self::WalkPoint { move_to: Some(m) } => Some(m),
            _ => None,
        }
    }

    /// Its walk's `MoveTo` taken out (the state stays), none in any other state.
    pub const fn take_move_to(&mut self) -> Option<MoveToAction> {
        match self.walk_mut() {
            Some(m) => m.take(),
            None => None,
        }
    }

    /// The same state, its walk without a `MoveTo`: the state a `set_state` names.
    #[must_use]
    pub const fn bare(self) -> Self {
        let mut s = self;
        let _ = s.take_move_to();
        s
    }
}

impl std::fmt::Display for RepairState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.get())
    }
}

#[cfg(test)]
mod tests {
    use crate::construction::repair::RepairState;

    /// Each state reads back from its byte; the bytes without a state are errors, and so is a
    /// `MoveTo` of a state that does not walk; the walks give theirs.
    #[test]
    fn a_repair_s_states_read_back_from_their_bytes() {
        for b in 0..=255_u8 {
            match RepairState::of_byte(b, None) {
                Ok(s) => assert_eq!(s.get(), b),
                Err(_) => assert!(matches!(b, 0 | 5 | 8 | 9 | 12 | 14..), "{b}"),
            }
        }
        assert_eq!(RepairState::Wait.to_string(), "10");
        let walk = crate::movement::move_to::MoveToAction::sub_in(
            crate::movement::move_to::MoveTo::Moving,
        );
        for b in [4, 11] {
            let mut s = RepairState::of_byte(b, Some(walk)).unwrap();
            assert_eq!((s.get(), s.move_to()), (b, Some(&walk)));
            assert_eq!(s.bare().move_to(), None);
            assert_eq!(s.take_move_to(), Some(walk));
            assert_eq!(s.move_to(), None);
        }
        assert!(RepairState::of_byte(7, Some(walk)).is_err());
        assert_eq!(RepairState::Work.bare(), RepairState::Work);
    }
}
