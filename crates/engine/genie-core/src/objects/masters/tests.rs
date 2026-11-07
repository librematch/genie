use super::*;
use crate::combat::Charge;
use crate::combat::ChargeRate;
use crate::combat::Firepower;
use crate::combat::MissileCount;
use crate::combat::missile::ArcRatio;
use crate::combat::missile::HitMode;
use crate::combat::missile::ProjectileKind;
use crate::combat::missile::SmartMode;
use crate::combat::missile::VanishMode;
use crate::construction::repair::REPAIR;
use crate::garrison::ENTER;
use crate::gathering::gather::GATHER;
use crate::gathering::hunt::HUNT;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Offset;
use crate::map::Radii;
use crate::map::RestrictionId;
use crate::map::TerrainId;
use crate::map::TerrainMask;
use crate::movement::MoveAlgorithm;
use crate::movement::MoveStats;
use crate::movement::Speed;
use crate::movement::TurnRate;
use crate::objects::FacetCount;
use crate::objects::Frame;
use crate::objects::Graphics;
use crate::objects::HitPoints;
use crate::objects::MasterHitPoints;
use crate::objects::ObjectState;
use crate::objects::Regeneration;
use crate::objects::SpeedMultiplier;
use crate::objects::SpriteFlags;
use crate::objects::SpriteId;
use crate::objects::WorkRate;
use crate::objects::facing::SpriteFacets;
use crate::objects::masters::Availability;
use crate::objects::masters::DisableMode;
use crate::objects::sprite::Graphic;
use crate::players::Amount;
use crate::players::CivId;
use crate::players::PlayerId;
use crate::production::HotkeyId;
use crate::production::LocationButton;
use crate::test_ids::carrying;
use crate::test_ids::costs;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::test_ids::{class_of, opt_master};
use crate::testing::attribute as player_attribute;
use crate::testing::class;
use crate::testing::ids;
use crate::testing::roles;
use crate::testing::slots;
use crate::testing::static_roles;
use crate::testing::tuning;
use crate::unit_ai::UnitAiKind;
use crate::unit_ai::task_finder::TaskOn;
use crate::unit_ai::unit_ai_kind;
use crate::world::Seconds;
use crate::world::WholeSeconds;
use std::sync::Arc;
use std::sync::LazyLock;

/// A sprite field as the attribute tests read it: its id, −1 for none.
fn sprite_value(sprite_id: Option<SpriteId>) -> f32 {
    f32::from(sprite_id.map_or(-1, SpriteId::get))
}

fn task(action_kind: ActionKind, object_group: i16, object_id: i16, work_val1: f32) -> Task {
    Task {
        action_kind,
        object_group: class_of(object_group),
        object_id: opt_master(object_id),
        work_val1: WorkValue::new(work_val1),
        ..Task::default()
    }
}

fn master(id: MasterId, task_by_group: u8, tasks: Vec<Task>) -> Master {
    Master {
        id,
        kind: MasterKind::COMBAT,
        class: class::VILLAGER,
        radius: Footprint::new(0.2, 0.2),
        height: Distance::new(1.0),
        obstruction_kind: ObstructionKind::NONE,
        buildable_on: BuildableOn::NO,
        obstruction_class: ObstructionClass::NONE,
        object_flags: ObjectFlags::NONE,
        combat_level: CombatLevel::NONE,
        hit_points: MasterHitPoints::of_data(25),
        creatable: Some(CreatablePart {
            cost: costs(&[]),
            build_time: WholeSeconds::ZERO,
            ..CreatablePart::default()
        }),
        action: Some(ActionPart {
            task_by_group: TaskGroup::of_data(task_by_group),
            tasks,
            work_rate: WorkRate::new(1.0),
            ..ActionPart::default()
        }),
        ..Master::default()
    }
}

fn house() -> Master {
    Master {
        id: ids::HOUSE,
        kind: MasterKind::BUILDING,
        class: class::BUILDING,
        radius: Footprint::new(1.0, 1.0),
        height: Distance::new(1.0),
        obstruction_kind: ObstructionKind::NONE,
        buildable_on: BuildableOn::NO,
        obstruction_class: ObstructionClass::UNIT,
        object_flags: ObjectFlags::NONE,
        combat_level: CombatLevel::NONE,
        hit_points: MasterHitPoints::of_data(550),
        creatable: Some(CreatablePart {
            cost: costs(&[(player_attribute::WOOD, 25, true)]),
            build_time: WholeSeconds::of_data(25),
            ..CreatablePart::default()
        }),
        action: Some(ActionPart {
            task_by_group: TaskGroup::NONE,
            tasks: vec![],
            work_rate: WorkRate::ZERO,
            ..ActionPart::default()
        }),
        ..Master::default()
    }
}

/// The villagers of build 158041's `.dat`, shortened: the base masters have no Build task, the
/// builders a generic one; a fisherman (56) of the male group has none either.
fn villagers() -> Masters {
    let base = || {
        vec![
            task(ActionKind::COMBAT, -1, -1, 0.0),
            task(ENTER, 3, -1, 0.0),
            task(HUNT, -1, 126, 0.0),
        ]
    };
    let builder = || {
        vec![
            task(ActionKind::BUILD, -1, -1, 1.0),
            task(ENTER, 3, -1, 0.0),
        ]
    };
    Masters::new([
        master(master_id(56), 1, vec![task(GATHER, -1, 53, 1.0)]),
        master(ids::VILLAGER_MALE, 1, base()),
        master(ids::BUILDER, 1, builder()),
        master(master_id(212), 2, builder()),
        master(ids::VILLAGER_FEMALE, 2, base()),
        house(),
    ])
}

/// Every player on its own (the world holds no player, so none has an ally), no drop sites.
const ALONE: TaskWorld<'static> = TaskWorld {
    players: &[],
    drop_sites: &[],
};

/// A sprite table holding the ids 1 to 99.
static SPRITES: LazyLock<Graphics> = LazyLock::new(|| {
    (1..100)
        .map(|id| {
            let sprite = SpriteId::named(id);
            let graphic = Graphic {
                id: sprite,
                frames: Frame::of_data(1),
                facets: FacetCount::of_data(1),
                speed_multiplier: SpeedMultiplier::ONE,
                frame_duration: Seconds::ZERO,
                replay_delay: Seconds::ZERO,
                flags: SpriteFlags::empty(),
            };
            (sprite, graphic)
        })
        .collect()
});

/// A world whose sprite table holds the ids 1 to 99, without a handicap.
fn world(capped: bool) -> AttributeWorld<'static> {
    AttributeWorld {
        sprites: &SPRITES,
        handicap: None,
        capped,
        roles: static_roles(),
        masters_tuning: &tuning().masters,
    }
}

fn of_kind(kind: MasterKind) -> Master {
    Master::of_kind(kind)
}

/// A field of a master, read as an `f32`.
type Read = fn(&Master) -> f32;

/// A master of `kind` with its kind's layers ([`Master::of_kind`]): three storage entries, 10 hit
/// points, two damage sprites and, with the creatable layer, a paid food cost and two train
/// locations (the second of each selected).
fn stocked(kind: MasterKind) -> Master {
    let base = Master::of_kind(kind);
    Master {
        hit_points: MasterHitPoints::of_data(10),
        storage: vec![
            (Some(player_attribute::FOOD), Amount::new(1.0), 1),
            (Some(player_attribute::WOOD), Amount::new(1.0), 1),
            (Some(player_attribute::STONE), Amount::new(1.0), 1),
        ],
        damage_graphics: vec![DamageGraphic::default(); 2],
        damage_graphic_entry: DamageGraphicEntry::of_data(1),
        creatable: base.creatable.as_ref().map(|_| CreatablePart {
            cost: costs(&[(player_attribute::FOOD, 1, true)]),
            train_locations: vec![TrainLocation::default(); 2],
            train_location_entry: TrainLocationEntry::of_data(1),
            ..CreatablePart::default()
        }),
        ..base
    }
}

/// Each row (attribute, its field, the field after a set of 3, then after an add of 2, then
/// after a multiply by 2) on a fresh [`stocked`] master of `kind`.
fn sets_adds_multiplies(kind: MasterKind, rows: &[(MasterAttribute, Read, [f32; 3])]) {
    let attribute_world = world(false);
    for &(master_attribute, read, [set, add, multiply]) in rows {
        let mut master = stocked(kind);
        master.set_attribute(master_attribute, 3.0, &attribute_world);
        assert_eq!(read(&master), set, "set {master_attribute}");
        master.add_attribute(master_attribute, 2.0, &attribute_world);
        assert_eq!(read(&master), add, "add {master_attribute}");
        master.multiply_attribute(master_attribute, 2.0, &attribute_world);
        assert_eq!(read(&master), multiply, "multiply {master_attribute}");
    }
}

/// `FUN_141126400`, `FUN_141126af0` and `FUN_1411271e0`: each static attribute's field takes
/// the set, the add and the multiply its arm has (a multiply rounds the shorts and bytes by
/// 0.5, the blast defence not); the others leave it.
#[test]
fn each_static_attribute_sets_adds_and_multiplies_its_field() {
    let rows: &[(MasterAttribute, Read, [f32; 3])] = &[
        (
            MasterAttribute::HIT_POINTS,
            |master| HitPoints::of_master(master.hit_points).get(),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::LINE_OF_SIGHT,
            |master| master.line_of_sight.get(),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::GARRISON_CAPACITY,
            |master| f32::from(master.garrison.get()),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::SIZE_X,
            |master| master.radius.x(),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::SIZE_Y,
            |master| master.radius.y(),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::CARRY_CAPACITY,
            |master| f32::from(master.capacity.get()),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::STORAGE_1,
            |master| master.storage[0].1.get(),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::STORAGE_2,
            |master| master.storage[1].1.get(),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::STORAGE_3,
            |master| master.storage[2].1.get(),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::FOG_VISIBILITY,
            |master| f32::from(master.fog_visibility.get()),
            [3.0, 3.0, 3.0],
        ),
        (
            MasterAttribute::OCCLUSION_MODE,
            |master| f32::from(master.occlusion.get()),
            [3.0, 5.0, 5.0],
        ),
        (
            MasterAttribute::SIZE_Z,
            |master| master.height.get(),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::CAN_BE_BUILT_ON,
            |master| f32::from(master.blast_defense.get()),
            [3.0, 5.0, 5.0],
        ),
        (
            MasterAttribute::BLAST_DEFENSE_LEVEL,
            |master| f32::from(master.blast_defense.get()),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::TERRAIN_RESTRICTION_ID,
            |master| f32::from(RestrictionId::field(master.terrain_restriction)),
            [3.0, 3.0, 3.0],
        ),
        (
            MasterAttribute::UNIT_TRAIT,
            |master| master.object_flags.bits() as f32,
            [3.0, 5.0, 5.0],
        ),
        (
            MasterAttribute::TRAIT_PIECE,
            |master| (master.object_flags.bits() >> 16) as f32,
            [3.0, 3.0, 3.0],
        ),
        (
            MasterAttribute::DEAD_UNIT_ID,
            |master| f32::from(MasterId::field(master.death_spawns[0])),
            [3.0, 3.0, 3.0],
        ),
        (
            MasterAttribute::BLOOD_UNIT,
            |master| f32::from(MasterId::field(master.death_spawns[1])),
            [3.0, 3.0, 3.0],
        ),
        (
            MasterAttribute::STANDING_GRAPHIC,
            |master| sprite_value(master.standing_sprite),
            [3.0, 5.0, 5.0],
        ),
        (
            MasterAttribute::STANDING_GRAPHIC_2,
            |master| sprite_value(master.standing_sprite_2),
            [3.0, 5.0, 5.0],
        ),
        (
            MasterAttribute::DYING_GRAPHIC,
            |master| sprite_value(master.death_sprite),
            [3.0, 5.0, 5.0],
        ),
        (
            MasterAttribute::UNDEAD_GRAPHIC,
            |master| sprite_value(master.undead_sprite),
            [3.0, 5.0, 5.0],
        ),
        (
            MasterAttribute::OBSTRUCTION_KIND,
            |master| f32::from(master.obstruction_kind.get()),
            [3.0, 3.0, 3.0],
        ),
        (
            MasterAttribute::BLOCKAGE_CLASS,
            |master| f32::from(master.obstruction_class.get()),
            [3.0, 3.0, 3.0],
        ),
        (
            MasterAttribute::SELECTION_EFFECT,
            |master| f32::from(master.selection_effect.get()),
            [3.0, 3.0, 3.0],
        ),
        (
            MasterAttribute::DAMAGE_GRAPHIC,
            |master| sprite_value(master.damage_graphics[1].sprite),
            [3.0, 5.0, 5.0],
        ),
        (
            MasterAttribute::AVAILABLE_UNIT_FLAG,
            |master| f32::from(master.available.get()),
            // the byte the game keeps: an add wraps, a scale rounds (not a flag of 0 or 1)
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::DISABLED_UNIT_FLAG,
            |master| f32::from(master.disabled.get()),
            [3.0, 5.0, 5.0],
        ),
        (
            MasterAttribute::INTERFACE_KIND,
            |master| f32::from(master.unit_level.get()),
            [3.0, 5.0, 5.0],
        ),
        (
            MasterAttribute::COMBAT_LEVEL,
            |master| f32::from(master.combat_level.get()),
            [3.0, 5.0, 5.0],
        ),
        (
            MasterAttribute::INTERACTION_MODE,
            |master| f32::from(master.select_level.get()),
            [3.0, 5.0, 5.0],
        ),
        (
            MasterAttribute::MINIMAP_MODE,
            |master| f32::from(master.map_draw_level.get()),
            [3.0, 5.0, 5.0],
        ),
        // the selected entry plus ten times the count: the set and the add select at most
        // the last, the multiply inserts at 2 and selects it
        (
            MasterAttribute::DAMAGE_GRAPHICS_ENTRY_MOD,
            |master| {
                f32::from(master.damage_graphic_entry.get())
                    + 10.0 * master.damage_graphics.len() as f32
            },
            [21.0, 21.0, 32.0],
        ),
        // the add's count is the selected entry (1) plus 2
        (
            MasterAttribute::DAMAGE_GRAPHICS_TOTAL_NUM,
            |master| master.damage_graphics.len() as f32,
            [3.0, 3.0, 6.0],
        ),
        (
            MasterAttribute::DAMAGE_GRAPHIC_PERCENT,
            |master| f32::from(master.damage_graphics[1].percent.get()),
            [3.0, 5.0, 5.0],
        ),
        (
            MasterAttribute::DAMAGE_GRAPHIC_APPLY_MODE,
            |master| f32::from(master.damage_graphics[1].apply_mode.get()),
            [3.0, 5.0, 5.0],
        ),
    ];
    sets_adds_multiplies(MasterKind::STATIC, rows);
    // an obstruction type the table does not hold is 0; the trait piece keeps the low half;
    // a flag of 0 is off
    let mut master = stocked(MasterKind::STATIC);
    master.object_flags = ObjectFlags::from_bits_retain(0x1234);
    master.set_attribute(MasterAttribute::TRAIT_PIECE, 2.0, &world(false));
    master.set_attribute(MasterAttribute::OBSTRUCTION_KIND, 4.0, &world(false));
    master.set_attribute(MasterAttribute::AVAILABLE_UNIT_FLAG, 0.0, &world(false));
    assert_eq!(
        (master.object_flags, master.obstruction_kind),
        (
            ObjectFlags::from_bits_retain(0x2_1234),
            ObstructionKind::NONE
        )
    );
    assert!(!master.available.is_available());
}

/// `FUN_1410c1bf0` on the armor and attack amounts: a percentage of the class's entry plus
/// 0.5, truncated; a class the master lacks changes nothing; an attack class at 0 the handicap
/// An effect on a layer the master lacks changes nothing: a moving master (30) holds no action
/// or combat layer, so its setters (the moving master's slots, then the animated and the static
/// one) pass the attack, armour, range, reload and work rate by, as the game's do.
#[test]
fn effects_on_layers_the_master_lacks_change_nothing() {
    let before = of_kind(MasterKind::MOVING);
    let mut master = before.clone();
    for master_attribute in [
        MasterAttribute::ATTACK,
        MasterAttribute::ARMOR,
        MasterAttribute::MAXIMUM_RANGE,
        MasterAttribute::ATTACK_RELOAD_TIME,
        MasterAttribute::WORK_RATE,
    ] {
        master.set_attribute(master_attribute, 3.0, &world(false));
        master.add_attribute(master_attribute, 2.0, &world(false));
        master.multiply_attribute(master_attribute, 2.0, &world(false));
    }
    assert_eq!(master, before);
}

/// raises scales the percentage by the factor (which a product of 0 keeps at 0).
#[test]
fn combat_multiply_takes_percentage_of_the_class_amount() {
    let mut master = of_kind(MasterKind::RGE_COMBAT);
    master.combat.get_or_insert_with(Default::default).armors =
        vec![ClassAmount::new(4, 3), ClassAmount::new(3, 5)];
    master.combat.get_or_insert_with(Default::default).weapons =
        vec![ClassAmount::new(4, 7), ClassAmount::new(21, 0)];
    // class 3 at 150%: 5 x 1.5 + 0.5 = 8; class 4 at 50%: 7 x 0.5 + 0.5 = 4
    master.multiply_attribute(MasterAttribute::ARMOR, f32::from(0x396_i16), &world(false));
    master.multiply_attribute(MasterAttribute::ATTACK, f32::from(0x432_i16), &world(false));
    assert_eq!(
        (
            master.combat_part().armors.clone(),
            master.combat_part().weapons[0]
        ),
        (
            vec![ClassAmount::new(4, 3), ClassAmount::new(3, 8)],
            ClassAmount::new(4, 4)
        )
    );
    master.multiply_attribute(MasterAttribute::ARMOR, f32::from(0x996_i16), &world(false));
    assert_eq!(
        master.combat_part().armors,
        vec![ClassAmount::new(4, 3), ClassAmount::new(3, 8)]
    );
    let raised = AttributeWorld {
        handicap: Some(1.5),
        ..world(false)
    };
    master.multiply_attribute(MasterAttribute::ATTACK, f32::from(0x15c8_i16), &raised);
    assert_eq!(master.combat_part().weapons[1], ClassAmount::new(21, 0));
}

/// `FUN_1410c12b0`, `FUN_1410c16e0` and `FUN_1410c1bf0`: each RGE combat attribute's field
/// takes the set, the add and the multiply (the frame delay, the blast level and the shown
/// values by the truncated amount); the rotation speed comes from the moving master's set.
#[test]
fn each_combat_attribute_sets_adds_and_multiplies_its_field() {
    let rows: &[(MasterAttribute, Read, [f32; 3])] = &[
        (
            MasterAttribute::ROTATION_SPEED,
            |master| master.moving_part().turn_speed.get(),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::ATTACK_RELOAD_TIME,
            |master| master.combat_part().reload.get(),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::ACCURACY_PERCENT,
            |master| f32::from(master.combat_part().hit_chance.get()),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::MAXIMUM_RANGE,
            |master| master.combat_part().weapon_range.get(),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::BASE_ARMOR,
            |master| master.combat_part().base_armor.as_f32(),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::PROJECTILE_UNIT,
            |master| f32::from(master.combat_part().missile.get()),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::TERRAIN_DEFENSE_BONUS,
            |master| f32::from(master.combat_part().terrain_bonus.get()),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::MINIMUM_RANGE,
            |master| master.combat_part().min_range.get(),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::BLAST_WIDTH,
            |master| master.combat_part().area_effect.get(),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::BONUS_DAMAGE_RESISTANCE,
            |master| master.combat_part().resistance.get(),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::FRAME_DELAY,
            |master| master.combat_part().fire_frame.as_f32(),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::BLAST_ATTACK_LEVEL,
            |master| f32::from(master.combat_part().blast_level.bits()),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::SHOWN_ATTACK,
            |master| master.combat_part().shown.attack.as_f32(),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::SHOWN_RANGE,
            |master| master.combat_part().shown.range.get(),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::SHOWN_MELEE_ARMOR,
            |master| master.combat_part().shown.melee_armor.as_f32(),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::COMBAT_ABILITY,
            |master| f32::from(master.combat_part().combat_ability.bits()),
            [3.0, 5.0, 5.0],
        ),
        (
            MasterAttribute::ATTACK_DISPERSION,
            |master| master.combat_part().missed_spread.get(),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::ATTACK_GRAPHIC,
            |master| sprite_value(master.combat_part().attack_sprite),
            [3.0, 5.0, 5.0],
        ),
        (
            MasterAttribute::AREA_DAMAGE,
            |master| master.combat_part().area_damage.get(),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::DAMAGE_REFLECTION,
            |master| master.combat_part().reflection.get(),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::FRIENDLY_FIRE_DAMAGE,
            |master| master.combat_part().friendly_fire.get(),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::GARRISON_FIREPOWER,
            |master| master.combat_part().garrison_firepower.get(),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::DISPLACEMENT_X,
            |master| master.combat_part().weapon_offset.x(),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::DISPLACEMENT_Z,
            |master| master.combat_part().weapon_offset.z(),
            [3.0, 5.0, 10.0],
        ),
    ];
    sets_adds_multiplies(MasterKind::RGE_COMBAT, rows);
}

/// `FUN_1412661d0`, `FUN_141266a60` and `FUN_141267040`: each combat (TRIBE) attribute's
/// field takes the set, the add and the multiply its arm has; the selected train location
/// takes the train values.
#[test]
fn each_tribe_combat_attribute_sets_adds_and_multiplies_its_field() {
    let rows: &[(MasterAttribute, Read, [f32; 3])] = &[
        (
            MasterAttribute::HERO_STATUS,
            |master| f32::from(master.creatable_part().hero_flag.bits()),
            [3.0, 5.0, 5.0],
        ),
        (
            MasterAttribute::TRAIN_LOCATION,
            |master| {
                f32::from(MasterId::field(
                    master.creatable_part().train_locations[1].location,
                ))
            },
            [3.0; 3],
        ),
        (
            MasterAttribute::TRAIN_BUTTON,
            |master| f32::from(master.creatable_part().train_locations[1].button.get()),
            [3.0; 3],
        ),
        (
            MasterAttribute::SHOWN_PIERCE_ARMOR,
            |master| master.creatable_part().pierce_armor_shown.as_f32(),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::HOTKEY_ID,
            |master| master.creatable_part().train_locations[1].hotkey.get() as f32,
            [3.0; 3],
        ),
        (
            MasterAttribute::MAXIMUM_CHARGE,
            |master| master.creatable_part().max_charge.get(),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::RECHARGE_RATE,
            |master| master.creatable_part().recharge_rate.get(),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::CHARGE_EVENT,
            |master| f32::from(master.creatable_part().charge_event.get()),
            [3.0, 5.0, 5.0],
        ),
        (
            MasterAttribute::CHARGE_KIND,
            |master| f32::from(master.creatable_part().charge_kind.get()),
            [3.0; 3],
        ),
        (
            MasterAttribute::SECONDARY_PROJECTILE_UNIT,
            |master| f32::from(MasterId::field(master.creatable_part().volley_missile)),
            [3.0; 3],
        ),
        (
            MasterAttribute::SPECIAL_GRAPHIC,
            |master| sprite_value(master.creatable_part().special_sprite),
            [3.0, 5.0, 5.0],
        ),
        (
            MasterAttribute::SPECIAL_ABILITY,
            |master| f32::from(master.creatable_part().special_attack.get()),
            [3.0, 5.0, 5.0],
        ),
        (
            MasterAttribute::IDLE_ATTACK_GRAPHIC,
            |master| sprite_value(master.creatable_part().idle_attack_sprite),
            [3.0, 5.0, 5.0],
        ),
        (
            MasterAttribute::HERO_GLOW_GRAPHIC,
            |master| sprite_value(master.creatable_part().hero_glow_sprite),
            [3.0, 5.0, 5.0],
        ),
        (
            MasterAttribute::GARRISON_GRAPHIC,
            |master| sprite_value(master.creatable_part().garrison_sprite),
            [3.0, 5.0, 5.0],
        ),
        (
            MasterAttribute::FOOD_COSTS,
            |master| master.creatable_part().cost[0].amount.as_f32(),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::TRAIN_TIME,
            |master| f32::from(master.creatable_part().train_locations[1].time.get()),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::TOTAL_MISSILES,
            |master| master.creatable_part().volley_fire.get(),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::MAXIMUM_TOTAL_MISSILES,
            |master| f32::from(master.creatable_part().max_volley.get()),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::REGENERATION_RATE,
            |master| master.creatable_part().regeneration.get(),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::MIN_CONVERSION_TIME_MODIFIER,
            |master| master.creatable_part().conversion.min_time,
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::CONVERSION_CHANCE_MODIFIER,
            |master| master.creatable_part().conversion.chance,
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::FORMATION_CATEGORY,
            |master| f32::from(master.creatable_part().formation_category.get()),
            [3.0; 3],
        ),
        // 10 hit points times the amount, set then added
        (
            MasterAttribute::REGENERATION_HP_PERCENT,
            |master| master.creatable_part().regeneration.get(),
            [30.0, 50.0, 50.0],
        ),
        (
            MasterAttribute::CHARGE_PROJECTILE_UNIT,
            |master| f32::from(MasterId::field(master.creatable_part().charge_projectile)),
            [3.0; 3],
        ),
        (
            MasterAttribute::ATTACK_PRIORITY,
            |master| f32::from(master.creatable_part().attack_priority.get()),
            [3.0; 3],
        ),
        (
            MasterAttribute::INVULNERABILITY_LEVEL,
            |master| master.creatable_part().invulnerability.get(),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::ATTACK_GRAPHIC_2,
            |master| sprite_value(master.combat_part().attack_sprite2),
            [3.0, 5.0, 5.0],
        ),
        (
            MasterAttribute::SPAWNING_AREA_WIDTH,
            |master| master.creatable_part().volley_spread.x.get(),
            [3.0, 5.0, 10.0],
        ),
        (
            MasterAttribute::SPAWNING_AREA_RANDOMNESS,
            |master| master.creatable_part().volley_spread.start.get(),
            [3.0, 5.0, 10.0],
        ),
        // the selected entry plus ten times the count, as the damage sprites'
        (
            MasterAttribute::TRAIN_LOCATIONS_ENTRY_MOD,
            |master| {
                f32::from(master.creatable_part().train_location_entry.get())
                    + 10.0 * master.creatable_part().train_locations.len() as f32
            },
            [21.0, 21.0, 32.0],
        ),
        (
            MasterAttribute::TRAIN_LOCATIONS_TOTAL_NUM,
            |master| master.creatable_part().train_locations.len() as f32,
            [3.0, 3.0, 6.0],
        ),
    ];
    sets_adds_multiplies(MasterKind::COMBAT, rows);
}

/// `FUN_141126400`, `FUN_141126af0` and `FUN_1411271e0` on the hit points: a set truncates, an
/// add adds the truncated amount (wrapping), a multiply rounds by 0.5; capped (the player's
/// `+0x648`) both stop at 0x7fff, a NaN too (`COMISS` then `JBE`).
#[test]
fn the_hit_points_add_and_multiply_up_to_their_cap_when_capped() {
    let mut master = of_kind(MasterKind::STATIC);
    master.set_attribute(MasterAttribute::HIT_POINTS, 25.9, &world(false));
    assert_eq!(master.hit_points, MasterHitPoints::of_data(25));
    master.add_attribute(MasterAttribute::HIT_POINTS, 15.7, &world(false));
    assert_eq!(master.hit_points, MasterHitPoints::of_data(40));
    master.multiply_attribute(MasterAttribute::HIT_POINTS, 1.2, &world(false));
    // 40 × 1.2 + 0.5 = 48.5
    assert_eq!(master.hit_points, MasterHitPoints::of_data(48));
    master.hit_points = MasterHitPoints::of_data(32760);
    master.add_attribute(MasterAttribute::HIT_POINTS, 10.0, &world(false));
    assert_eq!(master.hit_points, MasterHitPoints::of_data(-32766));
    master.hit_points = MasterHitPoints::of_data(32760);
    master.add_attribute(MasterAttribute::HIT_POINTS, 10.0, &world(true));
    assert_eq!(master.hit_points, MasterHitPoints::of_data(0x7fff));
    master.hit_points = MasterHitPoints::of_data(100);
    master.add_attribute(MasterAttribute::HIT_POINTS, f32::NAN, &world(true));
    assert_eq!(master.hit_points, MasterHitPoints::of_data(0x7fff));
    master.hit_points = MasterHitPoints::of_data(20000);
    master.multiply_attribute(MasterAttribute::HIT_POINTS, 2.0, &world(true));
    assert_eq!(master.hit_points, MasterHitPoints::of_data(0x7fff));
    master.capacity = CarryCapacity::of_data(10);
    master.multiply_attribute(MasterAttribute::CARRY_CAPACITY, 1.25, &world(true));
    // 12.5 + 0.5 = 13
    assert_eq!(master.capacity, CarryCapacity::of_data(13));
}

/// The storage amounts take a short (`(float)(int)(short)(int)v`), the object flags a 64-bit
/// truncation, the trait piece its high half; an obstruction type the table refuses is 0.
#[test]
fn the_static_setters_truncate_as_the_game_does() {
    let mut master = of_kind(MasterKind::TREE);
    master.storage = vec![
        (Some(player_attribute::MEAT), Amount::new(100.0), 1),
        (None, Amount::ZERO, 0),
        (None, Amount::ZERO, 0),
    ];
    master.set_attribute(MasterAttribute::STORAGE_1, 70000.5, &world(false));
    assert_eq!(master.storage[0].1, Amount::new(4464.0));
    master.add_attribute(MasterAttribute::STORAGE_1, 5.9, &world(false));
    assert_eq!(master.storage[0].1, Amount::new(4469.0));
    master.multiply_attribute(MasterAttribute::STORAGE_1, 0.5, &world(false));
    // 2234.5 + 0.5 = 2235
    assert_eq!(master.storage[0].1, Amount::new(2235.0));
    master.set_attribute(MasterAttribute::UNIT_TRAIT, 5_000_000_000.0, &world(false));
    assert_eq!(
        master.object_flags,
        ObjectFlags::from_bits_retain(5_000_000_000_i64 as u32)
    );
    master.object_flags = ObjectFlags::from_bits_retain(0x1234_5678);
    master.set_attribute(MasterAttribute::TRAIT_PIECE, 3.0, &world(false));
    assert_eq!(
        master.object_flags,
        ObjectFlags::from_bits_retain(0x0003_5678)
    );
    master.set_attribute(MasterAttribute::OBSTRUCTION_KIND, 5.0, &world(false));
    assert_eq!(master.obstruction_kind, ObstructionKind::CIRCLE);
    master.set_attribute(MasterAttribute::OBSTRUCTION_KIND, 4.0, &world(false));
    assert_eq!(master.obstruction_kind, ObstructionKind::NONE);
    // "Can Be Built On" writes the blast defense level; its multiply does not round
    master.set_attribute(MasterAttribute::CAN_BE_BUILT_ON, 3.0, &world(false));
    master.multiply_attribute(MasterAttribute::BLAST_DEFENSE_LEVEL, 1.9, &world(false));
    assert_eq!(master.blast_defense, BlastDefense::new(5));
    // the available flag holds the byte's test against 0
    master.set_attribute(MasterAttribute::AVAILABLE_UNIT_FLAG, 256.0, &world(false));
    assert!(!master.available.is_available());
    master.add_attribute(MasterAttribute::AVAILABLE_UNIT_FLAG, 1.0, &world(false));
    assert!(master.available.is_available());
}

/// `FUN_14152e360`: a sprite attribute takes an id only where the world's table holds a
/// sprite; an add starts from −1 for none.
#[test]
fn sprite_attribute_takes_only_sprite_the_world_has() {
    let mut master = of_kind(MasterKind::MOVING);
    master.standing_sprite = Some(SpriteId::named(5));
    master
        .moving
        .get_or_insert_with(Default::default)
        .move_sprite = None;
    master.set_attribute(MasterAttribute::STANDING_GRAPHIC, 150.0, &world(false));
    assert_eq!(master.standing_sprite, Some(SpriteId::named(5)));
    master.add_attribute(MasterAttribute::STANDING_GRAPHIC, 3.0, &world(false));
    assert_eq!(master.standing_sprite, Some(SpriteId::named(8)));
    master.add_attribute(MasterAttribute::WALKING_GRAPHIC, 11.0, &world(false));
    assert_eq!(master.moving_part().move_sprite, Some(SpriteId::named(10)));
    master.set_attribute(MasterAttribute::STANDING_GRAPHIC_2, 7.0, &world(false));
    assert_eq!(master.standing_sprite_2, Some(SpriteId::named(7)));
}

/// `FUN_1410a8b60`, `FUN_1410a8c00`: a capped add or multiply of the speed stops at 4.5, a
/// missile master's at 20; a NaN speed passes (`COMISS`, `JBE`).
#[test]
fn capped_speed_stops_at_its_kind_cap() {
    let mut master = of_kind(MasterKind::COMBAT);
    master
        .animated
        .get_or_insert_with(Default::default)
        .max_speed = Speed::new(4.0);
    master.add_attribute(MasterAttribute::MOVEMENT_SPEED, 1.0, &world(false));
    assert_eq!(master.animated_part().max_speed, Speed::new(5.0));
    master.multiply_attribute(MasterAttribute::MOVEMENT_SPEED, 1.1, &world(true));
    assert_eq!(master.animated_part().max_speed, Speed::new(4.5));
    let mut missile = of_kind(MasterKind::MISSILE);
    missile
        .animated
        .get_or_insert_with(Default::default)
        .max_speed = Speed::new(7.0);
    missile.multiply_attribute(MasterAttribute::MOVEMENT_SPEED, 4.0, &world(true));
    assert_eq!(missile.animated_part().max_speed, Speed::new(20.0));
    missile.add_attribute(MasterAttribute::MOVEMENT_SPEED, f32::NAN, &world(true));
    assert!(missile.animated_part().max_speed.get().is_nan());
}

/// `FUN_1410c12b0` and its add and multiply: the amount's high byte names the class, its low
/// byte the amount (negated for a negative amount); the first entry of the class changes, a
/// multiply by `amount / 100`.
#[test]
fn armor_and_attack_change_the_class_in_the_amount_high_byte() {
    let mut master = of_kind(MasterKind::COMBAT);
    master.combat.get_or_insert_with(Default::default).armors = vec![
        ClassAmount::new(4, 1),
        ClassAmount::new(3, 2),
        ClassAmount::new(3, 9),
    ];
    master.combat.get_or_insert_with(Default::default).weapons =
        vec![ClassAmount::new(4, 6), ClassAmount::new(3, 0)];
    // class 3, 1 more (0x301)
    master.add_attribute(MasterAttribute::ARMOR, 769.0, &world(false));
    assert_eq!(
        master.combat_part().armors,
        vec![
            ClassAmount::new(4, 1),
            ClassAmount::new(3, 3),
            ClassAmount::new(3, 9)
        ]
    );
    // −0x402: class 4, 2 less
    master.add_attribute(MasterAttribute::ARMOR, -1026.0, &world(false));
    assert_eq!(master.combat_part().armors[0], ClassAmount::new(4, -1));
    master.set_attribute(MasterAttribute::ARMOR, 1029.0, &world(false));
    assert_eq!(master.combat_part().armors[0], ClassAmount::new(4, 5));
    // 150 %: 6 × 1.5 + 0.5 = 9.5
    master.multiply_attribute(MasterAttribute::ATTACK, f32::from(0x496_i16), &world(false));
    assert_eq!(master.combat_part().weapons[0], ClassAmount::new(4, 9));
    // a NaN takes the negative branch: class 0, amount 0, which no entry has
    master.add_attribute(MasterAttribute::ATTACK, f32::NAN, &world(false));
    assert_eq!(
        master.combat_part().weapons,
        vec![ClassAmount::new(4, 9), ClassAmount::new(3, 0)]
    );
    // a handicap of 1.5 raises a class at 0 (not 3, 4, 31 or 33)
    let raised = AttributeWorld {
        handicap: Some(1.5),
        ..world(false)
    };
    master.combat.get_or_insert_with(Default::default).weapons =
        vec![ClassAmount::new(4, 0), ClassAmount::new(21, 0)];
    master.set_attribute(MasterAttribute::ATTACK, f32::from(0x406_i16), &raised);
    master.set_attribute(MasterAttribute::ATTACK, f32::from(0x1506_i16), &raised);
    assert_eq!(
        master.combat_part().weapons,
        vec![ClassAmount::new(4, 6), ClassAmount::new(21, 9)]
    );
    // once above 0 the add adds
    master.add_attribute(MasterAttribute::ATTACK, f32::from(0x1502_i16), &raised);
    assert_eq!(master.combat_part().weapons[1], ClassAmount::new(21, 11));
}

/// The RGE combat master's multiply rounds most shorts by 0.5 but multiplies the frame delay
/// and the shown values by the truncated amount (`IMUL`).
#[test]
fn some_combat_shorts_multiply_by_the_truncated_amount() {
    let mut master = of_kind(MasterKind::RGE_COMBAT);
    master
        .combat
        .get_or_insert_with(Default::default)
        .base_armor = DamageAmount::new(3);
    master
        .combat
        .get_or_insert_with(Default::default)
        .fire_frame = Frame::of_data(5);
    master
        .combat
        .get_or_insert_with(Default::default)
        .shown
        .attack = DamageAmount::new(7);
    master.multiply_attribute(MasterAttribute::BASE_ARMOR, 1.5, &world(false));
    assert_eq!(master.combat_part().base_armor, DamageAmount::new(5));
    master.multiply_attribute(MasterAttribute::FRAME_DELAY, 2.9, &world(false));
    assert_eq!(master.combat_part().fire_frame, Frame::of_data(10));
    master.multiply_attribute(MasterAttribute::SHOWN_ATTACK, 3.5, &world(false));
    assert_eq!(master.combat_part().shown.attack, DamageAmount::new(21));
    master.add_attribute(MasterAttribute::ROTATION_SPEED, 0.25, &world(false));
    assert_eq!(master.moving_part().turn_speed, Seconds::new(0.25));
    master.set_attribute(MasterAttribute::DISPLACEMENT_Y, 0.75, &world(false));
    assert_eq!(
        master.combat_part().weapon_offset,
        Offset::new(0.0, 0.75, 0.0)
    );
}

/// `FUN_1410d5b50`: a missile master's smart mode takes the multiply's amount; its arc
/// multiplies; then the RGE combat master's setter runs.
#[test]
fn missile_smart_mode_multiply_sets_it() {
    let mut master = of_kind(MasterKind::MISSILE);
    master.missile = Some(MissilePart {
        projectile_kind: ProjectileKind::STRAIGHT,
        smart_mode: SmartMode::LEADS_TARGET,
        hit_mode: HitMode::TARGET_ALONE,
        vanish_mode: VanishMode::NONE,
        arc: ArcRatio::new(0.5),
    });
    master.combat.get_or_insert_with(Default::default).reload = Seconds::new(2.0);
    master.multiply_attribute(MasterAttribute::PROJECTILE_SMART_MODE, 3.0, &world(false));
    master.multiply_attribute(MasterAttribute::PROJECTILE_ARC, 0.5, &world(false));
    master.multiply_attribute(MasterAttribute::ATTACK_RELOAD_TIME, 0.5, &world(false));
    let missile_part = master.missile.unwrap();
    assert_eq!(
        (
            missile_part.smart_mode,
            missile_part.arc,
            master.combat_part().reload
        ),
        (
            SmartMode::from_bits_retain(3),
            ArcRatio::new(0.25),
            Seconds::new(1.0)
        )
    );
}

/// `FUN_1412661d0` and its add and multiply on the costs: 100 changes the paid entries, 103
/// to 106 those of food, wood, gold (3) and stone (2); population (110) sets the unpaid ones
/// to the negated amount and the population storage to the amount.
#[test]
fn the_cost_attributes_change_their_resource_entries() {
    let mut master = of_kind(MasterKind::BUILDING);
    master.creatable.get_or_insert_with(Default::default).cost = costs(&[
        (player_attribute::WOOD, 25, true),
        (player_attribute::HEADROOM, 1, false),
    ]);
    master.storage = vec![
        (Some(player_attribute::HEADROOM), Amount::new(5.0), 4),
        (Some(player_attribute::POPULATION), Amount::new(-1.0), 4),
        (None, Amount::ZERO, 0),
    ];
    master.add_attribute(MasterAttribute::WOOD_COSTS, -5.0, &world(false));
    assert_eq!(
        master.creatable_part().cost,
        costs(&[
            (player_attribute::WOOD, 20, true),
            (player_attribute::HEADROOM, 1, false)
        ])
    );
    master.multiply_attribute(MasterAttribute::RESOURCE_COSTS, 0.85, &world(false));
    // 20 × 0.85 + 0.5 = 17.5
    assert_eq!(
        master.creatable_part().cost,
        costs(&[
            (player_attribute::WOOD, 17, true),
            (player_attribute::HEADROOM, 1, false)
        ])
    );
    master.set_attribute(MasterAttribute::GOLD_COSTS, 50.0, &world(false));
    assert_eq!(
        master.creatable_part().cost[0].entry(),
        Some((player_attribute::WOOD, CostAmount::of_data(17), true))
    );
    master.set_attribute(MasterAttribute::POPULATION, 2.0, &world(false));
    assert_eq!(
        master.creatable_part().cost,
        costs(&[
            (player_attribute::WOOD, 17, true),
            (player_attribute::HEADROOM, -2, false)
        ])
    );
    assert_eq!(
        master.storage[..2],
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
        button: LocationButton::of_data(1),
        hotkey: HotkeyId::of_data(16_000),
    };
    let mut master = of_kind(MasterKind::COMBAT);
    master
        .creatable
        .get_or_insert_with(Default::default)
        .train_locations = vec![
        barracks(WholeSeconds::of_data(21)),
        barracks(WholeSeconds::of_data(30)),
    ];
    master
        .creatable
        .get_or_insert_with(Default::default)
        .train_location_entry = TrainLocationEntry::of_data(1);
    master.multiply_attribute(MasterAttribute::TRAIN_TIME, 0.9, &world(false));
    // 30 × 0.9 + 0.5 = 27.5
    assert_eq!(
        master.creatable_part().train_locations[1].time,
        WholeSeconds::of_data(27)
    );
    master.set_attribute(
        MasterAttribute::TRAIN_LOCATIONS_ENTRY_MOD,
        9.0,
        &world(false),
    );
    assert_eq!(
        master.creatable_part().train_location_entry,
        TrainLocationEntry::of_data(1)
    );
    // the add's count is the selected entry plus the amount: 1 + 2
    master.add_attribute(
        MasterAttribute::TRAIN_LOCATIONS_TOTAL_NUM,
        2.0,
        &world(false),
    );
    assert_eq!(master.creatable_part().train_locations.len(), 3);
    assert_eq!(
        master.creatable_part().train_locations[2],
        master.creatable_part().train_locations[1]
    );
    master.set_attribute(
        MasterAttribute::TRAIN_LOCATIONS_TOTAL_NUM,
        -4.0,
        &world(false),
    );
    assert_eq!(master.creatable_part().train_locations, []);
    assert_eq!(
        master.creatable_part().train_location_entry,
        TrainLocationEntry::of_data(-1)
    );
    master
        .creatable
        .get_or_insert_with(Default::default)
        .train_locations = vec![
        barracks(WholeSeconds::of_data(21)),
        barracks(WholeSeconds::of_data(30)),
    ];
    master
        .creatable
        .get_or_insert_with(Default::default)
        .train_location_entry = TrainLocationEntry::of_data(0);
    // ~(−1) = 0: the first goes, the entry stays at 0
    master.multiply_attribute(
        MasterAttribute::TRAIN_LOCATIONS_ENTRY_MOD,
        -1.0,
        &world(false),
    );
    assert_eq!(
        (
            master.creatable_part().train_locations.len(),
            master.creatable_part().train_locations[0].time
        ),
        (1, WholeSeconds::of_data(30))
    );
    master.multiply_attribute(
        MasterAttribute::TRAIN_LOCATIONS_ENTRY_MOD,
        5.0,
        &world(false),
    );
    assert_eq!(
        (
            master.creatable_part().train_locations.len(),
            master.creatable_part().train_location_entry.get()
        ),
        (2, 1)
    );
    follow(
        &mut master,
        MasterAttribute::TRAIN_TIME,
        &Graphics::new(),
        None,
    );
    assert_eq!(
        (
            master.creatable_part().trained_at.clone(),
            master.creatable_part().build_time
        ),
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
    let mut master = of_kind(MasterKind::ANIMATED);
    master.damage_graphic_entry = DamageGraphicEntry::of_data(2);
    master.set_attribute(
        MasterAttribute::DAMAGE_GRAPHICS_TOTAL_NUM,
        2.0,
        &world(false),
    );
    assert_eq!(master.damage_graphics, vec![DamageGraphic::default(); 2]);
    assert_eq!(master.damage_graphic_entry, DamageGraphicEntry::of_data(1));
    master.set_attribute(MasterAttribute::DAMAGE_GRAPHIC, 40.0, &world(false));
    master.set_attribute(MasterAttribute::DAMAGE_GRAPHIC_PERCENT, 75.0, &world(false));
    assert_eq!(master.damage_graphics[1].sprite, Some(SpriteId::named(40)));
    assert_eq!(master.damage_graphics[1].percent, HpPercent::of_data(75));
    master.set_attribute(
        MasterAttribute::DAMAGE_GRAPHICS_TOTAL_NUM,
        f32::NAN,
        &world(false),
    );
    assert_eq!(master.damage_graphics.len(), 255);
    master.set_attribute(
        MasterAttribute::DAMAGE_GRAPHICS_TOTAL_NUM,
        0.0,
        &world(false),
    );
    assert_eq!(master.damage_graphic_entry, DamageGraphicEntry::of_data(1));
    master.damage_graphics = vec![DamageGraphic::default()];
    master.multiply_attribute(
        MasterAttribute::DAMAGE_GRAPHICS_ENTRY_MOD,
        -3.0,
        &world(false),
    );
    assert_eq!(master.damage_graphics, []);
    assert_eq!(
        master.damage_graphic_entry,
        DamageGraphicEntry::of_data(0xff)
    );
    follow(
        &mut master,
        MasterAttribute::DAMAGE_GRAPHICS_ENTRY_MOD,
        &Graphics::new(),
        None,
    );
    assert!(!master.has_damage_sprites());
}

/// `FUN_14123d310`: a building master's garrison heal rate, foundation and garrison type are
/// its own; its six sprites go to a copy (nothing changes); the rest falls to the combat
/// master's setter (the regeneration from the hit points, 120).
#[test]
fn building_master_setters_fall_back_on_the_combat_master_s() {
    let mut master = of_kind(MasterKind::BUILDING);
    master.building = Some(BuildingMaster {
        foundation_terrain: Some(TerrainId::named(0)),
        ..BuildingMaster::default()
    });
    master.hit_points = MasterHitPoints::of_data(600);
    master.set_attribute(MasterAttribute::GARRISON_HEAL_RATE, 2.0, &world(false));
    master.multiply_attribute(MasterAttribute::GARRISON_HEAL_RATE, 1.5, &world(false));
    master.set_attribute(MasterAttribute::CONSTRUCTION_GRAPHIC, 9.0, &world(false));
    master.set_attribute(MasterAttribute::REGENERATION_HP_PERCENT, 0.5, &world(false));
    master.add_attribute(MasterAttribute::FOUNDATION_TERRAIN, 2.0, &world(false));
    let building_master = master.building.unwrap();
    assert_eq!(
        (
            building_master.garrison_heal_rate,
            building_master.foundation_terrain
        ),
        (HitPoints::new(3.0), Some(TerrainId::named(2)))
    );
    assert_eq!(
        master.creatable_part().regeneration,
        Regeneration::new(300.0)
    );
    assert_eq!(master, {
        let mut same = of_kind(MasterKind::BUILDING);
        same.building = Some(building_master);
        same.hit_points = MasterHitPoints::of_data(600);
        same.creatable
            .get_or_insert_with(Default::default)
            .regeneration = Regeneration::new(300.0);
        same
    });
}

/// The creatable layer's setters are the TRIBE combat master's (slots 0x18, 0x20 and 0x28 of
/// its class): a master of a type without the layer (30, a moving one) has no such values, so
/// an effect on them leaves it as it was.
#[test]
fn an_effect_on_the_creatable_layer_leaves_masters_without_it_unchanged() {
    let moving = of_kind(MasterKind::MOVING);
    for attribute in [
        MasterAttribute::MAXIMUM_CHARGE,
        MasterAttribute::RECHARGE_RATE,
        MasterAttribute::SHOWN_PIERCE_ARMOR,
        MasterAttribute::TRAIN_TIME,
        MasterAttribute::HERO_STATUS,
    ] {
        let mut master = moving.clone();
        master.set_attribute(attribute, 5.0, &world(false));
        master.add_attribute(attribute, 5.0, &world(false));
        master.multiply_attribute(attribute, 2.0, &world(false));
        assert_eq!(master, moving, "{attribute:?}");
    }
}

/// The foundation terrain's add works on the game's short: none is −1, so an add of 2 lays
/// terrain 1, and a set below 0 is none again.
#[test]
fn foundation_terrain_add_counts_none_as_minus_one() {
    let mut master = of_kind(MasterKind::BUILDING);
    master.building = Some(BuildingMaster::default());
    master.add_attribute(MasterAttribute::FOUNDATION_TERRAIN, 2.0, &world(false));
    assert_eq!(
        master
            .building
            .as_ref()
            .and_then(|building_master| building_master.foundation_terrain),
        Some(TerrainId::named(1))
    );
    master.set_attribute(MasterAttribute::FOUNDATION_TERRAIN, -1.0, &world(false));
    assert_eq!(
        master
            .building
            .as_ref()
            .and_then(|building_master| building_master.foundation_terrain),
        None
    );
}

/// What a unit moving with a master reads follows the master's values: its radii, its base
/// speed and its speeds by its sprites' factors, its turn time and its facets.
#[test]
fn moving_master_derived_values_follow_its_attributes() {
    let graphic = |id, speed_multiplier, facets| Graphic {
        id,
        frames: Frame::of_data(10),
        facets,
        speed_multiplier,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::ANIMATED,
    };
    let graphics = [
        (
            SpriteId::named(1),
            graphic(
                SpriteId::named(1),
                SpeedMultiplier::ZERO,
                FacetCount::of_data(8),
            ),
        ),
        (
            SpriteId::named(2),
            graphic(
                SpriteId::named(2),
                SpeedMultiplier::of_data(1.5),
                FacetCount::of_data(8),
            ),
        ),
        (
            SpriteId::named(3),
            graphic(
                SpriteId::named(3),
                SpeedMultiplier::of_data(0.5),
                FacetCount::of_data(16),
            ),
        ),
    ]
    .into_iter()
    .collect();
    let mut master = of_kind(MasterKind::COMBAT);
    master.standing_sprite = Some(SpriteId::named(1));
    let moving = master.moving.get_or_insert_with(Default::default);
    (moving.move_sprite, moving.run_sprite) = (Some(SpriteId::named(2)), None);
    master
        .animated
        .get_or_insert_with(Default::default)
        .max_speed = Speed::new(1.0);
    master.radius = Footprint::new(0.2, 0.2);
    master.move_stats = Some(MoveStats {
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
    master.multiply_attribute(MasterAttribute::MOVEMENT_SPEED, 1.1, &world(false));
    follow(
        &mut master,
        MasterAttribute::MOVEMENT_SPEED,
        &graphics,
        None,
    );
    let move_stats = master.move_stats.unwrap();
    assert_eq!(
        (move_stats.base, move_stats.speed),
        (Speed::new(1.1), Speed::new(1.1 * 1.5))
    );
    master.set_attribute(MasterAttribute::SIZE_Z, 1.0, &world(false));
    follow(&mut master, MasterAttribute::SIZE_Z, &graphics, None);
    assert!(master.is_tall());
    assert_eq!(master.move_stats.unwrap().radius, Radii::new(0.2, 0.2, 1.0));
    master.set_attribute(MasterAttribute::RUNNING_GRAPHIC, 3.0, &world(false));
    follow(
        &mut master,
        MasterAttribute::RUNNING_GRAPHIC,
        &graphics,
        None,
    );
    let move_stats = master.move_stats.unwrap();
    assert_eq!(move_stats.speed, Speed::new(1.1 * 0.5));
    // 8 facets and 16: no common count
    assert_eq!(move_stats.facets, None);
    master.set_attribute(MasterAttribute::RUNNING_GRAPHIC, 2.0, &world(false));
    follow(
        &mut master,
        MasterAttribute::RUNNING_GRAPHIC,
        &graphics,
        None,
    );
    assert_eq!(
        master.move_stats.unwrap().facets,
        Some(SpriteFacets {
            count: FacetCount::of_data(8),
            turns: false
        })
    );
}

/// The missile, the trail's unit and the terrain bonus keep the short the effects leave: set
/// to −3 and 5 added is 2, as in the game, where a field read as none from −1 would give 4; a
/// multiply's negative result is kept as well.
#[test]
fn an_id_field_keeps_negative_value_through_an_add() {
    let attribute_world = world(false);
    let mut master = of_kind(MasterKind::RGE_COMBAT);
    for master_attribute in [
        MasterAttribute::PROJECTILE_UNIT,
        MasterAttribute::TERRAIN_DEFENSE_BONUS,
    ] {
        master.set_attribute(master_attribute, -3.0, &attribute_world);
    }
    assert_eq!(
        (
            master.combat_part().missile.master(),
            master.combat_part().terrain_bonus.restriction()
        ),
        (None, None)
    );
    for master_attribute in [
        MasterAttribute::PROJECTILE_UNIT,
        MasterAttribute::TERRAIN_DEFENSE_BONUS,
    ] {
        master.add_attribute(master_attribute, 5.0, &attribute_world);
    }
    assert_eq!(master.combat_part().missile.master(), Some(master_id(2)));
    assert_eq!(
        master.combat_part().terrain_bonus.restriction(),
        RestrictionId::of_data(2)
    );
    // 2 × −2 + 0.5 truncates to −3, kept: plus 4 is 1
    master.multiply_attribute(MasterAttribute::PROJECTILE_UNIT, -2.0, &attribute_world);
    master.add_attribute(MasterAttribute::PROJECTILE_UNIT, 4.0, &attribute_world);
    assert_eq!(master.combat_part().missile.get(), 1);
    let mut master = of_kind(MasterKind::MOVING);
    master.set_attribute(MasterAttribute::TRAILING_UNIT, -3.0, &attribute_world);
    master.add_attribute(MasterAttribute::TRAILING_UNIT, 5.0, &attribute_world);
    assert_eq!(master.moving_part().trail.unit.master(), Some(master_id(2)));
}

/// The action master's (`FUN_14109fbb0` and its add and multiply) work rate, search radius
/// and run pattern; the moving master's (`FUN_1410e1c00` ...) turn speed and trail; the
/// interface's sounds change nothing.
#[test]
fn the_action_and_moving_setters_change_their_values() {
    let mut master = of_kind(MasterKind::ACTION);
    master.set_attribute(MasterAttribute::WORK_RATE, 0.5, &world(false));
    master.add_attribute(MasterAttribute::WORK_RATE, 0.25, &world(false));
    master.multiply_attribute(MasterAttribute::WORK_RATE, 2.0, &world(false));
    master.set_attribute(MasterAttribute::SEARCH_RADIUS, 4.0, &world(false));
    master.multiply_attribute(MasterAttribute::SEARCH_RADIUS, 1.5, &world(false));
    master.set_attribute(MasterAttribute::RUN_PATTERN, 2.0, &world(false));
    master.add_attribute(MasterAttribute::RUN_PATTERN, 255.0, &world(false));
    master.set_attribute(MasterAttribute::ROTATION_SPEED, 0.5, &world(false));
    master.multiply_attribute(MasterAttribute::ROTATION_SPEED, 2.0, &world(false));
    master.set_attribute(MasterAttribute::TRAILING_UNIT, 1300.0, &world(false));
    master.add_attribute(MasterAttribute::TRAILING_UNIT, 2.0, &world(false));
    master.set_attribute(MasterAttribute::TRAIL_MODE, 3.0, &world(false));
    master.set_attribute(MasterAttribute::TRAIL_DENSITY, 1.5, &world(false));
    master.multiply_attribute(MasterAttribute::TRAIL_DENSITY, 2.0, &world(false));
    let before = master.clone();
    master.set_attribute(MasterAttribute::COMMAND_SOUND, 9.0, &world(false));
    master.add_attribute(MasterAttribute::COMMAND_SOUND, 9.0, &world(false));
    assert_eq!(master, before);
    assert_eq!(
        (
            master.action_part().work_rate,
            master.action_part().search_radius,
            master.action_part().run_pattern
        ),
        (
            WorkRate::new(1.5),
            Distance::new(6.0),
            RunPattern::of_data(1)
        )
    );
    assert_eq!(master.moving_part().turn_speed, Seconds::new(1.0));
    assert_eq!(
        master.moving_part().trail,
        Trail {
            unit: MasterField::of(Some(master_id(1302))),
            mode: TrailMode::of_data(3),
            density: TrailDensity::new(3.0)
        }
    );
}

/// The combat master's own values (`FUN_1412661d0` and its add and multiply): hero status,
/// charge, conversion, regeneration (by the hit points' share for 120), volleys, the second
/// attack sprite (an add from its id), the spawning area; the ability's interface values
/// change nothing.
#[test]
fn the_combat_master_own_values_change() {
    let mut master = of_kind(MasterKind::COMBAT);
    master.hit_points = MasterHitPoints::of_data(100);
    master
        .combat
        .get_or_insert_with(Default::default)
        .attack_sprite2 = Some(SpriteId::named(10));
    master.set_attribute(MasterAttribute::HERO_STATUS, 1.0, &world(false));
    master.add_attribute(MasterAttribute::HERO_STATUS, 4.0, &world(false));
    master.set_attribute(MasterAttribute::MAXIMUM_CHARGE, 10.0, &world(false));
    master.multiply_attribute(MasterAttribute::MAXIMUM_CHARGE, 1.5, &world(false));
    master.set_attribute(MasterAttribute::RECHARGE_RATE, 0.5, &world(false));
    master.set_attribute(MasterAttribute::CHARGE_EVENT, 2.0, &world(false));
    master.set_attribute(MasterAttribute::CHARGE_KIND, 6.0, &world(false));
    master.set_attribute(
        MasterAttribute::MAX_CONVERSION_TIME_MODIFIER,
        3.0,
        &world(false),
    );
    master.add_attribute(
        MasterAttribute::MAX_CONVERSION_TIME_MODIFIER,
        1.0,
        &world(false),
    );
    master.set_attribute(
        MasterAttribute::REGENERATION_HP_PERCENT,
        0.25,
        &world(false),
    );
    master.add_attribute(MasterAttribute::REGENERATION_RATE, 5.0, &world(false));
    master.set_attribute(MasterAttribute::TOTAL_MISSILES, 2.0, &world(false));
    master.multiply_attribute(MasterAttribute::MAXIMUM_TOTAL_MISSILES, 3.0, &world(false));
    master.add_attribute(MasterAttribute::ATTACK_GRAPHIC_2, 2.0, &world(false));
    master.set_attribute(
        MasterAttribute::SPAWNING_AREA_RANDOMNESS,
        0.5,
        &world(false),
    );
    master.set_attribute(
        MasterAttribute::SECONDARY_PROJECTILE_UNIT,
        -1.0,
        &world(false),
    );
    master.set_attribute(
        MasterAttribute::CHARGE_PROJECTILE_UNIT,
        1500.0,
        &world(false),
    );
    let before = master.clone();
    master.set_attribute(MasterAttribute::ABILITY_BUTTON_ICON_ID, 9.0, &world(false));
    assert_eq!(master, before);
    assert_eq!(
        (
            master.creatable_part().hero_flag,
            master.creatable_part().max_charge,
            master.creatable_part().recharge_rate
        ),
        (
            HeroFlags::FULL_HERO | HeroFlags::REGENERATES,
            Charge::new(15.0),
            ChargeRate::new(0.5)
        )
    );
    assert_eq!(
        (
            master.creatable_part().charge_event.get(),
            master.creatable_part().charge_kind.get()
        ),
        (2, 6)
    );
    assert_eq!(
        master.creatable_part().conversion,
        ConversionAdjust {
            min_time: 0.0,
            max_time: 4.0,
            chance: 0.0
        }
    );
    assert_eq!(
        master.creatable_part().regeneration,
        Regeneration::new(30.0)
    );
    assert_eq!(
        (
            master.creatable_part().volley_fire,
            master.creatable_part().max_volley,
            master.combat_part().attack_sprite2
        ),
        (
            Firepower::new(2.0),
            MissileCount::of_data(0),
            Some(SpriteId::named(12))
        )
    );
    assert_eq!(
        master.creatable_part().volley_spread,
        VolleySpread {
            x: Distance::new(0.0),
            y: Distance::new(0.0),
            start: Distance::new(0.5)
        }
    );
    assert_eq!(
        (
            master.creatable_part().volley_missile,
            master.creatable_part().charge_projectile
        ),
        (None, Some(master_id(1500)))
    );
}

/// The missile master's add (`FUN_1410d5ab0`) adds to its own bytes and arc, then runs the
/// RGE combat master's; a building master's add (`FUN_14123d530`) its facet and garrison
/// type; an animated master's set (`FUN_1410a8b00`) its speed.
#[test]
fn the_missile_building_and_animated_setters() {
    let mut master = of_kind(MasterKind::MISSILE);
    master.missile = Some(MissilePart {
        projectile_kind: ProjectileKind::STRAIGHT,
        smart_mode: SmartMode::NONE,
        hit_mode: HitMode::of_data(1),
        vanish_mode: VanishMode::NONE,
        arc: ArcRatio::new(0.25),
    });
    master.add_attribute(MasterAttribute::PROJECTILE_SMART_MODE, 1.0, &world(false));
    master.add_attribute(MasterAttribute::PROJECTILE_HIT_MODE, 1.0, &world(false));
    master.add_attribute(MasterAttribute::PROJECTILE_ARC, 0.25, &world(false));
    master.set_attribute(MasterAttribute::MAXIMUM_RANGE, 5.0, &world(false));
    let missile_part = master.missile.unwrap();
    assert_eq!(
        (
            missile_part.smart_mode,
            missile_part.hit_mode,
            missile_part.arc
        ),
        (
            SmartMode::LEADS_TARGET,
            HitMode::of_data(2),
            ArcRatio::new(0.5)
        )
    );
    assert_eq!(master.combat_part().weapon_range, Distance::new(5.0));
    let mut building = of_kind(MasterKind::BUILDING);
    building.building = Some(BuildingMaster::default());
    building.add_attribute(MasterAttribute::BUILDING_ICON_OVERRIDE, 3.0, &world(false));
    building.add_attribute(MasterAttribute::GARRISON_KIND, 2.0, &world(false));
    assert_eq!(
        (
            building.building.unwrap().facet,
            building.building.unwrap().garrison_kind
        ),
        (3, GarrisonKinds::INFANTRY)
    );
    let mut master = of_kind(MasterKind::DOPPELGANGER);
    master.set_attribute(MasterAttribute::MOVEMENT_SPEED, 1.2, &world(false));
    assert_eq!(master.animated_part().max_speed, Speed::new(1.2));
}

/// The master copy (slot 8, `Master::copy_from`): every value but the id, class,
/// availability, disabling and base id (`+0x260`).
#[test]
fn master_copy_keeps_its_id_class_and_flags() {
    let mut militia = master(
        master_id(74),
        1,
        vec![task(ActionKind::COMBAT, -1, -1, 0.0)],
    );
    militia.class = class::CLASS_6;
    militia.available = Availability::AVAILABLE;
    militia.base_id = Some(master_id(74));
    let mut man = master(
        master_id(75),
        2,
        vec![
            task(ActionKind::COMBAT, -1, -1, 0.0),
            task(ENTER, 3, -1, 0.0),
        ],
    );
    man.class = class::CLASS_7;
    man.hit_points = MasterHitPoints::of_data(45);
    man.disabled = DisableMode::DISABLED;
    militia.copy_from(&man);
    assert_eq!(
        (militia.id, militia.class, militia.base_id),
        (master_id(74), class::CLASS_6, Some(master_id(74)))
    );
    assert!(militia.available.is_available() && !militia.disabled.is_disabled());
    assert_eq!(
        (militia.hit_points, militia.action_part().tasks.len()),
        (MasterHitPoints::of_data(45), 2)
    );
    assert_eq!(
        militia.action_part().task_by_group,
        TaskGroup::FEMALE_VILLAGERS
    );
}

/// The work command's choice for a gather (0x6a) on a farm (master 50, one of the objects
/// the work command lists): the first master of the swap group with a task for the farm's
/// class and a work value wins over the first with a task for any target, though later; a
/// task naming the farm's master is taken at once; a master whose task has the own task's
/// type is passed over. A unit of a master the player lacks is not modelled.
#[test]
fn villager_on_farm_takes_its_group_farmer_for_the_farm_class() {
    let farm = Master {
        id: ids::FARM,
        kind: MasterKind::BUILDING,
        class: class::FARM,
        ..Master::default()
    };
    let gather = |group, id, work| task(REPAIR, group, id, work);
    let with = |own: Vec<Task>, others: Vec<Master>| {
        let mut all = vec![master(ids::VILLAGER_MALE, 1, own), farm.clone()];
        all.extend(others);
        Masters::new(all)
    };
    let pick = |masters: &Masters, own: &Master| {
        let site = TaskTarget {
            id: object_id(12557),
            master: masters.get(ids::FARM).unwrap(),
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
        let on = TaskOn {
            target: Some(&site),
            general: Some(&site),
            terrain: None,
        };
        work_master(masters, &actor, &on, None, &ALONE, slots(), &roles()).map(|work| {
            work.map(|(master, task)| {
                (
                    master.id.get(),
                    ClassId::field(task.object_group),
                    MasterId::field(task.object_id),
                )
            })
        })
    };
    let own = |masters: &Masters| masters.get(ids::VILLAGER_MALE).unwrap().clone();
    let generic = master(master_id(120), 1, vec![gather(-1, -1, 1.0)]);
    let by_class = master(master_id(122), 1, vec![gather(49, -1, 0.5)]);
    let masters = with(vec![], vec![generic.clone(), by_class.clone()]);
    assert_eq!(pick(&masters, &own(&masters)), Ok(Some((122, 49, -1))));
    let masters = with(vec![], vec![generic]);
    assert_eq!(pick(&masters, &own(&masters)), Ok(Some((120, -1, -1))));
    let idle = master(master_id(122), 1, vec![gather(49, -1, 0.0)]);
    let masters = with(vec![], vec![idle]);
    assert_eq!(pick(&masters, &own(&masters)), Ok(None));
    let named = master(master_id(120), 1, vec![gather(-1, ids::FARM.get(), 0.0)]);
    let masters = with(vec![], vec![named, by_class.clone()]);
    assert_eq!(pick(&masters, &own(&masters)), Ok(Some((120, -1, 50))));
    let attacker = master(
        master_id(120),
        1,
        vec![task(ActionKind::COMBAT, -1, -1, 0.0)],
    );
    let masters = with(
        vec![task(ActionKind::COMBAT, -1, -1, 0.0)],
        vec![attacker, by_class],
    );
    assert_eq!(pick(&masters, &own(&masters)), Ok(Some((122, 49, -1))));
    let stranger = master(master_id(999), 1, vec![]);
    assert!(pick(&masters, &stranger).is_err());
}

#[test]
fn villager_takes_the_first_master_of_its_swap_group_with_task_for_its_foundation() {
    let masters = villagers();
    let site = TaskTarget {
        id: object_id(10028),
        master: masters.get(ids::HOUSE).unwrap(),
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
            master: masters.get(own).unwrap(),
            player: PlayerId::new(2),
            held: carrying(None, 0.0),
            attribute_95: Amount::ZERO,
            civ: CivId::new(1),
        };
        let on = TaskOn {
            target: Some(&site),
            general: Some(&site),
            terrain: None,
        };
        work_master(&masters, &actor, &on, None, &ALONE, slots(), &roles())
            .map(|work| work.map(|(master, task)| (master.id.get(), task.action_kind)))
    };
    assert_eq!(pick(ids::VILLAGER_MALE), Ok(Some((118, ActionKind::BUILD))));
    assert_eq!(
        pick(ids::VILLAGER_FEMALE),
        Ok(Some((212, ActionKind::BUILD)))
    );
    assert_eq!(pick(ids::BUILDER), Ok(Some((118, ActionKind::BUILD))));
}

#[test]
fn master_unit_ai_kind_reads_its_class_volley_missile_and_combat_level() {
    let master = |class, volley_missile, combat_level| Master {
        class,
        combat_level,
        creatable: Some(CreatablePart {
            volley_missile,
            ..CreatablePart::default()
        }),
        ..Master::default()
    };
    // a building without a volley missile, with one (the tower's), and a boar's combat level
    assert_eq!(
        unit_ai_kind(
            &master(class::BUILDING, None, CombatLevel::NONE),
            ids::HOUSE,
            &roles()
        ),
        Some(UnitAiKind::Building)
    );
    assert_eq!(
        unit_ai_kind(
            &master(class::BUILDING, Some(master_id(5)), CombatLevel::NONE),
            master_id(79),
            &roles(),
        ),
        Some(UnitAiKind::Tower)
    );
    assert_eq!(
        unit_ai_kind(
            &master(class::PREDATOR, None, CombatLevel::ANIMAL),
            master_id(48),
            &roles()
        ),
        Some(UnitAiKind::Boar)
    );
    assert_eq!(
        unit_ai_kind(
            &master(class::PREDATOR, None, CombatLevel::NONE),
            master_id(126),
            &roles()
        ),
        Some(UnitAiKind::Lion)
    );
    // the factory's own ids first
    assert_eq!(
        unit_ai_kind(
            &master(class::BUILDING, None, CombatLevel::NONE),
            ids::ID_1105,
            &roles()
        ),
        Some(UnitAiKind::SiegeTower)
    );
}

/// The lookup by player and id: the player's table's master, none without the table or the
/// master.
#[test]
fn master_is_found_by_its_player_and_id() {
    let every = vec![Arc::new(Masters::default()), Arc::new(villagers())];
    let found = player_master(&every, PlayerId::new(1), ids::VILLAGER_MALE).map(|master| master.id);
    assert_eq!(found, Some(ids::VILLAGER_MALE));
    assert!(player_master(&every, PlayerId::new(0), ids::VILLAGER_MALE).is_none());
    assert!(player_master(&every, PlayerId::new(2), ids::VILLAGER_MALE).is_none());
}

/// The world's masters by player: a player's own table, none beyond the tables.
#[test]
fn the_world_masters_are_looked_up_by_player() {
    use crate::objects::masters::PlayersMastersExt;
    let mut every = [Masters::new([house()]), Masters::new([])].map(Arc::new);
    assert!(
        every
            .of(PlayerId::new(0))
            .is_some_and(|masters| masters.get(house().id).is_some())
    );
    assert!(
        every
            .of(PlayerId::new(1))
            .is_some_and(|masters| masters.get(house().id).is_none())
    );
    assert!(every.of(PlayerId::new(2)).is_none());
    *every.of_mut(PlayerId::new(1)).unwrap() = Masters::new([house()]);
    assert!(
        every
            .of(PlayerId::new(1))
            .is_some_and(|masters| masters.get(house().id).is_some())
    );
    assert!(every.of_mut(PlayerId::new(2)).is_none());
}

/// The world's masters as it shares them: a player's own table's `Arc`, which a reader holds
/// without a copy, none beyond the tables.
#[test]
fn the_world_masters_are_lent_shared_by_player() {
    use crate::objects::masters::PlayersMastersExt;
    let every = [Masters::new([house()]), Masters::new([])].map(Arc::new);
    let held = every.shared(PlayerId::new(0)).map(Arc::clone);
    assert!(
        held.as_ref()
            .is_some_and(|masters| Arc::ptr_eq(masters, &every[0]))
    );
    assert!(
        held.as_deref()
            .is_some_and(|masters| masters.get(house().id).is_some())
    );
    assert!(every.shared(PlayerId::new(2)).is_none());
}

/// A master's objects move ([`Master::moves`], `FUN_14123bb20`'s test of the master's slot
/// 0xe8, its speed `+0x740`): a moving master whose speed is above 0; one of speed 0 and a
/// master without a moving part do not.
#[test]
fn master_moves_with_moving_part_whose_speed_is_above_0() {
    let moving = |base: f32| MoveStats {
        fly_mode: FlyMode::GROUND,
        speed: Speed::new(base * 1.5),
        base: Speed::new(base),
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
    };
    let mut master = of_kind(MasterKind::COMBAT);
    master.move_stats = None;
    assert!(!master.moves());
    master.move_stats = Some(moving(0.9));
    assert!(master.moves());
    master.move_stats = Some(moving(0.0));
    assert!(!master.moves());
}
