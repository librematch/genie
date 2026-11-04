//! A master's class as one type: the group of masters the game treats alike (villagers, siege
//! weapons, gates, farms), the value the `.dat` names `unit_class`.

use crate::{GenieError, GenieResult};
use derive_more::Display;
use genie_diff::{Diff, Differ};
use serde::Deserialize;
use serde::Serialize;

/// A master's class (`+0x410`, the `.dat`'s `unit_class`), 0 or more, or −1 for a master that has
/// none ([`ClassId::of_master`]). The `.dat` holds classes the engine does not tell apart, and
/// the ones it does are roles of the build layer (`Roles::of_class`, FD-046-D03), so the class
/// is a newtype, not an enum: any class the data holds is a class.
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
    Serialize,
    Deserialize,
    Display,
)]
#[serde(try_from = "i16", into = "i16")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ClassId(
    /// Its value, as the `.dat` and the obfuscated build's decoded short hold it.
    i16,
);

impl ClassId {
    /// Its value, for a table the data indexes by class and for the task's object group (a class
    /// or a group), which stays an `i16`.
    #[must_use]
    pub const fn get(self) -> i16 {
        self.0
    }

    /// Its place in a table the registry keeps by class; None for the class of a master that has
    /// none ([`ClassId::of_master`] of −1).
    #[must_use]
    pub fn index(self) -> Option<usize> {
        usize::try_from(self.0).ok()
    }

    /// The class a master holds (`+0x410`, the `.dat`'s `unit_class`): −1 for a master without
    /// one, as 5733 of the `.dat`'s units (the empty slots of each civ's table) hold it. No
    /// search or table of the simulation matches it.
    #[must_use]
    pub const fn of_master(value: i16) -> Self {
        Self(value)
    }

    /// The class as a field that holds −1 for none takes it: None for the −1 of a master
    /// without one.
    #[must_use]
    pub const fn or_none(self) -> Option<Self> {
        if self.0 < 0 { None } else { Some(self) }
    }

    /// The value a field takes for none.
    const NONE: i16 = -1;

    /// The class of a raw field that holds −1 for none.
    ///
    /// # Errors
    ///
    /// A negative value other than −1, which no class is.
    pub fn of_field(value: i16) -> GenieResult<Option<Self>> {
        (value != Self::NONE)
            .then(|| Self::try_from(value))
            .transpose()
    }

    /// The value of a field that holds −1 for none (a packet, a notify's parameter, the
    /// game's own save).
    #[must_use]
    pub const fn field(class: Option<Self>) -> i16 {
        match class {
            Some(class_id) => class_id.0,
            None => Self::NONE,
        }
    }

    /// The class of an `i32` field or parameter that holds −1 for none: a saved gather's or
    /// hunt's target type (`+0x68`), a notify's class parameter.
    ///
    /// # Errors
    ///
    /// A negative value other than −1 or one past a class's `i16`.
    pub fn of_param(value: i32) -> GenieResult<Option<Self>> {
        let short = i16::try_from(value)
            .map_err(|_| GenieError::invalid_input("a class past an i16").with("class", value))?;
        Self::of_field(short)
    }

    /// A class a constant names: a slot of the build layer's fixture, a test's class.
    ///
    /// # Panics
    ///
    /// For a negative value, which no class is (at compile time in a constant).
    #[must_use]
    pub const fn named(value: i16) -> Self {
        assert!(0 <= value, "a negative class");
        Self(value)
    }
}

/// A master's class in its serde form (the model's cache, FD-046-D18): the short the master
/// holds, −1 too ([`ClassId::of_master`]). `ClassId`'s own form, which the build layer's roles
/// read, refuses a negative class.
pub mod master_class {
    use crate::objects::masters::ClassId;
    use serde::Deserialize;
    use serde::Deserializer;
    use serde::Serializer;

    /// Writes the class's short.
    ///
    /// # Errors
    ///
    /// The serializer's.
    pub fn serialize<S: Serializer>(class: &ClassId, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i16(class.get())
    }

    /// Reads a short as a master's class.
    ///
    /// # Errors
    ///
    /// The deserializer's, for a value that is no short.
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<ClassId, D::Error> {
        i16::deserialize(deserializer).map(ClassId::of_master)
    }
}

impl Diff for ClassId {
    fn diff(&self, other: &Self, differ: &mut Differ) {
        differ.equal(self, other);
    }
}

impl TryFrom<i16> for ClassId {
    type Error = GenieError;

    /// The class of value `value`.
    ///
    /// # Errors
    ///
    /// A negative value, which no class is.
    fn try_from(value: i16) -> GenieResult<Self> {
        if value < 0 {
            return Err(
                GenieError::invalid_input("a negative class, which no master has")
                    .with("class", value),
            );
        }
        Ok(Self(value))
    }
}

impl From<ClassId> for i16 {
    /// The class's value, as a slot's serde form holds it.
    fn from(id: ClassId) -> Self {
        id.0
    }
}

#[cfg(test)]
mod tests {
    use crate::ErrorKind;
    use crate::objects::masters::ClassId;

    /// A class is 0 or more; a negative value is no class.
    #[test]
    fn negative_value_is_no_class() {
        assert_eq!(ClassId::try_from(0).map(ClassId::get), Ok(0));
        assert_eq!(ClassId::try_from(99).map(ClassId::get), Ok(99));
        assert_eq!(
            ClassId::try_from(-1).map_err(|error| error.kind()),
            Err(ErrorKind::InvalidInput)
        );
    }

    /// A master's −1 is no class as an optional field holds it; any other class is itself.
    #[test]
    fn master_minus_one_is_no_class_in_field() {
        assert_eq!(ClassId::of_master(-1).or_none(), None);
        assert_eq!(ClassId::named(49).or_none(), Some(ClassId::named(49)));
    }

    /// A field of −1 is no class, any other value a class, and each converts back to its value.
    #[test]
    fn field_minus_one_is_none_and_any_other_value_class() {
        assert_eq!(ClassId::of_field(-1), Ok(None));
        assert_eq!(ClassId::of_field(0), Ok(Some(ClassId::named(0))));
        assert_eq!(ClassId::field(None), -1);
        assert_eq!(ClassId::field(Some(ClassId::named(4))), 4);
        assert_eq!(
            ClassId::of_field(-2).map_err(|error| error.kind()),
            Err(ErrorKind::InvalidInput)
        );
    }

    /// A master of the `.dat` may hold −1, which is a value of the type with no registry place.
    #[test]
    fn master_without_class_holds_minus_one() {
        assert_eq!(ClassId::of_master(-1).get(), -1);
        assert_eq!(ClassId::of_master(-1).index(), None);
        assert_eq!(ClassId::named(4).index(), Some(4));
        assert_eq!(ClassId::of_master(4), ClassId::named(4));
    }

    /// A class a constant names is a value of the type, equal to the data's.
    #[test]
    fn the_named_classes_are_values_of_the_kind() {
        assert_eq!(ClassId::named(4), ClassId::try_from(4).unwrap());
        assert_ne!(ClassId::named(4), ClassId::named(3));
        assert!(ClassId::named(3) < ClassId::named(4));
    }
}
