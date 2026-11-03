//! What a missile's last hit did.

/// What a hit did (`+0x98` of a missile, its last hit's): none yet, a hit, or a hit a shield
/// blocked (`FUN_141084230`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StrikeResult {
    /// No hit yet (0).
    #[default]
    None,
    /// A hit (1).
    Hit,
    /// A hit a shield blocked (2).
    Shielded,
    /// A value the game names nowhere, as the save or the game holds it.
    Other(Unnamed),
}

/// A raw value of [`StrikeResult`] that names none of its variants: only the set's own
/// constructors make one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Unnamed(u8);

impl StrikeResult {
    /// The value of raw `value`: its variant, else `Other`.
    const fn of_raw(value: u8) -> Self {
        match value {
            0 => Self::None,
            1 => Self::Hit,
            2 => Self::Shielded,
            other => Self::Other(Unnamed(other)),
        }
    }

    /// Its raw value.
    const fn raw(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Hit => 1,
            Self::Shielded => 2,
            Self::Other(Unnamed(value)) => value,
        }
    }

    /// The result of the save's byte `value`: every byte is one.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self::of_raw(value)
    }

    /// The byte.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.raw()
    }
}

impl genie_diff::Diff for StrikeResult {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.raw().diff(&other.raw(), d);
    }
}

#[cfg(test)]
mod tests {
    use crate::combat::missile::StrikeResult;

    /// The results are the game's bytes.
    #[test]
    fn strike_results_are_the_game_bytes() {
        assert_eq!(StrikeResult::default(), StrikeResult::None);
        assert_eq!(StrikeResult::of_data(2), StrikeResult::Shielded);
        assert_eq!(StrikeResult::Hit.get(), 1);
    }

    /// Each named value and one the set names nowhere go to their variant and back.
    #[test]
    fn every_strike_result_value_maps_to_its_variant_and_back() {
        for value in [0, 1, 2, 3] {
            assert_eq!(StrikeResult::of_raw(value).raw(), value);
        }
        assert!(matches!(StrikeResult::of_raw(3), StrikeResult::Other(_)));
    }
}
