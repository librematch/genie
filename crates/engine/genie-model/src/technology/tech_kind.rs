//! A technology's type as one type: its record's `+0x3c`, the `.dat`'s `type`.

use serde::Deserialize;
use serde::Serialize;

/// A technology's type (its record's `+0x3c`, the `.dat`'s `type`): the tech tree's tests name
/// the shared research (0x20) and the type a disable command makes available instead (0x21);
/// every other value is a type the simulation does not tell apart.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TechKind(i16);

impl TechKind {
    /// The type whose research the buildings that may do it share (`FUN_1413d1020`).
    pub const SHARED: Self = Self(0x20);
    /// The type that a disable command makes available instead of disabled (`FUN_1413cdb00`).
    pub const REENABLED: Self = Self(0x21);

    /// Its value.
    #[must_use]
    pub const fn get(self) -> i16 {
        self.0
    }
}

impl From<i16> for TechKind {
    /// The type a `.dat` technology holds: every value is one.
    fn from(value: i16) -> Self {
        Self(value)
    }
}

#[cfg(test)]
mod tests {
    use crate::technology::TechKind;

    /// A `.dat` value is its type; the shared and the re-enabled types are 0x20 and 0x21.
    #[test]
    fn dat_value_is_its_kind() {
        assert_eq!(TechKind::from(0x20), TechKind::SHARED);
        assert_eq!(TechKind::REENABLED.get(), 0x21);
        assert_eq!(TechKind::default().get(), 0);
    }
}
