//! A unit's formation category in its group ([`FormationCategory`]): the sub-formation its
//! master's type, hero flag and formation category put it in.

/// A formation category of a group (`mFormationCategory` of a statistic, `mDesiredCategory` of a
/// slot): the sub-formation `FUN_141011300` gives a unit (0 to 5, [`crate::groups::sub_formation`]),
/// −1 for none (a unit of no sub-formation, a slot that takes any unit).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FormationCategory(i32);

impl FormationCategory {
    /// None (−1): a unit of no sub-formation, a slot that takes any.
    pub const ANY: Self = Self(-1);
    /// A protected unit's (5), and categories 0 and 5 of the master's.
    pub const PROTECTED: Self = Self(5);
    /// The six sub-formations, 0 to 5, in the layouts' order.
    pub const EACH: [Self; 6] = [Self(0), Self(1), Self(2), Self(3), Self(4), Self(5)];

    /// The category of value `value`, as the save and the game's memory hold it.
    #[must_use]
    pub const fn named(value: i32) -> Self {
        Self(value)
    }

    /// The category after this one, in the layout's order (the value plus 1).
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    /// Its value, as the save holds it.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use crate::groups::FormationCategory;

    /// None is −1, the next category is one more, and a zeroed one is 0.
    #[test]
    fn categories_count_up_from_none() {
        assert_eq!(FormationCategory::ANY.get(), -1);
        assert_eq!(FormationCategory::ANY.next(), FormationCategory::default());
        assert_eq!(
            FormationCategory::named(4).next(),
            FormationCategory::PROTECTED
        );
        assert_eq!(
            FormationCategory::EACH.map(FormationCategory::get),
            [0, 1, 2, 3, 4, 5]
        );
    }
}
