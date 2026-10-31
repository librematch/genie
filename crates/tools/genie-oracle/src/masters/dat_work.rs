//! A `.dat` master's work-command values (genie-core's [`work::Master`]): its tasks with their
//! sound events, its costs in their slots and the rest the work command reads.

use genie_core::GenieError;
use genie_core::map::Distance;
use genie_core::map::{Footprint, GroundOffset, Radii};
use genie_core::objects::HitPoints;
use genie_core::objects::SpriteId;
use genie_core::objects::WorkRate;
use genie_core::objects::masters as work;
use genie_core::objects::masters::ActionType;
use genie_core::objects::masters::GatherGroup;
use genie_core::objects::masters::MasterId;
use genie_core::objects::masters::ObstructionClass;
use genie_core::objects::masters::ObstructionType;
use genie_core::objects::masters::SoundEvents;
use genie_core::objects::masters::TaskSprites;
use genie_core::objects::masters::VolleySpread;
use genie_core::objects::masters::{COST_SLOTS, CostSlot};
use genie_core::players::attribute::AttributeId;
use genie_core::world::Seconds;

/// The default event id a task's load gives a sound without one (`FUN_141062470`'s
/// `FUN_1413af9a0` calls, an immediate of the code).
const DEFAULT_SOUND_EVENT: u32 = 0x62d4_b3ec;

/// The event id a task's load (`FUN_141062470`) leaves at `+0x60` or `+0x78`
/// (`FUN_1413af9a0` with the world's sound table): an id of 0 whose sound number names a sound
/// of the table with sample files takes the default event ([`DEFAULT_SOUND_EVENT`]); any other
/// id stays. The id is read as an `int` (`MOVSXD`).
pub(super) fn sound_event(event: u32, sound: i16, sounds: &[genie_dat::de::Sound]) -> i32 {
    let has_files = usize::try_from(sound)
        .ok()
        .and_then(|i| sounds.get(i))
        .is_some_and(|s| !s.items.is_empty());
    if event == 0 && has_files {
        DEFAULT_SOUND_EVENT.cast_signed()
    } else {
        event.cast_signed()
    }
}

/// A `.dat` task as the task finder reads it (`genie_core::objects::masters::Task`), with the world's
/// sound table its sound events read ([`sound_event`]).
///
/// # Errors
///
/// A negative action type, or an attribute below −1.
fn dat_task(
    t: &genie_dat::de::Task,
    sounds: &[genie_dat::de::Sound],
) -> Result<work::Task, GenieError> {
    let [
        attribute_type,
        attribute_type2,
        attribute_type3,
        attribute_type4,
    ] = attributes([
        t.resource_in,
        t.resource_multiplier,
        t.resource_out,
        t.unused_resource,
    ])?;
    Ok(work::Task {
        id: genie_core::objects::masters::TaskId::of_task(t.id),
        action_type: ActionType::try_from(t.action_type)?,
        object_group: genie_core::objects::masters::ClassId::of_field(t.class_id)?,
        object_id: MasterId::of_data(t.unit_id),
        combat_level: t.enable_targeting,
        combat_level_flag: t.combat_level_flag,
        terrain: t.terrain_id,
        owner_type: t.target_diplomacy,
        holding_attr: t.carry_check,
        attribute_type,
        attribute_type2,
        attribute_type3,
        attribute_type4,
        wait_time: Seconds::new(t.search_wait_time),
        work_val1: t.work_value1,
        work_val2: t.work_value2,
        work_range: Distance::new(t.work_range),
        work_flag: t.gather_type,
        enablement: t.enabled.map(AttributeId::of_field).transpose()?.flatten(),
        sprites: TaskSprites::of_data([
            t.moving_graphic_id,
            t.proceeding_graphic_id,
            t.working_graphic_id,
            t.carrying_graphic_id,
        ]),
        sound_events: SoundEvents::new(
            sound_event(
                t.wwise_resource_gathering_sound_id,
                t.resource_gathering_sound_id,
                sounds,
            ),
            sound_event(
                t.wwise_resource_deposit_sound_id,
                t.resource_deposit_sound_id,
                sounds,
            ),
        ),
    })
}

/// The master load's remap of a base id (`FUN_1411319f0` at `0x141132363`..`0x14113249c`):
/// 768 becomes 420, 770 becomes 691.
const fn base_id(id: i16) -> i16 {
    match id {
        0x300 => 0x1a4,
        0x302 => 0x2b3,
        _ => id,
    }
}

/// The work-command values of a `.dat` unit, with the world's sound table its tasks read.
///
/// # Errors
///
/// A task or a cost slot that holds no type or attribute ([`dat_task`]).
pub(super) fn dat_work(
    id: MasterId,
    unit: &genie_dat::de::Unit,
    sounds: &[genie_dat::de::Sound],
) -> Result<work::Master, GenieError> {
    Ok(work::Master {
        id,
        kind: genie_core::objects::masters::MasterKind::of_data(unit.unit_type),
        copy_id: MasterId::of_data(unit.copy_id),
        base_id: MasterId::of_data(base_id(unit.base_id)),
        class: genie_core::objects::masters::ClassId::of_master(unit.class),
        terrain_restriction: genie_core::map::RestrictionId::of_field(unit.terrain_restriction)?,
        task_by_group: unit.bird.as_ref().map_or(0, |b| b.task_swap_group),
        radius: Radii::from(unit.collision_size).ground(),
        tall: 0.0 < Radii::from(unit.collision_size).z(),
        height: Radii::from(unit.collision_size).z(),
        obstruction_type: ObstructionType::from(unit.obstruction_type),
        buildable_on: unit.can_be_built_on,
        obstruction_class: ObstructionClass::from(unit.obstruction_class),
        object_flags: 0,
        combat_level: unit.combat_level,
        tasks: dat_tasks(unit, sounds)?,
        default_task: unit
            .bird
            .as_ref()
            .map(|b| b.default_task_id)
            .filter(|&t| 0 <= t),
        cost: dat_cost(unit)?,
        work_rate: unit
            .bird
            .as_ref()
            .map_or(WorkRate::ZERO, |b| WorkRate::new(b.work_rate)),
        drop_sites: unit.bird.as_ref().map_or_else(Vec::new, |b| {
            b.drop_sites
                .iter()
                .filter_map(|&d| MasterId::of_data(d))
                .collect()
        }),
        hit_points: unit.hit_points,
        capacity: unit.resource_capacity,
        build_time: unit
            .creatable
            .as_ref()
            .and_then(|c| c.train_locations.first())
            .map_or(0, |t| t.queue_time),
        trained_at: unit.creatable.as_ref().map_or_else(Vec::new, |c| {
            c.train_locations
                .iter()
                .filter_map(|t| MasterId::of_data(t.location_id))
                .collect()
        }),
        // the whole entries, which the attribute setters change (a .dat before VER 8.7 holds no
        // hotkey: 0)
        train_locations: unit.creatable.as_ref().map_or_else(Vec::new, |c| {
            c.train_locations
                .iter()
                .map(|t| work::TrainLocation {
                    location: MasterId::of_data(t.location_id),
                    time: t.queue_time,
                    button: t.button_id,
                    hotkey: t.hot_key_id.unwrap_or_default(),
                })
                .collect()
        }),
        attack_sprite2: unit
            .type50
            .as_ref()
            .and_then(|t| t.attack_graphic2)
            .and_then(SpriteId::of_data),
        idle_attack_sprite: unit
            .creatable
            .as_ref()
            .and_then(|c| c.idle_attack_graphic)
            .and_then(SpriteId::of_data),
        clearance: Footprint::from(unit.clearance_size),
        outline: Radii::from(unit.outline_size).ground(),
        outline_height: Radii::from(unit.outline_size).z(),
        terrain_bonus: unit.type50.as_ref().map_or(-1, |t| t.defense_terrain_bonus),
        building: unit.building.as_ref().map(|b| work::BuildingMaster {
            adjacent_mode: b.adjacent_mode,
            disappears_when_built: b.disappears_when_built != 0,
            stack_unit: MasterId::of_data(b.stack_unit_id),
            foundation_terrain: b.foundation_terrain_id,
            tech: genie_core::technology::TechId::of_data(b.tech_id),
            garrison_type: b.garrison_type,
            garrison_heal_rate: HitPoints::new(b.garrison_heal_rate),
            garrison_repair_rate: HitPoints::new(b.garrison_repair_rate),
            annexes: b.annexes.map(|a| MasterId::of_data(a.unit_id)),
            annex_offsets: b.annexes.map(|a| GroundOffset::from(a.misplacement)),
            head_unit: MasterId::of_data(b.head_unit),
            transform_unit: MasterId::of_data(b.transform_unit),
            destruction_sprite: SpriteId::of_data(b.destruction_graphic_id),
            construction_sprite: SpriteId::of_data(b.construction_graphic_id),
        }),
        garrison: unit.garrison_capacity,
        recyclable: unit.recyclable != 0,
        auto_gather: unit.enable_auto_gather != 0,
        gather_group: GatherGroup::from(unit.resource_gather_group),
        hill_mode: unit.hill_mode,
        fog_visibility: unit.fog_visibility,
        available: unit.enabled != 0,
        disabled: unit.disabled != 0,
        ghost_on_death: unit.create_doppelganger_on_death,
        placement: [
            unit.placement_side_terrain[0],
            unit.placement_side_terrain[1],
            unit.placement_terrain[0],
            unit.placement_terrain[1],
        ],
        special_graphic: unit
            .creatable
            .as_ref()
            .is_some_and(|c| 0 <= c.special_graphic),
        special_sprite: unit
            .creatable
            .as_ref()
            .and_then(|c| i16::try_from(c.special_graphic).ok())
            .and_then(SpriteId::of_data),
        death_sprite: SpriteId::of_data(unit.dying_graphic),
        death_spawns: [unit.dead_unit_id, unit.blood_unit_id].map(MasterId::of_data),
        garrison_firepower: unit
            .type50
            .as_ref()
            .and_then(|t| t.garrison_firepower)
            .unwrap_or(0.0),
        volley_fire: unit.creatable.as_ref().map_or(0.0, |c| c.total_projectiles),
        // the DE master's mSpacingModifier (+0x8b8), the data file's "flank attack modifier"
        spacing_modifier: unit
            .creatable
            .as_ref()
            .map_or(0.0, |c| c.flank_attack_modifier),
        max_volley: unit
            .creatable
            .as_ref()
            .map_or(0, |c| c.max_total_projectiles),
        volley_spread: unit
            .creatable
            .as_ref()
            .map_or_else(VolleySpread::default, |c| {
                let [x, y, start] = c.projectile_spawning_area;
                VolleySpread { x, y, start }
            }),
        volley_missile: unit
            .creatable
            .as_ref()
            .and_then(|c| i16::try_from(c.secondary_projectile_unit).ok())
            .and_then(MasterId::of_data),
        charge_projectile: unit
            .creatable
            .as_ref()
            .and_then(|c| c.charge_projectile_unit)
            .and_then(|id| i16::try_from(id).ok())
            .and_then(MasterId::of_data),
        ..work::Master::default()
    })
}

/// The tasks of a `.dat` unit's action part ([`dat_task`]), none without one.
///
/// # Errors
///
/// A task that holds no type or attribute.
fn dat_tasks(
    unit: &genie_dat::de::Unit,
    sounds: &[genie_dat::de::Sound],
) -> Result<Vec<work::Task>, GenieError> {
    unit.bird.as_ref().map_or_else(
        || Ok(Vec::new()),
        |b| b.tasks.iter().map(|t| dat_task(t, sounds)).collect(),
    )
}

/// A `.dat` unit's cost slots (`resource_costs`) in their places, empty without a creatable
/// part.
///
/// # Errors
///
/// A slot whose attribute is below −1.
fn dat_cost(unit: &genie_dat::de::Unit) -> Result<[CostSlot; COST_SLOTS], GenieError> {
    let mut cost = [CostSlot::default(); COST_SLOTS];
    if let Some(c) = unit.creatable.as_ref() {
        for (slot, r) in cost.iter_mut().zip(&c.resource_costs) {
            *slot = CostSlot {
                attribute: AttributeId::of_field(r.kind)?,
                amount: r.amount,
                paid: r.flag != 0,
            };
        }
    }
    Ok(cost)
}

/// The attributes of fields that hold −1 for none, in their order (a task's four, its
/// enablement).
///
/// # Errors
///
/// A value below −1, which no attribute is.
pub(crate) fn attributes<const N: usize>(
    values: [i16; N],
) -> Result<[Option<AttributeId>; N], GenieError> {
    let mut out = [None; N];
    for (slot, value) in out.iter_mut().zip(values) {
        *slot = AttributeId::of_field(value)?;
    }
    Ok(out)
}
