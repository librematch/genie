//! An attack's or an armor's amount.

use crate::math::conversions::cvttss2si;

/// An attack's or an armor's amount: the `.dat`'s `amount` of an attack or armor entry, the base
/// armor (`+0x800`), the shown attack and armors, and an object's armor powerups and the armor
/// its charge's hits took off it. A signed short, which the armor slots add and subtract
/// wrapping and the damage reads as a float.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct DamageAmount(i16);

impl DamageAmount {
    /// None.
    pub const ZERO: Self = Self(0);
    /// The least (−32768), where the best-armor folds start (`FUN_141085450`).
    pub const LEAST: Self = Self(i16::MIN);
    /// −1, where the pierce armor's fold starts: no armor of the class (`FUN_1410857f0`).
    pub const NO_CLASS: Self = Self(-1);

    /// The amount of the short `value`, as the `.dat` and the save hold it.
    #[must_use]
    pub const fn new(value: i16) -> Self {
        Self(value)
    }

    /// The short.
    #[must_use]
    pub const fn get(self) -> i16 {
        self.0
    }

    /// The sum, wrapping as the game's 16-bit add.
    #[must_use]
    pub const fn plus(self, other: Self) -> Self {
        Self(self.0.wrapping_add(other.0))
    }

    /// The difference, wrapping as the game's 16-bit subtract.
    #[must_use]
    pub const fn minus(self, other: Self) -> Self {
        Self(self.0.wrapping_sub(other.0))
    }

    /// The amount an effect's float `v` sets: `CVTTSS2SI`, then its low 16 bits.
    #[must_use]
    pub fn of_effect(v: f32) -> Self {
        Self(cvttss2si(v) as i16)
    }

    /// The product with the short `factor`, wrapping as the game's 16-bit multiply.
    #[must_use]
    pub const fn times(self, factor: i16) -> Self {
        Self(self.0.wrapping_mul(factor))
    }

    /// The float the damage reads (exact for every short).
    #[must_use]
    pub fn as_f32(self) -> f32 {
        f32::from(self.0)
    }
}

impl genie_diff::Diff for DamageAmount {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::DamageAmount;

    /// The sums wrap as 16 bits; the float is exact; the order is the short's.
    #[test]
    fn damage_amounts_wrap_as_shorts() {
        let (a, b) = (DamageAmount::new(3), DamageAmount::new(-4));
        assert_eq!(a.plus(b), DamageAmount::new(-1));
        assert_eq!(a.minus(b), DamageAmount::new(7));
        assert_eq!(
            DamageAmount::new(i16::MAX).plus(DamageAmount::new(1)),
            DamageAmount::LEAST
        );
        assert_eq!(
            DamageAmount::LEAST.minus(DamageAmount::new(1)).get(),
            i16::MAX
        );
        assert_eq!(b.as_f32(), -4.0);
        assert_eq!(a.times(-4), DamageAmount::new(-12));
        assert_eq!(DamageAmount::new(300).times(300).get(), 24_464);
        assert_eq!(DamageAmount::of_effect(-2.9), DamageAmount::new(-2));
        assert_eq!(DamageAmount::of_effect(65_541.0), DamageAmount::new(5));
        assert!(DamageAmount::LEAST < DamageAmount::NO_CLASS);
        assert!(DamageAmount::NO_CLASS < DamageAmount::ZERO);
        assert_eq!(DamageAmount::default(), DamageAmount::ZERO);
    }
}
