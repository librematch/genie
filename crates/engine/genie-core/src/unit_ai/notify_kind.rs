//! A unit AI notify's kind as one type: what an object tells a unit AI (an action completed, a
//! hit, a search asked for), the value the save's `temp_notify` holds at `+0x8`.

use crate::unit_ai::ai_kinds;
use std::fmt::{Error, Formatter, LowerHex};

/// A notify's kind (`+0x8` of the save's `temp_notify`): the case of the unit AI's notify
/// handlers (`processNotify`) it takes. The game posts kinds the slice does not name (the
/// generated [`crate::unit_ai::ai_kinds`] has the ones its log lines name) and a save may hold
/// any of them, so the kind is a newtype with named constants: every `i32` is a kind. Its
/// value is open to the crate (no value breaks an invariant), so that the handlers' switches
/// match the game's ranges of kinds (`NotifyKind(0x259..=0x262)`, the action values).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NotifyKind(pub(crate) i32);

impl NotifyKind {
    /// `500`: the unit took damage (p1 the attacker, p2 the damage).
    pub const DAMAGED: Self = Self(ai_kinds::NOTIFY_DAMAGED);
    /// `0x1f9`: an action failed (p1 its action value).
    pub const ACTION_FAILED: Self = Self(ai_kinds::NOTIFY_ACTION_FAILED);
    /// `0x1fa`: an action completed (p1 the action value).
    pub const ACTION_COMPLETED: Self = Self(ai_kinds::NOTIFY_ACTION_COMPLETED);
    /// `0x1fb`: an action's invalidation (p1 the action value).
    pub const ACTION_INVALIDATED: Self = Self(ai_kinds::NOTIFY_ACTION_INVALIDATED);
    /// `0x1fc`: a `MoveTo`'s target left its point and its path's end did not follow (p1..p3 0).
    pub const TARGET_MOVED: Self = Self(0x1fc);
    /// `0x1fd`: readyToAttack's target is out of reach.
    pub const OUT_OF_REACH: Self = Self(0x1fd);
    /// `0x1fe`: slot 0x7e8's target is below the unit's least range (the unit's own id twice,
    /// the target, −1, −1).
    pub const TOO_CLOSE: Self = Self(0x1fe);
    /// `0x200`: "`ActionExecuted`" (the soldier's log line), an attack's swing (p1 600, p2 the
    /// target).
    pub const ACTION_EXECUTED: Self = Self(0x200);
    /// `0x202` ("Action %d has been invalidated or requires a search.", the villager's log
    /// line): an action asks for another target (p1 its action value, p2 the target class, p3 a
    /// drop site): a Build waiting without work, a searching Hunt or Gather, an Explore waiting
    /// for a point (state 3).
    pub const REQUIRES_SEARCH: Self = Self(0x202);
    /// `0x203`: an action's drop-off found no drop site (the Build's `set_state(8)`, a gather's).
    pub const NO_DROP_SITE: Self = Self(0x203);
    /// `0x20b`: the unit changed hands (p1 the new owner).
    pub const OWNER_CHANGED: Self = Self(0x20b);
    /// `0x20f`: a missile's warning, which a shot whose blast does not spare friends sends its
    /// target when a computer player owns it (`FUN_141081020`).
    pub const MISSILE_WARNING: Self = Self(0x20f);
    /// `0x215` (no log line): the motion step met a unit of its walked target's master that
    /// neither player counts an ally (p1 its id).
    pub const IN_THE_WAY: Self = Self(0x215);
    /// `0x216`: a unit sent off a foundation's footprint (`FUN_14123bb20`): the foundation
    /// master's radii times 100 and the foundation.
    pub const SENT_OFF: Self = Self(0x216);
    /// `0x21b`: a unit sent off a footprint with a group (`FUN_14123bb20`'s group move).
    pub const SENT_OFF_GROUP: Self = Self(0x21b);
    /// `0x21c`: an action asked to drop its load off carries another resource than its task's
    /// (the base action's slot 0x78, `FUN_140a78060`).
    pub const OTHER_LOAD: Self = Self(0x21c);
    /// `0x21d`: a human player's builder left a pasture already built (p2 the pasture its stack
    /// unit made).
    pub const PASTURE_BUILT: Self = Self(0x21d);
    /// `0x258`: the unit was attacked.
    pub const ATTACKED: Self = Self(ai_kinds::NOTIFY_ATTACKED);
    /// `699`: an object no longer acts on the unit (its action let the unit go,
    /// `FUN_141086e60`).
    pub const NO_LONGER_ON_ME: Self = Self(ai_kinds::NOTIFY_NO_LONGER_ON_ME);

    /// Its value, as the save and the game's log lines hold it.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

impl From<i32> for NotifyKind {
    /// The kind a saved notify holds: every value is one.
    fn from(value: i32) -> Self {
        Self(value)
    }
}

impl LowerHex for NotifyKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        LowerHex::fmt(&self.0, f)
    }
}

impl genie_diff::Diff for NotifyKind {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::unit_ai::NotifyKind;

    /// Every saved value is a kind, the named ones their constants; the value converts back and
    /// prints in hex as the game's log lines do.
    #[test]
    fn every_value_is_kind() {
        assert_eq!(NotifyKind::from(0x202), NotifyKind::REQUIRES_SEARCH);
        assert_eq!(NotifyKind::from(500), NotifyKind::DAMAGED);
        assert_eq!(NotifyKind::from(0x999).get(), 0x999);
        assert_eq!(format!("{:#x}", NotifyKind::ACTION_COMPLETED), "0x1fa");
        assert!(matches!(NotifyKind::from(0x25d), NotifyKind(0x259..=0x262)));
    }
}
