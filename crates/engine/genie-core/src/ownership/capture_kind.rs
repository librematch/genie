//! How another player takes an object over.

use crate::{GenieError, GenieResult};
use strum::FromRepr;

/// An object's `capture_flag` (`+0x924`), which a combat object's setup sets
/// ([`crate::vision::visibility::capture_kind`]): none (0), taken over by sight (1), taken more
/// than once (2) or livestock (3). The set is closed: the game's byte stores write 0 (the
/// constructor `FUN_141072ad0`, `FUN_14107dfb0`, `FUN_14108bf30`), 1 and 3 (the setup,
/// `FUN_14121df60`), and the save holds what they wrote.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, FromRepr)]
#[repr(u8)]
pub enum CaptureKind {
    /// 0: no player takes it over.
    #[default]
    None = 0,
    /// 1: another player takes it over by seeing it.
    BySight = 1,
    /// 2: another player can take it more than once ("an object which can be captured
    /// multiple times"); no store of the build writes it.
    Again = 2,
    /// 3: livestock (classes 58 and 1).
    Livestock = 3,
}

impl CaptureKind {
    /// The byte.
    #[must_use]
    pub const fn get(self) -> u8 {
        self as u8
    }

    /// Whether another player takes it over at all (not 0).
    #[must_use]
    pub const fn any(self) -> bool {
        !matches!(self, Self::None)
    }
}

impl TryFrom<u8> for CaptureKind {
    type Error = GenieError;

    /// The kind of the save's byte `value`.
    ///
    /// # Errors
    ///
    /// A byte no store of the game writes.
    fn try_from(value: u8) -> GenieResult<Self> {
        Self::from_repr(value).ok_or_else(|| {
            GenieError::invalid_input("a capture flag no store writes").with("capture_flag", value)
        })
    }
}

impl genie_diff::Diff for CaptureKind {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.get().diff(&other.get(), differ);
    }
}

#[cfg(test)]
mod tests {
    use crate::ownership::CaptureKind;

    /// Only 0 is no capture; the byte reads back.
    #[test]
    fn only_zero_is_no_capture() {
        assert!(!CaptureKind::None.any());
        for capture_kind in [
            CaptureKind::BySight,
            CaptureKind::Again,
            CaptureKind::Livestock,
        ] {
            assert!(capture_kind.any());
        }
        assert_eq!(CaptureKind::default(), CaptureKind::None);
    }

    /// Each named byte goes to its variant and back; a byte no store writes is refused.
    #[test]
    fn every_capture_kind_byte_maps_to_its_variant_and_back() {
        for value in [0, 1, 2, 3] {
            assert_eq!(
                CaptureKind::try_from(value).map(CaptureKind::get),
                Ok(value)
            );
        }
        for value in [4, 0xff] {
            let error = CaptureKind::try_from(value).unwrap_err();
            assert!(error.to_string().contains("no store writes"), "{error}");
        }
    }
}
