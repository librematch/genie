//! The damage an object deals per second ([`DamageRate`]).

/// The damage an object deals per second: its weapons' amounts over its reload
/// ([`crate::garrison::attack_rate`], [`crate::garrison::firepower`]); a building's rate is what a
/// unit inside it adds a share of ([`crate::garrison::share`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct DamageRate(f32);

impl DamageRate {
    /// No damage.
    pub const ZERO: Self = Self(0.0);

    /// A rate of `damage` per second.
    #[must_use]
    pub const fn new(damage: f32) -> Self {
        Self(damage)
    }

    /// The damage per second.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }

    /// The rate over `other` (`DIVSS`): how many of `other` it makes.
    #[must_use]
    pub fn over(self, other: Self) -> f32 {
        self.0 / other.0
    }
}

#[cfg(test)]
mod tests {
    use crate::garrison::DamageRate;

    /// A rate reads back and divides by another as the game's `DIVSS`.
    #[test]
    fn rate_divides_by_another() {
        assert_eq!(DamageRate::new(6.0).over(DamageRate::new(4.0)), 1.5);
        assert_eq!(DamageRate::ZERO.get(), 0.0);
        assert!(DamageRate::new(1.0).over(DamageRate::ZERO).is_infinite());
    }
}
