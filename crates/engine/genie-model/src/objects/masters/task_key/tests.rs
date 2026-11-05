use crate::objects::masters::ActionKind;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::objects::masters::Task;
use crate::objects::masters::TaskId;
use crate::objects::masters::TaskKey;

/// A task's key is its action type, target class and target master; its id is no part of it.
#[test]
fn the_key_of_each_task_is_its_action_class_and_target() {
    let task = Task {
        id: TaskId::of_task(4),
        action_kind: ActionKind::BUILD,
        object_group: Some(ClassId::named(3)),
        object_id: Some(MasterId::named(70)),
        ..Task::default()
    };
    let other = Task {
        id: TaskId::of_task(9),
        ..task
    };
    assert_eq!(
        task.key(),
        TaskKey {
            action_kind: ActionKind::BUILD,
            object_group: Some(ClassId::named(3)),
            object_id: Some(MasterId::named(70)),
        }
    );
    assert_eq!(other.key(), task.key());
}
