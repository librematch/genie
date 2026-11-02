//! Whether a building goes over an object.

/// A master's `can_be_built_on` (`+0x5e9`): 0 for most masters, 1 for others (the dead
/// units' among them), 2 for one master of build 158041's `.dat`. A foundation clears an
/// object of any but 0 off its footprint; a placement passes over one of 1 only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct BuildableOn(u8);

impl BuildableOn {
    /// 0: a building cannot go over it.
    pub const NO: Self = Self(0);
    /// 1: a building goes over it.
    pub const YES: Self = Self(1);

    /// The value of the `.dat`'s byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// The byte.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// Whether a foundation clears it off its footprint: any but 0.
    #[must_use]
    pub const fn cleared(self) -> bool {
        self.0 != 0
    }

    /// Whether a placement passes over it: 1 only.
    #[must_use]
    pub const fn passed(self) -> bool {
        self.0 == 1
    }
}

impl genie_diff::Diff for BuildableOn {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::BuildableOn;

    /// A foundation clears 1 and 2, a placement passes over 1 only.
    #[test]
    fn foundation_clears_more_than_placement_passes() {
        let two = BuildableOn::of_data(2);
        assert_eq!(
            [BuildableOn::NO, BuildableOn::YES, two].map(BuildableOn::cleared),
            [false, true, true]
        );
        assert_eq!(
            [BuildableOn::NO, BuildableOn::YES, two].map(BuildableOn::passed),
            [false, true, false]
        );
        assert_eq!(two.get(), 2);
    }
}
