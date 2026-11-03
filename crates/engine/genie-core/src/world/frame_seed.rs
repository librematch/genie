//! The world's frame seed.

/// The world's frame seed (`world+0x7d0`): a draw of the world's stream each update, which seeds
/// the update's other streams.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FrameSeed(u32);

impl FrameSeed {
    /// The seed of the save's word `value`.
    #[must_use]
    pub const fn of_data(value: u32) -> Self {
        Self(value)
    }

    /// The word.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl genie_diff::Diff for FrameSeed {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::world::FrameSeed;

    /// The seed keeps its word.
    #[test]
    fn the_seed_keeps_its_word() {
        assert_eq!(FrameSeed::of_data(0x7fff).get(), 0x7fff);
        assert_eq!(FrameSeed::default().get(), 0);
    }
}
