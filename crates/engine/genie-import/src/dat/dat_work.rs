//! A `.dat` master's work-command values (genie-core's [`work::Master`]): its tasks with their
//! sound events, its costs in their slots and the rest the work command reads.

use genie_dat::de::{Sound, Task, Unit};
use genie_model::GenieError;
use genie_model::combat::Firepower;
use genie_model::combat::MissileCount;
use genie_model::map::Distance;
use genie_model::map::RestrictionId;
use genie_model::map::TerrainId;
use genie_model::map::{Footprint, GroundOffset, Radii};
use genie_model::objects::HitPoints;
use genie_model::objects::MasterHitPoints;
use genie_model::objects::SpriteId;
use genie_model::objects::WorkRate;
use genie_model::objects::masters as work;
use genie_model::objects::masters::ActionType;
use genie_model::objects::masters::AdjacentMode;
use genie_model::objects::masters::CarryCapacity;
use genie_model::objects::masters::CombatLevel;
use genie_model::objects::masters::CostAmount;
use genie_model::objects::masters::FogVisibility;
use genie_model::objects::masters::GarrisonCapacity;
use genie_model::objects::masters::GarrisonKinds;
use genie_model::objects::masters::GatherGroup;
use genie_model::objects::masters::GhostMode;
use genie_model::objects::masters::HillMode;
use genie_model::objects::masters::MasterId;
use genie_model::objects::masters::ObjectFlags;
use genie_model::objects::masters::ObstructionClass;
use genie_model::objects::masters::ObstructionType;
use genie_model::objects::masters::PlacementTerrains;
use genie_model::objects::masters::RestrictionField;
use genie_model::objects::masters::SoundEvents;
use genie_model::objects::masters::TargetOwner;
use genie_model::objects::masters::TaskFlags;
use genie_model::objects::masters::TaskGroup;
use genie_model::objects::masters::TaskIndex;
use genie_model::objects::masters::TaskSprites;
use genie_model::objects::masters::TaskTerrain;
use genie_model::objects::masters::VolleySpread;
use genie_model::objects::masters::{BuildableOn, ClassId, MasterKind, TaskId};
use genie_model::objects::masters::{COST_SLOTS, CostSlot};
use genie_model::players::attribute::AttributeId;
use genie_model::production::HotkeyId;
use genie_model::production::LocationButton;
use genie_model::technology::TechId;
use genie_model::world::Seconds;
use genie_model::world::WholeSeconds;

/// The default event id a task's load gives a sound without one (`FUN_141062470`'s
/// `FUN_1413af9a0` calls, an immediate of the code).
const DEFAULT_SOUND_EVENT: u32 = 0x62d4_b3ec;

/// The event id a task's load (`FUN_141062470`) leaves at `+0x60` or `+0x78`
/// (`FUN_1413af9a0` with the world's sound table): an id of 0 whose sound number names a sound
/// of the table with sample files takes the default event ([`DEFAULT_SOUND_EVENT`]); any other
/// id stays. The id is read as an `int` (`MOVSXD`).
pub(super) fn sound_event(event: u32, sound: i16, sounds: &[Sound]) -> i32 {
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

/// A `.dat` task as the task finder reads it (`genie_model::objects::masters::Task`), with the world's
/// sound table its sound events read ([`sound_event`]).
///
/// # Errors
///
/// A negative action type, or an attribute below −1.
fn dat_task(t: &Task, sounds: &[Sound]) -> Result<work::Task, GenieError> {
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
        id: TaskId::of_task(t.id),
        action_type: ActionType::try_from(t.action_type)?,
        object_group: ClassId::of_field(t.class_id)?,
        object_id: MasterId::of_data(t.unit_id),
        combat_level: t.enable_targeting,
        combat_level_flag: TaskFlags::from_bits_retain(t.combat_level_flag),
        terrain: TaskTerrain::of_data(t.terrain_id),
        owner_type: TargetOwner::of_data(t.target_diplomacy),
        holding_attr: t.carry_check,
        attribute_type,
        attribute_type2,
        attribute_type3,
        attribute_type4,
        wait_time: Seconds::new(t.search_wait_time),
        work_val1: work::WorkValue::new(t.work_value1),
        work_val2: work::WorkValue::new(t.work_value2),
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
    unit: &Unit,
    sounds: &[Sound],
) -> Result<work::Master, GenieError> {
    Ok(work::Master {
        id,
        kind: MasterKind::of_data(unit.unit_type),
        copy_id: MasterId::of_data(unit.copy_id),
        base_id: MasterId::of_data(base_id(unit.base_id)),
        class: ClassId::of_master(unit.class),
        terrain_restriction: RestrictionId::of_field(unit.terrain_restriction)?,
        task_by_group: TaskGroup::of_data(unit.bird.as_ref().map_or(0, |b| b.task_swap_group)),
        radius: Radii::from(unit.collision_size).ground(),
        tall: 0.0 < Radii::from(unit.collision_size).z(),
        height: Distance::new(Radii::from(unit.collision_size).z()),
        obstruction_type: ObstructionType::from(unit.obstruction_type),
        buildable_on: BuildableOn::of_data(unit.can_be_built_on),
        obstruction_class: ObstructionClass::from(unit.obstruction_class),
        object_flags: ObjectFlags::NONE,
        combat_level: CombatLevel::of_data(unit.combat_level),
        tasks: dat_tasks(unit, sounds)?,
        default_task: unit
            .bird
            .as_ref()
            .and_then(|b| TaskIndex::of_data(b.default_task_id)),
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
        hit_points: MasterHitPoints::of_data(unit.hit_points),
        capacity: CarryCapacity::of_data(unit.resource_capacity),
        build_time: unit
            .creatable
            .as_ref()
            .and_then(|c| c.train_locations.first())
            .map_or(WholeSeconds::ZERO, |t| WholeSeconds::of_data(t.queue_time)),
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
                    time: WholeSeconds::of_data(t.queue_time),
                    button: LocationButton::of_data(t.button_id),
                    hotkey: HotkeyId::of_data(t.hot_key_id.unwrap_or_default()),
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
        outline_height: Distance::new(Radii::from(unit.outline_size).z()),
        terrain_bonus: RestrictionField::of_data(
            unit.type50.as_ref().map_or(-1, |t| t.defense_terrain_bonus),
        ),
        building: unit.building.as_ref().map(|b| work::BuildingMaster {
            adjacent_mode: AdjacentMode::of_data(b.adjacent_mode),
            disappears_when_built: b.disappears_when_built != 0,
            stack_unit: MasterId::of_data(b.stack_unit_id),
            foundation_terrain: TerrainId::of_data(b.foundation_terrain_id),
            tech: TechId::of_data(b.tech_id),
            garrison_type: GarrisonKinds::from_bits_retain(b.garrison_type),
            garrison_heal_rate: HitPoints::new(b.garrison_heal_rate),
            garrison_repair_rate: HitPoints::new(b.garrison_repair_rate),
            annexes: b.annexes.map(|a| MasterId::of_data(a.unit_id)),
            annex_offsets: b.annexes.map(|a| GroundOffset::from(a.misplacement)),
            head_unit: MasterId::of_data(b.head_unit),
            transform_unit: MasterId::of_data(b.transform_unit),
            destruction_sprite: SpriteId::of_data(b.destruction_graphic_id),
            construction_sprite: SpriteId::of_data(b.construction_graphic_id),
        }),
        garrison: GarrisonCapacity::of_data(unit.garrison_capacity),
        recyclable: unit.recyclable != 0,
        auto_gather: unit.enable_auto_gather != 0,
        gather_group: GatherGroup::from(unit.resource_gather_group),
        hill_mode: HillMode::of_data(unit.hill_mode),
        fog_visibility: FogVisibility::of_data(unit.fog_visibility),
        available: unit.enabled != 0,
        disabled: unit.disabled != 0,
        ghost_on_death: GhostMode::of_data(unit.create_doppelganger_on_death),
        placement: PlacementTerrains::of_data([
            unit.placement_side_terrain[0],
            unit.placement_side_terrain[1],
            unit.placement_terrain[0],
            unit.placement_terrain[1],
        ]),
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
            .map_or(Firepower::ZERO, Firepower::new),
        volley_fire: Firepower::new(unit.creatable.as_ref().map_or(0.0, |c| c.total_projectiles)),
        // the DE master's mSpacingModifier (+0x8b8), the data file's "flank attack modifier"
        spacing_modifier: work::SpacingModifier::new(
            unit.creatable
                .as_ref()
                .map_or(0.0, |c| c.flank_attack_modifier),
        ),
        max_volley: MissileCount::of_data(
            unit.creatable
                .as_ref()
                .map_or(0, |c| c.max_total_projectiles),
        ),
        volley_spread: unit
            .creatable
            .as_ref()
            .map_or_else(VolleySpread::default, |c| {
                let [x, y, start] = c.projectile_spawning_area;
                VolleySpread {
                    x: Distance::new(x),
                    y: Distance::new(y),
                    start: Distance::new(start),
                }
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
fn dat_tasks(unit: &Unit, sounds: &[Sound]) -> Result<Vec<work::Task>, GenieError> {
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
fn dat_cost(unit: &Unit) -> Result<[CostSlot; COST_SLOTS], GenieError> {
    let mut cost = [CostSlot::default(); COST_SLOTS];
    if let Some(c) = unit.creatable.as_ref() {
        for (slot, r) in cost.iter_mut().zip(&c.resource_costs) {
            *slot = CostSlot {
                attribute: AttributeId::of_field(r.kind)?,
                amount: CostAmount::of_data(r.amount),
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
pub fn attributes<const N: usize>(
    values: [i16; N],
) -> Result<[Option<AttributeId>; N], GenieError> {
    let mut out = [None; N];
    for (slot, value) in out.iter_mut().zip(values) {
        *slot = AttributeId::of_field(value)?;
    }
    Ok(out)
}
