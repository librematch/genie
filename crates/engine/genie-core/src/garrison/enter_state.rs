//! An Enter's state: walking to a building or transport, waiting, inside; each walk with its
//! `MoveTo`.

use crate::GenieError;
use crate::movement::move_to::MoveToAction;
use genie_diff::Diff;
use std::fmt::{Display, Error, Formatter};

/// An Enter's state (`+0x18`, set by `set_state`, `FUN_140a87320`) with the data only that
/// state holds (FD-045-D62): every `set_state` drops the sub-actions first, and the two walks
/// make their quiet `MoveTo`, none once the `MoveTo`'s update dropped it, until the next
/// `set_state`. Its update switches on it. The bytes without a state have no case, and no step
/// modelled here sets them.
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub enum EnterState {
    /// 1: done: inside; the list removes it.
    Done,
    /// 2: waiting a second before it walks again.
    Wait,
    /// 4: walking to the target (the constructor's).
    Walk {
        /// Its quiet `MoveTo` to the building, or to the meeting point toward the transport.
        move_to: Option<MoveToAction>,
    },
    /// 11: its target died or is not allied: a quiet `MoveTo` to the meeting point.
    WalkPoint {
        /// Its quiet `MoveTo` to the point.
        move_to: Option<MoveToAction>,
    },
    /// 13 ("Failed"): the list removes it.
    Failed,
}

impl EnterState {
    /// The state of a byte of the save or the game's memory, with the saved sub-action `walk`
    /// (a walk's `MoveTo`, the walks' only).
    ///
    /// # Errors
    ///
    /// A byte that names no state of an Enter, and a `MoveTo` in a state that does not walk.
    pub fn of_byte(value: u8, walk: Option<MoveToAction>) -> Result<Self, GenieError> {
        let state = match value {
            1 => Self::Done,
            2 => Self::Wait,
            4 => return Ok(Self::Walk { move_to: walk }),
            11 => return Ok(Self::WalkPoint { move_to: walk }),
            13 => Self::Failed,
            _ => {
                return Err(GenieError::invalid_input("no state of an Enter").with("state", value));
            }
        };
        if walk.is_some() {
            return Err(
                GenieError::invalid_input("a MoveTo of an Enter that does not walk")
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
            Self::Wait => 2,
            Self::Walk { .. } => 4,
            Self::WalkPoint { .. } => 11,
            Self::Failed => 13,
        }
    }

    /// Its walk's `MoveTo` slot, none in a state that does not walk.
    pub const fn walk_mut(&mut self) -> Option<&mut Option<MoveToAction>> {
        match self {
            Self::Walk { move_to } | Self::WalkPoint { move_to } => Some(move_to),
            _ => None,
        }
    }

    /// Its walk's `MoveTo`, none in any other state.
    #[must_use]
    pub const fn move_to(&self) -> Option<&MoveToAction> {
        match self {
            Self::Walk { move_to: Some(m) } | Self::WalkPoint { move_to: Some(m) } => Some(m),
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

impl Display for EnterState {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{}", self.get())
    }
}

#[cfg(test)]
mod tests {
    use crate::garrison::EnterState;
    #[cfg(test)]
    use crate::movement::move_to::{MoveTo, MoveToAction};

    /// Each state reads back from its byte; the bytes without a state are errors, and so is a
    /// `MoveTo` of a state that does not walk; the walks give theirs.
    #[test]
    fn its_states_read_back_from_their_bytes() {
        for b in 0..=255_u8 {
            match EnterState::of_byte(b, None) {
                Ok(s) => assert_eq!(s.get(), b),
                Err(_) => assert!(matches!(b, 0 | 3 | 5..=10 | 12 | 14..), "{b}"),
            }
        }
        assert_eq!(EnterState::Failed.to_string(), "13");
        let walk = MoveToAction::sub_in(MoveTo::Moving);
        for b in [4, 11] {
            let mut s = EnterState::of_byte(b, Some(walk)).unwrap();
            assert_eq!((s.get(), s.move_to()), (b, Some(&walk)));
            assert_eq!(s.bare().move_to(), None);
            assert_eq!(s.take_move_to(), Some(walk));
            assert_eq!(s.move_to(), None);
        }
        assert!(EnterState::of_byte(2, Some(walk)).is_err());
        assert_eq!(EnterState::Wait.bare(), EnterState::Wait);
    }
}
