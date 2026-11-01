//! The statistics' values: an entry's kill time and scores, the table's counts and the order
//! history's weights.

/// The time a unit takes to kill an entry (`+8`): its hit points over the unit's damage to it,
/// over the unit's reload time; a stand-in for one it does no damage to.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct KillTime(f32);

impl KillTime {
    /// The kill time of the float `value`.
    #[must_use]
    pub const fn new(value: f32) -> Self {
        Self(value)
    }

    /// The float.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

/// An entry's score (`+0x14`..`+0x28`): its kill-time and distance scores, its category's
/// weight, the current target's bonus, their total and the rank the sort orders by.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct TargetScore(f32);

impl TargetScore {
    /// None.
    pub const ZERO: Self = Self(0.0);

    /// The score of the float `value`.
    #[must_use]
    pub const fn new(value: f32) -> Self {
        Self(value)
    }

    /// The float.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl std::ops::Add for TargetScore {
    type Output = Self;

    /// One `f32` add, `self` first.
    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl std::ops::MulAssign<f32> for TargetScore {
    /// One `f32` product.
    fn mul_assign(&mut self, f: f32) {
        self.0 *= f;
    }
}

/// A count of the table's entries (`+0x20`..`+0x5c`): the others, the moving and armed ones,
/// those within reach, and those per attack category.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct EntryCount(i32);

impl EntryCount {
    /// None.
    pub const ZERO: Self = Self(0);

    /// The count of the int `value`.
    #[must_use]
    pub const fn new(value: i32) -> Self {
        Self(value)
    }

    /// One more (the table holds at most [`crate::unit_ai::best_target::STATISTICS`]).
    pub const fn count_one(&mut self) {
        self.0 += 1;
    }

    /// The int.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

/// The order history's weight of an attack category
/// ([`crate::unit_ai::best_target::order_weights`]): each of its orders younger than a minute adds
/// the rest of the minute over 60000.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct OrderWeight(f32);

impl OrderWeight {
    /// None.
    pub const ZERO: Self = Self(0.0);

    /// The weight of the float `value`.
    #[must_use]
    pub const fn new(value: f32) -> Self {
        Self(value)
    }

    /// The float.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl std::ops::AddAssign for OrderWeight {
    /// One `f32` add.
    fn add_assign(&mut self, other: Self) {
        self.0 += other.0;
    }
}

#[cfg(test)]
mod tests {
    use crate::unit_ai::best_target::{EntryCount, KillTime, OrderWeight, TargetScore};

    /// Each operator is the one `f32` or int operation; the order is the float's.
    #[test]
    fn the_statistics_values_do_the_game_s_operations() {
        let (a, b) = (TargetScore::new(0.1), TargetScore::new(0.2));
        assert_eq!((a + b).get().to_bits(), (0.1_f32 + 0.2).to_bits());
        let mut c = a;
        c *= 3.0;
        assert_eq!(c.get().to_bits(), (0.1_f32 * 3.0).to_bits());
        assert!(TargetScore::ZERO < a);
        assert_eq!(TargetScore::new(f32::NAN).partial_cmp(&a), None);
        assert!(KillTime::new(1.0) < KillTime::new(f32::MAX));
        assert_eq!(KillTime::default().get(), 0.0);
        let mut n = EntryCount::ZERO;
        n.count_one();
        n.count_one();
        assert_eq!(n, EntryCount::new(2));
        let mut w = OrderWeight::new(0.1);
        w += OrderWeight::new(0.2);
        assert_eq!(w.get().to_bits(), (0.1_f32 + 0.2).to_bits());
    }
}
