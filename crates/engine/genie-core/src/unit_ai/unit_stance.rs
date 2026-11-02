//! A unit's stance: how far it goes after what it sees.

use std::fmt::{Display, Error, Formatter};

/// A unit's stance (the unit AI's `+0x1a9`, the save's `state_value`): aggressive (0), defensive
/// (1), stand ground (2) or no attack (3). A new soldier may take its owner's posture's low byte,
/// so the byte keeps any value; the game's tests name only these four.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnitStance(u8);

impl UnitStance {
    /// 0: it goes after what it sees.
    pub const AGGRESSIVE: Self = Self(0);
    /// 1: it goes after what comes near, on a leash from its state position.
    pub const DEFENSIVE: Self = Self(1);
    /// 2: it attacks only what is in reach.
    pub const STAND_GROUND: Self = Self(2);
    /// 3: it attacks nothing.
    pub const NO_ATTACK: Self = Self(3);

    /// The stance of byte `value`.
    #[must_use]
    pub const fn of_byte(value: u8) -> Self {
        Self(value)
    }

    /// Its byte.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// Whether its byte is below `other`'s (the game's unsigned compares, as a const fn).
    #[must_use]
    pub const fn below(self, other: Self) -> bool {
        self.0 < other.0
    }

    /// Defensive or stand ground: the stances the retarget and the flight hold to a leash.
    #[must_use]
    pub const fn leashed(self) -> bool {
        matches!(self, Self::DEFENSIVE | Self::STAND_GROUND)
    }

    /// Its place among the four (0 to 3) a group's count of its members' stances keeps
    /// (`FUN_14100d140`), none for another byte.
    #[must_use]
    pub const fn counted(self) -> Option<usize> {
        if self.0 <= Self::NO_ATTACK.0 {
            Some(self.0 as usize)
        } else {
            None
        }
    }
}

impl Display for UnitStance {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{}", self.0)
    }
}

impl genie_diff::Diff for UnitStance {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::unit_ai::UnitStance;

    /// The four stances are 0 to 3; defensive and stand ground are leashed; a group counts only
    /// those four.
    #[test]
    fn stance_is_its_byte_and_group_counts_the_four() {
        assert_eq!(
            [
                UnitStance::AGGRESSIVE,
                UnitStance::DEFENSIVE,
                UnitStance::STAND_GROUND,
                UnitStance::NO_ATTACK,
                UnitStance::of_byte(7),
            ]
            .map(|s| (s.get(), s.leashed(), s.counted())),
            [
                (0, false, Some(0)),
                (1, true, Some(1)),
                (2, true, Some(2)),
                (3, false, Some(3)),
                (7, false, None),
            ]
        );
        assert!(UnitStance::DEFENSIVE < UnitStance::STAND_GROUND);
        assert!(UnitStance::DEFENSIVE.below(UnitStance::STAND_GROUND));
        assert!(!UnitStance::STAND_GROUND.below(UnitStance::STAND_GROUND));
    }
}
