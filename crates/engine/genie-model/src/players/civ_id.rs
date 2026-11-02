//! A civilisation's number.

use crate::GenieError;
use std::fmt::{Display, Error, Formatter};

/// A civilisation (`player+0x738`, the save's `civ_id`; the data's civ lists): its number, 0
/// Gaia's, the place of its row in the civilisations' tables.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CivId(u8);

impl CivId {
    /// Gaia's (0).
    pub const GAIA: Self = Self(0);

    /// The civilisation of number `value`.
    #[must_use]
    pub const fn new(value: u8) -> Self {
        Self(value)
    }

    /// A technology's civilisation (its `.dat` record's `civ`): below 1 every civilisation's
    /// (none), else that one.
    ///
    /// # Errors
    ///
    /// A number past the byte the players hold theirs in.
    pub fn of_tech(value: i16) -> Result<Option<Self>, GenieError> {
        if value < 1 {
            return Ok(None);
        }
        u8::try_from(value).map(|v| Some(Self(v))).map_err(|_| {
            GenieError::invalid_input("a technology's civilisation past 255").with("civ", value)
        })
    }

    /// Its number.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// Its place in the civilisations' tables.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl Display for CivId {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{}", self.0)
    }
}

impl genie_diff::Diff for CivId {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::players::CivId;

    /// A technology's civilisation below 1 is every one's; past a byte is an error.
    #[test]
    fn technology_civ_below_one_is_every_civ_s() {
        assert_eq!(CivId::of_tech(0).unwrap(), None);
        assert_eq!(CivId::of_tech(-1).unwrap(), None);
        assert_eq!(CivId::of_tech(12).unwrap(), Some(CivId::new(12)));
        assert!(CivId::of_tech(256).is_err());
        assert_eq!((CivId::new(12).get(), CivId::new(12).index()), (12, 12));
    }
}
