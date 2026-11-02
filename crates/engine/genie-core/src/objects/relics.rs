//! A building's relic counts ([`Relics`]).

use crate::objects::RelicCount;
use genie_diff::Diff;

/// A building's `relic_count` (`+0xc20`) and `specific_relic_count` (`+0xc24`): the relics it
/// holds, which its elevation factor reads as the shooter and as the target
/// ([`crate::combat::damage::building_elevation_factor`]). Each compares signed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Diff)]
pub struct Relics {
    /// `relic_count`.
    pub count: RelicCount,
    /// `specific_relic_count`.
    pub specific: RelicCount,
}

impl Relics {
    /// No relics.
    pub const NONE: Self = Self::new(RelicCount::ZERO, RelicCount::ZERO);

    /// `count` and `specific`.
    #[must_use]
    pub const fn new(count: RelicCount, specific: RelicCount) -> Self {
        Self { count, specific }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    use crate::objects::RelicCount;
    use crate::objects::Relics;

    /// The counts keep their order: the relic count, then the specific one.
    #[test]
    fn relics_hold_the_count_then_the_specific_count() {
        let r = Relics::new(RelicCount::of_data(2), RelicCount::of_data(-1));
        assert_eq!((r.count.get(), r.specific.get()), (2, -1));
        assert_eq!(Relics::NONE, Relics::default());
    }
}
