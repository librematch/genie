//! A moving master's turning values (`+0x76c`..`+0x77c`) and the rate every copy of the master
//! sets from its turn speed and walking sprite (FD-046: the model's part of the movement).

use crate::map::Distance;
use crate::math::Angle;
use crate::math::image::movement::TWO_PI;
use crate::movement::Speed;
use crate::movement::TurnRate;
use crate::objects::FacetCount;
use crate::world::Seconds;

/// The most a unit of a moving master turns per second (`+0x774`) as every copy of the master
/// sets it (master slot 0x80, `FUN_1410e3a70`, which also leaves the turn radius 0 and its
/// speed the largest float): with a turn speed (`+0x758`) and a walking sprite, the sprite's
/// facets times the turn speed are the time of a revolution (`+0x778`, slot 0x98
/// `FUN_1410e39e0`), and 2π over that is the rate (`+0x77c`, copied to `+0x774`); without either
/// the largest float. The `.dat`'s own turning values do not survive the copy.
#[must_use]
pub fn turn_rate(turn_speed: Seconds, walking_facets: Option<FacetCount>) -> TurnRate {
    turning(turn_speed, walking_facets, Turning::PRESET).0.rate
}

/// A moving master's turning values (`+0x76c`..`+0x77c`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Turning {
    /// `+0x76c`: the turn radius.
    pub radius: Distance,
    /// `+0x770`: the speed at which the turn radius is its value.
    pub radius_speed: Speed,
    /// `+0x774`: the most the unit turns per second.
    pub rate: TurnRate,
    /// `+0x778`: a revolution's time (slot 0x98, `FUN_1410e39e0`).
    pub revolution: Seconds,
    /// `+0x77c`: 2π over the revolution's time.
    pub revolution_rate: TurnRate,
}

impl Turning {
    /// What the master's three constructors (`FUN_1410d9440`, `FUN_1410d95b0`,
    /// `FUN_1410d9790`) set: radius 0, the largest float, and the 8-byte stores
    /// `0x7f7fffff` at `+0x774` and `+0x77c` (the largest float, then 0 above it).
    pub const PRESET: Self = Self {
        radius: Distance::ZERO,
        radius_speed: Speed::MAX,
        rate: TurnRate::MAX,
        revolution: Seconds::ZERO,
        revolution_rate: TurnRate::MAX,
    };
}

/// `FUN_1410e3a70` (master slot 0x80) on a master's turning values `before`, with its answer: a
/// turn speed (`+0x758`) of 0 resets them (radius 0, the largest float for the radius speed,
/// the rate and the revolution's rate, the revolution time 0; false); with a walking sprite
/// (`+0x748`) the facets times the turn speed are the revolution's time (`FUN_1410e39e0`: 2π
/// over it, the largest float for 0) and the rate takes 2π over it (true); without one nothing
/// changes (false).
#[must_use]
pub fn turning(
    turn_speed: Seconds,
    walking_facets: Option<FacetCount>,
    before: Turning,
) -> (Turning, bool) {
    if turn_speed == Seconds::ZERO {
        return (Turning::PRESET, false);
    }
    let Some(facets) = walking_facets else {
        return (before, false);
    };
    let revolution = Seconds::new(f32::from(facets.get()) * turn_speed.get());
    let revolution_rate = if revolution == Seconds::ZERO {
        TurnRate::MAX
    } else {
        Angle::new(TWO_PI) / revolution
    };
    let after = Turning {
        revolution,
        revolution_rate,
        rate: revolution_rate,
        ..before
    };
    (after, true)
}
