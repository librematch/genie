//! How a Transport was last ordered ([`TransportMode`]).

/// A Transport's `+0x34`: −1 from its constructor, 1 after a work order (slot 9), 0 after a
/// move (slot 8); the save's `target2_id`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TransportMode(i32);

impl TransportMode {
    /// −1: as made.
    pub const NEW: Self = Self(-1);
    /// 0: after a move.
    pub const MOVE: Self = Self(0);
    /// 1: after a work order.
    pub const WORK: Self = Self(1);

    /// The mode of the save's int `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// The int.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

impl genie_diff::Diff for TransportMode {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::garrison::transport::TransportMode;

    /// The three modes are the game's −1, 0 and 1.
    #[test]
    fn the_modes_are_the_game_ints() {
        assert_eq!(
            [TransportMode::NEW, TransportMode::MOVE, TransportMode::WORK].map(TransportMode::get),
            [-1, 0, 1]
        );
        assert_eq!(TransportMode::of_data(1), TransportMode::WORK);
    }
}
