//! An object's attack powerup.

/// An object's attack powerup (`+0xa24`, a float, slot 0x6d8): what its first melee, pierce or
/// class-31 weapon adds to its attack, and what its shown attack adds as a whole number.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct AttackPowerup(f32);

impl AttackPowerup {
    /// None.
    pub const ZERO: Self = Self(0.0);

    /// The powerup of the save's float `value`.
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

impl genie_diff::Diff for AttackPowerup {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

#[cfg(test)]
mod tests {
    use crate::combat::AttackPowerup;

    /// The powerup keeps the save's float.
    #[test]
    fn the_powerup_keeps_its_float() {
        assert_eq!(AttackPowerup::new(2.5).get(), 2.5);
        assert_eq!(AttackPowerup::default(), AttackPowerup::ZERO);
    }
}
