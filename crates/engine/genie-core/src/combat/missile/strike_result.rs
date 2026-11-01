//! What a missile's last hit did.

/// What a hit did (`+0x98` of a missile, its last hit's): none yet, a hit, or a hit a shield
/// blocked (`FUN_141084230`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StrikeResult(u8);

impl StrikeResult {
    /// No hit yet (0).
    pub const NONE: Self = Self(0);
    /// A hit (1).
    pub const HIT: Self = Self(1);
    /// A hit a shield blocked (2).
    pub const SHIELDED: Self = Self(2);

    /// The result of the save's byte `value`: every byte is one.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// The byte.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl genie_diff::Diff for StrikeResult {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::combat::missile::StrikeResult;

    /// The results are the game's bytes.
    #[test]
    fn strike_results_are_the_game_s_bytes() {
        assert_eq!(StrikeResult::default(), StrikeResult::NONE);
        assert_eq!(StrikeResult::of_data(2), StrikeResult::SHIELDED);
        assert_eq!(StrikeResult::HIT.get(), 1);
    }
}
