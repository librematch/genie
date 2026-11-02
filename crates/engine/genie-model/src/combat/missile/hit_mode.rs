//! A missile's hit mode.

/// A missile's hit mode (its master's `hit_mode`, `+0x882`, the save's `missile_hit_info`; the
/// missile's copy `+0x80`): which objects on its way it hits. Mode 0 hits its target alone; a
/// miss turns it into mode 1, which hits another player's objects too. Mode 2 strikes its
/// target on landing as mode 0 does; its hits on the way are not modelled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct HitMode(u8);

impl HitMode {
    /// Mode 0: its target alone.
    pub const TARGET_ALONE: Self = Self(0);
    /// Mode 1, a missed missile's: another player's objects on its way too.
    pub const MISSED: Self = Self(1);

    /// The mode of the data's byte `value`: every byte is one.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// The byte.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// The mode an add effect of `n` makes of it: the byte's sum, wrapping.
    #[must_use]
    pub const fn plus(self, n: u8) -> Self {
        Self(self.0.wrapping_add(n))
    }

    /// Whether a missile that lands on its target strikes it: in mode 0 or 2, or in mode 1 when
    /// the target is another player's (`foreign`).
    #[must_use]
    pub const fn strikes_on_landing(self, foreign: bool) -> bool {
        matches!(self.0, 0 | 2) || (self.0 == 1 && foreign)
    }
}

impl genie_diff::Diff for HitMode {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::combat::missile::HitMode;

    /// The modes are the data's bytes.
    #[test]
    fn hit_modes_are_the_data_bytes() {
        assert_eq!(HitMode::default(), HitMode::TARGET_ALONE);
        assert_eq!(HitMode::of_data(1), HitMode::MISSED);
        assert_eq!(HitMode::of_data(2).get(), 2);
        assert!(HitMode::TARGET_ALONE.strikes_on_landing(false));
        assert!(HitMode::of_data(2).strikes_on_landing(false));
        assert!(!HitMode::MISSED.strikes_on_landing(false));
        assert!(HitMode::MISSED.strikes_on_landing(true));
        assert!(!HitMode::of_data(3).strikes_on_landing(true));
        assert_eq!(HitMode::MISSED.plus(255), HitMode::TARGET_ALONE);
    }
}
