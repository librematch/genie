//! The world's game state.

/// The world's game state (`+0x88`, the save's `tempGameState`): 0 while the game is played, 1
/// once it is over, 2 while the world is cleared or loaded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PlayState {
    /// The game is played (0).
    #[default]
    Playing,
    /// The game is over (1).
    Over,
    /// The world is cleared (2): its clear (`BaseWorld::slot9`, `FUN_14152f270`) and the start of
    /// its load (`BaseWorld::load_world`) set it. A death leaves no ghosts, and a relic leaving a
    /// building takes back no technology (`FUN_1411d6ad0`).
    Clearing,
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
            2 => Self::Clearing,
            other => Self::Other(Unnamed(other)),
        }
    }

    /// Its raw value.
    const fn raw(self) -> u32 {
        match self {
            Self::Playing => 0,
            Self::Over => 1,
            Self::Clearing => 2,
            Self::Other(Unnamed(value)) => value,
        }
    }

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

    /// Whether a death leaves its ghosts (any state but [`PlayState::Clearing`]:
    /// `FUN_14118e990`'s callers).
    #[must_use]
    pub const fn leaves_ghosts(self) -> bool {
        !matches!(self, Self::Clearing)
    }
}

impl genie_diff::Diff for PlayState {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.raw().diff(&other.raw(), differ);
    }
}

#[cfg(test)]
mod tests {
    use crate::world::PlayState;

    /// Only state 2 keeps a death's ghosts away.
    #[test]
    fn only_state_2_leaves_no_ghosts() {
        assert!(PlayState::Playing.leaves_ghosts() && PlayState::Over.leaves_ghosts());
        assert!(PlayState::of_data(3).leaves_ghosts());
        assert!(!PlayState::of_data(2).leaves_ghosts());
        assert_eq!(PlayState::default(), PlayState::Playing);
    }

    /// Each named value and one the set names nowhere go to their variant and back.
    #[test]
    fn every_play_state_value_maps_to_its_variant_and_back() {
        for value in [0, 1, 2, 3] {
            assert_eq!(PlayState::of_raw(value).raw(), value);
        }
        assert_eq!(PlayState::of_raw(2), PlayState::Clearing);
        assert!(matches!(PlayState::of_raw(3), PlayState::Other(_)));
    }
}
