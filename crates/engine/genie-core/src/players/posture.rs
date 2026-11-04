//! A player's offensive posture.

use crate::unit_ai::UnitStance;

/// A player's offensive posture (`+0x71c`, an int GAME 0x14 flips between 0 and 1): its low
/// byte is the stance a new soldier or scorpion of the player takes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Posture {
    /// 0, a load's: new soldiers are aggressive.
    #[default]
    Aggressive,
    /// 1: new soldiers are defensive.
    Defensive,
    /// A value the game names nowhere, as the save or the game holds it.
    Other(Unnamed),
}

/// A raw value of [`Posture`] that names none of its variants: only the set's own
/// constructors make one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Unnamed(i32);

impl Posture {
    /// The value of raw `value`: its variant, else `Other`.
    const fn of_raw(value: i32) -> Self {
        match value {
            0 => Self::Aggressive,
            1 => Self::Defensive,
            other => Self::Other(Unnamed(other)),
        }
    }

    /// Its raw value.
    const fn raw(self) -> i32 {
        match self {
            Self::Aggressive => 0,
            Self::Defensive => 1,
            Self::Other(Unnamed(value)) => value,
        }
    }

    /// The posture of the int `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self::of_raw(value)
    }

    /// The int.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.raw()
    }

    /// GAME 0x14's flip (slot 0xa0): 0 becomes 1, any other value 0.
    #[must_use]
    pub const fn flipped(self) -> Self {
        Self::of_raw((self.raw() == 0) as i32)
    }

    /// The stance of its low byte (slot 0x90).
    #[must_use]
    pub const fn stance(self) -> UnitStance {
        UnitStance::of_byte(self.raw() as u8)
    }
}

impl genie_diff::Diff for Posture {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.raw().diff(&other.raw(), differ);
    }
}

#[cfg(test)]
mod tests {
    use crate::players::Posture;
    use crate::unit_ai::UnitStance;

    /// The flip makes 0 of anything but 0, and the stance is the low byte.
    #[test]
    fn the_flip_and_the_stance() {
        assert_eq!(Posture::Aggressive.flipped(), Posture::Defensive);
        assert_eq!(Posture::Defensive.flipped(), Posture::Aggressive);
        assert_eq!(Posture::of_data(7).flipped(), Posture::Aggressive);
        assert_eq!(Posture::of_data(0x101).stance(), UnitStance::Defensive);
        assert_eq!(Posture::Aggressive.stance(), UnitStance::Aggressive);
    }

    /// Each named value and one the set names nowhere go to their variant and back.
    #[test]
    fn every_posture_value_maps_to_its_variant_and_back() {
        for value in [0, 1, 2] {
            assert_eq!(Posture::of_raw(value).raw(), value);
        }
        assert!(matches!(Posture::of_raw(2), Posture::Other(_)));
    }
}
