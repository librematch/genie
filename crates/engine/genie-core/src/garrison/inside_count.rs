//! How many units an object holds inside ([`InsideCount`]).

use crate::objects::masters::GarrisonCapacity;

/// How many units an object holds inside (slot 0x498, [`crate::world::World::garrisoned`]): the
/// count the game keeps as an int and holds against a master's [`GarrisonCapacity`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InsideCount(i32);

impl InsideCount {
    /// None inside.
    pub const ZERO: Self = Self(0);
    /// One unit.
    pub const ONE: Self = Self(1);

    /// A count of `count` units.
    #[must_use]
    pub const fn new(count: i32) -> Self {
        Self(count)
    }

    /// The count.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }

    /// The count and `other`'s (a unit and those it holds).
    #[must_use]
    pub const fn plus(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }

    /// The count is at most `capacity` (slot 0x198's `<=`).
    #[must_use]
    pub const fn within(self, capacity: GarrisonCapacity) -> bool {
        self.0 <= capacity.get() as i32
    }

    /// The count is below `capacity` (a shelter with room, `FUN_14161e370`'s `<`).
    #[must_use]
    pub const fn below(self, capacity: GarrisonCapacity) -> bool {
        self.0 < capacity.get() as i32
    }
}

#[cfg(test)]
mod tests {
    use crate::garrison::InsideCount;
    use crate::objects::masters::GarrisonCapacity;

    /// A count adds a unit's and holds against a capacity: within up to it, below short of it.
    #[test]
    fn count_holds_against_capacity() {
        let two = InsideCount::ONE.plus(InsideCount::ONE);
        assert_eq!(two, InsideCount::new(2));
        assert_eq!(InsideCount::ZERO.get(), 0);
        let cap = GarrisonCapacity::of_data(2);
        assert!(two.within(cap) && !two.below(cap));
        assert!(InsideCount::ONE.below(cap));
        assert!(!two.plus(InsideCount::ONE).within(cap));
    }
}
