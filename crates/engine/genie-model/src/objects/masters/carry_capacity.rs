//! A master's carry capacity (`+0x61c`): how much an object of it carries.

/// A master's carry capacity (`attribute_max_amount`, `+0x61c`): the whole amount an object of it
/// carries, a short the effects set, add to and scale (capped where the game caps them). Every
/// short is kept; `Amount::of_master` is the game's conversion.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CarryCapacity(i16);

impl CarryCapacity {
    /// The capacity of the data's short `value`.
    #[must_use]
    pub const fn of_data(value: i16) -> Self {
        Self(value)
    }

    /// The short, as the data and the save hold it.
    #[must_use]
    pub const fn get(self) -> i16 {
        self.0
    }
}

impl genie_diff::Diff for CarryCapacity {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}
