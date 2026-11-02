//! A master's garrison capacity (`+0x5d4`): how many units an object of it holds.

/// A master's garrison capacity (`obj_max`, `+0x5d4`, the `.dat`'s `garrison_capacity`): how many
/// units an object of it holds; a building of any takes a gather point (`FUN_140e7e9a0`). Every
/// byte the data or an effect leaves is kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GarrisonCapacity(u8);

impl GarrisonCapacity {
    /// None: an object that holds no unit.
    pub const NONE: Self = Self(0);

    /// The capacity of the data's byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// Whether an object of it holds units (any but 0).
    #[must_use]
    pub const fn holds(self) -> bool {
        self.0 != 0
    }

    /// The units, as a count.
    #[must_use]
    pub const fn units(self) -> usize {
        self.0 as usize
    }

    /// The byte plus `amount` (an effect's add, wrapping as the game's byte add).
    #[must_use]
    pub const fn plus(self, amount: u8) -> Self {
        Self(self.0.wrapping_add(amount))
    }

    /// The byte, as the data and the save hold it.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl genie_diff::Diff for GarrisonCapacity {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::GarrisonCapacity;

    /// Only 0 holds none; the count is the byte; an add wraps.
    #[test]
    fn capacity_is_count_of_units() {
        assert!(!GarrisonCapacity::NONE.holds());
        assert!(GarrisonCapacity::of_data(5).holds());
        assert_eq!(GarrisonCapacity::of_data(20).units(), 20);
        assert_eq!(GarrisonCapacity::of_data(255).plus(2).get(), 1);
        assert!(GarrisonCapacity::NONE < GarrisonCapacity::of_data(1));
    }
}
