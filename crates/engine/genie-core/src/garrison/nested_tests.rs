//! What the units inside a unit come in and go out with (`+0x9a8`, [`NestedCount`]): the
//! container's count as a unit that holds others enters (slot 0x1a0, `FUN_141212e10`) and
//! leaves (slot 0x1a8, `FUN_141213120`), and slot 0x498 that adds it (`FUN_141213500`).

use crate::garrison::ContainerId;
use crate::garrison::EnterOutcome;
use crate::garrison::InsideCount;
use crate::garrison::LetOut;
use crate::garrison::NO_POINT;
use crate::garrison::NestedCount;
use crate::garrison::tests::give_room;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::Position;
use crate::movement::Speed;
use crate::objects::MasterHitPoints;
use crate::objects::masters::ClassId;
use crate::objects::masters::GarrisonCapacity;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObjectFlags;
use crate::test_ids::object_id;
use crate::testing::class;
use crate::testing::scene::{PLAYER, Scene, SceneObject, move_by, walking};
use crate::unit_ai::UnitAi;
use crate::world::World;

/// The building (type 80, room for 10) the units enter.
const BUILDING: i32 = 7;
/// The transport (type 70, object flag 0, room for 10).
const TRANSPORT: i32 = 8;
/// A unit of type 70 with room for 10, which takes the soldiers in.
const CARRIER: i32 = 20;
/// The first soldier, which goes into the carrier.
const FIRST: i32 = 21;
/// The second soldier, which goes into the carrier.
const SECOND: i32 = 22;
/// A unit of type 50 (`RGE_Combat_Object`) with room for 10.
const CRATE: i32 = 23;
/// A third soldier, which goes into the crate.
const THIRD: i32 = 24;
/// A class of no meaning to the garrison (6, the infantry's).
const INFANTRY: ClassId = ClassId::named(6);

/// A master `id` of type `kind` and class `class` with room for 10 and 100 hit points: a
/// building of radius 1, a unit of radius 0.2 that walks ([`walking`]).
fn master(id: i16, kind: MasterKind, class: ClassId) -> Master {
    let radius = if kind == MasterKind::BUILDING {
        1.0
    } else {
        0.2
    };
    let mut master = Master {
        id: MasterId::named(id),
        kind,
        class,
        radius: Footprint::new(radius, radius),
        garrison: GarrisonCapacity::of_data(10),
        hit_points: MasterHitPoints::of_data(100),
        ..Master::default()
    };
    if kind != MasterKind::BUILDING {
        move_by(&mut master, walking(Speed::new(1.0)));
    }
    master
}

/// Unit `id` of master `master` with its unit AI at `x` in the second row.
fn unit(id: i32, master: Master, x: f32) -> SceneObject {
    SceneObject {
        ai: Some(UnitAi::default()),
        ..SceneObject::new(id, PLAYER, master, Position::new(x, 1.5, 0.0))
    }
}

/// Player 1's building, transport, carrier, crate and three soldiers, all outside.
fn nested_world() -> World {
    let soldier = || master(4, MasterKind::COMBAT, INFANTRY);
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
            Master {
                object_flags: ObjectFlags::GARRISON,
                ..master(545, MasterKind::COMBAT, class::TRANSPORT)
            },
            Position::new(9.5, 9.5, 0.0),
        ))
        .object(unit(
            CARRIER,
            master(300, MasterKind::COMBAT, INFANTRY),
            1.5,
        ))
        .object(unit(FIRST, soldier(), 2.5))
        .object(unit(SECOND, soldier(), 3.5))
        .object(unit(
            CRATE,
            master(301, MasterKind::RGE_COMBAT, INFANTRY),
            4.5,
        ))
        .object(unit(THIRD, soldier(), 5.5))
        .build()
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

/// Object `id`'s `+0x9a8`.
fn nested(world: &World, id: i32) -> u8 {
    world.parts.combat_of(object_id(id)).nested.get()
}

/// Object `id`'s slot 0x498.
fn held(world: &World, id: i32) -> i32 {
    world.garrisoned(object_id(id)).get()
}

/// The world with both soldiers inside the carrier.
fn loaded_carrier() -> World {
    let mut world = nested_world();
    enter(&mut world, FIRST, CARRIER);
    enter(&mut world, SECOND, CARRIER);
    world
}

/// The count adds the low byte of what a unit holds, wrapping as a byte does, and takes it off
/// down to 0; slot 0x498 reads it unsigned.
#[test]
fn count_adds_low_byte_wrapping_and_takes_off_down_to_zero() {
    let count = |value| NestedCount::of_data(value);
    assert_eq!(NestedCount::ZERO.took_in(InsideCount::new(3)), count(3));
    assert_eq!(count(250).took_in(InsideCount::new(10)), count(4));
    assert_eq!(count(1).took_in(InsideCount::new(257)), count(2));
    assert_eq!(count(5).let_out(InsideCount::new(2)), count(3));
    assert_eq!(count(2).let_out(InsideCount::new(2)), NestedCount::ZERO);
    assert_eq!(count(1).let_out(InsideCount::new(2)), NestedCount::ZERO);
    assert_eq!(count(5).let_out(InsideCount::new(258)), count(3));
    assert_eq!(count(200).count(), InsideCount::new(200));
    assert_eq!(NestedCount::default(), NestedCount::ZERO);
}

/// A unit that holds others enters a building with them: the building's `+0x9a8` takes what
/// the unit holds (its slot 0x498, 2), and the building's slot 0x498 counts them with the unit.
#[test]
fn unit_brings_its_units_into_building() {
    let mut world = loaded_carrier();
    assert_eq!((held(&world, CARRIER), nested(&world, CARRIER)), (2, 0));
    enter(&mut world, CARRIER, BUILDING);
    assert_eq!(nested(&world, BUILDING), 2);
    assert_eq!(held(&world, BUILDING), 3);
    assert_eq!(held(&world, CARRIER), 2);
    // a unit that holds nothing adds nothing
    enter(&mut world, THIRD, BUILDING);
    assert_eq!((nested(&world, BUILDING), held(&world, BUILDING)), (2, 4));
}

/// A unit that holds others leaves a building with them: the building's `+0x9a8` gives back
/// what it holds, down to 0 where it held less (a count the load doubled or lost).
#[test]
fn unit_takes_its_units_out_of_building() {
    let mut world = loaded_carrier();
    enter(&mut world, CARRIER, BUILDING);
    leave(&mut world, CARRIER, BUILDING);
    assert_eq!((nested(&world, BUILDING), held(&world, BUILDING)), (0, 0));
    assert_eq!(held(&world, CARRIER), 2);
    enter(&mut world, CARRIER, BUILDING);
    world.parts.combat_mut(object_id(BUILDING)).unwrap().nested = NestedCount::of_data(1);
    leave(&mut world, CARRIER, BUILDING);
    assert_eq!(nested(&world, BUILDING), 0);
}

/// A transport (type 70) counts what its units hold as a building does, and what it holds in
/// turn counts in the container it enters: its slot 0x498 with its own `+0x9a8`.
#[test]
fn transport_counts_what_its_units_hold() {
    let mut world = loaded_carrier();
    enter(&mut world, CARRIER, TRANSPORT);
    assert_eq!((nested(&world, TRANSPORT), held(&world, TRANSPORT)), (2, 3));
    enter(&mut world, TRANSPORT, BUILDING);
    assert_eq!((nested(&world, BUILDING), held(&world, BUILDING)), (3, 4));
    leave(&mut world, TRANSPORT, BUILDING);
    assert_eq!((nested(&world, BUILDING), held(&world, BUILDING)), (0, 0));
    leave(&mut world, CARRIER, TRANSPORT);
    assert_eq!((nested(&world, TRANSPORT), held(&world, TRANSPORT)), (0, 0));
}

/// Below type 70 nothing counts what a unit holds: a unit of type 50 takes slots 0x1a0 and
/// 0x1a8 of `RGE_Combat_Object` (`FUN_141084b30`, `FUN_141192460`), and a container of type 50
/// keeps no `+0x9a8` (`FUN_141212e10`'s `JC` below 0x46); its slot 0x498 is its list's count.
#[test]
fn nothing_below_kind_70_counts_what_units_hold() {
    let mut world = nested_world();
    enter(&mut world, THIRD, CRATE);
    enter(&mut world, CRATE, BUILDING);
    assert_eq!((nested(&world, BUILDING), held(&world, BUILDING)), (0, 1));
    leave(&mut world, CRATE, BUILDING);
    let mut world = loaded_carrier();
    enter(&mut world, CARRIER, CRATE);
    assert_eq!((nested(&world, CRATE), held(&world, CRATE)), (0, 1));
}

/// `FUN_1411e7220` lets a unit that holds others out of a building: it comes out with them,
/// and the building gives back what it holds.
#[test]
fn let_out_takes_unit_out_with_its_units() {
    let mut world = loaded_carrier();
    enter(&mut world, CARRIER, BUILDING);
    let building = world.live_index(PLAYER, object_id(BUILDING)).unwrap();
    let out = world.let_out(
        PLAYER,
        building,
        object_id(CARRIER),
        Point::new(NO_POINT, NO_POINT),
        (None, false),
    );
    assert_eq!(out, Ok(LetOut::Out));
    let carrier = world.object(object_id(CARRIER)).unwrap().1;
    assert_eq!(carrier.life.inside, None);
    assert_eq!((nested(&world, BUILDING), held(&world, BUILDING)), (0, 0));
    let first = world.object(object_id(FIRST)).unwrap().1;
    assert_eq!(
        first.life.inside,
        Some(ContainerId::new(object_id(CARRIER)))
    );
}

/// Unit `id` enters the building as an Enter does (`FUN_140a867b0`), its room test that of
/// slot 0x1a0 (`FUN_1411921d0`: slot 0x198 with no unit).
fn enter_as_enter(world: &mut World, id: i32) -> EnterOutcome {
    let index = world.live_index(PLAYER, object_id(id)).unwrap();
    let roles = world.model.roles.clone();
    world
        .run_on_step(PLAYER, index, |unit, actions, step| {
            unit.enter_building(actions, step, ContainerId::new(object_id(BUILDING)), &roles)
        })
        .unwrap()
}

/// The room tests count what the units inside hold: the building's slot 0x498 with the
/// carrier and its two soldiers is 3, so a building of room 3 has no room for one more (slot
/// 0x198 with no unit, `FUN_1412169b0`) and keeps an entering unit out (the enter's slot 0x1a0
/// asks it), and one of room 4 has room.
#[test]
fn room_counts_what_units_inside_hold() {
    let mut world = loaded_carrier();
    enter(&mut world, CARRIER, BUILDING);
    let room = |world: &World| {
        let (bp, building) = world.object(object_id(BUILDING)).unwrap();
        world.has_room(bp, building, None).unwrap()
    };
    give_room(&mut world, MasterId::named(109), 3);
    assert!(!room(&world));
    assert_eq!(enter_as_enter(&mut world, THIRD), EnterOutcome::NoRoom);
    give_room(&mut world, MasterId::named(109), 4);
    assert!(room(&world));
}
