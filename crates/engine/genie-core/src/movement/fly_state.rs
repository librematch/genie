//! A Fly's state with the data only it holds: a bird resting, choosing its flight and flying
//! with its `MoveTo`.

use crate::GenieError;
use crate::movement::move_to::MoveToAction;
use genie_diff::Diff;

/// A Fly's state (`+0x18`, set by `set_state`, `0x140a84a50`) with the data only that state holds
/// (FD-045-D62): every `set_state` but 4's drops the `MoveTo`, which state 3 makes for the flight
/// just before it flies. Its update switches on it. The bytes without a state have no case, and
/// no step modelled here sets them.
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub enum FlyState {
    /// 0: made, not started (the constructor's).
    New,
    /// 1: done (a player's bird whose task has `+0x15` set goes here first, the start's caller).
    Done,
    /// 2: resting until its timer runs out.
    Resting,
    /// 3: choosing its flight.
    Choosing,
    /// 4: flying.
    Flying {
        /// Its `MoveTo` to the flight's point (not quiet), which state 3 makes; none once the
        /// `MoveTo`'s update dropped it, until the next `set_state`.
        move_to: Option<MoveToAction>,
    },
}

impl FlyState {
    /// The state of a byte of the save or the game's memory, with the saved sub-action `flight`
    /// (the flight's `MoveTo`, state 4's only).
    ///
    /// # Errors
    ///
    /// A byte that names no state of a Fly, and a `MoveTo` in another state.
    pub fn of_byte(value: u8, flight: Option<MoveToAction>) -> Result<Self, GenieError> {
        let state = match value {
            0 => Self::New,
            1 => Self::Done,
            2 => Self::Resting,
            3 => Self::Choosing,
            4 => return Ok(Self::Flying { move_to: flight }),
            _ => {
                return Err(GenieError::invalid_input("no state of a Fly").with("state", value));
            }
        };
        if flight.is_some() {
            return Err(
                GenieError::invalid_input("a MoveTo of a Fly that does not fly")
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
            Self::Resting => 2,
            Self::Choosing => 3,
            Self::Flying { .. } => 4,
        }
    }

    /// Its flight's `MoveTo`, none in any other state.
    #[must_use]
    pub const fn move_to(&self) -> Option<&MoveToAction> {
        match self {
            Self::Flying {
                move_to: Some(m), ..
            } => Some(m),
            _ => None,
        }
    }

    /// Its flight's `MoveTo` to change, none in any other state.
    pub const fn move_to_mut(&mut self) -> Option<&mut MoveToAction> {
        match self {
            Self::Flying {
                move_to: Some(m), ..
            } => Some(m),
            _ => None,
        }
    }
}

impl std::fmt::Display for FlyState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.get())
    }
}

#[cfg(test)]
mod tests {
    use crate::movement::fly::FlyState;
    use crate::objects::Voice;

    /// Each state reads back from its byte; the bytes without a state are errors, and so is a
    /// `MoveTo` of a state that does not fly.
    #[test]
    fn its_states_read_back_from_their_bytes() {
        for b in 0..=255_u8 {
            match FlyState::of_byte(b, None) {
                Ok(s) => assert_eq!(s.get(), b),
                Err(_) => assert!(matches!(b, 5..), "{b}"),
            }
        }
        assert_eq!(FlyState::Flying { move_to: None }.to_string(), "4");
        let mut flight = crate::movement::move_to::MoveToAction::sub_in(
            crate::movement::move_to::MoveTo::Moving,
        );
        flight.voice = Voice::Loud;
        let mut flying = FlyState::of_byte(4, Some(flight)).unwrap();
        assert_eq!(flying.move_to(), Some(&flight));
        flying.move_to_mut().unwrap().range = crate::map::Distance::new(2.0);
        assert_eq!(
            flying.move_to().map(|m| m.range),
            Some(crate::map::Distance::new(2.0))
        );
        assert!(FlyState::of_byte(2, Some(flight)).is_err());
        assert_eq!(FlyState::Resting.move_to(), None);
    }
}
