//! A player's culture: its civilisation's icon set, which a building's counts of the relics and
//! the livestock inside it test.

/// A player's `culture` (`+0x88`, saved): its civilisation's icon set (the `.dat` civilisation's
/// `+0x40`, which the player's constructor `FUN_14163d330` copies). A building of a player of a
/// culture (any but 0) counts the relics and the livestock that go in and out of it
/// (`FUN_1411cb850`, `FUN_1411d6ad0`); every civilisation of build 158041's `.dat` has one (1 to
/// 11).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Culture(u8);

impl Culture {
    /// None (0).
    pub const NONE: Self = Self(0);

    /// The culture of the save's byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// Whether the player has a culture (not 0): its buildings count their relics and livestock.
    #[must_use]
    pub const fn is_set(self) -> bool {
        self.0 != Self::NONE.0
    }

    /// The byte, as the save holds it.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

#[cfg(test)]
mod tests;
