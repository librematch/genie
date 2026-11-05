//! A saved master record's layers above its static part, and its lists: each layer's values as
//! the record holds them, by the stores of its class's load.

use crate::dat::MasterTable;
use crate::save::build_cost;
use crate::save::class_amounts;
use crate::save::own_master::Record;
use crate::save::tasks::core_task;
use genie_model::GenieResult;
use genie_model::combat::Charge;
use genie_model::combat::ChargeRate;
use genie_model::combat::Firepower;
use genie_model::combat::MissileCount;
use genie_model::combat::missile::MissSpread;
use genie_model::combat::missile::ProjectileKind;
use genie_model::combat::missile::{ArcRatio, HitMode, SmartMode, VanishMode};
use genie_model::map::Distance;
use genie_model::map::GroundOffset;
use genie_model::map::Offset;
use genie_model::map::Radii;
use genie_model::map::RestrictionId;
use genie_model::map::TerrainId;
use genie_model::movement::MoveAlgorithm;
use genie_model::movement::Speed;
use genie_model::movement::{MoveStats, turn_rate};
use genie_model::objects::Frame;
use genie_model::objects::HitPoints;
use genie_model::objects::SpeedMultiplier;
use genie_model::objects::SpriteFlags;
use genie_model::objects::SpriteId;
use genie_model::objects::WorkRate;
use genie_model::objects::facing::SpriteFacets;
use genie_model::objects::masters::ActionPart;
use genie_model::objects::masters::AdjacentMode;
use genie_model::objects::masters::AttackPriority;
use genie_model::objects::masters::BlastLevel;
use genie_model::objects::masters::BuildingMaster;
use genie_model::objects::masters::BuiltMode;
use genie_model::objects::masters::ChargeEvent;
use genie_model::objects::masters::ChargeKind;
use genie_model::objects::masters::ChargeTarget;
use genie_model::objects::masters::CombatAbility;
use genie_model::objects::masters::CombatPart;
use genie_model::objects::masters::CreatablePart;
use genie_model::objects::masters::DamageAmount;
use genie_model::objects::masters::DamageGraphic;
use genie_model::objects::masters::DamageShare;
use genie_model::objects::masters::DamageSpriteMode;
use genie_model::objects::masters::FlyMode;
use genie_model::objects::masters::FormationClass;
use genie_model::objects::masters::GarrisonKinds;
use genie_model::objects::masters::HeroFlags;
use genie_model::objects::masters::HitChance;
use genie_model::objects::masters::HpPercent;
use genie_model::objects::masters::InvulnerabilityLevel;
use genie_model::objects::masters::MasterField;
use genie_model::objects::masters::MasterId;
use genie_model::objects::masters::MissilePart;
use genie_model::objects::masters::MovingPart;
use genie_model::objects::masters::ObjectFlags;
use genie_model::objects::masters::RestrictionField;
use genie_model::objects::masters::RunPattern;
use genie_model::objects::masters::SpacingModifier;
use genie_model::objects::masters::SpecialAttack;
use genie_model::objects::masters::TaskGroup;
use genie_model::objects::masters::TaskIndex;
use genie_model::objects::masters::Trail;
use genie_model::objects::masters::TrailDensity;
use genie_model::objects::masters::TrailMode;
use genie_model::objects::masters::TrainLocation;
use genie_model::objects::masters::VolleySpread;
use genie_model::players::Amount;
use genie_model::players::attribute::AttributeId;
use genie_model::production::HotkeyId;
use genie_model::production::LocationButton;
use genie_model::technology::TechId;
use genie_model::world::Seconds;
use genie_model::world::WholeSeconds;
use genie_save::generated::world::{
    OwnRgeMasterActionObject, OwnRgeMasterCombatObject, OwnRgeMasterMissileObject,
    OwnRgeMasterMovingObject, OwnRgeMasterStaticObject, OwnTribeMasterBuildingObject,
    OwnTribeMasterCombatObject,
};

/// The moving layer (`FUN_1410e3b00`): its walking and running sprites (`+0x748`, `+0x750`),
/// turn speed (`+0x758`) and trail (`+0x75e`..`+0x764`).
pub(super) const fn own_moving(moving: &OwnRgeMasterMovingObject) -> MovingPart {
    MovingPart {
        move_sprite: SpriteId::of_data(moving.temp_move_sprite),
        run_sprite: SpriteId::of_data(moving.temp_run_sprite),
        turn_speed: Seconds::new(moving.turn_speed),
        trail: Trail {
            unit: MasterField::of_data(moving.obj_trail_id),
            mode: TrailMode::of_data(moving.obj_trail_options),
            density: TrailDensity::new(moving.obj_trail_spacing),
        },
    }
}

/// The action layer (`FUN_1410a14e0`): its default task (`+0x790`), search radius and work rate
/// (`+0x794`, `+0x798`), drop sites, task swap group (`+0x7b8`), run pattern (`+0x7b9`) and
/// tasks.
///
/// # Errors
///
/// A task that holds no type or attribute.
pub(super) fn own_action(action: &OwnRgeMasterActionObject) -> GenieResult<ActionPart> {
    Ok(ActionPart {
        task_by_group: TaskGroup::of_data(action.task_by_group),
        tasks: action
            .tasks
            .list
            .iter()
            .map(|entry| core_task(&entry.task))
            .collect::<GenieResult<_>>()?,
        default_task: TaskIndex::of_data(action.default_task),
        work_rate: WorkRate::new(action.work_rate),
        search_radius: Distance::new(action.search_radius),
        drop_sites: action
            .drop_site
            .iter()
            .filter_map(|&id| MasterId::of_data(id))
            .collect(),
        run_pattern: RunPattern::of_data(action.run_pattern),
    })
}

/// The combat layer (`FUN_1410c50b0`, `+0x7f8`..`+0x878`) over `part`, the `.dat` copy's: what
/// the shared masters' import and the `.dat` import read of it.
pub(super) fn own_combat(mut part: CombatPart, combat: &OwnRgeMasterCombatObject) -> CombatPart {
    part.weapon_range = Distance::new(combat.weapon_range);
    part.min_range = Distance::new(combat.minimum_weapon_range);
    part.terrain_bonus = RestrictionField::of_data(combat.defense_terrain_bonus);
    part.missile = MasterField::of_data(combat.missile_id);
    part.weapon_offset = Offset::new(
        combat.weapon_offset_x,
        combat.weapon_offset_y,
        combat.weapon_offset_z,
    );
    part.missed_spread = MissSpread::new(combat.missed_missile_spread);
    part.attack_sprite = SpriteId::of_data(combat.temp_fight_sprite);
    part.attack_sprite2 = SpriteId::of_data(combat.temp_fight_sprite_2);
    part.fire_frame = Frame::of_data(combat.fire_missile_at_frame);
    part.interrupt_frame = Frame::of_data(combat.interrupt_frame);
    part.hit_chance = HitChance::of_data(combat.base_hit_chance);
    part.reload = Seconds::new(combat.speed_of_attack);
    part.weapons = class_amounts(&combat.weapon);
    part.base_armor = DamageAmount::new(combat.base_armor);
    part.armors = class_amounts(&combat.armor);
    part.resistance = DamageShare::new(combat.bonus_damage_resistance);
    part.combat_ability = CombatAbility::from_bits_retain(combat.combat_ability);
    part.reflection = DamageShare::new(combat.damage_reflection);
    part.friendly_fire = DamageShare::new(combat.friendly_fire_damage);
    part.area_effect = Distance::new(combat.area_effect_range);
    part.blast_level = BlastLevel::from_bits_retain(combat.area_effect_level);
    part.area_damage = DamageShare::new(combat.area_damage);
    part.garrison_firepower = Firepower::new(combat.garrison_firepower);
    part
}

/// The creatable layer (`FUN_14126ae20`, `+0x880`..`+0x940`) over `part`, the `.dat` copy's:
/// what the shared masters' import and the `.dat` import read of it. Its sprites: the idle
/// attack sprite `+0x910` (the fifth `tempSpriteID`), the special sprite `+0x8e8` (the sixth).
pub(super) fn own_creatable(
    mut part: CreatablePart,
    creatable: &OwnTribeMasterCombatObject,
) -> CreatablePart {
    let id = |id: i32| i16::try_from(id).ok().and_then(MasterId::of_data);
    part.cost = build_cost(&creatable.build_inventory);
    part.build_time = creatable
        .train_location
        .first()
        .map_or(WholeSeconds::ZERO, |entry| {
            WholeSeconds::of_data(entry.train_time)
        });
    part.train_locations = creatable
        .train_location
        .iter()
        .map(|entry| TrainLocation {
            location: MasterId::of_data(entry.train_location_id),
            time: WholeSeconds::of_data(entry.train_time),
            button: LocationButton::of_data(entry.button_location_id),
            hotkey: HotkeyId::of_data(entry.hotkey_id),
        })
        .collect();
    part.trained_at = part
        .train_locations
        .iter()
        .filter_map(|train_location| train_location.location)
        .collect();
    part.spacing_modifier = SpacingModifier::new(creatable.spacing_modifier);
    part.formation_category = FormationClass::of_data(creatable.formation_category);
    part.hero_flag = HeroFlags::from_bits_retain(creatable.hero_flag);
    part.idle_attack_sprite = SpriteId::of_data(creatable.temp_sprite_id_5);
    part.special_sprite = i16::try_from(creatable.temp_sprite_id_6)
        .ok()
        .and_then(SpriteId::of_data);
    part.max_charge = Charge::new(creatable.max_charge);
    part.recharge_rate = ChargeRate::new(creatable.recharge_rate);
    part.charge_event = ChargeEvent::of_data(creatable.charge_event);
    part.charge_kind = ChargeKind::of_data(creatable.charge_kind);
    part.charge_target = ChargeTarget::of_data(creatable.charge_target);
    part.charge_projectile = id(creatable.charge_projectile_unit);
    part.attack_priority = AttackPriority::of_byte(creatable.attack_priority);
    part.invulnerability = InvulnerabilityLevel::new(creatable.invulnerability_level);
    part.volley_fire = Firepower::new(creatable.volley_fire_amount);
    part.max_volley = MissileCount::of_data(creatable.max_attacks_in_volley);
    part.volley_spread = VolleySpread {
        x: Distance::new(creatable.volley_xspread),
        y: Distance::new(creatable.volley_yspread),
        start: Distance::new(creatable.volley_start_spread_adjustment),
    };
    part.volley_missile = id(creatable.volley_missile_id);
    part.special_attack = SpecialAttack::of_data(creatable.special_attack_flag);
    part
}

/// The missile layer (`FUN_1410d6760`): its projectile type (`+0x880`), its smart, hit and
/// vanish modes and its arc.
pub(super) const fn own_missile(missile: &OwnRgeMasterMissileObject) -> MissilePart {
    MissilePart {
        projectile_kind: ProjectileKind::of_data(missile.missile_kind),
        smart_mode: SmartMode::from_bits_retain(missile.targetting_kind),
        hit_mode: HitMode::of_data(missile.missile_hit_info),
        vanish_mode: VanishMode::from_bits_retain(missile.missile_die_info),
        arc: ArcRatio::new(missile.ballistics_ratio),
    }
}

/// The building layer (`FUN_141240660`, `+0x950`..`+0x9c4`) over `part`, the `.dat` copy's: its
/// construction and destruction sprites (`+0x950`, `+0x960`: the first and third of its
/// sprites), its join mode, its stack unit, foundation terrain and technology, its annexes,
/// head and transform units and its garrison.
pub(super) fn own_building(
    mut part: BuildingMaster,
    building: &OwnTribeMasterBuildingObject,
) -> BuildingMaster {
    part.construction_sprite = SpriteId::of_data(building.sprite_num);
    part.destruction_sprite = SpriteId::of_data(building.destruction_sprite_num);
    part.adjacent_mode = AdjacentMode::of_data(building.building_connect_flag);
    part.facet = building.building_facet;
    part.disappears_when_built = BuiltMode::of_data(building.build_and_go_away);
    part.stack_unit = MasterId::of_data(building.on_build_make_obj);
    part.foundation_terrain = TerrainId::of_data(building.on_build_make_tile);
    part.tech = TechId::of_data(building.on_build_make_tech);
    for ((annex, offset), entry) in part
        .annexes
        .iter_mut()
        .zip(part.annex_offsets.iter_mut())
        .zip(&building.linked)
    {
        *annex = MasterId::of_data(entry.linked_id);
        *offset = GroundOffset::from([entry.linked_x, entry.linked_y]);
    }
    part.head_unit = MasterId::of_data(building.construction_id);
    part.transform_unit = MasterId::of_data(building.open_close_id);
    part.garrison_kind = GarrisonKinds::from_bits_retain(building.garrison_units);
    part.garrison_heal_rate = HitPoints::new(building.garrison_heal_rate);
    part.garrison_repair_rate = HitPoints::new(building.garrison_repair_rate);
    part
}

/// The resource storage, three entries (`+0x60a`, `+0x610`, `+0x628`).
pub(super) fn own_storage(
    static_part: &OwnRgeMasterStaticObject,
) -> Vec<(Option<AttributeId>, Amount, u8)> {
    static_part
        .attribute
        .iter()
        .map(|entry| {
            (
                AttributeId::of_data(entry.attribute_kind_held),
                Amount::new(entry.attribute_amount_held),
                entry.attribute_flag,
            )
        })
        .collect()
}

/// The damage sprites (`+0x638`).
pub(super) fn own_damage_graphics(static_part: &OwnRgeMasterStaticObject) -> Vec<DamageGraphic> {
    static_part
        .damage_sprite
        .iter()
        .map(|entry| DamageGraphic {
            sprite: SpriteId::of_data(entry.temp_damage_sprite),
            percent: HpPercent::of_data(entry.damage_sprites_damage_percent),
            apply_mode: DamageSpriteMode::of_data(entry.damage_sprites_flag),
        })
        .collect()
}

/// What a unit of the record's master moves with, as [`crate::save::moving_stats`] gives a
/// shared master's, but from the record alone: its turning (`+0x768`, `+0x780`), turn radius
/// (`+0x76c`, `+0x770`) and fly mode (`+0x608`) are its own; None for a record that does not
/// move or whose terrain restriction the `.dat` lacks.
pub(super) fn own_move_stats(record: &Record<'_>, table: &MasterTable) -> Option<MoveStats> {
    let moving = record.moving?;
    let static_part = record.static_part;
    let sprite = SpriteId::of_data(moving.temp_run_sprite)
        .or_else(|| SpriteId::of_data(moving.temp_move_sprite));
    let base = moving.own_rge_master_animated_object.max_speed;
    let factor = sprite
        .and_then(|sprite| table.sprite_speed(sprite))
        .unwrap_or(SpeedMultiplier::ZERO);
    let restriction = RestrictionId::of_data(static_part.terrain_restriction_mask_id)?;
    let facets = SpriteId::of_data(moving.temp_move_sprite)
        .and_then(|sprite_id| table.graphics.get(sprite_id))
        .map(|graphic| graphic.facets);
    Some(MoveStats {
        fly_mode: FlyMode::of_data(static_part.movement_kind),
        speed: factor.applied_to(Speed::new(base)),
        base: Speed::new(base),
        turn_rate: turn_rate(Seconds::new(moving.turn_speed), facets),
        turn_radius: Distance::new(moving.turn_radius),
        turn_radius_speed: Speed::new(moving.turn_radius_speed),
        radius: Radii::new(
            static_part.radius_x,
            static_part.radius_y,
            static_part.radius_z,
        ),
        restriction,
        restriction_mask: *table.terrain.masks().get(restriction.index())?,
        turn_time: Seconds::new(moving.turn_speed),
        move_algorithm: MoveAlgorithm::from_bits_retain(moving.move_algorithem),
        min_collision: moving.friendly_collision_circle_scale,
        fast_turn: ObjectFlags::from_bits_retain(static_part.object_flags)
            .contains(ObjectFlags::SHIP),
        facets: own_facets(record, table),
        weapon_range: record
            .combat
            .map_or(Distance::ZERO, |combat| Distance::new(combat.weapon_range)),
    })
}

/// The facets every sprite a live object of the record's master shows, as
/// [`crate::save::master_facets`] gives a shared master's: its standing, second standing,
/// walking, running and attack sprites and its tasks' four, those the data file has, all of one
/// count and bit 1 of the sequence flags; None when they differ or it has none.
fn own_facets(record: &Record<'_>, table: &MasterTable) -> Option<SpriteFacets> {
    let static_part = record.static_part;
    let mut sprites = vec![static_part.sprite_num, static_part.sprite_num2];
    if let Some(moving) = record.moving {
        sprites.extend([moving.temp_move_sprite, moving.temp_run_sprite]);
    }
    if let Some(combat) = record.combat {
        sprites.push(combat.temp_fight_sprite);
    }
    if let Some(action) = record.action {
        for entry in &action.tasks.list {
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
