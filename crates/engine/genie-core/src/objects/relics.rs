//! A building's relic counts ([`Relics`]).

use crate::objects::RelicCount;
use crate::world::ClockSeconds;
use genie_diff::Diff;

/// A building's `relic_count` (`+0xc20`) and `specific_relic_count` (`+0xc24`): the relics it
/// holds, which its elevation factor reads as the shooter and as the target
/// ([`crate::combat::damage::building_elevation_factor`]). Each compares signed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Diff)]
pub struct Relics {
    /// `relic_count`.
    pub(crate) count: RelicCount,
    /// `specific_relic_count`.
    pub(crate) specific: RelicCount,
}

impl Relics {
    /// No relics.
    pub const NONE: Self = Self::new(RelicCount::ZERO, RelicCount::ZERO);

    /// `count` and `specific`.
    #[must_use]
    pub const fn new(count: RelicCount, specific: RelicCount) -> Self {
        Self { count, specific }
    }

    /// The relic seconds of `seconds` whole seconds (`TRIBE_Building_Object::update`,
    /// `0x1411c9000`, a monastery's relic goods): the count plus twice the specific count
    /// (`LEA`), times the seconds (`IMUL`: both wrap as ints), then as an `f32` (`CVTDQ2PS`).
    #[must_use]
    pub(crate) const fn relic_seconds(self, seconds: ClockSeconds) -> f32 {
        let relics = self
            .count
            .get()
            .wrapping_add(self.specific.get().wrapping_mul(2));
        relics.wrapping_mul(seconds.get().cast_signed()) as f32
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::RelicCount;
    use crate::objects::Relics;
    use crate::world::Millis;

    /// The counts keep their order: the relic count, then the specific one.
    #[test]
    fn relics_hold_the_count_then_the_specific_count() {
        let relics = Relics::new(RelicCount::of_data(2), RelicCount::of_data(-1));
        assert_eq!((relics.count.get(), relics.specific.get()), (2, -1));
        assert_eq!(Relics::NONE, Relics::default());
    }

    /// The relic seconds: the count plus twice the specific count, times the whole seconds,
    /// each an int product that wraps, then the int as an `f32` (rounded to nearest).
    #[test]
    fn relic_seconds_count_specific_relics_twice_and_wrap_as_ints() {
        let seconds = |millis| Millis::new(millis).whole_seconds();
        let relics = Relics::new(RelicCount::of_data(2), RelicCount::of_data(1));
        assert_eq!(
            relics.relic_seconds(seconds(3999)).to_bits(),
            12.0f32.to_bits()
        );
        assert_eq!(Relics::NONE.relic_seconds(seconds(3000)).to_bits(), 0);
        let negative = Relics::new(RelicCount::of_data(-3), RelicCount::of_data(1));
        assert_eq!(negative.relic_seconds(seconds(5000)), -5.0);
        let doubled = Relics::new(RelicCount::ZERO, RelicCount::of_data(1 << 30));
        assert_eq!(doubled.relic_seconds(seconds(1000)), -2_147_483_648.0);
        let many = Relics::new(RelicCount::of_data(1 << 30), RelicCount::ZERO);
        assert_eq!(many.relic_seconds(seconds(2000)), -2_147_483_648.0);
        let odd = Relics::new(RelicCount::of_data(16_777_217), RelicCount::ZERO);
        assert_eq!(odd.relic_seconds(seconds(1000)), 16_777_216.0);
    }
}
