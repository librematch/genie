//! A Transport's state: the steps of its update from the sail to the unload; each sail with its
//! `MoveTo`.

use crate::movement::move_to::MoveToAction;
use crate::{GenieError, GenieResult};
use derive_more::Display;
use genie_diff::Diff;

/// A Transport's state (`+0x18`, set by `set_state`, slot 16 `FUN_140aa42a0`) with the data
/// only that state holds (FD-045-D62): every `set_state` drops the sub-actions first, and the
/// two sails make their quiet `MoveTo`, none once the `MoveTo`'s update dropped it, until the
/// next `set_state`. Its update switches on it. The bytes without a state have no case, and no
/// step modelled here sets them.
#[derive(Clone, Copy, Debug, PartialEq, Diff, Display)]
#[display("{}", self.get())]
pub enum TransportState {
    /// 0: made, not started (the constructor's).
    New,
    /// 1: done; the list removes it.
    Done,
    /// 2: idle: the update does nothing.
    Idle,
    /// 4: sailing to unload.
    SailToUnload {
        /// Its quiet `MoveTo` to the (shore) point.
        move_to: Option<MoveToAction>,
    },
    /// 6: unloading.
    Unload,
    /// 11: sailing.
    Sail {
        /// Its quiet `MoveTo` to the (shore) point.
        move_to: Option<MoveToAction>,
    },
    /// 13 ("Failed"): the update tells the unit AI, idles and returns 3.
    Failed,
}

impl TransportState {
    /// The state of a byte of the save or the game's memory, with the saved sub-action `sail`
    /// (a sail's `MoveTo`, the sails' only).
    ///
    /// # Errors
    ///
    /// A byte that names no state of a Transport, and a `MoveTo` in a state that does not sail.
    pub fn of_byte(value: u8, sail: Option<MoveToAction>) -> GenieResult<Self> {
        let state = match value {
            0 => Self::New,
            1 => Self::Done,
            2 => Self::Idle,
            4 => return Ok(Self::SailToUnload { move_to: sail }),
            6 => Self::Unload,
            11 => return Ok(Self::Sail { move_to: sail }),
            13 => Self::Failed,
            _ => {
                return Err(
                    GenieError::invalid_input("no state of a Transport").with("state", value)
                );
            }
        };
        if sail.is_some() {
            return Err(
                GenieError::invalid_input("a MoveTo of a Transport that does not sail")
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
            Self::SailToUnload { .. } => 4,
            Self::Unload => 6,
            Self::Sail { .. } => 11,
            Self::Failed => 13,
        }
    }

    /// Its sail's `MoveTo` slot, none in a state that does not sail.
    pub const fn walk_mut(&mut self) -> Option<&mut Option<MoveToAction>> {
        match self {
            Self::SailToUnload { move_to } | Self::Sail { move_to } => Some(move_to),
            _ => None,
        }
    }

    /// Its sail's `MoveTo`, none in any other state.
    #[must_use]
    pub const fn move_to(&self) -> Option<&MoveToAction> {
        match self {
            Self::SailToUnload { move_to: Some(m) } | Self::Sail { move_to: Some(m) } => Some(m),
            _ => None,
        }
    }

    /// Its sail's `MoveTo` taken out (the state stays), none in any other state.
    pub const fn take_move_to(&mut self) -> Option<MoveToAction> {
        match self.walk_mut() {
            Some(m) => m.take(),
            None => None,
        }
    }

    /// The same state, its sail without a `MoveTo`: the state a `set_state` names.
    #[must_use]
    pub const fn bare(self) -> Self {
        let mut s = self;
        let _ = s.take_move_to();
        s
    }
}

#[cfg(test)]
mod tests {
    use crate::garrison::transport::TransportState;
    #[cfg(test)]
    use crate::movement::move_to::{MoveTo, MoveToAction};

    /// Each state reads back from its byte; the bytes without a state are errors, and so is a
    /// `MoveTo` of a state that does not sail; the sails give theirs.
    #[test]
    fn its_states_read_back_from_their_bytes() {
        for b in 0..=255_u8 {
            match TransportState::of_byte(b, None) {
                Ok(s) => assert_eq!(s.get(), b),
                Err(_) => assert!(matches!(b, 3 | 5 | 7..=10 | 12 | 14..), "{b}"),
            }
        }
        assert_eq!(TransportState::Failed.to_string(), "13");
        let sail = MoveToAction::sub_in(MoveTo::Moving);
        for b in [4, 11] {
            let mut s = TransportState::of_byte(b, Some(sail)).unwrap();
            assert_eq!((s.get(), s.move_to()), (b, Some(&sail)));
            assert_eq!(s.bare().move_to(), None);
            assert_eq!(s.take_move_to(), Some(sail));
            assert_eq!(s.move_to(), None);
        }
        assert!(TransportState::of_byte(6, Some(sail)).is_err());
        assert_eq!(TransportState::Unload.bare(), TransportState::Unload);
    }
}
