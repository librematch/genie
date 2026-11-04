//! The values of a player's AI settings ([`crate::world::AiSettings`]).

/// A whole per cent a setting holds: the easiest and the easier reaction percentages and the
/// targeting factor, which the target scores take times 0.01.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Percent(i32);

impl Percent {
    /// The per cent of the int `value`.
    #[must_use]
    pub const fn new(value: i32) -> Self {
        Self(value)
    }

    /// The int.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }

    /// The float the scores multiply.
    #[must_use]
    pub const fn as_f32(self) -> f32 {
        self.0 as f32
    }
}

/// A player's local targeting mode (`local_targeting_mode`, setting 9): which target scores
/// its targeting factor scales. Mode 1 scales the kill-time score's top, mode 2 an armored
/// entry's total; another mode scales none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TargetingMode {
    /// Mode 1: the kill-time score's top.
    KillTime,
    /// Mode 2: the total of an entry whose shown armor is at least 100.
    Armored,
    /// A value the game names nowhere, as the save or the game holds it.
    Other(Unnamed),
}

/// A raw value of [`TargetingMode`] that names none of its variants: only the set's own
/// constructors make one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Unnamed(i32);

impl TargetingMode {
    /// The value of raw `value`: its variant, else `Other`.
    const fn of_raw(value: i32) -> Self {
        match value {
            1 => Self::KillTime,
            2 => Self::Armored,
            other => Self::Other(Unnamed(other)),
        }
    }

    /// Its raw value.
    const fn raw(self) -> i32 {
        match self {
            Self::KillTime => 1,
            Self::Armored => 2,
            Self::Other(Unnamed(value)) => value,
        }
    }

    /// The mode of the int `value`: every int is one.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self::of_raw(value)
    }

    /// The int.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.raw()
    }
}

impl Default for TargetingMode {
    /// The zero value, as zeroed memory holds it.
    fn default() -> Self {
        Self::of_raw(0)
    }
}

/// A player's boar lure destination (`boarLureDestination`, setting 0xc): the byte its AI's
/// script sets, which no modelled routine reads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LureDestination(u8);

impl LureDestination {
    /// The destination of the byte `value`: every byte is one.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// The byte.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// A player's villager garrison setting (`mDisableVillagerGarrison`, setting 0x11): the int its
/// AI's script sets, which no modelled routine reads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VillagerGarrison(i32);

impl VillagerGarrison {
    /// The setting of the int `value`: every int is one.
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// The int.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

impl genie_diff::Diff for Percent {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

impl genie_diff::Diff for TargetingMode {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.raw().diff(&other.raw(), differ);
    }
}

impl genie_diff::Diff for LureDestination {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

impl genie_diff::Diff for VillagerGarrison {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

#[cfg(test)]
mod tests {
    use crate::world::{LureDestination, Percent, TargetingMode, VillagerGarrison};

    /// The settings keep their values; a per cent is its int as a float.
    #[test]
    fn the_ai_settings_keep_their_values() {
        assert_eq!(Percent::new(150).as_f32(), 150.0);
        assert!(Percent::new(5) < Percent::new(6));
        assert_eq!(TargetingMode::of_data(2), TargetingMode::Armored);
        assert_ne!(TargetingMode::default(), TargetingMode::KillTime);
        assert_eq!(LureDestination::of_data(3).get(), 3);
        assert_eq!(VillagerGarrison::of_data(-1).get(), -1);
    }

    /// Each named value and one the set names nowhere go to their variant and back.
    #[test]
    fn every_targeting_mode_value_maps_to_its_variant_and_back() {
        for value in [1, 2, 0] {
            assert_eq!(TargetingMode::of_raw(value).raw(), value);
        }
        assert!(matches!(TargetingMode::of_raw(0), TargetingMode::Other(_)));
    }
}
