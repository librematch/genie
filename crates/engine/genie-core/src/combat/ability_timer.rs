//! A combat object's timed ability: how long it lasts and when it began.

use crate::math::conversions::cvttsd2si;
use crate::math::image::combat_update::PERCENT;
use crate::world::Millis;
use crate::world::WorldTime;

/// A combat object's timed ability (`m_specialAbilityDurationMs` `+0x914` and
/// `m_abilityActivatedTimeMs` `+0x918`, saved): how long it lasts and the world time it began.
/// `FUN_14107dcc0` starts one (its master's `charge_event` `+0x920` times 1000, the world time),
/// called by the ability's action (`FUN_140a942e0`, not ported); a charge of type −3 does not
/// refill while it lasts ([`AbilityTimer::lasts`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct AbilityTimer {
    /// `+0x914`: how long the ability lasts.
    pub(crate) duration: Millis,
    /// `+0x918`: the world time it began.
    pub(crate) activated: WorldTime,
}

impl AbilityTimer {
    /// `FUN_14107df50`: the per cent of the ability left at `now`. The span since it began
    /// (`SUB`, wrapping) past its duration (`CMP`/`JA`, unsigned) leaves none; else the
    /// duration less that span (`CVTSI2SD` of the 64-bit register). Over the duration
    /// (`DIVSD`) times 100 (`DAT_142f4feb0`, `MULSD`), `CVTTSD2SI` to 32 bits: a duration of 0
    /// divides 0 by 0, and the NaN gives the integer indefinite.
    #[must_use]
    pub fn left_percent(self, now: WorldTime) -> i32 {
        let span = now.since(self.activated).get();
        let duration = self.duration.get();
        let left = if span > duration {
            0.0
        } else {
            f64::from(duration - span)
        };
        cvttsd2si(left / f64::from(duration) * PERCENT)
    }

    /// `FUN_14107d270`: an ability began (its duration is not 0).
    #[must_use]
    pub const fn began(self) -> bool {
        self.duration.get() != 0
    }

    /// `FUN_14107d200`: an ability began and its duration has passed at `now` (the span since
    /// its start, `SUB` wrapping, at least the duration, `CMP`/`JBE` unsigned).
    #[must_use]
    pub const fn over(self, now: WorldTime) -> bool {
        self.began() && self.duration.get() <= now.since(self.activated).get()
    }

    /// Whether the ability lasts at `now`: the low byte of [`AbilityTimer::left_percent`] above
    /// 0 as a signed byte (`TEST AL,AL`, `JG`, `0x141208e98`).
    #[must_use]
    pub fn lasts(self, now: WorldTime) -> bool {
        0 < self.left_percent(now) as i8
    }
}

impl genie_diff::Diff for AbilityTimer {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        differ.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::combat::AbilityTimer;
    use crate::world::Millis;
    use crate::world::WorldTime;

    fn timer(duration: u32, activated: u32) -> AbilityTimer {
        AbilityTimer {
            duration: Millis::new(duration),
            activated: WorldTime::from_millis(activated),
        }
    }

    /// The per cent left falls from 100 to 0 over the duration and stays 0 past it; the timer
    /// lasts while it is above 0 as a byte.
    #[test]
    fn ability_left_falls_over_its_duration() {
        let ability = timer(10_000, 5_000);
        assert_eq!(ability.left_percent(WorldTime::from_millis(5_000)), 100);
        assert_eq!(ability.left_percent(WorldTime::from_millis(7_500)), 75);
        assert_eq!(ability.left_percent(WorldTime::from_millis(14_999)), 0);
        assert_eq!(ability.left_percent(WorldTime::from_millis(20_000)), 0);
        assert!(ability.lasts(WorldTime::from_millis(7_500)));
        assert!(!ability.lasts(WorldTime::from_millis(15_000)));
    }

    /// An ability begins with a duration and is over once the span since its start reaches it;
    /// none began without one, and none is over then.
    #[test]
    fn ability_is_over_once_its_duration_has_passed() {
        let ability = timer(10_000, 5_000);
        assert!(ability.began());
        assert!(!ability.over(WorldTime::from_millis(14_999)));
        assert!(ability.over(WorldTime::from_millis(15_000)));
        // a moment before its start wraps past the duration
        assert!(ability.over(WorldTime::from_millis(4_999)));
        let none = timer(0, 5_000);
        assert!(!none.began());
        assert!(!none.over(WorldTime::from_millis(20_000)));
    }

    /// A duration of 0 (no ability began) is 0 over 0: the integer indefinite, whose low byte is
    /// 0, so it does not last; a moment before its start wraps past the duration.
    #[test]
    fn ability_without_duration_never_lasts() {
        let none = AbilityTimer::default();
        assert_eq!(none.left_percent(WorldTime::from_millis(1_000)), i32::MIN);
        assert_eq!(none.left_percent(WorldTime::ZERO), i32::MIN);
        assert!(!none.lasts(WorldTime::from_millis(1_000)));
        assert_eq!(
            timer(1_000, 5_000).left_percent(WorldTime::from_millis(4_000)),
            0
        );
    }
}
