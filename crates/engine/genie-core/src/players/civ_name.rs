//! A player's civilisation name override, which the civilisation name override's attribute sets.

use getset::CopyGetters;

use crate::math::conversions::cvttss2si;
use crate::players::Amount;

/// The name the interface gives a player's civilisation in place of its own (`+0x628`..`+0x640`,
/// not saved). The chat dialog (`GameScreen::command_chat`, `FUN_1414acf90`) and its test of an
/// override (`FUN_141472d00`) read it; nothing in the simulation does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CivNameOverride {
    /// No override: the interface names the civilisation itself. The constructor
    /// (`FUN_14163d330`) sets it: `+0x628` 0, `+0x62c` −2 and no text at `+0x630`.
    #[default]
    None,
    /// The string the store of the civilisation name override's attribute names
    /// ([`crate::world::Player::store_attribute`]): `+0x628` 1 and the string at `+0x62c`, the
    /// text cleared (`FUN_140930a50` with 0). The interface names that string, and the
    /// civilisation itself for −2 with no text.
    String(StringId),
}

/// A string of the game's string table by its id: any `int`, as an attribute's value truncated
/// gives one. `FUN_1407b1930` looks one up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, CopyGetters)]
pub struct StringId(
    /// Its value, as the player's `+0x62c` holds it.
    #[getset(get_copy = "pub")]
    i32,
);

impl StringId {
    /// The string an attribute's `amount` names: the amount truncated to an `int` (`CVTTSS2SI`;
    /// `i32::MIN` for a NaN or a value out of range).
    #[must_use]
    pub fn of_amount(amount: Amount) -> Self {
        Self(cvttss2si(amount.get()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An attribute's amount names its string truncated toward 0, as `CVTTSS2SI` does: a NaN
    /// and a value past an `int` name `i32::MIN`.
    #[test]
    fn amount_names_its_string_truncated() {
        assert_eq!(StringId::of_amount(Amount::new(10_234.9)).get(), 10_234);
        assert_eq!(StringId::of_amount(Amount::new(-2.7)).get(), -2);
        assert_eq!(StringId::of_amount(Amount::new(f32::NAN)).get(), i32::MIN);
        assert_eq!(StringId::of_amount(Amount::new(3.0e9)).get(), i32::MIN);
        assert_eq!(CivNameOverride::default(), CivNameOverride::None);
    }
}
