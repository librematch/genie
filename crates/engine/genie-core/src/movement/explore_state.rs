//! An Explore's state with the data only it holds: waiting for a point, or walking to it with
//! its `MoveTo`.

use crate::movement::move_to::MoveToAction;
use crate::{GenieError, GenieResult};
use derive_more::Display;
use genie_diff::Diff;

/// An Explore's state (`+0x18`, set by `set_state`, slot 0x80 `0x140a87e30`) with the data only
/// that state holds (FD-045-D62): every `set_state` drops the sub-actions first, and only state
/// 11 makes one, the walk's `MoveTo`. Its update (`0x140a880c0`) and `set_state` switch on it.
/// The bytes without a state have no case, and no step modelled here sets them.
#[derive(Clone, Copy, Debug, PartialEq, Diff, Display)]
#[display("{}", self.get())]
pub enum ExploreState {
    /// 0: made, not started (the constructor's).
    New,
    /// 1: done; the list removes it and the unit stands.
    Done,
    /// 2: stopped, its point gone.
    Stopped,
    /// 3: waiting for a point; its unit AI is asked for one.
    Wait,
    /// 11: walking to the point.
    Walk {
        /// Its quiet `MoveTo` to the point, which `set_state` 11 makes; none once the
        /// `MoveTo`'s update dropped it, until the next `set_state`.
        move_to: Option<MoveToAction>,
    },
    /// 13 ("Failed"): the unit stands; the update tells the unit AI.
    Failed,
    /// 14: ended as 1 and 13 do: the unit stands (a state the interruptible test takes with 0, 1, 3
    /// and 13).
    Ended,
}

impl ExploreState {
    /// The state of a byte of the save or the game's memory, with the saved sub-action `walk`
    /// (the walk's `MoveTo`, state 11's only).
    ///
    /// # Errors
    ///
    /// A byte that names no state of an Explore, and a `MoveTo` in another state.
    pub fn of_byte(value: u8, walk: Option<MoveToAction>) -> GenieResult<Self> {
        let state = match value {
            0 => Self::New,
            1 => Self::Done,
            2 => Self::Stopped,
            3 => Self::Wait,
            11 => return Ok(Self::Walk { move_to: walk }),
            13 => Self::Failed,
            14 => Self::Ended,
            _ => {
                return Err(
                    GenieError::invalid_input("no state of an Explore").with("state", value)
                );
            }
        };
        if walk.is_some() {
            return Err(
                GenieError::invalid_input("a MoveTo of an Explore that does not walk")
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
            Self::Stopped => 2,
            Self::Wait => 3,
            Self::Walk { .. } => 11,
            Self::Failed => 13,
            Self::Ended => 14,
        }
    }

    /// Its walk's `MoveTo`, none in any other state.
    #[must_use]
    pub const fn move_to(&self) -> Option<&MoveToAction> {
        match self {
            Self::Walk {
                move_to: Some(m), ..
            } => Some(m),
            _ => None,
        }
    }

    /// Its walk's `MoveTo` to change, none in any other state.
    pub const fn move_to_mut(&mut self) -> Option<&mut MoveToAction> {
        match self {
            Self::Walk {
                move_to: Some(m), ..
            } => Some(m),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    use crate::map::{Distance, Position};
    use crate::movement::explore::ExploreState;
    #[cfg(test)]
    use crate::movement::move_to::{MoveTo, MoveToAction, MoveWait, TargetMoved};
    use crate::objects::Voice;

    /// Each state reads back from its byte; the bytes without a state are errors, and so is a
    /// `MoveTo` of a state that does not walk.
    #[test]
    fn its_states_read_back_from_their_bytes() {
        for b in 0..=255_u8 {
            match ExploreState::of_byte(b, None) {
                Ok(s) => assert_eq!(s.get(), b),
                Err(_) => assert!(matches!(b, 4..=10 | 12 | 15..), "{b}"),
            }
        }
        assert_eq!(ExploreState::Ended.to_string(), "14");
        let walk = MoveToAction {
            state: MoveTo::Start,
            point: Position::new(19.0, 84.0, 0.0),
            target: None,
            range: Distance::ZERO,
            sprite: None,
            voice: Voice::Quiet,
            on_target_moved: TargetMoved::Follows,
            wait: MoveWait::NEW,
        };
        assert_eq!(
            ExploreState::of_byte(11, Some(walk)),
            Ok(ExploreState::Walk {
                move_to: Some(walk)
            })
        );
        assert_eq!(
            ExploreState::of_byte(11, Some(walk)).unwrap().move_to(),
            Some(&walk)
        );
        assert!(ExploreState::of_byte(3, Some(walk)).is_err());
        assert_eq!(ExploreState::Wait.move_to(), None);
        let mut walking = ExploreState::of_byte(11, Some(walk)).unwrap();
        walking.move_to_mut().unwrap().range = Distance::new(2.0);
        assert_eq!(walking.move_to().map(|m| m.range), Some(Distance::new(2.0)));
        assert_eq!(ExploreState::Wait.move_to_mut(), None);
    }
}
