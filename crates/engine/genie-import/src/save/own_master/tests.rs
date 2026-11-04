use super::*;
use crate::dat::DefaultAction;
use genie_model::combat::ChargeRate;
use genie_model::map::Radii;
use genie_model::map::TerrainMask;
use genie_model::movement::MoveAlgorithm;
use genie_model::movement::{MoveStats, turn_rate};
use genie_model::objects::FacetCount;
use genie_model::objects::SpeedMultiplier;
use genie_model::objects::SpriteFlags;
use genie_model::objects::facing::SpriteFacets;
use genie_model::world::Seconds;
use genie_model::world::WholeSeconds;
use genie_save::generated::world::OwnTribeMasterCombatObjectEntry1;

/// The civ every test's object belongs to.
const CIV: CivId = CivId::new(1);

/// A static part of master `id` (the first `readTemp`), class 3 and 120 hit points, 2 high.
fn static_part(id: i16) -> OwnRgeMasterStaticObject {
    OwnRgeMasterStaticObject {
        read_temp: id,
        read_temp_2: 3,
        read_temp_3: 120,
        read_temp_4: -1,
        read_temp_5: id,
        radius_z: 2.0,
        ..OwnRgeMasterStaticObject::default()
    }
}

/// A moving layer over `static_part`: it walks with sprite 10 at 0.8 and turns in 2 s.
fn moving_layer(static_part: OwnRgeMasterStaticObject) -> OwnRgeMasterMovingObject {
    OwnRgeMasterMovingObject {
        own_rge_master_animated_object: OwnRgeMasterAnimatedObject {
            own_rge_master_static_object: static_part,
            max_speed: 0.8,
        },
        temp_move_sprite: 10,
        temp_run_sprite: -1,
        turn_speed: 2.0,
        move_algorithem: 1,
        turn_radius: 1.5,
        turn_radius_speed: 0.5,
        friendly_collision_circle_scale: 0.25,
        ..OwnRgeMasterMovingObject::default()
    }
}

/// A creatable layer over `static_part` that trains at master 109 in 25 s.
fn creatable_layer(static_part: OwnRgeMasterStaticObject) -> OwnTribeMasterCombatObject {
    let mut creatable = OwnTribeMasterCombatObject {
        train_location: vec![OwnTribeMasterCombatObjectEntry1 {
            train_location_id: 109,
            train_time: 25,
            ..OwnTribeMasterCombatObjectEntry1::default()
        }],
        ..OwnTribeMasterCombatObject::default()
    };
    let combat = &mut creatable.own_rge_master_combat_object;
    combat.weapon_range = 5.0;
    combat
        .own_rge_master_action_object
        .own_rge_master_moving_object = moving_layer(static_part);
    creatable
}

/// An object that holds `kind`'s record as its own master.
fn holding(kind: MasterKind, set: impl FnOnce(&mut RgeStaticObject)) -> RgeStaticObject {
    let mut object = RgeStaticObject {
        master_kind: Some(kind.get()),
        ..RgeStaticObject::default()
    };
    set(&mut object);
    object
}

#[test]
fn object_without_record_has_neither_master() {
    let object = RgeStaticObject::default();
    assert_eq!(own_master(&object, CIV, None).unwrap(), None);
    assert_eq!(successor_master(&object, CIV, None).unwrap(), None);
}

#[test]
fn record_without_its_kind_is_none() {
    let object = RgeStaticObject {
        rge_master_static_object: Some(Box::new(static_part(70))),
        ..RgeStaticObject::default()
    };
    assert_eq!(own_master(&object, CIV, None).unwrap(), None);
}

#[test]
fn static_record_reads_its_ids_class_hit_points_and_values() {
    let object = holding(MasterKind::STATIC, |object| {
        object.rge_master_static_object = Some(Box::new(OwnRgeMasterStaticObject {
            read_temp_4: 59,
            read_temp_5: 58,
            sprite_num: 200,
            death_sprite_num: 201,
            attribute_rot: 0.5,
            undead_flag: 1,
            ..static_part(70)
        }));
    });
    let master = own_master(&object, CIV, None).unwrap().unwrap();
    assert_eq!(
        (master.id, master.kind, master.copy_id, master.base_id),
        (
            MasterId::named(70),
            MasterKind::STATIC,
            MasterId::of_data(59),
            MasterId::of_data(58)
        )
    );
    assert_eq!(master.class, ClassId::of_master(3));
    assert_eq!(master.hit_points, MasterHitPoints::of_data(120));
    assert_eq!(master.height, Distance::new(2.0));
    assert_eq!(
        (master.standing_sprite, master.death_sprite),
        (SpriteId::of_data(200), SpriteId::of_data(201))
    );
    assert_eq!(
        master.life,
        Some(MasterData {
            decay: AmountRate::new(0.5),
            undead: true,
        })
    );
    assert_eq!(
        (master.animated, master.moving, master.action, master.combat),
        (None, None, None, None)
    );
}

#[test]
fn record_without_its_id_is_an_error() {
    let object = holding(MasterKind::STATIC, |object| {
        object.rge_master_static_object = Some(Box::new(static_part(-1)));
    });
    let error = own_master(&object, CIV, None).unwrap_err();
    assert_eq!(error.reason(), "a saved master record without its id");
}

#[test]
fn building_record_holds_every_layer_below_it() {
    let object = holding(MasterKind::BUILDING, |object| {
        object.tribe_master_building_object = Some(Box::new(OwnTribeMasterBuildingObject {
            own_tribe_master_combat_object: creatable_layer(static_part(70)),
            sprite_num: 600,
            destruction_sprite_num: 601,
            construction_id: 71,
            ..OwnTribeMasterBuildingObject::default()
        }));
    });
    let master = own_master(&object, CIV, None).unwrap().unwrap();
    assert_eq!(
        (master.id, master.kind),
        (MasterId::named(70), MasterKind::BUILDING)
    );
    let building = master.building.unwrap();
    assert_eq!(
        (
            building.construction_sprite,
            building.destruction_sprite,
            building.head_unit
        ),
        (
            SpriteId::of_data(600),
            SpriteId::of_data(601),
            MasterId::of_data(71)
        )
    );
    let creatable = master.creatable.unwrap();
    assert_eq!(creatable.trained_at, vec![MasterId::named(109)]);
    assert_eq!(creatable.build_time, WholeSeconds::of_data(25));
    assert_eq!(master.combat.unwrap().weapon_range, Distance::new(5.0));
    assert!(master.action.is_some());
    assert_eq!(master.moving.unwrap().move_sprite, SpriteId::of_data(10));
    assert_eq!(master.animated.unwrap().max_speed, Speed::new(0.8));
    assert_eq!(master.missile, None);
}

#[test]
fn record_starts_from_the_dat_master_of_its_copy_id() {
    let copy = MasterId::named(83);
    let mut dat_master = Master::of_kind(MasterKind::COMBAT);
    dat_master.id = copy;
    if let Some(creatable) = &mut dat_master.creatable {
        creatable.recharge_rate = ChargeRate::new(0.5);
    }
    let table = MasterTable::from_rows([((CIV, copy), MasterData::default(), DefaultAction::None)])
        .with_work(CIV, copy, dat_master);
    let creatable_object = holding(MasterKind::COMBAT, |object| {
        object.tribe_master_combat_object =
            Some(Box::new(creatable_layer(OwnRgeMasterStaticObject {
                read_temp_4: 83,
                ..static_part(70)
            })));
    });
    let master = own_master(&creatable_object, CIV, Some(&table))
        .unwrap()
        .unwrap();
    // the record holds no recharge rate: the `.dat` copy's stays
    assert_eq!(
        master.creatable.unwrap().recharge_rate,
        ChargeRate::new(0.5)
    );
    assert_eq!(master.id, MasterId::named(70));
    // a static record drops the layers its `.dat` copy has above it
    let static_object = holding(MasterKind::STATIC, |object| {
        object.rge_master_static_object = Some(Box::new(OwnRgeMasterStaticObject {
            read_temp_4: 83,
            ..static_part(70)
        }));
    });
    let master = own_master(&static_object, CIV, Some(&table))
        .unwrap()
        .unwrap();
    assert_eq!(
        (master.combat, master.creatable, master.moving),
        (None, None, None)
    );
}

#[test]
fn successor_master_reads_the_second_record() {
    let object = RgeStaticObject {
        master_kind_2: Some(MasterKind::STATIC.get()),
        rge_master_static_object_2: Some(Box::new(static_part(5))),
        ..RgeStaticObject::default()
    };
    assert_eq!(own_master(&object, CIV, None).unwrap(), None);
    let master = successor_master(&object, CIV, None).unwrap().unwrap();
    assert_eq!(master.id, MasterId::named(5));
}

#[test]
fn moving_record_moves_with_its_own_turning_and_the_tables_sprite() {
    let object = holding(MasterKind::MOVING, |object| {
        object.rge_master_moving_object = Some(Box::new(moving_layer(static_part(70))));
    });
    // without a table it has no movement values
    assert_eq!(
        own_master(&object, CIV, None).unwrap().unwrap().move_stats,
        None
    );
    let table = MasterTable::default()
        .with_sprite(
            SpriteId::named(10),
            SpeedMultiplier::of_data(1.5),
            FacetCount::of_data(8),
            SpriteFlags::TURNS,
        )
        .with_masks(vec![0b1]);
    let master = own_master(&object, CIV, Some(&table)).unwrap().unwrap();
    let facets = Some(FacetCount::of_data(8));
    assert_eq!(
        master.move_stats,
        Some(MoveStats {
            fly_mode: FlyMode::of_data(0),
            speed: Speed::new(0.8 * 1.5),
            base: Speed::new(0.8),
            turn_rate: turn_rate(Seconds::new(2.0), facets),
            turn_radius: Distance::new(1.5),
            turn_radius_speed: Speed::new(0.5),
            radius: Radii::new(0.0, 0.0, 2.0),
            restriction: RestrictionId::of_data(0).unwrap(),
            restriction_mask: TerrainMask::of_bits(0b1),
            turn_time: Seconds::new(2.0),
            move_algorithm: MoveAlgorithm::from_bits_retain(1),
            min_collision: 0.25,
            fast_turn: false,
            facets: Some(SpriteFacets {
                count: FacetCount::of_data(8),
                turns: true,
            }),
            weapon_range: Distance::ZERO,
        })
    );
}

/// A saved action of an object of its own master takes the task at its slot in that master's
/// list (`FUN_140a7a2f0`); a slot of −1 or past the list is reported.
#[test]
fn own_master_gives_the_task_at_its_slot() {
    use crate::save::own_task_at;
    use genie_model::objects::masters::{ActionPart, Task, TaskId};
    let own = Master {
        action: Some(ActionPart {
            tasks: vec![
                Task::default(),
                Task {
                    id: TaskId::of_task(1),
                    ..Task::default()
                },
            ],
            ..ActionPart::default()
        }),
        ..Master::default()
    };
    assert_eq!(
        own_task_at(&own, 1).map(|task| task.id),
        Ok(TaskId::of_task(1))
    );
    let reason = |index| own_task_at(&own, index).map_err(|error| error.reason());
    assert_eq!(
        reason(-1).map(|_| ()),
        Err("a saved action without its task")
    );
    assert_eq!(
        reason(2).map(|_| ()),
        Err("a saved action's task beyond its master's list")
    );
}
