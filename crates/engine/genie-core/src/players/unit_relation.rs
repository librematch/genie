//! What a player's queries make of another player's objects as one type, and the tables by it.

/// What a player's queries make of another player's objects (`player+0x278`, the save's
/// `unit_diplomacy`, which `FUN_141676b80` writes from the relation): Gaia's, its own, an
/// ally's, a neutral's or an enemy's. The visible-units query (`GetVisibleUnits`,
/// `FUN_141288c00`) lists each object by it; a saved value past 4 lists nowhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnitRelation(u8);

impl UnitRelation {
    /// Gaia's objects (0).
    pub const GAIA: Self = Self(0);
    /// Its own objects (1).
    pub const OWN: Self = Self(1);
    /// An ally's objects (2).
    pub const ALLY: Self = Self(2);
    /// A neutral's objects (3).
    pub const NEUTRAL: Self = Self(3);
    /// An enemy's objects (4).
    pub const ENEMY: Self = Self(4);
    /// The relations in their lists' order.
    pub const ALL: [Self; 5] = [
        Self::GAIA,
        Self::OWN,
        Self::ALLY,
        Self::NEUTRAL,
        Self::ENEMY,
    ];

    /// Its value.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// A saved value: 0 to 4 a relation, any other none.
    #[must_use]
    pub const fn of_field(value: i32) -> Option<Self> {
        if 0 <= value && value <= Self::ENEMY.0 as i32 {
            Some(Self(value as u8))
        } else {
            None
        }
    }

    /// Its bit in a query's mask of the relations it keeps (`1 << relation`).
    #[must_use]
    pub const fn bit(self) -> u8 {
        1 << self.0
    }

    /// Its list among the five.
    const fn index(self) -> usize {
        self.0 as usize
    }
}

/// What a player's queries make of each player's objects (`unit_diplomacy[i]`, `player+0x278`),
/// by the other player: none for a saved value past the five relations.
pub type UnitRelations = crate::players::ByPlayer<Option<UnitRelation>>;

/// A test's row of saved relations (`unit_diplomacy`): each value as [`UnitRelation::of_field`]
/// reads it.
#[cfg(test)]
pub(crate) fn relations<const N: usize>(values: [i32; N]) -> UnitRelations {
    values.into_iter().map(UnitRelation::of_field).collect()
}

/// One value per [`UnitRelation`], in their order: the visible-units query's lists.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ByRelation<T>([T; 5]);

impl<T> ByRelation<T> {
    /// The table of the five values in the relations' order.
    #[must_use]
    pub const fn new(values: [T; 5]) -> Self {
        Self(values)
    }

    /// The values in the relations' order.
    #[must_use]
    pub const fn values(&self) -> &[T; 5] {
        &self.0
    }

    /// Each relation with its value, in their order.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = (UnitRelation, &T)> {
        UnitRelation::ALL.into_iter().zip(&self.0)
    }

    /// The table of `f` of each value.
    pub fn map<U>(&self, f: impl FnMut(&T) -> U) -> ByRelation<U> {
        ByRelation(self.0.each_ref().map(f))
    }
}

impl<T> std::ops::Index<UnitRelation> for ByRelation<T> {
    type Output = T;

    fn index(&self, relation: UnitRelation) -> &T {
        &self.0[relation.index()]
    }
}

impl<T> std::ops::IndexMut<UnitRelation> for ByRelation<T> {
    fn index_mut(&mut self, relation: UnitRelation) -> &mut T {
        &mut self.0[relation.index()]
    }
}

#[cfg(test)]
mod tests {
    use crate::players::{ByRelation, UnitRelation};

    /// 0 to 4 are the relations, in their lists' order, each with its bit; other values none.
    #[test]
    fn a_saved_value_is_a_relation_with_its_list_and_bit() {
        assert_eq!(UnitRelation::of_field(4), Some(UnitRelation::ENEMY));
        assert_eq!(UnitRelation::of_field(5), None);
        assert_eq!(UnitRelation::of_field(-1), None);
        assert_eq!(UnitRelation::ALLY.get(), 2);
        assert_eq!(UnitRelation::NEUTRAL.bit(), 8);
        let mut lists = ByRelation::new([0, 1, 2, 3, 4]);
        lists[UnitRelation::OWN] = 10;
        assert_eq!(lists[UnitRelation::OWN], 10);
        assert_eq!(lists.values(), &[0, 10, 2, 3, 4]);
        let order: Vec<u8> = lists.iter().rev().map(|(r, _)| r.get()).collect();
        assert_eq!(order, [4, 3, 2, 1, 0]);
        assert_eq!(lists.map(|v| v * 2).values(), &[0, 20, 4, 6, 8]);
    }
}
