//! A drop site and the targets that take a load (FD-046: the model's part of the task finder).

use crate::objects::ObjectState;
use crate::objects::WorkerCount;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::players::CivId;
use crate::players::attribute::AttributeId;

/// One entry of the drop-site table (`DAT_143cc2948`, the game's `dropsites.json`): the building,
/// the workers it serves (their class, −1 any, or else a list of their masters), the civs it is
/// for (empty: all) and the resource kinds it takes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct DropSite {
    /// The building's master.
    pub building: MasterId,
    /// The class of the units that drop there (−1 any).
    pub worker_class: Option<ClassId>,
    /// The unit masters that drop there (empty: all).
    pub workers: Vec<MasterId>,
    /// The civs it is for (empty: all).
    pub civs: Vec<CivId>,
    /// `+8`, `auto_work` (1 when the key is missing): a builder whose building completes looks
    /// for more work around it (`FUN_1415cc4e0`).
    pub auto_work: bool,
    /// `+0x50`, `target_list`: the objects whose resources it takes, in the file's order.
    pub targets: Vec<DropTarget>,
    /// `+0x30`, `update_ai_resource_types`: the basic resources (0 to 3) it takes, which the
    /// table's search by resource reads (`table_drop_buildings`).
    pub resource_types: Vec<AttributeId>,
    /// The entry has `accepts_livestock` (`FUN_140fdf480` keeps the key's presence): its building
    /// is one of the drop sites a hunted livestock is led to (`DAT_143f157f0`, in table order).
    pub livestock: bool,
}

/// A drop site's target (`FUN_140fdf480`'s record of 0x14 bytes; each −1 when its key is missing).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct DropTarget {
    /// `object_group`: the class of the objects.
    pub class: ClassId,
    /// `worker_num`: the most workers such an object may have.
    pub workers: WorkerCount,
    /// `object_state`: the state such an object must be in (none for −1, any).
    pub state: Option<ObjectState>,
    /// `attribute_type`: the resource it gives, none for −1.
    pub attribute: Option<AttributeId>,
}

impl DropSite {
    /// `FUN_140fdd7b0`: the resource the drop site takes from an object of class `class` in
    /// state `state` with `workers` workers: its first target of that class decides, unless it
    /// caps the workers below the object's or wants another state (None).
    #[must_use]
    pub fn resource_of(
        &self,
        class: ClassId,
        state: ObjectState,
        workers: WorkerCount,
    ) -> Option<AttributeId> {
        let t = self.targets.iter().find(|t| t.class == class)?;
        if t.workers.caps(workers) || t.state.is_some_and(|wanted| wanted != state) {
            return None;
        }
        t.attribute
    }
}
