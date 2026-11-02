//! A task's id as one type: the place of a task in its master's task list, which an action
//! keeps to name the task it works on.

use crate::GenieError;
use genie_diff::Diff;
use getset::CopyGetters;

/// A task's id ([`crate::objects::masters::Task::id`], `+0xa`, the `.dat`'s task id): 0 to 85
/// in the current data ([`TaskId::of_task`]). A field that holds −1 for none holds
/// `Option<TaskId>` ([`TaskId::of_field`], which rejects any other negative value).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Diff, CopyGetters)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TaskId(
    /// Its value, as the task list holds it.
    #[getset(get_copy = "pub")]
    i16,
);

impl TaskId {
    /// The value a field takes for none.
    const NONE: i16 = -1;

    /// The task of a raw field that holds −1 for none (the save's action `short_val`).
    ///
    /// # Errors
    ///
    /// A negative value other than −1, which no task's id is.
    pub fn of_field(value: i16) -> Result<Option<Self>, GenieError> {
        (value != Self::NONE)
            .then(|| Self::try_from(value))
            .transpose()
    }

    /// The id a task holds in the `.dat` or the save, as it holds it: 0 to 85 in the current
    /// data; the boundary does not reject a value outside that.
    #[must_use]
    pub const fn of_task(value: i16) -> Self {
        Self(value)
    }

    /// The value of a field that holds −1 for none.
    #[must_use]
    pub fn field(task: Option<Self>) -> i16 {
        task.map_or(Self::NONE, |t| t.0)
    }
}

impl TryFrom<i16> for TaskId {
    type Error = GenieError;

    /// The task of id `value`.
    ///
    /// # Errors
    ///
    /// A negative value, which no task's id is.
    fn try_from(value: i16) -> Result<Self, GenieError> {
        if value < 0 {
            return Err(
                GenieError::invalid_input("a negative task id, which no task has")
                    .with("task", value),
            );
        }
        Ok(Self(value))
    }
}

#[cfg(test)]
mod tests {
    use crate::ErrorKind;
    use crate::objects::masters::TaskId;

    /// A field's −1 is none, a value of 0 or more a task, and each converts back.
    #[test]
    fn field_minus_one_is_none_and_any_value_from_0_task() {
        assert_eq!(TaskId::of_field(-1), Ok(None));
        for value in [0, 85, i16::MAX] {
            let task = TaskId::of_field(value).unwrap();
            assert_eq!(task.map(|t| t.get()), Some(value));
            assert_eq!(TaskId::field(task), value);
        }
        assert_eq!(TaskId::field(None), -1);
    }

    /// A negative value is no task; in a field, one other than −1 is an error.
    #[test]
    fn negative_value_is_no_task() {
        for value in [-1, -2, i16::MIN] {
            assert_eq!(
                TaskId::try_from(value).map_err(|e| e.kind()),
                Err(ErrorKind::InvalidInput)
            );
        }
        assert_eq!(
            TaskId::of_field(-2).map_err(|e| e.kind()),
            Err(ErrorKind::InvalidInput)
        );
    }
}
