//! A terrain restriction's id: which terrains an object may stand on.

use crate::GenieError;
use serde::Deserialize;
use serde::Serialize;
use std::fmt::{Display, Error, Formatter};

/// A terrain restriction (the `.dat`'s table; a master's `+0x606`, the save's
/// `terrain_restriction_mask_id`): its place in the restrictions' tables, each the terrains'
/// values an object of it reads (`genie_core::map::terrain::Terrain::restriction_mask`, the
/// pathing's planners). A field holds −1 for none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RestrictionId(u16);

impl RestrictionId {
    /// The field value of none.
    const NONE: i16 = -1;

    /// The restriction at place `index` of the table, for the tests and the data's places.
    #[must_use]
    pub const fn named(index: u16) -> Self {
        Self(index)
    }

    /// The restriction of a field that holds −1 for none.
    ///
    /// # Errors
    ///
    /// A negative value other than −1, which no restriction's place is.
    pub fn of_field(value: i16) -> Result<Option<Self>, GenieError> {
        match value {
            Self::NONE => Ok(None),
            v => u16::try_from(v).map(|v| Some(Self(v))).map_err(|_| {
                GenieError::invalid_input("a terrain restriction below −1").with("value", v)
            }),
        }
    }

    /// The restriction a value an effect sets names (`FUN_14160d6d0`'s low 16 bits), none for a
    /// negative one: genie reads every negative restriction as none, as it reads −1.
    #[must_use]
    pub fn of_data(value: i16) -> Option<Self> {
        u16::try_from(value).ok().map(Self)
    }

    /// The value of a field that holds −1 for none.
    #[must_use]
    pub fn field(restriction: Option<Self>) -> i16 {
        restriction.map_or(Self::NONE, |r| i16::try_from(r.0).unwrap_or(i16::MAX))
    }

    /// Its place in the restrictions' tables.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl Display for RestrictionId {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{}", self.0)
    }
}

impl genie_diff::Diff for RestrictionId {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::map::RestrictionId;

    /// −1 is none, 0 and up a restriction, any other negative value an error; the field writes
    /// them back.
    #[test]
    fn field_minus_one_is_none() {
        assert_eq!(RestrictionId::of_field(-1).unwrap(), None);
        let r = RestrictionId::of_field(7).unwrap();
        assert_eq!(r, Some(RestrictionId::named(7)));
        assert_eq!(r.map(RestrictionId::index), Some(7));
        assert!(RestrictionId::of_field(-2).is_err());
        assert_eq!(
            [-2, 3].map(RestrictionId::of_data),
            [None, Some(RestrictionId::named(3))]
        );
        assert_eq!([r, None].map(RestrictionId::field), [7, -1]);
    }
}
