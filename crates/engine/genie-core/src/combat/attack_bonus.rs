//! What an object's powerup or stingers add to its attack ([`AttackBonus`]).

/// What an object's powerup or stingers add to its attack, a float: what its first melee, pierce
/// or class-31 weapon adds to its attack, and what its shown attack adds as a whole number. The
/// powerup (`mAttackPowerup`, `+0xa24`, slot 0x6d8) takes an aura's adds; the stinger (`+0xb48`,
/// slot 0x768) the stinger update's (`FUN_1412092e0`).
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct AttackBonus(f32);

impl AttackBonus {
    /// None.
    pub const ZERO: Self = Self(0.0);

    /// The bonus of the float `value`.
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

impl genie_diff::Diff for AttackBonus {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

#[cfg(test)]
mod tests {
    use crate::combat::AttackBonus;

    /// The bonus keeps its float.
    #[test]
    fn the_bonus_keeps_its_float() {
        assert_eq!(AttackBonus::new(2.5).get(), 2.5);
        assert_eq!(AttackBonus::default(), AttackBonus::ZERO);
    }
}
