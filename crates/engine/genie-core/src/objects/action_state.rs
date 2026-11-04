//! The state of an action of any type ([`ActionState`]): its number.

use derive_more::Display;
use genie_diff::Diff;

/// An action's state by its number (`+0x18`, `RGE_Action_Object::actionState`), whatever the
/// action's type: what the readers of any action compare (the unit AI, the movement, the
/// drop-off search, the meeting, the checkpoint's word 9). Each action's own enum names its
/// states ([`crate::combat::attack::AttackState`] and the others); this is the number they
/// share.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Diff, Display)]
pub struct ActionState(u8);

impl ActionState {
    /// The number an object without an action reads (slot 0x2b0 gives 0).
    pub const NO_ACTION: Self = Self(0);

    /// The state of number `value`.
    #[must_use]
    pub const fn new(value: u8) -> Self {
        Self(value)
    }

    /// The number, as the action holds it.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::ActionState;

    /// The number goes in and out as it is; no action reads 0.
    #[test]
    fn an_action_state_is_its_number() {
        assert_eq!(ActionState::new(12).get(), 12);
        assert_eq!(ActionState::NO_ACTION, ActionState::new(0));
        assert_eq!(ActionState::NO_ACTION, ActionState::default());
        assert_eq!(ActionState::new(8).to_string(), "8");
    }
}
