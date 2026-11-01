//! A unit's place among a queue's units.

/// The place of a unit among a production queue's units, counting each entry's units in turn
/// (a `UNIT_ORDER` cancel's parameter): 0 is the first entry's first unit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnitIndex(u32);

impl UnitIndex {
    /// The first unit.
    pub const FIRST: Self = Self(0);

    /// The place the packet's parameter `value` names.
    #[must_use]
    pub const fn of_data(value: u32) -> Self {
        Self(value)
    }

    /// The parameter.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// The place as the game's signed int (`FUN_1411d61e0`'s start).
    #[must_use]
    pub const fn signed(self) -> i32 {
        self.0 as i32
    }
}

#[cfg(test)]
mod tests {
    use crate::production::UnitIndex;

    /// The place reads as the game's signed int.
    #[test]
    fn the_place_reads_signed() {
        assert_eq!(UnitIndex::of_data(u32::MAX).signed(), -1);
        assert_eq!(UnitIndex::FIRST.signed(), 0);
    }
}
