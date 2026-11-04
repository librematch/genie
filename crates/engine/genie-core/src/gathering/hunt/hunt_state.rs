//! A Hunt's state: the steps of its update from the attack to the drop, each walk with its
//! `MoveTo` and the attack with its Attack.

use crate::GenieError;
use crate::combat::attack::Attack;
use crate::movement::move_to::MoveToAction;
use derive_more::Display;
use genie_diff::Diff;

/// A Hunt's state (`+0x18`, set by `set_state`) with the data only that state holds
/// (FD-045-D62): every `set_state` drops the sub-actions first (`FUN_140a98620`), the four
/// walks make their quiet `MoveTo` and the attack its Attack, none once the sub-action's update
/// dropped it, until the next `set_state`. Its update (`0x140ac7a00`) switches on it. The bytes
/// without a state (5, 9, 14 and above 15) have no case.
#[derive(Clone, Copy, Debug, PartialEq, Diff, Display)]
#[display("{}", self.get())]
pub enum HuntState {
    /// 0: made, not started (the constructor's).
    New,
    /// 1: done; the list removes it.
    Done,
    /// 2: done with its target, it waits (a tree's target class would search again).
    Idle,
    /// 3: searching: it told its unit AI to look for the next target (notify 0x202), again
    /// every 10 seconds.
    Search,
    /// 4: walking to a dead target.
    Walk {
        /// Its quiet `MoveTo` to the target within the task's range (`OWN_LIVESTOCK_RANGE` to
        /// its own player's livestock).
        move_to: Option<MoveToAction>,
    },
    /// 6: facing it.
    Face,
    /// 7: gathering from it.
    Gather,
    /// 8: taking the load to its drop site.
    Drop {
        /// Its quiet `MoveTo` to the drop site.
        move_to: Option<MoveToAction>,
    },
    /// 10 (`0xa`): leading its own livestock to the drop site, it waits for the animal to stop
    /// there.
    Lead,
    /// 11 (`0xb`): walking to the point after the target went on the way to it; its end
    /// searches.
    WalkSearch {
        /// Its quiet `MoveTo` to the point.
        move_to: Option<MoveToAction>,
    },
    /// 12 (`0xc`): attacking it.
    Attack {
        /// Its Attack sub-action on the target.
        attack: Option<Attack>,
    },
    /// 13 ("Failed"), the state every action shares.
    Failed,
    /// 15 (`0xf`): walking to a point a move gave it (its slot 0x40).
    WalkPoint {
        /// Its quiet `MoveTo` to the point.
        move_to: Option<MoveToAction>,
    },
}

impl HuntState {
    /// The state of a byte of the save or the game's memory, with the saved sub-actions: `walk`
    /// (a walk's `MoveTo`) and `attack` (the attack's Attack).
    ///
    /// # Errors
    ///
    /// A byte that names no state of a Hunt, and a sub-action in a state that does not make it.
    pub fn of_byte(
        value: u8,
        walk: Option<MoveToAction>,
        attack: Option<Attack>,
    ) -> Result<Self, GenieError> {
        let state = match value {
            0 => Self::New,
            1 => Self::Done,
            2 => Self::Idle,
            3 => Self::Search,
            4 => Self::Walk { move_to: walk },
            6 => Self::Face,
            7 => Self::Gather,
            8 => Self::Drop { move_to: walk },
            10 => Self::Lead,
            11 => Self::WalkSearch { move_to: walk },
            12 => Self::Attack { attack },
            13 => Self::Failed,
            15 => Self::WalkPoint { move_to: walk },
            _ => {
                return Err(GenieError::invalid_input("no state of a Hunt").with("state", value));
            }
        };
        if walk.is_some() && state.walk().is_none() {
            return Err(
                GenieError::invalid_input("a MoveTo of a Hunt that does not walk")
                    .with("state", value),
            );
        }
        if attack.is_some() && !matches!(state, Self::Attack { .. }) {
            return Err(
                GenieError::invalid_input("an Attack of a Hunt that does not attack")
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
            Self::Gather => 7,
            Self::Drop { .. } => 8,
            Self::Lead => 10,
            Self::WalkSearch { .. } => 11,
            Self::Attack { .. } => 12,
            Self::Failed => 13,
            Self::WalkPoint { .. } => 15,
        }
    }

    /// Its walk's `MoveTo` slot, none in a state that does not walk.
    const fn walk(&self) -> Option<&Option<MoveToAction>> {
        match self {
            Self::Walk { move_to }
            | Self::Drop { move_to }
            | Self::WalkSearch { move_to }
            | Self::WalkPoint { move_to } => Some(move_to),
            _ => None,
        }
    }

    /// Its walk's `MoveTo` slot to write, none in a state that does not walk.
    pub const fn walk_mut(&mut self) -> Option<&mut Option<MoveToAction>> {
        match self {
            Self::Walk { move_to }
            | Self::Drop { move_to }
            | Self::WalkSearch { move_to }
            | Self::WalkPoint { move_to } => Some(move_to),
            _ => None,
        }
    }

    /// Its walk's `MoveTo`, none in any other state.
    #[must_use]
    pub const fn move_to(&self) -> Option<&MoveToAction> {
        match self.walk() {
            Some(Some(m)) => Some(m),
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

    /// Its attack's Attack, none in any other state.
    #[must_use]
    pub const fn attack(&self) -> Option<&Attack> {
        match self {
            Self::Attack { attack: Some(a) } => Some(a),
            _ => None,
        }
    }

    /// Its attack's Attack slot to write, none in any other state.
    pub const fn attack_mut(&mut self) -> Option<&mut Option<Attack>> {
        match self {
            Self::Attack { attack } => Some(attack),
            _ => None,
        }
    }

    /// Its attack's Attack taken out (the state stays), none in any other state.
    pub const fn take_attack(&mut self) -> Option<Attack> {
        match self.attack_mut() {
            Some(a) => a.take(),
            None => None,
        }
    }

    /// The same state without its sub-actions: the state a `set_state` names.
    #[must_use]
    pub const fn bare(self) -> Self {
        let mut s = self;
        let _ = s.take_move_to();
        let _ = s.take_attack();
        s
    }
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    use crate::combat::attack::AttackState;
    #[cfg(test)]
    use crate::combat::attack::tests::own_attack;
    use crate::gathering::hunt::HuntState;
    #[cfg(test)]
    use crate::map::Distance;
    use crate::movement::move_to::{MoveTo, MoveToAction};

    /// Each state reads back from its byte; the bytes without a state are errors, and so is a
    /// sub-action of a state that does not make it; the walks give their `MoveTo`, the attack
    /// its Attack.
    #[test]
    fn hunt_states_read_back_from_their_bytes() {
        for b in 0..=255_u8 {
            match HuntState::of_byte(b, None, None) {
                Ok(s) => assert_eq!(s.get(), b),
                Err(_) => assert!(matches!(b, 5 | 9 | 14 | 16..), "{b}"),
            }
        }
        assert_eq!(HuntState::Attack { attack: None }.to_string(), "12");
        let walk = MoveToAction::sub_in(MoveTo::Moving);
        for b in [4, 8, 11, 15] {
            let mut s = HuntState::of_byte(b, Some(walk), None).unwrap();
            assert_eq!((s.get(), s.move_to(), s.attack()), (b, Some(&walk), None));
            assert_eq!(s.bare().move_to(), None);
            assert_eq!(s.take_move_to(), Some(walk));
            assert_eq!(s.move_to(), None);
        }
        for b in [2, 12] {
            assert!(HuntState::of_byte(b, Some(walk), None).is_err(), "{b}");
        }
        let attack = own_attack(5, AttackState::New, Distance::ZERO);
        let mut s = HuntState::of_byte(12, None, Some(attack)).unwrap();
        assert_eq!((s.attack(), s.move_to()), (Some(&attack), None));
        assert_eq!(s.bare().attack(), None);
        assert_eq!(s.take_attack(), Some(attack));
        assert!(HuntState::of_byte(4, None, Some(attack)).is_err());
        assert_eq!(HuntState::Idle.bare(), HuntState::Idle);
    }
}
