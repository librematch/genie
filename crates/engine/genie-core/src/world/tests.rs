use super::*;
use crate::GenieError;
use crate::combat::attack::AttackState;
use crate::combat::damage::ArmorPair;
use crate::commands::GatherPointState;
use crate::commands::UnloadPick;
use crate::commands::game::GameCommand;
use crate::commands::{
    AllBackToWorkCommand, AttackMoveCommand, AutoScoutCommand, BuildCommand, BuildWallCommand,
    BuildingQueueCommand, Command, DeleteCommand, FlareCommand, GateCommand, GatherPoint,
    GatherPointCommand, GoBackToWork2Command, Order, OrderCommand, PatrolCommand, QueueCommand,
    ResearchCommand, ResignCommand, SeekShelterCommand, SetGatherStateCommand, StopCommand,
    TownBellCommand, TradeCommand, TransformObjectCommand, TributeCommand, UnitAiStateCommand,
    UnitList, UnitOrderAction, UnitOrderCommand, UnloadCommand, UnmodelledCommand, Waypoints,
};
use crate::construction::build::BuildState;
use crate::construction::build::{BuildAction, BuildTask, Construction};
use crate::construction::gates::gate_units;
use crate::construction::repair::RepairState;
use crate::gathering::closest_resource::search_origin;
use crate::gathering::gather::GatherState;
use crate::gathering::gather::unit::give_load;
use crate::gathering::hunt::HuntState;
use crate::gathering::make::MakeState;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Offset;
use crate::map::Point;
use crate::map::Position;
use crate::map::Radii;
use crate::map::RestrictionId;
use crate::map::TerrainMask;
use crate::map::Tile;
use crate::map::spatial::{Placed, Spatial};
use crate::math::Angle;
use crate::math::fmath::BVector;
use crate::math::rng::{Stream, Streams, Well512};
use crate::movement::Speed;
use crate::movement::TurnRate;
use crate::movement::ai_moves::PointMove;
use crate::movement::fly::Fly;
use crate::movement::fly::FlyState;
use crate::movement::move_to::MoveWaitState;
use crate::movement::move_to::{MoveTo, MoveToAction};
use crate::objects::HitPoints;
use crate::objects::HpPowerup;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::WorkRate;
use crate::objects::actions::ActionList;
use crate::objects::lifecycle::death_parts::{count_class, storage_leaves};
use crate::objects::lifecycle::{Class, Held, Lifecycle, MasterData};
use crate::objects::masters::AttackPriority;
use crate::objects::masters::ClassAmount;
use crate::objects::masters::ClassId;
use crate::objects::masters::GatherGroup;
use crate::objects::masters::ObstructionClass;
use crate::objects::masters::ObstructionType;
use crate::objects::masters::TaskId;
use crate::objects::masters::TaskSprites;
use crate::objects::masters::VolleySpread;
use crate::objects::masters::action_type;
use crate::objects::masters::class;
use crate::objects::masters::ids;
use crate::objects::masters::master_type::BUILDING;
use crate::objects::masters::{Master, Masters};
use crate::objects::object_list::ObjectList;
use crate::objects::release::{Releaser, released};
use crate::objects::sprite::Graphic;
use crate::objects::unit::{Unit, loses_actions};
use crate::ownership::artifact::Artifact;
use crate::ownership::artifact::ArtifactState;
use crate::ownership::change::change_hands;
use crate::players::Amount;
use crate::players::AmountRate;
use crate::players::CivId;
use crate::players::PlayerType;
use crate::players::Relation;
use crate::players::attribute;
use crate::players::attribute::AttributeId;
use crate::players::relation_row;
use crate::players::relations;
use crate::players::tracker::ListKey;
use crate::technology::ResearchState;
use crate::test_ids::attack_category;
use crate::test_ids::carrying;
use crate::test_ids::class_id;
use crate::test_ids::class_of;
use crate::test_ids::costs;
use crate::test_ids::group_id;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::test_ids::object_ids;
use crate::test_ids::opt_master;
use crate::test_ids::order_type;
use crate::test_ids::owner;
use crate::test_ids::player_id;
use crate::test_ids::tech_id;
use crate::test_ids::unit_list;
use crate::unit_ai::ActionKind;
use crate::unit_ai::AiCommand;
use crate::unit_ai::Attackers;
use crate::unit_ai::Issuer;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::OrderKind;
use crate::unit_ai::OrderTargetType;
use crate::unit_ai::UnitAiType;
use crate::unit_ai::UnitStance;
use crate::unit_ai::task_finder::world::{
    SideWorld, TaskWorld, task_search, task_target_with, wait_progress,
};
use crate::unit_ai::task_finder::{Actor, Diplomacy};
use crate::unit_ai::world_view::{DropWorld, Foreign, Wait, patrol_information};
use crate::unit_ai::{Effect, Notify, UnitAi};
use crate::vision::registry::UnitUpdate;
use crate::vision::visibility::{Seer, capture_kind};
use crate::world::checkpoint::TopAction;
use crate::world::others::OthersRef;
use crate::world::player::Player;
use ids::FISH_TRAP;

/// Gives every player a default master for each master id its units name (the table the game
/// always has), the ones the test set already has kept.
pub(crate) fn stock_masters(t: &mut World) {
    let tables: Vec<Masters> = t
        .players
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let held = t.masters.get(i);
            let mut all: Vec<Master> = held
                .map(|m| m.iter().cloned().collect())
                .unwrap_or_default();
            for u in p.objects.iter().chain(&p.sleeping) {
                if !all.iter().any(|m| m.id == u.life.master_id) {
                    all.push(Master {
                        id: u.life.master_id,
                        ..Master::default()
                    });
                }
            }
            Masters::new(all)
        })
        .collect();
    t.masters = tables;
}

fn player(objects: Vec<Option<UnitAi>>) -> Player {
    Player {
        objects: objects
            .into_iter()
            .map(|ai| Unit {
                ai,
                ..Unit::default()
            })
            .collect(),
        ..Player::default()
    }
}

fn building() -> UnitAi {
    UnitAi {
        ai_type: UnitAiType::BUILDING,
        current_order: None,
        current_action: None,
        current_target: None,
        adjusted_idle_timeout: Millis::new(3000),
        look_around_timeout: Millis::new(1000),
        retarget_timer: Millis::new(1),
        ..UnitAi::default()
    }
}

#[test]
fn the_turn_rotates_and_only_the_turn_player_updates() {
    let mut t = World::new(
        WorldTime::ZERO,
        29910,
        PlayerId::new(1),
        vec![Millis::ZERO, Millis::ZERO, Millis::ZERO],
        vec![
            player(vec![Some(building())]),
            player(vec![Some(building())]),
            player(vec![]),
        ],
    );
    stock_masters(&mut t);
    t.tick(Millis::new(10));
    assert_eq!(t.player_turn, PlayerId::new(2));
    assert_eq!(t.player_time, [10, 0, 10].map(Millis::new));
    assert_eq!(
        t.players[0].objects[ListIndex::new(0)]
            .ai
            .as_ref()
            .unwrap()
            .last_retarget_time,
        WorldTime::ZERO
    );
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .ai
            .as_ref()
            .unwrap()
            .last_retarget_time,
        WorldTime::from_millis(10)
    );
    stock_masters(&mut t);
    t.tick(Millis::new(10));
    stock_masters(&mut t);
    t.tick(Millis::new(10));
    // turns 2 (no objects), then 0; back at player 1
    assert_eq!(t.player_turn, PlayerId::new(1));
    assert_eq!(t.player_time, [0, 20, 10].map(Millis::new));
    assert_eq!(
        t.players[0].objects[ListIndex::new(0)]
            .ai
            .as_ref()
            .unwrap()
            .last_retarget_time,
        WorldTime::from_millis(30)
    );
}

#[test]
fn the_frame_seed_chains_through_two_world_draws() {
    let mut t = World::new(
        WorldTime::ZERO,
        29910,
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![player(vec![])],
    );
    t.tick(Millis::new(10));
    // SeedAll(29910); s1 = World.rand15 = 19550 (see rng); SeedAll(19550); World.rand15
    let mut s = Streams::default();
    s.seed_all(19550);
    assert_eq!(t.frame_seed, s.get(Stream::World).rand15());
}

#[test]
fn artifacts_of_every_player_start_updating_in_the_second_tick() {
    let animal = |gaia| Unit {
        actions: ActionList::default().with_artifact(Some(Artifact::new(gaia, None))),
        ..Unit::default()
    };
    let players = vec![
        Player {
            objects: vec![animal(true)].into(),
            ..Player::default()
        },
        Player {
            objects: vec![animal(false)].into(),
            ..Player::default()
        },
    ];
    let mut t = World::new(
        WorldTime::ZERO,
        1,
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        players,
    );
    let state = |t: &World, p: usize| {
        t.players[p].objects[ListIndex::new(0)]
            .actions
            .artifact()
            .copied()
            .unwrap()
            .state
    };
    stock_masters(&mut t);
    t.tick(Millis::new(50));
    assert_eq!(
        (state(&t, 0), state(&t, 1)),
        (ArtifactState::Check, ArtifactState::Check)
    );
    stock_masters(&mut t);
    t.tick(Millis::new(50));
    assert_eq!(
        (state(&t, 0), state(&t, 1)),
        (ArtifactState::Wait, ArtifactState::Wait)
    );
}

#[test]
fn an_unsupported_unit_stops_and_is_recorded() {
    let mut odd = building();
    // a unit-AI type the slice does not model
    odd.ai_type = UnitAiType::of_value(99);
    let mut t = World::new(
        WorldTime::ZERO,
        1,
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![player(vec![Some(odd)])],
    );
    stock_masters(&mut t);
    t.tick(Millis::new(10));
    stock_masters(&mut t);
    t.tick(Millis::new(10));
    assert_eq!(
        t.players[0].unsupported,
        Some((
            1,
            ListIndex::new(0),
            GenieError::unsupported("unit-AI type not modelled")
        ))
    );
}

fn dead_building() -> Unit {
    Unit {
        id: object_id(1),
        ai: Some(building()),
        life: Lifecycle {
            class: Class::Animated,
            hp: HitPoints::ZERO,
            master: Some(MasterData::default()),
            ..Lifecycle::default()
        },
        ..Unit::default()
    }
}

/// A live building whose retarget timer falls due in the fourth tick of 10 ms (world time 40).
fn live(id: i32) -> Unit {
    Unit {
        id: object_id(id),
        ai: Some(UnitAi {
            retarget_timer: Millis::new(40),
            ..building()
        }),
        ..Unit::default()
    }
}

fn ids(t: &World) -> Vec<i32> {
    t.players[0].objects.iter().map(|o| o.id.get()).collect()
}

fn retarget(t: &World) -> Vec<u32> {
    t.players[0]
        .objects
        .iter()
        .map(|o| o.ai.as_ref().unwrap().retarget_timer.get())
        .collect()
}

#[test]
fn a_removed_object_is_replaced_by_the_last_one_which_updates_in_the_same_tick() {
    // RGE_Object_List::update: the dead building leaves in its fourth update; the last object
    // takes its slot and updates there, so it draws before the object behind the slot
    let mut t = World::new(
        WorldTime::ZERO,
        29910,
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![Player {
            objects: vec![dead_building(), live(2), live(3)].into(),
            ..Player::default()
        }],
    );
    for _ in 0..4 {
        stock_masters(&mut t);
        t.tick(Millis::new(10));
    }
    assert_eq!(ids(&t), [3, 2]);
    // the same ticks with the two live buildings in that order from the start: their timers
    // fall due in the fourth tick, so its draws depend only on the order
    let mut reordered = World::new(
        WorldTime::ZERO,
        29910,
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![Player {
            objects: vec![live(3), live(2)].into(),
            ..Player::default()
        }],
    );
    for _ in 0..4 {
        stock_masters(&mut reordered);
        reordered.tick(Millis::new(10));
    }
    assert_eq!(retarget(&t), retarget(&reordered));
}

#[test]
fn a_dead_objects_unit_ai_draws_nothing() {
    let one = |objects| {
        let mut t = World::new(
            WorldTime::ZERO,
            29910,
            PlayerId::new(0),
            vec![Millis::ZERO],
            vec![Player {
                objects,
                ..Player::default()
            }],
        );
        stock_masters(&mut t);
        t.tick(Millis::new(10));
        t
    };
    let with_dead = one(vec![dead_building(), live(2)].into());
    let alone = one(vec![live(2)].into());
    assert_eq!(
        with_dead.players[0].objects[ListIndex::new(0)].life.state,
        ObjectState::Dead
    );
    // its full update adds the world's dt to the time since the last enemy sighting, and
    // returns before anything else
    let dead_ai = dead_building().ai.map(|ai| UnitAi {
        time_since_enemy: ai.time_since_enemy + Seconds::new(0.01),
        ..ai
    });
    assert_eq!(
        with_dead.players[0].objects[ListIndex::new(0)].ai,
        dead_ai,
        "the dead building's timers stay"
    );
    assert_eq!(
        with_dead.players[0].objects[ListIndex::new(1)].ai,
        alone.players[0].objects[ListIndex::new(0)].ai
    );
}

/// The masters of MarvinBlue/426048731's player 2 that its BUILD at tick 679 uses, shortened
/// from build 158041's save and `.dat`.
pub(crate) fn builder_masters() -> Masters {
    use crate::objects::masters::{Master, Masters, Task};
    let task = |action_type, object_group| Task {
        action_type,
        object_group,
        ..Task::default()
    };
    let villager = |id, group, tasks: Vec<Task>| Master {
        id,
        terrain_restriction: Some(RestrictionId::named(0)),
        kind: master_type::COMBAT,
        class: class::VILLAGER,
        task_by_group: group,
        radius: Footprint::new(0.2, 0.2),
        tall: true,
        obstruction_type: ObstructionType::NONE,
        buildable_on: 0,
        tasks,
        cost: costs(&[]),
        obstruction_class: ObstructionClass::NONE,
        object_flags: 0,
        combat_level: 0,
        work_rate: WorkRate::new(1.0),
        hit_points: 25,
        build_time: 0,
        // a villager moves (its master's slot 0xe8), which BUILD's last loop asks
        moving: Some(crate::movement::MoveStats {
            fly_mode: 0,
            speed: Speed::new(0.8),
            base: Speed::new(0.8),
            turn_rate: TurnRate::MAX,
            turn_radius: Distance::ZERO,
            turn_radius_speed: Speed::MAX,
            radius: Radii::new(0.2, 0.2, 0.5),
            restriction: RestrictionId::named(7),
            restriction_mask: TerrainMask::of_bits(6),
            turn_time: Seconds::ZERO,
            move_algorithm: 0,
            min_collision: 1.0,
            fast_turn: false,
            facets: None,
            weapon_range: Distance::ZERO,
        }),
        // no sprites (the world's sprites' table is empty)
        standing_sprite: None,
        move_sprite: None,
        run_sprite: None,
        ..Master::default()
    };
    let base = || {
        vec![
            task(action_type::COMBAT, None),
            task(crate::garrison::ENTER, Some(class_id(3))),
        ]
    };
    let builder = || {
        vec![
            task(action_type::BUILD, None),
            task(crate::garrison::ENTER, Some(class_id(3))),
        ]
    };
    Masters::new([
        villager(VILLAGER_MALE, 1, base()),
        villager(ids::BUILDER, 1, builder()),
        villager(master_id(212), 2, builder()),
        villager(VILLAGER_FEMALE, 2, base()),
        Master {
            id: ids::HOUSE,
            terrain_restriction: Some(RestrictionId::named(0)),
            kind: BUILDING,
            class: class::BUILDING,
            task_by_group: 0,
            radius: Footprint::new(1.0, 1.0),
            tall: true,
            obstruction_type: ObstructionType::NONE,
            buildable_on: 0,
            tasks: vec![],
            cost: costs(&[(attribute::WOOD, 25, true)]),
            obstruction_class: ObstructionClass::UNIT,
            object_flags: 0,
            combat_level: 0,
            work_rate: WorkRate::ZERO,
            hit_points: 550,
            build_time: 25,
            // a house's clearance, which BUILD's placement test reads
            clearance: Footprint::new(1.0, 1.0),
            // built by the villagers (the builders' test, FUN_141262270)
            trained_at: vec![ids::BUILDER],
            // a house's building part: no annexes, no stack unit, not an annex; its destruction
            // sprite (392 in build 158041's .dat), which it dies on unbuilt
            building: Some(crate::objects::masters::BuildingMaster {
                stack_unit: None,
                foundation_terrain: -1,
                tech: None,
                annexes: [None; 4],
                head_unit: None,
                transform_unit: None,
                destruction_sprite: Some(SpriteId::named(392)),
                ..crate::objects::masters::BuildingMaster::default()
            }),
            // its dying sprite (2196), which it dies on built
            death_sprite: Some(SpriteId::named(2196)),
            ..Master::default()
        },
    ])
}

/// A flat map of terrain 0 that the first terrain restriction lets anything stand on, every
/// tile seen by every player: BUILD's placement test passes anywhere free.
pub(crate) fn placeable(t: &mut World) {
    placeable_on(t, GridSize::new(120, 120));
}

/// [`placeable`]'s terrain and sight on a map of `w` × `h` tiles.
pub(crate) fn placeable_on(t: &mut World, map: GridSize) {
    let (w, h) = (map.width(), map.height());
    let n = (w * h) as usize;
    t.spatial.map = GridSize::new(w, h);
    let mut terrain = crate::map::terrain::Terrain::new(
        GridSize::new(w, h),
        vec![(0, 255, -1); n],
        vec![
            crate::map::terrain::TerrainKind {
                shown: true,
                flags: 0,
                forest: false,
            };
            1
        ],
        vec![vec![1.0]],
    );
    terrain.set_elevations(vec![0; n]);
    t.spatial.terrain = Some(terrain);
    let players = t.players.len();
    let seen = (0..players).fold(0u64, |b, p| b | (1 << p) | (1 << (32 + p)));
    t.spatial.sight = Some(crate::vision::visibility::Visibility::new(
        GridSize::new(w, h),
        (0..players)
            .map(|_| crate::vision::visibility::VisibleMap {
                counts: vec![0; n],
                explored: 0,
            })
            .collect(),
        vec![seen; n],
        Vec::new(),
        0,
        vec![false; players],
    ));
}

/// `FUN_140c42080` after a notify of a new action: a unit AI with a target the world holds
/// remembers the order with the world's time, both places as slot 0x520 gives them (a
/// [`BVector`]; these hold no movement state) and the target's attack category by its master
/// (an archer, class 0: 3); one whose target is gone, or on a point, remembers none.
///
/// [`BVector`]: crate::math::fmath::BVector
#[test]
fn an_order_on_a_target_joins_the_unit_ai_s_history() {
    use crate::math::fmath::BVector;
    let master = |id, class| Master {
        id,
        kind: master_type::COMBAT,
        class,
        ..Master::default()
    };
    let unit = |id, position, target| Unit {
        id,
        body: crate::objects::layers::Body::at(position),
        ai: Some(UnitAi {
            current_target: target,
            ..UnitAi::default()
        }),
        life: Lifecycle {
            master_id: if id == object_id(1) {
                master_id(74)
            } else {
                master_id(4)
            },
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let player = |objects| Player {
        objects,
        attributes: vec![Amount::ZERO; 300],
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        1,
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO, Millis::ZERO],
        vec![
            player(vec![].into()),
            player(
                vec![
                    unit(
                        object_id(1),
                        Position::new(10.0, 11.0, 0.0),
                        Some(object_id(2)),
                    ),
                    unit(
                        object_id(3),
                        Position::new(1.0, 1.0, 0.0),
                        Some(object_id(99)),
                    ),
                ]
                .into(),
            ),
            player(vec![unit(object_id(2), Position::new(20.0, 21.0, 1.0), None)].into()),
        ],
    );
    let table = Masters::new([
        master(master_id(74), class::CLASS_6),
        master(master_id(4), class::CLASS_0),
    ]);
    t.masters = vec![table.clone(), table.clone(), table];
    t.world_time = WorldTime::from_millis(4321);
    t.remember_order(
        PlayerId::new(1),
        ListIndex::new(0),
        (Some(OrderKind::Attack), Some(ActionKind::Attack)),
    )
    .unwrap();
    t.remember_order(
        PlayerId::new(1),
        ListIndex::new(1),
        (Some(OrderKind::Attack), Some(ActionKind::Attack)),
    )
    .unwrap();
    let history = |i: ListIndex| {
        t.players[1].objects[i]
            .ai
            .as_ref()
            .unwrap()
            .order_history
            .clone()
    };
    assert_eq!(
        history(ListIndex::new(0)),
        [crate::unit_ai::OrderRecord {
            order: Some(OrderKind::Attack),
            action: Some(ActionKind::Attack),
            time: WorldTime::from_millis(4321),
            position: BVector::new(10.0, 0.0, 11.0),
            target: Some(object_id(2)),
            category: attack_category(3),
            target_position: BVector::new(20.0, 1.0, 21.0),
        }]
    );
    assert_eq!(history(ListIndex::new(1)), []);
}

/// FLARE (`0x140e7bfd0`) at a point without a target: each player's flare comes at the point
/// with height 0, which the static `new_position` (slot 0x58, slot 0x60 `FUN_141193a10`) puts
/// on the ground, as 426048706's flare of update 2486 stands at 4 on raised ground.
#[test]
fn a_flare_at_a_point_stands_on_the_ground() {
    let player = || Player {
        attributes: vec![Amount::ZERO; 300],
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        1,
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO, Millis::ZERO],
        vec![player(), player(), player()],
    );
    placeable(&mut t);
    if let Some(terrain) = t.spatial.terrain.as_mut() {
        terrain.set_elevations(vec![4; 120 * 120]);
    }
    let flare = Master {
        id: master_id(274),
        kind: master_type::ANIMATED,
        standing_sprite: None,
        ..Master::default()
    };
    t.masters = (0..3).map(|_| Masters::new([flare.clone()])).collect();
    t.next_object_id = object_id(100);
    t.flare(None, Point::new(19.114_586, 29.197_92), 3).unwrap();
    for p in 1..3 {
        let at: Vec<Position> = t.players[p].objects.iter().map(Unit::position).collect();
        assert_eq!(at, [Position::new(19.114_586, 29.197_92, 4.0)]);
    }
}

/// A hit on player 2's object makes player 2 a flare (master 112) at the attacker's place
/// (`FUN_14120c0a0`'s slot 0x1c0), whose amount (`+0x310`, its time) a trebuchet's hit doubles.
#[test]
fn a_hit_s_flare_lasts_twice_as_long_from_a_trebuchet() {
    let player = || Player {
        attributes: vec![Amount::ZERO; 300],
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        1,
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO, Millis::ZERO],
        vec![player(), player(), player()],
    );
    placeable(&mut t);
    let flare = Master {
        id: master_id(112),
        kind: master_type::ANIMATED,
        standing_sprite: None,
        storage: vec![(Some(AttributeId::numbered(12)), Amount::new(1.0), 0)],
        ..Master::default()
    };
    t.masters = (0..3).map(|_| Masters::new([flare.clone()])).collect();
    t.next_object_id = object_id(100);
    t.make_flare(PlayerId::new(2), Position::new(10.5, 12.5, 0.0), false)
        .unwrap();
    t.make_flare(PlayerId::new(2), Position::new(10.5, 12.5, 0.0), true)
        .unwrap();
    let flares: Vec<(i16, f32)> = t.players[2]
        .objects
        .iter()
        .map(|u| (u.life.master_id.get(), u.life.amount.get()))
        .collect();
    assert_eq!(flares, [(112, 1.0), (112, 2.0)]);
}

/// Player 1 with 300 attributes, `livestock` of them at a pasture 50 (master 1897, radius 2) at
/// `at` and 300 food, on a 120-tile map, its masters `livestock_masters` of the two livestock
/// (moving, one hit point, a turning idle sprite of 16 facets with a random first one, 100 food
/// held).
fn pasture_game(at: Position, livestock: f32, livestock_masters: &[MasterId]) -> World {
    use crate::objects::masters::master_type::{BUILDING, MOVING};
    use crate::objects::sprite::Graphic;
    let player = || Player {
        attributes: vec![Amount::ZERO; 300],
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        1,
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![player(), player()],
    );
    t.spatial = Spatial::new(GridSize::new(120, 120));
    placeable(&mut t);
    t.players[1].attributes[289] = Amount::new(livestock);
    t.players[1].attributes[288] = Amount::new(300.0);
    let idle = |id| Graphic {
        id,
        frames: 60,
        facets: 16,
        speed_multiplier: 0.0,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: 7,
    };
    t.graphics = std::collections::BTreeMap::from([
        (SpriteId::named(13124), idle(SpriteId::named(13124))),
        (SpriteId::named(13170), idle(SpriteId::named(13170))),
    ]);
    let pasture = Master {
        id: ids::PASTURE,
        kind: BUILDING,
        class: class::FARM,
        radius: Footprint::new(2.0, 2.0),
        ..Master::default()
    };
    let animal = |id: MasterId, standing_sprite| Master {
        id,
        kind: MOVING,
        class: class::FARM,
        hit_points: 1,
        radius: Footprint::new(0.2, 0.2),
        standing_sprite,
        storage: vec![(Some(attribute::FOOD), Amount::new(100.0), 0)],
        moving: Some(crate::gathering::hunt::tests::walker().stats),
        ..Master::default()
    };
    let mut masters = vec![pasture];
    for &m in livestock_masters {
        masters.push(animal(
            m,
            if m.get() == 1899 {
                Some(SpriteId::named(13124))
            } else {
                Some(SpriteId::named(13170))
            },
        ));
    }
    t.masters = (0..2).map(|_| Masters::new(masters.clone())).collect();
    t.players[1].objects.push(Unit {
        id: object_id(50),
        body: crate::objects::layers::Body::at(at),
        life: Lifecycle {
            master_id: ids::PASTURE,
            ..Lifecycle::default()
        },
        pasture: None,
        my_pasture: None,
        ..Unit::default()
    });
    t.next_object_id = object_id(100);
    t
}

/// `FUN_1411c8410` (429381190's pasture 15342 of update 1385 made three): per animal two draws of
/// the Object stream pick its master and its point along the edges, a point on a tile that holds
/// one of the pasture's livestock goes and another is drawn; the new object's setups draw its
/// facet and its angle, then the routine its first facet. Each is the pasture's, holds its share
/// of the food, no unit targets it, and the pasture lists it.
#[test]
fn a_completed_pasture_puts_its_livestock_on_its_edges() {
    use crate::gathering::pasture::{LIVESTOCK, edge_points};
    let mut t = pasture_game(
        Position::new(20.0, 20.0, 0.0),
        3.0,
        &[master_id(1899), master_id(1900)],
    );
    t.streams.get(Stream::Object).set_seed(1385);
    let mut e = t.streams.get(Stream::Object).clone();
    let mut points = edge_points(Position::new(20.0, 20.0, 0.0), Distance::new(2.0));
    let mut tiles = Vec::new();
    let mut expected = Vec::new();
    for _ in 0..3 {
        let kind = LIVESTOCK[e.rand15() as usize % 2];
        let mut i = e.rand15() as usize % points.len();
        let tile = |p: Point| (p.x() as i32, p.y() as i32);
        while tiles.contains(&tile(points[i])) {
            let _ = points.remove(i);
            i = e.rand15() as usize % points.len();
        }
        tiles.push(tile(points[i]));
        expected.push((kind, points.remove(i)));
        // the static setup's facet, the moving setup's angle, the routine's facet
        for _ in 0..3 {
            let _ = e.rand15();
        }
    }
    t.setup_pasture(PlayerId::new(1), object_id(50)).unwrap();
    let made: Vec<(MasterId, Point)> = t.players[1].objects.as_slice()[1..]
        .iter()
        .map(|u| (u.life.master_id, u.position().ground()))
        .collect();
    assert_eq!(made, expected);
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)].livestock,
        [object_id(100), object_id(101), object_id(102)]
    );
    for u in &t.players[1].objects.as_slice()[1..] {
        assert_eq!(
            (
                u.pasture,
                u.life.amount,
                u.life.attribute_type,
                u.untargetable
            ),
            (
                Some(object_id(50)),
                Amount::new(100.0),
                Some(attribute::FOOD),
                true
            )
        );
        assert_eq!(u.life.state, ObjectState::Alive);
        assert!(u.sprite.is_some() && u.body.mover().is_some());
    }
    assert_eq!(*t.streams.get(Stream::Object), e);
}

/// One livestock a tile: a pasture at (20, 20) has twelve tiles under its sixteen points, so
/// sixteen asked give twelve and the points run out; at the map's left edge the points off it
/// (x below 0 after CVTTSS2SI: −1.5, not −0.6) go too, leaving six tiles.
#[test]
fn a_pasture_puts_one_animal_a_tile_on_the_map() {
    let mut t = pasture_game(
        Position::new(20.0, 20.0, 0.0),
        16.0,
        &[master_id(1899), master_id(1900)],
    );
    t.setup_pasture(PlayerId::new(1), object_id(50)).unwrap();
    let tiles = |t: &World| {
        let mut v: Vec<(i32, i32)> = t.players[1].objects.as_slice()[1..]
            .iter()
            .map(|u| (u.position().x() as i32, u.position().y() as i32))
            .collect();
        v.sort_unstable();
        v
    };
    let mut twelve = tiles(&t);
    assert_eq!(twelve.len(), 12);
    twelve.dedup();
    assert_eq!(twelve.len(), 12);
    let mut t = pasture_game(
        Position::new(0.0, 10.0, 0.0),
        16.0,
        &[master_id(1899), master_id(1900)],
    );
    t.setup_pasture(PlayerId::new(1), object_id(50)).unwrap();
    assert_eq!(
        tiles(&t),
        [(0, 8), (0, 11), (1, 8), (1, 9), (1, 10), (1, 11)]
    );
}

/// A master the owner has not got makes nothing but counts as an attempt, its point staying; no
/// livestock asked draws nothing; the food is the pasture's over the count (DIVSS).
#[test]
fn a_pasture_counts_each_attempt_and_shares_its_food() {
    use crate::gathering::pasture::{LIVESTOCK, edge_points};
    let mut t = pasture_game(Position::new(20.0, 20.0, 0.0), 3.0, &[master_id(1899)]);
    t.streams.get(Stream::Object).set_seed(7);
    let mut e = t.streams.get(Stream::Object).clone();
    let mut points = edge_points(Position::new(20.0, 20.0, 0.0), Distance::new(2.0));
    let mut expected = Vec::new();
    for _ in 0..3 {
        let kind = LIVESTOCK[e.rand15() as usize % 2];
        let i = e.rand15() as usize % points.len();
        let tile = |p: Point| (p.x() as i32, p.y() as i32);
        assert!(
            !expected.iter().any(|&q| tile(q) == tile(points[i])),
            "the seed's points"
        );
        if kind == master_id(1899) {
            expected.push(points.remove(i));
            for _ in 0..3 {
                let _ = e.rand15();
            }
        }
    }
    assert!(expected.len() < 3, "the seed draws an Ibex");
    t.setup_pasture(PlayerId::new(1), object_id(50)).unwrap();
    let made: Vec<Point> = t.players[1].objects.as_slice()[1..]
        .iter()
        .map(|u| u.position().ground())
        .collect();
    assert_eq!(made, expected);
    assert_eq!(*t.streams.get(Stream::Object), e);
    let mut t = pasture_game(
        Position::new(20.0, 20.0, 0.0),
        0.0,
        &[master_id(1899), master_id(1900)],
    );
    t.setup_pasture(PlayerId::new(1), object_id(50)).unwrap();
    assert_eq!(
        (
            t.players[1].objects.len(),
            t.streams.get(Stream::Object).draws
        ),
        (1, 0)
    );
    let mut t = pasture_game(
        Position::new(20.0, 20.0, 0.0),
        7.0,
        &[master_id(1899), master_id(1900)],
    );
    t.setup_pasture(PlayerId::new(1), object_id(50)).unwrap();
    let food = 300.0f32 / 7.0;
    assert!(
        t.players[1].objects.as_slice()[1..]
            .iter()
            .all(|u| u.life.amount == Amount::new(food))
    );
}

/// A moving object of type 30 that dies (429381190's Ibex 15371 in update 1426) shows its
/// master's dying sprite (slot 0x7a0, `FUN_1411887e0`, through slot 0x68) and goes on to state 4.
/// Its top speed (slot 65, `FUN_14114d1d0`) follows the sprite: the master's speed, times the
/// sprite's speed factor above 0.
#[test]
fn a_dying_livestock_shows_its_dying_sprite() {
    for (factor, speed) in [(0.0, 0.737), (2.0, 1.474)] {
        let u = dying_animal(master_type::MOVING, factor);
        assert_eq!(u.sprite.map(|s| s.graphic.id), Some(SpriteId::named(13171)));
        assert_eq!(u.life.state, ObjectState::DyingSprite);
        assert_eq!(
            u.body.mover().map(|m| m.stats.speed),
            Some(Speed::new(speed))
        );
    }
}

/// A livestock of [`pasture_game`] of master type `kind` (speed 0.737, the dying sprite 13171 of
/// speed factor `factor`) running at 1.4003 when it dies, two updates later (the deer 9655 of
/// MarvinBlue/426048731, a combat object, likewise in update 3147: the Gaia pin of
/// `oracle_snapshots`).
fn dying_animal(kind: MasterKind, factor: f32) -> Unit {
    let mut t = pasture_game(
        Position::new(20.0, 20.0, 0.0),
        1.0,
        &[master_id(1899), master_id(1900)],
    );
    let dying = Graphic {
        id: SpriteId::named(13171),
        frames: 10,
        facets: 16,
        speed_multiplier: factor,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: 7,
    };
    let _ = t.graphics.insert(SpriteId::named(13171), dying);
    for m in &mut t.masters {
        let mut all: Vec<Master> = m.iter().cloned().collect();
        for a in all.iter_mut().filter(|a| a.kind == master_type::MOVING) {
            a.kind = kind;
            a.death_sprite = Some(SpriteId::named(13171));
            a.max_speed = Speed::new(0.737);
        }
        *m = Masters::new(all);
    }
    t.setup_pasture(PlayerId::new(1), object_id(50)).unwrap();
    let animal = t.players[1].objects[ListIndex::new(1)].id;
    t.players[1].objects[ListIndex::new(1)].life.hp = HitPoints::ZERO;
    if let Some(m) = t.players[1].objects[ListIndex::new(1)].body.mover_mut() {
        m.stats.base = Speed::new(0.737);
        m.stats.speed = Speed::new(1.4003);
    }
    for _ in 0..2 {
        t.tick(Millis::new(50));
    }
    t.object(animal).unwrap().1.clone()
}

/// [`pasture_game`] with its three livestock made and player 1's villager 60 (class 4, radius
/// 0.2) at `at`, the owner's [`crate::gathering::pasture::PASTURE_WORKERS`] `room`.
fn tended_pasture(at: Position, room: f32) -> World {
    let mut t = pasture_game(
        Position::new(20.0, 20.0, 0.0),
        3.0,
        &[master_id(1899), master_id(1900)],
    );
    t.setup_pasture(PlayerId::new(1), object_id(50)).unwrap();
    t.players[1].attributes[290] = Amount::new(room);
    let villager = Master {
        id: VILLAGER_MALE,
        kind: master_type::COMBAT,
        class: class::VILLAGER,
        radius: Footprint::new(0.2, 0.2),
        ..Master::default()
    };
    for m in &mut t.masters {
        let mut all: Vec<Master> = m.iter().cloned().collect();
        all.push(villager.clone());
        *m = Masters::new(all);
    }
    t.players[1].objects.push(Unit {
        id: object_id(60),
        body: crate::objects::layers::Body::at(at),
        life: Lifecycle {
            master_id: VILLAGER_MALE,
            ..Lifecycle::default()
        },
        ai: Some(UnitAi::default()),
        pasture: None,
        my_pasture: None,
        ..Unit::default()
    });
    t
}

/// `FUN_1411c7670`: the pasture's livestock sorted by their footprints' gap to the villager;
/// the first without workers, else the first whose workers are below the owner's attribute
/// 290, else none; a unit of another player gets none. Slot 0x780 sums the livestock's workers.
#[test]
fn a_villager_tends_the_nearest_livestock_with_room() {
    let livestock = |t: &World| t.players[1].objects[ListIndex::new(0)].livestock.clone();
    let place = |t: &World, id: ObjectId| t.object(id).unwrap().1.position();
    let t = tended_pasture(Position::new(20.0, 20.0, 0.0), 2.0);
    let ids = livestock(&t);
    let near = place(&t, ids[0]);
    let mut t = tended_pasture(near.with_z(0.0), 2.0);
    let villager = t.players[1].objects.last_place().unwrap();
    assert_eq!(
        t.pasture_pick(object_id(50), PlayerId::new(1), villager),
        Ok(Some(ids[0]))
    );
    let mut gaps: Vec<(f32, ObjectId)> = ids
        .iter()
        .map(|&id| {
            let p = place(&t, id);
            let g = crate::movement::footprint_gap_squared(
                p,
                Footprint::new(0.2, 0.2),
                near,
                Footprint::new(0.2, 0.2),
            );
            (g.get(), id)
        })
        .collect();
    gaps.sort_by(|a, b| a.0.total_cmp(&b.0));
    let by_gap: Vec<ObjectId> = gaps.iter().map(|g| g.1).collect();
    let worked = |t: &mut World, id: ObjectId, n: i32| t.object_mut(id).unwrap().1.workers = n;
    worked(&mut t, by_gap[0], 1);
    assert_eq!(
        t.pasture_pick(object_id(50), PlayerId::new(1), villager),
        Ok(Some(by_gap[1]))
    );
    worked(&mut t, by_gap[1], 1);
    worked(&mut t, by_gap[2], 1);
    assert_eq!(t.livestock_workers(object_id(50)), 3);
    // all worked: the nearest below the room
    assert_eq!(
        t.pasture_pick(object_id(50), PlayerId::new(1), villager),
        Ok(Some(by_gap[0]))
    );
    worked(&mut t, by_gap[0], 2);
    assert_eq!(
        t.pasture_pick(object_id(50), PlayerId::new(1), villager),
        Ok(Some(by_gap[1]))
    );
    for &id in &by_gap {
        worked(&mut t, id, 2);
    }
    assert_eq!(
        t.pasture_pick(object_id(50), PlayerId::new(1), villager),
        Ok(None)
    );
    let mut other = tended_pasture(near, 2.0);
    let v = other.players[1].objects.pop().unwrap();
    other.players.push(Player {
        attributes: vec![Amount::ZERO; 300],
        objects: vec![v].into(),
        ..Player::default()
    });
    other.masters.push(other.masters[1].clone());
    assert_eq!(
        other.pasture_pick(object_id(50), PlayerId::new(2), ListIndex::new(0)),
        Ok(None)
    );
}

/// Slot 0x7e8 of a villager on a pasture: with room (the livestock's workers below attribute
/// 290) it works the livestock `FUN_1411c7670` picks, at its place, and tends the pasture; a
/// full one has its unit AI take a search (0x202 with the hunt action, class 49, −1) and it
/// tends none.
#[test]
fn a_villager_ordered_on_a_pasture_works_its_livestock() {
    let order = |target| crate::orders::Work {
        target,
        point: Position::new(20.0, 20.0, 0.0),
        carry_test: true,
        filter: None,
        notified: false,
    };
    let mut t = tended_pasture(Position::new(18.0, 18.0, 0.0), 2.0);
    let v = t.players[1].objects.last_place().unwrap();
    t.order_work(PlayerId::new(1), v, &order(Some(object_id(50))))
        .unwrap();
    assert_eq!(t.players[1].objects[v].my_pasture, Some(object_id(50)));
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)].pasture_workers,
        [object_id(60)]
    );
    let mut full = tended_pasture(Position::new(18.0, 18.0, 0.0), 0.0);
    let v = full.players[1].objects.last_place().unwrap();
    full.players[1].objects[v].my_pasture = Some(object_id(50));
    full.players[1].objects[ListIndex::new(0)].pasture_workers = vec![object_id(60)];
    full.order_work(PlayerId::new(1), v, &order(Some(object_id(50))))
        .unwrap();
    let u = &full.players[1].objects[v];
    assert_eq!(
        u.ai.as_ref().unwrap().notifies,
        [Notify::own(
            object_id(60),
            NotifyKind::REQUIRES_SEARCH,
            [0x265, 49, -1]
        )]
    );
    assert_eq!(u.my_pasture, None);
    assert_eq!(
        full.players[1].objects[ListIndex::new(0)].pasture_workers,
        Vec::new()
    );
}

/// Slot 0x7e8 of a unit in state 3 or later ignores the order: nothing of the pasture's or the
/// unit's changes.
#[test]
fn a_unit_past_state_2_ignores_a_work_order() {
    let order = crate::orders::Work {
        target: Some(object_id(50)),
        point: Position::new(20.0, 20.0, 0.0),
        carry_test: true,
        filter: None,
        notified: false,
    };
    let mut t = tended_pasture(Position::new(18.0, 18.0, 0.0), 2.0);
    let v = t.players[1].objects.last_place().unwrap();
    t.players[1].objects[v].life.state = ObjectState::Dying;
    let before = t.players[1].objects.clone();
    t.order_work(PlayerId::new(1), v, &order).unwrap();
    assert_eq!(t.players[1].objects, before);
}

/// Slot 0x7e8 of a villager on a pasture with room but no livestock: `FUN_1411c7670` gives
/// none and the order ends there, before `FUN_141189ca0`: the pasture the villager tended stays
/// its own.
#[test]
fn a_villager_ordered_on_a_pasture_without_livestock_tends_on() {
    let order = crate::orders::Work {
        target: Some(object_id(50)),
        point: Position::new(20.0, 20.0, 0.0),
        carry_test: true,
        filter: None,
        notified: false,
    };
    let mut t = tended_pasture(Position::new(18.0, 18.0, 0.0), 2.0);
    let v = t.players[1].objects.last_place().unwrap();
    t.players[1].objects[ListIndex::new(0)].livestock.clear();
    t.players[1].objects[v].my_pasture = Some(object_id(50));
    t.players[1].objects[ListIndex::new(0)].pasture_workers = vec![object_id(60)];
    t.order_work(PlayerId::new(1), v, &order).unwrap();
    assert_eq!(t.players[1].objects[v].my_pasture, Some(object_id(50)));
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)].pasture_workers,
        [object_id(60)]
    );
}

/// `FUN_141189ca0`: a unit tending a pasture joins its workers once; tending none drops it
/// from the pasture it tended (`FUN_1411cafa0`); an object that is no building keeps no list.
#[test]
fn a_unit_joins_and_leaves_a_pasture_s_workers() {
    let mut t = tended_pasture(Position::new(18.0, 18.0, 0.0), 2.0);
    let v = t.players[1].objects.last_place().unwrap();
    t.tend_pasture(PlayerId::new(1), v, Some(object_id(50)))
        .unwrap();
    t.tend_pasture(PlayerId::new(1), v, Some(object_id(50)))
        .unwrap();
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)].pasture_workers,
        [object_id(60)]
    );
    assert_eq!(t.players[1].objects[v].my_pasture, Some(object_id(50)));
    t.tend_pasture(PlayerId::new(1), v, None).unwrap();
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)].pasture_workers,
        Vec::new()
    );
    assert_eq!(t.players[1].objects[v].my_pasture, None);
    let animal = t.players[1].objects[ListIndex::new(1)].id;
    t.tend_pasture(PlayerId::new(1), v, ObjectId::of_field(animal.get()))
        .unwrap();
    assert_eq!(
        t.players[1].objects[ListIndex::new(1)].pasture_workers,
        Vec::new()
    );
    assert_eq!(
        t.players[1].objects[v].my_pasture,
        ObjectId::of_field(animal.get())
    );
}

/// `Process_New_Tiles`: player 1's newly explored tiles, in order, give the objects of its
/// masters that gather by themselves to its lists by group, at their places.
#[test]
fn a_player_s_new_tiles_give_their_resources_to_its_lists() {
    use crate::objects::masters::{Master, Masters};
    use crate::vision::visibility::{VisibleResource, VisibleResources};
    let object = |id, master, x, y| Unit {
        id,
        body: crate::objects::layers::Body::at(Position::new(x, y, 0.0)),
        life: Lifecycle {
            class: Class::Static,
            state: ObjectState::Alive,
            master_id: master,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        1,
        PlayerId::new(1),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player {
                sleeping: vec![
                    object(object_id(501), master_id(411), 10.5, 10.25),
                    object(object_id(502), master_id(412), 10.5, 10.5),
                    object(object_id(503), master_id(413), 11.5, 10.75),
                ]
                .into(),
                ..Player::default()
            },
            Player::default(),
        ],
    );
    let master = |id, auto_gather, gather_group| Master {
        id,
        kind: master_type::STATIC,
        auto_gather,
        gather_group,
        ..Master::default()
    };
    t.masters = vec![
        Masters::new([
            master(master_id(411), true, GatherGroup::WOOD),
            master(master_id(412), false, GatherGroup::WOOD),
            master(master_id(413), true, GatherGroup::GOLD),
        ]),
        Masters::default(),
    ];
    t.spatial = Spatial::with_tiles(
        GridSize::new(120, 120),
        crate::map::spatial::Tiles::new(GridSize::new(120, 120), vec![0; 120 * 120]),
    );
    placeable(&mut t);
    for u in &t.players[0].sleeping {
        t.spatial.insert(
            u.id,
            Placed {
                position: u.position(),
                radius: Footprint::new(0.5, 0.5),
                class: class::TREE,
                player: PlayerId::new(0),
                master: u.life.master_id,
                obstruction_type: ObstructionType::NONE,
                buildable_on: 0,
                tall: false,
                speed: Speed::ZERO,
                unknown: None,
                obstruction: None,
            },
        );
    }
    let v = t.spatial.sight.as_mut().unwrap();
    v.resources[1] = VisibleResources {
        capacities: vec![32; 5],
        lists: vec![Vec::new(); 5],
    };
    v.new_tiles[1] = vec![Tile::new(11, 10), Tile::new(10, 10)];
    t.process_new_tiles(PlayerId::new(1)).unwrap();
    let rec = |id, x, y| VisibleResource {
        id,
        x,
        y,
        ..VisibleResource::default()
    };
    let v = t.spatial.sight.as_ref().unwrap();
    assert_eq!(v.resources[1].lists[0], [rec(object_id(501), 10.5, 10.25)]);
    assert_eq!(v.resources[1].lists[4], [rec(object_id(503), 11.5, 10.75)]);
    assert_eq!(v.resources[0], VisibleResources::default());
}

/// `FUN_1411980b0`: a static resource that runs out (state 3 to 7) leaves the players'
/// visible-resource lists at once; an animated one (state 5 to 7, not below 4) only when it
/// is deleted (state 8 leaving its list, `FUN_14116b5e0`).
#[test]
fn a_resource_leaves_the_lists_when_it_runs_out_or_is_deleted() {
    use crate::objects::lifecycle::MasterData;
    use crate::objects::masters::{Master, Masters};
    use crate::vision::visibility::VisibleResources;
    let dead = |id, class, state, master| Unit {
        id,
        body: crate::objects::layers::Body::at(Position::new(10.5, 10.5, 0.0)),
        life: Lifecycle {
            class,
            state,
            master_id: master,
            attribute_type: None,
            master: Some(MasterData {
                decay: AmountRate::ZERO,
                undead: false,
            }),
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        1,
        PlayerId::new(1),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player {
                objects: vec![
                    dead(
                        object_id(501),
                        Class::Static,
                        ObjectState::Dying,
                        master_id(411),
                    ),
                    dead(
                        object_id(502),
                        Class::Animated,
                        ObjectState::Dead,
                        master_id(412),
                    ),
                ]
                .into(),
                ..Player::default()
            },
            Player::default(),
        ],
    );
    let master = |id, gather_group| Master {
        id,
        kind: master_type::STATIC,
        auto_gather: true,
        gather_group,
        ..Master::default()
    };
    t.masters = vec![
        Masters::new([
            master(master_id(411), GatherGroup::WOOD),
            master(master_id(412), GatherGroup::FORAGE),
        ]),
        Masters::default(),
    ];
    placeable(&mut t);
    let v = t.spatial.sight.as_mut().unwrap();
    for p in 0..2 {
        v.resources[p] = VisibleResources {
            capacities: vec![32; 5],
            lists: vec![Vec::new(); 5],
        };
        v.resources[p].add(object_id(501), GatherGroup::WOOD, 10.5, 10.5);
        v.resources[p].add(object_id(502), GatherGroup::FORAGE, 10.5, 10.5);
    }
    let held = |t: &World| -> Vec<(usize, usize)> {
        let v = t.spatial.sight.as_ref().unwrap();
        (0..2)
            .map(|p| (v.resources[p].lists[0].len(), v.resources[p].lists[1].len()))
            .collect()
    };
    t.tick(Millis::new(10));
    assert_eq!(
        t.players[0].objects[ListIndex::new(0)].life.state,
        ObjectState::Removed
    );
    assert_eq!(held(&t), [(0, 1), (0, 1)]);
    t.tick(Millis::new(10));
    assert_eq!(held(&t), [(0, 1), (0, 1)]);
    t.tick(Millis::new(10));
    assert_eq!(t.players[0].objects, []);
    assert_eq!(held(&t), [(0, 0), (0, 0)]);
}

/// `FUN_14162ce00`: a new object of a master that gathers by itself joins the lists of the
/// players that explored its tile; one of another master joins none.
#[test]
fn a_new_resource_joins_the_lists_of_the_players_that_explored_its_tile() {
    use crate::objects::masters::{Master, Masters};
    use crate::vision::visibility::{VisibleResource, VisibleResources};
    let made = |id, master| Unit {
        id,
        body: crate::objects::layers::Body::at(Position::new(20.75, 30.25, 0.0)),
        life: Lifecycle {
            state: ObjectState::Alive,
            master_id: master,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        1,
        PlayerId::new(1),
        vec![Millis::ZERO, Millis::ZERO, Millis::ZERO],
        vec![Player::default(), Player::default(), Player::default()],
    );
    let master = |id, auto_gather| Master {
        id,
        kind: master_type::MOVING,
        auto_gather,
        gather_group: GatherGroup::FISH,
        ..Master::default()
    };
    t.masters = vec![Masters::new([master(master_id(53), true), master(master_id(54), false)]); 3];
    placeable(&mut t);
    let v = t.spatial.sight.as_mut().unwrap();
    // players 0 and 2 explored the tile, player 1 did not
    v.unified[30 * 120 + 20] = 1 << 32 | 1 << 34;
    for r in &mut v.resources {
        *r = VisibleResources {
            capacities: vec![32; 5],
            lists: vec![Vec::new(); 5],
        };
    }
    t.players[0]
        .objects
        .push(made(object_id(700), master_id(53)));
    t.made_new_object(PlayerId::new(0)).unwrap();
    t.players[0]
        .objects
        .push(made(object_id(701), master_id(54)));
    t.made_new_object(PlayerId::new(0)).unwrap();
    let fish = VisibleResource {
        id: object_id(700),
        x: 20.75,
        y: 30.25,
        ..VisibleResource::default()
    };
    let v = t.spatial.sight.as_ref().unwrap();
    let lists: Vec<_> = v.resources.iter().map(|r| r.lists[2].clone()).collect();
    assert_eq!(lists, [vec![fish], vec![], vec![fish]]);
}

/// `FUN_141226060`: the class counters by class and master.
#[test]
fn a_combat_object_counts_in_its_player_s_class_attributes() {
    use crate::objects::masters::Master;
    let counted = |class, id, task_by_group| {
        let mut a = vec![Amount::ZERO; 240];
        let m = Master {
            id,
            class,
            task_by_group,
            ..Master::default()
        };
        count_class(&mut a, &m, 1.0);
        count_class(&mut a, &m, 1.0);
        [a[37], a[38], a[40], a[235]].map(Amount::get)
    };
    assert_eq!(
        counted(class::CLASS_6, master_id(4), 0),
        [0.0, 0.0, 2.0, 0.0]
    );
    assert_eq!(counted(class::SIEGE, ids::ID_1663, 0), [0.0, 0.0, 2.0, 2.0]);
    assert_eq!(
        counted(class::CLASS_57, ids::ID_1697, 0),
        [0.0, 0.0, 2.0, 2.0]
    );
    assert_eq!(
        counted(class::VILLAGER, VILLAGER_MALE, 1),
        [2.0, 2.0, 0.0, 0.0]
    );
    assert_eq!(
        counted(class::VILLAGER, VILLAGER_MALE, 2),
        [2.0, 2.0, 0.0, 0.0]
    );
    assert_eq!(
        counted(class::VILLAGER, VILLAGER_MALE, 3),
        [2.0, 0.0, 0.0, 0.0]
    );
    assert_eq!(
        counted(class::TRADE_CART, master_id(128), 0),
        [2.0, 0.0, 0.0, 0.0]
    );
    assert_eq!(counted(class::LIVESTOCK, ids::ID_1663, 1), [0.0; 4]);
    assert_eq!(counted(class::BUILDING, ids::TOWN_CENTRE, 0), [0.0; 4]);
    let mut a = vec![Amount::new(5.0); 41];
    count_class(
        &mut a,
        &Master {
            class: class::CLASS_6,
            ..Master::default()
        },
        -1.0,
    );
    assert_eq!(a[40], Amount::new(4.0));
}

/// `0x141071880`'s idle swap: a standing sprite that looped draws on the Sprite stream and,
/// above 0x3fff, swaps to the other standing sprite at its first frame; a sprite that did not
/// loop, another sprite or a master without a second standing sprite draws nothing.
#[test]
fn an_idle_object_swaps_its_standing_sprite_on_a_high_draw() {
    use crate::objects::sprite::{ActiveSprite, Graphic};
    let g = |id| Graphic {
        id,
        frames: 10,
        facets: 8,
        speed_multiplier: 1.0,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: 1,
    };
    let graphics: std::collections::BTreeMap<SpriteId, Graphic> = [
        (SpriteId::named(100), g(SpriteId::named(100))),
        (SpriteId::named(101), g(SpriteId::named(101))),
    ]
    .into_iter()
    .collect();
    let master = Master {
        standing_sprite: Some(SpriteId::named(100)),
        standing_sprite_2: Some(SpriteId::named(101)),
        ..Master::default()
    };
    let shown = |id, looped| ActiveSprite {
        frame: 4,
        looped,
        ..ActiveSprite::at_rest(g(id), 4)
    };
    // a seed whose first draw is above 0x3fff, another whose first is not
    let rng = |seed| {
        let mut r = Well512::default();
        r.set_seed(seed);
        r
    };
    let high = (0..100u32)
        .find(|&s| 0x3fff < rng(s).rand15())
        .expect("a high draw");
    let low = (0..100u32)
        .find(|&s| rng(s).rand15() <= 0x3fff)
        .expect("a low draw");
    let mut u = Unit {
        sprite: Some(shown(SpriteId::named(100), true)),
        ..Unit::default()
    };
    let mut r = rng(high);
    u.idle_swap(
        master.standing_sprite,
        master.standing_sprite_2,
        &graphics,
        &mut r,
    );
    assert_eq!(u.idle_variant, 1);
    let s = u.sprite.unwrap();
    assert_eq!((s.graphic.id, s.frame), (SpriteId::named(101), 0));
    assert_eq!(r.draws, 1 + rng(high).draws);
    // a low draw keeps the sprite
    let mut u = Unit {
        sprite: Some(shown(SpriteId::named(100), true)),
        ..Unit::default()
    };
    u.idle_swap(
        master.standing_sprite,
        master.standing_sprite_2,
        &graphics,
        &mut rng(low),
    );
    assert_eq!(
        (u.idle_variant, u.sprite.unwrap().graphic.id),
        (0, SpriteId::named(100))
    );
    // no loop, another sprite, no second standing sprite: no draw
    for (sprite, m) in [
        (shown(SpriteId::named(100), false), master.clone()),
        (shown(SpriteId::named(7), true), master.clone()),
        (
            shown(SpriteId::named(100), true),
            Master {
                standing_sprite_2: None,
                ..master
            },
        ),
    ] {
        let mut u = Unit {
            sprite: Some(sprite),
            ..Unit::default()
        };
        let mut r = rng(high);
        u.idle_swap(m.standing_sprite, m.standing_sprite_2, &graphics, &mut r);
        assert_eq!(r.draws, rng(high).draws);
    }
}

/// Player 1's two idle villagers (9175 at the house's place, 9181 away), its resources and
/// the builders' masters, the house placeable: the BUILD tests' world.
fn build_world() -> World {
    let villager = |id, master, x, y| Unit {
        id,
        body: crate::objects::layers::Body::at(Position::new(x, y, 0.0)),
        life: Lifecycle {
            class: Class::Animated,
            state: ObjectState::Alive,
            hp: HitPoints::new(25.0),
            master_id: master,
            ..Lifecycle::default()
        },
        // a villager's unit AI (type 8) without an order or action
        ai: Some(UnitAi {
            ai_type: UnitAiType::CIVILIAN,
            current_order: None,
            current_action: None,
            current_target: None,
            ..UnitAi::default()
        }),
        ..Unit::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        29910,
        PlayerId::new(1),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player::default(),
            Player {
                objects: vec![
                    villager(object_id(9175), VILLAGER_MALE, 30.0, 89.0),
                    villager(object_id(9181), VILLAGER_FEMALE, 29.0, 99.0),
                ]
                .into(),
                attributes: crate::test_ids::attributes(250, [200.0, 100.0, 200.0, 100.0]),
                ..Player::default()
            },
        ],
    );
    t.next_object_id = object_id(10028);
    t.masters = vec![Masters::default(), builder_masters()];
    placeable(&mut t);
    t
}

/// BUILD's handler (`FUN_140e7a400`) does nothing for a human's packet with a target (`+0x14`);
/// a house of civ 12 keeps the packet's sprite override (`+0x18`, 0x2c9 to 0x2ce), any other
/// none.
#[test]
fn a_human_build_with_a_target_places_nothing_and_a_house_keeps_its_sprite_override() {
    let build = |target, sprite_override| {
        Command::Build(BuildCommand {
            player: player_id(1),
            master: master_id(70),
            position: Point::new(30.0, 88.0),
            q_id: target,
            sprite_override,
            builders: unit_list(object_ids([9175, 9181])),
            queued: false,
            commits: false,
            frame: 0,
        })
    };
    let mut t = build_world();
    let before = t.players[1].objects.len();
    t.run(&build(3, -1));
    assert_eq!(t.players[1].objects.len(), before);
    let overridden = |civ, sprite_override| {
        let mut t = build_world();
        t.players[1].civ = civ;
        t.run(&build(-1, sprite_override));
        let foundation = t.players[1].objects.last().unwrap();
        foundation.construction.as_ref().unwrap().sprite_override
    };
    assert!(overridden(CivId::new(12), 0x2c9));
    assert!(!overridden(CivId::new(12), 0x2cf));
    assert!(!overridden(CivId::new(11), 0x2c9));
    assert!(!overridden(CivId::new(12), -1));
}

#[test]
fn a_build_pays_the_cost_and_tasks_the_builders_of_its_own_player() {
    let mut t = build_world();
    t.tick_with(
        Millis::new(50),
        &[Command::Build(BuildCommand {
            player: player_id(1),
            master: master_id(70),
            position: Point::new(30.0, 88.0),
            q_id: -1,
            sprite_override: -1,
            builders: unit_list(object_ids([9175, 9181])),
            queued: false,
            commits: false,
            frame: 0,
        })],
    );
    let p = &t.players[1];
    assert_eq!(
        p.resources(),
        crate::test_ids::amounts([200.0, 75.0, 200.0, 100.0])
    );
    // the house's paid cost, as a building with a train location, in attribute 247
    assert_eq!(p.attributes[247], Amount::new(25.0));
    let foundation = p.objects.last().unwrap();
    assert_eq!(
        (
            foundation.id.get(),
            foundation.life.master_id.get(),
            foundation.position()
        ),
        (10028, 70, Position::new(30.0, 88.0, 0.0))
    );
    // an animated object's constructor (FUN_141070cf0) sets its animation speed (+0x7b8) to 0:
    // its sprite's clock runs from its first update (a dying sprite plays out)
    assert_eq!(foundation.body.anim_speed(), Some(0.0));
    let builder = |i: ListIndex| {
        (
            p.objects[i].life.master_id.get(),
            p.objects[i].actions.build().map(|b| b.state),
        )
    };
    // the builder at the foundation works in its first update; the other's MoveTo fails at once
    // (FUN_140aa2100: these villagers lack their movement state, so their speed), and its
    // failed walk's path test needs that state
    assert_eq!(
        (builder(ListIndex::new(0)), builder(ListIndex::new(1))),
        ((118, Some(BuildState::Work)), (212, Some(BuildState::Move)))
    );
    // each idle villager's town-bell record: no work, at its own place
    assert_eq!(
        p.objects[ListIndex::new(1)].town_bell.work,
        crate::orders::town_bell::BackToWork {
            point: Point::new(29.0, 99.0),
            ..crate::orders::town_bell::BackToWork::NONE
        }
    );
    assert_eq!(
        p.unsupported,
        Some((
            1,
            ListIndex::new(1),
            crate::construction::build::BUILDER_UNMOVING
        ))
    );
}

/// BUILD's direct path is the builder's slot 0x128 with 1 (`FUN_141211b80`, a combat object's
/// and a building's): its unit AI's order queue empties (`FUN_140c5e010`) before the work goes
/// on. 430020666's villager 6676 drops the move it queued in update 40 when BUILD sends it to
/// another house in update 119.
#[test]
fn a_direct_build_empties_its_builder_s_order_queue() {
    let mut t = build_world();
    let queued_move = crate::unit_ai::QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::Move,
        priority: 100,
        target: None,
        target_owner: None,
        point: Position::new(19.760_416, 101.677_086, 0.0),
        range: Distance::ZERO,
        shown: Point::new(19.760_416, 102.177_086),
        ready: true,
        target_type: None,
    };
    for u in &mut t.players[1].objects {
        u.ai.as_mut().unwrap().orders.push(queued_move);
    }
    // the command phase alone (the units' updates would run the orders)
    t.run(&Command::Build(BuildCommand {
        player: player_id(1),
        master: master_id(70),
        position: Point::new(30.0, 88.0),
        q_id: -1,
        sprite_override: -1,
        builders: unit_list(object_ids([9175])),
        queued: false,
        commits: false,
        frame: 0,
    }));
    let unit = |id: ObjectId| t.object(id).map(|(_, u)| u).unwrap();
    let orders = |id: i32| {
        unit(object_id(id))
            .ai
            .as_ref()
            .map_or(0, |ai| ai.orders.len())
    };
    assert!(unit(object_id(9175)).actions.build().is_some());
    // the builder's queue empties; the villager BUILD did not name keeps its own
    assert_eq!((orders(9175), orders(9181)), (0, 1));
}

/// census v5's 257 stops "a builder of another player or a sleeping one": BUILD's last
/// loop (`FUN_140e7a400`) takes the packet's units the world holds whatever their player: a
/// queued BUILD puts the build order into another player's villager's queue, issued by its
/// own player and naming the BUILD's player as the target's; a unit of a master that does
/// not move is passed over unless the BUILD is queued. Another player's unit reaches the
/// handler only while players may order each other's (`do_command`'s check, the setting
/// `+0x1074`).
#[test]
fn a_queued_build_queues_its_order_for_another_player_s_villager() {
    let villager = |id, master, x, y| Unit {
        id,
        body: crate::objects::layers::Body::at(Position::new(x, y, 0.0)),
        life: Lifecycle {
            class: Class::Animated,
            state: ObjectState::Alive,
            hp: HitPoints::new(25.0),
            master_id: master,
            ..Lifecycle::default()
        },
        ai: Some(UnitAi {
            ai_type: UnitAiType::CIVILIAN,
            current_order: None,
            current_action: None,
            current_target: None,
            ..UnitAi::default()
        }),
        ..Unit::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        29910,
        PlayerId::new(1),
        vec![Millis::ZERO, Millis::ZERO, Millis::ZERO],
        vec![
            Player::default(),
            Player {
                objects: vec![villager(object_id(9175), VILLAGER_MALE, 30.0, 89.0)].into(),
                attributes: crate::test_ids::attributes(250, [200.0, 100.0, 200.0, 100.0]),
                ..Player::default()
            },
            Player {
                objects: vec![villager(object_id(9618), VILLAGER_FEMALE, 29.0, 99.0)].into(),
                attributes: vec![Amount::ZERO; 250],
                ..Player::default()
            },
        ],
    );
    t.next_object_id = object_id(10028);
    t.masters = vec![Masters::default(), builder_masters(), builder_masters()];
    placeable(&mut t);
    t.settings.cheats_enabled = true;
    // the command phase alone (the units' updates would run the orders)
    t.run(&Command::Build(BuildCommand {
        player: player_id(1),
        master: master_id(70),
        position: Point::new(30.0, 88.0),
        q_id: -1,
        sprite_override: -1,
        builders: unit_list(object_ids([9618, 9175])),
        queued: true,
        commits: false,
        frame: 2,
    }));
    // the foundation keeps the command's frame (+0xc54)
    assert_eq!(
        t.object(object_id(10028)).map(|(_, u)| u.build_frame),
        Some(2)
    );
    let order = |p: PlayerId| {
        t.players[p.index()].objects[ListIndex::new(0)]
            .ai
            .as_ref()
            .and_then(|ai| ai.orders.first().copied())
            .map(|o| (o.issuer, o.order, o.target, o.target_owner))
    };
    assert_eq!(
        order(PlayerId::new(2)),
        Some((
            Some(Issuer::Player(player_id(2))),
            OrderKind::Build,
            Some(object_id(10028)),
            Some(player_id(1))
        ))
    );
    assert_eq!(
        order(PlayerId::new(1)),
        Some((
            Some(Issuer::Player(player_id(1))),
            OrderKind::Build,
            Some(object_id(10028)),
            Some(player_id(1))
        ))
    );
}

/// A world of player 1's villager 9175 (master 83, which has no Build task) beside its
/// house's foundation 10028 (master 70, state 0, no build points yet), with the unit AI's
/// queue `orders`.
pub(crate) fn sent_off_world(orders: Vec<crate::unit_ai::QueuedOrder>) -> World {
    let villager = Unit {
        id: object_id(9175),
        body: crate::objects::layers::Body::at(Position::new(29.5, 89.5, 0.0)),
        life: Lifecycle {
            class: Class::Animated,
            state: ObjectState::Alive,
            hp: HitPoints::new(25.0),
            master_id: VILLAGER_MALE,
            ..Lifecycle::default()
        },
        ai: Some(UnitAi {
            ai_type: crate::unit_ai::CIVILIAN,
            current_order: None,
            current_action: None,
            current_target: None,
            last_target: None,
            orders,
            ..UnitAi::default()
        }),
        pasture: None,
        my_pasture: None,
        ..Unit::default()
    };
    let foundation = Unit {
        id: object_id(10028),
        body: crate::objects::layers::Body::at(Position::new(30.0, 88.0, 0.0)),
        life: Lifecycle {
            class: Class::Animated,
            state: ObjectState::Foundation,
            hp: HitPoints::new(1.0),
            master_id: ids::HOUSE,
            ..Lifecycle::default()
        },
        construction: Some(Construction {
            progress: Seconds::ZERO,
            desolid: true,
            builders: 0,
            hit_points: 550,
            build_time: 25,
            record: crate::map::spatial::Obstruction {
                kind: ObstructionType::TYPE_2,
                class: ObstructionClass::UNIT,
            },
            snow: false,
            sprite_override: false,
        }),
        pasture: None,
        my_pasture: None,
        ..Unit::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        29910,
        PlayerId::new(1),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player::default(),
            Player {
                objects: vec![villager, foundation].into(),
                attributes: vec![Amount::ZERO; 250],
                relations: relation_row([1, 0]),
                ..Player::default()
            },
        ],
    );
    t.masters = vec![builder_masters(), builder_masters()];
    t
}

fn task(action_type: ActionType) -> crate::objects::masters::Task {
    crate::objects::masters::Task {
        action_type,
        object_group: None,
        ..crate::objects::masters::Task::default()
    }
}

/// Slot 0x820's search (`FUN_14106c6a0`): the own master's list first; without a task there
/// the player's masters by id of the unit's class, type and swap group, with any filter,
/// the first with a task giving it with the master to switch to; none without a group.
#[test]
fn slot_0x820_s_search_takes_the_own_task_else_the_first_master_of_the_swap_group() {
    let t = sent_off_world(vec![]);
    let masters = &t.masters[1];
    let find = |id: ObjectId| t.object(id);
    let site = task_target_with(object_id(10028), 1, &find, &t.masters)
        .unwrap()
        .unwrap();
    let world = TaskWorld {
        players: &t.players,
        drop_sites: &[],
    };
    let search = |own: &Master| {
        let actor = Actor {
            id: object_id(9175),
            master: own,
            player: PlayerId::new(1),
            held: carrying(None, 0.0),
            attribute_95: Amount::ZERO,
            civ: CivId::new(0),
        };
        task_search(masters, &actor, &site, &site, None, None, &world)
            .unwrap()
            .map(|(task, switch)| (task.action_type, switch.map(|m| m.id.get())))
    };
    // a villager of master 83 builds as the builder 118, the first of its group with a task
    assert_eq!(
        search(masters.get(VILLAGER_MALE).unwrap()),
        Some((action_type::BUILD, Some(118)))
    );
    // the builders find it in their own lists
    assert_eq!(
        search(masters.get(ids::BUILDER).unwrap()),
        Some((action_type::BUILD, None))
    );
    assert_eq!(
        search(masters.get(master_id(212)).unwrap()),
        Some((action_type::BUILD, None))
    );
    // without a swap group there is no other list to try
    let alone = Master {
        task_by_group: 0,
        ..masters.get(VILLAGER_MALE).unwrap().clone()
    };
    assert_eq!(search(&alone), None);
}

/// The task finder's target (`World::task_target`'s reading): an annex stands for its main
/// building, once per step; an object not held is none.
#[test]
fn a_task_target_reads_an_annex_as_its_main_building() {
    let mut t = sent_off_world(vec![]);
    t.players[1].objects[ListIndex::new(0)].annex_of = Some(object_id(10028));
    let find = |id: ObjectId| t.object(id);
    let at = |steps| {
        task_target_with(object_id(9175), steps, &find, &t.masters)
            .unwrap()
            .map(|x| (x.id.get(), x.master.id.get(), x.state))
    };
    assert_eq!(at(0), Some((9175, 83, ObjectState::Alive)));
    assert_eq!(at(1), Some((10028, 70, ObjectState::Foundation)));
    assert_eq!(at(2), Some((10028, 70, ObjectState::Foundation)));
    assert!(
        task_target_with(object_id(77), 1, &find, &t.masters)
            .unwrap()
            .is_none()
    );
}

/// The unit's slot 0x388 after its unit AI's update (`FUN_141226fc0`): the switch to the
/// master slot 0x820's search gave (slot 0x90), the actions cleared and the wait (type 0x9a,
/// state 10) the only one, its target −1 (the foundation is at its `+0x68`).
#[test]
fn a_wait_for_a_foundation_switches_the_villager_and_holds_the_task() {
    let mut t = sent_off_world(vec![]);
    let wait = Wait {
        target: object_id(10028),
        task: task(action_type::BUILD),
        switch: Some(ids::BUILDER),
    };
    t.wait_for_foundation(PlayerId::new(1), ListIndex::new(0), wait)
        .unwrap();
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(v.life.master_id, ids::BUILDER);
    assert_eq!(
        v.actions.wait_solidify().copied(),
        Some(crate::construction::wait_solidify::WaitSolidify::new(
            task(action_type::BUILD),
            object_id(10028)
        ))
    );
    assert_eq!(
        v.top_action(),
        Some(TopAction {
            kind: crate::construction::wait_solidify::WAIT_SOLIDIFY,
            state: 10
        })
    );
    // the wait names its foundation apart: its action has no target
    assert_eq!(v.top_target(), None);
    // without a switch the master stays
    let mut kept = sent_off_world(vec![]);
    kept.wait_for_foundation(
        PlayerId::new(1),
        ListIndex::new(0),
        Wait {
            switch: None,
            ..wait
        },
    )
    .unwrap();
    assert_eq!(
        kept.players[1].objects[ListIndex::new(0)].life.master_id,
        VILLAGER_MALE
    );
}

/// While a player's list is out for its unit's update, the others are read around it, and
/// the task finder reads the owner's diplomacy from the update and another player's from its
/// own record.
#[test]
fn a_unit_s_side_reads_the_other_players_relations_around_its_own() {
    let players: Vec<Player> = (0..3)
        .map(|q| Player {
            relations: relation_row(vec![q as u8; 3]),
            ..Player::default()
        })
        .collect();
    let foreign = Foreign {
        before: &players[..1],
        after: &players[2..],
        left: None,
    };
    assert_eq!(
        foreign.player(PlayerId::new(0)).map(|p| p.relations[0]),
        Some(Relation::ALLY)
    );
    assert!(foreign.player(PlayerId::new(1)).is_none());
    assert_eq!(
        foreign.player(PlayerId::new(2)).map(|p| p.relations[0]),
        Some(Relation::from(2))
    );
    assert!(foreign.player(PlayerId::new(3)).is_none());
    // the owner (1) counts player 2 as an ally (0), player 2 counts the owner an enemy (2)
    let world = SideWorld {
        player: PlayerId::new(1),
        relations: &[Relation::NEUTRAL, Relation::NEUTRAL, Relation::ALLY],
        foreign,
        drop_sites: &[],
    };
    assert!(world.allied(PlayerId::new(1), PlayerId::new(2), false));
    assert!(!world.allied(PlayerId::new(2), PlayerId::new(1), false));
    assert!(world.allied(PlayerId::new(1), PlayerId::new(1), false));
}

/// The wait's foundation as its update reads it: its build points among the owner's other
/// objects or the other players', none when the world does not hold it.
#[test]
fn a_wait_reads_its_foundation_s_build_points_wherever_it_is() {
    let t = sent_off_world(vec![]);
    let (none, players) = (&t.players[..0], &t.players[..]);
    let own = Foreign {
        before: none,
        after: none,
        left: None,
    };
    let own = OthersRef::new(PlayerId::new(1), &t.players[1].objects, own);
    assert_eq!(wait_progress(object_id(10028), own), Some(Seconds::ZERO));
    assert_eq!(wait_progress(object_id(77), own), None);
    // another player's (player 0 updating, player 1 after it)
    let other = Foreign {
        before: none,
        after: &players[1..],
        left: None,
    };
    assert_eq!(
        wait_progress(
            object_id(10028),
            OthersRef::new(PlayerId::new(0), &ObjectList::default(), other)
        ),
        Some(Seconds::ZERO)
    );
}

/// `FUN_1411cc430` on an annex whose main building is in state 0 with a hit point (the
/// builder's work deferred to the caller, [`crate::construction::build::Worked::Main`]): the main
/// building's annexes start first (the annex itself among them, desolid), then the main
/// building starts and takes the work; the annex takes none.
#[test]
fn the_work_on_an_annex_goes_to_its_main_building_after_its_annexes_start() {
    let mut t = sent_off_world(vec![]);
    let annex = Unit {
        id: object_id(10031),
        annex_of: Some(object_id(10028)),
        ..t.players[1].objects[ListIndex::new(1)].clone()
    };
    t.players[1].objects.push(annex);
    t.spatial = Spatial::with_tiles(
        GridSize::new(120, 120),
        crate::map::spatial::Tiles::new(GridSize::new(120, 120), vec![4; 14400]),
    );
    for (id, x) in [(10028, 30.0), (10031, 32.0)] {
        t.spatial.insert(
            object_id(id),
            Placed {
                position: Position::new(x, 88.0, 0.0),
                radius: Footprint::new(1.0, 1.0),
                class: class::BUILDING,
                player: PlayerId::new(1),
                master: ids::HOUSE,
                obstruction_type: ObstructionType::TYPE_2,
                buildable_on: 0,
                tall: true,
                speed: Speed::ZERO,
                unknown: None,
                obstruction: None,
            },
        );
    }
    t.spatial
        .set_annex(object_id(10031), Some(object_id(10028)));
    assert_eq!(t.work_building(object_id(10031), 0.021), Ok(true));
    let c = |id: ObjectId| t.object(id).unwrap().1.construction.unwrap();
    let started = f32::from_bits(0x2edb_e6ff);
    assert!(!c(object_id(10031)).desolid);
    assert_eq!(c(object_id(10031)).progress, Seconds::new(started));
    assert!(!c(object_id(10028)).desolid);
    assert_eq!(c(object_id(10028)).progress, Seconds::new(started + 0.021));
    assert_eq!(
        t.object(object_id(10031)).unwrap().1.life.hp,
        HitPoints::new(1.0)
    );
    assert!(HitPoints::new(1.0) < t.object(object_id(10028)).unwrap().1.life.hp);
}

/// The world of [`sent_off_world`] with the house built and damaged (300 of its 550 hit
/// points), the villager a builder (118) with a Repair task on any object (work value 0.5) and
/// a weapon range of 0.5.
fn repair_world() -> World {
    let mut t = sent_off_world(vec![]);
    let v = &mut t.players[1].objects[ListIndex::new(0)];
    v.life.master_id = ids::BUILDER;
    let house = &mut t.players[1].objects[ListIndex::new(1)];
    house.life.state = ObjectState::Alive;
    house.life.hp = HitPoints::new(300.0);
    let c = house.construction.as_mut().unwrap();
    (c.progress, c.desolid) = (Seconds::new(1.0), false);
    for masters in &mut t.masters {
        let builder = masters.get_mut(ids::BUILDER).unwrap();
        builder.weapon_range = Distance::new(0.5);
        builder.tasks.push(crate::objects::masters::Task {
            id: TaskId::test(4),
            work_val1: 0.5,
            ..task(crate::construction::repair::REPAIR)
        });
    }
    t
}

/// The work command on the player's damaged house (`FUN_14106fd00`, then the Repair's start):
/// the task finder's Repair task (106), the target taken, the unit AI told order 0x2ce with
/// action 0x26a and the house's class, the Repair walking (state 4) with its master's weapon
/// range. The same command again while it faces the house (state 6) keeps the action; a
/// computer player's repairer walking to it keeps it too.
#[test]
fn a_work_command_on_a_damaged_building_starts_a_repair() {
    let mut t = repair_world();
    let work = crate::orders::Work {
        target: Some(object_id(10028)),
        point: Position::new(30.0, 88.0, 0.0),
        carry_test: false,
        filter: None,
        notified: true,
    };
    t.order_work(PlayerId::new(1), ListIndex::new(0), &work)
        .unwrap();
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        v.top_action_kind(),
        Some(crate::construction::repair::REPAIR)
    );
    let r = v.actions.repair().copied().unwrap();
    assert_eq!(
        (r.target, r.state, r.range, r.task.id, r.task.work_value),
        (
            Some(object_id(10028)),
            RepairState::Move,
            Distance::new(0.5),
            TaskId::test(4),
            0.5
        )
    );
    let ai = v.ai.as_ref().unwrap();
    assert_eq!(
        (ai.current_order, ai.current_action, ai.current_target),
        (
            Some(OrderKind::Repair),
            Some(ActionKind::Repair),
            Some(object_id(10028))
        )
    );
    t.players[1].objects[ListIndex::new(0)]
        .actions
        .repair_mut()
        .unwrap()
        .state = RepairState::Face;
    t.order_work(PlayerId::new(1), ListIndex::new(0), &work)
        .unwrap();
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .repair()
            .copied()
            .unwrap()
            .state,
        RepairState::Face
    );
}

/// A work command on another damaged house while repairing (`FUN_140adb480`'s slot 0x68): a
/// repairer counted among the first house's repairers leaves them, takes the new one and walks
/// to it again (state 4, not counted).
#[test]
fn a_work_command_on_another_building_moves_the_repair_there() {
    let mut t = repair_world();
    let other = Unit {
        id: object_id(10031),
        body: crate::objects::layers::Body::at(Position::new(34.0, 88.0, 0.0)),
        ..t.players[1].objects[ListIndex::new(1)].clone()
    };
    t.players[1].objects.push(other);
    let work = |target| crate::orders::Work {
        target,
        point: Position::new(30.0, 88.0, 0.0),
        carry_test: false,
        filter: None,
        notified: true,
    };
    t.order_work(
        PlayerId::new(1),
        ListIndex::new(0),
        &work(Some(object_id(10028))),
    )
    .unwrap();
    {
        let r = t.players[1].objects[ListIndex::new(0)]
            .actions
            .repair_mut()
            .unwrap();
        (r.state, r.counted) = (RepairState::Work, true);
    }
    t.players[1].objects[ListIndex::new(1)]
        .construction
        .as_mut()
        .unwrap()
        .builders = 1;
    t.order_work(
        PlayerId::new(1),
        ListIndex::new(0),
        &work(Some(object_id(10031))),
    )
    .unwrap();
    let r = t.players[1].objects[ListIndex::new(0)]
        .actions
        .repair()
        .copied()
        .unwrap();
    assert_eq!(
        (r.target, r.state, r.counted, r.point),
        (
            Some(object_id(10031)),
            RepairState::Move,
            false,
            Position::new(34.0, 88.0, 0.0)
        )
    );
    assert_eq!(
        t.players[1].objects[ListIndex::new(1)]
            .construction
            .unwrap()
            .builders,
        0
    );
}

/// The work command on the player's foundation (`FUN_14106ff70`, the factory's case 0x65, then
/// `FUN_140aadd80`): the villager (83) becomes the builder 118 of its swap group, takes the
/// foundation (one worker), tells its unit AI order 702 with action 602 on it and walks to it
/// (state 4, its `MoveTo` starting: the footprints lie 0.3 apart) with the task's range, work
/// value and the builder's work rate.
#[test]
fn a_work_command_on_a_foundation_starts_a_build_as_its_builder() {
    let mut t = sent_off_world(vec![]);
    // a villager moves: its MoveTo starts with speed (FUN_140aa2100)
    t.players[1].objects[ListIndex::new(0)]
        .body
        .put_mover(Some(crate::gathering::hunt::tests::walker()));
    let work = crate::orders::Work {
        target: Some(object_id(10028)),
        point: Position::new(30.0, 88.0, 0.0),
        carry_test: false,
        filter: None,
        notified: true,
    };
    t.order_work(PlayerId::new(1), ListIndex::new(0), &work)
        .unwrap();
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(v.life.master_id, ids::BUILDER);
    assert_eq!(v.top_action_kind(), Some(action_type::BUILD));
    let b = v.actions.build().copied().unwrap();
    assert_eq!(
        (
            b.target,
            b.state,
            b.move_to.map(|m| ObjectId::field(m.target)),
            b.point,
            b.move_to.map(|m| m.state),
            b.added
        ),
        (
            Some(object_id(10028)),
            BuildState::Move,
            Some(10028),
            Position::new(30.0, 88.0, 0.0),
            Some(MoveTo::Start),
            false
        )
    );
    assert_eq!(
        b.task,
        BuildTask {
            range: Distance::ZERO,
            work_value: 0.0,
            work_rate: WorkRate::new(1.0),
            resource: None,
            id: TaskId::test(0),
            moving: None,
        }
    );
    let ai = v.ai.as_ref().unwrap();
    assert_eq!(
        (ai.current_order, ai.current_action, ai.current_target),
        (
            Some(OrderKind::Build),
            Some(ActionKind::Build),
            Some(object_id(10028))
        )
    );
    assert_eq!(t.players[1].objects[ListIndex::new(1)].workers, 1);
}

/// A builder with a master of its own (a captured villager's copy) builds as that master: its
/// Build's `MoveTo` starts with the own master's walking sprite (`FUN_140aa0df0`'s default),
/// not the one of its player's master of its id, which has none.
#[test]
fn a_builder_with_a_master_of_its_own_walks_with_its_own_master_s_sprite() {
    use crate::map::spatial::Placed;
    use crate::objects::sprite::Graphic;
    let mut t = sent_off_world(vec![]);
    open_map(&mut t);
    t.spatial.relations = vec![relation_row([0, 3]), relation_row([3, 0])];
    let placed = |position, class, master, radius| Placed {
        position,
        radius,
        class,
        player: PlayerId::new(1),
        master,
        obstruction_type: ObstructionType::NONE,
        buildable_on: 0,
        tall: true,
        speed: Speed::ZERO,
        unknown: None,
        obstruction: None,
    };
    let at = Position::new(26.5, 89.5, 0.0);
    t.spatial.insert(
        object_id(9175),
        placed(at, class::VILLAGER, VILLAGER_MALE, Footprint::new(0.2, 0.2)),
    );
    t.spatial.insert(
        object_id(10028),
        placed(
            Position::new(30.0, 88.0, 0.0),
            class::BUILDING,
            ids::HOUSE,
            Footprint::new(1.0, 1.0),
        ),
    );
    let v = &mut t.players[1].objects[ListIndex::new(0)];
    v.place_for_test(at);
    let mut mover = crate::gathering::hunt::tests::walker();
    mover.moved_at = at;
    v.body.put_mover(Some(mover));
    // a walker of sixteen facets (its yaw reads them)
    for table in &mut t.masters {
        if let Some(m) = table.get_mut(ids::BUILDER).and_then(|m| m.moving.as_mut()) {
            m.facets = Some(crate::objects::facing::SpriteFacets {
                count: 16,
                flag_1: true,
            });
        }
    }
    t.build_mesh().unwrap();
    let work = crate::orders::Work {
        target: Some(object_id(10028)),
        point: Position::new(30.0, 88.0, 0.0),
        carry_test: false,
        filter: None,
        notified: true,
    };
    let walking = Graphic {
        id: SpriteId::named(7),
        frames: 10,
        facets: 16,
        speed_multiplier: 1.0,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: 1,
    };
    let _ = t.graphics.insert(SpriteId::named(7), walking);
    let shown_after_update = |t: &mut World| {
        in_update(t, (PlayerId::new(1), ListIndex::new(0)), 50, |unit, c| {
            let me = crate::movement::Me {
                id: unit.id,
                group: unit.group.id,
                action_state: unit.top_action().map_or(0, |a| a.state),
            };
            let _ = unit.build_part(c, me)?;
            Ok(unit.sprite.map(|s| s.graphic.id))
        })
        .unwrap()
    };
    // the player's master 118 has no sprites: the MoveTo shows none
    let mut table = t.clone();
    table
        .order_work(PlayerId::new(1), ListIndex::new(0), &work)
        .unwrap();
    assert_eq!(shown_after_update(&mut table), None);
    // the MoveTo's constructor (FUN_140aa0df0) reads the builder's own master for its sprite
    let own = Master {
        move_sprite: Some(SpriteId::named(7)),
        ..t.masters[1].get(ids::BUILDER).unwrap().clone()
    };
    t.players[1].objects[ListIndex::new(0)].own_master = Some(Box::new(own));
    t.order_work(PlayerId::new(1), ListIndex::new(0), &work)
        .unwrap();
    assert_eq!(shown_after_update(&mut t), Some(SpriteId::named(7)));
}

/// The work command again on the Build's own foundation (`FUN_140ab1b30`): a Build at work
/// (state 6) keeps on; one counted among the builders that waits (state 222) leaves them and
/// walks again (state 4), its `MoveTo` arrived where the footprints touch.
#[test]
fn a_work_command_on_the_build_s_own_foundation_keeps_the_work_or_walks_again() {
    let mut t = sent_off_world(vec![]);
    let work = crate::orders::Work {
        target: Some(object_id(10028)),
        point: Position::new(30.0, 88.0, 0.0),
        carry_test: false,
        filter: None,
        notified: true,
    };
    t.order_work(PlayerId::new(1), ListIndex::new(0), &work)
        .unwrap();
    t.players[1].objects[ListIndex::new(0)]
        .actions
        .build_mut()
        .unwrap()
        .state = BuildState::Work;
    t.order_work(PlayerId::new(1), ListIndex::new(0), &work)
        .unwrap();
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .build()
            .map(|b| b.state),
        Some(BuildState::Work)
    );
    {
        let b = t.players[1].objects[ListIndex::new(0)]
            .actions
            .build_mut()
            .unwrap();
        (b.state, b.added, b.move_to) = (BuildState::Wait, true, None);
    }
    t.players[1].objects[ListIndex::new(1)]
        .construction
        .as_mut()
        .unwrap()
        .builders = 1;
    t.players[1].objects[ListIndex::new(0)].place_for_test(Position::new(29.0, 88.0, 0.0));
    t.order_work(PlayerId::new(1), ListIndex::new(0), &work)
        .unwrap();
    let b = t.players[1].objects[ListIndex::new(0)]
        .actions
        .build()
        .copied()
        .unwrap();
    assert_eq!(
        (b.state, b.added, b.move_to.map(|m| m.state), b.target),
        (
            BuildState::Move,
            false,
            Some(MoveTo::Arrived),
            Some(object_id(10028))
        )
    );
    assert_eq!(
        t.players[1].objects[ListIndex::new(1)]
            .construction
            .map(|k| k.builders),
        Some(0)
    );
    // the same unit's work command on itself does nothing
    let before = t.players[1].objects[ListIndex::new(0)].clone();
    t.order_work(
        PlayerId::new(1),
        ListIndex::new(0),
        &crate::orders::Work {
            target: Some(object_id(9175)),
            ..work
        },
    )
    .unwrap();
    assert_eq!(t.players[1].objects[ListIndex::new(0)], before);
}

/// The work command on another foundation of the player's while building (`FUN_140ab0300`):
/// a counted builder leaves the first foundation's builders, lets it go and takes the new
/// one, walking to it again (state 4).
#[test]
fn a_work_command_on_another_foundation_moves_the_build_there() {
    let mut t = sent_off_world(vec![]);
    let other = Unit {
        id: object_id(10031),
        body: crate::objects::layers::Body::at(Position::new(34.0, 88.0, 0.0)),
        ..t.players[1].objects[ListIndex::new(1)].clone()
    };
    t.players[1].objects.push(other);
    let work = |target| crate::orders::Work {
        target,
        point: Position::new(30.0, 88.0, 0.0),
        carry_test: false,
        filter: None,
        notified: true,
    };
    t.order_work(
        PlayerId::new(1),
        ListIndex::new(0),
        &work(Some(object_id(10028))),
    )
    .unwrap();
    {
        let b = t.players[1].objects[ListIndex::new(0)]
            .actions
            .build_mut()
            .unwrap();
        (b.state, b.added) = (BuildState::Wait, true);
    }
    t.players[1].objects[ListIndex::new(1)]
        .construction
        .as_mut()
        .unwrap()
        .builders = 1;
    t.order_work(
        PlayerId::new(1),
        ListIndex::new(0),
        &work(Some(object_id(10031))),
    )
    .unwrap();
    let b = t.players[1].objects[ListIndex::new(0)]
        .actions
        .build()
        .copied()
        .unwrap();
    assert_eq!(
        (
            b.target,
            b.state,
            b.added,
            b.point,
            b.move_to.map(|m| ObjectId::field(m.target))
        ),
        (
            Some(object_id(10031)),
            BuildState::Move,
            false,
            Position::new(34.0, 88.0, 0.0),
            Some(10031)
        )
    );
    let site = |i: ListIndex| {
        let o = &t.players[1].objects[i];
        (o.workers, o.construction.map(|k| k.builders))
    };
    assert_eq!(
        (site(ListIndex::new(1)), site(ListIndex::new(2))),
        ((0, Some(0)), (1, Some(0)))
    );
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .ai
            .as_ref()
            .map(|ai| ai.current_target),
        Some(Some(object_id(10031)))
    );
}

/// The unit AI's queued order on a target (`FUN_140c5c170` case `0x2db`): the unit's work
/// command on its foundation at the foundation's place (the villager becomes the builder and
/// starts the Build), then the tail with the order the work gave the AI (the build order) and
/// the queued order's priority. An order on the unit itself works on nothing but still takes
/// the tail; one on a target that is gone, or for a unit without a unit AI, is not modelled.
#[test]
fn a_queued_order_on_a_target_works_on_it_with_the_order_s_priority() {
    use crate::unit_ai::QueuedOrder;
    let order = |target| QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::OnTarget,
        priority: 3,
        target,
        target_owner: owner(1),
        point: Position::ORIGIN,
        range: Distance::ZERO,
        shown: Point::ORIGIN,
        ready: true,
        target_type: order_type(80),
    };
    let mut t = sent_off_world(vec![]);
    t.order_on_target(
        PlayerId::new(1),
        ListIndex::new(0),
        order(Some(object_id(10028))),
    )
    .unwrap();
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(v.life.master_id, ids::BUILDER);
    assert_eq!(
        v.actions.build().map(|b| (b.target, b.state)),
        Some((Some(object_id(10028)), BuildState::Move))
    );
    let ai = v.ai.as_ref().unwrap();
    assert_eq!(
        (ai.current_order, ai.order_priority, ai.current_target),
        (Some(OrderKind::Build), 3, Some(object_id(10028)))
    );
    let mut t = sent_off_world(vec![]);
    t.order_on_target(
        PlayerId::new(1),
        ListIndex::new(0),
        order(Some(object_id(9175))),
    )
    .unwrap();
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(v.actions.build().copied(), None);
    assert_eq!(
        v.ai.as_ref()
            .map(|ai| (ai.current_order, ai.order_priority)),
        Some((None, 3))
    );
    assert!(
        t.order_on_target(
            PlayerId::new(1),
            ListIndex::new(0),
            order(Some(object_id(4444)))
        )
        .is_err()
    );
    t.players[1].objects[ListIndex::new(0)].ai = None;
    assert!(
        t.order_on_target(
            PlayerId::new(1),
            ListIndex::new(0),
            order(Some(object_id(9175)))
        )
        .is_err()
    );
}

/// [`placeable`]'s map with its tiles (class 4, open), on which the walker's restriction 7
/// lets a unit stand anywhere: the spatial table starts empty.
pub(crate) fn open_map(t: &mut World) {
    t.spatial = Spatial::with_tiles(
        GridSize::new(120, 120),
        crate::map::spatial::Tiles::new(GridSize::new(120, 120), vec![4; 120 * 120]),
    );
    placeable(t);
    if let Some(terrain) = t.spatial.terrain.as_mut() {
        terrain.restrictions = vec![vec![1.0]; 8];
    }
}

/// [`sent_off_world`] with its villager the forager 120 (a Gather task on class 7 for food,
/// dropping at the town centre 109, as the drop-site table has it), Gaia's berry bushes 600 and
/// 601 (master 59, class 7) and player 1's town centre 700, all placed on [`open_map`]; the
/// villager can move.
pub(crate) fn forage_world() -> World {
    let mut t = sent_off_world(vec![]);
    for masters in &mut t.masters {
        let forager = Master {
            id: master_id(120),
            capacity: 10,
            drop_sites: vec![ids::TOWN_CENTRE],
            tasks: vec![crate::objects::masters::Task {
                id: TaskId::test(2),
                action_type: crate::gathering::gather::GATHER,
                object_group: Some(class_id(7)),
                attribute_type: Some(AttributeId::numbered(17)),
                work_range: Distance::new(0.25),
                work_val1: 0.31,
                ..crate::objects::masters::Task::default()
            }],
            ..masters.get(VILLAGER_MALE).unwrap().clone()
        };
        let bush = Master {
            id: master_id(59),
            kind: master_type::MOVING,
            class: class::CLASS_7,
            radius: Footprint::new(0.5, 0.5),
            ..Master::default()
        };
        let centre = Master {
            id: ids::TOWN_CENTRE,
            kind: BUILDING,
            class: class::BUILDING,
            radius: Footprint::new(2.0, 2.0),
            ..Master::default()
        };
        *masters = Masters::new(masters.iter().cloned().chain([forager, bush, centre]));
    }
    let resource = |id, x| Unit {
        id,
        body: crate::objects::layers::Body::at(Position::new(x, 89.5, 0.0)),
        life: Lifecycle {
            state: ObjectState::Alive,
            hp: HitPoints::new(1.0),
            master_id: master_id(59),
            attribute_type: Some(AttributeId::numbered(17)),
            amount: Amount::new(125.0),
            ..Lifecycle::default()
        },
        pasture: None,
        my_pasture: None,
        ..Unit::default()
    };
    t.players[0].objects = vec![
        resource(object_id(600), 32.5),
        resource(object_id(601), 36.5),
    ]
    .into();
    t.players[1].objects.push(Unit {
        id: object_id(700),
        life: Lifecycle {
            state: ObjectState::Alive,
            hp: HitPoints::new(2400.0),
            master_id: ids::TOWN_CENTRE,
            ..Lifecycle::default()
        },
        // a building is an animated object (type 80)
        body: crate::objects::layers::Body::new(
            Position::new(20.0, 89.5, 0.0),
            Some(crate::objects::layers::Animated::default()),
            None,
        ),
        ..Unit::default()
    });
    t.players[1].objects[ListIndex::new(0)]
        .body
        .put_mover(Some(crate::gathering::hunt::tests::walker()));
    t.players[1].objects[ListIndex::new(0)].life.master_id = master_id(120);
    open_map(&mut t);
    let placed = |x: f32, class, player, master, radius| Placed {
        position: Position::new(x, 89.5, 0.0),
        radius,
        class,
        player,
        master,
        obstruction_type: ObstructionType::NONE,
        buildable_on: 0,
        tall: true,
        speed: Speed::ZERO,
        unknown: None,
        obstruction: None,
    };
    t.spatial.insert(
        object_id(9175),
        placed(
            29.5,
            class::VILLAGER,
            PlayerId::new(1),
            VILLAGER_MALE,
            Footprint::new(0.2, 0.2),
        ),
    );
    t.spatial.insert(
        object_id(600),
        placed(
            32.5,
            class::CLASS_7,
            PlayerId::new(0),
            master_id(59),
            Footprint::new(0.5, 0.5),
        ),
    );
    t.spatial.insert(
        object_id(601),
        placed(
            36.5,
            class::CLASS_7,
            PlayerId::new(0),
            master_id(59),
            Footprint::new(0.5, 0.5),
        ),
    );
    t.spatial.insert(
        object_id(700),
        placed(
            20.0,
            class::BUILDING,
            PlayerId::new(1),
            ids::TOWN_CENTRE,
            Footprint::new(2.0, 2.0),
        ),
    );
    // the drop-site table: villagers drop berries at the town centre
    t.drop_sites = vec![DropSite {
        building: ids::TOWN_CENTRE,
        worker_class: Some(class::VILLAGER),
        targets: vec![crate::unit_ai::task_finder::DropTarget {
            class: class::CLASS_7,
            workers: -1,
            state: None,
            attribute: Some(AttributeId::numbered(17)),
        }],
        ..DropSite::default()
    }];
    t
}

/// The work command on Gaia's berry bush (the factory's case 5, `FUN_140a97e00`): the forager's
/// Gather takes the bush (one worker), the unit
/// AI hears order 0x2c5 with action 0x261 on it and the Gather walks to it (state 4) with the
/// task's range. The command on the other bush while it walks (slot 0x48, `0x140a903a0`) moves
/// the Gather there: the first bush is let go, the second taken.
#[test]
fn a_work_command_on_a_bush_starts_a_gather_and_another_moves_it() {
    let mut t = forage_world();
    let work = |target| crate::orders::Work {
        target,
        point: Position::new(32.5, 89.5, 0.0),
        carry_test: false,
        filter: None,
        notified: true,
    };
    t.order_work(
        PlayerId::new(1),
        ListIndex::new(0),
        &work(Some(object_id(600))),
    )
    .unwrap();
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(v.life.master_id, master_id(120));
    assert_eq!(v.top_action_kind(), Some(crate::gathering::gather::GATHER));
    let g = v.actions.gather().copied().unwrap();
    assert_eq!(
        (
            g.target,
            g.carry_to,
            g.state,
            g.point,
            g.move_to.map(|m| m.range),
            g.task.id
        ),
        (
            Some(object_id(600)),
            None,
            GatherState::Walk,
            Position::new(32.5, 89.5, 0.0),
            Some(Distance::new(0.25)),
            TaskId::test(2)
        )
    );
    let ai = v.ai.as_ref().unwrap();
    assert_eq!(
        (ai.current_order, ai.current_action, ai.current_target),
        (
            Some(OrderKind::Gather),
            Some(ActionKind::Gather),
            Some(object_id(600))
        )
    );
    let workers = |t: &World| {
        (
            t.players[0].objects[ListIndex::new(0)].workers,
            t.players[0].objects[ListIndex::new(1)].workers,
        )
    };
    assert_eq!(workers(&t), (1, 0));
    t.order_work(
        PlayerId::new(1),
        ListIndex::new(0),
        &work(Some(object_id(601))),
    )
    .unwrap();
    let g = t.players[1].objects[ListIndex::new(0)]
        .actions
        .gather()
        .copied()
        .unwrap();
    assert_eq!(
        (g.target, g.state, g.point),
        (
            Some(object_id(601)),
            GatherState::Walk,
            Position::new(36.5, 89.5, 0.0)
        )
    );
    assert_eq!(workers(&t), (0, 1));
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .ai
            .as_ref()
            .map(|ai| ai.current_target),
        Some(Some(object_id(601)))
    );
    // at work on it (state 7) the same command keeps the Gather as it is
    t.players[1].objects[ListIndex::new(0)]
        .actions
        .gather_mut()
        .unwrap()
        .state = GatherState::Work;
    let before = t.players[1].objects[ListIndex::new(0)]
        .actions
        .gather()
        .copied();
    t.order_work(
        PlayerId::new(1),
        ListIndex::new(0),
        &work(Some(object_id(601))),
    )
    .unwrap();
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .gather()
            .copied(),
        before
    );
    assert_eq!(workers(&t), (0, 1));
}

/// The unit AI's gather command (`FUN_141085030`, forced): a new Gather on the bush replaces
/// the forager's actions without the unit AI's notify (the AI set its order itself); the command on the other bush while the Gather on the same task is on
/// top retargets it (slot 0x48): the first bush let go, the second taken.
#[test]
fn the_unit_ai_s_gather_command_starts_a_gather_or_retargets_its_own() {
    let mut t = forage_world();
    t.gather_command(PlayerId::new(1), ListIndex::new(0), object_id(600))
        .unwrap();
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(v.life.master_id, master_id(120));
    let g = v.actions.gather().copied().unwrap();
    assert_eq!(
        (g.target, g.state, g.task.id),
        (Some(object_id(600)), GatherState::Walk, TaskId::test(2))
    );
    assert_eq!(v.ai.as_ref().map(|ai| ai.current_order), Some(None));
    t.gather_command(PlayerId::new(1), ListIndex::new(0), object_id(601))
        .unwrap();
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .gather()
            .map(|g| g.target),
        Some(Some(object_id(601)))
    );
    assert_eq!(
        (
            t.players[0].objects[ListIndex::new(0)].workers,
            t.players[0].objects[ListIndex::new(1)].workers
        ),
        (0, 1)
    );
    // an object the world does not hold fails the command
    assert!(
        t.gather_command(PlayerId::new(1), ListIndex::new(0), object_id(4444))
            .is_err()
    );
}

/// A loaded forager's work command on its own town centre, a drop site of its master: the task
/// finder's drop-off gives the Gather task for what it carries, and the Gather started on the
/// centre carries the load there (`0x140a8b3a0`: state 8, the centre its second target, no
/// target or point); the centre, taken as the target and again as the second target, then let
/// go as the first, counts one worker. A Gather at work on its bush sent there (slot 0x48 on a
/// building that is no farm answers 0) starts the same new Gather.
#[test]
fn a_loaded_forager_sent_to_its_town_centre_carries_its_load_there() {
    let work = crate::orders::Work {
        target: Some(object_id(700)),
        point: Position::new(20.0, 89.5, 0.0),
        carry_test: false,
        filter: None,
        notified: true,
    };
    let loaded = || {
        let mut t = forage_world();
        let v = &mut t.players[1].objects[ListIndex::new(0)];
        (v.life.attribute_type, v.life.amount) =
            (Some(AttributeId::numbered(17)), Amount::new(6.0));
        t
    };
    let carries = |t: &World| {
        let v = &t.players[1].objects[ListIndex::new(0)];
        let g = v.actions.gather().copied().unwrap();
        (
            (g.target, g.carry_to, g.state, g.point),
            v.life.held(),
            t.players[1].objects[ListIndex::new(2)].workers,
        )
    };
    let loaded_to_700 = (
        (
            None,
            Some(object_id(700)),
            GatherState::Drop,
            Position::from([-1.0; 3]),
        ),
        carrying(Some(AttributeId::numbered(17)), 6.0),
        1,
    );
    let mut t = loaded();
    t.order_work(PlayerId::new(1), ListIndex::new(0), &work)
        .unwrap();
    assert_eq!(carries(&t), loaded_to_700);
    let mut t = loaded();
    t.gather_command(PlayerId::new(1), ListIndex::new(0), object_id(600))
        .unwrap();
    t.players[1].objects[ListIndex::new(0)]
        .actions
        .gather_mut()
        .unwrap()
        .state = GatherState::Work;
    t.order_work(PlayerId::new(1), ListIndex::new(0), &work)
        .unwrap();
    assert_eq!(carries(&t), loaded_to_700);
    assert_eq!(t.players[0].objects[ListIndex::new(0)].workers, 0);
}

/// The work command without a task (`FUN_14106ff70`'s move): the forager sent to its town
/// centre empty-handed (no task on class 3) is told of the move (order 0x2c6, the centre with
/// its class) and walks to the centre; sent to a point it walks there; in state 1 it is told of
/// no order (−1) and stays; a master that does not move does nothing.
#[test]
fn a_work_command_without_a_task_moves_the_unit_there() {
    let work = |target| crate::orders::Work {
        target,
        point: Position::new(20.0, 89.5, 0.0),
        carry_test: false,
        filter: None,
        notified: true,
    };
    let mut t = forage_world();
    t.order_work(
        PlayerId::new(1),
        ListIndex::new(0),
        &work(Some(object_id(700))),
    )
    .unwrap();
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert!(v.actions.move_to().is_some());
    let ai = v.ai.as_ref().unwrap();
    assert_eq!(
        (ai.current_order, ai.current_action, ai.current_target),
        (
            Some(OrderKind::Move),
            Some(ActionKind::Move),
            Some(object_id(700))
        )
    );
    let mut t = forage_world();
    t.order_work(PlayerId::new(1), ListIndex::new(0), &work(None))
        .unwrap();
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert!(v.actions.move_to().is_some());
    assert_eq!(
        v.ai.as_ref().map(|ai| ai.current_order),
        Some(Some(OrderKind::Move))
    );
    let mut t = forage_world();
    t.players[1].objects[ListIndex::new(0)].life.state = ObjectState::Unfinished;
    t.order_work(
        PlayerId::new(1),
        ListIndex::new(0),
        &work(Some(object_id(700))),
    )
    .unwrap();
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(v.actions.move_to().copied(), None);
    assert_eq!(v.ai.as_ref().map(|ai| ai.current_order), Some(None));
    let mut t = forage_world();
    t.masters[1].get_mut(master_id(120)).unwrap().moving = None;
    let before = t.players[1].objects[ListIndex::new(0)].clone();
    t.order_work(
        PlayerId::new(1),
        ListIndex::new(0),
        &work(Some(object_id(700))),
    )
    .unwrap();
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .move_to()
            .copied(),
        before.actions.move_to().copied()
    );
}

/// A move toward an object while gathering (`FUN_140a97730`, the Gather's slot 0x40,
/// `0x140a8b740`): the Gather takes the move; toward the other bush it walks to the move's point
/// (state 15) without a target, the first bush let go. Loaded and moved toward its town centre,
/// a drop site, it carries the load there (state 8, the centre its second target, taken).
#[test]
fn a_gather_takes_a_move_toward_an_object_as_its_point_or_drop_site() {
    let mut t = forage_world();
    t.gather_command(PlayerId::new(1), ListIndex::new(0), object_id(600))
        .unwrap();
    t.players[1].objects[ListIndex::new(0)]
        .actions
        .gather_mut()
        .unwrap()
        .state = GatherState::Work;
    let point = Position::new(36.0, 89.0, 0.0);
    assert_eq!(
        t.move_to_object_with(
            PlayerId::new(1),
            ListIndex::new(0),
            object_id(601),
            point,
            false
        ),
        Ok(true)
    );
    let g = t.players[1].objects[ListIndex::new(0)]
        .actions
        .gather()
        .copied()
        .unwrap();
    assert_eq!(
        (
            g.target,
            g.carry_to,
            g.state,
            g.point,
            g.move_to.and_then(|m| m.goal())
        ),
        (None, None, GatherState::Moved, point, Some(point))
    );
    assert_eq!(
        (
            t.players[0].objects[ListIndex::new(0)].workers,
            t.players[0].objects[ListIndex::new(1)].workers
        ),
        (0, 0)
    );
    let v = &mut t.players[1].objects[ListIndex::new(0)];
    (v.life.attribute_type, v.life.amount) = (Some(AttributeId::numbered(17)), Amount::new(6.0));
    assert_eq!(
        t.move_to_object_with(
            PlayerId::new(1),
            ListIndex::new(0),
            object_id(700),
            Position::new(20.0, 89.5, 0.0),
            false
        ),
        Ok(true)
    );
    let g = t.players[1].objects[ListIndex::new(0)]
        .actions
        .gather()
        .copied()
        .unwrap();
    assert_eq!(
        (g.target, g.carry_to, g.state, g.point),
        (
            None,
            Some(object_id(700)),
            GatherState::Drop,
            Position::from([-1.0; 3])
        )
    );
    assert_eq!(t.players[1].objects[ListIndex::new(2)].workers, 1);
    // a move toward the drop site it already carries to changes nothing
    let before = t.players[1].objects[ListIndex::new(0)].clone();
    assert_eq!(
        t.move_to_object_with(
            PlayerId::new(1),
            ListIndex::new(0),
            object_id(700),
            Position::new(20.0, 89.5, 0.0),
            false
        ),
        Ok(true)
    );
    assert_eq!(t.players[1].objects[ListIndex::new(0)], before);
    assert_eq!(t.players[1].objects[ListIndex::new(2)].workers, 1);
}

/// `UNIT_ORDER`'s drop-off 6 of a loaded forager without an action (`FUN_14161e370`, then
/// [`World::drop_off_unasked`]): its master drops at the town centre, so it seeks its drop site
/// from itself (`FUN_1411671e0`: its zone, the planner; by the squared distance) and works on the
/// nearer of the player's two centres in state 2, its Gather carrying the load there. A master
/// dropping at the dock (45) too searches without a seeker; with no drop site found it does
/// nothing.
#[test]
fn a_drop_off_order_takes_a_forager_to_its_nearest_drop_site() {
    let mut t = forage_world();
    let far = Unit {
        id: object_id(701),
        body: crate::objects::layers::Body::at(Position::new(60.0, 89.5, 0.0)),
        ..t.players[1].objects[ListIndex::new(2)].clone()
    };
    t.players[1].objects.push(far);
    let v = &mut t.players[1].objects[ListIndex::new(0)];
    (v.life.attribute_type, v.life.amount) = (Some(AttributeId::numbered(17)), Amount::new(6.0));
    let world = t.clone();
    t.unit_order(player_id(1), &object_ids([9175]), 6).unwrap();
    let v = &t.players[1].objects[ListIndex::new(0)];
    let g = v
        .actions
        .gather()
        .copied()
        .expect("a Gather to the drop site");
    assert_eq!(
        (g.carry_to, g.state),
        (Some(object_id(700)), GatherState::Drop)
    );
    assert_eq!(t.players[1].objects[ListIndex::new(2)].workers, 1);
    // dropping at the dock too: the search without a seeker finds the same centre
    let mut t = world.clone();
    for masters in &mut t.masters {
        masters.get_mut(master_id(120)).unwrap().drop_sites = vec![ids::ID_45, ids::TOWN_CENTRE];
    }
    t.unit_order(player_id(1), &object_ids([9175]), 6).unwrap();
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .gather()
            .map(|g| (g.carry_to, g.state)),
        Some((Some(object_id(700)), GatherState::Drop))
    );
    // no centre standing (state 2): no drop site, no table entry for berries by class
    let mut t = world;
    for i in [2, 3] {
        t.players[1].objects[ListIndex::new(i)].life.state = ObjectState::Unfinished;
    }
    t.unit_order(player_id(1), &object_ids([9175]), 6).unwrap();
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .gather()
            .copied(),
        None
    );
}

/// The Empire Wars start (`BaseWorld::update`'s first update) under the farm setting: the idle
/// forager standing on its player's farm 800 works it (the first of class 49 on its tile), its
/// Gather on the farm at the farm's place; the foundation, the town centre and the farm take
/// nothing. A villager with orders waits; a fishing ship without a fish trap under it, and the
/// pass without the farm setting, are not modelled.
#[test]
fn the_empire_wars_start_puts_a_villager_on_its_farm_to_work() {
    let mut t = forage_world();
    t.settings.walkable_farms = true;
    for masters in &mut t.masters {
        masters
            .get_mut(master_id(120))
            .unwrap()
            .tasks
            .push(crate::objects::masters::Task {
                id: TaskId::test(3),
                action_type: crate::gathering::gather::GATHER,
                object_group: Some(class_id(49)),
                attribute_type: Some(attribute::FOOD),
                work_range: Distance::new(0.25),
                ..crate::objects::masters::Task::default()
            });
        let farm = Master {
            id: ids::FARM,
            kind: BUILDING,
            class: class::FARM,
            radius: Footprint::new(1.5, 1.5),
            ..Master::default()
        };
        let fishing = Master {
            id: master_id(13),
            class: class::FISHING_SHIP,
            ..masters.get(master_id(120)).unwrap().clone()
        };
        *masters = Masters::new(masters.iter().cloned().chain([farm, fishing]));
    }
    t.players[1].objects.push(Unit {
        id: object_id(800),
        body: crate::objects::layers::Body::at(Position::new(29.5, 89.5, 0.0)),
        life: Lifecycle {
            state: ObjectState::Alive,
            hp: HitPoints::new(480.0),
            master_id: ids::FARM,
            attribute_type: Some(attribute::FOOD),
            amount: Amount::new(175.0),
            ..Lifecycle::default()
        },
        pasture: None,
        my_pasture: None,
        ..Unit::default()
    });
    t.spatial.insert(
        object_id(800),
        Placed {
            position: Position::new(29.5, 89.5, 0.0),
            radius: Footprint::new(1.5, 1.5),
            class: class::FARM,
            player: PlayerId::new(1),
            master: ids::FARM,
            obstruction_type: ObstructionType::NONE,
            buildable_on: 0,
            tall: false,
            speed: Speed::ZERO,
            unknown: None,
            obstruction: None,
        },
    );
    let world = t.clone();
    t.start_work(PlayerId::new(1)).unwrap();
    let g = t.players[1].objects[ListIndex::new(0)]
        .actions
        .gather()
        .copied()
        .expect("the farm's Gather");
    assert_eq!(
        (g.target, g.task.id, g.point),
        (
            Some(object_id(800)),
            TaskId::test(3),
            Position::new(29.5, 89.5, 0.0)
        )
    );
    assert_eq!(t.players[1].objects[ListIndex::new(3)].workers, 1);
    let others = |t: &World| {
        t.players[1].objects.as_slice()[1..]
            .iter()
            .map(Unit::top_action_kind)
            .collect::<Vec<_>>()
    };
    assert_eq!(others(&t), [None, None, None]);
    // a villager with an order in its queue waits for it
    let mut t = world.clone();
    t.players[1].objects[ListIndex::new(0)]
        .ai
        .as_mut()
        .unwrap()
        .orders
        .push(crate::unit_ai::QueuedOrder {
            issuer: Some(Issuer::Player(player_id(1))),
            order: OrderKind::Move,
            priority: 100,
            target: None,
            target_owner: None,
            point: Position::new(10.0, 10.0, 0.0),
            range: Distance::ZERO,
            shown: Point::new(10.0, 10.0),
            ready: true,
            target_type: None,
        });
    t.start_work(PlayerId::new(1)).unwrap();
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .gather()
            .copied(),
        None
    );
    // a fishing ship finds no fish trap on its tile
    let mut t = world.clone();
    t.players[1].objects[ListIndex::new(0)].life.master_id = master_id(13);
    assert!(t.start_work(PlayerId::new(1)).is_err());
    let mut t = world;
    t.settings.walkable_farms = false;
    assert!(t.start_work(PlayerId::new(1)).is_err());
}

/// The forager's unit AI run outside its update making the calls `effects` on its unit
/// ([`World::run_unit_ai`]): each at once, in order ([`crate::unit_ai::UnitSide::effect_now`]).
fn with_effects(t: &mut World, effects: Vec<Effect>) -> Result<(), GenieError> {
    t.run_unit_ai(
        (PlayerId::new(1), ListIndex::new(0)),
        move |ai, _, side, _, rng| {
            effects
                .into_iter()
                .try_for_each(|e| side.effect_now(e, ai, rng))
        },
    )
}

/// The unit AI's explore (slot 0x398, `FUN_141084db0`) of the idle forager: a new Explore to
/// the point at its height walks there (state 11, its `MoveTo` starting); a point off the map's
/// quarter for the Explore on top makes it wait (state 3), its walk stopped, and ask its unit
/// AI for a point (notify 0x202 with the explore's action). An Explore replacing a Gather is
/// not modelled.
#[test]
fn the_unit_ai_s_explore_walks_to_its_point_or_asks_for_one() {
    use crate::movement::explore::ExploreState;
    use crate::unit_ai::Effect;
    let mut t = forage_world();
    with_effects(&mut t, vec![Effect::Explore(Point::new(40.0, 90.0))]).unwrap();
    let e = t.players[1].objects[ListIndex::new(0)]
        .actions
        .explore()
        .copied()
        .unwrap();
    assert_eq!(
        (e.state, e.point),
        (ExploreState::Walk, Position::new(40.0, 90.0, 0.0))
    );
    assert!(e.move_to.is_some());
    with_effects(&mut t, vec![Effect::Explore(Point::new(-1.0, 90.0))]).unwrap();
    let v = &t.players[1].objects[ListIndex::new(0)];
    let e = v.actions.explore().copied().unwrap();
    assert_eq!((e.state, e.move_to), (ExploreState::Wait, None));
    let notifies = &v.ai.as_ref().unwrap().notifies;
    assert_eq!(
        notifies.last().map(|n| (n.kind, n.p)),
        Some((
            NotifyKind::REQUIRES_SEARCH,
            [ActionKind::Explore.value(), 0, 0]
        ))
    );
    let mut t = forage_world();
    t.gather_command(PlayerId::new(1), ListIndex::new(0), object_id(600))
        .unwrap();
    assert!(with_effects(&mut t, vec![Effect::Explore(Point::new(40.0, 90.0))]).is_err());
}

/// The unit AI's move to an object (slot 0x348, `FUN_141086440`): the forager's Gather goes (its
/// bush let go), a `MoveTo` to the other bush replaces it within the range given; the unit AI's
/// cap of its last human order (slot 0x300) and its works follow in order: a work on the bush
/// (slot 0x7e8 with the carried-resource test) starts a Gather there.
#[test]
fn the_unit_ai_s_moves_and_works_apply_in_order() {
    use crate::unit_ai::Effect;
    let mut t = forage_world();
    t.gather_command(PlayerId::new(1), ListIndex::new(0), object_id(600))
        .unwrap();
    t.players[1].objects[ListIndex::new(0)].last_human_order = WorldTime::from_millis(900);
    with_effects(
        &mut t,
        vec![
            Effect::CapHumanOrder(WorldTime::from_millis(300)),
            Effect::MoveToObject {
                target: object_id(601),
                state: ObjectState::Alive,
                range: Distance::new(0.5),
            },
        ],
    )
    .unwrap();
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        (v.actions.gather().copied(), v.last_human_order),
        (None, WorldTime::from_millis(300))
    );
    assert!(v.actions.move_to().is_some());
    assert_eq!(t.players[0].objects[ListIndex::new(0)].workers, 0);
    with_effects(
        &mut t,
        vec![Effect::Command(AiCommand::WorkOn {
            target: object_id(601),
            point: Position::new(36.5, 89.5, 0.0),
        })],
    )
    .unwrap();
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        v.actions.gather().map(|g| (g.target, g.state)),
        Some((Some(object_id(601)), GatherState::Walk))
    );
    assert_eq!(v.actions.move_to().copied(), None);
    // a unit that does not move cannot be moved to an object
    t.players[1].objects[ListIndex::new(0)].body.put_mover(None);
    assert!(
        with_effects(
            &mut t,
            vec![Effect::MoveToObject {
                target: object_id(600),
                state: ObjectState::Alive,
                range: Distance::new(0.5),
            }],
        )
        .is_err()
    );
}

/// The unit AI's hunt and build commands and its order on a target, as its effects give them
/// after the run: `HuntObject` hunts the deer (the hunt command, `FUN_1412151f0`); `BuildObject`
/// sends the villager to build its foundation (the build command, slot 0x380, as the builder);
/// `OrderOnTarget` works on the foundation with the order's priority, at once, a second one
/// again. An AI move of a master the slice does not have is not modelled; a restored patrol
/// without one saved does nothing, and the stop stops.
#[test]
fn the_unit_ai_s_hunt_build_and_target_orders_apply_after_its_run() {
    use crate::unit_ai::{AiMove, Effect, QueuedOrder};
    let mut t = hunt_world();
    with_effects(
        &mut t,
        vec![Effect::Command(AiCommand::HuntObject(object_id(500)))],
    )
    .unwrap();
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .hunt()
            .map(|h| h.target),
        Some(Some(object_id(500)))
    );
    with_effects(&mut t, vec![Effect::Stop]).unwrap();
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)].top_action_kind(),
        None
    );
    let mut t = sent_off_world(vec![]);
    with_effects(
        &mut t,
        vec![Effect::Command(AiCommand::BuildObject(object_id(10028)))],
    )
    .unwrap();
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(v.life.master_id, ids::BUILDER);
    assert_eq!(
        v.actions.build().map(|b| b.target),
        Some(Some(object_id(10028)))
    );
    let mut t = sent_off_world(vec![]);
    let order = QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::OnTarget,
        priority: 3,
        target: Some(object_id(10028)),
        target_owner: owner(1),
        point: Position::ORIGIN,
        range: Distance::ZERO,
        shown: Point::ORIGIN,
        ready: true,
        target_type: order_type(80),
    };
    with_effects(&mut t, vec![Effect::OrderOnTarget(order)]).unwrap();
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        v.actions.build().map(|b| b.target),
        Some(Some(object_id(10028)))
    );
    assert_eq!(v.ai.as_ref().map(|ai| ai.order_priority), Some(3));
    let mut t = sent_off_world(vec![]);
    let twice = vec![Effect::OrderOnTarget(order), Effect::OrderOnTarget(order)];
    assert_eq!(with_effects(&mut t, twice), Ok(()));
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        v.actions.build().map(|b| b.target),
        Some(Some(object_id(10028)))
    );
    let mut t = sent_off_world(vec![]);
    t.players[1].objects[ListIndex::new(0)].life.master_id = master_id(4444);
    let walk = AiMove {
        point: Position::new(20.0, 20.0, 0.0),
        range: Distance::ZERO,
        force: false,
        walk: false,
    };
    assert!(with_effects(&mut t, vec![Effect::Move(walk)]).is_err());
    let mut t = sent_off_world(vec![]);
    let before = t.players[1].objects[ListIndex::new(0)].clone();
    with_effects(&mut t, vec![Effect::RestorePatrol]).unwrap();
    assert_eq!(t.players[1].objects[ListIndex::new(0)], before);
}

/// A full update whose unit AI takes an order on a target (`0x2db`) runs the order and the
/// update's rest after it inside the one update (slot 0x230): the villager builds the
/// foundation with the order's priority within its update, the rest after the order runs (its
/// look-around check redraws the timeout past it), and the regeneration after the unit AI runs
/// once, as in an update without the order.
#[test]
fn an_order_on_a_target_and_the_rest_after_it_run_inside_the_one_update() {
    use crate::unit_ai::QueuedOrder;
    let order = QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::OnTarget,
        priority: 3,
        target: Some(object_id(10028)),
        target_owner: owner(1),
        point: Position::ORIGIN,
        range: Distance::ZERO,
        shown: Point::ORIGIN,
        ready: true,
        target_type: order_type(80),
    };
    let run = |orders| {
        let mut t = forage_world();
        let p = *t.spatial.get(object_id(700)).unwrap();
        let site = Placed {
            position: Position::new(30.0, 88.0, 0.0),
            radius: Footprint::new(1.0, 1.0),
            class: ClassId::of_master(80),
            master: ids::HOUSE,
            ..p
        };
        t.spatial.insert(object_id(10028), site);
        t.players[1].objects[ListIndex::new(0)]
            .ai
            .as_mut()
            .unwrap()
            .orders = orders;
        // the villager, the forager and the builder it becomes regenerate 20 a minute
        for masters in &mut t.masters {
            for id in [83, 118, 120] {
                if let Some(m) = masters.get_mut(master_id(id)) {
                    (m.kind, m.hit_points, m.regeneration) = (master_type::COMBAT, 25, 20.0);
                }
            }
        }
        let v = &mut t.players[1].objects[ListIndex::new(0)];
        v.life.hp = HitPoints::new(20.0);
        let ai = v.ai.as_mut().unwrap();
        (ai.idle_timer, ai.look_around_timer) = (Millis::new(5000), Millis::new(2000));
        t.update_objects(PlayerId::new(1), (Millis::new(50), Some(Millis::new(50))))
            .unwrap();
        let v = &t.players[1].objects[ListIndex::new(0)];
        let ai = v.ai.as_ref().unwrap();
        (
            v.regen_timer,
            v.actions.build().map(|b| b.target),
            (ai.order_priority, ai.orders.len(), ai.look_around_timer),
        )
    };
    let (idle_regen, idle_build, _) = run(vec![]);
    assert_eq!(idle_build, None);
    let (regen, build, (priority, queued, look)) = run(vec![order]);
    assert_eq!(
        (build, priority, queued),
        (Some(Some(object_id(10028))), 3, 0)
    );
    assert_eq!(regen, idle_regen);
    assert!(
        look < Millis::new(2000),
        "the rest after the order ran its look-around check"
    );
}

/// The base handler's case 0x216 from the forager's side ([`UnitSide::off_footprint`],
/// `FUN_1411566b0` with 0): with the forager's height and class, its player, the foundation by
/// its master and the Gather's bush as the action's target; without the foundation placed no
/// spot, placed at (30, 88) the flood's first empty tile beside its footprint, (30, 89).
///
/// [`UnitSide::off_footprint`]: crate::unit_ai::UnitSide::off_footprint
#[test]
fn a_unit_sent_off_a_foundation_finds_the_spot_beside_it() {
    let mut t = forage_world();
    t.gather_command(PlayerId::new(1), ListIndex::new(0), object_id(600))
        .unwrap();
    let mut found = None;
    t.run_unit_ai((PlayerId::new(1), ListIndex::new(0)), |_, _, side, _, _| {
        found = Some(side.off_footprint(object_id(10028), Footprint::new(1.0, 1.0))?);
        Ok(())
    })
    .unwrap();
    let off = found.unwrap();
    assert_eq!(
        (off.spot, off.class, off.player, off.computer),
        (None, class::VILLAGER, player_id(1), false)
    );
    assert_eq!(
        off.site.map(|s| (s.id, s.master, s.owner)),
        Some((object_id(10028), master_id(70), player_id(1)))
    );
    assert_eq!(
        off.action_target.map(|s| (s.id, s.master, s.owner)),
        Some((object_id(600), master_id(59), player_id(0)))
    );
    t.spatial.insert(
        object_id(10028),
        Placed {
            position: Position::new(30.0, 88.0, 0.0),
            radius: Footprint::new(1.0, 1.0),
            class: class::BUILDING,
            player: PlayerId::new(1),
            master: ids::HOUSE,
            obstruction_type: ObstructionType::TYPE_2,
            buildable_on: 0,
            tall: true,
            speed: Speed::ZERO,
            unknown: None,
            obstruction: None,
        },
    );
    let mut found = None;
    t.run_unit_ai((PlayerId::new(1), ListIndex::new(0)), |_, _, side, _, _| {
        found = Some(side.off_footprint(object_id(10028), Footprint::new(1.0, 1.0))?);
        Ok(())
    })
    .unwrap();
    assert_eq!(found.unwrap().spot, Some(Point::new(30.5, 89.5)));
}

/// The auto scout's search from the forager's side ([`UnitSide::seek_unexplored`]): on a map
/// its player explored whole, neither the area about its town centre nor the unit AI's own
/// search (slot 0x90) finds a tile.
///
/// [`UnitSide::seek_unexplored`]: crate::unit_ai::UnitSide::seek_unexplored
#[test]
fn an_explored_map_leaves_the_auto_scout_nothing_to_seek() {
    let mut t = forage_world();
    let mut found = Some(None);
    t.run_unit_ai(
        (PlayerId::new(1), ListIndex::new(0)),
        |ai, _, side, _, rng| {
            found = Some(side.seek_unexplored(ai, rng)?);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(found, Some(None));
}

/// The unit AI's `ClosestResourceObject` from the forager's side (`FUN_140c49760`,
/// [`UnitSide::closest_resource`]): of the bushes (gather group 2, gathering by themselves) the
/// nearer, 600; passing over its current target the other; from its town centre as the drop
/// site the one nearer the centre; none below the state asked for.
///
/// [`UnitSide::closest_resource`]: crate::unit_ai::UnitSide::closest_resource
#[test]
fn the_unit_ai_s_resource_search_finds_the_nearest_bush_it_reaches() {
    use crate::unit_ai::{NO_POINT, ResourceSearch};
    let mut t = forage_world();
    for masters in &mut t.masters {
        let bush = masters.get_mut(master_id(59)).unwrap();
        (bush.auto_gather, bush.gather_group) = (true, GatherGroup::FISH);
        // the walk reaches the forager's line of sight plus 5 tiles: 601 lies 7 away
        masters.get_mut(master_id(120)).unwrap().line_of_sight = Distance::new(4.0);
    }
    let search = |t: &mut World, s: ResourceSearch| {
        let mut found = None;
        t.run_unit_ai((PlayerId::new(1), ListIndex::new(0)), |_, _, side, _, _| {
            found = Some(side.closest_resource(s)?);
            Ok(())
        })
        .unwrap();
        found.unwrap().map(|g| (g.id, g.class))
    };
    let any = ResourceSearch {
        kind: GatherGroup::FISH,
        cap: -1,
        site: None,
        current: None,
        point: NO_POINT,
        min_state: ObjectState::Alive,
    };
    assert_eq!(search(&mut t, any), Some((object_id(600), class::CLASS_7)));
    assert_eq!(
        search(
            &mut t,
            ResourceSearch {
                current: Some(object_id(600)),
                ..any
            }
        ),
        Some((object_id(601), class::CLASS_7))
    );
    assert_eq!(
        search(
            &mut t,
            ResourceSearch {
                site: Some(object_id(700)),
                ..any
            }
        ),
        Some((object_id(600), class::CLASS_7))
    );
    assert_eq!(
        search(
            &mut t,
            ResourceSearch {
                site: Some(object_id(4444)),
                ..any
            }
        ),
        Some((object_id(600), class::CLASS_7))
    );
    assert_eq!(
        search(
            &mut t,
            ResourceSearch {
                min_state: ObjectState::Dying,
                ..any
            }
        ),
        None
    );
}

/// [`forage_world`] with player 1's mule cart 750 (master 1808: a building of a task of action
/// type 0x99 on class 7, berries) at (24.5, 89.5), moving and with a unit AI; the bushes gather
/// by themselves as group 1.
fn cart_world() -> World {
    let mut t = forage_world();
    for masters in &mut t.masters {
        let bush = masters.get_mut(master_id(59)).unwrap();
        (bush.auto_gather, bush.gather_group) = (true, GatherGroup::FORAGE);
        let cart = Master {
            id: ids::MASTER_1808,
            kind: BUILDING,
            class: class::BUILDING,
            radius: Footprint::new(0.5, 0.5),
            line_of_sight: Distance::new(4.0),
            moving: Some(crate::gathering::hunt::tests::walker().stats),
            tasks: vec![crate::objects::masters::Task {
                action_type: crate::construction::build::CART_TASK,
                object_group: Some(class_id(7)),
                ..crate::objects::masters::Task::default()
            }],
            ..Master::default()
        };
        *masters = Masters::new(masters.iter().cloned().chain([cart]));
    }
    t.players[1].objects.push(Unit {
        id: object_id(750),
        life: Lifecycle {
            state: ObjectState::Alive,
            hp: HitPoints::new(100.0),
            master_id: ids::MASTER_1808,
            ..Lifecycle::default()
        },
        body: crate::objects::layers::Body::new(
            Position::new(24.5, 89.5, 0.0),
            None,
            Some(crate::gathering::hunt::tests::walker()),
        ),
        ai: Some(UnitAi {
            current_order: None,
            current_action: None,
            current_target: None,
            ..UnitAi::default()
        }),
        pasture: None,
        my_pasture: None,
        ..Unit::default()
    });
    t.spatial.insert(
        object_id(750),
        Placed {
            position: Position::new(24.5, 89.5, 0.0),
            radius: Footprint::new(0.5, 0.5),
            class: class::BUILDING,
            player: PlayerId::new(1),
            master: ids::MASTER_1808,
            obstruction_type: ObstructionType::NONE,
            buildable_on: 0,
            tall: true,
            speed: Speed::ZERO,
            unknown: None,
            obstruction: None,
        },
    );
    t
}

/// The mule cart's own search (slot 0x1e8, `FUN_140c16240`): its task's class 7 names gather
/// group 1, searched from the cart as the drop site; the nearer bush by the footprints' gap
/// with its place. Without its movement state the search is not modelled; without a task of
/// a known class it finds none (its own place).
#[test]
fn a_mule_cart_finds_the_nearest_resource_of_its_task_s_class() {
    let mut t = cart_world();
    assert_eq!(
        t.cart_find(PlayerId::new(1), ListIndex::new(3)),
        Ok((Some(object_id(600)), Position::new(32.5, 89.5, 0.0)))
    );
    let mut t = cart_world();
    t.masters[1].get_mut(ids::MASTER_1808).unwrap().tasks[0].object_group = Some(class_id(99));
    assert_eq!(
        t.cart_find(PlayerId::new(1), ListIndex::new(3)),
        Ok((None, Position::new(24.5, 89.5, 0.0)))
    );
    let mut t = cart_world();
    t.players[1].objects[ListIndex::new(3)].body.put_mover(None);
    assert!(t.cart_find(PlayerId::new(1), ListIndex::new(3)).is_err());
}

/// [`forage_world`] with the house's foundation 10028 placed (radii 1) and the forager standing
/// on its footprint at (30, 88.5).
fn footprint_world() -> World {
    let mut t = forage_world();
    t.spatial.insert(
        object_id(10028),
        Placed {
            position: Position::new(30.0, 88.0, 0.0),
            radius: Footprint::new(1.0, 1.0),
            class: class::BUILDING,
            player: PlayerId::new(1),
            master: ids::HOUSE,
            obstruction_type: ObstructionType::TYPE_2,
            buildable_on: 0,
            tall: true,
            speed: Speed::ZERO,
            unknown: None,
            obstruction: None,
        },
    );
    t.players[1].objects[ListIndex::new(0)].place_for_test(Position::new(30.0, 88.5, 0.0));
    t.spatial
        .set_position(object_id(9175), Position::new(30.0, 88.5, 0.0))
        .unwrap();
    t
}

/// `FUN_14123bb20`'s sending off of the units on a new foundation's footprint: the idle forager
/// standing there hears notify 0x216 with the foundation master's radii in hundredths and the
/// foundation; a bush (without a unit AI), an object the world does not hold, a unit at work
/// on the foundation, one walking and one moving off already are passed over.
#[test]
fn the_units_on_a_new_foundation_are_sent_off_it() {
    let sent = |t: &World| {
        t.players[1].objects[ListIndex::new(0)]
            .ai
            .as_ref()
            .unwrap()
            .notifies
            .iter()
            .filter(|n| n.kind == NotifyKind::SENT_OFF)
            .map(|n| n.p)
            .collect::<Vec<_>>()
    };
    let mut t = footprint_world();
    t.evacuate(
        object_id(10028),
        &[object_id(9175), object_id(600), object_id(4444)],
    )
    .unwrap();
    assert_eq!(sent(&t), [[100, 100, 10028]]);
    let mut t = footprint_world();
    t.players[1].objects[ListIndex::new(0)]
        .body
        .set_anim_speed(1.0);
    t.evacuate(object_id(10028), &[object_id(9175)]).unwrap();
    assert_eq!(sent(&t), Vec::<[i32; 3]>::new());
    let mut t = footprint_world();
    t.players[1].objects[ListIndex::new(0)]
        .ai
        .as_mut()
        .unwrap()
        .current_action = Some(ActionKind::MoveAside);
    t.evacuate(object_id(10028), &[object_id(9175)]).unwrap();
    assert_eq!(sent(&t), Vec::<[i32; 3]>::new());
    let mut t = footprint_world();
    t.order_work(
        PlayerId::new(1),
        ListIndex::new(0),
        &crate::orders::Work {
            target: Some(object_id(10028)),
            point: Position::new(30.0, 88.0, 0.0),
            carry_test: false,
            filter: None,
            notified: true,
        },
    )
    .unwrap();
    t.evacuate(object_id(10028), &[object_id(9175)]).unwrap();
    assert_eq!(sent(&t), Vec::<[i32; 3]>::new());
    assert!(t.evacuate(object_id(4444), &[object_id(9175)]).is_err());
}

/// `FUN_14123bb20`'s owner's units of a formation category on the footprint go together
/// (`FUN_1411566b0` with 1, then `FUN_1416123a0`): the farthest free spot beside the footprint
/// from the first, and the forager the splitter leaves loose takes the action of moving off a
/// footprint (`0x2bf`) and moves there, so it hears no notify `0x21b`.
#[test]
fn a_formation_s_units_on_a_new_foundation_move_off_together() {
    let mut t = footprint_world();
    for masters in &mut t.masters {
        masters.get_mut(master_id(120)).unwrap().formation_category = 1;
    }
    t.evacuate(object_id(10028), &[object_id(9175)]).unwrap();
    let v = &t.players[1].objects[ListIndex::new(0)];
    let ai = v.ai.as_ref().unwrap();
    assert_eq!(ai.current_action, Some(ActionKind::MoveAside));
    assert!(
        ai.notifies
            .iter()
            .all(|n| n.kind != NotifyKind::SENT_OFF_GROUP)
    );
    assert!(v.actions.move_to().is_some());
}

/// A built castle's death outside its update (`FUN_1411dbb70` to state 3 from 2,
/// [`World::set_state`]): the population headroom its storage gave leaves its owner (mode 4),
/// the castle count (attribute 134) falls by one, not below 0, and it goes to state 3. A
/// computer player's building, or one with units inside, is not modelled.
#[test]
fn a_built_castle_s_death_takes_its_headroom_and_its_count() {
    let castle = Master {
        id: ids::CASTLE,
        kind: BUILDING,
        class: class::BUILDING,
        radius: Footprint::new(2.0, 2.0),
        hit_points: 4800,
        storage: vec![(Some(attribute::HEADROOM), Amount::new(20.0), 4)],
        building: Some(crate::objects::masters::BuildingMaster {
            tech: None,
            foundation_terrain: -1,
            stack_unit: None,
            head_unit: None,
            transform_unit: None,
            annexes: [None; 4],
            ..crate::objects::masters::BuildingMaster::default()
        }),
        ..Master::default()
    };
    let world = |count: f32| {
        let mut attributes = vec![Amount::ZERO; 250];
        attributes[4] = Amount::new(25.0);
        attributes[134] = Amount::new(count);
        let mut t = World::new(
            WorldTime::ZERO,
            1,
            PlayerId::new(0),
            vec![Millis::ZERO, Millis::ZERO],
            vec![
                Player::default(),
                Player {
                    objects: vec![Unit {
                        id: object_id(900),
                        body: crate::objects::layers::Body::at(Position::new(50.0, 50.0, 0.0)),
                        life: Lifecycle {
                            state: ObjectState::Alive,
                            hp: HitPoints::new(4800.0),
                            master_id: ids::CASTLE,
                            ..Lifecycle::default()
                        },
                        pasture: None,
                        my_pasture: None,
                        ..Unit::default()
                    }]
                    .into(),
                    attributes,
                    ..Player::default()
                },
            ],
        );
        t.masters = vec![Masters::default(), Masters::new([castle.clone()])];
        t
    };
    let mut t = world(1.0);
    t.set_state(PlayerId::new(1), object_id(900), ObjectState::Dying)
        .unwrap();
    let p = &t.players[1];
    assert_eq!(
        (p.attributes[4], p.attributes[134]),
        (Amount::new(5.0), Amount::ZERO)
    );
    assert_eq!(p.objects[ListIndex::new(0)].life.state, ObjectState::Dying);
    let mut t = world(0.0);
    t.set_state(PlayerId::new(1), object_id(900), ObjectState::Dying)
        .unwrap();
    assert_eq!(t.players[1].attributes[134], Amount::ZERO);
    let mut t = world(1.0);
    t.players[1].computer = true;
    assert!(
        t.set_state(PlayerId::new(1), object_id(900), ObjectState::Dying)
            .is_err()
    );
    let mut t = world(1.0);
    t.players[1].objects[ListIndex::new(0)].garrison = vec![object_id(5)];
    assert!(
        t.set_state(PlayerId::new(1), object_id(900), ObjectState::Dying)
            .is_err()
    );
}

/// The load's shape mesh (`RGE_Static_Object::rehook` per object): a standing house of a tall
/// master with an obstruction kind and class takes its shape; a dead one (state 7), one inside
/// another, a desolid foundation, an object the spatial table lacks and one whose master lacks
/// a kind or class take none.
#[test]
fn the_load_s_mesh_takes_the_shapes_of_the_standing_tall_objects() {
    let house = |id, state, inside, desolid: bool, master| Unit {
        id,
        body: crate::objects::layers::Body::at(Position::new(10.5, 10.5, 0.0)),
        life: Lifecycle {
            state,
            master_id: master,
            inside,
            ..Lifecycle::default()
        },
        construction: desolid.then_some(Construction {
            progress: Seconds::ZERO,
            desolid: true,
            builders: 0,
            hit_points: 550,
            build_time: 25,
            record: crate::map::spatial::Obstruction {
                kind: ObstructionType::TYPE_2,
                class: ObstructionClass::UNIT,
            },
            snow: false,
            sprite_override: false,
        }),
        ..Unit::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        1,
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![Player {
            objects: vec![
                house(object_id(1), ObjectState::Alive, None, false, ids::HOUSE),
                house(object_id(2), ObjectState::Removed, None, false, ids::HOUSE),
                house(
                    object_id(3),
                    ObjectState::Alive,
                    Some(object_id(1)),
                    false,
                    ids::HOUSE,
                ),
                house(
                    object_id(4),
                    ObjectState::Foundation,
                    None,
                    true,
                    ids::HOUSE,
                ),
                house(object_id(5), ObjectState::Alive, None, false, ids::HOUSE),
                house(object_id(6), ObjectState::Alive, None, false, master_id(71)),
            ]
            .into(),
            ..Player::default()
        }],
    );
    let master = |id, obstruction_class| Master {
        id,
        kind: BUILDING,
        class: class::BUILDING,
        radius: Footprint::new(1.0, 1.0),
        tall: true,
        obstruction_type: ObstructionType::TYPE_2,
        obstruction_class,
        ..Master::default()
    };
    t.masters = vec![Masters::new([
        master(ids::HOUSE, ObstructionClass::UNIT),
        master(master_id(71), ObstructionClass::NONE),
    ])];
    t.spatial = Spatial::new(GridSize::new(40, 40));
    for id in [1, 2, 3, 4, 6] {
        t.spatial.insert(
            object_id(id),
            Placed {
                position: Position::new(10.5, 10.5, 0.0),
                radius: Footprint::new(1.0, 1.0),
                class: class::BUILDING,
                player: PlayerId::new(0),
                master: ids::HOUSE,
                obstruction_type: ObstructionType::TYPE_2,
                buildable_on: 0,
                tall: true,
                speed: Speed::ZERO,
                unknown: None,
                obstruction: None,
            },
        );
    }
    t.build_mesh().unwrap();
    let shaped: Vec<i32> = (1..=6)
        .filter(|&id| t.pathing.shape(object_id(id)).is_some())
        .collect();
    assert_eq!(shaped, [1]);
}

/// [`sent_off_world`] with its villager the hunter 122 (task 110 on class 9 within 0.5,
/// dropping at the town centre 109), Gaia's deer 500 and 501 (master 65, class 9) and player 1's
/// town centre 700, all placed on [`open_map`]; the villager can move.
pub(crate) fn hunt_world() -> World {
    let mut t = sent_off_world(vec![]);
    for masters in &mut t.masters {
        let hunter = Master {
            id: master_id(122),
            drop_sites: vec![ids::TOWN_CENTRE],
            tasks: vec![crate::objects::masters::Task {
                action_type: crate::gathering::hunt::HUNT,
                object_group: Some(class_id(9)),
                work_range: Distance::new(0.5),
                ..crate::objects::masters::Task::default()
            }],
            ..masters.get(VILLAGER_MALE).unwrap().clone()
        };
        let deer = Master {
            id: master_id(65),
            kind: master_type::COMBAT,
            class: class::PREY,
            radius: Footprint::new(0.4, 0.4),
            ..Master::default()
        };
        let centre = Master {
            id: ids::TOWN_CENTRE,
            kind: BUILDING,
            class: class::BUILDING,
            radius: Footprint::new(2.0, 2.0),
            ..Master::default()
        };
        *masters = Masters::new(masters.iter().cloned().chain([hunter, deer, centre]));
    }
    let deer = |id, x| Unit {
        id,
        body: crate::objects::layers::Body::at(Position::new(x, 89.5, 0.0)),
        life: Lifecycle {
            state: ObjectState::Alive,
            hp: HitPoints::new(5.0),
            master_id: master_id(65),
            ..Lifecycle::default()
        },
        pasture: None,
        my_pasture: None,
        ..Unit::default()
    };
    t.players[0].objects = vec![deer(object_id(500), 32.5), deer(object_id(501), 36.5)].into();
    t.players[1].objects.push(Unit {
        id: object_id(700),
        body: crate::objects::layers::Body::at(Position::new(20.0, 89.5, 0.0)),
        life: Lifecycle {
            state: ObjectState::Alive,
            hp: HitPoints::new(2400.0),
            master_id: ids::TOWN_CENTRE,
            ..Lifecycle::default()
        },
        pasture: None,
        my_pasture: None,
        ..Unit::default()
    });
    open_map(&mut t);
    let placed = |x: f32, class, player, master, radius| Placed {
        position: Position::new(x, 89.5, 0.0),
        radius,
        class,
        player,
        master,
        obstruction_type: ObstructionType::NONE,
        buildable_on: 0,
        tall: true,
        speed: Speed::ZERO,
        unknown: None,
        obstruction: None,
    };
    t.spatial.insert(
        object_id(9175),
        placed(
            29.5,
            class::VILLAGER,
            PlayerId::new(1),
            master_id(122),
            Footprint::new(0.2, 0.2),
        ),
    );
    t.spatial.insert(
        object_id(500),
        placed(
            32.5,
            class::PREY,
            PlayerId::new(0),
            master_id(65),
            Footprint::new(0.4, 0.4),
        ),
    );
    t.spatial.insert(
        object_id(501),
        placed(
            36.5,
            class::PREY,
            PlayerId::new(0),
            master_id(65),
            Footprint::new(0.4, 0.4),
        ),
    );
    t.spatial.insert(
        object_id(700),
        placed(
            20.0,
            class::BUILDING,
            PlayerId::new(1),
            ids::TOWN_CENTRE,
            Footprint::new(2.0, 2.0),
        ),
    );
    let v = &mut t.players[1].objects[ListIndex::new(0)];
    v.life.master_id = master_id(122);
    v.body
        .put_mover(Some(crate::gathering::hunt::tests::walker()));
    t
}

/// The work command on the other deer while hunting the first on the same task (the Hunt's slot
/// 9, `0x140ac97c0`): the Hunt takes it and starts over on it (its Attack walking there), the
/// first deer let go and the second taken; the unit AI hears the hunt order on it.
#[test]
fn a_work_command_on_another_deer_moves_the_hunt_there() {
    let mut t = hunt_world();
    t.hunt_command(PlayerId::new(1), ListIndex::new(0), object_id(500))
        .unwrap();
    let work = crate::orders::Work {
        target: Some(object_id(501)),
        point: Position::new(36.5, 89.5, 0.0),
        carry_test: false,
        filter: None,
        notified: true,
    };
    t.order_work(PlayerId::new(1), ListIndex::new(0), &work)
        .unwrap();
    let v = &t.players[1].objects[ListIndex::new(0)];
    let h = v.actions.hunt().copied().unwrap();
    assert_eq!(
        (h.target, h.carry_to, h.target_class),
        (Some(object_id(501)), None, Some(class::PREY))
    );
    assert_eq!(
        (h.state, h.attack.map(|a| a.target)),
        (HuntState::Attack, Some(Some(object_id(501))))
    );
    let ai = v.ai.as_ref().unwrap();
    assert_eq!(
        (ai.current_order, ai.current_action, ai.current_target),
        (
            Some(OrderKind::Hunt),
            Some(ActionKind::Hunt),
            Some(object_id(501))
        )
    );
    assert_eq!(
        (
            t.players[0].objects[ListIndex::new(0)].workers,
            t.players[0].objects[ListIndex::new(1)].workers
        ),
        (0, 1)
    );
}

/// A move toward an object while hunting (the Hunt's slot 0x40, `FUN_140ac52c0`): toward the
/// other deer it walks to the move's point (state 15) and lets the first go; loaded and moved
/// toward its town centre, a drop site, it carries the load there (state 8, the centre its second
/// target, taken).
#[test]
fn a_hunt_takes_a_move_toward_an_object_as_its_point_or_drop_site() {
    let mut t = hunt_world();
    t.hunt_command(PlayerId::new(1), ListIndex::new(0), object_id(500))
        .unwrap();
    let point = Position::new(36.0, 89.0, 0.0);
    assert_eq!(
        t.move_to_object_with(
            PlayerId::new(1),
            ListIndex::new(0),
            object_id(501),
            point,
            false
        ),
        Ok(true)
    );
    let h = t.players[1].objects[ListIndex::new(0)]
        .actions
        .hunt()
        .copied()
        .unwrap();
    assert_eq!(
        (h.target, h.carry_to, h.state, h.point),
        (None, None, HuntState::WalkPoint, point)
    );
    assert_eq!(
        (
            t.players[0].objects[ListIndex::new(0)].workers,
            t.players[0].objects[ListIndex::new(1)].workers
        ),
        (0, 0)
    );
    let v = &mut t.players[1].objects[ListIndex::new(0)];
    (v.life.attribute_type, v.life.amount) = (Some(attribute::MEAT), Amount::new(6.0));
    assert_eq!(
        t.move_to_object_with(
            PlayerId::new(1),
            ListIndex::new(0),
            object_id(700),
            Position::new(20.0, 89.5, 0.0),
            false
        ),
        Ok(true)
    );
    let h = t.players[1].objects[ListIndex::new(0)]
        .actions
        .hunt()
        .copied()
        .unwrap();
    assert_eq!(
        (h.target, h.carry_to, h.state, h.point, h.target_class),
        (
            None,
            Some(object_id(700)),
            HuntState::Drop,
            Position::from([-1.0; 3]),
            Some(class::BUILDING)
        )
    );
    assert_eq!(t.players[1].objects[ListIndex::new(2)].workers, 1);
}

/// Runs `f` on object `i` of player `p` with its `Step` as its update has it (dt `dt`, outside
/// a turn), the unit staying in its list, then what the `Step` gathered
/// ([`World::step_object`]): `f`'s answer, or the first error.
pub(crate) fn in_update<R>(
    t: &mut World,
    (p, i): (PlayerId, ListIndex),
    dt: u32,
    f: impl FnOnce(&mut Unit, &mut step::Step<'_>) -> Result<R, GenieError>,
) -> Result<R, GenieError> {
    let mut answer = None;
    let _ = t.step_object((p, i), (Millis::new(dt), None, false), |u, c| {
        answer = Some(f(u, c)?);
        Ok(crate::objects::unit::Outcome::Stay)
    })?;
    Ok(answer.unwrap())
}

/// [`hunt_world`]'s hunter on deer 500 (the hunt command's), its hunt then set by `set`.
fn hunter_in(set: impl FnOnce(&mut crate::gathering::hunt::HuntAction)) -> World {
    let mut t = hunt_world();
    t.hunt_command(PlayerId::new(1), ListIndex::new(0), object_id(500))
        .unwrap();
    set(t.players[1].objects[ListIndex::new(0)]
        .actions
        .hunt_mut()
        .unwrap());
    t
}

/// The hunter's notifies of `kind` (their parameters).
fn notified(t: &World, kind: NotifyKind) -> Vec<[i32; 3]> {
    t.players[1].objects[ListIndex::new(0)]
        .ai
        .as_ref()
        .unwrap()
        .notifies
        .iter()
        .filter(|n| n.kind == kind)
        .map(|n| n.p)
        .collect()
}

/// The hunter's part of the update in its hunt's state ([`Unit::hunt_part`]) with `dt`, the
/// hunter as a combat object and its lost tree as the update reads them.
fn hunt_part_of(t: &mut World, dt: u32) -> Result<(), GenieError> {
    in_update(t, (PlayerId::new(1), ListIndex::new(0)), dt, |unit, c| {
        let me = crate::movement::Me {
            id: unit.id,
            group: unit.group.id,
            action_state: unit.top_action().map_or(0, |a| a.state),
        };
        let mut own = unit.own_victim(c)?;
        let tree_lost = unit.tree_lost(c);
        let code = unit.hunt_part(c, me, &mut own, tree_lost)?;
        unit.end_head_update(c, code)
    })
}

/// The drop-off's wait (state 8, its timer below 0, `0x140ac7a00` case 7): the timer runs by
/// dt; at its end the drop site goes and `set_state(8)` searches again, finding the town centre
/// and walking there.
#[test]
fn a_hunter_s_drop_off_waits_then_searches_its_drop_site_again() {
    let wait = |timer| {
        hunter_in(|h| {
            (h.state, h.timer, h.target, h.carry_to) =
                (HuntState::Drop, timer, None, Some(object_id(700)));
            h.attack = None;
        })
    };
    let mut t = wait(Seconds::new(-0.5));
    t.players[1].objects[ListIndex::new(2)].workers = 1;
    assert_eq!(hunt_part_of(&mut t, 50), Ok(()));
    let h = t.players[1].objects[ListIndex::new(0)]
        .actions
        .hunt()
        .copied()
        .unwrap();
    assert_eq!(
        (h.state, h.carry_to),
        (HuntState::Drop, Some(object_id(700)))
    );
    assert!((h.timer + Seconds::new(0.45)).get().abs() < 1e-6);
    let mut t = wait(Seconds::new(-0.01));
    t.players[1].objects[ListIndex::new(0)].life.attribute_type = Some(attribute::MEAT);
    t.players[1].objects[ListIndex::new(0)].life.amount = Amount::new(5.0);
    t.players[1].objects[ListIndex::new(2)].workers = 1;
    assert_eq!(hunt_part_of(&mut t, 50), Ok(()));
    let h = t.players[1].objects[ListIndex::new(0)]
        .actions
        .hunt()
        .copied()
        .unwrap();
    assert_eq!(
        (h.state, h.carry_to, h.timer),
        (HuntState::Drop, Some(object_id(700)), Seconds::ZERO)
    );
    // the centre let go, then taken again as the new drop site
    assert_eq!(t.players[1].objects[ListIndex::new(2)].workers, 1);
}

/// The drop-off's walk (state 8, case 7): arrived at the town centre within a tile, where the
/// drop-site table takes meat, the hunter's load goes to its player as food and it carries
/// nothing; the hunt goes back to its deer (`set_state(4)`: the live deer's chase, state 12).
/// A failed walk waits 3
/// seconds and stops the animation; a centre that does not take the load is not modelled.
#[test]
fn a_hunter_drops_its_meat_at_the_town_centre_and_goes_back() {
    use crate::gathering::hunt::HuntState;
    use crate::movement::move_to::{MoveTo, MoveToAction};
    let dropping = |move_to| {
        let mut t = hunter_in(|h| {
            (h.state, h.timer, h.carry_to, h.move_to) = (
                HuntState::Drop,
                Seconds::ZERO,
                Some(object_id(700)),
                Some(MoveToAction::sub_in(move_to)),
            );
            h.attack = None;
        });
        let v = &mut t.players[1].objects[ListIndex::new(0)];
        (v.life.attribute_type, v.life.amount) = (Some(attribute::MEAT), Amount::new(5.0));
        v.place_for_test(Position::new(22.3, 89.5, 0.0));
        t.spatial
            .set_position(object_id(9175), Position::new(22.3, 89.5, 0.0))
            .unwrap();
        t.drop_sites = vec![DropSite {
            building: ids::TOWN_CENTRE,
            worker_class: Some(class::VILLAGER),
            targets: vec![crate::unit_ai::task_finder::DropTarget {
                class: class::PREY,
                workers: -1,
                state: None,
                attribute: Some(attribute::MEAT),
            }],
            ..DropSite::default()
        }];
        t
    };
    let mut t = dropping(MoveTo::Arrived);
    assert_eq!(hunt_part_of(&mut t, 50), Ok(()));
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(v.life.amount, Amount::ZERO);
    assert_eq!(t.players[1].attributes[0], Amount::new(5.0));
    let h = v.actions.hunt().copied().unwrap();
    assert_eq!(
        (h.state, h.target, h.carry_to),
        (HuntState::Attack, Some(object_id(500)), None)
    );
    let mut t = dropping(MoveTo::Failed);
    assert_eq!(hunt_part_of(&mut t, 50), Ok(()));
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(v.body.anim_speed(), Some(0.0));
    assert!(v.actions.hunt().is_some_and(|h| h.timer < Seconds::ZERO));
    let mut t = dropping(MoveTo::Arrived);
    t.drop_sites.clear();
    assert!(hunt_part_of(&mut t, 50).is_err());
}

/// The failed hunt (state 13, case 0xc): notify 0x1f9 with the hunt action; the hunt goes and
/// its deer, another player's, is let go after the update. With a sub-action it is not
/// modelled; without a hunt the part does nothing.
#[test]
fn a_failed_hunt_tells_its_unit_ai_and_lets_its_target_go() {
    let mut t = hunter_in(|h| {
        h.state = HuntState::Failed;
        h.attack = None;
    });
    assert_eq!(t.players[0].objects[ListIndex::new(0)].workers, 1);
    assert_eq!(hunt_part_of(&mut t, 50), Ok(()));
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .hunt()
            .copied(),
        None
    );
    assert_eq!(
        notified(&t, NotifyKind::ACTION_FAILED),
        [[ActionKind::Hunt.value(), 0, 0]]
    );
    assert_eq!(t.players[0].objects[ListIndex::new(0)].workers, 0);
    assert_eq!(hunt_part_of(&mut t, 50), Ok(()));
    let mut t = hunter_in(|h| h.state = HuntState::Failed);
    assert!(hunt_part_of(&mut t, 50).is_err());
}

/// The search (state 3, case 2): the timer runs by dt; at its end it starts again at −10 s and
/// the unit AI hears notify 0x202 with the hunt action, the hunt's class (9) and its drop site,
/// the town centre the search finds. Without its target a lead (state 10, case 9) searches at
/// once; a done hunt (state 2) waits, but of a tree's class it is not modelled.
#[test]
fn a_searching_hunt_asks_its_unit_ai_again_after_ten_seconds() {
    use crate::gathering::hunt::HuntState;
    let search = [ActionKind::Hunt.value(), 9, 700];
    let mut t = hunter_in(|h| {
        (h.state, h.timer) = (HuntState::Search, Seconds::new(-0.5));
        h.attack = None;
    });
    assert_eq!(hunt_part_of(&mut t, 50), Ok(()));
    assert_eq!(notified(&t, NotifyKind::REQUIRES_SEARCH), [[0; 3]; 0]);
    t.players[1].objects[ListIndex::new(0)]
        .actions
        .hunt_mut()
        .unwrap()
        .timer = Seconds::new(-0.01);
    assert_eq!(hunt_part_of(&mut t, 50), Ok(()));
    assert_eq!(notified(&t, NotifyKind::REQUIRES_SEARCH), [search]);
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .hunt()
            .map(|h| h.timer),
        Some(SEARCH_AGAIN)
    );
    let mut t = hunter_in(|h| {
        (h.state, h.target) = (HuntState::Lead, None);
        h.attack = None;
    });
    assert_eq!(hunt_part_of(&mut t, 50), Ok(()));
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .hunt()
            .map(|h| h.state),
        Some(HuntState::Search)
    );
    assert_eq!(notified(&t, NotifyKind::REQUIRES_SEARCH), [search]);
    let mut t = hunter_in(|h| {
        h.state = HuntState::Idle;
        h.attack = None;
    });
    let before = t.players[1].objects[ListIndex::new(0)]
        .actions
        .hunt()
        .copied();
    assert_eq!(hunt_part_of(&mut t, 50), Ok(()));
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .hunt()
            .copied(),
        before
    );
    t.players[1].objects[ListIndex::new(0)]
        .actions
        .hunt_mut()
        .unwrap()
        .target_class = Some(class::TREE);
    assert!(hunt_part_of(&mut t, 50).is_err());
}

/// The walks to a point (states 15 and 11): arrived, state 15 is done (`set_state(2)`: the
/// target goes and is let go) and state 11 searches (`set_state(3)`: notify 0x202); a waiting
/// walk asks again after its delay. Waiting after a given-up `MoveTo` without its target (state
/// 4, its timer below 0) the timer runs, then the walk to the point (state 11).
#[test]
fn a_hunt_s_walk_to_a_point_ends_done_or_in_a_search() {
    use crate::gathering::hunt::HuntState;
    use crate::movement::move_to::{MoveTo, MoveToAction, MoveWait};
    let walked = |state| {
        hunter_in(|h| {
            (h.state, h.move_to) = (state, Some(MoveToAction::sub_in(MoveTo::Arrived)));
            h.attack = None;
        })
    };
    let mut t = walked(HuntState::WalkPoint);
    assert_eq!(hunt_part_of(&mut t, 50), Ok(()));
    let h = t.players[1].objects[ListIndex::new(0)]
        .actions
        .hunt()
        .copied()
        .unwrap();
    assert_eq!(
        (h.state, h.target, h.move_to),
        (HuntState::Idle, None, None)
    );
    assert_eq!(t.players[0].objects[ListIndex::new(0)].workers, 0);
    let mut t = walked(HuntState::WalkSearch);
    assert_eq!(hunt_part_of(&mut t, 50), Ok(()));
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .hunt()
            .map(|h| h.state),
        Some(HuntState::Search)
    );
    assert_eq!(notified(&t, NotifyKind::REQUIRES_SEARCH).len(), 1);
    let mut t = hunter_in(|h| {
        (h.state, h.move_to) = (
            HuntState::WalkPoint,
            Some(MoveToAction {
                wait: MoveWait {
                    delay: Seconds::new(1.0),
                    ..MoveWait::NEW
                },
                ..MoveToAction::sub_in(MoveTo::Waiting)
            }),
        );
        h.attack = None;
    });
    assert_eq!(hunt_part_of(&mut t, 50), Ok(()));
    let h = t.players[1].objects[ListIndex::new(0)]
        .actions
        .hunt()
        .copied()
        .unwrap();
    assert_eq!(
        (h.move_to.map(|m| m.state), h.state),
        (Some(MoveTo::Waiting), HuntState::WalkPoint)
    );
    let mut t = hunter_in(|h| {
        (h.state, h.target, h.timer, h.move_to) =
            (HuntState::Walk, None, Seconds::new(-0.01), None);
        h.point = Position::new(36.0, 89.5, 0.0);
        h.attack = None;
    });
    assert_eq!(hunt_part_of(&mut t, 50), Ok(()));
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .hunt()
            .map(|h| h.state),
        Some(HuntState::WalkSearch)
    );
}

/// The hunt's strike (`0x140ac7a00`'s states 12 and 7, [`Unit::hunt_strike`]): the hunt
/// command's Attack on the live deer walks to it (state 4, its `MoveTo` moving on a path).
/// Gathering the dead deer (work rate 1 by work value 1 a second) the hunter gains 0.05 in
/// 50 ms and the deer loses it; with less room than that it is full and carries its load to the
/// town centre (state 8) without taking it.
#[test]
fn a_hunter_strikes_its_live_deer_and_gathers_the_dead_one_until_full() {
    use crate::gathering::hunt::HuntState;
    use crate::movement::move_to::MoveTo;
    let mut t = hunter_in(|_| {});
    assert_eq!(hunt_part_of(&mut t, 50), Ok(()));
    let v = &t.players[1].objects[ListIndex::new(0)];
    let h = v.actions.hunt().copied().unwrap();
    assert_eq!(
        (
            h.state,
            h.attack.map(|a| (a.state, a.move_to.map(|m| m.state)))
        ),
        (
            HuntState::Attack,
            Some((AttackState::Move, Some(MoveTo::Moving)))
        )
    );
    assert!(v.body.mover().is_some_and(|m| !m.paths.is_empty()));
    let gathering = |held: f32| {
        let mut t = hunter_in(|h| {
            (h.state, h.attack, h.move_to) = (HuntState::Gather, None, None);
            (h.task.work_value, h.task.resource) = (1.0, Some(attribute::MEAT));
        });
        for masters in &mut t.masters {
            let hunter = masters.get_mut(master_id(122)).unwrap();
            (hunter.capacity, hunter.work_rate) = (10, WorkRate::new(1.0));
        }
        let deer = &mut t.players[0].objects[ListIndex::new(0)];
        (deer.life.state, deer.life.hp) = (ObjectState::Dying, HitPoints::ZERO);
        (deer.life.attribute_type, deer.life.amount) = (Some(attribute::MEAT), Amount::new(100.0));
        let v = &mut t.players[1].objects[ListIndex::new(0)];
        (v.life.attribute_type, v.life.amount) = (Some(attribute::MEAT), Amount::new(held));
        v.place_for_test(Position::new(31.9, 89.5, 0.0));
        t.spatial
            .set_position(object_id(9175), Position::new(31.9, 89.5, 0.0))
            .unwrap();
        t
    };
    let mut t = gathering(5.0);
    assert_eq!(hunt_part_of(&mut t, 50), Ok(()));
    let (gained, left) = (
        t.players[1].objects[ListIndex::new(0)].life.amount,
        t.players[0].objects[ListIndex::new(0)].life.amount,
    );
    assert!(
        (gained - Amount::new(5.05)).abs() < Amount::new(1e-5)
            && (left - Amount::new(99.95)).abs() < Amount::new(1e-4)
    );
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .hunt()
            .map(|h| h.state),
        Some(HuntState::Gather)
    );
    let mut t = gathering(9.99);
    assert_eq!(hunt_part_of(&mut t, 50), Ok(()));
    let h = t.players[1].objects[ListIndex::new(0)]
        .actions
        .hunt()
        .copied()
        .unwrap();
    assert_eq!(
        (h.state, h.carry_to),
        (HuntState::Drop, Some(object_id(700)))
    );
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)].life.amount,
        Amount::new(9.99)
    );
    assert_eq!(
        t.players[0].objects[ListIndex::new(0)].life.amount,
        Amount::new(100.0)
    );
}

/// A hunter's work on its dead deer reaches the deer where it ran (the hit's write-back,
/// [`crate::combat::victims::write_back`]): in the same update the deer holds 0.05 less.
#[test]
fn a_hunter_s_work_reaches_its_deer_in_the_same_update() {
    let mut t = hunter_in(|h| {
        (h.state, h.attack, h.move_to) = (HuntState::Gather, None, None);
        (h.task.work_value, h.task.resource) = (1.0, Some(attribute::MEAT));
    });
    for masters in &mut t.masters {
        let hunter = masters.get_mut(master_id(122)).unwrap();
        (hunter.capacity, hunter.work_rate) = (10, WorkRate::new(1.0));
    }
    let deer = &mut t.players[0].objects[ListIndex::new(0)];
    (deer.life.state, deer.life.hp) = (ObjectState::Dying, HitPoints::ZERO);
    (deer.life.attribute_type, deer.life.amount) = (Some(attribute::MEAT), Amount::new(100.0));
    let v = &mut t.players[1].objects[ListIndex::new(0)];
    (v.life.attribute_type, v.life.amount) = (Some(attribute::MEAT), Amount::new(5.0));
    v.place_for_test(Position::new(31.9, 89.5, 0.0));
    t.spatial
        .set_position(object_id(9175), Position::new(31.9, 89.5, 0.0))
        .unwrap();
    let left = in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, c| {
            let me = crate::movement::Me {
                id: unit.id,
                group: unit.group.id,
                action_state: unit.top_action().map_or(0, |a| a.state),
            };
            let mut own = unit.own_victim(c)?;
            let tree_lost = unit.tree_lost(c);
            let code = unit.hunt_part(c, me, &mut own, tree_lost)?;
            unit.end_head_update(c, code)?;
            Ok(OthersRef::of(&c.w.players, c.player)
                .unwrap()
                .find(object_id(500))
                .map(|(_, deer)| deer.life.amount))
        },
    );
    assert!(
        left.as_ref()
            .is_ok_and(|l| l.is_some_and(|l| (l - Amount::new(99.95)).abs() < Amount::new(1e-4))),
        "{left:?}"
    );
}

/// A gathering hunt (state 7) whose deer left the world drops it at its update's start and
/// ends without a target in the same update: holding nothing, it searches (state 3), reading no
/// target.
#[test]
fn a_gathering_hunt_whose_target_left_searches_without_reading_one() {
    use crate::gathering::hunt::HuntState;
    let mut t = hunter_in(|h| (h.state, h.attack, h.move_to) = (HuntState::Gather, None, None));
    let _ = t.players[0].objects.remove(ListIndex::new(0)).unwrap();
    let _ = t.spatial.leave(object_id(500));
    let r = in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, c| {
            unit.hunt_targets(c)?;
            let me = crate::movement::Me {
                id: unit.id,
                group: unit.group.id,
                action_state: unit.top_action().map_or(0, |a| a.state),
            };
            let mut own = unit.own_victim(c)?;
            let code = unit.hunt_part(c, me, &mut own, false)?;
            unit.end_head_update(c, code)
        },
    );
    assert_eq!(r, Ok(()));
    let h = t.players[1].objects[ListIndex::new(0)]
        .actions
        .hunt()
        .copied()
        .unwrap();
    assert_eq!((h.state, h.target), (HuntState::Search, None));
}

/// [`hunt_world`] with player 1's own sheep 510 (master 594, class 58, moving) at `x` beside
/// the hunter, its hunter's task on livestock, the drop-site table's town centre taking
/// livestock; the hunt command's hunt faces it (state 4, arrived).
fn shepherd_world(x: f32) -> World {
    use crate::movement::move_to::{MoveTo, MoveToAction};
    let mut t = hunt_world();
    for masters in &mut t.masters {
        let sheep = Master {
            id: master_id(594),
            kind: master_type::COMBAT,
            class: class::LIVESTOCK,
            radius: Footprint::new(0.2, 0.2),
            moving: Some(crate::gathering::hunt::tests::walker().stats),
            ..Master::default()
        };
        *masters = Masters::new(masters.iter().cloned().chain([sheep]));
        masters
            .get_mut(master_id(122))
            .unwrap()
            .tasks
            .push(crate::objects::masters::Task {
                action_type: crate::gathering::hunt::HUNT,
                object_group: Some(class_id(58)),
                work_range: Distance::new(0.5),
                ..crate::objects::masters::Task::default()
            });
    }
    t.players[1].objects.push(Unit {
        id: object_id(510),
        life: Lifecycle {
            state: ObjectState::Alive,
            hp: HitPoints::new(7.0),
            master_id: master_id(594),
            attribute_type: Some(attribute::MEAT),
            amount: Amount::new(100.0),
            ..Lifecycle::default()
        },
        body: crate::objects::layers::Body::new(
            Position::new(x, 89.5, 0.0),
            None,
            Some(crate::gathering::hunt::tests::walker()),
        ),
        pasture: None,
        my_pasture: None,
        ..Unit::default()
    });
    t.spatial.insert(
        object_id(510),
        Placed {
            position: Position::new(x, 89.5, 0.0),
            radius: Footprint::new(0.2, 0.2),
            class: class::LIVESTOCK,
            player: PlayerId::new(1),
            master: master_id(594),
            obstruction_type: ObstructionType::NONE,
            buildable_on: 0,
            tall: true,
            speed: Speed::ZERO,
            unknown: None,
            obstruction: None,
        },
    );
    t.drop_sites = vec![DropSite {
        building: ids::TOWN_CENTRE,
        worker_class: Some(class::VILLAGER),
        livestock: true,
        ..DropSite::default()
    }];
    t.hunt_command(PlayerId::new(1), ListIndex::new(0), object_id(510))
        .unwrap();
    let h = t.players[1].objects[ListIndex::new(0)]
        .actions
        .hunt_mut()
        .unwrap();
    (h.move_to, h.attack) = (Some(MoveToAction::sub_in(MoveTo::Arrived)), None);
    t
}

/// A hunt facing its own live sheep (`set_state(6)` on livestock with its herd,
/// [`Unit::herd_for`]): the sheep stands more than 3 tiles from the drop-site table's nearest
/// town centre, so the hunter leads it there (state 10, the centre its drop site) and the sheep
/// walks; the lead goes on in the next update. A sheep beside the centre is slaughtered at once
/// (state 12, the Attack).
#[test]
fn a_hunter_leads_its_far_sheep_to_the_town_centre_and_slaughters_a_near_one() {
    use crate::gathering::hunt::HuntState;
    let mut t = shepherd_world(30.2);
    assert_eq!(hunt_part_of(&mut t, 50), Ok(()));
    let h = t.players[1].objects[ListIndex::new(0)]
        .actions
        .hunt()
        .copied()
        .unwrap();
    assert_eq!(
        (h.state, h.target, h.drop_site),
        (HuntState::Lead, Some(object_id(510)), Some(object_id(700)))
    );
    assert!(
        t.players[1].objects[ListIndex::new(3)]
            .actions
            .move_to()
            .is_some()
    );
    assert_eq!(hunt_part_of(&mut t, 50), Ok(()));
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .hunt()
            .map(|h| h.state),
        Some(HuntState::Lead)
    );
    let mut t = shepherd_world(22.6);
    t.players[1].objects[ListIndex::new(0)].place_for_test(Position::new(23.2, 89.5, 0.0));
    t.spatial
        .set_position(object_id(9175), Position::new(23.2, 89.5, 0.0))
        .unwrap();
    assert_eq!(hunt_part_of(&mut t, 50), Ok(()));
    let h = t.players[1].objects[ListIndex::new(0)]
        .actions
        .hunt()
        .copied()
        .unwrap();
    assert_eq!(
        (h.state, h.attack.map(|a| a.target)),
        (HuntState::Attack, Some(Some(object_id(510))))
    );
}

/// The lead is the Hunt's own call (`0x140ac60e0` case 6, `FUN_140ac4c60` before
/// `set_state(10)`): the far sheep has its `MoveTo` to the town centre while the hunter's
/// update still runs, not after it.
#[test]
fn a_hunter_s_sheep_walks_before_the_hunter_s_update_ends() {
    let mut t = shepherd_world(30.2);
    let walks = in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, c| {
            let me = crate::movement::Me {
                id: unit.id,
                group: unit.group.id,
                action_state: unit.top_action().map_or(0, |a| a.state),
            };
            let mut own = unit.own_victim(c)?;
            let tree_lost = unit.tree_lost(c);
            let _ = unit.hunt_part(c, me, &mut own, tree_lost)?;
            Ok(c.w.players[1].objects[ListIndex::new(3)]
                .actions
                .move_to()
                .is_some())
        },
    );
    assert_eq!(walks, Ok(true));
}

/// [`hunt_world`] with Gaia's trees 520 and 521 (master 349, class 15, 100 wood, gathered by
/// themselves as group 0) at x `near` and 40, the hunter's task on trees, its unit AI targeting
/// class 15 and its hunt without a target at its own place.
fn lumber_world(near: f32) -> World {
    let mut t = hunt_world();
    for masters in &mut t.masters {
        let tree = Master {
            id: master_id(349),
            kind: master_type::STATIC,
            class: class::TREE,
            radius: Footprint::new(0.5, 0.5),
            auto_gather: true,
            gather_group: GatherGroup::WOOD,
            ..Master::default()
        };
        *masters = Masters::new(masters.iter().cloned().chain([tree]));
        let hunter = masters.get_mut(master_id(122)).unwrap();
        hunter.line_of_sight = Distance::new(4.0);
        hunter.tasks.push(crate::objects::masters::Task {
            action_type: crate::gathering::hunt::HUNT,
            object_group: Some(class_id(15)),
            work_range: Distance::new(0.5),
            ..crate::objects::masters::Task::default()
        });
    }
    let tree = |id, x: f32| Unit {
        id,
        body: crate::objects::layers::Body::at(Position::new(x, 89.5, 0.0)),
        life: Lifecycle {
            state: ObjectState::Alive,
            hp: HitPoints::new(20.0),
            master_id: master_id(349),
            attribute_type: Some(attribute::WOOD),
            amount: Amount::new(100.0),
            ..Lifecycle::default()
        },
        pasture: None,
        my_pasture: None,
        ..Unit::default()
    };
    t.players[0]
        .objects
        .extend([tree(object_id(520), near), tree(object_id(521), 40.0)]);
    for (id, x) in [(520, near), (521, 40.0)] {
        t.spatial.insert(
            object_id(id),
            Placed {
                position: Position::new(x, 89.5, 0.0),
                radius: Footprint::new(0.5, 0.5),
                class: class::TREE,
                player: PlayerId::new(0),
                master: master_id(349),
                obstruction_type: ObstructionType::NONE,
                buildable_on: 0,
                tall: true,
                speed: Speed::ZERO,
                unknown: None,
                obstruction: None,
            },
        );
    }
    t.spatial.relations = vec![relation_row([0, 3]), relation_row([3, 0])];
    t.hunt_command(PlayerId::new(1), ListIndex::new(0), object_id(500))
        .unwrap();
    let v = &mut t.players[1].objects[ListIndex::new(0)];
    let here = v.position();
    let h = v.actions.hunt_mut().unwrap();
    (h.target, h.target_class, h.attack, h.point) = (None, Some(class::TREE), None, here);
    v.ai.as_mut().unwrap().target_class = Some(class::TREE);
    t
}

/// [`lumber_world`] with its tree 521 asleep and a Gaia foundation 530 (state 0) of a building
/// master with a stack unit (master 99, which the game never uses) that is an annex of tree
/// 520.
fn lets_world() -> World {
    let mut t = lumber_world(31.5);
    let asleep = t.players[0].objects.swap_remove(ListIndex::new(3)).unwrap();
    t.players[0].sleeping.push(asleep);
    for masters in &mut t.masters {
        let annex = Master {
            id: master_id(99),
            kind: BUILDING,
            building: Some(crate::objects::masters::BuildingMaster {
                stack_unit: Some(master_id(5)),
                ..crate::objects::masters::BuildingMaster::default()
            }),
            ..Master::default()
        };
        *masters = Masters::new(masters.iter().cloned().chain([annex]));
    }
    t.players[0].objects.push(Unit {
        id: object_id(530),
        annex_of: Some(object_id(520)),
        life: Lifecycle {
            state: ObjectState::Foundation,
            master_id: master_id(99),
            ..Lifecycle::default()
        },
        pasture: None,
        my_pasture: None,
        ..Unit::default()
    });
    t
}

/// An update's take through its view (slot 0x178, [`crate::world::lets::take`]): a live tree
/// counts its worker; a sleeping one counts its worker and wakes, last in its player's live
/// list and tracker; an unallied player's foundation with a stack unit lists its taker, and
/// Gaia's own taker is allied; one the world does not hold is not taken; the updating unit
/// itself counts its worker at once, within its update.
#[test]
fn an_update_s_take_counts_its_worker_and_wakes_a_sleeping_object() {
    use crate::world::lets::take;
    let mut t = lets_world();
    let gaia = Releaser {
        id: object_id(500),
        class: Some(class::PREY),
        player: PlayerId::new(0),
    };
    let own = in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, c| {
            let by = unit.releaser(c)?;
            for id in [520, 521, 530, 9999] {
                take(c, unit, object_id(id), by).unwrap();
            }
            let before = unit.workers;
            take(c, unit, unit.id, by).unwrap();
            take(c, unit, object_id(530), gaia).unwrap();
            Ok((before, unit.workers))
        },
    );
    assert_eq!(own, Ok((0, 1)));
    let workers = |t: &World, id: ObjectId| t.object(id).map(|(_, u)| u.workers);
    assert_eq!(workers(&t, object_id(520)), Some(1));
    assert_eq!(workers(&t, object_id(521)), Some(1));
    assert_eq!(
        t.players[0].objects.iter().last().map(|u| u.id.get()),
        Some(521)
    );
    assert!(t.players[0].sleeping.by_id(object_id(521)).is_none());
    let k = crate::players::tracker::key(t.masters.first(), &t.lines, master_id(349));
    assert_eq!(
        t.players[0]
            .tracker
            .list(ListKey::of_master(k.master))
            .last(),
        Some(&Some(object_id(521)))
    );
    let villager = t.players[1].objects[ListIndex::new(0)].id;
    assert_eq!(workers(&t, villager), Some(1));
    let annex = t
        .object(object_id(530))
        .map(|(_, u)| (u.workers, u.attacking_units.clone()));
    assert_eq!(annex, Some((2, vec![villager])));
}

/// An update's release through its view (slot 0x180, [`crate::world::lets::release`]): a live
/// tree no one works on any more goes to sleep; a sleeping one counts a worker less and stays
/// asleep; one the world does not hold is not let go; the updating unit itself counts a worker
/// less at once, within its update. Then a counted builder's leave
/// ([`crate::world::lets::Lets::leave_builders`]): an object counts one builder less, an annex's
/// main building for the annex, the updating unit itself at once; the teardown's pair applies
/// leaves before releases ([`crate::world::lets::torn_down`]).
#[test]
fn an_update_s_release_and_builder_leave_change_the_objects_at_once() {
    use crate::world::lets::{leave_builders, release, torn_down};
    let mut t = lets_world();
    for id in [520, 521] {
        let u = t.object_mut(object_id(id)).unwrap().1;
        (u.workers, u.builders) = (2, 3);
    }
    let own = in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, c| {
            let by = unit.releaser(c)?;
            (unit.workers, *unit.repairers_mut()) = (1, 1);
            for id in [520, 521, 9999] {
                release(c, unit, object_id(id), by)?;
            }
            release(c, unit, unit.id, by)?;
            leave_builders(c, unit, object_id(520))?;
            leave_builders(c, unit, object_id(530))?;
            leave_builders(c, unit, unit.id)?;
            torn_down(c, unit, (vec![object_id(521)], vec![(object_id(520), by)]))?;
            Ok((unit.workers, *unit.repairers_mut()))
        },
    );
    assert_eq!(own, Ok((0, 0)));
    let tree = |t: &World, id| {
        t.object(object_id(id))
            .map(|(_, u)| (u.workers, u.builders, u.life.goto_sleep))
    };
    assert_eq!(tree(&t, 520), Some((0, 1, true)));
    assert_eq!(tree(&t, 521), Some((1, 2, false)));
    assert!(t.players[0].sleeping.by_id(object_id(521)).is_some());
}

/// Outside an update a take, a release and a builder's leave run the update's calls on a view
/// of the whole world ([`World::with_lets`]): the sleeping tree wakes to the end of Gaia's live
/// list and tracker, a release lets it sleep again, and the annex's main building counts a
/// builder less.
#[test]
fn outside_an_update_the_lets_run_on_the_whole_world() {
    let mut t = lets_world();
    let by = Releaser {
        id: object_id(t.players[1].objects[ListIndex::new(0)].id.get()),
        class: Some(class::VILLAGER),
        player: PlayerId::new(1),
    };
    t.take(object_id(521), by).unwrap();
    assert_eq!(
        t.players[0].objects.iter().last().map(|u| u.id.get()),
        Some(521)
    );
    let k = crate::players::tracker::key(t.masters.first(), &t.lines, master_id(349));
    assert_eq!(
        t.players[0]
            .tracker
            .list(ListKey::of_master(k.master))
            .last(),
        Some(&Some(object_id(521)))
    );
    assert_eq!(t.object(object_id(521)).map(|(_, u)| u.workers), Some(1));
    t.release(object_id(521), by).unwrap();
    let tree = t
        .object(object_id(521))
        .map(|(_, u)| (u.workers, u.life.goto_sleep));
    assert_eq!(tree, Some((0, true)));
    t.object_mut(object_id(520)).unwrap().1.builders = 2;
    t.leave_builders(object_id(530)).unwrap();
    assert_eq!(t.object(object_id(520)).map(|(_, u)| u.builders), Some(1));
}

/// A player's live object out of its slot ([`World::take_out_own`]) is out of the world's
/// lookups as an updating one is, its stand-in in its place, until it goes back
/// ([`World::put_back_own`]); the sleeping object wakes once ([`World::wake_sleeper`]).
#[test]
fn an_own_object_taken_out_leaves_the_world_until_it_goes_back() {
    let mut t = lets_world();
    let id = t.players[1].objects[ListIndex::new(0)].id;
    assert!(
        t.take_out_own(PlayerId::new(0), id).unwrap().is_none(),
        "another player's"
    );
    let mut u = t
        .take_out_own(PlayerId::new(1), id)
        .unwrap()
        .expect("the owner's");
    assert_eq!(u.id, id);
    assert!(t.object(id).is_none() && t.locate(id).is_none());
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)].id,
        crate::objects::unit::TAKEN_OUT
    );
    u.life.hp = HitPoints::new(8.0);
    assert_eq!(
        t.put_back_own(PlayerId::new(1), u).ok(),
        Some(ListIndex::new(0))
    );
    assert_eq!(
        t.locate(id).map(|(q, u, s)| (q, u.life.hp.get(), s)),
        Some((PlayerId::new(1), 8.0, false))
    );
    assert_eq!(
        t.locate(object_id(521)).map(|(q, _, s)| (q, s)),
        Some((PlayerId::new(0), true))
    );
    assert!(t.wake_sleeper(object_id(521)).unwrap());
    assert!(!t.wake_sleeper(object_id(521)).unwrap(), "awake already");
    assert_eq!(
        t.locate(object_id(521)).map(|(q, _, s)| (q, s)),
        Some((PlayerId::new(0), false))
    );
}

/// The unit AI's side's release of the unit itself ([`crate::world::lets::Lets::release`] on
/// the side's unit): the unit counts its worker less at once, and the notify 699 for its own AI,
/// which the side does not hold while it runs, reaches the AI as the side's call returns.
#[test]
fn a_side_s_own_release_tells_the_unit_ai_as_the_side_returns() {
    use crate::unit_ai::world_view::OwnerView;
    let mut t = hunt_world();
    let id = t.players[1].objects[ListIndex::new(0)].id;
    let ai = t.players[1].objects[ListIndex::new(0)].ai.as_mut().unwrap();
    (ai.attackers, ai.notifies) = (Attackers::of_saved(&[id.get()]), Vec::new());
    let seen = in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, c| {
            (unit.workers, unit.combat) = (1, true);
            let by = unit.releaser(c)?;
            let view = OwnerView::of(c, None).unwrap();
            let attacker = unit.attacker_of(
                UnitAiType::SOLDIER,
                None,
                (PlayerId::new(1), false),
                false,
                0,
            );
            let inside = unit.with_side(
                c,
                &view,
                attacker,
                (PlayerId::new(1), false),
                |ai, side, _| {
                    side.release(by.id, by)?;
                    Ok((side.unit.workers, ai.notifies.len()))
                },
            )?;
            Ok((inside, unit.ai.as_ref().map(|ai| ai.notifies.len())))
        },
    );
    assert_eq!(seen, Ok(((0, 0), Some(1))));
    assert_eq!(notified(&t, NotifyKind::NO_LONGER_ON_ME).len(), 1);
}

/// A notify for the updating unit's own AI from a call of its step ([`Step::notify_own`]):
/// posted at once while the AI is in the unit; held while the AI's side runs, the AI out of the
/// unit, and posted as the side returns.
///
/// [`Step::notify_own`]: crate::world::step::Step::notify_own
#[test]
fn a_step_s_own_notify_waits_for_the_unit_ai_s_side_to_return() {
    use crate::unit_ai::Notify;
    use crate::unit_ai::world_view::OwnerView;
    let mut t = hunt_world();
    let id = t.players[1].objects[ListIndex::new(0)].id;
    let ai = t.players[1].objects[ListIndex::new(0)].ai.as_mut().unwrap();
    (ai.attackers, ai.notifies) = (Attackers::of_saved(&[id.get()]), Vec::new());
    let n = Notify::own(id, NotifyKind::NO_LONGER_ON_ME, [1, 2, 3]);
    let seen = in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, c| {
            c.notify_own(unit, n)?;
            let at_once = unit.ai.as_ref().map(|ai| ai.notifies.len());
            let view = OwnerView::of(c, None).unwrap();
            let attacker = unit.attacker_of(
                UnitAiType::SOLDIER,
                None,
                (PlayerId::new(1), false),
                false,
                0,
            );
            let inside = unit.with_side(
                c,
                &view,
                attacker,
                (PlayerId::new(1), false),
                |ai, side, _| {
                    side.c.notify_own(side.unit, n)?;
                    Ok(ai.notifies.len())
                },
            )?;
            Ok((
                at_once,
                inside,
                unit.ai.as_ref().map(|ai| ai.notifies.len()),
            ))
        },
    );
    assert_eq!(seen, Ok((Some(1), 1, Some(2))));
    assert_eq!(
        notified(&t, NotifyKind::NO_LONGER_ON_ME),
        vec![[1, 2, 3]; 2]
    );
}

/// The AI stream is the unit AI's while it runs on its side ([`Unit::with_side`]): its draws are
/// the world's stream after the run, and a world routine that draws from it meanwhile fails the
/// run.
#[test]
fn the_ai_stream_is_the_unit_ai_s_while_its_side_runs() {
    use crate::unit_ai::world_view::OwnerView;
    let mut t = hunt_world();
    let mut drawn = t.streams.get(Stream::Ai).clone();
    let first = drawn.rand15();
    let run = |t: &mut World, world_draws: bool| {
        in_update(t, (PlayerId::new(1), ListIndex::new(0)), 50, |unit, c| {
            let view = OwnerView::of(c, None).unwrap();
            let attacker = unit.attacker_of(
                UnitAiType::SOLDIER,
                None,
                (PlayerId::new(1), false),
                false,
                0,
            );
            unit.with_side(
                c,
                &view,
                attacker,
                (PlayerId::new(1), false),
                |_, side, rng| {
                    if world_draws {
                        let _ = side.c.w.streams.get(Stream::Ai).rand15();
                    }
                    Ok(rng.rand15())
                },
            )
        })
    };
    assert_eq!(run(&mut t, false), Ok(first));
    assert_eq!(*t.streams.get(Stream::Ai), drawn);
    let failed = run(&mut t, true).expect_err("a draw beside the unit AI's");
    assert!(failed.to_string().contains("AI stream"), "{failed}");
}

/// The unit AI's stop (`Effect::Stop`, [`Unit::stop_action`]): the forager's Gather goes and
/// its bush counts a worker less within the update, through the view, not after the step.
#[test]
fn a_unit_ai_s_stop_lets_its_target_go_within_the_update() {
    let mut t = forage_world();
    let work = crate::orders::Work {
        target: Some(object_id(600)),
        point: Position::new(32.5, 89.5, 0.0),
        carry_test: false,
        filter: None,
        notified: true,
    };
    t.order_work(PlayerId::new(1), ListIndex::new(0), &work)
        .unwrap();
    assert_eq!(t.players[0].objects[ListIndex::new(0)].workers, 1);
    let seen = in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, c| {
            unit.stop_action(c)?;
            let bush = OthersRef::of(&c.w.players, c.player)
                .unwrap()
                .find(object_id(600))
                .map(|(_, b)| b.workers);
            Ok((bush, unit.actions.gather().is_some()))
        },
    );
    assert_eq!(seen, Ok((Some(0), false)));
    assert_eq!(t.players[0].objects[ListIndex::new(0)].workers, 0);
}

/// The hunt's search for the next tree (`FUN_140ac4670`, [`Unit::hunt_tree_search`]): the
/// unit AI's `ClosestResourceObject` of group 0 names the nearer tree, which the hunt takes
/// (slot 0x48: its Attack on the tree) and the tree counts the hunter. A hunt farther than
/// `within` from its point, a tree farther than `near` from the unit, or no tree, find none.
#[test]
fn a_hunt_that_lost_its_tree_takes_the_next_one() {
    let search = |t: &mut World, within, near| {
        in_update(t, (PlayerId::new(1), ListIndex::new(0)), 50, |unit, c| {
            unit.hunt_tree_search(c, within, near)
        })
    };
    let mut t = lumber_world(31.5);
    assert_eq!(search(&mut t, Some(8.0), None), Ok(true));
    let h = t.players[1].objects[ListIndex::new(0)]
        .actions
        .hunt()
        .copied()
        .unwrap();
    assert_eq!(
        (h.target, h.attack.map(|a| a.target)),
        (Some(object_id(520)), Some(Some(object_id(520))))
    );
    assert_eq!(t.players[0].objects[ListIndex::new(2)].workers, 1);
    let mut t = lumber_world(31.5);
    t.players[1].objects[ListIndex::new(0)]
        .actions
        .hunt_mut()
        .unwrap()
        .point = Position::new(40.0, 89.5, 0.0);
    assert_eq!(search(&mut t, Some(8.0), None), Ok(false));
    assert_eq!(search(&mut t, None, Some(1.5)), Ok(false));
    let mut t = lumber_world(31.5);
    for i in [2, 3] {
        t.players[0].objects[ListIndex::new(i)].life.amount = Amount::ZERO;
    }
    assert_eq!(search(&mut t, None, None), Ok(false));
}

/// A hunt's end of an Attack that failed (`FUN_140ac4670` with -1 and 5,
/// [`Unit::hunt_attack_failed`]): a hunter whose unit AI targets trees takes the next tree
/// within 5 of it; with none so near, or a unit AI that targets another class, the hunt
/// searches (state 3, its timer at -10 s) with its target kept.
#[test]
fn a_hunt_whose_attack_failed_takes_a_near_tree_or_searches() {
    let failed = |t: &mut World| {
        in_update(
            t,
            (PlayerId::new(1), ListIndex::new(0)),
            50,
            Unit::hunt_attack_failed,
        )
    };
    let mut t = lumber_world(31.5);
    assert_eq!(failed(&mut t), Ok(()));
    let h = t.players[1].objects[ListIndex::new(0)]
        .actions
        .hunt()
        .copied()
        .unwrap();
    assert_eq!(
        (h.target, h.attack.map(|a| a.target)),
        (Some(object_id(520)), Some(Some(object_id(520))))
    );
    for (near, target_class) in [(39.0, class::TREE), (31.5, class_id(99))] {
        let mut t = lumber_world(near);
        t.players[1].objects[ListIndex::new(0)]
            .ai
            .as_mut()
            .unwrap()
            .target_class = Some(target_class);
        assert_eq!(failed(&mut t), Ok(()));
        let h = t.players[1].objects[ListIndex::new(0)]
            .actions
            .hunt()
            .copied()
            .unwrap();
        assert_eq!(
            (h.state, h.timer, h.target),
            (HuntState::Search, SEARCH_AGAIN, None)
        );
    }
}

/// [`sent_off_world`] with the queue `orders`, the house built and damaged (300 hit points), and
/// with `repairer` the repairer master 156 (the villager's with the Repair task 4, range 0.5).
fn repairer_world(orders: Vec<crate::unit_ai::QueuedOrder>, repairer: bool) -> World {
    let mut t = sent_off_world(orders);
    let house = &mut t.players[1].objects[ListIndex::new(1)];
    house.life.state = ObjectState::Alive;
    house.life.hp = HitPoints::new(300.0);
    let c = house.construction.as_mut().unwrap();
    (c.progress, c.desolid) = (Seconds::new(1.0), false);
    if repairer {
        for masters in &mut t.masters {
            let mut m = masters.get(VILLAGER_MALE).unwrap().clone();
            m.id = master_id(156);
            m.weapon_range = Distance::new(0.5);
            m.tasks.push(crate::objects::masters::Task {
                id: TaskId::test(4),
                work_val1: 0.5,
                ..task(crate::construction::repair::REPAIR)
            });
            *masters = Masters::new(masters.iter().cloned().chain([m]));
        }
    }
    t
}

/// The unit AI's repair order on the player's damaged house (`0x2ce`, slot 0x378 forced,
/// `FUN_141217c00`): the villager (83, task swap group 1) becomes its player's repairer (156,
/// slot 0x90), whose task list gives the Repair task; the Repair walks to the house (state 4)
/// and the AI holds the repair action on it. Without the repairer master the villager's own list
/// must give the task, which it does not: the order is not taken.
#[test]
fn the_unit_ai_s_repair_order_switches_a_villager_to_the_repairer() {
    use crate::unit_ai::QueuedOrder;
    let order = QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::Repair,
        priority: 100,
        target: Some(object_id(10028)),
        target_owner: owner(1),
        point: Position::new(30.0, 88.0, 0.0),
        range: Distance::ZERO,
        shown: Point::new(30.0, 88.0),
        ready: true,
        target_type: order_type(80),
    };
    let world = |repairer: bool| repairer_world(vec![order], repairer);
    let mut t = world(true);
    t.tick(Millis::new(50));
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(v.life.master_id, master_id(156));
    let r = v.actions.repair().copied().expect("the Repair");
    assert_eq!(
        (r.target, r.state, r.range, r.task.id),
        (
            Some(object_id(10028)),
            RepairState::Move,
            Distance::new(0.5),
            TaskId::test(4)
        )
    );
    let ai = v.ai.as_ref().unwrap();
    assert_eq!(
        (ai.current_action, ai.current_target),
        (Some(ActionKind::Repair), Some(object_id(10028)))
    );
    let mut t = world(false);
    t.tick(Millis::new(50));
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        (v.life.master_id, v.actions.repair().copied()),
        (VILLAGER_MALE, None)
    );
}

/// The unit AI's side of player 1's villager in `t` as its update runs it ([`Unit::with_side`]):
/// what `f` gives and the villager's unit AI as the side returns.
fn villager_side<R>(
    t: &mut World,
    f: impl FnOnce(&mut crate::unit_ai::world_view::UnitView<'_, '_>) -> Result<R, GenieError>,
) -> Result<(R, Option<UnitAi>), GenieError> {
    use crate::unit_ai::world_view::OwnerView;
    in_update(t, (PlayerId::new(1), ListIndex::new(0)), 0, |unit, c| {
        let view = OwnerView::of(c, None).unwrap();
        let attacker =
            unit.attacker_of(UnitAiType::BOAR, None, (PlayerId::new(1), false), false, 0);
        let answer = unit.with_side(
            c,
            &view,
            attacker,
            (PlayerId::new(1), false),
            |_, side, _| f(side),
        )?;
        Ok((answer, unit.ai.clone()))
    })
}

/// The unit AI's repair command at once (slot 0x378 while the AI runs, the villager lent back to
/// its slot): within the run the villager is its player's repairer, holds the Repair, and its top
/// target is the house ([`crate::unit_ai::UnitSide::top_target_now`]).
#[test]
fn the_unit_ai_s_repair_command_is_the_unit_s_within_its_run() {
    use crate::unit_ai::{UnitCommand, UnitSide};
    let mut t = repairer_world(vec![], true);
    let house = Placed {
        position: Position::new(30.0, 88.0, 0.0),
        radius: Footprint::new(1.0, 1.0),
        class: class::BUILDING,
        player: PlayerId::new(1),
        master: ids::HOUSE,
        obstruction_type: ObstructionType::TYPE_2,
        buildable_on: 0,
        tall: true,
        speed: Speed::ZERO,
        unknown: None,
        obstruction: None,
    };
    t.spatial.insert(object_id(10028), house);
    let seen = villager_side(&mut t, |side| {
        let repairs = side.command(UnitCommand::Repair(object_id(10028)))?;
        let r = side.unit.actions.repair().map(|r| r.target);
        Ok((
            repairs,
            side.unit.life.master_id.get(),
            r,
            side.top_target_now()?,
        ))
    });
    assert_eq!(
        seen.map(|(s, _)| s),
        Ok((
            true,
            156,
            Some(Some(object_id(10028))),
            Some(object_id(10028))
        ))
    );
}

/// A world routine the unit AI's side lends its unit to ([`Step::lend`]) finds a stand-in for
/// the AI, which is out of the unit while it runs: one that changes it fails the lend.
///
/// [`Step::lend`]: crate::world::step::Step::lend
#[test]
fn a_world_routine_that_reaches_the_running_unit_ai_fails_the_side_s_lend() {
    let mut t = repairer_world(vec![], false);
    let held = villager_side(&mut t, |side| {
        side.c.lend_at(side.unit, |w, p, i| {
            Ok(w.players[p.index()].objects[i].ai.is_some())
        })
    });
    assert_eq!(held.map(|(h, ai)| (h, ai.is_some())), Ok((true, true)));
    let reached = villager_side(&mut t, |side| {
        side.c.lend_at(side.unit, |w, p, i| {
            w.players[p.index()].objects[i]
                .ai
                .as_mut()
                .unwrap()
                .current_target = Some(object_id(7));
            Ok(())
        })
    });
    let e = reached.expect_err("the stand-in changed");
    assert!(e.to_string().contains("reached the unit's AI"), "{e}");
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .ai
            .as_ref()
            .map(|ai| ai.current_target),
        Some(None)
    );
}

/// The world of [`sent_off_world`] with the queue `orders`, the house built and whole, taking
/// villagers in with room for `room`, both placed and the villager moving.
fn enter_world(orders: Vec<crate::unit_ai::QueuedOrder>, room: u8) -> World {
    let mut t = sent_off_world(orders);
    let house = &mut t.players[1].objects[ListIndex::new(1)];
    house.life.state = ObjectState::Alive;
    house.life.hp = HitPoints::new(550.0);
    let c = house.construction.as_mut().unwrap();
    (c.progress, c.desolid) = (Seconds::new(1.0), false);
    for masters in &mut t.masters {
        let house = masters.get_mut(ids::HOUSE).unwrap();
        house.garrison = room;
        // it takes villagers in (bit 0)
        house.building.as_mut().unwrap().garrison_type = 1;
    }
    t.players[1].objects[ListIndex::new(0)]
        .body
        .put_mover(Some(crate::gathering::hunt::tests::walker()));
    t.spatial = Spatial::with_tiles(
        GridSize::new(120, 120),
        crate::map::spatial::Tiles::new(GridSize::new(120, 120), vec![4; 14400]),
    );
    t.spatial.insert(
        object_id(10028),
        Placed {
            position: Position::new(30.0, 88.0, 0.0),
            radius: Footprint::new(1.0, 1.0),
            class: class::BUILDING,
            player: PlayerId::new(1),
            master: ids::HOUSE,
            obstruction_type: ObstructionType::TYPE_2,
            buildable_on: 0,
            tall: true,
            speed: Speed::ZERO,
            unknown: None,
            obstruction: None,
        },
    );
    t.spatial.insert(
        object_id(9175),
        Placed {
            position: Position::new(29.5, 89.5, 0.0),
            radius: Footprint::new(0.2, 0.2),
            class: class::VILLAGER,
            player: PlayerId::new(1),
            master: VILLAGER_MALE,
            obstruction_type: ObstructionType::NONE,
            buildable_on: 0,
            tall: true,
            speed: Speed::new(0.8),
            unknown: None,
            obstruction: None,
        },
    );
    t
}

/// Slot 0x3a0 (`FUN_1410847e0`) itself: a target the world does not hold is 0; a task found
/// gives the unit its Enter (1); unforced, a unit entering the target already keeps its Enter
/// (1), forced it takes a new one.
#[test]
fn the_enter_command_enters_or_keeps_the_enter_on_its_target() {
    let mut t = enter_world(vec![], 5);
    // the villager out of reach, so that it walks
    t.players[1].objects[ListIndex::new(0)].place_for_test(Position::new(20.0, 80.0, 0.0));
    t.spatial
        .set_position(object_id(9175), Position::new(20.0, 80.0, 0.0))
        .unwrap();
    assert_eq!(
        t.enter_command(PlayerId::new(1), ListIndex::new(0), object_id(4242), true),
        Ok(false)
    );
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .enter()
            .copied(),
        None
    );
    assert_eq!(
        t.enter_command(PlayerId::new(1), ListIndex::new(0), object_id(10028), true),
        Ok(true)
    );
    let first = t.players[1].objects[ListIndex::new(0)]
        .actions
        .enter()
        .copied()
        .expect("the Enter");
    assert_eq!(
        (first.target, first.state),
        (Some(object_id(10028)), crate::garrison::EnterState::Walk)
    );
    t.players[1].objects[ListIndex::new(0)]
        .actions
        .enter_mut()
        .unwrap()
        .timer = Seconds::new(7.0);
    assert_eq!(
        t.enter_command(PlayerId::new(1), ListIndex::new(0), object_id(10028), false),
        Ok(true)
    );
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .enter()
            .copied()
            .unwrap()
            .timer,
        Seconds::new(7.0)
    );
    assert_eq!(
        t.enter_command(PlayerId::new(1), ListIndex::new(0), object_id(10028), true),
        Ok(true)
    );
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .enter()
            .copied()
            .unwrap()
            .timer,
        first.timer
    );
}

/// The Enter's start (`FUN_140a86f20`) of a unit inside an object: `set_state(13)`, its
/// standing sprite and notify 0x1f9 with the enter's action value to its unit AI; the update
/// then returns 3 and the list removes it.
#[test]
fn an_enter_of_a_unit_inside_an_object_fails_at_its_start() {
    let mut t = enter_world(vec![], 5);
    t.players[1].objects[ListIndex::new(0)].life.inside = Some(object_id(4242));
    assert_eq!(
        t.enter_command(PlayerId::new(1), ListIndex::new(0), object_id(10028), true),
        Ok(true)
    );
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        v.actions.enter().map(|e| e.state),
        Some(crate::garrison::EnterState::Failed)
    );
    assert_eq!(
        v.ai.as_ref().unwrap().notifies,
        [Notify::own(
            object_id(9175),
            NotifyKind::ACTION_FAILED,
            [ActionKind::Enter.value(), 0, 0]
        )]
    );
}

/// The unit AI's enter command at once (slot 0x3a0 while the AI runs) of a unit inside an
/// object: its Enter fails at the start, and the notify 0x1f9 for its own AI, which the side does
/// not hold, reaches the AI as the side returns.
#[test]
fn the_unit_ai_s_enter_from_inside_tells_its_ai_as_the_side_returns() {
    use crate::unit_ai::{UnitCommand, UnitSide};
    let mut t = enter_world(vec![], 5);
    t.players[1].objects[ListIndex::new(0)].life.inside = Some(object_id(4242));
    let seen = villager_side(&mut t, |side| {
        let enters = side.command(UnitCommand::Enter(object_id(10028)))?;
        Ok((enters, side.unit.actions.enter().map(|e| e.state)))
    });
    let failed = Notify::own(
        object_id(9175),
        NotifyKind::ACTION_FAILED,
        [ActionKind::Enter.value(), 0, 0],
    );
    assert_eq!(
        seen.map(|(s, ai)| (s, ai.map(|ai| ai.notifies))),
        Ok((
            (true, Some(crate::garrison::EnterState::Failed)),
            Some(vec![failed])
        ))
    );
}

/// [`enter_world`] whose villager is a sheep under its Artifact, the house taking herdables in.
fn livestock_enter_world() -> World {
    let mut t = enter_world(vec![], 5);
    let sheep = Master {
        id: master_id(594),
        kind: master_type::COMBAT,
        class: class::LIVESTOCK,
        radius: Footprint::new(0.2, 0.2),
        tasks: vec![crate::objects::masters::Task {
            id: TaskId::test(1),
            action_type: crate::garrison::ENTER,
            object_group: Some(class_id(3)),
            ..crate::objects::masters::Task::default()
        }],
        moving: Some(crate::gathering::hunt::tests::walker().stats),
        // no walking or standing sprite: its Enter's MoveTo takes none (the world's sprites'
        // table holds none)
        move_sprite: None,
        standing_sprite: None,
        ..Master::default()
    };
    for masters in &mut t.masters {
        *masters = Masters::new(masters.iter().cloned().chain([sheep.clone()]));
        // the house takes herdables in (bit 4)
        masters
            .get_mut(ids::HOUSE)
            .unwrap()
            .building
            .as_mut()
            .unwrap()
            .garrison_type = 1 << 4;
    }
    let v = &mut t.players[1].objects[ListIndex::new(0)];
    v.life.master_id = master_id(594);
    v.actions.set_artifact(Some(Artifact::new(false, None)));
    t
}

/// The enter command of livestock under its Artifact (`FUN_1410847e0`): the Enter becomes the
/// Artifact's sub-action (its list emptied, `FUN_140a98e20`, then `FUN_140a977e0`), the Artifact
/// staying the top action; the next update runs it through the Artifact's.
#[test]
fn livestock_s_enter_command_becomes_its_artifact_s_sub_action() {
    let mut t = livestock_enter_world();
    assert_eq!(
        t.enter_command(PlayerId::new(1), ListIndex::new(0), object_id(10028), false),
        Ok(true)
    );
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(v.actions.enter().copied(), None);
    assert_eq!(
        v.top_action_kind(),
        Some(crate::ownership::artifact::ARTIFACT_TYPE)
    );
    let sub = v.actions.artifact().copied().unwrap().enter;
    assert!(
        sub.is_some_and(|e| e.target == Some(object_id(10028)))
            || v.life.inside == Some(object_id(10028)),
        "{v:?}"
    );
}

/// The Enter's `MoveTo` shows its sprite as it starts (`FUN_140a87320` case 4 gives its
/// constructor the task's moving sprite, `0x140aa29e0`'s `set_state(4)` shows it): the task's,
/// else `FUN_140aa0df0`'s default toward a building, the master's walking sprite.
#[test]
fn an_enter_s_move_to_shows_its_sprite_as_it_starts() {
    for (moving, shown) in [
        (Some(SpriteId::named(30)), SpriteId::named(30)),
        (None, SpriteId::named(31)),
    ] {
        let mut t = enter_world(vec![], 5);
        t.pathing = Pathing::new(
            GridSize::new(120, 120),
            vec![0; 120 * 120],
            vec![vec![1.0]; 8],
            0,
        );
        t.build_mesh().unwrap();
        for id in [30, 31] {
            let g = Graphic {
                id: SpriteId::named(id),
                frames: 10,
                facets: 16,
                speed_multiplier: 1.0,
                frame_duration: Seconds::new(0.1),
                replay_delay: Seconds::ZERO,
                flags: 1,
            };
            let _ = t.graphics.insert(SpriteId::named(id), g);
        }
        t.masters[1].get_mut(VILLAGER_MALE).unwrap().move_sprite = Some(SpriteId::named(31));
        let v = &mut t.players[1].objects[ListIndex::new(0)];
        let here = v.position();
        v.body.mover_mut().unwrap().moved_at = here;
        let task = crate::garrison::tests::enter_task(Distance::ZERO, class::BUILDING);
        // waiting at its end: the update's walk again makes the MoveTo with its sprite, the
        // next update starts it and shows the sprite
        v.actions.set_enter(Some(crate::garrison::EnterAction {
            state: crate::garrison::EnterState::Wait,
            timer: Seconds::ZERO,
            moving,
            ..crate::garrison::EnterAction::new(
                object_id(10028),
                Position::new(30.0, 88.0, 0.0),
                &task,
            )
        }));
        let _ = in_update(
            &mut t,
            (PlayerId::new(1), ListIndex::new(0)),
            50,
            Unit::update,
        )
        .unwrap();
        let made = t.players[1].objects[ListIndex::new(0)]
            .actions
            .enter()
            .and_then(|e| e.move_to);
        assert_eq!(
            made.map(|m| (m.state, m.sprite.map(|g| g.id))),
            Some((MoveTo::Start, Some(shown)))
        );
        let _ = in_update(
            &mut t,
            (PlayerId::new(1), ListIndex::new(0)),
            50,
            Unit::update,
        )
        .unwrap();
        let v = &t.players[1].objects[ListIndex::new(0)];
        assert_eq!(
            v.actions.enter().and_then(|e| e.move_to).map(|m| m.state),
            Some(MoveTo::Moving)
        );
        assert_eq!(v.sprite.map(|s| s.graphic.id), Some(shown));
    }
}

/// A second enter command of livestock walking under its Artifact (430061417's pig 7920 in
/// update 32, INTERACT on the transport again): the new Enter takes the house (slot 0x178), then
/// `FUN_140a98e20` pops the old one, whose teardown lets the house go (slot 0x180) and whose
/// `MoveTo`'s stops the unit: its speed and acceleration 0, so the update's velocity step does
/// not move it.
#[test]
fn livestock_s_second_enter_command_stops_the_walk_and_lets_the_old_enter_s_target_go() {
    let mut t = livestock_enter_world();
    let mut walking = crate::garrison::EnterAction::new(
        object_id(10028),
        Position::new(30.0, 88.0, 0.0),
        &crate::garrison::tests::enter_task(Distance::ZERO, class::BUILDING),
    );
    walking = walking.walking(MoveTo::Moving);
    let v = &mut t.players[1].objects[ListIndex::new(0)];
    v.actions.artifact_mut().unwrap().enter = Some(walking);
    v.body.mover_mut().unwrap().velocity = Some(crate::movement::Velocity {
        velocity: BVector::new(0.0, 0.0, 0.7),
        acceleration: BVector::new(0.0, 0.0, f32::MAX),
    });
    // the old Enter took the house
    t.players[1].objects[ListIndex::new(1)].workers = 1;
    assert_eq!(
        t.enter_command(PlayerId::new(1), ListIndex::new(0), object_id(10028), true),
        Ok(true)
    );
    let v = &t.players[1].objects[ListIndex::new(0)];
    let velocity = v.body.mover().unwrap().velocity.unwrap();
    assert_eq!(
        (velocity.velocity.y(), velocity.acceleration.y()),
        (0.0, 0.0)
    );
    assert_eq!(t.players[1].objects[ListIndex::new(1)].workers, 1);
}

/// The inside update (`FUN_140a98790`) of a villager inside the house deletes its top action
/// whatever its kind: a `MoveTo` left from before the enter goes (its destructor stops the
/// unit), and the update after has nothing left to delete (426027790-style villagers in a town
/// bell: 427445417's p3, 429184426's p4).
#[test]
fn an_inside_unit_s_update_deletes_its_top_action() {
    let mut t = enter_world(vec![], 5);
    t.players[1].objects[ListIndex::new(0)].life.inside = Some(object_id(10028));
    t.spatial
        .go_inside(object_id(9175), Position::new(30.0, 88.0, 0.0));
    t.players[1].objects[ListIndex::new(0)]
        .actions
        .set_move_to(Some(MoveToAction::to_point(
            Position::new(40.0, 80.0, 0.0),
            Distance::ZERO,
            None,
            None,
        )));
    t.tick(Millis::new(50));
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(v.actions.move_to().copied(), None);
    assert_eq!(v.top_action_kind(), None);
    assert_eq!(t.players[1].freezes, vec![]);
}

/// An Enter whose walk arrived in reach gets its unit in in its update (`FUN_140a867b0`): the
/// building adds the unit to its list in the same update, its own player's and an ally's
/// (player 2's) alike.
#[test]
fn an_enter_s_update_adds_its_unit_to_its_building_own_or_an_ally_s() {
    let entered = |ally: bool| {
        let mut t = enter_world(vec![], 5);
        if ally {
            let house = t.players[1].objects.remove(ListIndex::new(1)).unwrap();
            let mut ally = Player {
                attributes: vec![Amount::ZERO; 250],
                ..Player::default()
            };
            ally.objects.push(house);
            t.players.push(ally);
            t.masters.push(t.masters[1].clone());
            let mut placed = *t.spatial.get(object_id(10028)).unwrap();
            placed.player = PlayerId::new(2);
            t.spatial.insert(object_id(10028), placed);
            t.players[1].relations = relation_row([3, 0, 0]);
            t.players[2].relations = relation_row([3, 0, 0]);
            t.spatial.relations = vec![
                relation_row([0, 3, 3]),
                relation_row([3, 0, 0]),
                relation_row([3, 0, 0]),
            ];
        }
        let mut e = crate::garrison::EnterAction::new(
            object_id(10028),
            Position::new(30.0, 88.0, 0.0),
            &crate::garrison::tests::enter_task(Distance::new(2.0), class::BUILDING),
        );
        e = e.walking(MoveTo::Arrived);
        let v = &mut t.players[1].objects[ListIndex::new(0)];
        v.place_for_test(Position::new(29.5, 89.5, 0.0));
        v.actions.set_enter(Some(e));
        in_update(
            &mut t,
            (PlayerId::new(1), ListIndex::new(0)),
            50,
            |unit, c| {
                let me = crate::movement::Me {
                    id: unit.id,
                    group: unit.group.id,
                    action_state: unit.top_action().map_or(0, |a| a.state),
                };
                let code = unit.enter_part(c, me)?;
                unit.end_head_update(c, code)?;
                let (_, house) = OthersRef::of(&c.w.players, c.player)
                    .unwrap()
                    .find(object_id(10028))
                    .unwrap();
                Ok((unit.life.inside, house.garrison.clone()))
            },
        )
    };
    assert_eq!(
        entered(false),
        Ok((Some(object_id(10028)), vec![object_id(9175)]))
    );
    assert_eq!(
        entered(true),
        Ok((Some(object_id(10028)), vec![object_id(9175)]))
    );
}

/// The Enter's enter takes the unit out of its group in the update (removeUnit, `FUN_14102c160`
/// inside `FUN_140a867b0`): the group has lost it and the unit its link while the update still
/// runs.
#[test]
fn an_enter_s_unit_leaves_its_group_inside_the_update() {
    let mut t = enter_world(vec![], 5);
    let member = |id| crate::groups::Member {
        id,
        ..crate::groups::Member::default()
    };
    t.groups.groups.push(crate::groups::UnitGroup {
        id: group_id(7),
        owner: 1,
        members: vec![member(object_id(9175)), member(object_id(401))],
        ..crate::groups::UnitGroup::default()
    });
    let mut e = crate::garrison::EnterAction::new(
        object_id(10028),
        Position::new(30.0, 88.0, 0.0),
        &crate::garrison::tests::enter_task(Distance::new(2.0), class::BUILDING),
    );
    e = e.walking(MoveTo::Arrived);
    let v = &mut t.players[1].objects[ListIndex::new(0)];
    v.place_for_test(Position::new(29.5, 89.5, 0.0));
    v.actions.set_enter(Some(e));
    v.group.id = Some(group_id(7));
    let left = in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, c| {
            let me = crate::movement::Me {
                id: unit.id,
                group: unit.group.id,
                action_state: unit.top_action().map_or(0, |a| a.state),
            };
            let code = unit.enter_part(c, me)?;
            unit.end_head_update(c, code)?;
            let members: Vec<ObjectId> =
                c.w.groups
                    .get(group_id(7))
                    .map_or_else(Vec::new, |g| g.members.iter().map(|m| m.id).collect());
            Ok((unit.life.inside, unit.group.id, members))
        },
    );
    assert_eq!(
        left,
        Ok((Some(object_id(10028)), None, vec![object_id(401)]))
    );
}

/// An Enter that got the unit in, the unit let out again before its next update: that update's
/// 1 has the action list remove it (its teardown lets the building go).
#[test]
fn an_entered_enter_of_a_unit_let_out_goes_at_its_update() {
    let mut t = enter_world(vec![], 5);
    let mut e = crate::garrison::EnterAction::new(
        object_id(10028),
        Position::new(30.0, 88.0, 0.0),
        &crate::garrison::tests::enter_task(Distance::new(0.5), class::BUILDING),
    );
    e.state = crate::garrison::EnterState::Done;
    t.players[1].objects[ListIndex::new(0)]
        .actions
        .set_enter(Some(e));
    t.tick(Millis::new(50));
    t.tick(Millis::new(50));
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .enter()
            .copied(),
        None
    );
    assert_eq!(t.players[1].freezes, vec![]);
}

/// The unit AI's enter order on the player's house (`0x2cb`, slot 0x3a0 forced,
/// `FUN_1410847e0`): its slot 0x820 with the garrison filter finds the villager's Enter task on
/// buildings (action 3, class 3); after the update the Enter is the unit's (or the unit inside
/// the house), and the AI holds the enter action on it. A house without room gives no task:
/// the order is not taken.
#[test]
fn the_unit_ai_s_enter_order_gives_the_unit_an_enter() {
    use crate::unit_ai::QueuedOrder;
    let order = QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::Enter,
        priority: 100,
        target: Some(object_id(10028)),
        target_owner: owner(1),
        point: Position::new(30.0, 88.0, 0.0),
        range: Distance::ZERO,
        shown: Point::new(30.0, 88.0),
        ready: true,
        target_type: order_type(80),
    };
    let mut t = enter_world(vec![order], 5);
    t.tick(Millis::new(50));
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert!(
        v.actions
            .enter()
            .is_some_and(|e| e.target == Some(object_id(10028)))
            || v.life.inside == Some(object_id(10028)),
        "{:?}",
        t.players[1].freezes
    );
    let ai = v.ai.as_ref().unwrap();
    assert_eq!(
        (ai.current_action, ai.current_target),
        (Some(ActionKind::Enter), Some(object_id(10028)))
    );
    let mut t = enter_world(vec![order], 0);
    t.tick(Millis::new(50));
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        (t.players[1].freezes.len(), v.actions.enter().copied()),
        (0, None)
    );
    assert_ne!(
        v.ai.as_ref().unwrap().current_action,
        Some(ActionKind::Enter)
    );
}

/// The whole of it: a villager the foundation sent off (its queue the foundation's order)
/// runs the order on its turn (slot 0x108 forced: its slot 0x388 finds the builder's task by
/// the swap group), becomes the builder with the wait as its only action, and keeps waiting
/// while the foundation has no build points; once they pass 0.01 its next update drops the
/// wait, the unit AI's action value staying 0x27a.
#[test]
fn a_villager_sent_off_a_foundation_waits_for_it_as_a_builder() {
    use crate::unit_ai::QueuedOrder;
    let order = QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::Foundation,
        priority: 100,
        target: Some(object_id(10028)),
        target_owner: owner(1),
        point: Position::new(30.0, 0.0, 0.0),
        range: Distance::ZERO,
        shown: Point::new(30.0, 0.0),
        ready: true,
        target_type: order_type(70),
    };
    let mut t = sent_off_world(vec![order]);
    t.tick(Millis::new(50));
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert!(!v.stopped, "{:?}", t.players[1].objects[ListIndex::new(0)]);
    assert_eq!(v.life.master_id, ids::BUILDER);
    assert_eq!(
        v.top_action_kind(),
        Some(crate::construction::wait_solidify::WAIT_SOLIDIFY)
    );
    let ai = v.ai.as_ref().unwrap();
    assert_eq!(
        (ai.current_action, ai.last_action, ai.current_order),
        (
            Some(ActionKind::WaitSolidify),
            Some(ActionKind::WaitSolidify),
            Some(OrderKind::Foundation)
        )
    );
    assert_eq!(ai.orders, vec![]);
    // player 0's update, then player 1's: the wait reads 0 build points and stays
    t.tick(Millis::new(50));
    t.tick(Millis::new(50));
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)].top_action_kind(),
        Some(crate::construction::wait_solidify::WAIT_SOLIDIFY)
    );
    t.players[1].objects[ListIndex::new(1)]
        .construction
        .as_mut()
        .unwrap()
        .progress = Seconds::new(0.5);
    t.tick(Millis::new(50));
    t.tick(Millis::new(50));
    assert_eq!(t.players[1].freezes, vec![]);
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(v.actions.wait_solidify().copied(), None);
    assert_eq!(
        v.ai.as_ref().and_then(|ai| ai.current_action),
        Some(ActionKind::WaitSolidify)
    );
}

/// Slot 0x230's build order (`0x2be`): slot 0xf0 builds the foundation (the builder's task by
/// the swap group), then the tail makes the order the AI's with its priority, leaving a patrol
/// order: its secondary timer capped at 750 and an adjusted idle timeout of 750 redrawn.
#[test]
fn a_queued_build_order_builds_and_leaves_a_patrol_order() {
    use crate::unit_ai::QueuedOrder;
    let order = QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::Build,
        priority: 100,
        target: Some(object_id(10028)),
        target_owner: owner(1),
        point: Position::new(30.0, 88.0, 0.0),
        range: Distance::ZERO,
        shown: Point::new(30.0, 88.0),
        ready: true,
        target_type: order_type(70),
    };
    let mut t = sent_off_world(vec![]);
    {
        let ai = t.players[1].objects[ListIndex::new(0)].ai.as_mut().unwrap();
        ai.current_order = Some(OrderKind::Patrol);
        ai.secondary_timer = Millis::new(900);
        ai.adjusted_idle_timeout = Millis::new(750);
        ai.idle_timeout = Millis::new(3000);
    }
    let draws = t.streams.get(Stream::Ai).draws;
    // the unit AI runs the order (slot 0x230) and gives the build command, which runs at once
    t.run_unit_ai(
        (PlayerId::new(1), ListIndex::new(0)),
        |ai, at, side, _, rng| {
            let r = ai.process_order(&order, Some((at, false)), None, rng, side)?;
            assert_eq!(r, 1);
            Ok(())
        },
    )
    .unwrap();
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(v.life.master_id, ids::BUILDER);
    assert!(v.actions.build().is_some());
    let ai = v.ai.as_ref().unwrap();
    assert_eq!(
        (
            ai.current_order,
            ai.order_priority,
            ai.current_action,
            ai.current_target
        ),
        (
            Some(OrderKind::Build),
            100,
            Some(ActionKind::Build),
            Some(object_id(10028))
        )
    );
    assert_eq!(ai.secondary_timer, Millis::new(750));
    assert_ne!(ai.adjusted_idle_timeout, Millis::new(750));
    assert_eq!(t.streams.get(Stream::Ai).draws, draws + 1);
}

/// Slot 0x380 (`FUN_14120d410`) answers 0 and does nothing for a foundation the world does not
/// hold and for a unit without a task on it (its own master's list and its swap group's); the
/// Gather's `set_state(6)` ignores that 0.
#[test]
fn a_build_command_without_its_foundation_or_a_task_does_nothing() {
    let without_build_tasks = |t: &mut World| {
        for id in [118, 212] {
            let m = t.masters[1].get_mut(master_id(id)).unwrap();
            m.tasks.retain(|k| k.action_type != action_type::BUILD);
        }
    };
    let mut t = sent_off_world(vec![]);
    let before = t.players[1].objects[ListIndex::new(0)].clone();
    assert_eq!(
        t.build_command(PlayerId::new(1), ListIndex::new(0), object_id(99_999)),
        Ok(false)
    );
    without_build_tasks(&mut t);
    assert_eq!(
        t.build_command(PlayerId::new(1), ListIndex::new(0), object_id(10028)),
        Ok(false)
    );
    assert_eq!(t.players[1].objects[ListIndex::new(0)], before);
    // with a task it builds as the builder 118
    let mut t = sent_off_world(vec![]);
    assert_eq!(
        t.build_command(PlayerId::new(1), ListIndex::new(0), object_id(10028)),
        Ok(true)
    );
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)].life.master_id,
        ids::BUILDER
    );
}

/// A world routine's unit AI run (a stance's, a game status's or a let-out's stop,
/// `World::run_ai_calls`): the AI's calls run on the unit at once, so the AI reads the action
/// list a stop left before it returns, and the unit is back in its slot after it.
#[test]
fn a_world_routine_s_unit_ai_calls_run_on_the_unit_at_once() {
    let mut t = sent_off_world(vec![]);
    let (explore, _) = crate::movement::explore::ExploreAction::new(
        Position::new(10.0, 10.0, 0.0),
        &crate::movement::explore::bare_walk(),
    )
    .unwrap();
    t.players[1].objects[ListIndex::new(0)]
        .actions
        .set_explore(Some(explore));
    let seen = t
        .run_ai_calls((PlayerId::new(1), ListIndex::new(0)), |ai, side, rng| {
            let before = side.busy_now()?;
            ai.stop_with(true, None, side, rng)?;
            Ok((before, side.busy_now()?))
        })
        .unwrap();
    assert_eq!(seen, (true, false));
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!((v.id.get(), v.top_action().is_none()), (9175, true));
}

/// The unit AI reads its target where it runs (`FUN_1415417a0` on `+0x48`), not as the unit's
/// step began: a target the update changed (the Build's work order on a resource) is the new
/// one, and a unit that is its own target reads itself out of its slot.
#[test]
fn the_unit_ai_reads_the_target_its_unit_s_update_left() {
    use crate::unit_ai::world_view::OwnerView;
    let mut t = sent_off_world(vec![]);
    let mut read = Vec::new();
    t.run_on_step(PlayerId::new(1), ListIndex::new(0), |unit, c| {
        for target in [10028, 9175, -1] {
            unit.ai.as_mut().unwrap().current_target = ObjectId::of_field(target);
            let master = unit.master(&c.w.masters, PlayerId::new(1)).ok();
            let at = unit.surroundings(
                c,
                &OwnerView::of(c, None).unwrap(),
                (master, master.map(|m| m.class)),
                (PlayerId::new(1), false, false),
            )?;
            read.push(at.target.map(|s| (s.state, s.owner, s.position)));
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(
        read,
        [
            Some((
                ObjectState::Foundation,
                PlayerId::new(1),
                Position::new(30.0, 88.0, 0.0)
            )),
            Some((
                ObjectState::Alive,
                PlayerId::new(1),
                Position::new(29.5, 89.5, 0.0)
            )),
            None
        ]
    );
}

/// The unit AI's view answers slot 0x380 before `BuildObject` takes the build
/// (`UnitSide::builds`): 0 for a foundation the world does not hold or no task for it, 1 with a
/// task or a Build on top (its slot 0x48), whatever the master's lists then hold.
#[test]
fn the_unit_ai_s_view_answers_the_build_command_before_it_runs() {
    let answers = |t: &mut World, foundation| {
        let mut answer = None;
        t.run_unit_ai((PlayerId::new(1), ListIndex::new(0)), |_, _, side, _, _| {
            answer = Some(side.builds(foundation)?);
            Ok(())
        })
        .unwrap();
        answer.unwrap()
    };
    let mut t = sent_off_world(vec![]);
    assert!(answers(&mut t, object_id(10028)));
    assert!(!answers(&mut t, object_id(99_999)));
    // the build command on top: a Build answers 1 without a task in the lists
    assert_eq!(
        t.build_command(PlayerId::new(1), ListIndex::new(0), object_id(10028)),
        Ok(true)
    );
    for id in [83, 118, 212] {
        let m = t.masters[1].get_mut(master_id(id)).unwrap();
        m.tasks.retain(|k| k.action_type != action_type::BUILD);
    }
    assert!(answers(&mut t, object_id(10028)));
    let mut t = sent_off_world(vec![]);
    for id in [118, 212] {
        let m = t.masters[1].get_mut(master_id(id)).unwrap();
        m.tasks.retain(|k| k.action_type != action_type::BUILD);
    }
    assert!(!answers(&mut t, object_id(10028)));
}

/// The build command at once from the unit AI's run (`UnitSide::build_now`: a requeued build
/// order's, which a following attack command replaces) does what the build command after the
/// update does: the switch to the builder (118), the Build on the foundation, which it takes.
#[test]
fn the_build_command_at_once_matches_the_one_after_the_update() {
    let mut later = sent_off_world(vec![]);
    let mut now = later.clone();
    let _ = later
        .build_command(PlayerId::new(1), ListIndex::new(0), object_id(10028))
        .unwrap();
    now.run_unit_ai(
        (PlayerId::new(1), ListIndex::new(0)),
        |ai, _, side, _, rng| {
            side.effect_now(
                Effect::Command(AiCommand::BuildObject(object_id(10028))),
                ai,
                rng,
            )
        },
    )
    .unwrap();
    let (a, b) = (
        &later.players[1].objects[ListIndex::new(0)],
        &now.players[1].objects[ListIndex::new(0)],
    );
    assert_eq!(b.life.master_id, ids::BUILDER);
    assert!(
        b.actions
            .build()
            .is_some_and(|build| build.target == Some(object_id(10028)))
    );
    assert_eq!(
        (b.life.master_id, b.actions.build().copied(), &b.sprite),
        (a.life.master_id, a.actions.build().copied(), &a.sprite)
    );
    assert_eq!(now.players[1].attributes, later.players[1].attributes);
    assert_eq!(now.streams, later.streams);
    let taken = |t: &World| t.object(object_id(10028)).map(|(_, f)| f.clone());
    assert_eq!(taken(&now), taken(&later));
}

/// `FUN_141205ac0`, the way back after a drop-off: the order on a target `0x325` in front of the
/// unit AI's queue, on the town-bell record's object the world holds (its master the target
/// type, its owner the target's, its place the point), else on the record's target as recorded
/// (owner −1, the record's point); issued by the unit's owner, priority 100, ready.
#[test]
fn the_way_back_after_a_drop_off_queues_the_order_on_the_recorded_work() {
    use crate::orders::town_bell::BackToWork;
    let mut t = sent_off_world(vec![]);
    let work = BackToWork {
        target: Some(object_id(10028)),
        target_class: class_of(3),
        action: Some(ActionKind::Build),
        target2: None,
        point: Point::new(1.0, 2.0),
    };
    t.players[1].objects[ListIndex::new(0)].town_bell.work = work;
    t.queue_way_back(PlayerId::new(1), ListIndex::new(0))
        .unwrap();
    let ai = t.players[1].objects[ListIndex::new(0)].ai.as_ref().unwrap();
    assert_eq!(
        ai.orders,
        [crate::unit_ai::QueuedOrder {
            issuer: Some(Issuer::Player(player_id(1))),
            order: OrderKind::OnTarget2,
            priority: 100,
            target: Some(object_id(10028)),
            target_owner: owner(1),
            point: Position::new(30.0, 88.0, 0.0),
            range: Distance::ZERO,
            shown: Point::new(30.0, 88.0),
            ready: true,
            target_type: order_type(70),
        }]
    );
    // a recorded target the world no longer holds: the record as it is, in front
    t.players[1].objects[ListIndex::new(0)].town_bell.work = BackToWork {
        target: Some(object_id(999)),
        ..work
    };
    t.queue_way_back(PlayerId::new(1), ListIndex::new(0))
        .unwrap();
    let first = t.players[1].objects[ListIndex::new(0)]
        .ai
        .as_ref()
        .unwrap()
        .orders[0];
    assert_eq!(
        (
            first.target,
            first.target_type,
            first.target_owner,
            first.point
        ),
        (
            Some(object_id(999)),
            OrderTargetType::of_class(class::BUILDING),
            None,
            Position::new(1.0, 2.0, 0.0)
        )
    );
}

/// `UNIT_ORDER`'s drop-off (action 8, `FUN_14161e370`) runs on any player's units its command
/// names: a carrying villager without a gather action, of a master without drop sites and a
/// class the drop-site table does not list, does nothing, whoever commands it.
#[test]
fn a_unit_order_drop_off_takes_any_player_s_unit() {
    let mut t = sent_off_world(vec![]);
    let v = &mut t.players[1].objects[ListIndex::new(0)];
    (v.life.attribute_type, v.life.amount) = (Some(attribute::WOOD), Amount::new(5.0));
    let before = t.players[1].objects[ListIndex::new(0)].clone();
    t.unit_order(player_id(1), &object_ids([9175]), 8).unwrap();
    t.unit_order(player_id(0), &object_ids([9175]), 8).unwrap();
    assert_eq!(t.players[1].objects[ListIndex::new(0)], before);
}

/// The unit AI's move at once (`UnitSide::ai_move_now`, ahead of a stop and an attack command
/// in the same run) does what the move after the update does (slot 0x340: the actions go, a
/// `MoveTo` to the point).
#[test]
fn the_ai_move_at_once_matches_the_one_after_the_update() {
    let mut later = sent_off_world(vec![]);
    later.players[1].objects[ListIndex::new(0)]
        .body
        .put_mover(Some(crate::gathering::hunt::tests::walker()));
    let mut now = later.clone();
    let m = crate::unit_ai::AiMove {
        point: Position::new(35.0, 80.0, 0.0),
        range: Distance::ZERO,
        force: true,
        walk: false,
    };
    later
        .move_aside(PlayerId::new(1), ListIndex::new(0), m)
        .unwrap();
    now.run_unit_ai(
        (PlayerId::new(1), ListIndex::new(0)),
        |ai, _, side, _, rng| side.effect_now(Effect::Move(m), ai, rng),
    )
    .unwrap();
    let (a, b) = (
        &later.players[1].objects[ListIndex::new(0)],
        &now.players[1].objects[ListIndex::new(0)],
    );
    assert_eq!(
        b.actions.move_to().copied().and_then(|mt| mt.goal()),
        Some(Position::new(35.0, 80.0, 0.0))
    );
    assert_eq!(
        (b.actions.move_to().copied(), &b.body, &b.sprite),
        (a.actions.move_to().copied(), &a.body, &a.sprite)
    );
    assert_eq!(now.streams, later.streams);
}

/// The hunt command at once from the unit AI's run (`UnitSide::hunt_now`: a requeued hunt
/// order's, which a following attack command replaces) does what `HuntObject`'s unit part after
/// the update does: the stopAction's standing sprite, the switch to the hunter (122) whose task
/// hunts deer (class 9), the Hunt on the deer, which it takes.
#[test]
fn the_hunt_command_at_once_matches_the_one_after_the_update() {
    let mut later = sent_off_world(vec![]);
    for masters in &mut later.masters {
        let hunter = Master {
            id: master_id(122),
            tasks: vec![crate::objects::masters::Task {
                action_type: crate::gathering::hunt::HUNT,
                object_group: Some(class_id(9)),
                work_range: Distance::new(0.5),
                ..crate::objects::masters::Task::default()
            }],
            ..masters.get(VILLAGER_MALE).unwrap().clone()
        };
        let deer = Master {
            id: master_id(65),
            kind: master_type::COMBAT,
            class: class::PREY,
            radius: Footprint::new(0.4, 0.4),
            ..Master::default()
        };
        *masters = Masters::new(masters.iter().cloned().chain([hunter, deer]));
    }
    later.players[0].objects.push(Unit {
        id: object_id(500),
        body: crate::objects::layers::Body::at(Position::new(32.5, 89.5, 0.0)),
        life: Lifecycle {
            state: ObjectState::Alive,
            hp: HitPoints::new(5.0),
            master_id: master_id(65),
            ..Lifecycle::default()
        },
        ..Unit::default()
    });
    let placed = |x: f32, class, player, master| Placed {
        position: Position::new(x, 89.5, 0.0),
        radius: Footprint::new(0.2, 0.2),
        class,
        player,
        master,
        obstruction_type: ObstructionType::NONE,
        buildable_on: 0,
        tall: true,
        speed: Speed::ZERO,
        unknown: None,
        obstruction: None,
    };
    later.spatial.insert(
        object_id(9175),
        placed(29.5, class::VILLAGER, PlayerId::new(1), VILLAGER_MALE),
    );
    later.spatial.insert(
        object_id(500),
        placed(32.5, class::PREY, PlayerId::new(0), master_id(65)),
    );
    later.players[1].objects[ListIndex::new(0)]
        .body
        .put_mover(Some(crate::gathering::hunt::tests::walker()));
    let mut now = later.clone();
    later
        .hunt_command(PlayerId::new(1), ListIndex::new(0), object_id(500))
        .unwrap();
    now.run_unit_ai(
        (PlayerId::new(1), ListIndex::new(0)),
        |ai, _, side, _, rng| {
            side.effect_now(
                Effect::Command(AiCommand::HuntObject(object_id(500))),
                ai,
                rng,
            )
        },
    )
    .unwrap();
    let (a, b) = (
        &later.players[1].objects[ListIndex::new(0)],
        &now.players[1].objects[ListIndex::new(0)],
    );
    assert_eq!(b.life.master_id, master_id(122));
    assert!(
        b.actions
            .hunt()
            .is_some_and(|h| h.target == Some(object_id(500)))
    );
    assert_eq!(
        (
            b.life.master_id,
            b.actions.hunt().copied(),
            &b.sprite,
            &b.body
        ),
        (
            a.life.master_id,
            a.actions.hunt().copied(),
            &a.sprite,
            &a.body
        )
    );
    assert_eq!(now.players[1].attributes, later.players[1].attributes);
    assert_eq!(now.streams, later.streams);
    assert_eq!(now.players[0].objects, later.players[0].objects);
}

#[test]
fn a_build_creates_a_foundation_whose_unit_ai_draws_before_the_turn_players_units() {
    // MarvinBlue/426048731 tick 281: player 1's BUILD draws three AI values in the command
    // phase, so player 2's units, on their turn, get the draws after them (a computer's
    // BUILD here: it needs no builder that can build it)
    let world = |objects: Vec<Unit>| {
        let mut t = World::new(
            WorldTime::ZERO,
            29910,
            PlayerId::new(1),
            vec![Millis::ZERO, Millis::ZERO],
            vec![
                Player {
                    attributes: crate::test_ids::attributes(250, [0.0, 100.0, 0.0, 0.0]),
                    computer: true,
                    ..Player::default()
                },
                Player {
                    objects: objects.into(),
                    ..Player::default()
                },
            ],
        );
        t.next_object_id = object_id(100);
        t.masters = vec![builder_masters(), builder_masters()];
        placeable(&mut t);
        t
    };
    let mut built = world(vec![live(2)]);
    stock_masters(&mut built);
    built.tick_with(
        Millis::new(40),
        &[Command::Build(BuildCommand {
            player: player_id(0),
            master: master_id(70),
            position: Point::new(10.0, 10.0),
            q_id: -1,
            sprite_override: -1,
            builders: unit_list(vec![]),
            queued: false,
            commits: false,
            frame: 0,
        })],
    );
    let foundation = &built.players[0].objects[ListIndex::new(0)];
    assert_eq!(
        (
            foundation.id.get(),
            foundation.life.state,
            foundation.life.master_id.get()
        ),
        (100, ObjectState::Foundation, 70)
    );
    assert_eq!(built.next_object_id, object_id(101));
    // the same tick without the BUILD, with three AI draws taken by hand after the reseed
    let mut by_hand = world(vec![live(2)]);
    let mut streams = Streams::default();
    streams.seed_all(29910);
    let ai = streams.get(Stream::Ai);
    let new = crate::unit_ai::NewObject {
        position: Position::new(10.0, 10.0, 0.0),
        ..crate::unit_ai::NewObject::default()
    };
    let first = UnitAi::constructed(UnitAiType::BUILDING, &new, ai).unwrap();
    assert_eq!(foundation.ai, Some(first));
    let next = 750 + ai.rand15() % 500;
    stock_masters(&mut by_hand);
    by_hand.tick(Millis::new(40));
    assert_eq!(
        built.players[1].objects[ListIndex::new(0)]
            .ai
            .as_ref()
            .unwrap()
            .retarget_timer,
        Millis::new(next)
    );
    assert_ne!(
        by_hand.players[1].objects[ListIndex::new(0)]
            .ai
            .as_ref()
            .unwrap()
            .retarget_timer,
        Millis::new(next)
    );
}

#[test]
fn an_order_resets_the_idle_timeouts_of_its_units_wherever_they_are() {
    // MOVE: one AI draw per listed unit with a unit AI, in list order, before the turn player's
    // units update; the ordering player's lane stops
    let ordered = |id| Unit {
        id,
        ai: Some(UnitAi {
            idle_timeout: Millis::new(3000),
            adjusted_idle_timeout: Millis::new(3000),
            ..building()
        }),
        ..Unit::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        29910,
        PlayerId::new(1),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player {
                objects: vec![ordered(object_id(7)), ordered(object_id(8))].into(),
                ..Player::default()
            },
            Player {
                objects: vec![live(2)].into(),
                ..Player::default()
            },
        ],
    );
    stock_masters(&mut t);
    t.tick_with(
        Millis::new(40),
        &[Command::Order(OrderCommand {
            player: player_id(0),
            units: unit_list(object_ids([8, 99, 7])),
            order: Order::Retreat(crate::orders::Interact::default()),
        })],
    );
    let mut streams = Streams::default();
    streams.seed_all(29910);
    let ai = streams.get(Stream::Ai);
    let (eight, seven) = (ai.rand15() % 1500 + 2250, ai.rand15() % 1500 + 2250);
    let next = 750 + ai.rand15() % 500;
    let timeout = |t: &World, i: ListIndex| {
        t.players[0].objects[i]
            .ai
            .as_ref()
            .unwrap()
            .adjusted_idle_timeout
    };
    assert_eq!(
        (
            timeout(&t, ListIndex::new(1)),
            timeout(&t, ListIndex::new(0))
        ),
        (Millis::new(eight), Millis::new(seven))
    );
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .ai
            .as_ref()
            .unwrap()
            .retarget_timer,
        Millis::new(next)
    );
    assert!(t.players[0].unsupported.is_some());
}

#[test]
fn an_unmodelled_action_stops_its_player_unless_the_object_dies_first() {
    let task = GenieError::unsupported("a default task the slice does not model");
    let with_task = |life| Unit {
        id: object_id(5),
        life,
        unmodelled_action: Some(task.clone()),
        ..Unit::default()
    };
    let run = |unit| {
        let mut t = World::new(
            WorldTime::ZERO,
            1,
            PlayerId::new(0),
            vec![Millis::ZERO],
            vec![player(vec![])],
        );
        t.players[0].objects.push(unit);
        stock_masters(&mut t);
        t.tick(Millis::new(10));
        t.players[0].unsupported.clone()
    };
    assert_eq!(
        run(with_task(Lifecycle::default())),
        Some((1, ListIndex::new(0), task.clone()))
    );
    let dying = Lifecycle {
        class: Class::Animated,
        hp: HitPoints::ZERO,
        master: Some(MasterData::default()),
        ..Lifecycle::default()
    };
    assert_eq!(run(with_task(dying)), None);
}

/// Build 158041's farm (master 50): its default task 0 is a Make of the farm food kind 16
/// at `work_val1` 1.0, its work rate 0.4 a second, its capacity 15.
fn farm_master() -> Master {
    Master {
        id: ids::FARM,
        terrain_restriction: Some(RestrictionId::named(0)),
        kind: BUILDING,
        class: class::FARM,
        work_rate: WorkRate::new(0.4),
        capacity: 15,
        default_task: Some(0),
        tasks: vec![crate::objects::masters::Task {
            action_type: crate::gathering::make::MAKE,
            attribute_type: Some(attribute::FARM_FOOD),
            attribute_type2: Some(attribute::FOOD),
            work_val1: 1.0,
            ..crate::objects::masters::Task::default()
        }],
        ..Master::default()
    }
}

/// Livestock whose action list is empty in state 2 (out of a building, its Artifact gone)
/// starts its default task's Artifact again (slot 0x7f0), Gaia's waiting half as long; its first
/// update comes in the next tick, as a Fly's.
#[test]
fn livestock_without_actions_starts_its_artifact_again() {
    let sheep = Unit {
        id: object_id(40),
        life: Lifecycle {
            master_id: master_id(594),
            state: ObjectState::Alive,
            hp: HitPoints::new(7.0),
            ..Lifecycle::default()
        },
        default_artifact: true,
        ..Unit::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        1,
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player::default(),
            Player {
                objects: vec![sheep].into(),
                attributes: vec![Amount::ZERO; 100],
                ..Player::default()
            },
        ],
    );
    t.masters = vec![
        Masters::default(),
        Masters::new([Master {
            id: master_id(594),
            kind: master_type::COMBAT,
            class: class::LIVESTOCK,
            ..Master::default()
        }]),
    ];
    t.tick(Millis::new(50));
    let a = t.players[1].objects[ListIndex::new(0)]
        .actions
        .artifact()
        .copied()
        .expect("the Artifact again");
    assert_eq!(a, Artifact::new(false, None));
    // its first update checks for a new owner, finds none and waits a second
    t.tick(Millis::new(50));
    let a = t.players[1].objects[ListIndex::new(0)]
        .actions
        .artifact()
        .copied()
        .unwrap();
    assert_eq!((a.state, a.timer), (ArtifactState::Wait, Seconds::new(1.0)));
}

/// [`forage_world`]'s villager 9175 (master 120) with `set` run on its master and on itself,
/// a master 99 for the remains, then `part` on it in an update: what the part left in the
/// Step, the attribute changes and the remains.
fn life_part(
    set: impl FnOnce(&mut Master, &mut Unit),
    part: impl FnOnce(&mut Unit, &mut step::Step<'_>) -> Result<(), GenieError>,
) -> Result<LifeLeft, GenieError> {
    let mut t = forage_world();
    let mut master = t.masters[1].get(master_id(120)).unwrap().clone();
    set(&mut master, &mut t.players[1].objects[ListIndex::new(0)]);
    // the remains and death spawn the tests name (99, a master the game does not have)
    let left = Master {
        id: master_id(99),
        kind: master_type::ANIMATED,
        ..Master::default()
    };
    let ms = &mut t.masters[1];
    *ms = Masters::new(
        ms.iter()
            .filter(|m| m.id != master_id(120))
            .cloned()
            .chain([master, left]),
    );
    in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, c| {
            let before = attribute_values(c.w, 1);
            let ids: Vec<ObjectId> = c.w.players[1].objects.iter().map(|u| u.id).collect();
            part(unit, c)?;
            let own = c.w.players[1].objects.iter();
            let made = own
                .filter(|u| !ids.contains(&u.id))
                .map(|u| (u.life.master_id.get(), u.position()));
            Ok((gained(c.w, 1, &before), made.collect()))
        },
    )
}

/// Player `p`'s food, wood, stone and gold, then its attributes from 4 on: every attribute's
/// value in attribute order.
fn attribute_values(t: &World, p: usize) -> Vec<f32> {
    let owner = &t.players[p];
    owner
        .resources()
        .iter()
        .chain(owner.attributes.iter().skip(4))
        .copied()
        .map(Amount::get)
        .collect()
}

/// What player `p`'s attributes changed by since `before` ([`attribute_values`]): each changed
/// attribute and its change, in attribute order.
fn gained(t: &World, p: usize, before: &[f32]) -> Vec<(i16, f32)> {
    let now = attribute_values(t, p);
    let changed = now
        .iter()
        .zip(before)
        .enumerate()
        .filter(|(_, (a, b))| a != b);
    changed.map(|(i, (a, b))| (i as i16, a - b)).collect()
}

/// What [`life_part`] gives within the update: the owner's attribute changes and the objects
/// it made (master, place).
type LifeLeft = (Vec<(i16, f32)>, Vec<(i16, Position)>);

/// The life cycle's `set_state(3)` storage (`FUN_1411a56c0` with 0): a live object below one
/// hit point gives back each storage entry of kind 2 or 16 with an attribute (the amount
/// negated); one at a hit point, or at a NaN one, gives back nothing.
#[test]
fn a_dying_object_gives_back_its_storage_of_kinds_2_and_16() {
    let storage = |hp: HitPoints| {
        life_part(
            |m, v| {
                m.storage = vec![
                    (Some(attribute::HEADROOM), Amount::new(1.0), 2),
                    (Some(attribute::POPULATION), Amount::new(5.0), 16),
                    (Some(AttributeId::numbered(9)), Amount::new(1.0), 1),
                    (None, Amount::new(1.0), 2),
                ];
                (v.life.state, v.life.hp) = (ObjectState::Alive, hp);
            },
            |u, c| u.dying_storage_leaves(c),
        )
        .unwrap()
        .0
    };
    assert_eq!(storage(HitPoints::new(0.5)), [(4, -1.0), (11, -5.0)]);
    assert_eq!(storage(HitPoints::new(1.0)), []);
    assert_eq!(storage(HitPoints::new(f32::NAN)), []);
}

/// The dying sprite's end (`RGE_Animated_Object::update`, [`Unit::dying_sprite_ended`]): the
/// animation stops and a unit (type below 80) leaves its remains (here 99) at its place within
/// its update (the dead unit lent to the world); without remains (−1) or as a building none. A
/// dead object's death spawn ([`Unit::death_spawn_comes`]) comes the same way. A master the
/// player lacks fails.
#[test]
fn a_dead_object_leaves_its_remains_and_its_death_spawn() {
    let ended = |kind: MasterKind, remains: Option<MasterId>| {
        life_part(
            |m, v| {
                m.kind = kind;
                v.life.remains = remains;
            },
            Unit::dying_sprite_ended,
        )
    };
    let v = forage_world().players[1].objects[ListIndex::new(0)].clone();
    assert_eq!(
        ended(master_type::COMBAT, Some(master_id(99))).unwrap().1,
        [(99, v.position())]
    );
    assert_eq!(ended(master_type::COMBAT, None).unwrap().1, []);
    assert_eq!(ended(BUILDING, Some(master_id(99))).unwrap().1, []);
    let spawned = life_part(
        |m, _| m.death_spawns = [Some(master_id(99)), None],
        Unit::death_spawn_comes,
    );
    assert_eq!(spawned.unwrap().1, [(99, v.position())]);
    let lacking = life_part(
        |_, v| v.life.master_id = master_id(999),
        Unit::dying_sprite_ended,
    );
    assert_eq!(
        lacking.unwrap_err().reason(),
        "an object of a master the slice does not have"
    );
}

/// The Artifact's handover to Gaia (state 100, [`Unit::artifact_handover`]): player 1's sheep
/// stops and changes hands to Gaia (slot 0x70), and its Artifact checks again (state 6). The
/// world's first update runs no Artifact (`0x140aad250` from the second on).
#[test]
fn an_artifact_in_state_100_hands_its_sheep_to_gaia() {
    let master = Master {
        id: master_id(594),
        kind: master_type::COMBAT,
        class: class::LIVESTOCK,
        ..Master::default()
    };
    let sheep = Unit {
        id: object_id(40),
        life: Lifecycle {
            master_id: master_id(594),
            state: ObjectState::Alive,
            hp: HitPoints::new(7.0),
            ..Lifecycle::default()
        },
        actions: ActionList::default().with_artifact(Some(Artifact {
            state: ArtifactState::ToGaia,
            ..Artifact::new(false, None)
        })),
        ..Unit::default()
    };
    let player = |objects| Player {
        objects,
        attributes: vec![Amount::ZERO; 250],
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        1,
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![player(vec![].into()), player(vec![sheep].into())],
    );
    t.masters = vec![Masters::new([master.clone()]), Masters::new([master])];
    placeable(&mut t);
    let placed = Placed {
        position: Position::new(10.5, 10.5, 0.0),
        radius: Footprint::new(0.2, 0.2),
        class: class::LIVESTOCK,
        player: PlayerId::new(1),
        master: master_id(594),
        obstruction_type: ObstructionType::NONE,
        buildable_on: 0,
        tall: true,
        speed: Speed::ZERO,
        unknown: None,
        obstruction: None,
    };
    t.spatial.insert(object_id(40), placed);
    t.tick(Millis::new(50));
    assert_eq!(t.players[1].objects.len(), 1);
    t.tick(Millis::new(50));
    assert_eq!(
        (
            t.players[0].unsupported.clone(),
            t.players[1].unsupported.clone()
        ),
        (None, None)
    );
    assert_eq!(t.players[1].objects.len(), 0);
    let sheep = &t.players[0].objects[ListIndex::new(0)];
    assert_eq!(sheep.id, object_id(40));
    assert_eq!(
        sheep.actions.artifact().map(|a| a.state),
        Some(ArtifactState::Check)
    );
}

#[test]
fn a_farm_starts_its_make_in_its_first_update_and_fills_from_the_next() {
    let farm = Unit {
        id: object_id(40),
        life: Lifecycle {
            master_id: ids::FARM,
            attribute_type: Some(attribute::FOOD),
            amount: Amount::new(15.0),
            ..Lifecycle::default()
        },
        default_make: true,
        ..Unit::default()
    };
    let mut attributes = vec![Amount::ZERO; 100];
    attributes[attribute::FARM_FOOD_AMOUNT.index()] = Amount::new(175.0);
    let mut t = World::new(
        WorldTime::ZERO,
        1,
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![Player {
            objects: vec![farm].into(),
            attributes,
            ..Player::default()
        }],
    );
    t.masters = vec![Masters::new([farm_master()])];
    t.tick(Millis::new(104));
    let f = &t.players[0].objects[ListIndex::new(0)];
    let make = f.actions.make().copied().unwrap();
    assert_eq!(
        (make.state, make.remaining, make.total),
        (MakeState::Making, Amount::new(175.0), Amount::new(175.0))
    );
    assert_eq!(f.life.held(), carrying(Some(attribute::FARM_FOOD), 0.0));
    assert_eq!(
        f.top_action(),
        Some(TopAction {
            kind: crate::gathering::make::MAKE,
            state: 6
        })
    );
    t.tick(Millis::new(104));
    let f = &t.players[0].objects[ListIndex::new(0)];
    let made = 0.4f32 * 1.0 * Millis::new(104).seconds();
    assert_eq!(
        (f.life.amount, f.actions.make().copied().unwrap().remaining),
        (Amount::new(made), Amount::new(175.0 - made))
    );
    assert!(t.players[0].unsupported.is_none());
}

#[test]
fn a_built_farm_s_master_starts_a_make_and_another_default_task_is_reported() {
    assert_eq!(farm_master().default_start(), (true, None));
    let mut other = farm_master();
    other.tasks[0].action_type = crate::movement::fly::FLY;
    assert!(other.default_start().1.is_some());
    // a type the factories make nothing of starts nothing (a wild animal's Explore, 6)
    other.tasks[0].action_type = ActionType::named(6);
    assert_eq!(other.default_start(), (false, None));
    other.default_task = None;
    assert_eq!(other.default_start(), (false, None));
}

/// `TRIBE_Building_Object::update`'s whole seconds (`0x1411c95a2`): a built building outside
/// the walls' branch (a barracks) has its `damaged_lately_timer` (`+0x30d`) lose one in an
/// update that passes whole seconds, however many, and stay at 0.
#[test]
fn a_building_s_damaged_lately_timer_loses_one_an_update_with_whole_seconds() {
    let barracks = Unit {
        id: object_id(40),
        life: Lifecycle {
            master_id: master_id(12),
            hp: HitPoints::new(550.0),
            state: ObjectState::Alive,
            ..Lifecycle::default()
        },
        damaged_lately: 2,
        ..Unit::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        1,
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![Player {
            objects: vec![barracks].into(),
            attributes: vec![Amount::ZERO; 300],
            ..Player::default()
        }],
    );
    t.masters = vec![Masters::new([Master {
        id: master_id(12),
        kind: BUILDING,
        class: class::BUILDING,
        hit_points: 550,
        building: Some(crate::objects::masters::BuildingMaster::default()),
        ..Master::default()
    }])];
    let timer = |t: &World| t.players[0].objects[ListIndex::new(0)].damaged_lately;
    t.tick(Millis::new(500));
    assert_eq!(timer(&t), 2);
    t.tick(Millis::new(2600));
    assert_eq!(timer(&t), 1);
    t.tick(Millis::new(1000));
    assert_eq!(timer(&t), 0);
    t.tick(Millis::new(1000));
    assert_eq!(timer(&t), 0);
    assert!(t.players[0].unsupported.is_none());
}

fn town_centre(id: i32) -> Unit {
    Unit {
        id: object_id(id),
        ..Unit::default()
    }
}

fn queue(building: i32, unit: i16, amount: i16) -> Command {
    Command::Queue(QueueCommand {
        player: player_id(0),
        buildings: unit_list(vec![Some(object_id(building))]),
        building_master: master_id(109),
        unit: master_id(unit),
        amount,
        button: -1,
    })
}

/// A town centre of master 109 ([`one_player`]'s).
fn centre(id: i32) -> Unit {
    let mut tc = town_centre(id);
    tc.life.master_id = ids::TOWN_CENTRE;
    tc
}

#[test]
fn a_unit_inside_heals_up_to_its_hit_points() {
    // records/425906646's update 2191: villager 27260 inside TC 27150 heals by the TC's
    // garrison heal rate once a second
    let master = Master {
        id: master_id(592),
        class: class::VILLAGER,
        hit_points: 25,
        ..Master::default()
    };
    let mut v = Unit {
        id: object_id(27260),
        life: Lifecycle {
            hp: HitPoints::new(11.0),
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    v.heal(master.class, master.hit_points, HitPoints::new(0.1))
        .unwrap();
    assert_eq!(v.life.hp, HitPoints::new(11.1));
    // a heal past the hit points stops at them, the powerup's room taking the rest
    v.life.hp = HitPoints::new(24.95);
    v.hp_powerup = HpPowerup {
        taken: HitPoints::new(1.0),
        max: HitPoints::new(2.0),
    };
    v.heal(master.class, master.hit_points, HitPoints::new(0.1))
        .unwrap();
    assert_eq!(
        (v.life.hp, v.hp_powerup.taken, v.hp_powerup.max),
        (
            HitPoints::new(24.95f32 + 0.1),
            HitPoints::new(1.0),
            HitPoints::new(2.0)
        )
    );
    // past them with the room: the overflow comes off the powerup, then a full unit holds
    // exactly its master's hit points
    v.life.hp = HitPoints::new(26.95);
    v.heal(master.class, master.hit_points, HitPoints::new(0.1))
        .unwrap();
    assert_eq!(v.life.hp, HitPoints::new(25.0));
    assert_eq!(
        v.hp_powerup.taken,
        HitPoints::new(1.0 - (26.95f32 + 0.1 - 27.0))
    );
    // a paid class heals at a cost: not modelled
    let ship = Master {
        class: class::WARSHIP,
        ..master
    };
    assert!(
        v.heal(ship.class, ship.hit_points, HitPoints::new(0.1))
            .is_err()
    );
}

/// `FUN_140e6cfc0` resets every listed unit the world holds, a sleeping one too, and passes
/// over an id it does not hold.
#[test]
fn an_order_resets_its_units_sleeping_ones_too() {
    let unit = |id: i32| Unit {
        id: object_id(id),
        ai: Some(UnitAi {
            patrol_path: Some(crate::unit_ai::PatrolPath::default()),
            patrol_waypoint: 2,
            adjusted_idle_timeout: Millis::new(100),
            secondary_timer: Millis::new(900),
            ..UnitAi::default()
        }),
        ..Unit::default()
    };
    let player = Player {
        objects: vec![unit(1)].into(),
        sleeping: vec![unit(2)].into(),
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        0,
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![Player::default(), player],
    );
    t.reset_order_units(&object_ids([2, 9, 1])).unwrap();
    for u in t.players[1].objects.iter().chain(&t.players[1].sleeping) {
        let ai = u.ai.as_ref().unwrap();
        assert_eq!(
            (ai.patrol_path.is_none(), ai.secondary_timer),
            (true, Millis::new(750))
        );
    }
}

#[test]
fn a_load_goes_to_its_player_alone_unless_the_player_shares() {
    // records/425906646's update 818: player 6, with allies, does not share (+0x1426 0)
    let relations = vec![
        relation_row([0, 3, 3]),
        relation_row([3, 0, 0]),
        relation_row([3, 0, 0]),
    ];
    let mut adds = Vec::new();
    give_load(
        (&relations, false),
        PlayerId::new(1),
        &mut adds,
        attribute::FOOD,
        Amount::new(10.0),
    )
    .unwrap();
    assert_eq!(
        adds,
        crate::test_ids::adds([(attribute::FOOD, 10.0), (attribute::GATHERED, 10.0)])
    );
    assert!(
        give_load(
            (&relations, true),
            PlayerId::new(1),
            &mut adds,
            attribute::GOLD,
            Amount::new(10.0)
        )
        .is_err()
    );
}

/// A gate's slot 0x678 (`FUN_1411d4a30`): a standing, unlocked object of class 39 whose
/// master transforms (`+0x9ba` not -1), between allies; any other object answers false.
#[test]
fn only_a_standing_unlocked_transforming_gate_lets_an_ally_pass() {
    let object = |id: i32, master: MasterId, state: ObjectState, locked: bool| Unit {
        id: object_id(id),
        life: Lifecycle {
            master_id: master,
            state,
            ..Lifecycle::default()
        },
        gate: Some(Gate {
            locked,
            ..Gate::default()
        }),
        ..Unit::default()
    };
    let player = |objects: Vec<Unit>| Player {
        objects: objects.into(),
        relations: relation_row([0, 0]),
        attributes: vec![Amount::ZERO; 250],
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        1,
        PlayerId::new(0),
        vec![Millis::ZERO; 2],
        vec![
            player(Vec::new()),
            player(vec![
                object(1, master_id(64), ObjectState::Alive, false),
                object(2, master_id(65), ObjectState::Alive, false),
                object(3, master_id(66), ObjectState::Alive, false),
                object(4, master_id(64), ObjectState::Dying, false),
                object(5, master_id(64), ObjectState::Alive, true),
            ]),
        ],
    );
    let master = |id: i16, class: ClassId, transform_unit: i16| Master {
        id: master_id(id),
        class,
        building: Some(crate::objects::masters::BuildingMaster {
            transform_unit: opt_master(transform_unit),
            ..crate::objects::masters::BuildingMaster::default()
        }),
        ..Master::default()
    };
    let masters = Masters::new([
        master(64, class::GATE, 63),
        master(65, class::GATE, -1),
        master(66, class::VILLAGER, 63),
    ]);
    t.masters = vec![masters.clone(), masters];
    let passes: Vec<bool> = (0..5)
        .map(|i| {
            t.gate_passes(PlayerId::new(1), ListIndex::new(i), PlayerId::new(1))
                .unwrap()
        })
        .collect();
    assert_eq!(passes, [true, false, false, false, false]);
}

#[test]
fn a_gate_opens_for_its_allies_while_they_update() {
    // records/425906646's update 636: player 2's gate 27687 (master 64, class 39 with a
    // transform unit) has no record, marks or shape while its team updates, so 27274's plan
    // passes it; the next player, an enemy, finds it closed again
    let gate = Unit {
        id: object_id(27687),
        life: Lifecycle {
            master_id: master_id(64),
            state: ObjectState::Alive,
            ..Lifecycle::default()
        },
        gate: Some(Gate::default()),
        ..Unit::default()
    };
    let player = |objects: Vec<Unit>, relations: Vec<u8>| Player {
        objects: objects.into(),
        relations: relation_row(relations),
        attributes: vec![Amount::ZERO; 250],
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        1,
        PlayerId::new(0),
        vec![Millis::ZERO; 4],
        vec![
            player(Vec::new(), vec![0, 3, 3, 3]),
            player(Vec::new(), vec![3, 0, 0, 3]),
            player(vec![gate], vec![3, 0, 0, 3]),
            player(Vec::new(), vec![3, 3, 3, 0]),
        ],
    );
    let master = Master {
        id: master_id(64),
        class: class::GATE,
        obstruction_type: ObstructionType::TYPE_2,
        obstruction_class: ObstructionClass::GATE,
        building: Some(crate::objects::masters::BuildingMaster {
            transform_unit: Some(master_id(63)),
            ..crate::objects::masters::BuildingMaster::default()
        }),
        ..Master::default()
    };
    t.masters = (0..4).map(|_| Masters::new([master.clone()])).collect();
    t.spatial = Spatial::with_tiles(
        GridSize::new(20, 20),
        crate::map::spatial::Tiles::new(GridSize::new(20, 20), vec![1; 400]),
    );
    t.spatial.relations = t.players.iter().map(|p| p.relations.clone()).collect();
    let record = crate::map::spatial::Obstruction {
        kind: ObstructionType::TYPE_2,
        class: ObstructionClass::GATE,
    };
    t.spatial.insert(
        object_id(27687),
        Placed {
            position: Position::new(10.0, 10.5, 0.0),
            radius: Footprint::new(1.0, 0.5),
            class: class::GATE,
            player: PlayerId::new(2),
            master: master_id(64),
            obstruction_type: ObstructionType::TYPE_2,
            buildable_on: 0,
            tall: true,
            speed: Speed::ZERO,
            unknown: None,
            obstruction: Some(record),
        },
    );
    let blocked = |t: &World| {
        t.spatial
            .tiles()
            .word(Tile::new(9, 10))
            .is_some_and(|w| w & crate::map::spatial::BLOCKED != 0)
    };
    assert!(blocked(&t));
    let mut open = vec![false; 4];
    // player 1, an ally, opens both its own and player 2's gates
    t.open_gates(PlayerId::new(1), &mut open).unwrap();
    assert_eq!(open, [false, true, true, false]);
    assert!(
        t.spatial
            .get(object_id(27687))
            .unwrap()
            .obstruction
            .is_none()
    );
    assert!(!blocked(&t));
    // player 2 is next and an ally of both: they stay open
    t.close_gates(PlayerId::new(1), &mut open).unwrap();
    assert_eq!(open, [false, true, true, false]);
    t.open_gates(PlayerId::new(2), &mut open).unwrap();
    // player 3, next, is an enemy: they close
    t.close_gates(PlayerId::new(2), &mut open).unwrap();
    assert_eq!(open, [false; 4]);
    assert_eq!(
        t.spatial.get(object_id(27687)).unwrap().obstruction,
        Some(record)
    );
    assert!(blocked(&t));
    t.open_gates(PlayerId::new(3), &mut open).unwrap();
    assert_eq!(open, [false, false, false, true]);
    assert!(blocked(&t));
    // a locked gate stays closed for its allies
    t.close_gates(PlayerId::new(3), &mut open).unwrap();
    t.players[2].objects[ListIndex::new(0)].gate = Some(Gate {
        locked: true,
        ..Gate::default()
    });
    t.open_gates(PlayerId::new(1), &mut open).unwrap();
    assert!(blocked(&t));
}

/// records/425906646's gate 27745 of player 3 (annexes 27746 and 27747) in update 2865: an
/// ally walking between the posts opens it; a standing Gaia animal does not count, an enemy
/// counts but is no ally, a wall next to it neither.
#[test]
fn a_gate_counts_the_moving_units_between_its_posts() {
    let mut s = Spatial::with_tiles(
        GridSize::new(40, 40),
        crate::map::spatial::Tiles::new(GridSize::new(40, 40), vec![1; 1600]),
    );
    s.relations = vec![
        relation_row([0, 3, 3]),
        relation_row([3, 0, 3]),
        relation_row([3, 3, 0]),
    ];
    let placed = |x: f32, class, player, speed, kind| Placed {
        position: Position::new(x, 20.0, 0.0),
        radius: Footprint::new(0.2, 0.2),
        class,
        player,
        master: ids::ID_0,
        obstruction_type: kind,
        buildable_on: 0,
        tall: true,
        speed,
        unknown: None,
        obstruction: Some(crate::map::spatial::Obstruction {
            kind,
            class: ObstructionClass::UNIT,
        }),
    };
    s.insert(
        object_id(27745),
        placed(
            20.0,
            class::GATE,
            PlayerId::new(1),
            Speed::ZERO,
            ObstructionType::TYPE_2,
        ),
    );
    s.insert(
        object_id(27746),
        placed(
            18.5,
            class::GATE,
            PlayerId::new(1),
            Speed::ZERO,
            ObstructionType::TYPE_2,
        ),
    );
    s.insert(
        object_id(27747),
        placed(
            21.5,
            class::GATE,
            PlayerId::new(1),
            Speed::ZERO,
            ObstructionType::TYPE_2,
        ),
    );
    s.insert(
        object_id(1),
        placed(
            19.6,
            class::VILLAGER,
            PlayerId::new(1),
            Speed::new(0.8),
            ObstructionType::CIRCLE,
        ),
    );
    let (units, allied) = gate_units(
        &mut s,
        object_id(27745),
        &[object_id(27746), object_id(27747)],
    )
    .unwrap();
    assert_eq!((units, allied), (vec![object_id(1)], 1));
    s.insert(
        object_id(2),
        placed(
            20.4,
            class::PREDATOR,
            PlayerId::new(0),
            Speed::ZERO,
            ObstructionType::CIRCLE,
        ),
    );
    s.insert(
        object_id(3),
        placed(
            20.8,
            class::CLASS_6,
            PlayerId::new(2),
            Speed::new(0.9),
            ObstructionType::CIRCLE,
        ),
    );
    s.insert(
        object_id(4),
        placed(
            21.0,
            class::WALL,
            PlayerId::new(1),
            Speed::ZERO,
            ObstructionType::TYPE_2,
        ),
    );
    let (units, allied) = gate_units(
        &mut s,
        object_id(27745),
        &[object_id(27746), object_id(27747)],
    )
    .unwrap();
    assert_eq!((units, allied), (vec![object_id(1), object_id(3)], 1));
    assert!(gate_units(&mut s, object_id(27745), &[object_id(27746)]).is_err());
}

/// records/425906646's gate 27745 of player 1 (annexes 27746 and 27747, master 88: open, no
/// height, its `transform_unit` the shut 91 of height 2; 92 transforms into nothing) at (20, 20)
/// with the moving soldier 1 (player 1's object 3) between its posts, at world time 400.
fn gate_world() -> World {
    let gate_master = |id: i16, transform_unit: i16, height| Master {
        id: master_id(id),
        kind: BUILDING,
        class: class::GATE,
        height,
        hit_points: 1650,
        // a gate moves (its master's slot 0xe8): its last move time is the mover's
        moving: Some(crate::gathering::hunt::tests::walker().stats),
        building: Some(crate::objects::masters::BuildingMaster {
            transform_unit: opt_master(transform_unit),
            stack_unit: None,
            head_unit: None,
            foundation_terrain: -1,
            tech: None,
            annexes: [None; 4],
            ..crate::objects::masters::BuildingMaster::default()
        }),
        ..Master::default()
    };
    let soldier = Master {
        id: master_id(4),
        kind: master_type::COMBAT,
        class: class::CLASS_6,
        ..Master::default()
    };
    let unit = |id, master, x: f32, annex_of| Unit {
        id,
        body: crate::objects::layers::Body::at(Position::new(x, 20.0, 0.0)),
        annex_of,
        life: Lifecycle {
            master_id: master,
            state: ObjectState::Alive,
            hp: HitPoints::new(10.0),
            ..Lifecycle::default()
        },
        pasture: None,
        my_pasture: None,
        ..Unit::default()
    };
    let mut gate = unit(object_id(27745), master_id(88), 20.0, None);
    gate.gate = Some(Gate::default());
    gate.body
        .put_mover(Some(crate::gathering::hunt::tests::walker()));
    let mut t = World::new(
        WorldTime::ZERO,
        1,
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player::default(),
            Player {
                objects: vec![
                    gate,
                    unit(
                        object_id(27746),
                        master_id(88),
                        18.5,
                        Some(object_id(27745)),
                    ),
                    unit(
                        object_id(27747),
                        master_id(88),
                        21.5,
                        Some(object_id(27745)),
                    ),
                    unit(object_id(1), master_id(4), 19.6, None),
                ]
                .into(),
                ..Player::default()
            },
        ],
    );
    t.world_time = WorldTime::from_millis(400);
    t.masters = vec![
        Masters::default(),
        Masters::new([
            gate_master(88, 91, 0.0),
            gate_master(91, 88, 2.0),
            gate_master(92, -1, 0.0),
            soldier,
        ]),
    ];
    t.spatial = Spatial::with_tiles(
        GridSize::new(40, 40),
        crate::map::spatial::Tiles::new(GridSize::new(40, 40), vec![1; 1600]),
    );
    t.spatial.relations = vec![relation_row([0, 3]), relation_row([3, 0])];
    let placed = |x: f32, class, speed, kind| Placed {
        position: Position::new(x, 20.0, 0.0),
        radius: Footprint::new(0.2, 0.2),
        class,
        player: PlayerId::new(1),
        master: ids::ID_0,
        obstruction_type: kind,
        buildable_on: 0,
        tall: true,
        speed,
        unknown: None,
        obstruction: Some(crate::map::spatial::Obstruction {
            kind,
            class: ObstructionClass::UNIT,
        }),
    };
    t.spatial.insert(
        object_id(27745),
        placed(20.0, class::GATE, Speed::ZERO, ObstructionType::TYPE_2),
    );
    t.spatial.insert(
        object_id(27746),
        placed(18.5, class::GATE, Speed::ZERO, ObstructionType::TYPE_2),
    );
    t.spatial.insert(
        object_id(27747),
        placed(21.5, class::GATE, Speed::ZERO, ObstructionType::TYPE_2),
    );
    t.spatial.insert(
        object_id(1),
        placed(
            19.6,
            class::CLASS_6,
            Speed::new(0.8),
            ObstructionType::CIRCLE,
        ),
    );
    t
}

/// The player's gate toggle (`FUN_14162f8c0`) on records/425906646's gate 27745 (annexes 27746
/// and 27747): the open gate (master 88) becomes its `transform_unit`, the shut 91 (a height),
/// which kills the moving soldier between its posts and takes the world time plus 1 as its last
/// move time; the shut one opens again (no height) without either. A gate without a
/// `transform_unit` stays; one with masters of its own is not modelled.
#[test]
fn a_gate_toggle_shuts_on_the_units_between_its_posts_or_opens() {
    let mut t = gate_world();
    let state = |t: &World| {
        let g = &t.players[1].objects[ListIndex::new(0)];
        (
            g.life.master_id.get(),
            g.body.mover().map(|m| m.last_move),
            t.players[1].objects[ListIndex::new(3)].life.hp,
        )
    };
    t.toggle_gate(PlayerId::new(1), ListIndex::new(0)).unwrap();
    assert_eq!(
        state(&t),
        (91, Some(WorldTime::from_millis(401)), HitPoints::ZERO)
    );
    t.players[1].objects[ListIndex::new(3)].life.hp = HitPoints::new(10.0);
    t.world_time = WorldTime::from_millis(500);
    t.toggle_gate(PlayerId::new(1), ListIndex::new(0)).unwrap();
    assert_eq!(
        state(&t),
        (88, Some(WorldTime::from_millis(401)), HitPoints::new(10.0))
    );
    t.players[1].objects[ListIndex::new(0)].life.master_id = master_id(92);
    t.toggle_gate(PlayerId::new(1), ListIndex::new(0)).unwrap();
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)].life.master_id,
        master_id(92)
    );
    t.players[1].objects[ListIndex::new(0)].gate = Some(Gate {
        other_masters: true,
        ..Gate::default()
    });
    assert!(t.toggle_gate(PlayerId::new(1), ListIndex::new(0)).is_err());
}

/// [`Unit::gate_update`] of [`gate_world`]'s gate as master `master` with its gate state `gate`:
/// the gate's master within the update after it (its toggle's, [`World::toggle_gate`]), its
/// state after, or the error.
fn gate_update_of(
    master: MasterId,
    gate: Option<Gate>,
    setup: impl FnOnce(&mut World),
) -> Result<(MasterId, Option<Gate>), GenieError> {
    let mut t = gate_world();
    let g = &mut t.players[1].objects[ListIndex::new(0)];
    (g.life.master_id, g.gate) = (master, gate);
    setup(&mut t);
    let master = in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, c| {
            unit.gate_update(c)?;
            Ok(unit.life.master_id)
        },
    )?;
    Ok((master, t.players[1].objects[ListIndex::new(0)].gate))
}

/// The gate's update (`FUN_1411e7730`): a shut (height 2, master 91), unlocked gate with an ally
/// moving between its posts opens within its update (its player's toggle: master 88), its close
/// timer 0; a locked one stays shut. An open gate (no height) with nothing in it starts its close
/// timer at 0.5 s, runs it down by the world's dt (0.05 s), and at 0 shuts within its update.
/// Its first update clears every time it runs; a gate whose master transforms into nothing is
/// passed over, and one without its gate state is not modelled.
#[test]
fn a_gate_opens_for_an_ally_inside_and_shuts_after_its_close_timer() {
    let gate = |locked, close_timer| {
        Some(Gate {
            locked,
            close_timer,
            first_update: true,
            ..Gate::default()
        })
    };
    let ran = |locked, close_timer| {
        Some(Gate {
            first_update: false,
            ..gate(locked, close_timer).unwrap()
        })
    };
    assert_eq!(
        gate_update_of(master_id(91), gate(false, Seconds::new(0.3)), |_| ()),
        Ok((master_id(88), ran(false, Seconds::ZERO)))
    );
    assert_eq!(
        gate_update_of(master_id(91), gate(true, Seconds::new(0.3)), |_| ()),
        Ok((master_id(91), ran(true, Seconds::new(0.3))))
    );
    // the soldier stands aside: nothing in the open gate
    let aside = |t: &mut World| {
        let p = *t.spatial.get(object_id(1)).unwrap();
        t.spatial.insert(
            object_id(1),
            Placed {
                position: Position::new(20.0, 30.0, 0.0),
                ..p
            },
        );
    };
    assert_eq!(
        gate_update_of(master_id(88), gate(false, Seconds::ZERO), aside),
        Ok((master_id(88), ran(false, GATE_CLOSE_DELAY)))
    );
    assert_eq!(
        gate_update_of(master_id(88), gate(false, Seconds::new(0.5)), aside),
        Ok((master_id(88), ran(false, Seconds::new(0.5 - 0.05))))
    );
    assert_eq!(
        gate_update_of(master_id(88), gate(false, Seconds::new(0.05)), aside),
        Ok((master_id(91), ran(false, Seconds::ZERO)))
    );
    // the soldier in the open gate holds its timer
    assert_eq!(
        gate_update_of(master_id(88), gate(false, Seconds::new(0.5)), |_| ()),
        Ok((master_id(88), ran(false, Seconds::new(0.5))))
    );
    assert_eq!(
        gate_update_of(master_id(92), gate(false, Seconds::new(0.5)), |_| ()),
        Ok((master_id(92), gate(false, Seconds::new(0.5))))
    );
    let err = gate_update_of(master_id(91), None, |_| ()).unwrap_err();
    assert_eq!(err.reason(), "a gate without its gate state");
}

#[test]
fn the_forced_replay_takes_the_game_s_state_for_what_froze() {
    // a frozen unit takes the game's (and runs again), a frozen unit the game no longer
    // holds goes, a unit that ran keeps the slice's, a stopped player takes its resources
    let mut t = one_player(vec![live(7), live(8), live(9)], [0.0; 4]);
    t.players[0].objects[ListIndex::new(0)].stopped = true;
    t.players[0].objects[ListIndex::new(2)].stopped = true;
    t.players[0].stopped = false;
    let game_unit = |id, timer| Unit {
        id,
        ai: Some(UnitAi {
            retarget_timer: timer,
            ..building()
        }),
        ..Unit::default()
    };
    let game = GameState {
        frame_seed: 77,
        players: vec![vec![
            game_unit(object_id(7), Millis::new(900)),
            game_unit(object_id(8), Millis::new(901)),
        ]],
        resources: vec![[1.0, 2.0, 3.0, 4.0]],
        ..GameState::default()
    };
    assert_eq!(t.force(&game), vec![]);
    assert_eq!(t.frame_seed, 77);
    assert_eq!(ids(&t), vec![7, 8]);
    assert_eq!(retarget(&t), vec![900, 40]);
    assert!(t.players[0].objects.iter().all(|u| !u.stopped));
    // a stopped player runs again with the game's resources; what differed is forced too
    t.players[0].stopped = true;
    let game = GameState {
        resync: vec![object_id(8)],
        ..game
    };
    let _ = t.force(&game);
    assert_eq!(
        (
            t.players[0].stopped,
            t.players[0].resources().map(Amount::get)
        ),
        (false, [1.0, 2.0, 3.0, 4.0])
    );
    assert_eq!(retarget(&t), vec![900, 901]);
    // a game object the slice never made is named, not added
    let made = GameState {
        players: vec![vec![
            game_unit(object_id(7), Millis::new(900)),
            game_unit(object_id(8), Millis::new(901)),
            game_unit(object_id(10), Millis::new(5)),
        ]],
        ..GameState::default()
    };
    assert_eq!(
        t.force(&made),
        vec![(
            PlayerId::new(0),
            Some(object_id(10)),
            GenieError::unsupported("an object the game made and the slice did not")
        )]
    );
}

/// The forced replay's removal of a frozen object the game no longer holds is the leave of an
/// object whose update returned 1 ([`World::leave_world`]): its owner's watched list lets it go
/// (`FUN_141091310`) and a recyclable master's missile goes to the pool of type 60.
#[test]
fn the_forced_replay_s_removal_leaves_as_an_update_s_leave_does() {
    let missile = Unit {
        id: object_id(9),
        stopped: true,
        missile: Some(crate::combat::missile::Missile {
            max_range: Distance::ZERO,
            fired_from: None,
            charge: false,
            flying: true,
            step: Offset::ZERO,
        }),
        life: Lifecycle {
            master_id: master_id(54),
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let mut t = one_player(vec![live(7), missile], [0.0; 4]);
    t.masters = vec![Masters::new([Master {
        id: master_id(54),
        kind: master_type::MISSILE,
        recyclable: true,
        ..Master::default()
    }])];
    t.players[0].watched.push(crate::vision::ghost::Watched {
        id: object_id(9),
        cell: crate::vision::ghost::Cell::Tile(0),
        bits: 0,
    });
    let game = GameState {
        players: vec![vec![live(7)]],
        ..GameState::default()
    };
    assert_eq!(t.force(&game), vec![]);
    assert_eq!(ids(&t), vec![7]);
    assert_eq!(t.players[0].watched, Vec::new());
    assert_eq!(t.recycling.missiles, [(object_id(9), true)]);
}

/// A player with the villagers' masters (a population of one each) and room for more.
fn one_player(objects: Vec<Unit>, resources: [f32; 4]) -> World {
    let mut attributes = crate::test_ids::attributes(250, resources);
    (attributes[4], attributes[11], attributes[32]) =
        (Amount::new(5.0), Amount::new(4.0), Amount::new(10.0));
    let mut t = World::new(
        WorldTime::ZERO,
        29910,
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![Player {
            objects: objects.into(),
            attributes,
            ..Player::default()
        }],
    );
    // the town centre (109, work rate 1); the villagers (46 food, a population of one, 25
    // seconds at the centre); master 99 without costs or population, and 97 with a population
    // of one, made in 2 seconds
    let trained = |id, time| Master {
        id,
        kind: master_type::COMBAT,
        available: true,
        trained_at: vec![ids::TOWN_CENTRE],
        train_locations: vec![crate::objects::masters::TrainLocation {
            location: Some(ids::TOWN_CENTRE),
            time,
            button: 1,
            hotkey: 0,
        }],
        ..Master::default()
    };
    let villager = |id| Master {
        class: class::VILLAGER,
        cost: costs(&[(attribute::FOOD, 46, true), (attribute::HEADROOM, 1, false)]),
        storage: vec![
            (Some(attribute::HEADROOM), Amount::new(-1.0), 2),
            (Some(attribute::POPULATION), Amount::new(1.0), 2),
        ],
        ..trained(id, 25)
    };
    let centre = Master {
        id: ids::TOWN_CENTRE,
        kind: BUILDING,
        work_rate: WorkRate::new(1.0),
        ..Master::default()
    };
    let quick = Master {
        storage: vec![
            (Some(attribute::HEADROOM), Amount::new(-1.0), 2),
            (Some(attribute::POPULATION), Amount::new(1.0), 2),
        ],
        ..trained(master_id(97), 2)
    };
    t.masters = vec![Masters::new([
        villager(VILLAGER_MALE),
        villager(VILLAGER_FEMALE),
        trained(master_id(99), 25),
        quick,
        centre,
    ])];
    stock_masters(&mut t);
    t
}

#[test]
fn a_queue_pays_its_cost_and_the_building_starts_making() {
    let mut t = one_player(vec![centre(9)], [200.0; 4]);
    t.tick_with(Millis::new(104), &[queue(9, 83, 1)]);
    let p = &t.players[0];
    assert_eq!(
        p.resources(),
        crate::test_ids::amounts([154.0, 200.0, 200.0, 200.0])
    );
    let production = p.objects[ListIndex::new(0)].production.as_ref().unwrap();
    // the make action works in the tick it is created in
    let Some(crate::production::Making::Unit(make)) = production.action else {
        panic!("a make action");
    };
    assert_eq!(
        (make.progress, make.state, make.master),
        (
            Seconds::new(0.104),
            crate::production::MakeObjState::Making,
            production.queue.slots[0].master
        )
    );
    let q = &production.queue;
    assert_eq!(
        (q.count, q.total, q.slots[0].train_time, q.slots[0].costs[0]),
        (1, 1, 25, (Some(attribute::FOOD), 46.0))
    );
    assert!(p.unsupported.is_none());
}

#[test]
fn a_queued_villager_is_male_or_female_by_an_action_stream_draw() {
    let mut t = one_player(vec![centre(9)], [200.0; 4]);
    t.tick_with(Millis::new(104), &[queue(9, 83, 1)]);
    let mut streams = Streams::default();
    streams.seed_all(29910);
    let r = streams.get(Stream::Action).raw64();
    let expected = if r.wrapping_mul(100) % 100 < 50 {
        Some(VILLAGER_MALE)
    } else {
        Some(master_id(293))
    };
    assert_eq!(
        t.players[0].objects[ListIndex::new(0)]
            .production
            .as_ref()
            .unwrap()
            .queue
            .slots[0]
            .master,
        expected
    );
}

#[test]
fn a_queue_the_player_cannot_pay_or_at_a_missing_building_queues_nothing() {
    // too little food for the villager (46 here): no entry, no cost, no draw (records/
    // 425906646's update 31: player 5's refused QUEUE leaves player 6's draws alone)
    let mut poor = one_player(vec![centre(9)], [40.0, 0.0, 0.0, 0.0]);
    poor.tick_with(Millis::new(104), &[queue(9, 83, 1)]);
    assert!(poor.players[0].unsupported.is_none());
    assert_eq!(poor.players[0].attributes[0], Amount::new(40.0));
    let tc = &poor.players[0].objects[ListIndex::new(0)];
    assert!(
        tc.production
            .as_ref()
            .is_none_or(|p| p.queue.entries().is_empty())
    );
    let mut idle = one_player(vec![centre(9)], [40.0, 0.0, 0.0, 0.0]);
    idle.tick_with(Millis::new(104), &[]);
    assert_eq!(
        poor.streams.get(Stream::Action).draws,
        idle.streams.get(Stream::Action).draws
    );
    // the packet's check drops a building the world lacks and refuses the packet
    let mut missing = one_player(vec![centre(9)], [200.0; 4]);
    missing.tick_with(Millis::new(104), &[queue(9 - 1, 83, 1)]);
    assert!(missing.players[0].unsupported.is_none());
    assert_eq!(
        missing.players[0].resources(),
        crate::test_ids::amounts([200.0; 4])
    );
}

/// `FUN_141614450` over two town centres: unit by unit the one with room and the least load
/// (`FUN_1411d0e90`) takes it, the last pick kept on a tie: five of master 99 (25 seconds each)
/// go to 9, 10, 10, 9, 9.
#[test]
fn a_queue_at_several_buildings_shares_its_units_out_by_load() {
    let several = |amount| {
        Command::Queue(QueueCommand {
            player: player_id(0),
            buildings: unit_list(object_ids([9, 10])),
            building_master: master_id(109),
            unit: master_id(99),
            amount,
            button: -1,
        })
    };
    let total = |t: &World, k: ListIndex| {
        t.players[0].objects[k]
            .production
            .as_ref()
            .map_or(0, |p| p.queue.total)
    };
    let mut t = one_player(vec![centre(9), centre(10)], [200.0; 4]);
    t.tick_with(Millis::new(104), &[several(5)]);
    assert_eq!(
        (total(&t, ListIndex::new(0)), total(&t, ListIndex::new(1))),
        (3, 2)
    );
    // a centre with work queued: 25 seconds less the work done the update gave it
    let mut t = one_player(vec![centre(9), centre(10)], [200.0; 4]);
    t.tick_with(Millis::new(104), &[queue(9, 99, 1)]);
    let Some(crate::production::Making::Unit(make)) = t.players[0].objects[ListIndex::new(0)]
        .production
        .as_ref()
        .and_then(|p| p.action)
    else {
        panic!("a make action");
    };
    assert_eq!(
        t.production_load(PlayerId::new(0), &t.players[0].objects[ListIndex::new(0)])
            .unwrap(),
        Seconds::new(25.0) - make.progress
    );
    assert_eq!(
        t.production_load(PlayerId::new(0), &t.players[0].objects[ListIndex::new(1)])
            .unwrap(),
        Seconds::ZERO
    );
    // the empty one takes the first, then the loaded one the second (25 above its load)
    t.tick_with(Millis::new(104), &[several(2)]);
    assert_eq!(
        (total(&t, ListIndex::new(0)), total(&t, ListIndex::new(1))),
        (2, 1)
    );
}

/// `FUN_141614450` gives a building room and load only where the unit trains (`FUN_141263610`,
/// [`World::trains_at`]): a building of another master keeps room −1 and takes none.
/// 427647190's players queued villagers at their trophy (master 1649, which trains none) and
/// the game queued nothing there.
#[test]
fn a_queue_at_a_building_that_does_not_train_the_unit_queues_nothing_there() {
    let trophy = |id| {
        let mut u = town_centre(id);
        u.life.master_id = master_id(1649);
        u
    };
    let with_trophy = |objects| {
        let mut t = one_player(objects, [200.0; 4]);
        let master = Master {
            id: master_id(1649),
            kind: BUILDING,
            class: class::BUILDING,
            ..Master::default()
        };
        t.masters = vec![Masters::new(t.masters[0].iter().cloned().chain([master]))];
        t
    };
    let total = |t: &World, k: ListIndex| {
        t.players[0].objects[k]
            .production
            .as_ref()
            .map_or(0, |p| p.queue.total)
    };
    let both = Command::Queue(QueueCommand {
        player: player_id(0),
        buildings: unit_list(object_ids([9, 10])),
        building_master: master_id(109),
        unit: master_id(99),
        amount: 3,
        button: -1,
    });
    let mut t = with_trophy(vec![trophy(9), centre(10)]);
    t.tick_with(Millis::new(104), &[both]);
    assert!(t.players[0].unsupported.is_none());
    assert_eq!(
        (total(&t, ListIndex::new(0)), total(&t, ListIndex::new(1))),
        (0, 3)
    );
    // the trophy alone: no room, nothing queued, nothing paid
    let mut alone = with_trophy(vec![trophy(9)]);
    alone.tick_with(Millis::new(104), &[queue(9, 83, 1)]);
    assert!(alone.players[0].unsupported.is_none());
    assert_eq!(
        alone.players[0].resources(),
        crate::test_ids::amounts([200.0; 4])
    );
    assert!(
        alone.players[0].objects[ListIndex::new(0)]
            .production
            .is_none()
    );
}

/// `FUN_141614450` with players ordering each other's (`+0x1074`): a building of another
/// player takes part as its own, and its part (`FUN_1411cbc40`) queues and pays for its owner
/// (`+0x138`); the cost check before reads the ordering player's stock. Without the setting the
/// packet's check drops it.
#[test]
fn a_queue_at_another_player_s_building_queues_and_pays_for_its_owner() {
    let two = |cheats| {
        let mut t = one_player(vec![], [200.0; 4]);
        let mut other = t.players[0].clone();
        other.objects = vec![centre(10)].into();
        other.attributes[..4].copy_from_slice(&[
            Amount::new(100.0),
            Amount::ZERO,
            Amount::ZERO,
            Amount::ZERO,
        ]);
        t.players.push(other);
        t.player_time.push(Millis::ZERO);
        t.masters.push(t.masters[0].clone());
        t.settings.cheats_enabled = cheats;
        t
    };
    let mut t = two(true);
    t.tick_with(Millis::new(104), &[queue(10, 83, 1)]);
    assert!(t.players.iter().all(|p| p.unsupported.is_none()));
    assert_eq!(
        t.players[0].resources(),
        crate::test_ids::amounts([200.0; 4])
    );
    assert_eq!(t.players[1].attributes[0], Amount::new(54.0));
    let production = t.players[1].objects[ListIndex::new(0)]
        .production
        .as_ref()
        .unwrap();
    assert_eq!(production.queue.total, 1);
    let mut t = two(false);
    t.tick_with(Millis::new(104), &[queue(10, 83, 1)]);
    assert_eq!(t.players[1].attributes[0], Amount::new(100.0));
    assert!(t.players[1].objects[ListIndex::new(0)].production.is_none());
}

/// `FUN_141614450` reads a listed object live or sleeping (`FUN_140e6b750`): a sleeping one the
/// unit does not train at keeps room −1, and the town centre beside it takes the unit.
#[test]
fn a_queue_passes_over_a_sleeping_object_the_unit_does_not_train_at() {
    let mut t = one_player(vec![centre(9)], [200.0; 4]);
    let mut asleep = town_centre(11);
    asleep.life.master_id = master_id(99);
    t.players[0].sleeping.push(asleep);
    let both = Command::Queue(QueueCommand {
        player: player_id(0),
        buildings: unit_list(object_ids([11, 9])),
        building_master: master_id(109),
        unit: master_id(99),
        amount: 2,
        button: -1,
    });
    t.tick_with(Millis::new(104), &[both]);
    assert!(t.players[0].unsupported.is_none());
    let production = t.players[0].objects[ListIndex::new(0)]
        .production
        .as_ref()
        .unwrap();
    assert_eq!(production.queue.total, 2);
    assert!(
        t.players[0].sleeping[ListIndex::new(0)]
            .production
            .is_none()
    );
}

/// RESEARCH's building (`0x140e7e270`): with more than one listed, the least loaded of the
/// packet's and the listed ones the world holds; a tie keeps the one taken; one alone is the
/// packet's.
#[test]
fn a_research_at_several_buildings_takes_the_least_loaded() {
    let mut t = one_player(vec![centre(9), centre(10)], [200.0; 4]);
    t.tick_with(Millis::new(104), &[queue(9, 99, 1)]);
    let at = |player, building, count, buildings: Vec<i32>| ResearchCommand {
        player,
        building,
        count,
        buildings: object_ids(buildings),
        tech: 0,
        q_id: -1,
        shift: false,
    };
    let loaded = |r: ResearchCommand| t.least_loaded(&r, &r.buildings).unwrap();
    assert_eq!(
        loaded(at(player_id(0), Some(object_id(9)), 3, vec![9, 77, 10])),
        Some(object_id(10))
    );
    assert_eq!(
        loaded(at(player_id(0), Some(object_id(10)), 2, vec![10, 9])),
        Some(object_id(10))
    );
    assert_eq!(
        loaded(at(player_id(0), Some(object_id(9)), 1, vec![9, 10])),
        Some(object_id(9))
    );
    assert_eq!(
        loaded(at(player_id(0), Some(object_id(77)), 1, vec![])),
        None
    );
}

/// `do_command`'s case 0x65 (`FUN_140e6f470`), RESEARCH's check before its handler: listing
/// none is refused; the packet's building, when the world holds it, must be the player's own
/// (any while players may order each other's), and one listed alone must be held; of more
/// listed, another player's or one the world lacks becomes −1, and a human's left without one
/// is refused.
#[test]
fn a_research_s_check_refuses_another_player_s_buildings() {
    let mut t = one_player(vec![centre(9), centre(10)], [200.0; 4]);
    t.players.push(Player::default());
    let at = |player, building, count, buildings: Vec<i32>| ResearchCommand {
        player,
        building,
        count,
        buildings: object_ids(buildings),
        tech: 0,
        q_id: -1,
        shift: false,
    };
    // its own: listed alone, or with the others checked
    assert_eq!(
        t.research_check(&at(player_id(0), Some(object_id(9)), 1, vec![]))
            .unwrap(),
        Some(vec![])
    );
    assert_eq!(
        t.research_check(&at(player_id(0), Some(object_id(9)), 3, vec![9, 77, 10]))
            .unwrap(),
        Some(object_ids([9, -1, 10]))
    );
    assert_eq!(
        t.research_check(&at(player_id(0), Some(object_id(9)), 0, vec![]))
            .unwrap(),
        None
    );
    assert_eq!(
        t.research_check(&at(player_id(0), Some(object_id(9)), -2, vec![]))
            .unwrap(),
        None
    );
    // player 1's: another's building, one alone the world lacks, a list of none of its own
    assert_eq!(
        t.research_check(&at(player_id(1), Some(object_id(9)), 1, vec![]))
            .unwrap(),
        None
    );
    assert_eq!(
        t.research_check(&at(player_id(1), Some(object_id(9)), 2, vec![9, 10]))
            .unwrap(),
        None
    );
    assert_eq!(
        t.research_check(&at(player_id(1), Some(object_id(77)), 1, vec![]))
            .unwrap(),
        None
    );
    assert_eq!(
        t.research_check(&at(player_id(1), Some(object_id(77)), 2, vec![9, 10]))
            .unwrap(),
        None
    );
    // a building the world lacks with a list skips the packet's own test
    assert_eq!(
        t.research_check(&at(player_id(0), Some(object_id(77)), 2, vec![77, 10]))
            .unwrap(),
        Some(object_ids([-1, 10]))
    );
    // a computer's list left without one still runs
    t.players[1].computer = true;
    assert_eq!(
        t.research_check(&at(player_id(1), Some(object_id(77)), 2, vec![9, 10]))
            .unwrap(),
        Some(object_ids([-1, -1]))
    );
    // while players may order each other's
    t.settings.cheats_enabled = true;
    t.players[1].computer = false;
    assert_eq!(
        t.research_check(&at(player_id(1), Some(object_id(9)), 1, vec![]))
            .unwrap(),
        Some(vec![])
    );
}

/// RESEARCH (`0x140e7e270`) of Castle Age (0x66) at the town centre: queued as any technology;
/// only in game mode 16 does it mark the world's treaty monitor (`+0x820`'s `+0x17`), which the
/// slice does not hold.
#[test]
fn a_research_of_castle_age_queues_it_but_in_game_mode_16() {
    use crate::technology::{Location, TechDef, TechState};
    let castle = |mode| {
        let mut t = one_player(vec![centre(9)], [1000.0; 4]);
        t.settings.game_mode = mode;
        t.techs = vec![TechDef::default(); 0x67];
        let mut techs = vec![TechState::default(); 0x67];
        techs[0x66] = TechState {
            state: ResearchState::Available,
            research_cap: -1,
            building_master: None,
            locations: vec![Location {
                building: Some(ids::TOWN_CENTRE),
                time: 160,
                button: 1,
                adjustment: Seconds::ZERO,
            }],
            ..TechState::default()
        };
        t.players[0].techs = techs;
        let at = ResearchCommand {
            player: player_id(0),
            building: Some(object_id(9)),
            count: 1,
            buildings: vec![],
            tech: 0x66,
            q_id: -1,
            shift: false,
        };
        let r = research_listed(&mut t, &at);
        let queued = t.players[0].objects[ListIndex::new(0)]
            .production
            .as_ref()
            .map_or(0, |p| p.queue.total);
        (r, queued)
    };
    let (r, queued) = castle(0);
    assert_eq!((r.is_ok(), queued), (true, 1));
    assert!(castle(16).0.is_err());
}

/// [`one_player`]'s town centre 9 with a unit AI and technology 5, available (state 1) at the
/// centre's master 109, its first cost 50 food.
fn research_world() -> World {
    use crate::technology::{Cost, Location, TechDef, TechState};
    let mut t = one_player(vec![centre(9)], [1000.0; 4]);
    t.players[0].objects[ListIndex::new(0)].ai = Some(UnitAi::default());
    t.techs = vec![TechDef::default(); 6];
    t.techs[5].costs[0] = Cost {
        attribute: Some(attribute::FOOD),
        amount: 50,
        paid: 1,
    };
    let mut techs = vec![TechState::default(); 6];
    techs[5] = TechState {
        state: ResearchState::Available,
        research_cap: -1,
        building_master: None,
        overrides: [(0, None, -2), (0, None, 0), (0, None, 0)],
        locations: vec![Location {
            building: Some(ids::TOWN_CENTRE),
            time: 30,
            button: 1,
            adjustment: Seconds::ZERO,
        }],
        ..TechState::default()
    };
    t.players[0].techs = techs;
    t
}

/// RESEARCH of technology `tech` at player 0's building 9 with the target `target`.
fn research_at(tech: i16, target: i32) -> ResearchCommand {
    ResearchCommand {
        player: player_id(0),
        building: Some(object_id(9)),
        count: 1,
        buildings: vec![],
        tech,
        q_id: target,
        shift: false,
    }
}

/// RESEARCH (`0x140e7e270`): its listed costs come off the pending debit, and an alive building
/// at one of the technology's locations queues it, its unit AI on the action value 0x26c; a
/// target, a technology the tree lacks, one neither available nor repeatable, a building
/// nowhere, a dead one or one at no location do nothing past the debit.
#[test]
fn a_research_queues_at_an_alive_location_after_its_costs_come_off_the_debit() {
    let queued = |t: &World| {
        t.players[0].objects[ListIndex::new(0)]
            .production
            .as_ref()
            .map_or(0, |p| p.queue.total)
    };
    let mut t = research_world();
    research_listed(&mut t, &research_at(5, -1)).unwrap();
    assert_eq!(queued(&t), 1);
    assert_eq!(t.players[0].pending_debit, [-50.0, 0.0, 0.0, 0.0]);
    let ai = t.players[0].objects[ListIndex::new(0)].ai.as_ref().unwrap();
    assert_eq!(ai.current_action, Some(ActionKind::Research));
    let untouched = research_world();
    for (tech, target) in [(5, 9), (6, -1), (-1, -1)] {
        let mut t = research_world();
        research_listed(&mut t, &research_at(tech, target)).unwrap();
        assert_eq!(t, untouched);
    }
    let refused = |change: &dyn Fn(&mut World)| {
        let mut t = research_world();
        change(&mut t);
        research_listed(&mut t, &research_at(5, -1)).unwrap();
        (queued(&t), t.players[0].pending_debit[0])
    };
    assert_eq!(
        refused(&|t| t.players[0].techs[5].state = ResearchState::Researching),
        (0, -50.0)
    );
    assert_eq!(
        refused(&|t| t.players[0].objects[ListIndex::new(0)].id = object_id(8)),
        (0, -50.0)
    );
    assert_eq!(
        refused(&|t| t.players[0].objects[ListIndex::new(0)].life.state = ObjectState::Dying),
        (0, -50.0)
    );
    assert_eq!(
        refused(&|t| t.players[0].techs[5].locations[0].building = Some(master_id(110))),
        (0, -50.0)
    );
}

/// A queued research starts in the building's production list (`0x140ad1d80`): the technology
/// takes state 2 at the building's master, the action state 6 with no work done.
#[test]
fn a_queued_research_starts_at_the_buildings_master() {
    let mut t = research_world();
    research_listed(&mut t, &research_at(5, -1)).unwrap();
    t.produce(PlayerId::new(0), ListIndex::new(0), Seconds::ZERO)
        .unwrap();
    let master = t.players[0].objects[ListIndex::new(0)].life.master_id;
    let action = t.players[0].objects[ListIndex::new(0)]
        .production
        .as_ref()
        .and_then(|p| p.action);
    let Some(crate::production::Making::Research(r)) = action else {
        panic!("a research action: {action:?}");
    };
    assert_eq!(
        (r.state, r.progress),
        (crate::production::MakeTechState::Researching, Seconds::ZERO)
    );
    assert_eq!(t.players[0].techs[5].state, ResearchState::Researching);
    assert_eq!(t.players[0].techs[5].building_master, Some(master));
}

/// RESEARCH's unmodelled parts: a technology the data file lacks, the shared kind, a linked
/// technology, a building of another player, a computer player's research and a building with
/// an unmodelled action.
#[test]
fn a_research_s_unmodelled_parts_fail() {
    let fails = |change: &dyn Fn(&mut World)| {
        let mut t = research_world();
        change(&mut t);
        research_listed(&mut t, &research_at(5, -1))
            .unwrap_err()
            .reason()
    };
    assert_eq!(
        fails(&|t| t.techs.truncate(5)),
        "a technology the data file has not got"
    );
    assert_eq!(
        fails(&|t| t.techs[5].kind = crate::technology::TechType::SHARED),
        "RESEARCH of the shared kind (FUN_1411c3fd0 over the packet's buildings)"
    );
    assert_eq!(
        fails(&|t| {
            let _ = t.linked_techs.insert(5, crate::technology::Linked::Cycle);
        }),
        "RESEARCH of a linked technology (FUN_140f3a0f0, FUN_140f39e80)"
    );
    assert_eq!(
        fails(&|t| {
            let tc = t.players[0].objects.remove(ListIndex::new(0)).unwrap();
            t.players.push(Player {
                objects: vec![tc].into(),
                ..Player::default()
            });
            t.masters.push(t.masters[0].clone());
        }),
        "RESEARCH at another player's object"
    );
    assert_eq!(
        fails(&|t| t.players[0].computer = true),
        "a computer player's RESEARCH (FUN_1411d5520, player+0x7a8)"
    );
    assert_eq!(
        fails(&|t| {
            t.players[0].objects[ListIndex::new(0)].unmodelled_action =
                Some(GenieError::unsupported("an action"));
        }),
        "RESEARCH at a building with an unmodelled action"
    );
}

/// `FUN_1411cbc40`: a queue adds its units' population to attribute 80 (the queued units),
/// which keeps it until the unit spawns; the make action's start counts the one it makes in
/// 81.
#[test]
fn a_queue_counts_its_units_population_as_queued() {
    let mut t = one_player(vec![centre(9)], [1000.0; 4]);
    t.tick_with(Millis::new(104), &[queue(9, 83, 3)]);
    let a = &t.players[0].attributes;
    assert_eq!((a[80], a[81]), (Amount::new(3.0), Amount::new(1.0)));
    // a unit without population (master 99, no storage of kind 4) adds none
    let mut other = one_player(vec![centre(9)], [1000.0; 4]);
    other.tick_with(Millis::new(104), &[queue(9, 99, 2)]);
    assert_eq!(other.players[0].attributes[80], Amount::ZERO);
}

#[test]
fn a_queue_takes_as_many_as_the_room_and_the_resources_allow() {
    // food for two of three: two entries, two draws
    let mut two = one_player(vec![centre(9)], [100.0, 0.0, 0.0, 0.0]);
    two.tick_with(Millis::new(104), &[queue(9, 83, 3)]);
    let mut one = one_player(vec![centre(9)], [100.0, 0.0, 0.0, 0.0]);
    one.tick_with(Millis::new(104), &[queue(9, 83, 1)]);
    let queued = |t: &World| {
        t.players[0].objects[ListIndex::new(0)]
            .production
            .as_ref()
            .unwrap()
            .queue
            .total
    };
    assert_eq!(queued(&two), 2);
    assert_eq!(
        two.players[0].attributes[0],
        Amount::new(100.0 - 2.0 * 46.0)
    );
    assert_eq!(
        two.streams.get(Stream::Action).draws,
        one.streams.get(Stream::Action).draws + 1
    );
    // 14 queued: room for one of five
    let mut full = one_player(vec![centre(9)], [1000.0; 4]);
    full.tick_with(Millis::new(104), &[queue(9, 99, 14), queue(9, 99, 5)]);
    assert_eq!(queued(&full), 15);
    // a computer player fills to 2
    let mut computer = one_player(vec![centre(9)], [1000.0; 4]);
    computer.players[0].computer = true;
    computer.tick_with(Millis::new(104), &[queue(9, 99, 5)]);
    assert_eq!(queued(&computer), 2);
}

/// `FUN_1411cbc40` joins units of the last entry's master and button whose costs are the same;
/// another button opens an entry with that button's train time; a full buffer of ten grows by
/// ten, the serials numbering the entries.
#[test]
fn queued_units_join_the_last_entry_and_a_full_buffer_grows() {
    let with = |amount, button| {
        Command::Queue(QueueCommand {
            player: player_id(0),
            buildings: unit_list(object_ids([9])),
            building_master: master_id(109),
            unit: master_id(99),
            amount,
            button,
        })
    };
    let mut t = one_player(vec![centre(9)], [1000.0; 4]);
    t.tick_with(Millis::new(104), &[with(2, -1), with(3, -1), with(1, 1)]);
    let q = &t.players[0].objects[ListIndex::new(0)]
        .production
        .as_ref()
        .unwrap()
        .queue;
    assert_eq!((q.count, q.total, q.changes, q.next_serial), (2, 6, 3, 2));
    assert_eq!(
        (q.slots[0].count, q.slots[1].count, q.slots[1].button),
        (5, 1, 1)
    );
    assert_eq!((q.slots[1].train_time, q.slots[1].serial), (25, 1));
    assert_eq!(t.players[0].tracker.queued, 6);
    let mut t = one_player(vec![centre(9)], [1000.0; 4]);
    let alternating: Vec<_> = (0..11).map(|k| with(1, k % 2)).collect();
    t.tick_with(Millis::new(104), &alternating);
    let q = &t.players[0].objects[ListIndex::new(0)]
        .production
        .as_ref()
        .unwrap()
        .queue;
    assert_eq!((q.slots.len(), q.count, q.total), (20, 11, 11));
    let serials: Vec<_> = q.entries().iter().map(|e| e.serial).collect();
    assert_eq!(serials, (0..11).collect::<Vec<i64>>());
}

/// `FUN_1411e7dc0`: a computer player's housed building cancels its queue
/// (`FUN_1411cff00`): the villager's 46 food come back and attribute 80 empties; a disabled
/// queue (`+0xbf6` clear) makes nothing.
#[test]
fn a_computer_player_s_housed_building_cancels_its_queue() {
    let mut t = one_player(vec![centre(9)], [1000.0; 4]);
    t.players[0].computer = true;
    t.players[0].attributes[4] = Amount::new(0.5);
    t.tick_with(Millis::new(104), &[queue(9, 83, 1)]);
    let p = &t.players[0];
    assert_eq!(
        (p.attributes[0], p.attributes[80], p.attributes[81]),
        (Amount::new(1000.0), Amount::ZERO, Amount::ZERO)
    );
    let production = p.objects[ListIndex::new(0)].production.as_ref().unwrap();
    assert_eq!((production.queue.count, production.queue.total), (0, 0));
    assert!(production.action.is_none());
    let mut t = one_player(vec![centre(9)], [1000.0; 4]);
    t.players[0].objects[ListIndex::new(0)].production = Some(crate::production::Production {
        queue: crate::production::Queue {
            enabled: false,
            ..crate::production::Queue::default()
        },
        ..crate::production::Production::default()
    });
    t.tick_with(Millis::new(104), &[queue(9, 83, 1)]);
    let p = &t.players[0];
    assert_eq!(
        (p.attributes[80], p.attributes[81]),
        (Amount::new(1.0), Amount::ZERO)
    );
    assert!(
        p.objects[ListIndex::new(0)]
            .production
            .as_ref()
            .unwrap()
            .action
            .is_none()
    );
}

/// Game mode 16 with attribute 6 at most 1: a military unit (master 99, class 0) is queued
/// one at a time, the player's flag (`+0x1378`) set; a second QUEUE queues nothing.
#[test]
fn game_mode_16_queues_one_military_unit_at_a_time() {
    let mut t = one_player(vec![centre(9)], [1000.0; 4]);
    t.settings.game_mode = 16;
    t.tick_with(Millis::new(104), &[queue(9, 99, 3)]);
    t.tick_with(Millis::new(104), &[queue(9, 99, 2)]);
    let q = &t.players[0].objects[ListIndex::new(0)]
        .production
        .as_ref()
        .unwrap()
        .queue;
    assert_eq!(q.total, 1);
    assert!(t.players[0].one_military_queued);
}

#[test]
fn an_unmodelled_command_stops_its_player() {
    let mut t = one_player(vec![town_centre(9)], [200.0; 4]);
    let why = GenieError::unsupported("QUEUE at two buildings");
    t.tick_with(
        Millis::new(104),
        &[Command::Unmodelled(UnmodelledCommand {
            player: player_id(0),
            packet: Vec::new(),
            why: why.clone(),
        })],
    );
    assert_eq!(t.players[0].unsupported, Some((1, ListIndex::new(0), why)));
}

/// The masters of the ownership changes' tests, by player: Gaia's (0), player 1's and
/// player 2's archer 4 (class 0, its dead archer 3 of type 10; line of sight 6, 7, 8; fog
/// visibility 0, 1, 2; hit points 30, 35, 40) and sheep 705 (class 58; line of sight 2, 3,
/// 4; fog visibility 0, 1, 2; food 100); player 2 lacks the dead archer, and the dead
/// archer of player 1 is of type 25 (a doppelganger's, which the copy leaves).
fn hands_masters() -> Vec<Masters> {
    let table = |p: u8| {
        let archer = Master {
            id: master_id(4),
            kind: master_type::COMBAT,
            class: class::CLASS_0,
            line_of_sight: Distance::new(6.0 + f32::from(p)),
            fog_visibility: p,
            hit_points: 30 + 5 * i16::from(p),
            death_spawns: [Some(master_id(3)), None],
            storage: vec![
                (Some(attribute::HEADROOM), Amount::new(1.0), 2),
                (Some(attribute::POPULATION), Amount::ZERO, 0),
                (None, Amount::ZERO, 0),
            ],
            ..Master::default()
        };
        let dead = Master {
            id: master_id(3),
            kind: if p == 1 {
                master_type::DOPPELGANGER
            } else {
                master_type::STATIC
            },
            class: class::CLASS_0,
            hit_points: 9 + i16::from(p),
            ..Master::default()
        };
        let sheep = Master {
            id: master_id(705),
            kind: master_type::COMBAT,
            class: class::LIVESTOCK,
            line_of_sight: Distance::new(2.0 + f32::from(p)),
            fog_visibility: p,
            storage: vec![(Some(attribute::FARM_FOOD), Amount::new(100.0), 1)],
            ..Master::default()
        };
        if p == 2 {
            Masters::new([archer, sheep])
        } else {
            Masters::new([archer, dead, sheep])
        }
    };
    vec![table(0), table(1), table(2)]
}

/// A 40 × 40 world of three players' sight with the registry, holding object `id` of
/// player `p`, class `class` and master `master` at (10.5, 10.5) in player `p`'s registry
/// list, its sight on.
fn hands_world(id: i32, p: PlayerId, class: ClassId, master: MasterId, los: Distance) -> Spatial {
    let n = 40 * 40;
    let mut s = Spatial::new(GridSize::new(40, 40));
    let mut v = crate::vision::visibility::Visibility::new(
        GridSize::new(40, 40),
        (0..3)
            .map(|_| crate::vision::visibility::VisibleMap {
                counts: vec![-1; n],
                explored: 0,
            })
            .collect(),
        vec![0; n],
        Vec::new(),
        0,
        vec![false; 3],
    );
    let mut r = crate::vision::registry::Registry::new(3, 65);
    r.entries.entry(object_id(id)).or_default().mask = 1 << p.get();
    r.update_unit(UnitUpdate {
        id: object_id(id),
        owner: p.index() as u8,
        position: Point::new(10.5, 10.5),
        radii: Footprint::new(0.2, 0.2),
        class,
        old: 0,
        new: 1 << p.get(),
    })
    .unwrap();
    v.registry = Some(r);
    let at = Position::new(10.5, 10.5, 0.0);
    v.add_seer(
        object_id(id),
        Seer {
            player: p,
            master,
            los,
            state: ObjectState::Alive,
            explored: true,
            building: None,
            umv: 0,
            capture: capture_kind(p, class, master),
            kind: master_type::COMBAT,
        },
    );
    let _ = v.sight_on(object_id(id), at, Footprint::new(0.2, 0.2), false, false);
    s.sight = Some(v);
    s.insert(
        object_id(id),
        Placed {
            position: at,
            radius: Footprint::new(0.2, 0.2),
            class,
            player: p,
            master,
            obstruction_type: ObstructionType::NONE,
            buildable_on: 0,
            tall: true,
            speed: Speed::ZERO,
            unknown: None,
            obstruction: None,
        },
    );
    s
}

/// A live unit `id` of master `master` with a unit AI holding a notify.
fn hands_unit(id: i32, master: MasterId) -> Unit {
    let mut ai = UnitAi::default();
    ai.notifies
        .push(Notify::own(object_id(id), NotifyKind::DAMAGED, [1, 2, 3]));
    Unit {
        id: object_id(id),
        body: crate::objects::layers::Body::at(Position::new(10.5, 10.5, 0.0)),
        life: Lifecycle {
            master_id: master,
            ..Lifecycle::default()
        },
        ai: Some(ai),
        combat: true,
        ..Unit::default()
    }
}

/// `FUN_14120f800` on MarvinBlue/426048681's Gaia archer 21712 (master 4) that player 1
/// takes over: its records and place take player 1, its sight moves from Gaia's map to
/// player 1's with Gaia's line of sight, it takes a master of its own (a copy of Gaia's,
/// with player 1's fog visibility) and a copy of Gaia's dead archer, its unit AI's notifies
/// go with the world time and it tells its unit AI it changed hands.
#[test]
fn a_taken_gaia_archer_keeps_a_copy_of_gaia_s_master_with_its_new_owner_s_fog() {
    let masters = hands_masters();
    let mut s = hands_world(
        21712,
        PlayerId::new(0),
        class::CLASS_0,
        master_id(4),
        Distance::new(6.0),
    );
    let mut u = hands_unit(21712, master_id(4));
    assert_eq!(
        change_hands(
            &mut u,
            (0, PlayerId::new(1)),
            &masters,
            &mut s,
            WorldTime::from_millis(1250)
        ),
        Ok(true)
    );
    let own = u.own_master.as_deref().unwrap();
    assert_eq!(
        (own.hit_points, own.line_of_sight, own.fog_visibility),
        (30, Distance::new(6.0), 1)
    );
    assert_eq!(u.dead_master.as_deref().map(|d| d.hit_points), Some(9));
    assert_eq!(
        s.get(object_id(21712)).map(|p| p.player),
        Some(PlayerId::new(1))
    );
    let v = s.sight.as_ref().unwrap();
    let r = v.registry.as_ref().unwrap();
    assert_eq!(r.lists[0][0][0].owner, 1);
    let seer = v.seer(object_id(21712)).copied().unwrap();
    assert_eq!(
        (seer.player, seer.los),
        (PlayerId::new(1), Distance::new(6.0))
    );
    let tile = |x: usize, y: usize| y * 40 + x;
    assert_eq!(v.maps[0].counts[tile(10, 10)], 0);
    assert_eq!(v.maps[1].counts[tile(16, 10)], 1);
    let ai = u.ai.as_ref().unwrap();
    assert_eq!(ai.last_update_time, Millis::new(1250));
    assert_eq!(
        ai.notifies,
        [Notify::own(
            object_id(21712),
            NotifyKind::OWNER_CHANGED,
            [1, 0, 0]
        )]
    );
}

/// `FUN_14120f800` on a sheep: its master of its own takes the new owner's line of sight
/// (class 58), which its sight on the new owner's map reads; its master has no death spawn.
/// A master of its own it had stays (with the new fog visibility), and so does a dead
/// master; a dead master of a type the copy leaves (25) gives none.
#[test]
fn a_taken_sheep_sees_with_its_new_owner_s_line_of_sight() {
    let masters = hands_masters();
    let mut s = hands_world(
        9520,
        PlayerId::new(1),
        class::LIVESTOCK,
        master_id(705),
        Distance::new(3.0),
    );
    let mut u = hands_unit(9520, master_id(705));
    assert_eq!(
        change_hands(
            &mut u,
            (1, PlayerId::new(2)),
            &masters,
            &mut s,
            WorldTime::ZERO
        ),
        Ok(true)
    );
    let own = u.own_master.as_deref().unwrap();
    assert_eq!(
        (own.line_of_sight, own.fog_visibility),
        (Distance::new(4.0), 2)
    );
    assert_eq!(u.dead_master, None);
    let v = s.sight.as_ref().unwrap();
    assert_eq!(
        v.seer(object_id(9520)).map(|s| s.los),
        Some(Distance::new(4.0))
    );
    assert_eq!(v.maps[2].counts[10 * 40 + 14], 1);
    // back to player 1: the copy stays, with player 1's fog and line of sight
    assert_eq!(
        change_hands(
            &mut u,
            (2, PlayerId::new(1)),
            &masters,
            &mut s,
            WorldTime::ZERO
        ),
        Ok(true)
    );
    let own = u.own_master.as_deref().unwrap();
    assert_eq!(
        (own.line_of_sight, own.fog_visibility),
        (Distance::new(3.0), 1)
    );
    // player 1's archer: its dead archer is of type 25, and none is copied
    let mut s = hands_world(
        7,
        PlayerId::new(1),
        class::CLASS_0,
        master_id(4),
        Distance::new(7.0),
    );
    let mut a = hands_unit(7, master_id(4));
    assert_eq!(
        change_hands(
            &mut a,
            (1, PlayerId::new(0)),
            &masters,
            &mut s,
            WorldTime::ZERO
        ),
        Ok(true)
    );
    assert_eq!(a.dead_master, None);
    // a dead master it holds stays
    let held = Box::new(Master {
        id: master_id(3),
        hit_points: 99,
        ..Master::default()
    });
    a.dead_master = Some(held.clone());
    assert_eq!(
        change_hands(
            &mut a,
            (0, PlayerId::new(1)),
            &masters,
            &mut s,
            WorldTime::ZERO
        ),
        Ok(true)
    );
    assert_eq!(a.dead_master, Some(held));
    assert_eq!(a.own_master.as_deref().map(|m| m.hit_points), Some(35));
}

/// A unit of player `p` at (`x`, 10.5) for the owner search of [`hands_world`]'s object: a
/// combat-type seer in state 2 on its tile's list.
fn owner_search_unit(s: &mut Spatial, id: i32, p: PlayerId, x: f32) {
    s.sight.as_mut().unwrap().add_seer(
        object_id(id),
        Seer {
            player: p,
            master: master_id(4),
            los: Distance::new(6.0),
            state: ObjectState::Alive,
            explored: true,
            building: None,
            umv: 0,
            capture: capture_kind(p, class::CLASS_0, master_id(4)),
            kind: master_type::COMBAT,
        },
    );
    s.insert(
        object_id(id),
        Placed {
            position: Position::new(x, 10.5, 0.0),
            radius: Footprint::new(0.2, 0.2),
            class: class::CLASS_0,
            player: p,
            master: master_id(4),
            obstruction_type: ObstructionType::NONE,
            buildable_on: 0,
            tall: true,
            speed: Speed::ZERO,
            unknown: None,
            obstruction: None,
        },
    );
}

/// Players of attribute 97 `attribute_97` and game status `status`, as an Artifact's owner
/// search reads them.
fn artifact_players(attribute_97: &[f32], status: &[u8]) -> Vec<Player> {
    status
        .iter()
        .enumerate()
        .map(|(q, &game_status)| {
            let mut attributes = vec![Amount::ZERO; 98];
            attributes[97] = Amount::new(attribute_97.get(q).copied().unwrap_or(0.0));
            Player {
                attributes,
                game_status,
                ..Player::default()
            }
        })
        .collect()
}

/// The Artifact action's owner search (`FUN_140aabc40`) passes over the units of a player that
/// is out (`+0x89` not 0) and over such a player as a taker: 428265662's Gaia sheep 27530 in
/// update 642 stays Gaia's next to the units of player 7, who resigned in update 428.
#[test]
fn an_owner_search_passes_over_a_player_out_of_the_game() {
    let unit = owner_search_unit;
    let mut s = hands_world(
        27530,
        PlayerId::new(0),
        class::LIVESTOCK,
        master_id(594),
        Distance::new(2.0),
    );
    unit(&mut s, 1, PlayerId::new(1), 11.5);
    let none = [0.0; 3];
    assert_eq!(
        s.artifact_owner(object_id(27530), &artifact_players(&none, &[0, 0, 0])),
        Ok(Some(PlayerId::new(1)))
    );
    assert_eq!(
        s.artifact_owner(object_id(27530), &artifact_players(&none, &[0, 2, 0])),
        Ok(None)
    );
    // of two players, the nearest takes it: player 2 once player 1 is out
    unit(&mut s, 2, PlayerId::new(2), 12.5);
    assert_eq!(
        s.artifact_owner(object_id(27530), &artifact_players(&none, &[0, 0, 0])),
        Ok(Some(PlayerId::new(1)))
    );
    assert_eq!(
        s.artifact_owner(object_id(27530), &artifact_players(&none, &[0, 2, 0])),
        Ok(Some(PlayerId::new(2)))
    );
    assert_eq!(
        s.artifact_owner(object_id(27530), &artifact_players(&none, &[0, 2, 1])),
        Ok(None)
    );
}

/// The owner search's own owner and attribute 97 (`FUN_140aabc40`): a unit of the animal's own
/// owner keeps it, and so does that owner's attribute 97 above 0 for livestock; another
/// player's attribute 97 above 0 counts its units 100 more, so it takes the animal from its
/// owner. A unit of a class that never counts (a building's, 3) and a Gaia unit with `+0x924`
/// set do not count; an animal inside another object searches about its record there.
#[test]
fn an_owner_search_keeps_the_owner_s_animal_unless_attribute_97_takes_it() {
    let unit = owner_search_unit;
    let none = [0.0; 3];
    let mut s = hands_world(
        27530,
        PlayerId::new(1),
        class::LIVESTOCK,
        master_id(594),
        Distance::new(2.0),
    );
    unit(&mut s, 1, PlayerId::new(1), 11.5);
    unit(&mut s, 2, PlayerId::new(2), 11.5);
    assert_eq!(
        s.artifact_owner(object_id(27530), &artifact_players(&none, &[0, 0, 0])),
        Ok(None)
    );
    assert_eq!(
        s.artifact_owner(
            object_id(27530),
            &artifact_players(&[0.0, 0.0, 1.0], &[0, 0, 0])
        ),
        Ok(Some(PlayerId::new(2)))
    );
    assert_eq!(
        s.artifact_owner(
            object_id(27530),
            &artifact_players(&[0.0, 1.0, 1.0], &[0, 0, 0])
        ),
        Ok(None)
    );
    let mut s = hands_world(
        27530,
        PlayerId::new(1),
        class::LIVESTOCK,
        master_id(594),
        Distance::new(2.0),
    );
    unit(&mut s, 0, PlayerId::new(0), 11.5);
    unit(&mut s, 3, PlayerId::new(2), 12.5);
    let building = Placed {
        class: class::BUILDING,
        ..*s.get(object_id(3)).unwrap()
    };
    s.insert(object_id(3), building);
    assert_eq!(
        s.artifact_owner(object_id(27530), &artifact_players(&none, &[0, 0, 0])),
        Ok(None)
    );
    let mut s = hands_world(
        27530,
        PlayerId::new(0),
        class::LIVESTOCK,
        master_id(594),
        Distance::new(2.0),
    );
    unit(&mut s, 1, PlayerId::new(1), 11.5);
    s.go_inside(object_id(27530), Position::new(10.5, 10.5, 0.0));
    assert_eq!(
        s.artifact_owner(object_id(27530), &artifact_players(&none, &[0, 0, 0])),
        Ok(Some(PlayerId::new(1)))
    );
}

/// The owner search's unmodelled parts: an animal the world does not hold, a unit of a player
/// beyond the lists, and a stopped unit that counts and may have moved onto the tiles.
#[test]
fn an_owner_search_s_unmodelled_parts_fail() {
    let none = [0.0; 3];
    let reason = |s: &Spatial, me, status: &[u8]| {
        s.artifact_owner(me, &artifact_players(&none, status))
            .unwrap_err()
            .reason()
    };
    let s = hands_world(
        27530,
        PlayerId::new(0),
        class::LIVESTOCK,
        master_id(594),
        Distance::new(2.0),
    );
    assert_eq!(
        reason(&s, object_id(99), &[0, 0, 0]),
        "an owner search of an object the world does not hold"
    );
    let mut s = hands_world(
        27530,
        PlayerId::new(0),
        class::LIVESTOCK,
        master_id(594),
        Distance::new(2.0),
    );
    owner_search_unit(&mut s, 1, PlayerId::new(3), 11.5);
    assert_eq!(
        reason(&s, object_id(27530), &[0, 0, 0]),
        "an owner search's unit of a player beyond the list"
    );
    let mut s = hands_world(
        27530,
        PlayerId::new(0),
        class::LIVESTOCK,
        master_id(594),
        Distance::new(2.0),
    );
    owner_search_unit(&mut s, 1, PlayerId::new(1), 20.5);
    let stopped = Placed {
        unknown: Some(10.0),
        ..*s.get(object_id(1)).unwrap()
    };
    s.insert(object_id(1), stopped);
    assert_eq!(
        reason(&s, object_id(27530), &[0, 0, 0]),
        "an owner search near a stopped unit that counts"
    );
}

/// The owner search walks the tiles' lists (`+0x28`, `FUN_140aabc40`), as the game does: a unit
/// on a full tile (`0xfa` objects: `FUN_140f5f0a0` files no more) is not on its list and does not
/// count, one on the next tile does.
#[test]
fn an_owner_search_counts_only_the_units_on_the_tiles_lists() {
    let mut s = hands_world(
        27530,
        PlayerId::new(0),
        class::LIVESTOCK,
        master_id(594),
        Distance::new(2.0),
    );
    for id in 1000..1250 {
        s.insert(
            object_id(id),
            Placed {
                position: Position::new(11.5, 10.5, 0.0),
                radius: Footprint::new(0.2, 0.2),
                class: class::CLASS_0,
                player: PlayerId::new(0),
                master: master_id(4),
                obstruction_type: ObstructionType::NONE,
                buildable_on: 0,
                tall: false,
                speed: Speed::ZERO,
                unknown: None,
                obstruction: None,
            },
        );
    }
    owner_search_unit(&mut s, 1, PlayerId::new(1), 11.5);
    let none = [0.0; 3];
    assert_eq!(
        s.artifact_owner(object_id(27530), &artifact_players(&none, &[0, 0, 0])),
        Ok(None)
    );
    owner_search_unit(&mut s, 2, PlayerId::new(1), 12.5);
    assert_eq!(
        s.artifact_owner(object_id(27530), &artifact_players(&none, &[0, 0, 0])),
        Ok(Some(PlayerId::new(1)))
    );
}

/// `FUN_14120f800` for a new owner whose table lacks the master writes the new owner into
/// the records and changes nothing else; one inside another (or not in state 2) changes
/// hands without moving its sight.
#[test]
fn an_ownership_change_without_the_master_writes_the_records_alone() {
    let mut masters = hands_masters();
    masters[2] = Masters::new([]);
    let mut s = hands_world(
        21712,
        PlayerId::new(0),
        class::CLASS_0,
        master_id(4),
        Distance::new(6.0),
    );
    let before = s.sight.as_ref().unwrap().maps.clone();
    let mut u = hands_unit(21712, master_id(4));
    assert_eq!(
        change_hands(
            &mut u,
            (0, PlayerId::new(2)),
            &masters,
            &mut s,
            WorldTime::from_millis(9)
        ),
        Ok(false)
    );
    let v = s.sight.as_ref().unwrap();
    assert_eq!(v.registry.as_ref().unwrap().lists[0][0][0].owner, 2);
    assert_eq!(v.maps, before);
    assert_eq!(
        v.seer(object_id(21712)).map(|s| s.player),
        Some(PlayerId::new(0))
    );
    assert_eq!(
        s.get(object_id(21712)).map(|p| p.player),
        Some(PlayerId::new(0))
    );
    assert_eq!(
        (u.own_master.is_none(), u.dead_master.is_none()),
        (true, true)
    );
    assert_eq!(u.ai.as_ref().unwrap().notifies.len(), 1);
    u.life.inside = Some(object_id(100));
    assert_eq!(
        change_hands(
            &mut u,
            (0, PlayerId::new(1)),
            &masters,
            &mut s,
            WorldTime::from_millis(9)
        ),
        Ok(true)
    );
    let v = s.sight.as_ref().unwrap();
    assert_eq!(v.maps, before);
    assert_eq!(
        v.seer(object_id(21712)).map(|s| (s.player, s.los)),
        Some((PlayerId::new(1), Distance::new(6.0)))
    );
    assert_eq!(
        s.get(object_id(21712)).map(|p| p.player),
        Some(PlayerId::new(1))
    );
}

/// `RGE_Combat_Object::update` on MarvinBlue/426048681's Gaia archer 21712 in update 1: its
/// mask now holds player 1's bit, so player 1 takes it over (slot 0x70) and `+0x924` goes 0;
/// it leaves Gaia's list for the end of player 1's, its class count with it (attribute 40).
/// A player whose table lacks the master ends the loop without taking it; outside the
/// standard modes a computer player that does not allow it is passed over.
#[test]
fn a_gaia_archer_a_player_sees_goes_to_that_player() {
    let world = |masters: Vec<Masters>, computer: bool, mode: i32| {
        let mut t = World::new(
            WorldTime::ZERO,
            1,
            PlayerId::new(0),
            vec![Millis::ZERO, Millis::ZERO, Millis::ZERO],
            vec![
                Player {
                    objects: vec![Unit {
                        ai: None,
                        capture_flag: 1,
                        ..hands_unit(21712, master_id(4))
                    }]
                    .into(),
                    attributes: vec![Amount::ZERO; 300],
                    ..Player::default()
                },
                Player {
                    objects: vec![live(500)].into(),
                    attributes: vec![Amount::ZERO; 300],
                    computer,
                    kind: if computer {
                        PlayerType::COMPUTER
                    } else {
                        PlayerType::HUMAN
                    },
                    ..Player::default()
                },
                Player {
                    attributes: vec![Amount::ZERO; 300],
                    ..Player::default()
                },
            ],
        );
        t.settings.game_mode = mode;
        t.players[0].attributes[40] = Amount::new(1.0);
        t.masters = masters;
        t.spatial = hands_world(
            21712,
            PlayerId::new(0),
            class::CLASS_0,
            master_id(4),
            Distance::new(6.0),
        );
        let v = t.spatial.sight.as_mut().unwrap();
        v.unified[10 * 40 + 10] = 0b111;
        t
    };
    let mut t = world(hands_masters(), false, 0);
    t.tick(Millis::new(100));
    assert_eq!(t.players[0].objects.len(), 0);
    let ids: Vec<ObjectId> = t.players[1].objects.iter().map(|u| u.id).collect();
    assert_eq!(ids, [object_id(500), object_id(21712)]);
    let archer = &t.players[1].objects[ListIndex::new(1)];
    assert_eq!(archer.capture_flag, 0);
    assert_eq!(
        archer.own_master.as_deref().map(|m| m.fog_visibility),
        Some(1)
    );
    let v = t.spatial.sight.as_ref().unwrap();
    assert_eq!(
        v.seer(object_id(21712)).map(|s| (s.player, s.capture)),
        Some((PlayerId::new(1), 0))
    );
    assert_eq!(
        [t.players[0].attributes[40], t.players[1].attributes[40]],
        crate::test_ids::amounts([0.0, 1.0])
    );
    // player 1 lacks the master: the loop ends with it, and Gaia keeps the archer
    let mut masters = hands_masters();
    masters[1] = Masters::new([]);
    let mut t = world(masters, false, 0);
    t.tick(Millis::new(100));
    assert_eq!(t.players[0].objects[ListIndex::new(0)].capture_flag, 1);
    assert_eq!(
        t.spatial.get(object_id(21712)).map(|p| p.player),
        Some(PlayerId::new(0))
    );
    // a computer player 1 in a scenario passes it to player 2
    let mut t = world(hands_masters(), true, 3);
    t.tick(Millis::new(100));
    assert_eq!(t.players[2].objects.len(), 1);
    assert_eq!(
        t.spatial.get(object_id(21712)).map(|p| p.player),
        Some(PlayerId::new(2))
    );
    // unless it allows taking Gaia's units
    let mut t = world(hands_masters(), true, 3);
    t.allow_gaia_capture = vec![false, true, false];
    t.tick(Millis::new(100));
    assert_eq!(
        t.spatial.get(object_id(21712)).map(|p| p.player),
        Some(PlayerId::new(1))
    );
}

/// A world for the teardowns: player 1 holds the sheep 20 (master 705), the bird 30 (96),
/// the villager 40 (83) and town centres 9149 and 9150 (109, one worker each); player 2 (an
/// ally) holds the foundation 10028 (70) with one builder.
fn teardown_world() -> World {
    let walker = crate::gathering::hunt::tests::walker();
    let unit = |id, master| Unit {
        id,
        body: crate::objects::layers::Body::new(
            Position::new(10.5, 10.5, 0.0),
            None,
            Some(walker.clone()),
        ),
        life: Lifecycle {
            master_id: master,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let tc = |id| Unit {
        id,
        body: crate::objects::layers::Body::at(Position::new(20.0, 20.0, 0.0)),
        workers: 1,
        combat: true,
        life: Lifecycle {
            master_id: ids::TOWN_CENTRE,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let foundation = Unit {
        id: object_id(10028),
        body: crate::objects::layers::Body::at(Position::new(30.0, 30.0, 0.0)),
        workers: 1,
        life: Lifecycle {
            state: ObjectState::Foundation,
            master_id: ids::HOUSE,
            ..Lifecycle::default()
        },
        construction: Some(Construction {
            progress: Seconds::ZERO,
            desolid: false,
            builders: 1,
            hit_points: 550,
            build_time: 25,
            record: crate::map::spatial::Obstruction {
                kind: ObstructionType::TYPE_2,
                class: ObstructionClass::UNIT,
            },
            snow: false,
            sprite_override: false,
        }),
        ..Unit::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        1,
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO, Millis::ZERO],
        vec![
            Player::default(),
            Player {
                objects: vec![
                    unit(object_id(20), master_id(705)),
                    unit(object_id(30), master_id(96)),
                    unit(object_id(40), VILLAGER_MALE),
                    tc(object_id(9149)),
                    tc(object_id(9150)),
                ]
                .into(),
                relations: vec![Relation::ALLY, Relation::NEUTRAL, Relation::from(2)],
                ..Player::default()
            },
            Player {
                objects: vec![foundation].into(),
                relations: vec![Relation::ALLY, Relation::from(2), Relation::NEUTRAL],
                ..Player::default()
            },
        ],
    );
    // no sprites: a MoveTo the masters make shows none
    let master = |id, kind, class| Master {
        id,
        kind,
        class,
        move_sprite: None,
        standing_sprite: None,
        ..Master::default()
    };
    let table = Masters::new([
        master(master_id(705), master_type::COMBAT, class::LIVESTOCK),
        master(master_id(96), master_type::COMBAT, class::CLASS_11),
        master(VILLAGER_MALE, master_type::COMBAT, class::VILLAGER),
        master(ids::TOWN_CENTRE, BUILDING, class::TOWER),
        master(ids::HOUSE, BUILDING, class::BUILDING),
    ]);
    t.masters = vec![table.clone(), table.clone(), table];
    t
}

/// The action list's clearing (`FUN_140a98620`): the Artifact's teardown (`FUN_140aab0d0`)
/// lets its town centre go and its `MoveTo`'s stops the sheep; the Fly's (`FUN_140a84750`)
/// gives the bird its top speed as its animation speed, then the base teardown's `MoveTo`
/// stops it (`+0x7b8` 0 at once; without a `MoveTo` the top speed stays); the
/// Enter's lets its building go; a counted builder's Build leaves another player's
/// foundation's builders (`World::leave_builders`) and lets it go; a wait for a foundation
/// goes.
#[test]
fn clearing_the_actions_tears_the_artifact_fly_enter_and_build_down() {
    let mut t = teardown_world();
    t.players[1].objects[ListIndex::new(0)]
        .actions
        .set_artifact(Some(Artifact {
            target: Some(object_id(9149)),
            goal: Some(Position::new(20.0, 20.0, 0.0)),
            move_to: Some(MoveToAction {
                point: Position::ORIGIN,
                target: Some(object_id(9149)),
                range: Distance::new(1.0),
                ..MoveToAction::sub_in(MoveTo::Moving)
            }),
            ..Artifact::new(false, None)
        }));
    t.clear_actions(PlayerId::new(1), ListIndex::new(0))
        .unwrap();
    let sheep = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(sheep.actions.artifact().copied(), None);
    assert_eq!(sheep.body.mover().unwrap().animated.speed, 0.0);
    assert_eq!(t.players[1].objects[ListIndex::new(3)].workers, 0);
    let bird = &mut t.players[1].objects[ListIndex::new(1)];
    bird.actions.set_fly(Some(Fly {
        state: FlyState::Choosing,
        timer: Seconds::new(1.0),
        move_to: Some(MoveToAction {
            state: MoveTo::Moving,
            quiet: false,
            ..MoveToAction::sub_to_point(Position::new(12.0, 12.0, 0.0), Distance::ZERO, None)
        }),
        home: Point::new(10.0, 10.0),
        task: crate::objects::masters::Task::default(),
    }));
    bird.body.mover_mut().unwrap().stats.speed = Speed::new(1.3);
    let flying = bird.actions.fly().copied();
    t.clear_actions(PlayerId::new(1), ListIndex::new(1))
        .unwrap();
    let bird = &mut t.players[1].objects[ListIndex::new(1)];
    assert_eq!(
        (bird.actions.fly().copied(), bird.body.anim_speed()),
        (None, Some(0.0))
    );
    bird.actions
        .set_fly(flying.map(|f| Fly { move_to: None, ..f }));
    t.clear_actions(PlayerId::new(1), ListIndex::new(1))
        .unwrap();
    let bird = &t.players[1].objects[ListIndex::new(1)];
    assert_eq!(
        (bird.actions.fly().copied(), bird.body.anim_speed()),
        (None, Some(1.3))
    );
    let villager = &mut t.players[1].objects[ListIndex::new(2)];
    villager
        .actions
        .set_enter(Some(crate::garrison::EnterAction {
            target: Some(object_id(9150)),
            state: crate::garrison::EnterState::Wait,
            move_to: None,
            point: Position::new(20.0, 20.0, 0.0),
            range: Distance::ZERO,
            timer: Seconds::ZERO,
            retry: true,
            task_class: Some(class_id(0)),
            moving: None,
        }));
    // a walking villager: an Enter without a MoveTo does not stop it
    villager.body.set_anim_speed(1.0);
    t.clear_actions(PlayerId::new(1), ListIndex::new(2))
        .unwrap();
    let villager = &mut t.players[1].objects[ListIndex::new(2)];
    assert_eq!(villager.actions.enter().copied(), None);
    assert_ne!(villager.body.mover().unwrap().animated.speed, 0.0);
    villager.actions.set_wait_solidify(Some(
        crate::construction::wait_solidify::WaitSolidify::new(
            crate::objects::masters::Task::default(),
            object_id(10028),
        ),
    ));
    t.clear_actions(PlayerId::new(1), ListIndex::new(2))
        .unwrap();
    let villager = &t.players[1].objects[ListIndex::new(2)];
    assert_eq!(villager.actions.wait_solidify().copied(), None);
    assert_eq!(t.players[1].objects[ListIndex::new(4)].workers, 0);
    t.players[1].objects[ListIndex::new(2)]
        .actions
        .set_build(Some(BuildAction {
            target: Some(object_id(10028)),
            state: BuildState::Idle,
            move_to: None,
            point: Position::new(-1.0, -1.0, -1.0),
            task: BuildTask {
                range: Distance::ZERO,
                work_value: 0.0,
                work_rate: WorkRate::new(1.0),
                resource: None,
                id: TaskId::of_task(-1),
                moving: None,
            },
            added: true,
            timer: Seconds::ZERO,
            site: None,
        }));
    t.clear_actions(PlayerId::new(1), ListIndex::new(2))
        .unwrap();
    let foundation = &t.players[2].objects[ListIndex::new(0)];
    assert_eq!(foundation.workers, 0);
    assert_eq!(foundation.construction.map(|k| k.builders), Some(0));
}

/// `FUN_1411e63f0` then `FUN_1411e6580` at a built building's death: each storage entry of mode
/// 4 with an attribute and an amount above 0 leaves its owner (negated), then the population
/// the object holds itself (attribute 4: it holds nothing after); each entry of mode 16 (a flag
/// bit 0x10) leaves its own store when the store holds its attribute.
#[test]
fn a_built_building_s_storage_leaves_its_owner_and_its_store() {
    let master = Master {
        id: ids::TOWN_CENTRE,
        kind: BUILDING,
        storage: vec![
            (Some(attribute::HEADROOM), Amount::new(5.0), 4),
            (None, Amount::new(5.0), 4),
            (Some(attribute::RELICS_HELD), Amount::ZERO, 4),
            (Some(attribute::POPULATION), Amount::new(2.0), 16),
            (Some(AttributeId::numbered(12)), Amount::new(3.0), 0x30),
            (Some(AttributeId::numbered(13)), Amount::new(1.0), 16),
        ],
        ..Master::default()
    };
    let mut u = Unit {
        id: object_id(1),
        store: vec![
            (attribute::POPULATION, Amount::new(6.0)),
            (AttributeId::numbered(12), Amount::new(4.0)),
        ],
        ..Unit::default()
    };
    u.life.attribute_type = Some(attribute::HEADROOM);
    u.life.amount = Amount::new(3.0);
    let adds = storage_leaves(&mut u, &master);
    assert_eq!(
        adds,
        crate::test_ids::adds([(attribute::HEADROOM, -5.0), (attribute::HEADROOM, -3.0)])
    );
    assert_eq!(u.life.held(), carrying(None, 0.0));
    assert_eq!(
        u.store,
        crate::test_ids::adds([
            (attribute::POPULATION, 4.0),
            (AttributeId::numbered(12), 1.0)
        ])
    );
    // holding no population: the master's entries alone
    let adds = storage_leaves(&mut u, &master);
    assert_eq!(adds, crate::test_ids::adds([(attribute::HEADROOM, -5.0)]));
    assert_eq!(
        u.store,
        crate::test_ids::adds([
            (attribute::POPULATION, 2.0),
            (AttributeId::numbered(12), -2.0)
        ])
    );
}

/// A world for the deaths: Gaia holds the tree 50 (master 349, type 15, one worker) and the boar
/// 60 (master 48, type 70, one worker, a unit AI that counts the hunter 41 among its attackers);
/// player 1 holds the villagers 40 (gathering the tree), 41 (hunting the boar) and 42 (a counted
/// builder of its foundation 10028, one builder, one worker), each with hit points 0.
fn deaths_world() -> World {
    let dead = |id| Unit {
        id,
        body: crate::objects::layers::Body::at(Position::new(10.5, 10.5, 0.0)),
        life: Lifecycle {
            class: Class::Animated,
            hp: HitPoints::ZERO,
            master_id: VILLAGER_MALE,
            master: Some(MasterData::default()),
            ..Lifecycle::default()
        },
        pasture: None,
        my_pasture: None,
        ..Unit::default()
    };
    let chop = crate::gathering::hunt::tests::hunting(50).task;
    let tree = crate::gathering::gather::Resource {
        id: object_id(50),
        position: Position::new(12.5, 10.5, 0.0),
        class: class::TREE,
        master: master_id(349),
        player: PlayerId::new(0),
        workers: 0,
        drop_site: false,
        gap: Distance::new(1.0),
        state: ObjectState::Alive,
        radius: Footprint::new(0.5, 0.5),
    };
    let rate = crate::gathering::gather::StopRate {
        work_rate: WorkRate::new(1.0),
        capacity: Amount::new(10.0),
        productivity: 1.0,
    };
    let gather = crate::gathering::gather::GatherAction::new(chop, &tree, false, rate).unwrap();
    let mut villagers = vec![
        dead(object_id(40)),
        dead(object_id(41)),
        dead(object_id(42)),
    ];
    villagers[0]
        .actions
        .set_gather(Some(crate::gathering::gather::GatherAction {
            state: GatherState::Work,
            ..gather
        }));
    villagers[1]
        .actions
        .set_hunt(Some(crate::gathering::hunt::tests::hunting(60)));
    villagers[2].actions.set_build(Some(BuildAction {
        target: Some(object_id(10028)),
        state: BuildState::Place,
        move_to: None,
        point: Position::new(-1.0, -1.0, -1.0),
        task: BuildTask {
            range: Distance::ZERO,
            work_value: 1.0,
            work_rate: WorkRate::new(1.0),
            resource: None,
            id: TaskId::of_task(-1),
            moving: None,
        },
        added: true,
        timer: Seconds::ZERO,
        site: None,
    }));
    let gaia = vec![
        Unit {
            id: object_id(50),
            body: crate::objects::layers::Body::at(Position::new(12.5, 10.5, 0.0)),
            workers: 1,
            life: Lifecycle {
                master_id: master_id(349),
                ..Lifecycle::default()
            },
            ..Unit::default()
        },
        Unit {
            id: object_id(60),
            body: crate::objects::layers::Body::at(Position::new(14.5, 10.5, 0.0)),
            workers: 1,
            combat: true,
            ai: Some(UnitAi {
                attackers: Attackers::of_saved(&[41]),
                ..building()
            }),
            life: Lifecycle {
                class: Class::Animated,
                master_id: master_id(48),
                ..Lifecycle::default()
            },
            ..Unit::default()
        },
    ];
    let foundation = Unit {
        id: object_id(10028),
        body: crate::objects::layers::Body::at(Position::new(30.0, 30.0, 0.0)),
        workers: 1,
        life: Lifecycle {
            state: ObjectState::Foundation,
            master_id: ids::HOUSE,
            ..Lifecycle::default()
        },
        construction: Some(Construction {
            progress: Seconds::ZERO,
            desolid: false,
            builders: 1,
            hit_points: 550,
            build_time: 25,
            record: crate::map::spatial::Obstruction {
                kind: ObstructionType::TYPE_2,
                class: ObstructionClass::UNIT,
            },
            snow: false,
            sprite_override: false,
        }),
        ..Unit::default()
    };
    // player 1's turn: the boar's unit AI keeps its attackers
    let mut t = World::new(
        WorldTime::ZERO,
        1,
        PlayerId::new(1),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player {
                objects: gaia.into(),
                ..Player::default()
            },
            Player {
                objects: villagers.into_iter().chain([foundation]).collect(),
                relations: relation_row([0, 1]),
                attributes: vec![Amount::ZERO; 250],
                ..Player::default()
            },
        ],
    );
    // no dying graphic (-1): slot 0x7a0 gives the master's, which these masters lack
    let master = |id, kind, class| Master {
        id,
        kind,
        class,
        death_sprite: None,
        ..Master::default()
    };
    let table = Masters::new([
        master(master_id(349), master_type::TREE, class::TREE),
        master(master_id(48), master_type::COMBAT, class::PREY),
        master(VILLAGER_MALE, master_type::COMBAT, class::VILLAGER),
        master(ids::HOUSE, BUILDING, class::BUILDING),
    ]);
    t.masters = vec![table.clone(), table];
    t
}

/// `RGE_Action_Object::update` (`0x14106ac90`) deletes a dead unit's action list in the update
/// of its death (`FUN_140a98620`), before any action's update: the Gather lets its tree go (no
/// worker left, it goes to sleep), the Hunt lets its boar go (notify 699 from the hunter, its
/// class 4), the Build takes its builder out of the foundation's builders and lets it go.
#[test]
fn a_dead_unit_s_actions_go_and_let_their_targets_go_in_the_update_of_its_death() {
    let mut t = deaths_world();
    t.tick(Millis::new(100));
    assert_eq!(
        (
            t.players[0].unsupported.clone(),
            t.players[1].unsupported.clone()
        ),
        (None, None)
    );
    let villagers = &t.players[1].objects;
    for v in &villagers.as_slice()[..3] {
        assert!(ObjectState::Dead <= v.life.state, "{} died", v.id);
        assert_eq!(v.top_action(), None, "{} lost its actions", v.id);
    }
    let tree = &t.players[0].objects[ListIndex::new(0)];
    assert_eq!((tree.workers, tree.life.goto_sleep), (0, true));
    let boar = &t.players[0].objects[ListIndex::new(1)];
    assert_eq!(boar.workers, 0);
    let notify = boar.ai.as_ref().unwrap().notifies.last().copied();
    assert_eq!(
        notify.map(|n| (n.caller, n.kind, n.p[0])),
        Some((Some(object_id(41)), NotifyKind::NO_LONGER_ON_ME, 4))
    );
    let foundation = &villagers[ListIndex::new(3)];
    assert_eq!(foundation.workers, 0);
    assert_eq!(foundation.construction.map(|k| k.builders), Some(0));
}

/// `FUN_140aaddb0`: a Build counts its builder in a building annex's main building (`+0xbb8`),
/// in any other object itself, and in none once the world no longer holds its target; a dead
/// builder of the annex leaves the main building's builders.
#[test]
fn a_build_on_an_annex_counts_its_builder_in_the_main_building() {
    let mut t = deaths_world();
    let mut main = t.players[1].objects[ListIndex::new(3)].clone();
    main.id = object_id(10030);
    t.players[1].objects[ListIndex::new(3)].annex_of = Some(object_id(10030));
    t.players[1].objects[ListIndex::new(3)]
        .construction
        .as_mut()
        .unwrap()
        .builders = 0;
    t.players[1].objects.push(main);
    assert_eq!(
        t.counted_building(object_id(10028)),
        Ok(Some(object_id(10030)))
    );
    assert_eq!(t.counted_building(object_id(50)), Ok(Some(object_id(50))));
    assert_eq!(t.counted_building(object_id(999)), Ok(None));
    t.tick(Millis::new(100));
    assert_eq!(
        (
            t.players[0].unsupported.clone(),
            t.players[1].unsupported.clone()
        ),
        (None, None)
    );
    let builders = |id| {
        t.players[1]
            .objects
            .iter()
            .find(|u| u.id == id)
            .and_then(|u| u.construction)
            .map(|k| k.builders)
    };
    assert_eq!(
        (builders(object_id(10028)), builders(object_id(10030))),
        (Some(0), Some(0))
    );
}

/// A `MoveTo` of its own to an object takes the object (`FUN_140aa0df0` with quiet 0, then
/// `FUN_140a7a230`: slot 0x178 wakes the sleeping tree 50) and lets it go at every end: another
/// move to an object takes the new one before the clearing's teardown (`0x140aa1f30`, then the
/// base teardown `FUN_140a73fc0`) lets the old one go (no worker left on the tree: it is flagged
/// to sleep), and a clearing of the list lets the town centre go.
#[test]
fn a_move_to_an_object_takes_it_and_lets_it_go_when_it_is_replaced_or_cleared() {
    let mut t = teardown_world();
    let tree = Unit {
        id: object_id(50),
        body: crate::objects::layers::Body::at(Position::new(12.5, 10.5, 0.0)),
        life: Lifecycle {
            master_id: master_id(349),
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    t.players[0].sleeping.push(tree);
    for m in &mut t.masters {
        *m = Masters::new(m.iter().cloned().chain([Master {
            id: master_id(349),
            kind: master_type::TREE,
            class: class::TREE,
            ..Master::default()
        }]));
    }
    let placed = |x: f32, class, master| Placed {
        position: Position::new(x, 10.5, 0.0),
        radius: Footprint::new(0.5, 0.5),
        class,
        player: PlayerId::new(0),
        master,
        obstruction_type: ObstructionType::NONE,
        buildable_on: 0,
        tall: true,
        speed: Speed::ZERO,
        unknown: None,
        obstruction: None,
    };
    t.spatial
        .insert(object_id(50), placed(12.5, class::TREE, master_id(349)));
    t.spatial.insert(
        object_id(9149),
        placed(20.0, class::TOWER, ids::TOWN_CENTRE),
    );
    t.move_to_object(
        PlayerId::new(1),
        ListIndex::new(2),
        object_id(50),
        Position::new(12.5, 10.5, 0.0),
    )
    .unwrap();
    assert_eq!(t.players[0].sleeping.len(), 0);
    let tree = &t.players[0].objects[ListIndex::new(0)];
    assert_eq!((tree.id.get(), tree.workers), (50, 1));
    assert_eq!(
        t.players[1].objects[ListIndex::new(2)]
            .actions
            .move_to()
            .map(|m| ObjectId::field(m.target)),
        Some(50)
    );
    t.move_to_object(
        PlayerId::new(1),
        ListIndex::new(2),
        object_id(9149),
        Position::new(20.0, 10.5, 0.0),
    )
    .unwrap();
    let tree = &t.players[0].objects[ListIndex::new(0)];
    assert_eq!((tree.workers, tree.life.goto_sleep), (0, true));
    assert_eq!(t.players[1].objects[ListIndex::new(3)].workers, 2);
    t.clear_actions(PlayerId::new(1), ListIndex::new(2))
        .unwrap();
    assert_eq!(t.players[1].objects[ListIndex::new(3)].workers, 1);
}

/// A `MoveTo` of its own that ends (its update in state 1 returns 1, the action list removes it,
/// `FUN_140a98e20`) lets its target go (the base teardown, slot 0x180); one that dies with the
/// unit does too (`FUN_140a98620`).
#[test]
fn a_move_to_an_object_lets_it_go_when_it_ends_or_its_unit_dies() {
    let mut t = deaths_world();
    t.players[0].objects[ListIndex::new(0)].workers = 2;
    let mover = crate::gathering::hunt::tests::walker();
    let walker = |id| Unit {
        id,
        body: crate::objects::layers::Body::new(
            Position::new(11.5, 10.5, 0.0),
            None,
            Some(mover.clone()),
        ),
        actions: ActionList::default().with_move_to(Some(MoveToAction {
            state: MoveTo::Arrived,
            point: Position::ORIGIN,
            target: Some(object_id(50)),
            range: Distance::ZERO,
            sprite: None,
            quiet: false,
            on_target_moved: crate::movement::move_to::TargetMoved::Follows,
            wait: crate::movement::move_to::MoveWait::default(),
        })),
        pasture: None,
        my_pasture: None,
        life: Lifecycle {
            class: Class::Animated,
            master_id: VILLAGER_MALE,
            master: Some(MasterData::default()),
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let dead = Unit {
        life: Lifecycle {
            hp: HitPoints::ZERO,
            ..walker(object_id(43)).life
        },
        ..walker(object_id(43))
    };
    t.players[1].objects = vec![walker(object_id(44)), dead].into();
    t.spatial.insert(
        object_id(50),
        Placed {
            position: Position::new(12.5, 10.5, 0.0),
            radius: Footprint::new(0.5, 0.5),
            class: class::TREE,
            player: PlayerId::new(0),
            master: master_id(349),
            obstruction_type: ObstructionType::NONE,
            buildable_on: 0,
            tall: true,
            speed: Speed::ZERO,
            unknown: None,
            obstruction: None,
        },
    );
    t.tick(Millis::new(100));
    assert_eq!(
        (
            t.players[0].unsupported.clone(),
            t.players[1].unsupported.clone()
        ),
        (None, None)
    );
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .move_to()
            .copied(),
        None
    );
    assert_eq!(
        t.players[1].objects[ListIndex::new(1)]
            .actions
            .move_to()
            .copied(),
        None
    );
    let tree = &t.players[0].objects[ListIndex::new(0)];
    assert_eq!((tree.workers, tree.life.goto_sleep), (0, true));
}

/// A `MoveTo` of its own checks its target first (`0x140aa2a70`,
/// [`MoveToAction::check_held`]): a dying one (state 7) is let go and released (slot 0x180: the
/// tree counts a worker less), and the list keeps the `MoveTo` (code 4), now to its point; one
/// the world no longer holds is cleared, with nothing released.
#[test]
fn a_move_to_of_its_own_lets_a_dying_target_go_and_forgets_a_lost_one() {
    let run = |target: i32, tree_state: ObjectState| {
        let mut t = deaths_world();
        t.players[0].objects[ListIndex::new(0)].workers = 2;
        t.players[0].objects[ListIndex::new(0)].life.state = tree_state;
        let mover = crate::gathering::hunt::tests::walker();
        let walker = Unit {
            id: object_id(44),
            body: crate::objects::layers::Body::new(
                Position::new(11.5, 10.5, 0.0),
                None,
                Some(mover),
            ),
            actions: ActionList::default().with_move_to(Some(MoveToAction {
                state: MoveTo::Waiting,
                point: Position::new(12.5, 10.5, 0.0),
                target: ObjectId::of_field(target),
                range: Distance::ZERO,
                sprite: None,
                quiet: false,
                on_target_moved: crate::movement::move_to::TargetMoved::Follows,
                wait: crate::movement::move_to::MoveWait {
                    state: MoveWaitState::Waiting,
                    timer: Seconds::ZERO,
                    delay: Seconds::new(60.0),
                },
            })),
            pasture: None,
            my_pasture: None,
            life: Lifecycle {
                class: Class::Animated,
                master_id: VILLAGER_MALE,
                master: Some(MasterData::default()),
                ..Lifecycle::default()
            },
            ..Unit::default()
        };
        t.players[1].objects = vec![walker].into();
        t.tick(Millis::new(100));
        assert_eq!(t.players[1].unsupported.clone(), None);
        let m = t.players[1].objects[ListIndex::new(0)]
            .actions
            .move_to()
            .copied();
        let workers = t.players[0].objects[ListIndex::new(0)].workers;
        (m.map(|m| (m.state, ObjectId::field(m.target))), workers)
    };
    assert_eq!(
        run(50, ObjectState::Removed),
        (Some((MoveTo::Waiting, -1)), 1)
    );
    assert_eq!(
        run(51, ObjectState::Alive),
        (Some((MoveTo::Waiting, -1)), 2)
    );
    assert_eq!(
        run(50, ObjectState::Alive),
        (Some((MoveTo::Waiting, 50)), 2)
    );
}

/// The hunt's `set_state(8)` (`0x140ac60e0` case 8): without a second target its search
/// (`FUN_1411671e0`; without planners by distance) finds the town centre, which becomes the
/// second target (`FUN_140a7a190`): the caller takes it (slot 0x178). With the drop site held
/// already it walks there and takes nothing new.
#[test]
fn a_hunter_s_drop_off_takes_the_drop_site_it_found() {
    let mut t = World::new(
        WorldTime::ZERO,
        1,
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![Player::default(), Player::default()],
    );
    placeable(&mut t);
    let mut walker = crate::gathering::hunt::tests::walker();
    walker.stats.restriction = RestrictionId::named(0);
    let mut hunter = Unit {
        id: object_id(40),
        body: crate::objects::layers::Body::new(Position::new(10.5, 10.5, 0.0), None, Some(walker)),
        actions: ActionList::default().with_hunt(Some(crate::gathering::hunt::tests::hunting(-1))),
        life: Lifecycle {
            master_id: VILLAGER_MALE,
            attribute_type: Some(attribute::FOOD),
            amount: Amount::new(10.0),
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let tc = Unit {
        id: object_id(9149),
        body: crate::objects::layers::Body::at(Position::new(20.0, 20.0, 0.0)),
        life: Lifecycle {
            master_id: ids::TOWN_CENTRE,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let masters = Masters::new([
        Master {
            id: VILLAGER_MALE,
            kind: master_type::COMBAT,
            class: class::VILLAGER,
            radius: Footprint::new(0.2, 0.2),
            drop_sites: vec![ids::TOWN_CENTRE],
            // no sprites: the sprites' table holds none
            move_sprite: None,
            standing_sprite: None,
            ..Master::default()
        },
        Master {
            id: ids::TOWN_CENTRE,
            kind: BUILDING,
            class: class::TOWER,
            radius: Footprint::new(2.0, 2.0),
            ..Master::default()
        },
    ]);
    let placed = |x: f32, radius: f32, class, master| Placed {
        position: Position::new(x, x, 0.0),
        radius: Footprint::new(radius, radius),
        class,
        player: PlayerId::new(1),
        master,
        obstruction_type: ObstructionType::NONE,
        buildable_on: 0,
        tall: true,
        speed: Speed::ZERO,
        unknown: None,
        obstruction: None,
    };
    t.spatial.insert(
        object_id(40),
        placed(10.5, 0.2, class::VILLAGER, VILLAGER_MALE),
    );
    t.spatial.insert(
        object_id(9149),
        placed(20.0, 2.0, class::TOWER, ids::TOWN_CENTRE),
    );
    let others = ObjectList::from(vec![tc]);
    let drop_off = |hunter: &mut Unit, t: &mut World| {
        let world = DropWorld {
            masters: Some(&masters),
            spatial: &mut t.spatial,
            zones: &mut t.zones,
            pathing: &mut t.pathing,
            object: t.streams.get(Stream::Object),
            world_time: WorldTime::ZERO,
            computer: false,
            construction_rate: 1.0,
            groups: &t.groups,
        };
        hunter.drop_off_in(
            world,
            OthersRef::alone(PlayerId::new(1), &others),
            &t.graphics,
        )
    };
    assert_eq!(drop_off(&mut hunter, &mut t), Ok(Some(object_id(9149))));
    let hunt = hunter.actions.hunt().copied().unwrap();
    assert_eq!(
        (hunt.state, hunt.carry_to),
        (HuntState::Drop, Some(object_id(9149)))
    );
    assert_eq!(drop_off(&mut hunter, &mut t), Ok(None));
}

/// The Hunt's update (`0x140ac7a00`) first checks its second target: a drop site past state 6
/// is let go (`FUN_140a7a190` with none: slot 0x180) and dropped, one the world's table no longer
/// holds only dropped; the teardown (`FUN_140a73fc0`) of a dead hunter's Hunt and of a failed
/// one (state 13, removed by `FUN_140a98e20`) lets its drop site go too.
#[test]
fn a_hunt_lets_its_drop_site_go_when_it_dies_or_the_hunter_does() {
    let mut t = deaths_world();
    let site = |state| Unit {
        id: object_id(9149),
        body: crate::objects::layers::Body::at(Position::new(20.0, 20.0, 0.0)),
        workers: 1,
        life: Lifecycle {
            state,
            master_id: ids::HOUSE,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    t.players[1].objects.push(site(ObjectState::Alive));
    // the dead hunter 41's Hunt walks its load to the town centre
    t.players[1].objects[ListIndex::new(1)]
        .actions
        .hunt_mut()
        .unwrap()
        .carry_to = Some(object_id(9149));
    t.tick(Millis::new(100));
    assert_eq!(
        (
            t.players[0].unsupported.clone(),
            t.players[1].unsupported.clone()
        ),
        (None, None)
    );
    let workers = |t: &World| {
        t.players[1]
            .objects
            .iter()
            .find(|u| u.id == object_id(9149))
            .unwrap()
            .workers
    };
    assert_eq!(workers(&t), 0);
    // a live hunter waiting to search (state 3) whose drop site is past state 6 (the world's
    // table holds it still); one whose drop site the table no longer holds only drops it
    let mut t = deaths_world();
    t.players[1].objects.push(site(ObjectState::Removed));
    t.spatial.insert(
        object_id(9149),
        Placed {
            position: Position::new(20.0, 20.0, 0.0),
            radius: Footprint::new(2.0, 2.0),
            class: class::BUILDING,
            player: PlayerId::new(1),
            master: ids::HOUSE,
            obstruction_type: ObstructionType::TYPE_2,
            buildable_on: 0,
            tall: true,
            speed: Speed::ZERO,
            unknown: None,
            obstruction: None,
        },
    );
    let hunter = &mut t.players[1].objects[ListIndex::new(1)];
    hunter.life.hp = HitPoints::new(1.0);
    hunter
        .actions
        .set_hunt(Some(crate::gathering::hunt::HuntAction {
            state: HuntState::Search,
            target: None,
            carry_to: Some(object_id(9149)),
            ..crate::gathering::hunt::tests::hunting(-1)
        }));
    let mut gone = t.clone();
    gone.spatial = Spatial::default();
    t.tick(Millis::new(100));
    assert_eq!(
        (
            t.players[0].unsupported.clone(),
            t.players[1].unsupported.clone()
        ),
        (None, None)
    );
    assert_eq!(workers(&t), 0);
    assert_eq!(
        t.players[1].objects[ListIndex::new(1)]
            .actions
            .hunt()
            .map(|h| h.carry_to),
        Some(None)
    );
    gone.tick(Millis::new(100));
    assert_eq!(workers(&gone), 1);
    assert_eq!(
        gone.players[1].objects[ListIndex::new(1)]
            .actions
            .hunt()
            .map(|h| h.carry_to),
        Some(None)
    );
    // a failed hunt (state 13) is removed: its teardown lets the drop site go too
    let mut failed = deaths_world();
    failed.players[1].objects.push(site(ObjectState::Alive));
    failed.spatial = t.spatial.clone();
    let hunter = &mut failed.players[1].objects[ListIndex::new(1)];
    hunter.life.hp = HitPoints::new(1.0);
    hunter
        .actions
        .set_hunt(Some(crate::gathering::hunt::HuntAction {
            state: HuntState::Failed,
            target: None,
            carry_to: Some(object_id(9149)),
            ..crate::gathering::hunt::tests::hunting(-1)
        }));
    failed.tick(Millis::new(100));
    assert_eq!(failed.players[1].unsupported, None);
    assert_eq!(workers(&failed), 0);
    assert_eq!(
        failed.players[1].objects[ListIndex::new(1)]
            .actions
            .hunt()
            .copied(),
        None
    );
}

/// A Gather's and a Hunt's second target at the start of their updates (`0x140a8e1d0`,
/// `0x140ac7a00`): a drop site past state 6 is let go and goes, one the world's table lacks only
/// goes, one in state 2 (or 6) stays.
#[test]
fn a_second_target_past_state_6_is_let_go_and_one_gone_dropped() {
    use crate::objects::unit::second_target_checked;
    assert_eq!(
        second_target_checked(Some(object_id(9149)), true, Some(ObjectState::Removed)),
        (None, Some(object_id(9149)))
    );
    assert_eq!(
        second_target_checked(Some(object_id(9149)), true, Some(ObjectState::Undead)),
        (Some(object_id(9149)), None)
    );
    assert_eq!(
        second_target_checked(Some(object_id(9149)), true, Some(ObjectState::Alive)),
        (Some(object_id(9149)), None)
    );
    assert_eq!(
        second_target_checked(Some(object_id(9149)), false, None),
        (None, None)
    );
    assert_eq!(second_target_checked(None, false, None), (None, None));
}

/// A foundation's attacking units (`+0xce0`): slot 0x178 of a building (`FUN_1411dba70`) below
/// state 2 whose master has a stack unit (`+0x986`) lists each taker of a player its owner is
/// not allied with, once per take; slot 0x180 (`FUN_1411d6020`) drops the releaser's first entry,
/// the others keeping their order. An ally's take, a built building's and one of a master
/// without a stack unit list nothing. Both still count the workers.
#[test]
fn a_foundation_lists_its_unallied_takers_and_drops_them_when_they_let_it_go() {
    let unit = |id, master| Unit {
        id,
        life: Lifecycle {
            master_id: master,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let foundation = |id, master| Unit {
        life: Lifecycle {
            state: ObjectState::Foundation,
            ..unit(id, master).life
        },
        combat: true,
        ..unit(id, master)
    };
    let mut t = World::new(
        WorldTime::ZERO,
        1,
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO, Millis::ZERO, Millis::ZERO],
        vec![
            Player::default(),
            Player {
                objects: vec![
                    foundation(object_id(10028), ids::TOWN_CENTRE_621),
                    foundation(object_id(10029), ids::HOUSE),
                ]
                .into(),
                relations: vec![
                    Relation::ALLY,
                    Relation::ALLY,
                    Relation::from(2),
                    Relation::RELATION_4,
                ],
                ..Player::default()
            },
            Player {
                objects: vec![
                    unit(object_id(40), VILLAGER_MALE),
                    unit(object_id(41), VILLAGER_MALE),
                ]
                .into(),
                relations: vec![
                    Relation::ALLY,
                    Relation::from(2),
                    Relation::ALLY,
                    Relation::from(2),
                ],
                ..Player::default()
            },
            Player {
                objects: vec![unit(object_id(50), VILLAGER_MALE)].into(),
                relations: vec![
                    Relation::ALLY,
                    Relation::RELATION_4,
                    Relation::from(2),
                    Relation::ALLY,
                ],
                ..Player::default()
            },
        ],
    );
    let building = |id: i16, stack_unit: i16| Master {
        id: master_id(id),
        kind: BUILDING,
        class: class::BUILDING,
        building: Some(crate::objects::masters::BuildingMaster {
            stack_unit: opt_master(stack_unit),
            ..crate::objects::masters::BuildingMaster::default()
        }),
        ..Master::default()
    };
    let table = Masters::new([
        building(621, 109),
        building(70, -1),
        Master {
            id: VILLAGER_MALE,
            kind: master_type::COMBAT,
            class: class::VILLAGER,
            ..Master::default()
        },
    ]);
    t.masters = vec![table.clone(), table.clone(), table.clone(), table];
    let by = |t: &World, p, i| t.releaser(p, i).unwrap();
    let (enemy, other, ally) = (
        by(&t, PlayerId::new(2), ListIndex::new(0)),
        by(&t, PlayerId::new(2), ListIndex::new(1)),
        by(&t, PlayerId::new(3), ListIndex::new(0)),
    );
    for taker in [enemy, other, enemy, ally] {
        t.take(object_id(10028), taker).unwrap();
        t.take(object_id(10029), taker).unwrap();
    }
    let site = |t: &World| t.players[1].objects[ListIndex::new(0)].clone();
    assert_eq!(
        site(&t).attacking_units,
        [object_id(40), object_id(41), object_id(40)]
    );
    assert_eq!(site(&t).workers, 4);
    assert_eq!(
        t.players[1].objects[ListIndex::new(1)].attacking_units,
        Vec::new()
    );
    t.release(object_id(10028), enemy).unwrap();
    assert_eq!(
        (site(&t).attacking_units, site(&t).workers),
        (vec![object_id(41), object_id(40)], 3)
    );
    t.release(object_id(10028), ally).unwrap();
    assert_eq!(site(&t).attacking_units, [object_id(41), object_id(40)]);
    t.players[1].objects[ListIndex::new(0)].life.state = ObjectState::Alive;
    t.take(object_id(10028), other).unwrap();
    t.release(object_id(10028), enemy).unwrap();
    assert_eq!(site(&t).attacking_units, [object_id(41), object_id(40)]);
}

/// Slot 0x180 by class: the static object's (`FUN_141197d00`, types 10 and 15) flags an object
/// alive without workers left to sleep (not one of NaN hit points: `COMISS`, `JNC`); the animated
/// object's (`FUN_141071ea0`, types 20 to 40) only counts the worker less. A sleeping object
/// (`FUN_1415417a0` finds it) counts one less and stays asleep without the flag.
#[test]
fn only_a_static_object_let_go_by_its_last_worker_goes_to_sleep() {
    let by = Releaser {
        id: object_id(40),
        class: Some(class::VILLAGER),
        player: PlayerId::new(1),
    };
    let let_go = |kind, hp| {
        let mut u = Unit {
            workers: 1,
            life: Lifecycle {
                hp,
                ..Lifecycle::default()
            },
            ..Unit::default()
        };
        released(&mut u, kind, false, by).unwrap();
        (u.workers, u.life.goto_sleep)
    };
    assert_eq!(let_go(master_type::STATIC, HitPoints::new(1.0)), (0, true));
    assert_eq!(let_go(master_type::TREE, HitPoints::new(1.0)), (0, true));
    assert_eq!(
        let_go(master_type::ANIMATED, HitPoints::new(1.0)),
        (0, false)
    );
    assert_eq!(
        let_go(master_type::DOPPELGANGER, HitPoints::new(1.0)),
        (0, false)
    );
    assert_eq!(
        let_go(master_type::STATIC, HitPoints::new(f32::NAN)),
        (0, false)
    );
    let mut t = deaths_world();
    let mut tree = t.players[0].objects.remove(ListIndex::new(0)).unwrap();
    tree.workers = 2;
    t.players[0].sleeping.push(tree);
    t.release(object_id(50), by).unwrap();
    let tree = &t.players[0].sleeping[ListIndex::new(0)];
    assert_eq!((tree.workers, tree.life.goto_sleep), (1, false));
}

/// The Wonder (master 276) keeps its action list up to state 5 (`0x14106ac90`'s threshold 5,
/// any other object's 2).
#[test]
fn only_the_wonder_keeps_its_actions_past_state_2() {
    let life = |master_id, state| Lifecycle {
        state,
        master_id,
        ..Lifecycle::default()
    };
    assert!(!loses_actions(&life(VILLAGER_MALE, ObjectState::Alive)));
    assert!(loses_actions(&life(VILLAGER_MALE, ObjectState::Dying)));
    assert!(!loses_actions(&life(ids::WONDER, ObjectState::Dead)));
    assert!(loses_actions(&life(ids::WONDER, ObjectState::Undead)));
}

/// The Artifact's move to an object (slot 8 with an object, `FUN_140aaca30`): the old target
/// is let go, then the new one taken (slot 0x68 twice, `+0x60` 0).
#[test]
fn an_artifact_s_move_to_an_object_lets_the_old_one_go_and_takes_the_new() {
    let mut t = teardown_world();
    for (id, at) in [(9149, [20.0, 20.0]), (9150, [25.0, 20.0])] {
        t.spatial.insert(
            object_id(id),
            Placed {
                position: Position::new(at[0], at[1], 0.0),
                radius: Footprint::new(2.0, 2.0),
                class: class::TOWER,
                player: PlayerId::new(1),
                master: ids::TOWN_CENTRE,
                obstruction_type: ObstructionType::TYPE_2,
                buildable_on: 0,
                tall: true,
                speed: Speed::ZERO,
                unknown: None,
                obstruction: None,
            },
        );
    }
    t.players[1].objects[ListIndex::new(0)]
        .actions
        .set_artifact(Some(Artifact::new(false, None)));
    t.artifact_take_object(PlayerId::new(1), ListIndex::new(0), object_id(9149))
        .unwrap();
    assert_eq!(t.players[1].objects[ListIndex::new(3)].workers, 2);
    t.artifact_take_object(PlayerId::new(1), ListIndex::new(0), object_id(9150))
        .unwrap();
    assert_eq!(
        [
            t.players[1].objects[ListIndex::new(3)].workers,
            t.players[1].objects[ListIndex::new(4)].workers
        ],
        [1, 2]
    );
    let a = t.players[1].objects[ListIndex::new(0)]
        .actions
        .artifact()
        .copied()
        .unwrap();
    assert_eq!(
        (a.target, a.goal),
        (Some(object_id(9150)), Some(Position::new(25.0, 20.0, 0.0)))
    );
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)].body.anim_speed(),
        Some(0.0)
    );
}

/// A building's slot 0x70 (`FUN_1411cd860`), as Aftermath's trade workshops change hands: Gaia's
/// workshop 7 (master 1378, class 1, the artifact's unit AI) finds player 1's villager 500 and
/// asks to change hands; after the update its whole change runs. Built, it gives its storage
/// of flag 4 (5 of attribute 4) from Gaia to player 1, leaves Gaia's watched list for player
/// 1's, completes in player 1's tracker, loses its gather point and its actions, and comes to
/// the end of player 1's list with its record again.
#[test]
fn a_gaia_building_with_the_artifact_s_unit_ai_changes_hands_whole() {
    let master = |id, class, los| Master {
        id,
        kind: master_type::COMBAT,
        class,
        line_of_sight: los,
        radius: Footprint::new(0.2, 0.2),
        ..Master::default()
    };
    let workshop = Master {
        kind: BUILDING,
        build_time: 10,
        storage: vec![
            (Some(attribute::HEADROOM), Amount::new(5.0), 4),
            (None, Amount::ZERO, 0),
            (None, Amount::ZERO, 0),
        ],
        obstruction_type: ObstructionType::TYPE_2,
        obstruction_class: ObstructionClass::UNIT,
        building: Some(crate::objects::masters::BuildingMaster {
            stack_unit: None,
            foundation_terrain: -1,
            tech: None,
            head_unit: None,
            transform_unit: None,
            ..crate::objects::masters::BuildingMaster::default()
        }),
        ..master(master_id(1378), class::ARTIFACT, Distance::new(4.0))
    };
    let table = Masters::new([
        workshop,
        master(VILLAGER_MALE, class::VILLAGER, Distance::new(4.0)),
    ]);
    let torch = Unit {
        ai: Some(UnitAi {
            ai_type: crate::unit_ai::ARTIFACT_AI,
            current_order: None,
            current_action: None,
            current_target: None,
            idle_timeout: Millis::new(5000),
            adjusted_idle_timeout: Millis::new(4000),
            retarget_timer: Millis::new(1000),
            ..UnitAi::default()
        }),
        life: Lifecycle {
            master_id: master_id(1378),
            state: ObjectState::Alive,
            hp: HitPoints::new(20000.0),
            ..Lifecycle::default()
        },
        construction: Some(Construction {
            progress: Seconds::new(10.0),
            desolid: false,
            builders: 0,
            hit_points: 20000,
            build_time: 10,
            record: crate::map::spatial::Obstruction {
                kind: ObstructionType::TYPE_2,
                class: ObstructionClass::UNIT,
            },
            snow: false,
            sprite_override: false,
        }),
        gather_point: Some(GatherPoint {
            state: GatherPointState::POINT,
            point: Position::new(12.0, 12.0, 0.0),
            target: None,
            target_master: None,
        }),
        ..hands_unit(7, master_id(1378))
    };
    let villager = Unit {
        body: crate::objects::layers::Body::at(Position::new(11.5, 10.5, 0.0)),
        ai: None,
        ..hands_unit(500, VILLAGER_MALE)
    };
    let mut t = World::new(
        WorldTime::ZERO,
        1,
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO, Millis::ZERO],
        vec![
            Player {
                objects: vec![torch].into(),
                attributes: vec![Amount::ZERO; 300],
                ..Player::default()
            },
            Player {
                objects: vec![villager].into(),
                attributes: vec![Amount::ZERO; 300],
                ..Player::default()
            },
            Player {
                attributes: vec![Amount::ZERO; 300],
                ..Player::default()
            },
        ],
    );
    t.masters = vec![table.clone(), table.clone(), table];
    t.spatial = hands_world(
        7,
        PlayerId::new(0),
        class::ARTIFACT,
        master_id(1378),
        Distance::new(4.0),
    );
    t.players[0].attributes[4] = Amount::new(5.0);
    t.players[0].watched.push(crate::vision::ghost::Watched {
        id: object_id(7),
        cell: crate::vision::ghost::Cell::Tile(10 * 40 + 10),
        bits: 0,
    });
    t.spatial.unit_diplomacy = vec![
        relations([1, 4, 4]),
        relations([4, 1, 4]),
        relations([4, 4, 1]),
    ];
    t.spatial.insert(
        object_id(500),
        Placed {
            position: Position::new(11.5, 10.5, 0.0),
            radius: Footprint::new(0.2, 0.2),
            class: class::VILLAGER,
            player: PlayerId::new(1),
            master: VILLAGER_MALE,
            obstruction_type: ObstructionType::NONE,
            buildable_on: 0,
            tall: true,
            speed: Speed::ZERO,
            unknown: None,
            obstruction: None,
        },
    );
    let v = t.spatial.sight.as_mut().unwrap();
    v.add_seer(
        object_id(500),
        Seer {
            player: PlayerId::new(1),
            master: VILLAGER_MALE,
            los: Distance::new(4.0),
            state: ObjectState::Alive,
            explored: true,
            building: None,
            umv: 0,
            capture: 0,
            kind: master_type::COMBAT,
        },
    );
    let r = v.registry.as_mut().unwrap();
    r.entries.entry(object_id(500)).or_default().mask = 0b11;
    r.update_unit(UnitUpdate {
        id: object_id(500),
        owner: 1,
        position: Point::new(11.5, 10.5),
        radii: Footprint::new(0.2, 0.2),
        class: class::VILLAGER,
        old: 0,
        new: 0b11,
    })
    .unwrap();
    t.tick(Millis::new(100));
    assert_eq!(t.players[0].unsupported, None);
    assert_eq!(t.players[0].objects.len(), 0);
    let ids: Vec<ObjectId> = t.players[1].objects.iter().map(|u| u.id).collect();
    assert_eq!(ids, [object_id(500), object_id(7)]);
    assert_eq!(
        t.spatial.get(object_id(7)).map(|p| p.player),
        Some(PlayerId::new(1))
    );
    assert_eq!(
        [t.players[0].attributes[4], t.players[1].attributes[4]],
        crate::test_ids::amounts([0.0, 5.0])
    );
    let watched = |p: PlayerId| {
        t.players[p.index()]
            .watched
            .iter()
            .any(|w| w.id == object_id(7))
    };
    assert_eq!(
        [watched(PlayerId::new(0)), watched(PlayerId::new(1))],
        [false, true]
    );
    let shop = &t.players[1].objects[ListIndex::new(1)];
    assert_eq!(shop.gather_point, None);
    assert_eq!(
        shop.own_master.as_deref().map(|m| m.id),
        Some(master_id(1378))
    );
    assert!(
        t.spatial
            .get(object_id(7))
            .is_some_and(|p| p.obstruction.is_some())
    );
}

/// The artifact's unit AI in a tick, with Gaia's torch 7 (master 854, class 1, a combat
/// unit): on Gaia's turn its processMisc (`FUN_140c21d40`) finds player 1's villager 500,
/// the one player's object in its lists, and asks its unit to change hands (slot 0x70); the
/// torch leaves Gaia's list for the end of player 1's.
#[test]
fn a_gaia_torch_goes_to_the_one_player_near_it() {
    let master = |id, class, los| Master {
        id,
        kind: master_type::COMBAT,
        class,
        line_of_sight: los,
        radius: Footprint::new(0.2, 0.2),
        ..Master::default()
    };
    let table = Masters::new([
        master(ids::ID_854, class::ARTIFACT, Distance::new(4.0)),
        master(VILLAGER_MALE, class::VILLAGER, Distance::new(4.0)),
    ]);
    let torch = Unit {
        ai: Some(UnitAi {
            ai_type: crate::unit_ai::ARTIFACT_AI,
            current_order: None,
            current_action: None,
            current_target: None,
            idle_timeout: Millis::new(5000),
            adjusted_idle_timeout: Millis::new(4000),
            retarget_timer: Millis::new(1000),
            ..UnitAi::default()
        }),
        ..hands_unit(7, ids::ID_854)
    };
    let villager = Unit {
        body: crate::objects::layers::Body::at(Position::new(11.5, 10.5, 0.0)),
        ai: None,
        ..hands_unit(500, VILLAGER_MALE)
    };
    let mut t = World::new(
        WorldTime::ZERO,
        1,
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO, Millis::ZERO],
        vec![
            Player {
                objects: vec![torch].into(),
                attributes: vec![Amount::ZERO; 300],
                ..Player::default()
            },
            Player {
                objects: vec![villager].into(),
                attributes: vec![Amount::ZERO; 300],
                ..Player::default()
            },
            Player {
                attributes: vec![Amount::ZERO; 300],
                ..Player::default()
            },
        ],
    );
    t.masters = vec![table.clone(), table.clone(), table];
    t.spatial = hands_world(
        7,
        PlayerId::new(0),
        class::ARTIFACT,
        ids::ID_854,
        Distance::new(4.0),
    );
    t.spatial.unit_diplomacy = vec![
        relations([1, 4, 4]),
        relations([4, 1, 4]),
        relations([4, 4, 1]),
    ];
    t.spatial.insert(
        object_id(500),
        Placed {
            position: Position::new(11.5, 10.5, 0.0),
            radius: Footprint::new(0.2, 0.2),
            class: class::VILLAGER,
            player: PlayerId::new(1),
            master: VILLAGER_MALE,
            obstruction_type: ObstructionType::NONE,
            buildable_on: 0,
            tall: true,
            speed: Speed::ZERO,
            unknown: None,
            obstruction: None,
        },
    );
    let v = t.spatial.sight.as_mut().unwrap();
    v.add_seer(
        object_id(500),
        Seer {
            player: PlayerId::new(1),
            master: VILLAGER_MALE,
            los: Distance::new(4.0),
            state: ObjectState::Alive,
            explored: true,
            building: None,
            umv: 0,
            capture: 0,
            kind: master_type::COMBAT,
        },
    );
    let r = v.registry.as_mut().unwrap();
    r.entries.entry(object_id(500)).or_default().mask = 0b11;
    r.update_unit(UnitUpdate {
        id: object_id(500),
        owner: 1,
        position: Point::new(11.5, 10.5),
        radii: Footprint::new(0.2, 0.2),
        class: class::VILLAGER,
        old: 0,
        new: 0b11,
    })
    .unwrap();
    t.tick(Millis::new(100));
    assert_eq!(t.players[0].objects.len(), 0);
    let ids: Vec<ObjectId> = t.players[1].objects.iter().map(|u| u.id).collect();
    assert_eq!(ids, [object_id(500), object_id(7)]);
    assert_eq!(
        t.spatial.get(object_id(7)).map(|p| p.player),
        Some(PlayerId::new(1))
    );
    let torch = &t.players[1].objects[ListIndex::new(1)];
    assert_eq!(
        torch.own_master.as_deref().map(|m| m.id),
        Some(master_id(854))
    );
    let ai = torch.ai.as_ref().unwrap();
    assert_eq!(
        ai.notifies,
        [Notify::own(
            object_id(7),
            NotifyKind::OWNER_CHANGED,
            [1, 0, 0]
        )]
    );
    assert_eq!(ai.idle_timer, Millis::ZERO);
}

/// The livestock capture in a tick (`FUN_140aab570` from the Artifact's check in the second
/// update): player 1's villager stands beside Gaia's sheep 9520, which stops (its Artifact's
/// slot 0x38: the standing sprite's speed) and goes to player 1 (slot 0x70: a master of its
/// own with player 1's line of sight), the end of player 1's list, with its food (storage
/// flag 1 to the new owner).
#[test]
fn a_gaia_sheep_a_player_stands_by_goes_to_it() {
    let mut walker = crate::gathering::hunt::tests::walker();
    walker.velocity = None;
    walker.stats.speed = Speed::new(0.8);
    let sheep = Unit {
        actions: ActionList::default().with_artifact(Some(Artifact::new(true, None))),
        body: crate::objects::layers::Body::Moving(walker),
        ai: None,
        ..hands_unit(9520, master_id(705))
    };
    let villager = Unit {
        body: crate::objects::layers::Body::at(Position::new(11.5, 10.5, 0.0)),
        ai: None,
        ..hands_unit(500, VILLAGER_MALE)
    };
    let mut t = World::new(
        WorldTime::ZERO,
        1,
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO, Millis::ZERO],
        vec![
            Player {
                objects: vec![sheep].into(),
                attributes: vec![Amount::ZERO; 300],
                ..Player::default()
            },
            Player {
                objects: vec![villager].into(),
                attributes: vec![Amount::ZERO; 300],
                ..Player::default()
            },
            Player {
                attributes: vec![Amount::ZERO; 300],
                ..Player::default()
            },
        ],
    );
    let mut masters = hands_masters();
    for m in &mut masters {
        *m = Masters::new(m.iter().cloned().chain([Master {
            id: VILLAGER_MALE,
            kind: master_type::COMBAT,
            class: class::VILLAGER,
            radius: Footprint::new(0.2, 0.2),
            ..Master::default()
        }]));
    }
    t.masters = masters;
    t.spatial = hands_world(
        9520,
        PlayerId::new(0),
        class::LIVESTOCK,
        master_id(705),
        Distance::new(2.0),
    );
    t.spatial.relations = vec![
        relation_row([1, 4, 4]),
        relation_row([4, 1, 4]),
        relation_row([4, 4, 1]),
    ];
    t.spatial.insert(
        object_id(500),
        Placed {
            position: Position::new(11.5, 10.5, 0.0),
            radius: Footprint::new(0.2, 0.2),
            class: class::VILLAGER,
            player: PlayerId::new(1),
            master: VILLAGER_MALE,
            obstruction_type: ObstructionType::NONE,
            buildable_on: 0,
            tall: true,
            speed: Speed::ZERO,
            unknown: None,
            obstruction: None,
        },
    );
    t.spatial.sight.as_mut().unwrap().add_seer(
        object_id(500),
        Seer {
            player: PlayerId::new(1),
            master: VILLAGER_MALE,
            los: Distance::new(4.0),
            state: ObjectState::Alive,
            explored: true,
            building: None,
            umv: 0,
            capture: 0,
            kind: master_type::COMBAT,
        },
    );
    t.tick(Millis::new(100));
    assert_eq!(t.players[0].objects.len(), 1);
    t.tick(Millis::new(100));
    assert_eq!(t.players[0].objects.len(), 0);
    let ids: Vec<ObjectId> = t.players[1].objects.iter().map(|u| u.id).collect();
    assert_eq!(ids, [object_id(500), object_id(9520)]);
    let sheep = &t.players[1].objects[ListIndex::new(1)];
    assert_eq!(
        sheep.own_master.as_deref().map(|m| m.line_of_sight),
        Some(Distance::new(3.0))
    );
    // its standing sprite has no factor: the top speed
    assert_eq!(
        sheep.body.mover().map(|m| m.stats.speed),
        Some(Speed::new(0.8))
    );
    assert_eq!(sheep.actions.artifact().map(|a| a.gaia), Some(false));
    assert_eq!(t.players[1].attributes[16], Amount::new(100.0));
    assert_eq!(
        t.spatial.get(object_id(9520)).map(|p| p.player),
        Some(PlayerId::new(1))
    );
}

/// `FUN_141302450`: the scenario modes' descriptors have `+0x9c` 0; a mode above 16 reads the
/// first descriptor's.
#[test]
fn the_standard_modes_are_all_but_the_scenario_ones() {
    let standard: Vec<i32> = (0..=17).filter(|&m| standard_mode(m)).collect();
    assert_eq!(standard, [0, 1, 2, 6, 7, 8, 9, 10, 11, 12, 13, 16, 17]);
    assert!(standard_mode(-1));
}

/// The attributes an ownership change moves with its object, by its master of its own: the
/// old owner loses storage flags 2, 16 and 32 and counts one archer less (40), the new one
/// counts one more and gains flags 1, 2 and 32.
#[test]
fn an_object_changing_hands_takes_its_storage_and_class_counts_along() {
    let mut t = World::new(
        WorldTime::ZERO,
        1,
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player {
                attributes: vec![Amount::ZERO; 300],
                ..Player::default()
            },
            Player {
                attributes: vec![Amount::ZERO; 300],
                ..Player::default()
            },
        ],
    );
    t.players[0].attributes[4] = Amount::new(5.0);
    t.players[0].attributes[40] = Amount::new(2.0);
    let mut u = hands_unit(1, master_id(4));
    t.hand_over_attributes(0, 1, &u).unwrap();
    assert_eq!(t.players[0].attributes[4], Amount::new(5.0));
    u.own_master = hands_masters()[0].get(master_id(4)).cloned().map(Box::new);
    t.hand_over_attributes(0, 1, &u).unwrap();
    assert_eq!(
        [t.players[0].attributes[4], t.players[0].attributes[40]],
        crate::test_ids::amounts([4.0, 1.0])
    );
    assert_eq!(
        [t.players[1].attributes[4], t.players[1].attributes[40]],
        crate::test_ids::amounts([1.0, 1.0])
    );
    assert_eq!(t.players[1].attributes[11], Amount::ZERO);
}

/// `FUN_141082e80` before the kill's counts: a killed villager (class 4) counts for its owner an
/// animal attacker (class 9 or 10) in attribute 227, and by the attacker's player type a human's
/// in 229, Gaia's in 226, a computer's in 228 (type 0 none), the attacker's class as the hit
/// saw it (an attacker the world does not hold then, out for its update or gone, counts too); a
/// computer player's killed object (its AI is told) is not modelled.
#[test]
fn a_killed_villager_counts_its_attacker_s_kind_and_player_type() {
    let unit = |id, master_id| Unit {
        id,
        life: Lifecycle {
            master_id,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let player = |kind, objects| Player {
        objects,
        kind,
        attributes: vec![Amount::ZERO; 500],
        ..Player::default()
    };
    let world = |attacker_kind| {
        let mut t = World::new(
            WorldTime::ZERO,
            1,
            PlayerId::new(0),
            vec![Millis::ZERO; 3],
            vec![
                player(
                    PlayerType::GAIA,
                    vec![unit(object_id(50), master_id(126))].into(),
                ),
                player(
                    PlayerType::HUMAN,
                    vec![
                        unit(object_id(40), VILLAGER_MALE),
                        unit(object_id(41), VILLAGER_MALE),
                    ]
                    .into(),
                ),
                player(
                    attacker_kind,
                    vec![unit(object_id(60), master_id(4))].into(),
                ),
            ],
        );
        let masters = Masters::new([
            Master {
                id: VILLAGER_MALE,
                class: class::VILLAGER,
                ..Master::default()
            },
            Master {
                id: master_id(126),
                class: class::PREY,
                ..Master::default()
            },
            Master {
                id: master_id(4),
                class: class::CLASS_0,
                ..Master::default()
            },
        ]);
        t.masters = vec![masters; 3];
        t
    };
    let by = |attacker, attacker_player, attacker_class| crate::combat::damage::Kill {
        attacker,
        attacker_player,
        attacker_class,
        to_gaia: false,
    };
    let counts = |t: &World| [226, 227, 228, 229].map(|i| t.players[1].attributes[i]);
    // Gaia's wolf: an animal, Gaia's type
    let mut t = world(PlayerType::HUMAN);
    t.killed(
        PlayerId::new(1),
        ListIndex::new(0),
        by(object_id(50), 0, class::PREY),
    )
    .unwrap();
    assert_eq!(counts(&t), crate::test_ids::amounts([1.0, 1.0, 0.0, 0.0]));
    // the archers of a human, a computer and the editor's phantom
    for (kind, seen) in [
        (PlayerType::HUMAN, [0.0, 0.0, 0.0, 1.0]),
        (PlayerType::COMPUTER, [0.0, 0.0, 1.0, 0.0]),
        (PlayerType::PHANTOM, [0.0; 4]),
    ] {
        let mut t = world(kind);
        t.killed(
            PlayerId::new(1),
            ListIndex::new(1),
            by(object_id(60), 2, class::CLASS_0),
        )
        .unwrap();
        assert_eq!(counts(&t), crate::test_ids::amounts(seen));
    }
    let mut t = world(PlayerType::HUMAN);
    t.killed(
        PlayerId::new(1),
        ListIndex::new(0),
        by(object_id(99), 2, class::PREDATOR),
    )
    .unwrap();
    assert_eq!(counts(&t), crate::test_ids::amounts([0.0, 1.0, 0.0, 1.0]));
    t.players[1].computer = true;
    assert!(
        t.killed(
            PlayerId::new(1),
            ListIndex::new(0),
            by(object_id(60), 2, class::CLASS_0)
        )
        .is_err()
    );
}

/// [`a_killed_sheep_goes_to_gaia`]'s world: player 1's town centre 10029 (`killer_last`: the
/// list's last object) and its sheep 9520, and the hit that kills the sheep for Gaia.
fn killed_sheep(killer_last: bool) -> (World, crate::combat::damage::Victim) {
    let sheep = Unit {
        id: object_id(9520),
        life: Lifecycle {
            master_id: master_id(705),
            hp: HitPoints::new(2.0),
            ..Lifecycle::default()
        },
        ai: Some(UnitAi::default()),
        ..Unit::default()
    };
    let objects = if killer_last {
        vec![sheep, town_centre(10029)]
    } else {
        vec![town_centre(10029), sheep]
    }
    .into();
    let mut t = World::new(
        WorldTime::ZERO,
        1,
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player {
                attributes: vec![Amount::ZERO; 500],
                ..Player::default()
            },
            Player {
                objects,
                attributes: vec![Amount::ZERO; 500],
                ..Player::default()
            },
        ],
    );
    let master = Master {
        id: master_id(705),
        class: class::LIVESTOCK,
        storage: vec![(Some(attribute::FARM_FOOD), Amount::new(100.0), 1)],
        ..Master::default()
    };
    t.masters = vec![Masters::new([master.clone()]), Masters::new([master])];
    stock_masters(&mut t);
    t.spatial.insert(
        object_id(9520),
        Placed {
            position: Position::new(107.0, 48.0, 0.0),
            radius: Footprint::new(0.35, 0.35),
            class: class::LIVESTOCK,
            player: PlayerId::new(1),
            master: master_id(705),
            obstruction_type: ObstructionType::NONE,
            buildable_on: 0,
            tall: true,
            speed: Speed::ZERO,
            unknown: None,
            obstruction: None,
        },
    );
    let mut v = t
        .victim(object_id(9520), PlayerId::new(1))
        .unwrap()
        .unwrap();
    v.hp = HitPoints::ZERO;
    v.damaged = vec![(Some(object_id(10029)), [10029, -10, 14])];
    v.killed = Some(crate::combat::damage::Kill {
        attacker: object_id(10029),
        attacker_player: 1,
        attacker_class: ClassId::of_master(80),
        to_gaia: true,
    });
    (t, v)
}

/// MarvinBlue/426048731's update 1250: 10029's second hit kills its sheep 9520 (2 hit points,
/// 12 damage): it has none, Gaia takes it (its list, its storage, its unit AI's notifies),
/// and the players count the kill (300 + 1 and 325 + 1; a sheep costs nothing).
#[test]
fn a_killed_sheep_goes_to_gaia() {
    let (mut t, v) = killed_sheep(false);
    write_hit(&mut t, (PlayerId::new(1), ListIndex::new(0)), v);
    assert_eq!(t.players[1].objects.len(), 1);
    let gone = &t.players[0].objects[ListIndex::new(0)];
    assert_eq!((gone.id.get(), gone.life.hp), (9520, HitPoints::ZERO));
    assert_eq!(
        t.spatial.get(object_id(9520)).map(|p| p.player),
        Some(PlayerId::new(0))
    );
    assert_eq!(t.players[1].attributes[301], Amount::new(1.0));
    assert_eq!(t.players[1].attributes[326], Amount::new(1.0));
    assert_eq!(t.players[0].attributes[16], Amount::new(100.0));
}

/// The update lends its object back to its slot for a world call that may reach it
/// ([`step::Step::lend`], the game's world holding the object while its update runs):
/// the world holds it as the update left it, what the call changes on it comes back, and its
/// slot holds the stand-in again after; a removal in the call that moves the object is
/// followed. One the call takes from its list fails the update, the stand-in in nobody's list.
#[test]
fn a_lent_object_is_the_world_s_for_the_call() {
    let mut t = house_builder();
    let last = t.players[1].objects.last_place().unwrap();
    let id = t.players[1].objects[last].id;
    let seen = in_update(&mut t, (PlayerId::new(1), last), 0, |unit, c| {
        unit.life.hp = HitPoints::new(7.0);
        let during = c.lend(unit, |w| {
            let hp = w.object(id).map(|(_, u)| u.life.hp.get());
            w.object_mut(id).expect("lent").1.life.amount = Amount::new(3.0);
            let _ = w.players[1].objects.swap_remove(ListIndex::new(0)).unwrap();
            Ok(hp)
        })?;
        let out = c.w.object(id).is_none();
        Ok((during, unit.life.amount.get(), out, unit.id))
    });
    assert_eq!(
        seen.map_err(|e| e.reason().to_owned()),
        Ok((Some(7.0), 3.0, true, id))
    );
    assert_eq!(
        t.players[1].objects.position_of(id),
        Some(ListIndex::new(0))
    );
    let mut t = house_builder();
    let failed = in_update(&mut t, (PlayerId::new(1), last), 0, |unit, c| {
        c.lend(unit, |w| {
            let _ = w.players[1].objects.pop();
            Ok(())
        })
    });
    assert_eq!(
        failed.map_err(|e| e.kind()),
        Err(crate::ErrorKind::Unreachable)
    );
    let ids: Vec<ObjectId> = t.players[1].objects.iter().map(|u| u.id).collect();
    assert!(!ids.contains(&crate::objects::unit::TAKEN_OUT), "{ids:?}");
}

/// The hit's kill gives Gaia the sheep in slot 0 of its owner's list where the hit runs
/// (`FUN_1411664a0`: the list's last object takes its slot): the town centre that steps, the
/// last, moves into it while it is out, goes back there, and the step reports that slot for the
/// list's loop to file it at.
#[test]
fn a_kill_in_the_step_reports_the_object_s_new_slot() {
    let (mut t, mut v) = killed_sheep(true);
    let stepped = t
        .step_object(
            (PlayerId::new(1), ListIndex::new(1)),
            (Millis::ZERO, None, false),
            |u, c| {
                if let Some(k) = crate::combat::victims::write_back(c, &mut v)? {
                    crate::combat::victims::file_kill(c, u, k)?;
                }
                Ok(crate::objects::unit::Outcome::Stay)
            },
        )
        .unwrap();
    assert_eq!(stepped.slot, ListIndex::new(0));
    assert_eq!(
        t.players[1].objects.position_of(object_id(10029)),
        Some(ListIndex::new(0))
    );
    assert_eq!(t.players[0].objects[ListIndex::new(0)].id, object_id(9520));
}

/// A hit's kill reaches the world where the hit runs (the damage slot `FUN_1412108e0` kills and
/// counts at once: `FUN_141082e80`, slot 0x8b0): the update's later parts see the sheep Gaia's
/// and the kill counted. The updating unit's own kill (its target's retaliation) counts there
/// too, on the unit out of its slot: its owner counts the loss to Gaia (attribute 325).
#[test]
fn a_hit_s_kill_reaches_the_world_where_the_hit_runs() {
    use crate::combat::victims::{KillLeft, file_kill};
    let (mut t, mut v) = killed_sheep(false);
    let seen = in_update(&mut t, (PlayerId::new(1), ListIndex::new(0)), 0, |u, c| {
        if let Some(k) = crate::combat::victims::write_back(c, &mut v)? {
            file_kill(c, u, k)?;
        }
        let gaia_s = c.w.players[0]
            .objects
            .position_of(object_id(9520))
            .is_some();
        let counted = c.w.players[1].attributes[301];
        let kill = crate::combat::damage::Kill {
            attacker: object_id(9520),
            attacker_player: 0,
            attacker_class: class::LIVESTOCK,
            to_gaia: false,
        };
        // the town centre as a master of the table, its own kill counted by it
        u.life.master_id = master_id(705);
        let before = c.w.players[1].attributes[325];
        file_kill(c, u, KillLeft::own(c.me, kill))?;
        Ok((gaia_s, counted, c.w.players[1].attributes[325] - before))
    });
    assert_eq!(seen.unwrap(), (true, Amount::new(1.0), Amount::new(1.0)));
}

/// The updating unit's own kill that gives it to Gaia (a player's livestock): its slot 0x70 at
/// once on the unit out of its slot (its owner's fields Gaia's, [`Unit::ai_change_owner`]), the
/// move to Gaia's list a takeover's after the update ([`crate::world::step::Step`]'s
/// `taken_over`).
#[test]
fn an_own_kill_gives_the_livestock_to_gaia_at_once() {
    use crate::combat::victims::{KillLeft, file_kill};
    let (mut t, _) = killed_sheep(false);
    let seen = in_update(&mut t, (PlayerId::new(1), ListIndex::new(1)), 0, |u, c| {
        let kill = crate::combat::damage::Kill {
            attacker: object_id(10029),
            attacker_player: 1,
            attacker_class: class::CLASS_0,
            to_gaia: true,
        };
        file_kill(c, u, KillLeft::own(c.me, kill))?;
        Ok((
            c.taken_over,
            c.w.spatial.get(object_id(9520)).map(|p| p.player),
        ))
    });
    assert_eq!(
        seen,
        Ok((Some((PlayerId::new(0), true)), Some(PlayerId::new(0))))
    );
}

/// A world call in the step that moves the stepping object's stand-in (a removal from its list,
/// the stand-in its last object) puts the object back where the stand-in went; one whose
/// stand-in left the list goes to its end, and the step fails.
#[test]
fn a_step_puts_its_object_back_where_its_stand_in_went() {
    use crate::objects::unit::Outcome;
    let ids = |t: &World| {
        t.players[1]
            .objects
            .iter()
            .map(|u| u.id)
            .collect::<Vec<_>>()
    };
    let mut t = house_builder();
    let before = ids(&t);
    let last = ListIndex::new(before.len() - 1);
    let stepped = t
        .step_object(
            (PlayerId::new(1), last),
            (Millis::new(50), None, false),
            |_, c| {
                let _ = c.w.players[1]
                    .objects
                    .swap_remove(ListIndex::new(0))
                    .unwrap();
                Ok(Outcome::Stay)
            },
        )
        .unwrap();
    assert_eq!(stepped.slot, ListIndex::new(0));
    assert_eq!(ids(&t)[0], before[last.get()]);
    let mut t = house_builder();
    let failed = t
        .step_object(
            (PlayerId::new(1), ListIndex::new(0)),
            (Millis::new(50), None, false),
            |_, c| {
                c.w.players[1].objects.truncate(0);
                Ok(Outcome::Stay)
            },
        )
        .err()
        .unwrap();
    assert_eq!(failed.kind(), crate::ErrorKind::Unreachable);
    assert_eq!(ids(&t), [before[0]]);
}

/// The list's loop goes on by its own count (`FUN_141166550`): past an object that stays,
/// sleeps or changes hands, and again at the same count after one that leaves, whose slot the
/// list's last object took.
#[test]
fn the_list_s_loop_runs_its_count_again_only_after_a_leave() {
    use crate::objects::unit::Outcome;
    let mut t = house_builder();
    let n = t.players[1].objects.len();
    assert!(
        t.file_step(PlayerId::new(1), ListIndex::new(0), Outcome::Stay)
            .unwrap()
    );
    assert!(
        t.file_step(PlayerId::new(1), ListIndex::new(0), Outcome::Sleep)
            .unwrap()
    );
    assert_eq!(t.players[1].objects.len(), n - 1);
    assert!(
        !t.file_step(PlayerId::new(1), ListIndex::new(0), Outcome::Leave)
            .unwrap()
    );
    assert_eq!(t.players[1].objects.len(), n - 2);
}

/// A hit on player 2's castle 30 (master 82, 650 stone paid) with its annexes 31 and 32:
/// `World::victim` reads its annexes from the world's links, its main building for an annex,
/// its paid cost and that it is no trebuchet. The write-back of a razing hit (`FUN_1411cea90`):
/// the castle's `damaged_lately_timer`, its annexes' hit points, its kill (`FUN_1412005b0`: a
/// building's class counts no kill, both players count it against the other and its cost),
/// then the razing's history event (7, `data1` the owner) on the attacker's player and its adds.
#[test]
fn a_razed_building_s_write_back_reaches_its_annexes_history_and_counts() {
    let building = |id: i32, master_id: MasterId, annex_of: Option<ObjectId>| Unit {
        id: object_id(id),
        life: Lifecycle {
            master_id,
            hp: HitPoints::new(100.0),
            state: ObjectState::Alive,
            ..Lifecycle::default()
        },
        annex_of,
        ..Unit::default()
    };
    let player = |objects| Player {
        objects,
        attributes: vec![Amount::ZERO; 500],
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        1,
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO, Millis::ZERO],
        vec![
            player(vec![].into()),
            player(vec![building(9, ids::CASTLE, None)].into()),
            player(
                vec![
                    building(30, ids::CASTLE, None),
                    building(31, master_id(1), Some(object_id(30))),
                    building(32, master_id(1), Some(object_id(30))),
                ]
                .into(),
            ),
        ],
    );
    let castle = Master {
        id: ids::CASTLE,
        kind: BUILDING,
        class: class::BUILDING,
        cost: costs(&[(attribute::STONE, 650, true), (attribute::FOOD, 10, false)]),
        weapons: vec![ClassAmount::new(3, 11)],
        ..Master::default()
    };
    let annex = Master {
        id: master_id(1),
        kind: BUILDING,
        class: class::BUILDING,
        ..Master::default()
    };
    let masters = Masters::new([castle, annex]);
    t.masters = vec![masters.clone(), masters.clone(), masters];
    t.spatial.set_annex(object_id(31), Some(object_id(30)));
    t.spatial.set_annex(object_id(32), Some(object_id(30)));
    // a history adds events once it has an entry
    t.players[1].history.entries = vec![[0, 0]];
    let mut v = t.victim(object_id(30), PlayerId::new(1)).unwrap().unwrap();
    let b = v.building.clone().unwrap();
    assert_eq!(
        (b.main, b.annexes, b.cost, b.trebuchet),
        (None, vec![object_id(31), object_id(32)], 650.0, false)
    );
    let a = t.victim(object_id(31), PlayerId::new(1)).unwrap().unwrap();
    assert_eq!(a.building.and_then(|b| b.main), Some(object_id(30)));
    v.hp = HitPoints::ZERO;
    v.killed = Some(crate::combat::damage::Kill {
        attacker: object_id(9),
        attacker_player: 1,
        attacker_class: class::CLASS_0,
        to_gaia: false,
    });
    let b = v.building.as_mut().unwrap();
    (b.struck, b.damaged_lately) = (true, 60);
    b.razed.push(crate::combat::damage::Razed {
        attacker_player: 1,
        owner: PlayerId::new(2),
        event: Some(crate::players::history::EventKind::CASTLE_DESTROYED),
        adds: vec![
            (PlayerId::new(1), attribute::RAZINGS, 1.0),
            (PlayerId::new(2), attribute::RAZINGS_OTHER, 1.0),
            (PlayerId::new(1), attribute::KILLS_VALUE, 650.0),
        ],
    });
    write_hit(&mut t, (PlayerId::new(1), ListIndex::new(0)), v);
    let hp: Vec<f32> = t.players[2]
        .objects
        .iter()
        .map(|u| u.life.hp.get())
        .collect();
    assert_eq!(hp, [0.0, 0.0, 0.0]);
    assert_eq!(t.players[2].objects[ListIndex::new(0)].damaged_lately, 60);
    let (attacker, owner) = (&t.players[1].attributes, &t.players[2].attributes);
    assert_eq!(
        [attacker[20], owner[154]],
        crate::test_ids::amounts([0.0, 0.0])
    );
    assert_eq!(
        [attacker[302], owner[326], attacker[402]],
        crate::test_ids::amounts([1.0, 1.0, 650.0])
    );
    assert_eq!(
        [attacker[43], owner[155]],
        crate::test_ids::amounts([1.0, 1.0])
    );
    assert_eq!(
        [attacker[170], owner[152]],
        crate::test_ids::amounts([1300.0, 650.0])
    );
    let event = t.players[1]
        .history
        .event_of_kind(crate::players::history::EventKind::CASTLE_DESTROYED)
        .unwrap();
    assert_eq!(event.data1, 2.0);
}

/// The hit `v` written back where it ran, in the update of object `i` of player `p`
/// ([`crate::combat::victims::write_back`]), its kill and razings with it
/// ([`crate::combat::victims::file_kill`]).
fn write_hit(t: &mut World, (p, i): (PlayerId, ListIndex), mut v: crate::combat::damage::Victim) {
    in_update(t, (p, i), 0, |u, c| {
        if let Some(k) = crate::combat::victims::write_back(c, &mut v)? {
            crate::combat::victims::file_kill(c, u, k)?;
        }
        Ok(())
    })
    .unwrap();
}

/// A hit on a sleeping object (a tree a blast reached) written back where it ran: it takes
/// its hit points and what it holds and sleeps on, still flagged, at 1 hit point or more;
/// below 1 it wakes to the end of its player's live list and takes the rest, the hit's cleared
/// sleep flag among it.
#[test]
fn a_hit_wakes_a_sleeping_object_only_below_1_hit_point() {
    let tree = |id| Unit {
        id,
        life: Lifecycle {
            master_id: master_id(349),
            hp: HitPoints::new(20.0),
            amount: Amount::new(100.0),
            state: ObjectState::Alive,
            goto_sleep: true,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let hit = |hp: HitPoints| {
        let mut t = World::new(
            WorldTime::ZERO,
            1,
            PlayerId::new(0),
            vec![Millis::ZERO, Millis::ZERO],
            vec![
                Player {
                    objects: vec![tree(object_id(51))].into(),
                    sleeping: vec![tree(object_id(50))].into(),
                    attributes: vec![Amount::ZERO; 500],
                    ..Player::default()
                },
                Player {
                    objects: vec![tree(object_id(9))].into(),
                    attributes: vec![Amount::ZERO; 500],
                    ..Player::default()
                },
            ],
        );
        let master = Master {
            id: master_id(349),
            kind: master_type::STATIC,
            class: class::TREE,
            ..Master::default()
        };
        t.masters = vec![Masters::new([master.clone()]), Masters::new([master])];
        let mut v = t.victim(object_id(50), PlayerId::new(1)).unwrap().unwrap();
        (v.hp, v.amount, v.goto_sleep) = (hp, Amount::new(90.0), false);
        write_hit(&mut t, (PlayerId::new(1), ListIndex::new(0)), v);
        let gaia = &t.players[0];
        let ids = |l: &ObjectList| -> Vec<i32> { l.iter().map(|u| u.id.get()).collect() };
        let (_, u) = t.object(object_id(50)).unwrap();
        (
            ids(&gaia.objects),
            ids(&gaia.sleeping),
            (u.life.hp.get(), u.life.amount.get(), u.life.goto_sleep),
        )
    };
    assert_eq!(
        hit(HitPoints::new(5.0)),
        (vec![51], vec![50], (5.0, 90.0, true))
    );
    assert_eq!(
        hit(HitPoints::new(0.5)),
        (vec![51, 50], vec![], (0.5, 90.0, false))
    );
}

/// `FUN_140fde380`: a master of type 70 and up, of class 51 or 54, with a weapon (`+0x810`
/// above 0) is a trebuchet; the paid cost (`+0x880`) sums the entries with a resource and the
/// paid flag, as floats in their order.
#[test]
fn a_trebuchet_is_a_master_of_class_51_or_54_with_a_weapon() {
    use crate::combat::victims::is_trebuchet;
    use crate::combat::victims::paid_cost;
    let m = |kind, class, weapons: Vec<ClassAmount>| Master {
        kind,
        class,
        weapons,
        ..Master::default()
    };
    assert!(is_trebuchet(&m(
        BUILDING,
        class::CLASS_54,
        vec![ClassAmount::new(4, 200)]
    )));
    assert!(is_trebuchet(&m(
        BUILDING,
        class::CLASS_51,
        vec![ClassAmount::new(4, 1)]
    )));
    assert!(is_trebuchet(&m(
        master_type::COMBAT,
        class::CLASS_54,
        vec![ClassAmount::new(4, 1)]
    )));
    assert!(!is_trebuchet(&m(BUILDING, class::CLASS_51, vec![])));
    assert!(!is_trebuchet(&m(
        master_type::MISSILE,
        class::CLASS_54,
        vec![ClassAmount::new(4, 1)]
    )));
    assert!(!is_trebuchet(&m(
        BUILDING,
        class::BUILDING,
        vec![ClassAmount::new(3, 11)]
    )));
    let paid = Master {
        cost: costs(&[
            (attribute::WOOD, 200, true),
            (attribute::GOLD, 200, true),
            (attribute::STONE, 7, false),
        ]),
        ..Master::default()
    };
    assert_eq!(paid_cost(&paid), 400.0);
}

/// MarvinBlue/426048731's 1086: SET GATHER POINT on TC 9149 at (107.2451, 46.075863) on the
/// sheep 9520 (master 705).
#[test]
fn set_gather_point_takes_the_point_and_target_or_clears_it() {
    let mut tc = town_centre(9149);
    tc.life.master_id = ids::TOWN_CENTRE;
    let mut t = one_player(vec![tc], [200.0; 4]);
    t.masters = vec![Masters::new([Master {
        id: ids::TOWN_CENTRE,
        garrison: 15,
        ..Master::default()
    }])];
    let set = |target, point, own| {
        Command::GatherPoint(GatherPointCommand {
            player: player_id(0),
            units: unit_list(object_ids([9149])),
            point,
            target,
            target_master: Some(master_id(705)),
            own,
        })
    };
    t.tick_with(
        Millis::new(25),
        &[set(
            Some(object_id(9520)),
            Point::new(107.2451, 46.075_863),
            false,
        )],
    );
    assert_eq!(t.players[0].unsupported, None);
    assert_eq!(
        t.players[0].objects[ListIndex::new(0)].gather_point,
        Some(GatherPoint {
            state: GatherPointState::POINT,
            point: Position::new(107.2451, 46.075_863, 0.0),
            target: Some(object_id(9520)),
            target_master: Some(master_id(705)),
        })
    );
    t.tick_with(Millis::new(25), &[set(None, Point::new(-1.0, -1.0), false)]);
    assert_eq!(t.players[0].objects[ListIndex::new(0)].gather_point, None);
    t.tick_with(Millis::new(25), &[set(None, Point::new(-1.0, -1.0), true)]);
    assert_eq!(
        t.players[0].objects[ListIndex::new(0)]
            .gather_point
            .map(|g| g.target),
        Some(Some(object_id(9149)))
    );
}

/// A town centre of master 109 with the given position and a gather state, for SET GATHER STATE.
fn gathering_centre(id: i32, at: Position) -> Unit {
    let mut tc = centre(id);
    tc.body = crate::objects::layers::Body::at(at);
    tc
}

/// SET GATHER STATE (`FUN_140e727e0`; 428052362's computer players send 0, 1 and 0xff): a building
/// takes itself for its gather point at its own position with the packet's state, 0 clears it.
#[test]
fn set_gather_state_makes_the_building_its_own_gather_point_or_clears_it() {
    let mut t = one_player(
        vec![gathering_centre(9149, Position::new(104.0, 47.0, 0.5))],
        [200.0; 4],
    );
    t.masters = vec![Masters::new([Master {
        id: ids::TOWN_CENTRE,
        garrison: 15,
        ..Master::default()
    }])];
    let set = |state| {
        Command::SetGatherState(SetGatherStateCommand {
            player: player_id(0),
            units: unit_list(object_ids([9149])),
            state,
        })
    };
    let own = |state| {
        Some(GatherPoint {
            state,
            point: Position::new(104.0, 47.0, 0.5),
            target: Some(object_id(9149)),
            target_master: Some(ids::TOWN_CENTRE),
        })
    };
    t.tick_with(Millis::new(25), &[set(Some(GatherPointState::POINT))]);
    assert_eq!(t.players[0].unsupported, None);
    assert_eq!(
        t.players[0].objects[ListIndex::new(0)].gather_point,
        own(GatherPointState::POINT)
    );
    // the state byte is the state, 0xff included
    t.tick_with(Millis::new(25), &[set(Some(GatherPointState::KEPT))]);
    assert_eq!(
        t.players[0].objects[ListIndex::new(0)].gather_point,
        own(GatherPointState::KEPT)
    );
    t.tick_with(Millis::new(25), &[set(None)]);
    assert_eq!(t.players[0].objects[ListIndex::new(0)].gather_point, None);
}

/// SET GATHER STATE's test is SET GATHER POINT's: a building that neither garrisons nor trains
/// keeps what it had (a state of 0 clears whatever the building is), and an object the player
/// does not hold is left out.
#[test]
fn set_gather_state_leaves_a_building_that_takes_no_gather_point_and_a_gone_object() {
    let mut house = gathering_centre(10909, Position::new(20.0, 30.0, 0.0));
    house.life.master_id = ids::HOUSE;
    house.gather_point = Some(GatherPoint {
        state: GatherPointState::POINT,
        point: Position::new(5.0, 6.0, 0.0),
        target: None,
        target_master: None,
    });
    let mut t = one_player(vec![house], [200.0; 4]);
    t.masters = vec![Masters::new([Master {
        id: ids::HOUSE,
        kind: BUILDING,
        ..Master::default()
    }])];
    let set = |state| {
        Command::SetGatherState(SetGatherStateCommand {
            player: player_id(0),
            units: unit_list(object_ids([4242, 10909])),
            state,
        })
    };
    let held = t.players[0].objects[ListIndex::new(0)].gather_point;
    t.tick_with(Millis::new(25), &[set(Some(GatherPointState::KEPT))]);
    assert_eq!(t.players[0].unsupported, None);
    assert_eq!(t.players[0].objects[ListIndex::new(0)].gather_point, held);
    t.tick_with(Millis::new(25), &[set(None)]);
    assert_eq!(t.players[0].objects[ListIndex::new(0)].gather_point, None);
}

/// SET GATHER POINT on a gather point of state 0xff (`FUN_140e7e9a0`, `FUN_1411c7ff0`): its x
/// and y move, its target and master stay and its state stays 0xff; on itself the state becomes 1
/// with the target kept.
#[test]
fn set_gather_point_keeps_the_target_of_a_kept_gather_point() {
    let mut tc = gathering_centre(9149, Position::new(104.0, 47.0, 0.5));
    tc.gather_point = Some(GatherPoint {
        state: GatherPointState::KEPT,
        point: Position::new(104.0, 47.0, 0.5),
        target: Some(object_id(9149)),
        target_master: Some(ids::TOWN_CENTRE),
    });
    let mut t = one_player(vec![tc], [200.0; 4]);
    t.masters = vec![Masters::new([Master {
        id: ids::TOWN_CENTRE,
        garrison: 15,
        ..Master::default()
    }])];
    let set = |target: Option<i32>, own| {
        Command::GatherPoint(GatherPointCommand {
            player: player_id(0),
            units: unit_list(object_ids([9149])),
            point: Point::new(107.25, 46.5),
            target: target.map(object_id),
            target_master: Some(master_id(705)),
            own,
        })
    };
    t.tick_with(Millis::new(25), &[set(Some(9520), false)]);
    assert_eq!(t.players[0].unsupported, None);
    assert_eq!(
        t.players[0].objects[ListIndex::new(0)].gather_point,
        Some(GatherPoint {
            state: GatherPointState::KEPT,
            point: Position::new(107.25, 46.5, 0.5),
            target: Some(object_id(9149)),
            target_master: Some(ids::TOWN_CENTRE),
        })
    );
    t.players[0].objects[ListIndex::new(0)]
        .gather_point
        .as_mut()
        .unwrap()
        .target = Some(object_id(9520));
    t.tick_with(Millis::new(25), &[set(None, true)]);
    assert_eq!(
        t.players[0].objects[ListIndex::new(0)].gather_point,
        Some(GatherPoint {
            state: GatherPointState::POINT,
            point: Position::new(104.0, 47.0, 0.5),
            target: Some(object_id(9520)),
            target_master: Some(ids::TOWN_CENTRE),
        })
    );
}

/// SET GATHER POINT's test (`FUN_140e7e9a0`): a building whose master does not garrison takes
/// the gather point when the player trains a unit there (`FUN_141625a20`'s count above 0); one
/// that neither garrisons nor trains ignores it, and the player goes on.
#[test]
fn set_gather_point_takes_a_building_that_trains_without_garrisoning() {
    let mut barracks = town_centre(10908);
    barracks.life.master_id = master_id(12);
    let mut house = town_centre(10909);
    house.life.master_id = ids::HOUSE;
    let mut t = one_player(vec![barracks, house], [200.0; 4]);
    t.masters = vec![Masters::new([
        Master {
            id: master_id(12),
            kind: BUILDING,
            ..Master::default()
        },
        Master {
            id: ids::HOUSE,
            kind: BUILDING,
            ..Master::default()
        },
        Master {
            id: master_id(74),
            kind: master_type::COMBAT,
            available: true,
            trained_at: vec![master_id(12)],
            ..Master::default()
        },
    ])];
    let set = Command::GatherPoint(GatherPointCommand {
        player: player_id(0),
        units: unit_list(object_ids([10908, 10909])),
        point: Point::new(20.5, 30.5),
        target: None,
        target_master: None,
        own: false,
    });
    t.tick_with(Millis::new(25), &[set]);
    assert_eq!(t.players[0].unsupported, None);
    assert_eq!(
        t.players[0].objects[ListIndex::new(0)].gather_point,
        Some(GatherPoint {
            state: GatherPointState::POINT,
            point: Position::new(20.5, 30.5, 0.0),
            target: None,
            target_master: None,
        })
    );
    assert_eq!(t.players[0].objects[ListIndex::new(1)].gather_point, None);
}

/// `BUILDING_COMMAND_QUEUE` on a town centre with a gather point: the gather point goes
/// into the queue first and clears, then the packet's step; a second packet adds its step
/// alone; SET GATHER POINT empties the queue.
#[test]
fn a_command_queue_takes_the_gather_point_first_then_the_steps() {
    let mut tc = town_centre(9149);
    tc.life.master_id = ids::TOWN_CENTRE;
    tc.ai = Some(UnitAi {
        ai_type: UnitAiType::BUILDING,
        current_order: None,
        current_action: None,
        current_target: None,
        last_target: None,
        ..UnitAi::default()
    });
    tc.gather_point = Some(GatherPoint {
        state: GatherPointState::POINT,
        point: Position::new(30.5, 40.5, 0.0),
        target: None,
        target_master: None,
    });
    let mut t = one_player(vec![tc], [200.0; 4]);
    t.masters = vec![Masters::new([Master {
        id: ids::TOWN_CENTRE,
        kind: BUILDING,
        garrison: 15,
        ..Master::default()
    }])];
    let step = |point| {
        Command::CommandQueue(BuildingQueueCommand {
            player: player_id(0),
            units: unit_list(object_ids([9149])),
            target: None,
            point,
        })
    };
    t.tick_with(Millis::new(25), &[step(Point::new(50.0, 60.0))]);
    t.tick_with(Millis::new(25), &[step(Point::new(70.0, 80.0))]);
    assert_eq!(t.players[0].unsupported, None);
    let tc = &t.players[0].objects[ListIndex::new(0)];
    assert_eq!(tc.gather_point, None);
    let steps: Vec<_> = tc
        .ai
        .as_ref()
        .unwrap()
        .building_orders
        .iter()
        .map(|o| (o.order, o.target, o.point, o.priority, o.ready))
        .collect();
    let at = |x, y| (OrderKind::Move, None, Position::new(x, y, -1.0), -1, false);
    assert_eq!(steps, vec![at(30.5, 40.5), at(50.0, 60.0), at(70.0, 80.0)]);
    t.tick_with(
        Millis::new(25),
        &[Command::GatherPoint(GatherPointCommand {
            player: player_id(0),
            units: unit_list(object_ids([9149])),
            point: Point::new(-1.0, -1.0),
            target: None,
            target_master: None,
            own: false,
        })],
    );
    let left = &t.players[0].objects[ListIndex::new(0)]
        .ai
        .as_ref()
        .unwrap()
        .building_orders;
    assert_eq!(left.len(), 0);
}

#[test]
fn a_unit_due_to_spawn_stops_the_player() {
    let mut t = one_player(vec![centre(9)], [200.0; 4]);
    let quick = queue(9, 97, 1);
    t.tick_with(Millis::new(1000), &[quick]);
    // the building makes the make action in its update: one unit in production
    assert_eq!(t.players[0].unsupported, None);
    assert_eq!(t.players[0].attributes[81], Amount::new(1.0));
    t.tick(Millis::new(1000));
    assert_eq!(
        t.players[0].freezes.first().map(|f| f.2.clone()),
        Some(GenieError::unsupported(
            "a spawn without the players' sight"
        ))
    );
}

/// Slot 0x468 (`FUN_141157c50`): the top level's path, its points, length and flags, and
/// that level's waypoint; nothing without a path at the top or without movement state.
#[test]
fn a_unit_s_patrol_information_is_its_top_path_and_waypoint() {
    let mut m = crate::gathering::hunt::tests::walker();
    let mut walked = crate::movement::PathData::without_path();
    walked.has_path = true;
    walked.points = vec![BVector::new(1.0, 0.0, 1.0), BVector::new(2.0, 0.0, 1.0)];
    let mut top = crate::movement::PathData::without_path();
    top.has_path = true;
    top.points = vec![BVector::new(20.0, 0.0, 20.0), BVector::new(30.0, 0.0, 20.0)];
    top.length = Distance::new(10.0);
    top.path_flags = 7;
    top.waypoint = 1;
    m.paths = vec![walked, top];
    let (saved, waypoint) = patrol_information(Some(&m)).unwrap();
    assert_eq!(
        saved.path.points,
        [BVector::new(20.0, 0.0, 20.0), BVector::new(30.0, 0.0, 20.0)]
    );
    assert_eq!(
        (saved.path.length, saved.flags, waypoint),
        (Distance::new(10.0), 7, 1)
    );
    m.paths[1].has_path = false;
    assert_eq!(patrol_information(Some(&m)), None);
    assert_eq!(patrol_information(None), None);
}

/// `restorePatrolInformation`'s unit part: the own waypoints become the saved patrol's
/// points (slot 0x448, a whole coordinate moved to the tile's middle), and a forced, running
/// `MoveTo` goes to the saved waypoint; the old `MoveTo` to an object lets it go (its teardown's
/// slot 0x180, the caller's).
#[test]
fn a_restored_patrol_takes_its_points_and_runs_to_its_waypoint() {
    let saved = crate::unit_ai::PatrolPath {
        path: crate::pathing::tile_path::Path {
            points: vec![
                BVector::new(20.0, 0.5, 20.0),
                BVector::new(30.0, 0.5, 20.25),
            ],
            length: Distance::new(10.0),
        },
        flags: 7,
    };
    let mut u = Unit {
        body: crate::objects::layers::Body::new(
            Position::ORIGIN,
            None,
            Some(crate::gathering::hunt::tests::walker()),
        ),
        ai: Some(UnitAi {
            patrol_path: Some(saved),
            patrol_waypoint: 1,
            ..UnitAi::default()
        }),
        actions: ActionList::default().with_move_to(Some(MoveToAction {
            state: MoveTo::Moving,
            point: Position::ORIGIN,
            target: Some(object_id(77)),
            range: Distance::ZERO,
            sprite: None,
            quiet: false,
            on_target_moved: crate::movement::move_to::TargetMoved::Follows,
            wait: crate::movement::move_to::MoveWait::default(),
        })),
        ..Unit::default()
    };
    u.life.master_id = VILLAGER_MALE;
    let run = u.restore_patrol().expect("the saved patrol's move");
    assert_eq!(
        (run.point, run.range, run.force, run.walk),
        (Position::new(30.0, 20.25, 0.5), Distance::ZERO, true, false)
    );
    assert_eq!(
        u.body.mover().unwrap().user_waypoints,
        [
            BVector::new(20.5, 0.5, 20.5),
            BVector::new(30.5, 0.5, 20.25)
        ]
    );
    assert_eq!(
        ai_move_of(&mut u, run, class::CLASS_6),
        Ok(vec![object_id(77)])
    );
    let move_to = u.actions.move_to().copied().unwrap();
    assert_eq!(
        (move_to.goal(), move_to.range, move_to.sprite.map(|g| g.id)),
        (
            Some(Position::new(30.0, 20.25, 0.5)),
            Distance::ZERO,
            Some(SpriteId::named(13154))
        )
    );
}

/// `TRIBE_Combat_Object::update` (`0x141208440`) before `RGE_Combat_Object::update`: a unit
/// of class 35 (the petard; also 64 and the masters 527, 528, 1104, 2133 to 2135) below 1 hit
/// point in state 2 blasts about its place (slot 0x848, `FUN_14107f690`, factor 1, itself left
/// out, no main target), then dies. 426048706 (Aftermath): Gaia's petards 7948.. placed at −1
/// hit points blast the players' walls in update 1.
#[test]
fn a_petard_below_1_hit_point_blasts_about_itself_then_dies() {
    use crate::map::spatial::Placed;
    let petard = Master {
        id: master_id(706),
        kind: master_type::COMBAT,
        class: class::CLASS_35,
        hit_points: 45,
        weapons: vec![ClassAmount::new(4, 100)],
        area_effect: Distance::new(2.0),
        area_damage: 1.0,
        missile: None,
        death_sprite: Some(SpriteId::named(1751)),
        ..Master::default()
    };
    let wall = Master {
        id: master_id(117),
        kind: BUILDING,
        class: class::WALL,
        hit_points: 1800,
        armors: vec![ClassAmount::new(4, 0)],
        terrain_bonus: -1,
        ..Master::default()
    };
    let unit = |id, master, hp, position| Unit {
        id,
        body: crate::objects::layers::Body::at(position),
        pasture: None,
        my_pasture: None,
        life: Lifecycle {
            master_id: master,
            state: ObjectState::Alive,
            hp,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        1,
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player {
                objects: vec![unit(
                    object_id(7948),
                    master_id(706),
                    HitPoints::new(-1.0),
                    Position::new(101.5, 158.5, 0.0),
                )]
                .into(),
                attributes: vec![Amount::ZERO; 250],
                ..Player::default()
            },
            Player {
                objects: vec![unit(
                    object_id(7231),
                    master_id(117),
                    HitPoints::new(1080.0),
                    Position::new(102.5, 158.5, 0.0),
                )]
                .into(),
                attributes: vec![Amount::ZERO; 250],
                ..Player::default()
            },
        ],
    );
    let tree = Master {
        id: master_id(1250),
        kind: master_type::STATIC,
        class: class::TREE,
        hit_points: 500,
        ..Master::default()
    };
    t.masters = vec![Masters::new([petard, tree]), Masters::new([wall])];
    // a sleeping tree beside it (426048706's 8858): the blast walks the sleeping too
    t.players[0].sleeping.push(Unit {
        life: Lifecycle {
            class: Class::Static,
            ..unit(
                object_id(8858),
                master_id(1250),
                HitPoints::new(500.0),
                Position::new(100.5, 158.5, 0.0),
            )
            .life
        },
        ..unit(
            object_id(8858),
            master_id(1250),
            HitPoints::new(500.0),
            Position::new(100.5, 158.5, 0.0),
        )
    });
    t.graphics = std::collections::BTreeMap::from([(
        SpriteId::named(1751),
        Graphic {
            id: SpriteId::named(1751),
            frames: 10,
            facets: 1,
            speed_multiplier: 1.0,
            frame_duration: Seconds::new(0.1),
            replay_delay: Seconds::ZERO,
            flags: 1,
        },
    )]);
    let placed = |x: f32, class, player, master| Placed {
        position: Position::new(x, 158.5, 0.0),
        radius: Footprint::new(0.5, 0.5),
        class,
        player,
        master,
        obstruction_type: ObstructionType::NONE,
        buildable_on: 0,
        tall: true,
        speed: Speed::ZERO,
        unknown: None,
        obstruction: None,
    };
    t.spatial = crate::map::spatial::Spatial::new(GridSize::new(200, 200));
    t.spatial.insert(
        object_id(7948),
        placed(101.5, class::CLASS_35, PlayerId::new(0), master_id(706)),
    );
    t.spatial.insert(
        object_id(7231),
        placed(102.5, class::WALL, PlayerId::new(1), master_id(117)),
    );
    t.spatial.insert(
        object_id(8858),
        placed(100.5, class::TREE, PlayerId::new(0), master_id(1250)),
    );
    // a sleeping tree of id 0 out of the blast's reach but among the objects about it
    // (428491175's oak 0 at (107.5, 0.5), 9 tiles from the petard 5342): the updating petard's
    // place in its list holds no object of that id, so the tree is read as itself and sleeps on
    let far = unit(
        object_id(0),
        master_id(1250),
        HitPoints::new(500.0),
        Position::new(95.5, 158.5, 0.0),
    );
    t.players[0].sleeping.push(Unit {
        life: Lifecycle {
            class: Class::Static,
            ..far.life
        },
        ..far
    });
    t.spatial.insert(
        object_id(0),
        placed(95.5, class::TREE, PlayerId::new(0), master_id(1250)),
    );
    t.tick(Millis::new(100));
    assert_eq!(t.players[0].unsupported, None);
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)].life.hp,
        HitPoints::new(980.0)
    );
    // the sleeping tree is among the objects the blast walks; a petard's blast (its weapon
    // range at most 1) spares its own player's objects, so Gaia's tree sleeps on untouched
    assert_eq!(
        t.players[0]
            .sleeping
            .iter()
            .map(|u| (u.id.get(), u.life.hp.get()))
            .collect::<Vec<_>>(),
        [(8858, 500.0), (0, 500.0)]
    );
    assert!(ObjectState::Dying <= t.players[0].objects[ListIndex::new(0)].life.state);
}

/// A dying building lets its annexes go with its hit points (the static update's death,
/// `0x14118ad50`): a gate (426048706's 7228, master 88) at no hit points dies (3) and goes on to
/// its dying sprite (the animated update's `set_state(4)`); its annex (7229, master 95, holding
/// nothing), after it in the list, dies in its own update of the same tick and reaches state 4
/// too, no longer an annex.
#[test]
fn an_annex_follows_its_dying_main_building_into_state_4() {
    let g = Graphic {
        id: SpriteId::named(1751),
        frames: 10,
        facets: 1,
        speed_multiplier: 1.0,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: 1,
    };
    let gate = |id| Master {
        id,
        kind: BUILDING,
        class: class::GATE,
        hit_points: 1650,
        standing_sprite: Some(SpriteId::named(1751)),
        death_sprite: Some(SpriteId::named(1751)),
        building: Some(crate::objects::masters::BuildingMaster {
            stack_unit: None,
            foundation_terrain: -1,
            tech: None,
            head_unit: None,
            transform_unit: None,
            ..crate::objects::masters::BuildingMaster::default()
        }),
        ..Master::default()
    };
    let building = |id, master, hp| Unit {
        id,
        body: crate::objects::layers::Body::at(Position::new(124.5, 174.5, 0.0)),
        pasture: None,
        my_pasture: None,
        sprite: Some(crate::objects::sprite::ActiveSprite::start(
            g,
            &mut Well512::default(),
        )),
        life: Lifecycle {
            class: Class::Animated,
            master_id: master,
            state: ObjectState::Alive,
            hp,
            dying_sprite: Some(SpriteId::named(1751)),
            master: Some(MasterData::default()),
            ..Lifecycle::default()
        },
        construction: Some(Construction {
            progress: Seconds::new(100.0),
            desolid: false,
            builders: 0,
            hit_points: 1650,
            build_time: 100,
            record: crate::map::spatial::Obstruction {
                kind: ObstructionType::TYPE_2,
                class: ObstructionClass::UNIT,
            },
            snow: false,
            sprite_override: false,
        }),
        ..Unit::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        1,
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player {
                attributes: vec![Amount::ZERO; 250],
                ..Player::default()
            },
            Player {
                objects: vec![
                    building(object_id(7228), master_id(88), HitPoints::new(-58.0)),
                    Unit {
                        annex_of: Some(object_id(7228)),
                        ..building(object_id(7229), master_id(95), HitPoints::new(1650.0))
                    },
                ]
                .into(),
                attributes: vec![Amount::ZERO; 250],
                ..Player::default()
            },
        ],
    );
    t.masters = vec![
        Masters::new([]),
        Masters::new([gate(master_id(88)), gate(master_id(95))]),
    ];
    t.graphics = std::collections::BTreeMap::from([(SpriteId::named(1751), g)]);
    t.tick(Millis::new(100));
    assert_eq!(t.players[1].unsupported, None);
    let states: Vec<(i32, ObjectState)> = t.players[1]
        .objects
        .iter()
        .map(|u| (u.id.get(), u.life.state))
        .collect();
    assert_eq!(
        states,
        [
            (7228, ObjectState::DyingSprite),
            (7229, ObjectState::DyingSprite)
        ]
    );
    assert_eq!(t.players[1].objects[ListIndex::new(1)].annex_of, None);
    assert_eq!(t.spatial.annexes_of(object_id(7228)), []);
}

/// 428799090's palisade gate foundation (34600, master 800) dies at no hit points in update 2674
/// while its two annexes (34601, 34602, master 1192) come before it in the list: they stay
/// foundations with the gate's hit points (0) and no main building, then die in their own
/// update of the next tick on their own sprites, not the gate's states.
#[test]
fn annexes_before_their_dying_foundation_die_in_the_next_update() {
    let g = |id, flags| Graphic {
        id,
        frames: 10,
        facets: 1,
        speed_multiplier: 1.0,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags,
    };
    let master = |id, death_sprite, destruction_sprite| Master {
        id,
        kind: BUILDING,
        class: class::GATE,
        hit_points: 10,
        build_time: 70,
        standing_sprite: Some(SpriteId::named(1751)),
        death_sprite,
        building: Some(crate::objects::masters::BuildingMaster {
            stack_unit: None,
            foundation_terrain: -1,
            tech: None,
            head_unit: None,
            transform_unit: None,
            destruction_sprite,
            construction_sprite: None,
            ..crate::objects::masters::BuildingMaster::default()
        }),
        ..Master::default()
    };
    let foundation = |id, m, hp, annex_of| Unit {
        id,
        pasture: None,
        my_pasture: None,
        annex_of,
        sprite: Some(crate::objects::sprite::ActiveSprite::at_rest(
            g(SpriteId::named(1751), 0),
            0,
        )),
        body: crate::objects::layers::Body::new(
            Position::new(150.0, 46.0, 0.0),
            Some(crate::objects::layers::Animated::default()),
            None,
        ),
        life: Lifecycle {
            class: Class::Animated,
            master_id: m,
            state: ObjectState::Foundation,
            hp,
            dying_sprite: Some(SpriteId::named(38)),
            master: Some(MasterData::default()),
            ..Lifecycle::default()
        },
        construction: Some(Construction {
            progress: Seconds::new(10.0),
            desolid: false,
            builders: 0,
            hit_points: 10,
            build_time: 70,
            record: crate::map::spatial::Obstruction {
                kind: ObstructionType::TYPE_2,
                class: ObstructionClass::UNIT,
            },
            snow: false,
            sprite_override: false,
        }),
        ..Unit::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        1,
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player {
                attributes: vec![Amount::ZERO; 250],
                ..Player::default()
            },
            Player {
                objects: vec![
                    foundation(
                        object_id(34602),
                        master_id(1192),
                        HitPoints::new(1.0),
                        Some(object_id(34600)),
                    ),
                    foundation(
                        object_id(34601),
                        master_id(1192),
                        HitPoints::new(1.0),
                        Some(object_id(34600)),
                    ),
                    foundation(object_id(34600), master_id(800), HitPoints::ZERO, None),
                ]
                .into(),
                attributes: vec![Amount::ZERO; 250],
                ..Player::default()
            },
        ],
    );
    t.spatial
        .set_annex(object_id(34602), Some(object_id(34600)));
    t.spatial
        .set_annex(object_id(34601), Some(object_id(34600)));
    t.masters = vec![
        Masters::new([]),
        Masters::new([
            master(master_id(800), Some(SpriteId::named(38)), None),
            master(
                master_id(1192),
                Some(SpriteId::named(37)),
                Some(SpriteId::named(112)),
            ),
        ]),
    ];
    t.graphics = std::collections::BTreeMap::from([
        (SpriteId::named(1751), g(SpriteId::named(1751), 0)),
        (SpriteId::named(38), g(SpriteId::named(38), 9)),
        (SpriteId::named(37), g(SpriteId::named(37), 9)),
        (SpriteId::named(112), g(SpriteId::named(112), 9)),
    ]);
    let looks = |t: &World| -> Vec<(ObjectId, ObjectState, f32, Option<ObjectId>)> {
        t.players[1]
            .objects
            .iter()
            .map(|u| (u.id, u.life.state, u.life.hp.get(), u.annex_of))
            .collect()
    };
    t.tick(Millis::new(100));
    assert_eq!(t.players[1].unsupported, None);
    assert_eq!(
        looks(&t),
        [
            (object_id(34602), ObjectState::Foundation, 0.0, None),
            (object_id(34601), ObjectState::Foundation, 0.0, None),
            (object_id(34600), ObjectState::DyingSprite, 0.0, None)
        ]
    );
    t.tick(Millis::new(100));
    assert_eq!(t.players[1].unsupported, None);
    let states: Vec<(ObjectId, ObjectState)> = looks(&t).iter().map(|l| (l.0, l.1)).collect();
    assert_eq!(
        states,
        [
            (object_id(34602), ObjectState::DyingSprite),
            (object_id(34601), ObjectState::DyingSprite),
            (object_id(34600), ObjectState::DyingSprite)
        ]
    );
    let sprites: Vec<Option<SpriteId>> = t.players[1]
        .objects
        .iter()
        .map(|u| u.sprite.map(|s| s.graphic.id))
        .collect();
    assert_eq!(
        sprites,
        [
            Some(SpriteId::named(112)),
            Some(SpriteId::named(112)),
            Some(SpriteId::named(38))
        ]
    );
}

/// The regeneration's end: an object at its most hit points after it (slot 0xc0 at least slot
/// 200) has the armor its charge's hits took back (`+0x9e4`/`+0x9e6` zeroed); below its most it
/// keeps the loss.
#[test]
fn a_unit_regenerated_to_its_most_has_its_armor_back() {
    let berserk = Master {
        id: master_id(692),
        kind: master_type::COMBAT,
        hit_points: 54,
        regeneration: 20.0,
        ..Master::default()
    };
    let unit = |hp| Unit {
        id: object_id(1),
        life: Lifecycle {
            state: ObjectState::Alive,
            hp,
            ..Lifecycle::default()
        },
        armor_removed: ArmorPair::new(2, 1),
        ..Unit::default()
    };
    let mut healed = unit(HitPoints::new(53.0));
    healed.regenerate(&berserk, Seconds::new(3.0));
    assert_eq!(
        (healed.life.hp, healed.armor_removed),
        (HitPoints::new(54.0), ArmorPair::ZERO)
    );
    let mut hurt = unit(HitPoints::new(40.0));
    hurt.regenerate(&berserk, Seconds::new(3.0));
    assert_eq!(
        (hurt.life.hp, hurt.armor_removed),
        (HitPoints::new(41.0), ArmorPair::new(2, 1))
    );
}

/// `TRIBE_Combat_Object::update`'s regeneration (`0x141208440`): the rate (slot 0x260,
/// `FUN_1412033a0`) is the stinger plus (the powerup plus (a hero's 30 plus the master's)); a
/// whole hit point comes per 60 / |rate| seconds of the world's dt plus the time held back
/// (`+0xa10`), the fraction kept as time. A positive rate heals up to the most (slot 0xb8); a
/// negative one takes off at most the hit points (slot 0xb0); below 1 hit point, out of state
/// 2 or below 1.2e-7 in size nothing changes.
#[test]
fn a_unit_regenerates_whole_hit_points_and_keeps_the_rest_as_time() {
    use crate::objects::hit_points::regeneration_rate;
    let berserk = Master {
        id: master_id(692),
        kind: master_type::COMBAT,
        hit_points: 54,
        regeneration: 20.0,
        ..Master::default()
    };
    let unit = |hp| Unit {
        id: object_id(1),
        life: Lifecycle {
            state: ObjectState::Alive,
            hp,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    // 20 a minute: a point per 3 s; 2 s of dt plus 1.5 held is one point, the rest held back
    // (in f32: 3.5 / 3 less its whole point, times 3)
    let mut u = unit(HitPoints::new(30.0));
    u.regen_timer = Seconds::new(1.5);
    u.regenerate(&berserk, Seconds::new(2.0));
    assert_eq!(
        (u.life.hp, u.regen_timer),
        (
            HitPoints::new(31.0),
            Seconds::new(((3.5f32 / 3.0) % 1.0) * 3.0)
        )
    );
    // at the most nothing is added, and the time still runs
    let mut full = unit(HitPoints::new(54.0));
    full.regenerate(&berserk, Seconds::new(3.0));
    assert_eq!(
        (full.life.hp, full.regen_timer),
        (HitPoints::new(54.0), Seconds::ZERO)
    );
    // a hero's 30 and the powerups: stinger + (powerup + (30 + master))
    let hero = Master {
        hero_flag: 1,
        ..berserk.clone()
    };
    let mut powered = unit(HitPoints::new(10.0));
    powered.regen_powerup = 5.0;
    powered.regen_stinger = 5.0;
    assert_eq!(regeneration_rate(&hero, &powered), 60.0);
    let not_hero = Master {
        hero_flag: 0x84,
        ..berserk.clone()
    };
    assert_eq!(regeneration_rate(&not_hero, &powered), 30.0);
    // a gate of Aftermath's: -1000 a minute takes 1 hit point per 0.06 s, at most what it has
    let gate = Master {
        hit_points: 1650,
        regeneration: -1000.0,
        kind: BUILDING,
        ..Master::default()
    };
    let mut g = unit(HitPoints::new(1650.0));
    g.regenerate(&gate, Seconds::new(0.104));
    assert_eq!(g.life.hp, HitPoints::new(1649.0));
    let mut low = unit(HitPoints::new(1.0));
    low.regenerate(&gate, Seconds::new(1.0));
    assert_eq!(low.life.hp, HitPoints::ZERO);
    // below 1 hit point or out of state 2: nothing
    let mut dead = unit(HitPoints::new(0.5));
    dead.regenerate(&gate, Seconds::new(1.0));
    assert_eq!(
        (dead.life.hp, dead.regen_timer),
        (HitPoints::new(0.5), Seconds::ZERO)
    );
}

/// `TRIBE_Combat_Object::update`'s hit point powerup check: a powerup above its room comes off
/// the hit points, at least 1 left (`MAXSS`), and takes the room; at most the room, nothing.
#[test]
fn a_hit_point_powerup_above_its_room_comes_off_the_hit_points() {
    let with = |hp, up, room| Unit {
        life: Lifecycle {
            hp: HitPoints::new(hp),
            ..Lifecycle::default()
        },
        hp_powerup: HpPowerup {
            taken: HitPoints::new(up),
            max: HitPoints::new(room),
        },
        ..Unit::default()
    };
    let mut u = with(100.0, 10.0, 4.0);
    u.clamp_hp_powerup();
    let at_room = HpPowerup {
        taken: HitPoints::new(4.0),
        max: HitPoints::new(4.0),
    };
    assert_eq!((u.life.hp, u.hp_powerup), (HitPoints::new(94.0), at_room));
    let mut low = with(3.0, 10.0, 4.0);
    low.clamp_hp_powerup();
    assert_eq!(
        (low.life.hp, low.hp_powerup),
        (HitPoints::new(1.0), at_room)
    );
    let mut nan = with(f32::NAN, 10.0, 4.0);
    nan.clamp_hp_powerup();
    assert_eq!(nan.life.hp, HitPoints::new(1.0));
    let mut kept = with(100.0, 4.0, 4.0);
    kept.clamp_hp_powerup();
    assert_eq!(
        (kept.life.hp, kept.hp_powerup),
        (HitPoints::new(100.0), at_room)
    );
}

/// `RGE_Combat_Object::update`: the attack timer runs down by dt times the object's rate
/// (`+0x87c`), which a terrain look sets ([`crate::combat::abilities`]).
#[test]
fn the_attack_timer_runs_down_at_the_object_s_rate() {
    let unit = Unit {
        id: object_id(1),
        attack_timer: Seconds::new(1.0),
        abilities: crate::combat::abilities::Abilities {
            attack_rate: 0.5,
            ..crate::combat::abilities::Abilities::default()
        },
        ..Unit::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        1,
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![Player {
            objects: vec![unit].into(),
            ..Player::default()
        }],
    );
    stock_masters(&mut t);
    t.tick(Millis::new(100));
    assert_eq!(
        t.players[0].objects[ListIndex::new(0)].attack_timer,
        Seconds::new(1.0 - 0.1 * 0.5)
    );
}

/// records/425984813: a barracks (12) that dies short of its build time plays its destruction
/// sprite (389), not its death sprite (2573) (`RGE_Static_Object::update`'s sprite after the
/// death), a foundation (state 0) as a building in state 2.
#[test]
fn a_building_short_of_its_build_time_that_dies_plays_its_destruction_sprite() {
    for state in [ObjectState::Foundation, ObjectState::Alive] {
        building_short_of_its_build_time_dies(state);
    }
}

fn building_short_of_its_build_time_dies(state: ObjectState) {
    use crate::objects::sprite::{ActiveSprite, Graphic};
    let g = |id| Graphic {
        id,
        frames: 10,
        facets: 1,
        speed_multiplier: 1.0,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: 1,
    };
    let barracks = Master {
        id: master_id(12),
        kind: BUILDING,
        class: class::BUILDING,
        standing_sprite: Some(SpriteId::named(207)),
        death_sprite: Some(SpriteId::named(2573)),
        build_time: 50,
        hit_points: 1200,
        building: Some(crate::objects::masters::BuildingMaster {
            destruction_sprite: Some(SpriteId::named(389)),
            construction_sprite: None,
            ..crate::objects::masters::BuildingMaster::default()
        }),
        ..Master::default()
    };
    let foundation = Unit {
        id: object_id(5),
        life: Lifecycle {
            class: Class::Animated,
            state,
            hp: HitPoints::new(0.5),
            master_id: master_id(12),
            master: Some(MasterData::default()),
            dying_sprite: Some(SpriteId::named(2573)),
            ..Lifecycle::default()
        },
        construction: Some(Construction {
            progress: Seconds::new(10.0),
            desolid: false,
            builders: 0,
            hit_points: 1200,
            build_time: 50,
            record: crate::map::spatial::Obstruction {
                kind: ObstructionType::TYPE_2,
                class: ObstructionClass::NONE,
            },
            snow: false,
            sprite_override: false,
        }),
        sprite: Some(ActiveSprite::at_rest(g(SpriteId::named(207)), 0)),
        pasture: None,
        my_pasture: None,
        ..Unit::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        1,
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![Player {
            objects: vec![foundation].into(),
            attributes: vec![Amount::ZERO; 300],
            ..Player::default()
        }],
    );
    t.masters = vec![Masters::new([barracks])];
    t.graphics = [
        (SpriteId::named(207), g(SpriteId::named(207))),
        (SpriteId::named(389), g(SpriteId::named(389))),
        (SpriteId::named(2573), g(SpriteId::named(2573))),
    ]
    .into_iter()
    .collect();
    t.tick(Millis::new(10));
    assert_eq!(t.players[0].unsupported, None);
    let u = &t.players[0].objects[ListIndex::new(0)];
    assert_eq!(u.life.state, ObjectState::DyingSprite);
    assert_eq!(u.sprite.unwrap().graphic.id, SpriteId::named(389));
}

/// GAME's cheat code (sub-command 6, `FUN_1415c92c0`): without the game's cheats it does
/// nothing; with them the cheat would run, which the slice does not model: its player stops.
#[test]
fn a_cheat_code_does_nothing_without_the_game_s_cheats() {
    let mut t = sent_off_world(vec![]);
    let before = t.players.clone();
    let cheat = Command::Game(GameCommand {
        issuer: player_id(1),
        sub: 6,
        player: 1,
        a: 4,
        speed: 0.0,
        value: 0,
    });
    t.run(&cheat);
    assert_eq!(t.players, before);
    t.settings.cheats_enabled = true;
    t.run(&cheat);
    assert!(t.players[1].stopped);
}

/// A unit of the AI-move tests: the walker of the hunt tests at (10.5, 10.5), master 83, on no
/// pasture.
fn mover_unit(id: i32) -> Unit {
    Unit {
        id: object_id(id),
        body: crate::objects::layers::Body::new(
            Position::new(10.5, 10.5, 0.0),
            None,
            Some(crate::gathering::hunt::tests::walker()),
        ),
        my_pasture: None,
        life: Lifecycle {
            master_id: VILLAGER_MALE,
            ..Lifecycle::default()
        },
        ..Unit::default()
    }
}

/// A unit's own Attack on `target` (second target `target2`) in `state`, without a `MoveTo`.
fn own_attack(target: i32, target2: i32, state: AttackState) -> crate::combat::attack::Attack {
    crate::combat::attack::Attack {
        target: ObjectId::of_field(target),
        target2: ObjectId::of_field(target2),
        state,
        timer: Seconds::ZERO,
        range: Distance::ZERO,
        min_range: Distance::ZERO,
        point: Position::new(-1.0, -1.0, -1.0),
        seen: Position::ORIGIN,
        move_to: None,
        pending: false,
        swing: Seconds::ZERO,
        rest: Seconds::ZERO,
        arms: crate::combat::attack::Arms {
            move_sprite: None,
            sprite: None,
            second_sprite: None,
            idle_sprite: None,
            idle_attack_sprite: None,
            missile: None,
            fire_frame: -1,
            interrupt_frame: -1,
        },
        sub_action: false,
        on_target_moved: crate::movement::move_to::TargetMoved::Follows,
        same_owner: false,
    }
}

/// The unit's slot 0x340 ([`Unit::ai_move_with`]) for the unit of class `class` of player 1:
/// what it let go, or why it stopped.
fn ai_move_of(
    u: &mut Unit,
    m: crate::unit_ai::AiMove,
    class: ClassId,
) -> Result<Vec<ObjectId>, GenieError> {
    let by = Releaser {
        id: u.id,
        class: Some(class),
        player: PlayerId::new(1),
    };
    let (mut releases, mut leaves) = (Vec::new(), Vec::new());
    let mut rng = Well512::default();
    let (masters, graphics) = scout_sprites();
    let _ = u.ai_move_with(
        m,
        (class, PlayerId::new(1)),
        (&mut releases, &mut leaves, by),
        (Some(&masters), &graphics, &mut rng),
    )?;
    assert!(releases.iter().all(|&(_, b)| b == by));
    Ok(releases.into_iter().map(|(t, _)| t).collect())
}

/// The master 83 of [`mover_unit`] with its running (13154), walking (13153) and standing
/// (13151) sprites, and their table.
fn scout_sprites() -> (Masters, std::collections::BTreeMap<SpriteId, Graphic>) {
    let master = Master {
        id: VILLAGER_MALE,
        run_sprite: Some(SpriteId::named(13154)),
        move_sprite: Some(SpriteId::named(13153)),
        standing_sprite: Some(SpriteId::named(13151)),
        ..Master::default()
    };
    let graphics = [13154, 13153, 13151]
        .into_iter()
        .map(|id| {
            let g = Graphic {
                id: SpriteId::named(id),
                frames: 10,
                facets: 16,
                speed_multiplier: 1.0,
                frame_duration: Seconds::new(0.1),
                replay_delay: Seconds::ZERO,
                flags: 1,
            };
            (g.id, g)
        })
        .collect();
    (Masters::new([master]), graphics)
}

const AI_POINT: Position = Position::new(14.5, 12.0, 0.0);

fn forced(walk: bool) -> crate::unit_ai::AiMove {
    crate::unit_ai::AiMove {
        point: AI_POINT,
        range: Distance::new(0.5),
        force: true,
        walk,
    }
}

/// `FUN_1410866e0` on a unit whose Attack is done (state 1, 426391614's scout 123 in update
/// 1052): the Attack's slot 0x40 is the base one (`FUN_140a78490`, 0), so a `MoveTo` to the point
/// within the move's range, running unless the move walks, replaces the list; the Attack's
/// teardown lets both its targets go. The `MoveTo` starts (state 3: no target, so never within
/// 0.01, and the unit has speed).
#[test]
fn an_ai_move_replaces_a_done_attack_with_a_move_to_the_point() {
    let mut u = mover_unit(123);
    u.actions
        .set_attack(Some(own_attack(9149, 9150, AttackState::Done)));
    assert_eq!(
        ai_move_of(&mut u, forced(false), class::CLASS_47),
        Ok(vec![object_id(9149), object_id(9150)])
    );
    assert_eq!(u.actions.attack().copied(), None);
    let mt = u.actions.move_to().copied().expect("the AI's MoveTo");
    assert_eq!(
        (
            mt.state,
            mt.goal(),
            ObjectId::field(mt.target),
            mt.range,
            mt.sprite.map(|g| g.id)
        ),
        (
            MoveTo::Start,
            Some(AI_POINT),
            -1,
            Distance::new(0.5),
            Some(SpriteId::named(13154))
        )
    );
    // walking: the master's walking sprite
    let mut u = mover_unit(123);
    assert_eq!(
        ai_move_of(&mut u, forced(true), class::CLASS_47),
        Ok(vec![])
    );
    let walked = u.actions.move_to().copied().unwrap().sprite;
    assert_eq!(walked.map(|g| g.id), Some(SpriteId::named(13153)));
}

/// The Gather's slot 0x40 (`0x140a8b740`) takes a move to a point: the target goes (slot 0x68
/// with none, the caller's release), the point is the move's and `set_state(15)` walks there; the
/// unit's list keeps the Gather (425780724's villager in update 5526, its Gather idle).
#[test]
fn an_ai_move_hands_the_point_to_the_gather() {
    let task = crate::gathering::hunt::HuntTask {
        id: TaskId::test(2),
        range: Distance::new(0.25),
        resource: Some(AttributeId::numbered(17)),
        sprites: TaskSprites::NONE,
        work_value: 1.0,
        productivity: None,
        kind4: None,
    };
    let bush = crate::gathering::gather::Resource {
        id: object_id(28222),
        position: Position::new(23.5, 93.5, 0.0),
        class: class::CLASS_7,
        master: master_id(59),
        player: PlayerId::new(0),
        workers: 0,
        drop_site: false,
        gap: Distance::new(1.2),
        state: ObjectState::Alive,
        radius: Footprint::new(0.5, 0.5),
    };
    let rate = crate::gathering::gather::StopRate {
        work_rate: WorkRate::new(1.0),
        capacity: Amount::new(10.0),
        productivity: 1.0,
    };
    let mut gather = crate::gathering::gather::GatherAction::new(task, &bush, false, rate).unwrap();
    gather.state = GatherState::Idle;
    let mut u = mover_unit(31139);
    u.actions.set_gather(Some(gather));
    assert_eq!(
        ai_move_of(&mut u, forced(false), class::VILLAGER),
        Ok(vec![object_id(28222)])
    );
    let g = u.actions.gather().copied().expect("the Gather stays");
    assert_eq!(
        (g.state, g.target, g.point, g.move_to.and_then(|m| m.goal())),
        (GatherState::Moved, None, AI_POINT, Some(AI_POINT))
    );
    assert_eq!(g.move_to.map(|m| m.state), Some(MoveTo::Start));
    assert_eq!(u.actions.move_to().copied(), None);
}

/// The Hunt's slot 0x40 (`FUN_140ac52c0`) takes a point unless its task has a work range: its
/// target goes, `+0x68` is −1 and `set_state(15)` walks to the point (431193960's villager in
/// update 1465, its Hunt idle). A task with a range answers 0: the `MoveTo` replaces the list and
/// the Hunt's teardown lets its target and its drop site go.
#[test]
fn an_ai_move_hands_the_point_to_a_hunt_without_a_work_range() {
    let mut u = mover_unit(47);
    let mut hunt = crate::gathering::hunt::tests::hunting(9149);
    hunt.state = HuntState::Idle;
    hunt.carry_to = Some(object_id(9150));
    u.actions.set_hunt(Some(hunt));
    assert_eq!(
        ai_move_of(&mut u, forced(false), class::VILLAGER),
        Ok(vec![object_id(9149)])
    );
    let h = u.actions.hunt().copied().expect("the Hunt stays");
    assert_eq!(
        (h.state, h.target, h.carry_to, h.point, h.target_class),
        (
            HuntState::WalkPoint,
            None,
            Some(object_id(9150)),
            AI_POINT,
            None
        )
    );
    assert_eq!(u.actions.move_to().copied(), None);
    let mut u = mover_unit(47);
    hunt.task.range = Distance::new(1.0);
    u.actions.set_hunt(Some(hunt));
    assert_eq!(
        ai_move_of(&mut u, forced(false), class::VILLAGER),
        Ok(vec![object_id(9149), object_id(9150)])
    );
    assert_eq!(u.actions.hunt().copied(), None);
    assert_eq!(
        u.actions.move_to().map(MoveToAction::goal),
        Some(Some(AI_POINT))
    );
}

/// Without the force a head `MoveTo` whose point (`+0x38`) is the move's stands, a point one or
/// an object one (the target's place when it last took it); forced, or to another point,
/// the new `MoveTo` replaces it (its teardown stops the unit and lets its target go).
#[test]
fn an_unforced_ai_move_leaves_a_move_to_the_same_point() {
    let unforced = crate::unit_ai::AiMove {
        force: false,
        ..forced(false)
    };
    let old = MoveToAction::to_point(AI_POINT, Distance::ZERO, None, None);
    let mut u = mover_unit(5);
    u.actions.set_move_to(Some(old));
    assert_eq!(ai_move_of(&mut u, unforced, class::CLASS_47), Ok(vec![]));
    assert_eq!(u.actions.move_to().copied(), Some(old));
    let to_object = MoveToAction {
        point: AI_POINT,
        target: Some(object_id(9149)),
        ..old
    };
    u.actions.set_move_to(Some(to_object));
    assert_eq!(ai_move_of(&mut u, unforced, class::CLASS_47), Ok(vec![]));
    assert_eq!(u.actions.move_to().copied(), Some(to_object));
    assert_eq!(
        ai_move_of(&mut u, forced(false), class::CLASS_47),
        Ok(vec![object_id(9149)])
    );
    assert_eq!(
        u.actions
            .move_to()
            .map(|m| (ObjectId::field(m.target), m.range)),
        Some((-1, Distance::new(0.5)))
    );
    u.actions.set_move_to(Some(old));
    let elsewhere = crate::unit_ai::AiMove {
        point: Position::new(1.0, 2.0, 0.0),
        ..unforced
    };
    assert_eq!(ai_move_of(&mut u, elsewhere, class::CLASS_47), Ok(vec![]));
    assert_eq!(
        u.actions.move_to().copied().unwrap().goal(),
        Some(Position::new(1.0, 2.0, 0.0))
    );
}

/// `FUN_141216c50`: an unpacked trebuchet (class 54) packs instead (not modelled); a packed one
/// (class 51) moves as any unit. The base slot 0x40 of an Explore and of a wait for a foundation
/// takes nothing; an action the slice does not hold stops it.
#[test]
fn an_ai_move_of_a_trebuchet_packs_and_the_base_slot_takes_nothing() {
    let mut u = mover_unit(7);
    assert!(ai_move_of(&mut u, forced(false), class::CLASS_54).is_err());
    assert_eq!(u.actions.move_to().copied(), None);
    assert_eq!(
        ai_move_of(&mut u, forced(false), class::CLASS_51),
        Ok(vec![])
    );
    assert_eq!(
        u.actions.move_to().map(MoveToAction::goal),
        Some(Some(AI_POINT))
    );
    let mut u = mover_unit(7);
    u.actions
        .set_wait_solidify(Some(crate::construction::wait_solidify::WaitSolidify::new(
            crate::objects::masters::Task::default(),
            object_id(10028),
        )));
    let none = std::collections::BTreeMap::new();
    assert_eq!(
        u.head_takes_point(AI_POINT, PlayerId::new(1), None, &none),
        Ok(PointMove::Declined)
    );
    let mut u = mover_unit(7);
    u.unmodelled_action = Some(GenieError::unsupported("a test's action"));
    assert!(ai_move_of(&mut u, forced(false), class::CLASS_47).is_err());
}

/// A unit without speed: the `MoveTo`'s start fails it (state 13), whose `set_state` (not
/// quiet) shows the master's standing sprite: the mover's speed is the standing sprite's and
/// the unit's `+0x7b8` clears.
#[test]
fn an_ai_move_of_a_unit_without_speed_fails_at_its_start_standing() {
    let mut u = mover_unit(8);
    let m = u.body.mover_mut().unwrap();
    m.stats.speed = Speed::ZERO;
    u.body.set_anim_speed(1.0);
    assert_eq!(
        ai_move_of(&mut u, forced(false), class::CLASS_47),
        Ok(vec![])
    );
    assert_eq!(u.actions.move_to().map(|m| m.state), Some(MoveTo::Failed));
    assert_eq!(u.body.anim_speed(), Some(0.0));
}

/// The motion step gave up on a unit's own `MoveTo` (`LAB_140aa320c`): any unit but a human's
/// villager fails it (`set_state(13)`), whose `set_state` (`0x140aa29e0`, not quiet) shows the
/// standing sprite (slot 0x68 with slot 0x798's): the mover's speed becomes the standing one
/// (426049848's deer 14064 in update 2077, which then evaded at 0.737, not 1.4) and `+0x7b8`
/// clears. A human's villager waits once (`set_state(2)`), which shows nothing.
#[test]
fn an_own_move_to_that_gives_up_fails_standing_unless_a_villager_waits() {
    let standing = Graphic {
        id: SpriteId::named(764),
        frames: 10,
        facets: 8,
        speed_multiplier: 1.0,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: 1,
    };
    let walking = Graphic {
        id: SpriteId::named(765),
        ..standing
    };
    let graphics = std::collections::BTreeMap::from([
        (SpriteId::named(764), standing),
        (SpriteId::named(765), walking),
    ]);
    let masters = Masters::new([Master {
        id: master_id(65),
        standing_sprite: Some(SpriteId::named(764)),
        ..Master::default()
    }]);
    let mut rng = Well512::default();
    let deer = |rng: &mut Well512| {
        let mut u = mover_unit(14064);
        u.life.master_id = master_id(65);
        let m = u.body.mover_mut().unwrap();
        // the deer's speed 0.737 with its standing sprite's factor 1, its walking one's 1.9
        m.stats.base = Speed::new(0.737);
        m.stats.speed = Speed::new(1.4003);
        u.sprite = Some(crate::objects::sprite::ActiveSprite::start(walking, rng));
        u.body.set_anim_speed(1.0);
        u.actions.set_move_to(Some(MoveToAction::to_point(
            Position::new(119.5, 172.5, 0.0),
            Distance::ZERO,
            None,
            None,
        )));
        u
    };
    let mut u = deer(&mut rng);
    u.own_move_gave_up(
        false,
        UnitStance::AGGRESSIVE,
        (Some(&masters), &graphics, &mut rng),
    )
    .unwrap();
    assert_eq!(u.actions.move_to().map(|m| m.state), Some(MoveTo::Failed));
    assert_eq!(
        u.body.mover().map(|m| m.stats.speed),
        Some(Speed::new(0.737))
    );
    assert_eq!(u.sprite.map(|s| s.graphic.id), Some(SpriteId::named(764)));
    assert_eq!(u.body.anim_speed(), Some(0.0));
    let mut v = deer(&mut rng);
    v.own_move_gave_up(
        true,
        UnitStance::AGGRESSIVE,
        (Some(&masters), &graphics, &mut rng),
    )
    .unwrap();
    assert_eq!(v.actions.move_to().map(|m| m.state), Some(MoveTo::Waiting));
    assert_eq!(
        v.body.mover().map(|m| m.stats.speed),
        Some(Speed::new(1.4003))
    );
    assert_eq!(v.sprite.map(|s| s.graphic.id), Some(SpriteId::named(765)));
}

/// The unit AI's attack command ([`crate::unit_ai::UnitSide::attack`]) of the scout 123 (master
/// 448, class 47, 426049998's) on Gaia's wolf 50 half a tile east, its attack timer `timer`, inside
/// `inside` if any: the scout and what the command answered and took.
fn scout_attack(
    timer: f32,
    inside: Option<ObjectId>,
) -> (Unit, crate::unit_ai::Attacked, Vec<i32>) {
    scout_attack_on(timer, inside, None, false)
}

/// `f` on the unit AI's side ([`crate::unit_ai::world_view::UnitView`]) of the first of player
/// 1's live objects `own` (the scout 123 of master 448 at (10, 10)), player 1 holding `tracker`, in
/// a world of Gaia's player `gaia`, `spatial`, the masters `every`, `graphics` and the unit groups
/// `groups`, as the unit's update runs it: what `f` gives and the world after the update.
fn on_side<R>(
    (gaia, own, tracker): (Player, Vec<Unit>, crate::players::tracker::Tracker),
    (spatial, every, graphics): (
        Spatial,
        Vec<Masters>,
        std::collections::BTreeMap<SpriteId, Graphic>,
    ),
    groups: crate::groups::UnitGroups,
    f: impl FnOnce(&mut crate::unit_ai::world_view::UnitView<'_, '_>) -> R,
) -> (R, World) {
    let mut attributes = vec![Amount::ZERO; 8];
    attributes[4] = Amount::new(10.0);
    let scout = Player {
        objects: own.into(),
        relations: relation_row([0, 0]),
        attributes,
        tracker,
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        0,
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![gaia, scout],
    );
    (t.spatial, t.masters, t.graphics, t.groups) = (spatial, every, graphics, groups);
    let answer = in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(0)),
        0,
        |unit, c| {
            let mut side = crate::unit_ai::world_view::UnitView {
                c,
                unit,
                attacker: crate::unit_ai::Attacker {
                    ai_type: UnitAiType::SOLDIER,
                    player: PlayerId::new(1),
                    computer: false,
                    state: ObjectState::Alive,
                    held: None,
                    top_action: None,
                    master: ids::ID_448,
                    kind: master_type::COMBAT,
                    class: Some(class::CLASS_47),
                    attack_priority: AttackPriority::PRIORITY_0,
                    missile: None,
                    range: Distance::new(1.0),
                    min_range: Distance::ZERO,
                    charge_type: 0,
                    area_effect: Distance::ZERO,
                    blast_level: 0,
                    los: Distance::new(4.0),
                    position: Position::new(10.0, 10.0, 0.0),
                    radius: Footprint::new(0.2, 0.2),
                    speed: Speed::new(1.2),
                    scenario: false,
                    firepower: 0.0,
                    capture: 0,
                },
                player: PlayerId::new(1),
                computer: false,
                construction_rate: 1.0,
                new_owner: None,
                civ: CivId::new(0),
                ai_settings: AiSettings::default(),
            };
            Ok(f(&mut side))
        },
    );
    (answer.unwrap(), t)
}

/// [`scout_attack`] of a scout whose own Attack is `own`, the command `force`d or not.
fn scout_attack_on(
    timer: f32,
    inside: Option<ObjectId>,
    own: Option<&crate::combat::attack::Attack>,
    force: bool,
) -> (Unit, crate::unit_ai::Attacked, Vec<i32>) {
    use crate::unit_ai::UnitSide;
    let standing = Graphic {
        id: SpriteId::named(300),
        frames: 10,
        facets: 8,
        speed_multiplier: 1.0,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: 1,
    };
    let graphics = std::collections::BTreeMap::from([
        (SpriteId::named(300), standing),
        (
            SpriteId::named(301),
            Graphic {
                id: SpriteId::named(301),
                ..standing
            },
        ),
    ]);
    let scout = Master {
        id: ids::ID_448,
        kind: master_type::COMBAT,
        class: class::CLASS_47,
        standing_sprite: Some(SpriteId::named(300)),
        attack_sprite: Some(SpriteId::named(301)),
        weapon_range: Distance::new(1.0),
        reload: Seconds::new(2.0),
        ..Master::default()
    };
    let wolf = Master {
        id: master_id(126),
        kind: master_type::COMBAT,
        class: class::PREDATOR,
        ..Master::default()
    };
    let placed = |x: f32, class, player, master| Placed {
        position: Position::new(x, 10.0, 0.0),
        radius: Footprint::new(0.2, 0.2),
        class,
        player,
        master,
        obstruction_type: ObstructionType::NONE,
        buildable_on: 0,
        tall: true,
        speed: Speed::ZERO,
        unknown: None,
        obstruction: None,
    };
    let mut spatial = Spatial::new(GridSize::new(120, 120));
    spatial.insert(
        object_id(123),
        placed(10.0, class::CLASS_47, PlayerId::new(1), ids::ID_448),
    );
    spatial.insert(
        object_id(50),
        placed(10.5, class::PREDATOR, PlayerId::new(0), master_id(126)),
    );
    let gaia = Player {
        objects: vec![Unit {
            id: object_id(50),
            body: crate::objects::layers::Body::at(Position::new(10.5, 10.0, 0.0)),
            life: Lifecycle {
                master_id: master_id(126),
                ..Lifecycle::default()
            },
            ..Unit::default()
        }]
        .into(),
        ..Player::default()
    };
    let mut rng = Well512::default();
    let mut walker = crate::gathering::hunt::tests::walker();
    walker.stats.speed = Speed::new(1.2);
    let unit = Unit {
        id: object_id(123),
        body: crate::objects::layers::Body::new(Position::new(10.0, 10.0, 0.0), None, Some(walker)),
        sprite: Some(crate::objects::sprite::ActiveSprite::start(
            standing, &mut rng,
        )),
        attack_timer: Seconds::new(timer),
        actions: ActionList::default().with_attack(own.copied()),
        life: Lifecycle {
            master_id: ids::ID_448,
            inside,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let ai = UnitAi {
        stance: UnitStance::STAND_GROUND,
        current_order: None,
        ai_type: UnitAiType::SOLDIER,
        ..UnitAi::default()
    };
    let every = vec![Masters::new([wolf]), Masters::new([scout])];
    let (attacked, t) = on_side(
        (
            gaia,
            vec![unit],
            crate::players::tracker::Tracker::default(),
        ),
        (spatial, every, graphics),
        crate::groups::UnitGroups::default(),
        |side| side.attack(object_id(50), force, &ai).unwrap(),
    );
    // the takes reach Gaia's objects through the view: each counts a worker
    let taken = t.players[0].objects.iter();
    let takes = taken.flat_map(|u| std::iter::repeat_n(u.id.get(), u.workers as usize));
    (
        t.players[1].objects[ListIndex::new(0)].clone(),
        attacked,
        takes.collect(),
    )
}

/// The attack command of unit 123 of master `who` (in a table with `also`) on Gaia's wolf 50, as
/// [`scout_attack_on`], the unit in its owner's done tracker list of its master: the unit, the
/// answer, its owner's attribute 4 as the command returns and its owner's tracker after it.
fn attack_by(
    who: Master,
    also: Vec<Master>,
) -> (
    Unit,
    crate::unit_ai::Attacked,
    f32,
    crate::players::tracker::Tracker,
) {
    use crate::unit_ai::UnitSide;
    let (timer, inside, own, force) = (0.0, None, None, true);
    let attacker_master = who.id;
    let standing = Graphic {
        id: SpriteId::named(300),
        frames: 10,
        facets: 8,
        speed_multiplier: 1.0,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: 1,
    };
    let graphics = std::collections::BTreeMap::from([
        (SpriteId::named(300), standing),
        (
            SpriteId::named(301),
            Graphic {
                id: SpriteId::named(301),
                ..standing
            },
        ),
    ]);
    let scout = Master {
        id: ids::ID_448,
        kind: master_type::COMBAT,
        class: class::CLASS_47,
        standing_sprite: Some(SpriteId::named(300)),
        attack_sprite: Some(SpriteId::named(301)),
        weapon_range: Distance::new(1.0),
        reload: Seconds::new(2.0),
        ..Master::default()
    };
    let wolf = Master {
        id: master_id(126),
        kind: master_type::COMBAT,
        class: class::PREDATOR,
        ..Master::default()
    };
    let placed = |x: f32, class, player, master| Placed {
        position: Position::new(x, 10.0, 0.0),
        radius: Footprint::new(0.2, 0.2),
        class,
        player,
        master,
        obstruction_type: ObstructionType::NONE,
        buildable_on: 0,
        tall: true,
        speed: Speed::ZERO,
        unknown: None,
        obstruction: None,
    };
    let mut spatial = Spatial::new(GridSize::new(120, 120));
    spatial.insert(
        object_id(123),
        placed(10.0, class::CLASS_47, PlayerId::new(1), ids::ID_448),
    );
    spatial.insert(
        object_id(50),
        placed(10.5, class::PREDATOR, PlayerId::new(0), master_id(126)),
    );
    let gaia = Player {
        objects: vec![Unit {
            id: object_id(50),
            body: crate::objects::layers::Body::at(Position::new(10.5, 10.0, 0.0)),
            life: Lifecycle {
                master_id: master_id(126),
                ..Lifecycle::default()
            },
            ..Unit::default()
        }]
        .into(),
        ..Player::default()
    };
    let mut rng = Well512::default();
    let mut walker = crate::gathering::hunt::tests::walker();
    walker.stats.speed = Speed::new(1.2);
    let unit = Unit {
        id: object_id(123),
        body: crate::objects::layers::Body::new(Position::new(10.0, 10.0, 0.0), None, Some(walker)),
        sprite: Some(crate::objects::sprite::ActiveSprite::start(
            standing, &mut rng,
        )),
        attack_timer: Seconds::new(timer),
        actions: ActionList::default().with_attack(own),
        life: Lifecycle {
            master_id: attacker_master,
            inside,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let ai = UnitAi {
        stance: UnitStance::STAND_GROUND,
        current_order: None,
        ai_type: UnitAiType::SOLDIER,
        ..UnitAi::default()
    };
    let _ = scout;
    let masters = Masters::new(std::iter::once(who).chain(also));
    let every = vec![Masters::new([wolf]), masters];
    let lines = crate::objects::lines::UnitLines::default();
    let mut tracker = crate::players::tracker::Tracker::default();
    tracker.add(
        object_id(123),
        crate::players::tracker::key(every.get(1), &lines, attacker_master),
    );
    let ((attacked, adds), t) = on_side(
        (gaia, vec![unit], tracker),
        (spatial, every, graphics),
        crate::groups::UnitGroups::default(),
        |side| {
            let attacked = side.attack(object_id(50), force, &ai).unwrap();
            (attacked, side.c.w.players[1].attributes[4].get())
        },
    );
    let unit = t.players[1].objects[ListIndex::new(0)].clone();
    (unit, attacked, adds, t.players[1].tracker.clone())
}

/// A villager's attack command (`FUN_141081a80`): a builder (118, task swap group 1) switches to
/// its player's villager 83 first (slot 0x90 with 0: its storage leaves and the new one's comes,
/// the tracker promotes it), then attacks with the new master.
#[test]
fn a_villager_s_attack_command_switches_it_to_the_villager_first() {
    let villager = |id, storage| Master {
        id,
        kind: master_type::COMBAT,
        class: class::VILLAGER,
        task_by_group: 1,
        hit_points: 25,
        standing_sprite: Some(SpriteId::named(300)),
        attack_sprite: Some(SpriteId::named(301)),
        weapon_range: Distance::new(0.5),
        reload: Seconds::new(2.0),
        storage,
        moving: Some(crate::gathering::hunt::tests::walker().stats),
        ..Master::default()
    };
    let (unit, attacked, housed, tracker) = attack_by(
        villager(
            ids::BUILDER,
            vec![(Some(attribute::HEADROOM), Amount::new(1.0), 2)],
        ),
        vec![villager(
            VILLAGER_MALE,
            vec![(Some(attribute::HEADROOM), Amount::new(3.0), 2)],
        )],
    );
    assert_eq!(unit.life.master_id, VILLAGER_MALE);
    assert!(attacked.attacks);
    assert!(
        unit.actions
            .attack()
            .is_some_and(|a| a.target == Some(object_id(50)))
    );
    // the owner's attribute 4 (10 before) as the attack command returns
    assert_eq!(housed, 10.0 - 1.0 + 3.0);
    assert_eq!(
        (
            tracker.list(ListKey::master(master_id(118))),
            tracker.list(ListKey::master(master_id(83)))
        ),
        (&[][..], &[Some(object_id(123))][..])
    );
}

/// The scout 124 (player 1, master 448, at (10.3, 10), its unit AI `helper`, its own Attack
/// `own`), a helper against Gaia's wolf 50 asked by the scout 123.
fn helper_scout(helper: UnitAi, own: Option<&crate::combat::attack::Attack>) -> Unit {
    Unit {
        id: object_id(124),
        body: crate::objects::layers::Body::at(Position::new(10.3, 10.0, 0.0)),
        actions: ActionList::default().with_attack(own.copied()),
        ai: Some(helper),
        life: Lifecycle {
            master_id: ids::ID_448,
            state: ObjectState::Alive,
            ..Lifecycle::default()
        },
        ..Unit::default()
    }
}

/// `AskForHelp`'s world: the scout 123's view (player 1, master 448, at (10, 10)) with the
/// `helper` among its owner's other objects, Gaia's wolf 50 at (10.5, 10), the lists the last
/// search left (`searched`, none before any) and the world's unit groups `groups`; `f` runs on
/// the view.
fn help_view<R>(
    helper: Unit,
    searched: Option<crate::players::ByRelation<Vec<crate::vision::registry::Record>>>,
    groups: &crate::groups::UnitGroups,
    f: impl FnOnce(&mut crate::unit_ai::world_view::UnitView<'_, '_>) -> R,
) -> R {
    let scout = Master {
        id: ids::ID_448,
        kind: master_type::COMBAT,
        class: class::CLASS_47,
        weapon_range: Distance::new(1.0),
        weapons: vec![ClassAmount::new(4, 3)],
        ..Master::default()
    };
    let wolf = Master {
        id: master_id(126),
        kind: master_type::COMBAT,
        class: class::PREDATOR,
        combat_level: 4,
        ..Master::default()
    };
    let placed = |x: f32, class, player, master| Placed {
        position: Position::new(x, 10.0, 0.0),
        radius: Footprint::new(0.2, 0.2),
        class,
        player,
        master,
        obstruction_type: ObstructionType::NONE,
        buildable_on: 0,
        tall: true,
        speed: Speed::ZERO,
        unknown: None,
        obstruction: None,
    };
    let mut spatial = Spatial::new(GridSize::new(120, 120));
    spatial.insert(
        object_id(123),
        placed(10.0, class::CLASS_47, PlayerId::new(1), ids::ID_448),
    );
    spatial.insert(
        object_id(124),
        placed(10.3, class::CLASS_47, PlayerId::new(1), ids::ID_448),
    );
    spatial.insert(
        object_id(50),
        placed(10.5, class::PREDATOR, PlayerId::new(0), master_id(126)),
    );
    let gaia = Player {
        objects: vec![Unit {
            id: object_id(50),
            body: crate::objects::layers::Body::at(Position::new(10.5, 10.0, 0.0)),
            life: Lifecycle {
                master_id: master_id(126),
                ..Lifecycle::default()
            },
            ..Unit::default()
        }]
        .into(),
        ..Player::default()
    };
    if let Some(lists) = searched {
        spatial.set_searched(lists);
    }
    let unit = Unit {
        id: object_id(123),
        body: crate::objects::layers::Body::at(Position::new(10.0, 10.0, 0.0)),
        life: Lifecycle {
            master_id: ids::ID_448,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let masters = Masters::new([scout]);
    let every = vec![Masters::new([wolf]), masters];
    let tracker = crate::players::tracker::Tracker::default();
    let graphics = std::collections::BTreeMap::new();
    let (answer, _) = on_side(
        (gaia, vec![unit, helper], tracker),
        (spatial, every, graphics),
        groups.clone(),
        f,
    );
    answer
}

/// `AskForHelp`'s helper test ([`UnitView::helper_free`]) of [`helper_scout`] against the wolf.
///
/// [`UnitView::helper_free`]: crate::unit_ai::world_view::UnitView::helper_free
fn helper_free(helper: UnitAi, own: Option<&crate::combat::attack::Attack>) -> bool {
    helper_free_of(
        PlayerId::new(1),
        helper_scout(helper, own),
        &crate::groups::UnitGroups::default(),
    )
}

/// [`helper_free`] of the unit `helper` of player `owner` with the world's unit groups `groups`.
fn helper_free_of(owner: PlayerId, helper: Unit, groups: &crate::groups::UnitGroups) -> bool {
    help_view(helper, None, groups, |side| {
        let scout =
            crate::objects::masters::player_master(&side.c.w.masters, side.player, ids::ID_448)
                .unwrap()
                .clone();
        let helper = OthersRef::of(&side.c.w.players, PlayerId::new(1))
            .unwrap()
            .own_iter()
            .next()
            .unwrap();
        side.helper_free(owner, helper, &scout, object_id(50))
            .unwrap()
    })
}

/// `AskForHelp` (`FUN_140c42ce0`) asks only an idle helper in no group (no action value, action,
/// order, queued order or target) whose attack check lets it attack the attacker; a helper whose
/// top action's target lies within its range keeps it.
#[test]
fn ask_for_help_asks_only_an_idle_helper_that_may_attack() {
    let idle = UnitAi {
        ai_type: UnitAiType::SOLDIER,
        current_order: None,
        current_action: None,
        current_target: None,
        stance: UnitStance::AGGRESSIVE,
        ..UnitAi::default()
    };
    assert!(helper_free(idle.clone(), None));
    let busy = [
        UnitAi {
            current_action: Some(ActionKind::Attack),
            ..idle.clone()
        },
        UnitAi {
            current_order: Some(OrderKind::Attack),
            ..idle.clone()
        },
        UnitAi {
            current_target: Some(object_id(50)),
            ..idle.clone()
        },
    ];
    for ai in busy {
        assert!(!helper_free(ai, None));
    }
    // an action of its own makes it busy
    assert!(!helper_free(
        idle,
        Some(&own_attack(50, -1, AttackState::Move))
    ));
}

/// `AskForHelp`'s helper in a group answers whatever it does while its group patrols, and not
/// while it does not; a patrolling helper whose own Attack's target lies within its range keeps
/// it. A unit without a unit AI never answers; the helper of another player than the asker's
/// answers as one of its own.
#[test]
fn a_helper_in_a_group_answers_only_while_its_group_patrols() {
    let idle = UnitAi {
        ai_type: UnitAiType::SOLDIER,
        current_order: None,
        current_action: None,
        current_target: None,
        stance: UnitStance::AGGRESSIVE,
        ..UnitAi::default()
    };
    let busy = UnitAi {
        current_order: Some(OrderKind::Attack),
        current_action: Some(ActionKind::Attack),
        ..idle.clone()
    };
    let groups = crate::groups::UnitGroups {
        groups: vec![
            crate::groups::UnitGroup {
                flags: 0x800,
                ..crate::groups::UnitGroup::new(group_id(7))
            },
            crate::groups::UnitGroup::new(group_id(8)),
        ],
        ..crate::groups::UnitGroups::default()
    };
    let in_group = |group, own| {
        let mut u = helper_scout(busy.clone(), own);
        u.group.id = group;
        u
    };
    assert!(helper_free_of(
        PlayerId::new(1),
        in_group(Some(group_id(7)), None),
        &groups
    ));
    assert!(!helper_free_of(
        PlayerId::new(1),
        in_group(Some(group_id(8)), None),
        &groups
    ));
    assert!(!helper_free_of(
        PlayerId::new(1),
        in_group(
            Some(group_id(7)),
            Some(&own_attack(50, -1, AttackState::Move))
        ),
        &groups
    ));
    let mut thoughtless = helper_scout(idle.clone(), None);
    thoughtless.ai = None;
    assert!(!helper_free_of(PlayerId::new(1), thoughtless, &groups));
    assert!(helper_free_of(
        PlayerId::new(2),
        helper_scout(idle, None),
        &groups
    ));
}

/// `FUN_141081a80` on the target of the unit's own Attack: it answers true; forced, the Attack's
/// slot 0x48 (`FUN_140a841d0`) runs on the target first. In state 1 the scout, which may not move
/// (stance 2, no order), keeps the wolf in its reach: where it saw it becomes its place, the
/// target the same (nothing taken). In state 4 the Attack keeps it as it is; unforced nothing
/// changes either.
#[test]
fn an_attack_command_on_its_own_attack_s_target_asks_that_attack_again() {
    let own = |state: AttackState| crate::combat::attack::Attack {
        state,
        seen: Position::new(-1.0, -1.0, -1.0),
        ..own_attack(50, -1, state)
    };
    let (unit, attacked, takes) = scout_attack_on(0.0, None, Some(&own(AttackState::Done)), true);
    assert_eq!((attacked.attacks, takes), (true, vec![]));
    let a = unit.actions.attack().copied().expect("the scout's Attack");
    assert_eq!(
        (a.state, a.target, a.seen),
        (
            AttackState::Done,
            Some(object_id(50)),
            Position::new(10.5, 10.0, 0.0)
        )
    );
    let (unit, attacked, _) = scout_attack_on(0.0, None, Some(&own(AttackState::Move)), true);
    assert!(attacked.attacks);
    assert_eq!(
        unit.actions.attack().map(|a| a.seen),
        Some(Position::from([-1.0; 3]))
    );
    let (unit, _, _) = scout_attack_on(0.0, None, Some(&own(AttackState::Done)), false);
    assert_eq!(
        unit.actions.attack().map(|a| a.seen),
        Some(Position::from([-1.0; 3]))
    );
}

/// `FUN_141081a80`'s start (`0x140a7f8c0`) of a unit that cannot move (stance 2, no order) in
/// reach: `set_state(6)` at once (426049998's scout 123 in update 1222). Its readyToAttack finds
/// the target on the point; facing it with its attack timer run out the scout swings
/// (`set_state(7)`: the attack sprite, a hit pending); with time left it waits in state 6 on the
/// standing sprite. The constructor takes the target either way. A unit inside another fails at
/// the start (`set_state(13)`: speed 1, the standing sprite).
#[test]
fn an_attack_command_of_a_unit_that_cannot_move_goes_ready_and_swings_at_once() {
    let (unit, attacked, takes) = scout_attack(0.0, None);
    assert_eq!((attacked.attacks, attacked.posted), (true, vec![]));
    assert_eq!(takes, vec![50]);
    let attack = unit.actions.attack().copied().expect("the scout's Attack");
    assert_eq!(
        (attack.state, attack.pending, attack.point),
        (AttackState::Swing, true, Position::new(10.5, 10.0, 0.0))
    );
    assert_eq!(
        unit.sprite.map(|s| s.graphic.id),
        Some(SpriteId::named(301))
    );
    let (unit, _, _) = scout_attack(0.5, None);
    let attack = unit.actions.attack().copied().expect("the scout's Attack");
    assert_eq!((attack.state, attack.pending), (AttackState::Ready, false));
    assert_eq!(
        unit.sprite.map(|s| s.graphic.id),
        Some(SpriteId::named(300))
    );
    let (unit, attacked, takes) = scout_attack(0.0, Some(object_id(9149)));
    assert_eq!((attacked.attacks, takes), (true, vec![50]));
    assert_eq!(
        unit.actions.attack().map(|a| a.state),
        Some(AttackState::Failed)
    );
    assert_eq!(unit.body.anim_speed(), Some(1.0));
}

/// A house's movement values: it never moves (speed 0).
const fn house_moves() -> crate::movement::MoveStats {
    crate::movement::MoveStats {
        fly_mode: 0,
        speed: Speed::ZERO,
        base: Speed::ZERO,
        turn_rate: TurnRate::MAX,
        turn_radius: Distance::ZERO,
        turn_radius_speed: Speed::MAX,
        radius: Radii::new(1.0, 1.0, 1.0),
        restriction: RestrictionId::named(7),
        restriction_mask: TerrainMask::of_bits(6),
        turn_time: Seconds::ZERO,
        move_algorithm: 0,
        min_collision: 1.0,
        fast_turn: false,
        facets: None,
        weapon_range: Distance::ZERO,
    }
}

/// BUILD's foundation is a moving object (type 80): the moving setup (`FUN_1411580a0`) gives it
/// its movement state, though it never moves (speed 0), and the position setter makes its place
/// a move at the world time (`+0x870`), which the walk reads of a building it meets (slot 0x5e8,
/// `FUN_14106d2d0`): a foundation made after a unit's path was can be kept as a hit.
#[test]
fn a_build_command_s_foundation_holds_its_movement_state() {
    let mut masters = builder_masters();
    let house = masters.get_mut(ids::HOUSE).unwrap();
    house.moving = Some(house_moves());
    let mut t = World::new(
        WorldTime::ZERO,
        29910,
        PlayerId::new(1),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player::default(),
            Player {
                objects: vec![Unit {
                    id: object_id(9175),
                    body: crate::objects::layers::Body::at(Position::new(30.0, 89.0, 0.0)),
                    life: Lifecycle {
                        class: Class::Animated,
                        state: ObjectState::Alive,
                        hp: HitPoints::new(25.0),
                        master_id: VILLAGER_MALE,
                        ..Lifecycle::default()
                    },
                    ai: Some(UnitAi {
                        ai_type: UnitAiType::CIVILIAN,
                        current_order: None,
                        current_action: None,
                        current_target: None,
                        ..UnitAi::default()
                    }),
                    ..Unit::default()
                }]
                .into(),
                attributes: crate::test_ids::attributes(250, [200.0, 100.0, 200.0, 100.0]),
                ..Player::default()
            },
        ],
    );
    t.next_object_id = object_id(10028);
    t.world_time = WorldTime::from_millis(72890);
    t.masters = vec![Masters::default(), masters];
    placeable(&mut t);
    t.run(&Command::Build(BuildCommand {
        player: player_id(1),
        master: master_id(70),
        position: Point::new(30.0, 88.0),
        q_id: -1,
        sprite_override: -1,
        builders: unit_list(object_ids([9175])),
        queued: false,
        commits: false,
        frame: 0,
    }));
    assert_eq!(t.players[1].unsupported, None);
    let foundation = t.players[1].objects.last().unwrap();
    assert_eq!(foundation.id, object_id(10028));
    let m = foundation
        .body
        .mover()
        .expect("a building's movement state");
    assert_eq!(
        (m.forward, m.waiting, m.last_move, m.moved_at, m.paths.len()),
        (
            BVector::new(1.0, 0.0, 0.0),
            0,
            WorldTime::from_millis(72890),
            Position::new(30.0, 88.0, 0.0),
            0
        )
    );
    t.sync_world();
    let gait = t
        .spatial
        .gait(object_id(10028))
        .expect("the walk's view of the building");
    assert_eq!(
        (gait.moving, gait.last_move, gait.pause),
        (false, WorldTime::from_millis(72890), None)
    );
}

/// BUILD's foundation takes the setups' draws a new unit takes (`FUN_1411aca00`'s combat setup
/// `FUN_14121df60` and the setups it runs first), with its master's standing sprite current
/// (slot 0x798, `FUN_1412029f0`): a sprite of several facets with sequence flag 2 draws the
/// facet the building faces (the moving setup `FUN_141161450`, "setup" line 0x2c3), before the
/// powerup offset (line 0x1c1). 429730863's palisade 15472 (standing sprite 588: 5 facets,
/// sequence type 3) faces facet 4, 5.0265484.
#[test]
fn a_build_command_s_foundation_draws_its_facing_before_its_powerup_offset() {
    let build = |flags: u8| {
        let mut masters = builder_masters();
        let house = masters.get_mut(ids::HOUSE).unwrap();
        house.standing_sprite = Some(SpriteId::named(588));
        house.moving = Some(house_moves());
        let mut t = World::new(
            WorldTime::ZERO,
            29910,
            PlayerId::new(1),
            vec![Millis::ZERO, Millis::ZERO],
            vec![
                Player::default(),
                Player {
                    objects: vec![Unit {
                        id: object_id(9175),
                        body: crate::objects::layers::Body::at(Position::new(30.0, 89.0, 0.0)),
                        life: Lifecycle {
                            class: Class::Animated,
                            state: ObjectState::Alive,
                            hp: HitPoints::new(25.0),
                            master_id: VILLAGER_MALE,
                            ..Lifecycle::default()
                        },
                        ai: Some(UnitAi {
                            ai_type: UnitAiType::CIVILIAN,
                            current_order: None,
                            current_action: None,
                            current_target: None,
                            ..UnitAi::default()
                        }),
                        ..Unit::default()
                    }]
                    .into(),
                    attributes: crate::test_ids::attributes(250, [200.0, 100.0, 200.0, 100.0]),
                    ..Player::default()
                },
            ],
        );
        t.next_object_id = object_id(10028);
        t.masters = vec![Masters::default(), masters];
        t.streams.seed_all(0x5eed);
        let _ = t.graphics.insert(
            SpriteId::named(588),
            Graphic {
                id: SpriteId::named(588),
                frames: 1,
                facets: 5,
                speed_multiplier: 1.0,
                frame_duration: Seconds::ZERO,
                replay_delay: Seconds::ZERO,
                flags,
            },
        );
        placeable(&mut t);
        let before = t.streams.get(Stream::Object).clone();
        t.run(&Command::Build(BuildCommand {
            player: player_id(1),
            master: master_id(70),
            position: Point::new(30.0, 88.0),
            q_id: -1,
            sprite_override: -1,
            builders: unit_list(object_ids([9175])),
            queued: false,
            commits: false,
            frame: 0,
        }));
        assert_eq!(t.players[1].unsupported, None);
        (t, before)
    };
    // sequence type 3: the facing's draw, then the powerup offset's
    let (mut t, mut before) = build(3);
    let facing = before.rand15();
    let _ = before.rand15();
    assert_eq!(t.streams.get(Stream::Object), &before);
    let facets = crate::objects::facing::SpriteFacets {
        count: 5,
        flag_1: true,
    };
    let foundation = t.players[1].objects.last().unwrap();
    let angle = crate::objects::facing::drawn_angle(facing, facets);
    assert_ne!(angle, Angle::ZERO);
    assert_eq!(foundation.body.mover().map(|m| m.angle), Some(angle));
    // without sequence flag 2 only the powerup offset's
    let (mut t, mut before) = build(1);
    let _ = before.rand15();
    assert_eq!(t.streams.get(Stream::Object), &before);
    let foundation = t.players[1].objects.last().unwrap();
    assert_eq!(foundation.body.mover().map(|m| m.angle), Some(Angle::ZERO));
}

/// [`builder_masters`] with the farmer (259, of the villagers' group 1, which builds as 118)
/// and the farm (50: class 49, its own base, available, 60 wood, a building part of its own).
fn farm_masters() -> Masters {
    let builders = builder_masters();
    let farmer = Master {
        id: master_id(259),
        ..builders.get(VILLAGER_MALE).unwrap().clone()
    };
    let farm = Master {
        id: ids::FARM,
        terrain_restriction: Some(RestrictionId::named(0)),
        base_id: Some(ids::FARM),
        class: class::FARM,
        available: true,
        radius: Footprint::new(1.5, 1.5),
        clearance: Footprint::new(1.5, 1.5),
        tall: false,
        cost: costs(&[(attribute::WOOD, 60, true)]),
        hit_points: 480,
        build_time: 15,
        ..builders.get(ids::HOUSE).unwrap().clone()
    };
    Masters::new(builders.iter().cloned().chain([farmer, farm]))
}

/// Player 1's farmer 9175 back at its farm spot (30.5, 88.5), its walk back (state 0xf2)
/// ended; the player holds 100 wood.
fn reseed_world() -> World {
    let task = crate::gathering::hunt::HuntTask {
        id: TaskId::test(2),
        range: Distance::new(0.25),
        resource: Some(attribute::MEAT),
        sprites: TaskSprites::NONE,
        work_value: 1.0,
        productivity: None,
        kind4: None,
    };
    let farm = crate::gathering::gather::Resource {
        id: object_id(12557),
        position: Position::new(30.5, 88.5, 0.0),
        class: class::FARM,
        master: ids::FARM,
        player: PlayerId::new(1),
        workers: 0,
        drop_site: false,
        gap: Distance::ZERO,
        state: ObjectState::Alive,
        radius: Footprint::new(1.5, 1.5),
    };
    let rate = crate::gathering::gather::StopRate {
        work_rate: WorkRate::new(1.0),
        capacity: Amount::new(10.0),
        productivity: 1.0,
    };
    let gather = crate::gathering::gather::GatherAction {
        state: GatherState::ReseedWalk,
        target: None,
        worked: true,
        ..crate::gathering::gather::GatherAction::new(task, &farm, false, rate).unwrap()
    };
    let farmer = Unit {
        id: object_id(9175),
        body: crate::objects::layers::Body::at(Position::new(30.5, 88.5, 0.0)),
        life: Lifecycle {
            class: Class::Animated,
            state: ObjectState::Alive,
            hp: HitPoints::new(25.0),
            master_id: master_id(259),
            ..Lifecycle::default()
        },
        actions: ActionList::default().with_gather(Some(gather)),
        ai: Some(UnitAi {
            ai_type: UnitAiType::CIVILIAN,
            current_order: None,
            current_action: None,
            current_target: None,
            ..UnitAi::default()
        }),
        ..Unit::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        29910,
        PlayerId::new(1),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player::default(),
            Player {
                objects: vec![farmer].into(),
                attributes: crate::test_ids::attributes(250, [0.0, 100.0, 0.0, 0.0]),
                ..Player::default()
            },
        ],
    );
    t.next_object_id = object_id(10028);
    t.masters = vec![Masters::default(), farm_masters()];
    placeable(&mut t);
    t
}

/// `FUN_140a8aa60` with the automatic farm queue: the farmer pays a farm (60 wood), its
/// foundation goes on the farm spot and the farmer builds it (as 118, its Gather gone).
#[test]
fn a_farmer_whose_farm_ran_out_reseeds_it_with_the_automatic_queue() {
    let mut t = reseed_world();
    t.players[1].auto_farm_queue = true;
    t.reseed_after_walk(PlayerId::new(1), ListIndex::new(0), true)
        .unwrap();
    let p = &t.players[1];
    assert_eq!(p.attributes[1], Amount::new(40.0));
    assert_eq!(p.farm_queue, 0);
    let foundation = p.objects.last().unwrap();
    assert_eq!(
        (
            foundation.id.get(),
            foundation.life.master_id.get(),
            foundation.position()
        ),
        (10028, 50, Position::new(30.5, 88.5, 0.0))
    );
    let farmer = &p.objects[ListIndex::new(0)];
    assert_eq!(farmer.life.master_id, ids::BUILDER);
    assert_eq!(
        farmer.actions.build().map(|b| b.target),
        Some(Some(object_id(10028)))
    );
    assert!(farmer.actions.gather().is_none());
}

/// Without the automatic queue the farm comes from the mill's queue (one less) unpaid; with
/// it but too little wood, from the queue too.
#[test]
fn a_reseed_takes_a_queued_farm() {
    for (auto, wood) in [(false, 100.0), (true, 59.0)] {
        let mut t = reseed_world();
        t.players[1].auto_farm_queue = auto;
        t.players[1].attributes[1] = Amount::new(wood);
        t.players[1].farm_queue = 2;
        t.reseed_after_walk(PlayerId::new(1), ListIndex::new(0), true)
            .unwrap();
        let p = &t.players[1];
        assert_eq!((p.attributes[1], p.farm_queue), (Amount::new(wood), 1));
        assert_eq!(p.objects.last().unwrap().life.master_id, ids::FARM);
    }
}

/// Without a farm to make (nothing queued, no automatic queue, or no wood) the reseed makes
/// nothing: a human's farmer idles, a computer's is done.
#[test]
fn without_a_farm_to_make_the_farmer_idles() {
    let mut t = reseed_world();
    t.reseed_after_walk(PlayerId::new(1), ListIndex::new(0), true)
        .unwrap();
    let p = &t.players[1];
    assert_eq!(p.objects.len(), 1);
    assert_eq!(
        p.objects[ListIndex::new(0)]
            .actions
            .gather()
            .map(|g| g.state),
        Some(GatherState::Idle)
    );
    let mut t = reseed_world();
    t.players[1].auto_farm_queue = true;
    t.players[1].attributes[1] = Amount::new(59.0);
    t.players[1].computer = true;
    t.reseed_after_walk(PlayerId::new(1), ListIndex::new(0), true)
        .unwrap();
    let p = &t.players[1];
    assert_eq!((p.objects.len(), p.attributes[1]), (1, Amount::new(59.0)));
    assert_eq!(
        p.objects[ListIndex::new(0)]
            .actions
            .gather()
            .map(|g| g.state),
        Some(GatherState::Done)
    );
}

/// [`reseed_world`] with one farm queued, `setup` run on it, then the reseed itself
/// ([`World::reseed`]): its answer and the count of player 1's objects after it.
fn reseeded(setup: impl FnOnce(&mut World)) -> (Result<bool, GenieError>, usize) {
    let mut t = reseed_world();
    t.players[1].farm_queue = 1;
    setup(&mut t);
    let made = t.reseed(PlayerId::new(1), ListIndex::new(0));
    (made, t.players[1].objects.len())
}

/// `FUN_140a8aa60` makes nothing for a gatherer without a Gather or without a farm spot, for a
/// villager with only fish traps queued, for a fishing ship (class 21) with only farms queued and
/// for a farm the player cannot make (its master unavailable).
#[test]
fn a_reseed_without_a_farm_to_make_makes_nothing() {
    let none: [fn(&mut World); 5] = [
        |t| {
            t.players[1].objects[ListIndex::new(0)]
                .actions
                .set_gather(None);
        },
        |t| {
            let g = t.players[1].objects[ListIndex::new(0)]
                .actions
                .gather_mut()
                .unwrap();
            g.farm = Point::new(-1.0, -1.0);
        },
        |t| (t.players[1].farm_queue, t.players[1].fish_trap_queue) = (0, 1),
        |t| t.masters[1].get_mut(master_id(259)).unwrap().class = class::FISHING_SHIP,
        |t| t.masters[1].get_mut(ids::FARM).unwrap().available = false,
    ];
    for setup in none {
        assert_eq!(reseeded(setup), (Ok(false), 1));
    }
}

/// A fishing ship (class 21) reseeds a fish trap (199) from the dock's queue (one less), with
/// the automatic fish trap queue paying it (60 wood) instead.
#[test]
fn a_fishing_ship_reseeds_a_fish_trap() {
    let ship = |t: &mut World| {
        let trap = Master {
            id: FISH_TRAP,
            base_id: Some(FISH_TRAP),
            ..t.masters[1].get(ids::FARM).unwrap().clone()
        };
        let ms = &mut t.masters[1];
        // a ship builds as itself: its own Build task, no group
        let tasks = ms.get(ids::BUILDER).unwrap().tasks.clone();
        let ship = ms.get_mut(master_id(259)).unwrap();
        (ship.class, ship.task_by_group, ship.tasks) = (class::FISHING_SHIP, 0, tasks);
        *ms = Masters::new(ms.iter().cloned().chain([trap]));
        t.players[1].farm_queue = 0;
    };
    let mut t = reseed_world();
    ship(&mut t);
    t.players[1].fish_trap_queue = 2;
    assert_eq!(t.reseed(PlayerId::new(1), ListIndex::new(0)), Ok(true));
    let p = &t.players[1];
    assert_eq!(
        (p.attributes[1], p.fish_trap_queue),
        (Amount::new(100.0), 1)
    );
    let trap = p.objects.last().unwrap();
    assert_eq!(
        (trap.life.master_id, trap.position()),
        (FISH_TRAP, Position::new(30.5, 88.5, 0.0))
    );
    let mut t = reseed_world();
    ship(&mut t);
    t.players[1].auto_fishtrap_queue = true;
    assert_eq!(t.reseed(PlayerId::new(1), ListIndex::new(0)), Ok(true));
    let p = &t.players[1];
    assert_eq!((p.attributes[1], p.fish_trap_queue), (Amount::new(40.0), 0));
}

/// The farm's placement test passes over the gather's old target (the spent farm 12557 on the
/// farm spot, a building): the new foundation goes over it. The same farm when it is not the
/// target fails the test, and nothing is made.
#[test]
fn a_reseed_s_placement_passes_over_the_old_farm() {
    let old_farm = |target: i32| {
        move |t: &mut World| {
            let farm = Unit {
                id: object_id(12557),
                body: crate::objects::layers::Body::at(Position::new(30.5, 88.5, 0.0)),
                life: Lifecycle {
                    class: Class::Animated,
                    state: ObjectState::Alive,
                    hp: HitPoints::new(480.0),
                    master_id: ids::FARM,
                    ..Lifecycle::default()
                },
                ..Unit::default()
            };
            t.players[1].objects.push(farm);
            open_map(t);
            let at = Placed {
                position: Position::new(30.5, 88.5, 0.0),
                radius: Footprint::new(1.5, 1.5),
                class: class::FARM,
                player: PlayerId::new(1),
                master: ids::FARM,
                obstruction_type: ObstructionType::TYPE_2,
                buildable_on: 0,
                tall: true,
                speed: Speed::ZERO,
                unknown: None,
                obstruction: None,
            };
            t.spatial.insert(object_id(12557), at);
            t.players[1].objects[ListIndex::new(0)]
                .actions
                .gather_mut()
                .unwrap()
                .target = ObjectId::of_field(target);
        }
    };
    assert_eq!(reseeded(old_farm(12557)), (Ok(true), 3));
    assert_eq!(reseeded(old_farm(-1)), (Ok(false), 2));
}

/// A committing BUILD (`+0x1f` 0) to a builder whose unit AI holds an order not ready
/// (`FUN_140c4dfa0`): the build order joins its queue, then the readiness pass with 0
/// (`FUN_140c42b60` at 0x140e7b252) makes every queued order ready, the ready ones kept, and
/// the unit is not stopped.
#[test]
fn a_committing_build_or_wall_to_a_busy_builder_readies_its_queue() {
    let unready = crate::unit_ai::QueuedOrder {
        order: OrderKind::Move,
        ready: false,
        ..crate::unit_ai::QueuedOrder::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        29910,
        PlayerId::new(1),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player::default(),
            Player {
                objects: vec![Unit {
                    id: object_id(9175),
                    body: crate::objects::layers::Body::at(Position::new(30.0, 89.0, 0.0)),
                    life: Lifecycle {
                        class: Class::Animated,
                        state: ObjectState::Alive,
                        hp: HitPoints::new(25.0),
                        master_id: VILLAGER_MALE,
                        ..Lifecycle::default()
                    },
                    ai: Some(UnitAi {
                        ai_type: UnitAiType::CIVILIAN,
                        current_order: Some(OrderKind::Move),
                        current_action: None,
                        current_target: None,
                        orders: vec![unready],
                        ..UnitAi::default()
                    }),
                    ..Unit::default()
                }]
                .into(),
                attributes: crate::test_ids::attributes(250, [200.0, 100.0, 200.0, 100.0]),
                ..Player::default()
            },
        ],
    );
    t.next_object_id = object_id(10028);
    t.masters = vec![Masters::default(), builder_masters()];
    placeable(&mut t);
    let mut wall = t.clone();
    t.run(&Command::Build(BuildCommand {
        player: player_id(1),
        master: master_id(70),
        position: Point::new(30.0, 88.0),
        q_id: -1,
        sprite_override: -1,
        builders: unit_list(object_ids([9175])),
        queued: false,
        commits: true,
        frame: 0,
    }));
    assert_eq!(t.players[1].unsupported, None);
    let ai = t.players[1].objects[ListIndex::new(0)].ai.as_ref().unwrap();
    let queue: Vec<_> = ai.orders.iter().map(|o| (o.order, o.ready)).collect();
    assert_eq!(queue, [(OrderKind::Move, true), (OrderKind::Build, true)]);
    assert_eq!(ai.current_order, Some(OrderKind::Move));
    // BUILD_WALL's line of one tile the same (0x140e7be4b)
    wall.run(&Command::BuildWall(BuildWallCommand {
        player: player_id(1),
        master: master_id(70),
        from: [30, 88],
        to: [30, 88],
        q_id: -1,
        builders: unit_list(object_ids([9175])),
        queued: false,
        commits: true,
    }));
    assert_eq!(wall.players[1].unsupported, None);
    let ai = wall.players[1].objects[ListIndex::new(0)]
        .ai
        .as_ref()
        .unwrap();
    let queue: Vec<_> = ai.orders.iter().map(|o| (o.order, o.ready)).collect();
    assert_eq!(queue, [(OrderKind::Move, true), (OrderKind::Build, true)]);
}

/// `ClosestResourceObject`'s point (`FUN_140c49760`): without a site the search starts from the
/// point at the unit's height; (−1, −1) exactly, or a site, keeps the unit's place; one part −1
/// or not a number is a point.
#[test]
fn a_resource_search_starts_from_its_point_unless_it_is_none_or_has_a_site() {
    let unit = Position::new(5.0, 6.0, 0.5);
    assert_eq!(
        search_origin(Point::new(12.5, 30.5), false, unit),
        Position::new(12.5, 30.5, 0.5)
    );
    assert_eq!(search_origin(crate::unit_ai::NO_POINT, false, unit), unit);
    assert_eq!(search_origin(Point::new(12.5, 30.5), true, unit), unit);
    assert_eq!(
        search_origin(Point::new(-1.0, 3.0), false, unit),
        Position::new(-1.0, 3.0, 0.5)
    );
    assert!(
        search_origin(Point::new(f32::NAN, -1.0), false, unit)
            .x()
            .is_nan()
    );
}

/// Where a stop lands its error names what was running: a command's stop the command, an
/// object's freeze the object, its master, its top action and its unit AI's order.
#[test]
fn a_stop_s_error_names_the_command_or_the_object_it_happened_on() {
    let why = GenieError::unsupported("QUEUE at two buildings");
    let mut t = one_player(vec![town_centre(9)], [200.0; 4]);
    stock_masters(&mut t);
    t.tick_with(
        Millis::new(104),
        &[Command::Unmodelled(UnmodelledCommand {
            player: player_id(0),
            packet: Vec::new(),
            why,
        })],
    );
    let (_, _, stopped) = t.players[0].unsupported.clone().unwrap();
    assert_eq!(
        stopped.context().collect::<Vec<_>>(),
        [("command", "Unmodelled")]
    );
    let task = GenieError::unsupported("a default task the slice does not model");
    let mut t = World::new(
        WorldTime::ZERO,
        1,
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![player(vec![])],
    );
    t.players[0].objects.push(Unit {
        id: object_id(5),
        unmodelled_action: Some(task),
        life: Lifecycle {
            master_id: VILLAGER_MALE,
            ..Lifecycle::default()
        },
        ..Unit::default()
    });
    stock_masters(&mut t);
    t.tick(Millis::new(10));
    let (_, _, frozen) = t.players[0].freezes[0].clone();
    assert_eq!(
        frozen.context().collect::<Vec<_>>(),
        [("object", "5"), ("master", "83")]
    );
}

/// `FUN_1411671e0`'s planner results: a path to a drop site or into its range counts (0 and 1),
/// a partial one never (2); one the planner does not reach (3 and up) counts only for a fishing
/// ship seeking by path whose footprint gap to it is within 100 squared (429521544's p3 in
/// update 5601).
#[test]
fn a_fishing_ship_counts_a_near_drop_site_its_path_does_not_reach() {
    use crate::gathering::drop_sites::counted;
    assert!(counted(0, true, false, || false));
    assert!(counted(1, false, false, || false));
    assert!(!counted(2, true, true, || true));
    assert!(!counted(3, true, false, || true));
    assert!(counted(3, true, true, || true));
    assert!(!counted(3, true, true, || false));
    assert!(!counted(3, false, true, || true));
    assert!(counted(4, true, true, || true));
}

/// `TRIBE_Building_Object::update`'s farm stage (`0x1411ca2ef`): a farm lays 29 to 31 by its
/// build points over its build time (times 3, at most 2); a rice farm or a pasture on mangrove
/// or rice 65 to 67, elsewhere its foundation terrain plus 2 (63 and 117: 65.., 119..); another
/// master none (427445417's p4 in update 16981, 427557935's p4 in 22813).
#[test]
fn a_farm_foundation_lays_its_stage_terrain() {
    use crate::construction::building_update::farm_stage_terrain;
    assert_eq!(
        farm_stage_terrain(ids::FARM, (false, 7), Seconds::ZERO, 15),
        Some(29)
    );
    assert_eq!(
        farm_stage_terrain(ids::FARM, (false, 7), Seconds::new(7.5), 15),
        Some(30)
    );
    assert_eq!(
        farm_stage_terrain(ids::FARM, (false, 7), Seconds::new(15.0), 15),
        Some(31)
    );
    assert_eq!(
        farm_stage_terrain(ids::ID_1187, (true, 63), Seconds::new(5.0), 15),
        Some(66)
    );
    assert_eq!(
        farm_stage_terrain(ids::ID_1187, (false, 63), Seconds::ZERO, 15),
        Some(65)
    );
    assert_eq!(
        farm_stage_terrain(ids::PASTURE_1889, (false, 117), Seconds::new(15.0), 15),
        Some(121)
    );
    assert_eq!(
        farm_stage_terrain(ids::PASTURE_1889, (false, -1), Seconds::ZERO, 15),
        Some(1)
    );
    assert_eq!(
        farm_stage_terrain(ids::HOUSE, (false, -1), Seconds::ZERO, 15),
        None
    );
}

/// `RGE_Moving_Object::update`'s count-down of the last human order time (`+0x650`, at most
/// `FUN_140fe0d10`'s 5000, by the world's dt `+0x7c`, not below 0) is the world's view at once:
/// the unit's own walk later in the update ranks a met unit by the counted-down time
/// (431096717's goat 12959 in update 78: 2001 against villager 16025's 2034, so it does not
/// pause it).
#[test]
fn a_unit_s_human_order_count_down_is_the_walk_s_view_at_once() {
    let mut t = enter_world(vec![], 5);
    let mut counted = |before: u32| {
        let v = &mut t.players[1].objects[ListIndex::new(0)];
        v.last_human_order = WorldTime::from_millis(before);
        let id = v.id;
        t.spatial.set_gait(id, v.gait());
        v.count_down_human_order(Millis::new(33), &mut t.spatial);
        let seen = t.spatial.gait(id).map(|g| g.human_order);
        (
            t.players[1].objects[ListIndex::new(0)].last_human_order,
            seen,
        )
    };
    assert_eq!(
        counted(2034),
        (
            WorldTime::from_millis(2001),
            Some(WorldTime::from_millis(2001))
        )
    );
    assert_eq!(
        counted(5000),
        (
            WorldTime::from_millis(4967),
            Some(WorldTime::from_millis(4967))
        )
    );
    assert_eq!(counted(20), (WorldTime::ZERO, Some(WorldTime::ZERO)));
    assert_eq!(
        counted(5001),
        (
            WorldTime::from_millis(5001),
            Some(WorldTime::from_millis(5001))
        )
    );
}

/// `RGE_Action_Object::update` runs the actions of a unit not inside another, else its inside
/// update (`FUN_140a98790`), never both: livestock whose Artifact's Enter got it in keeps the
/// Artifact to its next update, whose inside update deletes it (429705045's sheep 10555 and
/// 10556 entering the transport 10579 in update 137, the Artifact gone after 138); an animal
/// inside at the update's start runs no Artifact step.
#[test]
fn livestock_got_in_by_its_artifact_s_enter_keeps_the_artifact_to_its_next_update() {
    let mut t = livestock_enter_world();
    // the sheep out of reach of the house, standing, so that its Enter walks
    let place = |t: &mut World, at: Position| {
        let v = &mut t.players[1].objects[ListIndex::new(0)];
        v.place_for_test(at);
        let m = v.body.mover_mut().unwrap();
        m.moved_at = at;
        m.velocity = Some(crate::movement::Velocity {
            velocity: BVector::ZERO,
            acceleration: BVector::ZERO,
        });
        t.spatial.set_position(object_id(9175), at).unwrap();
    };
    place(&mut t, Position::new(30.0, 91.0, 0.0));
    assert_eq!(
        t.enter_command(PlayerId::new(1), ListIndex::new(0), object_id(10028), false),
        Ok(true)
    );
    assert_eq!(t.players[1].objects[ListIndex::new(0)].life.inside, None);
    // the first update runs no Artifact (its first update is in the second)
    t.tick(Millis::new(50));
    // its walk arrived at the house's edge: the next update of the Enter gets it in
    place(&mut t, Position::new(30.0, 89.1, 0.0));
    let art = t.players[1].objects[ListIndex::new(0)]
        .actions
        .artifact_mut()
        .unwrap();
    if let Some(m) = art.enter.as_mut().and_then(|e| e.move_to.as_mut()) {
        m.state = MoveTo::Arrived;
    }
    t.tick(Millis::new(50));
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(v.life.inside, Some(object_id(10028)));
    assert_eq!(
        v.top_action_kind(),
        Some(crate::ownership::artifact::ARTIFACT_TYPE)
    );
    // inside at the next update's start: no Artifact step, the inside update deletes it
    let before = v.actions.artifact().map(|a| (a.state, a.timer));
    t.tick(Millis::new(50));
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        (v.actions.artifact().copied(), v.top_action_kind()),
        (None, None)
    );
    assert_eq!(t.players[1].freezes, vec![]);
    assert!(before.is_some());
}

/// `RGE_Action_Object::update` with an empty list makes the default task and does no inside
/// update in the same update: livestock inside another object gets its Artifact in one update
/// and loses it in the next, by turns (431108946's sheep 11628 in the transport 11638 from update
/// 61: 107 in state 6 after 63, 65, …, none after 62, 64, …).
#[test]
fn livestock_inside_gets_its_default_artifact_and_loses_it_by_turns() {
    let mut t = livestock_enter_world();
    let v = &mut t.players[1].objects[ListIndex::new(0)];
    v.default_artifact = true;
    v.actions.set_artifact(None);
    v.life.inside = Some(object_id(10028));
    t.spatial
        .go_inside(object_id(9175), Position::new(30.0, 88.0, 0.0));
    let mut kinds = Vec::new();
    for _ in 0..4 {
        t.tick(Millis::new(50));
        let v = &t.players[1].objects[ListIndex::new(0)];
        kinds.push(v.actions.artifact().map(|a| a.state));
    }
    assert_eq!(
        kinds,
        [
            Some(ArtifactState::Check),
            None,
            Some(ArtifactState::Check),
            None
        ]
    );
    assert_eq!(t.players[1].freezes, vec![]);
}

/// `TRIBE_Command::do_command`'s case 0x66 checks BUILD's builders before its handler
/// ([`World::command_check`]): another player's becomes −1, and a human's BUILD left without one
/// is refused, placing and paying nothing (429712917's player 8 in update 96, its BUILD naming
/// player 5's villagers 9618 and 9620). With one of its own the foundation goes down, and only
/// that one builds.
#[test]
fn a_build_of_another_player_s_builders_is_refused_and_theirs_are_left_out() {
    let mut t = build_world();
    t.masters[0] = builder_masters();
    let theirs = t.players[1].objects.remove(ListIndex::new(1)).unwrap();
    t.players[0].objects.push(theirs);
    let build = |builders: Vec<i32>| {
        Command::Build(BuildCommand {
            player: player_id(1),
            master: master_id(70),
            position: Point::new(30.0, 88.0),
            q_id: -1,
            sprite_override: -1,
            builders: unit_list(object_ids(builders)),
            queued: false,
            commits: false,
            frame: 0,
        })
    };
    t.tick_with(Millis::new(50), &[build(vec![9181])]);
    let p = &t.players[1];
    assert_eq!(
        (p.resources().map(Amount::get), p.objects.len()),
        ([200.0, 100.0, 200.0, 100.0], 1)
    );
    assert_eq!(t.next_object_id, object_id(10028));
    assert_eq!(p.freezes, vec![]);
    t.tick_with(Millis::new(50), &[build(vec![9181, 9175])]);
    let p = &t.players[1];
    assert_eq!(
        p.resources(),
        crate::test_ids::amounts([200.0, 75.0, 200.0, 100.0])
    );
    assert_eq!(p.objects.last().map(|f| f.id.get()), Some(10028));
    assert_eq!(
        p.objects[ListIndex::new(0)]
            .actions
            .build()
            .map(|b| b.target),
        Some(Some(object_id(10028)))
    );
    assert_eq!(
        t.players[0].objects[ListIndex::new(0)]
            .actions
            .build()
            .copied(),
        None
    );
}

/// The archer's master (4, 2 s reload, range 4, its missile 363 a tile up), its arrow (363,
/// speed 7, straight) and a deer (65, 5 hit points, 1.5 high) for [`archer_world`].
fn archer_masters(hit_chance: i16, max_volley: u8, spread: f32) -> Masters {
    let archer = Master {
        id: master_id(4),
        kind: master_type::COMBAT,
        class: class::CLASS_0,
        hit_points: 30,
        radius: Footprint::new(0.2, 0.2),
        standing_sprite: Some(SpriteId::named(300)),
        attack_sprite: Some(SpriteId::named(301)),
        missile: Some(master_id(363)),
        weapon_offset: Offset::new(0.0, 0.0, 1.0),
        weapon_range: Distance::new(4.0),
        reload: Seconds::new(2.0),
        hit_chance,
        max_volley,
        volley_spread: VolleySpread {
            x: spread,
            y: spread,
            start: 0.0,
        },
        moving: Some(crate::gathering::hunt::tests::walker().stats),
        ..Master::default()
    };
    let arrow = Master {
        id: master_id(363),
        kind: master_type::MISSILE,
        class: class::WALL,
        hit_points: 1,
        radius: Footprint::new(0.1, 0.1),
        standing_sprite: None,
        death_sprite: None,
        death_spawns: [None, None],
        life: Some(MasterData::default()),
        recyclable: true,
        moving: Some(crate::movement::MoveStats {
            speed: Speed::new(7.0),
            base: Speed::new(7.0),
            ..crate::gathering::hunt::tests::walker().stats
        }),
        missile_stats: Some(crate::combat::missile::MissileStats {
            speed: Speed::new(7.0),
            radius: Radii::new(0.1, 0.1, 0.0),
            ..crate::combat::missile::MissileStats::default()
        }),
        ..Master::default()
    };
    let deer = Master {
        id: master_id(65),
        kind: master_type::COMBAT,
        class: class::PREDATOR,
        hit_points: 5,
        radius: Footprint::new(0.2, 0.2),
        height: 1.5,
        terrain_bonus: -1,
        ..Master::default()
    };
    Masters::new([archer, arrow, deer])
}

/// A flat 40-tile map: player 1's archer 123 at (10, 10) in its Attack's READY on Gaia's deer 50
/// at (13, 10); its swing fires at frame 5 of 0.1 s. The masters as [`archer_masters`]; the next
/// reusable id −1.
pub(crate) fn archer_world(hit_chance: i16, max_volley: u8, spread: f32) -> World {
    use crate::garrison::tests::{unit, walker};
    let mut players: Vec<Player> = (0..2).map(|_| Player::default()).collect();
    players[0].relations = relation_row([0, 3]);
    players[1].relations = relation_row([3, 0]);
    let mut t = World::new(
        WorldTime::ZERO,
        29910,
        PlayerId::new(1),
        vec![Millis::ZERO; 2],
        players,
    );
    t.spatial = Spatial::new(GridSize::new(40, 40));
    let mut terrain = crate::map::terrain::Terrain::new(
        GridSize::new(40, 40),
        vec![(0, 0, -1); 40 * 40],
        vec![crate::map::terrain::TerrainKind {
            shown: true,
            flags: 0,
            forest: false,
        }],
        Vec::new(),
    );
    terrain.set_elevations(vec![0; 40 * 40]);
    t.spatial.terrain = Some(terrain);
    let sprite = |id| Graphic {
        id,
        frames: 10,
        facets: 8,
        speed_multiplier: 1.0,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: 1,
    };
    t.graphics = std::collections::BTreeMap::from([
        (SpriteId::named(300), sprite(SpriteId::named(300))),
        (SpriteId::named(301), sprite(SpriteId::named(301))),
    ]);
    let masters = archer_masters(hit_chance, max_volley, spread);
    t.masters = vec![masters.clone(), masters];
    walker(
        &mut t,
        PlayerId::new(1),
        123,
        master_id(4),
        Point::new(10.0, 10.0),
    );
    let archer = &mut t.players[1].objects[ListIndex::new(0)];
    if let Some(m) = archer.body.mover_mut() {
        m.sprite_facets = Some(crate::objects::facing::SpriteFacets {
            count: 8,
            flag_1: false,
        });
    }
    archer.firepower = f32::from(max_volley);
    let mut attack = own_attack(50, -1, AttackState::Ready);
    attack.range = Distance::new(4.0);
    attack.arms.missile = Some(master_id(363));
    attack.arms.fire_frame = 5;
    attack.arms.sprite = Some(sprite(SpriteId::named(301)));
    attack.arms.idle_sprite = Some(sprite(SpriteId::named(300)));
    archer.actions.set_attack(Some(attack));
    let mut deer = unit(50, master_id(65), HitPoints::new(5.0));
    deer.place_for_test(Position::new(13.0, 10.0, 0.0));
    t.players[0].objects.push(deer);
    t.spatial.insert(
        object_id(50),
        Placed {
            position: Position::new(13.0, 10.0, 0.0),
            radius: Footprint::new(0.2, 0.2),
            class: class::PREDATOR,
            player: PlayerId::new(0),
            master: master_id(65),
            obstruction_type: ObstructionType::TYPE_2,
            buildable_on: 0,
            tall: true,
            speed: Speed::ZERO,
            unknown: None,
            obstruction: Some(crate::map::spatial::Obstruction {
                kind: ObstructionType::TYPE_2,
                class: ObstructionClass::NONE,
            }),
        },
    );
    t.recycling.next_id = object_id(-2);
    t.next_object_id = object_id(200);
    t
}

/// A hit's flare (master 112, the target's player's slot 0x1c0) is made where the volley runs,
/// within the shooter's update: here for the shooter's own player, whose list holds the
/// shooter's stand-in.
#[test]
fn a_hit_s_flare_is_made_where_the_volley_runs() {
    let mut t = archer_world(100, 1, 0.0);
    let flare = Master {
        id: master_id(112),
        kind: master_type::ANIMATED,
        standing_sprite: None,
        storage: vec![(Some(AttributeId::numbered(12)), Amount::new(1.0), 0)],
        ..Master::default()
    };
    t.masters[1] = Masters::new(t.masters[1].iter().cloned().chain([flare]));
    t.players[1].attributes = vec![Amount::ZERO; 300];
    let aimed = crate::combat::volley::Aimed {
        target: Some(object_id(50)),
        position: Position::new(13.0, 10.0, 0.0),
        radius: Radii::new(0.2, 0.2, 1.0),
        class: class::PREDATOR,
        player: PlayerId::new(1),
    };
    let seen = in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(0)),
        0,
        |unit, c| {
            unit.volley(c, aimed)?;
            let own = &c.w.players[1].objects;
            Ok(own
                .iter()
                .filter(|u| u.life.master_id == master_id(112))
                .count())
        },
    );
    assert_eq!(seen.map_err(|e| e.reason().to_owned()), Ok(1));
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)].id,
        object_id(123),
        "the shooter is back in its slot"
    );
}

/// The missiles in player 1's list: id, master, place.
pub(crate) fn missiles(t: &World) -> Vec<(i32, i16, Position)> {
    t.players[1]
        .objects
        .iter()
        .filter(|u| u.missile.is_some())
        .map(|u| (u.id.get(), u.life.master_id.get(), u.position()))
        .collect()
}

/// The deer's hit points in [`archer_world`].
fn deer_hp(t: &World) -> f32 {
    t.players[0].objects[ListIndex::new(0)].life.hp.get()
}

/// `a` within 1e-5 of `b` on each axis.
fn assert_near(a: Position, b: Position) {
    assert!(
        [a.x() - b.x(), a.y() - b.y(), a.z() - b.z()]
            .iter()
            .all(|d| d.abs() < 1e-5),
        "{a:?} against {b:?}"
    );
}

/// Slot 0x858 (`FUN_14120c0a0`) and `FUN_141081020`: the archer's swing fires at its firing
/// frame (frame 5 of 0.1 s: the sixth update) its master's missile (363) with the next reusable
/// id (−1), a tile up (its weapon offset) and aimed at the deer a third of its height up; each
/// update the missile flies its speed's share (0.7 of a tile) along that line. It hits the deer
/// in the update its sweep, grown by its radius, meets the deer's footprint: 1 hit point (the
/// least damage, as neither has attacks) and its action done. The dead missile leaves the list
/// four updates after the hit for the pool of type 60, whose id the next swing takes again 20
/// updates after the first (the 2 s reload).
#[test]
fn a_ranged_swing_fires_its_missile_which_flies_to_the_target_and_hits_it() {
    let mut t = archer_world(100, 0, 0.0);
    for _ in 0..5 {
        t.tick(Millis::new(100));
    }
    assert_eq!(missiles(&t), vec![]);
    let way = [3.0f32, 0.0, -0.5];
    let length = (way[0] * way[0] + way[2] * way[2]).sqrt();
    let flown = |k: f32| {
        Position::new(
            10.0 + k * 0.7 * way[0] / length,
            10.0,
            1.0 + k * 0.7 * way[2] / length,
        )
    };
    for k in 1..=3 {
        t.tick(Millis::new(100));
        let m = missiles(&t);
        assert_eq!((m.len(), m[0].0, m[0].1), (1, -2, 363));
        assert_near(m[0].2, flown(k as f32));
        assert_eq!(deer_hp(&t), 5.0);
    }
    // the fourth step ends 0.1 short of the deer's footprint (12.8): its sweep meets it
    t.tick(Millis::new(100));
    assert_near(missiles(&t)[0].2, flown(4.0));
    assert_eq!(deer_hp(&t), 4.0);
    let action = t.players[1].objects[ListIndex::new(1)]
        .actions
        .missile()
        .map(|a| (a.state, a.result));
    assert_eq!(
        action,
        Some((crate::combat::missile::MissileState::Done, 1))
    );
    for _ in 0..3 {
        t.tick(Millis::new(100));
    }
    assert_eq!(missiles(&t).len(), 1);
    t.tick(Millis::new(100));
    assert_eq!(missiles(&t), vec![]);
    assert_eq!(t.recycling.missiles, vec![(object_id(-2), true)]);
    for _ in 0..12 {
        t.tick(Millis::new(100));
    }
    assert_eq!(missiles(&t), vec![]);
    t.tick(Millis::new(100));
    assert_eq!(missiles(&t).iter().map(|m| m.0).collect::<Vec<_>>(), [-2]);
    assert_eq!(t.recycling.missiles, vec![]);
}

/// The missile a swing fires joins the end of its owner's live list at its making (slot
/// 0x2d0, which files it in the tracker), inside the archer's update: what the update does after
/// the fire reaches it by id, as the game's table does.
#[test]
fn a_fired_missile_joins_its_owner_s_list_at_its_making() {
    let mut t = archer_world(100, 0, 0.0);
    for _ in 0..5 {
        t.tick(Millis::new(100));
    }
    let made = in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(0)),
        100,
        |unit, c| {
            let _ = unit.update(c)?;
            let k = crate::players::tracker::key(
                c.w.masters.get(c.player.index()),
                &c.w.lines,
                master_id(363),
            );
            let listed = OthersRef::of(&c.w.players, c.player)
                .unwrap()
                .own(object_id(-2))
                .map(|m| m.life.master_id);
            Ok((
                listed,
                c.w.players[c.player.index()]
                    .tracker
                    .list(ListKey::of_master(k.master))
                    .contains(&Some(object_id(-2))),
            ))
        },
    );
    assert_eq!(made, Ok((Some(master_id(363)), true)));
    let ids: Vec<ObjectId> = t.players[1].objects.iter().map(|u| u.id).collect();
    assert_eq!(ids.last(), Some(&object_id(-2)));
}

/// `FUN_140a9c600`'s miss: a hit chance of 0 strays the missile by up to the ground distance
/// times the master's missed spread; it flies past the deer, lands on the ground (its height 0,
/// `FUN_140a9c890`) and goes to the pool as landed (not flying).
#[test]
fn a_missed_missile_strays_and_lands_on_the_ground() {
    let mut t = archer_world(0, 0, 0.0);
    for m in &mut t.masters {
        m.get_mut(master_id(4)).unwrap().missed_spread = 2.0;
    }
    for _ in 0..6 {
        t.tick(Millis::new(100));
    }
    let at = missiles(&t)[0].2;
    assert_ne!(at.y(), 10.0);
    for _ in 0..4 {
        t.tick(Millis::new(100));
    }
    assert_eq!(missiles(&t)[0].2.z(), 0.0);
    for _ in 0..2 {
        t.tick(Millis::new(100));
    }
    assert_eq!(missiles(&t), vec![]);
    assert_eq!(t.recycling.missiles, vec![(object_id(-2), false)]);
    assert_eq!(deer_hp(&t), 5.0);
    // the next shot takes the landed missile again: on the ground by its fly mode, a tile up
    // again once it is placed (its new position's height is the weapon offset's)
    for _ in 0..14 {
        t.tick(Millis::new(100));
    }
    assert_eq!(missiles(&t).iter().map(|m| m.0).collect::<Vec<_>>(), [-2]);
    assert_eq!(t.recycling.missiles, vec![]);
}

/// A volley (`max_attacks_in_volley` 3, firepower 3): with a spread, a swing fires 1 to 3 of its
/// missiles (an Object-stream draw), the first the master's missile on the line and the others
/// scattered off it, fired before it; `next_volley` counts the fired, the rest follow at the next
/// swing and then it is 0. Each missile that meets the deer takes a hit point. Without a spread
/// one missile a swing.
#[test]
fn a_volley_fires_its_missiles_over_swings_scattered_by_its_spread() {
    let mut t = archer_world(100, 3, 0.5);
    for _ in 0..6 {
        t.tick(Millis::new(100));
    }
    let first = missiles(&t);
    let fired = t.players[1].objects[ListIndex::new(0)].next_volley;
    assert!((1..3).contains(&fired), "{fired}");
    assert_eq!(first.len(), usize::from(fired));
    let (main, scattered) = first.split_last().unwrap();
    assert_eq!(main.0, -i32::from(fired) - 1);
    assert!(scattered.iter().all(|m| m.2 != main.2));
    for _ in 0..3 {
        t.tick(Millis::new(100));
    }
    assert_eq!(deer_hp(&t), 5.0 - f32::from(fired));
    // the next swing, six updates after the first, fires the rest
    t.tick(Millis::new(100));
    t.tick(Millis::new(100));
    assert_eq!(t.players[1].objects[ListIndex::new(0)].next_volley, fired);
    t.tick(Millis::new(100));
    assert_eq!(t.players[1].objects[ListIndex::new(0)].next_volley, 0);
    let mut one = archer_world(100, 3, 0.0);
    for _ in 0..6 {
        one.tick(Millis::new(100));
    }
    assert_eq!(
        (
            missiles(&one).len(),
            one.players[1].objects[ListIndex::new(0)].next_volley
        ),
        (1, 1)
    );
    // the missile's own +0x900 is its archer's attack count when fired
    let counts: Vec<i32> = one.players[1].objects.as_slice()[1..]
        .iter()
        .filter(|u| u.missile.is_some())
        .map(|u| u.attack_count)
        .collect();
    assert_eq!(
        counts,
        [one.players[1].objects[ListIndex::new(0)].attack_count]
    );
    assert_ne!(counts, [0]);
}

/// `FUN_141226690` with the master's own volley missile (`volley_missile`, 364): the extra
/// missiles are shooterless, each hits by its own master (`FUN_140a9e6e0`'s slot 0x860, the
/// missile as the attacker: 1 hit point), the first stays the master's missile with the shooter.
#[test]
fn a_volley_s_own_missiles_hit_without_the_shooter() {
    let mut t = archer_world(100, 3, 0.5);
    for m in &mut t.masters {
        let mut all: Vec<Master> = m.iter().cloned().collect();
        let own = Master {
            id: master_id(364),
            // a missile master that names a missile of its own always hits (`a.missile` 0 or more)
            missile: Some(master_id(0)),
            ..m.get(master_id(363)).unwrap().clone()
        };
        all.push(own);
        m.get_mut(master_id(4)).unwrap().volley_missile = Some(master_id(364));
        all.retain(|x| x.id != master_id(4));
        all.push(m.get(master_id(4)).unwrap().clone());
        *m = Masters::new(all);
    }
    for _ in 0..6 {
        t.tick(Millis::new(100));
    }
    let fired = t.players[1].objects[ListIndex::new(0)].next_volley;
    let first = missiles(&t);
    let masters: Vec<i16> = first.iter().map(|m| m.1).collect();
    let mut expected = vec![364; usize::from(fired) - 1];
    expected.push(363);
    assert_eq!(masters, expected);
    let shooters: Vec<Option<ObjectId>> = t.players[1].objects.as_slice()[1..]
        .iter()
        .filter_map(|u| u.actions.missile().map(|a| a.fired_from))
        .collect();
    let mut expected = vec![None; usize::from(fired) - 1];
    expected.push(Some(object_id(123)));
    assert_eq!(shooters, expected);
    for _ in 0..3 {
        t.tick(Millis::new(100));
    }
    assert_eq!(deer_hp(&t), 5.0 - f32::from(fired));
}

/// `FUN_1410d5920`'s setups for a new missile draw as a new object's: an animated standing sprite
/// (flag 1) with a random first frame (flag 4) starts on the Sprite stream (two draws), then its
/// facet and, as it turns (flag 2) with several facets, its angle on the Object stream; the
/// missile's angle is then its flight's. A missile without a standing sprite draws none of them
/// (the draw counters count each step; the streams' indexes restart at each seeding).
#[test]
fn a_new_missile_s_sprite_draws_as_a_new_object_s() {
    let mut plain = archer_world(100, 0, 0.0);
    let mut drawn = archer_world(100, 0, 0.0);
    let sprite = Graphic {
        id: SpriteId::named(302),
        frames: 10,
        facets: 8,
        speed_multiplier: 1.0,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: 7,
    };
    let _ = drawn.graphics.insert(SpriteId::named(302), sprite);
    for m in &mut drawn.masters {
        m.get_mut(master_id(363)).unwrap().standing_sprite = Some(SpriteId::named(302));
    }
    for _ in 0..6 {
        plain.tick(Millis::new(100));
        drawn.tick(Millis::new(100));
    }
    assert_eq!(missiles(&plain), missiles(&drawn));
    let mut more = |stream| drawn.streams.get(stream).draws - plain.streams.get(stream).draws;
    assert_eq!((more(Stream::Sprite), more(Stream::Object)), (2, 2));
}

/// The missile chain's cases the slice does not model stop the shooter's player with their
/// reason, in the update the swing fires (or the missile flies): a charged hit, a target of a
/// computer player (its notify 0x214), a missile master that cannot be pooled, moved or flown,
/// a smart missile and a projectile of type 1.
#[test]
fn the_missile_chain_s_unmodelled_cases_stop_the_player_with_their_reason() {
    type Change = fn(&mut World);
    let cases: [(Change, &str); 7] = [
        (
            |t| {
                for m in &mut t.masters {
                    m.get_mut(master_id(4)).unwrap().charge_type = 6;
                }
            },
            "a charged hit",
        ),
        (
            |t| t.players[0].computer = true,
            "a hit on a computer player's object",
        ),
        (
            |t| {
                for m in &mut t.masters {
                    m.get_mut(master_id(363)).unwrap().recyclable = false;
                }
            },
            "a missile of a master that is not recyclable",
        ),
        (
            |t| {
                for m in &mut t.masters {
                    m.get_mut(master_id(363)).unwrap().moving = None;
                }
            },
            "a missile master that does not move",
        ),
        (
            |t| {
                for m in &mut t.masters {
                    m.get_mut(master_id(363)).unwrap().missile_stats = None;
                }
            },
            "a missile master without its flight",
        ),
        (
            |t| {
                for m in &mut t.masters {
                    if let Some(s) = m.get_mut(master_id(363)).unwrap().missile_stats.as_mut() {
                        s.smart_mode = 1;
                    }
                }
            },
            "a smart missile leading its target",
        ),
        (
            |t| {
                for m in &mut t.masters {
                    if let Some(s) = m.get_mut(master_id(363)).unwrap().missile_stats.as_mut() {
                        s.projectile_type = 1;
                    }
                }
            },
            "a missile of projectile type 1 or 2",
        ),
    ];
    for (change, reason) in cases {
        let mut t = archer_world(100, 0, 0.0);
        change(&mut t);
        for _ in 0..6 {
            t.tick(Millis::new(100));
        }
        let stopped = t.players[1].unsupported.as_ref().map(|u| u.2.to_string());
        assert!(
            stopped.as_deref().is_some_and(|s| s.starts_with(reason)),
            "{reason}: {stopped:?}"
        );
    }
}

/// `FUN_140a9e6e0`'s start: a missile whose target died (state 3) lets it go (its release) and
/// flies on without one: past the place it would have hit the live deer, down to the ground, and
/// into the pool as landed.
#[test]
fn a_missile_whose_target_died_flies_on_without_it_and_lands() {
    let mut t = archer_world(100, 0, 0.0);
    for _ in 0..7 {
        t.tick(Millis::new(100));
    }
    t.players[0].objects[ListIndex::new(0)].life.state = ObjectState::Dying;
    t.tick(Millis::new(100));
    let target = t.players[1].objects[ListIndex::new(1)]
        .actions
        .missile()
        .map(|a| a.target);
    assert_eq!(target, Some(None));
    // the update the live deer took its hit in passes it by
    t.tick(Millis::new(100));
    assert_eq!(deer_hp(&t), 5.0);
    t.tick(Millis::new(100));
    assert_eq!(missiles(&t)[0].2.z(), 0.0);
    t.tick(Millis::new(100));
    t.tick(Millis::new(100));
    assert_eq!(missiles(&t), vec![]);
    assert_eq!(t.recycling.missiles, vec![(object_id(-2), false)]);
    assert_eq!(deer_hp(&t), 5.0);
}

/// QUEUE's pick (`FUN_141614450`): the building with room and the least load, the last pick holding
/// against an equal load; a comparison with a NaN load replaces nothing; with none with room, none.
#[test]
fn a_queued_unit_goes_to_the_least_loaded_building_with_room() {
    use crate::commands::least_loaded;
    assert_eq!(
        least_loaded(
            &[2, 2, 2],
            &[Seconds::new(5.0), Seconds::new(3.0), Seconds::new(4.0)],
            None
        ),
        Some(1)
    );
    assert_eq!(
        least_loaded(
            &[2, 0, 2],
            &[Seconds::new(5.0), Seconds::new(3.0), Seconds::new(4.0)],
            None
        ),
        Some(2)
    );
    assert_eq!(
        least_loaded(&[2, 2], &[Seconds::new(3.0), Seconds::new(3.0)], Some(1)),
        Some(1)
    );
    assert_eq!(
        least_loaded(&[2, 2], &[Seconds::new(f32::NAN), Seconds::new(3.0)], None),
        Some(0)
    );
    assert_eq!(
        least_loaded(
            &[2, 2, 2],
            &[Seconds::new(4.0), Seconds::new(f32::NAN), Seconds::new(3.0)],
            None
        ),
        Some(2)
    );
    assert_eq!(
        least_loaded(&[2, 2], &[Seconds::new(3.0), Seconds::new(f32::NAN)], None),
        Some(0)
    );
    assert_eq!(
        least_loaded(&[0, 2], &[Seconds::new(1.0), Seconds::new(3.0)], Some(0)),
        Some(0)
    );
    assert_eq!(
        least_loaded(&[0, 0], &[Seconds::new(3.0), Seconds::new(1.0)], None),
        None
    );
    assert_eq!(least_loaded(&[], &[], None), None);
}

/// [`forage_world`]'s forager on bush 600 (the unit AI's gather command), its last move at its
/// place, the load's mesh built.
fn foraging() -> World {
    let mut t = forage_world();
    t.players[1].objects[ListIndex::new(0)]
        .body
        .mover_mut()
        .unwrap()
        .moved_at = Position::new(29.5, 89.5, 0.0);
    t.gather_command(PlayerId::new(1), ListIndex::new(0), object_id(600))
        .unwrap();
    t.build_mesh().unwrap();
    t
}

/// `n` of the forager's whole updates (50 ms each), up to the first after which `stop` holds.
fn forage_until(t: &mut World, n: usize, stop: impl Fn(&World) -> bool) {
    for _ in 0..n {
        let _ = in_update(t, (PlayerId::new(1), ListIndex::new(0)), 50, Unit::update).unwrap();
        if stop(t) {
            return;
        }
    }
}

/// The forager's Gather, as its update runs it (`0x140a8e1d0`): it walks to its bush (state 4),
/// arrives (6) and works (7) until it holds its master's capacity of 10 berries; then it carries
/// them to its town centre (8, the centre its second target), whose drop gives the player 10
/// food, and walks back to the bush (4), arrives and works again.
#[test]
fn a_forager_fills_up_carries_its_berries_home_and_comes_back() {
    let mut t = foraging();
    let mut states = vec![GatherState::Walk];
    for _ in 0..1200 {
        forage_until(&mut t, 1, |_| false);
        let v = &t.players[1].objects[ListIndex::new(0)];
        let g = v.actions.gather().copied().unwrap();
        if states.last() != Some(&g.state) {
            states.push(g.state);
            if g.state == GatherState::Drop {
                assert_eq!(
                    v.life.held(),
                    carrying(Some(AttributeId::numbered(17)), 10.0)
                );
                assert_eq!(
                    (g.target, g.carry_to),
                    (Some(object_id(600)), Some(object_id(700)))
                );
            }
        }
    }
    assert_eq!(
        states,
        [
            GatherState::Walk,
            GatherState::Face,
            GatherState::Work,
            GatherState::Drop,
            GatherState::Walk,
            GatherState::Face,
            GatherState::Work
        ]
    );
    assert_eq!(t.players[1].attributes[0], Amount::new(10.0));
    let g = t.players[1].objects[ListIndex::new(0)]
        .actions
        .gather()
        .copied()
        .unwrap();
    assert_eq!((g.target, g.carry_to), (Some(object_id(600)), None));
}

/// A bush that dies under its forager (past state 6) is let go (its worker leaves) and the
/// Gather searches (state 3): the unit AI hears 0x202 with the gather action and the target
/// type 7.
#[test]
fn a_forager_whose_bush_dies_lets_it_go_and_asks_for_a_search() {
    let mut t = foraging();
    let working = |t: &World| {
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .gather()
            .map(|g| g.state)
            == Some(GatherState::Work)
    };
    forage_until(&mut t, 200, working);
    assert_eq!(t.players[0].objects[ListIndex::new(0)].workers, 1);
    t.players[0].objects[ListIndex::new(0)].life.state = ObjectState::Removed;
    forage_until(&mut t, 1, |_| false);
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        v.actions.gather().map(|g| (g.state, g.target)),
        Some((GatherState::Search, None))
    );
    assert_eq!(t.players[0].objects[ListIndex::new(0)].workers, 0);
    let heard: Vec<_> =
        v.ai.as_ref()
            .unwrap()
            .notifies
            .iter()
            .map(|n| (n.kind, n.p[0], n.p[1]))
            .collect();
    assert_eq!(
        heard,
        [(NotifyKind::REQUIRES_SEARCH, ActionKind::Gather.value(), 7)]
    );
}

/// A forager whose bush went while it carried its load drops the load (the player's food) and,
/// without a target, takes the nearest bush its unit AI's search finds (`ClosestResourceObject`
/// of the Gather's target type's group, 1 for bushes, within its sight plus 5: bush 601 beside
/// the centre, state 4); with none it walks back to where it worked to search again (state 11).
#[test]
fn a_forager_without_its_bush_after_the_drop_takes_the_next_one() {
    let carrying = |near: bool| {
        let mut t = foraging();
        for masters in &mut t.masters {
            let bush = masters.get_mut(master_id(59)).unwrap();
            (bush.auto_gather, bush.gather_group) = (true, GatherGroup::FORAGE);
            masters.get_mut(master_id(120)).unwrap().line_of_sight = Distance::new(4.0);
        }
        t.players[1].objects[ListIndex::new(0)]
            .ai
            .as_mut()
            .unwrap()
            .target_class = None;
        if near {
            t.players[0].objects[ListIndex::new(1)].place_for_test(Position::new(25.5, 89.5, 0.0));
            let p = *t.spatial.get(object_id(601)).unwrap();
            t.spatial.insert(
                object_id(601),
                Placed {
                    position: Position::new(25.5, 89.5, 0.0),
                    ..p
                },
            );
        }
        let carries = |t: &World| {
            t.players[1].objects[ListIndex::new(0)]
                .actions
                .gather()
                .map(|g| g.state)
                == Some(GatherState::Drop)
        };
        forage_until(&mut t, 800, carries);
        let _ = t.players[0].objects.remove(ListIndex::new(0)).unwrap();
        t.spatial.remove(object_id(600));
        let dropped = |t: &World| {
            t.players[1].objects[ListIndex::new(0)]
                .actions
                .gather()
                .map(|g| g.state)
                != Some(GatherState::Drop)
        };
        forage_until(&mut t, 400, dropped);
        assert_eq!(t.players[1].attributes[0], Amount::new(10.0));
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .gather()
            .map(|g| (g.state, g.target))
    };
    assert_eq!(
        carrying(true),
        Some((GatherState::Walk, Some(object_id(601))))
    );
    assert_eq!(carrying(false), Some((GatherState::WalkBack, None)));
}

/// [`foraging`] up to the forager's drop at its town centre without a target (bush 600 gone
/// on the way, `setup` run before), after which the Gather's state follows.
fn dropped_without_a_bush(setup: impl FnOnce(&mut World)) -> World {
    let mut t = foraging();
    let carries = |t: &World| {
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .gather()
            .map(|g| g.state)
            == Some(GatherState::Drop)
    };
    forage_until(&mut t, 800, carries);
    let _ = t.players[0].objects.remove(ListIndex::new(0)).unwrap();
    t.spatial.remove(object_id(600));
    setup(&mut t);
    let dropped = |t: &World| {
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .gather()
            .map(|g| g.state)
            != Some(GatherState::Drop)
    };
    forage_until(&mut t, 400, dropped);
    assert_eq!(t.players[1].attributes[0], Amount::new(10.0));
    t
}

/// After the drop without a target and nothing found: a Gather without its point idles for a
/// human player (state 2, its targets and point cleared, the centre let go) and is done for a
/// computer player; with orders queued (on a target the world holds) the unit AI ends its
/// order and the Gather is done. An order on a target gone leaves the queue first.
#[test]
fn a_forager_with_nothing_found_after_the_drop_idles_or_is_done() {
    let pointless = |t: &mut World| {
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .gather_mut()
            .unwrap()
            .point = Position::new(-1.0, -1.0, -1.0);
    };
    let t = dropped_without_a_bush(pointless);
    let g = t.players[1].objects[ListIndex::new(0)]
        .actions
        .gather()
        .copied()
        .unwrap();
    assert_eq!(
        (g.state, g.target, g.carry_to, g.point),
        (GatherState::Idle, None, None, Position::from([-1.0; 3]))
    );
    assert_eq!(t.players[1].objects[ListIndex::new(1)].workers, 0);
    let t = dropped_without_a_bush(|t| {
        pointless(t);
        t.players[1].computer = true;
    });
    assert!(
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .gather()
            .is_none()
    );
    let order = |target| crate::unit_ai::QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::Gather,
        priority: 100,
        target,
        target_owner: owner(0),
        point: Position::new(36.5, 89.5, 0.0),
        range: Distance::ZERO,
        shown: Point::new(36.5, 89.5),
        ready: true,
        target_type: order_type(7),
    };
    // the Gather's state 8 after the deposit, its bush gone, two orders queued
    let mut t = foraging();
    let _ = t.players[0].objects.remove(ListIndex::new(0)).unwrap();
    t.spatial.remove(object_id(600));
    let v = &mut t.players[1].objects[ListIndex::new(0)];
    v.actions.gather_mut().unwrap().target = None;
    v.ai.as_mut().unwrap().orders = vec![order(Some(object_id(600))), order(Some(object_id(601)))];
    in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, c| {
            let code = unit.gather_after_drop(c)?;
            unit.end_head_update(c, code)
        },
    )
    .unwrap();
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert!(v.actions.gather().is_none());
    let ai = v.ai.as_ref().unwrap();
    assert_eq!(
        ai.orders.iter().map(|o| o.target).collect::<Vec<_>>(),
        [Some(object_id(601))]
    );
    assert_eq!((ai.current_order, ai.current_action), (None, None));
}

/// The forager's Gather update alone (`gather_update`, 50 ms).
fn gather_step(t: &mut World) -> Result<(), GenieError> {
    in_update(t, (PlayerId::new(1), ListIndex::new(0)), 50, |unit, c| {
        let me = crate::movement::Me {
            id: unit.id,
            group: unit.group.id,
            action_state: unit.top_action().map_or(0, |a| a.state),
        };
        let code = unit.gather_update(c, me)?;
        unit.end_head_update(c, code)
    })
}

/// What follows the Gather's update ([`Unit::gather_next`]) for the forager on bush 600 with
/// its town centre 700, each worked by one: the end lets both go and removes the Gather; a
/// target the update changed lets the old one go, where the update does; the reseed after the
/// walk back, without a farm to reseed, idles the human player's gatherer and lets both targets
/// go within the update (the build command:
/// [`the_unit_ai_s_orders_reach_the_unit_and_the_units_they_name`]).
#[test]
fn what_follows_the_gather_s_update_lets_go_builds_or_reseeds() {
    use crate::gathering::gather::Next;
    type Followed = (
        Result<(), GenieError>,
        Vec<i32>,
        Option<(Option<ObjectId>, GatherState)>,
    );
    let follow = |next: Next, old: (Option<ObjectId>, Option<ObjectId>)| -> (World, Followed) {
        let mut t = foraging();
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .gather_mut()
            .unwrap()
            .carry_to = Some(object_id(700));
        for id in [600, 700] {
            t.object_mut(object_id(id)).unwrap().1.workers = 1;
        }
        let seen = in_update(
            &mut t,
            (PlayerId::new(1), ListIndex::new(0)),
            50,
            |unit, c| {
                let by = unit.releaser(c)?;
                let r = unit
                    .gather_next(c, next, old, by)
                    .and_then(|code| unit.end_head_update(c, code));
                // the objects let go, as the update leaves them
                let released = [600, 700]
                    .into_iter()
                    .filter(|&id| {
                        OthersRef::of(&c.w.players, c.player)
                            .unwrap()
                            .find(object_id(id))
                            .is_some_and(|(_, u)| u.workers == 0)
                    })
                    .collect();
                let gather = unit.actions.gather().map(|g| (g.target, g.state));
                Ok((r, released, gather))
            },
        )
        .unwrap();
        (t, seen)
    };
    let (t, seen) = follow(Next::Done, (Some(object_id(600)), Some(object_id(700))));
    assert_eq!(seen, (Ok(()), vec![600, 700], None));
    assert!(
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .gather()
            .is_none()
    );
    let (t, seen) = follow(Next::Stay, (Some(object_id(700)), None));
    assert_eq!(seen.0, Ok(()));
    assert_eq!(seen.1, vec![700]);
    assert!(
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .gather()
            .is_some()
    );
    let (_, seen) = follow(Next::Stay, (Some(object_id(600)), Some(object_id(600))));
    assert_eq!((seen.0, seen.1), (Ok(()), vec![600]));
    let (_, seen) = follow(
        Next::Reseed { arrived: true },
        (Some(object_id(600)), Some(object_id(700))),
    );
    assert_eq!(
        seen,
        (Ok(()), vec![600, 700], Some((None, GatherState::Idle)))
    );
}

/// The Gather's other states (`0x140a8e1d0`): done, it goes and lets its bush go; the search's
/// wait (case 2) at its end asks the unit AI again (0x202) and waits 10 seconds more; the wait
/// after a failed walk (case 3) at its end walks back to the Gather's point to search there
/// (state 11).
#[test]
fn the_gather_s_update_ends_asks_again_and_walks_back() {
    let set = |state, timer| {
        let mut t = foraging();
        let g = t.players[1].objects[ListIndex::new(0)]
            .actions
            .gather_mut()
            .unwrap();
        (g.state, g.timer) = (state, timer);
        t
    };
    let mut t = set(GatherState::Done, Seconds::ZERO);
    assert_eq!(t.players[0].objects[ListIndex::new(0)].workers, 1);
    gather_step(&mut t).unwrap();
    assert!(
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .gather()
            .is_none()
    );
    assert_eq!(t.players[0].objects[ListIndex::new(0)].workers, 0);
    let mut t = set(GatherState::Search, Seconds::new(-0.02));
    gather_step(&mut t).unwrap();
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        v.actions.gather().map(|g| g.timer),
        Some(Seconds::new(-10.0))
    );
    let heard: Vec<NotifyKind> =
        v.ai.as_ref()
            .unwrap()
            .notifies
            .iter()
            .map(|n| n.kind)
            .collect();
    assert_eq!(heard, [NotifyKind::REQUIRES_SEARCH]);
    let mut t = set(GatherState::Search, Seconds::new(-0.2));
    gather_step(&mut t).unwrap();
    assert!(
        (t.players[1].objects[ListIndex::new(0)]
            .actions
            .gather()
            .copied()
            .unwrap()
            .timer
            + Seconds::new(0.15))
        .get()
        .abs()
            < 1e-6
    );
    let mut t = set(GatherState::Walk, Seconds::new(-0.02));
    gather_step(&mut t).unwrap();
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .gather()
            .map(|g| g.state),
        Some(GatherState::WalkBack)
    );
}

/// [`forage_world`]'s forager (player 1's object 0) building house 10028 (master 70, at (30, 88),
/// player 1's object 2) by the build command; the town centre 700 is complete in its tracker
/// and the drop-site table's town centre takes the four basic resources.
fn house_builder() -> World {
    let mut t = forage_world();
    let p = *t.spatial.get(object_id(700)).unwrap();
    let site = Placed {
        position: Position::new(30.0, 88.0, 0.0),
        radius: Footprint::new(1.0, 1.0),
        class: ClassId::of_master(80),
        master: ids::HOUSE,
        ..p
    };
    t.spatial.insert(object_id(10028), site);
    let _ = t
        .build_command(PlayerId::new(1), ListIndex::new(0), object_id(10028))
        .unwrap();
    let k = crate::players::tracker::key(t.masters.get(1), &t.lines, ids::TOWN_CENTRE);
    t.players[1].tracker.add(object_id(700), k);
    t.players[1].tracker.complete(object_id(700), k);
    t.drop_sites[0].resource_types = attribute::RESOURCES.to_vec();
    t
}

/// The Build's drop-off (`set_state(8)` from its search, [`BuildAction::drop_again`]) of the
/// villager building foundation 10028 while it carries `held`, its town centre a done drop site
/// of the table (resources 0 to 3 and berries): Ok with the Build's state, its site and range,
/// the centre's workers and what the unit AI heard.
fn drop_again_with(held: Held) -> (BuildState, Option<ObjectId>, f32, i32, Vec<NotifyKind>) {
    let mut t = house_builder();
    let v = &mut t.players[1].objects[ListIndex::new(0)];
    (v.life.attribute_type, v.life.amount) = (held.kind, held.amount);
    in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, c| {
            let mut b = *unit.actions.build().unwrap();
            let by = unit.releaser(c)?;
            let dropped = b.drop_again(c, unit, by);
            *unit.actions.build_mut().unwrap() = b;
            dropped
        },
    )
    .unwrap();
    let v = &t.players[1].objects[ListIndex::new(0)];
    let b = v.actions.build().copied().unwrap();
    let heard =
        v.ai.as_ref()
            .unwrap()
            .notifies
            .iter()
            .map(|n| n.kind)
            .collect();
    (
        b.state,
        b.site,
        b.move_to.map_or(0.0, |m| m.range.get()),
        t.players[1].objects[ListIndex::new(2)].workers,
        heard,
    )
}

/// The Build's drop-off: a load the table's town centre takes makes it the Build's site (taken,
/// one worker) with the range of the task it gives (the forager's Gather on berries, 0.25; none
/// for stone: the Build task's, 0); an empty builder walks back to its foundation (state 4); a
/// load no drop site takes leaves no site and tells the unit AI (0x203).
#[test]
fn a_loaded_builder_s_drop_off_finds_its_town_centre() {
    assert_eq!(
        drop_again_with(carrying(Some(AttributeId::numbered(17)), 6.0)),
        (BuildState::Drop, Some(object_id(700)), 0.25, 1, vec![])
    );
    assert_eq!(
        drop_again_with(carrying(Some(attribute::STONE), 6.0)),
        (BuildState::Drop, Some(object_id(700)), 0.0, 1, vec![])
    );
    assert_eq!(
        drop_again_with(carrying(None, 0.0)),
        (BuildState::Move, None, 0.0, 0, vec![])
    );
    assert_eq!(
        drop_again_with(carrying(Some(AttributeId::numbered(5)), 6.0)),
        (
            BuildState::Move,
            None,
            0.0,
            0,
            vec![NotifyKind::NO_DROP_SITE]
        )
    );
}

/// [`forage_world`]'s forager 9175 (at 29.5) seeking a drop site from itself
/// ([`Unit::drop_site_with`] with the search's arguments 10, 9 and 8 `args`) on a map with a tile
/// planner, a second town centre 701 at 45.0 beside centre 700 at 20.0; `setup` changes the
/// world first.
fn drop_site_sought(
    setup: impl FnOnce(&mut World),
    args: (bool, bool, Option<ActionType>),
) -> Result<Option<ObjectId>, GenieError> {
    let mut t = forage_world();
    let mut far = t.players[1].objects[ListIndex::new(2)].clone();
    far.id = object_id(701);
    far.place_for_test(far.position().with_x(45.0));
    t.players[1].objects.push(far);
    let p = *t.spatial.get(object_id(700)).unwrap();
    t.spatial.insert(
        object_id(701),
        Placed {
            position: Position::new(45.0, 89.5, 0.0),
            ..p
        },
    );
    t.pathing = Pathing::new(
        GridSize::new(120, 120),
        vec![0; 120 * 120],
        vec![vec![1.0]; 8],
        0,
    );
    t.build_mesh().unwrap();
    setup(&mut t);
    let mut world = DropWorld {
        masters: t.masters.get(1),
        spatial: &mut t.spatial,
        zones: &mut t.zones,
        pathing: &mut t.pathing,
        object: t.streams.get(Stream::Object),
        world_time: WorldTime::ZERO,
        computer: false,
        construction_rate: 1.0,
        groups: &t.groups,
    };
    let objects = OthersRef::of(&t.players, PlayerId::new(1)).unwrap();
    t.players[1].objects[ListIndex::new(0)].drop_site_with(&mut world, objects, 0, args)
}

/// The drop-site search from a unit (`FUN_1411671e0`): the nearer town centre wins by path (its
/// length within 10 tiles measured by the short path too) and by the squared distance of the
/// centres; a centre inside another object is passed over when the search keeps outside, one at
/// work on the searched action type always (an object of type 40 up), so the far centre wins.
/// A drop site of a master the player does not hold fails.
#[test]
fn a_drop_site_search_from_a_unit_takes_the_nearest_it_reaches() {
    use crate::gathering::make::{MAKE, MakeAction};
    let keep = |_: &mut World| ();
    assert_eq!(
        drop_site_sought(keep, (false, true, None)),
        Ok(Some(object_id(700)))
    );
    assert_eq!(
        drop_site_sought(keep, (false, false, None)),
        Ok(Some(object_id(700)))
    );
    let inside =
        |t: &mut World| t.players[1].objects[ListIndex::new(2)].life.inside = Some(object_id(9175));
    assert_eq!(
        drop_site_sought(inside, (true, true, None)),
        Ok(Some(object_id(701)))
    );
    assert_eq!(
        drop_site_sought(inside, (false, true, None)),
        Ok(Some(object_id(700)))
    );
    let making = |t: &mut World| {
        t.players[1].objects[ListIndex::new(2)]
            .actions
            .set_make(Some(MakeAction {
                state: MakeState::Making,
                remaining: Amount::ZERO,
                total: Amount::ZERO,
                task: crate::objects::masters::Task::default(),
            }));
    };
    assert_eq!(
        drop_site_sought(making, (false, true, Some(MAKE))),
        Ok(Some(object_id(701)))
    );
    assert_eq!(
        drop_site_sought(making, (false, true, None)),
        Ok(Some(object_id(700)))
    );
    let unknown = |t: &mut World| {
        for masters in &mut t.masters {
            masters.get_mut(master_id(120)).unwrap().drop_sites =
                vec![ids::TOWN_CENTRE, master_id(999)];
        }
        t.players[1].objects[ListIndex::new(2)].life.master_id = master_id(999);
    };
    assert_eq!(
        drop_site_sought(unknown, (false, true, None))
            .unwrap_err()
            .reason(),
        OBJECT_MASTER.reason()
    );
}

/// [`forage_world`]'s villager 9175 with its own `MoveTo` `m` on a map with a tile planner.
fn own_move_world(m: MoveToAction) -> World {
    let mut t = forage_world();
    t.pathing = Pathing::new(
        GridSize::new(120, 120),
        vec![0; 120 * 120],
        vec![vec![1.0]; 8],
        0,
    );
    t.build_mesh().unwrap();
    t.players[1].objects[ListIndex::new(0)]
        .actions
        .set_move_to(Some(m));
    t
}

/// What [`own_move`] gives: the part's result, the `MoveTo` after it, what the unit AI heard
/// and bush 600's workers after the step's releases.
type OwnMoved = (
    Result<(), GenieError>,
    Option<MoveToAction>,
    Vec<(NotifyKind, [i32; 3])>,
    i32,
);

/// The villager's update's `MoveTo` part ([`Unit::move_to_part`]) in `t`.
fn own_move(t: &mut World) -> OwnMoved {
    let r = in_update(t, (PlayerId::new(1), ListIndex::new(0)), 50, |unit, c| {
        let me = crate::movement::Me {
            id: unit.id,
            group: unit.group.id,
            action_state: unit.actions.move_to().map_or(0, |m| m.state.number()),
        };
        let code = unit.move_to_part(c, me)?;
        unit.end_head_update(c, code)
    });
    let v = &t.players[1].objects[ListIndex::new(0)];
    let heard = v.ai.as_ref().map_or_else(Vec::new, |ai| {
        ai.notifies.iter().map(|n| (n.kind, n.p)).collect()
    });
    (
        r,
        v.actions.move_to().copied(),
        heard,
        t.players[0].objects[ListIndex::new(0)].workers,
    )
}

/// The unit's own `MoveTo` in its update (`0x140aa2a70`): a start toward bush 600 asks for its
/// path and moves; one that arrived tells the unit AI (0x1fa with the move action value, which
/// an AI on another action ignores), lets its target go (one worker less) and goes; a failed
/// one tells 0x1f9 and goes. A target that
/// left the point the walk goes to (more than half a tile) makes it tell 0x1fc.
#[test]
fn a_unit_s_own_move_to_walks_tells_its_end_and_lets_its_target_go() {
    let to_bush = |state| MoveToAction {
        state,
        point: Position::ORIGIN,
        target: Some(object_id(600)),
        range: Distance::new(0.25),
        sprite: None,
        quiet: false,
        on_target_moved: crate::movement::move_to::TargetMoved::Follows,
        wait: crate::movement::move_to::MoveWait::default(),
    };
    let mut t = own_move_world(to_bush(MoveTo::Start));
    let (r, m, heard, _) = own_move(&mut t);
    assert_eq!(
        (r, m.map(|m| m.state), heard),
        (Ok(()), Some(MoveTo::Moving), vec![])
    );
    // the bush moves two tiles off the point the walk goes to
    t.spatial
        .set_position(object_id(600), Position::new(32.5, 91.5, 0.0))
        .unwrap();
    let (r, _, heard, _) = own_move(&mut t);
    assert_eq!(r, Ok(()));
    assert_eq!(heard, [(NotifyKind::TARGET_MOVED, [0, 0, 0])]);
    let arrived = |action| {
        let mut t = own_move_world(to_bush(MoveTo::Arrived));
        t.players[0].objects[ListIndex::new(0)].workers = 1;
        t.players[1].objects[ListIndex::new(0)]
            .ai
            .as_mut()
            .unwrap()
            .current_action = action;
        own_move(&mut t)
    };
    let (r, m, heard, workers) = arrived(Some(ActionKind::Move));
    assert_eq!((r, m, workers), (Ok(()), None, 0));
    assert_eq!(
        heard,
        [(
            NotifyKind::ACTION_COMPLETED,
            [ActionKind::Move.value(), 0, 0]
        )]
    );
    // the unit AI on another action ignores the move's completion (FUN_140c556d0)
    assert_eq!(arrived(Some(ActionKind::Gather)).2, []);
    let to_point = MoveToAction {
        point: Position::new(40.5, 89.5, 0.0),
        target: None,
        ..to_bush(MoveTo::Failed)
    };
    let (r, m, heard, _) = own_move(&mut own_move_world(to_point));
    assert_eq!((r, m), (Ok(()), None));
    assert_eq!(
        heard,
        [(NotifyKind::ACTION_FAILED, [ActionKind::Move.value(), 0, 0])]
    );
}

/// The result of a bird's Fly part, its Fly after it and what its unit AI heard.
type Flown = (
    Result<(), GenieError>,
    Option<Fly>,
    Vec<(NotifyKind, [i32; 3])>,
);

/// [`forage_world`]'s villager 9175 as a bird flying (state 4) toward (40.5, 89.5) with its
/// `MoveTo` in `move_to`, two seconds of flight left and a rest of two seconds; `setup`
/// changes the unit, then its update's Fly part runs ([`Unit::fly_part`]): the result, the Fly
/// after it and what its unit AI heard.
fn flown(move_to: MoveTo, setup: impl FnOnce(&mut Unit)) -> Flown {
    let mut t = forage_world();
    let task = crate::objects::masters::Task {
        wait_time: Seconds::new(2.0),
        ..crate::objects::masters::Task::default()
    };
    let v = &mut t.players[1].objects[ListIndex::new(0)];
    v.actions.set_fly(Some(Fly {
        state: FlyState::Flying,
        timer: Seconds::new(2.0),
        move_to: Some(MoveToAction {
            state: move_to,
            quiet: false,
            ..MoveToAction::sub_to_point(Position::new(40.5, 89.5, 0.0), Distance::ZERO, None)
        }),
        home: Point::new(29.5, 89.5),
        task,
    }));
    setup(v);
    let r = in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, c| {
            let me = crate::movement::Me {
                id: unit.id,
                group: unit.group.id,
                action_state: 4,
            };
            unit.fly_part(c, me)
        },
    );
    let v = &t.players[1].objects[ListIndex::new(0)];
    let heard = v.ai.as_ref().map_or_else(Vec::new, |ai| {
        ai.notifies.iter().map(|n| (n.kind, n.p)).collect()
    });
    (r, v.actions.fly().copied(), heard)
}

/// A bird's Fly in its update: a failed `MoveTo` ends the flight, its failure told to the unit
/// AI (0x1f9 with the move action value); the bird stops and rests (state 2) for the flight's
/// time left and the rest; a bird without movement state is not modelled.
#[test]
fn a_bird_s_failed_flight_tells_its_unit_ai_and_rests() {
    let (r, fly, heard) = flown(MoveTo::Failed, |_| ());
    assert_eq!(r, Ok(()));
    let fly = fly.unwrap();
    assert_eq!((fly.state, fly.move_to), (FlyState::Resting, None));
    assert_eq!(
        fly.timer,
        Seconds::new(2.0) - Millis::new(50).seconds() + Seconds::new(2.0)
    );
    assert_eq!(
        heard,
        [(NotifyKind::ACTION_FAILED, [ActionKind::Move.value(), 0, 0])]
    );
    let (r, _, _) = flown(MoveTo::Failed, |v| v.body.put_mover(None));
    assert_eq!(r.unwrap_err().reason(), "a bird without movement state");
}

/// [`forage_world`]'s villager 9175 with an Explore toward (40.5, 89.5) on a map with a tile
/// planner, its Explore changed by `set`, through its update's Explore part
/// ([`Unit::explore_part`]): the result, the Explore after it and what its unit AI heard.
fn explored(set: impl FnOnce(&mut crate::movement::explore::ExploreAction)) -> Explored {
    let mut t = forage_world();
    t.pathing = Pathing::new(
        GridSize::new(120, 120),
        vec![0; 120 * 120],
        vec![vec![1.0]; 8],
        0,
    );
    t.build_mesh().unwrap();
    let (mut explore, _) = crate::movement::explore::ExploreAction::new(
        Position::new(40.5, 89.5, 0.0),
        &crate::movement::explore::bare_walk(),
    )
    .unwrap();
    set(&mut explore);
    t.players[1].objects[ListIndex::new(0)]
        .actions
        .set_explore(Some(explore));
    let r = in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, c| {
            let me = crate::movement::Me {
                id: unit.id,
                group: unit.group.id,
                action_state: crate::movement::explore::ExploreState::Walk.get(),
            };
            unit.explore_part(c, me)
        },
    );
    let v = &t.players[1].objects[ListIndex::new(0)];
    let heard = v.ai.as_ref().map_or_else(Vec::new, |ai| {
        ai.notifies.iter().map(|n| (n.kind, n.p)).collect()
    });
    (r, v.actions.explore().copied(), heard)
}

/// The Explore's `MoveTo` shows its sprite as it starts (`0x140a87e30` gives `FUN_140aa0d30`
/// the master's walking sprite, an Explore having no task; `0x140aa29e0`'s `set_state(4)` shows
/// it before the motion step).
#[test]
fn an_explore_s_move_to_shows_the_master_s_walking_sprite_as_it_starts() {
    let mut t = forage_world();
    t.pathing = Pathing::new(
        GridSize::new(120, 120),
        vec![0; 120 * 120],
        vec![vec![1.0]; 8],
        0,
    );
    t.build_mesh().unwrap();
    let walking = Graphic {
        id: SpriteId::named(31),
        frames: 10,
        facets: 16,
        speed_multiplier: 1.0,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: 1,
    };
    let _ = t.graphics.insert(SpriteId::named(31), walking);
    let master = t.players[1].objects[ListIndex::new(0)].life.master_id;
    t.masters[1].get_mut(master).unwrap().move_sprite = Some(SpriteId::named(31));
    let r = in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, c| {
            unit.explore_to(Point::new(40.5, 89.5), c)?;
            let me = crate::movement::Me {
                id: unit.id,
                group: unit.group.id,
                action_state: crate::movement::explore::ExploreState::Walk.get(),
            };
            unit.explore_part(c, me)
        },
    );
    assert_eq!(r, Ok(()));
    let v = &t.players[1].objects[ListIndex::new(0)];
    let move_to = v.actions.explore().and_then(|e| e.move_to);
    assert_eq!(
        move_to.map(|m| (m.state, m.sprite.map(|g| g.id))),
        Some((MoveTo::Moving, Some(SpriteId::named(31))))
    );
    assert_eq!(v.sprite.map(|s| s.graphic.id), Some(SpriteId::named(31)));
}

/// What [`explored`] gives: the part's result, the Explore after it and the notifies heard.
type Explored = (
    Result<(), GenieError>,
    Option<crate::movement::explore::ExploreAction>,
    Vec<(NotifyKind, [i32; 3])>,
);

/// The Explore's part of the update (`0x140a880c0`): in state 11 its quiet `MoveTo` to the
/// point starts (code 0: it walks on). A `MoveTo` that ended, or one that failed (code 1 or 3),
/// makes it wait for a new point (state 3): the unit stops and its unit AI hears 0x202 with the
/// explore action value 605. A waiting Explore does nothing; a done (1) or failed (13) one is
/// not modelled.
#[test]
fn an_explore_walks_to_its_point_then_asks_for_the_next() {
    use crate::movement::explore::ExploreState;
    let (r, e, heard) = explored(|_| ());
    assert_eq!(r, Ok(()));
    let e = e.unwrap();
    assert_eq!(
        (e.state, e.move_to.map(|m| m.state)),
        (ExploreState::Walk, Some(MoveTo::Moving))
    );
    assert_eq!(heard, []);
    let asked = vec![(
        NotifyKind::REQUIRES_SEARCH,
        [ActionKind::Explore.value(), 0, 0],
    )];
    for end in [MoveTo::Arrived, MoveTo::Failed] {
        let (r, e, heard) = explored(|e| {
            if let Some(m) = e.move_to.as_mut() {
                m.state = end;
            }
        });
        assert_eq!(r, Ok(()));
        let e = e.unwrap();
        assert_eq!((e.state, e.move_to), (ExploreState::Wait, None));
        assert_eq!(heard, asked);
    }
    let (r, e, heard) = explored(|e| {
        let _ = e.set_state(ExploreState::Wait, None);
    });
    assert_eq!(
        (r, e.map(|e| e.state), heard),
        (Ok(()), Some(ExploreState::Wait), vec![])
    );
    for (state, reason) in [
        (
            ExploreState::Done,
            "a done Explore (the action list after it)",
        ),
        (
            ExploreState::Failed,
            "a failed Explore (notify 0x1f9, set_state 2)",
        ),
    ] {
        let (r, _, _) = explored(|e| e.state = state);
        assert_eq!(r.unwrap_err().reason(), reason);
    }
}

/// A hunter back from its drop-off without a deer (`hunt.target` -1, the deer it hunted let
/// go): both deer seen by player 1, the hunter with a sight of 4, the mesh built. `setup` changes
/// the hunter before [`Unit::hunt_after_drop`] runs.
fn hunted_without_a_deer(setup: impl FnOnce(&mut Unit)) -> World {
    let mut t = hunt_world();
    t.spatial.relations = vec![relation_row([0, 3]), relation_row([3, 0])];
    t.spatial.unit_diplomacy = vec![relations([1, 4]), relations([4, 1])];
    for masters in &mut t.masters {
        let m = masters.get_mut(master_id(122)).unwrap();
        (m.line_of_sight, m.search_radius) = (Distance::new(4.0), Distance::new(4.0));
    }
    t.hunt_command(PlayerId::new(1), ListIndex::new(0), object_id(500))
        .unwrap();
    t.build_mesh().unwrap();
    let mut r = crate::vision::registry::Registry::new(2, 65);
    for (id, x) in [(500, 32.5), (501, 36.5)] {
        r.entries.entry(object_id(id)).or_default().mask = 1 << 1;
        r.update_unit(UnitUpdate {
            id: object_id(id),
            owner: 0,
            position: Point::new(x, 89.5),
            radii: Footprint::new(0.2, 0.2),
            class: class::PREY,
            old: 0,
            new: 1 << 1,
        })
        .unwrap();
    }
    let sight = t.spatial.sight.as_mut().unwrap();
    sight.registry = Some(r);
    for id in [500, 501] {
        let deer = Seer {
            player: PlayerId::new(0),
            master: master_id(65),
            los: Distance::new(2.0),
            state: ObjectState::Alive,
            explored: false,
            building: None,
            umv: 0,
            capture: 0,
            kind: master_type::COMBAT,
        };
        sight.add_seer(object_id(id), deer);
    }
    t.players[0].objects[ListIndex::new(0)].workers = 0;
    let v = &mut t.players[1].objects[ListIndex::new(0)];
    let at = v.position();
    v.body.mover_mut().unwrap().moved_at = at;
    v.actions.hunt_mut().unwrap().target = None;
    v.ai.as_mut().unwrap().target_class = None;
    setup(v);
    in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, c| {
            let code = unit.hunt_after_drop(c)?;
            unit.end_head_update(c, code)
        },
    )
    .unwrap();
    t
}

/// The hunt's state, target and drop site after [`hunted_without_a_deer`], and the two deer's
/// workers.
fn hunt_after(t: &World) -> (Option<(HuntState, i32, i32)>, [i32; 2]) {
    let h = t.players[1].objects[ListIndex::new(0)]
        .actions
        .hunt()
        .copied();
    let deer = &t.players[0].objects;
    (
        h.map(|h| {
            (
                h.state,
                ObjectId::field(h.target),
                ObjectId::field(h.carry_to),
            )
        }),
        [
            deer[ListIndex::new(0)].workers,
            deer[ListIndex::new(1)].workers,
        ],
    )
}

/// Without a deer after the drop the hunter's `ClosestObject` over its search (class 9, every
/// relation) finds the nearer deer, which the hunt takes and attacks (`FUN_140ac7a00`,
/// `LAB_140ac9509`'s other way).
#[test]
fn a_hunter_without_its_deer_after_the_drop_takes_the_closest_one() {
    let t = hunted_without_a_deer(|_| ());
    assert_eq!(hunt_after(&t), (Some((HuntState::Attack, 500, -1)), [1, 0]));
}

/// A queued order on a target the world still holds ends the hunt after the drop, so the unit AI
/// runs it; one on a target gone leaves the queue (`FUN_140c5de60`) and the search runs.
#[test]
fn a_hunter_with_a_queued_order_after_the_drop_ends_its_hunt() {
    let order = |target: i32| crate::unit_ai::QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::Hunt,
        priority: 100,
        target: ObjectId::of_field(target),
        target_owner: owner(0),
        point: Position::new(36.5, 89.5, 0.0),
        range: Distance::ZERO,
        shown: Point::new(36.5, 89.5),
        ready: true,
        target_type: order_type(9),
    };
    let held = hunted_without_a_deer(|v| v.ai.as_mut().unwrap().orders = vec![order(501)]);
    assert_eq!(hunt_after(&held), (None, [0, 0]));
    let gone = hunted_without_a_deer(|v| v.ai.as_mut().unwrap().orders = vec![order(999)]);
    assert_eq!(
        gone.players[1].objects[ListIndex::new(0)]
            .ai
            .as_ref()
            .unwrap()
            .orders
            .len(),
        0
    );
    assert_eq!(
        hunt_after(&gone),
        (Some((HuntState::Attack, 500, -1)), [1, 0])
    );
}

/// A hunt of class 10 searches nothing after the drop, and a wood search finds no tree: either
/// walks back to its point to search again (`set_state(11)`).
#[test]
fn a_hunter_that_finds_nothing_after_the_drop_walks_to_its_point() {
    for class in [class::PREDATOR, class::TREE] {
        let t = hunted_without_a_deer(|v| v.actions.hunt_mut().unwrap().target_class = Some(class));
        assert_eq!(
            hunt_after(&t),
            (Some((HuntState::WalkSearch, -1, -1)), [0, 0]),
            "class {class}"
        );
        let h = t.players[1].objects[ListIndex::new(0)]
            .actions
            .hunt()
            .copied()
            .unwrap();
        assert_eq!(h.point, Position::new(32.5, 89.5, 0.0));
    }
}

/// How often player `p` watches object `id`.
fn watches(t: &World, p: usize, id: i32) -> usize {
    t.players[p]
        .watched
        .iter()
        .filter(|w| w.id == object_id(id))
        .count()
}

/// A site's state, hit points and build points.
fn site_of(u: &Unit) -> (ObjectState, f32, Option<f32>) {
    (
        u.life.state,
        u.life.hp.get(),
        u.construction.map(|c| c.progress.get()),
    )
}

/// [`house_builder`]'s house 10028 with an annex: a second house foundation 10030 (player 1's
/// last object) in its owner's pending tracker list, linked to the house as the game links an
/// annex (`+0xbc0`, `+0xbb8`).
fn annexed_house() -> World {
    let mut t = house_builder();
    let mut annex = t.players[1]
        .objects
        .by_id(object_id(10028))
        .unwrap()
        .clone();
    annex.id = object_id(10030);
    annex.annex_of = Some(object_id(10028));
    t.players[1].objects.push(annex);
    let placed = *t.spatial.get(object_id(10028)).unwrap();
    t.spatial.insert(object_id(10030), placed);
    t.spatial
        .set_annex(object_id(10030), Some(object_id(10028)));
    let k = crate::players::tracker::key(t.masters.get(1), &t.lines, ids::HOUSE);
    t.players[1].tracker.add(object_id(10030), k);
    t
}

/// Where houses `a` and `b` stand in their owner's done tracker list of master 70.
fn done_at(t: &World, a: i32, b: i32) -> (Option<usize>, Option<usize>) {
    let k = crate::players::tracker::key(t.masters.get(1), &t.lines, ids::HOUSE);
    let done = t.players[1].tracker.list(ListKey::of_master(k.master));
    let at = |id| done.iter().position(|&d| d == id);
    (at(ObjectId::of_field(a)), at(ObjectId::of_field(b)))
}

/// A building that completes in its own update takes its annexes to state 2 first (its
/// `set_state(2)`, `FUN_1411dbb70`: their slot 0xa0 before its own completion): the annex's
/// completion reaches the world at once, its tracker part before the building's.
#[test]
fn a_building_s_own_completion_completes_its_annexes_first() {
    let mut t = annexed_house();
    let h = t.players[1].objects.by_id_mut(object_id(10028)).unwrap();
    let k = h.construction.as_mut().unwrap();
    k.progress = Seconds::new(f32::from(k.build_time));
    let i = t.players[1].objects.position_of(object_id(10028)).unwrap();
    let seen = in_update(&mut t, (PlayerId::new(1), i), 50, |unit, c| {
        unit.complete_on_update(c)?;
        Ok(c.w.object(object_id(10030)).map(|(_, a)| a.life.state))
    });
    assert_eq!(
        seen.map_err(|e| e.reason().to_owned()),
        Ok(Some(ObjectState::Alive))
    );
    let (annex, house) = done_at(&t, 10030, 10028);
    assert!(annex.is_some() && annex < house, "{annex:?} {house:?}");
}

/// [`house_builder`]'s forager after it built town centre 700 (its Build's target in state 224,
/// the end of the work, its `MoveTo` gone; the centre built) on a map with a tile planner, its
/// update with `setup` run first: the work it found (slot 0x7e8's target and point, the
/// caller's after the update) and the notifies its unit AI heard.
fn built_the_centre(setup: impl FnOnce(&mut World)) -> (World, BuildSearched) {
    let mut t = house_builder();
    t.pathing = Pathing::new(
        GridSize::new(120, 120),
        vec![0; 120 * 120],
        vec![vec![1.0]; 8],
        0,
    );
    t.build_mesh().unwrap();
    for masters in &mut t.masters {
        let bush = masters.get_mut(master_id(59)).unwrap();
        (bush.auto_gather, bush.gather_group) = (true, GatherGroup::FORAGE);
        masters.get_mut(master_id(120)).unwrap().line_of_sight = Distance::new(4.0);
        masters.get_mut(ids::TOWN_CENTRE).unwrap().line_of_sight = Distance::new(16.0);
    }
    t.drop_sites[0].auto_work = true;
    let b = t.players[1].objects[ListIndex::new(0)]
        .actions
        .build_mut()
        .unwrap();
    (b.target, b.move_to, b.state) = (Some(object_id(700)), None, BuildState::Finished);
    let centre = &mut t.players[1].objects[ListIndex::new(2)];
    centre.workers = 1;
    centre.construction = Some(Construction {
        progress: Seconds::new(25.0),
        desolid: false,
        builders: 0,
        hit_points: 2400,
        build_time: 25,
        record: crate::map::spatial::Obstruction {
            kind: ObstructionType::TYPE_2,
            class: ObstructionClass::UNIT,
        },
        snow: false,
        sprite_override: false,
    });
    setup(&mut t);
    let r = in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, c| {
            let me = crate::movement::Me {
                id: unit.id,
                group: unit.group.id,
                action_state: unit.top_action().map_or(0, |a| a.state),
            };
            let _ = unit.build_part(c, me)?;
            let heard = unit.ai.as_ref().unwrap().notifies.iter();
            Ok((
                (unit.top_action_kind(), unit.top_target()),
                heard.map(|n| n.kind).collect(),
            ))
        },
    );
    (t, r)
}

/// What [`built_the_centre`]'s search left within the update: the builder's top action (its
/// type and target) and the notifies' kinds.
type BuildSearched = Result<((Option<ActionType>, Option<ObjectId>), Vec<NotifyKind>), GenieError>;

/// The search for more work around the town centre a builder finished (`0x140ab0b43`,
/// [`Unit::more_work`], within the Build's update): the table's entry for the centre (auto
/// work) takes berries, within the centre's line of sight (16 tiles) the bush at 32.5 is the
/// nearest, and the unit AI's `ClosestResourceObject` names it: the work command on it at its
/// place, within the update (the Gather replaces the Build). An entry without auto work, or a
/// line of sight below the floor (the reach 4 tiles, short of the bushes), finds nothing: the
/// Build waits without a target, lets the centre go and asks its unit AI for a search.
#[test]
fn a_builder_finds_more_work_around_the_building_it_finished() {
    use crate::gathering::gather::GATHER;
    let (t, found) = built_the_centre(|_| ());
    assert_eq!(found, Ok(((Some(GATHER), Some(object_id(600))), vec![])));
    assert!(
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .build()
            .is_none()
    );
    let off: fn(&mut World) = |t| t.drop_sites[0].auto_work = false;
    let short: fn(&mut World) = |t| {
        for masters in &mut t.masters {
            masters.get_mut(ids::TOWN_CENTRE).unwrap().line_of_sight = Distance::ZERO;
        }
    };
    for setup in [off, short] {
        let (t, found) = built_the_centre(setup);
        assert_eq!(
            found,
            Ok((
                (Some(action_type::BUILD), None),
                vec![NotifyKind::REQUIRES_SEARCH]
            ))
        );
        let b = t.players[1].objects[ListIndex::new(0)]
            .actions
            .build()
            .copied()
            .unwrap();
        assert_eq!((b.state, b.target), (BuildState::Idle, None));
        assert_eq!(t.players[1].objects[ListIndex::new(2)].workers, 0);
    }
}

/// [`house_builder`]'s forager at work on foundation `site` (the Build's state 7, counted among
/// its builders, with a task of rate and value 1), the house counting one builder, `setup` run
/// first: one update of `dt` milliseconds.
fn worked(t: &mut World, site: i32, dt: u32) -> Result<(), GenieError> {
    let b = t.players[1].objects[ListIndex::new(0)]
        .actions
        .build_mut()
        .unwrap();
    (b.target, b.state, b.added, b.move_to) =
        (ObjectId::of_field(site), BuildState::Place, true, None);
    b.task = BuildTask {
        range: Distance::new(0.1),
        work_value: 1.0,
        work_rate: WorkRate::new(1.0),
        resource: None,
        id: TaskId::of_task(-1),
        moving: None,
    };
    t.players[1]
        .objects
        .by_id_mut(object_id(10028))
        .and_then(|h| h.construction.as_mut())
        .unwrap()
        .builders = 1;
    in_update(t, (PlayerId::new(1), ListIndex::new(0)), dt, |unit, c| {
        let me = crate::movement::Me {
            id: unit.id,
            group: unit.group.id,
            action_state: unit.top_action().map_or(0, |a| a.state),
        };
        unit.build_part(c, me).map(drop)
    })
}

/// The first work on a desolid foundation starts its construction (`FUN_1411e5670`): its owner
/// watches it once, within the builder's update, and the start's flag clears.
#[test]
fn a_builder_s_first_work_starts_the_site_its_owner_watches() {
    let mut t = house_builder();
    assert_eq!(
        worked(&mut t, 10028, 50).map_err(|e| e.reason().to_owned()),
        Ok(())
    );
    let k = t.players[1]
        .objects
        .by_id(object_id(10028))
        .unwrap()
        .construction
        .unwrap();
    assert!(!k.desolid);
    assert_eq!(watches(&t, 1, 10028), 1);
}

/// The work on an annex (started) whose main building (in state 0, 1 hit point) takes it
/// (`FUN_1411cc430`'s recursion, [`World::work_building`] with the builder lent): the house gets
/// the 2.5 seconds, its build points and its share of hit points, within the builder's update.
#[test]
fn a_builder_s_work_on_an_annex_goes_to_its_main_building() {
    let mut t = annexed_house();
    // the annex started: it clears no foundation under it (FUN_1411cd0d0) on the house's start
    let annex = t.players[1].objects.by_id_mut(object_id(10030)).unwrap();
    annex.construction.as_mut().unwrap().desolid = false;
    assert_eq!(
        worked(&mut t, 10030, 2500).map_err(|e| e.reason().to_owned()),
        Ok(())
    );
    let house = t.players[1].objects.by_id(object_id(10028)).unwrap();
    assert_eq!(site_of(house), (ObjectState::Foundation, 56.0, Some(2.5)));
    let annex = t.players[1].objects.by_id(object_id(10030)).unwrap();
    assert_eq!(site_of(annex).2, Some(0.0));
}

/// The work that completes the house (`Construction::complete`) gives its parts at once within
/// the builder's update: the house stands in state 2, leaves its pending tracker list for its
/// done one, its owner gains its paid cost (attributes 98 and 165, `World::built`); then the
/// builder's work ends (`set_state(224)`), out of the builders.
#[test]
fn a_builder_s_completion_reaches_the_world_within_its_update() {
    let mut t = house_builder();
    let h = t.players[1].objects.by_id_mut(object_id(10028)).unwrap();
    h.life.hp = HitPoints::new(549.0);
    let k = h.construction.as_mut().unwrap();
    (k.progress, k.desolid) = (Seconds::new(24.99), false);
    assert_eq!(
        worked(&mut t, 10028, 50).map_err(|e| e.reason().to_owned()),
        Ok(())
    );
    let house = t.players[1].objects.by_id(object_id(10028)).unwrap();
    assert_eq!(
        (house.life.state, house.repairers()),
        (ObjectState::Alive, 0)
    );
    let k = crate::players::tracker::key(t.masters.get(1), &t.lines, ids::HOUSE);
    assert!(
        t.players[1]
            .tracker
            .list(ListKey::of_master(k.master))
            .contains(&Some(object_id(10028)))
    );
    let attributes = &t.players[1].attributes;
    assert_eq!(
        (attributes[98], attributes[165]),
        (Amount::new(25.0), Amount::new(25.0))
    );
    let b = t.players[1].objects[ListIndex::new(0)]
        .actions
        .build()
        .copied()
        .unwrap();
    assert_eq!((b.state, b.added), (BuildState::Finished, false));
}

/// A builder's completion of its foundation takes the foundation's annexes to state 2 where it
/// runs, before the foundation's tracker part (`FUN_1411dbb70`).
#[test]
fn a_builder_s_completion_completes_the_foundation_s_annexes_where_it_runs() {
    let mut t = annexed_house();
    let h = t.players[1].objects.by_id_mut(object_id(10028)).unwrap();
    let k = h.construction.as_mut().unwrap();
    (k.progress, k.desolid) = (Seconds::new(24.99), false);
    assert_eq!(
        worked(&mut t, 10028, 50).map_err(|e| e.reason().to_owned()),
        Ok(())
    );
    let annex = t.players[1].objects.by_id(object_id(10030)).unwrap();
    assert_eq!(annex.life.state, ObjectState::Alive);
    let (annex, house) = done_at(&t, 10030, 10028);
    assert!(annex.is_some() && annex < house, "{annex:?} {house:?}");
}

/// A building's `set_state` past 3 takes its annexes along where it runs (`FUN_1411dbb70`,
/// [`Unit::annexes_follow`]): the annex holds the state within the building's update.
#[test]
fn a_building_s_later_state_reaches_its_annexes_where_it_runs() {
    let mut t = annexed_house();
    let i = t.players[1].objects.position_of(object_id(10028)).unwrap();
    let seen = in_update(&mut t, (PlayerId::new(1), i), 0, |unit, c| {
        unit.annexes_follow(c, ObjectState::Dying, ObjectState::Removed)?;
        Ok(c.w.object(object_id(10030)).map(|(_, a)| a.life.state))
    });
    assert_eq!(
        seen.map_err(|e| e.reason().to_owned()),
        Ok(Some(ObjectState::Removed))
    );
}

/// What [`completed_house`]'s completion left within the update: the attributes its owner
/// gained and how often it watches the house.
type Completed = (Vec<(i16, f32)>, usize);

/// [`house_builder`]'s house 10028 (player 1's object 1) with its build points at its build
/// time, changed by `setup`, after its own update's completion test
/// ([`Unit::complete_on_update`]): the attributes its owner gains and how often it watches the
/// house within the update.
fn completed_house(setup: fn(&mut Unit)) -> (World, Completed) {
    let mut t = house_builder();
    let h = &mut t.players[1].objects[ListIndex::new(1)];
    let k = h.construction.as_mut().unwrap();
    k.progress = Seconds::new(f32::from(k.build_time));
    setup(h);
    let r = in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(1)),
        50,
        |unit, c| {
            let before = attribute_values(c.w, 1);
            unit.complete_on_update(c)?;
            Ok((gained(c.w, 1, &before), watches(c.w, 1, 10028)))
        },
    )
    .unwrap();
    (t, r)
}

/// A building in state 0 whose build points reached its build time completes in its own update
/// (`0x1411c9000`, [`Unit::complete_on_update`]): the house goes to state 2, solid, into its
/// owner's done tracker list; its owner gains the attributes its master gives, and its
/// construction starts in the completion (its owner watches it). Short of its build time it
/// stays a foundation, and an annex outside the walls' branch (class 27, master 70) waits for
/// its main building.
#[test]
fn a_foundation_whose_build_points_reached_its_build_time_completes() {
    let (t, r) = completed_house(|_| ());
    assert_eq!(r, (vec![(98, 25.0), (165, 25.0)], 1));
    let h = &t.players[1].objects[ListIndex::new(1)];
    let k = h.construction.unwrap();
    assert_eq!((h.life.state, k.desolid), (ObjectState::Alive, false));
    let key = crate::players::tracker::key(t.masters.get(1), &t.lines, ids::HOUSE);
    assert_eq!(
        t.players[1].tracker.list(ListKey::of_master(key.master)),
        [Some(object_id(10028))]
    );
    let short: fn(&mut Unit) = |h| h.construction.as_mut().unwrap().progress = Seconds::new(24.0);
    let annex: fn(&mut Unit) =
        |h| (h.life.master_id, h.annex_of) = (master_id(109), Some(object_id(700)));
    for setup in [short, annex] {
        let (t, r) = completed_house(setup);
        assert_eq!(r, (vec![], 0));
        assert_eq!(
            t.players[1].objects[ListIndex::new(1)].life.state,
            ObjectState::Foundation
        );
    }
}

/// A building completing in its own update leaves its owner what the builder's completion
/// leaves (`World::built`, the building lent to the world), within the update: a master that
/// disappears when built is destroyed at once (state 7), and a castle's completion records its
/// history event (castle built, the owner's id as data1).
#[test]
fn a_building_completing_in_its_own_update_leaves_what_a_builder_s_completion_leaves() {
    let finish = |t: &mut World| {
        let h = &mut t.players[1].objects[ListIndex::new(1)];
        let k = h.construction.as_mut().unwrap();
        k.progress = Seconds::new(f32::from(k.build_time));
        in_update(t, (PlayerId::new(1), ListIndex::new(1)), 50, |unit, c| {
            unit.complete_on_update(c)?;
            let events: Vec<(crate::players::history::EventKind, f32)> = c.w.players[1]
                .history
                .events
                .iter()
                .map(|e| (e.kind, e.data1))
                .collect();
            Ok((unit.life.state, events))
        })
        .map_err(|e| e.reason().to_owned())
    };
    let mut t = house_builder();
    for masters in &mut t.masters {
        let b = masters
            .get_mut(ids::HOUSE)
            .unwrap()
            .building
            .as_mut()
            .unwrap();
        b.disappears_when_built = true;
    }
    assert_eq!(finish(&mut t), Ok((ObjectState::Removed, vec![])));
    let mut t = house_builder();
    t.players[1].history.entries.push([0, 0]);
    t.players[1].attributes.resize(250, Amount::ZERO);
    for masters in &mut t.masters {
        let castle = Master {
            id: ids::CASTLE,
            ..masters.get(ids::HOUSE).unwrap().clone()
        };
        *masters = Masters::new(masters.iter().cloned().chain([castle]));
    }
    t.players[1].objects[ListIndex::new(1)].life.master_id = ids::CASTLE;
    let castle_built = crate::players::history::EventKind::CASTLE_BUILT;
    assert_eq!(
        finish(&mut t),
        Ok((ObjectState::Alive, vec![(castle_built, 1.0)]))
    );
}

/// A desolid foundation that dies (`FUN_1411dbb70` to state 3 from state 0) starts its
/// construction ([`Unit::foundation_died`], [`Construction::begin`]): its owner watches it at
/// once, the foundation lent to the world within its own update, and the start's flag clears.
#[test]
fn a_dying_desolid_foundation_s_owner_watches_it_within_its_update() {
    let mut t = house_builder();
    for masters in &mut t.masters {
        let house = masters.get_mut(ids::HOUSE).unwrap();
        house.death_sprite = None;
        if let Some(b) = house.building.as_mut() {
            b.destruction_sprite = None;
        }
    }
    let k = t.players[1].objects[ListIndex::new(1)]
        .construction
        .as_mut()
        .unwrap();
    k.desolid = true;
    let seen = in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(1)),
        0,
        |unit, c| {
            unit.life.state = ObjectState::Dying;
            unit.died(c, ObjectState::Foundation)?;
            Ok(watches(c.w, 1, 10028))
        },
    );
    assert_eq!(seen.map_err(|e| e.reason().to_owned()), Ok(1));
}

/// [`house_builder`]'s town centre 700 (master 109: 2400 hit points, a garrison heal rate of 2)
/// at world time 3000 with the forager (master 120: 25 hit points, at 10) inside, changed by
/// `setup`, after the centre's whole seconds ([`Unit::building_seconds`]): the forager's hit
/// points and the centre's garrison time.
fn centre_seconds(setup: impl FnOnce(&mut World)) -> Result<(f32, u32), GenieError> {
    use crate::objects::masters::BuildingMaster;
    let mut t = house_builder();
    for masters in &mut t.masters {
        let centre = masters.get_mut(ids::TOWN_CENTRE).unwrap();
        centre.hit_points = 2400;
        centre.building = Some(BuildingMaster {
            garrison_heal_rate: HitPoints::new(2.0),
            ..BuildingMaster::default()
        });
        masters.get_mut(master_id(120)).unwrap().hit_points = 25;
    }
    t.world_time = WorldTime::from_millis(3000);
    t.players[1].objects[ListIndex::new(2)].garrison = vec![object_id(9175)];
    t.players[1].objects[ListIndex::new(0)].life.hp = HitPoints::new(10.0);
    setup(&mut t);
    in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(2)),
        50,
        Unit::building_seconds,
    )?;
    let hp = t.players[1].objects[ListIndex::new(0)].life.hp.get();
    Ok((
        hp,
        t.players[1].objects[ListIndex::new(2)]
            .garrison_time
            .millis(),
    ))
}

/// A built building's whole seconds (`0x1411c9000`): since its garrison time, which moves on by
/// them, the units inside below their master's hit points heal by its garrison heal rate; no
/// whole second, a unit at its hit points, or a building not built heals nothing. Not modelled:
/// a building at most a fifth of its hit points (it lets its garrison out), one attribute 255
/// names, a unit inside whose raider countdown runs, and villagers inside repairing a damaged
/// building.
#[test]
fn a_building_heals_its_garrison_each_whole_second() {
    let reason = |r: Result<(f32, u32), GenieError>| r.unwrap_err().reason();
    assert_eq!(centre_seconds(|_| ()), Ok((12.0, 3000)));
    assert_eq!(
        centre_seconds(|t| t.world_time = WorldTime::from_millis(999)),
        Ok((10.0, 0))
    );
    assert_eq!(
        centre_seconds(|t| t.players[1].objects[ListIndex::new(0)].life.hp = HitPoints::new(25.0)),
        Ok((25.0, 3000))
    );
    assert_eq!(
        centre_seconds(
            |t| t.players[1].objects[ListIndex::new(2)].life.state = ObjectState::Foundation
        ),
        Ok((10.0, 0))
    );
    assert_eq!(
        reason(centre_seconds(|t| t.players[1].objects
            [ListIndex::new(2)]
        .life
        .hp = HitPoints::new(480.0))),
        "a damaged building lets its garrison out (a fifth of its hit points)"
    );
    assert_eq!(
        reason(centre_seconds(|t| {
            t.players[1].attributes.resize(256, Amount::ZERO);
            t.players[1].attributes[255] = Amount::new(109.0);
        })),
        "a building whose master attribute 255 names (FUN_1411c39b0)"
    );
    assert_eq!(
        reason(centre_seconds(|t| t.players[1].objects
            [ListIndex::new(0)]
        .raider_countdown = 1)),
        "a unit inside whose raider_build_countdown runs (FUN_1411e7220)"
    );
    let repairs = |t: &mut World| {
        for masters in &mut t.masters {
            let b = masters
                .get_mut(ids::TOWN_CENTRE)
                .unwrap()
                .building
                .as_mut()
                .unwrap();
            b.garrison_repair_rate = HitPoints::new(1.0);
        }
        t.players[1].objects[ListIndex::new(2)].life.hp = HitPoints::new(2000.0);
    };
    assert_eq!(
        reason(centre_seconds(repairs)),
        "a building its villagers inside repair (FUN_1411d33e0)"
    );
}

/// A wall's or a house's update takes the walls' branch (`0x1411c9000`, [`walls_branch`]): no
/// whole seconds, so its garrison time stays and nothing inside heals.
///
/// [`walls_branch`]: crate::construction::building_update::walls_branch
#[test]
fn a_wall_or_a_house_counts_no_whole_seconds() {
    use crate::construction::building_update::walls_branch;
    let wall = |t: &mut World| {
        for masters in &mut t.masters {
            masters.get_mut(ids::TOWN_CENTRE).unwrap().class = class::WALL;
        }
    };
    assert_eq!(centre_seconds(wall), Ok((10.0, 0)));
    let master = |id, class| Master {
        id,
        class,
        ..Master::default()
    };
    assert!(walls_branch(&master(ids::HOUSE, class::BUILDING)));
    assert!(walls_branch(&master(master_id(117), class::WALL)));
    assert!(!walls_branch(&master(ids::TOWN_CENTRE, class::BUILDING)));
}

/// [`repair_world`] on [`open_map`] with the repairer 9175 (a mover) at `at` and the house
/// 10028 at (30, 88) placed, the player holding 100 of each resource, the mesh built.
pub(crate) fn repairing(at: Position) -> World {
    let mut t = repair_world();
    open_map(&mut t);
    t.spatial.relations = vec![relation_row([0, 3]), relation_row([3, 0])];
    let placed = |position, class, master, radius| Placed {
        position,
        radius,
        class,
        player: PlayerId::new(1),
        master,
        obstruction_type: ObstructionType::NONE,
        buildable_on: 0,
        tall: true,
        speed: Speed::ZERO,
        unknown: None,
        obstruction: None,
    };
    t.spatial.insert(
        object_id(9175),
        placed(at, class::VILLAGER, ids::BUILDER, Footprint::new(0.2, 0.2)),
    );
    t.spatial.insert(
        object_id(9175),
        placed(
            at,
            class::VILLAGER,
            master_id(118),
            Footprint::new(0.2, 0.2),
        ),
    );
    t.spatial.insert(
        object_id(10028),
        placed(
            Position::new(30.0, 88.0, 0.0),
            class::BUILDING,
            ids::HOUSE,
            Footprint::new(1.0, 1.0),
        ),
    );
    let v = &mut t.players[1].objects[ListIndex::new(0)];
    v.place_for_test(at);
    let mut mover = crate::gathering::hunt::tests::walker();
    mover.moved_at = at;
    v.body.put_mover(Some(mover));
    t.players[1].attributes[..4].copy_from_slice(&[Amount::new(100.0); 4]);
    t.build_mesh().unwrap();
    t
}

/// The work command on the house, then `n` of the repairer's repair parts
/// ([`Unit::repair_part`], 50 ms each): the Repair's states in turn (None once it ended).
fn repair_for(t: &mut World, n: usize) -> Vec<Option<RepairState>> {
    let work = crate::orders::Work {
        target: Some(object_id(10028)),
        point: Position::new(30.0, 88.0, 0.0),
        carry_test: false,
        filter: None,
        notified: true,
    };
    t.order_work(PlayerId::new(1), ListIndex::new(0), &work)
        .unwrap();
    let mut states = vec![
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .repair()
            .map(|r| r.state),
    ];
    for _ in 0..n {
        in_update(t, (PlayerId::new(1), ListIndex::new(0)), 50, |unit, c| {
            let me = crate::movement::Me {
                id: unit.id,
                group: unit.group.id,
                action_state: unit.top_action().map_or(0, |a| a.state),
            };
            let code = unit.repair_part(c, me)?;
            unit.end_head_update(c, code)
        })
        .unwrap();
        let state = t.players[1].objects[ListIndex::new(0)]
            .actions
            .repair()
            .map(|r| r.state);
        if states.last() != Some(&state) {
            states.push(state);
        }
    }
    states
}

/// A repairer that arrives out of the Repair's reach of the house (0.3 tiles off it) cannot
/// repair: the reach test fails and the quick path finds no way closer, so the Repair fails
/// (state 13), lets the house go, tells its unit AI (0x1f9 with the repair action) and ends; the
/// house keeps its 300 hit points and nothing is paid. Moved within reach (its footprint touching
/// the house's), the same work command repairs: its `MoveTo` arrived as its list added it
/// (`0x140aa2100`: within 0.01), so it faces the house (6) in its first update, joins its
/// repairers and repairs (7), the house gaining hit points.
#[test]
fn a_repairer_out_of_reach_fails_and_one_within_reach_repairs() {
    let mut t = repairing(Position::new(29.5, 89.5, 0.0));
    assert_eq!(
        repair_for(&mut t, 10),
        [Some(RepairState::Move), Some(RepairState::Failed), None]
    );
    let house = &t.players[1].objects[ListIndex::new(1)];
    assert_eq!(
        (house.life.hp, house.repairers(), house.workers),
        (HitPoints::new(300.0), 0, 0)
    );
    let heard = &t.players[1].objects[ListIndex::new(0)]
        .ai
        .as_ref()
        .unwrap()
        .notifies;
    assert_eq!(
        heard.iter().map(|n| (n.kind, n.p)).collect::<Vec<_>>(),
        [(
            NotifyKind::ACTION_FAILED,
            [ActionKind::Repair.value(), 0, 0]
        )]
    );
    let at = Position::new(30.0, 89.2, 0.0);
    let p = *t.spatial.get(object_id(9175)).unwrap();
    t.spatial
        .insert(object_id(9175), Placed { position: at, ..p });
    let v = &mut t.players[1].objects[ListIndex::new(0)];
    v.place_for_test(at);
    v.body.mover_mut().unwrap().moved_at = at;
    assert_eq!(
        repair_for(&mut t, 400),
        [
            Some(RepairState::Move),
            Some(RepairState::Face),
            Some(RepairState::Work)
        ]
    );
    let house = &t.players[1].objects[ListIndex::new(1)];
    assert_eq!(
        (house.life.hp, house.repairers(), house.workers),
        (HitPoints::new(309.947_57), 1, 1)
    );
    assert_eq!(
        t.players[1].resources(),
        crate::test_ids::amounts([100.0; 4])
    );
}

/// A repair pays from what its owner holds when it pays, within the update (`FUN_141618db0` on
/// the player's attributes): wood spent earlier in the same update is not there to pay with, so
/// the house gains no hit points and the wood stays at 0.
#[test]
fn a_repair_pays_from_what_its_owner_holds_at_the_payment() {
    let mut t = repairing(Position::new(30.0, 89.2, 0.0));
    t.players[1].attributes.resize(300, Amount::ZERO);
    t.players[1].attributes[crate::construction::repair::BUILDING_REPAIR_COST] = Amount::new(1.0);
    assert_eq!(
        repair_for(&mut t, 100),
        [
            Some(RepairState::Move),
            Some(RepairState::Face),
            Some(RepairState::Work)
        ]
    );
    let hp = t.players[1].objects[ListIndex::new(1)].life.hp;
    in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, c| {
            c.w.players[1].attributes[1] = Amount::ZERO;
            let me = crate::movement::Me {
                id: unit.id,
                group: unit.group.id,
                action_state: unit.top_action().map_or(0, |a| a.state),
            };
            let code = unit.repair_part(c, me)?;
            unit.end_head_update(c, code)
        },
    )
    .unwrap();
    let house = &t.players[1].objects[ListIndex::new(1)];
    assert_eq!(
        (house.life.hp, t.players[1].attributes[1]),
        (hp, Amount::ZERO)
    );
}

/// The Repair's payment, its end and another player's target ([`Unit::repair_part`]): with a
/// building repair cost (attribute 271) the repair pays the share of the building's costs its
/// hit points make (the house's in wood, a castle's in stone); a repair that
/// takes the house to its hit points ends (state 1, then the action goes) with the house's armor
/// back, the house let go and out of its repairers, the unit AI told 0x1fa; an ally's house takes
/// its hit points and its repairer at once, through the update's view ([`Others::get_mut`]).
#[test]
fn a_repair_pays_ends_at_full_hit_points_and_reaches_an_ally_s_house() {
    let near = Position::new(30.0, 89.2, 0.0);
    let mut t = repairing(near);
    t.players[1].attributes.resize(300, Amount::ZERO);
    t.players[1].attributes[crate::construction::repair::BUILDING_REPAIR_COST] = Amount::new(1.0);
    assert_eq!(
        repair_for(&mut t, 100),
        [
            Some(RepairState::Move),
            Some(RepairState::Face),
            Some(RepairState::Work)
        ]
    );
    assert_eq!(
        t.players[1].objects[ListIndex::new(1)].life.hp,
        HitPoints::new(302.449_4)
    );
    assert_eq!(
        t.players[1].resources(),
        crate::test_ids::amounts([100.0, 99.888_596, 100.0, 100.0])
    );
    // a castle (650 stone, 4800 hit points) pays its share in stone
    let mut t = repairing(near);
    for masters in &mut t.masters {
        let house = masters.get(ids::HOUSE).unwrap().clone();
        let castle = Master {
            id: ids::CASTLE,
            hit_points: 4800,
            cost: costs(&[(attribute::STONE, 650, true)]),
            ..house
        };
        *masters = Masters::new(masters.iter().cloned().chain([castle]));
    }
    let castle = &mut t.players[1].objects[ListIndex::new(1)];
    (castle.life.master_id, castle.life.hp) = (master_id(82), HitPoints::new(3000.0));
    let p = *t.spatial.get(object_id(10028)).unwrap();
    t.spatial.insert(
        object_id(10028),
        Placed {
            master: ids::CASTLE,
            ..p
        },
    );
    t.players[1].attributes.resize(300, Amount::ZERO);
    t.players[1].attributes[crate::construction::repair::BUILDING_REPAIR_COST] = Amount::new(1.0);
    assert_eq!(
        repair_for(&mut t, 100),
        [
            Some(RepairState::Move),
            Some(RepairState::Face),
            Some(RepairState::Work)
        ]
    );
    assert_eq!(
        t.players[1].objects[ListIndex::new(1)].life.hp,
        HitPoints::new(3_002.440_4)
    );
    assert_eq!(
        t.players[1].resources(),
        crate::test_ids::amounts([100.0, 100.0, 99.668_03, 100.0])
    );
    let mut t = repairing(near);
    let house = &mut t.players[1].objects[ListIndex::new(1)];
    (house.life.hp, house.armor_removed) = (HitPoints::new(549.0), ArmorPair::new(3, 4));
    let states = repair_for(&mut t, 100);
    assert_eq!(
        states,
        [
            Some(RepairState::Move),
            Some(RepairState::Face),
            Some(RepairState::Work),
            Some(RepairState::Done),
            None
        ]
    );
    let house = &t.players[1].objects[ListIndex::new(1)];
    assert_eq!(
        (
            house.life.hp,
            house.armor_removed,
            house.repairers(),
            house.workers
        ),
        (HitPoints::new(550.0), ArmorPair::ZERO, 0, 0)
    );
    let heard = &t.players[1].objects[ListIndex::new(0)]
        .ai
        .as_ref()
        .unwrap()
        .notifies;
    assert_eq!(
        heard.iter().map(|n| (n.kind, n.p)).collect::<Vec<_>>(),
        [(
            NotifyKind::ACTION_COMPLETED,
            [ActionKind::Repair.value(), 0, 0]
        )]
    );
    // player 0's house, the players allied: repaired, then repaired to full
    let allied = |hp: HitPoints| {
        let mut t = repairing(near);
        let mut house = t.players[1].objects.remove(ListIndex::new(1)).unwrap();
        (house.life.hp, house.armor_removed, house.workers) = (hp, ArmorPair::new(3, 4), 0);
        t.players[0].objects.push(house);
        t.players[0].attributes = vec![Amount::ZERO; 300];
        t.spatial.relations = vec![relation_row([0, 0]), relation_row([0, 0])];
        for p in &mut t.players {
            p.relations = relation_row([0, 0]);
        }
        let p = *t.spatial.get(object_id(10028)).unwrap();
        t.spatial.insert(
            object_id(10028),
            Placed {
                player: PlayerId::new(0),
                ..p
            },
        );
        let states = repair_for(&mut t, 100);
        let h = &t.players[0].objects[ListIndex::new(0)];
        (
            states,
            h.life.hp.get(),
            h.armor_removed,
            h.repairers(),
            h.workers,
        )
    };
    assert_eq!(
        allied(HitPoints::new(300.0)),
        (
            vec![
                Some(RepairState::Move),
                Some(RepairState::Face),
                Some(RepairState::Work)
            ],
            302.449_4,
            ArmorPair::new(3, 4),
            1,
            1
        )
    );
    assert_eq!(
        allied(HitPoints::new(549.0)),
        (
            vec![
                Some(RepairState::Move),
                Some(RepairState::Face),
                Some(RepairState::Work),
                Some(RepairState::Done),
                None
            ],
            550.0,
            ArmorPair::ZERO,
            0,
            0
        )
    );
}

/// A repair of an annex (house 10028 of the main building 10029, both at 300 hit points) is its
/// main building's (`FUN_1411d37d0`): the main building repairs its annexes by the amount first,
/// then itself, so both gain it; the repairer counts among the main building's repairers
/// (`FUN_140ad9050`).
#[test]
fn a_repair_of_an_annex_repairs_its_main_building_and_counts_there() {
    let mut t = repairing(Position::new(30.0, 89.2, 0.0));
    let mut main = t.players[1].objects[ListIndex::new(1)].clone();
    main.id = object_id(10029);
    main.place_for_test(Position::new(34.0, 88.0, 0.0));
    t.players[1].objects.push(main);
    t.players[1].objects[ListIndex::new(1)].annex_of = Some(object_id(10029));
    t.spatial
        .set_annex(object_id(10028), Some(object_id(10029)));
    let p = *t.spatial.get(object_id(10028)).unwrap();
    t.spatial.insert(
        object_id(10029),
        Placed {
            position: Position::new(34.0, 88.0, 0.0),
            ..p
        },
    );
    assert_eq!(
        repair_for(&mut t, 100),
        [
            Some(RepairState::Move),
            Some(RepairState::Face),
            Some(RepairState::Work)
        ]
    );
    let [annex, main] = [
        &t.players[1].objects[ListIndex::new(1)],
        &t.players[1].objects[ListIndex::new(2)],
    ];
    assert_eq!(
        (
            annex.life.hp,
            main.life.hp,
            annex.repairers(),
            main.repairers()
        ),
        (HitPoints::new(302.449_4), HitPoints::new(302.449_4), 0, 1)
    );
}

/// [`repairing`] with a building repair cost and `n` repairers on the house (9175, then copies
/// 9185 and 9195 at its east and west sides, all within reach), each repairing in turn: the
/// house's hit points and the player's wood after 10 updates (every repairer at work) and after
/// 100 more.
fn repaired_by(n: usize) -> [(f32, f32); 2] {
    let mut t = repairing(Position::new(30.0, 89.2, 0.0));
    t.players[1].attributes.resize(300, Amount::ZERO);
    t.players[1].attributes[crate::construction::repair::BUILDING_REPAIR_COST] = Amount::new(1.0);
    for (k, at) in [
        Position::new(31.2, 88.0, 0.0),
        Position::new(28.8, 88.0, 0.0),
    ]
    .into_iter()
    .enumerate()
    .take(n - 1)
    {
        let mut v = t.players[1].objects[ListIndex::new(0)].clone();
        v.id = object_id(9185 + 10 * k as i32);
        v.place_for_test(at);
        v.body.mover_mut().unwrap().moved_at = at;
        let p = *t.spatial.get(object_id(9175)).unwrap();
        t.spatial.insert(v.id, Placed { position: at, ..p });
        t.players[1].objects.push(v);
    }
    let repairers: Vec<ListIndex> = std::iter::once(0)
        .chain(2..=n)
        .map(ListIndex::new)
        .collect();
    let work = crate::orders::Work {
        target: Some(object_id(10028)),
        point: Position::new(30.0, 88.0, 0.0),
        carry_test: false,
        filter: None,
        notified: true,
    };
    for &i in &repairers {
        t.order_work(PlayerId::new(1), i, &work).unwrap();
    }
    let update = |t: &mut World| {
        for &i in &repairers {
            in_update(t, (PlayerId::new(1), i), 50, |unit, c| {
                let me = crate::movement::Me {
                    id: unit.id,
                    group: unit.group.id,
                    action_state: unit.top_action().map_or(0, |a| a.state),
                };
                let code = unit.repair_part(c, me)?;
                unit.end_head_update(c, code)
            })
            .unwrap();
        }
    };
    let seen = |t: &World| {
        (
            t.players[1].objects[ListIndex::new(1)].life.hp.get(),
            t.players[1].attributes[1].get(),
        )
    };
    (0..10).for_each(|_| update(&mut t));
    let before = seen(&t);
    assert_eq!(
        usize::from(t.players[1].objects[ListIndex::new(1)].repairers()),
        n
    );
    (0..100).for_each(|_| update(&mut t));
    [before, seen(&t)]
}

/// More repairers repair faster, but not in proportion (`0x140adaa20` case 6): each of `n`
/// repairs `((n − 1) × 0.5 + 1) / n` of one's work, so two repair 1.5 times as fast as one and
/// three twice as fast. The cost (`FUN_1411d37d0`: the building repair cost times the amount over
/// the master's hit points, of the master's costs) follows the hit points repaired: more
/// repairers pay more wood each second, the same for each hit point (the house's 25 wood over
/// its 550 hit points).
#[test]
fn more_repairers_repair_faster_but_not_in_proportion_and_pay_by_the_hit_point() {
    let gained = |[(hp0, wood0), (hp1, wood1)]: [(f32, f32); 2]| (hp1 - hp0, wood0 - wood1);
    let one = gained(repaired_by(1));
    let two = gained(repaired_by(2));
    let three = gained(repaired_by(3));
    assert_eq!(one, (2.499_389_6, 0.113_677_98));
    assert_eq!(two, (3.747_558_6, 0.170_898_44));
    assert_eq!(three, (4.998_779_3, 0.226_593_02));
    let near = |a: f32, b: f32| (a - b).abs() < 1e-3;
    assert!(near(two.0 / one.0, 1.5) && near(three.0 / one.0, 2.0));
    for (hp, wood) in [one, two, three] {
        assert!(
            near(wood / hp, 25.0 / 550.0),
            "{wood} wood for {hp} hit points"
        );
    }
}

/// A builder's whole Build of house 10028 ([`house_builder`] with the Build's work value 1, a
/// tile planner and the builder's sprites of 8 facets) through its updates (`0x140ab0370`): it
/// walks to the house (state 4, its `MoveTo` moving, then arrived), faces it (6) and works (7)
/// a build point a second until the house's 25 complete it (state 2; the Build's 224); then its
/// search for more work finds nothing around a house (no drop-site entry), so the Build waits
/// without a target (3), lets the house go and asks its unit AI for a search (0x202).
#[test]
fn a_builder_walks_to_its_house_builds_it_and_waits_after() {
    let mut t = house_builder();
    t.pathing = Pathing::new(
        GridSize::new(120, 120),
        vec![0; 120 * 120],
        vec![vec![1.0]; 8],
        0,
    );
    t.build_mesh().unwrap();
    let v = &mut t.players[1].objects[ListIndex::new(0)];
    let here = v.position();
    let m = v.body.mover_mut().unwrap();
    m.moved_at = here;
    m.stats.facets = Some(crate::objects::facing::SpriteFacets {
        count: 8,
        flag_1: false,
    });
    v.actions.build_mut().unwrap().task.work_value = 1.0;
    let mut seen = Vec::new();
    for n in 0..520 {
        let _ = in_update(
            &mut t,
            (PlayerId::new(1), ListIndex::new(0)),
            50,
            Unit::update,
        )
        .unwrap();
        let b = t.players[1].objects[ListIndex::new(0)]
            .actions
            .build()
            .copied()
            .unwrap();
        let house = t.players[1].objects[ListIndex::new(1)].life.state;
        let now = (b.state, b.move_to.map(|m| m.state), house);
        if seen.last().is_none_or(|&(_, last)| last != now) {
            seen.push((n, now));
        }
    }
    assert_eq!(
        seen,
        [
            (
                0,
                (
                    BuildState::Move,
                    Some(MoveTo::Moving),
                    ObjectState::Foundation
                )
            ),
            (
                8,
                (
                    BuildState::Move,
                    Some(MoveTo::Arrived),
                    ObjectState::Foundation
                )
            ),
            (9, (BuildState::Work, None, ObjectState::Foundation)),
            (10, (BuildState::Place, None, ObjectState::Foundation)),
            (511, (BuildState::Finished, None, ObjectState::Alive)),
            (512, (BuildState::Idle, None, ObjectState::Alive)),
        ]
    );
    let house = &t.players[1].objects[ListIndex::new(1)];
    assert_eq!(
        (house.construction.unwrap().progress, house.workers),
        (Seconds::new(25.0), 0)
    );
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(v.actions.build().copied().unwrap().target, None);
    let heard = v.ai.as_ref().unwrap().notifies.iter().map(|n| n.kind);
    assert_eq!(heard.collect::<Vec<_>>(), [NotifyKind::REQUIRES_SEARCH]);
}

/// A loaded builder's drop-off through its updates (`0x140ab0370` case 7, then
/// [`BuildAction::update`]'s state 4): from [`house_builder`]'s house it walks to the town centre
/// its drop-off found (state 8, its quiet `MoveTo` moving, then arrived), gives the player its 6
/// food there and carries nothing, lets the centre go and walks back to the house (4), faces it
/// (6) and builds again (7).
///
/// [`BuildAction::update`]: crate::construction::build::BuildAction::update
#[test]
fn a_loaded_builder_drops_its_food_at_the_centre_and_builds_on() {
    let mut t = house_builder();
    t.pathing = Pathing::new(
        GridSize::new(120, 120),
        vec![0; 120 * 120],
        vec![vec![1.0]; 8],
        0,
    );
    t.build_mesh().unwrap();
    let v = &mut t.players[1].objects[ListIndex::new(0)];
    (v.life.attribute_type, v.life.amount) = (Some(AttributeId::numbered(17)), Amount::new(6.0));
    let here = v.position();
    let m = v.body.mover_mut().unwrap();
    m.moved_at = here;
    m.stats.facets = Some(crate::objects::facing::SpriteFacets {
        count: 8,
        flag_1: false,
    });
    in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, c| {
            let mut b = *unit.actions.build().unwrap();
            let by = unit.releaser(c)?;
            let dropped = b.drop_again(c, unit, by);
            *unit.actions.build_mut().unwrap() = b;
            dropped
        },
    )
    .unwrap();
    let mut seen = Vec::new();
    for n in 0..350 {
        let _ = in_update(
            &mut t,
            (PlayerId::new(1), ListIndex::new(0)),
            50,
            Unit::update,
        )
        .unwrap();
        let v = &t.players[1].objects[ListIndex::new(0)];
        let b = v.actions.build().copied().unwrap();
        let now = (
            b.state,
            b.move_to.map(|m| m.state),
            b.site,
            v.life.amount.get(),
        );
        if seen.last().is_none_or(|&(_, last)| last != now) {
            seen.push((n, now));
        }
    }
    let drop = BuildState::Drop;
    assert_eq!(
        seen,
        [
            (0, (drop, Some(MoveTo::Moving), Some(object_id(700)), 6.0)),
            (
                177,
                (drop, Some(MoveTo::Arrived), Some(object_id(700)), 6.0)
            ),
            (178, (BuildState::Move, Some(MoveTo::Start), None, 0.0)),
            (179, (BuildState::Move, Some(MoveTo::Moving), None, 0.0)),
            (342, (BuildState::Move, Some(MoveTo::Arrived), None, 0.0)),
            (343, (BuildState::Work, None, None, 0.0)),
            (344, (BuildState::Place, None, None, 0.0)),
        ]
    );
    assert_eq!(t.players[1].attributes[0], Amount::new(6.0));
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)].life.attribute_type,
        None
    );
}

/// Object `i` of player `p`'s step (dt 50, outside a turn) whose only part is `step`: what it
/// leaves in the `Step`, and its result, which what follows it sees ([`World::step_object`]).
fn finished_with(
    t: &mut World,
    (p, i): (PlayerId, ListIndex),
    step: impl FnOnce(&mut step::Step<'_>) -> Result<crate::objects::unit::Outcome, GenieError>,
) -> Result<crate::objects::unit::Outcome, GenieError> {
    t.step_object((p, i), (Millis::new(50), None, false), |_, c| step(c))
        .map(|s| s.outcome)
}

/// A dying building lets its annexes go at once (the static update's death, `0x14118ad50`): in
/// the same update its annex, here the town centre, holds no main building and the building's
/// hit points, and the world's annex slots are clear.
#[test]
fn a_dying_building_lets_its_annexes_go_in_its_own_update() {
    use crate::objects::sprite::{Graphic, building_death};
    let mut t = house_builder();
    t.spatial.set_annex(object_id(700), Some(object_id(10028)));
    t.players[1].objects[ListIndex::new(2)].annex_of = Some(object_id(10028));
    let house = &t.players[1].objects[ListIndex::new(1)];
    let master = t.masters[1].get(house.life.master_id).unwrap();
    let progress = house.construction.unwrap().progress;
    let id = building_death(master, None, progress).unwrap();
    let shown = Graphic {
        id,
        frames: 10,
        facets: 1,
        speed_multiplier: 1.0,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: 0,
    };
    let _ = t.graphics.insert(id, shown);
    let seen = in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(1)),
        0,
        |unit, c| {
            unit.life.hp = HitPoints::new(3.0);
            unit.died(c, ObjectState::Unfinished)?;
            let (_, annex) = OthersRef::of(&c.w.players, c.player)
                .unwrap()
                .find(object_id(700))
                .unwrap();
            Ok((
                annex.annex_of,
                annex.life.hp.get(),
                c.w.spatial.annexes_of(object_id(10028)),
            ))
        },
    );
    assert_eq!(
        seen.map_err(|e| e.reason().to_owned()),
        Ok((None, 3.0, vec![]))
    );
}

/// The unit AI's orders of a step reach the unit and the units they name: a `GatherObject`'s
/// gather command takes the bush at once (the Gather is the unit's while its update runs); the
/// Gather's reseed without a farm lets it go and its build command gives the builder its Build,
/// both within the update; a drop elsewhere sends the unit to the drop site with the order back
/// to its target queued, and a move off a footprint gives it a `MoveTo` at once.
#[test]
fn the_unit_ai_s_orders_reach_the_unit_and_the_units_they_name() {
    use crate::gathering::gather::Next;
    use crate::unit_ai::Effect;
    // the calls run in the unit's step, and its top action right after them
    let applied = |t: &mut World, effects: Vec<Effect>| {
        in_update(t, (PlayerId::new(1), ListIndex::new(0)), 50, |unit, c| {
            for e in effects {
                unit.run_effect(e, None, c)?;
            }
            Ok(unit.top_action_kind())
        })
        .map_err(|e| e.reason().to_owned())
    };
    // what follows the Gather's update, and the unit's actions right after it
    let followed = |t: &mut World, next: Next| {
        in_update(t, (PlayerId::new(1), ListIndex::new(0)), 50, |unit, c| {
            let by = unit.releaser(c)?;
            let g = unit.actions.gather().copied().unwrap();
            let _ = unit.gather_next(c, next, (g.target, g.carry_to), by)?;
            let bush = OthersRef::of(&c.w.players, PlayerId::new(0))
                .unwrap()
                .find(object_id(600))
                .map(|(_, b)| b.workers);
            let gather = unit.actions.gather().map(|g| (g.target, g.state));
            let build = unit.actions.build().map(|b| b.target);
            Ok((bush, gather, build))
        })
        .map_err(|e| e.reason().to_owned())
    };
    let mut t = house_builder();
    assert_eq!(
        applied(
            &mut t,
            vec![Effect::Command(AiCommand::GatherObject(object_id(600)))]
        ),
        Ok(Some(crate::gathering::gather::GATHER))
    );
    let gatherer = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        (
            gatherer.top_action_kind(),
            gatherer.actions.gather().map(|g| (g.target, g.state))
        ),
        (
            Some(crate::gathering::gather::GATHER),
            Some((Some(object_id(600)), GatherState::Walk))
        )
    );
    assert_eq!(t.players[0].objects[ListIndex::new(0)].workers, 1);
    let reseeded = followed(&mut t, Next::Reseed { arrived: true });
    assert_eq!(
        reseeded,
        Ok((Some(0), Some((None, GatherState::Idle)), None))
    );

    let mut t = house_builder();
    t.players[1].objects[ListIndex::new(0)]
        .actions
        .set_build(None);
    assert!(
        applied(
            &mut t,
            vec![Effect::Command(AiCommand::GatherObject(object_id(600)))]
        )
        .is_ok()
    );
    t.players[1].objects[ListIndex::new(0)]
        .actions
        .gather_mut()
        .unwrap()
        .target = Some(object_id(10028));
    assert_eq!(
        followed(&mut t, Next::Build),
        Ok((Some(1), None, Some(Some(object_id(10028)))))
    );
    let builder = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(builder.top_action_kind(), Some(action_type::BUILD));

    let mut t = house_builder();
    let drop = Effect::Command(AiCommand::DropElsewhere {
        site: object_id(700),
        back: Some(object_id(600)),
    });
    assert!(applied(&mut t, vec![drop]).is_ok());
    let ai = t.players[1].objects[ListIndex::new(0)].ai.as_ref().unwrap();
    let queued: Vec<_> = ai.orders.iter().map(|o| (o.order, o.target)).collect();
    assert_eq!(
        (ai.current_target, queued),
        (
            Some(object_id(700)),
            vec![(OrderKind::OnTarget, Some(object_id(600)))]
        )
    );

    let mut t = house_builder();
    let aside = crate::unit_ai::AiMove {
        point: Position::new(25.0, 89.5, 0.0),
        range: Distance::ZERO,
        force: false,
        walk: false,
    };
    assert_eq!(
        applied(&mut t, vec![Effect::Command(AiCommand::MoveAside(aside))]),
        Ok(Some(crate::movement::move_to::MOVE_TO))
    );
}

/// An Enter's transport moves to the meeting point within the entering unit's update (slot
/// 0x340, [`World::ship_move`], the unit lent to the world): the transport's `MoveTo` is there
/// before the update ends; one the world does not hold cannot move. Its sprite is
/// [`an_ai_move_replaces_a_done_attack_with_a_move_to_the_point`]'s.
#[test]
fn an_enter_s_transport_moves_to_the_meeting_point_where_the_unit_runs() {
    let mut t = house_builder();
    let mut ship = t.players[1].objects[ListIndex::new(0)].clone();
    ship.id = object_id(9176);
    ship.actions = ActionList::default();
    t.players[1].objects.push(ship);
    let placed = *t.spatial.get(object_id(9175)).unwrap();
    t.spatial.insert(object_id(9176), placed);
    let meet = Position::new(25.0, 89.5, 0.0);
    let moved = |t: &mut World, ship: i32| {
        in_update(t, (PlayerId::new(1), ListIndex::new(0)), 50, |unit, c| {
            c.lend(unit, |w| w.ship_move(ObjectId::of_field(ship), meet))?;
            let (_, s) = c.w.object(object_id(9176)).expect("held");
            Ok(s.actions.move_to().map(|m| (m.goal(), m.range)))
        })
        .map_err(|e| e.reason().to_owned())
    };
    assert_eq!(moved(&mut t, 9176), Ok(Some((Some(meet), Distance::ZERO))));
    let unreached = crate::garrison::UNREACHED.reason().to_owned();
    assert_eq!(moved(&mut t, 4242), Err(unreached));
}

/// The updating unit's own kill with what a razing hit counted: player 1's attribute 98 by
/// each of `adds` (attribute, amount).
fn own_razing(adds: &[(AttributeId, f32)]) -> crate::combat::victims::KillLeft {
    crate::combat::victims::KillLeft {
        id: object_id(9175),
        kill: None,
        razed: vec![crate::combat::damage::Razed {
            attacker_player: 1,
            owner: PlayerId::new(1),
            event: None,
            adds: adds
                .iter()
                .map(|&(a, v)| (PlayerId::new(1), a, v))
                .collect(),
        }],
    }
}

/// The updating unit's own razings count where the hit runs, within its update
/// ([`crate::combat::victims::file_kill`] on the unit out of its slot): the attribute adds in
/// their order, and the first that fails stops the ones after it (a count of an attribute the
/// player does not have stops the next one).
#[test]
fn the_own_razings_count_where_the_hit_runs() {
    use crate::combat::victims::file_kill;
    let razed = |adds: &[(AttributeId, f32)]| {
        let mut t = house_builder();
        t.players[1].attributes[98] = Amount::new(10.0);
        let r = in_update(&mut t, (PlayerId::new(1), ListIndex::new(0)), 0, |u, c| {
            file_kill(c, u, own_razing(adds))?;
            Ok(c.w.players[1].attributes[98].get())
        });
        (
            r.map_err(|e| e.reason().to_owned()),
            t.players[1].attributes[98].get(),
        )
    };
    assert_eq!(razed(&[(attribute::TOTAL_VALUE, -5.0)]), (Ok(5.0), 5.0));
    let missing = Err("an attribute the player does not have".to_owned());
    assert_eq!(
        razed(&[
            (AttributeId::numbered(9999), 5.0),
            (attribute::TOTAL_VALUE, -5.0)
        ]),
        (missing, 10.0)
    );
}

/// A step that fails stops what follows it: the tiles whose marks it changed are not handed to
/// the planners after it ([`World::step_object`]'s `hand_changed_tiles`); a step that holds
/// hands them on.
#[test]
fn a_step_that_fails_stops_what_follows_it() {
    use crate::map::spatial::Obstruction;
    use crate::objects::unit::Outcome;
    let record = Obstruction {
        kind: ObstructionType::TYPE_2,
        class: ObstructionClass::UNIT,
    };
    let mut t = house_builder();
    let r = finished_with(&mut t, (PlayerId::new(1), ListIndex::new(0)), |c| {
        c.w.spatial.add_record(object_id(10028), record);
        Err(GenieError::unsupported("the step's own gap"))
    });
    assert_eq!(
        r.map_err(|e| e.reason().to_owned()),
        Err("the step's own gap".to_owned())
    );
    assert_ne!(
        t.spatial.take_changes(),
        Vec::<crate::pathing::tile_path::PlanRect>::new()
    );
    let mut t = house_builder();
    let r = finished_with(&mut t, (PlayerId::new(1), ListIndex::new(0)), |c| {
        c.w.spatial.add_record(object_id(10028), record);
        Ok(Outcome::Stay)
    });
    assert_eq!(r.map_err(|e| e.reason().to_owned()), Ok(Outcome::Stay));
    assert_eq!(
        t.spatial.take_changes(),
        Vec::<crate::pathing::tile_path::PlanRect>::new()
    );
}

/// What a completion leaves its owner, each at once ([`World::complete_building`] and the
/// routines it calls): a pasture puts its livestock on its edges
/// ([`World::setup_pasture`], from its first part), a building's stack unit is made at its
/// place ([`World::stack_unit`]), its master's technology is researched, and a master that
/// disappears when built is destroyed. A completed mule cart with nothing to find keeps its
/// state ([`World::cart_search`], from its first part).
#[test]
fn a_completion_s_parts_reach_their_owner() {
    use crate::construction::build::StackUnit;
    use crate::gathering::pasture::LIVESTOCK;
    use crate::technology::{TechDef, TechState};
    let made = |t: &World| -> Vec<(i32, i16, Position)> {
        t.players[1].objects.as_slice()[1..]
            .iter()
            .map(|u| (u.id.get(), u.life.master_id.get(), u.position()))
            .collect()
    };
    let mut t = pasture_game(Position::new(60.0, 60.0, 0.0), 2.0, &LIVESTOCK);
    assert_eq!(t.setup_pasture(PlayerId::new(1), object_id(50)), Ok(()));
    assert_eq!(
        made(&t),
        [
            (100, 1899, Position::new(58.5, 58.5, 0.0)),
            (101, 1899, Position::new(59.4, 58.5, 0.0))
        ]
    );
    let mut t = pasture_game(Position::new(60.0, 60.0, 0.0), 2.0, &LIVESTOCK);
    let stack = StackUnit {
        master: master_id(1899),
        at: Position::new(60.0, 60.0, 0.0),
        hp_lost: HitPoints::ZERO,
    };
    assert_eq!(t.stack_unit(PlayerId::new(1), object_id(50), stack), Ok(()));
    assert_eq!(made(&t), [(100, 1899, Position::new(60.0, 60.0, 0.0))]);

    // the house's completion with its master's technology, or disappearing when built
    let completed = |setup: fn(&mut crate::objects::masters::BuildingMaster)| {
        let mut t = house_builder();
        for masters in &mut t.masters {
            if let Some(b) = masters
                .get_mut(ids::HOUSE)
                .and_then(|m| m.building.as_mut())
            {
                setup(b);
            }
        }
        t.techs = vec![TechDef::default()];
        t.players[1].techs = vec![TechState {
            state: ResearchState::Available,
            research_cap: -1,
            building_master: None,
            ..TechState::default()
        }];
        let r = t
            .complete_building(object_id(10028))
            .map_err(|e| e.reason().to_owned());
        (t, r)
    };
    let (t, r) = completed(|b| b.tech = Some(tech_id(0)));
    assert_eq!(r, Ok(()));
    assert_eq!(t.players[1].techs[0].state, ResearchState::Researched);
    let (t, r) = completed(|b| b.disappears_when_built = true);
    assert_eq!(r, Ok(()));
    let house = &t.players[1].objects[ListIndex::new(1)].life;
    assert_eq!(
        (house.state, house.hp),
        (ObjectState::Removed, HitPoints::ZERO)
    );
    let mut t = house_builder();
    assert_eq!(
        t.cart_search(PlayerId::new(1), object_id(9175), ObjectState::Alive),
        Ok(())
    );
    let cart = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        (cart.life.state, cart.ai.as_ref().map(|a| a.current_target)),
        (ObjectState::Alive, Some(None))
    );
}

/// A built building's death in its own update (`FUN_1411dbb70`, state 3 from 2), each part
/// within the update in the game's order: its owner's watched list lets it go, a castle's count
/// (attribute 134, held at 0) and its paid cost (98 alone for a castle, the combat object's
/// `FUN_14121cc40`) leave the owner, and as the last building of its master it withdraws the
/// master's technology from state 3 (`FUN_141166ab0`'s scan over the owner's objects, the
/// castle lent to the world). Another castle standing keeps the technology.
#[test]
fn a_built_building_s_death_reaches_its_owner_within_its_update() {
    use crate::technology::{TechDef, TechState};
    let died = |another: bool| {
        let mut t = enter_world(vec![], 5);
        for masters in &mut t.masters {
            let mut castle = Master {
                id: ids::CASTLE,
                death_sprite: None,
                cost: costs(&[(attribute::STONE, 650, true)]),
                ..masters.get(ids::HOUSE).unwrap().clone()
            };
            let b = castle.building.as_mut().unwrap();
            (b.tech, b.destruction_sprite) = (Some(tech_id(0)), None);
            *masters = Masters::new(masters.iter().cloned().chain([castle]));
        }
        t.players[1].objects[ListIndex::new(1)].life.master_id = ids::CASTLE;
        if another {
            let mut other = t.players[1].objects[ListIndex::new(1)].clone();
            other.id = object_id(10031);
            t.players[1].objects.push(other);
        }
        t.techs = vec![TechDef::default()];
        t.players[1].techs = vec![TechState {
            state: ResearchState::Researched,
            ..TechState::default()
        }];
        let owner = &mut t.players[1];
        owner.attributes.resize(250, Amount::ZERO);
        (
            owner.attributes[134],
            owner.attributes[98],
            owner.attributes[165],
        ) = (Amount::new(0.5), Amount::new(1000.0), Amount::new(1000.0));
        t.players[1].watched.push(crate::vision::ghost::Watched {
            id: object_id(10028),
            cell: crate::vision::ghost::Cell::Tile(0),
            bits: 0,
        });
        in_update(
            &mut t,
            (PlayerId::new(1), ListIndex::new(1)),
            0,
            |unit, c| {
                unit.life.state = ObjectState::Dying;
                unit.died(c, ObjectState::Alive)?;
                let a = &c.w.players[1].attributes;
                let withdrawn = c.w.players[1].techs[0].state != ResearchState::Researched;
                Ok((
                    watches(c.w, 1, 10028),
                    [a[134], a[98], a[165]].map(Amount::get),
                    withdrawn,
                ))
            },
        )
        .map_err(|e| e.reason().to_owned())
    };
    assert_eq!(died(false), Ok((0, [0.0, 350.0, 1000.0], true)));
    assert_eq!(died(true), Ok((0, [0.0, 350.0, 1000.0], false)));
}

/// The world's update runs the builder of the walk-and-build test the same way the object's
/// own update does (4 walking, 6 and 7 at the house, 224 at its completion, then idle), and
/// the completion's stop before its unit AI resumes it in the same update: nothing freezes.
#[test]
fn a_builder_s_house_completes_in_the_world_s_update_and_its_unit_ai_resumes() {
    let mut t = house_builder();
    t.pathing = Pathing::new(
        GridSize::new(120, 120),
        vec![0; 120 * 120],
        vec![vec![1.0]; 8],
        0,
    );
    t.build_mesh().unwrap();
    let v = &mut t.players[1].objects[ListIndex::new(0)];
    let here = v.position();
    let m = v.body.mover_mut().unwrap();
    m.moved_at = here;
    m.stats.facets = Some(crate::objects::facing::SpriteFacets {
        count: 8,
        flag_1: false,
    });
    v.actions.build_mut().unwrap().task.work_value = 1.0;
    let mut seen = Vec::new();
    for n in 0..520 {
        t.tick(Millis::new(50));
        let b = t.players[1].objects[ListIndex::new(0)]
            .actions
            .build()
            .copied()
            .unwrap();
        let house = t.players[1].objects[ListIndex::new(1)].life.state;
        let now = (b.state, b.move_to.map(|m| m.state), house);
        if seen.last().is_none_or(|&(_, last)| last != now) {
            seen.push((n, now));
        }
    }
    assert_eq!(
        seen,
        [
            (
                0,
                (
                    BuildState::Move,
                    Some(MoveTo::Moving),
                    ObjectState::Foundation
                )
            ),
            (
                8,
                (
                    BuildState::Move,
                    Some(MoveTo::Arrived),
                    ObjectState::Foundation
                )
            ),
            (9, (BuildState::Work, None, ObjectState::Foundation)),
            (10, (BuildState::Place, None, ObjectState::Foundation)),
            (511, (BuildState::Finished, None, ObjectState::Alive)),
            (512, (BuildState::Idle, None, ObjectState::Alive)),
        ]
    );
    assert_eq!(t.players.iter().map(|p| p.freezes.len()).sum::<usize>(), 0);
}

/// [`World::run`] on `command` against its handler called by hand on a copy (`by_hand`, whose
/// error stops `player`'s lane): the same world after. Whether the command changed the world.
fn routes(
    t: &World,
    command: &Command,
    player: PlayerId,
    by_hand: impl FnOnce(&mut World) -> Result<(), GenieError>,
) -> bool {
    let mut ran = t.clone();
    ran.run(command);
    let mut hand = t.clone();
    if let Err(e) = by_hand(&mut hand) {
        hand.stop_player(player, e);
    }
    assert!(ran == hand, "{command:?}");
    ran != *t
}

/// The command phase runs each unit command by its handler ([`World::run`]), on
/// [`forage_world`]'s villager 9175 and town centre 700; an error stops the issuer's lane. A
/// human player's `UNIT_ORDER` takes the world time as its units' last human order time first.
#[test]
fn the_command_phase_runs_each_unit_command_by_its_handler() {
    let t = forage_world();
    let v = object_ids([9175]);
    let way = vec![Point::new(40.5, 89.5)];
    let sweep = crate::orders::Sweep {
        patrol: false,
        reset: true,
        queued: false,
        ignore_pending: false,
    };
    let unit_order = |action, human| {
        Command::UnitOrder(UnitOrderCommand {
            player: player_id(1),
            units: unit_list(v.clone()),
            target: Some(object_id(700)),
            point: Point::new(-1.0, -1.0),
            q_id: -1,
            through_ai: true,
            nearby: false,
            action: UnitOrderAction::of(action),
            param: 0,
            human,
        })
    };
    let changed = [
        routes(
            &t,
            &Command::Stop(StopCommand {
                player: player_id(1),
                units: unit_list(v.clone()),
            }),
            PlayerId::new(1),
            |w| w.stop_command(player_id(1), &v),
        ),
        routes(
            &t,
            &Command::Patrol(PatrolCommand {
                player: player_id(1),
                units: unit_list(v.clone()),
                waypoints: Waypoints::of(&way).unwrap(),
                reset: true,
            }),
            PlayerId::new(1),
            |w| w.patrol(player_id(1), &v, &way, true),
        ),
        routes(
            &t,
            &Command::AttackMove(AttackMoveCommand {
                player: player_id(1),
                units: unit_list(v.clone()),
                waypoints: Waypoints::of(&way).unwrap(),
                reset: true,
                queued: false,
                ignore_pending: false,
            }),
            PlayerId::new(1),
            |w| w.attack_move(player_id(1), &v, &way, sweep),
        ),
        routes(
            &t,
            &Command::SeekShelter(SeekShelterCommand {
                player: player_id(1),
                units: unit_list(v.clone()),
            }),
            PlayerId::new(1),
            |w| w.seek_shelter(player_id(1), &v),
        ),
        routes(
            &t,
            &Command::UnitAiState(UnitAiStateCommand {
                player: player_id(1),
                units: unit_list(v.clone()),
                state: UnitStance::DEFENSIVE,
            }),
            PlayerId::new(1),
            |w| w.unit_ai_state(player_id(1), &v, UnitStance::DEFENSIVE),
        ),
        routes(
            &t,
            &Command::AutoScout(AutoScoutCommand {
                player: player_id(1),
                units: unit_list(v.clone()),
            }),
            PlayerId::new(1),
            |w| w.auto_scout(player_id(1), &v),
        ),
        routes(
            &t,
            &Command::Unload(UnloadCommand {
                player: player_id(1),
                units: unit_list(object_ids([700])),
                point: Point::new(24.5, 89.5),
                pick: UnloadPick::of_packet(0, -1),
                sound: false,
            }),
            PlayerId::new(1),
            |w| {
                UnloadCommand {
                    player: player_id(1),
                    units: unit_list(object_ids([700])),
                    point: Point::new(24.5, 89.5),
                    pick: UnloadPick::of_packet(0, -1),
                    sound: false,
                }
                .run(w)
            },
        ),
        routes(
            &t,
            &Command::GoBackToWork2(GoBackToWork2Command {
                player: player_id(1),
                units: unit_list(v.clone()),
            }),
            PlayerId::new(1),
            |w| w.go_back_to_work(player_id(1), &v),
        ),
        routes(
            &t,
            &Command::Delete(DeleteCommand {
                player: player_id(1),
                id: object_id(9175),
            }),
            PlayerId::new(1),
            |w| w.delete(player_id(1), object_id(9175)),
        ),
        routes(
            &t,
            &Command::TransformObject(TransformObjectCommand {
                player: player_id(1),
                id: object_id(9175),
                master: master_id(120),
            }),
            PlayerId::new(1),
            |w| w.transform_object(player_id(1), object_id(9175), master_id(120)),
        ),
        routes(&t, &unit_order(5, false), PlayerId::new(1), |w| {
            w.garrison_order(player_id(1), &v, Some(object_id(700)), true, false)
        }),
        routes(&t, &unit_order(4, false), PlayerId::new(1), |w| {
            w.cancel_order(&v, 0, true)
        }),
        routes(&t, &unit_order(9, true), PlayerId::new(1), |w| {
            w.human_order(&v);
            w.unit_order(player_id(1), &v, 9)
        }),
    ];
    assert_eq!(
        changed,
        [
            true, true, true, false, true, false, false, false, true, false, false, false, true
        ]
    );
}

/// The command phase runs each player's command by its handler ([`World::run`]): a
/// tribute, a resignation, a flare, a market trade (which cannot fail), a GAME sub-command
/// (what the slice cannot model of it stops the player it is for) and an unmodelled command
/// (its player stops).
#[test]
fn the_command_phase_runs_each_player_command_by_its_handler() {
    use crate::players::tribute::Gift;
    let t = forage_world();
    let gifts = [Gift {
        attribute: 0,
        amount: Amount::new(10.0),
        fee: 0.0,
    }; 4];
    let game = |sub| GameCommand {
        issuer: player_id(1),
        sub,
        player: 0,
        a: 0,
        speed: 0.0,
        value: 0,
    };
    let why = GenieError::unsupported("a command the slice does not model");
    let changed = [
        routes(
            &t,
            &Command::Tribute(TributeCommand {
                player: player_id(1),
                to: player_id(0),
                gifts,
            }),
            PlayerId::new(1),
            |w| w.tribute(player_id(1), player_id(0), &gifts),
        ),
        routes(
            &t,
            &Command::Resign(ResignCommand {
                player: player_id(1),
                dropped: false,
            }),
            PlayerId::new(1),
            |w| w.resign(player_id(1), false),
        ),
        routes(
            &t,
            &Command::Flare(FlareCommand {
                player: player_id(1),
                target: None,
                point: Point::new(40.5, 89.5),
                players: 2,
                shown: Vec::new(),
            }),
            PlayerId::new(1),
            |w| w.flare(None, Point::new(40.5, 89.5), 2),
        ),
        routes(
            &t,
            &Command::Trade(TradeCommand {
                player: player_id(1),
                attribute: 1,
                times: 1,
                market: None,
                buy: true,
            }),
            PlayerId::new(1),
            |w| {
                w.buy(player_id(1), attribute::WOOD, 1).unwrap();
                Ok(())
            },
        ),
        routes(
            &t,
            &Command::Trade(TradeCommand {
                player: player_id(1),
                attribute: 1,
                times: 1,
                market: None,
                buy: false,
            }),
            PlayerId::new(1),
            |w| {
                w.sell(player_id(1), attribute::WOOD, 1).unwrap();
                Ok(())
            },
        ),
        routes(&t, &Command::Game(game(0)), PlayerId::new(0), |w| {
            w.game(&game(0))
        }),
        routes(
            &t,
            &Command::Unmodelled(UnmodelledCommand {
                player: player_id(1),
                packet: Vec::new(),
                why: why.clone(),
            }),
            PlayerId::new(1),
            |_| Err(why.clone()),
        ),
    ];
    assert_eq!(changed, [false, true, true, false, false, false, true]);
    // with the game's cheats a cheat code is not modelled: it stops the player it is for, or
    // its issuer for a player outside the game
    let mut cheats = t;
    cheats.settings.cheats_enabled = true;
    let cheat = |player| GameCommand {
        issuer: player_id(1),
        sub: crate::commands::game::sub::CHEAT,
        player,
        ..game(0)
    };
    assert!(routes(
        &cheats,
        &Command::Game(cheat(0)),
        PlayerId::new(0),
        |w| w.game(&cheat(0))
    ));
    assert!(routes(
        &cheats,
        &Command::Game(cheat(9)),
        PlayerId::new(1),
        |w| w.game(&cheat(9))
    ));
}

/// An order of units runs after its command's check ([`World::run`]): a MOVE of the villager
/// leaves it as the last commanded units, resets it for the order, takes the world time as its
/// last human order time and moves it; a human player's MOVE left without a unit (Gaia's bush)
/// does nothing, as does a FOLLOW of a target the world lacks; a GUARD takes the builders'
/// check; a RETREAT is not modelled (after the reset).
#[test]
fn the_command_phase_runs_an_order_after_its_check() {
    use crate::commands::Order;
    let t = forage_world();
    let v = object_ids([9175]);
    let walk = crate::orders::MoveOrder {
        target: None,
        point: Point::new(40.5, 89.5),
        queued: false,
        direct: false,
        human: true,
        control: false,
    };
    let order = |units: Vec<Option<ObjectId>>, order| {
        Command::Order(OrderCommand {
            player: player_id(1),
            units: unit_list(units),
            order,
        })
    };
    assert!(routes(
        &t,
        &order(v.clone(), Order::Move(walk)),
        PlayerId::new(1),
        |w| {
            w.set_last_commanded(1, v.clone());
            w.reset_order_units(&v).unwrap();
            w.human_order(&v);
            w.move_order(PlayerId::new(1), &v, &walk)
        }
    ));
    let none_left = order(object_ids([600]), Order::Move(walk));
    assert!(!routes(&t, &none_left, PlayerId::new(1), |_| Ok(())));
    let lost = order(v.clone(), Order::Follow(object_id(4444)));
    assert!(!routes(&t, &lost, PlayerId::new(1), |_| Ok(())));
    assert!(routes(
        &t,
        &order(v.clone(), Order::Guard(object_id(700))),
        PlayerId::new(1),
        |w| {
            w.reset_order_units(&v).unwrap();
            w.human_order(&v);
            w.guard(PlayerId::new(1), &v, object_id(700))
        }
    ));
    assert!(routes(
        &t,
        &order(
            v.clone(),
            Order::Retreat(crate::orders::Interact::default())
        ),
        PlayerId::new(1),
        |w| {
            w.set_last_commanded(1, v.clone());
            w.reset_order_units(&v).unwrap();
            Err(GenieError::unsupported("an order moves its units"))
        }
    ));
}

/// The last commanded units (`FUN_140e6b290`): a MOVE of its own list that its check lets
/// run leaves that list as its player's last commanded units, −1s and all; a refused MOVE
/// leaves them. A count of `0xffff` orders them, and its check writes its −1s back, refused
/// or not.
#[test]
fn an_order_of_the_last_commanded_units_reads_what_the_last_run_order_left() {
    use crate::commands::{Order, UnitList};
    let walk = crate::orders::MoveOrder {
        target: None,
        point: Point::new(40.5, 89.5),
        queued: false,
        direct: false,
        human: true,
        control: false,
    };
    let order = |units| {
        Command::Order(OrderCommand {
            player: player_id(1),
            units,
            order: Order::Move(walk),
        })
    };
    let mut t = forage_world();
    t.run(&order(unit_list(object_ids([9175, 600]))));
    assert_eq!(t.last_commanded.get(1), Some(&object_ids([9175, -1])));
    t.run(&order(unit_list(object_ids([600]))));
    assert_eq!(t.last_commanded.get(1), Some(&object_ids([9175, -1])));

    let mut cached = forage_world();
    cached.set_last_commanded(1, object_ids([600, 9175]));
    let checked = object_ids([-1, 9175]);
    assert!(routes(
        &cached,
        &order(UnitList::last_commanded()),
        PlayerId::new(1),
        |w| {
            w.set_last_commanded(1, checked.clone());
            w.reset_order_units(&checked).unwrap();
            w.human_order(&checked);
            w.move_order(PlayerId::new(1), &checked, &walk)
        }
    ));
    cached.set_last_commanded(1, object_ids([600]));
    let mut refused = cached.clone();
    refused.run(&order(UnitList::last_commanded()));
    cached.set_last_commanded(1, vec![None]);
    assert_eq!(refused, cached);
}

/// An order's units (`FUN_140e6b290`): comm player 0's and one past the world's are the
/// packet's as they are, with no last commanded units; any other's are the first 60, 500 for
/// a computer, or its last commanded units, none before its first.
#[test]
fn an_order_s_units_are_capped_unless_its_comm_player_is_outside_the_world() {
    use UnitList;
    let many = object_ids(0..600);
    let mut t = forage_world();
    let listed = unit_list(many.clone());
    assert_eq!(t.command_units(0, &listed), many);
    assert_eq!(t.command_units(t.players.len(), &listed), many);
    assert_eq!(t.command_units(1, &listed), many[..60]);
    assert_eq!(
        t.command_units(1, &UnitList::last_commanded()),
        Vec::<Option<ObjectId>>::new()
    );
    t.players[1].computer = true;
    assert_eq!(t.command_units(1, &listed), many[..500]);
    assert_eq!(t.most_units(t.players.len()), 0);
    t.set_last_commanded(0, object_ids([5]));
    t.set_last_commanded(t.players.len(), object_ids([6]));
    t.set_last_commanded(1, object_ids([7]));
    assert_eq!(t.last_commanded, [vec![], object_ids([7])]);
    assert_eq!(
        t.command_units(0, &UnitList::last_commanded()),
        Vec::<Option<ObjectId>>::new()
    );
    assert_eq!(
        t.command_units(1, &UnitList::last_commanded()),
        object_ids([7])
    );
}

/// The command phase runs each building command by its handler ([`World::run`]) on
/// [`forage_world`]'s town centre 700: a queue, a research after its buildings' check, a
/// gather point, the command queue, a gate, the town bell and its all-back-to-work.
#[test]
fn the_command_phase_runs_each_building_command_by_its_handler() {
    let t = forage_world();
    let tc = object_ids([700]);
    let research = || ResearchCommand {
        player: player_id(1),
        building: Some(object_id(700)),
        count: 1,
        buildings: tc.clone(),
        tech: 22,
        q_id: -1,
        shift: false,
    };
    let queue = QueueCommand {
        player: player_id(1),
        buildings: unit_list(tc.clone()),
        building_master: master_id(109),
        unit: master_id(83),
        amount: 1,
        button: 0,
    };
    let changed = [
        routes(
            &t,
            &Command::Queue(QueueCommand {
                player: player_id(1),
                buildings: unit_list(tc.clone()),
                building_master: master_id(109),
                unit: master_id(83),
                amount: 1,
                button: 0,
            }),
            PlayerId::new(1),
            |w| queue.run(w),
        ),
        routes(
            &t,
            &Command::Research(ResearchCommand {
                player: player_id(1),
                building: Some(object_id(700)),
                count: 1,
                buildings: tc.clone(),
                tech: 22,
                q_id: -1,
                shift: false,
            }),
            PlayerId::new(1),
            |w| {
                let Some(buildings) = w.research_check(&research())? else {
                    return Ok(());
                };
                w.research(&research(), &buildings)
            },
        ),
        routes(
            &t,
            &Command::GatherPoint(GatherPointCommand {
                player: player_id(1),
                units: unit_list(tc.clone()),
                point: Point::new(30.5, 85.5),
                target: None,
                target_master: None,
                own: false,
            }),
            PlayerId::new(1),
            |w| {
                GatherPointCommand {
                    player: player_id(1),
                    units: unit_list(tc.clone()),
                    point: Point::new(30.5, 85.5),
                    target: None,
                    target_master: None,
                    own: false,
                }
                .run(w)
            },
        ),
        routes(
            &t,
            &Command::CommandQueue(BuildingQueueCommand {
                player: player_id(1),
                units: unit_list(tc.clone()),
                target: None,
                point: Point::new(30.5, 85.5),
            }),
            PlayerId::new(1),
            |w| w.command_queue(player_id(1), &tc, None, Point::new(30.5, 85.5)),
        ),
        routes(
            &t,
            &Command::Gate(GateCommand {
                player: player_id(1),
                id: object_id(700),
            }),
            PlayerId::new(1),
            |w| w.gate(player_id(1), object_id(700)),
        ),
        routes(
            &t,
            &Command::TownBell(TownBellCommand {
                player: player_id(1),
                id: object_id(700),
                activate: true,
            }),
            PlayerId::new(1),
            |w| w.town_bell(player_id(1), object_id(700), true),
        ),
        routes(
            &t,
            &Command::AllBackToWork(AllBackToWorkCommand {
                player: player_id(1),
                id: object_id(700),
            }),
            PlayerId::new(1),
            |w| w.all_back_to_work(player_id(1), object_id(700)),
        ),
    ];
    // the town centre's master is bare: only the gate changes it (any building of type 80
    // toggles its lock)
    assert_eq!(changed, [false, false, false, false, true, false, false]);
}

/// `AskForHelp`'s walk of the last search's own and friends' lists
/// ([`UnitSide::call_for_help`] of the world's view): the asking unit passes itself; a live
/// helper with a unit AI within 3 tiles that is free ([`helper_free`]) answers, which is not
/// modelled; a busy, dead, unthinking or distant one does not. Without a search before it
/// the registry's last answer is not held.
///
/// [`UnitSide::call_for_help`]: crate::unit_ai::UnitSide::call_for_help
#[test]
fn a_call_for_help_walks_the_lists_for_a_free_helper_in_reach() {
    use crate::unit_ai::UnitSide;
    use crate::vision::registry::Record;
    let idle = UnitAi {
        ai_type: UnitAiType::SOLDIER,
        current_order: None,
        current_action: None,
        current_target: None,
        stance: UnitStance::AGGRESSIVE,
        ..UnitAi::default()
    };
    let record = |id, x| Record {
        id,
        distance: Distance::MAX,
        owner: 1,
        x,
        y: 10.0,
        rx: 0.2,
        ry: 0.2,
    };
    let lists = || {
        crate::players::ByRelation::new([
            Vec::new(),
            vec![record(object_id(123), 10.0), record(object_id(124), 10.3)],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        ])
    };
    let groups = crate::groups::UnitGroups::default();
    let call = |helper: Unit, searched| {
        help_view(helper, searched, &groups, |side| {
            side.call_for_help(object_id(50))
        })
    };
    assert_eq!(
        call(helper_scout(idle.clone(), None), Some(lists())),
        crate::unit_ai::helper_answers()
    );
    let busy = UnitAi {
        current_order: Some(OrderKind::Attack),
        ..idle.clone()
    };
    assert_eq!(call(helper_scout(busy, None), Some(lists())), Ok(()));
    let mut dead = helper_scout(idle.clone(), None);
    dead.life.state = ObjectState::Dying;
    assert_eq!(call(dead, Some(lists())), Ok(()));
    let mut unthinking = helper_scout(idle.clone(), None);
    unthinking.ai = None;
    assert_eq!(call(unthinking, Some(lists())), Ok(()));
    let mut far = helper_scout(idle.clone(), None);
    far.place_for_test(Position::new(14.0, 10.0, 0.0));
    assert_eq!(call(far, Some(lists())), Ok(()));
    assert_eq!(
        call(helper_scout(idle, None), None),
        Err(GenieError::unsupported(
            "a search that reads the registry's last answer (before any query)"
        ))
    );
}

/// RESEARCH's handler on its listed buildings, every one kept by its check.
fn research_listed(t: &mut World, r: &ResearchCommand) -> Result<(), GenieError> {
    t.research(r, &r.buildings)
}

/// [`forage_world`] with player 1 a computer player whose AI module (`player+0x7a8`) holds a
/// path map of the map's size, explored (0) but for the tiles `open` (-1), and the tactical
/// module's slots `slots`.
fn explorer_world(open: &[[i32; 2]], slots: Vec<i32>) -> World {
    use crate::players::ai_module::{AiModule, InfoModule, TacticalModule};
    let mut t = forage_world();
    t.players[1].computer = true;
    let mut path_map = crate::movement::explore::InfluenceMap::new(GridSize::new(120, 120), 0);
    for &[x, y] in open {
        path_map.set(Tile::new(x, y), 0xff);
    }
    t.ai.set(
        PlayerId::new(1),
        AiModule {
            info: InfoModule {
                size: GridSize::new(120, 120),
                path_map,
                quadrants: [[0; 3]; 16],
                explorer: None,
                search: Vec::new(),
            },
            tactical: TacticalModule { slot_times: slots },
        },
    );
    t
}

/// Player 1's villager 9175 asks its owner to hear that its Explore (605) waits for a point
/// (the base handler's case 0x202 hands the notify on): what the AI decided, as the stop that
/// its tasking or detasking ends in names it.
fn explorer_hears(t: &mut World) -> String {
    let heard = t.run_unit_ai((PlayerId::new(1), ListIndex::new(0)), |_, _, side, _, _| {
        side.owner_hears(Notify::own(
            object_id(9175),
            NotifyKind::REQUIRES_SEARCH,
            [605, 0, 0],
        ))
    });
    format!("{:?}", heard.unwrap_err())
}

/// A computer player's AI hears its explorer wanting a point: with the wait slot -1 early in
/// the game it pushes the rings about its town centre (700 at (20, 89.5), line of sight 5; the
/// explorer's 4, so the rings lie 12, 19, 26, 33 and 40 tiles out), takes the top point, (28, 80),
/// which the path map holds unexplored, and searches from it: the explorer reaches it, so the AI
/// takes the explorer's place and tasks the unit to it, and the tile becomes explored (0).
#[test]
fn a_computer_players_ai_tasks_its_explorer_to_the_unexplored_tile() {
    let mut t = explorer_world(&[[28, 80]], vec![]);
    for masters in &mut t.masters {
        masters.get_mut(master_id(120)).unwrap().line_of_sight = Distance::new(4.0);
        masters.get_mut(ids::TOWN_CENTRE).unwrap().line_of_sight = Distance::new(5.0);
    }
    // no sight: the player sees no tile now
    t.spatial.sight = None;
    let said = explorer_hears(&mut t);
    assert!(said.contains("tasks explorer"), "{said}");
    assert!(said.contains("(28, 80)"), "{said}");
    let info = &t.ai.get(PlayerId::new(1)).unwrap().info;
    assert_eq!(info.explorer, ObjectId::of_field(9175));
    assert_eq!(info.path_map.get(Tile::new(28, 80)), Some(0));
    // the rings' other points wait for the next notify
    assert_eq!(info.search.len(), 30);
}

/// With every tile explored the search finds none and the AI detasks the explorer.
#[test]
fn a_computer_players_ai_detasks_an_explorer_with_nothing_left_to_explore() {
    let mut t = explorer_world(&[], vec![]);
    let said = explorer_hears(&mut t);
    assert!(said.contains("detasks explorer"), "{said}");
}

/// A human player's owner hears nothing the simulation reads, and a notify of another
/// kind is not the explore case.
#[test]
fn only_a_computer_players_explorer_notify_reaches_the_ai() {
    let mut human = explorer_world(&[[31, 89]], vec![]);
    human.players[1].computer = false;
    let before = human.ai.clone();
    human
        .run_unit_ai((PlayerId::new(1), ListIndex::new(0)), |_, _, side, _, _| {
            side.owner_hears(Notify::own(
                object_id(9175),
                NotifyKind::REQUIRES_SEARCH,
                [605, 0, 0],
            ))
        })
        .unwrap();
    assert_eq!(human.ai, before);
    let mut t = explorer_world(&[[31, 89]], vec![]);
    t.run_unit_ai((PlayerId::new(1), ListIndex::new(0)), |_, _, side, _, _| {
        side.owner_hears(Notify::own(
            object_id(9175),
            NotifyKind::from(0x201),
            [605, 0, 0],
        ))?;
        side.owner_hears(Notify::own(
            object_id(9175),
            NotifyKind::REQUIRES_SEARCH,
            [999, 0, 0],
        ))
    })
    .unwrap();
}
