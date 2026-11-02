//! An acceleration in tiles per second squared.

use crate::movement::Speed;
use crate::world::Seconds;
use std::ops::Mul;

/// An acceleration in tiles per second squared: a missile's arc's vertical one (`+0x78`), which
/// the launch sets and each step's dt turns into vertical speed.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct Acceleration(f32);

impl Acceleration {
    /// None.
    pub const ZERO: Self = Self(0.0);

    /// `tiles` per second squared.
    #[must_use]
    pub const fn new(tiles: f32) -> Self {
        Self(tiles)
    }

    /// The float.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl Mul<Acceleration> for Seconds {
    type Output = Speed;

    /// The speed `self` of the acceleration gains (one `f32` product, the time first).
    fn mul(self, a: Acceleration) -> Speed {
        Speed::new(self.get() * a.0)
    }
}

impl genie_diff::Diff for Acceleration {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::movement::Acceleration;
    use crate::world::Seconds;

    /// A time times an acceleration is the one `f32` product, the time first.
    #[test]
    fn time_times_an_acceleration_is_speed() {
        let a = Acceleration::new(-23.52);
        assert_eq!(
            (Seconds::new(0.1) * a).get().to_bits(),
            (0.1_f32 * -23.52).to_bits()
        );
        assert_eq!(Acceleration::default(), Acceleration::ZERO);
        assert_eq!(a.get(), -23.52);
    }
}
