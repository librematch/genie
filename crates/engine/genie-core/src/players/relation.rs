//! How a player stands to another as one type ([`Relation`]), and a player's row of them
//! ([`Relations`], `relation[i]`, `player+0x260`).

use crate::players::ByPlayer;

/// How a player stands to player i (`relation[i]`, `player+0x260`, which `FUN_141676b80`
/// writes from a stance, `FUN_1416397d0`): an ally, a neutral or an enemy. Every byte is a
/// relation (the save holds what the scenario set).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Relation {
    /// An ally (0, the stances 4 and 5): also a player's relation to itself.
    #[default]
    Ally,
    /// A neutral (1, the stance 3).
    Neutral,
    /// An enemy (3, the stances 1 and 2).
    Enemy,
    /// The relation the stances 1 and 2 make while `FUN_1415e8e10` holds (the world's `+0x820`
    /// object in its state 1): the queries take it for an ally's, the task finder only with
    /// its ally flag.
    Relation4,
    /// A value the game names nowhere, as the save or the game holds it.
    Other(Unnamed),
}

/// A raw value of [`Relation`] that names none of its variants: only the set's own
/// constructors make one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Unnamed(u8);

impl Relation {
    /// The value of raw `value`: its variant, else `Other`.
    const fn of_raw(value: u8) -> Self {
        match value {
            0 => Self::Ally,
            1 => Self::Neutral,
            3 => Self::Enemy,
            4 => Self::Relation4,
            other => Self::Other(Unnamed(other)),
        }
    }

    /// Its raw value.
    const fn raw(self) -> u8 {
        match self {
            Self::Ally => 0,
            Self::Neutral => 1,
            Self::Enemy => 3,
            Self::Relation4 => 4,
            Self::Other(Unnamed(value)) => value,
        }
    }

    /// Its value.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.raw()
    }

    /// Whether the relation counts as allied (`FUN_141663dc0`): an ally always, relation 4 with
    /// the ally flag `with_4`.
    #[must_use]
    pub const fn allied(self, with_4: bool) -> bool {
        self.raw() == Self::Ally.raw() || (with_4 && self.raw() == Self::Relation4.raw())
    }
}

/// A player's relations toward every player (`relation[i]`, `player+0x260`), by the other
/// player: the diplomacy row the stances write and the queries read.
pub type Relations = ByPlayer<Relation>;

impl ByPlayer<Relation> {
    /// The row of the save's bytes, toward players 0, 1, … in their order: every byte is a
    /// relation.
    pub fn of_bytes(values: impl IntoIterator<Item = u8>) -> Self {
        values.into_iter().map(Relation::from).collect()
    }
}

/// The relations of a player the world does not hold: toward no player.
pub(crate) static NO_RELATIONS: Relations = ByPlayer::new(Vec::new());

/// A test's relation row (`player+0x260`) of saved bytes.
#[cfg(test)]
pub(crate) fn relation_row(values: impl IntoIterator<Item = u8>) -> Relations {
    Relations::of_bytes(values)
}

impl From<u8> for Relation {
    /// The relation a saved byte holds: every value is one.
    fn from(value: u8) -> Self {
        Self::of_raw(value)
    }
}

impl genie_diff::Diff for Relation {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::players::Relation;

    /// An ally counts as allied; relation 4 only with the flag; a neutral or an enemy never.
    #[test]
    fn an_ally_and_relation_4_with_the_flag_count_as_allied() {
        assert!(Relation::Ally.allied(false));
        assert!(!Relation::Relation4.allied(false));
        assert!(Relation::Relation4.allied(true));
        assert!(!Relation::Neutral.allied(true));
        assert!(!Relation::Enemy.allied(true));
        assert_eq!(Relation::from(3), Relation::Enemy);
        assert_eq!(Relation::default().get(), 0);
    }

    /// A saved row's bytes are the relations toward the players in their order.
    #[test]
    fn saved_row_is_the_relations_toward_each_player() {
        use crate::players::{PlayerId, Relations};
        let row = Relations::of_bytes([0, 3, 4]);
        assert_eq!(row.get(PlayerId::new(1)), Some(&Relation::Enemy));
        assert_eq!(row.get(PlayerId::new(2)), Some(&Relation::Relation4));
        assert_eq!(row.get(PlayerId::new(3)), None);
    }

    /// Each named value and one the set names nowhere go to their variant and back.
    #[test]
    fn every_relation_value_maps_to_its_variant_and_back() {
        for value in [0, 1, 3, 4, 2] {
            assert_eq!(Relation::of_raw(value).raw(), value);
        }
        assert!(matches!(Relation::of_raw(2), Relation::Other(_)));
    }
}
