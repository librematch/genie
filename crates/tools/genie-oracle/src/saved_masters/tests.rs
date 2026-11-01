use crate::fixtures::{combat_master, master_moving_mut, master_static_mut};
use crate::masters::{MasterTable, Turning};
use crate::saved_masters::{master_facets, moving_stats};
use genie_core::movement::MoveAlgorithm;
use genie_core::objects::FacetCount;
use genie_core::objects::SpriteFlags;
use genie_core::objects::SpriteId;
use genie_core::objects::facing::SpriteFacets;
use genie_core::objects::masters::FlyMode;
use genie_save::generated::world::Master;

/// A walker: master 83 standing as sprite 10, walking as 11 and running as 12.
fn walker() -> Master {
    let mut master = combat_master(83);
    let s = master_static_mut(&mut master);
    s.idle_sprite_id = 10;
    s.idle2_sprite_id = -1;
    s.terrain_restriction_mask_id = 1;
    s.radius_x = 0.2;
    let m = master_moving_mut(&mut master);
    m.move_sprite_id = 11;
    m.run_sprite_id = 12;
    m.turn_speed = 0.5;
    m.rge_master_animated_object.max_speed = 1.0;
    master
}

fn table(facets: [i16; 3]) -> MasterTable {
    let turning = Turning {
        move_algorithm: 1,
        min_collision_size_multiplier: 0.75,
    };
    MasterTable::of_master(1, 83, Some(turning), 2)
        .with_sprite(
            SpriteId::named(10),
            0.0,
            FacetCount::of_data(facets[0]),
            SpriteFlags::TURNS,
        )
        .with_sprite(
            SpriteId::named(11),
            2.0,
            FacetCount::of_data(facets[1]),
            SpriteFlags::TURNS,
        )
        .with_sprite(
            SpriteId::named(12),
            3.0,
            FacetCount::of_data(facets[2]),
            SpriteFlags::TURNS,
        )
        .with_masks(vec![0, 0b101])
}

/// The facets of the sprites the master shows, when they are all of one count; the data
/// file's sprites only.
#[test]
fn a_master_s_facets_are_its_sprites_when_they_agree() {
    let master = walker();
    let s = master.tribe_master_combat_object.as_deref().unwrap();
    let s = &s
        .rge_master_combat_object
        .rge_master_action_object
        .rge_master_moving_object
        .rge_master_animated_object
        .rge_master_static_object;
    let facets = SpriteFacets {
        count: FacetCount::of_data(8),
        turns: true,
    };
    // the fight sprite (0) is not in the data file
    assert_eq!(master_facets(&master, s, &table([8; 3])), Some(facets));
    assert_eq!(master_facets(&master, s, &table([8, 8, 16])), None);
    assert_eq!(master_facets(&master, s, &MasterTable::default()), None);
}

/// The speed is the top speed times the sprite's factor above 0 (the run sprite, else the
/// move sprite), the turning the data file's, the restriction its mask.
#[test]
fn a_walker_moves_by_its_sprites_speed_factors() {
    let master = walker();
    let s = {
        let c = master.tribe_master_combat_object.as_deref().unwrap();
        &c.rge_master_combat_object
            .rge_master_action_object
            .rge_master_moving_object
            .rge_master_animated_object
            .rge_master_static_object
    };
    let stats = moving_stats(&master, s, 1, &table([8; 3])).unwrap();
    assert_eq!(stats.speed.get().to_bits(), 3.0_f32.to_bits());
    assert_eq!(
        (stats.fly_mode, stats.move_algorithm),
        (FlyMode::of_data(2), MoveAlgorithm::ALTERNATES_SPRITES)
    );
    assert_eq!(
        (stats.restriction, stats.restriction_mask),
        (
            genie_core::map::RestrictionId::named(1),
            genie_core::map::TerrainMask::of_bits(0b101)
        )
    );
    assert_eq!(stats.radius.x().to_bits(), 0.2_f32.to_bits());
    assert!(stats.facets.is_some());
    // without a run sprite, the move sprite's factor
    let mut no_run = walker();
    master_moving_mut(&mut no_run).run_sprite_id = -1;
    let stats = moving_stats(&no_run, s, 1, &table([8; 3])).unwrap();
    assert_eq!(stats.speed.get().to_bits(), 2.0_f32.to_bits());
    // no factor at all: the top speed
    let plain =
        MasterTable::of_master(1, 83, table([8; 3]).turning(1, 83), 0).with_masks(vec![0, 1]);
    let stats = moving_stats(&master, s, 1, &plain).unwrap();
    assert_eq!(stats.speed.get().to_bits(), 1.0_f32.to_bits());
}

/// Without a moving part, the data file's turning, or a restriction the table has, no stats.
#[test]
fn a_master_without_its_movement_has_no_stats() {
    let master = walker();
    let s = {
        let c = master.tribe_master_combat_object.as_deref().unwrap();
        c.rge_master_combat_object
            .rge_master_action_object
            .rge_master_moving_object
            .rge_master_animated_object
            .rge_master_static_object
            .clone()
    };
    assert!(moving_stats(&Master::default(), &s, 1, &table([8; 3])).is_none());
    assert!(moving_stats(&master, &s, 2, &table([8; 3])).is_none());
    assert!(moving_stats(&master, &s, 1, &table([8; 3]).with_masks(vec![0])).is_none());
    let mut negative = s;
    negative.terrain_restriction_mask_id = -1;
    assert!(moving_stats(&master, &negative, 1, &table([8; 3])).is_none());
}
