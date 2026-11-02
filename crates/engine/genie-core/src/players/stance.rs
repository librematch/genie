//! A player's stance toward another as one type: `diplomacy[i]` (`+0xe12`).

use crate::players::Relation;

/// A player's stance toward player i (`diplomacy[i]`, `+0xe12`, saved), which a diplomacy
/// change sets (`FUN_141639690`) and which makes the relation (`FUN_1416397d0`). Every byte is
/// a stance (0 none, as the save holds for Gaia and itself).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Stance(u8);

impl Stance {
    /// No stance (0): it makes no relation.
    pub const NONE: Self = Self(0);
    /// Stance 1, an enemy's as 2.
    pub const ENEMY_1: Self = Self(1);
    /// An enemy's stance (2), which a diplomacy change to an enemy sets.
    pub const ENEMY: Self = Self(2);
    /// A neutral's stance (3).
    pub const NEUTRAL: Self = Self(3);
    /// An ally's stance (4), which a diplomacy change to an ally sets.
    pub const ALLY: Self = Self(4);
    /// Stance 5, an ally's as 4.
    pub const ALLY_5: Self = Self(5);

    /// Its value.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// The stance GAME's diplomacy (`FUN_141639690`) sets for the command's relation `value`:
    /// 0 an ally's, 1 a neutral's, 3 and 4 an enemy's; another none, which changes nothing.
    #[must_use]
    pub const fn of_diplomacy(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::ALLY),
            1 => Some(Self::NEUTRAL),
            3 | 4 => Some(Self::ENEMY),
            _ => None,
        }
    }

    /// `FUN_1416397d0`'s relation for the stance: 1 and 2 an enemy's (relation 4 while
    /// `FUN_1415e8e10` holds: the world's `+0x820` object in its state 1, which the slice does
    /// not hold, as the task finder's [`crate::unit_ai::task_finder::allied`]), 3 a neutral's,
    /// 4 and 5 an ally's; another none.
    #[must_use]
    pub const fn relation(self) -> Option<Relation> {
        match self.0 {
            1 | 2 => Some(Relation::ENEMY),
            3 => Some(Relation::NEUTRAL),
            4 | 5 => Some(Relation::ALLY),
            _ => None,
        }
    }
}

impl From<u8> for Stance {
    /// The stance a saved byte holds: every value is one.
    fn from(value: u8) -> Self {
        Self(value)
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
        assert_eq!(relation(1), Some(Relation::ENEMY));
        assert_eq!(relation(2), Some(Relation::ENEMY));
        assert_eq!(relation(3), Some(Relation::NEUTRAL));
        assert_eq!(relation(4), Some(Relation::ALLY));
        assert_eq!(relation(5), Some(Relation::ALLY));
        assert_eq!(relation(6), None);
        assert_eq!(Stance::ALLY.get(), 4);
    }

    /// GAME's diplomacy: relation 0 an ally's stance, 1 a neutral's, 3 and 4 an enemy's, 2 and
    /// the others none.
    #[test]
    fn diplomacy_command_relation_makes_its_stance() {
        let stances = [0, 1, 2, 3, 4, 5, u32::MAX].map(Stance::of_diplomacy);
        assert_eq!(
            stances,
            [
                Some(Stance::ALLY),
                Some(Stance::NEUTRAL),
                None,
                Some(Stance::ENEMY),
                Some(Stance::ENEMY),
                None,
                None
            ]
        );
    }
}
