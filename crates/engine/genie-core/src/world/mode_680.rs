//! The game object's tagged value at `+0x680`.

/// The game object's tagged value at `+0x680` (`DAT_143f07a58`, read through `FUN_140ec0cd0`):
/// at 1 a boar does not wander, a hit may do no damage and a terrain's smoothing differs; the
/// full tech tree's fallback civilisation is 18 at 1 and 59 at 2. Its writer is not located and
/// the save does not hold it; genie-oracle leaves it at 0.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Mode680(i32);

impl Mode680 {
    /// The value genie-oracle leaves (0).
    pub const ZERO: Self = Self(0);
    /// The value whose rules the simulation tests (1).
    pub const ONE: Self = Self(1);

    /// The value of the int `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// The int.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }

    /// Whether it is 1, the value the simulation's rules test.
    #[must_use]
    pub const fn is_one(self) -> bool {
        self.0 == 1
    }
}

impl genie_diff::Diff for Mode680 {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

#[cfg(test)]
mod tests {
    use crate::world::Mode680;

    /// Only 1 is one.
    #[test]
    fn only_one_is_one() {
        assert!(Mode680::ONE.is_one());
        assert!(!Mode680::ZERO.is_one() && !Mode680::of_data(2).is_one());
        assert_eq!(Mode680::default(), Mode680::ZERO);
    }
}
