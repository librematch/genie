//! A group's unit type: a master's id as the statistics' and the slots' words hold it.

use crate::objects::masters::MasterId;

/// A unit type as a group holds it (`mUnitTypes`, `mDesiredSubCategory`): a master's id in a
/// word, −1 for none. The statistic's 50 words are the shared scratch array's
/// ([`crate::groups::UnitGroups::unit_kinds_scratch`]), so the words past its types are an
/// earlier count's types, or the 0 the array starts with; every word is kept as it is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UnitKind(i32);

impl UnitKind {
    /// None (−1): a slot that takes any unit type.
    pub const NONE: Self = Self(-1);
    /// The word the scratch array starts with (0).
    pub const CLEARED: Self = Self(0);

    /// The unit type of master `master` (its id widened to the word).
    #[must_use]
    pub fn of(master: MasterId) -> Self {
        Self(i32::from(master.get()))
    }

    /// The unit type the save's word `word` holds.
    #[must_use]
    pub const fn of_word(word: u32) -> Self {
        Self(word.cast_signed())
    }

    /// The id as a signed word, as a slot's save holds it (`mDesiredSubCategory`).
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }

    /// Its word, as a statistic's save holds it (`mUnitTypes`).
    #[must_use]
    pub const fn word(self) -> u32 {
        self.0.cast_unsigned()
    }

    /// Whether a slot of it takes any unit type (−1).
    #[must_use]
    pub const fn is_none(self) -> bool {
        self.0 == Self::NONE.0
    }
}

#[cfg(test)]
mod tests {
    use crate::groups::UnitKind;
    use crate::objects::masters::MasterId;

    /// A master's id widens to the word and back; −1 is all bits.
    #[test]
    fn unit_kind_is_master_id_in_word() {
        let archer = MasterId::of_data(4).unwrap();
        assert_eq!(UnitKind::of(archer).word(), 4);
        assert_eq!(UnitKind::of_word(4), UnitKind::of(archer));
        assert_eq!(
            (UnitKind::NONE.word(), UnitKind::NONE.get()),
            (u32::MAX, -1)
        );
        assert!(UnitKind::of_word(u32::MAX).is_none());
        assert!(!UnitKind::CLEARED.is_none());
        assert_eq!(UnitKind::default(), UnitKind::CLEARED);
    }
}
