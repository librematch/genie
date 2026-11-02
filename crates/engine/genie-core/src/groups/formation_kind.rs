//! A formation's type as one type: `BFormation`'s `mType`, a group's lock, a unit AI's
//! formation type and the `FORM_FORMATION` command's.

use std::fmt::{Display, Error, Formatter};

/// A formation's type (`mType`; the log names of `UpdateDesiredPositions`, `FUN_14103fd00`):
/// the layout a group's members take. A group's lock (`mFormationLockedType`), a unit AI's
/// formation type (`+0x1d4`, a byte) and `FORM_FORMATION`'s `+0x8` hold one too. Every value
/// is a type: the layout's switch lays out the named ones and only transforms any other.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FormationKind(i32);

impl FormationKind {
    /// A horde (`FUN_14100f6b0`): the members where they stand. As a group's lock, no lock.
    pub const HORDE: Self = Self(0);
    /// A column (`FUN_141012280` with a column ratio).
    pub const COLUMN: Self = Self(1);
    /// A line (`FUN_141012280` with a line ratio).
    pub const LINE: Self = Self(2);
    /// A phalanx (`FUN_1410147b0`).
    pub const PHALANX: Self = Self(3);
    /// A box (`FUN_141011a00`).
    pub const BOX: Self = Self(4);
    /// A user defined formation (`FUN_141014f60`).
    pub const USER_DEFINED: Self = Self(5);
    /// Type 6, which the layout's switch does not know (only transformed); an attacking group
    /// of it refills its slots when its members change.
    pub const TYPE_6: Self = Self(6);
    /// A staggered line (`FUN_141012280` staggered).
    pub const STAGGERED: Self = Self(7);
    /// A flanking formation (`FUN_141012280` flanked).
    pub const FLANKED: Self = Self(8);
    /// The types a group's members vote for (`FUN_141024c40`'s nine counts).
    const VOTED: i32 = 9;

    /// Its value.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }

    /// A saved or given type: −1 is none, any other value a type.
    #[must_use]
    pub const fn of_field(value: i32) -> Option<Self> {
        if value == -1 { None } else { Some(Self(value)) }
    }

    /// The field of a type: −1 for none.
    #[must_use]
    pub const fn field(kind: Option<Self>) -> i32 {
        match kind {
            Some(k) => k.0,
            None => -1,
        }
    }

    /// A unit AI's byte (`+0x1d4`) as its type.
    #[must_use]
    pub const fn of_byte(value: u8) -> Self {
        Self(value as i32)
    }

    /// The type as a unit AI's byte keeps it (`FUN_14100d140` stores the low byte).
    #[must_use]
    pub const fn unit_byte(self) -> Self {
        Self::of_byte(self.0 as u8)
    }

    /// The vote a unit AI's type counts for in its group's choice (`FUN_141024c40`): the types
    /// 0 to 8, by their value; none for any other.
    #[must_use]
    pub const fn vote(self) -> Option<usize> {
        if 0 <= self.0 && self.0 < Self::VOTED {
            Some(self.0 as usize)
        } else {
            None
        }
    }

    /// The type a vote names.
    #[must_use]
    pub const fn of_vote(vote: usize) -> Self {
        Self(vote as i32)
    }
}

impl From<i32> for FormationKind {
    /// The type a command or a field holds: every value is one.
    fn from(value: i32) -> Self {
        Self(value)
    }
}

impl Display for FormationKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        self.0.fmt(f)
    }
}

impl genie_diff::Diff for FormationKind {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::groups::FormationKind;

    /// −1 is no type; a unit AI's byte round-trips; only the types 0 to 8 vote.
    #[test]
    fn type_reads_its_field_byte_and_vote() {
        assert_eq!(FormationKind::of_field(-1), None);
        assert_eq!(FormationKind::of_field(4), Some(FormationKind::BOX));
        assert_eq!(FormationKind::field(None), -1);
        assert_eq!(FormationKind::field(Some(FormationKind::FLANKED)), 8);
        assert_eq!(FormationKind::of_byte(7), FormationKind::STAGGERED);
        assert_eq!(
            FormationKind::from(0x105).unit_byte(),
            FormationKind::USER_DEFINED
        );
        assert_eq!(FormationKind::FLANKED.vote(), Some(8));
        assert_eq!(FormationKind::from(9).vote(), None);
        assert_eq!(FormationKind::from(-2).vote(), None);
        assert_eq!(FormationKind::of_vote(3), FormationKind::PHALANX);
    }
}
