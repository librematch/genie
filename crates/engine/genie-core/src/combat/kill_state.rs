//! Whether an object's kill is counted.

/// A combat object's `mProcessedKill` (`+0x954`, a saved byte): whether its kill is counted. The
/// constructors leave it open (`FUN_141072ad0`); slot 0x8b0 (`FUN_14107da00`) and slot 22's
/// stinger credits ([`crate::combat::stingers::credit`]) count it, and slot 0x8b0's callers (the
/// damage `FUN_141082e80`, `take_unit` `FUN_1412108e0`) and `take_building`'s count of a
/// trebuchet's kill (`FUN_1411cea90`) run only while it is open.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum KillState {
    /// 0: nothing has counted its kill.
    #[default]
    Open,
    /// Any other byte: its kill is counted.
    Counted,
}

impl KillState {
    /// The state of the save's byte `value`: any byte but 0 is counted (its readers test the byte
    /// against 0).
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        if value == 0 {
            Self::Open
        } else {
            Self::Counted
        }
    }

    /// Its kill is counted.
    #[must_use]
    pub const fn is_counted(self) -> bool {
        matches!(self, Self::Counted)
    }
}

impl genie_diff::Diff for KillState {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        differ.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::combat::KillState;

    /// Any byte but 0 counts the kill.
    #[test]
    fn kill_state_counts_for_any_byte_but_zero() {
        assert_eq!(KillState::of_data(0), KillState::Open);
        assert_eq!(KillState::of_data(1), KillState::Counted);
        assert_eq!(KillState::of_data(9), KillState::Counted);
        assert!(KillState::Counted.is_counted() && !KillState::Open.is_counted());
    }
}
