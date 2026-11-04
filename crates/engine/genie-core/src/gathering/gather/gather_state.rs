//! A Gather's state: the steps of its update from the walk to the drop, each walk with its
//! `MoveTo`.

use crate::movement::move_to::MoveToAction;
use crate::{GenieError, GenieResult};
use derive_more::Display;
use genie_diff::Diff;

/// A Gather's state (`+0x18`, set by `set_state`, `0x140a8cd40`) with the data only that state
/// holds (FD-045-D62): every `set_state` drops the sub-actions first (`FUN_140a98620`), and the
/// five walks make their `MoveTo`, none once the `MoveTo`'s update dropped it, until the next
/// `set_state`. Its update switches on it. The bytes without a state (5, 9, 10, 12, 14, 16 to
/// 0xf1 and above 0xf3) have no case.
#[derive(Clone, Copy, Debug, PartialEq, Diff, Display)]
#[display("{}", self.get())]
pub enum GatherState {
    /// 0: made, not started (the constructor's).
    New,
    /// 1: done; the list removes it.
    Done,
    /// 2: idle, without a target, a drop site or a point; its update does nothing.
    Idle,
    /// 3: without a target or a load, it asks the unit AI for another (notify 0x202) and asks
    /// again every 10 seconds.
    Search,
    /// 4: walking to the target.
    Walk {
        /// Its `MoveTo` to the target, or to a farm's stop.
        move_to: Option<MoveToAction>,
    },
    /// 6: facing it.
    Face,
    /// 7: gathering from it.
    Work,
    /// 8: taking the load to its drop site.
    Drop {
        /// Its `MoveTo` to the drop site.
        move_to: Option<MoveToAction>,
    },
    /// 11: walking to its point, to search from there (a farm another farmer works).
    WalkBack {
        /// Its quiet `MoveTo` to the point.
        move_to: Option<MoveToAction>,
    },
    /// 13 ("Failed"), the state every action shares.
    Failed,
    /// 15: walking to the point the unit was moved to (slot 0x40).
    Moved {
        /// Its `MoveTo` to the point.
        move_to: Option<MoveToAction>,
    },
    /// 0xf2: a farmer walking back to where it gathered (`mGatherX`, `mGatherY`) to reseed its
    /// farm there.
    ReseedWalk {
        /// Its quiet `MoveTo` to where it gathered.
        move_to: Option<MoveToAction>,
    },
    /// 0xf3: a farmer without its farm (or at one another farmer works) waits a second, its
    /// unit AI asked for another.
    FarmWait,
}

impl GatherState {
    /// The state of a byte of the save or the game's memory, with the saved sub-action `walk`
    /// (a walk's `MoveTo`, the walks' only).
    ///
    /// # Errors
    ///
    /// A byte that names no state of a Gather, and a `MoveTo` in a state that does not walk.
    pub fn of_byte(value: u8, walk: Option<MoveToAction>) -> GenieResult<Self> {
        let state = match value {
            0 => Self::New,
            1 => Self::Done,
            2 => Self::Idle,
            3 => Self::Search,
            4 => return Ok(Self::Walk { move_to: walk }),
            6 => Self::Face,
            7 => Self::Work,
            8 => return Ok(Self::Drop { move_to: walk }),
            11 => return Ok(Self::WalkBack { move_to: walk }),
            13 => Self::Failed,
            15 => return Ok(Self::Moved { move_to: walk }),
            0xf2 => return Ok(Self::ReseedWalk { move_to: walk }),
            0xf3 => Self::FarmWait,
            _ => {
                return Err(GenieError::invalid_input("no state of a Gather").with("state", value));
            }
        };
        if walk.is_some() {
            return Err(
                GenieError::invalid_input("a MoveTo of a Gather that does not walk")
                    .with("state", value),
            );
        }
        Ok(state)
    }

    /// Its number.
    #[must_use]
    pub const fn get(self) -> u8 {
        match self {
            Self::New => 0,
            Self::Done => 1,
            Self::Idle => 2,
            Self::Search => 3,
            Self::Walk { .. } => 4,
            Self::Face => 6,
            Self::Work => 7,
            Self::Drop { .. } => 8,
            Self::WalkBack { .. } => 11,
            Self::Failed => 13,
            Self::Moved { .. } => 15,
            Self::ReseedWalk { .. } => 0xf2,
            Self::FarmWait => 0xf3,
        }
    }

    /// Its walk's `MoveTo` slot, none in a state that does not walk.
    pub const fn walk_mut(&mut self) -> Option<&mut Option<MoveToAction>> {
        match self {
            Self::Walk { move_to }
            | Self::Drop { move_to }
            | Self::WalkBack { move_to }
            | Self::Moved { move_to }
            | Self::ReseedWalk { move_to } => Some(move_to),
            _ => None,
        }
    }

    /// Its walk's `MoveTo`, none in any other state.
    #[must_use]
    pub const fn move_to(&self) -> Option<&MoveToAction> {
        match self {
            Self::Walk {
                move_to: Some(move_to_action),
            }
            | Self::Drop {
                move_to: Some(move_to_action),
            }
            | Self::WalkBack {
                move_to: Some(move_to_action),
            }
            | Self::Moved {
                move_to: Some(move_to_action),
            }
            | Self::ReseedWalk {
                move_to: Some(move_to_action),
            } => Some(move_to_action),
            _ => None,
        }
    }

    /// Its walk's `MoveTo` taken out (the state stays), none in any other state.
    pub const fn take_move_to(&mut self) -> Option<MoveToAction> {
        match self.walk_mut() {
            Some(move_to_action) => move_to_action.take(),
            None => None,
        }
    }

    /// The same state, its walk without a `MoveTo`: the state a `set_state` names.
    #[must_use]
    pub const fn bare(self) -> Self {
        let mut gather_state = self;
        let _ = gather_state.take_move_to();
        gather_state
    }
}

#[cfg(test)]
mod tests {
    use crate::gathering::gather::GatherState;
    #[cfg(test)]
    use crate::movement::move_to::{MoveTo, MoveToAction};

    /// Each state reads back from its byte; the bytes without a state are errors, and so is a
    /// `MoveTo` of a state that does not walk; the walks give theirs.
    #[test]
    fn gather_states_read_back_from_their_bytes() {
        for index in 0..=255_u8 {
            match GatherState::of_byte(index, None) {
                Ok(gather_state) => assert_eq!(gather_state.get(), index),
                Err(_) => assert!(
                    matches!(index, 5 | 9 | 10 | 12 | 14 | 16..=0xf1 | 0xf4..),
                    "{index}"
                ),
            }
        }
        assert_eq!(GatherState::FarmWait.to_string(), "243");
        let walk = MoveToAction::sub_in(MoveTo::Moving);
        for byte in [4, 8, 11, 15, 0xf2] {
            let mut gather_state = GatherState::of_byte(byte, Some(walk)).unwrap();
            assert_eq!(
                (gather_state.get(), gather_state.move_to()),
                (byte, Some(&walk))
            );
            assert_eq!(gather_state.bare().move_to(), None);
            assert_eq!(gather_state.take_move_to(), Some(walk));
            assert_eq!(gather_state.move_to(), None);
        }
        assert!(GatherState::of_byte(7, Some(walk)).is_err());
        assert_eq!(GatherState::Work.bare(), GatherState::Work);
    }
}
