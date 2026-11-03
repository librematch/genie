//! A moving master's trail: the master it leaves behind, its mode and its density.

use crate::objects::masters::MasterField;
use genie_diff::Diff;
use serde::Deserialize;
use serde::Serialize;
use std::ops::{AddAssign, MulAssign};

/// A moving master's trail (`obj_trail_id`, `obj_trail_options`, `obj_trail_spacing`; the
/// `.dat`'s tracking unit, its mode and density).
#[derive(Clone, Copy, Debug, Default, PartialEq, Diff, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Trail {
    /// `+0x75e`: the master it leaves behind.
    pub unit: MasterField,
    /// `+0x760`.
    pub mode: TrailMode,
    /// `+0x764`.
    pub density: TrailDensity,
}

/// A trail's mode (`obj_trail_options`, `+0x760`, a byte), which no modelled routine reads.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord, Hash,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TrailMode(u8);

impl TrailMode {
    /// The mode of the data's byte `value`: every byte is one.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// The byte.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// The mode an add effect of `n` makes of it: the byte's sum, wrapping.
    #[must_use]
    pub const fn plus(self, n: u8) -> Self {
        Self(self.0.wrapping_add(n))
    }
}

/// A trail's density (`obj_trail_spacing`, `+0x764`, a float), which no modelled routine reads.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TrailDensity(f32);

impl TrailDensity {
    /// The density of the data's float `value`.
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

impl AddAssign for TrailDensity {
    /// An add effect's sum (one `f32` add).
    fn add_assign(&mut self, other: Self) {
        self.0 += other.0;
    }
}

impl MulAssign<f32> for TrailDensity {
    /// A multiply effect's product (one `f32` product).
    fn mul_assign(&mut self, f: f32) {
        self.0 *= f;
    }
}

impl Diff for TrailMode {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

impl Diff for TrailDensity {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::{TrailDensity, TrailMode};

    /// The effects add and multiply as the game: the mode's byte wraps, the density is one
    /// `f32` operation each.
    #[test]
    fn the_trail_effects_add_and_multiply() {
        assert_eq!(TrailMode::of_data(255).plus(2), TrailMode::of_data(1));
        let mut d = TrailDensity::new(0.1);
        d += TrailDensity::new(0.2);
        assert_eq!(d.get().to_bits(), (0.1_f32 + 0.2).to_bits());
        d *= 3.0;
        assert_eq!(d.get().to_bits(), ((0.1_f32 + 0.2) * 3.0).to_bits());
    }
}
