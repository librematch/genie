//! A building master's adjacent mode (`+0x980`): whether walls and gates join their neighbours.

/// A building master's adjacent mode (`+0x980`, `adjacent_mode`): walls and gates join their
/// neighbours (`FUN_1411ce890`). Every byte is kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct AdjacentMode(u8);

impl AdjacentMode {
    /// 0: stands alone.
    pub const NONE: Self = Self(0);

    /// The value of the data's byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// The byte, as the data and the save hold it.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl std::fmt::Display for AdjacentMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl genie_diff::Diff for AdjacentMode {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::AdjacentMode;

    /// The data's values are the named ones, any other is kept.
    #[test]
    fn an_adjacent_mode_is_the_data_byte() {
        assert_eq!(AdjacentMode::of_data(0), AdjacentMode::NONE);
        assert_eq!(AdjacentMode::of_data(1).get(), 1);
        assert_eq!(AdjacentMode::NONE.to_string(), "0");
    }
}
