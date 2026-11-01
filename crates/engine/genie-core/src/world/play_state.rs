//! The world's game state.

/// The world's game state (`+0x88`, the save's `tempGameState`): 0 while the game is played, 1
/// once it is over; in state 2 a death leaves no ghosts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayState(u32);

impl PlayState {
    /// The game is played (0).
    pub const PLAYING: Self = Self(0);
    /// The game is over (1).
    pub const OVER: Self = Self(1);
    /// The state in which a death leaves no ghosts (2).
    const NO_GHOSTS: u32 = 2;

    /// The state of the save's word `value`.
    #[must_use]
    pub const fn of_data(value: u32) -> Self {
        Self(value)
    }

    /// The word.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// Whether a death leaves its ghosts (any state but 2: `FUN_14118e990`'s callers).
    #[must_use]
    pub const fn leaves_ghosts(self) -> bool {
        self.0 != Self::NO_GHOSTS
    }
}

impl genie_diff::Diff for PlayState {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::world::PlayState;

    /// Only state 2 keeps a death's ghosts away.
    #[test]
    fn only_state_2_leaves_no_ghosts() {
        assert!(PlayState::PLAYING.leaves_ghosts() && PlayState::OVER.leaves_ghosts());
        assert!(!PlayState::of_data(2).leaves_ghosts());
        assert_eq!(PlayState::default(), PlayState::PLAYING);
    }
}
