//! A speed on the map in tiles per second ([`Speed`]), with the game's `f32` operations.

use crate::map::Distance;
use crate::world::Seconds;
use derive_more::{Add, AddAssign, Display, Neg, Sub};
use serde::Deserialize;
use serde::Serialize;
use std::ops::{Div, Mul, MulAssign};

/// A speed in tiles per second as the game's `f32` holds it: a master's speed, a unit's top
/// speed and forward speed, a group's most forward speed, a missile's speed, the speed at which a
/// turn radius holds. Each operator is the one `f32` operation the game does, in its order; a
/// speed over a span of seconds is a [`Distance`]. A comparison is `f32`'s (a NaN compares
/// false).
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    PartialOrd,
    Serialize,
    Deserialize,
    Display,
    Add,
    Sub,
    Neg,
    AddAssign,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Speed(f32);

impl Speed {
    /// Standing still.
    pub const ZERO: Self = Self(0.0);
    /// The fastest, the game's `FLT_MAX`: no speed limit (a turn radius that never grows).
    pub const MAX: Self = Self(f32::MAX);

    /// A speed of `tiles` tiles per second.
    #[must_use]
    pub const fn new(tiles: f32) -> Self {
        Self(tiles)
    }

    /// The tiles per second, as the save, the `.dat` and the game's arithmetic hold them.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl Mul<f32> for Speed {
    type Output = Self;

    /// The speed scaled by `f` (one `f32` product, the speed first).
    fn mul(self, f: f32) -> Self {
        Self(self.0 * f)
    }
}

impl Mul<Speed> for f32 {
    type Output = Speed;

    /// The speed scaled by `self` (one `f32` product, the factor first).
    fn mul(self, speed: Speed) -> Speed {
        Speed(self * speed.0)
    }
}

impl MulAssign<f32> for Speed {
    /// The speed scaled by `f` in place (one `f32` product: a master's speed times a
    /// technology's factor).
    fn mul_assign(&mut self, f: f32) {
        self.0 *= f;
    }
}

impl Div<f32> for Speed {
    type Output = Self;

    /// The speed divided by `f` (one `f32` quotient).
    fn div(self, f: f32) -> Self {
        Self(self.0 / f)
    }
}

impl Div for Speed {
    type Output = f32;

    /// The ratio of two speeds (one `f32` quotient).
    fn div(self, other: Self) -> f32 {
        self.0 / other.0
    }
}

impl Mul<Seconds> for Speed {
    type Output = Distance;

    /// The way the speed covers in `dt` (one `f32` product, the speed first).
    fn mul(self, dt: Seconds) -> Distance {
        Distance::new(self.0 * dt.get())
    }
}

impl Mul<Speed> for Seconds {
    type Output = Distance;

    /// The way `speed` covers in the span (one `f32` product, the span first).
    fn mul(self, speed: Speed) -> Distance {
        Distance::new(self.get() * speed.0)
    }
}

impl genie_diff::Diff for Speed {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

#[cfg(test)]
mod tests {
    use crate::map::Distance;
    use crate::movement::Speed;
    use crate::world::Seconds;

    /// Each operator is the one `f32` operation, bit for bit and in its operand order.
    #[test]
    fn speeds_do_the_game_f32_operations() {
        let (a, b) = (Speed::new(0.1), Speed::new(0.2));
        assert_eq!((a + b).get().to_bits(), (0.1_f32 + 0.2).to_bits());
        assert_eq!((a - b).get().to_bits(), (0.1_f32 - 0.2).to_bits());
        assert_eq!((-a).get().to_bits(), (-0.1_f32).to_bits());
        assert_eq!((a * 3.0).get().to_bits(), (0.1_f32 * 3.0).to_bits());
        assert_eq!((3.0 * a).get().to_bits(), (3.0_f32 * 0.1).to_bits());
        assert_eq!((a / 3.0).get().to_bits(), (0.1_f32 / 3.0).to_bits());
        assert_eq!((b / a).to_bits(), (0.2_f32 / 0.1).to_bits());
        let mut speed = Speed::new(1.1);
        speed *= 1.15;
        assert_eq!(speed.get().to_bits(), (1.1_f32 * 1.15).to_bits());
        assert!(Speed::ZERO < a && a <= a && a < Speed::MAX);
        assert_eq!(Speed::new(f32::NAN).partial_cmp(&Speed::ZERO), None);
        assert_eq!(Speed::new(0.5).to_string(), "0.5");
    }

    /// A speed over a span is the way it covers, one `f32` product in either order; a NaN
    /// operand first keeps its payload (`mulss` returns its first NaN operand).
    #[test]
    fn speed_over_seconds_is_distance() {
        let (speed, dt) = (Speed::new(1.2), Seconds::new(0.05));
        assert_eq!(speed * dt, Distance::new(1.2 * 0.05));
        assert_eq!((dt * speed).get().to_bits(), (0.05_f32 * 1.2).to_bits());
        let nan = f32::from_bits(0x7fc0_0001);
        assert_eq!(
            (Seconds::new(nan) * speed).get().to_bits(),
            (nan * 1.2).to_bits()
        );
        assert_eq!(
            (Speed::new(nan) * dt).get().to_bits(),
            (nan * 0.05).to_bits()
        );
    }
}
