//! An Attack's state: the steps of its update between the start and the end.

use crate::GenieError;

/// An Attack's state (`+0x18`, set by `set_state`, slot 16 `0x140a820c0`): the update's switch
/// (`0x140a82ae0`) runs a step for each. The switch has no case for the other bytes (3, 8 to 10,
/// 12 and above), and no step modelled here sets them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AttackState {
    /// 0: made, not started (the constructor's).
    #[default]
    New = 0,
    /// 1: done; the list removes it.
    Done = 1,
    /// 2: the target died; the swing ends.
    Killed = 2,
    /// 4: on the way to the target.
    Move = 4,
    /// 5 ("Goto2"): on the way to where the target was seen.
    GotoSeen = 5,
    /// 6: waiting to swing (its reload).
    Ready = 6,
    /// 7: swinging.
    Swing = 7,
    /// 11: a human player's unit that reached where it saw its target without seeing it walks
    /// onto that point.
    GotoPoint = 11,
    /// 13 ("Failed"): the target went out of sight or the way failed; each update tells the
    /// unit AI.
    Failed = 13,
}

impl AttackState {
    /// The state of a byte of the save or the game's memory.
    ///
    /// # Errors
    ///
    /// A byte that names no state of an Attack.
    pub fn of_byte(value: u8) -> Result<Self, GenieError> {
        Ok(match value {
            0 => Self::New,
            1 => Self::Done,
            2 => Self::Killed,
            4 => Self::Move,
            5 => Self::GotoSeen,
            6 => Self::Ready,
            7 => Self::Swing,
            11 => Self::GotoPoint,
            13 => Self::Failed,
            _ => {
                return Err(GenieError::invalid_input("no state of an Attack").with("state", value));
            }
        })
    }

    /// Its number.
    #[must_use]
    pub const fn get(self) -> u8 {
        self as u8
    }

    /// One of the states in which a new order on the same target keeps the Attack as it is (4
    /// to 7 or 11, slot 9's test, `FUN_140a841d0`).
    #[must_use]
    pub const fn keeps_target(self) -> bool {
        matches!(
            self,
            Self::Move | Self::GotoSeen | Self::Ready | Self::Swing | Self::GotoPoint
        )
    }
}

impl std::fmt::Display for AttackState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.get())
    }
}

impl genie_diff::Diff for AttackState {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::combat::attack::AttackState;

    /// Each state reads back from its byte; the bytes without a state are errors; the states
    /// that keep their target are 4 to 7 and 11.
    #[test]
    fn an_attack_states_read_back_from_their_bytes() {
        for b in 0..=255_u8 {
            match AttackState::of_byte(b) {
                Ok(s) => assert_eq!(s.get(), b),
                Err(_) => assert!(matches!(b, 3 | 8..=10 | 12 | 14..), "{b}"),
            }
        }
        let keeping: Vec<u8> = (0..=13)
            .filter_map(|b| AttackState::of_byte(b).ok())
            .filter(|s| s.keeps_target())
            .map(AttackState::get)
            .collect();
        assert_eq!(keeping, [4, 5, 6, 7, 11]);
        assert_eq!(AttackState::Swing.to_string(), "7");
        assert_eq!(AttackState::default(), AttackState::New);
    }
}
