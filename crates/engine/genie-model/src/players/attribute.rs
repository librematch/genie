//! The player's attributes (`player+0x8`'s array of `f32`) by number. The first four are the
//! resources (food, wood, stone, gold), the places of the player's resource array; any other
//! attribute the engine names is a role or a slot of the build layer (FD-046-D48). A master's
//! attribute types are a different set ([`crate::objects::masters::attribute`]).

use crate::{GenieError, GenieResult};
use derive_more::Display;
use getset::CopyGetters;
use serde::Deserialize;
use serde::Serialize;

/// A player's attribute number (`player+0x100`'s array, the `.dat`'s attribute types, an effect's
/// attribute argument): 0 or more. The first four are the resources (food, wood, stone, gold).
/// The engine names the ones it tells apart in the build layer (`Slots::attribute`, the
/// attributes' roles); any other is a number the data holds ([`AttributeId::named`]). A field
/// that holds −1 for none is `Option<AttributeId>`.
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
pub struct AttributeId(
    /// Its value, the attribute's place in the player's attributes.
    #[getset(get_copy = "pub")]
    i16,
);

impl AttributeId {
    /// An attribute by its number: a slot's value, a resource's place, a test's attribute.
    ///
    /// # Panics
    ///
    /// For a negative value, which no attribute is (at compile time in a constant).
    #[must_use]
    pub const fn named(value: i16) -> Self {
        assert!(0 <= value, "a negative attribute number");
        Self(value)
    }

    /// The four resources, food, wood, stone and gold: the first four attributes, in the order
    /// of the player's resource array (a cost's, a tribute's and a trickle's resource is its
    /// place among them).
    #[must_use]
    pub const fn resources() -> [Self; 4] {
        [Self(0), Self(1), Self(2), Self(3)]
    }

    /// Its place in a table by attribute number (the player's attributes).
    #[must_use]
    pub const fn index(self) -> usize {
        self.0.unsigned_abs() as usize
    }

    /// The attribute `by` places on: a table of attributes that run by player or by kind from this
    /// one (the trickle rates, the gathered totals).
    ///
    /// # Errors
    ///
    /// A number past the `i16` range.
    pub fn offset(self, by: usize) -> GenieResult<Self> {
        i16::try_from(by)
            .ok()
            .and_then(|by| self.0.checked_add(by))
            .map(Self)
            .ok_or_else(|| {
                GenieError::invalid_input("an attribute number past the table")
                    .with("attribute", self.0)
                    .with("offset", by)
            })
    }

    /// The attribute of a table place (a number of `usize`, as a command or a loop holds it).
    #[must_use]
    pub fn of_index(index: usize) -> Option<Self> {
        i16::try_from(index).ok().and_then(Self::of_data)
    }

    /// Whether it is one of the four resources (food, wood, stone, gold).
    #[must_use]
    pub const fn is_resource(self) -> bool {
        self.0 <= 3
    }

    /// The attribute a data field names, none for a negative value (−1 in the `.dat`).
    #[must_use]
    pub fn of_data(value: i16) -> Option<Self> {
        (0 <= value).then_some(Self(value))
    }

    /// The attribute of a raw field that holds −1 for none (a unit's load `+0x308`, a task's
    /// resources).
    ///
    /// # Errors
    ///
    /// A negative value other than −1, which no attribute is.
    pub fn of_field(value: i16) -> GenieResult<Option<Self>> {
        (value != -1).then(|| Self::try_from(value)).transpose()
    }

    /// A food load: food (0) or one of the foods a unit carries (15 to 17); a gatherer keeps a
    /// load of one for another.
    #[must_use]
    pub const fn is_food_load(self) -> bool {
        self.0 == 0 || (15 <= self.0 && self.0 <= 17)
    }

    /// The player's resource of a load (`FUN_140fdd4f0`'s first part): the kinds 0 to 3, 9 and
    /// 56 themselves, the foods 15 to 17 food, any other none.
    #[must_use]
    pub const fn load_resource(self) -> Option<Self> {
        match self.0 {
            0..=3 | 9 | 0x38 => Some(self),
            15..=17 => Some(Self(0)),
            _ => None,
        }
    }

    /// The value of a field that holds −1 for none.
    #[must_use]
    pub const fn field(attribute: Option<Self>) -> i16 {
        match attribute {
            Some(attribute_id) => attribute_id.0,
            None => -1,
        }
    }
}

impl genie_diff::Diff for AttributeId {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        differ.equal(self, other);
    }
}

impl TryFrom<i16> for AttributeId {
    type Error = GenieError;

    /// The attribute of value `value`.
    ///
    /// # Errors
    ///
    /// A negative value, which no attribute is.
    fn try_from(value: i16) -> GenieResult<Self> {
        Self::of_data(value)
            .ok_or_else(|| GenieError::unsupported("a negative attribute").with("attribute", value))
    }
}

impl From<AttributeId> for i16 {
    /// The attribute's id, as a slot's serde form holds it.
    fn from(id: AttributeId) -> Self {
        id.0
    }
}

#[cfg(test)]
mod tests {
    use crate::ErrorKind;
    use crate::players::attribute::AttributeId;

    const FOOD: AttributeId = AttributeId::named(0);
    const WOOD: AttributeId = AttributeId::named(1);
    const GOLD: AttributeId = AttributeId::named(3);
    const POPULATION: AttributeId = AttributeId::named(11);
    const MEAT: AttributeId = AttributeId::named(15);
    const FARM_FOOD: AttributeId = AttributeId::named(16);

    /// The resources are the four attributes that are resources, food first.
    #[test]
    fn resources_are_the_four_resource_attributes_in_order() {
        let resources = AttributeId::resources();
        assert!(
            resources
                .iter()
                .all(|attribute_id| attribute_id.is_resource())
        );
        assert_eq!(
            resources.map(|attribute_id| attribute_id.get()),
            [0, 1, 2, 3]
        );
        assert!(!AttributeId::named(4).is_resource());
    }

    /// The four resources come first, a negative value is no attribute.
    #[test]
    fn the_resources_are_the_first_four_and_negative_value_is_none() {
        assert!(FOOD.is_resource() && GOLD.is_resource());
        assert!(!POPULATION.is_resource());
        assert_eq!(AttributeId::of_data(-1), None);
        assert_eq!(AttributeId::of_data(11), Some(POPULATION));
        assert_eq!(AttributeId::field(Some(GOLD)), 3);
        assert_eq!(AttributeId::field(None), -1);
        assert!(AttributeId::try_from(-2).is_err());
    }

    /// A field's −1 is none, a value of 0 or more an attribute, another negative one an error.
    #[test]
    fn field_minus_one_is_none_and_another_negative_value_an_error() {
        assert_eq!(AttributeId::of_field(-1), Ok(None));
        assert_eq!(AttributeId::of_field(16), Ok(Some(FARM_FOOD)));
        assert!(AttributeId::of_field(-2).is_err_and(|error| error.kind() != ErrorKind::Io));
    }

    /// The food loads are food and the kinds 15 to 17; a load's resource is itself for 0 to 3,
    /// 9 and 56, food for 15 to 17, none for any other.
    #[test]
    fn load_resource_is_itself_food_or_none() {
        for number in [0, 15, 16, 17] {
            assert!(AttributeId::named(number).is_food_load(), "{number}");
        }
        for number in [1, 2, 3, 14, 18] {
            assert!(!AttributeId::named(number).is_food_load(), "{number}");
        }
        assert_eq!(WOOD.load_resource(), Some(WOOD));
        assert_eq!(
            AttributeId::named(56)
                .load_resource()
                .map(|attribute_id| attribute_id.get()),
            Some(56)
        );
        assert_eq!(MEAT.load_resource(), Some(FOOD));
        assert_eq!(AttributeId::named(17).load_resource(), Some(FOOD));
        assert_eq!(AttributeId::named(4).load_resource(), None);
    }

    #[test]
    fn an_offset_past_the_range_is_an_error_not_panic() {
        assert_eq!(FOOD.offset(3).unwrap(), GOLD);
        assert!(AttributeId::named(i16::MAX).offset(1).is_err());
        assert!(FOOD.offset(usize::MAX).is_err());
    }
}
