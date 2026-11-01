//! How another player takes an object over.

/// An object's `capture_flag` (`+0x924`), which a combat object's setup sets
/// ([`crate::vision::visibility::capture_kind`]): none (0), taken over by sight (1), taken more
/// than once (2) or livestock (3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct CaptureKind(u8);

impl CaptureKind {
    /// 0: no player takes it over.
    pub const NONE: Self = Self(0);
    /// 1: another player takes it over by seeing it.
    pub const BY_SIGHT: Self = Self(1);
    /// 2: another player can take it more than once ("an object which can be captured
    /// multiple times").
    pub const AGAIN: Self = Self(2);
    /// 3: livestock (classes 58 and 1).
    pub const LIVESTOCK: Self = Self(3);

    /// The kind of the save's byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// The byte.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// Whether another player takes it over at all (not 0).
    #[must_use]
    pub const fn any(self) -> bool {
        self.0 != 0
    }
}

impl genie_diff::Diff for CaptureKind {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::ownership::CaptureKind;

    /// Only 0 is no capture; the byte reads back.
    #[test]
    fn only_zero_is_no_capture() {
        assert!(!CaptureKind::NONE.any());
        for k in [
            CaptureKind::BY_SIGHT,
            CaptureKind::AGAIN,
            CaptureKind::LIVESTOCK,
        ] {
            assert!(k.any());
        }
        assert_eq!(CaptureKind::of_data(5).get(), 5);
        assert_eq!(CaptureKind::default(), CaptureKind::NONE);
    }
}
