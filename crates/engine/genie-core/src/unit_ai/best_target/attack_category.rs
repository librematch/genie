//! `bestUnitToAttack`'s attack categories and the tables it keeps by category.

use crate::GenieError;

/// An attack category (`FUN_140c4c8e0`, [`crate::unit_ai::best_target::category`]): the group of
/// targets by which `bestUnitToAttack` weighs and narrows its choice, 0 to 11. A class the
/// function does not rank has none (the game's −1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AttackCategory(u8);

impl AttackCategory {
    /// How many categories there are: the tables by category hold one entry each.
    pub const COUNT: usize = 12;

    /// The siege weapons' (classes 13, 54 and 55 but four masters), which the ranking does not
    /// rank by weapon range.
    pub const SIEGE: Self = Self(6);

    /// The value a field takes for none.
    const NONE: i32 = -1;

    /// A category the code names: below [`AttackCategory::COUNT`].
    pub(crate) const fn named(value: u8) -> Self {
        assert!((value as usize) < Self::COUNT, "an attack category past 11");
        Self(value)
    }

    /// Its value.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// The category of a saved field that holds −1 for none (an order record's
    /// `mTargetAttackCategory`).
    ///
    /// # Errors
    ///
    /// Any other value outside 0 to 11.
    pub fn of_field(value: i32) -> Result<Option<Self>, GenieError> {
        if value == Self::NONE {
            return Ok(None);
        }
        u8::try_from(value)
            .ok()
            .filter(|&v| usize::from(v) < Self::COUNT)
            .map(|v| Some(Self(v)))
            .ok_or_else(|| {
                GenieError::invalid_input("an attack category past 0 to 11").with("category", value)
            })
    }

    /// The value of a field that holds −1 for none.
    #[must_use]
    pub fn field(category: Option<Self>) -> i32 {
        category.map_or(Self::NONE, |c| i32::from(c.0))
    }
}

impl genie_diff::Diff for AttackCategory {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

/// A value per attack category (`bestUnitToAttack`'s counts and the order history's weights),
/// read and written by category, so that no index falls outside it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ByCategory<T>([T; AttackCategory::COUNT]);

impl<T> ByCategory<T> {
    /// The table of `values`, in the categories' order.
    #[must_use]
    pub const fn new(values: [T; AttackCategory::COUNT]) -> Self {
        Self(values)
    }

    /// The values in the categories' order.
    #[must_use]
    pub const fn values(&self) -> &[T; AttackCategory::COUNT] {
        &self.0
    }
}

impl<T> std::ops::Index<AttackCategory> for ByCategory<T> {
    type Output = T;

    fn index(&self, category: AttackCategory) -> &T {
        // a category is below COUNT, the table's length
        &self.0[usize::from(category.0)]
    }
}

impl<T> std::ops::IndexMut<AttackCategory> for ByCategory<T> {
    fn index_mut(&mut self, category: AttackCategory) -> &mut T {
        &mut self.0[usize::from(category.0)]
    }
}

#[cfg(test)]
mod tests {
    use crate::ErrorKind;
    use crate::unit_ai::best_target::{AttackCategory, ByCategory};

    /// A saved −1 is no category, 0 to 11 a category, and each converts back to its value; any
    /// other value is an error.
    #[test]
    fn saved_category_is_minus_one_or_0_to_11() {
        assert_eq!(AttackCategory::of_field(-1), Ok(None));
        let siege = AttackCategory::of_field(6).unwrap();
        assert_eq!(siege, Some(AttackCategory::SIEGE));
        assert_eq!(AttackCategory::field(siege), 6);
        assert_eq!(AttackCategory::field(None), -1);
        assert_eq!(
            AttackCategory::of_field(11).map(|c| c.map(AttackCategory::get)),
            Ok(Some(11))
        );
        for bad in [-2, 12, 300] {
            assert_eq!(
                AttackCategory::of_field(bad).map_err(|e| e.kind()),
                Err(ErrorKind::InvalidInput)
            );
        }
    }

    /// A table holds one value per category, read and written by category in their order.
    #[test]
    fn table_by_category_holds_one_value_each() {
        let mut t = ByCategory::new([0; AttackCategory::COUNT]);
        t[AttackCategory::SIEGE] += 3;
        t[AttackCategory::named(11)] = 7;
        assert_eq!(t[AttackCategory::SIEGE], 3);
        assert_eq!(t.values(), &[0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 7]);
    }
}
