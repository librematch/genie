//! How another player takes an object over.

/// An object's `capture_flag` (`+0x924`), which a combat object's setup sets
/// ([`crate::vision::visibility::capture_kind`]): none (0), taken over by sight (1), taken more
/// than once (2) or livestock (3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CaptureKind {
    /// 0: no player takes it over.
    #[default]
    None,
    /// 1: another player takes it over by seeing it.
    BySight,
    /// 2: another player can take it more than once ("an object which can be captured
    /// multiple times").
    Again,
    /// 3: livestock (classes 58 and 1).
    Livestock,
    /// A value the game names nowhere, as the save or the game holds it.
    Other(Unnamed),
}

/// A raw value of [`CaptureKind`] that names none of its variants: only the set's own
/// constructors make one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Unnamed(u8);

impl CaptureKind {
    /// The value of raw `value`: its variant, else `Other`.
    const fn of_raw(value: u8) -> Self {
        match value {
            0 => Self::None,
            1 => Self::BySight,
            2 => Self::Again,
            3 => Self::Livestock,
            other => Self::Other(Unnamed(other)),
        }
    }

    /// Its raw value.
    const fn raw(self) -> u8 {
        match self {
            Self::None => 0,
            Self::BySight => 1,
            Self::Again => 2,
            Self::Livestock => 3,
            Self::Other(Unnamed(value)) => value,
        }
    }

    /// The kind of the save's byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self::of_raw(value)
    }

    /// The byte.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.raw()
    }

    /// Whether another player takes it over at all (not 0).
    #[must_use]
    pub const fn any(self) -> bool {
        self.raw() != 0
    }
}

impl genie_diff::Diff for CaptureKind {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.raw().diff(&other.raw(), d);
    }
}

#[cfg(test)]
mod tests {
    use crate::ownership::CaptureKind;

    /// Only 0 is no capture; the byte reads back.
    #[test]
    fn only_zero_is_no_capture() {
        assert!(!CaptureKind::None.any());
        for k in [
            CaptureKind::BySight,
            CaptureKind::Again,
            CaptureKind::Livestock,
        ] {
            assert!(k.any());
        }
        assert_eq!(CaptureKind::of_data(5).get(), 5);
        assert_eq!(CaptureKind::default(), CaptureKind::None);
    }

    /// Each named value and one the set names nowhere go to their variant and back.
    #[test]
    fn every_capture_kind_value_maps_to_its_variant_and_back() {
        for value in [0, 1, 2, 3, 4] {
            assert_eq!(CaptureKind::of_raw(value).raw(), value);
        }
        assert!(matches!(CaptureKind::of_raw(4), CaptureKind::Other(_)));
    }
}
