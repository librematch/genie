//! A technology's id as one type: the place of a technology in the tech tree and in a player's
//! technologies, the `.dat`'s technology record index.

use crate::GenieError;
use getset::CopyGetters;
use std::fmt::{Display, Error as FmtError, Formatter};

/// A technology's id (the `.dat`'s technology index, the place in a player's technologies), 0
/// or more. A field that holds −1 for none holds `Option<TechId>` ([`TechId::of_field`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, CopyGetters)]
pub struct TechId(
    /// Its value, the technology's place in the tree.
    #[getset(get_copy = "pub")]
    i16,
);

impl TechId {
    /// The value a field takes for none.
    const NONE: i16 = -1;

    /// Spies (0x198), whose cost grows with the enemies' villagers (`FUN_1413cbd80`).
    pub const SPIES: Self = Self(0x198);
    /// The technology whose cost adjustments scale by attribute 38 (0x2f3) instead of 1.
    pub const SCALED_BY_38: Self = Self(0x2f3);
    /// The first age technology (`FUN_1413cc3c0`: 101 to 103).
    pub const FIRST_AGE: Self = Self(101);
    /// The last age technology.
    pub const LAST_AGE: Self = Self(103);
    /// Castle Age (`0x66`), whose RESEARCH marks the treaty monitor in game mode 16
    /// ([`crate::world::GameMode::ONE_UNIT`]).
    pub const CASTLE_AGE: Self = Self(0x66);

    /// Its place in a table by technology id.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0.unsigned_abs() as usize
    }

    /// The technology of a raw field that holds −1 for none.
    ///
    /// # Errors
    ///
    /// A negative value other than −1, which no technology's place is.
    pub fn of_field(value: i16) -> Result<Option<Self>, GenieError> {
        (value != Self::NONE)
            .then(|| Self::try_from(value))
            .transpose()
    }

    /// The technology a data field names, none for a negative value: the `.dat` and the save hold
    /// −1 for no technology, and the game's lookup of a negative id finds none.
    #[must_use]
    pub fn of_data(value: i16) -> Option<Self> {
        (0 <= value).then_some(Self(value))
    }

    /// The technology at place `index` of a table by technology id.
    ///
    /// # Errors
    ///
    /// An index past `i16`, which no table holds.
    pub fn of_index(index: usize) -> Result<Self, GenieError> {
        i16::try_from(index)
            .map_err(|_| {
                GenieError::invalid_input("a technology index past i16").with("index", index)
            })
            .map(Self)
    }

    /// The value of a field that holds −1 for none.
    #[must_use]
    pub const fn field(tech: Option<Self>) -> i16 {
        match tech {
            Some(t) => t.0,
            None => Self::NONE,
        }
    }
}

impl Display for TechId {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), FmtError> {
        self.0.fmt(f)
    }
}

impl genie_diff::Diff for TechId {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

impl TryFrom<i16> for TechId {
    type Error = GenieError;

    /// The technology of value `value`.
    ///
    /// # Errors
    ///
    /// A negative value, which no technology's place is.
    fn try_from(value: i16) -> Result<Self, GenieError> {
        if value < 0 {
            return Err(GenieError::invalid_input(
                "a negative technology id, which no technology has",
            )
            .with("tech", value));
        }
        Ok(Self(value))
    }
}

#[cfg(test)]
mod tests {
    use crate::ErrorKind;
    use crate::technology::TechId;

    /// A field's −1 is none, a value of 0 or more a technology, and each converts back.
    #[test]
    fn field_minus_one_is_none_and_any_value_from_0_technology() {
        assert_eq!(TechId::of_field(-1), Ok(None));
        for value in [0, 101, i16::MAX] {
            let tech = TechId::of_field(value).unwrap();
            assert_eq!(tech.map(|t| t.get()), Some(value));
            assert_eq!(TechId::field(tech), value);
        }
        assert_eq!(TechId::field(None), -1);
    }

    /// A negative value is no technology; in a field, one other than −1 is an error.
    #[test]
    fn negative_value_is_no_technology() {
        for value in [-1, -2, i16::MIN] {
            assert_eq!(
                TechId::try_from(value).map_err(|e| e.kind()),
                Err(ErrorKind::InvalidInput)
            );
        }
        assert_eq!(
            TechId::of_field(-2).map_err(|e| e.kind()),
            Err(ErrorKind::InvalidInput)
        );
    }
}
