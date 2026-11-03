//! How a Transport was last ordered ([`TransportMode`]).

/// A Transport's `+0x34`: −1 from its constructor, 1 after a work order (slot 9), 0 after a
/// move (slot 8); the save's `target2_id`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TransportMode {
    /// −1: as made.
    New,
    /// 0: after a move.
    #[default]
    Move,
    /// 1: after a work order.
    Work,
    /// A value the game names nowhere, as the save or the game holds it.
    Other(Unnamed),
}

/// A raw value of [`TransportMode`] that names none of its variants: only the set's own
/// constructors make one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Unnamed(i32);

impl TransportMode {
    /// The value of raw `value`: its variant, else `Other`.
    const fn of_raw(value: i32) -> Self {
        match value {
            -1 => Self::New,
            0 => Self::Move,
            1 => Self::Work,
            other => Self::Other(Unnamed(other)),
        }
    }

    /// Its raw value.
    const fn raw(self) -> i32 {
        match self {
            Self::New => -1,
            Self::Move => 0,
            Self::Work => 1,
            Self::Other(Unnamed(value)) => value,
        }
    }

    /// The mode of the save's int `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self::of_raw(value)
    }

    /// The int.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.raw()
    }
}

impl genie_diff::Diff for TransportMode {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.raw().diff(&other.raw(), d);
    }
}

#[cfg(test)]
mod tests {
    use crate::garrison::transport::TransportMode;

    /// The three modes are the game's −1, 0 and 1.
    #[test]
    fn the_modes_are_the_game_ints() {
        assert_eq!(
            [TransportMode::New, TransportMode::Move, TransportMode::Work].map(TransportMode::get),
            [-1, 0, 1]
        );
        assert_eq!(TransportMode::of_data(1), TransportMode::Work);
    }

    /// Each named value and one the set names nowhere go to their variant and back.
    #[test]
    fn every_transport_mode_value_maps_to_its_variant_and_back() {
        for value in [-1, 0, 1, 2] {
            assert_eq!(TransportMode::of_raw(value).raw(), value);
        }
        assert!(matches!(TransportMode::of_raw(2), TransportMode::Other(_)));
    }
}
