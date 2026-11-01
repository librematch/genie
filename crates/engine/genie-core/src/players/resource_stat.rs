//! How a resource add counts in a player's statistics ([`ResourceStat`]).

/// How a resource add counts in a player's statistics: slot 0xf8's fourth argument
/// ([`crate::world::World::add_resource`]). 1 is a gathered amount (the gathered totals,
/// attributes 166 to 169); any but 0 counts gold's counter (attribute 49).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ResourceStat(u8);

impl ResourceStat {
    /// 0: the add counts in no statistic (a trade's lot, a market's payment).
    pub const NONE: Self = Self(0);
    /// 1: a gathered amount (a gatherer's, a mill's, a trade's gold).
    pub const GATHERED: Self = Self(1);

    /// The statistic of the argument's byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// The byte.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// The gathered totals grow (1).
    #[must_use]
    pub const fn gathered(self) -> bool {
        self.0 == 1
    }

    /// Gold's counter grows (any but 0).
    #[must_use]
    pub const fn counts_gold(self) -> bool {
        self.0 != 0
    }
}

#[cfg(test)]
mod tests {
    use crate::players::ResourceStat;

    /// 1 counts as gathered and in gold's counter, another non-zero byte in the counter alone, 0
    /// in neither.
    #[test]
    fn a_stat_s_byte_names_its_counters() {
        let counts = |s: ResourceStat| (s.gathered(), s.counts_gold());
        assert_eq!(counts(ResourceStat::GATHERED), (true, true));
        assert_eq!(counts(ResourceStat::of_data(2)), (false, true));
        assert_eq!(counts(ResourceStat::NONE), (false, false));
        assert_eq!(ResourceStat::GATHERED.get(), 1);
    }
}
