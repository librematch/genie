//! The arguments of the technology commands: how a cost or a research time changes
//! ([`ChangeMode`]), what kind 8 changes ([`ModifyMode`]) and at which location
//! ([`ModifyLocation`]), and the technology a disable names ([`DisabledTech`]).

use crate::technology::TechId;
use serde::Deserialize;
use serde::Serialize;

/// How a technology's cost or research time changes (`'e'`'s and `'g'`'s third argument's low
/// byte, or kind 8's mode through its map): 0 sets, 2 scales by the amount less 1 (rounded half
/// away from 1), any other adds. Every byte is kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(from = "u8", into = "u8")]
pub enum ChangeMode {
    /// 0: the value becomes the amount.
    #[default]
    Set,
    /// 1: the amount adds to the value (as every byte not named here).
    Add,
    /// 2: the value grows by the amount less 1 times the `.dat`'s value.
    Scale,
    /// A value the game names nowhere, as the save or the game holds it.
    Other(UnnamedChangeMode),
}

/// A raw value of [`ChangeMode`] that names none of its variants: only the set's own
/// constructors make one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UnnamedChangeMode(u8);

impl ChangeMode {
    /// The value of raw `value`: its variant, else `Other`.
    const fn of_raw(value: u8) -> Self {
        match value {
            0 => Self::Set,
            1 => Self::Add,
            2 => Self::Scale,
            other => Self::Other(UnnamedChangeMode(other)),
        }
    }

    /// Its raw value.
    const fn raw(self) -> u8 {
        match self {
            Self::Set => 0,
            Self::Add => 1,
            Self::Scale => 2,
            Self::Other(UnnamedChangeMode(value)) => value,
        }
    }

    /// The mode of the command's byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self::of_raw(value)
    }

    /// The byte, as the `.dat` holds it.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.raw()
    }
}

impl From<u8> for ChangeMode {
    /// The value of a serialized raw `value`.
    fn from(value: u8) -> Self {
        Self::of_raw(value)
    }
}

impl From<ChangeMode> for u8 {
    /// The raw value it serializes as.
    fn from(value: ChangeMode) -> Self {
        value.raw()
    }
}

/// What a technology change (kind 8, `FUN_1415c3f20`'s `0x6f` case) changes: its second
/// argument. The cost modes (0 to 3, `0x4000` to `0x4003`, `0xd` to `0x11`) and the research
/// time modes (−1 to −3) map to a [`ChangeMode`]; the others name a field. Every short is kept.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(from = "i16", into = "i16")]
pub enum ModifyMode {
    /// 4: the location's building.
    Building,
    /// 5: the location's button.
    Button,
    /// 9: the stacking.
    Stacking,
    /// 10: the research cap.
    ResearchCap,
    /// 12: the state.
    State,
    /// 18: the effect override.
    EffectOverride,
    /// A value the game names nowhere, as the save or the game holds it.
    Other(UnnamedModifyMode),
}

/// A raw value of [`ModifyMode`] that names none of its variants: only the set's own
/// constructors make one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UnnamedModifyMode(i16);

impl ModifyMode {
    /// The value of raw `value`: its variant, else `Other`.
    const fn of_raw(value: i16) -> Self {
        match value {
            4 => Self::Building,
            5 => Self::Button,
            9 => Self::Stacking,
            10 => Self::ResearchCap,
            12 => Self::State,
            18 => Self::EffectOverride,
            other => Self::Other(UnnamedModifyMode(other)),
        }
    }

    /// Its raw value.
    const fn raw(self) -> i16 {
        match self {
            Self::Building => 4,
            Self::Button => 5,
            Self::Stacking => 9,
            Self::ResearchCap => 10,
            Self::State => 12,
            Self::EffectOverride => 18,
            Self::Other(UnnamedModifyMode(value)) => value,
        }
    }

    /// The mode of the command's short `value`.
    #[must_use]
    pub const fn of_data(value: i16) -> Self {
        Self::of_raw(value)
    }

    /// The short, as the `.dat` holds it.
    #[must_use]
    pub const fn get(self) -> i16 {
        self.raw()
    }
}

impl Default for ModifyMode {
    /// The zero value, as zeroed memory holds it.
    fn default() -> Self {
        Self::of_raw(0)
    }
}

impl From<i16> for ModifyMode {
    /// The value of a serialized raw `value`.
    fn from(value: i16) -> Self {
        Self::of_raw(value)
    }
}

impl From<ModifyMode> for i16 {
    /// The raw value it serializes as.
    fn from(value: ModifyMode) -> Self {
        value.raw()
    }
}

/// The location a technology change (kind 8) changes: its third argument, below 1 every
/// location, else the entry at that index (past the technology's locations a new one, a copy of
/// the last with no building). Every short is kept.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord,
)]
pub struct ModifyLocation(i16);

impl ModifyLocation {
    /// The location of the command's short `value`.
    #[must_use]
    pub const fn of_data(value: i16) -> Self {
        Self(value)
    }

    /// The entry it names: none below 1 (every location).
    #[must_use]
    pub fn entry(self) -> Option<usize> {
        usize::try_from(self.0).ok().filter(|&i| 0 < i)
    }

    /// The short, as the `.dat` holds it.
    #[must_use]
    pub const fn get(self) -> i16 {
        self.0
    }
}

/// The technology a disable command (`'f'`) names: its amount as CVTTSS2SI gives it. −1 disables
/// none; the shared kind's test (`FUN_1413d1020`) reads the whole number, the disabling
/// (`FUN_1413cdb00`) its low 16 bits, a negative one every technology.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct DisabledTech(i32);

impl DisabledTech {
    /// −1: no technology.
    pub const NONE: Self = Self(-1);

    /// The technology of the CVTTSS2SI number `value`.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// The technology the whole number names: none below 0 or past a short.
    #[must_use]
    pub fn whole(self) -> Option<TechId> {
        i16::try_from(self.0).ok().and_then(TechId::of_data)
    }

    /// The low 16 bits, which the disabling reads.
    #[must_use]
    pub const fn short(self) -> i16 {
        self.0 as i16
    }

    /// The number, as CVTTSS2SI gives it.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use crate::technology::TechId;
    use crate::technology::command_args::{ChangeMode, DisabledTech, ModifyLocation, ModifyMode};

    /// The modes keep their values; a location below 1 names no entry; a disable's whole number
    /// and its low 16 bits differ past a short.
    #[test]
    fn the_command_arguments_keep_the_data_values() {
        assert_eq!(ChangeMode::of_data(2), ChangeMode::Scale);
        assert_eq!(ChangeMode::of_data(7).get(), 7);
        assert_eq!(ModifyMode::of_data(12), ModifyMode::State);
        assert_eq!(ModifyMode::of_data(-2).get(), -2);
        assert_eq!(
            [0, -3, 2].map(|l| ModifyLocation::of_data(l).entry()),
            [None, None, Some(2)]
        );
        let far = DisabledTech::of_data(65538);
        assert_eq!((far.whole(), far.short()), (None, 2));
        assert_eq!(DisabledTech::of_data(7).whole(), TechId::of_data(7));
        assert_eq!(DisabledTech::NONE.whole(), None);
        assert_eq!(DisabledTech::NONE.get(), -1);
    }

    /// Each named value and one the set names nowhere go to their variant and back.
    #[test]
    fn every_change_mode_value_maps_to_its_variant_and_back() {
        for value in [0, 1, 2, 3] {
            assert_eq!(ChangeMode::of_raw(value).raw(), value);
        }
        assert!(matches!(ChangeMode::of_raw(3), ChangeMode::Other(_)));
    }

    /// Each named value and one the set names nowhere go to their variant and back.
    #[test]
    fn every_modify_mode_value_maps_to_its_variant_and_back() {
        for value in [4, 5, 9, 10, 12, 18, 0] {
            assert_eq!(ModifyMode::of_raw(value).raw(), value);
        }
        assert!(matches!(ModifyMode::of_raw(0), ModifyMode::Other(_)));
    }
}
