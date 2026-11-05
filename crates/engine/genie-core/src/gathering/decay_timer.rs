//! A combat object's decay timer, which a salvage pile's decay runs.

use crate::math::conversions::cvttss2si;
use crate::math::image::decay::MS_PER_SECOND;
use crate::players::Amount;
use crate::world::Millis;

/// A combat object's `decay_timer` (`+0x99c`, a short, saved): the milliseconds before a salvage
/// pile (class 40) loses one of what it holds (`TRIBE_Combat_Object::update`). The combat setup
/// (`FUN_14121df60`) starts every combat object's at its player's salvage decay rate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DecayTimer(i16);

impl DecayTimer {
    /// The timer of the save's short `value`.
    #[must_use]
    pub const fn of_data(value: i16) -> Self {
        Self(value)
    }

    /// The short.
    #[must_use]
    pub const fn get(self) -> i16 {
        self.0
    }

    /// The timer less the world's dt `dt` (`SUB word ptr` with the dt's low 16 bits, `+0x7c`),
    /// wrapping as the game's short does.
    #[must_use]
    pub const fn less(self, dt: Millis) -> Self {
        Self(self.0.wrapping_sub(dt.get() as u16 as i16))
    }

    /// Whether it still runs: above 0 (`CMP`/`JG`, `0x1412086d2`).
    #[must_use]
    pub const fn runs(self) -> bool {
        0 < self.0
    }

    /// The timer a salvage decay rate `rate` (attribute 65, seconds) starts: the rate times 1000
    /// (`MULSS`), `CVTTSS2SI` to 32 bits, its low 16 bits stored (`FUN_14121df60` at
    /// `0x14121e7d1`).
    #[must_use]
    pub fn of_rate(rate: Amount) -> Self {
        Self(cvttss2si(rate.get() * MS_PER_SECOND) as i16)
    }
}

impl genie_diff::Diff for DecayTimer {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

#[cfg(test)]
mod tests {
    use crate::gathering::DecayTimer;
    use crate::players::Amount;

    /// The rate's milliseconds truncate toward zero and keep their low 16 bits; a NaN gives the
    /// integer indefinite's low half, 0.
    #[test]
    fn decay_timer_keeps_the_rate_in_milliseconds_in_sixteen_bits() {
        assert_eq!(DecayTimer::of_rate(Amount::new(2.0)).get(), 2000);
        assert_eq!(DecayTimer::of_rate(Amount::new(0.0015)).get(), 1);
        assert_eq!(
            DecayTimer::of_rate(Amount::new(40.0)).get(),
            40_000_i32 as i16
        );
        assert_eq!(DecayTimer::of_rate(Amount::new(f32::NAN)).get(), 0);
        assert_eq!(DecayTimer::default(), DecayTimer::of_data(0));
    }

    /// The dt's low 16 bits come off the timer, wrapping; it runs above 0.
    #[test]
    fn decay_timer_loses_the_dt_and_runs_above_zero() {
        use crate::world::Millis;
        let timer = DecayTimer::of_data(100);
        assert_eq!(timer.less(Millis::new(40)), DecayTimer::of_data(60));
        assert!(timer.less(Millis::new(99)).runs());
        assert!(!timer.less(Millis::new(100)).runs());
        assert_eq!(
            DecayTimer::of_data(i16::MIN).less(Millis::new(1)),
            DecayTimer::of_data(i16::MAX)
        );
        assert_eq!(
            timer.less(Millis::new(0x1_0000 + 40)),
            DecayTimer::of_data(60)
        );
    }
}
