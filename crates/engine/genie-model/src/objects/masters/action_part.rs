//! A master's action layer: the RGE action master's values (types 40 and up but 90, the `.dat`'s
//! type-40 section; FD-046-D05).

use crate::map::Distance;
use crate::objects::WorkRate;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::RunPattern;
use crate::objects::masters::Task;
use crate::objects::masters::TaskGroup;
use crate::objects::masters::TaskIndex;
use genie_diff::Diff;
use serde::Deserialize;
use serde::Serialize;

/// The action layer of a master of type 40 and up but 90 (`RGE_Master_Action_Object`,
/// `+0x790`..`+0x7b9`; the `.dat`'s type-40 section, genieutils' `Bird`): its tasks and the default
/// one, its task swap group, its work rate, its search radius, its drop sites and its run pattern.
/// An action unit (40), a combat master (50), a missile (60), a combat unit (70) and a building
/// (80) hold it.
#[derive(Clone, Debug, Default, PartialEq, Diff, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ActionPart {
    /// `+0x7b8`, the `.dat`'s `TaskSwapGroup` (the save does not carry it); 0 for none.
    pub task_by_group: TaskGroup,
    /// Its task list.
    pub tasks: Vec<Task>,
    /// `+0x790`, `default_task`: the id of the task its object's empty action list starts in
    /// state 2 (`RGE_Action_Object::update`, slot 0x7f0); None for the game's −1.
    pub default_task: Option<TaskIndex>,
    /// `work_rate` (`+0x798`): what a builder of this master adds to the work per second.
    pub work_rate: WorkRate,
    /// `search_radius` (`+0x794`, the object's slot 0x2c8).
    pub search_radius: Distance,
    /// The `.dat`'s `drop_sites` (`+0x7a0`..`+0x7a8`, `FUN_14109d4f0`): the buildings a unit of
    /// this master takes its load to.
    pub drop_sites: Vec<MasterId>,
    /// `+0x7b9`, `run_pattern`.
    pub run_pattern: RunPattern,
}

impl ActionPart {
    /// What a master without the layer reads ([`Master::action_part`]): no tasks, no default
    /// task, no task swap group, no drop sites and zeros, the import's values for a unit without
    /// the type-40 section.
    pub const ABSENT: Self = Self {
        task_by_group: TaskGroup::NONE,
        tasks: Vec::new(),
        default_task: None,
        work_rate: WorkRate::ZERO,
        search_radius: Distance::ZERO,
        drop_sites: Vec::new(),
        run_pattern: RunPattern::NONE,
    };
}

impl Master {
    /// Its action layer, or for a master of another type the values a master without it reads
    /// ([`ActionPart::ABSENT`]).
    #[must_use]
    pub fn action_part(&self) -> &ActionPart {
        // A constant that holds a `Vec` is not promoted: the inline `const` lends it for
        // `'static`.
        self.action
            .as_ref()
            .unwrap_or(const { &ActionPart::ABSENT })
    }
}

#[cfg(test)]
mod tests {
    use crate::map::Distance;
    use crate::objects::WorkRate;
    use crate::objects::masters::ActionPart;
    use crate::objects::masters::Master;
    use crate::objects::masters::MasterKind;
    use crate::objects::masters::RunPattern;
    use crate::objects::masters::TaskGroup;

    /// A master of type 40 and up but 90 holds the action layer, a missile (60) and a building
    /// (80) too; a moving one (30) and one of the data's own type 90 do not.
    #[test]
    fn masters_hold_the_action_layer_of_their_kind() {
        let layered = |kind| Master::of_kind(MasterKind::of_data(kind)).action.is_some();
        assert_eq!(
            [20, 30, 40, 50, 60, 70, 80, 90].map(layered),
            [false, false, true, true, true, true, true, false]
        );
    }

    /// A master without the layer reads no tasks, no default task, no task swap group, no drop
    /// sites and zeros, as the import gave a unit without the type-40 section.
    #[test]
    fn masters_without_the_action_layer_read_no_tasks() {
        let moving = Master::of_kind(MasterKind::MOVING);
        let part = moving.action_part();
        assert!(part.tasks.is_empty() && part.drop_sites.is_empty());
        assert_eq!(part.default_task, None);
        assert_eq!(
            (
                part.task_by_group,
                part.work_rate,
                part.search_radius,
                part.run_pattern
            ),
            (
                TaskGroup::of_data(0),
                WorkRate::ZERO,
                Distance::ZERO,
                RunPattern::NONE
            )
        );
        assert_eq!(part, &ActionPart::default());
    }
}
