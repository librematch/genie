//! A player's offensive posture.

use crate::unit_ai::UnitStance;

/// A player's offensive posture (`+0x71c`, an int GAME 0x14 flips between 0 and 1): its low
/// byte is the stance a new soldier or scorpion of the player takes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Posture(i32);

impl Posture {
    /// 0, a load's: new soldiers are aggressive.
    pub const AGGRESSIVE: Self = Self(0);
    /// 1: new soldiers are defensive.
    pub const DEFENSIVE: Self = Self(1);

    /// The posture of the int `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// The int.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }

    /// GAME 0x14's flip (slot 0xa0): 0 becomes 1, any other value 0.
    #[must_use]
    pub const fn flipped(self) -> Self {
        Self((self.0 == 0) as i32)
    }

    /// The stance of its low byte (slot 0x90).
    #[must_use]
    pub const fn stance(self) -> UnitStance {
        UnitStance::of_byte(self.0 as u8)
    }
}

impl genie_diff::Diff for Posture {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::players::Posture;
    use crate::unit_ai::UnitStance;

    /// The flip makes 0 of anything but 0, and the stance is the low byte.
    #[test]
    fn the_flip_and_the_stance() {
        assert_eq!(Posture::AGGRESSIVE.flipped(), Posture::DEFENSIVE);
        assert_eq!(Posture::DEFENSIVE.flipped(), Posture::AGGRESSIVE);
        assert_eq!(Posture::of_data(7).flipped(), Posture::AGGRESSIVE);
        assert_eq!(Posture::of_data(0x101).stance(), UnitStance::DEFENSIVE);
        assert_eq!(Posture::AGGRESSIVE.stance(), UnitStance::AGGRESSIVE);
    }
}
