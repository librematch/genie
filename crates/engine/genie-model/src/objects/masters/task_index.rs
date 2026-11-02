//! A place in a master's task list: its default task (`+0x790`).

/// A place in a master's task list (`default_task`, `+0x790`), which `FUN_14109e0c0` takes the
/// list's entry at; the data's −1 is none, and no negative one names a place.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TaskIndex(u16);

impl TaskIndex {
    /// The place the data's short `value` names: none below 0.
    #[must_use]
    pub fn of_data(value: i16) -> Option<Self> {
        u16::try_from(value).ok().map(Self)
    }

    /// The short a field holds for `index`, −1 for none.
    #[must_use]
    pub fn field(index: Option<Self>) -> i16 {
        index.map_or(-1, |i| i16::try_from(i.0).unwrap_or(i16::MAX))
    }

    /// Its place in the list.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl genie_diff::Diff for TaskIndex {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::TaskIndex;

    /// A short of 0 or more is a place, a negative one none.
    #[test]
    fn place_is_short_of_0_or_more() {
        assert_eq!(TaskIndex::of_data(3).map(TaskIndex::index), Some(3));
        assert_eq!(TaskIndex::of_data(-1), None);
        assert_eq!(TaskIndex::of_data(-7), None);
        assert_eq!(TaskIndex::field(TaskIndex::of_data(5)), 5);
        assert_eq!(TaskIndex::field(None), -1);
    }
}
