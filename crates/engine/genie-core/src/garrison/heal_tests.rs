//! A building's whole seconds with units inside (`TRIBE_Building_Object::update`, `0x1411c9000`):
//! the heal of every unit inside by its own master and type, the armor a full heal gives back,
//! the repair by the villagers inside, and a foundation completed in its update, which runs none.

use crate::combat::damage::ArmorPair;
use crate::construction::repair::BUILDING_REPAIR_COST;
use crate::map::GridSize;
use crate::map::Position;
use crate::objects::HitPoints;
use crate::objects::MasterHitPoints;
use crate::objects::ObjectState;
use crate::objects::masters::BuildingMaster;
use crate::objects::masters::ClassId;
use crate::objects::masters::CostAmount;
use crate::objects::masters::CostSlot;
use crate::objects::masters::CreatablePart;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::TrainLocation;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::test_ids::object_id;
use crate::testing::attribute;
use crate::testing::class;
use crate::testing::scene::{PLAYER, Scene, SceneObject};
use crate::world::WholeSeconds;
use crate::world::World;
use crate::world::WorldTime;
use crate::world::tests::in_update;
use std::sync::Arc;

/// Player 1's castle (master 82: 800 hit points, a garrison heal rate of 2 and a repair rate of
/// 1, built in 40 s for 100 stone).
const CASTLE: i32 = 7;
/// Player 1's soldier (master 4, infantry: 40 hit points).
const SOLDIER: i32 = 5;
/// Player 2's villager (master 83, class 4: 25 hit points in player 1's table, 30 in its own).
const VILLAGER: i32 = 9;
/// A second villager of player 2's.
const SECOND_VILLAGER: i32 = 10;
/// Player 1's walker (master 300, a moving object, type 30: 20 hit points).
const WALKER: i32 = 11;
/// Player 1's tower (master 79, a building: 100 hit points, built in 10 s for 50 stone).
const TOWER: i32 = 12;
/// The ally whose villagers are inside.
const ALLY: PlayerId = PlayerId::new(2);
/// The villager's master.
const VILLAGER_MASTER: MasterId = MasterId::named(83);
/// The castle's master.
const CASTLE_MASTER: MasterId = MasterId::named(82);

/// Master `id` of type `kind` and class `class`, of `hit_points`, costing `stone` stone and made in
/// `time` seconds.
fn master(
    id: i16,
    kind: MasterKind,
    class: ClassId,
    hit_points: i16,
    stone: i16,
    time: i16,
) -> Master {
    Master {
        id: MasterId::named(id),
        kind,
        class,
        hit_points: MasterHitPoints::of_data(hit_points),
        creatable: Some(CreatablePart {
            cost: [
                CostSlot::new(attribute::STONE, CostAmount::of_data(stone), true),
                CostSlot::EMPTY,
                CostSlot::EMPTY,
            ],
            train_locations: vec![TrainLocation {
                time: WholeSeconds::of_data(time),
                ..TrainLocation::default()
            }],
            ..CreatablePart::default()
        }),
        ..Master::default()
    }
}

/// The castle (a foundation in `state`), the soldier, the two villagers, the walker and the tower,
/// `inside` the castle; each player holds 100 stone and the building repair cost rate (271) of
/// 0.5; the world's time is 3 s.
fn garrison_world(state: ObjectState, inside: &[i32]) -> World {
    let castle = Master {
        building: Some(BuildingMaster {
            garrison_heal_rate: HitPoints::new(2.0),
            garrison_repair_rate: HitPoints::new(1.0),
            ..BuildingMaster::default()
        }),
        ..master(82, MasterKind::BUILDING, class::BUILDING, 800, 100, 40)
    };
    let villager = || master(83, MasterKind::COMBAT, class::VILLAGER, 25, 0, 25);
    let at = |x| Position::new(x, 2.5, 0.0);
    let mut world = Scene::open(GridSize::new(16, 16))
        .players(2)
        .object(SceneObject {
            state,
            desolid: state == ObjectState::Foundation,
            ..SceneObject::new(CASTLE, PLAYER, castle, Position::new(8.0, 8.0, 0.0))
        })
        .object(SceneObject::new(
            SOLDIER,
            PLAYER,
            master(4, MasterKind::COMBAT, ClassId::named(6), 40, 0, 21),
            at(2.5),
        ))
        .object(SceneObject::new(VILLAGER, ALLY, villager(), at(3.5)))
        .object(SceneObject::new(SECOND_VILLAGER, ALLY, villager(), at(4.5)))
        .object(SceneObject::new(
            WALKER,
            PLAYER,
            master(300, MasterKind::MOVING, ClassId::named(99), 20, 0, 1),
            at(5.5),
        ))
        .object(SceneObject::new(
            TOWER,
            PLAYER,
            master(79, MasterKind::BUILDING, class::TOWER, 100, 50, 10),
            Position::new(12.0, 12.0, 0.0),
        ))
        .build();
    Arc::make_mut(&mut world.model.masters[2])
        .get_mut(VILLAGER_MASTER)
        .unwrap()
        .hit_points = MasterHitPoints::of_data(30);
    for player in &mut world.players {
        player.attributes = vec![Amount::ZERO; 280];
        player.attributes[attribute::STONE.index()] = Amount::new(100.0);
        player.attributes[BUILDING_REPAIR_COST] = Amount::new(0.5);
    }
    let castle = world.live_index(PLAYER, object_id(CASTLE)).unwrap();
    world.players[1].objects[castle].garrison = inside.iter().map(|&id| object_id(id)).collect();
    world.world_time = WorldTime::from_millis(3000);
    world
}

/// Object `id` of `owner`'s hit points, to change.
fn hp_mut(world: &mut World, owner: PlayerId, id: i32) -> &mut HitPoints {
    let index = world.live_index(owner, object_id(id)).unwrap();
    &mut world.players[owner.index()].objects[index].life.hp
}

/// Object `id` of `owner`'s hit points.
fn hp(world: &World, owner: PlayerId, id: i32) -> f32 {
    let index = world.live_index(owner, object_id(id)).unwrap();
    world.players[owner.index()].objects[index].life.hp.get()
}

/// The castle's master's building values, to change.
fn castle_master_mut(world: &mut World) -> &mut BuildingMaster {
    Arc::make_mut(&mut world.model.masters[1])
        .get_mut(CASTLE_MASTER)
        .unwrap()
        .building
        .as_mut()
        .unwrap()
}

/// The castle's whole seconds in its update ([`Unit::building_seconds`]): then its hit points
/// and its owner's stone.
///
/// [`Unit::building_seconds`]: crate::objects::unit::Unit::building_seconds
fn castle_seconds(world: &mut World) -> (f32, f32) {
    let castle = world.live_index(PLAYER, object_id(CASTLE)).unwrap();
    let roles = world.model.roles.clone();
    in_update(world, (PLAYER, castle), 50, |unit, actions, step| {
        unit.building_seconds(actions, step, &roles)
    })
    .unwrap();
    (
        hp(world, PLAYER, CASTLE),
        world.players[1].attributes[attribute::STONE.index()].get(),
    )
}

/// Every unit inside heals, whoever's it is, below its own master's hit points (the ally's
/// villager by its player's master of 30, past player 1's 25), once in an update of three whole
/// seconds; a building at its hit points does not repair itself.
#[test]
fn building_heals_every_unit_inside_by_its_own_master() {
    let mut world = garrison_world(ObjectState::Alive, &[SOLDIER, VILLAGER]);
    *hp_mut(&mut world, PLAYER, SOLDIER) = HitPoints::new(10.0);
    *hp_mut(&mut world, ALLY, VILLAGER) = HitPoints::new(24.5);
    assert_eq!(castle_seconds(&mut world), (800.0, 100.0));
    assert_eq!(
        (hp(&world, PLAYER, SOLDIER), hp(&world, ALLY, VILLAGER)),
        (12.0, 26.5)
    );
}

/// A unit inside heals by its master's kind: below type 70 by its own slots (`FUN_141193b30`: up
/// to its master's hit points, no stinger's), a building inside as a combat object
/// (`FUN_141213b10`, by the rate for nothing; not its own slot 0x110, which charges by its build
/// time).
#[test]
fn unit_inside_heals_by_its_master_kind() {
    let mut world = garrison_world(ObjectState::Alive, &[WALKER, TOWER]);
    *hp_mut(&mut world, PLAYER, WALKER) = HitPoints::new(10.0);
    *hp_mut(&mut world, PLAYER, TOWER) = HitPoints::new(50.0);
    assert_eq!(castle_seconds(&mut world), (800.0, 100.0));
    assert_eq!(
        (hp(&world, PLAYER, WALKER), hp(&world, PLAYER, TOWER)),
        (12.0, 52.0)
    );
    let mut world = garrison_world(ObjectState::Alive, &[WALKER]);
    *hp_mut(&mut world, PLAYER, WALKER) = HitPoints::new(19.0);
    // a combat object's most would hold the stinger's 5 more (slot 200)
    world
        .parts
        .combat_mut(object_id(WALKER))
        .unwrap()
        .hp_stinger = HitPoints::new(5.0);
    let _ = castle_seconds(&mut world);
    assert_eq!(hp(&world, PLAYER, WALKER), 20.0);
}

/// A unit of type 70 or more that its heal left full gets back the armor its charge's hits took
/// (`FUN_1412066a0`: `+0x9e4` zeroed); one still short of its hit points keeps it off.
#[test]
fn full_heal_gives_charged_armor_back() {
    let armor_after = |hp: f32| {
        let mut world = garrison_world(ObjectState::Alive, &[SOLDIER]);
        *hp_mut(&mut world, PLAYER, SOLDIER) = HitPoints::new(hp);
        world
            .parts
            .combat_mut(object_id(SOLDIER))
            .unwrap()
            .armor_removed = ArmorPair::new(2, 1);
        let _ = castle_seconds(&mut world);
        world.parts.combat_of(object_id(SOLDIER)).armor_removed
    };
    assert_eq!(armor_after(39.0), ArmorPair::ZERO);
    assert_eq!(armor_after(10.0), ArmorPair::new(2, 1));
}

/// A heal rate of NaN heals nothing (`COMISS`, `SETA`), nor does any rate a unit at NaN hit
/// points (`COMISS`, `JBE`).
#[test]
fn nan_heals_nothing() {
    let mut world = garrison_world(ObjectState::Alive, &[SOLDIER, VILLAGER]);
    *hp_mut(&mut world, PLAYER, SOLDIER) = HitPoints::new(10.0);
    *hp_mut(&mut world, ALLY, VILLAGER) = HitPoints::new(f32::NAN);
    let _ = castle_seconds(&mut world);
    assert_eq!(hp(&world, PLAYER, SOLDIER), 12.0);
    assert!(hp(&world, ALLY, VILLAGER).is_nan());
    castle_master_mut(&mut world).garrison_heal_rate = HitPoints::new(f32::NAN);
    world.world_time = WorldTime::from_millis(5000);
    let _ = castle_seconds(&mut world);
    assert_eq!(hp(&world, PLAYER, SOLDIER), 12.0);
}

/// The villagers inside, an ally's too, repair the damaged building at its owner's cost
/// (`FUN_1411d33e0` by the villagers times the repair rate: its master's hit points times that
/// over its build time, 20 for one, 40 for two), each paying the attribute 271 times the hit
/// points over its master's of its 100 stone.
#[test]
fn villagers_inside_repair_building_at_its_owner_cost() {
    let repaired = |inside: &[i32]| {
        let mut world = garrison_world(ObjectState::Alive, inside);
        *hp_mut(&mut world, PLAYER, CASTLE) = HitPoints::new(600.0);
        let castle = castle_seconds(&mut world);
        (
            castle,
            world.players[2].attributes[attribute::STONE.index()].get(),
        )
    };
    assert_eq!(
        repaired(&[VILLAGER]),
        ((620.0, 100.0 - 100.0 * (0.5 * 20.0 / 800.0)), 100.0)
    );
    assert_eq!(
        repaired(&[SOLDIER, VILLAGER, SECOND_VILLAGER]),
        ((640.0, 100.0 - 100.0 * (0.5 * 40.0 / 800.0)), 100.0)
    );
}

/// The repair waits for a villager inside, a repair rate above 0 (a NaN is not) and hit points
/// below the master's.
#[test]
fn repair_waits_for_villagers_rate_and_damage() {
    let damaged = |inside: &[i32], rate: f32| {
        let mut world = garrison_world(ObjectState::Alive, inside);
        *hp_mut(&mut world, PLAYER, CASTLE) = HitPoints::new(600.0);
        castle_master_mut(&mut world).garrison_repair_rate = HitPoints::new(rate);
        castle_seconds(&mut world)
    };
    assert_eq!(damaged(&[SOLDIER], 1.0), (600.0, 100.0));
    assert_eq!(damaged(&[VILLAGER], 0.0), (600.0, 100.0));
    assert_eq!(damaged(&[VILLAGER], f32::NAN), (600.0, 100.0));
    let mut world = garrison_world(ObjectState::Alive, &[VILLAGER]);
    assert_eq!(castle_seconds(&mut world), (800.0, 100.0));
}

/// The update reads its building's state once before its switch (`cVar3`): a foundation that
/// completes in this update runs no whole seconds, so its garrison heals from the next update on.
#[test]
fn foundation_completed_in_its_update_runs_no_seconds() {
    let mut world = garrison_world(ObjectState::Foundation, &[SOLDIER]);
    *hp_mut(&mut world, PLAYER, SOLDIER) = HitPoints::new(10.0);
    let castle = world.live_index(PLAYER, object_id(CASTLE)).unwrap();
    let roles = world.model.roles.clone();
    let update = |world: &mut World| {
        in_update(world, (PLAYER, castle), 50, |unit, actions, step| {
            unit.building_update(actions, step, &roles)
        })
        .unwrap();
    };
    update(&mut world);
    assert_eq!(
        world.players[1].objects[castle].life.state,
        ObjectState::Alive
    );
    assert_eq!(hp(&world, PLAYER, SOLDIER), 10.0);
    update(&mut world);
    assert_eq!(hp(&world, PLAYER, SOLDIER), 12.0);
}
