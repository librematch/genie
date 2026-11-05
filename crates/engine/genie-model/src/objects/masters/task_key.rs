//! What a task change matches a task by ([`TaskKey`]).

use crate::objects::masters::ActionKind;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::objects::masters::Task;
use genie_diff::Diff;

/// A task's action type (`+0xe`), target class (`+0x10`) and target master (`+0x12`): an
/// action's task change (`FUN_140a78150`) takes the new master's last task whose three equal
/// those of the action's task.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Diff)]
pub struct TaskKey {
    /// `+0xe`: the action the task makes the unit do.
    pub action_kind: ActionKind,
    /// `+0x10`: the target class, none (−1) for any.
    pub object_group: Option<ClassId>,
    /// `+0x12`: the target master, none (−1) for any.
    pub object_id: Option<MasterId>,
}

impl Task {
    /// Its [`TaskKey`].
    #[must_use]
    pub const fn key(&self) -> TaskKey {
        TaskKey {
            action_kind: self.action_kind,
            object_group: self.object_group,
            object_id: self.object_id,
        }
    }
}

#[cfg(test)]
mod tests;
