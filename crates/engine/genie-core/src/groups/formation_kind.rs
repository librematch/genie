//! A formation's type as one type: `BFormation`'s `mType`, a group's lock, a unit AI's
//! formation type and the `FORM_FORMATION` command's.

use std::fmt::{Display, Error, Formatter};

/// A formation's type (`mType`; the log names of `UpdateDesiredPositions`, `FUN_14103fd00`):
/// the layout a group's members take. A group's lock (`mFormationLockedType`), a unit AI's
/// formation type (`+0x1d4`, a byte) and `FORM_FORMATION`'s `+0x8` hold one too. Every value
/// is a type: the layout's switch lays out the named ones and only transforms any other.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FormationKind {
    /// A horde (`FUN_14100f6b0`): the members where they stand. As a group's lock, no lock.
    #[default]
    Horde,
    /// A column (`FUN_141012280` with a column ratio).
    Column,
    /// A line (`FUN_141012280` with a line ratio).
    Line,
    /// A phalanx (`FUN_1410147b0`).
    Phalanx,
    /// A box (`FUN_141011a00`).
    Box,
    /// A user defined formation (`FUN_141014f60`).
    UserDefined,
    /// Type 6, which the layout's switch does not know (only transformed); an attacking group
    /// of it refills its slots when its members change.
    Kind6,
    /// A staggered line (`FUN_141012280` staggered).
    Staggered,
    /// A flanking formation (`FUN_141012280` flanked).
    Flanked,
    /// A value the game names nowhere, as the save or the game holds it.
    Other(Unnamed),
}

/// A raw value of [`FormationKind`] that names none of its variants: only the set's own
/// constructors make one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Unnamed(i32);

impl FormationKind {
    /// The value of raw `value`: its variant, else `Other`.
    const fn of_raw(value: i32) -> Self {
        match value {
            0 => Self::Horde,
            1 => Self::Column,
            2 => Self::Line,
            3 => Self::Phalanx,
            4 => Self::Box,
            5 => Self::UserDefined,
            6 => Self::Kind6,
            7 => Self::Staggered,
            8 => Self::Flanked,
            other => Self::Other(Unnamed(other)),
        }
    }

    /// Its raw value.
    const fn raw(self) -> i32 {
        match self {
            Self::Horde => 0,
            Self::Column => 1,
            Self::Line => 2,
            Self::Phalanx => 3,
            Self::Box => 4,
            Self::UserDefined => 5,
            Self::Kind6 => 6,
            Self::Staggered => 7,
            Self::Flanked => 8,
            Self::Other(Unnamed(value)) => value,
        }
    }

    /// The types a group's members vote for (`FUN_141024c40`'s nine counts).
    const VOTED: i32 = 9;

    /// Its value.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.raw()
    }

    /// A saved or given type: −1 is none, any other value a type.
    #[must_use]
    pub const fn of_field(value: i32) -> Option<Self> {
        if value == -1 {
            None
        } else {
            Some(Self::of_raw(value))
        }
    }

    /// The field of a type: −1 for none.
    #[must_use]
    pub const fn field(kind: Option<Self>) -> i32 {
        match kind {
            Some(k) => k.raw(),
            None => -1,
        }
    }

    /// A unit AI's byte (`+0x1d4`) as its type.
    #[must_use]
    pub const fn of_byte(value: u8) -> Self {
        Self::of_raw(value as i32)
    }

    /// The type as a unit AI's byte keeps it (`FUN_14100d140` stores the low byte).
    #[must_use]
    pub const fn unit_byte(self) -> Self {
        Self::of_byte(self.raw() as u8)
    }

    /// The vote a unit AI's type counts for in its group's choice (`FUN_141024c40`): the types
    /// 0 to 8, by their value; none for any other.
    #[must_use]
    pub const fn vote(self) -> Option<usize> {
        if 0 <= self.raw() && self.raw() < Self::VOTED {
            Some(self.raw() as usize)
        } else {
            None
        }
    }

    /// The type a vote names.
    #[must_use]
    pub const fn of_vote(vote: usize) -> Self {
        Self::of_raw(vote as i32)
    }
}

impl From<i32> for FormationKind {
    /// The type a command or a field holds: every value is one.
    fn from(value: i32) -> Self {
        Self::of_raw(value)
    }
}

impl Display for FormationKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        self.raw().fmt(f)
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
    fn kind_reads_its_field_byte_and_vote() {
        assert_eq!(FormationKind::of_field(-1), None);
        assert_eq!(FormationKind::of_field(4), Some(FormationKind::Box));
        assert_eq!(FormationKind::field(None), -1);
        assert_eq!(FormationKind::field(Some(FormationKind::Flanked)), 8);
        assert_eq!(FormationKind::of_byte(7), FormationKind::Staggered);
        assert_eq!(
            FormationKind::from(0x105).unit_byte(),
            FormationKind::UserDefined
        );
        assert_eq!(FormationKind::Flanked.vote(), Some(8));
        assert_eq!(FormationKind::from(9).vote(), None);
        assert_eq!(FormationKind::from(-2).vote(), None);
        assert_eq!(FormationKind::of_vote(3), FormationKind::Phalanx);
    }

    /// Each named value and one the set names nowhere go to their variant and back.
    #[test]
    fn every_formation_kind_value_maps_to_its_variant_and_back() {
        for value in [0, 1, 2, 3, 4, 5, 6, 7, 8, 9] {
            assert_eq!(FormationKind::of_raw(value).raw(), value);
        }
        assert!(matches!(FormationKind::of_raw(9), FormationKind::Other(_)));
    }
}
