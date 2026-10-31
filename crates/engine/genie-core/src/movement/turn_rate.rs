//! How fast a unit or a group turns ([`TurnRate`], radians per second), with the game's `f32`
//! operations.

use crate::math::Angle;
use crate::world::Seconds;

/// The most a unit or a group turns per second, in radians, as the game's `f32` holds it: a
/// master's turn rate (`+0x774`) and its revolution's rate (`+0x77c`, 2π over a revolution's
/// time), a group's most yaw per second moving and standing. The largest float
/// ([`TurnRate::MAX`]) turns at once. Each operator is the one `f32` operation the game does, in
/// its order; a comparison is `f32`'s (a NaN compares false).
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct TurnRate(f32);

impl TurnRate {
    /// No turn.
    pub const ZERO: Self = Self(0.0);
    /// The game's `FLT_MAX`: no limit to the turn.
    pub const MAX: Self = Self(f32::MAX);

    /// `radians` per second.
    #[must_use]
    pub const fn new(radians: f32) -> Self {
        Self(radians)
    }

    /// The radians per second, as the save and the game's arithmetic hold them.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl std::ops::Mul<Seconds> for TurnRate {
    type Output = Angle;

    /// The most turn in `dt` (one `f32` product, the rate first).
    fn mul(self, dt: Seconds) -> Angle {
        Angle::new(self.0 * dt.get())
    }
}

impl std::ops::Div<Seconds> for Angle {
    type Output = TurnRate;

    /// The rate of turning by the angle in `dt` (one `f32` quotient: 2π over a revolution's
    /// time).
    fn div(self, dt: Seconds) -> TurnRate {
        TurnRate(self.get() / dt.get())
    }
}

impl std::fmt::Display for TurnRate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl genie_diff::Diff for TurnRate {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::math::Angle;
    use crate::movement::TurnRate;
    use crate::world::Seconds;

    /// A rate over seconds is an angle and an angle over seconds a rate, each one `f32`
    /// operation in the game's operand order.
    #[test]
    fn a_turn_rate_over_seconds_is_an_angle() {
        let (r, dt) = (TurnRate::new(0.7), Seconds::new(0.3));
        assert_eq!((r * dt).get().to_bits(), (0.7_f32 * 0.3).to_bits());
        assert_eq!(
            (Angle::new(6.5) / Seconds::new(1.6)).get().to_bits(),
            (6.5_f32 / 1.6).to_bits()
        );
        assert!(TurnRate::ZERO < r && r < TurnRate::MAX);
        assert_eq!(TurnRate::new(f32::NAN).partial_cmp(&r), None);
        assert_eq!(TurnRate::new(1.5).to_string(), "1.5");
    }
}
