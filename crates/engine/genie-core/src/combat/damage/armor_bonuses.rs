//! What an object's powerups or stingers add to its melee and pierce armor ([`ArmorBonuses`]).

use crate::objects::masters::DamageAmount;

/// What an object's powerups or stingers add to its melee and pierce armor: 32-bit integers that
/// the armor reads as their low 16 bits. The powerups (`mMeleeArmorPowerup`,
/// `mPierceArmorPowerup`, `+0xa48`, `+0xa4c`, slots 0x708 and 0x710) take an aura's adds
/// (`FUN_141200dd0`, `ADD dword ptr`); the stingers (`+0xb4c`, `+0xb50`, slots 0x770 and 0x778)
/// take the stinger update's (`FUN_1412092e0`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, genie_diff::Diff)]
pub struct ArmorBonuses {
    /// The melee armor's (`+0xa48`, `+0xb4c`).
    pub(crate) melee: i32,
    /// The pierce armor's (`+0xa4c`, `+0xb50`).
    pub(crate) pierce: i32,
}

impl ArmorBonuses {
    /// None of either.
    pub const ZERO: Self = Self::new(0, 0);

    /// The integers `melee` and `pierce`, as the save holds them.
    #[must_use]
    pub const fn new(melee: i32, pierce: i32) -> Self {
        Self { melee, pierce }
    }

    /// What it adds to the melee armor (its low 16 bits).
    #[must_use]
    pub const fn melee_armor(self) -> DamageAmount {
        DamageAmount::new(self.melee as i16)
    }

    /// What it adds to the pierce armor (its low 16 bits).
    #[must_use]
    pub const fn pierce_armor(self) -> DamageAmount {
        DamageAmount::new(self.pierce as i16)
    }
}

#[cfg(test)]
mod tests {
    use crate::combat::damage::ArmorBonuses;
    use crate::objects::masters::DamageAmount;

    /// The armor reads each integer's low 16 bits; the integers keep the rest.
    #[test]
    fn armor_reads_the_low_sixteen_bits_of_each_powerup() {
        let bonuses = ArmorBonuses::new(0x1_0002, -3);
        assert_eq!(
            (bonuses.melee_armor(), bonuses.pierce_armor()),
            (DamageAmount::new(2), DamageAmount::new(-3))
        );
        assert_eq!(bonuses.melee, 0x1_0002);
        assert_eq!(ArmorBonuses::ZERO, ArmorBonuses::default());
    }
}
