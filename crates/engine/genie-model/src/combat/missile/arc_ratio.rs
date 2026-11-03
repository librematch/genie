//! A missile master's arc.

use serde::Deserialize;
use serde::Serialize;
use std::ops::{AddAssign, MulAssign};

/// A missile master's arc (the `.dat`'s `projectile_arc`, `+0x888`, the save's
/// `ballistics_ratio`): 0 flies flat; else the launch makes the arc's rise of it and the way
/// (`−arc × way × way` for one at most 0, `−arc × way` above).
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ArcRatio(f32);

impl ArcRatio {
    /// A flat flight.
    pub const FLAT: Self = Self(0.0);

    /// The arc of the data's float `value`.
    #[must_use]
    pub const fn new(value: f32) -> Self {
        Self(value)
    }

    /// The float.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }

    /// Whether the missile flies flat (a NaN arc does not).
    #[must_use]
    pub fn is_flat(self) -> bool {
        self.0 == 0.0
    }
}

impl AddAssign for ArcRatio {
    /// An add effect's sum (one `f32` add).
    fn add_assign(&mut self, other: Self) {
        self.0 += other.0;
    }
}

impl MulAssign<f32> for ArcRatio {
    /// A multiply effect's product (one `f32` product).
    fn mul_assign(&mut self, f: f32) {
        self.0 *= f;
    }
}

impl genie_diff::Diff for ArcRatio {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::combat::missile::ArcRatio;

    /// Only 0 (either sign) is flat; a NaN arc is not.
    #[test]
    fn only_zero_is_flat() {
        assert!(ArcRatio::FLAT.is_flat());
        assert!(ArcRatio::new(-0.0).is_flat());
        assert!(!ArcRatio::new(0.5).is_flat());
        assert!(!ArcRatio::new(f32::NAN).is_flat());
        assert_eq!(ArcRatio::new(-0.25).get(), -0.25);
        let mut a = ArcRatio::new(0.1);
        a += ArcRatio::new(0.2);
        assert_eq!(a.get().to_bits(), (0.1_f32 + 0.2).to_bits());
        a *= 3.0;
        assert_eq!(a.get().to_bits(), ((0.1_f32 + 0.2) * 3.0).to_bits());
    }
}
