//! How a hit's two players stand to each other ([`Allied`]).

use genie_diff::Diff;

/// Whether the attacker's player counts the target's player as its own or allied
/// (`FUN_141663d90` with the ally flag), and the other way round. A blast spares the target
/// its attacker's player counts allied and scales its damage on a target whose player counts
/// the attacker's allied by `friendly_fire_damage`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Diff)]
pub struct Allied {
    /// The attacker's player counts the target's allied.
    pub by_attacker: bool,
    /// The target's player counts the attacker's allied.
    pub by_target: bool,
}

impl Allied {
    /// Neither counts the other allied.
    pub const NONE: Self = Self {
        by_attacker: false,
        by_target: false,
    };
    /// Each counts the other allied.
    pub const BOTH: Self = Self {
        by_attacker: true,
        by_target: true,
    };
    /// Only the attacker's player counts the target's allied.
    pub const BY_ATTACKER: Self = Self {
        by_attacker: true,
        by_target: false,
    };
    /// Only the target's player counts the attacker's allied.
    pub const BY_TARGET: Self = Self {
        by_attacker: false,
        by_target: true,
    };

    /// Each counts the other allied.
    #[must_use]
    pub const fn both(self) -> bool {
        self.by_attacker && self.by_target
    }
}

#[cfg(test)]
mod tests {
    use crate::combat::damage::Allied;

    /// Both only when each player counts the other allied.
    #[test]
    fn both_needs_each_side() {
        assert!(Allied::BOTH.both());
        assert!(!Allied::BY_ATTACKER.both() && !Allied::BY_TARGET.both());
        assert_eq!(Allied::NONE, Allied::default());
    }
}
