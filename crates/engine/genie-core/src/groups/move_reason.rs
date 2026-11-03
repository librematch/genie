//! Why a group member moves on its own (`mReason`), and the path of that move (`mPathID`).

/// Why a group member moves on its own (`mReason`, read through `FUN_141017590`): −1 for no own
/// move. Every value is kept; the ones the game sets and reads are named.
/// The default is 0, as a zeroed member holds it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MoveReason {
    /// −1: no own move.
    None,
    /// 0: the way around a blocker the steering recorded; its walker leaves buildings, walls and
    /// gates alone.
    #[default]
    Detour,
    /// 2: moved aside after too many failed moves to its slot.
    Aside,
    /// A value the game names nowhere, as the save or the game holds it.
    Other(Unnamed),
}

/// A raw value of [`MoveReason`] that names none of its variants: only the set's own
/// constructors make one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Unnamed(i32);

impl MoveReason {
    /// The value of raw `value`: its variant, else `Other`.
    const fn of_raw(value: i32) -> Self {
        match value {
            -1 => Self::None,
            0 => Self::Detour,
            2 => Self::Aside,
            other => Self::Other(Unnamed(other)),
        }
    }

    /// Its raw value.
    const fn raw(self) -> i32 {
        match self {
            Self::None => -1,
            Self::Detour => 0,
            Self::Aside => 2,
            Self::Other(Unnamed(value)) => value,
        }
    }

    /// The reason of the save's value `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self::of_raw(value)
    }

    /// The value, as the save holds it.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.raw()
    }
}

/// A group member's own move's path (`mPathID`), −1 for none: the two callers of the move's
/// setter (`FUN_141039f60`) give −1, and nothing reads it.
/// The default is 0, as a zeroed member holds it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct MovePath(i32);

impl MovePath {
    /// −1: no path.
    pub const NONE: Self = Self(-1);

    /// The path of the save's value `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// The value, as the save holds it.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use crate::groups::{MovePath, MoveReason};

    /// Both default to a zeroed member's 0 and keep any value.
    #[test]
    fn reason_and_path_keep_the_save_value() {
        assert_eq!(MoveReason::default(), MoveReason::Detour);
        assert_eq!(MoveReason::of_data(2), MoveReason::Aside);
        assert_eq!(MoveReason::of_data(5).get(), 5);
        assert_eq!((MovePath::default().get(), MovePath::NONE.get()), (0, -1));
        assert_eq!(MovePath::of_data(7).get(), 7);
    }

    /// Each named value and one the set names nowhere go to their variant and back.
    #[test]
    fn every_move_reason_value_maps_to_its_variant_and_back() {
        for value in [-1, 0, 2, 1] {
            assert_eq!(MoveReason::of_raw(value).raw(), value);
        }
        assert!(matches!(MoveReason::of_raw(1), MoveReason::Other(_)));
    }
}
