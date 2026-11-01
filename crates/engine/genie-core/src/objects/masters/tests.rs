use super::*;
use crate::combat::missile::SmartMode;
use crate::combat::missile::VanishMode;
use crate::map::Radii;
use crate::map::RestrictionId;
use crate::map::TerrainMask;
use crate::movement::MoveAlgorithm;
use crate::movement::TurnRate;
use crate::objects::HitPoints;
use crate::objects::ObjectState;
use crate::objects::SpriteFlags;
use crate::objects::masters::class;
use crate::players::CivId;
use crate::players::PlayerId;
use crate::players::attribute as player_attribute;
use crate::test_ids::carrying;
use crate::test_ids::costs;
use crate::test_ids::master_id;
use crate::test_ids::object_id;

/// A sprite field as the attribute tests read it: its id, −1 for none.
fn sprite_value(s: Option<SpriteId>) -> f32 {
    f32::from(s.map_or(-1, SpriteId::get))
}

fn task(action_type: ActionType, object_group: i16, object_id: i16, work_val1: f32) -> Task {
    Task {
        action_type,
        object_group: crate::test_ids::class_of(object_group),
        object_id: crate::test_ids::opt_master(object_id),
        work_val1,
        ..Task::default()
    }
}

fn master(id: MasterId, task_by_group: u8, tasks: Vec<Task>) -> Master {
    Master {
        id,
        kind: master_type::COMBAT,
        class: class::VILLAGER,
        task_by_group: TaskGroup::of_data(task_by_group),
        radius: Footprint::new(0.2, 0.2),
        tall: true,
        obstruction_type: ObstructionType::NONE,
        buildable_on: 0,
        tasks,
        cost: costs(&[]),
        obstruction_class: ObstructionClass::NONE,
        object_flags: ObjectFlags::NONE,
        combat_level: CombatLevel::NONE,
        work_rate: WorkRate::new(1.0),
        hit_points: MasterHitPoints::of_data(25),
        build_time: WholeSeconds::ZERO,
        ..Master::default()
    }
}

fn house() -> Master {
    Master {
        id: ids::HOUSE,
        kind: master_type::BUILDING,
        class: class::BUILDING,
        task_by_group: TaskGroup::NONE,
        radius: Footprint::new(1.0, 1.0),
        tall: true,
        obstruction_type: ObstructionType::NONE,
        buildable_on: 0,
        tasks: vec![],
        cost: costs(&[(player_attribute::WOOD, 25, true)]),
        obstruction_class: ObstructionClass::UNIT,
        object_flags: ObjectFlags::NONE,
        combat_level: CombatLevel::NONE,
        work_rate: WorkRate::ZERO,
        hit_points: MasterHitPoints::of_data(550),
        build_time: WholeSeconds::of_data(25),
        ..Master::default()
    }
}

/// The villagers of build 158041's `.dat`, shortened: the base masters have no Build task, the
/// builders a generic one; a fisherman (56) of the male group has none either.
fn villagers() -> Masters {
    let base = || {
        vec![
            task(action_type::COMBAT, -1, -1, 0.0),
            task(crate::garrison::ENTER, 3, -1, 0.0),
            task(crate::gathering::hunt::HUNT, -1, 126, 0.0),
        ]
    };
    let builder = || {
        vec![
            task(action_type::BUILD, -1, -1, 1.0),
            task(crate::garrison::ENTER, 3, -1, 0.0),
        ]
    };
    Masters::new([
        master(
            master_id(56),
            1,
            vec![task(crate::gathering::gather::GATHER, -1, 53, 1.0)],
        ),
        master(ids::VILLAGER_MALE, 1, base()),
        master(ids::BUILDER, 1, builder()),
        master(master_id(212), 2, builder()),
        master(ids::VILLAGER_FEMALE, 2, base()),
        house(),
    ])
}

/// Every player on its own, no drop sites.
struct Alone;

impl Diplomacy for Alone {
    fn allied(&self, from: PlayerId, to: PlayerId, _: bool) -> bool {
        from == to
    }

    fn drops_at(&self, _: MasterId, _: &Actor<'_>) -> bool {
        false
    }
}

/// A world whose sprite table holds the ids 1 to 99, without a handicap.
fn world(capped: bool) -> AttributeWorld<'static> {
    AttributeWorld {
        sprite: &|id| (1..100).contains(&id),
        handicap: None,
        capped,
    }
}

fn of_type(kind: MasterKind) -> Master {
    Master {
        kind,
        ..Master::default()
    }
}

/// A field of a master, read as an `f32`.
type Read = fn(&Master) -> f32;

/// A master of `kind` with three storage entries, a paid food cost, 10 hit points, two damage
/// sprites and two train locations (the second of each selected).
fn stocked(kind: MasterKind) -> Master {
    Master {
        kind,
        hit_points: MasterHitPoints::of_data(10),
        storage: vec![
            (Some(player_attribute::FOOD), Amount::new(1.0), 1),
            (Some(player_attribute::WOOD), Amount::new(1.0), 1),
            (Some(player_attribute::STONE), Amount::new(1.0), 1),
        ],
        cost: costs(&[(player_attribute::FOOD, 1, true)]),
        damage_graphics: vec![DamageGraphic::default(); 2],
        damage_graphic_entry: 1,
        train_locations: vec![TrainLocation::default(); 2],
        train_location_entry: 1,
        ..Master::default()
    }
}

/// Each row (attribute, its field, the field after a set of 3, then after an add of 2, then
/// after a multiply by 2) on a fresh [`stocked`] master of `kind`.
fn sets_adds_multiplies(kind: MasterKind, rows: &[(MasterAttribute, Read, [f32; 3])]) {
    let w = world(false);
    for &(a, read, [set, add, multiply]) in rows {
        let mut m = stocked(kind);
        m.set_attribute(a, 3.0, &w);
        assert_eq!(read(&m), set, "set {a}");
        m.add_attribute(a, 2.0, &w);
        assert_eq!(read(&m), add, "add {a}");
        m.multiply_attribute(a, 2.0, &w);
        assert_eq!(read(&m), multiply, "multiply {a}");
    }
}

/// `FUN_141126400`, `FUN_141126af0` and `FUN_1411271e0`: each static attribute's field takes
/// the set, the add and the multiply its arm has (a multiply rounds the shorts and bytes by
/// 0.5, the blast defence not); the others leave it.
#[test]
fn each_static_attribute_sets_adds_and_multiplies_its_field() {
    use attribute as a;
    let rows: &[(MasterAttribute, Read, [f32; 3])] = &[
        (
            a::HIT_POINTS,
            |m| HitPoints::of_master(m.hit_points).get(),
            [3.0, 5.0, 10.0],
        ),
        (
            a::LINE_OF_SIGHT,
            |m| m.line_of_sight.get(),
            [3.0, 5.0, 10.0],
        ),
        (
            a::GARRISON_CAPACITY,
            |m| f32::from(m.garrison.get()),
            [3.0, 5.0, 10.0],
        ),
        (a::SIZE_X, |m| m.radius.x(), [3.0, 5.0, 10.0]),
        (a::SIZE_Y, |m| m.radius.y(), [3.0, 5.0, 10.0]),
        (
            a::CARRY_CAPACITY,
            |m| f32::from(m.capacity.get()),
            [3.0, 5.0, 10.0],
        ),
        (a::STORAGE_1, |m| m.storage[0].1.get(), [3.0, 5.0, 10.0]),
        (a::STORAGE_2, |m| m.storage[1].1.get(), [3.0, 5.0, 10.0]),
        (a::STORAGE_3, |m| m.storage[2].1.get(), [3.0, 5.0, 10.0]),
        (
            a::FOG_VISIBILITY,
            |m| f32::from(m.fog_visibility.get()),
            [3.0, 3.0, 3.0],
        ),
        (
            a::OCCLUSION_MODE,
            |m| f32::from(m.occlusion.get()),
            [3.0, 5.0, 5.0],
        ),
        (a::SIZE_Z, |m| m.height, [3.0, 5.0, 10.0]),
        (
            a::CAN_BE_BUILT_ON,
            |m| f32::from(m.blast_defense.get()),
            [3.0, 5.0, 5.0],
        ),
        (
            a::BLAST_DEFENSE_LEVEL,
            |m| f32::from(m.blast_defense.get()),
            [3.0, 5.0, 10.0],
        ),
        (
            a::TERRAIN_RESTRICTION_ID,
            |m| f32::from(RestrictionId::field(m.terrain_restriction)),
            [3.0, 3.0, 3.0],
        ),
        (
            a::UNIT_TRAIT,
            |m| m.object_flags.bits() as f32,
            [3.0, 5.0, 5.0],
        ),
        (
            a::TRAIT_PIECE,
            |m| (m.object_flags.bits() >> 16) as f32,
            [3.0, 3.0, 3.0],
        ),
        (
            a::DEAD_UNIT_ID,
            |m| f32::from(MasterId::field(m.death_spawns[0])),
            [3.0, 3.0, 3.0],
        ),
        (
            a::BLOOD_UNIT,
            |m| f32::from(MasterId::field(m.death_spawns[1])),
            [3.0, 3.0, 3.0],
        ),
        (
            a::STANDING_GRAPHIC,
            |m| sprite_value(m.standing_sprite),
            [3.0, 5.0, 5.0],
        ),
        (
            a::STANDING_GRAPHIC_2,
            |m| sprite_value(m.standing_sprite_2),
            [3.0, 5.0, 5.0],
        ),
        (
            a::DYING_GRAPHIC,
            |m| sprite_value(m.death_sprite),
            [3.0, 5.0, 5.0],
        ),
        (
            a::UNDEAD_GRAPHIC,
            |m| sprite_value(m.undead_sprite),
            [3.0, 5.0, 5.0],
        ),
        (
            a::OBSTRUCTION_TYPE,
            |m| f32::from(m.obstruction_type.get()),
            [3.0, 3.0, 3.0],
        ),
        (
            a::BLOCKAGE_CLASS,
            |m| f32::from(m.obstruction_class.get()),
            [3.0, 3.0, 3.0],
        ),
        (
            a::SELECTION_EFFECT,
            |m| f32::from(m.selection_effect.get()),
            [3.0, 3.0, 3.0],
        ),
        (
            a::DAMAGE_GRAPHIC,
            |m| sprite_value(m.damage_graphics[1].sprite),
            [3.0, 5.0, 5.0],
        ),
        (
            a::AVAILABLE_UNIT_FLAG,
            |m| f32::from(u8::from(m.available)),
            [1.0; 3],
        ),
        (
            a::DISABLED_UNIT_FLAG,
            |m| f32::from(u8::from(m.disabled)),
            [1.0; 3],
        ),
        (
            a::INTERFACE_KIND,
            |m| f32::from(m.unit_level.get()),
            [3.0, 5.0, 5.0],
        ),
        (
            a::COMBAT_LEVEL,
            |m| f32::from(m.combat_level.get()),
            [3.0, 5.0, 5.0],
        ),
        (
            a::INTERACTION_MODE,
            |m| f32::from(m.select_level.get()),
            [3.0, 5.0, 5.0],
        ),
        (
            a::MINIMAP_MODE,
            |m| f32::from(m.map_draw_level.get()),
            [3.0, 5.0, 5.0],
        ),
        // the selected entry plus ten times the count: the set and the add select at most
        // the last, the multiply inserts at 2 and selects it
        (
            a::DAMAGE_GRAPHICS_ENTRY_MOD,
            |m| f32::from(m.damage_graphic_entry) + 10.0 * m.damage_graphics.len() as f32,
            [21.0, 21.0, 32.0],
        ),
        // the add's count is the selected entry (1) plus 2
        (
            a::DAMAGE_GRAPHICS_TOTAL_NUM,
            |m| m.damage_graphics.len() as f32,
            [3.0, 3.0, 6.0],
        ),
        (
            a::DAMAGE_GRAPHIC_PERCENT,
            |m| f32::from(m.damage_graphics[1].percent),
            [3.0, 5.0, 5.0],
        ),
        (
            a::DAMAGE_GRAPHIC_APPLY_MODE,
            |m| f32::from(m.damage_graphics[1].apply_mode),
            [3.0, 5.0, 5.0],
        ),
    ];
    sets_adds_multiplies(master_type::STATIC, rows);
    // an obstruction type the table does not hold is 0; the trait piece keeps the low half;
    // a flag of 0 is off
    let mut m = stocked(master_type::STATIC);
    m.object_flags = ObjectFlags::from_bits_retain(0x1234);
    m.set_attribute(a::TRAIT_PIECE, 2.0, &world(false));
    m.set_attribute(a::OBSTRUCTION_TYPE, 4.0, &world(false));
    m.set_attribute(a::AVAILABLE_UNIT_FLAG, 0.0, &world(false));
    assert_eq!(
        (m.object_flags, m.obstruction_type),
        (
            ObjectFlags::from_bits_retain(0x2_1234),
            ObstructionType::NONE
        )
    );
    assert!(!m.available);
}

/// `FUN_1410c1bf0` on the armor and attack amounts: a percentage of the class's entry plus
/// 0.5, truncated; a class the master lacks changes nothing; an attack class at 0 the handicap
/// raises scales the percentage by the factor (which a product of 0 keeps at 0).
#[test]
fn a_combat_multiply_takes_a_percentage_of_the_class_s_amount() {
    let mut m = of_type(master_type::RGE_COMBAT);
    m.armors = vec![ClassAmount::new(4, 3), ClassAmount::new(3, 5)];
    m.weapons = vec![ClassAmount::new(4, 7), ClassAmount::new(21, 0)];
    // class 3 at 150%: 5 x 1.5 + 0.5 = 8; class 4 at 50%: 7 x 0.5 + 0.5 = 4
    m.multiply_attribute(attribute::ARMOR, f32::from(0x396_i16), &world(false));
    m.multiply_attribute(attribute::ATTACK, f32::from(0x432_i16), &world(false));
    assert_eq!(
        (m.armors.clone(), m.weapons[0]),
        (
            vec![ClassAmount::new(4, 3), ClassAmount::new(3, 8)],
            ClassAmount::new(4, 4)
        )
    );
    m.multiply_attribute(attribute::ARMOR, f32::from(0x996_i16), &world(false));
    assert_eq!(
        m.armors,
        vec![ClassAmount::new(4, 3), ClassAmount::new(3, 8)]
    );
    let raised = AttributeWorld {
        handicap: Some(1.5),
        ..world(false)
    };
    m.multiply_attribute(attribute::ATTACK, f32::from(0x15c8_i16), &raised);
    assert_eq!(m.weapons[1], ClassAmount::new(21, 0));
}

/// `FUN_1410c12b0`, `FUN_1410c16e0` and `FUN_1410c1bf0`: each RGE combat attribute's field
/// takes the set, the add and the multiply (the frame delay, the blast level and the shown
/// values by the truncated amount); the rotation speed comes from the moving master's set.
#[test]
fn each_combat_attribute_sets_adds_and_multiplies_its_field() {
    use attribute as a;
    let rows: &[(MasterAttribute, Read, [f32; 3])] = &[
        (a::ROTATION_SPEED, |m| m.turn_speed.get(), [3.0, 5.0, 10.0]),
        (a::ATTACK_RELOAD_TIME, |m| m.reload.get(), [3.0, 5.0, 10.0]),
        (
            a::ACCURACY_PERCENT,
            |m| f32::from(m.hit_chance.get()),
            [3.0, 5.0, 10.0],
        ),
        (a::MAXIMUM_RANGE, |m| m.weapon_range.get(), [3.0, 5.0, 10.0]),
        (a::BASE_ARMOR, |m| f32::from(m.base_armor), [3.0, 5.0, 10.0]),
        (
            a::PROJECTILE_UNIT,
            |m| f32::from(m.missile.get()),
            [3.0, 5.0, 10.0],
        ),
        (
            a::TERRAIN_DEFENSE_BONUS,
            |m| f32::from(m.terrain_bonus.get()),
            [3.0, 5.0, 10.0],
        ),
        (a::MINIMUM_RANGE, |m| m.min_range.get(), [3.0, 5.0, 10.0]),
        (a::BLAST_WIDTH, |m| m.area_effect.get(), [3.0, 5.0, 10.0]),
        (
            a::BONUS_DAMAGE_RESISTANCE,
            |m| m.resistance,
            [3.0, 5.0, 10.0],
        ),
        (
            a::FRAME_DELAY,
            |m| f32::from(m.fire_frame),
            [3.0, 5.0, 10.0],
        ),
        (
            a::BLAST_ATTACK_LEVEL,
            |m| f32::from(m.blast_level.bits()),
            [3.0, 5.0, 10.0],
        ),
        (
            a::SHOWN_ATTACK,
            |m| f32::from(m.shown.attack),
            [3.0, 5.0, 10.0],
        ),
        (a::SHOWN_RANGE, |m| m.shown.range.get(), [3.0, 5.0, 10.0]),
        (
            a::SHOWN_MELEE_ARMOR,
            |m| f32::from(m.shown.melee_armor),
            [3.0, 5.0, 10.0],
        ),
        (
            a::COMBAT_ABILITY,
            |m| f32::from(m.combat_ability.bits()),
            [3.0, 5.0, 5.0],
        ),
        (a::ATTACK_DISPERSION, |m| m.missed_spread, [3.0, 5.0, 10.0]),
        (
            a::ATTACK_GRAPHIC,
            |m| sprite_value(m.attack_sprite),
            [3.0, 5.0, 5.0],
        ),
        (a::AREA_DAMAGE, |m| m.area_damage, [3.0, 5.0, 10.0]),
        (a::DAMAGE_REFLECTION, |m| m.reflection, [3.0, 5.0, 10.0]),
        (
            a::FRIENDLY_FIRE_DAMAGE,
            |m| m.friendly_fire,
            [3.0, 5.0, 10.0],
        ),
        (
            a::GARRISON_FIREPOWER,
            |m| m.garrison_firepower,
            [3.0, 5.0, 10.0],
        ),
        (a::DISPLACEMENT_X, |m| m.weapon_offset.x(), [3.0, 5.0, 10.0]),
        (a::DISPLACEMENT_Z, |m| m.weapon_offset.z(), [3.0, 5.0, 10.0]),
    ];
    sets_adds_multiplies(master_type::RGE_COMBAT, rows);
}

/// `FUN_1412661d0`, `FUN_141266a60` and `FUN_141267040`: each combat (TRIBE) attribute's
/// field takes the set, the add and the multiply its arm has; the selected train location
/// takes the train values.
#[test]
fn each_tribe_combat_attribute_sets_adds_and_multiplies_its_field() {
    use attribute as a;
    let rows: &[(MasterAttribute, Read, [f32; 3])] = &[
        (
            a::HERO_STATUS,
            |m| f32::from(m.hero_flag.bits()),
            [3.0, 5.0, 5.0],
        ),
        (
            a::TRAIN_LOCATION,
            |m| f32::from(MasterId::field(m.train_locations[1].location)),
            [3.0; 3],
        ),
        (
            a::TRAIN_BUTTON,
            |m| f32::from(m.train_locations[1].button),
            [3.0; 3],
        ),
        (
            a::SHOWN_PIERCE_ARMOR,
            |m| f32::from(m.shown.pierce_armor),
            [3.0, 5.0, 10.0],
        ),
        (
            a::HOTKEY_ID,
            |m| m.train_locations[1].hotkey as f32,
            [3.0; 3],
        ),
        (a::MAXIMUM_CHARGE, |m| m.max_charge.get(), [3.0, 5.0, 10.0]),
        (
            a::RECHARGE_RATE,
            |m| m.recharge_rate.get(),
            [3.0, 5.0, 10.0],
        ),
        (
            a::CHARGE_EVENT,
            |m| f32::from(m.charge_event.get()),
            [3.0, 5.0, 5.0],
        ),
        (a::CHARGE_TYPE, |m| f32::from(m.charge_type.get()), [3.0; 3]),
        (
            a::SECONDARY_PROJECTILE_UNIT,
            |m| f32::from(MasterId::field(m.volley_missile)),
            [3.0; 3],
        ),
        (
            a::SPECIAL_GRAPHIC,
            |m| sprite_value(m.special_sprite),
            [3.0, 5.0, 5.0],
        ),
        (
            a::SPECIAL_ABILITY,
            |m| f32::from(m.special_attack.get()),
            [3.0, 5.0, 5.0],
        ),
        (
            a::IDLE_ATTACK_GRAPHIC,
            |m| sprite_value(m.idle_attack_sprite),
            [3.0, 5.0, 5.0],
        ),
        (
            a::HERO_GLOW_GRAPHIC,
            |m| sprite_value(m.hero_glow_sprite),
            [3.0, 5.0, 5.0],
        ),
        (
            a::GARRISON_GRAPHIC,
            |m| sprite_value(m.garrison_sprite),
            [3.0, 5.0, 5.0],
        ),
        (
            a::FOOD_COSTS,
            |m| f32::from(m.cost[0].amount),
            [3.0, 5.0, 10.0],
        ),
        (
            a::TRAIN_TIME,
            |m| f32::from(m.train_locations[1].time.get()),
            [3.0, 5.0, 10.0],
        ),
        (a::TOTAL_MISSILES, |m| m.volley_fire, [3.0, 5.0, 10.0]),
        (
            a::MAXIMUM_TOTAL_MISSILES,
            |m| f32::from(m.max_volley),
            [3.0, 5.0, 10.0],
        ),
        (a::REGENERATION_RATE, |m| m.regeneration, [3.0, 5.0, 10.0]),
        (
            a::MIN_CONVERSION_TIME_MODIFIER,
            |m| m.conversion.min_time,
            [3.0, 5.0, 10.0],
        ),
        (
            a::CONVERSION_CHANCE_MODIFIER,
            |m| m.conversion.chance,
            [3.0, 5.0, 10.0],
        ),
        (
            a::FORMATION_CATEGORY,
            |m| f32::from(m.formation_category.get()),
            [3.0; 3],
        ),
        // 10 hit points times the amount, set then added
        (
            a::REGENERATION_HP_PERCENT,
            |m| m.regeneration,
            [30.0, 50.0, 50.0],
        ),
        (
            a::CHARGE_PROJECTILE_UNIT,
            |m| f32::from(MasterId::field(m.charge_projectile)),
            [3.0; 3],
        ),
        (
            a::ATTACK_PRIORITY,
            |m| f32::from(m.attack_priority.get()),
            [3.0; 3],
        ),
        (
            a::INVULNERABILITY_LEVEL,
            |m| m.invulnerability,
            [3.0, 5.0, 10.0],
        ),
        (
            a::ATTACK_GRAPHIC_2,
            |m| sprite_value(m.attack_sprite2),
            [3.0, 5.0, 5.0],
        ),
        (
            a::SPAWNING_AREA_WIDTH,
            |m| m.volley_spread.x,
            [3.0, 5.0, 10.0],
        ),
        (
            a::SPAWNING_AREA_RANDOMNESS,
            |m| m.volley_spread.start,
            [3.0, 5.0, 10.0],
        ),
        // the selected entry plus ten times the count, as the damage sprites'
        (
            a::TRAIN_LOCATIONS_ENTRY_MOD,
            |m| f32::from(m.train_location_entry) + 10.0 * m.train_locations.len() as f32,
            [21.0, 21.0, 32.0],
        ),
        (
            a::TRAIN_LOCATIONS_TOTAL_NUM,
            |m| m.train_locations.len() as f32,
            [3.0, 3.0, 6.0],
        ),
    ];
    sets_adds_multiplies(master_type::COMBAT, rows);
}

/// `FUN_141126400`, `FUN_141126af0` and `FUN_1411271e0` on the hit points: a set truncates, an
/// add adds the truncated amount (wrapping), a multiply rounds by 0.5; capped (the player's
/// `+0x648`) both stop at 0x7fff, a NaN too (`COMISS` then `JBE`).
#[test]
fn the_hit_points_add_and_multiply_up_to_their_cap_when_capped() {
    let mut m = of_type(master_type::STATIC);
    m.set_attribute(attribute::HIT_POINTS, 25.9, &world(false));
    assert_eq!(m.hit_points, MasterHitPoints::of_data(25));
    m.add_attribute(attribute::HIT_POINTS, 15.7, &world(false));
    assert_eq!(m.hit_points, MasterHitPoints::of_data(40));
    m.multiply_attribute(attribute::HIT_POINTS, 1.2, &world(false));
    // 40 × 1.2 + 0.5 = 48.5
    assert_eq!(m.hit_points, MasterHitPoints::of_data(48));
    m.hit_points = MasterHitPoints::of_data(32760);
    m.add_attribute(attribute::HIT_POINTS, 10.0, &world(false));
    assert_eq!(m.hit_points, MasterHitPoints::of_data(-32766));
    m.hit_points = MasterHitPoints::of_data(32760);
    m.add_attribute(attribute::HIT_POINTS, 10.0, &world(true));
    assert_eq!(m.hit_points, MasterHitPoints::of_data(0x7fff));
    m.hit_points = MasterHitPoints::of_data(100);
    m.add_attribute(attribute::HIT_POINTS, f32::NAN, &world(true));
    assert_eq!(m.hit_points, MasterHitPoints::of_data(0x7fff));
    m.hit_points = MasterHitPoints::of_data(20000);
    m.multiply_attribute(attribute::HIT_POINTS, 2.0, &world(true));
    assert_eq!(m.hit_points, MasterHitPoints::of_data(0x7fff));
    m.capacity = CarryCapacity::of_data(10);
    m.multiply_attribute(attribute::CARRY_CAPACITY, 1.25, &world(true));
    // 12.5 + 0.5 = 13
    assert_eq!(m.capacity, CarryCapacity::of_data(13));
}

/// The storage amounts take a short (`(float)(int)(short)(int)v`), the object flags a 64-bit
/// truncation, the trait piece its high half; an obstruction type the table refuses is 0.
#[test]
fn the_static_setters_truncate_as_the_game_does() {
    let mut m = of_type(master_type::TREE);
    m.storage = vec![
        (Some(player_attribute::MEAT), Amount::new(100.0), 1),
        (None, Amount::ZERO, 0),
        (None, Amount::ZERO, 0),
    ];
    m.set_attribute(attribute::STORAGE_1, 70000.5, &world(false));
    assert_eq!(m.storage[0].1, Amount::new(4464.0));
    m.add_attribute(attribute::STORAGE_1, 5.9, &world(false));
    assert_eq!(m.storage[0].1, Amount::new(4469.0));
    m.multiply_attribute(attribute::STORAGE_1, 0.5, &world(false));
    // 2234.5 + 0.5 = 2235
    assert_eq!(m.storage[0].1, Amount::new(2235.0));
    m.set_attribute(attribute::UNIT_TRAIT, 5_000_000_000.0, &world(false));
    assert_eq!(
        m.object_flags,
        ObjectFlags::from_bits_retain(5_000_000_000_i64 as u32)
    );
    m.object_flags = ObjectFlags::from_bits_retain(0x1234_5678);
    m.set_attribute(attribute::TRAIT_PIECE, 3.0, &world(false));
    assert_eq!(m.object_flags, ObjectFlags::from_bits_retain(0x0003_5678));
    m.set_attribute(attribute::OBSTRUCTION_TYPE, 5.0, &world(false));
    assert_eq!(m.obstruction_type, ObstructionType::CIRCLE);
    m.set_attribute(attribute::OBSTRUCTION_TYPE, 4.0, &world(false));
    assert_eq!(m.obstruction_type, ObstructionType::NONE);
    // "Can Be Built On" writes the blast defense level; its multiply does not round
    m.set_attribute(attribute::CAN_BE_BUILT_ON, 3.0, &world(false));
    m.multiply_attribute(attribute::BLAST_DEFENSE_LEVEL, 1.9, &world(false));
    assert_eq!(m.blast_defense, BlastDefense::new(5));
    // the available flag holds the byte's test against 0
    m.set_attribute(attribute::AVAILABLE_UNIT_FLAG, 256.0, &world(false));
    assert!(!m.available);
    m.add_attribute(attribute::AVAILABLE_UNIT_FLAG, 1.0, &world(false));
    assert!(m.available);
}

/// `FUN_14152e360`: a sprite attribute takes an id only where the world's table holds a
/// sprite; an add starts from −1 for none.
#[test]
fn a_sprite_attribute_takes_only_a_sprite_the_world_has() {
    let mut m = of_type(master_type::MOVING);
    m.standing_sprite = Some(SpriteId::named(5));
    m.move_sprite = None;
    m.set_attribute(attribute::STANDING_GRAPHIC, 150.0, &world(false));
    assert_eq!(m.standing_sprite, Some(SpriteId::named(5)));
    m.add_attribute(attribute::STANDING_GRAPHIC, 3.0, &world(false));
    assert_eq!(m.standing_sprite, Some(SpriteId::named(8)));
    m.add_attribute(attribute::WALKING_GRAPHIC, 11.0, &world(false));
    assert_eq!(m.move_sprite, Some(SpriteId::named(10)));
    m.set_attribute(attribute::STANDING_GRAPHIC_2, 7.0, &world(false));
    assert_eq!(m.standing_sprite_2, Some(SpriteId::named(7)));
}

/// `FUN_1410a8b60`, `FUN_1410a8c00`: a capped add or multiply of the speed stops at 4.5, a
/// missile master's at 20; a NaN speed passes (`COMISS`, `JBE`).
#[test]
fn a_capped_speed_stops_at_its_type_s_cap() {
    let mut m = of_type(master_type::COMBAT);
    m.max_speed = Speed::new(4.0);
    m.add_attribute(attribute::MOVEMENT_SPEED, 1.0, &world(false));
    assert_eq!(m.max_speed, Speed::new(5.0));
    m.multiply_attribute(attribute::MOVEMENT_SPEED, 1.1, &world(true));
    assert_eq!(m.max_speed, Speed::new(4.5));
    let mut missile = of_type(master_type::MISSILE);
    missile.max_speed = Speed::new(7.0);
    missile.multiply_attribute(attribute::MOVEMENT_SPEED, 4.0, &world(true));
    assert_eq!(missile.max_speed, Speed::new(20.0));
    missile.add_attribute(attribute::MOVEMENT_SPEED, f32::NAN, &world(true));
    assert!(missile.max_speed.get().is_nan());
}

/// `FUN_1410c12b0` and its add and multiply: the amount's high byte names the class, its low
/// byte the amount (negated for a negative amount); the first entry of the class changes, a
/// multiply by `amount / 100`.
#[test]
fn armor_and_attack_change_the_class_in_the_amount_s_high_byte() {
    let mut m = of_type(master_type::COMBAT);
    m.armors = vec![
        ClassAmount::new(4, 1),
        ClassAmount::new(3, 2),
        ClassAmount::new(3, 9),
    ];
    m.weapons = vec![ClassAmount::new(4, 6), ClassAmount::new(3, 0)];
    // class 3, 1 more (0x301)
    m.add_attribute(attribute::ARMOR, 769.0, &world(false));
    assert_eq!(
        m.armors,
        vec![
            ClassAmount::new(4, 1),
            ClassAmount::new(3, 3),
            ClassAmount::new(3, 9)
        ]
    );
    // −0x402: class 4, 2 less
    m.add_attribute(attribute::ARMOR, -1026.0, &world(false));
    assert_eq!(m.armors[0], ClassAmount::new(4, -1));
    m.set_attribute(attribute::ARMOR, 1029.0, &world(false));
    assert_eq!(m.armors[0], ClassAmount::new(4, 5));
    // 150 %: 6 × 1.5 + 0.5 = 9.5
    m.multiply_attribute(attribute::ATTACK, f32::from(0x496_i16), &world(false));
    assert_eq!(m.weapons[0], ClassAmount::new(4, 9));
    // a NaN takes the negative branch: class 0, amount 0, which no entry has
    m.add_attribute(attribute::ATTACK, f32::NAN, &world(false));
    assert_eq!(
        m.weapons,
        vec![ClassAmount::new(4, 9), ClassAmount::new(3, 0)]
    );
    // a handicap of 1.5 raises a class at 0 (not 3, 4, 31 or 33)
    let raised = AttributeWorld {
        handicap: Some(1.5),
        ..world(false)
    };
    m.weapons = vec![ClassAmount::new(4, 0), ClassAmount::new(21, 0)];
    m.set_attribute(attribute::ATTACK, f32::from(0x406_i16), &raised);
    m.set_attribute(attribute::ATTACK, f32::from(0x1506_i16), &raised);
    assert_eq!(
        m.weapons,
        vec![ClassAmount::new(4, 6), ClassAmount::new(21, 9)]
    );
    // once above 0 the add adds
    m.add_attribute(attribute::ATTACK, f32::from(0x1502_i16), &raised);
    assert_eq!(m.weapons[1], ClassAmount::new(21, 11));
}

/// The RGE combat master's multiply rounds most shorts by 0.5 but multiplies the frame delay
/// and the shown values by the truncated amount (`IMUL`).
#[test]
fn some_combat_shorts_multiply_by_the_truncated_amount() {
    let mut m = of_type(master_type::RGE_COMBAT);
    m.base_armor = 3;
    m.fire_frame = 5;
    m.shown.attack = 7;
    m.multiply_attribute(attribute::BASE_ARMOR, 1.5, &world(false));
    assert_eq!(m.base_armor, 5);
    m.multiply_attribute(attribute::FRAME_DELAY, 2.9, &world(false));
    assert_eq!(m.fire_frame, 10);
    m.multiply_attribute(attribute::SHOWN_ATTACK, 3.5, &world(false));
    assert_eq!(m.shown.attack, 21);
    m.add_attribute(attribute::ROTATION_SPEED, 0.25, &world(false));
    assert_eq!(m.turn_speed, Seconds::new(0.25));
    m.set_attribute(attribute::DISPLACEMENT_Y, 0.75, &world(false));
    assert_eq!(m.weapon_offset, Offset::new(0.0, 0.75, 0.0));
}

/// `FUN_1410d5b50`: a missile master's smart mode takes the multiply's amount; its arc
/// multiplies; then the RGE combat master's setter runs.
#[test]
fn a_missile_s_smart_mode_multiply_sets_it() {
    let mut m = of_type(master_type::MISSILE);
    m.missile_stats = Some(crate::combat::missile::MissileStats {
        projectile_type: 0,
        smart_mode: SmartMode::LEADS_TARGET,
        hit_mode: 0,
        vanish_mode: VanishMode::NONE,
        arc: 0.5,
        speed: Speed::new(7.0),
        radius: Radii::ZERO,
        fly_mode: FlyMode::GROUND,
    });
    m.reload = Seconds::new(2.0);
    m.multiply_attribute(attribute::PROJECTILE_SMART_MODE, 3.0, &world(false));
    m.multiply_attribute(attribute::PROJECTILE_ARC, 0.5, &world(false));
    m.multiply_attribute(attribute::ATTACK_RELOAD_TIME, 0.5, &world(false));
    let s = m.missile_stats.unwrap();
    assert_eq!(
        (s.smart_mode, s.arc, m.reload),
        (SmartMode::from_bits_retain(3), 0.25, Seconds::new(1.0))
    );
}

/// `FUN_1412661d0` and its add and multiply on the costs: 100 changes the paid entries, 103
/// to 106 those of food, wood, gold (3) and stone (2); population (110) sets the unpaid ones
/// to the negated amount and the population storage to the amount.
#[test]
fn the_cost_attributes_change_their_resource_s_entries() {
    let mut m = of_type(master_type::BUILDING);
    m.cost = costs(&[
        (player_attribute::WOOD, 25, true),
        (player_attribute::HEADROOM, 1, false),
    ]);
    m.storage = vec![
        (Some(player_attribute::HEADROOM), Amount::new(5.0), 4),
        (Some(player_attribute::POPULATION), Amount::new(-1.0), 4),
        (None, Amount::ZERO, 0),
    ];
    m.add_attribute(attribute::WOOD_COSTS, -5.0, &world(false));
    assert_eq!(
        m.cost,
        costs(&[
            (player_attribute::WOOD, 20, true),
            (player_attribute::HEADROOM, 1, false)
        ])
    );
    m.multiply_attribute(attribute::RESOURCE_COSTS, 0.85, &world(false));
    // 20 × 0.85 + 0.5 = 17.5
    assert_eq!(
        m.cost,
        costs(&[
            (player_attribute::WOOD, 17, true),
            (player_attribute::HEADROOM, 1, false)
        ])
    );
    m.set_attribute(attribute::GOLD_COSTS, 50.0, &world(false));
    assert_eq!(m.cost[0].entry(), Some((player_attribute::WOOD, 17, true)));
    m.set_attribute(attribute::POPULATION, 2.0, &world(false));
    assert_eq!(
        m.cost,
        costs(&[
            (player_attribute::WOOD, 17, true),
            (player_attribute::HEADROOM, -2, false)
        ])
    );
    assert_eq!(
        m.storage[..2],
        [
            (Some(player_attribute::HEADROOM), Amount::new(2.0), 4),
            (Some(player_attribute::POPULATION), Amount::new(-2.0), 4)
        ]
    );
}

/// The train locations: the selected entry (`+0x940`) takes the train time; a resize copies
/// the last entry, its add counts from the selected entry (the game's); a multiply of the
/// selected entry inserts a copy of the last or removes one at the amount's complement.
#[test]
fn the_train_location_attributes_change_the_selected_entry() {
    let barracks = |time| TrainLocation {
        location: Some(master_id(12)),
        time,
        button: 1,
        hotkey: 16_000,
    };
    let mut m = of_type(master_type::COMBAT);
    m.train_locations = vec![
        barracks(WholeSeconds::of_data(21)),
        barracks(WholeSeconds::of_data(30)),
    ];
    m.train_location_entry = 1;
    m.multiply_attribute(attribute::TRAIN_TIME, 0.9, &world(false));
    // 30 × 0.9 + 0.5 = 27.5
    assert_eq!(m.train_locations[1].time, WholeSeconds::of_data(27));
    m.set_attribute(attribute::TRAIN_LOCATIONS_ENTRY_MOD, 9.0, &world(false));
    assert_eq!(m.train_location_entry, 1);
    // the add's count is the selected entry plus the amount: 1 + 2
    m.add_attribute(attribute::TRAIN_LOCATIONS_TOTAL_NUM, 2.0, &world(false));
    assert_eq!(m.train_locations.len(), 3);
    assert_eq!(m.train_locations[2], m.train_locations[1]);
    m.set_attribute(attribute::TRAIN_LOCATIONS_TOTAL_NUM, -4.0, &world(false));
    assert_eq!(m.train_locations, []);
    assert_eq!(m.train_location_entry, -1);
    m.train_locations = vec![
        barracks(WholeSeconds::of_data(21)),
        barracks(WholeSeconds::of_data(30)),
    ];
    m.train_location_entry = 0;
    // ~(−1) = 0: the first goes, the entry stays at 0
    m.multiply_attribute(attribute::TRAIN_LOCATIONS_ENTRY_MOD, -1.0, &world(false));
    assert_eq!(
        (m.train_locations.len(), m.train_locations[0].time),
        (1, WholeSeconds::of_data(30))
    );
    m.multiply_attribute(attribute::TRAIN_LOCATIONS_ENTRY_MOD, 5.0, &world(false));
    assert_eq!((m.train_locations.len(), m.train_location_entry), (2, 1));
    m.follow(
        attribute::TRAIN_TIME,
        &std::collections::BTreeMap::new(),
        None,
    );
    assert_eq!(
        (m.trained_at.clone(), m.build_time),
        (
            vec![master_id(12), master_id(12)],
            WholeSeconds::of_data(30)
        )
    );
}

/// The damage sprites: a resize zeroes new entries (a NaN count is the most, `MAXSS`); with
/// none left the selected entry stays (an unsigned byte compare); a multiply of the selected
/// entry removing the last leaves 0xff.
#[test]
fn the_damage_sprite_attributes_resize_and_select() {
    let mut m = of_type(master_type::ANIMATED);
    m.damage_graphic_entry = 2;
    m.set_attribute(attribute::DAMAGE_GRAPHICS_TOTAL_NUM, 2.0, &world(false));
    assert_eq!(m.damage_graphics, vec![DamageGraphic::default(); 2]);
    assert_eq!(m.damage_graphic_entry, 1);
    m.set_attribute(attribute::DAMAGE_GRAPHIC, 40.0, &world(false));
    m.set_attribute(attribute::DAMAGE_GRAPHIC_PERCENT, 75.0, &world(false));
    assert_eq!(m.damage_graphics[1].sprite, Some(SpriteId::named(40)));
    assert_eq!(m.damage_graphics[1].percent, 75);
    m.set_attribute(
        attribute::DAMAGE_GRAPHICS_TOTAL_NUM,
        f32::NAN,
        &world(false),
    );
    assert_eq!(m.damage_graphics.len(), 255);
    m.set_attribute(attribute::DAMAGE_GRAPHICS_TOTAL_NUM, 0.0, &world(false));
    assert_eq!(m.damage_graphic_entry, 1);
    m.damage_graphics = vec![DamageGraphic::default()];
    m.multiply_attribute(attribute::DAMAGE_GRAPHICS_ENTRY_MOD, -3.0, &world(false));
    assert_eq!(m.damage_graphics, []);
    assert_eq!(m.damage_graphic_entry, 0xff);
    m.follow(
        attribute::DAMAGE_GRAPHICS_ENTRY_MOD,
        &std::collections::BTreeMap::new(),
        None,
    );
    assert!(!m.damage_sprites);
}

/// `FUN_14123d310`: a building master's garrison heal rate, foundation and garrison type are
/// its own; its six sprites go to a copy (nothing changes); the rest falls to the combat
/// master's setter (the regeneration from the hit points, 120).
#[test]
fn a_building_master_s_setters_fall_back_on_the_combat_master_s() {
    let mut m = of_type(master_type::BUILDING);
    m.building = Some(BuildingMaster {
        foundation_terrain: Some(TerrainId::named(0)),
        ..BuildingMaster::default()
    });
    m.hit_points = MasterHitPoints::of_data(600);
    m.set_attribute(attribute::GARRISON_HEAL_RATE, 2.0, &world(false));
    m.multiply_attribute(attribute::GARRISON_HEAL_RATE, 1.5, &world(false));
    m.set_attribute(attribute::CONSTRUCTION_GRAPHIC, 9.0, &world(false));
    m.set_attribute(attribute::REGENERATION_HP_PERCENT, 0.5, &world(false));
    m.add_attribute(attribute::FOUNDATION_TERRAIN, 2.0, &world(false));
    let b = m.building.unwrap();
    assert_eq!(
        (b.garrison_heal_rate, b.foundation_terrain),
        (HitPoints::new(3.0), Some(TerrainId::named(2)))
    );
    assert_eq!(m.regeneration, 300.0);
    assert_eq!(m, {
        let mut same = of_type(master_type::BUILDING);
        same.building = Some(b);
        same.hit_points = MasterHitPoints::of_data(600);
        same.regeneration = 300.0;
        same
    });
}

/// The foundation terrain's add works on the game's short: none is −1, so an add of 2 lays
/// terrain 1, and a set below 0 is none again.
#[test]
fn a_foundation_terrain_add_counts_none_as_minus_one() {
    let mut m = of_type(master_type::BUILDING);
    m.building = Some(BuildingMaster::default());
    m.add_attribute(attribute::FOUNDATION_TERRAIN, 2.0, &world(false));
    assert_eq!(
        m.building.as_ref().and_then(|b| b.foundation_terrain),
        Some(TerrainId::named(1))
    );
    m.set_attribute(attribute::FOUNDATION_TERRAIN, -1.0, &world(false));
    assert_eq!(m.building.as_ref().and_then(|b| b.foundation_terrain), None);
}

/// What a unit moving with a master reads follows the master's values: its radii, its base
/// speed and its speeds by its sprites' factors, its turn time and its facets.
#[test]
fn a_moving_master_s_derived_values_follow_its_attributes() {
    let graphic = |id, speed_multiplier, facets| crate::objects::sprite::Graphic {
        id,
        frames: 10,
        facets,
        speed_multiplier,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::ANIMATED,
    };
    let graphics = [
        (SpriteId::named(1), graphic(SpriteId::named(1), 0.0, 8)),
        (SpriteId::named(2), graphic(SpriteId::named(2), 1.5, 8)),
        (SpriteId::named(3), graphic(SpriteId::named(3), 0.5, 16)),
    ]
    .into_iter()
    .collect();
    let mut m = of_type(master_type::COMBAT);
    m.standing_sprite = Some(SpriteId::named(1));
    m.move_sprite = Some(SpriteId::named(2));
    m.run_sprite = None;
    m.max_speed = Speed::new(1.0);
    m.radius = Footprint::new(0.2, 0.2);
    m.moving = Some(MoveStats {
        fly_mode: FlyMode::GROUND,
        speed: Speed::new(1.5),
        base: Speed::new(1.5),
        turn_rate: TurnRate::new(1.0),
        turn_radius: Distance::ZERO,
        turn_radius_speed: Speed::MAX,
        radius: Radii::new(0.2, 0.2, 2.0),
        restriction: RestrictionId::named(7),
        restriction_mask: TerrainMask::of_bits(1),
        turn_time: Seconds::ZERO,
        move_algorithm: MoveAlgorithm::NONE,
        min_collision: 0.0,
        fast_turn: false,
        facets: None,
        weapon_range: Distance::ZERO,
    });
    m.multiply_attribute(attribute::MOVEMENT_SPEED, 1.1, &world(false));
    m.follow(attribute::MOVEMENT_SPEED, &graphics, None);
    let s = m.moving.unwrap();
    assert_eq!((s.base, s.speed), (Speed::new(1.1), Speed::new(1.1 * 1.5)));
    m.set_attribute(attribute::SIZE_Z, 1.0, &world(false));
    m.follow(attribute::SIZE_Z, &graphics, None);
    assert!(m.tall);
    assert_eq!(m.moving.unwrap().radius, Radii::new(0.2, 0.2, 1.0));
    m.set_attribute(attribute::RUNNING_GRAPHIC, 3.0, &world(false));
    m.follow(attribute::RUNNING_GRAPHIC, &graphics, None);
    let s = m.moving.unwrap();
    assert_eq!(s.speed, Speed::new(1.1 * 0.5));
    // 8 facets and 16: no common count
    assert_eq!(s.facets, None);
    m.set_attribute(attribute::RUNNING_GRAPHIC, 2.0, &world(false));
    m.follow(attribute::RUNNING_GRAPHIC, &graphics, None);
    assert_eq!(
        m.moving.unwrap().facets,
        Some(crate::objects::facing::SpriteFacets {
            count: 8,
            turns: false
        })
    );
}

/// The missile, the trail's unit and the terrain bonus keep the short the effects leave: set
/// to −3 and 5 added is 2, as in the game, where a field read as none from −1 would give 4; a
/// multiply's negative result is kept as well.
#[test]
fn an_id_field_keeps_a_negative_value_through_an_add() {
    let w = world(false);
    let mut m = of_type(master_type::RGE_COMBAT);
    for a in [attribute::PROJECTILE_UNIT, attribute::TERRAIN_DEFENSE_BONUS] {
        m.set_attribute(a, -3.0, &w);
    }
    assert_eq!(
        (m.missile.master(), m.terrain_bonus.restriction()),
        (None, None)
    );
    for a in [attribute::PROJECTILE_UNIT, attribute::TERRAIN_DEFENSE_BONUS] {
        m.add_attribute(a, 5.0, &w);
    }
    assert_eq!(m.missile.master(), Some(master_id(2)));
    assert_eq!(m.terrain_bonus.restriction(), RestrictionId::of_data(2));
    // 2 × −2 + 0.5 truncates to −3, kept: plus 4 is 1
    m.multiply_attribute(attribute::PROJECTILE_UNIT, -2.0, &w);
    m.add_attribute(attribute::PROJECTILE_UNIT, 4.0, &w);
    assert_eq!(m.missile.get(), 1);
    let mut t = of_type(master_type::MOVING);
    t.set_attribute(attribute::TRAILING_UNIT, -3.0, &w);
    t.add_attribute(attribute::TRAILING_UNIT, 5.0, &w);
    assert_eq!(t.trail.unit.master(), Some(master_id(2)));
}

/// The action master's (`FUN_14109fbb0` and its add and multiply) work rate, search radius
/// and run pattern; the moving master's (`FUN_1410e1c00` ...) turn speed and trail; the
/// interface's sounds change nothing.
#[test]
fn the_action_and_moving_setters_change_their_values() {
    let mut m = of_type(master_type::ACTION);
    m.set_attribute(attribute::WORK_RATE, 0.5, &world(false));
    m.add_attribute(attribute::WORK_RATE, 0.25, &world(false));
    m.multiply_attribute(attribute::WORK_RATE, 2.0, &world(false));
    m.set_attribute(attribute::SEARCH_RADIUS, 4.0, &world(false));
    m.multiply_attribute(attribute::SEARCH_RADIUS, 1.5, &world(false));
    m.set_attribute(attribute::RUN_PATTERN, 2.0, &world(false));
    m.add_attribute(attribute::RUN_PATTERN, 255.0, &world(false));
    m.set_attribute(attribute::ROTATION_SPEED, 0.5, &world(false));
    m.multiply_attribute(attribute::ROTATION_SPEED, 2.0, &world(false));
    m.set_attribute(attribute::TRAILING_UNIT, 1300.0, &world(false));
    m.add_attribute(attribute::TRAILING_UNIT, 2.0, &world(false));
    m.set_attribute(attribute::TRAIL_MODE, 3.0, &world(false));
    m.set_attribute(attribute::TRAIL_DENSITY, 1.5, &world(false));
    m.multiply_attribute(attribute::TRAIL_DENSITY, 2.0, &world(false));
    let before = m.clone();
    m.set_attribute(attribute::COMMAND_SOUND, 9.0, &world(false));
    m.add_attribute(attribute::COMMAND_SOUND, 9.0, &world(false));
    assert_eq!(m, before);
    assert_eq!(
        (m.work_rate, m.search_radius, m.run_pattern),
        (
            WorkRate::new(1.5),
            Distance::new(6.0),
            RunPattern::of_data(1)
        )
    );
    assert_eq!(m.turn_speed, Seconds::new(1.0));
    assert_eq!(
        m.trail,
        Trail {
            unit: MasterField::of(Some(master_id(1302))),
            mode: 3,
            density: 3.0
        }
    );
}

/// The combat master's own values (`FUN_1412661d0` and its add and multiply): hero status,
/// charge, conversion, regeneration (by the hit points' share for 120), volleys, the second
/// attack sprite (an add from its id), the spawning area; the ability's interface values
/// change nothing.
#[test]
fn the_combat_master_s_own_values_change() {
    let mut m = of_type(master_type::COMBAT);
    m.hit_points = MasterHitPoints::of_data(100);
    m.attack_sprite2 = Some(SpriteId::named(10));
    m.set_attribute(attribute::HERO_STATUS, 1.0, &world(false));
    m.add_attribute(attribute::HERO_STATUS, 4.0, &world(false));
    m.set_attribute(attribute::MAXIMUM_CHARGE, 10.0, &world(false));
    m.multiply_attribute(attribute::MAXIMUM_CHARGE, 1.5, &world(false));
    m.set_attribute(attribute::RECHARGE_RATE, 0.5, &world(false));
    m.set_attribute(attribute::CHARGE_EVENT, 2.0, &world(false));
    m.set_attribute(attribute::CHARGE_TYPE, 6.0, &world(false));
    m.set_attribute(attribute::MAX_CONVERSION_TIME_MODIFIER, 3.0, &world(false));
    m.add_attribute(attribute::MAX_CONVERSION_TIME_MODIFIER, 1.0, &world(false));
    m.set_attribute(attribute::REGENERATION_HP_PERCENT, 0.25, &world(false));
    m.add_attribute(attribute::REGENERATION_RATE, 5.0, &world(false));
    m.set_attribute(attribute::TOTAL_MISSILES, 2.0, &world(false));
    m.multiply_attribute(attribute::MAXIMUM_TOTAL_MISSILES, 3.0, &world(false));
    m.add_attribute(attribute::ATTACK_GRAPHIC_2, 2.0, &world(false));
    m.set_attribute(attribute::SPAWNING_AREA_RANDOMNESS, 0.5, &world(false));
    m.set_attribute(attribute::SECONDARY_PROJECTILE_UNIT, -1.0, &world(false));
    m.set_attribute(attribute::CHARGE_PROJECTILE_UNIT, 1500.0, &world(false));
    let before = m.clone();
    m.set_attribute(attribute::ABILITY_BUTTON_ICON_ID, 9.0, &world(false));
    assert_eq!(m, before);
    assert_eq!(
        (m.hero_flag, m.max_charge, m.recharge_rate),
        (
            HeroFlags::FULL_HERO | HeroFlags::REGENERATES,
            Charge::new(15.0),
            ChargeRate::new(0.5)
        )
    );
    assert_eq!((m.charge_event.get(), m.charge_type.get()), (2, 6));
    assert_eq!(
        m.conversion,
        ConversionAdjust {
            min_time: 0.0,
            max_time: 4.0,
            chance: 0.0
        }
    );
    assert_eq!(m.regeneration, 30.0);
    assert_eq!(
        (m.volley_fire, m.max_volley, m.attack_sprite2),
        (2.0, 0, Some(SpriteId::named(12)))
    );
    assert_eq!(
        m.volley_spread,
        VolleySpread {
            x: 0.0,
            y: 0.0,
            start: 0.5
        }
    );
    assert_eq!(
        (m.volley_missile, m.charge_projectile),
        (None, Some(master_id(1500)))
    );
}

/// The missile master's add (`FUN_1410d5ab0`) adds to its own bytes and arc, then runs the
/// RGE combat master's; a building master's add (`FUN_14123d530`) its facet and garrison
/// type; an animated master's set (`FUN_1410a8b00`) its speed.
#[test]
fn the_missile_building_and_animated_setters() {
    let mut m = of_type(master_type::MISSILE);
    m.missile_stats = Some(crate::combat::missile::MissileStats {
        projectile_type: 0,
        smart_mode: SmartMode::NONE,
        hit_mode: 1,
        vanish_mode: VanishMode::NONE,
        arc: 0.25,
        speed: Speed::new(7.0),
        radius: Radii::ZERO,
        fly_mode: FlyMode::GROUND,
    });
    m.add_attribute(attribute::PROJECTILE_SMART_MODE, 1.0, &world(false));
    m.add_attribute(attribute::PROJECTILE_HIT_MODE, 1.0, &world(false));
    m.add_attribute(attribute::PROJECTILE_ARC, 0.25, &world(false));
    m.set_attribute(attribute::MAXIMUM_RANGE, 5.0, &world(false));
    let s = m.missile_stats.unwrap();
    assert_eq!(
        (s.smart_mode, s.hit_mode, s.arc),
        (SmartMode::LEADS_TARGET, 2, 0.5)
    );
    assert_eq!(m.weapon_range, Distance::new(5.0));
    let mut b = of_type(master_type::BUILDING);
    b.building = Some(BuildingMaster::default());
    b.add_attribute(attribute::BUILDING_ICON_OVERRIDE, 3.0, &world(false));
    b.add_attribute(attribute::GARRISON_TYPE, 2.0, &world(false));
    assert_eq!(
        (b.building_facet, b.building.unwrap().garrison_type),
        (3, GarrisonKinds::INFANTRY)
    );
    let mut a = of_type(master_type::DOPPELGANGER);
    a.set_attribute(attribute::MOVEMENT_SPEED, 1.2, &world(false));
    assert_eq!(a.max_speed, Speed::new(1.2));
}

/// The master copy (slot 8, `Master::copy_from`): every value but the id, class,
/// availability, disabling and base id (`+0x260`).
#[test]
fn a_master_copy_keeps_its_id_class_and_flags() {
    let mut militia = master(
        master_id(74),
        1,
        vec![task(action_type::COMBAT, -1, -1, 0.0)],
    );
    militia.class = class::CLASS_6;
    militia.available = true;
    militia.base_id = Some(master_id(74));
    let mut man = master(
        master_id(75),
        2,
        vec![
            task(action_type::COMBAT, -1, -1, 0.0),
            task(crate::garrison::ENTER, 3, -1, 0.0),
        ],
    );
    man.class = class::CLASS_7;
    man.hit_points = MasterHitPoints::of_data(45);
    man.disabled = true;
    militia.copy_from(&man);
    assert_eq!(
        (militia.id, militia.class, militia.base_id),
        (master_id(74), class::CLASS_6, Some(master_id(74)))
    );
    assert!(militia.available && !militia.disabled);
    assert_eq!(
        (militia.hit_points, militia.tasks.len()),
        (MasterHitPoints::of_data(45), 2)
    );
    assert_eq!(militia.task_by_group, TaskGroup::FEMALE_VILLAGERS);
}

/// The work command's choice for a gather (0x6a) on a farm (master 50, one of the objects
/// the work command lists): the first master of the swap group with a task for the farm's
/// class and a work value wins over the first with a task for any target, though later; a
/// task naming the farm's master is taken at once; a master whose task has the own task's
/// type is passed over. A unit of a master the player lacks is not modelled.
#[test]
fn a_villager_on_a_farm_takes_its_group_s_farmer_for_the_farm_s_class() {
    let farm = Master {
        id: ids::FARM,
        kind: master_type::BUILDING,
        class: class::FARM,
        ..Master::default()
    };
    let gather = |group, id, work| task(crate::construction::repair::REPAIR, group, id, work);
    let with = |own: Vec<Task>, others: Vec<Master>| {
        let mut all = vec![master(ids::VILLAGER_MALE, 1, own), farm.clone()];
        all.extend(others);
        Masters::new(all)
    };
    let pick = |m: &Masters, own: &Master| {
        let site = TaskTarget {
            id: object_id(12557),
            master: m.get(ids::FARM).unwrap(),
            player: PlayerId::new(2),
            state: ObjectState::Alive,
            held: carrying(None, 0.0),
            hp: HitPoints::new(1.0),
            max_hp: HitPoints::new(480.0),
            attackable: true,
            pasture: None,
            top_action: None,
        };
        let actor = Actor {
            id: object_id(9181),
            master: own,
            player: PlayerId::new(2),
            held: carrying(None, 0.0),
            attribute_95: Amount::ZERO,
            civ: CivId::new(1),
        };
        let on = crate::unit_ai::task_finder::TaskOn {
            target: Some(&site),
            general: Some(&site),
            terrain: None,
        };
        m.work_master(&actor, &on, None, &Alone).map(|w| {
            w.map(|(id, t)| {
                (
                    id.get(),
                    ClassId::field(t.object_group),
                    MasterId::field(t.object_id),
                )
            })
        })
    };
    let own = |m: &Masters| m.get(ids::VILLAGER_MALE).unwrap().clone();
    let generic = master(master_id(120), 1, vec![gather(-1, -1, 1.0)]);
    let by_class = master(master_id(122), 1, vec![gather(49, -1, 0.5)]);
    let m = with(vec![], vec![generic.clone(), by_class.clone()]);
    assert_eq!(pick(&m, &own(&m)), Ok(Some((122, 49, -1))));
    let m = with(vec![], vec![generic]);
    assert_eq!(pick(&m, &own(&m)), Ok(Some((120, -1, -1))));
    let idle = master(master_id(122), 1, vec![gather(49, -1, 0.0)]);
    let m = with(vec![], vec![idle]);
    assert_eq!(pick(&m, &own(&m)), Ok(None));
    let named = master(master_id(120), 1, vec![gather(-1, ids::FARM.get(), 0.0)]);
    let m = with(vec![], vec![named, by_class.clone()]);
    assert_eq!(pick(&m, &own(&m)), Ok(Some((120, -1, 50))));
    let attacker = master(
        master_id(120),
        1,
        vec![task(action_type::COMBAT, -1, -1, 0.0)],
    );
    let m = with(
        vec![task(action_type::COMBAT, -1, -1, 0.0)],
        vec![attacker, by_class],
    );
    assert_eq!(pick(&m, &own(&m)), Ok(Some((122, 49, -1))));
    let stranger = master(master_id(999), 1, vec![]);
    assert!(pick(&m, &stranger).is_err());
}

#[test]
fn a_villager_takes_the_first_master_of_its_swap_group_with_a_task_for_its_foundation() {
    let m = villagers();
    let site = TaskTarget {
        id: object_id(10028),
        master: m.get(ids::HOUSE).unwrap(),
        player: PlayerId::new(2),
        state: ObjectState::Foundation,
        held: carrying(None, 0.0),
        hp: HitPoints::new(1.0),
        max_hp: HitPoints::new(550.0),
        attackable: true,
        pasture: None,
        top_action: None,
    };
    let pick = |own| {
        let actor = Actor {
            id: object_id(9181),
            master: m.get(own).unwrap(),
            player: PlayerId::new(2),
            held: carrying(None, 0.0),
            attribute_95: Amount::ZERO,
            civ: CivId::new(1),
        };
        let on = crate::unit_ai::task_finder::TaskOn {
            target: Some(&site),
            general: Some(&site),
            terrain: None,
        };
        m.work_master(&actor, &on, None, &Alone)
            .map(|w| w.map(|(id, t)| (id.get(), t.action_type)))
    };
    assert_eq!(
        pick(ids::VILLAGER_MALE),
        Ok(Some((118, action_type::BUILD)))
    );
    assert_eq!(
        pick(ids::VILLAGER_FEMALE),
        Ok(Some((212, action_type::BUILD)))
    );
    assert_eq!(pick(ids::BUILDER), Ok(Some((118, action_type::BUILD))));
}

#[test]
fn a_master_s_unit_ai_type_reads_its_class_volley_missile_and_combat_level() {
    let master = |class, volley_missile, combat_level| Master {
        class,
        combat_level,
        volley_missile,
        ..Master::default()
    };
    // a building without a volley missile, with one (the tower's), and a boar's combat level
    assert_eq!(
        master(class::BUILDING, None, CombatLevel::NONE).unit_ai_type(ids::HOUSE),
        Some(UnitAiType::BUILDING)
    );
    assert_eq!(
        master(class::BUILDING, Some(master_id(5)), CombatLevel::NONE).unit_ai_type(master_id(79)),
        Some(UnitAiType::TOWER)
    );
    assert_eq!(
        master(class::PREDATOR, None, CombatLevel::ANIMAL).unit_ai_type(master_id(48)),
        Some(UnitAiType::BOAR)
    );
    assert_eq!(
        master(class::PREDATOR, None, CombatLevel::NONE).unit_ai_type(master_id(126)),
        Some(UnitAiType::LION)
    );
    // the factory's own ids first
    assert_eq!(
        master(class::BUILDING, None, CombatLevel::NONE).unit_ai_type(ids::ID_1105),
        Some(UnitAiType::SIEGE_TOWER)
    );
}

/// The lookup by player and id: the player's table's master, none without the table or the
/// master.
#[test]
fn a_master_is_found_by_its_player_and_id() {
    let every = vec![Masters::default(), villagers()];
    let found = player_master(&every, PlayerId::new(1), ids::VILLAGER_MALE).map(|m| m.id);
    assert_eq!(found, Some(ids::VILLAGER_MALE));
    assert!(player_master(&every, PlayerId::new(0), ids::VILLAGER_MALE).is_none());
    assert!(player_master(&every, PlayerId::new(2), ids::VILLAGER_MALE).is_none());
}

/// The world's masters by player: a player's own table, none beyond the tables.
#[test]
fn the_world_s_masters_are_looked_up_by_player() {
    use crate::objects::masters::PlayersMasters;
    let mut every = [Masters::new([house()]), Masters::new([])];
    assert!(
        every
            .of(PlayerId::new(0))
            .is_some_and(|m| m.get(house().id).is_some())
    );
    assert!(
        every
            .of(PlayerId::new(1))
            .is_some_and(|m| m.get(house().id).is_none())
    );
    assert!(every.of(PlayerId::new(2)).is_none());
    *every.of_mut(PlayerId::new(1)).unwrap() = Masters::new([house()]);
    assert!(
        every
            .of(PlayerId::new(1))
            .is_some_and(|m| m.get(house().id).is_some())
    );
    assert!(every.of_mut(PlayerId::new(2)).is_none());
}
