//! A train location's hotkey.

use serde::Deserialize;
use serde::Serialize;

/// A train location's hotkey (`hotkeyID`, `+0x8`, an int of the `.dat`), which no modelled
/// routine reads. Every int is kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct HotkeyId(i32);

impl HotkeyId {
    /// The hotkey of the data's int `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// The int, as the data and the save hold it.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

impl genie_diff::Diff for HotkeyId {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::production::HotkeyId;

    /// The hotkey keeps the data's int.
    #[test]
    fn the_hotkey_keeps_its_int() {
        assert_eq!(HotkeyId::of_data(-1).get(), -1);
        assert_eq!(HotkeyId::default(), HotkeyId::of_data(0));
    }
}
