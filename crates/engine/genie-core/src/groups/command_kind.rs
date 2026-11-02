//! A group command's kind ([`GroupCommandKind`]): a move, a follow, or the action type of the
//! task the group's members carry out.

use crate::objects::masters::ActionType;

/// A group command's kind (`mCommand` of an order or of a queued command, the group's
/// `mCurrentCommand` at `+0x3a8`): a move (1), a follow (2), or the action type of the task the
/// members carry out on a target; −1 for none. The game keeps any value it is given, so this
/// keeps every `i32`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct GroupCommandKind(i32);

impl GroupCommandKind {
    /// No command (−1).
    pub const NONE: Self = Self(-1);
    /// A move (`commandMoveTo`, 1).
    pub const MOVE: Self = Self(1);
    /// A follow (2).
    pub const FOLLOW: Self = Self(2);
    /// An attack: the attack task's action type (9).
    pub const ATTACK: Self = Self::of_action(crate::combat::attack::ATTACK);

    /// The kind of value `value`, as the save and the game's memory hold it.
    #[must_use]
    pub const fn named(value: i32) -> Self {
        Self(value)
    }

    /// The command of a task of action type `action` (its value widened).
    #[must_use]
    pub const fn of_action(action: ActionType) -> Self {
        Self(action.get() as i32)
    }

    /// The kinds of these values, in their order.
    #[must_use]
    pub const fn table<const N: usize>(values: [i32; N]) -> [Self; N] {
        let mut out = [Self(0); N];
        let mut i = 0;
        while i < N {
            out[i] = Self(values[i]);
            i += 1;
        }
        out
    }

    /// Its value, as the save holds it.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use crate::groups::GroupCommandKind;
    use crate::objects::masters::ActionType;

    /// A task's action type is its command's value; none is −1, a move 1, a follow 2, and a
    /// zeroed command 0.
    #[test]
    fn task_command_is_its_action_type() {
        assert_eq!(
            GroupCommandKind::of_action(ActionType::named(0x84)),
            GroupCommandKind::named(0x84)
        );
        assert_eq!(
            [
                GroupCommandKind::NONE,
                GroupCommandKind::MOVE,
                GroupCommandKind::FOLLOW,
                GroupCommandKind::default()
            ]
            .map(GroupCommandKind::get),
            [-1, 1, 2, 0]
        );
    }
}
