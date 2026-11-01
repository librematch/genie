//! The values of a player's AI settings ([`crate::world::AiSettings`]).

/// A whole per cent a setting holds: the easiest and the easier reaction percentages and the
/// targeting factor, which the target scores take times 0.01.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TargetingMode(i32);

impl TargetingMode {
    /// Mode 1: the kill-time score's top.
    pub const KILL_TIME: Self = Self(1);
    /// Mode 2: the total of an entry whose shown armor is at least 100.
    pub const ARMORED: Self = Self(2);

    /// The mode of the int `value`: every int is one.
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

/// A player's boar lure destination (`boarLureDestination`, setting 0xc): the byte its AI's
/// script sets, which no modelled routine reads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

impl genie_diff::Diff for TargetingMode {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

impl genie_diff::Diff for LureDestination {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

impl genie_diff::Diff for VillagerGarrison {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
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
        assert_eq!(TargetingMode::of_data(2), TargetingMode::ARMORED);
        assert_ne!(TargetingMode::default(), TargetingMode::KILL_TIME);
        assert_eq!(LureDestination::of_data(3).get(), 3);
        assert_eq!(VillagerGarrison::of_data(-1).get(), -1);
    }
}
