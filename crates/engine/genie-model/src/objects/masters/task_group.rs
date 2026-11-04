//! A master's task swap group (`+0x7b8`): the masters a unit may switch between for a task.

use derive_more::Display;
use serde::Deserialize;
use serde::Serialize;

/// A master's task swap group (`+0x7b8`, the `.dat`'s `TaskSwapGroup`; the save does not carry it):
/// a unit of a master of a group switches to another master of the same group whose task list has a
/// task for the target. 0 is none.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    PartialOrd,
    Ord,
    Display,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TaskGroup(u8);

impl TaskGroup {
    /// 0: no group.
    pub const NONE: Self = Self(0);
    /// 1: the `.dat`'s male villagers.
    pub const MALE_VILLAGERS: Self = Self(1);
    /// 2: the `.dat`'s female villagers.
    pub const FEMALE_VILLAGERS: Self = Self(2);
    /// 3: the `.dat`'s fishing ships.
    pub const FISHING_SHIPS: Self = Self(3);

    /// The value of the data's byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// The byte, as the data and the save hold it.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl genie_diff::Diff for TaskGroup {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::TaskGroup;

    /// The data's values are the named ones, any other is kept.
    #[test]
    fn task_group_is_the_data_byte() {
        assert_eq!(TaskGroup::of_data(0), TaskGroup::NONE);
        assert_eq!(TaskGroup::of_data(4).get(), 4);
        assert_eq!(TaskGroup::NONE.to_string(), "0");
    }
}
