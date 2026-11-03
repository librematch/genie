//! What a missile's last hit did.

use crate::GenieError;

/// What a hit did (`+0x98` of a missile, its last hit's): none yet, a hit, or a hit a shield
/// blocked (`FUN_141084230`). The set is closed: the missile action (`FUN_140a9e6e0`) stores the
/// byte its target's hit (slot 0x860: `FUN_141084230`, `FUN_1412115e0`) or blast (slot 0x848,
/// `FUN_14107f690`) answers, and those answer 0, 1 or 2; the save holds what they wrote.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum StrikeResult {
    /// No hit yet (0).
    #[default]
    None = 0,
    /// A hit (1).
    Hit = 1,
    /// A hit a shield blocked (2).
    Shielded = 2,
}

impl StrikeResult {
    /// The byte.
    #[must_use]
    pub const fn get(self) -> u8 {
        self as u8
    }
}

impl TryFrom<u8> for StrikeResult {
    type Error = GenieError;

    /// The result of the save's byte `value`.
    ///
    /// # Errors
    ///
    /// A byte no hit answers.
    fn try_from(value: u8) -> Result<Self, GenieError> {
        match value {
            0 => Ok(Self::None),
            1 => Ok(Self::Hit),
            2 => Ok(Self::Shielded),
            _ => {
                Err(GenieError::invalid_input("a strike result no hit answers")
                    .with("result", value))
            }
        }
    }
}

impl genie_diff::Diff for StrikeResult {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.get().diff(&other.get(), d);
    }
}

#[cfg(test)]
mod tests {
    use crate::combat::missile::StrikeResult;

    /// The results are the game's bytes.
    #[test]
    fn strike_results_are_the_game_bytes() {
        assert_eq!(StrikeResult::default(), StrikeResult::None);
        assert_eq!(StrikeResult::Hit.get(), 1);
    }

    /// Each named byte goes to its variant and back; a byte no hit answers is refused.
    #[test]
    fn every_strike_result_byte_maps_to_its_variant_and_back() {
        for value in [0, 1, 2] {
            assert_eq!(
                StrikeResult::try_from(value).map(StrikeResult::get),
                Ok(value)
            );
        }
        let e = StrikeResult::try_from(3).unwrap_err();
        assert!(e.to_string().contains("no hit answers"), "{e}");
    }
}
