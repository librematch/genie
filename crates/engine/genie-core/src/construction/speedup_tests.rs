//! A speed-up building's work on the objects about it (`FUN_1411c39b0`, `FUN_1411c4a00`): the
//! tiles of its reach, its three effects on the allied objects its player's object target names,
//! and the call in its whole seconds.

use crate::GenieResult;
use crate::combat::damage::ArmorPair;
use crate::combat::hp_transforms::HP_TRANSFORM;
use crate::construction::speedup::reach_tiles;
use crate::groups::formation::tests::ghost_at;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Position;
use crate::map::Tile;
use crate::movement::Speed;
use crate::objects::HitPoints;
use crate::objects::MasterHitPoints;
use crate::objects::Regeneration;
use crate::objects::layers::Speedup;
use crate::objects::layers::SpeedupValue;
use crate::objects::masters::ActionPart;
use crate::objects::masters::BuildingMaster;
use crate::objects::masters::ClassId;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::Task;
use crate::objects::masters::WorkValue;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::Relation;
use crate::test_ids::object_id;
use crate::testing::class;
use crate::testing::scene::{PLAYER, Scene, SceneObject, moving_master, walking};
use crate::world::Seconds;
use crate::world::World;
use crate::world::tests::in_update;

/// The speed-up building (player 1's, master 1754, at 5, 5).
const BUILDING: i32 = 7;
/// Player 1's cart in the building's reach (tile 8, 8).
const CART: i32 = 20;
/// Player 1's cart one tile past its reach (tile 9, 8).
const FAR_CART: i32 = 21;
/// The enemy's cart in its reach (tile 6, 6).
const ENEMY_CART: i32 = 22;
/// The ally's cart in its reach (tile 1, 1).
const ALLY_CART: i32 = 23;
/// Player 1's villager in its reach (tile 3, 3).
const VILLAGER: i32 = 24;
/// The player whom player 1 counts an enemy.
const ENEMY: PlayerId = PlayerId::new(2);
/// The player whom player 1 counts an ally.
const ALLY: PlayerId = PlayerId::new(3);
/// The speed-up building's master.
const SPEEDUP_MASTER: i16 = 1754;
/// The carts' master.
const CART_MASTER: i16 = 204;
/// The carts' class.
const CART_CLASS: ClassId = ClassId::named(19);
/// A class target's offset among the object targets (`FUN_140fde260`).
const CLASS_TARGET: f32 = 900.0;

/// The speed-up building's master: 1000 hit points, a footprint of 1.5 each way.
fn speedup_master() -> Master {
    Master {
        id: MasterId::named(SPEEDUP_MASTER),
        kind: MasterKind::BUILDING,
        class: class::BUILDING,
        hit_points: MasterHitPoints::of_data(1000),
        radius: Footprint::new(1.5, 1.5),
        building: Some(BuildingMaster::default()),
        ..Master::default()
    }
}

/// A walking master `id` of type `kind` and class `class`, of `hit_points`.
fn walker(id: i16, kind: MasterKind, class: ClassId, hit_points: i16) -> Master {
    Master {
        id: MasterId::named(id),
        kind,
        class,
        hit_points: MasterHitPoints::of_data(hit_points),
        ..moving_master(walking(Speed::new(1.0)))
    }
}

/// A cart of 100 hit points.
fn cart() -> Master {
    walker(CART_MASTER, MasterKind::COMBAT, CART_CLASS, 100)
}

/// A building master `id` of class 3, of 500 hit points.
fn building(id: i16) -> Master {
    Master {
        id: MasterId::named(id),
        kind: MasterKind::BUILDING,
        class: class::BUILDING,
        hit_points: MasterHitPoints::of_data(500),
        ..Master::default()
    }
}

/// Object `id` of player 1 of `master` at tile `x`, 4.
fn at_row_4(id: i32, master: Master, x: f32) -> SceneObject {
    SceneObject::new(id, PLAYER, master, Position::new(x, 4.5, 0.0))
}

/// The building, the five carts and the villager, then `others`, on a map of 12 by 12 tiles of
/// three players: player 1 counts player 3 an ally and player 2 an enemy. Every player holds 262
/// attributes of 0; player 1's speed-up attributes name the building's master (255), a reach of
/// 2 (256: tiles 1 to 8 each way), the carts' master (258) and the movement speed (259, 5) by 0.5
/// (257), then `attributes`.
fn speedup_world(attributes: &[(usize, f32)], others: Vec<SceneObject>) -> World {
    let mut scene = Scene::open(GridSize::new(12, 12))
        .players(3)
        .relation(PLAYER, ALLY, Relation::Ally)
        .object(SceneObject::new(
            BUILDING,
            PLAYER,
            speedup_master(),
            Position::new(5.0, 5.0, 0.0),
        ))
        .object(SceneObject::new(
            CART,
            PLAYER,
            cart(),
            Position::new(8.5, 8.5, 0.0),
        ))
        .object(SceneObject::new(
            FAR_CART,
            PLAYER,
            cart(),
            Position::new(9.5, 8.5, 0.0),
        ))
        .object(SceneObject::new(
            ENEMY_CART,
            ENEMY,
            cart(),
            Position::new(6.5, 6.5, 0.0),
        ))
        .object(SceneObject::new(
            ALLY_CART,
            ALLY,
            cart(),
            Position::new(1.5, 1.5, 0.0),
        ))
        .object(SceneObject::new(
            VILLAGER,
            PLAYER,
            walker(83, MasterKind::COMBAT, class::VILLAGER, 25),
            Position::new(3.5, 3.5, 0.0),
        ));
    for object in others {
        scene = scene.object(object);
    }
    let mut world = scene.build();
    for player in &mut world.players {
        player.attributes = vec![Amount::ZERO; 262];
    }
    let speedup = [
        (255, f32::from(SPEEDUP_MASTER)),
        (256, 2.0),
        (257, 0.5),
        (258, f32::from(CART_MASTER)),
        (259, 5.0),
    ];
    for &(index, value) in speedup.iter().chain(attributes) {
        world.players[1].attributes[index] = Amount::new(value);
    }
    world
}

/// The building's whole seconds in an update ([`Unit::building_seconds`]), its world's dt 1 s.
///
/// [`Unit::building_seconds`]: crate::objects::unit::Unit::building_seconds
fn building_seconds(world: &mut World) -> GenieResult<()> {
    world.dt_seconds = Seconds::new(1.0);
    let index = world.live_index(PLAYER, object_id(BUILDING)).unwrap();
    let roles = world.model.roles.clone();
    in_update(world, (PLAYER, index), 1000, |unit, actions, step| {
        unit.building_seconds(actions, step, &roles)
    })
}

/// Object `id`'s moving speed-up (`+0x808`, `+0x80c`).
fn mover_speedup(world: &World, id: i32) -> Speedup {
    world.parts.mover_of(object_id(id)).unwrap().speedup
}

/// Object `id`'s action speed-up (`+0x890`, `+0x894`).
fn work_speedup(world: &World, id: i32) -> Speedup {
    world.parts.actions_of(object_id(id)).speedup
}

/// The speed-up the building gives by `value`.
fn sped(value: f32) -> Speedup {
    Speedup::of_building(object_id(BUILDING), SpeedupValue::of_data(value))
}

/// Object `id`'s hit points.
fn hp(world: &World, id: i32) -> f32 {
    world.object(object_id(id)).unwrap().1.life.hp.get()
}

/// Object `id`'s hit points set to `hp`.
fn set_hp(world: &mut World, id: i32, hp: f32) {
    world.object_mut(object_id(id)).unwrap().1.life.hp = HitPoints::new(hp);
}

/// The reach covers the footprint grown by the range: from the place less both (`CVTTSS2SI`,
/// toward 0, none below 0) to the place plus both less 0.001 (`DAT_142eb1e9c`), at most the
/// map's last tile; a NaN reach covers nothing.
#[test]
fn reach_covers_footprint_grown_by_range_within_map() {
    let map = GridSize::new(12, 12);
    let tiles = |x: f32, y: f32, range: f32| {
        reach_tiles(
            Position::new(x, y, 0.0),
            Footprint::new(1.5, 1.0),
            Distance::new(range),
            map,
            0.001,
        )
    };
    assert_eq!(tiles(5.0, 5.0, 2.0), (Tile::new(1, 2), Tile::new(8, 7)));
    // a far edge on a tile's edge leaves that tile out
    assert_eq!(tiles(5.5, 5.0, 2.0), (Tile::new(2, 2), Tile::new(8, 7)));
    assert_eq!(tiles(1.0, 11.0, 2.0), (Tile::new(0, 8), Tile::new(4, 11)));
    let (low, high) = tiles(5.0, 5.0, f32::NAN);
    assert!(high.x() < low.x() && high.y() < low.y());
}

/// The movement speed effect (5) gives each allied mover the master target names in the reach
/// the building's speed-up (`+0x808` the building's id, `+0x80c` the amount): its own player's
/// and an ally's, not an enemy's, one past the reach or another master's; the action layer's
/// speed-up stays.
#[test]
fn movement_speed_effect_speeds_allied_movers_target_names() {
    let mut world = speedup_world(&[], Vec::new());
    building_seconds(&mut world).unwrap();
    assert_eq!(
        [CART, FAR_CART, ENEMY_CART, ALLY_CART, VILLAGER].map(|id| mover_speedup(&world, id)),
        [
            sped(0.5),
            Speedup::NONE,
            Speedup::NONE,
            sped(0.5),
            Speedup::NONE
        ]
    );
    assert_eq!(work_speedup(&world, CART), Speedup::NONE);
}

/// The movement speed effect skips a tile a player marked (the obstruction manager's word: the
/// low 32 bits of its bits from 23); a tile's other bits and a mark past those 32 bits do not
/// count.
#[test]
fn movement_speed_effect_skips_tiles_player_marked() {
    let mut world = speedup_world(&[], Vec::new());
    let tiles = world.spatial.tiles_mut_for_test();
    tiles.mark_for_test(Tile::new(8, 8), 0x80_0000 << 2);
    tiles.mark_for_test(Tile::new(1, 1), 0x7f_ffff | (1 << 55));
    building_seconds(&mut world).unwrap();
    assert_eq!(mover_speedup(&world, CART), Speedup::NONE);
    assert_eq!(mover_speedup(&world, ALLY_CART), sped(0.5));
}

/// An object target from 900 to 999 names a class (`FUN_140fde260`): the villagers' class
/// target speeds the villager and no cart; a moving object of type 30 takes it, an animated one
/// (type 20) does not.
#[test]
fn class_target_names_every_object_of_its_class() {
    let others = vec![
        at_row_4(
            30,
            walker(300, MasterKind::MOVING, class::VILLAGER, 20),
            6.5,
        ),
        at_row_4(
            31,
            walker(301, MasterKind::ANIMATED, class::VILLAGER, 20),
            7.5,
        ),
    ];
    let villagers = CLASS_TARGET + f32::from(class::VILLAGER.get());
    let mut world = speedup_world(&[(258, villagers)], others);
    building_seconds(&mut world).unwrap();
    assert_eq!(
        [VILLAGER, CART, 30, 31].map(|id| mover_speedup(&world, id)),
        [sped(0.5), Speedup::NONE, sped(0.5), Speedup::NONE]
    );
}

/// The work rate effect (13) gives each allied action object (type 40 and up) the target names
/// the building's speed-up on its action layer (`+0x890`, `+0x894`): with a class target the
/// building itself and a tower, not a moving object of the class (type 30), and neither the town
/// centre nor the castle (masters 109 and 82, which a master target names).
#[test]
fn work_rate_effect_speeds_action_objects_and_spares_town_centre_and_castle() {
    let others = || {
        vec![
            at_row_4(30, building(109), 2.5),
            at_row_4(31, building(82), 4.5),
            at_row_4(32, building(79), 6.5),
            at_row_4(
                33,
                walker(300, MasterKind::MOVING, class::BUILDING, 20),
                7.5,
            ),
        ]
    };
    let buildings = CLASS_TARGET + f32::from(class::BUILDING.get());
    let mut world = speedup_world(&[(258, buildings), (259, 13.0), (257, 0.25)], others());
    building_seconds(&mut world).unwrap();
    assert_eq!(
        [BUILDING, 30, 31, 32, 33].map(|id| work_speedup(&world, id)),
        [
            sped(0.25),
            Speedup::NONE,
            Speedup::NONE,
            sped(0.25),
            Speedup::NONE
        ]
    );
    let mut world = speedup_world(&[(258, 109.0), (259, 13.0), (257, 0.25)], others());
    building_seconds(&mut world).unwrap();
    assert_eq!(work_speedup(&world, 30), sped(0.25));
}

/// The second effect (260 by 261) follows the first on each tile.
#[test]
fn second_effect_follows_first() {
    let mut world = speedup_world(&[(260, 13.0), (261, 0.25)], Vec::new());
    building_seconds(&mut world).unwrap();
    assert_eq!(
        (mover_speedup(&world, CART), work_speedup(&world, CART)),
        (sped(0.5), sped(0.25))
    );
}

/// An effect that is none of the three (0, a NaN's integer indefinite) does nothing.
#[test]
fn other_effect_does_nothing() {
    for effect in [0.0, 6.0, f32::NAN] {
        let mut world = speedup_world(&[(259, effect)], Vec::new());
        set_hp(&mut world, CART, 50.0);
        building_seconds(&mut world).unwrap();
        assert_eq!(
            (mover_speedup(&world, CART), work_speedup(&world, CART)),
            (Speedup::NONE, Speedup::NONE)
        );
        assert_eq!(hp(&world, CART), 50.0);
    }
}

/// [`speedup_world`] with the regeneration effect (109) of 30 on the carts' class and the
/// building's own regeneration powerup (`+0xa34`, slot 0x6f0) 2 and stinger (`+0xb3c`, slot
/// 0x750) 3: 60 / (3 + (2 + 30)) hit points in the world's dt of 1 s, 1.7142857.
fn regeneration_world(others: Vec<SceneObject>) -> World {
    let carts = CLASS_TARGET + f32::from(CART_CLASS.get());
    let mut world = speedup_world(&[(258, carts), (259, 109.0), (257, 30.0)], others);
    let combat = world.parts.combat_mut(object_id(BUILDING)).unwrap();
    combat.regen_powerup = Regeneration::new(2.0);
    combat.regen_stinger = Regeneration::new(3.0);
    world
}

/// The regeneration effect heals each allied object the target names by slot 0xb8: a combat
/// object up to its most (`FUN_141203be0`); below type 70 its hit points add it
/// (`FUN_141188ca0`), then at most its master's. A combat object at its master's hit points then
/// gets back the armor its charge's hits took (`FUN_1412066a0`), and one above them comes down
/// to them. The enemy's cart heals nothing.
#[test]
fn regeneration_effect_heals_by_building_rate_and_caps_at_master() {
    let others = vec![
        at_row_4(30, walker(300, MasterKind::MOVING, CART_CLASS, 20), 6.5),
        at_row_4(31, cart(), 7.5),
        at_row_4(32, cart(), 2.5),
    ];
    let mut world = regeneration_world(others);
    for (id, hp) in [
        (CART, 50.0),
        (ENEMY_CART, 50.0),
        (30, 19.0),
        (31, 99.0),
        (32, 105.0),
    ] {
        set_hp(&mut world, id, hp);
    }
    for id in [CART, 31, 32] {
        world.parts.combat_mut(object_id(id)).unwrap().armor_removed = ArmorPair::new(2, 1);
    }
    building_seconds(&mut world).unwrap();
    let heal = 60.0 / (3.0_f32 + (2.0 + 30.0)) * 1.0;
    assert_eq!(
        [CART, ENEMY_CART, 30, 31, 32].map(|id| hp(&world, id)),
        [50.0 + heal, 50.0, 20.0, 100.0, 100.0]
    );
    assert_eq!(
        [CART, 31, 32].map(|id| world.parts.combat_of(object_id(id)).armor_removed),
        [ArmorPair::new(2, 1), ArmorPair::ZERO, ArmorPair::ZERO]
    );
}

/// The heal adds the building's stinger to its powerup plus the amount (`ADDSS` in that order):
/// a stinger of 1e8, a powerup of −1e8 and an amount of 1 make no rate in `f32` and an
/// unbounded heal (60 / 0), up to the cart's most.
#[test]
fn regeneration_rate_adds_stinger_to_powerup_and_amount() {
    let mut world = regeneration_world(Vec::new());
    world.players[1].attributes[257] = Amount::new(1.0);
    let combat = world.parts.combat_mut(object_id(BUILDING)).unwrap();
    combat.regen_powerup = Regeneration::new(-1e8);
    combat.regen_stinger = Regeneration::new(1e8);
    set_hp(&mut world, CART, 10.0);
    building_seconds(&mut world).unwrap();
    assert_eq!(hp(&world, CART), 100.0);
}

/// A combat object's gain below its most runs its hit point transforms' test at once
/// (`FUN_141204450`), and the cap reads the master it then has: a cart of 40 hit points at 29
/// that becomes one of 100 above 30 keeps its share, above the 40 its old master had, and the
/// armor its charge's hits took (below the new master's hit points).
#[test]
fn regeneration_transform_caps_at_new_master() {
    let transform = Task {
        action_kind: HP_TRANSFORM,
        work_val1: WorkValue::new(205.0),
        work_val2: WorkValue::new(30.0),
        ..Task::default()
    };
    let weak = Master {
        id: MasterId::named(206),
        hit_points: MasterHitPoints::of_data(40),
        action: Some(ActionPart {
            tasks: vec![transform],
            ..ActionPart::default()
        }),
        ..cart()
    };
    let others = vec![
        at_row_4(30, weak, 6.5),
        SceneObject::new(
            31,
            PLAYER,
            walker(205, MasterKind::COMBAT, CART_CLASS, 100),
            Position::new(11.5, 11.5, 0.0),
        ),
    ];
    let mut world = regeneration_world(others);
    set_hp(&mut world, 30, 29.0);
    world.parts.combat_mut(object_id(30)).unwrap().armor_removed = ArmorPair::new(2, 1);
    building_seconds(&mut world).unwrap();
    let (_, unit) = world.object(object_id(30)).unwrap();
    assert_eq!(unit.life.master_id, MasterId::named(205));
    assert!(40.0 < unit.life.hp.get());
    assert_eq!(
        world.parts.combat_of(object_id(30)).armor_removed,
        ArmorPair::new(2, 1)
    );
}

/// A ghost on a tile of the reach is an object of its viewer and the ghost master (type 25):
/// the regeneration names it by its class and changes nothing the world holds (its hit points
/// are not held), and the speed-ups skip it (below type 30).
#[test]
fn ghost_takes_nothing_world_holds() {
    let ghost_master = Master {
        id: MasterId::named(243),
        kind: MasterKind::DOPPELGANGER,
        class: ClassId::named(11),
        ..Master::default()
    };
    let others = vec![SceneObject::new(
        30,
        PLAYER,
        ghost_master,
        Position::new(11.5, 0.5, 0.0),
    )];
    for effect in [5.0, 13.0, 109.0] {
        let mut world = speedup_world(&[(258, CLASS_TARGET + 11.0), (259, effect)], others.clone());
        let ghost = ghost_at(-2, 4.5, 4.5);
        world.spatial.insert_ghost(ghost.id, ghost.position);
        world.players[1].ghosts = vec![ghost];
        assert_eq!(building_seconds(&mut world), Ok(()));
    }
}

/// An object on a tile of the reach that the world does not hold (a scene's rock) stops the
/// walk.
#[test]
fn object_world_does_not_hold_stops_walk() {
    let mut world = Scene::open(GridSize::new(12, 12))
        .rocks([Tile::new(4, 4)])
        .object(SceneObject::new(
            BUILDING,
            PLAYER,
            speedup_master(),
            Position::new(5.0, 5.0, 0.0),
        ))
        .build();
    world.players[1].attributes = vec![Amount::ZERO; 262];
    world.players[1].attributes[255] = Amount::new(f32::from(SPEEDUP_MASTER));
    world.players[1].attributes[256] = Amount::new(2.0);
    world.players[1].attributes[259] = Amount::new(109.0);
    assert_eq!(
        building_seconds(&mut world).unwrap_err().reason(),
        "an object the slice does not hold"
    );
}

/// The building works only while its player's attribute 255 names its master (`UCOMISS`, `JP`):
/// another master, a NaN or a player without the attribute names none; named, a player without
/// the other attributes stops the update.
#[test]
fn building_works_only_while_attribute_255_names_its_master() {
    for named in [1755.0, f32::NAN] {
        let mut world = speedup_world(&[(255, named)], Vec::new());
        building_seconds(&mut world).unwrap();
        assert_eq!(mover_speedup(&world, CART), Speedup::NONE);
    }
    let mut world = speedup_world(&[], Vec::new());
    world.players[1].attributes.truncate(255);
    building_seconds(&mut world).unwrap();
    assert_eq!(mover_speedup(&world, CART), Speedup::NONE);
    let mut world = speedup_world(&[], Vec::new());
    world.players[1].attributes.truncate(261);
    assert_eq!(
        building_seconds(&mut world).unwrap_err().reason(),
        "an attribute the player does not have"
    );
}
