//! An Attack's state: the steps of its update between the start and the end, each walk with
//! its `MoveTo`.

use crate::movement::move_to::MoveToAction;
use crate::{GenieError, GenieResult};
use derive_more::Display;
use genie_diff::Diff;

/// An Attack's state (`+0x18`, set by `set_state`, slot 16 `0x140a820c0`) with the data only
/// that state holds (FD-045-D62): every `set_state` drops the sub-actions first
/// ([`Attack::enter`]), and the three walks make their quiet `MoveTo`, none once the
/// `MoveTo`'s update dropped it, until the next `set_state`. The update's switch
/// (`0x140a82ae0`) runs a step for each. The switch has no case for the other bytes (3, 8 to
/// 10, 12 and above), and no step modelled here sets them.
///
/// [`Attack::enter`]: crate::combat::attack::Attack::enter
#[derive(Clone, Copy, Debug, Default, PartialEq, Diff, Display)]
#[display("{}", self.get())]
pub enum AttackState {
    /// 0: made, not started (the constructor's).
    #[default]
    New,
    /// 1: done; the list removes it.
    Done,
    /// 2: the target died; the swing ends.
    Killed,
    /// 4: on the way to the target.
    Move {
        /// Its `MoveTo` to the target within the attack's range.
        move_to: Option<MoveToAction>,
    },
    /// 5 ("Goto2"): on the way to where the target was seen.
    GotoSeen {
        /// Its `MoveTo` to where it saw the target, within the attack's range.
        move_to: Option<MoveToAction>,
    },
    /// 6: waiting to swing (its reload).
    Ready,
    /// 7: swinging.
    Swing,
    /// 11: a human player's unit that reached where it saw its target without seeing it walks
    /// onto that point.
    GotoPoint {
        /// Its `MoveTo` onto the point.
        move_to: Option<MoveToAction>,
    },
    /// 13 ("Failed"): the target went out of sight or the way failed; each update tells the
    /// unit AI.
    Failed,
}

impl AttackState {
    /// The state of a byte of the save or the game's memory, with the saved sub-action `walk`
    /// (a walk's `MoveTo`, the walks' only).
    ///
    /// # Errors
    ///
    /// A byte that names no state of an Attack, and a `MoveTo` in a state that does not walk.
    pub fn of_byte(value: u8, walk: Option<MoveToAction>) -> GenieResult<Self> {
        let state = match value {
            0 => Self::New,
            1 => Self::Done,
            2 => Self::Killed,
            4 => return Ok(Self::Move { move_to: walk }),
            5 => return Ok(Self::GotoSeen { move_to: walk }),
            6 => Self::Ready,
            7 => Self::Swing,
            11 => return Ok(Self::GotoPoint { move_to: walk }),
            13 => Self::Failed,
            _ => {
                return Err(GenieError::invalid_input("no state of an Attack").with("state", value));
            }
        };
        if walk.is_some() {
            return Err(
                GenieError::invalid_input("a MoveTo of an Attack that does not walk")
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
            Self::Killed => 2,
            Self::Move { .. } => 4,
            Self::GotoSeen { .. } => 5,
            Self::Ready => 6,
            Self::Swing => 7,
            Self::GotoPoint { .. } => 11,
            Self::Failed => 13,
        }
    }

    /// Its walk's `MoveTo` slot, none in a state that does not walk.
    pub const fn walk_mut(&mut self) -> Option<&mut Option<MoveToAction>> {
        match self {
            Self::Move { move_to } | Self::GotoSeen { move_to } | Self::GotoPoint { move_to } => {
                Some(move_to)
            }
            _ => None,
        }
    }

    /// Its walk's `MoveTo`, none in any other state.
    #[must_use]
    pub const fn move_to(&self) -> Option<&MoveToAction> {
        match self {
            Self::Move {
                move_to: Some(move_to_action),
            }
            | Self::GotoSeen {
                move_to: Some(move_to_action),
            }
            | Self::GotoPoint {
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
        let mut attack_state = self;
        let _ = attack_state.take_move_to();
        attack_state
    }

    /// One of the states in which a new order on the same target keeps the Attack as it is (4
    /// to 7 or 11, slot 9's test, `FUN_140a841d0`).
    #[must_use]
    pub const fn keeps_target(self) -> bool {
        matches!(
            self,
            Self::Move { .. }
                | Self::GotoSeen { .. }
                | Self::Ready
                | Self::Swing
                | Self::GotoPoint { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::combat::attack::AttackState;
    use crate::movement::move_to::{MoveTo, MoveToAction};

    /// Each state reads back from its byte; the bytes without a state are errors, and so is a
    /// `MoveTo` of a state that does not walk; the walks give theirs; the states that keep
    /// their target are 4 to 7 and 11.
    #[test]
    fn attack_states_read_back_from_their_bytes() {
        for index in 0..=255_u8 {
            match AttackState::of_byte(index, None) {
                Ok(attack_state) => assert_eq!(attack_state.get(), index),
                Err(_) => assert!(matches!(index, 3 | 8..=10 | 12 | 14..), "{index}"),
            }
        }
        let keeping: Vec<u8> = (0..=13)
            .filter_map(|byte| AttackState::of_byte(byte, None).ok())
            .filter(|attack_state| attack_state.keeps_target())
            .map(AttackState::get)
            .collect();
        assert_eq!(keeping, [4, 5, 6, 7, 11]);
        assert_eq!(AttackState::Swing.to_string(), "7");
        assert_eq!(AttackState::default(), AttackState::New);
        let walk = MoveToAction::sub_in(MoveTo::Moving);
        for byte in [4, 5, 11] {
            let mut attack_state = AttackState::of_byte(byte, Some(walk)).unwrap();
            assert_eq!(
                (attack_state.get(), attack_state.move_to()),
                (byte, Some(&walk))
            );
            assert_eq!(attack_state.bare().move_to(), None);
            assert_eq!(attack_state.take_move_to(), Some(walk));
            assert_eq!(attack_state.move_to(), None);
        }
        let stray = AttackState::of_byte(6, Some(walk)).unwrap_err();
        assert_eq!(stray.reason(), "a MoveTo of an Attack that does not walk");
        assert_eq!(AttackState::Ready.bare(), AttackState::Ready);
    }
}
