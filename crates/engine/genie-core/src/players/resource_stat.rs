//! How a resource add counts in a player's statistics ([`ResourceStat`]).

/// How a resource add counts in a player's statistics: slot 0xf8's fourth argument
/// ([`crate::world::World::add_resource`]). 1 is a gathered amount (the gathered totals,
/// attributes 166 to 169); any but 0 counts gold's counter (attribute 49).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ResourceStat {
    /// 0: the add counts in no statistic (a trade's lot, a market's payment).
    #[default]
    None,
    /// 1: a gathered amount (a gatherer's, a mill's, a trade's gold).
    Gathered,
    /// A value the game names nowhere, as the save or the game holds it.
    Other(Unnamed),
}

/// A raw value of [`ResourceStat`] that names none of its variants: only the set's own
/// constructors make one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Unnamed(u8);

impl ResourceStat {
    /// The value of raw `value`: its variant, else `Other`.
    const fn of_raw(value: u8) -> Self {
        match value {
            0 => Self::None,
            1 => Self::Gathered,
            other => Self::Other(Unnamed(other)),
        }
    }

    /// Its raw value.
    const fn raw(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Gathered => 1,
            Self::Other(Unnamed(value)) => value,
        }
    }

    /// The statistic of the argument's byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self::of_raw(value)
    }

    /// The byte.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.raw()
    }

    /// The gathered totals grow (1).
    #[must_use]
    pub const fn gathered(self) -> bool {
        self.raw() == 1
    }

    /// Gold's counter grows (any but 0).
    #[must_use]
    pub const fn counts_gold(self) -> bool {
        self.raw() != 0
    }
}

#[cfg(test)]
mod tests {
    use crate::players::ResourceStat;

    /// 1 counts as gathered and in gold's counter, another non-zero byte in the counter alone, 0
    /// in neither.
    #[test]
    fn stat_byte_names_its_counters() {
        let counts = |s: ResourceStat| (s.gathered(), s.counts_gold());
        assert_eq!(counts(ResourceStat::Gathered), (true, true));
        assert_eq!(counts(ResourceStat::of_data(2)), (false, true));
        assert_eq!(counts(ResourceStat::None), (false, false));
        assert_eq!(ResourceStat::Gathered.get(), 1);
    }

    /// Each named value and one the set names nowhere go to their variant and back.
    #[test]
    fn every_resource_stat_value_maps_to_its_variant_and_back() {
        for value in [0, 1, 2] {
            assert_eq!(ResourceStat::of_raw(value).raw(), value);
        }
        assert!(matches!(ResourceStat::of_raw(2), ResourceStat::Other(_)));
    }
}
