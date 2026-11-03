//! The world's game state.

/// The world's game state (`+0x88`, the save's `tempGameState`): 0 while the game is played, 1
/// once it is over; in state 2 a death leaves no ghosts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PlayState {
    /// The game is played (0).
    #[default]
    Playing,
    /// The game is over (1).
    Over,
    /// A value the game names nowhere, as the save or the game holds it.
    Other(Unnamed),
}

/// A raw value of [`PlayState`] that names none of its variants: only the set's own
/// constructors make one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Unnamed(u32);

impl PlayState {
    /// The value of raw `value`: its variant, else `Other`.
    const fn of_raw(value: u32) -> Self {
        match value {
            0 => Self::Playing,
            1 => Self::Over,
            other => Self::Other(Unnamed(other)),
        }
    }

    /// Its raw value.
    const fn raw(self) -> u32 {
        match self {
            Self::Playing => 0,
            Self::Over => 1,
            Self::Other(Unnamed(value)) => value,
        }
    }

    /// The state in which a death leaves no ghosts (2).
    const NO_GHOSTS: u32 = 2;

    /// The state of the save's word `value`.
    #[must_use]
    pub const fn of_data(value: u32) -> Self {
        Self::of_raw(value)
    }

    /// The word.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.raw()
    }

    /// Whether a death leaves its ghosts (any state but 2: `FUN_14118e990`'s callers).
    #[must_use]
    pub const fn leaves_ghosts(self) -> bool {
        self.raw() != Self::NO_GHOSTS
    }
}

impl genie_diff::Diff for PlayState {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.raw().diff(&other.raw(), d);
    }
}

#[cfg(test)]
mod tests {
    use crate::world::PlayState;

    /// Only state 2 keeps a death's ghosts away.
    #[test]
    fn only_state_2_leaves_no_ghosts() {
        assert!(PlayState::Playing.leaves_ghosts() && PlayState::Over.leaves_ghosts());
        assert!(!PlayState::of_data(2).leaves_ghosts());
        assert_eq!(PlayState::default(), PlayState::Playing);
    }

    /// Each named value and one the set names nowhere go to their variant and back.
    #[test]
    fn every_play_state_value_maps_to_its_variant_and_back() {
        for value in [0, 1, 2] {
            assert_eq!(PlayState::of_raw(value).raw(), value);
        }
        assert!(matches!(PlayState::of_raw(2), PlayState::Other(_)));
    }
}
