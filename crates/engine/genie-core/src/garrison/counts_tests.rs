//! A building's counts of the units inside it, its villagers (`+0xba8`) and its raiders
//! (`+0xc2c`): as units enter and leave, as the load links them in again, and in its slot 0x498.

use crate::garrison::ContainerId;
use crate::garrison::GarrisonCounts;
use crate::garrison::InsideCount;
use crate::garrison::RaiderCountdown;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Position;
use crate::objects::masters::ClassId;
use crate::objects::masters::GarrisonCapacity;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObjectFlags;
use crate::test_ids::object_id;
use crate::testing::class;
use crate::testing::scene::{PLAYER, Scene, SceneObject};
use crate::unit_ai::UnitAi;
use crate::world::World;
use crate::world::load::world::garrisons::garrison_firepower;

/// The building (type 80) the units enter.
const BUILDING: i32 = 7;
/// The transport (type 70, object flag 0) the units enter.
const TRANSPORT: i32 = 8;
/// A villager (class 4).
const VILLAGER: i32 = 10;
/// A unit of type 70 of another class, which may count down to its raid.
const RAIDER: i32 = 11;
/// A second unit of type 70 of that class.
const SOLDIER: i32 = 12;
/// A second villager, which the load finds inside the transport.
const CARRIED: i32 = 13;
/// A class of no meaning to the counts (6, the infantry's).
const INFANTRY: ClassId = ClassId::named(6);

/// A master `id` of type `kind` and class `class`, of radius 0.2 (a building's 1).
fn master(id: i16, kind: MasterKind, class: ClassId) -> Master {
    let radius = if kind == MasterKind::BUILDING {
        1.0
    } else {
        0.2
    };
    Master {
        id: MasterId::named(id),
        kind,
        class,
        radius: Footprint::new(radius, radius),
        garrison: GarrisonCapacity::of_data(10),
        ..Master::default()
    }
}

/// The transport's master: type 70 with object flag 0.
fn transport_master() -> Master {
    Master {
        object_flags: ObjectFlags::GARRISON,
        ..master(545, MasterKind::COMBAT, class::TRANSPORT)
    }
}

/// Unit `id` of master `master` with its unit AI (an entering villager records its work for the
/// town bell) at `x` in the first row, inside `inside` as a save holds it.
fn counted_unit(id: i32, master: Master, x: f32, inside: Option<ContainerId>) -> SceneObject {
    SceneObject {
        ai: Some(UnitAi::default()),
        inside,
        ..SceneObject::new(id, PLAYER, master, Position::new(x, 1.5, 0.0))
    }
}

/// Player 1's building, transport, villager, raider and soldier, the units outside
/// ([`counts_scene`]).
fn counts_world(inside: Option<ContainerId>) -> World {
    counts_scene(inside).build()
}

/// Player 1's building, transport, villager, raider and soldier ([`counted_unit`]); `inside`
/// puts the units in the building, as a save holds them.
fn counts_scene(inside: Option<ContainerId>) -> Scene {
    let unit = |id, master, x| counted_unit(id, master, x, inside);
    Scene::open(GridSize::new(12, 12))
        .object(SceneObject::new(
            BUILDING,
            PLAYER,
            master(109, MasterKind::BUILDING, class::TOWER),
            Position::new(5.0, 5.0, 0.0),
        ))
        .object(SceneObject::new(
            TRANSPORT,
            PLAYER,
            transport_master(),
            Position::new(9.5, 9.5, 0.0),
        ))
        .object(unit(
            VILLAGER,
            master(83, MasterKind::COMBAT, class::VILLAGER),
            1.5,
        ))
        .object(unit(RAIDER, master(4, MasterKind::COMBAT, INFANTRY), 2.5))
        .object(unit(SOLDIER, master(4, MasterKind::COMBAT, INFANTRY), 3.5))
}

/// Unit `id`'s countdown to its raid (`+0x9a0`) is `seconds`.
fn count_down(world: &mut World, id: i32, seconds: i32) {
    world
        .parts
        .combat_mut(object_id(id))
        .unwrap()
        .raider_countdown = RaiderCountdown::of_data(seconds);
}

/// Unit `id` goes into `container` at once (slot 0x1a0 with 1).
fn enter(world: &mut World, id: i32, container: i32) {
    let index = world.live_index(PLAYER, object_id(id)).unwrap();
    world
        .enter_forced(PLAYER, index, ContainerId::new(object_id(container)))
        .unwrap();
}

/// Unit `id` leaves `container` for a spot beside it (slot 0x1a8, then slot 0x58).
fn leave(world: &mut World, id: i32, container: i32) {
    let held = world.live_index(PLAYER, object_id(container)).unwrap();
    let index = world.live_index(PLAYER, object_id(id)).unwrap();
    let made = world.players[1].objects[index]
        .master(world.master_tables(), PLAYER)
        .unwrap()
        .clone();
    world
        .leave_container(
            (PLAYER, held),
            (PLAYER, index),
            &made,
            Position::new(2.5, 3.5, 0.0),
        )
        .unwrap();
}

/// `container`'s counts.
fn counts(world: &World, container: i32) -> (i32, i32) {
    let counts = world
        .parts
        .building_of(object_id(container))
        .garrison_counts;
    (counts.villagers.get(), counts.raiders.get())
}

/// What a unit counts as it enters or leaves: one villager for the class 4, one raider for a
/// unit of type 70 whose countdown is set, below 0 too; none of either for other units.
#[test]
fn unit_counts_by_its_class_and_its_set_countdown() {
    let villager = master(83, MasterKind::COMBAT, class::VILLAGER);
    let soldier = master(4, MasterKind::COMBAT, INFANTRY);
    let animal = master(99, MasterKind::ANIMATED, INFANTRY);
    let roles = counts_world(None).model.roles;
    let of = |master: &Master, seconds| {
        let counts = GarrisonCounts::of_unit(master, RaiderCountdown::of_data(seconds), &roles);
        (counts.villagers.get(), counts.raiders.get())
    };
    assert_eq!(of(&villager, 0), (1, 0));
    assert_eq!(of(&villager, 20), (1, 1));
    assert_eq!(of(&soldier, 0), (0, 0));
    assert_eq!(of(&soldier, -1), (0, 1));
    assert_eq!(of(&animal, 20), (0, 0));
    assert_eq!(GarrisonCounts::default(), GarrisonCounts::NONE);
}

/// The building's slot 0x1c0 (`FUN_1411cb850`) counts a villager in and a unit of type 70 that
/// comes in with its countdown set; its slot 0x498 (`FUN_1411d0ce0`) leaves the raiders out.
#[test]
fn building_counts_entering_villagers_and_raiders() {
    let mut world = counts_world(None);
    enter(&mut world, VILLAGER, BUILDING);
    assert_eq!(counts(&world, BUILDING), (1, 0));
    count_down(&mut world, RAIDER, 20);
    enter(&mut world, RAIDER, BUILDING);
    enter(&mut world, SOLDIER, BUILDING);
    assert_eq!(counts(&world, BUILDING), (1, 1));
    assert_eq!(
        world.garrisoned(object_id(BUILDING)),
        Ok(InsideCount::new(2))
    );
}

/// The building's slot 0x1c8 (`FUN_1411d6ad0`) counts them out by the countdown as it leaves,
/// before slot 0x1a8 ends it: a countdown that started inside takes off a raider that never
/// counted in, and slot 0x498 then counts one more than the units inside.
#[test]
fn building_counts_leaving_units_by_their_countdown_then() {
    let mut world = counts_world(None);
    count_down(&mut world, RAIDER, 20);
    for id in [VILLAGER, RAIDER, SOLDIER] {
        enter(&mut world, id, BUILDING);
    }
    leave(&mut world, RAIDER, BUILDING);
    assert_eq!(counts(&world, BUILDING), (1, 0));
    let raider = world.parts.combat_of(object_id(RAIDER)).raider_countdown;
    assert_eq!(raider, RaiderCountdown::NONE);
    count_down(&mut world, SOLDIER, 5);
    leave(&mut world, SOLDIER, BUILDING);
    assert_eq!(counts(&world, BUILDING), (1, -1));
    assert_eq!(
        world.garrisoned(object_id(BUILDING)),
        Ok(InsideCount::new(2))
    );
    leave(&mut world, VILLAGER, BUILDING);
    assert_eq!(counts(&world, BUILDING), (0, -1));
}

/// A transport's slots 0x1c0 and 0x1c8 (`FUN_14120b790`, `FUN_141217910`) keep no counts.
#[test]
fn transport_counts_nothing() {
    let mut world = counts_world(None);
    count_down(&mut world, RAIDER, 20);
    enter(&mut world, VILLAGER, TRANSPORT);
    enter(&mut world, RAIDER, TRANSPORT);
    assert_eq!(counts(&world, TRANSPORT), (0, 0));
    leave(&mut world, RAIDER, TRANSPORT);
    assert_eq!(counts(&world, TRANSPORT), (0, 0));
    assert_eq!(world.garrisoned(object_id(TRANSPORT)), Ok(InsideCount::ONE));
}

/// The load sets neither count (its constructor's 0); its re-link enters each unit inside
/// again (`RGE_Static_Object::rehook`, slot 0x1a0 with 1), which counts them in by the saved
/// countdowns: the building's slot 0x1c0 does, the transport's keeps no counts.
#[test]
fn load_counts_the_units_inside_again() {
    let carried = counted_unit(
        CARRIED,
        master(83, MasterKind::COMBAT, class::VILLAGER),
        4.5,
        Some(ContainerId::new(object_id(TRANSPORT))),
    );
    let mut world = counts_scene(Some(ContainerId::new(object_id(BUILDING))))
        .object(carried)
        .build();
    count_down(&mut world, RAIDER, 20);
    count_down(&mut world, CARRIED, 20);
    assert_eq!(counts(&world, BUILDING), (0, 0));
    garrison_firepower(&mut world);
    let garrison = |container| {
        let index = world.live_index(PLAYER, object_id(container)).unwrap();
        world.players[1].objects[index].garrison.clone()
    };
    assert_eq!(
        garrison(BUILDING),
        [object_id(VILLAGER), object_id(RAIDER), object_id(SOLDIER)]
    );
    assert_eq!(garrison(TRANSPORT), [object_id(CARRIED)]);
    assert_eq!(counts(&world, BUILDING), (1, 1));
    assert_eq!(counts(&world, TRANSPORT), (0, 0));
    assert_eq!(
        world.garrisoned(object_id(BUILDING)),
        Ok(InsideCount::new(2))
    );
    assert_eq!(world.garrisoned(object_id(TRANSPORT)), Ok(InsideCount::ONE));
}
