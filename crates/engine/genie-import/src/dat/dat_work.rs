//! A `.dat` master's work-command values (genie-core's [`work::Master`]): its tasks with their
//! sound events, its costs in their slots and the rest the work command reads.

use genie_dat::de::{Bird, Creatable, DeadFish, Projectile, Sound, Task, Type50, Unit};
use genie_model::GenieResult;
use genie_model::combat::Firepower;
use genie_model::combat::MissileCount;
use genie_model::combat::missile::{ArcRatio, HitMode, ProjectileKind, SmartMode, VanishMode};
use genie_model::map::Distance;
use genie_model::map::RestrictionId;
use genie_model::map::TerrainId;
use genie_model::map::{Footprint, GroundOffset, Radii};
use genie_model::movement::Speed;
use genie_model::objects::HitPoints;
use genie_model::objects::MasterHitPoints;
use genie_model::objects::SpriteId;
use genie_model::objects::WorkRate;
use genie_model::objects::masters as work;
use genie_model::objects::masters::ActionKind;
use genie_model::objects::masters::AdjacentMode;
use genie_model::objects::masters::AutoGatherMode;
use genie_model::objects::masters::Availability;
use genie_model::objects::masters::BuiltMode;
use genie_model::objects::masters::CarryCapacity;
use genie_model::objects::masters::CombatLevel;
use genie_model::objects::masters::CostAmount;
use genie_model::objects::masters::DisableMode;
use genie_model::objects::masters::FogVisibility;
use genie_model::objects::masters::GarrisonCapacity;
use genie_model::objects::masters::GarrisonKinds;
use genie_model::objects::masters::GatherGroup;
use genie_model::objects::masters::GhostMode;
use genie_model::objects::masters::HillMode;
use genie_model::objects::masters::MasterId;
use genie_model::objects::masters::ObjectFlags;
use genie_model::objects::masters::ObstructionClass;
use genie_model::objects::masters::ObstructionKind;
use genie_model::objects::masters::PlacementTerrains;
use genie_model::objects::masters::RecycleMode;
use genie_model::objects::masters::RestrictionField;
use genie_model::objects::masters::RunPattern;
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
        .and_then(|index| sounds.get(index))
        .is_some_and(|sound| !sound.items.is_empty());
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
fn dat_task(task: &Task, sounds: &[Sound]) -> GenieResult<work::Task> {
    let [
        attribute_kind,
        attribute_kind2,
        attribute_kind3,
        attribute_kind4,
    ] = attributes([
        task.resource_in,
        task.resource_multiplier,
        task.resource_out,
        task.unused_resource,
    ])?;
    Ok(work::Task {
        id: TaskId::of_task(task.id),
        action_kind: ActionKind::try_from(task.action_kind)?,
        object_group: ClassId::of_field(task.class_id)?,
        object_id: MasterId::of_data(task.unit_id),
        combat_level: task.enable_targeting,
        combat_level_flag: TaskFlags::from_bits_retain(task.combat_level_flag),
        terrain: TaskTerrain::of_data(task.terrain_id),
        owner_kind: TargetOwner::of_data(task.target_diplomacy),
        holding_attr: task.carry_check,
        search_flag: task.auto_search_targets,
        attribute_kind,
        attribute_kind2,
        attribute_kind3,
        attribute_kind4,
        wait_time: Seconds::new(task.search_wait_time),
        work_val1: work::WorkValue::new(task.work_value1),
        work_val2: work::WorkValue::new(task.work_value2),
        work_range: Distance::new(task.work_range),
        work_flag: task.gather_kind,
        work_flag2: task.work_flag2,
        enablement: task
            .enabled
            .map(AttributeId::of_field)
            .transpose()?
            .flatten(),
        sprites: TaskSprites::of_data([
            task.moving_graphic_id,
            task.proceeding_graphic_id,
            task.working_graphic_id,
            task.carrying_graphic_id,
        ]),
        sound_events: SoundEvents::new(
            sound_event(
                task.wwise_resource_gathering_sound_id,
                task.resource_gathering_sound_id,
                sounds,
            ),
            sound_event(
                task.wwise_resource_deposit_sound_id,
                task.resource_deposit_sound_id,
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
pub(super) fn dat_work(id: MasterId, unit: &Unit, sounds: &[Sound]) -> GenieResult<work::Master> {
    Ok(work::Master {
        id,
        kind: MasterKind::of_data(unit.unit_kind),
        copy_id: MasterId::of_data(unit.copy_id),
        base_id: MasterId::of_data(base_id(unit.base_id)),
        class: ClassId::of_master(unit.class),
        terrain_restriction: RestrictionId::of_field(unit.terrain_restriction)?,
        radius: Radii::from(unit.collision_size).ground(),
        height: Distance::new(Radii::from(unit.collision_size).z()),
        obstruction_kind: ObstructionKind::from(unit.obstruction_kind),
        buildable_on: BuildableOn::of_data(unit.can_be_built_on),
        obstruction_class: ObstructionClass::from(unit.obstruction_class),
        object_flags: ObjectFlags::NONE,
        combat_level: CombatLevel::of_data(unit.combat_level),
        hit_points: MasterHitPoints::of_data(unit.hit_points),
        capacity: CarryCapacity::of_data(unit.resource_capacity),
        clearance: Footprint::from(unit.clearance_size),
        outline: Radii::from(unit.outline_size).ground(),
        outline_height: Distance::new(Radii::from(unit.outline_size).z()),
        // the save's facet counts; which `.dat` field the loader reads into `+0x982` is not
        // known (MISSING.md)
        building: unit.building.as_ref().map(|building| work::BuildingMaster {
            adjacent_mode: AdjacentMode::of_data(building.adjacent_mode),
            disappears_when_built: BuiltMode::of_data(building.disappears_when_built),
            stack_unit: MasterId::of_data(building.stack_unit_id),
            foundation_terrain: TerrainId::of_data(building.foundation_terrain_id),
            tech: TechId::of_data(building.tech_id),
            garrison_kind: GarrisonKinds::from_bits_retain(building.garrison_kind),
            garrison_heal_rate: HitPoints::new(building.garrison_heal_rate),
            garrison_repair_rate: HitPoints::new(building.garrison_repair_rate),
            annexes: building
                .annexes
                .map(|building_annex| MasterId::of_data(building_annex.unit_id)),
            annex_offsets: building
                .annexes
                .map(|building_annex| GroundOffset::from(building_annex.misplacement)),
            head_unit: MasterId::of_data(building.head_unit),
            transform_unit: MasterId::of_data(building.transform_unit),
            destruction_sprite: SpriteId::of_data(building.destruction_graphic_id),
            construction_sprite: SpriteId::of_data(building.construction_graphic_id),
            ..work::BuildingMaster::default()
        }),
        garrison: GarrisonCapacity::of_data(unit.garrison_capacity),
        recyclable: RecycleMode::of_data(unit.recyclable),
        auto_gather: AutoGatherMode::of_data(unit.enable_auto_gather),
        gather_group: GatherGroup::from(unit.resource_gather_group),
        hill_mode: HillMode::of_data(unit.hill_mode),
        fog_visibility: FogVisibility::of_data(unit.fog_visibility),
        available: Availability::of_data(unit.enabled),
        disabled: DisableMode::of_data(unit.disabled),
        ghost_on_death: GhostMode::of_data(unit.create_doppelganger_on_death),
        placement: PlacementTerrains::of_data([
            unit.placement_side_terrain[0],
            unit.placement_side_terrain[1],
            unit.placement_terrain[0],
            unit.placement_terrain[1],
        ]),
        death_sprite: SpriteId::of_data(unit.dying_graphic),
        death_spawns: [unit.dead_unit_id, unit.blood_unit_id].map(MasterId::of_data),
        animated: unit.speed.map(|speed| work::AnimatedPart {
            max_speed: Speed::new(speed),
        }),
        moving: unit.dead_fish.as_ref().map(dat_moving),
        creatable: unit.creatable.as_ref().map(dat_creatable).transpose()?,
        combat: unit.type50.as_ref().map(dat_combat),
        missile: unit.projectile.as_ref().map(dat_missile),
        action: unit
            .bird
            .as_ref()
            .map(|bird| dat_action(bird, sounds))
            .transpose()?,
        ..work::Master::default()
    })
}

/// The moving layer of a `.dat` unit's type-30 section ([`work::MovingPart`], genieutils'
/// `DeadFish`): its walking and running sprites, its turn speed and its trail.
const fn dat_moving(dead_fish: &DeadFish) -> work::MovingPart {
    work::MovingPart {
        move_sprite: SpriteId::of_data(dead_fish.walking_graphic),
        run_sprite: SpriteId::of_data(dead_fish.running_graphic),
        turn_speed: Seconds::new(dead_fish.rotation_speed),
        trail: work::Trail {
            unit: work::MasterField::of_data(dead_fish.tracking_unit),
            mode: work::TrailMode::of_data(dead_fish.tracking_unit_mode),
            density: work::TrailDensity::new(dead_fish.tracking_unit_density),
        },
    }
}

/// The missile layer of a `.dat` unit's projectile section ([`work::MissilePart`], genieutils'
/// `Projectile`): its projectile type, smart, hit and vanish modes and its arc.
const fn dat_missile(projectile: &Projectile) -> work::MissilePart {
    work::MissilePart {
        projectile_kind: ProjectileKind::of_data(projectile.projectile_kind),
        smart_mode: SmartMode::from_bits_retain(projectile.smart_mode),
        hit_mode: HitMode::of_data(projectile.hit_mode),
        vanish_mode: VanishMode::from_bits_retain(projectile.vanish_mode),
        arc: ArcRatio::new(projectile.projectile_arc),
    }
}

/// The combat layer of a `.dat` unit's type-50 section ([`work::CombatPart`]): the values the
/// import reads of it, its second attack sprite, terrain bonus and garrison firepower (the save
/// holds the rest, `genie_import::save`).
fn dat_combat(type50: &Type50) -> work::CombatPart {
    work::CombatPart {
        attack_sprite2: type50.attack_graphic2.and_then(SpriteId::of_data),
        terrain_bonus: RestrictionField::of_data(type50.defense_terrain_bonus),
        garrison_firepower: type50
            .garrison_firepower
            .map_or(Firepower::ZERO, Firepower::new),
        ..work::CombatPart::default()
    }
}

/// The creatable layer of a `.dat` unit's creatable section ([`work::CreatablePart`]): its
/// costs, train locations, special graphic and volley values.
///
/// # Errors
///
/// A cost slot that holds no attribute ([`dat_cost`]).
fn dat_creatable(creatable: &Creatable) -> GenieResult<work::CreatablePart> {
    Ok(work::CreatablePart {
        cost: dat_cost(creatable)?,
        build_time: creatable
            .train_locations
            .first()
            .map_or(WholeSeconds::ZERO, |train_location| {
                WholeSeconds::of_data(train_location.queue_time)
            }),
        trained_at: creatable
            .train_locations
            .iter()
            .filter_map(|train_location| MasterId::of_data(train_location.location_id))
            .collect(),
        // the whole entries, which the attribute setters change (a .dat before VER 8.7 holds no
        // hotkey: 0)
        train_locations: creatable
            .train_locations
            .iter()
            .map(|train_location| work::TrainLocation {
                location: MasterId::of_data(train_location.location_id),
                time: WholeSeconds::of_data(train_location.queue_time),
                button: LocationButton::of_data(train_location.button_id),
                hotkey: HotkeyId::of_data(train_location.hot_key_id.unwrap_or_default()),
            })
            .collect(),
        idle_attack_sprite: creatable.idle_attack_graphic.and_then(SpriteId::of_data),
        special_sprite: i16::try_from(creatable.special_graphic)
            .ok()
            .and_then(SpriteId::of_data),
        volley_fire: Firepower::new(creatable.total_projectiles),
        // the DE master's mSpacingModifier (+0x8b8), the data file's "flank attack modifier"
        spacing_modifier: work::SpacingModifier::new(creatable.flank_attack_modifier),
        max_volley: MissileCount::of_data(creatable.max_total_projectiles),
        volley_spread: {
            let [x, y, start] = creatable.projectile_spawning_area;
            VolleySpread {
                x: Distance::new(x),
                y: Distance::new(y),
                start: Distance::new(start),
            }
        },
        volley_missile: i16::try_from(creatable.secondary_projectile_unit)
            .ok()
            .and_then(MasterId::of_data),
        charge_projectile: creatable
            .charge_projectile_unit
            .and_then(|id| i16::try_from(id).ok())
            .and_then(MasterId::of_data),
        ..work::CreatablePart::default()
    })
}

/// The action layer of a `.dat` unit's type-40 section ([`work::ActionPart`], genieutils'
/// `Bird`): its tasks ([`dat_task`], with the world's sound table), default task, task swap
/// group, work rate, search radius, drop sites and run pattern.
///
/// # Errors
///
/// A task that holds no type or attribute.
fn dat_action(bird: &Bird, sounds: &[Sound]) -> GenieResult<work::ActionPart> {
    Ok(work::ActionPart {
        task_by_group: TaskGroup::of_data(bird.task_swap_group),
        tasks: bird
            .tasks
            .iter()
            .map(|task| dat_task(task, sounds))
            .collect::<Result<_, _>>()?,
        default_task: TaskIndex::of_data(bird.default_task_id),
        work_rate: WorkRate::new(bird.work_rate),
        search_radius: Distance::new(bird.search_radius),
        drop_sites: bird
            .drop_sites
            .iter()
            .filter_map(|&id| MasterId::of_data(id))
            .collect(),
        run_pattern: RunPattern::of_data(bird.run_pattern),
    })
}

/// A `.dat` creatable section's cost slots (`resource_costs`) in their places.
///
/// # Errors
///
/// A slot whose attribute is below −1.
fn dat_cost(creatable: &Creatable) -> GenieResult<[CostSlot; COST_SLOTS]> {
    let mut cost = [CostSlot::default(); COST_SLOTS];
    for (slot, resource_cost) in cost.iter_mut().zip(&creatable.resource_costs) {
        *slot = CostSlot {
            attribute: AttributeId::of_field(resource_cost.kind)?,
            amount: CostAmount::of_data(resource_cost.amount),
            paid: resource_cost.flag != 0,
        };
    }
    Ok(cost)
}

/// The attributes of fields that hold −1 for none, in their order (a task's four, its
/// enablement).
///
/// # Errors
///
/// A value below −1, which no attribute is.
pub fn attributes<const N: usize>(values: [i16; N]) -> GenieResult<[Option<AttributeId>; N]> {
    let mut out = [None; N];
    for (slot, value) in out.iter_mut().zip(values) {
        *slot = AttributeId::of_field(value)?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use crate::dat::dat_work::dat_action;
    use crate::dat::dat_work::dat_missile;
    use crate::dat::dat_work::dat_moving;
    use genie_dat::de::Bird;
    use genie_dat::de::DeadFish;
    use genie_dat::de::Projectile;
    use genie_model::combat::missile::{ArcRatio, HitMode, ProjectileKind, SmartMode, VanishMode};
    use genie_model::map::Distance;
    use genie_model::objects::SpriteId;
    use genie_model::objects::WorkRate;
    use genie_model::objects::masters::MasterField;
    use genie_model::objects::masters::MissilePart;
    use genie_model::objects::masters::{MasterId, RunPattern, TaskGroup, TaskIndex};
    use genie_model::objects::masters::{Trail, TrailDensity, TrailMode};
    use genie_model::world::Seconds;

    /// A type-40 section with two drop sites, the second none (−1), and no tasks.
    fn bird() -> Bird {
        Bird {
            default_task_id: 1,
            search_radius: 4.5,
            work_rate: 0.75,
            drop_site_count: Some(2),
            drop_sites: vec![109, -1],
            task_swap_group: 1,
            attack_sound: -1,
            move_sound: -1,
            wwise_attack_sound_id: 0,
            wwise_move_sound_id: 0,
            run_pattern: 3,
            task_count: Some(0),
            tasks: Vec::new(),
        }
    }

    /// The action layer holds the type-40 section's values: the search radius and the run
    /// pattern as well, which the save holds too; a drop site of −1 is none.
    #[test]
    fn the_action_layer_holds_the_kind_40_sections_values() {
        let part = dat_action(&bird(), &[]).unwrap();
        assert_eq!(part.default_task, TaskIndex::of_data(1));
        assert_eq!(part.task_by_group, TaskGroup::of_data(1));
        assert_eq!(part.work_rate, WorkRate::new(0.75));
        assert_eq!(part.search_radius, Distance::new(4.5));
        assert_eq!(
            part.drop_sites,
            MasterId::of_data(109).into_iter().collect::<Vec<_>>()
        );
        assert_eq!(part.run_pattern, RunPattern::of_data(3));
        assert_eq!(part.tasks, Vec::new());
    }

    /// A type-30 section that leaves footprints (trail unit 1002, mode 1, density 0.5) and runs
    /// with no sprite of its own (−1).
    fn dead_fish() -> DeadFish {
        DeadFish {
            walking_graphic: 640,
            running_graphic: -1,
            rotation_speed: 0.25,
            old_size_class: 0,
            tracking_unit: 1002,
            tracking_unit_mode: 1,
            tracking_unit_density: 0.5,
            old_move_algorithm: 0,
            turn_radius: 0.0,
            turn_radius_speed: 3.4e38,
            max_yaw_per_second_moving: 0.0,
            stationary_yaw_revolution_time: 0.0,
            max_yaw_per_second_stationary: 0.0,
            min_collision_size_multiplier: 1.0,
        }
    }

    /// The moving layer holds the type-30 section's sprites, turn speed and trail; a running
    /// sprite of −1 is none.
    #[test]
    fn the_moving_layer_holds_the_kind_30_sections_values() {
        let part = dat_moving(&dead_fish());
        assert_eq!(part.move_sprite, SpriteId::of_data(640));
        assert_eq!(part.run_sprite, None);
        assert_eq!(part.turn_speed, Seconds::new(0.25));
        assert_eq!(
            part.trail,
            Trail {
                unit: MasterField::of_data(1002),
                mode: TrailMode::of_data(1),
                density: TrailDensity::new(0.5),
            }
        );
    }

    /// The missile layer holds the projectile section's type, modes and arc.
    #[test]
    fn the_missile_layer_holds_the_projectile_sections_values() {
        let part = dat_missile(&Projectile {
            projectile_kind: 1,
            smart_mode: 1,
            hit_mode: 2,
            vanish_mode: 1,
            area_effect_specials: 0,
            projectile_arc: 0.25,
        });
        assert_eq!(
            part,
            MissilePart {
                projectile_kind: ProjectileKind::of_data(1),
                smart_mode: SmartMode::from_bits_retain(1),
                hit_mode: HitMode::of_data(2),
                vanish_mode: VanishMode::from_bits_retain(1),
                arc: ArcRatio::new(0.25),
            }
        );
    }
}
