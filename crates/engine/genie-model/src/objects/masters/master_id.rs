//! A master's id as one type: the master a command builds, trains or turns an object into,
//! and a target's master.

use crate::GenieError;
use derive_more::Display;
use getset::CopyGetters;
use serde::Deserialize;
use serde::Serialize;

/// A master's id: the `.dat`'s unit id, the place of the master in a player's masters
/// ([`crate::objects::masters::Master::id`]), so 0 or more. A field that holds −1 for none
/// holds `Option<MasterId>` ([`MasterId::of_field`]).
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    CopyGetters,
    Serialize,
    Deserialize,
    Display,
)]
#[serde(try_from = "i16", into = "i16")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct MasterId(
    /// Its value, the master's place in a player's masters.
    #[getset(get_copy = "pub")]
    i16,
);

impl MasterId {
    /// The value a field takes for none.
    const NONE: i16 = -1;

    /// A master by its `.dat` id: a test's, or a slot of the build layer's generated fixture.
    ///
    /// # Panics
    ///
    /// For a negative value, which no master is (at compile time in a constant).
    #[must_use]
    pub const fn named(value: i16) -> Self {
        assert!(0 <= value, "a negative master id");
        Self(value)
    }

    /// Its place in a table by master id.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0.unsigned_abs() as usize
    }

    /// The master of a raw field that holds −1 for none.
    ///
    /// # Errors
    ///
    /// A negative value other than −1, which no master's place is.
    pub fn of_field(value: i16) -> Result<Option<Self>, GenieError> {
        (value != Self::NONE)
            .then(|| Self::try_from(value))
            .transpose()
    }

    /// The master a data field names, none for a negative value: the `.dat` and the save hold −1
    /// for a master that is not there, and the game's lookup of a negative id finds none.
    #[must_use]
    pub fn of_data(value: i16) -> Option<Self> {
        (0 <= value).then_some(Self(value))
    }

    /// The value of a field that holds −1 for none.
    #[must_use]
    pub const fn field(master: Option<Self>) -> i16 {
        match master {
            Some(m) => m.0,
            None => Self::NONE,
        }
    }
}

impl genie_diff::Diff for MasterId {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

impl TryFrom<i16> for MasterId {
    type Error = GenieError;

    /// The master of value `value`.
    ///
    /// # Errors
    ///
    /// A negative value, which no master's place is.
    fn try_from(value: i16) -> Result<Self, GenieError> {
        if value < 0 {
            return Err(
                GenieError::invalid_input("a negative master id, which no master has")
                    .with("master", value),
            );
        }
        Ok(Self(value))
    }
}

impl From<MasterId> for i16 {
    /// The master's id, as a slot's serde form holds it.
    fn from(id: MasterId) -> Self {
        id.0
    }
}

#[cfg(test)]
mod tests {
    use crate::ErrorKind;
    use crate::objects::masters::MasterId;

    /// A field's −1 is none, a value of 0 or more a master, and each converts back.
    #[test]
    fn field_minus_one_is_none_and_any_value_from_0_master() {
        assert_eq!(MasterId::of_field(-1), Ok(None));
        for value in [0, 83, i16::MAX] {
            let master = MasterId::of_field(value).unwrap();
            assert_eq!(master.map(|m| m.get()), Some(value));
            assert_eq!(MasterId::field(master), value);
        }
        assert_eq!(MasterId::field(None), -1);
    }

    /// A negative value is no master; in a field, one other than −1 is an error.
    #[test]
    fn negative_value_is_no_master() {
        for value in [-1, -2, i16::MIN] {
            assert_eq!(
                MasterId::try_from(value).map_err(|e| e.kind()),
                Err(ErrorKind::InvalidInput)
            );
        }
        assert_eq!(
            MasterId::of_field(-2).map_err(|e| e.kind()),
            Err(ErrorKind::InvalidInput)
        );
    }
}
