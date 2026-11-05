//! An object's melee and pierce armor powerups ([`ArmorPowerups`]).

use crate::objects::masters::DamageAmount;

/// An object's melee and pierce armor powerups (`mMeleeArmorPowerup`, `mPierceArmorPowerup`,
/// `+0xa48`, `+0xa4c`): 32-bit integers that an aura's powerup adds to (`FUN_141200dd0`, `ADD
/// dword ptr`) and the armor reads as their low 16 bits (slots 0x708 and 0x710).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, genie_diff::Diff)]
pub struct ArmorPowerups {
    /// The melee armor's (`+0xa48`).
    pub(crate) melee: i32,
    /// The pierce armor's (`+0xa4c`).
    pub(crate) pierce: i32,
}

impl ArmorPowerups {
    /// None of either.
    pub const ZERO: Self = Self::new(0, 0);

    /// The integers `melee` and `pierce`, as the save holds them.
    #[must_use]
    pub const fn new(melee: i32, pierce: i32) -> Self {
        Self { melee, pierce }
    }

    /// The melee armor's powerup as the armor reads it (its low 16 bits).
    #[must_use]
    pub const fn melee_armor(self) -> DamageAmount {
        DamageAmount::new(self.melee as i16)
    }

    /// The pierce armor's powerup as the armor reads it (its low 16 bits).
    #[must_use]
    pub const fn pierce_armor(self) -> DamageAmount {
        DamageAmount::new(self.pierce as i16)
    }
}

#[cfg(test)]
mod tests {
    use crate::combat::damage::ArmorPowerups;
    use crate::objects::masters::DamageAmount;

    /// The armor reads each powerup's low 16 bits; the integers keep the rest.
    #[test]
    fn armor_reads_the_low_sixteen_bits_of_each_powerup() {
        let powerups = ArmorPowerups::new(0x1_0002, -3);
        assert_eq!(
            (powerups.melee_armor(), powerups.pierce_armor()),
            (DamageAmount::new(2), DamageAmount::new(-3))
        );
        assert_eq!(powerups.melee, 0x1_0002);
        assert_eq!(ArmorPowerups::ZERO, ArmorPowerups::default());
    }
}
