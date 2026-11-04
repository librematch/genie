//! A saved building's parts: its lists of objects, its annex slots, its gate and its
//! construction.

use crate::GenieResult;
use crate::construction::Annexes;
use crate::construction::BuilderCount;
use crate::construction::build::Construction;
use crate::map::Snow;
use crate::map::spatial::Obstruction;
use crate::objects::MasterHitPoints;
use crate::objects::ObjectId;
use crate::objects::masters::ObstructionClass;
use crate::objects::masters::ObstructionKind;
use crate::world::Gate;
use crate::world::Seconds;
use crate::world::WholeSeconds;
use crate::world::load::object_ids;
use genie_save::generated::world::{PlayerObjects, RgeStaticObject};

/// A saved building's lists of objects (none for another object).
#[derive(Default)]
pub(super) struct BuildingLists {
    /// The livestock it holds.
    pub(super) livestock: Vec<ObjectId>,
    /// Its pasture's workers.
    pub(super) pasture_workers: Vec<ObjectId>,
    /// `attackingUnits` (+0xce0): the unallied takers of a foundation with a stack unit.
    pub(super) attacking_units: Vec<ObjectId>,
}

/// A main building's annex slots (`+0xbc0`..`+0xbd8`) from the save's linked object ids, −1
/// for an empty slot.
pub(super) fn annexes(ids: &[i32]) -> Annexes {
    let mut slots = [None; Annexes::SLOTS];
    for (slot, &id) in slots.iter_mut().zip(ids) {
        *slot = ObjectId::of_field(id);
    }
    Annexes::new(slots)
}

/// A saved object's [`BuildingLists`].
///
/// # Errors
///
/// An entry of −1, which names no object.
pub(super) fn building_lists(object: &genie_save::Object) -> GenieResult<BuildingLists> {
    let Some(tribe_building_object) = object.tribe_building_object.as_deref() else {
        return Ok(BuildingLists::default());
    };
    let workers: Vec<i32> = tribe_building_object
        .pasture_worker_id
        .iter()
        .map(|&word| word as i32)
        .collect();
    Ok(BuildingLists {
        livestock: object_ids(&tribe_building_object.livestock_id)?,
        pasture_workers: object_ids(&workers)?,
        attacking_units: object_ids(&tribe_building_object.unit_id)?,
    })
}

/// A saved building's gate state ([`crate::world::Gate`]): its lock, first update and close
/// timer, and whether a saved open or shut master or its original owner's would take part in a
/// toggle.
#[must_use]
pub fn saved_gate(object: &genie_save::Object) -> Option<Gate> {
    let tribe_building_object = object.tribe_building_object.as_deref()?;
    Some(Gate {
        locked: tribe_building_object.gate_locked != 0,
        first_update: tribe_building_object.first_update != 0,
        close_timer: Seconds::new(tribe_building_object.close_timer),
        other_masters: tribe_building_object.has_open_closed_master_object != 0
            || tribe_building_object.original_owner != -1,
    })
}

/// A saved building's construction: its `build_pts`, `desolid_flag` and `num_builders`, its
/// master's hit points (the sixth `readTemp`), build time (the first train location's) and
/// obstruction kind and class. None for an object that is not a building.
///
/// # Errors
///
/// A snow flag that is not the game's (above 2).
pub fn construction(
    object: &genie_save::Object,
    rge_static_object: &RgeStaticObject,
    objects: &PlayerObjects,
) -> GenieResult<Option<Construction>> {
    let Some(tribe_building_object) = object.tribe_building_object.as_deref() else {
        return Ok(None);
    };
    let Some(master) = objects.master(rge_static_object.master_obj) else {
        return Ok(None);
    };
    let Some(master_static) = master.static_part() else {
        return Ok(None);
    };
    let build_time = master
        .tribe_master_building_object
        .as_deref()
        .map(|tribe_master_building_object| {
            &tribe_master_building_object.tribe_master_combat_object
        })
        .or(master.tribe_master_combat_object.as_deref())
        .and_then(|tribe_master_combat_object| tribe_master_combat_object.train_location.first())
        .map_or(WholeSeconds::ZERO, |entry| {
            WholeSeconds::of_data(entry.train_time)
        });
    Ok(Some(Construction {
        progress: Seconds::new(tribe_building_object.build_pts),
        desolid: tribe_building_object.desolid_flag != 0,
        builders: BuilderCount::of_data(tribe_building_object.tribe_combat_object.num_builders),
        hit_points: MasterHitPoints::of_data(master_static.read_temp_6),
        build_time,
        record: Obstruction {
            kind: ObstructionKind::from(master_static.obstruction_kind),
            class: ObstructionClass::from(master_static.obstruction_class),
        },
        snow: Snow::of_byte(tribe_building_object.snow_flag)?,
        sprite_override: rge_static_object.sprite_override_id != -1,
    }))
}
