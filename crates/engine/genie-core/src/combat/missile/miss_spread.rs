//! How far a missed missile strays.

/// How far a missed missile strays per tile of the way (a master's `missed_missile_spread`,
/// `+0x860`, the attribute "Attack Dispersion"), which a miss multiplies its gap by.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct MissSpread(f32);

impl MissSpread {
    /// None.
    pub const ZERO: Self = Self(0.0);

    /// The spread of the data's float `value`.
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

impl std::ops::AddAssign for MissSpread {
    /// An add effect's sum (one `f32` add).
    fn add_assign(&mut self, other: Self) {
        self.0 += other.0;
    }
}

impl std::ops::MulAssign<f32> for MissSpread {
    /// A multiply effect's product (one `f32` product).
    fn mul_assign(&mut self, f: f32) {
        self.0 *= f;
    }
}

impl genie_diff::Diff for MissSpread {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::combat::missile::MissSpread;

    /// The effects add and multiply as one `f32` operation each.
    #[test]
    fn the_effects_add_and_multiply_the_spread() {
        let mut s = MissSpread::new(0.1);
        s += MissSpread::new(0.2);
        assert_eq!(s.get().to_bits(), (0.1_f32 + 0.2).to_bits());
        s *= 3.0;
        assert_eq!(s.get().to_bits(), ((0.1_f32 + 0.2) * 3.0).to_bits());
        assert_eq!(MissSpread::default(), MissSpread::ZERO);
    }
}
