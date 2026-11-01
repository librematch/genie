//! A player's saved masters as the engine reads them: the facets, the action part and
//! tasks, the work command's masters, the movement and the placement.

use crate::masters::MasterTable;
use crate::{animated_master, combat_master, moving_master};
use genie_core::GenieError;
use genie_core::combat::Charge;
use genie_core::combat::Firepower;
use genie_core::combat::MissileCount;
use genie_core::combat::missile::SmartMode;
use genie_core::combat::missile::VanishMode;
use genie_core::map::Distance;
use genie_core::map::Footprint;
use genie_core::map::Offset;
use genie_core::map::Radii;
use genie_core::map::TerrainId;
use genie_core::movement::MoveAlgorithm;
use genie_core::movement::Speed;
use genie_core::objects::Frame;
use genie_core::objects::HitPoints;
use genie_core::objects::MasterHitPoints;
use genie_core::objects::SpriteId;
use genie_core::objects::WorkRate;
use genie_core::objects::masters::CarryCapacity;
use genie_core::objects::masters::ChargeTarget;
use genie_core::objects::masters::ChargeType;
use genie_core::objects::masters::ClassAmount;
use genie_core::objects::masters::CombatAbility;
use genie_core::objects::masters::DamageShare;
use genie_core::objects::masters::FlyMode;
use genie_core::objects::masters::FormationClass;
use genie_core::objects::masters::GarrisonCapacity;
use genie_core::objects::masters::GarrisonKinds;
use genie_core::objects::masters::HeroFlags;
use genie_core::objects::masters::HitChance;
use genie_core::objects::masters::InvulnerabilityLevel;
use genie_core::objects::masters::MasterField;
use genie_core::objects::masters::ObjectFlags;
use genie_core::objects::masters::SoundEvents;
use genie_core::objects::masters::SpecialAttack;
use genie_core::objects::masters::TargetOwner;
use genie_core::objects::masters::TaskFlags;
use genie_core::objects::masters::TaskSprites;
use genie_core::objects::masters::TaskTerrain;
use genie_core::objects::masters::VolleySpread;
use genie_core::objects::masters::WorkValue;
use genie_core::objects::masters::{
    ActionType, ClassId, MasterId, ObstructionClass, ObstructionType,
};
use genie_core::objects::masters::{BlastDefense, BlastLevel};
use genie_core::objects::masters::{COST_SLOTS, CostSlot};
use genie_core::players::Amount;
use genie_core::players::attribute::AttributeId;
use genie_core::production::LocationButton;
use genie_core::world::Seconds;
use genie_core::world::WholeSeconds;
use genie_save::generated::world::{Master, PlayerObjects, RgeStaticObject};

/// The facets every sprite a live object of a saved master shows ([`genie_core::objects::facing::SpriteFacets`]):
/// its standing, second standing, walking, running and attack sprites and its tasks' four, those
/// the data file has, all of one count and bit 1 of the sequence flags; None when they differ or
/// it has none. The dying sprite shows once the object is dead, where nothing turns it.
pub(super) fn master_facets(
    m: &Master,
    s: &genie_save::generated::world::RgeMasterStaticObject,
    table: &MasterTable,
) -> Option<genie_core::objects::facing::SpriteFacets> {
    let mut sprites = vec![s.idle_sprite_id, s.idle2_sprite_id];
    if let Some(mv) = moving_master(m) {
        sprites.extend([mv.move_sprite_id, mv.run_sprite_id]);
    }
    if let Some(c) = combat_master(m) {
        sprites.push(c.fight_sprite_id);
    }
    if let Some(tasks) = action_part(m).and_then(|a| a.tasks.as_deref()) {
        for e in &tasks.list {
            let t = &e.task;
            sprites.extend([t.short_val, t.short_val_2, t.short_val_3, t.short_val_4]);
        }
    }
    let mut seen = sprites
        .into_iter()
        .filter_map(SpriteId::of_data)
        .filter_map(|id| table.graphics.get(&id))
        .map(|g| genie_core::objects::facing::SpriteFacets {
            count: g.facets,
            turns: g.flags.contains(genie_core::objects::SpriteFlags::TURNS),
        });
    let first = seen.next()?;
    seen.all(|f| f == first).then_some(first)
}

/// A master's action part, whatever class it is: what holds its work rate, search radius and
/// task list.
pub(super) fn action_part(
    m: &Master,
) -> Option<&genie_save::generated::world::RgeMasterActionObject> {
    None.or_else(|| {
        m.tribe_master_combat_object
            .as_deref()
            .map(|c| &c.rge_master_combat_object.rge_master_action_object)
    })
    .or_else(|| {
        m.tribe_master_building_object.as_deref().map(|b| {
            &b.tribe_master_combat_object
                .rge_master_combat_object
                .rge_master_action_object
        })
    })
    .or_else(|| {
        m.rge_master.as_deref().and_then(|r| {
            r.rge_master_action_object.as_deref().or_else(|| {
                r.rge_master_combat_object
                    .as_deref()
                    .map(|c| &c.rge_master_action_object)
            })
        })
    })
}

/// The task at `index` in a master's task list (its saved list, else the `.dat`'s): the task a
/// saved action's base `short_val` names (the base load `FUN_140a7a2f0` resolves it to `+0x48`
/// at `0x140a7b5e8`).
pub(super) fn task_at(
    objects: &PlayerObjects,
    civ: u8,
    masters: Option<&MasterTable>,
    master: i16,
    index: i16,
) -> Result<genie_core::objects::masters::Task, GenieError> {
    let i = usize::try_from(index)
        .map_err(|_| GenieError::unsupported("a saved action without its task"))?;
    let saved = objects
        .master(master)
        .and_then(action_part)
        .and_then(|a| a.tasks.as_deref());
    if let Some(tasks) = saved {
        let entry = tasks.list.get(i).ok_or(GenieError::unsupported(
            "a saved action's task beyond its master's list",
        ))?;
        return core_task(&entry.task);
    }
    masters
        .and_then(|t| t.work(civ, objects.dat_master(master)))
        .and_then(|w| w.tasks.get(i))
        .copied()
        .ok_or(GenieError::unsupported(
            "a saved action's task the data file does not have",
        ))
}

/// A saved task (`RGE_Task`) as genie-core's [`genie_core::objects::masters::Task`].
///
/// # Errors
///
/// A negative action type, or an attribute below −1.
pub(super) fn core_task(
    t: &genie_save::generated::world::Task,
) -> Result<genie_core::objects::masters::Task, GenieError> {
    let [
        attribute_type,
        attribute_type2,
        attribute_type3,
        attribute_type4,
        enablement,
    ] = crate::masters::attributes([
        t.attribute_type,
        t.attribute_type2,
        t.attribute_type3,
        t.attribute_type4,
        t.enablement_resource_id,
    ])?;
    Ok(genie_core::objects::masters::Task {
        id: genie_core::objects::masters::TaskId::of_task(t.id),
        action_type: ActionType::try_from(t.action_type)?,
        object_group: ClassId::of_field(t.object_group)?,
        object_id: MasterId::of_data(t.object_id),
        combat_level: t.combat_level,
        combat_level_flag: TaskFlags::from_bits_retain(t.combat_level_flag),
        terrain: TaskTerrain::of_data(t.terrain_id),
        owner_type: TargetOwner::of_data(t.owner_type),
        holding_attr: t.holding_attr,
        attribute_type,
        attribute_type2,
        attribute_type3,
        attribute_type4,
        wait_time: Seconds::new(t.search_wait_time),
        work_val1: WorkValue::new(t.work_val1),
        work_val2: WorkValue::new(t.work_val2),
        work_range: Distance::new(t.work_range),
        work_flag: t.work_flag,
        enablement,
        sprites: TaskSprites::of_data([t.short_val, t.short_val_2, t.short_val_3, t.short_val_4]),
        sound_events: SoundEvents::new(
            t.gather_sound_event_event_id.cast_signed(),
            t.deposit_sound_event_event_id.cast_signed(),
        ),
    })
}

/// A player's masters as the work command reads them: each master the save holds, with its saved
/// class, radii, cost and (where saved) tasks, and the `.dat`'s task swap group and type, which
/// the save does not carry (the game copies them from the `.dat` master).
pub(super) fn work_masters(
    objects: &PlayerObjects,
    civ: u8,
    table: &MasterTable,
) -> Result<genie_core::objects::masters::Masters, GenieError> {
    let task = core_task;
    let saved = objects.masters.iter().flatten().map(|m| &m.master);
    let masters = saved.filter_map(|m| {
        let s = m.static_part()?;
        // the `.dat` values of the master it copies (an upgraded one's), under its own id
        let mut work = table.work(civ, s.copy_id())?.clone();
        work.id = MasterId::of_data(s.id())?;
        work.class = ClassId::of_master(s.read_temp_5);
        work.radius = Footprint::new(s.radius_x, s.radius_y);
        work.tall = 0.0 < s.radius_z;
        work.height = s.radius_z;
        work.obstruction_type = ObstructionType::from(s.obstruction_type);
        work.buildable_on = s.can_be_built_on;
        work.obstruction_class = ObstructionClass::from(s.obstruction_class);
        work.object_flags = ObjectFlags::from_bits_retain(s.object_flags);
        work.hit_points = MasterHitPoints::of_data(s.read_temp_6);
        work.available = s.available != 0;
        work.disabled = s.disabled != 0;
        work.line_of_sight = Distance::new(s.los);
        work.capacity = CarryCapacity::of_data(s.attribute_max_amount);
        work.blast_defense = BlastDefense::new(s.area_effect_object_level);
        if let Some(a) = action_part(m) {
            work.work_rate = WorkRate::new(a.work_rate);
            work.search_radius = Distance::new(a.search_radius);
            if let Some(tasks) = a.tasks.as_deref() {
                work.tasks = match tasks.list.iter().map(|e| task(&e.task)).collect() {
                    Ok(tasks) => tasks,
                    Err(e) => return Some(Err(e)),
                };
            }
        }
        let combat = m.tribe_master_combat_object.as_deref().or_else(|| {
            m.tribe_master_building_object
                .as_deref()
                .map(|b| &b.tribe_master_combat_object)
        });
        if let Some(c) = combat {
            work.cost = build_cost(&c.build_inventory);
            work.build_time = c
                .train_location
                .first()
                .map_or(WholeSeconds::ZERO, |t| WholeSeconds::of_data(t.train_time));
            work.charge_type = ChargeType::of_data(c.charge_type);
            work.formation_category = FormationClass::of_data(c.temp_formation_category);
            work.hero_flag = HeroFlags::from_bits_retain(c.hero_flag);
            work.volley_fire = Firepower::new(c.volley_fire_amount);
            work.max_volley = MissileCount::of_data(c.max_attacks_in_volley);
            work.volley_spread = VolleySpread {
                x: c.volley_x_spread,
                y: c.volley_y_spread,
                start: c.volley_start_spread_adjustment,
            };
            let id = |id: i32| i16::try_from(id).ok().and_then(MasterId::of_data);
            work.volley_missile = id(c.volley_missile_id);
            work.charge_projectile = id(c.charge_projectile_unit);
            work.invulnerability = InvulnerabilityLevel::new(c.invulnerability_level);
            work.attack_priority =
                genie_core::objects::masters::AttackPriority::of_byte(c.attack_priority);
            work.special_attack = SpecialAttack::of_data(c.special_attack_flag);
            work.max_charge = Charge::new(c.max_charge);
            work.charge_target = ChargeTarget::of_data(c.charge_target);
        }
        // the combat part a missile master has too (`RGE_Master_Missile_Object`'s base)
        let rge_combat = combat.map(|c| &c.rge_master_combat_object).or_else(|| {
            m.rge_master
                .as_deref()
                .and_then(|r| r.rge_master_missile_object.as_deref())
                .map(|mm| &mm.rge_master_combat_object)
        });
        if let Some(rc) = rge_combat {
            work.weapon_range = Distance::new(rc.weapon_range);
            work.min_range = Distance::new(rc.minimum_weapon_range);
            work.missile = MasterField::of_data(rc.missile_id);
            work.weapon_offset =
                Offset::new(rc.weapon_offset_x, rc.weapon_offset_y, rc.weapon_offset_z);
            work.missed_spread = rc.missed_missile_spread;
            work.attack_sprite = SpriteId::of_data(rc.fight_sprite_id);
            work.fire_frame = Frame::of_data(rc.fire_missile_at_frame);
            work.interrupt_frame = Frame::of_data(rc.interrupt_frame);
            work.hit_chance = HitChance::of_data(rc.base_hit_chance);
            work.reload = Seconds::new(rc.speed_of_attack);
            work.weapons = rc
                .weapon
                .iter()
                .map(|&w| ClassAmount::new((w & 0xffff) as i16, (w >> 16) as i16))
                .collect();
            work.base_armor = rc.base_armor;
            work.armors = rc
                .armor
                .iter()
                .map(|&a| ClassAmount::new((a & 0xffff) as i16, (a >> 16) as i16))
                .collect();
            work.resistance = DamageShare::new(rc.bonus_damage_resistance);
            work.combat_ability = CombatAbility::from_bits_retain(rc.combat_ability);
            work.reflection = DamageShare::new(rc.damage_reflection);
            work.friendly_fire = DamageShare::new(rc.friendly_fire_damage);
            work.area_effect = Distance::new(rc.area_effect_range);
            work.blast_level = BlastLevel::from_bits_retain(rc.area_effect_level);
            work.area_damage = DamageShare::new(rc.area_damage);
            work.garrison_firepower = rc.garrison_firepower;
        }
        work.storage = s
            .attribute
            .iter()
            .map(|e| {
                (
                    AttributeId::of_data(e.attribute_type_held),
                    Amount::new(e.attribute_amount_held),
                    e.attribute_flag,
                )
            })
            .collect();
        // the save holds the foundation terrain of its own building masters, its garrison's
        // kinds (`garrison_units`, read into `+0x9bc`: a civilisation's bonus, as the Gurjaras'
        // herdables in mills, changes them at the start) and rates, and its facet
        if let Some(saved) = m.tribe_master_building_object.as_deref() {
            work.building_facet = saved.building_facet.cast_signed();
        }
        if let (Some(b), Some(saved)) = (
            work.building.as_mut(),
            m.tribe_master_building_object.as_deref(),
        ) {
            b.garrison_type = GarrisonKinds::from_bits_retain(saved.garrison_units);
            b.garrison_heal_rate = HitPoints::new(saved.garrison_heal_rate);
            b.garrison_repair_rate = HitPoints::new(saved.garrison_repair_rate);
            b.foundation_terrain = TerrainId::of_data(saved.on_build_make_tile);
            // `+0x960`, the third of its sprites (`tempId`)
            b.destruction_sprite = SpriteId::of_data(saved.temp_id_3.cast_signed());
            // `+0x950`, the first (`tempId`)
            b.construction_sprite = SpriteId::of_data(saved.temp_id.cast_signed());
        }
        // a missile master's flight: the save's three bytes and arc, the data file's projectile
        // type
        if let Some(mm) = m
            .rge_master
            .as_deref()
            .and_then(|r| r.rge_master_missile_object.as_deref())
        {
            work.missile_stats = Some(genie_core::combat::missile::MissileStats {
                projectile_type: table.projectile_type(civ, s.copy_id()).unwrap_or(0),
                smart_mode: SmartMode::from_bits_retain(mm.targetting_type),
                hit_mode: mm.missile_hit_info,
                vanish_mode: VanishMode::from_bits_retain(mm.missile_die_info),
                arc: mm.ballistics_ratio,
                speed: animated_master(m).map_or(Speed::ZERO, |a| Speed::new(a.max_speed)),
                radius: Radii::new(s.radius_x, s.radius_y, s.radius_z),
                fly_mode: FlyMode::of_data(table.fly_mode(civ, s.copy_id()).unwrap_or(0)),
            });
        }
        work.damage_sprites = !s.damage_sprite.is_empty();
        // what the masters' attribute setters change: the top and turn speeds (+0x740,
        // +0x758), the damage sprites (+0x638) and the train locations (+0x898)
        work.max_speed = animated_master(m).map_or(Speed::ZERO, |a| Speed::new(a.max_speed));
        work.turn_speed = moving_master(m).map_or(Seconds::ZERO, |mv| Seconds::new(mv.turn_speed));
        work.damage_graphics = s
            .damage_sprite
            .iter()
            .map(|d| genie_core::objects::masters::DamageGraphic {
                sprite: SpriteId::of_data(d.temp_id),
                percent: d.sprite_info_damage_percent,
                apply_mode: d.sprite_info_flag,
            })
            .collect();
        // a master without a combat part has no train locations, whatever its copy's are
        work.train_locations = combat.map_or_else(Vec::new, |c| {
            c.train_location
                .iter()
                .map(|t| genie_core::objects::masters::TrainLocation {
                    location: MasterId::of_data(t.train_location_id),
                    time: WholeSeconds::of_data(t.train_time),
                    button: LocationButton::of_data(t.button_location_id),
                    hotkey: t.hotkey_id,
                })
                .collect()
        });
        work.trained_at = work
            .train_locations
            .iter()
            .filter_map(|t| t.location)
            .collect();
        work.fly_mode = FlyMode::of_data(table.fly_mode(civ, s.copy_id()).unwrap_or(0));
        work.standing_sprite = SpriteId::of_data(s.idle_sprite_id);
        work.standing_sprite_2 = SpriteId::of_data(s.idle2_sprite_id);
        (work.move_sprite, work.run_sprite) = moving_master(m).map_or((None, None), |mv| {
            (
                SpriteId::of_data(mv.move_sprite_id),
                SpriteId::of_data(mv.run_sprite_id),
            )
        });
        work.death_sprite = SpriteId::of_data(s.death_sprite_id);
        work.garrison = GarrisonCapacity::of_data(s.obj_max);
        work.death_spawns = [
            MasterId::of_data(s.death_spawn_obj_id),
            MasterId::of_data(s.death_spawn_obj_id2),
        ];
        work.terrain_restriction =
            genie_core::map::RestrictionId::of_data(s.terrain_restriction_mask_id);
        work.life = table.get(civ, s.copy_id());
        work.moving = moving_stats(m, s, civ, table);
        Some(Ok(work))
    });
    Ok(genie_core::objects::masters::Masters::new(
        masters.collect::<Result<Vec<_>, GenieError>>()?,
    ))
}

/// What a unit of a saved master moves with ([`genie_core::movement::MoveStats`]): the master's
/// top speed times its walking sprite's speed factor when above 0 (the run sprite if it has one,
/// else the move sprite: the `MoveTo`'s, slots 2000 and 0x7c8), the `.dat`'s turning, the radii and
/// the terrain restriction with its mask.
pub(super) fn moving_stats(
    m: &Master,
    s: &genie_save::generated::world::RgeMasterStaticObject,
    civ: u8,
    table: &MasterTable,
) -> Option<genie_core::movement::MoveStats> {
    let moving = moving_master(m)?;
    let sprite = SpriteId::of_data(moving.run_sprite_id)
        .or_else(|| SpriteId::of_data(moving.move_sprite_id));
    let base = moving.rge_master_animated_object.max_speed;
    let factor = sprite
        .and_then(|sprite| table.sprite_speed(sprite))
        .unwrap_or(0.0);
    let turning = table.turning(civ, s.copy_id())?;
    let restriction = genie_core::map::RestrictionId::of_data(s.terrain_restriction_mask_id)?;
    let facets = SpriteId::of_data(moving.move_sprite_id)
        .and_then(|w| table.graphics.get(&w))
        .map(|g| g.facets);
    Some(genie_core::movement::MoveStats {
        fly_mode: FlyMode::of_data(table.fly_mode(civ, s.copy_id()).unwrap_or(0)),
        speed: if 0.0 < factor {
            Speed::new(base * factor)
        } else {
            Speed::new(base)
        },
        base: Speed::new(base),
        turn_rate: genie_core::movement::turn_rate(Seconds::new(moving.turn_speed), facets),
        turn_radius: Distance::ZERO,
        turn_radius_speed: Speed::MAX,
        radius: Radii::new(s.radius_x, s.radius_y, s.radius_z),
        restriction,
        restriction_mask: *table.terrain.masks().get(restriction.index())?,
        turn_time: Seconds::new(moving.turn_speed),
        move_algorithm: MoveAlgorithm::from_bits_retain(turning.move_algorithm),
        min_collision: turning.min_collision_size_multiplier,
        fast_turn: ObjectFlags::from_bits_retain(s.object_flags).contains(ObjectFlags::SHIP),
        facets: master_facets(m, s, table),
        weapon_range: combat_master(m).map_or(Distance::ZERO, |c| Distance::new(c.weapon_range)),
    })
}

/// A combat master's cost slots from its `build_inventory` entries `(resource, amount, flag)`,
/// three shorts each, in their places; the flag set means paid, resource −1 an empty slot.
pub(super) fn build_cost(inventory: &[[u8; 6]]) -> [CostSlot; COST_SLOTS] {
    let mut slots = [CostSlot::default(); COST_SLOTS];
    for (slot, (kind, amount, paid)) in slots.iter_mut().zip(build_entries(inventory)) {
        *slot = CostSlot {
            attribute: AttributeId::of_data(kind),
            amount,
            paid,
        };
    }
    slots
}

/// A combat master's `build_inventory` entries `(resource, amount, paid)`, the empty ones
/// (resource −1) kept in their places.
pub(super) fn build_entries(inventory: &[[u8; 6]]) -> Vec<(i16, i16, bool)> {
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

/// What movement reads of a saved object's master: radii, class and obstruction kind, from the
/// object's own master or its player's.
/// How fast an object may move, for the slice's bound on a frozen object's place: its own or its
/// player's master's speed, 0 for an object that does not move.
pub(super) fn moving_speed(
    o: &genie_save::Object,
    s: &RgeStaticObject,
    objects: &PlayerObjects,
) -> f32 {
    if o.moving_part().is_none() {
        return 0.0;
    }
    if s.own_master().is_some() {
        return s
            .own_animated_master()
            .map_or(f32::INFINITY, |a| a.max_speed);
    }
    objects
        .master(s.master_obj)
        .and_then(animated_master)
        .map_or(f32::INFINITY, |a| a.max_speed)
}

/// What movement and sight read of an object's master (its own or its player's).
pub(super) struct Placement {
    pub(super) radius: Radii,
    pub(super) class: ClassId,
    pub(super) obstruction_type: ObstructionType,
    pub(super) obstruction_class: ObstructionClass,
    pub(super) buildable_on: u8,
    pub(super) los: f32,
}

pub(super) fn placement(s: &RgeStaticObject, objects: &PlayerObjects) -> Option<Placement> {
    if let Some(own) = s.own_master() {
        // an own master's second readTemp is its class (its first the id, its third the hit
        // points: as the shared masters' of the same id read)
        return Some(Placement {
            radius: Radii::new(own.radius_x, own.radius_y, own.radius_z),
            class: ClassId::of_master(own.read_temp_2),
            obstruction_type: ObstructionType::from(own.obstruction_type),
            obstruction_class: ObstructionClass::from(own.obstruction_class),
            buildable_on: own.can_be_built_on,
            los: own.los,
        });
    }
    let m = objects.master(s.master_obj)?.static_part()?;
    Some(Placement {
        radius: Radii::new(m.radius_x, m.radius_y, m.radius_z),
        class: ClassId::of_master(m.read_temp_5),
        obstruction_type: ObstructionType::from(m.obstruction_type),
        obstruction_class: ObstructionClass::from(m.obstruction_class),
        buildable_on: m.can_be_built_on,
        los: m.los,
    })
}

#[cfg(test)]
mod tests;
