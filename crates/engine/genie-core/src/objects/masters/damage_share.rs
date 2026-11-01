//! A master's share of a damage: resisted, reflected, dealt to its own or by its blast.

/// A share of a damage (a float the `.dat` holds and the effects set, add to and scale): the
/// bonus damage a master resists (`bonus_damage_resistance`, `+0x824`), the damage it reflects
/// on its attacker (`damage_reflection`, `+0x864`), its own side takes of its blasts
/// (`friendly_fire_damage`, `+0x868`) and its blast deals (`area_damage`, `+0x834`). Each
/// operator is the one `f32` operation the game does; every value is kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct DamageShare(f32);

impl DamageShare {
    /// No share.
    pub const ZERO: Self = Self(0.0);

    /// The share of the data's float `value`.
    #[must_use]
    pub const fn new(value: f32) -> Self {
        Self(value)
    }

    /// The float, as the data and the save hold it.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl std::ops::AddAssign for DamageShare {
    /// An effect's add (`f32`).
    fn add_assign(&mut self, other: Self) {
        self.0 += other.0;
    }
}

impl std::ops::MulAssign<f32> for DamageShare {
    /// An effect's multiply (`f32`).
    fn mul_assign(&mut self, factor: f32) {
        self.0 *= factor;
    }
}

impl genie_diff::Diff for DamageShare {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::DamageShare;

    /// A share adds and scales as `f32`.
    #[test]
    fn a_damage_share_adds_and_scales_as_floats() {
        let mut s = DamageShare::new(0.25);
        s += DamageShare::new(0.5);
        assert_eq!(s, DamageShare::new(0.75));
        s *= 2.0;
        assert_eq!(s.get(), 1.5);
        assert!(DamageShare::ZERO < s);
    }
}
