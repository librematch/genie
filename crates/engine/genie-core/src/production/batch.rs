//! How many units of a kind a cancel takes back ([`Batch`]).

use crate::production::QueuedUnits;

/// How many units of a kind a queue's cancel takes back (`FUN_1411d66b0`'s flag): one, or up
/// to five.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Batch {
    /// One unit or research.
    #[default]
    One,
    /// Up to five of the kind.
    Five,
}

impl Batch {
    /// The units it counts (`flag * 4 + 1`): 1 or 5.
    #[must_use]
    pub const fn units(self) -> QueuedUnits {
        match self {
            Self::One => QueuedUnits::of_data(1),
            Self::Five => QueuedUnits::of_data(5),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::production::{Batch, QueuedUnits};

    /// One counts 1 unit, five 5 (the game's `flag * 4 + 1`).
    #[test]
    fn batches_count_one_or_five() {
        assert_eq!(Batch::One.units(), QueuedUnits::of_data(1));
        assert_eq!(Batch::Five.units(), QueuedUnits::of_data(5));
    }
}
