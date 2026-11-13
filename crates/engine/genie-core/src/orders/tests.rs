use super::*;
use crate::GenieResult;
use crate::combat::attack::tests::own_attack;
use crate::combat::attack::{ATTACK, AttackState};
use crate::combat::{AttackTargetId, Firepower};
use crate::commands::UnitOrderAction;
use crate::commands::UnloadPick;
use crate::construction::MainBuildingId;
use crate::construction::build::BuildState;
use crate::construction::build::tests::started;
use crate::construction::build::{BuildAction, BuildTask, Construction};
use crate::construction::{BuilderCount, FoundationId};
use crate::garrison::ContainerId;
use crate::garrison::ENTER;
use crate::garrison::WorkReturn;
use crate::gathering::DropSiteId;
use crate::gathering::gather::GATHER;
use crate::gathering::gather::GatherState;
use crate::gathering::hunt::HuntState;
use crate::gathering::hunt::tests::{hunting, walker};
use crate::gathering::{GatherTargetId, HuntTargetId};
use crate::groups::Differentiator;
use crate::groups::FormationKind;
use crate::groups::GroupCommandKind;
use crate::groups::GroupFlags;
use crate::groups::GroupTarget;
use crate::groups::{GroupLink, Member};
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Restriction;
use crate::map::RestrictionId;
use crate::map::Snow;
use crate::map::TerrainFlags;
use crate::map::Tile;
use crate::map::spatial::{Obstruction, Placed, Spatial, Tiles};
use crate::map::terrain::TileTerrain;
use crate::map::terrain::{Terrain, TerrainKind};
use crate::math::fmath::BVector;
use crate::movement::MoveStats;
use crate::movement::PathFlags;
use crate::movement::WaypointIndex;
use crate::movement::move_to::{MOVE_TO, MoveTo, MoveToAction, MoveWait, TargetMoved};
use crate::objects::Graphics;
use crate::objects::ObjectId;
use crate::objects::SpeedMultiplier;
use crate::objects::SpriteFlags;
use crate::objects::SpriteId;
use crate::objects::Voice;
use crate::objects::WorkRate;
use crate::objects::actions::ActionList;
use crate::objects::layers::{Action, Motion, MotionMut, Static};
use crate::objects::lifecycle::Held;
use crate::objects::lifecycle::Lifecycle;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ActionPart;
use crate::objects::masters::AnimatedPart;
use crate::objects::masters::BuildableOn;
use crate::objects::masters::ClassId;
use crate::objects::masters::CombatPart;
use crate::objects::masters::CreatablePart;
use crate::objects::masters::DamageGraphic;
use crate::objects::masters::FogVisibility;
use crate::objects::masters::FormationClass;
use crate::objects::masters::MasterKind;
use crate::objects::masters::MovingPart;
use crate::objects::masters::ObjectFlags;
use crate::objects::masters::ObstructionClass;
use crate::objects::masters::ObstructionKind;
use crate::objects::masters::TargetOwner;
use crate::objects::masters::TaskId;
use crate::objects::masters::TaskSprites;
use crate::objects::masters::TaskTerrain;
use crate::objects::masters::{Masters, Task, WorkValue};
use crate::objects::sprite::{ActiveSprite, Graphic};
use crate::objects::unit::UnitFlags;
use crate::objects::{FacetCount, Frame, WorkerCount};
use crate::orders::ControlKey;
use crate::orders::Kept;
use crate::orders::MasterSwap;
use crate::orders::OrderFlags;
use crate::orders::OrderSource;
use crate::orders::ReadyOrders;
use crate::orders::SavedPatrols;
use crate::orders::split::JoinTest;
use crate::orders::split::splits_together;
use crate::orders::town_bell::BellShelter;
use crate::ownership::artifact::Artifact;
use crate::pathing::Pathing;
use crate::players::Amount;
use crate::players::ByPlayer;
use crate::players::attribute::AttributeId;
use crate::players::relation_row;
use crate::players::relations;
use crate::test_ids::adds;
use crate::test_ids::carrying;
use crate::test_ids::class_of;
use crate::test_ids::group_id;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::test_ids::object_id_list;
use crate::test_ids::object_ids;
use crate::test_ids::player_id;
use crate::test_ids::task_id;
use crate::test_ids::task_key;
use crate::testing::ai::run_ai;
use crate::testing::attribute;
use crate::testing::class;
use crate::testing::ids;
use crate::testing::roles;
use crate::testing::scene::{PLAYER, Scene, SceneObject, move_by, moving_master, walking};
use crate::testing::tuning;
use crate::unit_ai::AiAction;
use crate::unit_ai::Answer;
use crate::unit_ai::Issuer;
use crate::unit_ai::Notifies;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::OrderForce;
use crate::unit_ai::OrderKind;
use crate::unit_ai::UnitAiKind;
use crate::unit_ai::UnitStance;
use crate::unit_ai::{Notify, OrderPriority, PatrolPath};
use crate::vision::VisionBits;
use crate::vision::registry::Registry;
use crate::vision::visibility::Visibility;
use crate::world::Formations;
use crate::world::Seconds;
use crate::world::WholeSeconds;
use crate::world::parts::Object;
use crate::world::parts::ObjectParts;
use crate::world::settings::OptionFlags;
use crate::world::tests::{forage_world, hunt_world};
use crate::world::{FrameSeed, Gate, Player};
use std::sync::Arc;

/// The object's own update knows its slot: it is taken while it still holds the object, and
/// the object is found by its id once the list moved or the slot is out of the lists; an
/// object asleep or gone has no live slot.
#[test]
fn an_object_own_slot_is_taken_while_it_holds_the_object() {
    let unit = |id| Unit {
        id,
        ..Unit::default()
    };
    let world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player {
                objects: vec![unit(object_id(1)), unit(object_id(2))].into(),
                ..Player::default()
            },
            Player {
                objects: vec![unit(object_id(3))].into(),
                sleeping: vec![unit(object_id(4))].into(),
                ..Player::default()
            },
        ],
    );
    assert_eq!(
        world.live_slot((PlayerId::new(1), ListIndex::new(0), object_id(3))),
        Some((PlayerId::new(1), ListIndex::new(0)))
    );
    assert_eq!(
        world.live_slot((PlayerId::new(0), ListIndex::new(0), object_id(2))),
        Some((PlayerId::new(0), ListIndex::new(1)))
    );
    assert_eq!(
        world.live_slot((PlayerId::new(5), ListIndex::new(9), object_id(3))),
        Some((PlayerId::new(1), ListIndex::new(0)))
    );
    assert_eq!(
        world.live_slot((PlayerId::new(1), ListIndex::new(0), object_id(4))),
        None
    );
    assert_eq!(
        world.live_slot((PlayerId::new(0), ListIndex::new(0), object_id(7))),
        None
    );
}

/// records/425906646's gate 27745 (masters 88 and 91, 1650 hit points each, three damage
/// sprites): unhurt, its share is 0 under either master; a resource's share is not modelled.
#[test]
fn damage_share_is_the_whole_less_the_hit_points_share() {
    assert_eq!(
        damage_share(
            HitPoints::new(1650.0),
            MasterHitPoints::of_data(1650),
            class::GATE,
            &roles()
        ),
        Ok(0)
    );
    assert_eq!(
        damage_share(
            HitPoints::new(825.0),
            MasterHitPoints::of_data(1650),
            class::GATE,
            &roles()
        ),
        Ok(50)
    );
    assert_eq!(
        damage_share(
            HitPoints::new(1.0),
            MasterHitPoints::of_data(1650),
            class::GATE,
            &roles()
        ),
        Ok(99)
    );
    assert_eq!(
        damage_share(
            HitPoints::new(2000.0),
            MasterHitPoints::of_data(1650),
            class::GATE,
            &roles()
        ),
        Ok(0)
    );
    assert!(
        damage_share(
            HitPoints::new(10.0),
            MasterHitPoints::of_data(100),
            class::LIVESTOCK,
            &roles()
        )
        .is_err()
    );
}

/// records/425906646's update 1641: player 6's drop-off order reaches its goose 28766 (class
/// 58, 100 food, no drop sites, its default task on top): no table entry takes class 58, so
/// it does nothing. An entry of its class (taking food) leads to the nearest such building
/// of the player's tracker in state 2 (`FUN_141658e20`), but a goose (below type 70) does no
/// work on it.
#[test]
fn drop_off_order_leaves_livestock_without_drop_site_alone() {
    let goose = Unit {
        id: object_id(28766),
        life: Lifecycle {
            master_id: master_id(1243),
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![Player {
            objects: vec![goose].into(),
            ..Player::default()
        }],
    );
    world.model.masters = vec![Arc::new(Masters::new([Master {
        id: master_id(1243),
        class: class::LIVESTOCK,
        ..Master::default()
    }]))];
    let tc = DropSite {
        building: ids::TOWN_CENTRE,
        worker_class: Some(class::VILLAGER),
        ..DropSite::default()
    };
    world.model.drop_sites = Arc::new(vec![tc.clone()]);
    assert!(
        world
            .drop_off_unasked(
                PlayerId::new(0),
                ListIndex::new(0),
                UnitOrderAction::DropOffAndBack
            )
            .is_ok()
    );
    world.model.drop_sites = Arc::new(vec![
        tc,
        DropSite {
            building: ids::PASTURE,
            worker_class: Some(class::LIVESTOCK),
            resource_kinds: vec![attribute::FOOD],
            ..DropSite::default()
        },
    ]);
    let pen = |id, x: f32, state| Unit {
        id,
        place: Static::at(Position::new(x, 10.0, 0.0)),
        life: Lifecycle {
            master_id: ids::PASTURE,
            state,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    world.model.masters[0] = Arc::new(Masters::new([
        world.model.masters[0].get(master_id(1243)).unwrap().clone(),
        Master {
            id: ids::PASTURE,
            kind: MasterKind::BUILDING,
            class: class::BUILDING,
            ..Master::default()
        },
    ]));
    world.players[0].objects[ListIndex::new(0)]
        .life
        .attribute_kind = Some(attribute::FOOD);
    world.players[0].objects[ListIndex::new(0)].place_for_test(Position::new(10.0, 10.0, 0.0));
    for (id, x, state) in [
        (500, 30.0, ObjectState::Alive),
        (501, 12.0, ObjectState::Foundation),
        (502, 20.0, ObjectState::Alive),
        (503, 15.0, ObjectState::Alive),
    ] {
        let _ = world
            .push_live(PlayerId::new(0), pen(object_id(id), x, state))
            .unwrap();
        world
            .track_add(PlayerId::new(0), object_id(id), ids::PASTURE)
            .unwrap();
    }
    // the tracker's done list: the foundation 501 waits, 503 is the nearest standing one
    let key = world.tracker_key(PlayerId::new(0), ids::PASTURE);
    for id in [503, 502, 500] {
        world.players[0].tracker.complete(object_id(id), key);
    }
    assert_eq!(
        world
            .table_drop_site(
                PlayerId::new(0),
                Point::new(10.0, 10.0),
                Some(attribute::FOOD),
                class::LIVESTOCK
            )
            .unwrap(),
        Some(DropSiteId::new(object_id(503)))
    );
    assert_eq!(
        world
            .table_drop_site(
                PlayerId::new(0),
                Point::new(10.0, 10.0),
                Some(attribute::WOOD),
                class::LIVESTOCK
            )
            .unwrap(),
        None
    );
    let before = world.players[0].objects[ListIndex::new(0)].clone();
    assert!(
        world
            .drop_off_unasked(
                PlayerId::new(0),
                ListIndex::new(0),
                UnitOrderAction::DropOffAndBack
            )
            .is_ok()
    );
    assert_eq!(world.players[0].objects[ListIndex::new(0)], before);
}

/// `UNIT_ORDER`'s drop-off on a builder carrying wood (`FUN_14161e370` on a Build: slot 0x78,
/// `FUN_140a78060`, then `set_state(8)`): it leaves its foundation's builders, takes the
/// drop-site table's nearest mill (`FUN_141658e20`) as its second target and walks to it
/// within the Build task's range (its master has no task on the mill). Without a mill in the
/// table it walks back to its foundation; carrying nothing it is not ordered at all.
#[test]
fn unit_order_drop_off_sends_carrying_builder_to_the_table_drop_site() {
    use crate::map::spatial::Placed;
    let placed = |position, master, radius| Placed {
        position,
        radius,
        class: class::BUILDING,
        player: PlayerId::new(0),
        master,
        obstruction_kind: ObstructionKind::KIND_2,
        buildable_on: BuildableOn::NO,
        tall: true,
        moves: false,
        unknown: None,
        obstruction: None,
    };
    let villager = Object::with_actions(
        Unit {
            id: object_id(10),
            life: Lifecycle {
                master_id: ids::VILLAGER_MALE,
                state: ObjectState::Alive,
                attribute_kind: Some(attribute::WOOD),
                amount: Amount::new(5.0),
                ..Lifecycle::default()
            },
            place: Static::at(Position::new(10.0, 10.0, 0.0)),
            ..Unit::default()
        },
        Action {
            list: ActionList::default().with_build(Some(BuildAction {
                state: BuildState::Work,
                added: true,
                ..started(
                    object_id(600),
                    Position::new(5.0, 10.0, 0.0),
                    Distance::ZERO,
                    BuildTask {
                        range: Distance::new(0.1),
                        work_value: WorkValue::new(1.0),
                        work_rate: WorkRate::new(1.0),
                        resource: None,
                        id: TaskId::of_task(-1),
                        key: task_key(ActionKind::BUILD),
                        sprites: TaskSprites::NONE,
                    },
                )
            })),
            ..Default::default()
        },
    );
    let foundation = Object::with_building(
        Unit {
            id: object_id(600),
            place: Static::at(Position::new(5.0, 10.0, 0.0)),
            life: Lifecycle {
                master_id: ids::HOUSE,
                ..Lifecycle::default()
            },
            ..Unit::default()
        },
        Building {
            construction: Some(Construction {
                progress: Seconds::new(1.0),
                desolid: false,
                builders: BuilderCount::of_data(1),
                hit_points: MasterHitPoints::of_data(550),
                build_time: WholeSeconds::of_data(25),
                record: Obstruction {
                    kind: ObstructionKind::KIND_2,
                    class: ObstructionClass::UNIT,
                },
                snow: Snow::None,
                sprite_override: false,
            }),
            ..Default::default()
        },
    );
    let mill = Unit {
        id: object_id(503),
        place: Static::at(Position::new(15.0, 10.0, 0.0)),
        life: Lifecycle {
            master_id: master_id(68),
            state: ObjectState::Alive,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let world = || {
        let mut world = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO],
            vec![Player {
                objects: vec![villager.unit.clone(), foundation.unit.clone(), mill.clone()].into(),
                ..Player::default()
            }],
        )
        .with_buildings([(foundation.unit.id, foundation.parts.building.clone())])
        .with_actions([(villager.unit.id, villager.parts.actions.clone())])
        .with_motions([(villager.unit.id, Motion::Moving(walker()))]);
        world.model.masters = vec![Arc::new(Masters::new([
            Master {
                id: ids::VILLAGER_MALE,
                kind: MasterKind::COMBAT,
                class: class::VILLAGER,
                radius: Footprint::new(0.2, 0.2),
                // no sprites (the world's sprites' table is empty)
                standing_sprite: None,
                moving: Some(MovingPart {
                    move_sprite: None,
                    run_sprite: None,
                    ..MovingPart::default()
                }),
                ..Master::default()
            },
            Master {
                id: ids::HOUSE,
                kind: MasterKind::BUILDING,
                class: class::BUILDING,
                radius: Footprint::new(1.0, 1.0),
                ..Master::default()
            },
            Master {
                id: master_id(68),
                kind: MasterKind::BUILDING,
                class: class::BUILDING,
                radius: Footprint::new(1.0, 1.0),
                ..Master::default()
            },
        ]))];
        world.spatial.insert(
            object_id(10),
            placed(
                Position::new(10.0, 10.0, 0.0),
                ids::VILLAGER_MALE,
                Footprint::new(0.2, 0.2),
            ),
        );
        world.spatial.insert(
            object_id(600),
            placed(
                Position::new(5.0, 10.0, 0.0),
                ids::HOUSE,
                Footprint::new(1.0, 1.0),
            ),
        );
        world.spatial.insert(
            object_id(503),
            placed(
                Position::new(15.0, 10.0, 0.0),
                master_id(68),
                Footprint::new(1.0, 1.0),
            ),
        );
        world
            .track_add(PlayerId::new(0), object_id(503), master_id(68))
            .unwrap();
        let key = world.tracker_key(PlayerId::new(0), master_id(68));
        world.players[0].tracker.complete(object_id(503), key);
        world
    };
    let mut game = world();
    game.model.drop_sites = Arc::new(vec![DropSite {
        building: master_id(68),
        worker_class: Some(class::VILLAGER),
        resource_kinds: vec![attribute::WOOD],
        ..DropSite::default()
    }]);
    game.unit_order(
        player_id(0),
        &object_ids([10]),
        UnitOrderAction::DropOffAndBack,
    )
    .unwrap();
    let build = game
        .parts
        .actions_of(game.players[0].objects[ListIndex::new(0)].id)
        .list
        .build()
        .copied()
        .unwrap();
    let walk = build.state.move_to().copied().unwrap();
    assert_eq!(
        (
            build.state.bare(),
            build.site,
            ObjectId::field(walk.target),
            walk.range
        ),
        (
            BuildState::Drop { move_to: None },
            Some(DropSiteId::new(object_id(503))),
            503,
            Distance::new(0.1)
        )
    );
    assert_eq!(
        (walk.state, walk.on_target_moved, build.added),
        (MoveTo::Start, TargetMoved::AsksAgain, false)
    );
    assert_eq!(
        game.parts
            .building_of(game.players[0].objects[ListIndex::new(1)].id)
            .construction
            .map(|construction| construction.builders.get()),
        Some(0)
    );
    assert_eq!(
        game.players[0].objects[ListIndex::new(2)].workers,
        WorkerCount::of_data(1)
    );
    // its slot 0x820 asks at no point (x, y and z −1): a task for the terrain under the mill
    // is not found, and the walk keeps the Build task's range
    let mut game = world();
    game.model.drop_sites = Arc::new(vec![DropSite {
        building: master_id(68),
        worker_class: Some(class::VILLAGER),
        resource_kinds: vec![attribute::WOOD],
        ..DropSite::default()
    }]);
    game.spatial.terrain = Some(Terrain::new(
        GridSize::new(20, 20),
        vec![TileTerrain::of_save(0, 255, -1); 400],
        vec![TerrainKind {
            shown: true,
            flags: TerrainFlags::LAND,
            forest: false,
        }],
        vec![Restriction::new(vec![1.0])],
    ));
    Arc::make_mut(&mut game.model.masters[0])
        .get_mut(ids::VILLAGER_MALE)
        .unwrap()
        .action = Some(ActionPart {
        tasks: vec![Task {
            action_kind: ActionKind::named(99),
            terrain: TaskTerrain::of_data(0),
            work_range: Distance::new(2.0),
            ..Task::default()
        }],
        ..ActionPart::default()
    });
    game.unit_order(
        player_id(0),
        &object_ids([10]),
        UnitOrderAction::DropOffAndBack,
    )
    .unwrap();
    let walk_range = game
        .parts
        .actions_of(game.players[0].objects[ListIndex::new(0)].id)
        .list
        .build()
        .and_then(|build_action| build_action.state.move_to().copied())
        .map(|walk| walk.range);
    assert_eq!(walk_range, Some(Distance::new(0.1)));
    // no table entry for wood: back to the foundation
    let mut game = world();
    game.unit_order(
        player_id(0),
        &object_ids([10]),
        UnitOrderAction::DropOffAndBack,
    )
    .unwrap();
    let build = game
        .parts
        .actions_of(game.players[0].objects[ListIndex::new(0)].id)
        .list
        .build()
        .copied()
        .unwrap();
    assert_eq!(
        (
            build.state.bare(),
            build.site,
            build
                .state
                .move_to()
                .copied()
                .map(|move_to_action| ObjectId::field(move_to_action.target))
        ),
        (BuildState::Move { move_to: None }, None, Some(600))
    );
    // carrying nothing: not ordered
    let mut game = world();
    game.players[0].objects[ListIndex::new(0)].life.amount = Amount::ZERO;
    game.unit_order(
        player_id(0),
        &object_ids([10]),
        UnitOrderAction::DropOffAndBack,
    )
    .unwrap();
    assert_eq!(
        game.parts
            .actions_of(game.players[0].objects[ListIndex::new(0)].id)
            .list
            .build()
            .map(|build_action| build_action.state.bare()),
        Some(BuildState::Work)
    );
    // 7 asks the Build as 8 does; 6 does not ask it: the search alone (no drop site in the
    // table here, nothing to work on)
    let mut game = world();
    game.model.drop_sites = Arc::new(vec![DropSite {
        building: master_id(68),
        worker_class: Some(class::VILLAGER),
        resource_kinds: vec![attribute::WOOD],
        ..DropSite::default()
    }]);
    game.unit_order(player_id(0), &object_ids([10]), UnitOrderAction::DropOff)
        .unwrap();
    assert_eq!(
        game.parts
            .actions_of(game.players[0].objects[ListIndex::new(0)].id)
            .list
            .build()
            .map(|build_action| (build_action.state.bare(), build_action.site)),
        Some((
            BuildState::Drop { move_to: None },
            Some(DropSiteId::new(object_id(503)))
        ))
    );
    let mut world = world();
    world
        .unit_order(
            player_id(0),
            &object_ids([10]),
            UnitOrderAction::DropOffSearch,
        )
        .unwrap();
    assert_eq!(
        world
            .parts
            .actions_of(world.players[0].objects[ListIndex::new(0)].id)
            .list
            .build()
            .map(|build_action| build_action.state.bare()),
        Some(BuildState::Work)
    );
    assert!(
        world
            .unit_order(player_id(0), &object_ids([10]), UnitOrderAction::Gather)
            .is_err()
    );
}

/// The work command's Attack (`0x140a7f8c0`) of a soldier that may move, touching Gaia's
/// wolf: the start ends in `set_state(4)`, whose `MoveTo` to the target starts arrived (state
/// 1 within 0.01, `FUN_140aa2100`; 426954810's p6 at 1730). Its `set_state(1)`
/// (`0x140aa29e0`, not quiet) shows the standing sprite (slot 0x798) at the standing speed and
/// clears `+0x7b8`; the Attack waits in state 4 for the `MoveTo`'s end.
#[test]
fn work_attack_on_touching_target_starts_its_move_to_arrived() {
    let (mut world, task) = work_attack(UnitStance::Aggressive);
    world
        .start_attack(
            PlayerId::new(1),
            ListIndex::new(0),
            object_id(50),
            &task,
            Position::new(10.4, 10.0, 0.0),
            Notifies::Cleared,
        )
        .unwrap();
    let unit = &world.players[1].objects[ListIndex::new(0)];
    let attack = world
        .parts
        .actions_of(unit.id)
        .list
        .attack()
        .expect("the soldier's Attack");
    assert_eq!(
        (
            attack.state.bare(),
            attack
                .state
                .move_to()
                .copied()
                .map(|move_to_action| move_to_action.state),
            attack.target2
        ),
        (
            AttackState::Move { move_to: None },
            Some(MoveTo::Arrived),
            Some(AttackTargetId::new(object_id(50)))
        )
    );
    assert_eq!(
        unit.sprite.map(|active_sprite| active_sprite.graphic.id),
        Some(SpriteId::named(300))
    );
    assert_eq!(world.parts.anim_speed_of(unit.id), Some(Speed::ZERO));
    assert_eq!(
        world.parts.mover_of(unit.id).map(|mover| mover.stats.speed),
        Some(Speed::new(0.8))
    );
}

/// The work command's check of the top Attack (`FUN_14106ff70`, [`World::keep_attack`]): an
/// Attack in state 4 on this target is kept when the filter is none or 9, and its unit AI hears
/// of it as of a new one (the retarget time takes the world's time); another target or filter
/// does not keep it.
#[test]
fn work_command_keeps_the_top_attack_on_its_target_and_tells_its_unit_ai() {
    let (mut game, task) = work_attack(UnitStance::Aggressive);
    let at = Position::new(10.4, 10.0, 0.0);
    game.start_attack(
        PlayerId::new(1),
        ListIndex::new(0),
        object_id(50),
        &task,
        at,
        Notifies::Cleared,
    )
    .unwrap();
    game.world_time = WorldTime::from_millis(3500);
    assert_eq!(
        game.keep_attack(
            PlayerId::new(1),
            ListIndex::new(0),
            Some(object_id(50)),
            Some(ENTER),
            at,
            Notifies::Cleared
        ),
        Ok(Answer::NotTaken)
    );
    assert_eq!(
        game.keep_attack(
            PlayerId::new(1),
            ListIndex::new(0),
            Some(object_id(51)),
            None,
            at,
            Notifies::Cleared
        ),
        Ok(Answer::NotTaken)
    );
    let ai = |world: &World| {
        world
            .parts
            .unit_ai
            .get(world.players[1].objects[ListIndex::new(0)].id)
            .map(|unit_ai| unit_ai.last_retarget_time)
    };
    assert_ne!(ai(&game), Some(WorldTime::from_millis(3500)));
    assert_eq!(
        game.keep_attack(
            PlayerId::new(1),
            ListIndex::new(0),
            Some(object_id(50)),
            Some(ATTACK),
            at,
            Notifies::Cleared
        ),
        Ok(Answer::Taken)
    );
    assert_eq!(ai(&game), Some(WorldTime::from_millis(3500)));
}

/// The attack task's aim (`FUN_141189b20`): Gaia's wolf aims at itself; a building annex at
/// its main building, with the annex's owner, class and fog visibility but the main building's
/// place and class; a target or a main building the slice does not hold fails.
#[test]
fn an_attack_task_aims_at_its_target_or_an_annex_main_building() {
    let (mut world, _) = work_attack(UnitStance::Aggressive);
    let aim = world.attack_aim(object_id(50)).unwrap();
    assert_eq!(
        (aim.owner, aim.class, aim.fog_visibility),
        (PlayerId::new(0), class::PREDATOR, FogVisibility::INVISIBLE)
    );
    assert_eq!(
        (aim.aimed, aim.at, aim.aimed_class),
        (
            object_id(50),
            Position::new(10.4, 10.0, 0.0),
            class::PREDATOR
        )
    );
    annexed(&mut world);
    let aim = world.attack_aim(object_id(61)).unwrap();
    assert_eq!(
        (aim.owner, aim.class, aim.fog_visibility),
        (PlayerId::new(0), class::TOWER, FogVisibility::VISIBLE)
    );
    assert_eq!(
        (aim.aimed, aim.at, aim.aimed_class),
        (
            object_id(60),
            Position::new(20.5, 20.5, 0.0),
            class::BUILDING
        )
    );
    let aim = world.attack_aim(object_id(98));
    assert_eq!(
        aim.err().map(|error| error.reason()),
        Some("an attack task's target the slice does not hold")
    );
    world
        .parts
        .building_mut(world.players[0].objects[ListIndex::new(2)].id)
        .unwrap()
        .annex_of = Some(MainBuildingId::new(object_id(99)));
    let attack_aim = world.attack_aim(object_id(61));
    assert_eq!(
        attack_aim.err().map(|error| error.reason()),
        Some("an attack task's main building is gone")
    );
}

/// The attack task's word to its unit AI (`FUN_140c64b50`, `FUN_140c64c10`, `FUN_14106e4e0`
/// kind 9): the aimed main building becomes the soldier's last target, the annex its target
/// with its class at the work's point under order 700 and action 600; the last retarget time
/// and the order history take the world's time, and the annex counts the soldier as a worker.
/// A soldier without a unit AI only takes the target.
#[test]
fn an_attack_task_tells_its_unit_ai_the_order_the_action_and_the_target() {
    let (mut world, _) = work_attack(UnitStance::Aggressive);
    annexed(&mut world);
    world.world_time = WorldTime::from_millis(1730);
    let aim = world.attack_aim(object_id(61)).unwrap();
    world
        .attack_notifies(
            PlayerId::new(1),
            ListIndex::new(0),
            (object_id(61), &aim),
            Position::new(21.0, 21.0, 0.0),
            Notifies::Cleared,
        )
        .unwrap();
    let ai = world
        .parts
        .unit_ai
        .get(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap();
    assert_eq!(
        (ai.current_order, ai.current_action, ai.current_target),
        (
            Some(OrderKind::Attack),
            Some(AiAction::Attack),
            Some(object_id(61))
        )
    );
    assert_eq!(
        (ai.target_class, ai.target_point),
        (class_of(52), Position::new(21.0, 21.0, 0.0))
    );
    assert_eq!(
        (ai.last_target, ai.last_target_class, ai.last_retarget_time),
        (
            Some(object_id(60)),
            class_of(3),
            WorldTime::from_millis(1730)
        )
    );
    let record = ai.order_history.last().expect("the order history");
    assert_eq!(
        (record.order, record.action, record.time, record.target),
        (
            Some(OrderKind::Attack),
            Some(AiAction::Attack),
            WorldTime::from_millis(1730),
            Some(object_id(61))
        )
    );
    assert_eq!(
        world.players[0].objects[ListIndex::new(2)].workers,
        WorkerCount::of_data(1)
    );
    world
        .parts
        .set_unit_ai(world.players[1].objects[ListIndex::new(0)].id, None);
    world
        .attack_notifies(
            PlayerId::new(1),
            ListIndex::new(0),
            (object_id(61), &aim),
            Position::new(21.0, 21.0, 0.0),
            Notifies::Cleared,
        )
        .unwrap();
    assert_eq!(
        world.players[0].objects[ListIndex::new(2)].workers,
        WorkerCount::of_data(2)
    );
}

/// The attack task's Attack of a soldier without a unit AI (it cannot move) touching the wolf
/// goes ready at its start (`set_state(6)`, [`crate::combat::attack::Attack::command`]): with its reload
/// under way it waits ready on its standing sprite; with it done it swings at once on its
/// attack sprite.
#[test]
fn an_attack_task_of_unit_that_cannot_move_goes_ready_in_reach() {
    use crate::combat::attack::AttackState;
    for (timer, state, pending, sprite) in [
        (0.5, AttackState::Ready, false, 300),
        (0.0, AttackState::Swing, true, 301),
    ] {
        let (mut world, task) = work_attack(UnitStance::Aggressive);
        let unit = &mut world.players[1].objects[ListIndex::new(0)];
        let id = unit.id;
        world.parts.set_unit_ai(id, None);
        world.parts.combat_mut(id).unwrap().attack_timer = Seconds::new(timer);
        world
            .start_attack(
                PlayerId::new(1),
                ListIndex::new(0),
                object_id(50),
                &task,
                Position::new(10.4, 10.0, 0.0),
                Notifies::Cleared,
            )
            .unwrap();
        let unit = &world.players[1].objects[ListIndex::new(0)];
        let attack = world
            .parts
            .actions_of(unit.id)
            .list
            .attack()
            .copied()
            .expect("the soldier's Attack");
        assert_eq!(
            (attack.state, attack.pending, attack.target2),
            (state, pending, Some(AttackTargetId::new(object_id(50)))),
            "reload timer {timer}"
        );
        assert_eq!(
            unit.sprite.map(|active_sprite| active_sprite.graphic.id),
            Some(SpriteId::named(sprite))
        );
    }
}

/// Gaia's main building 60 (master 200, class 3) at (20.5, 20.5) and its annex 61 (master 201,
/// class 52) at (22, 20.5), both fog visible, after the wolf of [`work_attack`]'s world.
fn annexed(world: &mut World) {
    let wolf = world.model.masters[0]
        .get(master_id(126))
        .cloned()
        .expect("the wolf's master");
    world.model.masters[0] = Arc::new(Masters::new([
        wolf,
        Master {
            id: master_id(200),
            kind: MasterKind::BUILDING,
            class: class::BUILDING,
            fog_visibility: FogVisibility::VISIBLE,
            ..Master::default()
        },
        Master {
            id: master_id(201),
            kind: MasterKind::BUILDING,
            class: class::TOWER,
            fog_visibility: FogVisibility::VISIBLE,
            ..Master::default()
        },
    ]));
    let building = |id, master_id, position, annex_of| {
        Object::with_building(
            Unit {
                id,
                place: Static::at(position),
                life: Lifecycle {
                    master_id,
                    state: ObjectState::Alive,
                    ..Lifecycle::default()
                },
                ..Unit::default()
            },
            Building {
                annex_of,
                ..Default::default()
            },
        )
    };
    let _ = world
        .push_live(
            PlayerId::new(0),
            building(
                object_id(60),
                master_id(200),
                Position::new(20.5, 20.5, 0.0),
                None,
            ),
        )
        .unwrap();
    let _ = world
        .push_live(
            PlayerId::new(0),
            building(
                object_id(61),
                master_id(201),
                Position::new(22.0, 20.5, 0.0),
                Some(MainBuildingId::new(object_id(60))),
            ),
        )
        .unwrap();
}

/// [`soldiers`]' soldier 11 (master 4, melee, reload 2) at (10, 10) on its feet with a walker
/// of speed 1.2 and a unit AI of `stance`, touching Gaia's wolf 50 (master 126, class 10, 25
/// hit points) at (10.4, 10), and the attack task (action type 7, no sprites of its own).
pub(crate) fn work_attack(stance: UnitStance) -> (World, Task) {
    use crate::map::spatial::Placed;
    use crate::math::rng::Well512;
    use crate::objects::sprite::Graphic;
    let standing = Graphic {
        id: SpriteId::named(300),
        frames: Frame::of_data(10),
        facets: FacetCount::of_data(8),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::ANIMATED,
    };
    let mut world = soldiers(&[(11, 4, Point::new(10.0, 10.0))], &[]);
    world.model.graphics = Arc::new(Graphics::from([
        (SpriteId::named(300), standing),
        (
            SpriteId::named(301),
            Graphic {
                id: SpriteId::named(301),
                ..standing
            },
        ),
        (
            SpriteId::named(302),
            Graphic {
                id: SpriteId::named(302),
                speed_multiplier: SpeedMultiplier::of_data(1.5),
                ..standing
            },
        ),
    ]));
    let mut walker = walker();
    // the soldier's speed 0.8: its standing sprite's factor 1, its walking one's 1.5
    walker.stats.speed = Speed::new(1.2);
    let soldier = Arc::make_mut(&mut world.model.masters[1])
        .get_mut(master_id(4))
        .unwrap();
    soldier.standing_sprite = Some(SpriteId::named(300));
    soldier
        .combat
        .get_or_insert_with(Default::default)
        .attack_sprite = Some(SpriteId::named(301));
    soldier
        .moving
        .get_or_insert_with(Default::default)
        .move_sprite = Some(SpriteId::named(302));
    soldier
        .combat
        .get_or_insert_with(Default::default)
        .weapon_range = Distance::ZERO;
    soldier.combat.get_or_insert_with(Default::default).reload = Seconds::new(2.0);
    move_by(soldier, walker.stats);
    world.model.masters[0] = Arc::new(Masters::new([Master {
        id: master_id(126),
        kind: MasterKind::COMBAT,
        class: class::PREDATOR,
        ..Master::default()
    }]));
    let _ = world
        .push_live(
            PlayerId::new(0),
            Unit {
                id: object_id(50),
                place: Static::at(Position::new(10.4, 10.0, 0.0)),
                life: Lifecycle {
                    master_id: master_id(126),
                    state: ObjectState::Alive,
                    hp: HitPoints::new(25.0),
                    ..Lifecycle::default()
                },
                ..Unit::default()
            },
        )
        .unwrap();
    let placed = |x: f32, class, player, master| Placed {
        position: Position::new(x, 10.0, 0.0),
        radius: Footprint::new(0.2, 0.2),
        class,
        player,
        master,
        obstruction_kind: ObstructionKind::NONE,
        buildable_on: BuildableOn::NO,
        tall: true,
        moves: false,
        unknown: None,
        obstruction: None,
    };
    world.spatial = Spatial::new(GridSize::new(120, 120));
    world.spatial.insert(
        object_id(11),
        placed(10.0, class::CLASS_0, PlayerId::new(1), master_id(4)),
    );
    world.spatial.insert(
        object_id(50),
        placed(10.4, class::PREDATOR, PlayerId::new(0), master_id(126)),
    );
    let mut rng = Well512::default();
    let unit = &mut world.players[1].objects[ListIndex::new(0)];
    world.parts.put_mover(unit.id, Some(walker));
    unit.sprite = Some(ActiveSprite::start(
        Graphic {
            id: SpriteId::named(302),
            ..standing
        },
        &mut rng,
        tuning().sprite,
    ));
    world.parts.set_anim_speed(unit.id, Speed::new(1.0));
    world.parts.set_unit_ai(
        unit.id,
        Some(UnitAi {
            ai_kind: UnitAiKind::Ranged,
            current_order: None,
            current_action: None,
            current_target: None,
            stance,
            ..UnitAi::default()
        }),
    );
    let task = Task {
        action_kind: ATTACK_TASK,
        sprites: TaskSprites::NONE,
        ..Task::default()
    };
    (world, task)
}

/// A world of player 1's soldiers (master 4, formation category 3) and villagers (master 83,
/// category 0) at `units`' places, auto formations on with an influence of 3 tiles, and one
/// group of id 5 holding `members`.
fn soldiers(units: &[(i32, i16, Point)], members: &[i32]) -> World {
    let objects = units
        .iter()
        .map(|&(id, master, at)| Unit {
            id: object_id(id),
            place: Static::at(at.with_height(0.0)),
            group: GroupLink {
                id: if members.contains(&id) {
                    Some(group_id(5))
                } else {
                    None
                },
                flag: -2,
            },
            life: Lifecycle {
                master_id: master_id(master),
                state: ObjectState::Alive,
                ..Lifecycle::default()
            },
            ..Unit::default()
        })
        .collect();
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player::default(),
            Player {
                objects,
                formations: Formations {
                    auto: true,
                    influence: Distance::new(3.0),
                    by_speed: false,
                    ..Formations::default()
                },
                ..Player::default()
            },
        ],
    );
    // no sprites: a MoveTo the masters make shows none
    let master = |id, formation_category| Master {
        id,
        terrain_restriction: Some(RestrictionId::named(0)),
        kind: MasterKind::COMBAT,
        standing_sprite: None,
        creatable: Some(CreatablePart {
            formation_category,
            ..CreatablePart::default()
        }),
        moving: Some(MovingPart {
            move_sprite: None,
            ..MovingPart::default()
        }),
        ..Master::default()
    };
    world.model.masters = vec![
        Arc::new(Masters::new([])),
        Arc::new(Masters::new([
            master(master_id(4), FormationClass::of_data(3)),
            master(ids::VILLAGER_MALE, FormationClass::UNGROUPED),
        ])),
    ];
    if !members.is_empty() {
        world.groups.groups.push(UnitGroup {
            id: group_id(5),
            members: members
                .iter()
                .map(|&id| Member {
                    id: object_id(id),
                    ..Member::default()
                })
                .collect(),
            ..UnitGroup::default()
        });
    }
    world
}

/// `FUN_140fd76d0`: villagers move loose, soldiers within the influence distance of each
/// other form a group (the one with the most joiners first, by id), one too far moves loose;
/// with Ctrl held each soldier is a group of its own.
#[test]
fn the_splitter_groups_soldiers_near_each_other_and_leaves_villagers_loose() {
    let units = [
        (10, 83, Point::new(10.0, 10.0)),
        (11, 4, Point::new(20.0, 20.0)),
        (12, 4, Point::new(21.0, 20.0)),
        (13, 4, Point::new(23.5, 20.0)),
        (14, 4, Point::new(40.0, 20.0)),
    ];
    let mut world = soldiers(&units, &[]);
    let split = world
        .split(&object_ids([14, 13, 12, 11, 10]), Kept::All, ControlKey::Up)
        .unwrap();
    // 12 has two to join it (11 and 13), 11 and 13 one each
    assert_eq!(split.groups, [object_id_list([12, 11, 13])]);
    assert_eq!(split.loose, object_id_list([10, 14]));
    let split = world
        .split(
            &object_ids([14, 13, 12, 11, 10]),
            Kept::All,
            ControlKey::Held,
        )
        .unwrap();
    assert_eq!(split.loose, object_id_list([10]));
    assert_eq!(split.groups.len(), 4);
    // without auto formations every unit moves loose
    world.players[1].formations.auto = false;
    let split = world
        .split(&object_ids([14, 13, 12, 11, 10]), Kept::All, ControlKey::Up)
        .unwrap();
    assert_eq!(split.loose, object_id_list([10, 11, 12, 13, 14]));
    assert_eq!(split.groups, Vec::<Vec<ObjectId>>::new());
}

/// `FUN_140fd76d0`'s first pass: units in a group the world holds keep their group with the
/// order's other members, auto formations or not; a member ordered alone moves loose.
#[test]
fn the_splitter_keeps_the_order_members_of_an_existing_group_together() {
    let units = [
        (11, 4, Point::new(20.0, 20.0)),
        (12, 4, Point::new(60.0, 20.0)),
        (13, 4, Point::new(21.0, 20.0)),
    ];
    let mut world = soldiers(&units, &[11, 12]);
    world.players[1].formations.auto = false;
    let split = world
        .split(&object_ids([13, 12, 11]), Kept::All, ControlKey::Up)
        .unwrap();
    assert_eq!(split.groups, [object_id_list([11, 12])]);
    assert_eq!(split.loose, object_id_list([13]));
    let split = world
        .split(&object_ids([12, 13]), Kept::All, ControlKey::Up)
        .unwrap();
    assert_eq!(split.loose, object_id_list([12, 13]));
}

/// `FUN_140e55a30`: of the first 60 ids (500 for a computer) one the world lacks or another
/// player's (without the setting `+0x1074`) becomes -1; a human's command left without a unit
/// is refused.
#[test]
fn the_command_check_drops_gone_units_and_others_units_of_its_first_60() {
    let units = [
        (11, 4, Point::new(20.0, 20.0)),
        (12, 4, Point::new(21.0, 20.0)),
    ];
    let mut soldiers = soldiers(&units, &[]);
    let _ = soldiers
        .push_live(
            PlayerId::new(0),
            Unit {
                id: object_id(30),
                ..Unit::default()
            },
        )
        .unwrap();
    assert_eq!(
        soldiers
            .command_check(PlayerId::new(1), &object_ids([11, 30, 99, -1]))
            .unwrap(),
        Some(object_ids([11, -1, -1, -1]))
    );
    soldiers
        .settings
        .flags
        .set(OptionFlags::CHEATS_ENABLED, true);
    assert_eq!(
        soldiers
            .command_check(PlayerId::new(1), &object_ids([11, 30, 99]))
            .unwrap(),
        Some(object_ids([11, 30, -1]))
    );
    soldiers
        .settings
        .flags
        .set(OptionFlags::CHEATS_ENABLED, false);
    assert_eq!(
        soldiers
            .command_check(PlayerId::new(1), &object_ids([30, 99]))
            .unwrap(),
        None
    );
    soldiers.players[1].kind = PlayerKind::Computer;
    assert_eq!(
        soldiers
            .command_check(PlayerId::new(1), &object_ids([30, 99]))
            .unwrap(),
        Some(object_ids([-1, -1]))
    );
    soldiers.players[1].kind = PlayerKind::Human;
    // past the first 60 nothing is checked
    let mut ids = vec![99; 60];
    ids.push(30);
    let check = |world: &World, ids: &[i32]| {
        world.command_check(PlayerId::new(1), &object_ids(ids.iter().copied()))
    };
    assert_eq!(check(&soldiers, &ids).unwrap(), None);
    ids[0] = 11;
    let checked = check(&soldiers, &ids).unwrap().unwrap();
    assert_eq!(
        (checked[0], checked[1], checked[60]),
        (Some(object_id(11)), None, Some(object_id(30)))
    );
}

/// `FUN_140e6f620`: UNLOAD's check of its first 60 ids (500 for a computer), which alone go on.
/// Its own unit stays and one the world lacks becomes −1. Another player's stays when the two
/// count each other as allies (`FUN_1416642c0`) and the command names the issuer (flag 1) or
/// an object of the issuer's (flags 3 to 6); with the setting `+0x1074` any stays. A human's
/// command left without a unit is refused.
#[test]
fn the_unload_check_keeps_an_ally_object_the_command_ties_to_the_issuer() {
    let units = [
        (11, 4, Point::new(20.0, 20.0)),
        (12, 4, Point::new(21.0, 20.0)),
    ];
    let mut soldiers = soldiers(&units, &[]);
    soldiers.players.push(Player {
        objects: vec![Unit {
            id: object_id(30),
            ..Unit::default()
        }]
        .into(),
        ..Player::default()
    });
    soldiers.reindex();
    soldiers.players[1].relations = relation_row([3, 0, 4]);
    soldiers.players[2].relations = relation_row([3, 0, 0]);
    let check = |world: &World, flag, named| {
        world
            .unload_check(
                PlayerId::new(1),
                &object_ids([11, 30, 99]),
                UnloadPick::of_packet(flag, named),
            )
            .unwrap()
    };
    assert_eq!(check(&soldiers, 3, 12), Some(object_ids([11, 30, -1])));
    assert_eq!(check(&soldiers, 6, 12), Some(object_ids([11, 30, -1])));
    assert_eq!(check(&soldiers, 1, 1), Some(object_ids([11, 30, -1])));
    // the named object of another player's, the issuer not named, another flag: out
    assert_eq!(check(&soldiers, 3, 30), Some(object_ids([11, -1, -1])));
    assert_eq!(check(&soldiers, 1, 2), Some(object_ids([11, -1, -1])));
    assert_eq!(check(&soldiers, 0, 12), Some(object_ids([11, -1, -1])));
    assert_eq!(check(&soldiers, 7, 12), Some(object_ids([11, -1, -1])));
    // allied one way only: out, unless the setting lets any through
    soldiers.players[2].relations = relation_row([3, 3, 0]);
    assert_eq!(check(&soldiers, 3, 12), Some(object_ids([11, -1, -1])));
    soldiers
        .settings
        .flags
        .set(OptionFlags::CHEATS_ENABLED, true);
    assert_eq!(check(&soldiers, 0, 12), Some(object_ids([11, 30, -1])));
    soldiers
        .settings
        .flags
        .set(OptionFlags::CHEATS_ENABLED, false);
    assert_eq!(
        soldiers
            .unload_check(
                PlayerId::new(1),
                &object_ids([30, 99]),
                UnloadPick::of_packet(3, 12)
            )
            .unwrap(),
        None
    );
    soldiers.players[1].kind = PlayerKind::Computer;
    assert_eq!(
        soldiers
            .unload_check(
                PlayerId::new(1),
                &object_ids([30, 99]),
                UnloadPick::of_packet(3, 12)
            )
            .unwrap(),
        Some(object_ids([-1, -1]))
    );
    soldiers.players[1].kind = PlayerKind::Human;
    // past the first 60 nothing goes on
    let mut ids = vec![11; 60];
    ids.push(12);
    assert_eq!(
        soldiers
            .unload_check(
                PlayerId::new(1),
                &object_ids(ids),
                UnloadPick::of_packet(3, 12)
            )
            .unwrap()
            .map(|object_ids| object_ids.len()),
        Some(60)
    );
}

/// `FUN_140e6b750`: each id's object, live or sleeping, None for a gone one, in the ids' order.
#[test]
fn the_lookup_keeps_gone_unit_entry() {
    let units = [(11, 4, Point::new(20.0, 20.0))];
    let mut world = soldiers(&units, &[]);
    let _ = world
        .push_asleep(
            PlayerId::new(1),
            Unit {
                id: object_id(40),
                ..Unit::default()
            },
        )
        .unwrap();
    assert_eq!(
        world.lookup(&object_ids([99, 40, 11, -1])),
        [
            None,
            Some(Place::Sleeping(PlayerId::new(1), ListIndex::new(0))),
            Some(Place::Live(PlayerId::new(1), ListIndex::new(0))),
            None
        ]
    );
}

/// `FUN_14161e200`: the units of a STOP leave their unit group unless they are exactly its
/// members (a gone unit's entry counts in the list); each unit AI's order queue empties and
/// its idle timer takes its adjusted timeout; another player's unit the check dropped and a
/// sleeping object take nothing.
#[test]
fn stop_takes_its_units_out_of_their_group_and_empties_their_orders() {
    let units = [
        (11, 4, Point::new(20.0, 20.0)),
        (12, 4, Point::new(21.0, 20.0)),
    ];
    let mut world = soldiers(&units, &[11, 12]);
    let order = QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::Move,
        priority: OrderPriority::of_data(100),
        target: None,
        target_owner: None,
        point: Position::new(1.0, 1.0, 0.0),
        range: Distance::ZERO,
        shown: Point::new(1.0, 1.0),
        ready: true,
        target_kind: None,
    };
    world.parts.set_unit_ai(
        world.players[1].objects[ListIndex::new(0)].id,
        Some(UnitAi {
            orders: vec![order],
            idle_timer: Millis::new(5),
            adjusted_idle_timeout: Millis::new(300),
            ..UnitAi::default()
        }),
    );
    let ai = world
        .parts
        .unit_ai
        .get(world.players[1].objects[ListIndex::new(0)].id)
        .cloned()
        .unwrap();
    let thirty = Unit {
        id: object_id(30),
        ..Unit::default()
    };
    let _ = world
        .push_live(PlayerId::new(0), Object::with_ai(thirty, ai))
        .unwrap();
    let _ = world
        .push_asleep(
            PlayerId::new(1),
            Unit {
                id: object_id(40),
                ..Unit::default()
            },
        )
        .unwrap();
    world
        .stop_command(player_id(1), &object_ids([11, 12, 99, 30, 40]))
        .unwrap();
    let ai = world
        .parts
        .unit_ai
        .get(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap();
    assert_eq!((ai.orders.len(), ai.idle_timer), (0, Millis::new(300)));
    assert_eq!(world.players[1].objects[ListIndex::new(0)].group.id, None);
    assert_eq!(world.players[1].objects[ListIndex::new(1)].group.id, None);
    assert_eq!(world.groups.get(group_id(5)).unwrap().members.len(), 0);
    assert_eq!(
        world
            .parts
            .unit_ai
            .get(world.players[0].objects[ListIndex::new(0)].id)
            .unwrap()
            .orders
            .len(),
        1
    );
}

/// With the game setting `+0x1074` (`mCheatsEnabled`) the command's check passes another
/// player's unit, and MOVE and INTERACT (no owner test past it) order it as its own player's:
/// a queued order joins its unit AI's queue with the issuer as the order's.
#[test]
fn an_order_to_another_player_unit_goes_to_its_queue_with_the_setting() {
    let mut world = soldiers(&[(11, 83, Point::new(20.0, 20.0))], &[]);
    world.settings.flags.set(OptionFlags::CHEATS_ENABLED, true);
    world.players.push(Player::default());
    world.reindex();
    world.player_time.push(Millis::ZERO);
    world.model.masters.push(Arc::new(Masters::new([])));
    let walker = walker();
    if let Some(master) = Arc::make_mut(&mut world.model.masters[1]).get_mut(ids::VILLAGER_MALE) {
        move_by(master, walker.stats);
    }
    let unit = &mut world.players[1].objects[ListIndex::new(0)];
    world.parts.put_mover(unit.id, Some(walker));
    world.parts.set_unit_ai(unit.id, Some(UnitAi::default()));
    assert_eq!(
        world.checked_units(PlayerId::new(2), &object_ids([11])),
        object_ids([11])
    );
    let order = MoveOrder {
        target: None,
        point: Point::new(30.0, 30.0),
        flags: OrderFlags::QUEUED,
        control: ControlKey::Up,
    };
    world
        .move_order(PlayerId::new(2), &object_ids([11]), &order)
        .unwrap();
    let interact = Interact {
        target: None,
        point: Point::new(25.0, 30.0),
        disable_retargeting: false,
        flags: OrderFlags::QUEUED,
        filter: None,
    };
    world
        .interact(PlayerId::new(2), &object_ids([11]), &interact)
        .unwrap();
    let ai = world
        .parts
        .unit_ai
        .get(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap();
    let queued: Vec<(Option<Issuer>, OrderKind)> = ai
        .orders
        .iter()
        .map(|queued_order| (queued_order.issuer, queued_order.order))
        .collect();
    let player_2 = Some(Issuer::Player(player_id(2)));
    assert_eq!(
        queued,
        [(player_2, OrderKind::Move), (player_2, OrderKind::OnTarget)]
    );
}

/// MOVE (`FUN_140e67800`) reads the master's speed at slot 0xe8 (`+0x740`, the animated layer's),
/// not its movement's top speed: a master of speed 0.8 moves its unit where its movement reads 0,
/// and one of speed 0 leaves it standing where its movement reads 0.8.
#[test]
fn move_reads_the_master_speed_at_slot_0xe8() {
    let moved = |max_speed: Speed, top_speed: Speed| {
        let mut world = soldiers(&[(11, 4, Point::new(20.0, 20.0))], &[]);
        let walker = walker();
        for masters in &mut world.model.masters {
            if let Some(soldier) = Arc::make_mut(masters).get_mut(master_id(4)) {
                move_by(
                    soldier,
                    MoveStats {
                        speed: top_speed,
                        ..walker.stats
                    },
                );
                soldier.animated = Some(AnimatedPart { max_speed });
            }
        }
        let id = world.players[1].objects[ListIndex::new(0)].id;
        world.parts.put_mover(id, Some(walker));
        let order = MoveOrder {
            target: None,
            point: Point::new(30.0, 30.0),
            flags: OrderFlags::empty(),
            control: ControlKey::Up,
        };
        world
            .move_order(PlayerId::new(1), &object_ids([11]), &order)
            .unwrap();
        world.parts.actions_of(id).list.move_to().is_some()
    };
    assert_eq!(
        (
            moved(Speed::new(0.8), Speed::ZERO),
            moved(Speed::ZERO, Speed::new(0.8))
        ),
        (true, false)
    );
}

/// MOVE and INTERACT (`FUN_140e67800`, `FUN_140e68b90`): their loose units in a group the
/// world holds leave it (`FUN_14102c160`), one unit or several; a unit in no group, or in a
/// group the world does not hold, stays as it is.
#[test]
fn the_loose_units_of_move_or_an_interact_leave_their_groups() {
    let units = [
        (11, 83, Point::new(20.0, 20.0)),
        (12, 83, Point::new(40.0, 20.0)),
    ];
    let order = MoveOrder {
        target: None,
        point: Point::new(30.0, 30.0),
        flags: OrderFlags::empty(),
        control: ControlKey::Up,
    };
    let mut game = soldiers(&units, &[11]);
    game.move_order(PlayerId::new(1), &object_ids([11]), &order)
        .unwrap();
    assert_eq!(game.players[1].objects[ListIndex::new(0)].group.id, None);
    assert_eq!(game.groups.get(group_id(5)).unwrap().members.len(), 0);
    // two groups' members ordered alone: both loose
    let two = || {
        let mut world = soldiers(&units, &[11]);
        world.groups.groups.push(UnitGroup {
            id: group_id(6),
            members: vec![Member {
                id: object_id(12),
                ..Member::default()
            }],
            ..UnitGroup::default()
        });
        world.players[1].objects[ListIndex::new(1)].group.id = Some(group_id(6));
        world
    };
    let mut world = two();
    world
        .move_order(PlayerId::new(1), &object_ids([11, 12]), &order)
        .unwrap();
    assert_eq!(world.players[1].objects[ListIndex::new(0)].group.id, None);
    assert_eq!(world.players[1].objects[ListIndex::new(1)].group.id, None);
    assert_eq!(world.groups.get(group_id(5)).unwrap().members.len(), 0);
    assert_eq!(world.groups.get(group_id(6)).unwrap().members.len(), 0);
    // a group link the world does not hold
    let mut soldiers = soldiers(&units, &[]);
    soldiers.players[1].objects[ListIndex::new(0)].group.id = Some(group_id(9));
    soldiers
        .move_order(PlayerId::new(1), &object_ids([11]), &order)
        .unwrap();
    assert_eq!(
        soldiers.players[1].objects[ListIndex::new(0)].group.id,
        Some(group_id(9))
    );
    let interact = Interact {
        target: None,
        point: Point::new(30.0, 30.0),
        disable_retargeting: false,
        flags: OrderFlags::empty(),
        filter: None,
    };
    let mut world = two();
    for unit in &mut world.players[1].objects {
        unit.pasture = None;
        unit.my_pasture = None;
    }
    world
        .interact(PlayerId::new(1), &object_ids([11, 12]), &interact)
        .unwrap();
    assert_eq!(world.players[1].objects[ListIndex::new(0)].group.id, None);
    assert_eq!(world.players[1].objects[ListIndex::new(1)].group.id, None);
}

/// A queued MOVE's new group (`FUN_140e67800` at `0x140e67b5d`): the first of its units the
/// world holds (99 is not), with a unit AI, names the target the group's first command takes
/// when the world holds it, at its place; a unit without a unit AI or a target the world
/// does not hold names none.
#[test]
fn queued_move_new_group_takes_its_first_unit_target() {
    let units = [
        (11, 4, Point::new(20.0, 20.0)),
        (13, 4, Point::new(21.0, 20.0)),
    ];
    let ai = |target| UnitAi {
        current_order: None,
        current_action: None,
        current_target: target,
        ..UnitAi::default()
    };
    let mut world = soldiers(&units, &[]);
    world.parts.set_unit_ai(
        world.players[1].objects[ListIndex::new(0)].id,
        Some(ai(Some(object_id(13)))),
    );
    world.parts.set_unit_ai(
        world.players[1].objects[ListIndex::new(1)].id,
        Some(ai(Some(object_id(11)))),
    );
    assert_eq!(
        world.first_unit_target(&object_id_list([99, 11, 13])),
        Some((object_id(13), Point::new(21.0, 20.0), true))
    );
    assert_eq!(
        world.first_unit_target(&object_id_list([13, 11])),
        Some((object_id(11), Point::new(20.0, 20.0), true))
    );
    world.parts.set_unit_ai(
        world.players[1].objects[ListIndex::new(0)].id,
        Some(ai(Some(object_id(98)))),
    );
    assert_eq!(world.first_unit_target(&object_id_list([11, 13])), None);
    world
        .parts
        .set_unit_ai(world.players[1].objects[ListIndex::new(0)].id, None);
    assert_eq!(world.first_unit_target(&object_id_list([11, 13])), None);
    assert_eq!(world.first_unit_target(&object_id_list([99])), None);
}

/// GATE and `FUN_1411dba30`: player 1's building 50 toggles its lock, a lock setting its last
/// move time to the world time plus 1, an unlock leaving it; player 2's GATE of it does
/// nothing unless players may order each other's units; a unit (type 70) takes nothing.
#[test]
fn gate_command_toggles_the_lock_of_building_of_its_player() {
    let units = [(11, 4, Point::new(20.0, 20.0))];
    let mut soldiers = soldiers(&units, &[]);
    soldiers.world_time = WorldTime::from_millis(400);
    soldiers.players.push(Player::default());
    soldiers.reindex();
    soldiers.model.masters.push(Arc::new(Masters::new([])));
    soldiers.model.masters[1] = Arc::new(Masters::new([
        Master {
            id: master_id(4),
            kind: MasterKind::COMBAT,
            ..Master::default()
        },
        Master {
            id: master_id(79),
            kind: MasterKind::BUILDING,
            ..Master::default()
        },
    ]));
    let _ = soldiers
        .push_live(
            PlayerId::new(1),
            Object::with_building(
                Unit {
                    id: object_id(50),
                    life: Lifecycle {
                        master_id: master_id(79),
                        state: ObjectState::Alive,
                        ..Lifecycle::default()
                    },
                    ..Unit::default()
                },
                Building {
                    gate: Some(Gate::default()),
                    ..Default::default()
                },
            ),
        )
        .unwrap();
    soldiers.set_motion(object_id(50), Some(Motion::Moving(walker())));
    let building = |world: &World| {
        let unit = &world.players[1].objects[ListIndex::new(1)];
        (
            world.parts.building_of(unit.id).gate.unwrap().locked,
            world.parts.mover_of(unit.id).unwrap().last_move,
        )
    };
    soldiers.gate(player_id(1), object_id(50)).unwrap();
    assert_eq!(building(&soldiers), (true, WorldTime::from_millis(401)));
    soldiers.world_time = WorldTime::from_millis(500);
    soldiers.gate(player_id(2), object_id(50)).unwrap();
    assert_eq!(building(&soldiers), (true, WorldTime::from_millis(401)));
    soldiers
        .settings
        .flags
        .set(OptionFlags::CHEATS_ENABLED, true);
    soldiers.gate(player_id(2), object_id(50)).unwrap();
    assert_eq!(building(&soldiers), (false, WorldTime::from_millis(401)));
    soldiers.gate(player_id(1), object_id(11)).unwrap();
    soldiers.gate(player_id(1), object_id(99)).unwrap();
    assert_eq!(
        soldiers
            .parts
            .building_of(soldiers.players[1].objects[ListIndex::new(0)].id)
            .gate,
        None
    );
}

/// `TRANSFORM_OBJECT` and `FUN_1411e6d90`: player 1's trophy 60 (master 1649) takes trophy
/// 1650, keeping its share of hit points, and in state 2 its build points take the new
/// master's build time; in state 1 they stay.
#[test]
fn transformed_building_takes_its_new_master_build_time_in_state_2() {
    let units = [(11, 4, Point::new(20.0, 20.0))];
    let mut world = soldiers(&units, &[]);
    let master = |id, build_time| Master {
        id,
        kind: MasterKind::BUILDING,
        hit_points: MasterHitPoints::of_data(100),
        creatable: Some(CreatablePart {
            build_time,
            volley_fire: Firepower::new(f32::from(build_time.get() / 15)),
            ..CreatablePart::default()
        }),
        ..Master::default()
    };
    world.model.masters[1] = Arc::new(Masters::new([
        master(ids::TROPHY_NONE, WholeSeconds::of_data(10)),
        master(master_id(1650), WholeSeconds::of_data(30)),
    ]));
    let trophy = |state| Object {
        unit: Unit {
            id: object_id(60),
            life: Lifecycle {
                master_id: ids::TROPHY_NONE,
                state,
                hp: HitPoints::new(50.0),
                ..Lifecycle::default()
            },
            ..Unit::default()
        },
        parts: ObjectParts {
            combat: Combat {
                firepower: Firepower::new(7.0),
                ..Default::default()
            },
            building: Building {
                construction: Some(Construction {
                    progress: Seconds::new(10.0),
                    desolid: false,
                    builders: BuilderCount::of_data(0),
                    hit_points: MasterHitPoints::of_data(100),
                    build_time: WholeSeconds::of_data(10),
                    record: Obstruction {
                        kind: ObstructionKind::KIND_2,
                        class: ObstructionClass::UNIT,
                    },
                    snow: Snow::None,
                    sprite_override: false,
                }),
                ..Default::default()
            },
            ..Default::default()
        },
    };
    world.set_live_for_test(player_id(1), [trophy(ObjectState::Alive)]);
    world
        .transform_object(player_id(1), object_id(60), master_id(1650))
        .unwrap();
    let unit = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(unit.life.master_id, master_id(1650));
    assert_eq!(unit.life.hp, HitPoints::new(50.0));
    assert_eq!(
        world
            .parts
            .building_of(unit.id)
            .construction
            .unwrap()
            .progress,
        Seconds::new(30.0)
    );
    // FUN_141225b70 in state 2: the volleys are the new master's volley fire
    assert_eq!(
        world.parts.combat_of(unit.id).firepower,
        Firepower::new(2.0)
    );
    world.set_live_for_test(player_id(1), [trophy(ObjectState::Unfinished)]);
    world
        .transform_object(player_id(1), object_id(60), master_id(1650))
        .unwrap();
    assert_eq!(
        world
            .parts
            .building_of(world.players[1].objects[ListIndex::new(0)].id)
            .construction
            .unwrap()
            .progress,
        Seconds::new(10.0)
    );
    assert_eq!(
        world
            .parts
            .combat_of(world.players[1].objects[ListIndex::new(0)].id)
            .firepower,
        Firepower::new(7.0)
    );
}

/// INTERACT to an object that does not move (`FUN_140e68b90`): it takes the order at once
/// (slot 0x128); a queued order then goes on as a mover's and its unit AI queues it (order
/// 0x2db at the point), with no movement state to touch.
#[test]
fn queued_interact_to_an_object_that_does_not_move_also_queues_it() {
    let interact = |queued| Interact {
        target: None,
        point: Point::new(25.0, 30.0),
        disable_retargeting: false,
        flags: if queued {
            OrderFlags::QUEUED
        } else {
            OrderFlags::empty()
        },
        filter: None,
    };
    let run = |queued| {
        let mut world = soldiers(&[(11, 83, Point::new(20.0, 20.0))], &[]);
        if let Some(master) = Arc::make_mut(&mut world.model.masters[1]).get_mut(ids::VILLAGER_MALE)
        {
            master.kind = MasterKind::BUILDING;
        }
        let unit = &mut world.players[1].objects[ListIndex::new(0)];
        (unit.pasture, unit.my_pasture) = (None, None);
        world.parts.set_unit_ai(unit.id, Some(UnitAi::default()));
        world
            .interact(PlayerId::new(1), &object_ids([11]), &interact(queued))
            .unwrap();
        let ai = world
            .parts
            .unit_ai
            .get(world.players[1].objects[ListIndex::new(0)].id)
            .cloned()
            .unwrap();
        ai.orders
            .iter()
            .map(|queued_order| queued_order.order)
            .collect::<Vec<_>>()
    };
    assert_eq!(run(false), Vec::<OrderKind>::new());
    assert_eq!(run(true), [OrderKind::OnTarget]);
}

/// INTERACT neither queued nor direct to a unit AI holding an order not ready
/// (`FUN_140e68b90` at 0x140e6983a): the order goes into its queue not ready, the readiness
/// pass with 1 makes both ready, and its tail stops the unit (the object's slot 0x148,
/// `FUN_14108c050`, 0x140c42cd4): the AI's order ends and it forgets its targets.
#[test]
fn an_order_to_busy_unit_ai_readies_its_queue_and_stops_the_unit() {
    let mut world = soldiers(&[(11, 83, Point::new(20.0, 20.0))], &[]);
    let walker = walker();
    if let Some(master) = Arc::make_mut(&mut world.model.masters[1]).get_mut(ids::VILLAGER_MALE) {
        move_by(master, walker.stats);
    }
    let unit = &mut world.players[1].objects[ListIndex::new(0)];
    world.parts.put_mover(unit.id, Some(walker));
    (unit.pasture, unit.my_pasture) = (None, None);
    world.parts.set_unit_ai(
        unit.id,
        Some(UnitAi {
            current_order: Some(OrderKind::Move),
            current_action: Some(AiAction::Move),
            current_target: Some(object_id(99)),
            orders: vec![QueuedOrder {
                order: OrderKind::Move,
                ready: false,
                ..QueuedOrder::default()
            }],
            ..UnitAi::default()
        }),
    );
    let interact = Interact {
        target: None,
        point: Point::new(25.0, 30.0),
        disable_retargeting: false,
        flags: OrderFlags::empty(),
        filter: None,
    };
    world
        .interact(PlayerId::new(1), &object_ids([11]), &interact)
        .unwrap();
    let ai = world
        .parts
        .unit_ai
        .get(world.players[1].objects[ListIndex::new(0)].id)
        .cloned()
        .unwrap();
    let queue: Vec<_> = ai
        .orders
        .iter()
        .map(|queued_order| (queued_order.order, queued_order.ready))
        .collect();
    assert_eq!(
        queue,
        [(OrderKind::Move, true), (OrderKind::OnTarget, true)]
    );
    assert_eq!((ai.current_order, ai.current_target), (None, None));
}

/// One unit of INTERACT (`FUN_140e68b90`): a unit the world does not hold takes nothing; a
/// queued order's unit AI queues it at the unit's offset from the centroid added to the order's
/// point, the order's own point shown, its ignore list the order's other units; a unit the order
/// targets itself takes it without a work command. A sleeping unit and a mover without movement
/// state are not modelled.
#[test]
fn an_interact_unit_queues_its_offset_point_and_takes_no_work_on_itself() {
    let interact = Interact {
        target: None,
        point: Point::new(25.0, 30.0),
        disable_retargeting: false,
        flags: OrderFlags::QUEUED,
        filter: None,
    };
    let mover = || {
        let mut soldiers = soldiers(&[(11, 83, Point::new(20.0, 20.0))], &[]);
        let walker = walker();
        if let Some(master) =
            Arc::make_mut(&mut soldiers.model.masters[1]).get_mut(ids::VILLAGER_MALE)
        {
            move_by(master, walker.stats);
        }
        let unit = &mut soldiers.players[1].objects[ListIndex::new(0)];
        soldiers.parts.put_mover(unit.id, Some(walker));
        (unit.pasture, unit.my_pasture) = (None, None);
        soldiers.parts.set_unit_ai(unit.id, Some(UnitAi::default()));
        soldiers.spatial.map = GridSize::new(100, 100);
        soldiers
    };
    let mut world = mover();
    world
        .interact_unit(PlayerId::new(1), object_id(99), &[], None, &interact)
        .unwrap();
    assert_eq!(world, mover());
    world
        .interact_unit(
            PlayerId::new(1),
            object_id(11),
            &[object_id(12)],
            Some(Point::new(21.0, 20.0)),
            &interact,
        )
        .unwrap();
    let unit = &world.players[1].objects[ListIndex::new(0)];
    let moved = world.parts.mover_of(unit.id).unwrap();
    assert_eq!(
        (moved.waiting, &moved.ignore_units[..]),
        (1, &[object_id(12)][..])
    );
    let queued: Vec<_> = world
        .parts
        .unit_ai
        .get(unit.id)
        .unwrap()
        .orders
        .iter()
        .map(|queued_order| {
            (
                queued_order.order,
                queued_order.point,
                queued_order.shown,
                queued_order.ready,
            )
        })
        .collect();
    assert_eq!(
        queued,
        [(
            OrderKind::OnTarget,
            Position::new(24.0, 30.0, 0.0),
            Point::new(25.0, 30.0),
            false
        )]
    );
    let mut game = mover();
    let ai = game
        .parts
        .unit_ai
        .get_mut(game.players[1].objects[ListIndex::new(0)].id)
        .unwrap();
    ai.orders.push(QueuedOrder::default());
    let itself = Interact {
        target: Some(object_id(11)),
        flags: OrderFlags::DIRECT,
        ..interact
    };
    game.interact_unit(PlayerId::new(1), object_id(11), &[], None, &itself)
        .unwrap();
    let players = &game.players[1].objects[ListIndex::new(0)];
    assert_eq!(game.parts.unit_ai.get(players.id).unwrap().orders, []);
    assert_eq!(
        game.parts.actions_of(players.id).list.move_to().copied(),
        None
    );
    let mut game = mover();
    game.parts
        .put_mover(game.players[1].objects[ListIndex::new(0)].id, None);
    let reason = |world: &mut World| {
        world
            .interact_unit(PlayerId::new(1), object_id(11), &[], None, &interact)
            .unwrap_err()
            .reason()
    };
    assert_eq!(reason(&mut game), "an ordered unit without movement state");
    let mut world = mover();
    let object = world
        .remove_live(PlayerId::new(1), ListIndex::new(0))
        .unwrap();
    let _ = world.push_asleep(PlayerId::new(1), object).unwrap();
    assert_eq!(
        reason(&mut world),
        "an order to a unit the slice does not hold live"
    );
}

/// `FUN_140c42b60` with 1: from the last order to the first, an order not ready becomes
/// ready and a ready one leaves the queue, the others keeping their order; with 0 the ready
/// ones stay.
#[test]
fn the_readiness_pass_readies_the_unready_and_drops_the_ready() {
    // each order told apart by its issuer
    let order = |issuer, ready| QueuedOrder {
        issuer,
        ready,
        ..QueuedOrder::default()
    };
    let mut orders = vec![
        order(Some(Issuer::Player(player_id(1))), true),
        order(Some(Issuer::Player(player_id(2))), false),
        order(Some(Issuer::Player(player_id(3))), true),
        order(Some(Issuer::Player(player_id(4))), false),
    ];
    let mut kept = orders.clone();
    readiness(&mut orders, ReadyOrders::Drop);
    assert_eq!(
        orders,
        [
            order(Some(Issuer::Player(player_id(2))), true),
            order(Some(Issuer::Player(player_id(4))), true)
        ]
    );
    let mut none: Vec<QueuedOrder> = Vec::new();
    readiness(&mut none, ReadyOrders::Drop);
    assert_eq!(none.len(), 0);
    // with 0 (BUILD's) the ready ones stay and every order is ready
    readiness(&mut kept, ReadyOrders::Keep);
    assert_eq!(
        kept,
        [
            order(Some(Issuer::Player(player_id(1))), true),
            order(Some(Issuer::Player(player_id(2))), true),
            order(Some(Issuer::Player(player_id(3))), true),
            order(Some(Issuer::Player(player_id(4))), true)
        ]
    );
}

/// `FUN_14106ff70`'s retargeting byte set: a gather or hunt task of another resource than
/// the one held leaves the unit holding none of the task's (the food kinds 0 and 15 to 17
/// are one); an attack task (7 or 9) turns a unit's retargeting off unless it is a villager.
#[test]
fn the_carried_resource_test_drops_another_resource_and_stops_soldier_retargeting() {
    let task = |action_kind, attribute_kind| Task {
        action_kind,
        attribute_kind,
        ..Task::default()
    };
    let run = |master: MasterId, held: Held, task: Task| {
        let mut world = soldiers(&[(11, master.get(), Point::new(20.0, 20.0))], &[]);
        for (id, class) in [(83, class::VILLAGER), (4, class::CLASS_6)] {
            if let Some(master) = Arc::make_mut(&mut world.model.masters[1]).get_mut(master_id(id))
            {
                master.class = class;
            }
        }
        let unit = &mut world.players[1].objects[ListIndex::new(0)];
        (unit.life.attribute_kind, unit.life.amount) = (held.kind, held.amount);
        world.parts.set_unit_ai(unit.id, Some(UnitAi::default()));
        world
            .carried_resource_test(PlayerId::new(1), ListIndex::new(0), &task)
            .unwrap();
        let unit = &world.players[1].objects[ListIndex::new(0)];
        (
            unit.life.attribute_kind,
            unit.life.amount,
            world.parts.unit_ai.get(unit.id).unwrap().retarget_disabled,
        )
    };
    // wood held, a gather of gold: none held, of gold
    assert_eq!(
        run(
            ids::VILLAGER_MALE,
            carrying(Some(attribute::WOOD), 10.0),
            task(GATHER, Some(attribute::GOLD))
        ),
        (Some(attribute::GOLD), Amount::ZERO, 0)
    );
    // meat (17) held, a hunt of food (0): both food, kept
    assert_eq!(
        run(
            ids::VILLAGER_MALE,
            carrying(Some(AttributeId::named(17)), 10.0),
            task(HUNT, Some(attribute::FOOD))
        ),
        (Some(AttributeId::named(17)), Amount::new(10.0), 0)
    );
    // the same resource: kept
    assert_eq!(
        run(
            ids::VILLAGER_MALE,
            carrying(Some(attribute::GOLD), 4.0),
            task(GATHER, Some(attribute::GOLD))
        ),
        (Some(attribute::GOLD), Amount::new(4.0), 0)
    );
    // an attack task: a soldier's retargeting goes off, a villager's stays on
    assert_eq!(
        run(master_id(4), carrying(None, 0.0), task(ATTACK_TASK, None)),
        (None, Amount::ZERO, 1)
    );
    assert_eq!(
        run(
            ids::VILLAGER_MALE,
            carrying(None, 0.0),
            task(ATTACK_TASK, None)
        ),
        (None, Amount::ZERO, 0)
    );
}

/// `FUN_141216f10` to a point: the unit AI's order queue empties only with the unit's
/// `+0xa14`, which slot 0x128 with 0 clears while its work command runs; the move follows
/// either way.
#[test]
fn move_inside_work_command_with_0_keeps_the_order_queue() {
    let units = [(11, 4, Point::new(20.0, 20.0))];
    let mut world = soldiers(&units, &[]);
    let order = QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::Move,
        priority: OrderPriority::of_data(100),
        target: None,
        target_owner: None,
        point: Position::new(1.0, 1.0, 0.0),
        range: Distance::ZERO,
        shown: Point::new(1.0, 1.0),
        ready: true,
        target_kind: None,
    };
    world.parts.set_unit_ai(
        world.players[1].objects[ListIndex::new(0)].id,
        Some(UnitAi {
            orders: vec![order],
            ..UnitAi::default()
        }),
    );
    let unit_id = world.players[1].objects[ListIndex::new(0)].id;
    world.parts.combat_mut(unit_id).unwrap().order_source = OrderSource::Queued;
    let _ = world
        .move_point_order(
            PlayerId::new(1),
            ListIndex::new(0),
            Position::new(30.0, 30.0, 0.0),
        )
        .unwrap();
    world.parts.combat_mut(unit_id).unwrap().order_source = OrderSource::Direct;
    let unit = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(world.parts.unit_ai.get(unit.id).unwrap().orders.len(), 1);
    assert_eq!(
        world
            .parts
            .actions_of(unit.id)
            .list
            .move_to()
            .copied()
            .unwrap()
            .goal(),
        Some(Position::new(30.0, 30.0, 0.0))
    );
    let _ = world
        .move_point_order(
            PlayerId::new(1),
            ListIndex::new(0),
            Position::new(31.0, 30.0, 0.0),
        )
        .unwrap();
    assert_eq!(
        world
            .parts
            .unit_ai
            .get(world.players[1].objects[ListIndex::new(0)].id)
            .unwrap()
            .orders
            .len(),
        0
    );
}

/// A soldier of [`soldiers`] at (20, 20) that walks ([`walker`]) and whose unit AI holds one
/// queued move order.
fn soldier_with_queued_order() -> World {
    let mut world = soldiers(&[(11, 4, Point::new(20.0, 20.0))], &[]);
    let walker = walker();
    for masters in &mut world.model.masters {
        if let Some(soldier) = Arc::make_mut(masters).get_mut(master_id(4)) {
            move_by(soldier, walker.stats);
        }
    }
    world
        .parts
        .put_mover(world.players[1].objects[ListIndex::new(0)].id, Some(walker));
    let order = QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::Move,
        priority: OrderPriority::of_data(100),
        target: None,
        target_owner: None,
        point: Position::new(1.0, 1.0, 0.0),
        range: Distance::ZERO,
        shown: Point::new(1.0, 1.0),
        ready: true,
        target_kind: None,
    };
    world.parts.set_unit_ai(
        world.players[1].objects[ListIndex::new(0)].id,
        Some(UnitAi {
            orders: vec![order],
            ..UnitAi::default()
        }),
    );
    world
}

/// The soldier's queued orders, `+0xa14` and command count (`+0x888`).
fn queue_source_count(world: &World) -> (usize, OrderSource, u8) {
    let id = world.players[1].objects[ListIndex::new(0)].id;
    (
        world.parts.unit_ai.get(id).map_or(0, |ai| ai.orders.len()),
        world.parts.combat_of(id).order_source,
        world.parts.actions_of(id).commands.get(),
    )
}

/// Slot 0x128 (`FUN_141211b80`): `+0xa14` holds the source while the work runs and is 1 after;
/// with 0 the unit AI's queue stays through the work's move (slot 0x138 reads `+0xa14`), with 1
/// it empties. A work at a point moves the unit: `FUN_14106d340` counts the move and
/// `FUN_14106c590` the work, two commands; on the unit itself the queue still empties but
/// nothing is counted.
#[test]
fn slot_0x128_holds_its_source_while_it_works_and_counts_the_commands() {
    let mut world = soldier_with_queued_order();
    let id = world.players[1].objects[ListIndex::new(0)].id;
    let point = Position::new(30.0, 30.0, 0.0);
    world
        .give_work(id, &Work::on(None, point), OrderSource::Queued)
        .unwrap();
    assert_eq!(queue_source_count(&world), (1, OrderSource::Direct, 2));
    world
        .give_work(id, &Work::on(None, point), OrderSource::Direct)
        .unwrap();
    assert_eq!(queue_source_count(&world), (0, OrderSource::Direct, 4));
    let mut world = soldier_with_queued_order();
    world
        .give_work(id, &Work::on(Some(id), point), OrderSource::Direct)
        .unwrap();
    assert_eq!(queue_source_count(&world), (0, OrderSource::Direct, 0));
}

/// `FUN_1412081e0` sets `+0xa14` 0 first and 1 again only where the unit went back: a unit
/// outside no longer under the town bell stays, so its next move (slot 0x138) keeps the unit
/// AI's queue; the next slot 0x128 sets 1 again.
#[test]
fn failed_way_back_to_work_leaves_the_order_source_queued() {
    let mut world = soldier_with_queued_order();
    let id = world.players[1].objects[ListIndex::new(0)].id;
    assert_eq!(world.back_to_work(None, id).unwrap(), WorkReturn::Stayed);
    assert_eq!(queue_source_count(&world), (1, OrderSource::Queued, 0));
    let _ = world
        .move_point_order(
            PlayerId::new(1),
            ListIndex::new(0),
            Position::new(30.0, 30.0, 0.0),
        )
        .unwrap();
    assert_eq!(queue_source_count(&world), (1, OrderSource::Queued, 1));
    world
        .give_work(
            id,
            &Work::on(None, Position::new(31.0, 30.0, 0.0)),
            OrderSource::Direct,
        )
        .unwrap();
    assert_eq!(queue_source_count(&world), (0, OrderSource::Direct, 3));
}

/// Slot 0x140 (`FUN_141070c90`): the work with the byte 0 and any task at `+0xa14` as it
/// stands, then one command more; on the unit itself nothing.
#[test]
fn slot_0x140_works_and_counts_at_the_order_source_as_it_stands() {
    let mut world = soldier_with_queued_order();
    let id = world.players[1].objects[ListIndex::new(0)].id;
    world.parts.combat_mut(id).unwrap().order_source = OrderSource::Queued;
    world
        .work_at(
            PlayerId::new(1),
            ListIndex::new(0),
            None,
            Position::new(30.0, 30.0, 0.0),
        )
        .unwrap();
    assert_eq!(queue_source_count(&world), (1, OrderSource::Queued, 2));
    world
        .work_at(
            PlayerId::new(1),
            ListIndex::new(0),
            Some(id),
            Position::ORIGIN,
        )
        .unwrap();
    assert_eq!(queue_source_count(&world), (1, OrderSource::Queued, 2));
}

/// An action object (types 40 to 60) has no `+0xa14`: its slot 0x128 (`FUN_14106c590`) and its
/// slot 0x138 (`FUN_14106d340`) leave its unit AI's order queue alone, and its notifies always
/// clear; the work and the move are counted as a combat object's.
#[test]
fn an_action_object_works_and_moves_without_emptying_its_order_queue() {
    let mut world = soldier_with_queued_order();
    let master = world.players[1].objects[ListIndex::new(0)].life.master_id;
    for masters in &mut world.model.masters {
        if let Some(own) = Arc::make_mut(masters).get_mut(master) {
            own.kind = MasterKind::ACTION;
        }
    }
    let id = world.players[1].objects[ListIndex::new(0)].id;
    world
        .give_work(
            id,
            &Work::on(None, Position::new(30.0, 30.0, 0.0)),
            OrderSource::Direct,
        )
        .unwrap();
    assert_eq!(queue_source_count(&world), (1, OrderSource::Direct, 2));
    let _ = world
        .move_point_order(
            PlayerId::new(1),
            ListIndex::new(0),
            Position::new(31.0, 30.0, 0.0),
        )
        .unwrap();
    assert_eq!(queue_source_count(&world), (1, OrderSource::Direct, 3));
}

/// The town-bell record (`FUN_141206a20`) takes its top action's target by slot 400
/// (`FUN_140a78450`): the berry bush 600 the villager walks to is its target; the recyclable
/// −600 (a copy the world holds) is none, and no other source names one.
#[test]
fn town_bell_record_skips_recyclable_top_target() {
    let run = |target: i32| {
        let mut world = forage_world();
        let bush = world.players[0]
            .objects
            .iter()
            .find(|unit| unit.id == object_id(600))
            .unwrap();
        let mut copy = bush.clone();
        copy.id = object_id(-600);
        world.players[0].objects.push(copy);
        world.reindex();
        world.parts.put_actions(
            object_id(9175),
            Action {
                list: ActionList::default().with_move_to(Some(MoveToAction {
                    target: Some(object_id(target)),
                    ..MoveToAction::sub_in(MoveTo::Moving)
                })),
                ..Action::default()
            },
        );
        world
            .record_town_bell(PlayerId::new(1), ListIndex::new(0))
            .unwrap();
        world.parts.town_bell_of(object_id(9175)).work.target
    };
    assert_eq!(run(600), Some(object_id(600)));
    assert_eq!(run(-600), None);
}

/// Slot 0x138's start (`FUN_141216f10`): a villager is no longer under the town bell.
#[test]
fn villager_move_drops_its_town_bell_flag() {
    let mut world = soldiers(&[(11, 4, Point::new(20.0, 20.0))], &[]);
    let villager = world.players[1].objects[ListIndex::new(0)].id;
    world.players[1].objects[ListIndex::new(0)].life.master_id = ids::VILLAGER_MALE;
    for masters in &mut world.model.masters {
        if let Some(own) = Arc::make_mut(masters).get_mut(ids::VILLAGER_MALE) {
            own.class = class::VILLAGER;
        }
    }
    world
        .parts
        .set_town_bell_flag(villager, BellShelter::Sheltered);
    let _ = world
        .move_point_order(
            PlayerId::new(1),
            ListIndex::new(0),
            Position::new(30.0, 30.0, 0.0),
        )
        .unwrap();
    assert_eq!(world.parts.town_bell_of(villager).flag, BellShelter::Free);
}

/// `FUN_140e666f0`: another player's scout takes the order as its owner's, a unit without
/// bit 4 nothing; of a human's ids only the first 60 count.
#[test]
fn an_auto_scout_reaches_any_player_scout_of_its_first_60_ids() {
    let units = [(11, 4, Point::new(20.0, 20.0))];
    let mut soldiers = soldiers(&units, &[]);
    soldiers.players.push(Player::default());
    soldiers.reindex();
    let scout = Master {
        id: ids::ID_448,
        kind: MasterKind::COMBAT,
        class: class::CLASS_12,
        object_flags: ObjectFlags::SCOUT,
        standing_sprite: None,
        moving: Some(MovingPart {
            // no sprites: the Explore's MoveTo takes none (the world's sprites' table holds none)
            move_sprite: None,
            ..MovingPart::default()
        }),
        ..Master::default()
    };
    soldiers
        .model
        .masters
        .push(Arc::new(Masters::new([scout.clone()])));
    soldiers.model.masters[1] = Arc::new(Masters::new([
        scout,
        Master {
            id: master_id(4),
            kind: MasterKind::COMBAT,
            ..Master::default()
        },
    ]));
    let ai = UnitAi {
        current_order: None,
        current_action: None,
        ..UnitAi::default()
    };
    let unit = |id| Unit {
        id,
        place: Static::at(Position::new(20.0, 20.0, 0.0)),
        life: Lifecycle {
            master_id: ids::ID_448,
            state: ObjectState::Alive,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    soldiers.parts.set_unit_ai(
        soldiers.players[1].objects[ListIndex::new(0)].id,
        Some(ai.clone()),
    );
    let _ = soldiers
        .push_live(
            PlayerId::new(1),
            Object::with_ai(unit(object_id(12)), ai.clone()),
        )
        .unwrap();
    let _ = soldiers
        .push_live(PlayerId::new(2), Object::with_ai(unit(object_id(30)), ai))
        .unwrap();
    let mut ids = vec![99; 59];
    ids.extend([30, 11, 12]);
    soldiers.auto_scout(player_id(1), &object_ids(ids)).unwrap();
    let order = |world: &World, player: usize, list_index: ListIndex| {
        world
            .parts
            .unit_ai
            .get(world.players[player].objects[list_index].id)
            .unwrap()
            .current_order
    };
    assert_eq!(
        order(&soldiers, 2, ListIndex::new(0)),
        Some(OrderKind::AutoScout)
    );
    assert_eq!(order(&soldiers, 1, ListIndex::new(0)), None);
    assert_eq!(order(&soldiers, 1, ListIndex::new(1)), None);
    soldiers
        .auto_scout(player_id(1), &object_ids([12]))
        .unwrap();
    assert_eq!(
        order(&soldiers, 1, ListIndex::new(1)),
        Some(OrderKind::AutoScout)
    );
}

/// `FUN_140c64d50`: the stance and its place are set; `NoAttack` on an attacking unit AI
/// stops its action (keeping an order not of the action), asks for its stopAction and
/// forgets its target; any other stance leaves them. Player 1's soldier at (3, 4) attacks 40
/// (its Attack waiting to swing).
#[test]
fn no_attack_stops_an_attacking_unit_ai_and_forgets_its_target() {
    let attacking = UnitAi {
        current_order: Some(OrderKind::Attack),
        current_action: Some(AiAction::Attack),
        current_target: Some(object_id(40)),
        target_class: class_of(6),
        ..UnitAi::default()
    };
    let soldier = Master {
        id: master_id(4),
        kind: MasterKind::COMBAT,
        class: class::CLASS_6,
        radius: Footprint::new(0.2, 0.2),
        hit_points: MasterHitPoints::of_data(40),
        ..Master::default()
    };
    let set = |stance| {
        let soldier_unit = SceneObject {
            ai: Some(attacking.clone()),
            actions: Some(Action {
                list: ActionList::default().with_attack(Some(own_attack(
                    40,
                    AttackState::Ready,
                    Distance::new(1.0),
                ))),
                ..Action::default()
            }),
            ..SceneObject::new(
                50,
                player_id(1),
                soldier.clone(),
                Position::new(3.0, 4.0, 0.0),
            )
        };
        let mut world = Scene::open(GridSize::new(32, 32))
            .players(2)
            .object(soldier_unit)
            .build();
        let answer = run_ai(
            &mut world,
            (player_id(1), object_id(50)),
            |ai, at, side, _, rng| {
                set_stance(
                    ai,
                    stance,
                    (at.position.ground(), at.class),
                    side,
                    rng,
                    &at.roles,
                )
            },
        );
        assert_eq!(answer, Ok(()));
        let unit_ai = world.parts.unit_ai.get(object_id(50)).unwrap().clone();
        (
            unit_ai,
            world
                .parts
                .actions
                .of(object_id(50))
                .list
                .attack()
                .is_some(),
        )
    };
    let (unit_ai, attacks) = set(UnitStance::Defensive);
    assert_eq!(
        (unit_ai.stance, unit_ai.state_position),
        (UnitStance::Defensive, Point::new(3.0, 4.0))
    );
    assert_eq!(
        (unit_ai.current_action, unit_ai.current_target, attacks),
        (Some(AiAction::Attack), Some(object_id(40)), true)
    );
    let (unit_ai, attacks) = set(UnitStance::NoAttack);
    assert_eq!(unit_ai.stance, UnitStance::NoAttack);
    // order 700 is the action's (600 + 100): the stop ends it, the state position the place
    assert_eq!(
        (unit_ai.current_order, unit_ai.current_action, attacks),
        (None, None, false)
    );
    assert_eq!(
        (unit_ai.current_target, unit_ai.last_target),
        (None, Some(object_id(40)))
    );
}

/// `FUN_140e6a7c0` after `FUN_140e55d30`: another player's unit takes the stance only while
/// players may order each other's; a class-54 unit takes no stance 1.
#[test]
fn stance_reaches_another_player_unit_only_under_the_setting() {
    let units = [(11, 4, Point::new(20.0, 20.0))];
    let mut soldiers = soldiers(&units, &[]);
    soldiers.players.push(Player::default());
    soldiers.reindex();
    soldiers.model.masters.push(Arc::new(Masters::new([Master {
        id: master_id(4),
        kind: MasterKind::COMBAT,
        class: class::CLASS_54,
        ..Master::default()
    }])));
    soldiers.parts.set_unit_ai(
        soldiers.players[1].objects[ListIndex::new(0)].id,
        Some(UnitAi::default()),
    );
    let _ = soldiers
        .push_live(
            PlayerId::new(2),
            Object::with_ai(
                Unit {
                    id: object_id(30),
                    life: Lifecycle {
                        master_id: master_id(4),
                        state: ObjectState::Alive,
                        ..Lifecycle::default()
                    },
                    ..Unit::default()
                },
                UnitAi::default(),
            ),
        )
        .unwrap();
    let stance = |world: &World, player: usize| {
        world
            .parts
            .unit_ai
            .get(world.players[player].objects[ListIndex::new(0)].id)
            .unwrap()
            .stance
    };
    soldiers
        .unit_ai_state(player_id(1), &object_ids([11, 30]), UnitStance::StandGround)
        .unwrap();
    assert_eq!(
        (stance(&soldiers, 1), stance(&soldiers, 2)),
        (UnitStance::StandGround, UnitStance::Aggressive)
    );
    soldiers
        .settings
        .flags
        .set(OptionFlags::CHEATS_ENABLED, true);
    soldiers
        .unit_ai_state(player_id(1), &object_ids([30]), UnitStance::Defensive)
        .unwrap();
    assert_eq!(stance(&soldiers, 2), UnitStance::Aggressive);
    soldiers
        .unit_ai_state(player_id(1), &object_ids([30]), UnitStance::NoAttack)
        .unwrap();
    assert_eq!(stance(&soldiers, 2), UnitStance::NoAttack);
}

/// `FUN_1411a6a70` on a unit inside a building (`+0x2f0` set, `+0x2e8` 0): no sight slots and
/// no tile, list or record change; its record (for its way out) takes the new master's values,
/// its hit points their share, its storage leaves and comes as anyone's.
#[test]
fn master_change_inside_building_keeps_to_its_record() {
    let old = Master {
        id: master_id(4),
        kind: MasterKind::COMBAT,
        class: class::CLASS_6,
        hit_points: MasterHitPoints::of_data(40),
        radius: Footprint::new(0.2, 0.2),
        standing_sprite: None,
        ..Master::default()
    };
    let next = Master {
        id: master_id(5),
        kind: MasterKind::COMBAT,
        class: class::CLASS_6,
        hit_points: MasterHitPoints::of_data(60),
        radius: Footprint::new(0.3, 0.3),
        standing_sprite: None,
        ..Master::default()
    };
    let mut spatial = Spatial::new(GridSize::new(10, 10));
    spatial.insert(
        object_id(7),
        Placed {
            position: Position::new(3.5, 3.5, 0.0),
            radius: Footprint::new(0.2, 0.2),
            class: class::CLASS_6,
            player: PlayerId::new(1),
            master: master_id(4),
            obstruction_kind: ObstructionKind::NONE,
            buildable_on: BuildableOn::NO,
            tall: false,
            moves: false,
            unknown: None,
            obstruction: None,
        },
    );
    spatial.go_inside(object_id(7), Position::new(5.0, 5.0, 0.0));
    let mut unit = Unit {
        id: object_id(7),
        place: Static::at(Position::new(5.0, 5.0, 0.0)),
        life: Lifecycle {
            hp: HitPoints::new(20.0),
            master_id: master_id(4),
            inside: Some(ContainerId::new(object_id(100))),
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let mut pathing = Pathing::default();
    let graphics = Graphics::new();
    let mut rng = Well512::default();
    let world = SwitchWorld {
        spatial: &mut spatial,
        pathing: &mut pathing,
        graphics: &graphics,
        sprite: &mut rng,
    };
    let mut combat = Combat::default();
    let switched = switch_master(
        (&mut unit, &mut combat, MotionMut::new(None)),
        &old,
        &next,
        world,
        ShownOnChange::Standing,
        &roles(),
        tuning(),
    )
    .unwrap();
    assert!(switched.promoted);
    assert_eq!(
        (unit.life.master_id, unit.life.hp),
        (master_id(5), HitPoints::new(30.0))
    );
    let record = spatial.get(object_id(7)).copied().unwrap();
    assert_eq!(
        (record.master, record.radius),
        (master_id(5), Footprint::new(0.3, 0.3))
    );
    assert!(!spatial.tile_list(Tile::new(5, 5)).contains(&object_id(7)));
}

/// What [`switched_in_the_open`] gives: what the switch leaves to the owner, the unit with its
/// combat layer, and its record.
type InTheOpen = (master_change::Switched, (Unit, Combat), Placed);

/// Slot 0x90's unit part ([`switch_master`]) of `unit` (with the motion part `motion`), at
/// (3.5, 3.5) in the open, from `old` to `next`, with the graphics `shown`: what it leaves to
/// its owner, the unit after it with its combat layer and its record.
fn switched_in_the_open(
    (mut unit, mut motion): (Unit, Option<Motion>),
    old: &Master,
    next: &Master,
    shown: &[i16],
) -> GenieResult<InTheOpen> {
    let mut spatial = Spatial::new(GridSize::new(10, 10));
    spatial.insert(
        unit.id,
        Placed {
            position: Position::new(3.5, 3.5, 0.0),
            radius: old.radius,
            class: old.class,
            player: PlayerId::new(1),
            master: old.id,
            obstruction_kind: old.obstruction_kind,
            buildable_on: BuildableOn::NO,
            tall: false,
            moves: false,
            unknown: None,
            obstruction: Some(Obstruction {
                kind: ObstructionKind::CIRCLE,
                class: ObstructionClass::UNIT,
            }),
        },
    );
    let mut pathing = Pathing::default();
    let graphics = shown
        .iter()
        .map(|&id| {
            let graphic = Graphic {
                id: SpriteId::named(id),
                frames: Frame::of_data(10),
                facets: FacetCount::of_data(8),
                speed_multiplier: SpeedMultiplier::ONE,
                frame_duration: Seconds::new(0.1),
                replay_delay: Seconds::ZERO,
                flags: SpriteFlags::ANIMATED,
            };
            (graphic.id, graphic)
        })
        .collect();
    let mut rng = Well512::default();
    let world = SwitchWorld {
        spatial: &mut spatial,
        pathing: &mut pathing,
        graphics: &graphics,
        sprite: &mut rng,
    };
    let mut combat = Combat::default();
    let switched = switch_master(
        (&mut unit, &mut combat, MotionMut::new(motion.as_mut())),
        old,
        next,
        world,
        ShownOnChange::Standing,
        &roles(),
        tuning(),
    )?;
    let record = spatial.get(unit.id).copied().unwrap();
    Ok((switched, (unit, combat), record))
}

/// The two masters of [`switched_in_the_open`]'s tests: 4 (40 hit points, standing sprite 10)
/// and 5 (60, sprite 11, volley fire 3), each with storage of several flags.
fn switch_masters() -> (Master, Master) {
    let old = Master {
        id: master_id(4),
        kind: MasterKind::COMBAT,
        class: class::CLASS_6,
        hit_points: MasterHitPoints::of_data(40),
        radius: Footprint::new(0.2, 0.2),
        standing_sprite: Some(SpriteId::named(10)),
        storage: vec![
            (Some(attribute::HEADROOM), Amount::new(1.0), 2),
            (Some(attribute::POPULATION), Amount::new(1.0), 16),
            (Some(AttributeId::named(12)), Amount::new(2.0), 32),
            (Some(AttributeId::named(13)), Amount::new(3.0), 1),
            (None, Amount::new(1.0), 2),
        ],
        ..Master::default()
    };
    let next = Master {
        id: master_id(5),
        kind: MasterKind::COMBAT,
        class: class::CLASS_6,
        hit_points: MasterHitPoints::of_data(60),
        radius: Footprint::new(0.3, 0.3),
        standing_sprite: Some(SpriteId::named(11)),
        storage: vec![
            (Some(attribute::HEADROOM), Amount::new(1.0), 2),
            (Some(AttributeId::named(14)), Amount::new(1.0), 16),
        ],
        obstruction_kind: ObstructionKind::KIND_3,
        obstruction_class: ObstructionClass::RESOURCE,
        creatable: Some(CreatablePart {
            volley_fire: Firepower::new(3.0),
            ..CreatablePart::default()
        }),
        ..Master::default()
    };
    (old, next)
}

/// `FUN_141225b70` with 0 on a unit in the open below state 3: the old master's storage of the
/// flags 2, 16 and 32 leaves, the new master's of flag 2 comes; the hit points keep their share
/// of the new master's (at most them, at least 1); its record and obstruction take the new
/// master's values; it shows the new standing sprite; in state 2 its volleys are the new
/// master's volley fire. In state 3 only the powerups go.
#[test]
fn master_change_in_the_open_moves_the_storage_and_shows_the_new_standing_sprite() {
    let (old, next) = switch_masters();
    let unit = |hp, state| Unit {
        id: object_id(7),
        place: Static::at(Position::new(3.5, 3.5, 0.0)),
        life: Lifecycle {
            hp,
            state,
            master_id: master_id(4),
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let mut sprites = Well512::default();
    let showing = Unit {
        sprite: Some(ActiveSprite::start(
            Graphic {
                id: SpriteId::named(10),
                frames: Frame::of_data(10),
                facets: FacetCount::of_data(8),
                speed_multiplier: SpeedMultiplier::ONE,
                frame_duration: Seconds::new(0.1),
                replay_delay: Seconds::ZERO,
                flags: SpriteFlags::ANIMATED,
            },
            &mut sprites,
            tuning().sprite,
        )),
        ..unit(HitPoints::new(20.0), ObjectState::Alive)
    };
    let (switched, (switched_unit, combat), record) =
        switched_in_the_open((showing, None), &old, &next, &[10, 11]).unwrap();
    assert_eq!(
        switched.leaving,
        adds([
            (attribute::HEADROOM, -1.0),
            (attribute::POPULATION, -1.0),
            (AttributeId::named(12), -2.0)
        ])
    );
    assert_eq!(switched.coming, adds([(attribute::HEADROOM, 1.0)]));
    assert!(switched.promoted);
    assert_eq!(
        (
            switched_unit.life.master_id,
            switched_unit.life.hp,
            combat.firepower
        ),
        (master_id(5), HitPoints::new(30.0), Firepower::new(3.0))
    );
    assert_eq!(
        switched_unit
            .sprite
            .map(|active_sprite| active_sprite.graphic.id),
        Some(SpriteId::named(11))
    );
    assert_eq!(
        (record.master, record.radius, record.obstruction_kind),
        (
            master_id(5),
            Footprint::new(0.3, 0.3),
            ObstructionKind::KIND_3
        )
    );
    assert_eq!(
        record.obstruction,
        Some(Obstruction {
            kind: ObstructionKind::KIND_3,
            class: ObstructionClass::RESOURCE
        })
    );
    let hp = |hp| {
        let (_, (unit, combat), _) = switched_in_the_open(
            (unit(hp, ObjectState::Unfinished), None),
            &old,
            &next,
            &[10, 11],
        )
        .unwrap();
        (unit.life.hp, combat.firepower)
    };
    assert_eq!(
        hp(HitPoints::new(50.0)),
        (HitPoints::new(60.0), Firepower::new(0.0))
    );
    assert_eq!(
        hp(HitPoints::new(0.5)),
        (HitPoints::new(1.0), Firepower::new(0.0))
    );
    let (switched, (unit, _), record) = switched_in_the_open(
        (unit(HitPoints::new(20.0), ObjectState::Dying), None),
        &old,
        &next,
        &[],
    )
    .unwrap();
    assert!(!switched.promoted && switched.leaving.is_empty());
    assert_eq!(
        (unit.life.master_id, record.master),
        (master_id(4), master_id(4))
    );
}

/// [`switch_master`]'s unmodelled parts: a share that changes the damage sprites, a moving
/// unit's new master that does not move and a new standing sprite the data file lacks.
#[test]
fn master_change_unmodelled_parts_fail() {
    let (old, next) = switch_masters();
    let unit = Unit {
        id: object_id(7),
        life: Lifecycle {
            hp: HitPoints::new(0.1),
            master_id: master_id(4),
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let damaged = Master {
        damage_graphics: vec![DamageGraphic::default()],
        ..old.clone()
    };
    let reason = |result: GenieResult<_>| result.unwrap_err().reason();
    assert_eq!(
        reason(switched_in_the_open(
            (unit.clone(), None),
            &damaged,
            &next,
            &[10, 11]
        )),
        "a master change that changes the damage sprites (FUN_1411859a0)"
    );
    let walking = Motion::Moving(walker());
    assert_eq!(
        reason(switched_in_the_open(
            (unit.clone(), Some(walking)),
            &old,
            &next,
            &[10, 11]
        )),
        "a moving unit's new master does not move"
    );
    assert_eq!(
        reason(switched_in_the_open((unit, None), &old, &next, &[10])),
        "a new master's standing sprite the data file does not have"
    );
}

/// `FUN_140a97730`: an empty list and a `MoveTo` head take no move; an Artifact head takes
/// the point (its goal); an action the slice does not hold is refused.
#[test]
fn the_head_action_takes_move_only_where_its_slot_0x40_does() {
    let units = [(11, 4, Point::new(20.0, 20.0))];
    let mut world = soldiers(&units, &[]);
    let point = Position::new(25.0, 21.0, 0.0);
    assert_eq!(
        world.head_takes_move(PlayerId::new(1), ListIndex::new(0), None, point),
        Ok(Answer::NotTaken)
    );
    world
        .parts
        .actions
        .of_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .set_move_to(Some(MoveToAction {
            sprite: None,
            state: MoveTo::Start,
            point: Position::new(30.0, 30.0, 0.0),
            target: None,
            range: Distance::ZERO,
            voice: Voice::Loud,
            on_target_moved: TargetMoved::Follows,
            wait: MoveWait::default(),
        }));
    assert_eq!(
        world.head_takes_move(
            PlayerId::new(1),
            ListIndex::new(0),
            Some(object_id(11)),
            point
        ),
        Ok(Answer::NotTaken)
    );
    world
        .parts
        .actions
        .of_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .set_move_to(None);
    world
        .parts
        .actions
        .of_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .set_artifact(Some(Artifact::new(PlayerId::new(1), None)));
    assert_eq!(
        world.head_takes_move(PlayerId::new(1), ListIndex::new(0), None, point),
        Ok(Answer::Taken)
    );
    let artifact = world
        .parts
        .actions_of(world.players[1].objects[ListIndex::new(0)].id)
        .list
        .artifact()
        .copied()
        .unwrap();
    assert_eq!(artifact.goal, Some(point));
    world
        .parts
        .actions
        .of_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .set_artifact(None);
    world.set_unread(
        world.players[1].objects[ListIndex::new(0)].id,
        GenieError::unsupported("a test's action"),
    );
    assert!(
        world
            .head_takes_move(PlayerId::new(1), ListIndex::new(0), None, point)
            .is_err()
    );
}

/// `FUN_14161c920`'s box: each unit's x is set against its own y, the units before the last
/// fold into the running least and greatest; units side by side on a diagonal pass, two near
/// each other far off it do not.
#[test]
fn follow_box_sets_each_unit_column_against_its_row() {
    assert_eq!(
        follow_spread(
            &[Point::new(20.0, 20.0), Point::new(21.0, 20.0)],
            &tuning().order
        ),
        Some(Point::new(20.5, 20.0))
    );
    assert_eq!(
        follow_spread(
            &[Point::new(10.0, 30.0), Point::new(10.5, 30.0)],
            &tuning().order
        ),
        None
    );
    // the last unit's spread: 8 passes, more does not
    assert_eq!(
        follow_spread(&[Point::new(10.0, 18.0)], &tuning().order),
        Some(Point::new(10.0, 18.0))
    );
    assert_eq!(
        follow_spread(&[Point::new(10.0, 18.5)], &tuning().order),
        None
    );
    // the running spread holds the units before the last
    assert_eq!(
        follow_spread(
            &[
                Point::new(10.0, 10.0),
                Point::new(30.0, 30.0),
                Point::new(30.0, 30.0)
            ],
            &tuning().order
        ),
        None
    );
    assert_eq!(
        follow_spread(&[], &tuning().order),
        Some(Point::new(0.0, 0.0))
    );
}

/// A FOLLOW offset stays on the map: below 0 it is 0, past the size less 0.01 that.
#[test]
fn follow_offset_stays_on_the_map() {
    assert_eq!(follow_axis(-0.5, 120, &tuning().group), 0.0);
    assert_eq!(follow_axis(130.0, 120, &tuning().group), 119.99);
    assert_eq!(follow_axis(60.25, 120, &tuning().group), 60.25);
    assert!(follow_axis(f32::NAN, 120, &tuning().group).is_nan());
}

/// Player 1's soldiers 11 and 12 (moving, each with a unit AI) and player 2's 30 at (40, 20),
/// a master with a footprint of class 6; auto formations off, a map of 120 tiles.
fn followers() -> World {
    let units = [
        (11, 4, Point::new(20.0, 20.0)),
        (12, 4, Point::new(21.0, 20.0)),
    ];
    let mut world = soldiers(&units, &[]);
    world.players[1].formations.auto = false;
    world.spatial.map = GridSize::new(120, 120);
    let walker = walker();
    world.model.masters[1] = Arc::new(Masters::new([Master {
        id: master_id(4),
        kind: MasterKind::COMBAT,
        ..moving_master(walker.stats)
    }]));
    for unit in &mut world.players[1].objects {
        world.parts.put_mover(unit.id, Some(walker.clone()));
        world.parts.set_unit_ai(
            unit.id,
            Some(UnitAi {
                current_order: None,
                current_action: None,
                current_target: None,
                ..UnitAi::default()
            }),
        );
    }
    world.players.push(Player::default());
    world.reindex();
    world.model.masters.push(Arc::new(Masters::new([Master {
        id: master_id(5),
        kind: MasterKind::COMBAT,
        class: class::CLASS_6,
        radius: Footprint::new(0.5, 0.5),
        ..Master::default()
    }])));
    let _ = world
        .push_live(
            PlayerId::new(2),
            Unit {
                id: object_id(30),
                place: Static::at(Position::new(40.0, 20.0, 0.0)),
                life: Lifecycle {
                    master_id: master_id(5),
                    state: ObjectState::Alive,
                    ..Lifecycle::default()
                },
                ..Unit::default()
            },
        )
        .unwrap();
    world.spatial.insert(
        object_id(30),
        Placed {
            position: Position::new(40.0, 20.0, 0.0),
            radius: Footprint::new(0.5, 0.5),
            class: class::CLASS_6,
            player: PlayerId::new(2),
            master: master_id(5),
            obstruction_kind: ObstructionKind::NONE,
            buildable_on: BuildableOn::NO,
            tall: false,
            moves: false,
            unknown: None,
            obstruction: None,
        },
    );
    world
}

/// GUARD (`FUN_14161dee0`): each unit's AI takes the defend order on the target at once,
/// run outside its update: it stops, forgets its target and defends the target within 2.5
/// (the defend action, the target at its place), the order processed and gone. Slot 0x30
/// (`FUN_140c563d0`) makes the order ready (its last argument, 1) and shows it at the
/// target's x and y, whole ones too (430024841's scout guarding a villager at x 144).
#[test]
fn guard_gives_its_units_the_defend_order_at_once() {
    for x in [20.3, 40.0] {
        let mut world = followers();
        world.players[2].objects[ListIndex::new(0)].place_for_test(Position::new(x, 20.0, 0.0));
        world
            .spatial
            .set_position(
                object_id(30),
                Position::new(x, 20.0, 0.0),
                &roles(),
                tuning().visibility,
            )
            .unwrap();
        world
            .guard(PlayerId::new(1), &object_ids([11]), object_id(30))
            .unwrap();
        let ai = world
            .parts
            .unit_ai
            .get(world.players[1].objects[ListIndex::new(0)].id)
            .unwrap();
        assert_eq!(
            (ai.current_order, ai.current_action, ai.order_priority.get()),
            (Some(OrderKind::Defend), Some(AiAction::Defend), 100)
        );
        assert_eq!(
            (ai.current_target, ai.defend_target, ai.desired_distance),
            (
                Some(object_id(30)),
                Some(object_id(30)),
                Distance::new(tuning().order.guard_range)
            )
        );
        assert_eq!(ai.target_point, Position::new(x, 20.0, 0.0));
        assert_eq!(ai.orders.len(), 0);
    }
}

/// `FUN_14161c920` for one unit: it follows the target (a `MoveTo` to it), its unit AI takes
/// the follow order and action and the target with its class at its place.
#[test]
fn follower_takes_the_follow_order_and_walks_to_its_target() {
    let mut world = followers();
    world
        .follow(PlayerId::new(1), &object_ids([11]), object_id(30))
        .unwrap();
    let unit = &world.players[1].objects[ListIndex::new(0)];
    let ai = world.parts.unit_ai.get(unit.id).unwrap();
    assert_eq!(
        (ai.current_order, ai.current_action),
        (Some(OrderKind::Follow), Some(AiAction::Follow))
    );
    assert_eq!(
        (ai.current_target, ai.target_class),
        (Some(object_id(30)), class_of(6))
    );
    assert_eq!(ai.target_point, Position::new(40.0, 20.0, 0.0));
    assert_eq!(
        ObjectId::field(
            world
                .parts
                .actions_of(unit.id)
                .list
                .move_to()
                .copied()
                .unwrap()
                .target
        ),
        30
    );
    assert_eq!(
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(1)].id)
            .list
            .move_to()
            .copied(),
        None
    );
}

/// `FUN_14161c920` for two units side by side: each moves to the target at its offset from
/// the two's mean, takes the move order and action, and ignores the other.
#[test]
fn followers_side_by_side_take_their_offsets_from_the_target() {
    let mut world = followers();
    world
        .follow(PlayerId::new(1), &object_ids([11, 12]), object_id(30))
        .unwrap();
    for (index, (other, x)) in [(12, 39.5), (11, 40.5)].into_iter().enumerate() {
        let unit = &world.players[1].objects[ListIndex::new(index)];
        let ai = world.parts.unit_ai.get(unit.id).unwrap();
        assert_eq!(
            (ai.current_order, ai.current_action),
            (Some(OrderKind::Move), Some(AiAction::Move))
        );
        assert_eq!(
            (ai.current_target, ai.target_point),
            (Some(object_id(30)), Position::new(x, 20.0, 0.0))
        );
        assert_eq!(
            world.parts.mover_of(unit.id).unwrap().ignore_units,
            [object_id(other)]
        );
        assert_eq!(
            ObjectId::field(
                world
                    .parts
                    .actions_of(unit.id)
                    .list
                    .move_to()
                    .copied()
                    .unwrap()
                    .target
            ),
            30
        );
    }
}

/// FOLLOW's check: another player's unit takes nothing while players may not order each
/// other's units.
#[test]
fn follow_of_another_player_unit_does_nothing() {
    let mut world = followers();
    world
        .follow(PlayerId::new(2), &object_ids([11]), object_id(30))
        .unwrap();
    assert_eq!(
        world
            .parts
            .unit_ai
            .get(world.players[1].objects[ListIndex::new(0)].id)
            .unwrap()
            .current_order,
        None
    );
}

/// `FUN_140aaddb0`: a Build on an annex counts its main building's builders one less; a
/// foundation the world no longer holds counts nothing.
#[test]
fn builder_leaves_the_main_building_of_an_annex() {
    let units = [(11, 4, Point::new(20.0, 20.0))];
    let mut soldiers = soldiers(&units, &[]);
    soldiers.model.masters[1] = Arc::new(Masters::new([Master {
        id: master_id(79),
        kind: MasterKind::BUILDING,
        ..Master::default()
    }]));
    let construction = Construction {
        progress: Seconds::ZERO,
        desolid: true,
        builders: BuilderCount::of_data(2),
        hit_points: MasterHitPoints::of_data(100),
        build_time: WholeSeconds::of_data(10),
        record: Obstruction {
            kind: ObstructionKind::KIND_2,
            class: ObstructionClass::UNIT,
        },
        snow: Snow::None,
        sprite_override: false,
    };
    let building = |id, annex_of| {
        Object::with_building(
            Unit {
                id,
                life: Lifecycle {
                    master_id: master_id(79),
                    ..Lifecycle::default()
                },
                ..Unit::default()
            },
            Building {
                annex_of,
                construction: Some(construction),
                ..Default::default()
            },
        )
    };
    soldiers.set_live_for_test(
        player_id(1),
        [
            building(object_id(60), None),
            building(object_id(70), Some(MainBuildingId::new(object_id(60)))),
        ],
    );
    soldiers.leave_builders(object_id(70)).unwrap();
    let builders = |world: &World, list_index: ListIndex| {
        world
            .parts
            .building_of(world.players[1].objects[list_index].id)
            .construction
            .unwrap()
            .builders
            .get()
    };
    assert_eq!(
        (
            builders(&soldiers, ListIndex::new(0)),
            builders(&soldiers, ListIndex::new(1))
        ),
        (1, 2)
    );
    soldiers.leave_builders(object_id(60)).unwrap();
    soldiers.leave_builders(object_id(99)).unwrap();
    assert_eq!(builders(&soldiers, ListIndex::new(0)), 0);
}

/// `FUN_140c4da50`: the first queued order with a target that the unit itself issued; an
/// order of another issuer or without a target does not count.
#[test]
fn unit_own_queued_order_names_its_target() {
    let order = |issuer, target| QueuedOrder {
        issuer,
        order: OrderKind::Move,
        priority: OrderPriority::of_data(100),
        target,
        target_owner: None,
        point: Position::new(1.0, 1.0, 0.0),
        range: Distance::ZERO,
        shown: Point::new(1.0, 1.0),
        ready: true,
        target_kind: None,
    };
    let mut ai = UnitAi {
        orders: vec![
            order(Some(Issuer::Player(player_id(1))), Some(object_id(50))),
            order(Some(Issuer::Object(object_id(11))), None),
            order(Some(Issuer::Object(object_id(11))), Some(object_id(60))),
            order(Some(Issuer::Object(object_id(11))), Some(object_id(70))),
        ],
        ..UnitAi::default()
    };
    assert_eq!(own_order_target(&ai, object_id(11)), Some(object_id(60)));
    ai.orders.truncate(2);
    assert_eq!(own_order_target(&ai, object_id(11)), None);
}

/// `FUN_140a7a230`: a new target is taken (its workers count one more) and the old one let
/// go; a quiet sub-action does neither; a target in state 7 or more is not taken; the same
/// target changes nothing.
#[test]
fn an_action_new_target_is_taken_and_its_old_one_let_go() {
    let units = [(11, 4, Point::new(20.0, 20.0))];
    let mut soldiers = soldiers(&units, &[]);
    soldiers.model.masters[0] = Arc::new(Masters::new([Master {
        id: master_id(66),
        kind: MasterKind::COMBAT,
        ..Master::default()
    }]));
    let deer = |id, state, workers| Unit {
        id,
        workers,
        life: Lifecycle {
            master_id: master_id(66),
            state,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    soldiers.players[0].objects = vec![
        deer(object_id(40), ObjectState::Alive, WorkerCount::of_data(1)),
        deer(object_id(41), ObjectState::Alive, WorkerCount::of_data(0)),
        deer(object_id(42), ObjectState::Removed, WorkerCount::of_data(0)),
    ]
    .into();
    soldiers.reindex();
    let workers = |world: &World| -> Vec<i32> {
        world.players[0]
            .objects
            .iter()
            .map(|unit| unit.workers.get())
            .collect()
    };
    assert_eq!(
        soldiers.retarget_action(
            PlayerId::new(1),
            ListIndex::new(0),
            Some(object_id(40)),
            object_id(40),
            Voice::Loud
        ),
        Ok(Some(object_id(40)))
    );
    assert_eq!(workers(&soldiers), [1, 0, 0]);
    assert_eq!(
        soldiers.retarget_action(
            PlayerId::new(1),
            ListIndex::new(0),
            Some(object_id(40)),
            object_id(41),
            Voice::Quiet
        ),
        Ok(Some(object_id(41)))
    );
    assert_eq!(workers(&soldiers), [1, 0, 0]);
    assert_eq!(
        soldiers.retarget_action(
            PlayerId::new(1),
            ListIndex::new(0),
            Some(object_id(40)),
            object_id(41),
            Voice::Loud
        ),
        Ok(Some(object_id(41)))
    );
    assert_eq!(workers(&soldiers), [0, 1, 0]);
    assert_eq!(
        soldiers.retarget_action(
            PlayerId::new(1),
            ListIndex::new(0),
            Some(object_id(41)),
            object_id(42),
            Voice::Loud
        ),
        Ok(None)
    );
    assert_eq!(workers(&soldiers), [0, 0, 0]);
    assert_eq!(
        soldiers.retarget_action(
            PlayerId::new(1),
            ListIndex::new(0),
            Some(object_id(99)),
            // a new target the world does not hold (the game's −1 finds none either)
            object_id(98),
            Voice::Loud
        ),
        Ok(None)
    );
}

/// `FUN_140ab0300`: a Build counted among its foundation's builders leaves them for a new
/// target, which it takes; the same target keeps the count.
#[test]
fn build_new_target_leaves_the_old_foundation_builders() {
    let units = [(11, 4, Point::new(20.0, 20.0))];
    let mut soldiers = soldiers(&units, &[]);
    soldiers.model.masters[1] = Arc::new(Masters::new([
        Master {
            id: master_id(4),
            kind: MasterKind::COMBAT,
            ..Master::default()
        },
        Master {
            id: master_id(79),
            kind: MasterKind::BUILDING,
            ..Master::default()
        },
    ]));
    let foundation = |id, builders| {
        Object::with_building(
            Unit {
                id,
                workers: WorkerCount::of_data(1),
                life: Lifecycle {
                    master_id: master_id(79),
                    ..Lifecycle::default()
                },
                ..Unit::default()
            },
            Building {
                construction: Some(Construction {
                    progress: Seconds::ZERO,
                    desolid: true,
                    builders,
                    hit_points: MasterHitPoints::of_data(100),
                    build_time: WholeSeconds::of_data(10),
                    record: Obstruction {
                        kind: ObstructionKind::KIND_2,
                        class: ObstructionClass::UNIT,
                    },
                    snow: Snow::None,
                    sprite_override: false,
                }),
                ..Default::default()
            },
        )
    };
    let _ = soldiers
        .push_live(
            PlayerId::new(1),
            foundation(object_id(60), BuilderCount::of_data(1)),
        )
        .unwrap();
    let _ = soldiers
        .push_live(
            PlayerId::new(1),
            foundation(object_id(61), BuilderCount::of_data(0)),
        )
        .unwrap();
    soldiers
        .parts
        .actions
        .of_mut(soldiers.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .set_build(Some(BuildAction {
            target: Some(FoundationId::new(object_id(60))),
            state: BuildState::Work,
            point: Position::new(-1.0, -1.0, -1.0),
            task: BuildTask {
                range: Distance::new(0.1),
                work_value: WorkValue::new(1.0),
                work_rate: WorkRate::new(1.0),
                resource: None,
                id: TaskId::of_task(-1),
                key: task_key(ActionKind::BUILD),
                sprites: TaskSprites::NONE,
            },
            added: true,
            timer: Seconds::ZERO,
            site: None,
        }));
    assert_eq!(
        soldiers.build_set_target(
            PlayerId::new(1),
            ListIndex::new(0),
            (Some(FoundationId::new(object_id(60))), true),
            FoundationId::new(object_id(60))
        ),
        Ok(Some(FoundationId::new(object_id(60))))
    );
    assert!(
        soldiers
            .parts
            .actions_of(soldiers.players[1].objects[ListIndex::new(0)].id)
            .list
            .build()
            .copied()
            .unwrap()
            .added
    );
    assert_eq!(
        soldiers.build_set_target(
            PlayerId::new(1),
            ListIndex::new(0),
            (Some(FoundationId::new(object_id(60))), true),
            FoundationId::new(object_id(61))
        ),
        Ok(Some(FoundationId::new(object_id(61))))
    );
    let build = soldiers
        .parts
        .actions_of(soldiers.players[1].objects[ListIndex::new(0)].id)
        .list
        .build()
        .copied()
        .unwrap();
    assert_eq!(
        (build.target.map(FoundationId::id), build.added),
        (Some(object_id(61)), false)
    );
    let site = |world: &World, list_index: ListIndex| {
        let unit = &world.players[1].objects[list_index];
        (
            world
                .parts
                .building_of(unit.id)
                .construction
                .unwrap()
                .builders
                .get(),
            unit.workers.get(),
        )
    };
    assert_eq!(
        (
            site(&soldiers, ListIndex::new(1)),
            site(&soldiers, ListIndex::new(2))
        ),
        ((0, 0), (0, 2))
    );
}

/// `FUN_141211340` and `FUN_14118f580` in a gate that shut: a soldier dies (no hit points),
/// class 42 and livestock with food are spared, livestock with a NaN load is not, and a
/// sleeping object (below type 70) wakes to the end of the live list and dies.
#[test]
fn shutting_gate_kills_the_units_in_it_but_the_spared() {
    let units = [(11, 4, Point::new(20.0, 20.0))];
    let mut world = soldiers(&units, &[]);
    let master = |id, kind, class| Master {
        id,
        kind,
        class,
        ..Master::default()
    };
    world.model.masters[1] = Arc::new(Masters::new([
        master(master_id(4), MasterKind::COMBAT, class::CLASS_6),
        master(ids::ID_42, MasterKind::COMBAT, class::RELIC),
        master(master_id(58), MasterKind::COMBAT, class::LIVESTOCK),
        master(master_id(10), MasterKind::STATIC, class::CLASS_5),
    ]));
    let unit = |id, master, amount| Unit {
        id,
        life: Lifecycle {
            master_id: master,
            hp: HitPoints::new(10.0),
            amount,
            state: ObjectState::Alive,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    world.players[1].objects = vec![
        unit(object_id(11), master_id(4), Amount::ZERO),
        unit(object_id(12), ids::ID_42, Amount::ZERO),
        unit(object_id(13), master_id(58), Amount::new(50.0)),
        unit(object_id(14), master_id(58), Amount::new(f32::NAN)),
    ]
    .into();
    world.players[1].sleeping = vec![unit(object_id(15), master_id(10), Amount::ZERO)].into();
    world.reindex();
    for id in 11..=16 {
        world.shut_in_gate(object_id(id)).unwrap();
    }
    let hp: Vec<(i32, f32)> = world.players[1]
        .objects
        .iter()
        .map(|unit| (unit.id.get(), unit.life.hp.get()))
        .collect();
    assert_eq!(
        hp,
        [(11, 0.0), (12, 10.0), (13, 10.0), (14, 0.0), (15, 0.0)]
    );
    assert_eq!(world.players[1].sleeping.len(), 0);
}

/// `0x140ac60e0` case 6 and `FUN_140ac4c60`: the hunt's own sheep goes to the drop site, and
/// so does the player's idle livestock in state 2 on a tile within 3 that the player sees,
/// then the livestock around that; another player's or one too far stays.
#[test]
fn led_sheep_takes_the_idle_livestock_about_it_to_the_drop_site() {
    let units = [(11, 4, Point::new(30.0, 30.0))];
    let mut world = soldiers(&units, &[]);
    world.spatial = Spatial::new(GridSize::new(40, 40));
    let sight = Visibility::new(
        GridSize::new(40, 40),
        vec![],
        vec![VisionBits::from_bits_retain(u64::MAX); 1600],
        vec![],
        0,
        vec![],
    );
    world.spatial.sight = Some(sight);
    let walker = walker();
    let sheep_master = Master {
        id: master_id(594),
        kind: MasterKind::COMBAT,
        class: class::LIVESTOCK,
        radius: Footprint::new(0.2, 0.2),
        ..moving_master(walker.stats)
    };
    world.model.masters[1] = Arc::new(Masters::new([
        sheep_master.clone(),
        Master {
            id: master_id(4),
            kind: MasterKind::COMBAT,
            radius: Footprint::new(1.5, 1.5),
            ..Master::default()
        },
    ]));
    world.players.push(Player::default());
    world.reindex();
    world
        .model
        .masters
        .push(Arc::new(Masters::new([sheep_master])));
    let sheep = |id, x: f32, y: f32| {
        Object::moving(
            Unit {
                id,
                place: Static::at(Position::new(x, y, 0.0)),
                life: Lifecycle {
                    master_id: master_id(594),
                    state: ObjectState::Alive,
                    ..Lifecycle::default()
                },
                ..Unit::default()
            },
            walker.clone(),
        )
    };
    world
        .extend_live(
            PlayerId::new(1),
            [
                sheep(object_id(20), 10.5, 10.5),
                sheep(object_id(21), 12.5, 11.5),
                sheep(object_id(22), 20.5, 10.5),
            ],
        )
        .unwrap();
    let _ = world
        .push_live(PlayerId::new(2), sheep(object_id(23), 11.5, 10.5))
        .unwrap();
    let placed = |player, master, at: Point, radius| Placed {
        position: at.with_height(0.0),
        radius: Footprint::new(radius, radius),
        class: class::LIVESTOCK,
        player,
        master,
        obstruction_kind: ObstructionKind::NONE,
        buildable_on: BuildableOn::NO,
        tall: false,
        moves: true,
        unknown: None,
        obstruction: None,
    };
    world.spatial.insert(
        object_id(11),
        placed(PlayerId::new(1), master_id(4), Point::new(30.0, 30.0), 1.5),
    );
    for (id, player, at) in [
        (20, 1, Point::new(10.5, 10.5)),
        (21, 1, Point::new(12.5, 11.5)),
        (22, 1, Point::new(20.5, 10.5)),
        (23, 2, Point::new(11.5, 10.5)),
    ] {
        world.spatial.insert(
            object_id(id),
            placed(PlayerId::from(player), master_id(594), at, 0.2),
        );
    }
    world
        .lead_herd(
            PlayerId::new(1),
            object_id(20),
            DropSiteId::new(object_id(11)),
        )
        .unwrap();
    let goes = |player: usize, id: i32| {
        world.players[player]
            .objects
            .iter()
            .find(|unit| unit.id == object_id(id))
            .and_then(|unit| world.parts.actions_of(unit.id).list.move_to().copied())
            .map(|move_to_action| ObjectId::field(move_to_action.target))
    };
    assert_eq!(goes(1, 20), Some(11));
    assert_eq!(goes(1, 21), Some(11));
    assert_eq!(goes(1, 22), None);
    assert_eq!(goes(2, 23), None);
}

/// `FUN_1410395a0`: the old list goes, the new one is copied whole, an empty one too.
#[test]
fn group_differentiator_groups_are_the_list_given() {
    let mut group = UnitGroup {
        differentiator_groups: vec![group_id(1), group_id(2), group_id(3)],
        ..UnitGroup::default()
    };
    set_differentiator_groups(&mut group, &[group_id(7), group_id(8)]);
    assert_eq!(group.differentiator_groups, [group_id(7), group_id(8)]);
    set_differentiator_groups(&mut group, &[]);
    assert_eq!(group.differentiator_groups, Vec::new());
}

/// `FUN_14118c790`: each id joins the end of the ignore list, a repeated one again.
#[test]
fn an_ignored_unit_joins_the_end_of_the_ignore_list() {
    let mut ignore = vec![object_id(7)];
    for id in [9, 8, 9] {
        ignore_unit(&mut ignore, object_id(id));
    }
    assert_eq!(ignore, object_id_list([7, 9, 8, 9]));
}

/// `FUN_1410862c0`: too close below the least range (COMISS, SETA: NaN not), in reach at
/// most the weapon range (COMISS, JC: NaN out) and not too close.
#[test]
fn target_is_in_reach_within_the_weapon_range_and_not_below_the_least() {
    assert_eq!(
        work_reach(Distance::new(3.0), Distance::new(5.0), Distance::new(2.0)),
        (true, false)
    );
    assert_eq!(
        work_reach(Distance::new(5.0), Distance::new(5.0), Distance::new(2.0)),
        (true, false)
    );
    assert_eq!(
        work_reach(Distance::new(5.5), Distance::new(5.0), Distance::new(2.0)),
        (false, false)
    );
    assert_eq!(
        work_reach(Distance::new(1.0), Distance::new(5.0), Distance::new(2.0)),
        (false, true)
    );
    assert_eq!(
        work_reach(Distance::new(2.0), Distance::new(5.0), Distance::new(2.0)),
        (true, false)
    );
    assert_eq!(
        work_reach(
            Distance::new(f32::NAN),
            Distance::new(5.0),
            Distance::new(2.0)
        ),
        (false, false)
    );
    assert_eq!(
        work_reach(Distance::ZERO, Distance::ZERO, Distance::ZERO),
        (true, false)
    );
}

/// Player 1's soldier 10 (master 4, ranges 2 to 5) at (10, 10) and player 2's unit 20 at
/// (`x`, 10), the two enemies, each with a unit AI in state 2.
fn soldier_and_enemy(x: f32, class: ClassId) -> World {
    let unit = |id, master, x: f32| Unit {
        id,
        place: Static::at(Position::new(x, 10.0, 0.0)),
        life: Lifecycle {
            master_id: master,
            state: ObjectState::Alive,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let player = |objects, relations| Player {
        objects,
        relations,
        ..Player::default()
    };
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO, Millis::ZERO],
        vec![
            player(vec![].into(), relation_row([0, 3, 3])),
            player(
                vec![unit(object_id(10), master_id(4), 10.0)].into(),
                relation_row([3, 0, 3]),
            ),
            player(
                vec![unit(object_id(20), master_id(5), x)].into(),
                relation_row([3, 3, 0]),
            ),
        ],
    )
    .with_ais([10, 20].map(|id| (object_id(id), UnitAi::default())));
    let master = |id, class| Master {
        id,
        kind: MasterKind::COMBAT,
        class,
        radius: Footprint::new(0.2, 0.2),
        combat: Some(CombatPart {
            weapon_range: Distance::new(5.0),
            min_range: Distance::new(2.0),
            ..CombatPart::default()
        }),
        ..Master::default()
    };
    world.model.masters = vec![
        Arc::new(Masters::new([])),
        Arc::new(Masters::new([master(master_id(4), class)])),
        Arc::new(Masters::new([master(master_id(5), class::CLASS_6)])),
    ];
    for (id, player, x) in [(10, 1, 10.0), (20, 2, x)] {
        world.spatial.insert(
            object_id(id),
            Placed {
                position: Position::new(x, 10.0, 0.0),
                radius: Footprint::new(0.2, 0.2),
                class,
                player: PlayerId::from(player),
                master: master_id(4),
                obstruction_kind: ObstructionKind::NONE,
                buildable_on: BuildableOn::NO,
                tall: false,
                moves: false,
                unknown: None,
                obstruction: None,
            },
        );
    }
    world
}

/// `FUN_141227190` on an enemy nearer than the least range: the unit posts notify 0x1fe
/// with the target to its own unit AI (slot 0x328) and takes no work.
#[test]
fn an_order_on_an_enemy_below_the_least_range_notifies_the_unit_ai() {
    let mut world = soldier_and_enemy(11.0, class::CLASS_6);
    let work = Work {
        target: Some(object_id(20)),
        point: Position::new(11.0, 10.0, 0.0),
        disable_retargeting: false,
        filter: None,
    };
    world
        .order_work(PlayerId::new(1), ListIndex::new(0), &work)
        .unwrap();
    let unit = &world.players[1].objects[ListIndex::new(0)];
    let ai = world.parts.unit_ai.get(unit.id).unwrap();
    assert_eq!(
        ai.notifies,
        [Notify::own(
            object_id(10),
            NotifyKind::TOO_CLOSE,
            [20, -1, -1]
        )]
    );
    assert_eq!(world.parts.actions_of(unit.id).top_action_kind(), None);
}

/// `FUN_140fd9ae0` with 61 or more: the sorted list (a gone unit null, sorted first) splits
/// in runs of 60, each split on its own (`FUN_140fd76d0`) into the one split: its groups count
/// on, and an id repeated across two runs is in each.
#[test]
fn the_splitter_splits_an_order_to_more_than_60_units_in_runs_of_60() {
    let units: Vec<_> = (100..161)
        .map(|id| (id, 4, Point::new(20.0, 20.0)))
        .collect();
    let mut world = soldiers(&units, &[]);
    let ids = object_ids((100..161).rev());
    let split = world.split(&ids, Kept::All, ControlKey::Up).unwrap();
    assert_eq!(split.groups, [object_id_list(100..160)]);
    assert_eq!(split.loose, object_id_list([160]));
    // the gone 999 is the first run's first entry
    let gone: Vec<_> = ids.iter().copied().chain(object_ids([999])).collect();
    let split = world.split(&gone, Kept::All, ControlKey::Up).unwrap();
    assert_eq!(
        split.groups,
        [object_id_list(100..159), object_id_list([159, 160])]
    );
    assert_eq!(split.loose, Vec::<ObjectId>::new());
    // 159 twice: the first run's last entry has no next one to repeat it
    let twice = object_ids((100..160).chain([159]));
    let split = world.split(&twice, Kept::All, ControlKey::Up).unwrap();
    assert_eq!(split.groups, [object_id_list(100..160)]);
    assert_eq!(split.loose, object_id_list([159]));
    // with Ctrl held the first run fills the 60 groups, the second's group is dropped
    let split = world.split(&ids, Kept::All, ControlKey::Held).unwrap();
    assert_eq!(split.groups.len(), 60);
    assert_eq!(split.groups[59], [object_id(159)]);
    assert_eq!(split.loose, Vec::<ObjectId>::new());
}

/// MarvinBlue/426048731's MOVE of 1569: player 2's villagers 9175 and 9177 (by id) take a
/// grid of one cell a row, half a tile apart, from the order's point (their `MoveTos`' points in
/// the save after 1568).
#[test]
fn units_of_move_take_the_cells_of_grid() {
    let point = Point::new(29.78125, 92.489_586);
    assert_eq!(
        grid_point(0, 2, point, GridSize::new(120, 120), &tuning().order),
        Point::new(29.78125, 92.489_586)
    );
    assert_eq!(
        grid_point(1, 2, point, GridSize::new(120, 120), &tuning().order),
        Point::new(29.78125, 92.989_586)
    );
    // nine units: three a row around the point; off the map an axis keeps a tile inside
    assert_eq!(
        grid_point(
            0,
            9,
            Point::new(10.0, 10.0),
            GridSize::new(120, 120),
            &tuning().order
        ),
        Point::new(9.5, 9.5)
    );
    assert_eq!(
        grid_point(
            4,
            9,
            Point::new(10.0, 10.0),
            GridSize::new(120, 120),
            &tuning().order
        ),
        Point::new(10.0, 10.0)
    );
    assert_eq!(
        grid_point(
            8,
            9,
            Point::new(119.75, 0.25),
            GridSize::new(120, 120),
            &tuning().order
        ),
        Point::new(119.0, 0.75)
    );
    assert_eq!(
        grid_point(
            0,
            9,
            Point::new(0.25, 0.25),
            GridSize::new(120, 120),
            &tuning().order
        ),
        Point::new(1.0, 1.0)
    );
}

/// MarvinBlue/426048731's INTERACT of 927: player 1's builders 9171 and 9173 stand side by
/// side; the queued orders of the save after 927 hold their points around the order's.
#[test]
fn units_of_an_order_take_their_offsets_from_their_centroid() {
    let (first, second) = (
        Point::new(104.32466, 39.299_885),
        Point::new(103.67509, 39.299_885),
    );
    let centroid = centroid(&[first, second], &tuning().order).unwrap();
    let point = Point::new(108.37596, 43.91338);
    assert_eq!(
        offset_point(
            first,
            centroid,
            point,
            GridSize::new(120, 120),
            &tuning().order
        ),
        Point::new(108.700_745, 43.91338)
    );
    assert_eq!(
        offset_point(
            second,
            centroid,
            point,
            GridSize::new(120, 120),
            &tuning().order
        ),
        Point::new(108.05117, 43.91338)
    );
}

/// A unit left of the first one counted moves only the least x, so the greatest stays 0 and
/// the box passes however far apart the units stand; a unit 9 tiles right of the first does
/// not.
#[test]
fn the_box_of_an_order_moves_only_its_least_value_for_lower_one() {
    assert!(
        centroid(
            &[Point::new(50.0, 50.0), Point::new(30.0, 50.0)],
            &tuning().order
        )
        .is_some()
    );
    assert_eq!(
        centroid(
            &[Point::new(30.0, 50.0), Point::new(39.0, 50.0)],
            &tuning().order
        ),
        None
    );
    assert_eq!(
        centroid(
            &[Point::new(30.0, 50.0), Point::new(38.0, 50.0)],
            &tuning().order
        ),
        Some(Point::new(34.0, 50.0))
    );
}

/// MarvinBlue/426048731's MOVE of 2979 (the save after 2977): player 2's livestock 9528 and
/// 9529 (formation category 0) stay loose, the scout 9185 has none to join it.
#[test]
fn the_splitter_joins_units_of_one_kind_within_the_influence_distance() {
    let formations = Formations {
        auto: true,
        influence: Distance::new(3.0),
        by_speed: false,
        ..Formations::default()
    };
    let unit = |id, grouped, position: Position| Splittable {
        id: object_id(id),
        group: None,
        kind: MasterKind::COMBAT,
        grouped,
        turns: false,
        speed: Speed::new(1.2),
        position,
        moved_at: position.ground(),
        state: ObjectState::Alive,
        radius: Distance::new(0.2),
        restriction: Some(RestrictionId::named(1)),
    };
    // no planner: the distance decides
    let mut none = |_: &Splittable, _: &Splittable| Ok(None);
    let mut away = BTreeSet::new();
    let scout = unit(9185, true, Position::new(42.047_367, 94.338_486, 0.0));
    let sheep = unit(9529, false, Position::new(45.5, 93.5, 0.0));
    assert!(
        !splits_together(
            &scout,
            &sheep,
            formations,
            JoinTest::Count,
            &mut none,
            &mut away,
            &tuning().order
        )
        .unwrap()
    );
    // nearer than 20 tiles the planner's path decides: its goal reached (1) within the
    // influence distance
    let other = unit(9186, true, Position::new(44.0, 94.0, 0.0));
    let mut short = |_: &Splittable, _: &Splittable| Ok(Some((Found::Goal, Distance::new(2.5))));
    assert!(
        splits_together(
            &scout,
            &other,
            formations,
            JoinTest::Count,
            &mut short,
            &mut away,
            &tuning().order
        )
        .unwrap()
    );
    let mut long = |_: &Splittable, _: &Splittable| Ok(Some((Found::Goal, Distance::new(3.5))));
    assert!(
        !splits_together(
            &scout,
            &other,
            formations,
            JoinTest::Count,
            &mut long,
            &mut away,
            &tuning().order
        )
        .unwrap()
    );
    let mut nearest =
        |_: &Splittable, _: &Splittable| Ok(Some((Found::Nearest, Distance::new(2.5))));
    assert!(
        !splits_together(
            &scout,
            &other,
            formations,
            JoinTest::Count,
            &mut nearest,
            &mut away,
            &tuning().order
        )
        .unwrap()
    );
    // a path that did not join puts the unit among the far ones; a NaN length joins
    assert_eq!(away, [object_id(9186)].into());
    let mut lost =
        |_: &Splittable, _: &Splittable| Ok(Some((Found::Goal, Distance::new(f32::NAN))));
    assert!(
        splits_together(
            &scout,
            &other,
            formations,
            JoinTest::Count,
            &mut lost,
            &mut away,
            &tuning().order
        )
        .unwrap()
    );
    // a NaN distance takes no path and is within the influence distance
    let nowhere = Splittable {
        moved_at: Point::new(f32::NAN, 94.0),
        ..other
    };
    assert!(
        splits_together(
            &scout,
            &nowhere,
            formations,
            JoinTest::Count,
            &mut short,
            &mut away,
            &tuning().order
        )
        .unwrap()
    );
    let far = unit(9187, true, Position::new(80.0, 94.0, 0.0));
    assert!(
        !splits_together(
            &scout,
            &far,
            formations,
            JoinTest::Count,
            &mut none,
            &mut away,
            &tuning().order
        )
        .unwrap()
    );
    let wide = Formations {
        influence: Distance::new(40.0),
        ..formations
    };
    assert!(
        splits_together(
            &scout,
            &far,
            wide,
            JoinTest::Count,
            &mut none,
            &mut away,
            &tuning().order
        )
        .unwrap()
    );
    // a member must stand in state 2 or more
    let born = Splittable {
        state: ObjectState::Foundation,
        ..far
    };
    assert!(
        !splits_together(
            &scout,
            &born,
            wide,
            JoinTest::Group,
            &mut none,
            &mut away,
            &tuning().order
        )
        .unwrap()
    );
}

#[test]
fn point_off_the_map_stays_inside_its_edge() {
    let point = Point::new(10.0, 10.0);
    assert_eq!(
        offset_point(
            Point::new(5.0, 15.0),
            point,
            Point::new(2.0, 118.0),
            GridSize::new(120, 120),
            &tuning().order
        ),
        Point::new(1.0, 119.0)
    );
}

/// `FUN_14161b970`'s timers: 750 ms for the first six entries of the list (the gone ones
/// count), the world's dt more for each six after.
#[test]
fn patrol_staggers_its_units_timeouts_by_sixes() {
    assert_eq!(patrol_timeout(0, Millis::new(100)), Millis::new(750));
    assert_eq!(patrol_timeout(5, Millis::new(100)), Millis::new(750));
    assert_eq!(patrol_timeout(6, Millis::new(100)), Millis::new(850));
    assert_eq!(patrol_timeout(13, Millis::new(33)), Millis::new(816));
    assert_eq!(patrol_timeout(499, Millis::ZERO), Millis::new(750));
}

/// `FUN_14161b970`'s filter: a waypoint less than a tile from the last one kept on both x
/// and y is passed over; a NaN difference does not count (`COMISS`/`JNC`).
#[test]
fn patrol_passes_over_waypoint_within_tile_of_the_last() {
    let points = [
        Point::new(10.5, 10.5),
        Point::new(11.0, 10.0),
        Point::new(11.5, 10.9),
        Point::new(11.5, 9.0),
        Point::new(f32::NAN, 9.0),
        Point::new(12.5, 9.0),
    ];
    assert_eq!(
        waypoints_from(Point::new(10.0, 10.0), &points, &tuning().order),
        [
            Point::new(11.0, 10.0),
            Point::new(11.5, 9.0),
            Point::new(12.5, 9.0)
        ]
    );
    assert_eq!(
        waypoints_from(Point::new(10.0, 10.0), &points[..1], &tuning().order),
        Vec::<Point>::new()
    );
}

/// Player 1's soldiers of `soldiers`, each walking and with a unit AI off any order, at a
/// world dt of 100 ms.
fn patrollers(units: &[(i32, i16, Point)], members: &[i32]) -> World {
    let mut world = soldiers(units, members);
    world.dt = Millis::new(100);
    for unit in &mut world.players[1].objects {
        world.parts.put_mover(unit.id, Some(walker()));
        world.parts.set_unit_ai(
            unit.id,
            Some(UnitAi {
                ai_kind: UnitAiKind::Soldier,
                current_order: None,
                current_action: None,
                current_target: None,
                target_class: None,
                idle_timer: Millis::new(400),
                adjusted_idle_timeout: Millis::new(2000),
                secondary_timer: Millis::new(3000),
                patrol_path: Some(PatrolPath::default()),
                patrol_waypoint: WaypointIndex::of_data(2),
                ..UnitAi::default()
            }),
        );
    }
    world
}

/// `FORM_FORMATION` (`FUN_14161dba0`) of three soldiers: the splitter makes one group of them, its
/// differentiator the command count, its formation locked to the command's type (5), the group
/// listed as its own differentiator group and the command count one higher.
#[test]
fn formation_of_soldiers_makes_one_group_locked_to_its_kind() {
    let units = [
        (11, 4, Point::new(20.0, 20.0)),
        (12, 4, Point::new(21.0, 20.0)),
        (13, 4, Point::new(22.0, 20.0)),
    ];
    let mut world = searchable(patrollers(&units, &[]));
    world
        .form_formation(
            player_id(1),
            &object_ids([11, 12, 13]),
            FormationKind::UserDefined,
        )
        .unwrap();
    let groups = &world.groups.groups;
    assert_eq!(groups.len(), 1);
    let ids: Vec<ObjectId> = groups[0].members.iter().map(|member| member.id).collect();
    assert_eq!(ids, [object_id(11), object_id(12), object_id(13)]);
    assert_eq!(
        groups[0].formation_locked_kind,
        Some(FormationKind::UserDefined)
    );
    assert_eq!(groups[0].differentiator_id, Differentiator::of_data(0));
    assert_eq!(groups[0].differentiator_groups, [groups[0].id]);
    assert_eq!(world.groups.commands, Differentiator::of_data(1));
    // the same units again: their group takes the command without a new one
    world
        .form_formation(
            player_id(1),
            &object_ids([11, 12, 13]),
            FormationKind::Kind6,
        )
        .unwrap();
    assert_eq!(world.groups.groups.len(), 1);
    assert_eq!(
        world.groups.groups[0].formation_locked_kind,
        Some(FormationKind::Kind6)
    );
    assert_eq!(
        world.groups.groups[0].differentiator_id,
        Differentiator::of_data(1)
    );
    assert_eq!(world.groups.commands, Differentiator::of_data(2));
}

/// A unit whose order is a defend or a follow stops first: its ignore list empties and its
/// order ends; any other unit's order stays. Units the world lacks make no group, and a human
/// whose units are all gone is refused (no command is counted).
#[test]
fn formation_stops_the_units_that_march_and_ignores_gone_ones() {
    let units = [
        (11, 4, Point::new(20.0, 20.0)),
        (12, 4, Point::new(21.0, 20.0)),
    ];
    let mut world = searchable(patrollers(&units, &[]));
    for (index, order) in [Some(OrderKind::Defend), Some(OrderKind::Patrol)]
        .into_iter()
        .enumerate()
    {
        let unit = &mut world.players[1].objects[ListIndex::new(index)];
        world.parts.unit_ai.get_mut(unit.id).unwrap().current_order = order;
        world.parts.mover_mut(unit.id).unwrap().ignore_units = vec![object_id(99)];
    }
    world
        .form_formation(
            player_id(1),
            &object_ids([11, 12]),
            FormationKind::UserDefined,
        )
        .unwrap();
    let marched = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(world.parts.mover_of(marched.id).unwrap().ignore_units, []);
    assert_ne!(
        world.parts.unit_ai.get(marched.id).unwrap().current_order,
        Some(OrderKind::Defend)
    );
    let other = &world.players[1].objects[ListIndex::new(1)];
    assert_eq!(
        world.parts.mover_of(other.id).unwrap().ignore_units,
        [object_id(99)]
    );
    // all gone: refused
    let before = world.groups.commands;
    world
        .form_formation(
            player_id(1),
            &object_ids([70, 71]),
            FormationKind::UserDefined,
        )
        .unwrap();
    assert_eq!(world.groups.commands, before);
}

/// MarvinBlue/426049701's first PATROL (update 25): player 1's scout alone, one waypoint,
/// `+0x8` set. It moves loose (slot 0x340: a forced, walking `MoveTo` to the waypoint at its
/// height), waits to move with an empty ignore list and no waypoints of its own; its unit AI
/// takes the patrol order, the move action and no target at the waypoint, its timers 0 and
/// 750, its saved patrol gone.
#[test]
fn lone_unit_patrols_to_its_waypoint() {
    let mut world = patrollers(&[(11, 4, Point::new(20.0, 20.0))], &[]);
    let at = world.players[1].objects[ListIndex::new(0)].position();
    world.players[1].objects[ListIndex::new(0)].place_for_test(at.with_z(1.5));
    world
        .patrol(
            player_id(1),
            &object_ids([11]),
            &[Point::new(30.0, 20.0)],
            SavedPatrols::Dropped,
        )
        .unwrap();
    let unit = &world.players[1].objects[ListIndex::new(0)];
    let move_to = world
        .parts
        .actions_of(unit.id)
        .list
        .move_to()
        .copied()
        .unwrap();
    assert_eq!(
        (
            move_to.goal(),
            ObjectId::field(move_to.target),
            move_to.range
        ),
        (Some(Position::new(30.0, 20.0, 1.5)), -1, Distance::ZERO)
    );
    let mover = world.parts.mover_of(unit.id).unwrap();
    assert_eq!(
        (
            mover.waiting,
            mover.ignore_units.len(),
            mover.user_waypoints.len()
        ),
        (1, 0, 0)
    );
    let ai = world.parts.unit_ai.get(unit.id).unwrap();
    assert_eq!(
        (ai.current_order, ai.last_order, ai.current_action),
        (Some(OrderKind::Patrol), None, Some(AiAction::Move))
    );
    assert_eq!(
        (ai.current_target, ai.target_class, ai.target_point),
        (None, None, Position::new(30.0, 20.0, 1.5))
    );
    assert_eq!(
        (ai.idle_timer, ai.adjusted_idle_timeout, ai.secondary_timer),
        (Millis::ZERO, Millis::new(750), Millis::new(750))
    );
    assert_eq!(
        (ai.patrol_path.is_none(), ai.patrol_waypoint.get()),
        (true, 0)
    );
    assert_eq!(world.groups.commands, Differentiator::of_data(1));
}

/// A lone unit's waypoints but the last become its own (slot 0x448, a whole coordinate moved
/// to the tile's middle); the move goes to the last one kept, the unit AI's point is the
/// command's last even where the filter left it out. Without `+0x8` the saved patrol stays.
#[test]
fn lone_unit_waypoints_but_the_last_become_its_own() {
    let mut world = patrollers(&[(11, 4, Point::new(20.0, 20.0))], &[]);
    let points = [
        Point::new(25.0, 20.0),
        Point::new(25.5, 20.5),
        Point::new(30.0, 20.0),
        Point::new(30.5, 20.5),
    ];
    world
        .patrol(player_id(1), &object_ids([11]), &points, SavedPatrols::Kept)
        .unwrap();
    let unit = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        world
            .parts
            .actions_of(unit.id)
            .list
            .move_to()
            .copied()
            .unwrap()
            .goal(),
        Some(Position::new(30.0, 20.0, 0.0))
    );
    assert_eq!(
        world.parts.mover_of(unit.id).unwrap().user_waypoints,
        [BVector::new(25.5, 0.0, 20.5)]
    );
    let ai = world.parts.unit_ai.get(unit.id).unwrap();
    assert_eq!(ai.target_point, Position::new(30.5, 20.5, 0.0));
    assert!(ai.patrol_path.is_some());
}

/// `FUN_14106d340`'s `FUN_140c5dc10` with a completion of the unit AI's current action
/// (no order) pending: the action stops (slot 0xb0 with 1: its value the last, none now) and
/// the unit's stopAction runs at once, before the move.
#[test]
fn move_runs_the_stop_of_pending_completion_first() {
    let mut world = patrollers(&[(11, 4, Point::new(20.0, 20.0))], &[]);
    let ai = world
        .parts
        .unit_ai
        .get_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap();
    ai.current_action = Some(AiAction::Move);
    ai.notifies = vec![Notify::own(
        object_id(11),
        NotifyKind::ACTION_COMPLETED,
        [AiAction::Move.value(), 0, 0],
    )];
    assert_eq!(
        world.move_to_point(
            PlayerId::new(1),
            ListIndex::new(0),
            Position::new(30.0, 20.0, 0.0)
        ),
        Ok(Answer::NotTaken)
    );
    let unit = &world.players[1].objects[ListIndex::new(0)];
    let ai = world.parts.unit_ai.get(unit.id).unwrap();
    assert_eq!(
        (ai.current_action, ai.last_action, ai.notifies.len()),
        (None, Some(AiAction::Move), 0)
    );
    assert_eq!(
        world
            .parts
            .actions_of(unit.id)
            .list
            .move_to()
            .copied()
            .unwrap()
            .goal(),
        Some(Position::new(30.0, 20.0, 0.0))
    );
}

/// The attack move of 428506340's update 940 (`+0x8` and `+0xa` set): player 1's lone unit
/// first resets its AI as for an order (`FUN_141631040`: its saved patrol goes even without
/// `+0x8`), then moves loose as a patrol's unit but unforced (a `MoveTo` already to the point
/// stands), its unit AI on the attack move order (`0x2dd`), the move action and no target at
/// the waypoint.
#[test]
fn lone_unit_attack_moves_to_its_waypoint() {
    let sweep = Sweep {
        patrol: false,
        patrols: SavedPatrols::Kept,
        queued: false,
        ignore_pending: true,
    };
    let mut world = patrollers(&[(11, 4, Point::new(20.0, 20.0))], &[]);
    // the reset redraws the adjusted idle timeout from it
    world
        .parts
        .unit_ai
        .get_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .idle_timeout = Millis::new(3000);
    world
        .attack_move(
            player_id(1),
            &object_ids([11]),
            &[Point::new(30.0, 20.0)],
            sweep,
        )
        .unwrap();
    let unit = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        world
            .parts
            .actions_of(unit.id)
            .list
            .move_to()
            .copied()
            .unwrap()
            .goal(),
        Some(Position::new(30.0, 20.0, 0.0))
    );
    let ai = world.parts.unit_ai.get(unit.id).unwrap();
    assert_eq!(
        (ai.current_order, ai.current_action, ai.target_point),
        (
            Some(OrderKind::AttackMove),
            Some(AiAction::Move),
            Position::new(30.0, 20.0, 0.0)
        )
    );
    assert_eq!(
        (ai.idle_timer, ai.adjusted_idle_timeout, ai.secondary_timer),
        (Millis::ZERO, Millis::new(750), Millis::new(750))
    );
    assert!(ai.patrol_path.is_none());
    // unforced: the MoveTo to the same point stands
    world
        .parts
        .actions
        .of_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .move_to_mut()
        .unwrap()
        .range = Distance::new(0.25);
    world
        .attack_move(
            player_id(1),
            &object_ids([11]),
            &[Point::new(30.0, 20.0)],
            sweep,
        )
        .unwrap();
    assert_eq!(
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .move_to()
            .copied()
            .unwrap()
            .range,
        Distance::new(0.25)
    );
    // a patrol's forced move makes a new one
    world
        .patrol(
            player_id(1),
            &object_ids([11]),
            &[Point::new(30.0, 20.0)],
            SavedPatrols::Kept,
        )
        .unwrap();
    assert_eq!(
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .move_to()
            .copied()
            .unwrap()
            .range,
        Distance::ZERO
    );
}

/// An attack move through the unit AI's queue (`+0x9`, or without `+0xa` an order in the
/// queue not ready) leaves the record's shown point and ready flag to the stack, which is
/// not modelled; with `+0xa` an unready order does not send it there.
#[test]
fn an_attack_move_through_the_unit_ai_queue_is_not_modelled() {
    let sweep = |queued, ignore_pending| Sweep {
        patrol: false,
        patrols: SavedPatrols::Dropped,
        queued,
        ignore_pending,
    };
    let patrollers = |units: &[(i32, i16, Point)]| {
        let mut world = patrollers(units, &[]);
        world
            .parts
            .unit_ai
            .get_mut(world.players[1].objects[ListIndex::new(0)].id)
            .unwrap()
            .idle_timeout = Millis::new(3000);
        world
    };
    let mut world = patrollers(&[(11, 4, Point::new(20.0, 20.0))]);
    assert_eq!(
        world.attack_move(
            player_id(1),
            &object_ids([11]),
            &[Point::new(30.0, 20.0)],
            sweep(true, true)
        ),
        Err(ATTACK_MOVE_QUEUED)
    );
    let unready = QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::Move,
        priority: OrderPriority::of_data(100),
        target: None,
        target_owner: None,
        point: Position::new(25.0, 20.0, 0.0),
        range: Distance::ZERO,
        shown: Point::ORIGIN,
        ready: false,
        target_kind: None,
    };
    let mut world = patrollers(&[(11, 4, Point::new(20.0, 20.0))]);
    world
        .parts
        .unit_ai
        .get_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .orders = vec![unready];
    assert_eq!(
        world.attack_move(
            player_id(1),
            &object_ids([11]),
            &[Point::new(30.0, 20.0)],
            sweep(false, false)
        ),
        Err(ATTACK_MOVE_QUEUED)
    );
    let mut world = patrollers(&[(11, 4, Point::new(20.0, 20.0))]);
    world
        .parts
        .unit_ai
        .get_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .orders = vec![unready];
    assert_eq!(
        world.attack_move(
            player_id(1),
            &object_ids([11]),
            &[Point::new(30.0, 20.0)],
            sweep(false, true)
        ),
        Ok(())
    );
}

/// Loose units of a PATROL (villagers, category 0) list each other in their ignore lists;
/// one whose waypoints all lie within a tile of it stays where it is, its order unchanged.
#[test]
fn loose_patrollers_ignore_each_other_and_one_at_its_waypoint_stays() {
    let mut world = patrollers(
        &[
            (11, 83, Point::new(20.0, 20.0)),
            (12, 83, Point::new(29.5, 20.0)),
        ],
        &[],
    );
    world
        .patrol(
            player_id(1),
            &object_ids([11, 12]),
            &[Point::new(30.0, 20.0)],
            SavedPatrols::Dropped,
        )
        .unwrap();
    let [first, second] = world.players[1].objects.as_slice() else {
        panic!("two units")
    };
    assert_eq!(
        world.parts.mover_of(first.id).unwrap().ignore_units,
        [object_id(12)]
    );
    assert_eq!(
        world.parts.mover_of(second.id).unwrap().ignore_units,
        [object_id(11)]
    );
    assert!(
        world.parts.actions_of(first.id).list.move_to().is_some()
            && world.parts.actions_of(second.id).list.move_to().is_none()
    );
    assert_eq!(
        world.parts.unit_ai.get(second.id).unwrap().current_order,
        None
    );
    assert_eq!(world.parts.mover_of(second.id).unwrap().waiting, 1);
}

/// `FUN_140fd9f40`: the splitter keeps group 5 (the order's 11 and 12 are all of it); the
/// loose 13 of type 70 with a formation category within the owner's influence of the
/// group's place joins it (flags bit 17), the villager does not, and a unit among the far
/// ones does not either.
#[test]
fn kept_group_takes_the_loose_soldiers_about_it() {
    let units = [
        (11, 4, Point::new(20.0, 20.0)),
        (12, 4, Point::new(21.0, 20.0)),
        (13, 4, Point::new(22.0, 20.0)),
        (14, 83, Point::new(21.0, 21.0)),
        (15, 4, Point::new(21.0, 19.0)),
    ];
    let mut world = soldiers(&units, &[11, 12]);
    for unit in &mut world.players[1].objects {
        let mut mover = walker();
        mover.moved_at = unit.position();
        world.parts.put_mover(unit.id, Some(mover));
    }
    world.groups.get_mut(group_id(5)).unwrap().motion.position[0] = BVector::new(21.0, 0.0, 20.0);
    let mut split = Split {
        owner: Some(PlayerId::new(1)),
        loose: object_id_list([13, 14, 15]),
        groups: vec![object_id_list([11, 12])],
        far: [object_id(15)].into(),
    };
    world.keep_group(&mut split, group_id(5), None).unwrap();
    assert_eq!(split.loose, object_id_list([14, 15]));
    let group = world.groups.get(group_id(5)).unwrap();
    assert!(group.has(object_id(13)));
    assert_eq!(group.flags & GroupFlags::JOINED, GroupFlags::JOINED);
    // the unit after one that joins is passed over: 13 joins, 15 slides into its place
    let mut world = soldiers(&units, &[11, 12]);
    world.groups.get_mut(group_id(5)).unwrap().motion.position[0] = BVector::new(21.0, 0.0, 20.0);
    let mut split = Split {
        owner: Some(PlayerId::new(1)),
        loose: object_id_list([13, 15]),
        ..Split::default()
    };
    world.keep_group(&mut split, group_id(5), None).unwrap();
    assert_eq!(split.loose, object_id_list([15]));
}

/// `FUN_140fd9f40` with a target the world holds: a loose soldier joins the kept group only when
/// the target is out of its line of sight (the squared gap on the ground above its master's line
/// of sight squared); one that sees it stays loose.
#[test]
fn kept_group_takes_only_the_soldiers_that_do_not_see_the_target() {
    let units = [
        (11, 4, Point::new(20.0, 20.0)),
        (12, 4, Point::new(21.0, 20.0)),
        (13, 4, Point::new(22.0, 20.0)),
    ];
    let world = |target_x: f32| {
        let mut soldiers = soldiers(&units, &[11, 12]);
        Arc::make_mut(&mut soldiers.model.masters[1])
            .get_mut(master_id(4))
            .unwrap()
            .line_of_sight = Distance::new(5.0);
        soldiers.model.masters[0] = Arc::new(Masters::new([Master {
            id: ids::HOUSE,
            kind: MasterKind::BUILDING,
            class: class::BUILDING,
            ..Master::default()
        }]));
        let _ = soldiers
            .push_live(
                PlayerId::new(0),
                Unit {
                    id: object_id(60),
                    place: Static::at(Position::new(target_x, 20.0, 0.0)),
                    life: Lifecycle {
                        master_id: ids::HOUSE,
                        state: ObjectState::Alive,
                        ..Lifecycle::default()
                    },
                    ..Unit::default()
                },
            )
            .unwrap();
        soldiers
            .groups
            .get_mut(group_id(5))
            .unwrap()
            .motion
            .position[0] = BVector::new(21.0, 0.0, 20.0);
        for unit in &mut soldiers.players[1].objects {
            let mut mover = walker();
            mover.moved_at = unit.position();
            soldiers.parts.put_mover(unit.id, Some(mover));
        }
        soldiers
    };
    for (target_x, joins) in [(30.0, true), (24.0, false)] {
        let mut target_world = world(target_x);
        let mut split = Split {
            owner: Some(PlayerId::new(1)),
            loose: object_id_list([13]),
            groups: vec![object_id_list([11, 12])],
            ..Split::default()
        };
        target_world
            .keep_group(&mut split, group_id(5), Some(object_id(60)))
            .unwrap();
        assert_eq!(split.loose.is_empty(), joins, "target at x {target_x}");
        assert_eq!(
            target_world
                .groups
                .get(group_id(5))
                .unwrap()
                .has(object_id(13)),
            joins
        );
    }
}

/// [`patrollers`]' two soldiers near each other in a world whose statistics search works.
fn two_patrollers() -> World {
    searchable(patrollers(
        &[
            (11, 4, Point::new(20.0, 20.0)),
            (12, 4, Point::new(21.0, 20.0)),
        ],
        &[],
    ))
}

/// `world` on a 60 by 60 map of open land, its sight's registry for the searches.
fn searchable(mut world: World) -> World {
    // the group's statistics search the members' surroundings
    let mut spatial = Spatial::with_tiles(
        GridSize::new(60, 60),
        Tiles::new(GridSize::new(60, 60), vec![1; 3600]),
    );
    let kind = TerrainKind {
        shown: true,
        flags: TerrainFlags::WATER,
        forest: false,
    };
    spatial.terrain = Some(Terrain::new(
        GridSize::new(60, 60),
        vec![TileTerrain::of_save(0, 255, -1); 3600],
        vec![kind],
        vec![Restriction::new(vec![1.0])],
    ));
    spatial.relations = ByPlayer::new(vec![relation_row([0, 3]), relation_row([0, 0])]);
    spatial.unit_diplomacy = ByPlayer::new(vec![relations([1, 4]), relations([0, 1])]);
    let mut visibility = Visibility::new(GridSize::new(60, 60), vec![], vec![], vec![], 0, vec![]);
    visibility.registry = Some(Registry::new(2, 65));
    spatial.sight = Some(visibility);
    world.spatial = spatial;
    world
}

/// An attack move of two soldiers near each other: their group's move to the last waypoint
/// holds (`commandMoveTo` with `param_6` 0, `param_8` 1): its path does not loop (bit 0)
/// and its flags bit 11 is set, where a patrol's loops and leaves it clear.
#[test]
fn an_attack_moving_group_holds_its_path_to_the_last_waypoint() {
    let sweep = Sweep {
        patrol: false,
        patrols: SavedPatrols::Dropped,
        queued: false,
        ignore_pending: true,
    };
    let mut world = two_patrollers();
    for unit in &mut world.players[1].objects {
        world.parts.unit_ai.get_mut(unit.id).unwrap().idle_timeout = Millis::new(3000);
    }
    world
        .attack_move(
            player_id(1),
            &object_ids([11, 12]),
            &[Point::new(25.0, 25.0), Point::new(30.0, 20.0)],
            sweep,
        )
        .unwrap();
    let groups = &world.groups.groups[0];
    let top = groups.motion.paths.last().unwrap();
    assert_eq!(
        (
            top.path_flags & PathFlags::LOOPS,
            groups.flags & GroupFlags::PATROL
        ),
        (PathFlags::NONE, GroupFlags::PATROL)
    );
    let mut world = two_patrollers();
    world
        .patrol(
            player_id(1),
            &object_ids([11, 12]),
            &[Point::new(25.0, 25.0), Point::new(30.0, 20.0)],
            SavedPatrols::Dropped,
        )
        .unwrap();
    let group = &world.groups.groups[0];
    let top = group.motion.paths.last().unwrap();
    assert_eq!(
        (
            top.path_flags & PathFlags::LOOPS,
            group.flags & GroupFlags::PATROL
        ),
        (PathFlags::LOOPS, GroupFlags::NONE)
    );
}

/// A PATROL of two soldiers near each other: the splitter's group becomes a new group of
/// player 1, listed (its differentiators the command count before and itself), with the
/// waypoints but the last as its own and a looping path to the last, back and forth from 5
/// tiles; the command count goes on.
#[test]
fn patrolling_group_walks_loop_through_its_waypoints() {
    let mut world = two_patrollers();
    let points = [Point::new(25.0, 25.0), Point::new(30.0, 20.0)];
    world
        .patrol(
            player_id(1),
            &object_ids([11, 12]),
            &points,
            SavedPatrols::Dropped,
        )
        .unwrap();
    assert_eq!(world.groups.groups.len(), 1);
    let group = &world.groups.groups[0];
    assert_eq!(
        (
            group.owner,
            group.has(object_id(11)),
            group.has(object_id(12))
        ),
        (Some(PlayerId::new(1)), true, true)
    );
    assert_eq!(
        (group.differentiator_id, group.current_command),
        (Differentiator::of_data(0), GroupCommandKind::MOVE)
    );
    let top = group.motion.paths.last().unwrap();
    assert_eq!(
        top.path_flags & (PathFlags::LOOPS | PathFlags::BACK_AND_FORTH),
        PathFlags::LOOPS | PathFlags::BACK_AND_FORTH
    );
    assert_eq!(top.points.len(), 3);
    assert_eq!(
        top.points[1..],
        [BVector::new(25.0, 0.0, 25.0), BVector::new(30.0, 0.0, 20.0)]
    );
    assert_eq!(world.groups.commands, Differentiator::of_data(1));
    let ai = world
        .parts
        .unit_ai
        .get(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap();
    assert_eq!(
        (ai.adjusted_idle_timeout, ai.patrol_path.is_none()),
        (Millis::new(750), true)
    );
}

/// MOVE (`FUN_140e67800`) of three soldiers in no group: the splitter groups them (each has
/// two joiners: the first by id leads), and a new group of their owner (`FUN_14152efc0`)
/// takes them, its statistics, its place and the command; with Ctrl held each is a group of
/// its own (its formation the line's lock) and the three list each other as their
/// differentiator groups.
#[test]
fn move_of_soldiers_in_no_group_forms_new_groups() {
    let units = [
        (11, 4, Point::new(20.0, 20.0)),
        (12, 4, Point::new(21.0, 20.0)),
        (13, 4, Point::new(22.0, 20.0)),
    ];
    let order = MoveOrder {
        target: None,
        point: Point::new(40.0, 40.0),
        flags: OrderFlags::empty(),
        control: ControlKey::Up,
    };
    let world = || searchable(patrollers(&units, &[]));
    let mut game = world();
    game.move_order(PlayerId::new(1), &object_ids([11, 12, 13]), &order)
        .unwrap();
    let groups = &game.groups.groups;
    assert_eq!(groups.len(), 1);
    let ids: Vec<ObjectId> = groups[0].members.iter().map(|member| member.id).collect();
    assert_eq!(ids, [object_id(11), object_id(12), object_id(13)]);
    assert_eq!(game.groups.commands, Differentiator::of_data(1));
    let mut world = world();
    let ctrl = MoveOrder {
        control: ControlKey::Held,
        ..order
    };
    world
        .move_order(PlayerId::new(1), &object_ids([11, 12, 13]), &ctrl)
        .unwrap();
    let groups = &world.groups.groups;
    assert_eq!(groups.len(), 3);
    for group in groups {
        assert_eq!(group.members.len(), 1);
        assert_eq!(
            group.differentiator_groups,
            [group_id(0), group_id(1), group_id(2)]
        );
    }
}

/// MOVE (`FUN_140e67800`) of the whole of group 5 and a loose soldier beside it: the
/// splitter keeps the group (`FUN_140fd9f40`), which takes the soldier and the command
/// (`BUnitGroup::command`); no new group is made.
#[test]
fn move_of_whole_group_keeps_it() {
    let units = [
        (11, 4, Point::new(20.0, 20.0)),
        (12, 4, Point::new(21.0, 20.0)),
        (13, 4, Point::new(22.0, 20.0)),
    ];
    let order = MoveOrder {
        target: None,
        point: Point::new(40.0, 40.0),
        flags: OrderFlags::empty(),
        control: ControlKey::Up,
    };
    let kept = || {
        let mut soldiers = soldiers(&units, &[11, 12]);
        soldiers
            .groups
            .get_mut(group_id(5))
            .unwrap()
            .motion
            .position[0] = BVector::new(21.0, 0.0, 20.0);
        soldiers
    };
    let mut world = kept();
    world
        .move_order(PlayerId::new(1), &object_ids([11, 12, 13]), &order)
        .unwrap();
    assert_eq!(world.groups.groups.len(), 1);
    let group = world.groups.get(group_id(5)).unwrap();
    assert!(group.has(object_id(13)));
    // the keep sets flags bit 17, which the group's command then clears
    assert_eq!(
        (group.flags & GroupFlags::JOINED, group.differentiator_id),
        (GroupFlags::NONE, Differentiator::of_data(0))
    );
    assert_eq!(world.groups.commands, Differentiator::of_data(1));
    let mut world = kept();
    // with a target the world holds the kept group takes the command on it (a unit of the
    // order itself, which has no task: the plain move to it)
    let at_12 = MoveOrder {
        target: Some(object_id(12)),
        ..order
    };
    assert!(
        world
            .move_order(PlayerId::new(1), &object_ids([11, 12, 13]), &at_12)
            .is_ok()
    );
}

/// [`crate::world::tests::forage_world`]'s forager 9175 carrying 6 berries (kind 17), its
/// Gather on bush 600 from a work command, its player holding 10 attributes.
fn foraging() -> World {
    let mut world = forage_world();
    let work = Work {
        target: Some(object_id(600)),
        point: Position::new(32.5, 89.5, 0.0),
        disable_retargeting: false,
        filter: None,
    };
    world
        .order_work(PlayerId::new(1), ListIndex::new(0), &work)
        .unwrap();
    world.players[1].attributes = vec![Amount::ZERO; 10];
    let unit = &mut world.players[1].objects[ListIndex::new(0)];
    (unit.life.attribute_kind, unit.life.amount) = (Some(AttributeId::named(17)), Amount::new(6.0));
    world
}

/// The drop-off order's ask (`FUN_14161e370`): the orders 7 and 8 ask a top Gather, Build or
/// Hunt, whose task's resource (berries, kind 0) asks for a load of its kind and not for wood
/// (kind 1); the order 8 to an action without a target or a point is unaimed; the order 6
/// and a unit without such an action ask nothing.
#[test]
fn drop_off_order_asks_the_top_gather_build_or_hunt() {
    use crate::orders::unit_order::{DropOffAsk, DropOffBy};
    let mut foraging = foraging();
    let ask = |by, asked, unaimed| Some(DropOffAsk { by, asked, unaimed });
    // the ask carries the top action it found
    let gather = |world: &World| {
        DropOffBy::Gather(
            *world
                .parts
                .actions_of(world.players[1].objects[ListIndex::new(0)].id)
                .list
                .gather()
                .unwrap(),
        )
    };
    assert_eq!(
        foraging
            .drop_off_ask(PlayerId::new(1), ListIndex::new(0), (true, true))
            .unwrap(),
        ask(gather(&foraging), true, false)
    );
    assert_eq!(
        foraging
            .drop_off_ask(PlayerId::new(1), ListIndex::new(0), (false, true))
            .unwrap(),
        None
    );
    foraging.players[1].objects[ListIndex::new(0)]
        .life
        .attribute_kind = Some(attribute::WOOD);
    assert_eq!(
        foraging
            .drop_off_ask(PlayerId::new(1), ListIndex::new(0), (true, false))
            .unwrap(),
        ask(gather(&foraging), false, false)
    );
    let gather_action = foraging
        .parts
        .actions
        .of_mut(foraging.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .gather_mut()
        .unwrap();
    (gather_action.target, gather_action.point) = (None, Position::new(NO_POINT, NO_POINT, 0.0));
    assert_eq!(
        foraging
            .drop_off_ask(PlayerId::new(1), ListIndex::new(0), (true, true))
            .unwrap(),
        ask(gather(&foraging), false, true)
    );
    assert_eq!(
        foraging
            .drop_off_ask(PlayerId::new(1), ListIndex::new(0), (true, false))
            .unwrap(),
        ask(gather(&foraging), false, false)
    );
    // a Hunt and a Build of a wood task, aimed
    let mut hunt = hunting(600);
    hunt.task.resource = Some(attribute::WOOD);
    let unit = &mut foraging.players[1].objects[ListIndex::new(0)];
    foraging
        .parts
        .actions
        .of_mut(unit.id)
        .unwrap()
        .list
        .set_gather(None);
    foraging
        .parts
        .actions
        .of_mut(unit.id)
        .unwrap()
        .list
        .set_hunt(Some(hunt));
    assert_eq!(
        foraging
            .drop_off_ask(PlayerId::new(1), ListIndex::new(0), (true, true))
            .unwrap(),
        ask(DropOffBy::Hunt(hunt), true, false)
    );
    let task = BuildTask {
        range: Distance::new(0.1),
        work_value: WorkValue::new(1.0),
        work_rate: WorkRate::new(1.0),
        resource: Some(attribute::WOOD),
        id: TaskId::of_task(-1),
        key: task_key(ActionKind::BUILD),
        sprites: TaskSprites::NONE,
    };
    let players = &mut foraging.players[1].objects[ListIndex::new(0)];
    let build = started(
        object_id(10028),
        Position::new(10.5, 10.5, 0.0),
        Distance::new(5.0),
        task,
    );
    foraging
        .parts
        .actions
        .of_mut(players.id)
        .unwrap()
        .list
        .set_build(Some(build));
    assert_eq!(
        foraging
            .drop_off_ask(PlayerId::new(1), ListIndex::new(0), (true, true))
            .unwrap(),
        ask(DropOffBy::Build(build), true, false)
    );
    let unit = &mut foraging.players[1].objects[ListIndex::new(0)];
    foraging
        .parts
        .actions
        .of_mut(unit.id)
        .unwrap()
        .list
        .set_build(None);
    foraging
        .parts
        .actions
        .of_mut(unit.id)
        .unwrap()
        .list
        .set_hunt(None);
    assert_eq!(
        foraging
            .drop_off_ask(PlayerId::new(1), ListIndex::new(0), (true, true))
            .unwrap(),
        None
    );
}

/// The drop-off order 7 to the forager's Gather: it carries its berries to the town centre
/// 700 (state 8), which counts it as a worker; a load of wood instead posts the other load
/// (0x21c) and leaves the Gather; the order 8 to an unaimed Gather records its work for the
/// town bell, drops off and queues the way back.
#[test]
fn drop_off_order_takes_gather_to_its_site_or_posts_the_other_load() {
    let mut world = foraging();
    world
        .unit_order(player_id(1), &object_ids([9175]), UnitOrderAction::DropOff)
        .unwrap();
    let gather = world
        .parts
        .actions_of(world.players[1].objects[ListIndex::new(0)].id)
        .list
        .gather()
        .copied()
        .expect("the Gather");
    assert_eq!(
        (gather.carry_to, gather.state.bare()),
        (
            Some(DropSiteId::new(object_id(700))),
            GatherState::Drop { move_to: None }
        )
    );
    assert_eq!(
        world.players[1].objects[ListIndex::new(2)].workers,
        WorkerCount::of_data(1)
    );
    let mut world = foraging();
    world.players[1].objects[ListIndex::new(0)]
        .life
        .attribute_kind = Some(attribute::WOOD);
    let before = world
        .parts
        .actions_of(world.players[1].objects[ListIndex::new(0)].id)
        .list
        .gather()
        .copied();
    world
        .unit_order(player_id(1), &object_ids([9175]), UnitOrderAction::DropOff)
        .unwrap();
    let players = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        world.parts.actions_of(players.id).list.gather().copied(),
        before
    );
    let ai = world
        .parts
        .unit_ai
        .get(players.id)
        .expect("the forager's unit AI");
    assert!(
        ai.notifies
            .iter()
            .any(|notify| notify.kind == NotifyKind::OTHER_LOAD),
        "{:?}",
        ai.notifies
    );
    let mut world = foraging();
    let gather = world
        .parts
        .actions
        .of_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .gather_mut()
        .unwrap();
    (gather.target, gather.point) = (None, Position::new(NO_POINT, NO_POINT, 0.0));
    world
        .unit_order(
            player_id(1),
            &object_ids([9175]),
            UnitOrderAction::DropOffAndBack,
        )
        .unwrap();
    let unit = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        world
            .parts
            .actions_of(unit.id)
            .list
            .gather()
            .map(|gather_action| (gather_action.carry_to, gather_action.state.bare())),
        Some((
            Some(DropSiteId::new(object_id(700))),
            GatherState::Drop { move_to: None }
        ))
    );
    assert_eq!(
        world.parts.unit_ai.get(unit.id).map(|ai| ai.orders.len()),
        Some(1)
    );
}

/// The drop-off order 7 to a Hunt carrying its load: it goes to the town centre 700 (state 8),
/// which counts it as a worker.
#[test]
fn drop_off_order_takes_hunt_to_its_site() {
    let mut world = foraging();
    let unit = &mut world.players[1].objects[ListIndex::new(0)];
    world
        .parts
        .actions
        .of_mut(unit.id)
        .unwrap()
        .list
        .set_gather(None);
    let mut hunt = hunting(600);
    hunt.task.resource = Some(AttributeId::named(17));
    // the sprites' table of the world holds none
    hunt.task.sprites = TaskSprites::NONE;
    world
        .parts
        .actions
        .of_mut(unit.id)
        .unwrap()
        .list
        .set_hunt(Some(hunt));
    world
        .unit_order(player_id(1), &object_ids([9175]), UnitOrderAction::DropOff)
        .unwrap();
    let hunt_action = world
        .parts
        .actions_of(world.players[1].objects[ListIndex::new(0)].id)
        .list
        .hunt()
        .copied()
        .expect("the Hunt");
    assert_eq!(
        (hunt_action.carry_to, hunt_action.state.bare()),
        (
            Some(DropSiteId::new(object_id(700))),
            HuntState::Drop { move_to: None }
        )
    );
    assert_eq!(
        world.players[1].objects[ListIndex::new(2)].workers,
        WorkerCount::of_data(1)
    );
}

/// The drop-off orders fail on a sleeping object and on a unit whose action the slice does not
/// hold; they pass over a unit not in state 2, below type 40 or carrying nothing.
#[test]
fn drop_off_order_fails_on_what_it_does_not_hold() {
    let mut world = foraging();
    world.set_unread(
        world.players[1].objects[ListIndex::new(0)].id,
        GenieError::unsupported("a test's action"),
    );
    assert_eq!(
        world
            .unit_order(player_id(1), &object_ids([9175]), UnitOrderAction::DropOff)
            .err()
            .map(|error| error.reason()),
        Some("a UNIT_ORDER drop-off of an action not held")
    );
    let mut world = foraging();
    world.players[1].objects[ListIndex::new(0)].life.amount = Amount::ZERO;
    let before = world.players[1].objects[ListIndex::new(0)].clone();
    world
        .unit_order(
            player_id(1),
            &object_ids([9175, 12345]),
            UnitOrderAction::DropOff,
        )
        .unwrap();
    assert_eq!(world.players[1].objects[ListIndex::new(0)], before);
}

/// The attack task's start ends where the game's start does ([`crate::combat::attack::Attack::command`]
/// on the soldier's step): with the wolf out of reach a soldier that may move posts 0x1fd to its
/// unit AI and turns to state 4, and one that cannot move (no unit AI) fails at once
/// (`set_state(13)`: its animation at speed 1, its standing sprite); with the wolf in reach a
/// soldier that may move starts its `MoveTo` arrived and shows its standing sprite.
#[test]
fn an_attack_task_start_ends_where_the_start_does() {
    use crate::combat::attack::AttackState;
    let started = |ai: bool, at: f32| {
        let (mut world, task) = work_attack(UnitStance::Aggressive);
        let unit = &mut world.players[1].objects[ListIndex::new(0)];
        if !ai {
            world.parts.set_unit_ai(unit.id, None);
        }
        world.parts.set_anim_speed(unit.id, Speed::new(0.5));
        let mut wolf = *world.spatial.get(object_id(50)).unwrap();
        wolf.position = Position::new(at, 10.0, 0.0);
        world.spatial.insert(object_id(50), wolf);
        world.players[0].objects[ListIndex::new(0)].place =
            Static::at(Position::new(at, 10.0, 0.0));
        world
            .start_attack(
                PlayerId::new(1),
                ListIndex::new(0),
                object_id(50),
                &task,
                Position::new(at, 10.0, 0.0),
                Notifies::Cleared,
            )
            .unwrap();
        let unit = &world.players[1].objects[ListIndex::new(0)];
        let state = world
            .parts
            .actions_of(unit.id)
            .list
            .attack()
            .map(|attack| attack.state.bare());
        let notifies = world.parts.unit_ai.get(unit.id).map(|ai| ai.notifies.len());
        let sprite = unit.sprite.map(|active_sprite| active_sprite.graphic.id);
        (state, world.parts.anim_speed_of(unit.id), sprite, notifies)
    };
    let (near, _, sprite, near_notifies) = started(true, 10.4);
    assert_eq!(sprite, Some(SpriteId::named(300)));
    let (far, _, _, far_notifies) = started(true, 30.0);
    assert_eq!(
        (far, far_notifies),
        (
            Some(AttackState::Move { move_to: None }),
            near_notifies.map(|count| count + 1)
        )
    );
    assert_ne!(near, None);
    let (failed, speed, sprite, _) = started(false, 30.0);
    assert_eq!(
        (failed, speed, sprite),
        (
            Some(AttackState::Failed),
            Some(Speed::new(1.0)),
            Some(SpriteId::named(300))
        )
    );
}

/// The work command's task (`FUN_14106ff70`): none on a point; the forager's Gather task 2 on
/// Gaia's bush (its master 120); none on its town centre, which no task of it names.
#[test]
fn work_command_finds_its_master_and_task_on_target() {
    let world = forage_world();
    assert_eq!(
        world
            .work_task(
                PlayerId::new(1),
                ListIndex::new(0),
                (None, Position::from([1.0; 3])),
                None
            )
            .unwrap(),
        None
    );
    let (with, task) = world
        .work_task(
            PlayerId::new(1),
            ListIndex::new(0),
            (Some(object_id(600)), Position::from([1.0; 3])),
            None,
        )
        .unwrap()
        .expect("the Gather task");
    assert_eq!(
        (with, task.id, task.action_kind),
        (master_id(120), task_id(2), GATHER)
    );
    assert_eq!(
        world
            .work_task(
                PlayerId::new(1),
                ListIndex::new(0),
                (Some(object_id(700)), Position::from([1.0; 3])),
                None
            )
            .unwrap(),
        None
    );
}

/// A work command on the other bush retargets the forager's Gather on the same task, and on
/// the other deer the hunter's Hunt: the unit AI hears the gather or hunt order on it.
#[test]
fn work_command_retargets_gather_or_hunt_on_its_task() {
    let mut forage_world = forage_world();
    assert_eq!(
        forage_world.gather_command(
            PlayerId::new(1),
            ListIndex::new(0),
            object_id(600),
            OrderForce::Forced
        ),
        Ok(Answer::Taken)
    );
    let took = forage_world
        .retarget_gather_work(
            PlayerId::new(1),
            ListIndex::new(0),
            Some(object_id(601)),
            Position::new(36.5, 89.5, 0.0),
            Notifies::Cleared,
        )
        .unwrap();
    let players = &forage_world.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        (
            took,
            forage_world
                .parts
                .actions_of(players.id)
                .list
                .gather()
                .map(|gather_action| gather_action.target)
        ),
        (
            Answer::Taken,
            Some(Some(GatherTargetId::new(object_id(601))))
        )
    );
    let ai = forage_world.parts.unit_ai.get(players.id).unwrap();
    assert_eq!(
        (ai.current_order, ai.current_action, ai.current_target),
        (
            Some(OrderKind::Gather),
            Some(AiAction::Gather),
            Some(object_id(601))
        )
    );
    let mut world = hunt_world();
    assert_eq!(
        world.hunt_command(
            PlayerId::new(1),
            ListIndex::new(0),
            Some(object_id(500)),
            OrderForce::Forced
        ),
        Ok(Answer::Taken)
    );
    world
        .retarget_hunt_work(
            PlayerId::new(1),
            ListIndex::new(0),
            (
                object_id(501),
                Position::new(36.5, 89.5, 0.0),
                Notifies::Cleared,
            ),
        )
        .unwrap();
    let unit = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        world
            .parts
            .actions_of(unit.id)
            .list
            .hunt()
            .map(|hunt_action| hunt_action.target),
        Some(Some(HuntTargetId::new(object_id(501))))
    );
    let ai = world.parts.unit_ai.get(unit.id).unwrap();
    assert_eq!(
        (ai.current_order, ai.current_action, ai.current_target),
        (
            Some(OrderKind::Hunt),
            Some(AiAction::Hunt),
            Some(object_id(501))
        )
    );
}

/// A Gather or a Hunt sent on to an object of its own player's master 84 is not modelled.
#[test]
fn work_command_retarget_to_its_player_master_84_fails() {
    let with_84 = |mut world: World| {
        for masters in &mut world.model.masters {
            let m84 = Master {
                id: ids::MARKET,
                kind: MasterKind::COMBAT,
                class: class::PREY,
                ..Master::default()
            };
            *masters = Arc::new(Masters::new(masters.iter().cloned().chain([m84])));
        }
        let _ = world
            .push_live(
                PlayerId::new(1),
                Unit {
                    id: object_id(800),
                    place: Static::at(Position::new(33.5, 89.5, 0.0)),
                    life: Lifecycle {
                        master_id: ids::MARKET,
                        state: ObjectState::Alive,
                        hp: HitPoints::new(5.0),
                        ..Lifecycle::default()
                    },
                    ..Unit::default()
                },
            )
            .unwrap();
        world
    };
    let mut world = with_84(forage_world());
    assert_eq!(
        world.gather_command(
            PlayerId::new(1),
            ListIndex::new(0),
            object_id(600),
            OrderForce::Forced
        ),
        Ok(Answer::Taken)
    );
    let answer = world.retarget_gather_work(
        PlayerId::new(1),
        ListIndex::new(0),
        Some(object_id(800)),
        Position::new(33.5, 89.5, 0.0),
        Notifies::Cleared,
    );
    assert_eq!(
        answer.err().map(|error| error.reason()),
        Some("a gather sent to its player's master 84")
    );
    let mut world = with_84(hunt_world());
    assert_eq!(
        world.hunt_command(
            PlayerId::new(1),
            ListIndex::new(0),
            Some(object_id(500)),
            OrderForce::Forced
        ),
        Ok(Answer::Taken)
    );
    let retarget_hunt_work = world.retarget_hunt_work(
        PlayerId::new(1),
        ListIndex::new(0),
        (
            object_id(800),
            Position::new(33.5, 89.5, 0.0),
            Notifies::Cleared,
        ),
    );
    assert_eq!(
        retarget_hunt_work.err().map(|error| error.reason()),
        Some("a hunt sent to its player's master 84")
    );
}

/// The retake of the work's task by the unit's top action: a Gather on the same task keeps
/// the work (retargeted); a base action type on top of the task's type answers 0 (the factory
/// follows); another current task, or a Build of the unit's former master, is not modelled;
/// no action on top answers 0.
#[test]
fn work_command_task_is_retaken_by_the_action_on_top() {
    use crate::objects::masters::Task;
    let mut world = forage_world();
    let task = *world.model.masters[1]
        .get(master_id(120))
        .unwrap()
        .action_part()
        .tasks
        .first()
        .unwrap();
    let at = (
        Some(object_id(601)),
        Position::new(36.5, 89.5, 0.0),
        Notifies::Cleared,
    );
    assert_eq!(
        world.retake_task(
            PlayerId::new(1),
            ListIndex::new(0),
            at,
            &task,
            MasterSwap::Keep
        ),
        Ok(Answer::NotTaken)
    );
    assert_eq!(
        world.gather_command(
            PlayerId::new(1),
            ListIndex::new(0),
            object_id(600),
            OrderForce::Forced
        ),
        Ok(Answer::Taken)
    );
    assert_eq!(
        world.retake_task(
            PlayerId::new(1),
            ListIndex::new(0),
            at,
            &task,
            MasterSwap::Keep
        ),
        Ok(Answer::Taken)
    );
    let other = Task {
        id: task_id(9),
        ..task
    };
    assert_eq!(
        world
            .retake_task(
                PlayerId::new(1),
                ListIndex::new(0),
                at,
                &other,
                MasterSwap::Keep
            )
            .err()
            .map(|error| error.reason()),
        Some("a work command on the unit's current task")
    );
    let mut world = forage_world();
    world
        .parts
        .actions
        .of_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .set_move_to(Some(MoveToAction {
            sprite: None,
            state: MoveTo::Moving,
            point: Position::new(40.0, 89.5, 0.0),
            target: None,
            range: Distance::ZERO,
            voice: Voice::Loud,
            on_target_moved: TargetMoved::Follows,
            wait: MoveWait::default(),
        }));
    let moving = Task {
        action_kind: MOVE_TO,
        ..task
    };
    assert_eq!(
        world.retake_task(
            PlayerId::new(1),
            ListIndex::new(0),
            at,
            &moving,
            MasterSwap::Keep
        ),
        Ok(Answer::NotTaken)
    );
    let mut world = forage_world();
    let build = BuildTask {
        range: Distance::new(0.1),
        work_value: WorkValue::new(1.0),
        work_rate: WorkRate::new(1.0),
        resource: None,
        id: task_id(3),
        key: task_key(ActionKind::BUILD),
        sprites: TaskSprites::NONE,
    };
    world
        .parts
        .actions
        .of_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .set_build(Some(started(
            object_id(10028),
            Position::new(30.0, 88.0, 0.0),
            Distance::new(1.0),
            build,
        )));
    let building = Task {
        action_kind: ActionKind::BUILD,
        ..task
    };
    assert_eq!(
        world
            .retake_task(
                PlayerId::new(1),
                ListIndex::new(0),
                at,
                &building,
                MasterSwap::Change
            )
            .err()
            .map(|error| error.reason()),
        Some("a work command on the Build task of the unit's former master")
    );
}

/// The action list's factory for the work's task: the Gather task starts a Gather on the
/// bush; a task of an action type the slice does not make is not modelled.
#[test]
fn work_command_factory_starts_the_task_action() {
    use crate::objects::masters::Task;
    let mut world = forage_world();
    let task = *world.model.masters[1]
        .get(master_id(120))
        .unwrap()
        .action_part()
        .tasks
        .first()
        .unwrap();
    world
        .start_task(
            PlayerId::new(1),
            ListIndex::new(0),
            (Some(object_id(600)), Position::new(32.5, 89.5, 0.0)),
            &task,
            Notifies::Cleared,
        )
        .unwrap();
    assert_eq!(
        world
            .parts
            .actions_of(world.players[1].objects[ListIndex::new(0)].id)
            .list
            .gather()
            .map(|gather_action| (gather_action.target, gather_action.state.bare())),
        Some((
            Some(GatherTargetId::new(object_id(600))),
            GatherState::Walk { move_to: None }
        ))
    );
    let unknown = Task {
        action_kind: ActionKind::named(0x55),
        ..task
    };
    assert_eq!(
        world
            .start_task(
                PlayerId::new(1),
                ListIndex::new(0),
                (Some(object_id(600)), Position::new(32.5, 89.5, 0.0)),
                &unknown,
                Notifies::Cleared
            )
            .err()
            .map(|error| error.reason()),
        Some("the action factory for a task the slice does not model")
    );
}

/// INTERACT (`FUN_140e68b90`) of three soldiers on an enemy building: the splitter groups them,
/// a new group of their owner takes them, its statistics and its place, the command count as its
/// differentiator and the order: the soldiers' attack task (type 7) on the building is the
/// group's command 9 on it, and the command count goes on. The soldiers are in the group, none
/// is loose.
#[test]
fn an_interact_of_soldiers_on_an_enemy_building_forms_group_that_attacks_it() {
    let units = [
        (11, 4, Point::new(20.0, 20.0)),
        (12, 4, Point::new(21.0, 20.0)),
        (13, 4, Point::new(22.0, 20.0)),
    ];
    let mut world = searchable(patrollers(&units, &[]));
    world.players.push(Player::default());
    world.players[2].objects = vec![Unit {
        id: object_id(60),
        place: Static::at(Position::new(40.0, 40.0, 0.0)),
        flags: UnitFlags::ATTACKABLE,
        life: Lifecycle {
            master_id: ids::HOUSE,
            state: ObjectState::Alive,
            hp: HitPoints::new(550.0),
            ..Lifecycle::default()
        },
        ..Unit::default()
    }]
    .into();
    world.reindex();
    let building = Master {
        id: ids::HOUSE,
        kind: MasterKind::BUILDING,
        class: class::BUILDING,
        radius: Footprint::new(2.0, 2.0),
        hit_points: MasterHitPoints::of_data(550),
        ..Master::default()
    };
    let soldier = Master {
        action: Some(ActionPart {
            tasks: vec![Task {
                action_kind: ActionKind::COMBAT,
                owner_kind: TargetOwner::ENEMY,
                object_group: Some(class::BUILDING),
                ..Task::default()
            }],
            ..world.model.masters[1]
                .get(master_id(4))
                .unwrap()
                .action
                .clone()
                .unwrap_or_default()
        }),
        ..world.model.masters[1].get(master_id(4)).unwrap().clone()
    };
    world.model.masters[1] = Arc::new(Masters::new([soldier, building.clone()]));
    world.model.masters.push(Arc::new(Masters::new([building])));
    for unit in &mut world.players[1].objects {
        unit.life.hp = HitPoints::new(25.0);
    }
    let order = Interact {
        target: Some(object_id(60)),
        point: Point::new(40.0, 40.0),
        disable_retargeting: false,
        flags: OrderFlags::DIRECT | OrderFlags::HUMAN,
        filter: None,
    };
    world
        .interact(PlayerId::new(1), &object_ids([11, 12, 13]), &order)
        .unwrap();
    let groups = &world.groups.groups;
    assert_eq!(groups.len(), 1);
    let ids: Vec<ObjectId> = groups[0].members.iter().map(|member| member.id).collect();
    assert_eq!(ids, [object_id(11), object_id(12), object_id(13)]);
    assert_eq!(
        (groups[0].current_command, groups[0].current_target.target),
        (GroupCommandKind::ATTACK, GroupTarget::Object(object_id(60)))
    );
    assert_eq!(world.groups.commands, Differentiator::of_data(1));
}

/// The graphic of sprite `id`: ten frames of 0.1 s, eight facets.
const fn eight_facet_sprite(id: i16) -> Graphic {
    Graphic {
        id: SpriteId::named(id),
        frames: Frame::of_data(10),
        facets: FacetCount::of_data(8),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::ANIMATED,
    }
}

/// A walking combat master `id` of 40 hit points standing with sprite `standing` and walking
/// with `walk_sprite`.
fn sprited_walker(id: i16, standing: i16, walk_sprite: i16) -> Master {
    Master {
        id: master_id(id),
        kind: MasterKind::COMBAT,
        hit_points: MasterHitPoints::of_data(40),
        standing_sprite: Some(SpriteId::named(standing)),
        moving: Some(MovingPart {
            move_sprite: Some(SpriteId::named(walk_sprite)),
            ..MovingPart::default()
        }),
        ..moving_master(walking(Speed::new(1.0)))
    }
}

/// Object 7, a 4 that stands with sprite 1, walks with 2 and shows 2, with a `MoveTo` in `state`
/// showing 2; and object 8, a 5 that stands with 11 and walks with 12. The master change of
/// object 7 to 5 with `actions`: the sprites the unit and its `MoveTo` show.
fn changed_master_shows(state: MoveTo, actions: ActionsOnChange) -> [Option<SpriteId>; 2] {
    let moving = MoveToAction {
        sprite: Some(eight_facet_sprite(2)),
        ..MoveToAction::sub_in(state)
    };
    let mut scene = Scene::open(GridSize::new(8, 8));
    for id in [1, 2, 11, 12] {
        scene = scene.graphic(eight_facet_sprite(id));
    }
    let mut world = scene
        .object(SceneObject {
            actions: Some(Action {
                list: ActionList::default().with_move_to(Some(moving)),
                ..Action::default()
            }),
            sprite: Some(SpriteId::named(2)),
            ..SceneObject::new(
                7,
                PLAYER,
                sprited_walker(4, 1, 2),
                Position::new(2.5, 2.5, 0.0),
            )
        })
        .object(SceneObject::new(
            8,
            PLAYER,
            sprited_walker(5, 11, 12),
            Position::new(5.5, 5.5, 0.0),
        ))
        .build();
    let Some(Place::Live(player, idx)) = world.place(object_id(7)) else {
        panic!("object 7 lives");
    };
    world
        .change_master(player, idx, master_id(5), actions)
        .unwrap();
    let unit = world.object(object_id(7)).unwrap().1;
    let move_to = world.parts.actions_of(object_id(7)).list.move_to();
    [
        unit.sprite.map(|active_sprite| active_sprite.graphic.id),
        move_to
            .and_then(|move_to_action| move_to_action.sprite)
            .map(|graphic| graphic.id),
    ]
}

/// `FUN_141225b70` with 1 (a hit point transform's): the unit keeps its actions, each taking
/// the new master's sprites (`FUN_140a979b0`); a `MoveTo` that moves (state 4) shows its new
/// sprite (slot 0x60), which the unit then keeps; one that waits only takes it. With 0 the list
/// goes and the unit shows the new standing sprite.
#[test]
fn master_change_keeping_actions_shows_the_new_master_walking_sprite() {
    let sprite = |id| Some(SpriteId::named(id));
    assert_eq!(
        changed_master_shows(MoveTo::Moving, ActionsOnChange::Keep),
        [sprite(12), sprite(12)]
    );
    assert_eq!(
        changed_master_shows(MoveTo::Waiting, ActionsOnChange::Keep),
        [sprite(2), sprite(12)]
    );
    assert_eq!(
        changed_master_shows(MoveTo::Moving, ActionsOnChange::Clear),
        [sprite(11), None]
    );
}
