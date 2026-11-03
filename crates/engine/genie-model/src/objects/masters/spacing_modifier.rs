//! A master's spacing in a group's formation.

use serde::Deserialize;
use serde::Serialize;

/// A master's spacing modifier (`+0x8b8`, `mSpacingModifier`; the `.dat`'s "flank attack
/// modifier"): with its radius on x the unit's radius in a group's formation (`FUN_141025920`).
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SpacingModifier(f32);

impl SpacingModifier {
    /// The modifier of the data's float `value`.
    #[must_use]
    pub const fn new(value: f32) -> Self {
        Self(value)
    }

    /// The float.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl genie_diff::Diff for SpacingModifier {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::SpacingModifier;

    /// The modifier keeps the data's float.
    #[test]
    fn the_spacing_modifier_keeps_its_float() {
        assert_eq!(SpacingModifier::new(1.5).get(), 1.5);
        assert_eq!(SpacingModifier::default().get(), 0.0);
    }
}
