//! What an order's processing answers ([`OrderOutcome`]).

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
