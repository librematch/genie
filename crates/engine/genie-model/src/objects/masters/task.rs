//! A master's task: what its unit can do to a target (the `.dat`'s task list, FD-046).

use crate::map::Distance;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::objects::masters::SoundEvents;
use crate::objects::masters::TargetOwner;
use crate::objects::masters::TaskFlags;
use crate::objects::masters::TaskId;
use crate::objects::masters::TaskSprites;
use crate::objects::masters::TaskTerrain;
use crate::objects::masters::WorkValue;
use crate::players::attribute::AttributeId;
use crate::world::Seconds;
use genie_diff::Diff;
use serde::Deserialize;
use serde::Serialize;

/// One task of a master's task list (`RGE_Task`), with the memory offsets the task finder reads
/// (the save's names; the `.dat`'s in brackets where they differ).
#[derive(Clone, Copy, Debug, PartialEq, Diff, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Task {
    /// `+0xa`.
    pub id: TaskId,
    /// `+0xe`: the action the task makes the unit do.
    pub action_kind: ActionKind,
    /// `+0x10`: the target class the task is for, none (−1) for any.
    pub object_group: Option<ClassId>,
    /// `+0x12`: the target master, -1 for any.
    pub object_id: Option<MasterId>,
    /// `+0x14`, `combat_level` (`enable_targeting`): the task is for a target's combat level.
    pub combat_level: u8,
    /// `+0x15`, `combat_level_flag`: a generation task (151, `genie_core::combat::abilities`) of a
    /// combat unit with [`TaskFlags::UNITS_ALWAYS`] or of a building with
    /// [`TaskFlags::BUILDINGS_ALWAYS`] works on its own; a removal reward task (161) with any
    /// flag pays the master's cost of its resource times the rate.
    pub combat_level_flag: TaskFlags,
    /// `+0x16`: the terrain the task is for, -1 for none.
    pub terrain: TaskTerrain,
    /// `+0x18`, `owner_type` (`target_diplomacy`): whose targets.
    pub owner_kind: TargetOwner,
    /// `+0x19`, `holding_attr` (`carry_check`): 1 wants a target holding the task's resource.
    pub holding_attr: u8,
    /// `+0x1b`, `search_flag` (`auto_search_targets`): an aura task (type 155) with it counts the
    /// objects it names toward its amount instead of powering them up (slot 277,
    /// `0x141206d50`, the entry's `+0x16`).
    pub search_flag: u8,
    /// `+0x20` to `+0x26`, `attribute_type` to `attribute_type4` (`resource_in`,
    /// `resource_multiplier`: the attribute that scales the gathering, `resource_out`,
    /// `unused_resource`).
    pub attribute_kind: Option<AttributeId>,
    /// `+0x22`, `attribute_type2` (`resource_multiplier`): the attribute that scales the gathering.
    pub attribute_kind2: Option<AttributeId>,
    /// `+0x24`, `attribute_type3` (`resource_out`).
    pub attribute_kind3: Option<AttributeId>,
    /// `+0x26`, `attribute_type4` (`unused_resource`).
    pub attribute_kind4: Option<AttributeId>,
    /// `+0x1c`, `search_wait_time`: a Fly task's rest between flights.
    pub wait_time: Seconds,
    /// `+0x28`, the save's `work_val1`; `+0x2c`, `work_val2`.
    pub work_val1: WorkValue,
    /// `+0x2c`, the save's `work_val2`.
    pub work_val2: WorkValue,
    /// `+0x30`: how near the target the work is done.
    pub work_range: Distance,
    /// `+0x34`, `work_flag` (the `.dat`'s `gather_type`): a Make adds to its player's attribute,
    /// not to its object's holding (`genie_core::gathering::make`).
    pub work_flag: i16,
    /// `+0x36`, `work_flag2` (the `.dat`'s `work_flag_2`): the task lookup by action, `+0x34`
    /// and this (`FUN_14109e1a0`) matches it; a charge's run is task (0x85, any, 0x7d1).
    pub work_flag2: i16,
    /// `+0x38`, `enablement_resource_id` (`enabled`): the resource that enables the task, -1 none.
    pub enablement: Option<AttributeId>,
    /// `+0x40`..`+0x58`: the moving, proceeding, working and carrying sprites.
    pub sprites: TaskSprites,
    /// `+0x60` and `+0x78`: the event ids (`mEventId`) of the gathering and the deposit sound,
    /// as the task's load (`FUN_141062470`) leaves them; a terrain look task (159) names its
    /// dying and its undead sprite with them (`genie_core::combat::abilities`).
    pub sound_events: SoundEvents,
}

impl Task {
    /// The task is for objects of class `class`: the game compares its `+0x10` short with the
    /// class, so a task for any class (−1) is for a class of −1 too.
    #[must_use]
    pub const fn for_class(&self, class: ClassId) -> bool {
        ClassId::field(self.object_group) == class.get()
    }
}

/// A task for any target, with no resource, terrain or requirement.
impl Default for Task {
    fn default() -> Self {
        Self {
            id: TaskId::default(),
            action_kind: ActionKind::default(),
            object_group: None,
            object_id: None,
            combat_level: 0,
            combat_level_flag: TaskFlags::NONE,
            terrain: TaskTerrain::NONE,
            owner_kind: TargetOwner::ANY,
            holding_attr: 0,
            search_flag: 0,
            attribute_kind: None,
            attribute_kind2: None,
            attribute_kind3: None,
            attribute_kind4: None,
            wait_time: Seconds::ZERO,
            work_val1: WorkValue::ZERO,
            work_val2: WorkValue::ZERO,
            work_range: Distance::ZERO,
            work_flag: 0,
            work_flag2: 0,
            enablement: None,
            sprites: TaskSprites::NONE,
            sound_events: SoundEvents::new(0, 0),
        }
    }
}
