//! A unit's stance: how far it goes after what it sees.

use std::cmp::Ordering;
use std::fmt::{Display, Error, Formatter};

/// A unit's stance (the unit AI's `+0x1a9`, the save's `state_value`): aggressive (0), defensive
/// (1), stand ground (2) or no attack (3). A new soldier may take its owner's posture's low byte,
/// so the byte keeps any value; the game's tests name only these four.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum UnitStance {
    /// 0: it goes after what it sees.
    #[default]
    Aggressive,
    /// 1: it goes after what comes near, on a leash from its state position.
    Defensive,
    /// 2: it attacks only what is in reach.
    StandGround,
    /// 3: it attacks nothing.
    NoAttack,
    /// A value the game names nowhere, as the save or the game holds it.
    Other(Unnamed),
}

/// A raw value of [`UnitStance`] that names none of its variants: only the set's own
/// constructors make one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Unnamed(u8);

impl UnitStance {
    /// The value of raw `value`: its variant, else `Other`.
    const fn of_raw(value: u8) -> Self {
        match value {
            0 => Self::Aggressive,
            1 => Self::Defensive,
            2 => Self::StandGround,
            3 => Self::NoAttack,
            other => Self::Other(Unnamed(other)),
        }
    }

    /// Its raw value.
    const fn raw(self) -> u8 {
        match self {
            Self::Aggressive => 0,
            Self::Defensive => 1,
            Self::StandGround => 2,
            Self::NoAttack => 3,
            Self::Other(Unnamed(value)) => value,
        }
    }

    /// The stance of byte `value`.
    #[must_use]
    pub const fn of_byte(value: u8) -> Self {
        Self::of_raw(value)
    }

    /// Its byte.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.raw()
    }

    /// Whether its byte is below `other`'s (the game's unsigned compares, as a const fn).
    #[must_use]
    pub const fn below(self, other: Self) -> bool {
        self.raw() < other.raw()
    }

    /// Defensive or stand ground: the stances the retarget and the flight hold to a leash.
    #[must_use]
    pub const fn leashed(self) -> bool {
        matches!(self, Self::Defensive | Self::StandGround)
    }

    /// Its place among the four (0 to 3) a group's count of its members' stances keeps
    /// (`FUN_14100d140`), none for another byte.
    #[must_use]
    pub const fn counted(self) -> Option<usize> {
        if self.raw() <= Self::NoAttack.raw() {
            Some(self.raw() as usize)
        } else {
            None
        }
    }
}

impl PartialOrd for UnitStance {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for UnitStance {
    /// The game's order: by the raw value.
    fn cmp(&self, other: &Self) -> Ordering {
        self.raw().cmp(&other.raw())
    }
}

impl Display for UnitStance {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{}", self.raw())
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
                UnitStance::Aggressive,
                UnitStance::Defensive,
                UnitStance::StandGround,
                UnitStance::NoAttack,
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
        assert!(UnitStance::Defensive < UnitStance::StandGround);
        assert!(UnitStance::Defensive.below(UnitStance::StandGround));
        assert!(!UnitStance::StandGround.below(UnitStance::StandGround));
    }

    /// Each named value and one the set names nowhere go to their variant and back.
    #[test]
    fn every_unit_stance_value_maps_to_its_variant_and_back() {
        for value in [0, 1, 2, 3, 4] {
            assert_eq!(UnitStance::of_raw(value).raw(), value);
        }
        assert!(matches!(UnitStance::of_raw(4), UnitStance::Other(_)));
    }
}
