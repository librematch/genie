//! The builders or repairers on an object.

/// The builders on a foundation or the repairers on a combat object (`num_builders`, `+0x9e8`, a
/// byte). A builder's join adds one and its leave takes one, wrapping as the game's byte.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BuilderCount(u8);

impl BuilderCount {
    /// None.
    pub const NONE: Self = Self(0);

    /// The count of the save's byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// The byte.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// The count changed by `by` (a join's 1, a leave's −1), wrapping as the game's byte.
    #[must_use]
    pub const fn changed_by(self, by: i8) -> Self {
        Self(self.0.wrapping_add_signed(by))
    }

    /// The count as the work's share reads it (`CVTSI2SS` of the byte).
    #[must_use]
    pub fn as_f32(self) -> f32 {
        f32::from(self.0)
    }

    /// One builder less (wrapping).
    #[must_use]
    pub const fn left(self) -> Self {
        Self(self.0.wrapping_sub(1))
    }
}

impl genie_diff::Diff for BuilderCount {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::construction::BuilderCount;

    /// A join adds one and a leave takes one, wrapping as a byte.
    #[test]
    fn builders_join_and_leave_wrapping() {
        assert_eq!(BuilderCount::NONE.changed_by(1).get(), 1);
        assert_eq!(BuilderCount::NONE.changed_by(-1).get(), 255);
        assert_eq!(BuilderCount::NONE.left().get(), 255);
        assert_eq!(BuilderCount::of_data(255).changed_by(1), BuilderCount::NONE);
        assert_eq!(
            BuilderCount::of_data(3).as_f32().to_bits(),
            3.0f32.to_bits()
        );
    }
}
