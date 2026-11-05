//! An action object's command count (`command_flag`, `+0x888`, a byte the save holds): one more
//! for every work and move its slots 0x128, 0x140 and 0x138 hand on (`FUN_14106c590`,
//! `FUN_141070c90`, `FUN_14106d340`), 0 from the constructor (`FUN_14106f9e0`). A Repair keeps
//! its unit's count when it walks it to a ship and walks the ship back only while the ship's count
//! is the one it kept (`0x140ad9120`, `0x140ad9c60`'s case 10).

use genie_diff::Diff;

/// The commands an action object took, counted in a byte that wraps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct CommandCount(u8);

impl CommandCount {
    /// No command: the constructor's.
    pub const ZERO: Self = Self(0);

    /// The count of the save's byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// One more command (`INC byte ptr`: 255 wraps to 0).
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    /// The byte, as the save holds it.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl Diff for CommandCount {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_count_wraps_past_255() {
        assert_eq!(CommandCount::of_data(254).next().get(), 255);
        assert_eq!(CommandCount::of_data(255).next(), CommandCount::ZERO);
    }
}
