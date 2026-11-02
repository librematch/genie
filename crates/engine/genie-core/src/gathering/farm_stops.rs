//! A gatherer's stops on a farm.

/// A Gather's stops (`+0x6c`, `mNumberStops`, a byte): 1, or a farm's four under the game's farm
/// setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FarmStops(u8);

impl FarmStops {
    /// One stop: no farm walk.
    pub const ONE: Self = Self(1);

    /// The stops of the save's byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// The byte.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// Whether the gatherer walks between stops: more than one.
    #[must_use]
    pub const fn several(self) -> bool {
        1 < self.0
    }

    /// The stops as a float (`CVTSI2SS` of the byte): what a stop's time divides by.
    #[must_use]
    pub fn as_f32(self) -> f32 {
        f32::from(self.0)
    }
}

impl genie_diff::Diff for FarmStops {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

/// The stop a Gather is at (`+0x6d`, `mCurrentStop`, a byte), counted from 0.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StopIndex(u8);

impl StopIndex {
    /// The first stop.
    pub const FIRST: Self = Self(0);

    /// The stop of the save's byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// The byte.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// The stop after it (wrapping as a byte).
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    /// Whether a stop follows it among `stops` (the game's widened `current + 1 < stops`).
    #[must_use]
    pub const fn before_last(self, stops: FarmStops) -> bool {
        (self.0 as u32) + 1 < stops.0 as u32
    }
}

impl genie_diff::Diff for StopIndex {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::gathering::{FarmStops, StopIndex};

    /// A farm of four stops walks on from its first three stops, not from its last.
    #[test]
    fn farm_of_four_stops_walks_on_from_the_first_three() {
        let four = FarmStops::of_data(4);
        assert!(four.several() && !FarmStops::ONE.several());
        assert!(StopIndex::of_data(2).before_last(four));
        assert!(!StopIndex::of_data(3).before_last(four));
        assert!(!StopIndex::of_data(255).before_last(FarmStops::of_data(255)));
        assert_eq!(StopIndex::FIRST.next().get(), 1);
        assert_eq!(four.as_f32().to_bits(), 4.0f32.to_bits());
    }
}
