//! A search's relation number: which players' objects `ClosestObject` asks for or passes over.

use crate::players::UnitRelation;

/// A relation number of `ClosestObject`'s arguments (`FUN_140c488b0`: its filter `param_3` and
/// the relation it passes over, argument 9): 0 the friends, 1 the neutrals, 2 the enemies, 3
/// itself. Any other number (the callers' −1) is none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum QueryRelation {
    /// 0: the friends.
    Friends,
    /// 1: the neutrals.
    Neutrals,
    /// 2: the enemies.
    Enemies,
    /// 3: itself.
    Itself,
}

impl QueryRelation {
    /// The relation of number `value` (0 to 3), none for any other.
    #[must_use]
    pub const fn of_number(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Friends),
            1 => Some(Self::Neutrals),
            2 => Some(Self::Enemies),
            3 => Some(Self::Itself),
            _ => None,
        }
    }

    /// `FUN_140c32b50`: the one relation list the number names, as the relation passed over
    /// reads it: the friends' (list 2), the neutrals' (3), the enemies' (4), its own (1).
    #[must_use]
    pub const fn list(self) -> UnitRelation {
        match self {
            Self::Friends => UnitRelation::Ally,
            Self::Neutrals => UnitRelation::Neutral,
            Self::Enemies => UnitRelation::Enemy,
            Self::Itself => UnitRelation::Own,
        }
    }

    /// `ClosestObject`'s relation filter on the relation list `relation`: the friends keep its
    /// own objects too (lists 1 and 2), the others their [`QueryRelation::list`].
    #[must_use]
    pub const fn keeps(self, relation: UnitRelation) -> bool {
        match self {
            Self::Friends => matches!(relation, UnitRelation::Own | UnitRelation::Ally),
            Self::Neutrals => matches!(relation, UnitRelation::Neutral),
            Self::Enemies => matches!(relation, UnitRelation::Enemy),
            Self::Itself => matches!(relation, UnitRelation::Own),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::players::{QueryRelation, UnitRelation};

    /// The numbers 0 to 3 are the four relations; any other is none.
    #[test]
    fn relation_number_is_one_of_four() {
        assert_eq!(
            [0, 1, 2, 3, -1, 4].map(QueryRelation::of_number),
            [
                Some(QueryRelation::Friends),
                Some(QueryRelation::Neutrals),
                Some(QueryRelation::Enemies),
                Some(QueryRelation::Itself),
                None,
                None
            ]
        );
    }

    /// `FUN_140c32b50` names one list each (friends 2, neutrals 3, enemies 4, itself 1); the
    /// filter keeps the friends with its own objects (1 and 2), the others that one list.
    #[test]
    fn the_friends_filter_keeps_its_own_objects_and_the_list_does_not() {
        let all = [
            QueryRelation::Friends,
            QueryRelation::Neutrals,
            QueryRelation::Enemies,
            QueryRelation::Itself,
        ];
        assert_eq!(all.map(|r| r.list().get()), [2, 3, 4, 1]);
        let kept = |filter: QueryRelation| {
            UnitRelation::ALL
                .into_iter()
                .filter(|&r| filter.keeps(r))
                .map(UnitRelation::get)
                .collect::<Vec<_>>()
        };
        assert_eq!(all.map(kept), [vec![1, 2], vec![3], vec![4], vec![1]]);
    }
}
