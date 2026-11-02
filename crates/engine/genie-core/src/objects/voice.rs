//! Whether an action speaks for its unit ([`Voice`]): the base action's `+0x60`.

use genie_diff::Diff;

/// `+0x60` (`subActionValue`) of a `MoveTo`, an Attack and a Repair: whether the action speaks
/// for its unit. A loud action posts its notifies to its unit AI (slot 0x328) and takes and lets
/// go of its targets (slots 0x178 and 0x180); a quiet one does neither, because the action that
/// made it its sub-action speaks for the unit. The make and research actions hold their
/// payment in the same byte.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Diff)]
pub enum Voice {
    /// 0, the base constructor's (`FUN_140a73e90`): a unit's own action, and the sub-actions a
    /// flight, a move order and a gather walk leave loud.
    #[default]
    Loud,
    /// 1: a sub-action its maker made quiet (`FUN_140a7a170` with 1).
    Quiet,
}

impl Voice {
    /// The saved byte (`sub_action_value`): 0 is loud, any other value quiet, as the game tests
    /// the byte (`CMP byte ptr [RDI + 0x60],0x0` in `0x140aa2a70`).
    #[must_use]
    pub const fn of_byte(byte: u8) -> Self {
        if byte == 0 { Self::Loud } else { Self::Quiet }
    }

    /// The action posts no notifies and takes or lets go of no target.
    #[must_use]
    pub const fn is_quiet(self) -> bool {
        matches!(self, Self::Quiet)
    }

    /// The action posts its notifies and takes and lets go of its targets.
    #[must_use]
    pub const fn is_loud(self) -> bool {
        matches!(self, Self::Loud)
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::Voice;

    /// The saved byte 0 is loud, 1 and any other value quiet; a new action is loud.
    #[test]
    fn voice_of_saved_byte_is_quiet_unless_zero() {
        assert_eq!(
            [0, 1, 3, 0xff].map(Voice::of_byte),
            [Voice::Loud, Voice::Quiet, Voice::Quiet, Voice::Quiet]
        );
        assert_eq!(Voice::default(), Voice::Loud);
        assert!(Voice::Quiet.is_quiet() && !Voice::Quiet.is_loud());
        assert!(Voice::Loud.is_loud() && !Voice::Loud.is_quiet());
    }
}
