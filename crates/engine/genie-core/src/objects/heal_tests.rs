//! An object's heal by an amount (slot 0x110): a combat object's (`FUN_141213b10`), at a cost of
//! its owner's for the ships' classes, a building's (`FUN_1411d33e0`) always at a cost with the
//! population tested, and the hit point transforms' test the heal runs at once.

use crate::GenieResult;
use crate::combat::hp_transforms::HP_TRANSFORM;
use crate::construction::repair::RepairOutcome;
use crate::construction::repair::{BUILDING_REPAIR_COST, UNIT_REPAIR_COST};
use crate::map::GridSize;
use crate::map::Position;
use crate::objects::HitPoints;
use crate::objects::ListIndex;
use crate::objects::MasterHitPoints;
use crate::objects::masters::ActionPart;
use crate::objects::masters::ClassId;
use crate::objects::masters::CostAmount;
use crate::objects::masters::CostSlot;
use crate::objects::masters::CreatablePart;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::Task;
use crate::objects::masters::TrainLocation;
use crate::objects::masters::WorkValue;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::test_ids::object_id;
use crate::testing::attribute;
use crate::testing::class;
use crate::testing::scene::{PLAYER, Scene, SceneObject};
use crate::world::WholeSeconds;
use crate::world::World;
use std::sync::Arc;

/// Player 1's soldier (master 4, infantry: 40 hit points).
const SOLDIER: i32 = 5;
/// Player 2's galley (master 539, a warship: 120 hit points, trained in 60 s for 90 wood and 30
/// gold).
const GALLEY: i32 = 6;
/// Player 1's castle (master 82, a building: 800 hit points, built in 40 s for 100 stone and a
/// unit's room in the population).
const CASTLE: i32 = 7;
/// Player 1's veteran (master 5), whose master the soldier's hit point transform gives.
const VETERAN: i32 = 8;
/// The player beside [`PLAYER`].
const OTHER: PlayerId = PlayerId::new(2);
/// The infantry's class.
const INFANTRY: ClassId = ClassId::named(6);
/// The veteran's master.
const VETERAN_MASTER: MasterId = MasterId::named(5);

/// Master `id` of type `kind` and class `class`, of `hit_points`, costing `cost` and trained or
/// built in `time` seconds at its one train location (none for `None`).
fn master(
    id: i16,
    (kind, class): (MasterKind, ClassId),
    hit_points: i16,
    cost: [CostSlot; 3],
    time: Option<i16>,
) -> Master {
    Master {
        id: MasterId::named(id),
        kind,
        class,
        hit_points: MasterHitPoints::of_data(hit_points),
        creatable: Some(CreatablePart {
            cost,
            train_locations: time
                .map(|seconds| TrainLocation {
                    time: WholeSeconds::of_data(seconds),
                    ..TrainLocation::default()
                })
                .into_iter()
                .collect(),
            ..CreatablePart::default()
        }),
        ..Master::default()
    }
}

/// A paid cost of `amount` of `attribute`.
const fn paid(attribute: AttributeId, amount: i16) -> CostSlot {
    CostSlot::new(attribute, CostAmount::of_data(amount), true)
}

/// The galley's master, trained in `time` seconds.
fn galley(time: Option<i16>) -> Master {
    master(
        539,
        (MasterKind::COMBAT, class::WARSHIP),
        120,
        [
            paid(attribute::WOOD, 90),
            paid(attribute::GOLD, 30),
            CostSlot::EMPTY,
        ],
        time,
    )
}

/// The soldier's master, its hit point transform into the veteran's above 20 hit points.
fn soldier() -> Master {
    Master {
        action: Some(ActionPart {
            tasks: vec![Task {
                action_kind: HP_TRANSFORM,
                work_val1: WorkValue::new(f32::from(VETERAN_MASTER.get())),
                work_val2: WorkValue::new(20.0),
                ..Task::default()
            }],
            ..ActionPart::default()
        }),
        ..master(
            4,
            (MasterKind::COMBAT, INFANTRY),
            40,
            [CostSlot::EMPTY; 3],
            Some(21),
        )
    }
}

/// The soldier, the galley, the castle and the veteran on a world of two players, each holding
/// 100 wood, gold and stone, the repair cost rates 270 and 271 at 0.5, a population of 2 within a
/// cap of 10 and a headroom of 5.
fn heal_world() -> World {
    let castle = master(
        82,
        (MasterKind::BUILDING, class::BUILDING),
        800,
        [
            paid(attribute::STONE, 100),
            CostSlot::new(attribute::HEADROOM, CostAmount::of_data(1), false),
            CostSlot::EMPTY,
        ],
        Some(40),
    );
    let veteran = master(
        5,
        (MasterKind::COMBAT, INFANTRY),
        60,
        [CostSlot::EMPTY; 3],
        None,
    );
    let at = |x| Position::new(x, 2.5, 0.0);
    let mut world = Scene::open(GridSize::new(12, 12))
        .players(2)
        .object(SceneObject::new(SOLDIER, PLAYER, soldier(), at(2.5)))
        .object(SceneObject::new(GALLEY, OTHER, galley(Some(60)), at(4.5)))
        .object(SceneObject::new(
            CASTLE,
            PLAYER,
            castle,
            Position::new(8.0, 8.0, 0.0),
        ))
        .object(SceneObject::new(VETERAN, PLAYER, veteran, at(6.5)))
        .build();
    for player in &mut world.players {
        player.attributes = vec![Amount::ZERO; 280];
        for resource in [attribute::WOOD, attribute::GOLD, attribute::STONE] {
            player.attributes[resource.index()] = Amount::new(100.0);
        }
        player.attributes[UNIT_REPAIR_COST] = Amount::new(0.5);
        player.attributes[BUILDING_REPAIR_COST] = Amount::new(0.5);
        player.attributes[attribute::POPULATION.index()] = Amount::new(2.0);
        player.attributes[attribute::POPULATION_CAP.index()] = Amount::new(10.0);
        player.attributes[attribute::HEADROOM.index()] = Amount::new(5.0);
    }
    world
}

/// Object `id` of `owner` at `hp` hit points heals by `amount` through `heal`
/// ([`World::heal`] or [`World::heal_building`]): the answer, its hit points after, and its
/// owner's wood, gold and stone.
fn healed(
    world: &mut World,
    (owner, id): (PlayerId, i32),
    hp: f32,
    amount: f32,
    heal: fn(&mut World, PlayerId, ListIndex, HitPoints) -> GenieResult<RepairOutcome>,
) -> (RepairOutcome, f32, [f32; 3]) {
    let index = world.live_index(owner, object_id(id)).unwrap();
    world.players[owner.index()].objects[index].life.hp = HitPoints::new(hp);
    let answer = heal(world, owner, index, HitPoints::new(amount)).unwrap();
    let held = |resource: AttributeId| world.players[owner.index()].attributes[resource.index()];
    (
        answer,
        world.players[owner.index()].objects[index].life.hp.get(),
        [attribute::WOOD, attribute::GOLD, attribute::STONE].map(|resource| held(resource).get()),
    )
}

/// A combat object of no ship's class heals by the amount for nothing (`FUN_141213b10` ->
/// `FUN_141193b30`): short of its master's hit points it answers 0, at them or past them it holds
/// exactly them and answers 1 (records/425906646's update 2191: a villager inside a town centre).
#[test]
fn combat_object_heals_by_its_amount_up_to_its_master_hit_points() {
    let mut world = heal_world();
    assert_eq!(
        healed(&mut world, (PLAYER, VETERAN), 11.0, 0.1, World::heal),
        (RepairOutcome::Healed, 11.0 + 0.1, [100.0; 3])
    );
    assert_eq!(
        healed(&mut world, (PLAYER, VETERAN), 59.95, 0.1, World::heal),
        (RepairOutcome::Full, 60.0, [100.0; 3])
    );
}

/// The heal's slot 0xb8 runs the hit point transforms' test at once (`FUN_141204450`): healed
/// above its transform's 20 hit points, the soldier takes the veteran's master.
#[test]
fn heal_runs_hit_point_transforms_test_at_once() {
    let mut world = heal_world();
    let master = |world: &World| {
        let index = world.live_index(PLAYER, object_id(SOLDIER)).unwrap();
        world.players[1].objects[index].life.master_id
    };
    let _ = healed(&mut world, (PLAYER, SOLDIER), 15.0, 2.0, World::heal);
    assert_eq!(master(&world), MasterId::named(4));
    let _ = healed(&mut world, (PLAYER, SOLDIER), 15.0, 10.0, World::heal);
    assert_eq!(master(&world), VETERAN_MASTER);
}

/// A ship's class heals at its owner's cost (`FUN_141213b10`): its master's hit points times the
/// amount over its train time, at most to its master's hit points, paid by its owner's
/// attribute 270 times them over its most of its costs; an owner who cannot pay heals nothing
/// (2). Without a train location the heal divides by 0: all it lacks, paid in full share.
#[test]
fn ship_heals_at_its_owner_cost_by_its_train_time() {
    let mut world = heal_world();
    let left = |share: f32| [100.0 - 90.0 * share, 100.0 - 30.0 * share, 100.0];
    assert_eq!(
        healed(&mut world, (OTHER, GALLEY), 60.0, 3.0, World::heal),
        (RepairOutcome::Healed, 66.0, left(0.5 * 6.0 / 120.0))
    );
    assert_eq!(world.players[1].attributes[1], Amount::new(100.0));
    let mut world = heal_world();
    assert_eq!(
        healed(&mut world, (OTHER, GALLEY), 118.0, 3.0, World::heal),
        (RepairOutcome::Full, 120.0, left(0.5 * 2.0 / 120.0))
    );
    let mut world = heal_world();
    world.players[2].attributes[1] = Amount::ZERO;
    assert_eq!(
        healed(&mut world, (OTHER, GALLEY), 60.0, 3.0, World::heal),
        (RepairOutcome::Unpaid, 60.0, [0.0, 100.0, 100.0])
    );
    let mut world = heal_world();
    for masters in &mut world.model.masters {
        *Arc::make_mut(masters)
            .get_mut(MasterId::named(539))
            .unwrap() = galley(None);
    }
    assert_eq!(
        healed(&mut world, (OTHER, GALLEY), 60.0, 3.0, World::heal),
        (RepairOutcome::Full, 120.0, left(0.5 * 60.0 / 120.0))
    );
}

/// A building heals at its owner's cost (`FUN_1411d33e0`): its master's hit points times the
/// amount over its build time, at most to its master's hit points (a NaN takes no less), paid by
/// its owner's attribute 271 times them over its master's hit points of its costs with the
/// population tested (`param_4` 0); an owner who cannot pay, or whose population is at its
/// cap, heals nothing (2).
#[test]
fn building_heals_at_its_owner_cost_with_population_tested() {
    let mut world = heal_world();
    let left = |share: f32| [100.0, 100.0, 100.0 - 100.0 * share];
    assert_eq!(
        healed(
            &mut world,
            (PLAYER, CASTLE),
            600.0,
            2.0,
            World::heal_building
        ),
        (RepairOutcome::Healed, 640.0, left(0.5 * 40.0 / 800.0))
    );
    let mut world = heal_world();
    assert_eq!(
        healed(
            &mut world,
            (PLAYER, CASTLE),
            790.0,
            2.0,
            World::heal_building
        ),
        (RepairOutcome::Full, 800.0, left(0.5 * 10.0 / 800.0))
    );
    let mut world = heal_world();
    let (answer, hp, held) = healed(
        &mut world,
        (PLAYER, CASTLE),
        f32::NAN,
        2.0,
        World::heal_building,
    );
    assert_eq!(
        (answer, held),
        (RepairOutcome::Healed, left(0.5 * 40.0 / 800.0))
    );
    assert!(hp.is_nan());
    let mut world = heal_world();
    world.players[1].attributes[2] = Amount::new(1.0);
    assert_eq!(
        healed(
            &mut world,
            (PLAYER, CASTLE),
            600.0,
            2.0,
            World::heal_building
        ),
        (RepairOutcome::Unpaid, 600.0, [100.0, 100.0, 1.0])
    );
    let mut world = heal_world();
    world.players[1].attributes[attribute::POPULATION_CAP.index()] = Amount::new(2.0);
    assert_eq!(
        healed(
            &mut world,
            (PLAYER, CASTLE),
            600.0,
            2.0,
            World::heal_building
        ),
        (RepairOutcome::Unpaid, 600.0, [100.0; 3])
    );
}
