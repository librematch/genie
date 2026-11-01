//! How many times a payment or a cost test takes a master's costs.

/// The multiplier of a master's costs a cost test and a payment take (`FUN_141618db0`'s
/// count, a float): a whole count of units, farms or fish traps, or a repair's share of the
/// costs.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct CostCount(f32);

impl CostCount {
    /// One master's costs.
    pub const ONE: Self = Self(1.0);

    /// The multiplier `value`.
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

#[cfg(test)]
mod tests {
    use crate::production::CostCount;

    /// The multiplier reads back; one is 1.0.
    #[test]
    fn the_multiplier_reads_back() {
        assert_eq!(CostCount::new(0.25).get(), 0.25);
        assert_eq!(CostCount::ONE, CostCount::new(1.0));
    }
}
