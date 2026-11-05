//! A saved action's task: the task a saved action names in its master's list ([`task_at`],
//! [`own_task_at`]) and a saved task as genie-model's ([`core_task`]).

use crate::dat::MasterTable;
use crate::dat::attributes;
use crate::dat::dat_id;
use crate::save::action_part;
use genie_model::map::Distance;
use genie_model::objects::masters::Master as MastersMaster;
use genie_model::objects::masters::SoundEvents;
use genie_model::objects::masters::TargetOwner;
use genie_model::objects::masters::TaskFlags;
use genie_model::objects::masters::TaskSprites;
use genie_model::objects::masters::TaskTerrain;
use genie_model::objects::masters::WorkValue;
use genie_model::objects::masters::{ActionKind, ClassId, MasterId, Task, TaskId};
use genie_model::players::CivId;
use genie_model::world::Seconds;
use genie_model::{GenieError, GenieResult};
use genie_save::generated::world::{PlayerObjects, Task as WorldTask};

/// A saved action whose task slot names none (−1).
const NO_TASK: GenieError = GenieError::unsupported("a saved action without its task");

/// A saved action whose task slot lies past its master's task list.
const TASK_BEYOND_LIST: GenieError =
    GenieError::unsupported("a saved action's task beyond its master's list");

/// The task at `index` in a master's task list (its saved list, else the `.dat`'s): the task a
/// saved action's base `short_val` names (the base load `FUN_140a7a2f0` resolves it to `+0x48`
/// at `0x140a7b5e8`).
pub fn task_at(
    objects: &PlayerObjects,
    civ: CivId,
    masters: Option<&MasterTable>,
    master: i16,
    index: i16,
) -> GenieResult<Task> {
    let task_index = usize::try_from(index).map_err(|_| NO_TASK)?;
    let saved = objects
        .master(master)
        .and_then(action_part)
        .and_then(|rge_master_action_object| rge_master_action_object.tasks.as_deref());
    if let Some(tasks) = saved {
        let entry = tasks.list.get(task_index).ok_or(TASK_BEYOND_LIST)?;
        return core_task(&entry.task);
    }
    masters
        .and_then(|master_table| master_table.work(civ, dat_id(objects, master)?))
        .and_then(|master| master.action_part().tasks.get(task_index))
        .copied()
        .ok_or(GenieError::unsupported(
            "a saved action's task the data file does not have",
        ))
}

/// [`task_at`] for an object of its own master `own`: the task at `index` in that master's list,
/// as the base load reads it through the object's master pointer (`+0xa0`, `FUN_140a7a2f0`).
///
/// # Errors
///
/// A slot of no task (−1) or one past the list.
pub fn own_task_at(own: &MastersMaster, index: i16) -> GenieResult<Task> {
    let task_index = usize::try_from(index).map_err(|_| NO_TASK)?;
    own.action_part()
        .tasks
        .get(task_index)
        .copied()
        .ok_or(TASK_BEYOND_LIST)
}

/// A saved task (`RGE_Task`) as genie-core's [`genie_model::objects::masters::Task`].
///
/// # Errors
///
/// A negative action type, or an attribute below −1.
pub fn core_task(task: &WorldTask) -> GenieResult<Task> {
    let [
        attribute_kind,
        attribute_kind2,
        attribute_kind3,
        attribute_kind4,
        enablement,
    ] = attributes([
        task.attribute_kind,
        task.attribute_kind2,
        task.attribute_kind3,
        task.attribute_kind4,
        task.enablement_resource_id,
    ])?;
    Ok(Task {
        id: TaskId::of_task(task.id),
        action_kind: ActionKind::try_from(task.action_kind)?,
        object_group: ClassId::of_field(task.object_group)?,
        object_id: MasterId::of_data(task.object_id),
        combat_level: task.combat_level,
        combat_level_flag: TaskFlags::from_bits_retain(task.combat_level_flag),
        terrain: TaskTerrain::of_data(task.terrain_id),
        owner_kind: TargetOwner::of_data(task.owner_kind),
        holding_attr: task.holding_attr,
        attribute_kind,
        attribute_kind2,
        attribute_kind3,
        attribute_kind4,
        wait_time: Seconds::new(task.search_wait_time),
        work_val1: WorkValue::new(task.work_val1),
        work_val2: WorkValue::new(task.work_val2),
        work_range: Distance::new(task.work_range),
        work_flag: task.work_flag,
        enablement,
        sprites: TaskSprites::of_data([
            task.short_val,
            task.short_val_2,
            task.short_val_3,
            task.short_val_4,
        ]),
        sound_events: SoundEvents::new(
            task.gather_sound_event_event_id.cast_signed(),
            task.deposit_sound_event_event_id.cast_signed(),
        ),
    })
}
