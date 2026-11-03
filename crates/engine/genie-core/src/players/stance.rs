//! A player's stance toward another as one type: `diplomacy[i]` (`+0xe12`).

use crate::players::Relation;

/// A player's stance toward player i (`diplomacy[i]`, `+0xe12`, saved), which a diplomacy
/// change sets (`FUN_141639690`) and which makes the relation (`FUN_1416397d0`). Every byte is
/// a stance (0 none, as the save holds for Gaia and itself).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Stance {
    /// No stance (0): it makes no relation.
    #[default]
    None,
    /// Stance 1, an enemy's as 2.
    Enemy1,
    /// An enemy's stance (2), which a diplomacy change to an enemy sets.
    Enemy,
    /// A neutral's stance (3).
    Neutral,
    /// An ally's stance (4), which a diplomacy change to an ally sets.
    Ally,
    /// Stance 5, an ally's as 4.
    Ally5,
    /// A value the game names nowhere, as the save or the game holds it.
    Other(Unnamed),
}

/// A raw value of [`Stance`] that names none of its variants: only the set's own
/// constructors make one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Unnamed(u8);

impl Stance {
    /// The value of raw `value`: its variant, else `Other`.
    const fn of_raw(value: u8) -> Self {
        match value {
            0 => Self::None,
            1 => Self::Enemy1,
            2 => Self::Enemy,
            3 => Self::Neutral,
            4 => Self::Ally,
            5 => Self::Ally5,
            other => Self::Other(Unnamed(other)),
        }
    }

    /// Its raw value.
    const fn raw(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Enemy1 => 1,
            Self::Enemy => 2,
            Self::Neutral => 3,
            Self::Ally => 4,
            Self::Ally5 => 5,
            Self::Other(Unnamed(value)) => value,
        }
    }

    /// Its value.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.raw()
    }

    /// The stance GAME's diplomacy (`FUN_141639690`) sets for the command's relation `value`:
    /// 0 an ally's, 1 a neutral's, 3 and 4 an enemy's; another none, which changes nothing.
    #[must_use]
    pub const fn of_diplomacy(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Ally),
            1 => Some(Self::Neutral),
            3 | 4 => Some(Self::Enemy),
            _ => None,
        }
    }

    /// `FUN_1416397d0`'s relation for the stance: 1 and 2 an enemy's (relation 4 while
    /// `FUN_1415e8e10` holds: the world's `+0x820` object in its state 1, which the slice does
    /// not hold, as the task finder's [`crate::unit_ai::task_finder::allied`]), 3 a neutral's,
    /// 4 and 5 an ally's; another none.
    #[must_use]
    pub const fn relation(self) -> Option<Relation> {
        match self.raw() {
            1 | 2 => Some(Relation::Enemy),
            3 => Some(Relation::Neutral),
            4 | 5 => Some(Relation::Ally),
            _ => None,
        }
    }
}

impl From<u8> for Stance {
    /// The stance a saved byte holds: every value is one.
    fn from(value: u8) -> Self {
        Self::of_raw(value)
    }
}

impl genie_diff::Diff for Stance {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::players::{Relation, Stance};

    /// `FUN_1416397d0`'s relations: 1, 2 an enemy's, 3 a neutral's, 4, 5 an ally's, others none.
    #[test]
    fn stance_makes_its_relation() {
        let relation = |s: u8| Stance::from(s).relation();
        assert_eq!(relation(0), None);
        assert_eq!(relation(1), Some(Relation::Enemy));
        assert_eq!(relation(2), Some(Relation::Enemy));
        assert_eq!(relation(3), Some(Relation::Neutral));
        assert_eq!(relation(4), Some(Relation::Ally));
        assert_eq!(relation(5), Some(Relation::Ally));
        assert_eq!(relation(6), None);
        assert_eq!(Stance::Ally.get(), 4);
    }

    /// GAME's diplomacy: relation 0 an ally's stance, 1 a neutral's, 3 and 4 an enemy's, 2 and
    /// the others none.
    #[test]
    fn diplomacy_command_relation_makes_its_stance() {
        let stances = [0, 1, 2, 3, 4, 5, u32::MAX].map(Stance::of_diplomacy);
        assert_eq!(
            stances,
            [
                Some(Stance::Ally),
                Some(Stance::Neutral),
                None,
                Some(Stance::Enemy),
                Some(Stance::Enemy),
                None,
                None
            ]
        );
    }

    /// Each named value and one the set names nowhere go to their variant and back.
    #[test]
    fn every_stance_value_maps_to_its_variant_and_back() {
        for value in [0, 1, 2, 3, 4, 5, 6] {
            assert_eq!(Stance::of_raw(value).raw(), value);
        }
        assert!(matches!(Stance::of_raw(6), Stance::Other(_)));
    }
}
