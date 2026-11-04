//! A Build's state: the steps of its update from the walk to the finished building, each walk
//! with its `MoveTo`.

use crate::GenieError;
use crate::movement::move_to::MoveToAction;
use derive_more::Display;
use genie_diff::Diff;

/// A Build's state (`+0x18`, set by `set_state`, `0x140aae200`) with the data only that state
/// holds (FD-045-D62): every `set_state` drops the sub-actions first
/// ([`super::BuildAction::leave`]), and the four walks make their `MoveTo`, none once the
/// `MoveTo`'s update dropped it, until the next `set_state`. Its update switches on it; the
/// update's default case takes the bytes without a state (0, 5, 9, 10, 12, 14 to 221 and above
/// 224), which no step modelled here sets.
#[derive(Clone, Copy, Debug, PartialEq, Diff, Display)]
#[display("{}", self.get())]
pub enum BuildState {
    /// 1: done; the list removes it.
    Done,
    /// 2: the unit's stop halted the Build (slot 0x38); its next update fails it.
    Halted,
    /// 3: the action waits without a target.
    Idle,
    /// 4: on the way to the foundation (the constructor's).
    Move {
        /// Its `MoveTo` to the foundation.
        move_to: Option<MoveToAction>,
    },
    /// 6: facing the foundation, then placing it.
    Work,
    /// 7: building.
    Place,
    /// 8: the builder carries its load to a drop site.
    Drop {
        /// Its `MoveTo` to the drop site.
        move_to: Option<MoveToAction>,
    },
    /// 11: the walk to the action's point.
    WalkPoint {
        /// Its quiet `MoveTo` to the point.
        move_to: Option<MoveToAction>,
    },
    /// 13 ("Failed"): its next update tells its unit AI and halts it.
    Failed,
    /// 222 (`0xde`): the wait for an occupied footprint.
    Wait,
    /// 223 (`0xdf`): off the footprint, to a free tile beside it.
    Clear {
        /// Its quiet `MoveTo` to the free tile.
        move_to: Option<MoveToAction>,
    },
    /// 224 (`0xe0`): the work ended, the building completed.
    Finished,
}

impl BuildState {
    /// The state of a byte of the save or the game's memory, with the saved sub-action `walk`
    /// (a walk's `MoveTo`, the walks' only).
    ///
    /// # Errors
    ///
    /// A byte that names no state of a Build, and a `MoveTo` in a state that does not walk.
    pub fn of_byte(value: u8, walk: Option<MoveToAction>) -> Result<Self, GenieError> {
        let state = match value {
            1 => Self::Done,
            2 => Self::Halted,
            3 => Self::Idle,
            4 => return Ok(Self::Move { move_to: walk }),
            6 => Self::Work,
            7 => Self::Place,
            8 => return Ok(Self::Drop { move_to: walk }),
            11 => return Ok(Self::WalkPoint { move_to: walk }),
            13 => Self::Failed,
            222 => Self::Wait,
            223 => return Ok(Self::Clear { move_to: walk }),
            224 => Self::Finished,
            _ => {
                return Err(GenieError::invalid_input("no state of a Build").with("state", value));
            }
        };
        if walk.is_some() {
            return Err(
                GenieError::invalid_input("a MoveTo of a Build that does not walk")
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
            Self::Work => 6,
            Self::Place => 7,
            Self::Drop { .. } => 8,
            Self::WalkPoint { .. } => 11,
            Self::Failed => 13,
            Self::Wait => 222,
            Self::Clear { .. } => 223,
            Self::Finished => 224,
        }
    }

    /// Its walk's `MoveTo` slot, none in a state that does not walk.
    pub const fn walk_mut(&mut self) -> Option<&mut Option<MoveToAction>> {
        match self {
            Self::Move { move_to }
            | Self::Drop { move_to }
            | Self::WalkPoint { move_to }
            | Self::Clear { move_to } => Some(move_to),
            _ => None,
        }
    }

    /// Its walk's `MoveTo`, none in any other state.
    #[must_use]
    pub const fn move_to(&self) -> Option<&MoveToAction> {
        match self {
            Self::Move { move_to: Some(m) }
            | Self::Drop { move_to: Some(m) }
            | Self::WalkPoint { move_to: Some(m) }
            | Self::Clear { move_to: Some(m) } => Some(m),
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

#[cfg(test)]
mod tests {
    use crate::construction::build::BuildState;
    #[cfg(test)]
    use crate::movement::move_to::{MoveTo, MoveToAction};

    /// Each state reads back from its byte; the bytes without a state are errors, and so is a
    /// `MoveTo` of a state that does not walk; the walks give theirs.
    #[test]
    fn build_states_read_back_from_their_bytes() {
        for b in 0..=255_u8 {
            match BuildState::of_byte(b, None) {
                Ok(s) => assert_eq!(s.get(), b),
                Err(_) => assert!(matches!(b, 0 | 5 | 9 | 10 | 12 | 14..=221 | 225..), "{b}"),
            }
        }
        assert_eq!(BuildState::Finished.to_string(), "224");
        let walk = MoveToAction::sub_in(MoveTo::Moving);
        for b in [4, 8, 11, 223] {
            let mut s = BuildState::of_byte(b, Some(walk)).unwrap();
            assert_eq!((s.get(), s.move_to()), (b, Some(&walk)));
            assert_eq!(s.bare().move_to(), None);
            assert_eq!(s.take_move_to(), Some(walk));
            assert_eq!(s.move_to(), None);
        }
        assert!(BuildState::of_byte(6, Some(walk)).is_err());
        assert_eq!(BuildState::Work.bare(), BuildState::Work);
    }
}
