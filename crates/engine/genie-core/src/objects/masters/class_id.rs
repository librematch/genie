//! A master's class as one type: the group of masters the game treats alike (villagers, siege
//! weapons, gates, farms), the value the `.dat` names `unit_class`.

use crate::GenieError;
use genie_diff::{Diff, Differ};

/// A master's class (`+0x410`, the `.dat`'s `unit_class`), 0 or more, or −1 for a master that has
/// none ([`ClassId::of_master`]). The `.dat` holds classes
/// that [`crate::objects::masters::class`] does not name (it names the ones the simulation tells
/// apart), so the class is a newtype with named constants, not an enum: any class the data
/// holds is a class, and the named ones are values of the type.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
    pub fn of_field(value: i16) -> Result<Option<Self>, GenieError> {
        (value != Self::NONE)
            .then(|| Self::try_from(value))
            .transpose()
    }

    /// The value of a field that holds −1 for none (a packet, a notify's parameter, the
    /// game's own save).
    #[must_use]
    pub const fn field(class: Option<Self>) -> i16 {
        match class {
            Some(c) => c.0,
            None => Self::NONE,
        }
    }

    /// The class of an `i32` field or parameter that holds −1 for none: a saved gather's or
    /// hunt's target type (`+0x68`), a notify's class parameter.
    ///
    /// # Errors
    ///
    /// A negative value other than −1 or one past a class's `i16`.
    pub fn of_param(value: i32) -> Result<Option<Self>, GenieError> {
        let short = i16::try_from(value)
            .map_err(|_| GenieError::invalid_input("a class past an i16").with("class", value))?;
        Self::of_field(short)
    }

    /// A class the code names, in a constant: only [`crate::objects::masters::class`] makes
    /// them.
    pub(super) const fn named(value: i16) -> Self {
        assert!(0 <= value, "a negative class");
        Self(value)
    }
}

impl std::fmt::Display for ClassId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Diff for ClassId {
    fn diff(&self, other: &Self, d: &mut Differ) {
        d.equal(self, other);
    }
}

impl TryFrom<i16> for ClassId {
    type Error = GenieError;

    /// The class of value `value`.
    ///
    /// # Errors
    ///
    /// A negative value, which no class is.
    fn try_from(value: i16) -> Result<Self, GenieError> {
        if value < 0 {
            return Err(
                GenieError::invalid_input("a negative class, which no master has")
                    .with("class", value),
            );
        }
        Ok(Self(value))
    }
}

#[cfg(test)]
mod tests {
    use crate::ErrorKind;
    use crate::objects::masters::{ClassId, class};

    /// A class is 0 or more; a negative value is no class.
    #[test]
    fn negative_value_is_no_class() {
        assert_eq!(ClassId::try_from(0).map(ClassId::get), Ok(0));
        assert_eq!(ClassId::try_from(99).map(ClassId::get), Ok(99));
        assert_eq!(
            ClassId::try_from(-1).map_err(|e| e.kind()),
            Err(ErrorKind::InvalidInput)
        );
    }

    /// A master's −1 is no class as an optional field holds it; any other class is itself.
    #[test]
    fn master_minus_one_is_no_class_in_field() {
        assert_eq!(class::NO_CLASS.or_none(), None);
        assert_eq!(class::FARM.or_none(), Some(class::FARM));
    }

    /// A field of −1 is no class, any other value a class, and each converts back to its value.
    #[test]
    fn field_minus_one_is_none_and_any_other_value_class() {
        assert_eq!(ClassId::of_field(-1), Ok(None));
        assert_eq!(ClassId::of_field(0), Ok(Some(class::CLASS_0)));
        assert_eq!(ClassId::field(None), -1);
        assert_eq!(ClassId::field(Some(class::VILLAGER)), 4);
        assert_eq!(
            ClassId::of_field(-2).map_err(|e| e.kind()),
            Err(ErrorKind::InvalidInput)
        );
    }

    /// A master of the `.dat` may hold −1, which is a value of the type with no registry place.
    #[test]
    fn master_without_class_holds_minus_one() {
        assert_eq!(ClassId::of_master(-1).get(), -1);
        assert_eq!(ClassId::of_master(-1).index(), None);
        assert_eq!(class::VILLAGER.index(), Some(4));
        assert_eq!(ClassId::of_master(4), class::VILLAGER);
    }

    /// The named classes are values of the type, equal to the data's.
    #[test]
    fn the_named_classes_are_values_of_the_type() {
        assert_eq!(class::VILLAGER, ClassId::try_from(4).unwrap());
        assert_ne!(class::VILLAGER, class::BUILDING);
        assert!(class::BUILDING < class::VILLAGER);
    }
}
