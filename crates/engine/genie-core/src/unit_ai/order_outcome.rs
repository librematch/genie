//! What an order's processing answers ([`OrderOutcome`]) and what an order slot or a unit's
//! command answers ([`Answer`]); what slot 73's checks answer ([`RetargetCheck`],
//! [`MiscEnd`]), the walk back to the state position ([`WalkBack`]) and processIdle's
//! answer ([`IdleOutcome`]).

/// What slot 0x230 (`FUN_140c5c170`, [`crate::unit_ai::UnitAi::process_order`]) answers for an
/// order: the queue keeps an order that is not ready and drops the others; one taken ends the
/// queue's run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderOutcome {
    /// 0: the order is not taken.
    NotTaken,
    /// 1: the order is taken (its tail ran).
    Taken,
    /// 0xe: the order is not ready; the queue keeps it.
    NotReady,
}

/// What a slot answers for an order, a command, a move or a new target it is given (the
/// game's 1 or 0): a unit AI's order slots, a unit's commands, an action's take of a move or
/// a target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum Answer {
    /// 0: the slot does not take it.
    NotTaken,
    /// 1: the slot takes it.
    Taken,
}

/// What processIdle (slot 72, [`crate::unit_ai::UnitAi::process_idle`]) and the checks it
/// runs answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum IdleOutcome {
    /// 6: the check produced an action.
    Acted,
    /// 5: it did not.
    Idle,
}

impl OrderOutcome {
    /// The game's return value.
    #[must_use]
    pub const fn code(self) -> u32 {
        match self {
            Self::NotTaken => 0,
            Self::Taken => 1,
            Self::NotReady => 0xe,
        }
    }
}

/// Whether slot 73 reached its retarget part ([`crate::unit_ai::UnitAi::retarget_check`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum RetargetCheck {
    /// The retarget timer ran out: the timer is drawn again and the checks go on.
    Due,
    /// A part ended the slot, retargeting is off, or the timer runs.
    NotDue,
}

/// Whether one of slot 73's parts ends the slot (its 9 or 10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum MiscEnd {
    /// The slot ends there.
    Ends,
    /// The slot goes on.
    GoesOn,
}

/// Whether a unit walks back to its state position ([`crate::unit_ai::UnitAi::walk_back`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum WalkBack {
    /// It walks.
    Walks,
    /// It is within the leash, or has no way back.
    Stays,
}

#[cfg(test)]
mod tests {
    use crate::unit_ai::OrderOutcome;

    /// The three answers are the game's 0, 1 and 0xe.
    #[test]
    fn the_outcomes_are_the_game_codes() {
        assert_eq!(
            [
                OrderOutcome::NotTaken,
                OrderOutcome::Taken,
                OrderOutcome::NotReady
            ]
            .map(OrderOutcome::code),
            [0, 1, 0xe]
        );
    }
}
