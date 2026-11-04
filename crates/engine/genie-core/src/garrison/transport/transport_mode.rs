//! How a Transport was last ordered ([`TransportMode`]).

use crate::{GenieError, GenieResult};
use strum::FromRepr;

/// A Transport's `+0x34`: −1 from its constructor, 1 after a work order (slot 9), 0 after a
/// move (slot 8); the save's `target2_id`. The set is closed: the Transport's and the
/// `TransportOverWall`'s slots store only 0 (`FUN_140aa3e40`, `FUN_140aa7b70`) and 1
/// (`FUN_140aa4830`, `FUN_140aa83f0`) over the constructor's −1, and the save holds what they
/// wrote.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, FromRepr)]
#[repr(i32)]
pub enum TransportMode {
    /// −1: as made.
    New = -1,
    /// 0: after a move.
    #[default]
    Move = 0,
    /// 1: after a work order.
    Work = 1,
}

impl TransportMode {
    /// The int.
    #[must_use]
    pub const fn get(self) -> i32 {
        self as i32
    }
}

impl TryFrom<i32> for TransportMode {
    type Error = GenieError;

    /// The mode of the save's int `value`.
    ///
    /// # Errors
    ///
    /// An int no slot of the Transport writes.
    fn try_from(value: i32) -> GenieResult<Self> {
        Self::from_repr(value).ok_or_else(|| {
            GenieError::invalid_input("a transport mode no slot writes").with("target2_id", value)
        })
    }
}

impl genie_diff::Diff for TransportMode {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.get().diff(&other.get(), d);
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
    }

    /// Each named int goes to its variant and back; an int no slot writes is refused.
    #[test]
    fn every_transport_mode_int_maps_to_its_variant_and_back() {
        for value in [-1, 0, 1] {
            assert_eq!(
                TransportMode::try_from(value).map(TransportMode::get),
                Ok(value)
            );
        }
        let e = TransportMode::try_from(2).unwrap_err();
        assert!(e.to_string().contains("no slot writes"), "{e}");
    }
}
