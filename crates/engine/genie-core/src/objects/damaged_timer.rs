//! A building's timer since its last hit.

/// The seconds since a building's last hit are few (`damaged_lately_timer`, `+0x30d`, a byte,
/// saved): 60 after a hit, one off in each update that passes whole seconds. The computer AI
/// reads it. Every byte is kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DamagedTimer(u8);

impl DamagedTimer {
    /// Run out: no hit lately.
    pub const OUT: Self = Self(0);
    /// A hit's (`0x3c`).
    pub const HIT: Self = Self(0x3c);

    /// The timer of the save's byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// Whether it still runs: above 0.
    #[must_use]
    pub const fn running(self) -> bool {
        0 < self.0
    }

    /// One second off.
    #[must_use]
    pub const fn tick(self) -> Self {
        Self(self.0 - 1)
    }

    /// The byte, as the save holds it.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl genie_diff::Diff for DamagedTimer {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::DamagedTimer;

    /// A hit sets 60, which runs down to 0.
    #[test]
    fn the_timer_runs_down_from_a_hit() {
        assert_eq!(DamagedTimer::HIT.get(), 60);
        assert!(DamagedTimer::HIT.running() && !DamagedTimer::OUT.running());
        assert_eq!(DamagedTimer::of_data(1).tick(), DamagedTimer::OUT);
    }
}
