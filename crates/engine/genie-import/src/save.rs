//! A player's saved masters as the engine reads them: the facets, the action part and
//! tasks, the work command's masters, the movement and the placement.

pub mod parts;

use crate::dat::MasterTable;
use crate::dat::attributes;
use crate::dat::dat_id;
use crate::save::parts::{animated_master, combat_master, moving_master};
use genie_model::combat::Charge;
use genie_model::combat::Firepower;
use genie_model::combat::MissileCount;
use genie_model::combat::missile::MissSpread;
use genie_model::combat::missile::SmartMode;
use genie_model::combat::missile::VanishMode;
use genie_model::combat::missile::{ArcRatio, HitMode};
use genie_model::map::Distance;
use genie_model::map::Footprint;
use genie_model::map::Offset;
use genie_model::map::Radii;
use genie_model::map::RestrictionId;
use genie_model::map::TerrainId;
use genie_model::movement::Speed;
use genie_model::movement::{MoveStats, turn_rate};
use genie_model::objects::Frame;
use genie_model::objects::HitPoints;
use genie_model::objects::MasterHitPoints;
use genie_model::objects::SpeedMultiplier;
use genie_model::objects::SpriteFlags;
use genie_model::objects::SpriteId;
use genie_model::objects::WorkRate;
use genie_model::objects::facing::SpriteFacets;
use genie_model::objects::masters::AnimatedPart;
use genie_model::objects::masters::Availability;
use genie_model::objects::masters::CarryCapacity;
use genie_model::objects::masters::ChargeKind;
use genie_model::objects::masters::ChargeTarget;
use genie_model::objects::masters::ClassAmount;
use genie_model::objects::masters::CombatAbility;
use genie_model::objects::masters::CombatPart;
use genie_model::objects::masters::CostAmount;
use genie_model::objects::masters::DamageShare;
use genie_model::objects::masters::DisableMode;
use genie_model::objects::masters::FormationClass;
use genie_model::objects::masters::GarrisonCapacity;
use genie_model::objects::masters::GarrisonKinds;
use genie_model::objects::masters::HeroFlags;
use genie_model::objects::masters::HitChance;
use genie_model::objects::masters::InvulnerabilityLevel;
use genie_model::objects::masters::MasterField;
use genie_model::objects::masters::MissilePart;
use genie_model::objects::masters::MovingPart;
use genie_model::objects::masters::ObjectFlags;
use genie_model::objects::masters::RunPattern;
use genie_model::objects::masters::SoundEvents;
use genie_model::objects::masters::SpecialAttack;
use genie_model::objects::masters::TargetOwner;
use genie_model::objects::masters::TaskFlags;
use genie_model::objects::masters::TaskSprites;
use genie_model::objects::masters::TaskTerrain;
use genie_model::objects::masters::Trail;
use genie_model::objects::masters::TrailDensity;
use genie_model::objects::masters::TrailMode;
use genie_model::objects::masters::VolleySpread;
use genie_model::objects::masters::WorkValue;
use genie_model::objects::masters::{
    ActionKind, ClassId, MasterId, ObstructionClass, ObstructionKind,
};
use genie_model::objects::masters::{
    AttackPriority, BuildableOn, DamageAmount, DamageGraphic, DamageSpriteMode, HpPercent, Masters,
    Task, TaskId, TrainLocation,
};
use genie_model::objects::masters::{BlastDefense, BlastLevel};
use genie_model::objects::masters::{COST_SLOTS, CostSlot};
use genie_model::players::Amount;
use genie_model::players::CivId;
use genie_model::players::attribute::AttributeId;
use genie_model::production::HotkeyId;
use genie_model::production::LocationButton;
use genie_model::world::Seconds;
use genie_model::world::WholeSeconds;
use genie_model::{GenieError, GenieResult};
use genie_save::generated::world::{Master, PlayerObjects, RgeStaticObject};
use genie_save::generated::world::{
    RgeMasterActionObject, RgeMasterStaticObject, Task as WorldTask,
};

/// The facets every sprite a live object of a saved master shows ([`genie_model::objects::facing::SpriteFacets`]):
/// its standing, second standing, walking, running and attack sprites and its tasks' four, those
/// the data file has, all of one count and bit 1 of the sequence flags; None when they differ or
/// it has none. The dying sprite shows once the object is dead, where nothing turns it.
pub fn master_facets(
    master: &Master,
    rge_master_static_object: &RgeMasterStaticObject,
    table: &MasterTable,
) -> Option<SpriteFacets> {
    let mut sprites = vec![
        rge_master_static_object.idle_sprite_id,
        rge_master_static_object.idle2_sprite_id,
    ];
    if let Some(mv) = moving_master(master) {
        sprites.extend([mv.move_sprite_id, mv.run_sprite_id]);
    }
    if let Some(rge_master_combat_object) = combat_master(master) {
        sprites.push(rge_master_combat_object.fight_sprite_id);
    }
    if let Some(tasks) = action_part(master)
        .and_then(|rge_master_action_object| rge_master_action_object.tasks.as_deref())
    {
        for entry in &tasks.list {
            let task = &entry.task;
            sprites.extend([
                task.short_val,
                task.short_val_2,
                task.short_val_3,
                task.short_val_4,
            ]);
        }
    }
    let mut seen = sprites
        .into_iter()
        .filter_map(SpriteId::of_data)
        .filter_map(|id| table.graphics.get(id))
        .map(|graphic| SpriteFacets {
            count: graphic.facets,
            turns: graphic.flags.contains(SpriteFlags::TURNS),
        });
    let first = seen.next()?;
    seen.all(|sprite_facets| sprite_facets == first)
        .then_some(first)
}

/// A master's action part, whatever class it is (a missile's under its RGE combat part too):
/// what holds its work rate, search radius, task list and run pattern.
#[must_use]
pub fn action_part(master: &Master) -> Option<&RgeMasterActionObject> {
    None.or_else(|| {
        master
            .tribe_master_combat_object
            .as_deref()
            .map(|tribe_master_combat_object| {
                &tribe_master_combat_object
                    .rge_master_combat_object
                    .rge_master_action_object
            })
    })
    .or_else(|| {
        master
            .tribe_master_building_object
            .as_deref()
            .map(|tribe_master_building_object| {
                &tribe_master_building_object
                    .tribe_master_combat_object
                    .rge_master_combat_object
                    .rge_master_action_object
            })
    })
    .or_else(|| {
        master.rge_master.as_deref().and_then(|rge_master| {
            rge_master
                .rge_master_action_object
                .as_deref()
                .or_else(|| {
                    rge_master
                        .rge_master_combat_object
                        .as_deref()
                        .map(|rge_master_combat_object| {
                            &rge_master_combat_object.rge_master_action_object
                        })
                })
                .or_else(|| {
                    rge_master
                        .rge_master_missile_object
                        .as_deref()
                        .map(|mm| &mm.rge_master_combat_object.rge_master_action_object)
                })
        })
    })
}

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
    let i = usize::try_from(index)
        .map_err(|_| GenieError::unsupported("a saved action without its task"))?;
    let saved = objects
        .master(master)
        .and_then(action_part)
        .and_then(|rge_master_action_object| rge_master_action_object.tasks.as_deref());
    if let Some(tasks) = saved {
        let entry = tasks.list.get(i).ok_or(GenieError::unsupported(
            "a saved action's task beyond its master's list",
        ))?;
        return core_task(&entry.task);
    }
    masters
        .and_then(|master_table| master_table.work(civ, dat_id(objects, master)?))
        .and_then(|w| w.action_part().tasks.get(i))
        .copied()
        .ok_or(GenieError::unsupported(
            "a saved action's task the data file does not have",
        ))
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

/// A player's masters as the work command reads them: each master the save holds, with its saved
/// class, radii, cost and (where saved) tasks, and the `.dat`'s task swap group and type, which
/// the save does not carry (the game copies them from the `.dat` master).
pub fn work_masters(
    objects: &PlayerObjects,
    civ: CivId,
    table: &MasterTable,
) -> GenieResult<Masters> {
    let task = core_task;
    let saved = objects
        .masters
        .iter()
        .flatten()
        .map(|player_master| &player_master.master);
    let masters = saved.filter_map(|master| {
        let rge_master_static_object = master.static_part()?;
        // the `.dat` values of the master it copies (an upgraded one's), under its own id
        let mut work = table
            .work(civ, MasterId::of_data(rge_master_static_object.copy_id())?)?
            .clone();
        work.id = MasterId::of_data(rge_master_static_object.id())?;
        work.class = ClassId::of_master(rge_master_static_object.read_temp_5);
        work.radius = Footprint::new(
            rge_master_static_object.radius_x,
            rge_master_static_object.radius_y,
        );
        work.height = Distance::new(rge_master_static_object.radius_z);
        work.obstruction_kind = ObstructionKind::from(rge_master_static_object.obstruction_kind);
        work.buildable_on = BuildableOn::of_data(rge_master_static_object.can_be_built_on);
        work.obstruction_class = ObstructionClass::from(rge_master_static_object.obstruction_class);
        work.object_flags = ObjectFlags::from_bits_retain(rge_master_static_object.object_flags);
        work.hit_points = MasterHitPoints::of_data(rge_master_static_object.read_temp_6);
        work.available = Availability::of_data(rge_master_static_object.available);
        work.disabled = DisableMode::of_data(rge_master_static_object.disabled);
        work.line_of_sight = Distance::new(rge_master_static_object.los);
        work.capacity = CarryCapacity::of_data(rge_master_static_object.attribute_max_amount);
        work.blast_defense = BlastDefense::new(rge_master_static_object.area_effect_object_level);
        // the action layer is the save's RGE action part (its tasks where it saved them, else
        // the copy's): a master without one has no tasks, whatever its copy's are
        if let Some(rge_master_action_object) = action_part(master) {
            let action = work.action.get_or_insert_with(Default::default);
            action.work_rate = WorkRate::new(rge_master_action_object.work_rate);
            action.search_radius = Distance::new(rge_master_action_object.search_radius);
            action.run_pattern = RunPattern::of_data(rge_master_action_object.run_pattern);
            if let Some(tasks) = rge_master_action_object.tasks.as_deref() {
                action.tasks = match tasks.list.iter().map(|e| task(&e.task)).collect() {
                    Ok(tasks) => tasks,
                    Err(error) => return Some(Err(error)),
                };
            }
        } else {
            work.action = None;
        }
        let combat = master.tribe_master_combat_object.as_deref().or_else(|| {
            master
                .tribe_master_building_object
                .as_deref()
                .map(|tribe_master_building_object| {
                    &tribe_master_building_object.tribe_master_combat_object
                })
        });
        // the creatable layer is the save's TRIBE combat part: a master without one has no
        // costs or train locations, whatever its copy's are
        if let Some(c) = combat {
            let creatable_part = work.creatable.get_or_insert_with(Default::default);
            creatable_part.cost = build_cost(&c.build_inventory);
            creatable_part.build_time = c
                .train_location
                .first()
                .map_or(WholeSeconds::ZERO, |t| WholeSeconds::of_data(t.train_time));
            creatable_part.charge_kind = ChargeKind::of_data(c.charge_kind);
            creatable_part.formation_category = FormationClass::of_data(c.temp_formation_category);
            creatable_part.hero_flag = HeroFlags::from_bits_retain(c.hero_flag);
            creatable_part.volley_fire = Firepower::new(c.volley_fire_amount);
            creatable_part.max_volley = MissileCount::of_data(c.max_attacks_in_volley);
            creatable_part.volley_spread = VolleySpread {
                x: Distance::new(c.volley_xspread),
                y: Distance::new(c.volley_yspread),
                start: Distance::new(c.volley_start_spread_adjustment),
            };
            let id = |id: i32| i16::try_from(id).ok().and_then(MasterId::of_data);
            creatable_part.volley_missile = id(c.volley_missile_id);
            creatable_part.charge_projectile = id(c.charge_projectile_unit);
            creatable_part.invulnerability = InvulnerabilityLevel::new(c.invulnerability_level);
            creatable_part.attack_priority = AttackPriority::of_byte(c.attack_priority);
            creatable_part.special_attack = SpecialAttack::of_data(c.special_attack_flag);
            creatable_part.max_charge = Charge::new(c.max_charge);
            creatable_part.charge_target = ChargeTarget::of_data(c.charge_target);
            creatable_part.train_locations = c
                .train_location
                .iter()
                .map(|t| TrainLocation {
                    location: MasterId::of_data(t.train_location_id),
                    time: WholeSeconds::of_data(t.train_time),
                    button: LocationButton::of_data(t.button_location_id),
                    hotkey: HotkeyId::of_data(t.hotkey_id),
                })
                .collect();
            creatable_part.trained_at = creatable_part
                .train_locations
                .iter()
                .filter_map(|train_location| train_location.location)
                .collect();
        } else {
            work.creatable = None;
        }
        // the combat part a missile master has too (`RGE_Master_Missile_Object`'s base)
        let rge_combat = combat
            .map(|tribe_master_combat_object| &tribe_master_combat_object.rge_master_combat_object)
            .or_else(|| {
                master
                    .rge_master
                    .as_deref()
                    .and_then(|r| r.rge_master_missile_object.as_deref())
                    .map(|mm| &mm.rge_master_combat_object)
            });
        // the combat layer is the save's RGE combat part: a master without one has no attack,
        // armor or blast, whatever its copy's are
        if let Some(rc) = rge_combat {
            let combat_part = work.combat.get_or_insert_with(CombatPart::absent);
            combat_part.weapon_range = Distance::new(rc.weapon_range);
            combat_part.min_range = Distance::new(rc.minimum_weapon_range);
            combat_part.missile = MasterField::of_data(rc.missile_id);
            combat_part.weapon_offset =
                Offset::new(rc.weapon_offset_x, rc.weapon_offset_y, rc.weapon_offset_z);
            combat_part.missed_spread = MissSpread::new(rc.missed_missile_spread);
            combat_part.attack_sprite = SpriteId::of_data(rc.fight_sprite_id);
            combat_part.fire_frame = Frame::of_data(rc.fire_missile_at_frame);
            combat_part.interrupt_frame = Frame::of_data(rc.interrupt_frame);
            combat_part.hit_chance = HitChance::of_data(rc.base_hit_chance);
            combat_part.reload = Seconds::new(rc.speed_of_attack);
            combat_part.weapons = rc
                .weapon
                .iter()
                .map(|&w| ClassAmount::new((w & 0xffff) as i16, (w >> 16) as i16))
                .collect();
            combat_part.base_armor = DamageAmount::new(rc.base_armor);
            combat_part.armors = rc
                .armor
                .iter()
                .map(|&a| ClassAmount::new((a & 0xffff) as i16, (a >> 16) as i16))
                .collect();
            combat_part.resistance = DamageShare::new(rc.bonus_damage_resistance);
            combat_part.combat_ability = CombatAbility::from_bits_retain(rc.combat_ability);
            combat_part.reflection = DamageShare::new(rc.damage_reflection);
            combat_part.friendly_fire = DamageShare::new(rc.friendly_fire_damage);
            combat_part.area_effect = Distance::new(rc.area_effect_range);
            combat_part.blast_level = BlastLevel::from_bits_retain(rc.area_effect_level);
            combat_part.area_damage = DamageShare::new(rc.area_damage);
            combat_part.garrison_firepower = Firepower::new(rc.garrison_firepower);
        } else {
            work.combat = None;
        }
        work.storage = rge_master_static_object
            .attribute
            .iter()
            .map(|e| {
                (
                    AttributeId::of_data(e.attribute_kind_held),
                    Amount::new(e.attribute_amount_held),
                    e.attribute_flag,
                )
            })
            .collect();
        // the save holds the foundation terrain of its own building masters, its garrison's
        // kinds (`garrison_units`, read into `+0x9bc`: a civilisation's bonus, as the Gurjaras'
        // herdables in mills, changes them at the start) and rates, and its facet
        if let (Some(building_master), Some(saved)) = (
            work.building.as_mut(),
            master.tribe_master_building_object.as_deref(),
        ) {
            building_master.facet = saved.building_facet.cast_signed();
            building_master.garrison_kind = GarrisonKinds::from_bits_retain(saved.garrison_units);
            building_master.garrison_heal_rate = HitPoints::new(saved.garrison_heal_rate);
            building_master.garrison_repair_rate = HitPoints::new(saved.garrison_repair_rate);
            building_master.foundation_terrain = TerrainId::of_data(saved.on_build_make_tile);
            // `+0x960`, the third of its sprites (`tempId`)
            building_master.destruction_sprite = SpriteId::of_data(saved.temp_id_3.cast_signed());
            // `+0x950`, the first (`tempId`)
            building_master.construction_sprite = SpriteId::of_data(saved.temp_id.cast_signed());
        }
        // a missile master's layer: the save's three bytes and arc, the data file's projectile
        // type
        work.missile = master
            .rge_master
            .as_deref()
            .and_then(|rge_master| rge_master.rge_master_missile_object.as_deref())
            .map(|mm| MissilePart {
                projectile_kind: MasterId::of_data(rge_master_static_object.copy_id())
                    .and_then(|m| table.projectile_kind(civ, m))
                    .unwrap_or_default(),
                smart_mode: SmartMode::from_bits_retain(mm.targetting_kind),
                hit_mode: HitMode::of_data(mm.missile_hit_info),
                vanish_mode: VanishMode::from_bits_retain(mm.missile_die_info),
                arc: ArcRatio::new(mm.ballistics_ratio),
            });
        // what the masters' attribute setters change: the top and turn speeds (+0x740,
        // +0x758), the damage sprites (+0x638) and the train locations (+0x898)
        work.animated = animated_master(master).map(|rge_master_animated_object| AnimatedPart {
            max_speed: Speed::new(rge_master_animated_object.max_speed),
        });
        work.moving = moving_master(master).map(|mv| MovingPart {
            move_sprite: SpriteId::of_data(mv.move_sprite_id),
            run_sprite: SpriteId::of_data(mv.run_sprite_id),
            turn_speed: Seconds::new(mv.turn_speed),
            // the save's trail spacing is the float's four bytes (`.dat` tracking_unit_density)
            trail: Trail {
                unit: MasterField::of_data(mv.obj_trail_id.cast_signed()),
                mode: TrailMode::of_data(mv.obj_trail_options),
                density: TrailDensity::new(f32::from_bits(mv.obj_trail_spacing)),
            },
        });
        work.damage_graphics = rge_master_static_object
            .damage_sprite
            .iter()
            .map(|entry| DamageGraphic {
                sprite: SpriteId::of_data(entry.temp_id),
                percent: HpPercent::of_data(entry.sprite_info_damage_percent),
                apply_mode: DamageSpriteMode::of_data(entry.sprite_info_flag),
            })
            .collect();
        work.fly_mode = MasterId::of_data(rge_master_static_object.copy_id())
            .and_then(|m| table.fly_mode(civ, m))
            .unwrap_or_default();
        work.standing_sprite = SpriteId::of_data(rge_master_static_object.idle_sprite_id);
        work.standing_sprite_2 = SpriteId::of_data(rge_master_static_object.idle2_sprite_id);
        work.death_sprite = SpriteId::of_data(rge_master_static_object.death_sprite_id);
        work.garrison = GarrisonCapacity::of_data(rge_master_static_object.obj_max);
        work.death_spawns = [
            MasterId::of_data(rge_master_static_object.death_spawn_obj_id),
            MasterId::of_data(rge_master_static_object.death_spawn_obj_id2),
        ];
        work.terrain_restriction =
            RestrictionId::of_data(rge_master_static_object.terrain_restriction_mask_id);
        work.life = MasterId::of_data(rge_master_static_object.copy_id())
            .and_then(|master_id| table.get(civ, master_id));
        work.move_stats = moving_stats(master, rge_master_static_object, civ, table);
        Some(Ok(work))
    });
    Ok(Masters::new(masters.collect::<GenieResult<Vec<_>>>()?))
}

/// What a unit of a saved master moves with ([`genie_model::movement::MoveStats`]): the master's
/// top speed times its walking sprite's speed factor when above 0 (the run sprite if it has one,
/// else the move sprite: the `MoveTo`'s, slots 2000 and 0x7c8), the `.dat`'s turning, the radii and
/// the terrain restriction with its mask.
#[must_use]
pub fn moving_stats(
    master: &Master,
    rge_master_static_object: &RgeMasterStaticObject,
    civ: CivId,
    table: &MasterTable,
) -> Option<MoveStats> {
    let moving = moving_master(master)?;
    let sprite = SpriteId::of_data(moving.run_sprite_id)
        .or_else(|| SpriteId::of_data(moving.move_sprite_id));
    let base = moving.rge_master_animated_object.max_speed;
    let factor = sprite
        .and_then(|sprite| table.sprite_speed(sprite))
        .unwrap_or(SpeedMultiplier::ZERO);
    let turning = table.turning(civ, MasterId::of_data(rge_master_static_object.copy_id())?)?;
    let restriction = RestrictionId::of_data(rge_master_static_object.terrain_restriction_mask_id)?;
    let facets = SpriteId::of_data(moving.move_sprite_id)
        .and_then(|sprite_id| table.graphics.get(sprite_id))
        .map(|graphic| graphic.facets);
    Some(MoveStats {
        fly_mode: MasterId::of_data(rge_master_static_object.copy_id())
            .and_then(|master_id| table.fly_mode(civ, master_id))
            .unwrap_or_default(),
        speed: factor.applied_to(Speed::new(base)),
        base: Speed::new(base),
        turn_rate: turn_rate(Seconds::new(moving.turn_speed), facets),
        turn_radius: Distance::ZERO,
        turn_radius_speed: Speed::MAX,
        radius: Radii::new(
            rge_master_static_object.radius_x,
            rge_master_static_object.radius_y,
            rge_master_static_object.radius_z,
        ),
        restriction,
        restriction_mask: *table.terrain.masks().get(restriction.index())?,
        turn_time: Seconds::new(moving.turn_speed),
        move_algorithm: turning.move_algorithm,
        min_collision: turning.min_collision_size_multiplier,
        fast_turn: ObjectFlags::from_bits_retain(rge_master_static_object.object_flags)
            .contains(ObjectFlags::SHIP),
        facets: master_facets(master, rge_master_static_object, table),
        weapon_range: combat_master(master).map_or(Distance::ZERO, |rge_master_combat_object| {
            Distance::new(rge_master_combat_object.weapon_range)
        }),
    })
}

/// A combat master's cost slots from its `build_inventory` entries `(resource, amount, flag)`,
/// three shorts each, in their places; the flag set means paid, resource −1 an empty slot.
#[must_use]
pub fn build_cost(inventory: &[[u8; 6]]) -> [CostSlot; COST_SLOTS] {
    let mut slots = [CostSlot::default(); COST_SLOTS];
    for (slot, (kind, amount, paid)) in slots.iter_mut().zip(build_entries(inventory)) {
        *slot = CostSlot {
            attribute: AttributeId::of_data(kind),
            amount: CostAmount::of_data(amount),
            paid,
        };
    }
    slots
}

/// A combat master's `build_inventory` entries `(resource, amount, paid)`, the empty ones
/// (resource −1) kept in their places.
#[must_use]
pub fn build_entries(inventory: &[[u8; 6]]) -> Vec<(i16, i16, bool)> {
    inventory
        .iter()
        .map(|e| {
            let kind = i16::from_le_bytes([e[0], e[1]]);
            let amount = i16::from_le_bytes([e[2], e[3]]);
            let flag = i16::from_le_bytes([e[4], e[5]]);
            (kind, amount, flag != 0)
        })
        .collect()
}

/// How fast an object may move: its own or its player's master's speed, 0 for an object that
/// does not move (whether it moves, genie-core's `Placed::moves`, and whether a
/// combat unit links its tiles); a master the save does not hold, the largest.
#[must_use]
pub fn moving_speed(
    object: &genie_save::Object,
    rge_static_object: &RgeStaticObject,
    objects: &PlayerObjects,
) -> f32 {
    if object.moving_part().is_none() {
        return 0.0;
    }
    if rge_static_object.own_master().is_some() {
        return rge_static_object
            .own_animated_master()
            .map_or(f32::INFINITY, |own_rge_master_animated_object| {
                own_rge_master_animated_object.max_speed
            });
    }
    objects
        .master(rge_static_object.master_obj)
        .and_then(animated_master)
        .map_or(f32::INFINITY, |rge_master_animated_object| {
            rge_master_animated_object.max_speed
        })
}

/// What movement and sight read of an object's master (its own or its player's).
#[derive(Clone, Copy, Debug)]
pub struct Placement {
    /// The master's radii.
    pub radius: Radii,
    /// The master's class.
    pub class: ClassId,
    /// How the object obstructs others.
    pub obstruction_kind: ObstructionKind,
    /// The obstruction's class.
    pub obstruction_class: ObstructionClass,
    /// What may be built on the object.
    pub buildable_on: BuildableOn,
    /// The master's line of sight.
    pub los: Distance,
}

/// What movement and sight read of the object `rge_static_object`'s master: its own master's, else
/// its player's; none for a master the save does not hold.
#[must_use]
pub fn placement(
    rge_static_object: &RgeStaticObject,
    objects: &PlayerObjects,
) -> Option<Placement> {
    if let Some(own) = rge_static_object.own_master() {
        // an own master's second readTemp is its class (its first the id, its third the hit
        // points: as the shared masters' of the same id read)
        return Some(Placement {
            radius: Radii::new(own.radius_x, own.radius_y, own.radius_z),
            class: ClassId::of_master(own.read_temp_2),
            obstruction_kind: ObstructionKind::from(own.obstruction_kind),
            obstruction_class: ObstructionClass::from(own.obstruction_class),
            buildable_on: BuildableOn::of_data(own.can_be_built_on),
            los: Distance::new(own.los),
        });
    }
    let rge_master_static_object = objects
        .master(rge_static_object.master_obj)?
        .static_part()?;
    Some(Placement {
        radius: Radii::new(
            rge_master_static_object.radius_x,
            rge_master_static_object.radius_y,
            rge_master_static_object.radius_z,
        ),
        class: ClassId::of_master(rge_master_static_object.read_temp_5),
        obstruction_kind: ObstructionKind::from(rge_master_static_object.obstruction_kind),
        obstruction_class: ObstructionClass::from(rge_master_static_object.obstruction_class),
        buildable_on: BuildableOn::of_data(rge_master_static_object.can_be_built_on),
        los: Distance::new(rge_master_static_object.los),
    })
}
