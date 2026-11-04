use crate::GenieResult;
use crate::map::{RestrictionId, TerrainMask};
use crate::movement::MoveAlgorithm;
use crate::movement::Mover;
use crate::objects::FacetCount;
use crate::objects::SpeedMultiplier;
use crate::objects::SpriteFlags;
use crate::objects::SpriteId;
use crate::objects::masters::FlyMode;
use crate::objects::masters::MasterId;
use crate::players::CivId;
use crate::world::load::fixtures::{
    combat_master, combat_object, master_moving_mut, master_static_mut, moving_mut, static_mut,
    with_masters,
};
use crate::world::load::movers::mover;
use genie_import::dat::{MasterTable, Turning};
use genie_save::generated::world::RgeObject;
use genie_save::generated::world::{Master, PathData, PlayerObjects};

/// Master 83: restriction 1, top speed 1, turning 0.5, walking as sprite 11 and running as 12.
fn master() -> Master {
    let mut master = combat_master(83);
    let rge_master_static_object = master_static_mut(&mut master);
    rge_master_static_object.terrain_restriction_mask_id = 1;
    rge_master_static_object.radius_x = 0.2;
    rge_master_static_object.idle_sprite_id = 10;
    let rge_master_moving_object = master_moving_mut(&mut master);
    rge_master_moving_object.move_sprite_id = 11;
    rge_master_moving_object.run_sprite_id = 12;
    rge_master_moving_object.turn_speed = 0.5;
    rge_master_moving_object
        .rge_master_animated_object
        .max_speed = 1.0;
    master
}

fn table() -> MasterTable {
    let turning = Turning {
        move_algorithm: MoveAlgorithm::NONE,
        min_collision_size_multiplier: 1.0,
    };
    MasterTable::of_master(
        CivId::GAIA,
        MasterId::of_data(83).unwrap(),
        Some(turning),
        FlyMode::of_data(0),
    )
    .with_sprite(
        SpriteId::named(10),
        SpeedMultiplier::ZERO,
        FacetCount::of_data(8),
        SpriteFlags::NONE,
    )
    .with_sprite(
        SpriteId::named(11),
        SpeedMultiplier::of_data(2.0),
        FacetCount::of_data(8),
        SpriteFlags::NONE,
    )
    .with_sprite(
        SpriteId::named(12),
        SpeedMultiplier::of_data(3.0),
        FacetCount::of_data(8),
        SpriteFlags::NONE,
    )
    .with_masks(vec![0, 0b11])
}

/// A walker of master 83 showing sprite `sprite`.
fn walker(sprite: i16) -> genie_save::Object {
    let mut object = combat_object(5, 83);
    static_mut(&mut object).sprite = sprite;
    moving_mut(&mut object).angle = 0.25;
    object
}

fn run(
    object: &genie_save::Object,
    objects: &PlayerObjects,
    table: Option<&MasterTable>,
) -> GenieResult<Option<Mover>> {
    let rge_static_object = object.static_part().unwrap().clone();
    mover(object, &rge_static_object, objects, &[], CivId::GAIA, table)
}

/// The speed is the master's times the shown sprite's factor; walking and running by their
/// sprites'; the restriction's mask from the table.
#[test]
fn walker_moves_by_its_master_and_sprites() {
    let objects = with_masters(vec![master()]);
    let m = run(&walker(11), &objects, Some(&table())).unwrap().unwrap();
    assert_eq!(m.stats.speed.get().to_bits(), 2.0_f32.to_bits());
    assert_eq!(
        (m.stats.restriction, m.stats.restriction_mask),
        (RestrictionId::named(1), TerrainMask::of_bits(0b11))
    );
    assert_eq!(m.angle.get().to_bits(), 0.25_f32.to_bits());
    assert!(m.velocity.is_none() && m.paths.is_empty());
    // without a sprite, no factor; without a run sprite, running is walking
    let mut no_run = master();
    master_moving_mut(&mut no_run).run_sprite_id = -1;
    let mover = run(&walker(-1), &with_masters(vec![no_run]), Some(&table()))
        .unwrap()
        .unwrap();
    assert_eq!(mover.stats.speed.get().to_bits(), 1.0_f32.to_bits());
}

/// An object below type 30 does not move.
#[test]
fn static_object_has_no_mover() {
    let tree = genie_save::Object {
        rge_object: Some(Box::new(RgeObject {
            rge_static_object: Some(Box::default()),
            ..Default::default()
        })),
        ..genie_save::Object::default()
    };
    assert!(
        run(&tree, &PlayerObjects::default(), None)
            .unwrap()
            .is_none()
    );
}

/// What the slice cannot read is reported: future path data, a master the save or the table
/// does not have, a sprite, turning or restriction the table does not have, a path level's state
/// that names none.
#[test]
fn what_the_mover_cannot_read_is_unsupported() {
    let objects = with_masters(vec![master()]);
    let mut future = walker(11);
    moving_mut(&mut future).future_path_data = Some(Box::default());
    assert!(run(&future, &objects, Some(&table())).is_err());
    assert!(run(&walker(11), &PlayerObjects::default(), Some(&table())).is_err());
    assert!(run(&walker(11), &objects, None).is_err());
    assert!(run(&walker(99), &objects, Some(&table())).is_err());
    let unturned = MasterTable::default().with_sprite(
        SpriteId::named(11),
        SpeedMultiplier::ONE,
        FacetCount::of_data(8),
        SpriteFlags::NONE,
    );
    assert!(run(&walker(11), &objects, Some(&unturned)).is_err());
    let masks = table().with_masks(vec![0]);
    assert!(run(&walker(11), &objects, Some(&masks)).is_err());
    let mut negative = master();
    master_static_mut(&mut negative).terrain_restriction_mask_id = -1;
    assert!(run(&walker(11), &with_masters(vec![negative]), Some(&table())).is_err());
    let mut lost_path = walker(11);
    moving_mut(&mut lost_path).temp_path_data = vec![PathData {
        path_id: 4,
        ..PathData::default()
    }];
    assert!(run(&lost_path, &objects, Some(&table())).is_err());
    // a level's state 1, which no routine stores, is no state; 2 is one
    let level = |state| {
        let mut object = walker(11);
        moving_mut(&mut object).temp_path_data = vec![PathData {
            path_id: -1,
            state,
            ..PathData::default()
        }];
        object
    };
    assert!(run(&level(2), &objects, Some(&table())).is_ok());
    assert!(run(&level(1), &objects, Some(&table())).is_err());
}
