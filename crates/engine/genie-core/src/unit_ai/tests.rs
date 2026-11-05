use super::*;
use crate::GenieResult;
use crate::combat::AttackTargetId;
use crate::combat::Firepower;
use crate::combat::attack::ATTACK;
use crate::combat::attack::AttackState;
use crate::combat::attack::tests::own_attack;
use crate::construction::FoundationId;
use crate::construction::MainBuildingId;
use crate::construction::RepairTargetId;
use crate::construction::repair::REPAIR;
use crate::construction::wait_solidify::{WAIT_SOLIDIFY, WaitSolidify};
use crate::garrison::ContainerId;
use crate::garrison::transport::TransportState;
use crate::gathering::DropSiteId;
use crate::gathering::GatherTargetId;
use crate::gathering::PastureId;
use crate::gathering::gather::GATHER;
use crate::gathering::gather::GatherState;
use crate::gathering::hunt::HUNT;
use crate::groups::{GroupCommandKind, GroupFlags, Member, Motion, UnitGroup};
use crate::map::DistanceSquared;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Radii;
use crate::map::RestrictionId;
use crate::map::Tile;
use crate::map::spatial::TileSight;
use crate::math::fmath::BVector;
use crate::math::fmath::sqrt;
use crate::math::rng::Stream;
use crate::movement::MoveStats;
use crate::movement::PathData;
use crate::movement::PathFlags;
use crate::movement::Speed;
use crate::movement::Velocity;
use crate::movement::move_to::{MOVE_TO, MoveTo, MoveToAction};
use crate::objects::ActionState;
use crate::objects::HitPoints;
use crate::objects::MasterHitPoints;
use crate::objects::ObjectState;
use crate::objects::WorkerCount;
use crate::objects::actions::ActionList;
use crate::objects::layers::Action;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ActionPart;
use crate::objects::masters::AttackPriority;
use crate::objects::masters::AutoGatherMode;
use crate::objects::masters::BlastLevel;
use crate::objects::masters::BuildingMaster;
use crate::objects::masters::ChargeKind;
use crate::objects::masters::ClassAmount;
use crate::objects::masters::ClassId;
use crate::objects::masters::CombatLevel;
use crate::objects::masters::CombatPart;
use crate::objects::masters::CreatablePart;
use crate::objects::masters::GarrisonCapacity;
use crate::objects::masters::GarrisonKinds;
use crate::objects::masters::GatherGroup;
use crate::objects::masters::HeroFlags;
use crate::objects::masters::MasterField;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObjectFlags;
use crate::objects::masters::TargetOwner;
use crate::objects::masters::WorkValue;
use crate::objects::masters::{Master, Task};
use crate::objects::sprite::Graphic;
use crate::objects::unit::UnitFlags;
use crate::objects::{FacetCount, Frame, SpeedMultiplier, SpriteFlags, SpriteId};
use crate::orders::Dispatch;
use crate::orders::GARRISON;
use crate::orders::PACK_OR_UNPACK;
use crate::ownership::CaptureKind;
use crate::ownership::artifact::Artifact;
use crate::players::Amount;
use crate::players::Chance;
use crate::players::PlayerKind;
use crate::players::Posture;
use crate::players::Relation;
use crate::players::UnitRelation;
use crate::players::attribute::AttributeId;
use crate::players::tracker::Key;
use crate::test_ids::attack_category;
use crate::test_ids::class_id;
use crate::test_ids::class_of;
use crate::test_ids::group_id;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::test_ids::order_kind;
use crate::test_ids::owner;
use crate::test_ids::player_id;
use crate::testing::ai::run_ai;
use crate::testing::attribute;
use crate::testing::class;
use crate::testing::ids;
use crate::testing::ids::WONDER;
use crate::testing::roles;
use crate::testing::scene::SceneObject;
use crate::testing::scene::{Scene, walking};
use crate::testing::slots;
use crate::testing::tuning;
use crate::unit_ai::AlertMode;
use crate::unit_ai::AttackTargetFlags;
use crate::unit_ai::GroupState;
use crate::unit_ai::Notifies;
use crate::unit_ai::OrderForce;
use crate::unit_ai::OrderHistory;
use crate::unit_ai::OrderKind;
use crate::unit_ai::OrderTest;
use crate::unit_ai::Pace;
use crate::unit_ai::QueueEnd;
use crate::unit_ai::StopOrder;
use crate::unit_ai::SurroundingFlags;
use crate::unit_ai::TakenAs;
use crate::unit_ai::TargetRules;
use crate::unit_ai::attack_orders::OrderNow;
use crate::unit_ai::flight::BadPoint;
use crate::unit_ai::task_finder::DropSite;
use crate::unit_ai::update::UpdateTail;
use crate::unit_ai::world_view::UnitView;
use crate::vision::VisionBits;
use crate::world::AiSwitches;
use crate::world::Mode680;
use crate::world::World;
use std::slice::from_ref;
use std::sync::Arc;

/// Surroundings with nothing in them but build 158041's roles and slots.
fn no_surroundings() -> Surroundings {
    Surroundings {
        roles: roles(),
        slots: slots(),
        unit_ai_tuning: tuning().unit_ai,
        ..Surroundings::default()
    }
}

fn idle(ai_kind: UnitAiKind) -> UnitAi {
    UnitAi {
        ai_kind,
        current_order: None,
        current_action: None,
        current_target: None,
        adjusted_idle_timeout: Millis::new(3000),
        look_around_timeout: Millis::new(1000),
        retarget_timer: Millis::new(900),
        best_this_update: None,
        ..UnitAi::default()
    }
}

fn rng() -> Well512 {
    let mut rng = Well512::default();
    rng.set_seed(0);
    rng
}

/// The builder's flood: of two foundations two tiles off, the one left of the unit comes before
/// the one above it (the neighbours go left, right, below, above); one at the edge of the reach
/// (`r − 1` tiles) is tested but its tile is not spread from, one beyond is never reached.
#[test]
fn the_builder_flood_tests_left_right_below_above() {
    let at = Tile::new(50, 50);
    let found = flood_search(at, 8, |tile: Tile| {
        Ok(match (tile.x(), tile.y()) {
            (48, 50) => Some(object_id(1)),
            (50, 52) => Some(object_id(2)),
            _ => None,
        })
    });
    assert_eq!(found, Ok(Some(object_id(1))));
    let mut tested = Vec::new();
    assert_eq!(
        flood_search(at, 3, |tile: Tile| {
            tested.push((tile.x(), tile.y()));
            Ok(None)
        }),
        Ok(None)
    );
    // r = 3: tiles less than 2 from the start spread: the 3x3 square, then its neighbours
    assert_eq!(tested.len(), 1 + 4 + 4 + 12);
    assert!(tested.contains(&(52, 50)) && !tested.contains(&(53, 50)));
    assert_eq!(
        &tested[..5],
        &[(50, 50), (49, 50), (51, 50), (50, 49), (50, 51)]
    );
}

/// The priest's slot 0x1b8 (`FUN_140c1af00`): faith asked unless `any`; never Gaia, sheep,
/// town centres, walls or a master whose hero flag forbids it; the convert task's attribute
/// (`+0x26`) must be above 0 for the object's class.
#[test]
fn priest_converts_by_its_faith_its_task_and_the_object() {
    let attributes = {
        let mut amounts = vec![Amount::ZERO; 100];
        amounts[67] = Amount::new(1.0);
        amounts[28] = Amount::ZERO;
        amounts
    };
    let tasks = [
        Task {
            action_kind: ActionKind::CONVERT,
            ..Task::default()
        },
        Task {
            action_kind: ActionKind::CONVERT,
            object_group: Some(class_id(3)),
            attribute_kind4: Some(AttributeId::named(28)),
            ..Task::default()
        },
    ];
    let monk = Converter {
        class: class::MONK,
        master: master_id(125),
        faith: Amount::new(100.0),
        attributes: &attributes,
        tasks: &tasks,
    };
    let knight = Convertee {
        owner: PlayerId::new(2),
        class: class::CLASS_12,
        master: master_id(38),
        hero: HeroFlags::NONE,
    };
    assert!(can_convert(
        &monk,
        Some(&knight),
        TargetRules::Own,
        &roles(),
        &tuning().unit_ai
    ));
    let tired = Converter {
        faith: Amount::new(99.0),
        ..monk
    };
    assert!(!can_convert(
        &tired,
        Some(&knight),
        TargetRules::Own,
        &roles(),
        &tuning().unit_ai
    ));
    assert!(can_convert(
        &tired,
        Some(&knight),
        TargetRules::Any,
        &roles(),
        &tuning().unit_ai
    ));
    assert!(!can_convert(
        &monk,
        None,
        TargetRules::Any,
        &roles(),
        &tuning().unit_ai
    ));
    let gaia = Convertee {
        owner: PlayerId::new(0),
        ..knight
    };
    assert!(!can_convert(
        &monk,
        Some(&gaia),
        TargetRules::Any,
        &roles(),
        &tuning().unit_ai
    ));
    let sheep = Convertee {
        class: class::LIVESTOCK,
        ..knight
    };
    assert!(!can_convert(
        &monk,
        Some(&sheep),
        TargetRules::Any,
        &roles(),
        &tuning().unit_ai
    ));
    let tc = Convertee {
        class: class::BUILDING,
        master: ids::TOWN_CENTRE,
        ..knight
    };
    assert!(!can_convert(
        &monk,
        Some(&tc),
        TargetRules::Any,
        &roles(),
        &tuning().unit_ai
    ));
    let hero = Convertee {
        hero: HeroFlags::FULL_HERO,
        ..knight
    };
    assert!(!can_convert(
        &monk,
        Some(&hero),
        TargetRules::Any,
        &roles(),
        &tuning().unit_ai
    ));
    // a building's task names attribute 28, which the owner holds at 0
    let house = Convertee {
        class: class::BUILDING,
        master: ids::HOUSE,
        ..knight
    };
    assert!(!can_convert(
        &monk,
        Some(&house),
        TargetRules::Any,
        &roles(),
        &tuning().unit_ai
    ));
}

/// `FUN_140c42080`: an order joins the history first; a ninth recycles the oldest.
#[test]
fn the_order_history_keeps_the_eight_newest_first() {
    let mut ai = UnitAi::default();
    for index in 0..10 {
        ai.remember_order(OrderRecord {
            order: Some(OrderKind::Attack),
            action: Some(AiAction::Attack),
            time: WorldTime::from_millis(index),
            target: ObjectId::of_field(100 + index as i32),
            category: attack_category(1),
            ..OrderRecord::default()
        });
    }
    let targets: Vec<Option<ObjectId>> = ai
        .order_history
        .iter()
        .map(|order_record| order_record.target)
        .collect();
    assert_eq!(
        targets,
        [
            Some(object_id(109)),
            Some(object_id(108)),
            Some(object_id(107)),
            Some(object_id(106)),
            Some(object_id(105)),
            Some(object_id(104)),
            Some(object_id(103)),
            Some(object_id(102))
        ]
    );
}

/// The full update's start (`0x140c66b10`): an expired retarget entry goes and a live one
/// stays; a target gone or below 1 hit point empties them all.
#[test]
fn the_retarget_entries_expire_and_dead_target_empties_them() {
    let seen = |hp: HitPoints| UnitSeen {
        hp,
        ..UnitSeen::default()
    };
    let mut ai = UnitAi {
        retries: vec![
            (object_id(1), WorldTime::from_millis(900)),
            (object_id(2), WorldTime::from_millis(1100)),
            (object_id(3), WorldTime::from_millis(1000)),
        ],
        ..UnitAi::default()
    };
    ai.expire_retries(WorldTime::from_millis(1000), &mut |_| {
        Ok(Some(seen(HitPoints::new(5.0))))
    })
    .unwrap();
    assert_eq!(ai.retries, [(object_id(2), WorldTime::from_millis(1100))]);
    let mut ai = UnitAi {
        retries: vec![
            (object_id(1), WorldTime::from_millis(2000)),
            (object_id(2), WorldTime::from_millis(2000)),
        ],
        ..UnitAi::default()
    };
    ai.expire_retries(WorldTime::from_millis(1000), &mut |id| {
        Ok((id == object_id(1)).then(|| seen(HitPoints::new(5.0))))
    })
    .unwrap();
    assert_eq!(ai.retries, []);
    let mut ai = UnitAi {
        retries: vec![(object_id(1), WorldTime::from_millis(2000))],
        ..UnitAi::default()
    };
    ai.expire_retries(WorldTime::from_millis(1000), &mut |_| {
        Ok(Some(seen(HitPoints::new(0.5))))
    })
    .unwrap();
    assert_eq!(ai.retries, []);
}

/// `AskForHelp` (`FUN_140c42ce0`): an attacker the world does not hold asks nothing; else the
/// unit searches from its place (slot 0x50, mode 1) over the classes its constructor gave
/// it within its search radius (the line of sight plus 1 when farther; plus 7 for the
/// villager and the fishing ship), the search sets the alert mode, and the walk of its lists
/// looks for helpers. 426048706's gate 7558 (a building) in update 4, here player 1's at
/// (124.5, 174.5), attacked by player 2's soldier 7955 two tiles east; a free soldier of its
/// own in its lists answers, whose answer is not modelled (its error shows the walk met it).
#[test]
fn call_for_help_searches_what_the_unit_cares_about_then_walks_the_lists() {
    let at = Surroundings {
        position: Position::new(124.5, 174.5, 0.0),
        los: Distance::new(6.0),
        search_radius: Distance::new(4.0),
        class: Some(class::GATE),
        roles: roles(),
        slots: slots(),
        unit_ai_tuning: tuning().unit_ai,
        ..Surroundings::default()
    };
    let area = |ai_kind, class| {
        let at = Surroundings {
            class: Some(class),
            ..at.clone()
        };
        let (cared, radius) = idle(ai_kind).search_area(&at, PlayerKind::Human);
        (cared.to_vec(), radius.get())
    };
    assert_eq!(
        area(UnitAiKind::Building, class::GATE),
        (BUILDING_CARED.to_vec(), 7.0)
    );
    assert_eq!(
        area(UnitAiKind::Civilian, class::GATE),
        (CIVILIAN_CARED.to_vec(), 14.0)
    );
    for (ai_kind, class, cared) in [
        (
            UnitAiKind::Transport,
            class::TRANSPORT,
            &TRANSPORT_CARED[..],
        ),
        (UnitAiKind::SiegeTower, class::SIEGE, &SIEGE_TOWER_CARED[..]),
        (UnitAiKind::Trade, class::TRADE_BOAT, &TRADE_SHIP_CARED[..]),
        (UnitAiKind::Trade, class::TRADE_CART, &TRADE_CART_CARED[..]),
    ] {
        assert_eq!(area(ai_kind, class), (cared.to_vec(), 7.0));
    }
    assert_eq!(
        area(UnitAiKind::Fishing, class::GATE),
        (FISHING_CARED.to_vec(), 14.0)
    );
    // the gate on a world: the attacker, its helper, both in player 1's lists
    let gate = Master {
        id: master_id(88),
        kind: MasterKind::BUILDING,
        class: class::GATE,
        radius: Footprint::new(0.5, 0.5),
        height: Distance::new(2.0),
        hit_points: MasterHitPoints::of_data(2750),
        line_of_sight: Distance::new(6.0),
        action: Some(ActionPart {
            search_radius: Distance::new(4.0),
            ..ActionPart::default()
        }),
        ..Master::default()
    };
    let seen = Some(VisionBits::visible(player_id(1)));
    let ask = |attacker: bool, helper: bool| {
        let mut scene = Scene::open(GridSize::new(200, 200))
            .players(2)
            .object(SceneObject {
                ai: Some(idle(UnitAiKind::Building)),
                ..SceneObject::new(
                    7558,
                    player_id(1),
                    gate.clone(),
                    Position::new(124.5, 174.5, 0.0),
                )
            });
        if attacker {
            scene = scene.object(SceneObject {
                seen_by: seen,
                ..SceneObject::new(
                    7955,
                    player_id(2),
                    walker(5, class::CLASS_6),
                    Position::new(126.5, 174.5, 0.0),
                )
            });
        }
        if helper {
            scene = scene.object(SceneObject {
                ai: Some(idle(UnitAiKind::Soldier)),
                seen_by: seen,
                ..SceneObject::new(
                    7960,
                    player_id(1),
                    walker(6, class::CLASS_6),
                    Position::new(124.5, 176.5, 0.0),
                )
            });
        }
        let mut build = scene.build();
        let run_ai = run_ai(
            &mut build,
            (player_id(1), object_id(7558)),
            |ai, at, side, kind, _| ai.ask_for_help(object_id(7955), at, kind, side),
        );
        (run_ai, build)
    };
    let alert = |world: &World| world.parts.unit_ai.get(object_id(7558)).unwrap().alert_mode;
    // an attacker the world does not hold: no search
    let (result, world) = ask(false, false);
    assert_eq!(result, Ok(()));
    assert!(world.spatial.searched().is_none());
    assert_eq!(alert(&world), AlertMode::Calm);
    // the search finds the enemy and the alert mode follows
    let (result, world) = ask(true, false);
    assert_eq!(result, Ok(()));
    assert!(world.spatial.searched().is_some());
    assert_eq!(alert(&world), AlertMode::Alert);
    // the walk meets the idle soldier within reach
    assert_eq!(ask(true, true).0, helper_answers());
}

/// A villager of player 1: a walking civilian of radius 0.2 on land.
fn villager_master() -> Master {
    Master {
        id: ids::VILLAGER_MALE,
        kind: MasterKind::COMBAT,
        class: class::VILLAGER,
        radius: Footprint::new(0.2, 0.2),
        height: Distance::new(1.0),
        hit_points: MasterHitPoints::of_data(25),
        terrain_restriction: Some(RestrictionId::named(7)),
        move_stats: Some(walking(Speed::new(0.8))),
        ..Master::default()
    }
}

/// A house (master 70) of radius 1 and a tree (master 349, Gaia's).
fn house_and_tree() -> (Master, Master) {
    let house = Master {
        id: ids::HOUSE,
        kind: MasterKind::BUILDING,
        class: class::BUILDING,
        radius: Footprint::new(1.0, 1.0),
        height: Distance::new(2.0),
        hit_points: MasterHitPoints::of_data(550),
        ..Master::default()
    };
    let tree = Master {
        id: master_id(349),
        kind: MasterKind::TREE,
        class: class::TREE,
        radius: Footprint::new(0.5, 0.5),
        height: Distance::new(2.0),
        ..Master::default()
    };
    (house, tree)
}

/// Notify 0x216 (`FUN_14123bb20`'s, the base handler's case) on a villager standing on house
/// 10028's foundation (at (30, 90); the notify's radii, from the hundredths, 1 and 1.5) on its
/// way to tree 600: its spot is the nearest empty tile off the footprint, (28.5, 89.5), and it
/// walks there (action 0x2bf, the old one the last); a human villager whose action has a
/// target queues the foundation's order in front and that work behind, the points the
/// vectors' x and height.
#[test]
fn unit_sent_off_footprint_walks_to_the_spot_and_comes_back_to_its_work() {
    let (house, tree) = house_and_tree();
    let to_tree = MoveToAction {
        target: Some(object_id(600)),
        point: Position::new(33.5, 95.5, 0.0),
        ..MoveToAction::sub_in(MoveTo::Moving)
    };
    let villager = SceneObject {
        ai: Some(UnitAi {
            ai_kind: UnitAiKind::Civilian,
            current_action: Some(AiAction::Gather),
            orders: vec![QueuedOrder {
                order: OrderKind::Move,
                ..QueuedOrder::default()
            }],
            ..UnitAi::default()
        }),
        actions: Some(Action {
            list: ActionList::default().with_move_to(Some(to_tree)),
            ..Action::default()
        }),
        ..SceneObject::new(
            9175,
            player_id(1),
            villager_master(),
            Position::new(29.6, 89.6, 0.0),
        )
    };
    let mut world = Scene::open(GridSize::new(120, 120))
        .object(villager)
        .object(SceneObject {
            desolid: true,
            state: ObjectState::Foundation,
            ..SceneObject::new(10028, player_id(1), house, Position::new(30.0, 90.0, 0.0))
        })
        .object(SceneObject::new(
            600,
            player_id(0),
            tree,
            Position::new(33.5, 95.5, 0.0),
        ))
        .build();
    let notify = Notify::own(object_id(9175), NotifyKind::SENT_OFF, [100, 150, 10028]);
    let answer = run_ai(
        &mut world,
        (player_id(1), object_id(9175)),
        |ai, _, side, _, rng| ai.sent_off(notify, side, rng, &roles(), &tuning().unit_ai),
    );
    assert_eq!(answer, Ok(GO_ON));
    let me = object_id(9175);
    let walk = world
        .parts
        .actions
        .of(me)
        .list
        .move_to()
        .map(|move_to_action| (move_to_action.point, move_to_action.target));
    assert_eq!(walk, Some((Position::new(28.5, 89.5, 0.0), None)));
    let ai = world.parts.unit_ai.get(me).unwrap();
    assert_eq!(
        (ai.current_action, ai.last_action),
        (Some(AiAction::MoveAside), Some(AiAction::Gather))
    );
    let orders: Vec<_> = ai
        .orders
        .iter()
        .map(|queued_order| {
            (
                queued_order.order,
                queued_order.target,
                queued_order.point,
                queued_order.priority.get(),
                queued_order.ready,
            )
        })
        .collect();
    assert_eq!(
        orders,
        vec![
            (
                OrderKind::Foundation,
                Some(object_id(10028)),
                Position::new(30.0, 0.0, 0.0),
                100,
                true
            ),
            (OrderKind::Move, None, Position::ORIGIN, 0, false),
            (
                OrderKind::OnTarget,
                Some(object_id(600)),
                Position::new(33.5, 0.0, 0.0),
                100,
                true
            ),
        ]
    );
}

/// 426049848's deer 14054 (Gaia's, unit-AI type 2, no actions) in update 685: a town centre's
/// foundation (here at (164.5, 49), the notify's radii 2) sent it off its footprint (0x216).
/// Its move off (slot 0x148 forced: the unit's slot 0x340) to the nearest tile off the
/// footprint gives it its `MoveTo` at once, so the full update's later check (slot 0x430) sees
/// it busy and the action value 0x2bf stays.
#[test]
fn unit_sent_off_footprint_keeps_its_move_aside_through_the_update() {
    let mut unit_ai = idle(UnitAiKind::Hunted);
    unit_ai
        .post(
            Notify::own(object_id(14054), NotifyKind::SENT_OFF, [200, 200, 14100]),
            ObjectState::Alive,
        )
        .unwrap();
    let deer = Master {
        id: master_id(65),
        kind: MasterKind::COMBAT,
        class: class::PREY,
        radius: Footprint::new(0.2, 0.2),
        height: Distance::new(1.0),
        hit_points: MasterHitPoints::of_data(5),
        terrain_restriction: Some(RestrictionId::named(7)),
        move_stats: Some(walking(Speed::new(1.0))),
        ..Master::default()
    };
    let town_centre = Master {
        id: ids::TOWN_CENTRE,
        radius: Footprint::new(2.0, 2.0),
        ..house_and_tree().0
    };
    let mut world = Scene::open(GridSize::new(200, 100))
        .object(SceneObject {
            ai: Some(unit_ai),
            ..SceneObject::new(14054, player_id(0), deer, Position::new(164.5, 50.5, 0.0))
        })
        .object(SceneObject {
            desolid: true,
            state: ObjectState::Foundation,
            ..SceneObject::new(
                14100,
                player_id(1),
                town_centre,
                Position::new(164.5, 49.0, 0.0),
            )
        })
        .build();
    let after = run_ai(
        &mut world,
        (player_id(0), object_id(14054)),
        |ai, at, side, kind, rng| {
            ai.update(
                Millis::new(50),
                WorldTime::from_millis(22643),
                kind,
                at,
                rng,
                side,
            )
        },
    );
    assert_eq!(after, Ok(None));
    let me = object_id(14054);
    let walk = world
        .parts
        .actions
        .of(me)
        .list
        .move_to()
        .map(|move_to_action| move_to_action.point);
    assert_eq!(walk, Some(Position::new(164.5, 51.5, 0.0)));
    let ai = world.parts.unit_ai.get(me).unwrap();
    assert_eq!(
        (ai.current_action, ai.last_action),
        (Some(AiAction::MoveAside), None)
    );
}

/// The unit AI's calls on its unit run at once, in the order it makes them
/// ([`UnitView::effect_now`]): the cap on the last human order time, then the stop, reach unit
/// 9 before the attack command that follows them, whose Attack the stop would have cleared.
#[test]
fn the_unit_ai_calls_reach_the_unit_in_their_order() {
    let enemy = order_target(70, player_id(2), class::CLASS_6);
    let mut world = order_world(
        order_unit(order_soldier(), idle(UnitAiKind::Soldier)),
        &[enemy],
    );
    world.players[1]
        .objects
        .by_id_mut(object_id(9))
        .unwrap()
        .last_human_order = WorldTime::from_millis(9000);
    let answer = run_ai(
        &mut world,
        (player_id(1), object_id(9)),
        |ai, at, side, _, rng| {
            ai.cap_human_order(side, rng)?;
            ai.stop_with(StopOrder::Keep, None, side, rng, &at.roles)?;
            ai.attack_command(object_id(70), OrderForce::Forced, at, (side, rng))
        },
    );
    assert_eq!(answer, Ok(Answer::Taken));
    assert_eq!(attacks_9(&world), Some(object_id(70)));
    let unit = world.players[1].objects.by_id(object_id(9)).unwrap();
    assert_eq!(unit.last_human_order, HUMAN_ORDER_CAP);
}

/// [`villager_master`] with a Build task on any class, which slot 0x820 finds on a foundation
/// in the villager's own list.
fn builder_master() -> Master {
    Master {
        action: Some(ActionPart {
            tasks: vec![build_task()],
            ..ActionPart::default()
        }),
        ..villager_master()
    }
}

/// A Build task (`action_kind` [`ActionKind::BUILD`]) on any class.
fn build_task() -> Task {
    Task {
        action_kind: ActionKind::BUILD,
        ..Task::default()
    }
}

/// Villager 9175 of player 1 (master `villager`, its unit AI `ai`) at (29.5, 89.5) beside
/// house 10028's foundation at (30, 88) (desolid, no build points), and Gaia's tree 600 at
/// (33.5, 89.5).
fn foundation_scene(villager: Master, ai: UnitAi) -> World {
    let (house, tree) = house_and_tree();
    Scene::open(GridSize::new(120, 120))
        .object(SceneObject {
            ai: Some(ai),
            ..SceneObject::new(9175, player_id(1), villager, Position::new(29.5, 89.5, 0.0))
        })
        .object(SceneObject {
            desolid: true,
            state: ObjectState::Foundation,
            ..SceneObject::new(10028, player_id(1), house, Position::new(30.0, 88.0, 0.0))
        })
        .object(SceneObject::new(
            600,
            player_id(0),
            tree,
            Position::new(33.5, 89.5, 0.0),
        ))
        .build()
}

/// The wait for foundation 10028 that villager 9175 of `world` holds, and whether it is its top
/// action (type 0x9a).
fn waits(world: &World) -> (Option<WaitSolidify>, bool) {
    let actions = world.parts.actions.of(object_id(9175));
    (
        actions.list.wait_solidify().copied(),
        actions.top_action_kind() == Some(WAIT_SOLIDIFY),
    )
}

/// The queue a villager sent off a foundation's footprint holds once its move aside ended
/// (0x2bf, which the full update resets): the foundation's order, then its work.
fn sent_off_queue() -> UnitAi {
    let order = |order: OrderKind, target: i32, x: f32| QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order,
        priority: OrderPriority::of_data(100),
        target: ObjectId::of_field(target),
        target_owner: owner(1),
        point: Position::new(x, 2.0, 0.0),
        range: Distance::ZERO,
        shown: Point::new(x, 2.0),
        ready: true,
        target_kind: order_kind(70),
    };
    UnitAi {
        current_action: None,
        last_action: Some(AiAction::MoveAside),
        current_order: None,
        order_priority: OrderPriority::of_data(-1),
        current_target: None,
        orders: vec![
            order(OrderKind::Foundation, 10028, 30.0),
            order(OrderKind::OnTarget, 600, 33.5),
        ],
        ..UnitAi::default()
    }
}

/// Case 0x2be, the build (`FUN_140c5c170`): `BuildObject` forced (slot 0xf0, `FUN_140c47d80`)
/// gives the unit its build command at once (a Build on the foundation, the task the
/// villager's own list holds), and makes the build the AI's action and the foundation its
/// target at its place with its class; the tail takes the order and its priority (428326873's
/// villager 8766 at update 1804, whose hunt after it must come later).
#[test]
fn the_build_order_builds_and_takes_the_order_at_once() {
    let build = QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::Build,
        priority: OrderPriority::of_data(-1),
        target: Some(object_id(10028)),
        target_owner: owner(1),
        point: Position::new(30.0, 88.0, 0.0),
        range: Distance::new(2.0),
        shown: Point::new(30.0, 88.0),
        ready: true,
        target_kind: order_kind(3),
    };
    let mut world = foundation_scene(
        builder_master(),
        UnitAi {
            current_action: None,
            current_order: None,
            current_target: Some(object_id(7)),
            target_class: class_of(10),
            orders: vec![build],
            ..UnitAi::default()
        },
    );
    let order_outcome = run_ai(
        &mut world,
        (player_id(1), object_id(9175)),
        |ai, at, side, kind, rng| {
            ai.process_order(
                &build,
                Some((at, kind)),
                None,
                rng,
                side,
                AiLayer {
                    roles: &roles(),
                    slots: slots(),
                    unit_ai_tuning: &tuning().unit_ai,
                },
            )
        },
    );
    assert_eq!(order_outcome, Ok(OrderOutcome::Taken));
    let me = object_id(9175);
    let built = world
        .parts
        .actions
        .of(me)
        .list
        .build()
        .map(|build_action| build_action.target);
    assert_eq!(built, Some(Some(FoundationId::new(object_id(10028)))));
    let ai = world.parts.unit_ai.get(me).unwrap();
    assert_eq!(
        (
            ai.current_action,
            ai.current_target,
            ai.target_class,
            ai.target_point
        ),
        (
            Some(AiAction::Build),
            Some(object_id(10028)),
            class_of(3),
            Position::new(30.0, 88.0, 0.0)
        )
    );
    assert_eq!(
        (ai.last_target, ai.last_target_class),
        (Some(object_id(7)), class_of(10))
    );
    assert_eq!(
        (ai.current_order, ai.order_priority.get()),
        (Some(OrderKind::Build), -1)
    );
}

/// Case 0x2de (`FUN_140c5c170`): slot 0x108 forced (`FUN_140c68a60`) runs the unit's slot
/// 0x388 on the foundation, whose wait (with the Build task slot 0x820 finds) replaces the
/// unit's actions at once; its 1 makes 0x27a the action value with the old one the last, then
/// the case sets 0x27a again (the last takes it) and the tail takes the order and its
/// priority, which ends the queue's turn: the work waits in the queue.
#[test]
fn foundation_order_waits_for_the_foundation_and_ends_the_queue_turn() {
    let mut world = foundation_scene(builder_master(), sent_off_queue());
    let run_ai = run_ai(
        &mut world,
        (player_id(1), object_id(9175)),
        UnitAi::off_turn,
    );
    assert_eq!(run_ai, Ok(()));
    let wait = WaitSolidify::new(build_task(), FoundationId::new(object_id(10028)));
    assert_eq!(waits(&world), (Some(wait), true));
    let ai = world.parts.unit_ai.get(object_id(9175)).unwrap();
    assert_eq!(
        (ai.current_action, ai.last_action),
        (Some(AiAction::WaitSolidify), Some(AiAction::WaitSolidify))
    );
    assert_eq!(
        (ai.current_order, ai.order_priority.get()),
        (Some(OrderKind::Foundation), 100)
    );
    let left: Vec<_> = ai
        .orders
        .iter()
        .map(|queued_order| (queued_order.order, queued_order.target))
        .collect();
    assert_eq!(left, vec![(OrderKind::OnTarget, Some(object_id(600)))]);
}

/// Slot 0x388's 0 (no task on the foundation): the case still makes 0x27a the action value,
/// the old one the last, and returns 0; the order goes and the queue runs the next, the work
/// on its old target (priority 100 passes the action value), at once: the villager, without a
/// task on the tree, is told of a move and walks to it (the work command without a task), and
/// the tail takes the move's order at the work's priority.
#[test]
fn foundation_order_without_wait_goes_and_the_work_runs() {
    let mut world = foundation_scene(villager_master(), sent_off_queue());
    let run_ai = run_ai(
        &mut world,
        (player_id(1), object_id(9175)),
        UnitAi::off_turn,
    );
    assert_eq!(run_ai, Ok(()));
    assert_eq!(waits(&world), (None, false));
    let me = object_id(9175);
    let walk = world
        .parts
        .actions
        .of(me)
        .list
        .move_to()
        .map(|move_to_action| (move_to_action.point, move_to_action.target));
    assert_eq!(
        walk,
        Some((Position::new(33.5, 89.5, 0.0), Some(object_id(600))))
    );
    let ai = world.parts.unit_ai.get(me).unwrap();
    assert_eq!(
        (ai.current_action, ai.last_action),
        (Some(AiAction::Move), Some(AiAction::WaitSolidify))
    );
    assert_eq!(
        (ai.current_order, ai.order_priority.get(), ai.current_target),
        (Some(OrderKind::Move), 100, Some(object_id(600)))
    );
    assert_eq!(ai.orders, vec![]);
    assert!(ai.ordered_on_target);
}

/// Slot 0x108 (`FUN_140c68a60`): unforced, a unit with an action value does not ask its
/// unit; forced or without one it does, and only its 1 (the wait its slot 0x388 makes, with a
/// Build task for the foundation) changes the action values.
#[test]
fn slot_0x108_asks_the_unit_when_forced_or_without_an_action_value() {
    let busy = UnitAi {
        current_action: Some(AiAction::Gather),
        last_action: Some(AiAction::Heal),
        ..UnitAi::default()
    };
    let ask = |villager: Master, ai: UnitAi, force: OrderForce| {
        let mut world = foundation_scene(villager, ai);
        let answer = run_ai(
            &mut world,
            (player_id(1), object_id(9175)),
            |ai, _, side, _, _| ai.wait_for_foundation(object_id(10028), force, side),
        );
        let ai = world.parts.unit_ai.get(object_id(9175)).unwrap();
        (answer, (ai.current_action, ai.last_action), waits(&world).1)
    };
    let (gather, heal) = (Some(AiAction::Gather), Some(AiAction::Heal));
    // unforced with an action value: the unit is not asked
    assert_eq!(
        ask(builder_master(), busy.clone(), OrderForce::Unforced),
        (Ok(Answer::NotTaken), (gather, heal), false)
    );
    // forced, a unit without a task for the foundation answers 0
    assert_eq!(
        ask(villager_master(), busy.clone(), OrderForce::Forced),
        (Ok(Answer::NotTaken), (gather, heal), false)
    );
    // forced, one with a task waits
    assert_eq!(
        ask(builder_master(), busy, OrderForce::Forced),
        (
            Ok(Answer::Taken),
            (Some(AiAction::WaitSolidify), gather),
            true
        )
    );
    // unforced without an action value
    let idle = UnitAi {
        current_action: None,
        ..UnitAi::default()
    };
    assert_eq!(
        ask(builder_master(), idle, OrderForce::Unforced),
        (
            Ok(Answer::Taken),
            (Some(AiAction::WaitSolidify), None),
            true
        )
    );
}

/// Villager 31237's master: [`villager_master`] of line of sight 4 that hunts deer (class 9) and
/// boars (0x21), gathers bushes (7), stone (8), gold (32), trees (15) and farms (49), builds
/// anything, and drops at the town centre.
fn gatherer_master() -> Master {
    let task = |action_kind, class: i16, attribute| Task {
        action_kind,
        object_group: Some(class_id(class)),
        attribute_kind: Some(attribute),
        work_range: Distance::new(0.5),
        ..Task::default()
    };
    Master {
        line_of_sight: Distance::new(4.0),
        action: Some(ActionPart {
            drop_sites: vec![ids::TOWN_CENTRE],
            tasks: vec![
                task(HUNT, 9, attribute::FOOD),
                task(HUNT, 0x21, attribute::FOOD),
                task(GATHER, 7, attribute::FOOD),
                task(GATHER, 8, attribute::STONE),
                task(GATHER, 32, attribute::GOLD),
                task(GATHER, 15, attribute::WOOD),
                task(GATHER, 49, attribute::FOOD),
                build_task(),
            ],
            ..ActionPart::default()
        }),
        ..villager_master()
    }
}

/// Player 1's villager 31237 of master `villager` at (20, 20) on a map of 64 by 64, its unit
/// AI `ai`, and its town centre 31247 at (26, 26); the `others`, each in player 1's lists.
fn gather_world((villager, ai): (Master, UnitAi), others: &[SceneObject]) -> World {
    gather_world_on(&[], (villager, ai), others)
}

/// [`gather_world`] with water on `water`.
fn gather_world_on(
    water: &[Tile],
    (villager, ai): (Master, UnitAi),
    others: &[SceneObject],
) -> World {
    let centre = Master {
        id: ids::TOWN_CENTRE,
        kind: MasterKind::BUILDING,
        class: class::BUILDING,
        radius: Footprint::new(2.0, 2.0),
        height: Distance::new(3.0),
        hit_points: MasterHitPoints::of_data(2400),
        terrain_restriction: Some(RestrictionId::named(7)),
        move_stats: Some(walking(Speed::ZERO)),
        ..Master::default()
    };
    let mut scene = Scene::open(GridSize::new(64, 64))
        .players(2)
        .water(water.iter().copied())
        .object(SceneObject {
            ai: Some(ai),
            ..SceneObject::new(
                31237,
                player_id(1),
                villager,
                Position::new(20.0, 20.0, 0.0),
            )
        })
        .object(SceneObject {
            seen_by: Some(VisionBits::visible(player_id(1))),
            ..SceneObject::new(31247, player_id(1), centre, Position::new(26.0, 26.0, 0.0))
        });
    for other in others {
        scene = scene.object(SceneObject {
            seen_by: Some(VisionBits::visible(player_id(1))),
            ..other.clone()
        });
    }
    scene.build()
}

/// Gaia's resource `id` of class `class` (master `master`, of `kind`, which gatherers take up
/// by themselves as `group`) at (x, y), holding `amount` of `attribute`.
fn resource(
    (id, master, class): (i32, i16, ClassId),
    (kind, group): (MasterKind, GatherGroup),
    (x, y): (f32, f32),
    (attribute, amount): (AttributeId, f32),
) -> SceneObject {
    let resource_master = Master {
        id: master_id(master),
        kind,
        class,
        radius: Footprint::new(0.5, 0.5),
        height: Distance::new(1.0),
        hit_points: MasterHitPoints::of_data(10),
        terrain_restriction: Some(RestrictionId::named(7)),
        auto_gather: AutoGatherMode::ON,
        gather_group: group,
        ..Master::default()
    };
    SceneObject {
        holds: Some((attribute, Amount::new(amount))),
        ..SceneObject::new(
            id,
            PlayerId::GAIA,
            resource_master,
            Position::new(x, y, 0.0),
        )
    }
}

/// Gaia's deer `id` (master 65, class 9) at (x, y), holding 100 food, its unit AI idle.
fn gaia_deer(id: i32, (x, y): (f32, f32)) -> SceneObject {
    let master = Master {
        id: master_id(65),
        kind: MasterKind::COMBAT,
        class: class_id(9),
        radius: Footprint::new(0.2, 0.2),
        height: Distance::new(1.0),
        hit_points: MasterHitPoints::of_data(5),
        terrain_restriction: Some(RestrictionId::named(7)),
        move_stats: Some(walking(Speed::new(1.0))),
        ..Master::default()
    };
    SceneObject {
        ai: Some(idle(UnitAiKind::Hunted)),
        holds: Some((attribute::FOOD, Amount::new(100.0))),
        ..SceneObject::new(id, PlayerId::GAIA, master, Position::new(x, y, 0.0))
    }
}

/// Player 1's farm 12557 at (17, 24), holding 175 food.
fn own_farm() -> SceneObject {
    let master = Master {
        id: ids::FARM,
        kind: MasterKind::BUILDING,
        class: class::FARM,
        radius: Footprint::new(1.5, 1.5),
        terrain_restriction: Some(RestrictionId::named(7)),
        move_stats: Some(walking(Speed::ZERO)),
        ..Master::default()
    };
    SceneObject {
        holds: Some((attribute::FOOD, Amount::new(175.0))),
        ..SceneObject::new(12557, player_id(1), master, Position::new(17.0, 24.0, 0.0))
    }
}

/// A civilian of player 1 on the gather order with action `action`, its current target 28202,
/// its target class `target` and last target class `last` (−1 for none).
fn civilian(action: AiAction, target: i16, last: i16) -> UnitAi {
    UnitAi {
        ai_kind: UnitAiKind::Civilian,
        current_order: Some(OrderKind::Gather),
        current_action: Some(action),
        current_target: Some(object_id(28202)),
        target_class: class_of(target),
        last_target_class: class_of(last),
        ..UnitAi::default()
    }
}

/// Villager 31237's notify 0x202 for action `action` with class `class` and drop site `site`.
fn search_notify(action: i32, class: i32, site: i32) -> Notify {
    Notify::own(
        object_id(31237),
        NotifyKind::REQUIRES_SEARCH,
        [action, class, site],
    )
}

/// What villager 31237 of `world` targets and does: its unit AI's current target, target class
/// and action, and its top action's kind.
fn gatherer_state(
    world: &World,
) -> (
    Option<ObjectId>,
    Option<ClassId>,
    Option<AiAction>,
    Option<ActionKind>,
) {
    let unit_ai = world.parts.unit_ai.get(object_id(31237)).unwrap();
    (
        unit_ai.current_target,
        unit_ai.target_class,
        unit_ai.current_action,
        world.parts.actions.of(object_id(31237)).top_action_kind(),
    )
}

/// Villager 31237's search of `world` on notify `notify` by its action (`hunt`:
/// [`UnitAi::hunter_search`], else [`UnitAi::gatherer_search`]).
fn gatherer_searches(world: &mut World, notify: Notify, hunt: bool) -> GenieResult<u32> {
    run_ai(
        world,
        (player_id(1), object_id(31237)),
        |ai, at, side, kind, rng| {
            if hunt {
                ai.hunter_search(notify, at, kind, side, rng)
            } else {
                ai.gatherer_search(notify, at, kind, side, rng)
            }
        },
    )
}

/// The villager's case 0x202 on its hunt ([`UnitAi::hunter_search`]): the class is the
/// notify's, else the AI's target class, else its last; with none the target goes and the
/// action stops. A class without a gather group (a deer, 9) takes `ClosestObject`: what it
/// finds (deer 40) it hunts (its Hunt at once) and the processing breaks; finding nothing a
/// human player's villager does nothing more. A group's class searches by kind (a boar's
/// 0x21: food, any workers, from the notify's drop site). A farm found (below one worker) the
/// villager works at its place, and so gathers. A computer player's villager finding nothing
/// is not modelled; every object a search finds has its place, so the double's placeless find
/// is a case no world reaches.
#[test]
fn hunter_asked_for_another_target_hunts_what_its_search_finds() {
    let hunt = AiAction::Hunt.value();
    let run = |ai: UnitAi, others: &[SceneObject], class: i32, kind: PlayerKind| {
        let mut world = gather_world((gatherer_master(), ai), others);
        world.players[1].kind = kind;
        let gatherer_searches =
            gatherer_searches(&mut world, search_notify(hunt, class, 31247), true);
        (gatherer_searches, gatherer_state(&world))
    };
    let human = PlayerKind::Human;
    let hunting = (
        Some(object_id(40)),
        class_of(9),
        Some(AiAction::Hunt),
        Some(HUNT),
    );
    let deer40 = [gaia_deer(40, (23.0, 20.0))];
    assert_eq!(
        run(civilian(AiAction::Hunt, -1, -1), &deer40, 9, human),
        (Ok(BREAK), hunting)
    );
    assert_eq!(
        run(civilian(AiAction::Hunt, 9, -1), &deer40, -1, human),
        (Ok(BREAK), hunting)
    );
    assert_eq!(
        run(civilian(AiAction::Hunt, -1, 9), &deer40, -1, human),
        (Ok(BREAK), hunting)
    );
    assert_eq!(
        run(civilian(AiAction::Hunt, -1, -1), &deer40, -1, human),
        (Ok(GO_ON), (None, None, None, None))
    );
    let still = (Some(object_id(28202)), None, Some(AiAction::Hunt), None);
    assert_eq!(
        run(civilian(AiAction::Hunt, -1, -1), &[], 9, human),
        (Ok(GO_ON), still)
    );
    assert_eq!(
        run(
            civilian(AiAction::Hunt, -1, -1),
            &[],
            9,
            PlayerKind::Computer
        )
        .0,
        Err(GenieError::unsupported(
            "a computer player's hunter that found nothing"
        ))
    );
    // a farm found: the unit works it at its place (slot 0x140), which gathers it
    assert_eq!(
        run(civilian(AiAction::Hunt, -1, -1), &[own_farm()], 49, human),
        (
            Ok(BREAK),
            (
                Some(object_id(12557)),
                class_of(49),
                Some(AiAction::Gather),
                Some(GATHER)
            )
        )
    );
    // a boar's 0x21 searches by its kind from the drop site
    let boar = resource(
        (28500, 48, class_id(0x21)),
        (MasterKind::STATIC, GatherGroup::FISH),
        (24.0, 23.0),
        (attribute::FOOD, 340.0),
    );
    assert_eq!(
        run(civilian(AiAction::Hunt, -1, -1), &[boar], 0x21, human),
        (
            Ok(BREAK),
            (
                Some(object_id(28500)),
                class_of(0x21),
                Some(AiAction::Hunt),
                Some(HUNT)
            )
        )
    );
}

/// The villager's case 0x202 on its gather ([`UnitAi::gatherer_search`]) beyond the bush:
/// the class is the AI's target class, else its last, else the notify's; with none the
/// target goes and the action stops. Stone (8) is searched below 4 workers: of two mines the
/// nearer with 4 workers is passed for the farther with 3. A farm (49, no gather group) takes
/// `ClosestObject`, what it finds gathered; finding nothing a human player's villager does
/// nothing more, a computer player's stops. Every object a search finds has its place.
#[test]
fn gatherer_without_class_stops_and_one_on_farm_searches_for_another() {
    let gather = AiAction::Gather.value();
    let run = |ai: UnitAi, others: &[SceneObject], class: i32, kind: PlayerKind| {
        let mut world = gather_world((gatherer_master(), ai), others);
        world.players[1].kind = kind;
        let gatherer_searches =
            gatherer_searches(&mut world, search_notify(gather, class, 31247), false);
        (gatherer_searches, gatherer_state(&world))
    };
    let human = PlayerKind::Human;
    assert_eq!(
        run(civilian(AiAction::Gather, -1, -1), &[], -1, human),
        (Ok(GO_ON), (None, None, None, None))
    );
    let mine = |id: i32, at| {
        resource(
            (id, 102, class_id(8)),
            (MasterKind::STATIC, GatherGroup::STONE),
            at,
            (attribute::STONE, 350.0),
        )
    };
    let (_, (target, ..)) = run(
        civilian(AiAction::Gather, -1, 8),
        &[
            SceneObject {
                workers: WorkerCount::of_data(4),
                ..mine(28400, (22.0, 22.0))
            },
            mine(28401, (24.0, 22.0)),
        ],
        -1,
        human,
    );
    assert_eq!(target, Some(object_id(28401)));
    assert_eq!(
        run(civilian(AiAction::Gather, -1, -1), &[own_farm()], 49, human),
        (
            Ok(BREAK),
            (
                Some(object_id(12557)),
                class_of(49),
                Some(AiAction::Gather),
                Some(GATHER)
            )
        )
    );
    assert_eq!(
        run(civilian(AiAction::Gather, -1, -1), &[], 49, human),
        (
            Ok(GO_ON),
            (Some(object_id(28202)), None, Some(AiAction::Gather), None)
        )
    );
    assert_eq!(
        run(
            civilian(AiAction::Gather, -1, -1),
            &[],
            49,
            PlayerKind::Computer
        ),
        (Ok(GO_ON), (None, None, None, None))
    );
}

/// Player 1's house foundation 10028 at `at` (desolid, no build points).
fn house_foundation((x, y): (f32, f32)) -> SceneObject {
    let (house, _) = house_and_tree();
    SceneObject {
        desolid: true,
        state: ObjectState::Foundation,
        ..SceneObject::new(10028, player_id(1), house, Position::new(x, y, 0.0))
    }
}

/// The villager's cases 0x1fb, 0x1fc and 0x202 by its action ([`UnitAi::villager_search`]):
/// the hunt and the gather search as theirs (deer 40 found); the builder takes the foundation
/// its search finds and reaches (its Build at once, the processing breaks); finding one it
/// does not reach (on an island) or none, a human player's stops its actions (slot 0x3c0, the
/// AI's target and action kept); the shepherd's search, a
/// computer builder without a foundation and the explorer's search are not modelled.
#[test]
fn villager_searches_by_its_action() {
    let villager = |current_action| UnitAi {
        ai_kind: UnitAiKind::Civilian,
        current_action,
        current_target: Some(object_id(28202)),
        target_class: class_of(9),
        ..UnitAi::default()
    };
    let notify = search_notify(0, -1, -1);
    let run = |ai: UnitAi, others: &[SceneObject], water: &[Tile], kind: PlayerKind| {
        let mut world = gather_world_on(water, (gatherer_master(), ai), others);
        world.players[1].kind = kind;
        let run_ai = run_ai(
            &mut world,
            (player_id(1), object_id(31237)),
            |ai, at, side, kind, rng| ai.villager_search(notify, at, kind, side, rng),
        );
        (run_ai, gatherer_state(&world))
    };
    let human = PlayerKind::Human;
    let deer40 = [gaia_deer(40, (23.0, 20.0))];
    assert_eq!(
        run(villager(Some(AiAction::Hunt)), &deer40, &[], human).0,
        Ok(BREAK)
    );
    let (result, (target, ..)) = run(villager(Some(AiAction::Gather)), &deer40, &[], human);
    assert_eq!((result, target), (Ok(BREAK), Some(object_id(40))));
    let foundation = [house_foundation((22.5, 20.5))];
    assert_eq!(
        run(villager(Some(AiAction::Build)), &foundation, &[], human),
        (
            Ok(BREAK),
            (
                Some(object_id(10028)),
                class_of(3),
                Some(AiAction::Build),
                Some(ActionKind::BUILD)
            )
        )
    );
    // on an island: found, not reached
    let island = [house_foundation((17.5, 17.5))];
    let ring: Vec<Tile> = (15..=19)
        .flat_map(|x| [Tile::new(x, 15), Tile::new(x, 19)])
        .chain((16..=18).flat_map(|y| [Tile::new(15, y), Tile::new(19, y)]))
        .collect();
    let stopped = (
        Some(object_id(28202)),
        class_of(9),
        Some(AiAction::Build),
        None,
    );
    assert_eq!(
        run(villager(Some(AiAction::Build)), &island, &ring, human),
        (Ok(BREAK), stopped)
    );
    assert_eq!(
        run(villager(Some(AiAction::Build)), &[], &[], human),
        (Ok(BREAK), stopped)
    );
    assert!(
        run(
            villager(Some(AiAction::Build)),
            &[],
            &[],
            PlayerKind::Computer
        )
        .0
        .is_err()
    );
    for action in [AiAction::Shepherd, AiAction::Explore].map(Some) {
        assert!(run(villager(action), &[], &[], human).0.is_err());
    }
}

/// `BuildObject` (slot 0xf0, `FUN_140c47d80`) whose build command answers 0 (slot 0x380: no
/// task for the foundation, a villager that cannot build) gives no command and leaves the AI
/// as it was; with a task it builds (its Build at once).
#[test]
fn build_object_whose_build_command_answers_0_changes_nothing() {
    let unit_ai = UnitAi {
        ai_kind: UnitAiKind::Civilian,
        current_action: Some(AiAction::Gather),
        current_target: Some(object_id(28202)),
        target_class: class_of(9),
        ..UnitAi::default()
    };
    let build = |villager: Master| {
        let mut gather_world = gather_world(
            (villager, unit_ai.clone()),
            &[house_foundation((22.5, 20.5))],
        );
        let run_ai = run_ai(
            &mut gather_world,
            (player_id(1), object_id(31237)),
            |ai, at, side, _, rng| {
                ai.build_object(
                    FoundationId::new(object_id(10028)),
                    OrderForce::Forced,
                    at,
                    side,
                    rng,
                )
            },
        );
        (run_ai, gather_world)
    };
    let (answer, world) = build(villager_master());
    assert_eq!(answer, Ok(Answer::NotTaken));
    assert_eq!(world.parts.unit_ai.get(object_id(31237)), Some(&unit_ai));
    assert_eq!(gatherer_state(&world).3, None);
    let (answer, world) = build(gatherer_master());
    assert_eq!(answer, Ok(Answer::Taken));
    let (target, _, action, top) = gatherer_state(&world);
    assert_eq!(
        (target, action, top),
        (
            Some(object_id(10028)),
            Some(AiAction::Build),
            Some(ActionKind::BUILD)
        )
    );
}

/// The villager's notify 0x202 on its gather (the 4v4's forager 31237 after update 10822):
/// its target class (7, a bush: gather group 1) names the search from the notify's drop
/// site, any workers; what it finds it gathers (its Gather at once), the AI's target, last
/// target and point taken. Gold (32) is searched below 4 workers; finding nothing a human's
/// villager stays as it is.
#[test]
fn gatherer_asked_for_another_target_gathers_what_its_search_finds() {
    let bush = resource(
        (28200, 59, class::CLASS_7),
        (MasterKind::STATIC, GatherGroup::FORAGE),
        (22.5, 23.5),
        (attribute::FOOD, 125.0),
    );
    let mut world = gather_world(
        (gatherer_master(), civilian(AiAction::Gather, 7, -1)),
        &[bush],
    );
    let gather = AiAction::Gather.value();
    assert_eq!(
        gatherer_searches(&mut world, search_notify(gather, 7, 31247), false),
        Ok(BREAK)
    );
    assert_eq!(
        gatherer_state(&world),
        (
            Some(object_id(28200)),
            class_of(7),
            Some(AiAction::Gather),
            Some(GATHER)
        )
    );
    let unit_ai = world.parts.unit_ai.get(object_id(31237)).unwrap();
    assert_eq!(
        (unit_ai.last_target, unit_ai.target_point),
        (Some(object_id(28202)), Position::new(22.5, 23.5, 0.0))
    );
    let goldless = UnitAi {
        current_order: None,
        current_target: None,
        ..civilian(AiAction::Gather, 32, -1)
    };
    let mut world = gather_world((gatherer_master(), goldless.clone()), &[]);
    assert_eq!(
        gatherer_searches(&mut world, search_notify(gather, 32, 0), false),
        Ok(GO_ON)
    );
    assert_eq!(world.parts.unit_ai.get(object_id(31237)), Some(&goldless));
    assert_eq!(gatherer_state(&world).3, None);
}

/// The villager's notify 0x202 on its hunt (`FUN_140c22bf0`, action 0x265) of a class the
/// civilian's slot 0x1b0 names a kind of: `ClosestResourceObject` (slot 0x80) from the
/// notify's drop site, kind 0 below 2 workers (of two trees the nearer with 2 workers is
/// passed for the farther with 1), another any; what it finds it hunts (`HuntObject`, slot
/// 0xc8 with 1: its Hunt at once). Finding nothing a human player's villager stays.
#[test]
fn hunter_of_kind_searches_by_it_from_its_drop_site() {
    let tree = |id: i32, at| {
        resource(
            (id, 349, class::TREE),
            (MasterKind::TREE, GatherGroup::WOOD),
            at,
            (attribute::WOOD, 100.0),
        )
    };
    let hunter = UnitAi {
        current_order: None,
        current_target: Some(object_id(28301)),
        ..civilian(AiAction::Hunt, 15, -1)
    };
    let trees = [
        SceneObject {
            workers: WorkerCount::of_data(2),
            ..tree(28300, (23.5, 23.5))
        },
        tree(28302, (24.5, 22.5)),
    ];
    let mut world = gather_world((gatherer_master(), hunter), &trees);
    assert_eq!(
        gatherer_searches(&mut world, search_notify(0x265, 15, 31247), true),
        Ok(BREAK)
    );
    assert_eq!(
        gatherer_state(&world),
        (
            Some(object_id(28302)),
            class_of(15),
            Some(AiAction::Hunt),
            Some(HUNT)
        )
    );
    let idle_hunter = UnitAi {
        current_order: None,
        current_target: None,
        ..civilian(AiAction::Hunt, -1, -1)
    };
    let mut world = gather_world((gatherer_master(), idle_hunter.clone()), &[]);
    assert_eq!(
        gatherer_searches(&mut world, search_notify(0x265, 32, -1), true),
        Ok(GO_ON)
    );
    assert_eq!(
        world.parts.unit_ai.get(object_id(31237)),
        Some(&idle_hunter)
    );
}

fn hit_boar() -> UnitAi {
    UnitAi {
        home: Some(Home::default()),
        ..idle(UnitAiKind::Boar)
    }
}

fn attacker(id: i32, class: ClassId, gap_squared: DistanceSquared) -> UnitSeen {
    UnitSeen {
        id: object_id(id),
        class,
        state: ObjectState::Alive,
        target: Some(object_id(9729)),
        gap_squared,
        position: Position::new(100.0, 40.0, 0.0),
        ..UnitSeen::default()
    }
}

/// Gaia's boar `id` (master 48, a predator of combat level `level` on [`STANDING`]) at
/// (20, 20), its unit AI `ai`, and player 1's villagers (id, x) ([`boar_villager`]).
fn boar_world(id: i32, (level, ai): (CombatLevel, UnitAi), villagers: &[(i32, f32)]) -> World {
    let villagers: Vec<SceneObject> = villagers
        .iter()
        .map(|&(villager, x)| boar_villager(villager, x, id))
        .collect();
    boar_world_with(id, (level, ai), &villagers)
}

/// Player 1's villager `id` at (x, 20), a walker whose unit AI targets the boar `boar`. Its
/// radius and the boar's are 0.2: the gap is the centres' distance less 0.4.
fn boar_villager(id: i32, x: f32, boar: i32) -> SceneObject {
    SceneObject {
        ai: Some(UnitAi {
            current_target: Some(object_id(boar)),
            ..idle(UnitAiKind::Civilian)
        }),
        ..SceneObject::new(
            id,
            player_id(1),
            walker(83, class::VILLAGER),
            Position::new(x, 20.0, 0.0),
        )
    }
}

/// [`boar_world`] with the `others`, each in Gaia's lists. The AI stream is seeded with 0, as
/// [`rng`] is.
fn boar_world_with(id: i32, (level, ai): (CombatLevel, UnitAi), others: &[SceneObject]) -> World {
    let boar = Master {
        line_of_sight: Distance::new(4.0),
        standing_sprite: Some(STANDING.id),
        combat_level: level,
        ..walker(48, class::PREDATOR)
    };
    let mut scene = Scene::open(GridSize::new(64, 64))
        .players(1)
        .graphic(STANDING)
        .object(SceneObject {
            ai: Some(ai),
            sprite: Some(STANDING.id),
            ..SceneObject::new(id, PlayerId::GAIA, boar, Position::new(20.0, 20.0, 0.0))
        });
    for other in others {
        scene = scene.object(SceneObject {
            seen_by: Some(VisionBits::visible(PlayerId::GAIA)),
            ..other.clone()
        });
    }
    let mut world = scene.build();
    world.streams.get_mut(Stream::Ai).set_seed(0);
    world
}

/// The target of Gaia's unit `id`'s Attack in `world`, none without one.
fn gaia_attacks(world: &World, id: i32) -> Option<ObjectId> {
    world
        .parts
        .actions
        .of(object_id(id))
        .list
        .attack()
        .and_then(|attack| attack.target)
        .map(AttackTargetId::id)
}

/// Gaia's unit `id` in `world` handles `notify` after its unit AI queued it ([`UnitAi::handle`]).
fn gaia_hears(world: &mut World, id: i32, notify: Notify) -> GenieResult<u32> {
    world
        .parts
        .unit_ai
        .get_mut(object_id(id))
        .unwrap()
        .post(notify, ObjectState::Alive)?;
    run_ai(
        world,
        (PlayerId::GAIA, object_id(id)),
        |ai, at, side, kind, rng| ai.handle(notify, Millis::ZERO, at, kind, rng, side),
    )
}

/// MarvinBlue/426048731's boar 9729 in update 4132: the hunter 10049's arrow (3 off) makes it
/// attack the hunter, forced (it was only wandering), furious at a villager; a second
/// attacker's hit (1 off) then adds to its attackers, but its `AttackObject` is not forced and
/// fails: it keeps the first.
#[test]
fn boar_attacks_its_first_attacker_and_keeps_it() {
    let mut boar_world = boar_world(
        9729,
        (CombatLevel::ANIMAL, hit_boar()),
        &[(10049, 23.4), (10050, 21.4)],
    );
    let hit = |caller| Notify {
        caller: Some(object_id(caller)),
        recipient: object_id(9729),
        kind: NotifyKind::DAMAGED,
        params: [caller, 72, 75],
    };
    let boar_damaged = |world: &mut World, notify: Notify| {
        world
            .parts
            .unit_ai
            .get_mut(object_id(9729))
            .unwrap()
            .post(notify, ObjectState::Alive)
            .unwrap();
        run_ai(
            world,
            (PlayerId::GAIA, object_id(9729)),
            |ai, at, side, _, rng| ai.boar_damaged(notify, Millis::ZERO, at, rng, side),
        )
    };
    assert_eq!(boar_damaged(&mut boar_world, hit(10049)), Ok(BREAK));
    assert_eq!(gaia_attacks(&boar_world, 9729), Some(object_id(10049)));
    let unit_ai = boar_world.parts.unit_ai.get(object_id(9729)).unwrap();
    assert_eq!(unit_ai.attackers, Attackers::of_saved(&[10049]));
    assert_eq!(
        (
            unit_ai.current_action,
            unit_ai.current_target,
            unit_ai.home.map(|home| home.furious)
        ),
        (Some(AiAction::Attack), Some(object_id(10049)), Some(true))
    );
    let boar = boar_world.players[0]
        .objects
        .by_id(object_id(9729))
        .unwrap();
    assert!(boar.flags.contains(UnitFlags::UNDER_ATTACK));
    // the nearer second attacker is the closest now, but the attack is not forced
    let _ = boar_damaged(&mut boar_world, hit(10050));
    let unit_ai = boar_world.parts.unit_ai.get(object_id(9729)).unwrap();
    assert_eq!(unit_ai.attackers, Attackers::of_saved(&[10049, 10050]));
    assert_eq!(unit_ai.current_target, Some(object_id(10049)));
    assert_eq!(gaia_attacks(&boar_world, 9729), Some(object_id(10049)));
}

/// The 4v4's boar 29825 in update 4599: its Attack on the villager 31126 completed (a stop,
/// slot 0x3c0) and the villager 27240's hit makes it attack again, forced. The stop is a call
/// on the unit at once: it empties the action list before the attack command fills it.
#[test]
fn stop_before_an_attack_command_runs_first() {
    let attacking = UnitAi {
        current_action: Some(AiAction::Attack),
        current_target: Some(object_id(31126)),
        ..hit_boar()
    };
    let mut world = boar_world(
        29825,
        (CombatLevel::ANIMAL, attacking),
        &[(31126, 21.0), (27240, 21.4)],
    );
    // its Attack on the villager 31126 first
    let first = run_ai(
        &mut world,
        (PlayerId::GAIA, object_id(29825)),
        |ai, at, side, _, rng| {
            ai.attack_command(object_id(31126), OrderForce::Forced, at, (side, rng))
        },
    );
    assert_eq!(first, Ok(Answer::Taken));
    let done = Notify::own(
        object_id(29825),
        NotifyKind::ACTION_COMPLETED,
        [600, 31126, 0],
    );
    assert_eq!(gaia_hears(&mut world, 29825, done), Ok(GO_ON));
    assert_eq!(
        world.parts.actions.of(object_id(29825)).top_action_kind(),
        None
    );
    let hit = Notify {
        caller: Some(object_id(27240)),
        recipient: object_id(29825),
        kind: NotifyKind::DAMAGED,
        params: [27240, 3, 33],
    };
    let _ = gaia_hears(&mut world, 29825, hit);
    assert_eq!(gaia_attacks(&world, 29825), Some(object_id(27240)));
    let unit_ai = world.parts.unit_ai.get(object_id(29825)).unwrap();
    assert_eq!(
        (unit_ai.current_action, unit_ai.current_target),
        (Some(AiAction::Attack), Some(object_id(27240)))
    );
}

/// `FUN_140c63e20`'s alert: a fresh search's enemies set the alert mode and start the time
/// since the last sighting again; a fresh one without clears the alert mode; a search that
/// read the last answer changes nothing.
#[test]
fn fresh_search_enemies_set_the_alert_mode() {
    let found = |fresh, enemies| Found {
        enemies,
        fresh,
        ..Found::NONE
    };
    let mut unit_ai = UnitAi {
        time_since_enemy: Seconds::new(12.0),
        ..idle(UnitAiKind::Civilian)
    };
    unit_ai.alert(found(true, true), Seconds::new(0.05));
    assert_eq!(
        (unit_ai.alert_mode, unit_ai.time_since_enemy),
        (AlertMode::Alert, Seconds::ZERO)
    );
    unit_ai.time_since_enemy = Seconds::new(3.0);
    unit_ai.alert(found(false, false), Seconds::new(0.05));
    assert_eq!(
        (unit_ai.alert_mode, unit_ai.time_since_enemy),
        (AlertMode::Alert, Seconds::new(3.0))
    );
    unit_ai.alert(found(true, false), Seconds::new(0.05));
    assert_eq!(
        (unit_ai.alert_mode, unit_ai.time_since_enemy),
        (AlertMode::Calm, Seconds::new(3.0))
    );
}

/// The 4v4's boar 29823 in update 2917 walks to its villager and meets another of the same
/// master: a boar (class 10 of combat level 1) lets it be; a wolf (combat level 4) with no
/// order attacks it, forced, unless its stance is not aggressive.
#[test]
fn unit_in_the_way_is_attacked_by_wolf_not_by_boar() {
    let notify = Notify::own(object_id(29823), NotifyKind::IN_THE_WAY, [29001, -1, -1]);
    let villagers = [(29001, 21.0), (29002, 30.0)];
    let walking = |ai| UnitAi {
        current_action: Some(AiAction::Attack),
        current_target: Some(object_id(29002)),
        ..ai
    };
    let mut world = boar_world(
        29823,
        (CombatLevel::ANIMAL, walking(hit_boar())),
        &villagers,
    );
    assert_eq!(gaia_hears(&mut world, 29823, notify), Ok(GO_ON));
    assert_eq!(gaia_attacks(&world, 29823), None);
    let wolf = walking(idle(UnitAiKind::Lion));
    let mut world = boar_world(29823, (CombatLevel::MILITARY, wolf), &villagers);
    assert_eq!(gaia_hears(&mut world, 29823, notify), Ok(GO_ON));
    assert_eq!(gaia_attacks(&world, 29823), Some(object_id(29001)));
    let unit_ai = world.parts.unit_ai.get(object_id(29823)).unwrap();
    assert_eq!(unit_ai.current_target, Some(object_id(29001)));
    let defensive = UnitAi {
        stance: UnitStance::Defensive,
        ..idle(UnitAiKind::Lion)
    };
    let mut world = boar_world(29823, (CombatLevel::MILITARY, defensive), &villagers);
    let _ = gaia_hears(&mut world, 29823, notify);
    assert_eq!(gaia_attacks(&world, 29823), None);
}

/// The standing sprite of the tests' attackers: 10 frames of 16 facets, looped.
const STANDING: Graphic = Graphic {
    id: SpriteId::named(7),
    frames: Frame::of_data(10),
    facets: FacetCount::of_data(16),
    speed_multiplier: SpeedMultiplier::ONE,
    frame_duration: Seconds::new(0.1),
    replay_delay: Seconds::ZERO,
    flags: SpriteFlags::ANIMATED,
};

/// Gaia's wolf 20 (master 126, a predator of line of sight 4 and search radius 1, standing on
/// [`STANDING`]) at (20, 20), its unit AI `ai`, and player 1's `prey` (id, class, x) at
/// (x, 20), each a walker in Gaia's lists. The wolf's last human order time is past the cap.
fn wolf_world(ai: UnitAi, prey: &[(i32, ClassId, f32)]) -> World {
    let wolf = Master {
        line_of_sight: Distance::new(4.0),
        standing_sprite: Some(STANDING.id),
        action: Some(ActionPart {
            search_radius: Distance::new(1.0),
            ..ActionPart::default()
        }),
        ..walker(126, class::PREDATOR)
    };
    let mut scene = Scene::open(GridSize::new(64, 64))
        .graphic(STANDING)
        .object(SceneObject {
            ai: Some(ai),
            sprite: Some(STANDING.id),
            ..SceneObject::new(20, PlayerId::GAIA, wolf, Position::new(20.0, 20.0, 0.0))
        });
    for &(id, class, x) in prey {
        scene = scene.object(SceneObject {
            seen_by: Some(VisionBits::visible(PlayerId::GAIA)),
            ..SceneObject::new(
                id,
                player_id(1),
                walker(i16::try_from(id).expect("a master's id"), class),
                Position::new(x, 20.0, 0.0),
            )
        });
    }
    let mut world = scene.build();
    world.players[0]
        .objects
        .by_id_mut(object_id(20))
        .unwrap()
        .last_human_order = WorldTime::from_millis(9000);
    world
}

/// The lion's idle check (`FUN_140c1fb70`): the cap on its last human order time first
/// (`FUN_140c41e50`); a villager it finds within half its line of sight (outside mode 1) it
/// attacks, not forced; finding none, any enemy within a third. Both radii 0.2: the gap is
/// the centres' distance less 0.4.
#[test]
fn an_idle_lion_attacks_what_it_finds_within_its_reach() {
    let run = |prey: (i32, ClassId, f32)| {
        let mut world = wolf_world(idle(UnitAiKind::Lion), &[prey]);
        let produced = run_ai(
            &mut world,
            (PlayerId::GAIA, object_id(20)),
            |ai, at, side, _, rng| ai.lion_idle(at, rng, side),
        )
        .unwrap();
        let wolf = world.players[0].objects.by_id(object_id(20)).unwrap();
        assert_eq!(wolf.last_human_order, HUMAN_ORDER_CAP);
        let attacks = world
            .parts
            .actions
            .of(object_id(20))
            .list
            .attack()
            .and_then(|attack| attack.target)
            .map(AttackTargetId::id);
        let ai = world.parts.unit_ai.get(object_id(20)).unwrap();
        (produced, attacks, ai.current_action, ai.current_target)
    };
    // a villager: the reach is 2 (half of 4), its gap 1.9; slot 0xb8 then gives 6 whatever it
    // did
    assert_eq!(
        run((700, class::VILLAGER, 22.3)),
        (
            IdleOutcome::Acted,
            Some(object_id(700)),
            Some(AiAction::Attack),
            Some(object_id(700))
        )
    );
    assert_eq!(
        run((700, class::VILLAGER, 22.5)),
        (IdleOutcome::Idle, None, None, None)
    );
    // any enemy: a third of 4, the gaps 1.2 and 1.4
    assert_eq!(
        run((701, class::CLASS_6, 21.6)),
        (
            IdleOutcome::Acted,
            Some(object_id(701)),
            Some(AiAction::Attack),
            Some(object_id(701))
        )
    );
    assert_eq!(
        run((701, class::CLASS_6, 21.8)),
        (IdleOutcome::Idle, None, None, None)
    );
}

/// Player 1's villager 20 at (10, 10) on its sprite, its unit AI `ai` and its actions
/// `actions`, animating at speed 1; Gaia's tree 40 at (5, 10); and player 2's unit 30 of
/// master `foe` at (12, 10), whose unit AI targets the villager; each in player 1's lists, on
/// a map of 100 by 100.
fn hit_villager_world(ai: UnitAi, actions: Option<Action>, foe: Master) -> World {
    let villager = Master {
        standing_sprite: Some(STANDING.id),
        ..villager_master()
    };
    let tree = resource(
        (40, 349, class::TREE),
        (MasterKind::TREE, GatherGroup::WOOD),
        (5.0, 10.0),
        (attribute::WOOD, 100.0),
    );
    let foe = SceneObject {
        ai: Some(UnitAi {
            current_target: Some(object_id(20)),
            ..idle(UnitAiKind::Soldier)
        }),
        ..SceneObject::new(30, player_id(2), foe, Position::new(12.0, 10.0, 0.0))
    };
    let mut scene = Scene::open(GridSize::new(100, 100))
        .players(2)
        .graphic(STANDING)
        .object(SceneObject {
            ai: Some(ai),
            actions,
            sprite: Some(STANDING.id),
            ..SceneObject::new(20, player_id(1), villager, Position::new(10.0, 10.0, 0.0))
        });
    for object in [tree, foe] {
        scene = scene.object(SceneObject {
            seen_by: Some(VisionBits::visible(player_id(1))),
            ..object
        });
    }
    let mut world = scene.build();
    world.parts.set_anim_speed(object_id(20), Speed::new(1.0));
    world.streams.get_mut(Stream::Ai).set_seed(0);
    world
}

/// Player 2's soldier master 5 (a walker of class 6).
fn foe_master() -> Master {
    walker(5, class::CLASS_6)
}

/// A villager's own copy of the villager's master 293 with object flag 0x10, an attack of 3,
/// no melee armor and no range: the villager does not fear it ([`UnitView::harmless`]).
fn harmless_villager() -> Master {
    Master {
        copy_id: Some(master_id(293)),
        standing_sprite: Some(STANDING.id),
        object_flags: ObjectFlags::SCOUT,
        hit_points: MasterHitPoints::of_data(25),
        combat: Some(CombatPart {
            weapons: vec![ClassAmount::new(4, 3), ClassAmount::new(21, 6)],
            armors: vec![ClassAmount::new(4, 0), ClassAmount::new(3, 0)],
            weapon_range: Distance::ZERO,
            ..CombatPart::default()
        }),
        ..walker(293, class::VILLAGER)
    }
}

/// The villager 20 on the gather order on tree 40 (`0x2c5`, priority 3).
fn gathering_villager() -> UnitAi {
    UnitAi {
        current_order: Some(OrderKind::Gather),
        current_action: Some(AiAction::Gather),
        current_target: Some(object_id(40)),
        order_priority: OrderPriority::of_data(3),
        ..idle(UnitAiKind::Civilian)
    }
}

/// Unit 30's hit on the villager 20 (21 hit points left).
fn villager_hit() -> Notify {
    Notify {
        caller: Some(object_id(30)),
        recipient: object_id(20),
        kind: NotifyKind::DAMAGED,
        params: [30, 21, 25],
    }
}

/// The villager 20 of `world` takes `notify` through its damage notify
/// ([`UnitAi::villager_damaged`]) after its unit AI queued it: its answer and its unit AI.
fn villager_takes(world: &mut World, notify: Notify) -> (GenieResult<u32>, UnitAi) {
    world
        .parts
        .unit_ai
        .get_mut(object_id(20))
        .unwrap()
        .post(notify, ObjectState::Alive)
        .unwrap();
    let answer = run_ai(
        world,
        (player_id(1), object_id(20)),
        |ai, at, side, kind, rng| ai.villager_damaged(notify, Millis::ZERO, at, kind, rng, side),
    );
    (
        answer,
        world.parts.unit_ai.get(object_id(20)).unwrap().clone(),
    )
}

/// The point the villager 20 of `world` walks to, none without a `MoveTo`.
fn villager_walks(world: &World) -> Option<Position> {
    world
        .parts
        .actions
        .of(object_id(20))
        .list
        .move_to()
        .map(|move_to| move_to.point)
}

/// A villager hit by another player's unit it fears ("I have %d attackers, evading them.")
/// calls for help, sends its order to the queue's end, stops its action and runs from the
/// attacker (its `MoveTo` at once, to its west, the attacker being east of it); the call keeps
/// the attacker.
#[test]
fn villager_evades_another_player_attacker() {
    let mut world = hit_villager_world(gathering_villager(), None, foe_master());
    let (answer, unit_ai) = villager_takes(&mut world, villager_hit());
    assert_eq!(answer, Ok(BREAK));
    assert_eq!(unit_ai.attackers, Attackers::of_saved(&[30]));
    let villager = world.players[1].objects.by_id(object_id(20)).unwrap();
    assert!(villager.flags.contains(UnitFlags::UNDER_ATTACK));
    let queued: Vec<_> = unit_ai
        .orders
        .iter()
        .map(|order| (order.order, order.target, order.issuer))
        .collect();
    assert_eq!(
        queued,
        vec![(
            OrderKind::Gather,
            Some(object_id(40)),
            Some(Issuer::Object(object_id(20)))
        )]
    );
    let to = villager_walks(&world).expect("its run");
    assert!(to.x() < 10.0 && to.y() == 10.0, "{to:?}");
    assert_eq!(world.parts.actions.of(object_id(20)).list.attack(), None);
    assert_eq!(unit_ai.current_action, Some(AiAction::Move));
}

/// The villager's cases 0x1fb, 0x1fc and 0x202 off its searching actions: on the move
/// action an invalidated Attack (600) makes the target go and the action stop (its `MoveTo`
/// too; records 426339330's villager 34028 in update 5296, running from its attackers); a
/// search (0x202) or the action `0x26a` changes nothing; an invalidated gather or hunt of a
/// class other than 10 would search for another of that class (not modelled).
#[test]
fn villager_invalidated_action_off_its_searches_stops_it() {
    let moving = |action| UnitAi {
        current_action: Some(action),
        current_target: Some(object_id(30)),
        ..idle(UnitAiKind::Civilian)
    };
    let walk = Action {
        list: ActionList::default().with_move_to(Some(MoveToAction {
            point: Position::new(30.0, 20.0, 0.0),
            ..MoveToAction::sub_in(MoveTo::Moving)
        })),
        ..Action::default()
    };
    let search = |ai, notify: Notify| {
        let mut world = player_unit_world(20, (villager_master(), ai), &[]);
        *world.parts.actions.of_mut(object_id(20)).unwrap() = walk.clone();
        let answer = run_ai(
            &mut world,
            (player_id(1), object_id(20)),
            |ai, at, side, kind, rng| ai.villager_search(notify, at, kind, side, rng),
        );
        let unit_ai = world.parts.unit_ai.get(object_id(20)).unwrap();
        (
            answer,
            unit_ai.current_target,
            world.parts.actions.of(object_id(20)).top_action_kind(),
        )
    };
    let notify = Notify::own(
        object_id(20),
        NotifyKind::ACTION_INVALIDATED,
        [AiAction::Attack.value(), 0, 514],
    );
    assert_eq!(
        search(moving(AiAction::Move), notify),
        (Ok(GO_ON), None, None)
    );
    for (kind, action) in [
        (NotifyKind::REQUIRES_SEARCH, AiAction::Move),
        (NotifyKind::TARGET_MOVED, AiAction::Repair),
    ] {
        assert_eq!(
            search(moving(action), Notify { kind, ..notify }),
            (Ok(GO_ON), Some(object_id(30)), Some(MOVE_TO))
        );
    }
    let gather = Notify::own(
        object_id(20),
        NotifyKind::ACTION_INVALIDATED,
        [AiAction::Gather.value(), 5, -1],
    );
    assert!(search(moving(AiAction::Move), gather).0.is_err());
    let tree = Notify::own(
        object_id(20),
        NotifyKind::ACTION_INVALIDATED,
        [AiAction::Gather.value(), 10, -1],
    );
    assert_eq!(
        search(moving(AiAction::Move), tree),
        (Ok(GO_ON), None, None)
    );
}

/// Player 1's soldier 20 (master 4, a walker of class 6, `kind` its master's kind) at `at` on
/// its sprite, its unit AI `ai`, its owner of type `player_kind`, alone on a map of 100 by 100
/// (the attack search finds nothing). The AI stream is seeded with 0, as [`rng`] is.
fn lone_soldier(ai: UnitAi, at: Position, (kind, player_kind): (MasterKind, PlayerKind)) -> World {
    let soldier = Master {
        kind,
        standing_sprite: Some(STANDING.id),
        ..walker(4, class::CLASS_6)
    };
    let mut world = Scene::open(GridSize::new(100, 100))
        .players(2)
        .kind(player_id(1), player_kind)
        .graphic(STANDING)
        .object(SceneObject {
            ai: Some(ai),
            sprite: Some(STANDING.id),
            ..SceneObject::new(20, player_id(1), soldier, at)
        })
        .build();
    world.streams.get_mut(Stream::Ai).set_seed(0);
    world
}

/// [`lone_soldier`] at (12, 13), a combat object of a human player, takes `routine` (its unit AI's
/// routine inside its update): its answer and its unit AI.
fn soldier_runs<R>(
    ai: UnitAi,
    kind: (MasterKind, PlayerKind),
    routine: impl FnOnce(
        &mut UnitAi,
        &Surroundings,
        &mut UnitView<'_, '_>,
        PlayerKind,
        &mut Well512,
    ) -> GenieResult<R>,
) -> (GenieResult<R>, UnitAi) {
    let mut world = lone_soldier(ai, Position::new(12.0, 13.0, 0.0), kind);
    let answer = run_ai(
        &mut world,
        (player_id(1), object_id(20)),
        |ai, at, side, kind, rng| routine(ai, at, side, kind, rng),
    );
    (
        answer,
        world.parts.unit_ai.get(object_id(20)).unwrap().clone(),
    )
}

/// A human player's combat object.
const HUMAN_COMBAT: (MasterKind, PlayerKind) = (MasterKind::COMBAT, PlayerKind::Human);

/// The base handler's case `0x1fb` ([`UnitAi::action_invalidated`]): an attack invalidated on
/// the attack order, or on a search (`0x202`), runs processIdle, and with nothing produced a
/// human player's unit forgets its target and stops (a computer player's tests and
/// retargeting off are not modelled); without an order the current action's invalidation
/// forgets the target and stops, with one the order's own action's (action + 100) and any
/// attack's; orders `0x2d4` and `0x2bd` on action `0x262` are not modelled; the rest go on.
#[test]
fn an_invalidated_action_of_the_order_or_an_attack_stops_the_unit() {
    let unit = |order, action| UnitAi {
        current_order: order,
        current_action: action,
        current_target: Some(object_id(30)),
        ..idle(UnitAiKind::Soldier)
    };
    let invalid = |p0, p2| Notify::own(object_id(20), NotifyKind::ACTION_INVALIDATED, [p0, 0, p2]);
    let run = |unit_ai: UnitAi, notify, player_kind| {
        soldier_runs(
            unit_ai,
            (MasterKind::COMBAT, player_kind),
            |ai, at, side, kind, rng| ai.action_invalidated(notify, at, kind, rng, side),
        )
    };
    let human = PlayerKind::Human;
    let attack = AiAction::Attack.value();
    let (answer, unit_ai) = run(
        unit(Some(OrderKind::Attack), Some(AiAction::Attack)),
        invalid(attack, 0),
        human,
    );
    assert_eq!(
        (answer, unit_ai.current_target, unit_ai.current_order),
        (Ok(GO_ON), None, None)
    );
    let (answer, unit_ai) = run(unit(None, None), invalid(attack, 0x202), human);
    assert_eq!((answer, unit_ai.current_target), (Ok(GO_ON), None));
    let (answer, _) = run(
        unit(Some(OrderKind::Attack), Some(AiAction::Attack)),
        invalid(attack, 0),
        PlayerKind::Computer,
    );
    assert!(answer.is_err());
    let off = UnitAi {
        retarget_disabled: 1,
        ..unit(Some(OrderKind::Attack), Some(AiAction::Attack))
    };
    assert!(run(off, invalid(attack, 0), human).0.is_err());
    let target = |(answer, unit_ai): (GenieResult<u32>, UnitAi)| (answer, unit_ai.current_target);
    let gone = (Ok(GO_ON), None);
    let kept = (Ok(GO_ON), Some(object_id(30)));
    let mv = AiAction::Move.value();
    assert_eq!(
        target(run(unit(None, Some(AiAction::Move)), invalid(mv, 0), human)),
        gone
    );
    assert_eq!(
        target(run(
            unit(None, Some(AiAction::Move)),
            invalid(AiAction::Gather.value(), 0),
            human
        )),
        kept
    );
    assert_eq!(
        target(run(
            unit(Some(OrderKind::Move), Some(AiAction::Move)),
            invalid(mv, 0),
            human
        )),
        gone
    );
    assert_eq!(
        target(run(
            unit(Some(OrderKind::Follow), Some(AiAction::Move)),
            invalid(mv, 0),
            human
        )),
        kept
    );
    assert_eq!(
        target(run(
            unit(Some(OrderKind::Follow), Some(AiAction::Move)),
            invalid(attack, 0),
            human
        )),
        gone
    );
    for order in [OrderKind::GroupGather, OrderKind::Defend] {
        let (answer, _) = run(
            unit(Some(order), Some(AiAction::Move)),
            invalid(mv, 0),
            human,
        );
        assert!(answer.is_err(), "{order:?}");
    }
}

/// The base handler's case `0x1fa` ([`UnitAi::action_completed`]): with an order the state
/// position takes the unit's place; without one the current action's completion, or an
/// attack's, forgets the target and stops; on the attack order an attack's completion runs
/// processIdle, and with nothing produced the target goes and the order ends; with another
/// order its own action's completion stops, order 724 on a move forgets the target and ends,
/// 701 on a move and 712 on a move of action `0x2bf` only drop the action, and an attack's
/// completion stops; the rest go on.
#[test]
fn completed_action_of_the_order_or_an_attack_stops_the_unit() {
    let unit = |order, action| UnitAi {
        current_order: order,
        current_action: action,
        current_target: Some(object_id(30)),
        ..idle(UnitAiKind::Soldier)
    };
    let done = |p0| Notify::own(object_id(20), NotifyKind::ACTION_COMPLETED, [p0, 0, 0]);
    let run = |unit_ai: UnitAi, p0| {
        soldier_runs(unit_ai, HUMAN_COMBAT, |ai, at, side, kind, rng| {
            ai.action_completed(done(p0), at, kind, rng, side)
        })
    };
    let state = |(answer, unit_ai): (GenieResult<u32>, UnitAi)| {
        (answer, unit_ai.current_target, unit_ai.current_action)
    };
    let (mv, attack) = (AiAction::Move.value(), AiAction::Attack.value());
    assert_eq!(
        state(run(unit(None, Some(AiAction::Move)), mv)),
        (Ok(GO_ON), None, None)
    );
    assert_eq!(
        state(run(unit(None, Some(AiAction::Move)), attack)),
        (Ok(GO_ON), None, None)
    );
    let (answer, unit_ai) = run(unit(None, Some(AiAction::Move)), AiAction::Gather.value());
    assert_eq!(
        (answer, unit_ai.current_target, unit_ai.current_action),
        (Ok(GO_ON), Some(object_id(30)), Some(AiAction::Move))
    );
    assert_eq!(unit_ai.state_position, Point::ORIGIN);
    let (answer, unit_ai) = run(
        unit(Some(OrderKind::Attack), Some(AiAction::Attack)),
        attack,
    );
    assert_eq!(
        (answer, unit_ai.current_target, unit_ai.current_action),
        (Ok(GO_ON), None, None)
    );
    assert_eq!(
        (unit_ai.current_order, unit_ai.state_position),
        (None, Point::new(12.0, 13.0))
    );
    assert_eq!(
        state(run(unit(Some(OrderKind::Move), Some(AiAction::Move)), mv)),
        (Ok(GO_ON), None, None)
    );
    let (answer, unit_ai) = run(unit(Some(OrderKind::GroupGather), Some(AiAction::Move)), mv);
    assert_eq!(
        (answer, unit_ai.current_target, unit_ai.current_action),
        (Ok(GO_ON), None, None)
    );
    assert_eq!(unit_ai.current_order, None);
    let (answer, unit_ai) = run(unit(Some(OrderKind::Defend), Some(AiAction::Move)), mv);
    assert_eq!(
        (answer, unit_ai.current_target, unit_ai.current_action),
        (Ok(GO_ON), Some(object_id(30)), None)
    );
    assert_eq!(unit_ai.current_order, Some(OrderKind::Defend));
    assert_eq!(
        state(run(
            unit(Some(OrderKind::Follow), Some(AiAction::MoveAside)),
            mv
        )),
        (Ok(GO_ON), Some(object_id(30)), None)
    );
    assert_eq!(
        state(run(unit(Some(OrderKind::Follow), Some(AiAction::Move)), mv)),
        (Ok(GO_ON), Some(object_id(30)), Some(AiAction::Move))
    );
    assert_eq!(
        state(run(
            unit(Some(OrderKind::Follow), Some(AiAction::Move)),
            attack
        )),
        (Ok(GO_ON), None, None)
    );
}

/// The base handler's kinds (`FUN_140c589c0`, [`UnitAi::base_notify`]): an ownership change
/// stops the unit (its stopAction first), forgets its target and empties its order queue;
/// a moved target without a saved patrol, "I'm ignoring that." and the action values change
/// nothing; readyToAttack's target out of reach gives the attack order up for a unit that
/// cannot move; a unit of a group sent off a footprint takes action value `0x2c0`; an
/// attacker no longer on the unit leaves its set; another kind is not modelled.
#[test]
fn the_base_handler_takes_each_kind_it_models() {
    let unit = || UnitAi {
        current_order: Some(OrderKind::Attack),
        current_action: Some(AiAction::Attack),
        current_target: Some(object_id(30)),
        attackers: Attackers::of_saved(&[30, 31]),
        orders: vec![QueuedOrder::default()],
        ..idle(UnitAiKind::Soldier)
    };
    let run = |unit_ai: UnitAi, kind, caller: i32, master_kind| {
        soldier_runs(
            unit_ai,
            (master_kind, PlayerKind::Human),
            |ai, at, side, player_kind, rng| {
                let notify = Notify::own(object_id(caller), kind, [0, 0, 0]);
                ai.base_notify(notify, Millis::ZERO, at, player_kind, rng, side)
            },
        )
    };
    let combat = MasterKind::COMBAT;
    let (answer, unit_ai) = run(unit(), NotifyKind::OWNER_CHANGED, 20, combat);
    assert_eq!(
        (answer, unit_ai.current_target, unit_ai.orders.len()),
        (Ok(GO_ON), None, 0)
    );
    for kind in [
        NotifyKind::TARGET_MOVED,
        NotifyKind::from(0x1f6),
        NotifyKind::from(0x1ff),
        NotifyKind::ACTION_EXECUTED,
        NotifyKind::NO_DROP_SITE,
        NotifyKind::from(0x25a),
        NotifyKind::from(0x266),
        NotifyKind::from(0x2be),
    ] {
        let (answer, unit_ai) = run(unit(), kind, 20, combat);
        assert_eq!(answer, Ok(GO_ON), "{kind:#x}");
        assert_eq!(unit_ai, unit(), "{kind:#x}");
    }
    let (answer, unit_ai) = run(unit(), NotifyKind::OUT_OF_REACH, 20, combat);
    assert_eq!(
        (answer, unit_ai.current_order),
        (Ok(GO_ON), Some(OrderKind::Attack))
    );
    let (answer, unit_ai) = run(unit(), NotifyKind::OUT_OF_REACH, 20, MasterKind::ANIMATED);
    assert_eq!((answer, unit_ai.current_order), (Ok(GO_ON), None));
    let (answer, unit_ai) = run(unit(), NotifyKind::SENT_OFF_GROUP, 20, combat);
    assert_eq!(
        (answer, unit_ai.current_action),
        (Ok(GO_ON), Some(AiAction::GroupAside))
    );
    let (answer, unit_ai) = run(unit(), NotifyKind::NO_LONGER_ON_ME, 30, combat);
    assert_eq!(
        (answer, unit_ai.attackers),
        (Ok(GO_ON), Attackers::of_saved(&[31]))
    );
    let (answer, unit_ai) = run(unit(), NotifyKind::NO_LONGER_ON_ME, 77, combat);
    assert_eq!(
        (answer, unit_ai.attackers),
        (Ok(GO_ON), Attackers::of_saved(&[30, 31]))
    );
    assert!(run(unit(), NotifyKind::from(0x999), 20, combat).0.is_err());
}

/// A villager dodges a missile's warning by its owner's chance (one AI draw); one it does
/// not dodge is its damage.
#[test]
fn villager_dodges_missile_warning_or_takes_it_as_damage() {
    let roll = rng().rand15() as i32 * 100 / 0x7fff;
    let warning = Notify {
        kind: NotifyKind::MISSILE_WARNING,
        ..villager_hit()
    };
    let warned = |chance: i32| {
        let mut world = hit_villager_world(gathering_villager(), None, foe_master());
        world.players[1].dodge_chance = Chance::of_data(u8::try_from(chance).unwrap());
        world
            .parts
            .unit_ai
            .get_mut(object_id(20))
            .unwrap()
            .post(warning, ObjectState::Alive)
            .unwrap();
        let before = world.streams.get_mut(Stream::Ai).draws;
        let answer = hears(&mut world, (player_id(1), 20), warning);
        let draws = world.streams.get_mut(Stream::Ai).draws - before;
        (answer, draws, world)
    };
    let (answer, draws, world) = warned(roll + 1);
    assert_eq!((answer, draws), (Ok(GO_ON), 1));
    let unit_ai = world.parts.unit_ai.get(object_id(20)).unwrap();
    assert_eq!(unit_ai.attackers.iter().count(), 0);
    assert_eq!(villager_walks(&world), None);
    let (answer, _, world) = warned(roll);
    assert_eq!(answer, Ok(BREAK));
    let unit_ai = world.parts.unit_ai.get(object_id(20)).unwrap();
    assert_eq!(unit_ai.attackers, Attackers::of_saved(&[30]));
}

/// A villager's harmless attacker ([`UnitView::harmless`]: a weak villager of its own
/// master) is attacked back, as a Gaia attacker is: without an order straight away
/// (`AttackObject` forced: its Attack at once), and it stops after the kill.
#[test]
fn villager_attacks_harmless_attacker_back() {
    let resting = UnitAi {
        current_order: None,
        current_action: None,
        current_target: None,
        ..gathering_villager()
    };
    let mut world = hit_villager_world(resting, None, harmless_villager());
    let (answer, unit_ai) = villager_takes(&mut world, villager_hit());
    assert_eq!(answer, Ok(BREAK));
    let attacks = world
        .parts
        .actions
        .of(object_id(20))
        .list
        .attack()
        .and_then(|attack| attack.target)
        .map(AttackTargetId::id);
    assert_eq!(attacks, Some(object_id(30)));
    assert!(unit_ai.stop_after_kill && unit_ai.orders.is_empty());
}

/// On the move action (`0x262`) a villager at a speed keeps moving (and returns 3), a speed
/// that is not a number too (`UCOMISS`); at a standstill it evades (its order queued, its run
/// at once). (The slice's stop for a villager without its animation speed is a case no world
/// reaches: every unit that moves animates.)
#[test]
fn moving_villager_keeps_its_way() {
    let walking = UnitAi {
        current_action: Some(AiAction::Move),
        ..gathering_villager()
    };
    let walk = Action {
        list: ActionList::default().with_move_to(Some(MoveToAction {
            point: Position::new(30.0, 10.0, 0.0),
            ..MoveToAction::sub_in(MoveTo::Moving)
        })),
        ..Action::default()
    };
    let run = |speed: f32| {
        let mut world = hit_villager_world(walking.clone(), Some(walk.clone()), foe_master());
        world.parts.set_anim_speed(object_id(20), Speed::new(speed));
        let (answer, unit_ai) = villager_takes(&mut world, villager_hit());
        (answer, unit_ai.orders.len(), villager_walks(&world))
    };
    let on = Some(Position::new(30.0, 10.0, 0.0));
    assert_eq!(run(1.0), (Ok(BREAK), 0, on));
    assert_eq!(run(f32::NAN), (Ok(BREAK), 0, on));
    let (answer, queued, to) = run(0.0);
    assert_eq!((answer, queued), (Ok(BREAK), 1));
    assert!(to.is_some_and(|point| point.x() < 10.0), "{to:?}");
}

/// Player 1's soldier 1 at (10, 10) (master `master`: [`guard_master`]'s), its unit AI `ai`,
/// its owner of type `player_kind`, and the `others`, each in player 1's lists, on a map of
/// 100 by 100. The AI stream is seeded with 0, as [`rng`] is.
fn guard_world(
    (master, ai): (Master, UnitAi),
    player_kind: PlayerKind,
    others: &[SceneObject],
) -> World {
    let mut world = guard_scene((master, ai), player_kind, others).build();
    world.streams.get_mut(Stream::Ai).set_seed(0);
    world
}

/// [`guard_world`]'s scene, before its build.
fn guard_scene(
    (master, ai): (Master, UnitAi),
    player_kind: PlayerKind,
    others: &[SceneObject],
) -> Scene {
    let mut scene = Scene::open(GridSize::new(100, 100))
        .players(2)
        .kind(player_id(1), player_kind)
        .graphic(STANDING)
        .object(SceneObject {
            ai: Some(ai),
            sprite: Some(STANDING.id),
            ..SceneObject::new(1, player_id(1), master, Position::new(10.0, 10.0, 0.0))
        });
    for other in others {
        scene = scene.object(SceneObject {
            seen_by: Some(VisionBits::visible(player_id(1))),
            ..other.clone()
        });
    }
    scene
}

/// The guard soldier's master 4: a walker of class 6, line of sight 4, on [`STANDING`], that
/// deals 10 a hit to a melee armor of 0 and takes a melee attack whole.
fn guard_master() -> Master {
    Master {
        line_of_sight: Distance::new(4.0),
        standing_sprite: Some(STANDING.id),
        combat: Some(CombatPart {
            weapons: vec![ClassAmount::new(4, 10)],
            armors: vec![ClassAmount::new(4, 0)],
            ..CombatPart::default()
        }),
        ..walker(4, class::CLASS_6)
    }
}

/// Player 2's unit `id` (master `id`, a walker of class `class`, 30 hit points) at (x, 10):
/// its attacks are 2 melee and 3 pierce (5; the guard takes the 2), its melee armor 0; its
/// unit AI targets the guard soldier 1.
fn foe_unit(id: i32, class: ClassId, x: f32) -> SceneObject {
    let master = Master {
        standing_sprite: Some(STANDING.id),
        combat: Some(CombatPart {
            weapons: vec![ClassAmount::new(4, 2), ClassAmount::new(3, 3)],
            armors: vec![ClassAmount::new(4, 0)],
            ..CombatPart::default()
        }),
        ..walker(i16::try_from(id).expect("a master's id"), class)
    };
    SceneObject {
        hp: HitPoints::new(30.0),
        ai: Some(UnitAi {
            current_target: Some(object_id(1)),
            ..idle(UnitAiKind::Soldier)
        }),
        ..SceneObject::new(id, player_id(2), master, Position::new(x, 10.0, 0.0))
    }
}

/// The guard soldier 1 of `world` runs `routine`, its unit AI's routine inside its update, after
/// its unit AI queued `heard`: its answer and its unit AI.
fn guard_runs<R>(
    world: &mut World,
    heard: &[Notify],
    routine: impl FnOnce(
        &mut UnitAi,
        &Surroundings,
        &mut UnitView<'_, '_>,
        PlayerKind,
        &mut Well512,
    ) -> GenieResult<R>,
) -> (GenieResult<R>, UnitAi) {
    for &notify in heard {
        world
            .parts
            .unit_ai
            .get_mut(object_id(1))
            .unwrap()
            .post(notify, ObjectState::Alive)
            .unwrap();
    }
    let answer = run_ai(
        world,
        (player_id(1), object_id(1)),
        |ai, at, side, kind, rng| routine(ai, at, side, kind, rng),
    );
    (
        answer,
        world.parts.unit_ai.get(object_id(1)).unwrap().clone(),
    )
}

/// The target of the guard soldier 1's Attack in `world`, none without one.
fn guard_attacks(world: &World) -> Option<ObjectId> {
    world
        .parts
        .actions
        .of(object_id(1))
        .list
        .attack()
        .and_then(|attack| attack.target)
        .map(AttackTargetId::id)
}

/// The guard soldier 1 of `world` is under attack.
fn guard_under_attack(world: &World) -> bool {
    let guard = world.players[1].objects.by_id(object_id(1)).unwrap();
    guard.flags.contains(UnitFlags::UNDER_ATTACK)
}

/// A walk of the guard soldier to (30, 10).
fn guard_walk() -> Action {
    Action {
        list: ActionList::default().with_move_to(Some(MoveToAction {
            point: Position::new(30.0, 10.0, 0.0),
            ..MoveToAction::sub_in(MoveTo::Moving)
        })),
        ..Action::default()
    }
}

/// The ward 20 of `owner` (a walker of class 6, master 6) at (x, 10), in state `state`.
fn ward_unit(owner: PlayerId, x: f32, state: ObjectState) -> SceneObject {
    SceneObject {
        state,
        ai: Some(idle(UnitAiKind::Soldier)),
        ..SceneObject::new(
            20,
            owner,
            walker(6, class::CLASS_6),
            Position::new(x, 10.0, 0.0),
        )
    }
}

/// A soldier on the defend order of `defend_target` within 2.5.
fn guarding(defend_target: i32) -> UnitAi {
    UnitAi {
        current_order: Some(OrderKind::Defend),
        current_action: Some(AiAction::Defend),
        current_target: ObjectId::of_field(defend_target),
        defend_target: ObjectId::of_field(defend_target),
        desired_distance: Distance::new(2.5),
        target_point: Position::new(11.0, 10.0, 0.0),
        ..idle(UnitAiKind::Soldier)
    }
}

/// The guard soldier of `world`, walking, runs processMisc's defend order
/// ([`UnitAi::defend_misc`]): its answer and its unit AI.
fn defends(world: &mut World) -> (GenieResult<MiscEnd>, UnitAi) {
    guard_runs(world, &[], |ai, at, side, kind, rng| {
        ai.defend_misc(at, kind, rng, side)
    })
}

/// processMisc's defend order: a ward the world does not hold, another player's or a dead
/// ward ends it (the target goes, the action stops: its walk too, the order is −1); the
/// routine lets Gaia's dead ward be, but Gaia is no one's ally, so its ward ends it first; an
/// unset target point takes the ward's place.
#[test]
fn defend_order_ends_with_its_ward_gone_foreign_or_dead() {
    let alive = ObjectState::Alive;
    for ward in [
        None,
        Some(ward_unit(player_id(2), 11.0, alive)),
        Some(ward_unit(player_id(1), 11.0, ObjectState::Dying)),
    ] {
        let mut world = guard_world(
            (guard_master(), guarding(20)),
            PlayerKind::Human,
            ward.as_slice(),
        );
        *world.parts.actions.of_mut(object_id(1)).unwrap() = guard_walk();
        let (answer, unit_ai) = defends(&mut world);
        assert_eq!(answer, Ok(MiscEnd::Ends));
        assert_eq!(
            (
                unit_ai.defend_target,
                unit_ai.current_target,
                unit_ai.current_order
            ),
            (None, None, None)
        );
        assert_eq!(world.parts.actions.of(object_id(1)).top_action_kind(), None);
    }
    // Gaia's dead ward ends it too: no ally test counts Gaia (MISSING.md)
    let gaia = ward_unit(PlayerId::GAIA, 11.0, ObjectState::Dying);
    let mut world = guard_scene((guard_master(), guarding(20)), PlayerKind::Human, &[gaia])
        .relation(player_id(1), PlayerId::GAIA, Relation::Ally)
        .build();
    let (answer, unit_ai) = defends(&mut world);
    assert_eq!((answer, unit_ai.defend_target), (Ok(MiscEnd::Ends), None));
    // on the attack action only the unset point takes the ward's place
    let attacking = UnitAi {
        current_action: Some(AiAction::Attack),
        target_point: Position::new(-1.0, -1.0, 3.0),
        ..guarding(20)
    };
    let ward = ward_unit(player_id(1), 11.0, alive);
    let mut world = guard_world((guard_master(), attacking), PlayerKind::Human, &[ward]);
    let (answer, unit_ai) = defends(&mut world);
    assert_eq!(
        (answer, unit_ai.target_point),
        (Ok(MiscEnd::GoesOn), Position::new(11.0, 10.0, 3.0))
    );
    assert_eq!(world.parts.actions.of(object_id(1)).top_action_kind(), None);
}

/// processMisc's defend order beyond the desired distance plus 1: the soldier walks to its
/// ward (slot 0x138: its `MoveTo` at once) or runs to the point it defends (slot 0x130,
/// forced); within it, it attacks the ward's nearest attacker (slot 0xb8, forced), or with
/// none its processIdle runs. Both radii are 0.2: the gap is the centres' distance less 0.4.
#[test]
fn defend_order_follows_its_ward_and_attacks_its_attackers() {
    let alive = ObjectState::Alive;
    let far = ward_unit(player_id(1), 20.4, alive);
    let mut world = guard_world((guard_master(), guarding(20)), PlayerKind::Human, &[far]);
    let (answer, unit_ai) = defends(&mut world);
    assert_eq!(answer, Ok(MiscEnd::Ends));
    let walk = world
        .parts
        .actions
        .of(object_id(1))
        .list
        .move_to()
        .map(|move_to| (move_to.target, move_to.range));
    assert_eq!(walk, Some((Some(object_id(20)), Distance::new(2.5))));
    assert_eq!(
        (unit_ai.current_action, unit_ai.current_target),
        (Some(AiAction::Move), Some(object_id(20)))
    );
    // (2.5 + 1)^2 is 12.25: within it (a gap of 3.5) the ward's nearest attacker is attacked
    let near = SceneObject {
        ai: Some(UnitAi {
            attackers: Attackers::of_saved(&[40]),
            ..idle(UnitAiKind::Soldier)
        }),
        ..ward_unit(player_id(1), 13.9, alive)
    };
    let others = [near, foe_unit(40, class::CLASS_6, 15.0)];
    let mut world = guard_world((guard_master(), guarding(20)), PlayerKind::Human, &others);
    let (answer, _) = defends(&mut world);
    assert_eq!(
        (answer, guard_attacks(&world)),
        (Ok(MiscEnd::Ends), Some(object_id(40)))
    );
    // a point to defend, beyond reach: the forced run to it
    let point = UnitAi {
        target_point: Position::new(20.0, 10.0, 0.0),
        ..guarding(-1)
    };
    let mut world = guard_world((guard_master(), point), PlayerKind::Human, &[]);
    let (answer, _) = defends(&mut world);
    assert_eq!(answer, Ok(MiscEnd::Ends));
    let run = world
        .parts
        .actions
        .of(object_id(1))
        .list
        .move_to()
        .map(|move_to| (move_to.point, move_to.range));
    assert_eq!(
        run,
        Some((Position::new(20.0, 10.0, 0.0), Distance::new(2.5)))
    );
    // within reach of its point, nothing
    let close = UnitAi {
        target_point: Position::new(12.0, 10.0, 0.0),
        ..guarding(-1)
    };
    let mut world = guard_world((guard_master(), close), PlayerKind::Human, &[]);
    let (answer, _) = defends(&mut world);
    assert_eq!(answer, Ok(MiscEnd::GoesOn));
    assert_eq!(world.parts.actions.of(object_id(1)).top_action_kind(), None);
}

/// Unit `caller`'s hit on the guard soldier 1 that leaves it `hp` hit points.
fn damage(caller: i32, hp: i32) -> Notify {
    Notify {
        caller: ObjectId::of_field(caller),
        recipient: object_id(1),
        kind: NotifyKind::DAMAGED,
        params: [0, hp, 0],
    }
}

/// The guard soldier 1 of `world` takes its damage case ([`UnitAi::damaged`]) on `notify`, which
/// its unit AI queued.
fn guard_damaged(world: &mut World, notify: Notify) -> (GenieResult<u32>, UnitAi) {
    guard_runs(world, &[notify], |ai, at, side, kind, rng| {
        ai.damaged(notify, Millis::ZERO, at, kind, rng, side)
    })
}

/// `FUN_140c589c0`'s case 500 up to its attack search: a known attacker of a unit whose top
/// action has a target (its Attack on 9) changes nothing but the retarget entries; the dead
/// give up; an allied attacker (its own player's) is let be once it joined the set; a human
/// player's unit on an order other than a patrol ignores it. None of them searches.
#[test]
fn the_damage_case_lets_known_dead_allied_and_ordered_ones_be() {
    let known = UnitAi {
        attackers: Attackers::of_saved(&[7]),
        retries: vec![(object_id(3), WorldTime::from_millis(900))],
        ..idle(UnitAiKind::Soldier)
    };
    let foes = [
        foe_unit(7, class::CLASS_6, 12.0),
        foe_unit(9, class::CLASS_6, 13.0),
    ];
    let mut world = guard_world((guard_master(), known), PlayerKind::Human, &foes);
    *world.parts.actions.of_mut(object_id(1)).unwrap() = Action {
        list: ActionList::default().with_attack(Some(own_attack(
            9,
            AttackState::Ready,
            Distance::new(1.0),
        ))),
        ..Action::default()
    };
    let (answer, unit_ai) = guard_damaged(&mut world, damage(7, 20));
    assert_eq!((answer, unit_ai.retries), (Ok(GO_ON), vec![]));
    assert!(!guard_under_attack(&world));
    assert!(world.spatial.searched().is_none());
    // the dead
    let mut world = guard_world(
        (guard_master(), idle(UnitAiKind::Soldier)),
        PlayerKind::Human,
        &foes,
    );
    let (answer, unit_ai) = guard_damaged(&mut world, damage(7, 0));
    assert_eq!(
        (answer, unit_ai.attackers),
        (Ok(DEAD), Attackers::of_saved(&[7]))
    );
    assert!(guard_under_attack(&world));
    // an allied attacker: its own player's
    let own = SceneObject {
        player: player_id(1),
        ..foe_unit(7, class::CLASS_6, 12.0)
    };
    let mut world = guard_world(
        (guard_master(), idle(UnitAiKind::Soldier)),
        PlayerKind::Human,
        &[own],
    );
    let (answer, unit_ai) = guard_damaged(&mut world, damage(7, 20));
    assert_eq!(
        (answer, unit_ai.attackers),
        (Ok(GO_ON), Attackers::of_saved(&[7]))
    );
    assert!(world.spatial.searched().is_none());
    // on an order: the new attacker makes it ask for help first, nothing more
    let ordered = UnitAi {
        current_order: Some(OrderKind::Attack),
        ..idle(UnitAiKind::Soldier)
    };
    let mut world = guard_world((guard_master(), ordered), PlayerKind::Human, &foes);
    let (answer, unit_ai) = guard_damaged(&mut world, damage(7, 20));
    assert_eq!((answer, unit_ai.best_this_update), (Ok(GO_ON), None));
    assert_eq!(guard_attacks(&world), None);
}

/// The unit AIs' class lists by factory type (their constructors' `+0x190`): the animals',
/// the boar's and ranged units' empty ones, the priest's 30, the trade AI's by class, a
/// computer player's soldier's with 10.
#[test]
fn each_unit_ai_kind_searches_its_constructor_classes() {
    let list = |unit_ai_kind: UnitAiKind, class: ClassId, player_kind: PlayerKind| {
        cared_classes(unit_ai_kind, Some(class), player_kind, &roles())
    };
    assert_eq!(
        list(UnitAiKind::Hunted, class::PREY, PlayerKind::Human),
        &HUNTED_CARED
    );
    assert_eq!(
        list(UnitAiKind::Predator, class::PREDATOR, PlayerKind::Human),
        &[class::LIVESTOCK, class::VILLAGER]
    );
    let none: &[ClassId] = &[];
    assert_eq!(
        list(UnitAiKind::Boar, class::PREDATOR, PlayerKind::Human),
        none
    );
    assert_eq!(
        (
            list(UnitAiKind::Ranged, class::CLASS_0, PlayerKind::Human),
            list(UnitAiKind::Ranged2, class::CLASS_0, PlayerKind::Human)
        ),
        (none, none)
    );
    assert_eq!(
        list(UnitAiKind::Lion, class::PREDATOR, PlayerKind::Human),
        &LION_CARED
    );
    assert_eq!(
        list(UnitAiKind::Priest, class::MONK, PlayerKind::Human).len(),
        30
    );
    assert_eq!(
        list(UnitAiKind::Trade, class::TRADE_BOAT, PlayerKind::Human),
        &TRADE_SHIP_CARED
    );
    assert_eq!(
        list(UnitAiKind::Trade, class::TRADE_CART, PlayerKind::Human),
        &TRADE_CART_CARED
    );
    assert_eq!(
        list(UnitAiKind::Soldier, class::CLASS_6, PlayerKind::Computer).last(),
        Some(&class::PREDATOR)
    );
    assert_eq!(
        list(UnitAiKind::Soldier, class::CLASS_6, PlayerKind::Human),
        &SOLDIER_CARED
    );
    assert_eq!(
        list(UnitAiKind::Livestock, class::LIVESTOCK, PlayerKind::Human),
        none
    );
}

/// Slot 0xa8 by factory type: the civilian and the building never attack a neutral object,
/// the base's types always; the predator and the lion spare their five classes, the priest
/// its mask below 50 (61 passes), the tower the soldier's mask; attack priority 3 keeps only
/// its classes.
#[test]
fn each_unit_ai_kind_tests_neutral_object_by_its_module() {
    for unit_ai_kind in [UnitAiKind::Civilian, UnitAiKind::Building] {
        assert!(!kind_attacks_neutral(
            unit_ai_kind,
            AttackPriority::PRIORITY_0,
            class::CLASS_6,
            &roles()
        ));
    }
    for unit_ai_kind in [
        UnitAiKind::Hunted,
        UnitAiKind::Boar,
        UnitAiKind::Ranged,
        UnitAiKind::Ranged2,
        UnitAiKind::Trade,
        UnitAiKind::Transport,
        UnitAiKind::Fishing,
        UnitAiKind::Artifact,
    ] {
        assert!(
            kind_attacks_neutral(
                unit_ai_kind,
                AttackPriority::PRIORITY_0,
                class::VILLAGER,
                &roles()
            ),
            "type {unit_ai_kind}"
        );
        assert!(
            kind_attacks_neutral(
                unit_ai_kind,
                AttackPriority::PRIORITY_3,
                class::CLASS_6,
                &roles()
            ),
            "type {unit_ai_kind}"
        );
    }
    assert!(kind_attacks_neutral(
        UnitAiKind::SiegeTower,
        AttackPriority::PRIORITY_0,
        class::BUILDING,
        &roles()
    ));
    assert!(kind_attacks_neutral(
        UnitAiKind::Livestock,
        AttackPriority::PRIORITY_0,
        class::BUILDING,
        &roles()
    ));
    for unit_ai_kind in [UnitAiKind::Predator, UnitAiKind::Lion] {
        for class in [13, 22, 52, 54, 55].map(ClassId::of_master) {
            assert!(
                !kind_attacks_neutral(unit_ai_kind, AttackPriority::PRIORITY_0, class, &roles()),
                "type {unit_ai_kind} class {class}"
            );
        }
        assert!(kind_attacks_neutral(
            unit_ai_kind,
            AttackPriority::PRIORITY_0,
            class::VILLAGER,
            &roles()
        ));
        assert!(!kind_attacks_neutral(
            unit_ai_kind,
            AttackPriority::PRIORITY_3,
            class::CLASS_6,
            &roles()
        ));
    }
    for class in [2, 3, 4, 19, 20, 21, 27, 39, 49].map(ClassId::of_master) {
        assert!(
            !kind_attacks_neutral(
                UnitAiKind::Priest,
                AttackPriority::PRIORITY_0,
                class,
                &roles()
            ),
            "class {class}"
        );
        assert!(
            !kind_attacks_neutral(
                UnitAiKind::Tower,
                AttackPriority::PRIORITY_0,
                class,
                &roles()
            ),
            "class {class}"
        );
    }
    assert!(kind_attacks_neutral(
        UnitAiKind::Priest,
        AttackPriority::PRIORITY_0,
        class::CLASS_61,
        &roles()
    ));
    assert!(!kind_attacks_neutral(
        UnitAiKind::Tower,
        AttackPriority::PRIORITY_0,
        class::CLASS_61,
        &roles()
    ));
    assert!(kind_attacks_neutral(
        UnitAiKind::Priest,
        AttackPriority::PRIORITY_0,
        class::CLASS_6,
        &roles()
    ));
    assert!(!kind_attacks_neutral(
        UnitAiKind::Priest,
        AttackPriority::PRIORITY_3,
        class::CLASS_6,
        &roles()
    ));
    assert_eq!(
        kind_attacks_neutral(
            UnitAiKind::Siege,
            AttackPriority::PRIORITY_0,
            class::TRADE_CART,
            &roles()
        ),
        siege_attacks_neutral(AttackPriority::PRIORITY_0, class::TRADE_CART, &roles())
    );
}

/// An idle human player's soldier without a target: `bestUnitToAttack` names the attacker 7,
/// which it reaches and attacks back (3, its Attack at once); a second attacker in the same
/// update (beyond the search's reach) takes the first search's answer and is ignored (it asks
/// for help and joins the set). In its no-attack stance its search finds nothing it may attack
/// and it lets it be (2). Named but not reached, it runs (3). Nothing found (an attacker of
/// class 42, which a soldier never attacks) and too slow to run, it lets it be.
#[test]
fn an_idle_soldier_attacks_back_what_best_unit_to_attack_names() {
    let foes = [
        foe_unit(7, class::CLASS_6, 12.0),
        foe_unit(8, class::CLASS_6, 17.0),
    ];
    let soldier = |ai| (guard_master(), ai);
    let mut world = guard_world(soldier(idle(UnitAiKind::Soldier)), PlayerKind::Human, &foes);
    let (seven, eight) = (damage(7, 20), damage(8, 20));
    let (answer, unit_ai) = guard_runs(&mut world, &[seven, eight], |ai, at, side, kind, rng| {
        let first = ai.damaged(seven, Millis::ZERO, at, kind, rng, side)?;
        let best = ai.best_this_update;
        let second = ai.damaged(eight, Millis::ZERO, at, kind, rng, side)?;
        Ok((first, best, second))
    });
    assert_eq!(answer, Ok((BREAK, Some(object_id(7)), GO_ON)));
    assert_eq!(guard_attacks(&world), Some(object_id(7)));
    assert_eq!(
        (
            unit_ai.current_action,
            unit_ai.current_target,
            unit_ai.attackers
        ),
        (
            Some(AiAction::Attack),
            Some(object_id(7)),
            Attackers::of_saved(&[7, 8])
        )
    );
    let passive = UnitAi {
        stance: UnitStance::NoAttack,
        ..idle(UnitAiKind::Soldier)
    };
    let mut world = guard_world(soldier(passive), PlayerKind::Human, &foes);
    let (answer, unit_ai) = guard_damaged(&mut world, seven);
    assert_eq!(
        (answer, unit_ai.best_this_update, guard_attacks(&world)),
        (Ok(GO_ON), None, None)
    );
    // named but not reached (a lake around it): it runs (3)
    let lake: Vec<Tile> = (13..=17)
        .flat_map(|x| [Tile::new(x, 8), Tile::new(x, 12)])
        .chain((9..=11).flat_map(|y| [Tile::new(13, y), Tile::new(17, y)]))
        .collect();
    let island = [foe_unit(7, class::CLASS_6, 15.0)];
    let mut world = guard_scene(
        soldier(idle(UnitAiKind::Soldier)),
        PlayerKind::Human,
        &island,
    )
    .water(lake)
    .build();
    let (answer, unit_ai) = guard_damaged(&mut world, seven);
    assert_eq!(
        (answer, unit_ai.best_this_update),
        (Ok(BREAK), Some(object_id(7)))
    );
    assert_eq!(guard_attacks(&world), None);
    let run = world
        .parts
        .actions
        .of(object_id(1))
        .list
        .move_to()
        .map(|move_to| move_to.point);
    assert!(run.is_some_and(|point| point.x() < 10.0), "{run:?}");
    let still = Master {
        move_stats: Some(walking(Speed::ZERO)),
        ..guard_master()
    };
    let unseen = foe_unit(7, class::RELIC, 12.0);
    let mut world = guard_world(
        (still, idle(UnitAiKind::Soldier)),
        PlayerKind::Human,
        &[unseen],
    );
    let (answer, unit_ai) = guard_damaged(&mut world, seven);
    assert_eq!((answer, unit_ai.best_this_update), (Ok(GO_ON), None));
}

/// A human player's soldier with a live target 9 attacks back only from a building, wall or tower
/// target (player 2's house of class 3, 27 or 52: the search from it names the attacker; from a
/// soldier it names the soldier); on a patrol its patrol is saved (its top path level, waypoint 2)
/// and, the attack taken, the order ends with its timers (an adjusted idle timeout of 750 redrawn,
/// the secondary timer capped).
#[test]
fn soldier_with_target_attacks_back_from_buildings_only() {
    let run = |class: ClassId, order: Option<OrderKind>| {
        let unit_ai = UnitAi {
            current_order: order,
            current_action: Some(AiAction::Attack),
            current_target: Some(object_id(9)),
            idle_timeout: Millis::new(3000),
            adjusted_idle_timeout: Millis::new(750),
            secondary_timer: Millis::new(900),
            ..idle(UnitAiKind::Soldier)
        };
        let target = if class == class::CLASS_6 {
            foe_unit(9, class, 13.0)
        } else {
            let (house, _) = house_and_tree();
            SceneObject {
                ai: Some(idle(UnitAiKind::Building)),
                ..SceneObject::new(
                    9,
                    player_id(2),
                    Master { class, ..house },
                    Position::new(14.0, 10.0, 0.0),
                )
            }
        };
        let others = [foe_unit(7, class::CLASS_6, 12.0), target];
        let mut world = guard_world((guard_master(), unit_ai), PlayerKind::Human, &others);
        world.parts.mover_mut(object_id(1)).unwrap().paths = vec![PathData {
            points: vec![BVector::new(10.0, 0.0, 10.0), BVector::new(20.0, 0.0, 10.0)],
            has_path: true,
            waypoint: WaypointIndex::of_data(2),
            ..PathData::without_path()
        }];
        let (answer, unit_ai) = guard_damaged(&mut world, damage(7, 20));
        (answer, unit_ai, guard_attacks(&world))
    };
    let (answer, _, attacks) = run(class::CLASS_6, None);
    assert_eq!((answer, attacks), (Ok(GO_ON), None));
    for class in [3, 27, 52].map(ClassId::of_master) {
        let (answer, unit_ai, attacks) = run(class, None);
        assert_eq!(
            (answer, attacks, unit_ai.current_target),
            (Ok(BREAK), Some(object_id(7)), Some(object_id(7))),
            "{class:?}"
        );
    }
    let (answer, unit_ai, attacks) = run(class::BUILDING, Some(OrderKind::Patrol));
    assert_eq!((answer, attacks), (Ok(BREAK), Some(object_id(7))));
    assert_eq!(unit_ai.current_order, None);
    assert_eq!(unit_ai.patrol_waypoint.get(), 2);
    assert!(unit_ai.patrol_path.is_some());
    assert_eq!(unit_ai.secondary_timer, Millis::new(750));
    assert_ne!(unit_ai.adjusted_idle_timeout, Millis::new(750));
}

/// The computer player's end of the damage case on the guard soldier 1 attacking player 2's
/// 9 while player 2's 7 attacks it (the duels from their masters: each attacks 5 and harms
/// the guard by 2, the guard harms each by 10). A wonder target, a tower against a tower, a
/// missile, and a tower under `disableTowerPriority` (but for a siege weapon) are let be. A
/// target that cannot harm it makes it ask for help and attack back, then the kill times
/// decide again: the attacker's sooner (50 hit points against 100: 5 hits against 10), or its
/// conversion, but never against a converting target. Each attack back meets the computer
/// soldier's attack check, which is not modelled (MISSING.md).
#[test]
fn computer_unit_weighs_its_target_against_its_attacker() {
    let attacking = UnitAi {
        current_action: Some(AiAction::Attack),
        current_target: Some(object_id(9)),
        ..idle(UnitAiKind::Soldier)
    };
    let hp = |object: SceneObject, hp: f32| SceneObject {
        hp: HitPoints::new(hp),
        ..object
    };
    let weighs = |guard: Master, pair: (SceneObject, SceneObject), shy: bool| {
        let others = <[SceneObject; 2]>::from(pair);
        let mut world = guard_world((guard, attacking.clone()), PlayerKind::Computer, &others);
        if shy {
            world.players[1]
                .ai_settings
                .switches
                .insert(AiSwitches::DISABLE_TOWER_PRIORITY);
        }
        let (answer, _) = guard_runs(&mut world, &[], |ai, at, side, _, rng| {
            let attacker = side.seen_unit(object_id(7))?.expect("the attacker");
            let target = side.seen_unit(object_id(9))?.expect("the target");
            ai.computer_weighs(&attacker, &target, at, rng, side)
        });
        (answer, guard_attacks(&world))
    };
    let foe = |id, class| {
        hp(
            foe_unit(id, class, if id == 7 { 12.0 } else { 13.0 }),
            if id == 7 { 50.0 } else { 100.0 },
        )
    };
    let fair = || (foe(7, class::CLASS_6), foe(9, class::CLASS_6));
    let let_be = (Ok(GO_ON), None);
    let strikes = (
        Err(GenieError::unsupported(
            "a computer player's soldier's attack check",
        )),
        None,
    );
    let guard = guard_master;
    let wonder = SceneObject {
        master: Master {
            id: WONDER,
            kind: MasterKind::BUILDING,
            ..foe(9, class::BUILDING).master
        },
        ..foe(9, class::BUILDING)
    };
    assert_eq!(
        weighs(guard(), (foe(7, class::CLASS_6), wonder), false),
        (Ok(GO_ON), None)
    );
    assert_eq!(
        weighs(guard(), (foe(7, class::TOWER), foe(9, class::TOWER)), false),
        (Ok(GO_ON), None)
    );
    let missile = SceneObject {
        master: Master {
            kind: MasterKind::MISSILE,
            ..foe(7, class::CLASS_6).master
        },
        ..foe(7, class::CLASS_6)
    };
    assert_eq!(
        weighs(guard(), (missile, foe(9, class::CLASS_6)), false),
        let_be
    );
    assert_eq!(
        weighs(
            guard(),
            (foe(7, class::TOWER), foe(9, class::CLASS_6)),
            true
        ),
        let_be
    );
    let siege = Master {
        class: class::SIEGE,
        ..guard_master()
    };
    // the attacker dies in 5 hits, the target in 10
    assert_eq!(
        weighs(siege, (foe(7, class::TOWER), foe(9, class::CLASS_6)), true),
        strikes
    );
    assert_eq!(weighs(guard(), fair(), false), strikes);
    let slow = (hp(foe(7, class::CLASS_6), 200.0), foe(9, class::CLASS_6));
    assert_eq!(weighs(guard(), slow, false), let_be);
    // a converting attacker (a priest of player 2 that converts any class)
    let priest = |object: SceneObject| SceneObject {
        ai: Some(UnitAi {
            current_target: Some(object_id(1)),
            ..idle(UnitAiKind::Priest)
        }),
        master: Master {
            action: Some(ActionPart {
                tasks: vec![Task {
                    action_kind: ActionKind::CONVERT,
                    ..Task::default()
                }],
                ..ActionPart::default()
            }),
            ..object.master.clone()
        },
        ..object
    };
    let converting = (
        priest(hp(foe(7, class::CLASS_6), 200.0)),
        foe(9, class::CLASS_6),
    );
    assert_eq!(weighs(guard(), converting, false), strikes);
    // a target that cannot harm it: the attack back, then the kill times (the attacker
    // never dies against 10 hits)
    let harmless = SceneObject {
        master: Master {
            combat: Some(CombatPart {
                armors: vec![ClassAmount::new(4, 0)],
                ..CombatPart::default()
            }),
            ..foe(9, class::CLASS_6).master
        },
        ..foe(9, class::CLASS_6)
    };
    let unharmed = Master {
        combat: Some(CombatPart {
            weapons: vec![ClassAmount::new(4, 10)],
            armors: vec![ClassAmount::new(4, 0)],
            ..CombatPart::default()
        }),
        ..guard_master()
    };
    assert_eq!(
        weighs(unharmed, (foe(7, class::CLASS_6), harmless), false),
        strikes
    );
    // a converting target, and an attacker converting too: never
    let both = (
        priest(hp(foe(7, class::CLASS_6), 1.0)),
        priest(foe(9, class::CLASS_6)),
    );
    assert_eq!(weighs(guard(), both, false), let_be);
}

/// A scout (master 448) in stand ground whose sheep died (census v4's blocker): its
/// processIdle with 0 (`FUN_140c20db0`) attacks what `bestUnitToAttack` finds next, forced
/// (its Attack at once: player 2's villager 26000 beside its state position; a sheep is none of
/// the soldier's cared classes), and its order goes; finding none, it moves to the dead sheep's
/// point.
#[test]
fn scout_whose_sheep_died_attacks_the_next_or_walks_to_its_point() {
    let scout = UnitAi {
        current_order: Some(OrderKind::Attack),
        current_action: Some(AiAction::Attack),
        current_target: Some(object_id(25954)),
        target_class: class_of(58),
        target_point: Position::new(5.0, 6.0, 0.0),
        stance: UnitStance::StandGround,
        state_position: Point::new(20.0, 20.0),
        ..idle(UnitAiKind::Soldier)
    };
    let master = Master {
        line_of_sight: Distance::new(4.0),
        standing_sprite: Some(STANDING.id),
        combat: Some(CombatPart::default()),
        ..walker(448, class::CLASS_47)
    };
    let villager = SceneObject {
        ai: Some(idle(UnitAiKind::Civilian)),
        ..SceneObject::new(
            26000,
            player_id(2),
            villager_master(),
            Position::new(20.3, 20.0, 0.0),
        )
    };
    let run = |others: &[SceneObject]| {
        let mut soldier_world = soldier_world((master.clone(), scout.clone()), None, others);
        let run_ai = run_ai(
            &mut soldier_world,
            (player_id(1), object_id(500)),
            |ai, at, side, kind, rng| ai.next_after_kill(at, kind, rng, side),
        );
        (run_ai, soldier_world)
    };
    let (result, world) = run(&[villager]);
    assert_eq!(result, Ok(()));
    assert_eq!(attacks_500(&world), Some(object_id(26000)));
    let unit_ai = ai_500(&world);
    assert_eq!(
        (
            unit_ai.current_action,
            unit_ai.current_target,
            unit_ai.current_order
        ),
        (Some(AiAction::Attack), Some(object_id(26000)), None)
    );
    assert_eq!(
        (unit_ai.last_order, unit_ai.order_priority.get()),
        (None, -1)
    );
    let (result, world) = run(&[]);
    assert_eq!(result, Ok(()));
    assert_eq!(attacks_500(&world), None);
    assert_eq!(walks_to(&world), Some(Position::new(5.0, 6.0, 0.0)));
    let unit_ai = ai_500(&world);
    assert_eq!(
        (unit_ai.current_order, unit_ai.last_order),
        (None, Some(OrderKind::Attack))
    );
}

/// The target block's attack whose target died on a patrol order: the order ends with its
/// timers reset (an adjusted idle timeout of 750 redrawn, the secondary timer capped).
#[test]
fn patrol_kill_ends_the_patrol_order_with_its_timers() {
    let scout = UnitAi {
        current_order: Some(OrderKind::Patrol),
        current_action: Some(AiAction::Attack),
        current_target: Some(object_id(25954)),
        target_point: Position::new(5.0, 6.0, 0.0),
        idle_timeout: Millis::new(3000),
        adjusted_idle_timeout: Millis::new(750),
        secondary_timer: Millis::new(900),
        ..idle(UnitAiKind::Soldier)
    };
    let mut world = soldier_world((soldier_master(), scout), None, &[]);
    let run_ai = run_ai(
        &mut world,
        (player_id(1), object_id(500)),
        |ai, at, side, kind, rng| ai.next_after_kill(at, kind, rng, side),
    );
    assert_eq!(run_ai, Ok(()));
    let unit_ai = ai_500(&world);
    assert_eq!(
        (
            unit_ai.current_order,
            unit_ai.last_order,
            unit_ai.secondary_timer
        ),
        (None, Some(OrderKind::Patrol), Millis::new(750))
    );
    assert_ne!(unit_ai.adjusted_idle_timeout, Millis::new(750));
}

/// processMisc's retarget check past its timer (`0x140c567c0`): a scout on the attack order
/// against an enemy's sheep looks no further in stand ground, nor while it swings (its Attack
/// in state 6 or 7); otherwise the search for a better target follows.
#[test]
fn retarget_check_stops_in_stand_ground_and_while_attacking() {
    let scout = |stance: UnitStance| UnitAi {
        current_order: Some(OrderKind::Attack),
        current_action: Some(AiAction::Attack),
        current_target: Some(object_id(25954)),
        stance,
        ..idle(UnitAiKind::Soldier)
    };
    let sheep = Some((PlayerId::new(1), class::LIVESTOCK));
    assert_eq!(
        scout(UnitStance::StandGround).retarget_outcome(
            sheep,
            Distance::ZERO,
            PlayerKind::Human,
            Some(ActionState::new(4)),
            Some(true),
            AiLayer {
                roles: &roles(),
                slots: slots(),
                unit_ai_tuning: &tuning().unit_ai
            }
        ),
        Ok(None)
    );
    assert_eq!(
        scout(UnitStance::Aggressive).retarget_outcome(
            sheep,
            Distance::ZERO,
            PlayerKind::Human,
            Some(ActionState::new(6)),
            Some(true),
            AiLayer {
                roles: &roles(),
                slots: slots(),
                unit_ai_tuning: &tuning().unit_ai
            }
        ),
        Ok(None)
    );
    assert_eq!(
        scout(UnitStance::Aggressive).retarget_outcome(
            sheep,
            Distance::ZERO,
            PlayerKind::Human,
            Some(ActionState::new(4)),
            Some(true),
            AiLayer {
                roles: &roles(),
                slots: slots(),
                unit_ai_tuning: &tuning().unit_ai
            }
        ),
        Ok(Some(RetargetSearch {
            building: false,
            work_state: false
        }))
    );
    // a building target is searched past even while attacking
    assert_eq!(
        scout(UnitStance::Aggressive).retarget_outcome(
            Some((PlayerId::new(1), class::BUILDING)),
            Distance::ZERO,
            PlayerKind::Computer,
            Some(ActionState::new(6)),
            Some(true),
            AiLayer {
                roles: &roles(),
                slots: slots(),
                unit_ai_tuning: &tuning().unit_ai
            }
        ),
        Ok(Some(RetargetSearch {
            building: true,
            work_state: true
        }))
    );
    // without an Attack in the action list (slot 0x538 with 9) there is no search: a king
    // entering a castle, its target a building (records 426406723's type-2 unit AIs)
    let entering = UnitAi {
        current_order: Some(OrderKind::Enter),
        current_action: Some(AiAction::Enter),
        current_target: Some(object_id(3301)),
        stance: UnitStance::Aggressive,
        ..idle(UnitAiKind::Hunted)
    };
    assert_eq!(
        entering.retarget_outcome(
            Some((PlayerId::new(8), class::BUILDING)),
            Distance::ZERO,
            PlayerKind::Human,
            Some(ActionState::new(4)),
            Some(false),
            AiLayer {
                roles: &roles(),
                slots: slots(),
                unit_ai_tuning: &tuning().unit_ai
            }
        ),
        Ok(None)
    );
    // an action list the slice does not hold stops at the state's test
    assert!(
        entering
            .retarget_outcome(
                Some((PlayerId::new(8), class::BUILDING)),
                Distance::ZERO,
                PlayerKind::Human,
                None,
                None,
                AiLayer {
                    roles: &roles(),
                    slots: slots(),
                    unit_ai_tuning: &tuning().unit_ai
                }
            )
            .is_err()
    );
}

#[test]
fn lion_attacks_by_its_priority_or_its_classes() {
    assert!(lion_may_attack(
        AttackPriority::PRIORITY_1,
        class::BUILDING,
        &roles()
    ));
    assert!(!lion_may_attack(
        AttackPriority::PRIORITY_3,
        class::VILLAGER,
        &roles()
    ));
    assert!(lion_may_attack(
        AttackPriority::PRIORITY_3,
        class::BUILDING,
        &roles()
    ));
    assert!(lion_may_attack(
        AttackPriority::PRIORITY_0,
        class::VILLAGER,
        &roles()
    ));
    assert!(!lion_may_attack(
        AttackPriority::PRIORITY_0,
        class::SIEGE,
        &roles()
    ));
}

/// Player 1's hunter 10041 (a villager that hunts predators) at (20, 20) on its Hunt of Gaia's
/// boar 9731 at (20.9, 20), which targets it (the hunt order's Hunt at once, attacking: state
/// 12), and the `others`, each in player 1's lists. The AI stream is seeded with 0.
fn hunter_world(others: &[SceneObject]) -> World {
    let hunter = Master {
        standing_sprite: Some(STANDING.id),
        action: Some(ActionPart {
            tasks: vec![Task {
                action_kind: HUNT,
                object_group: Some(class::PREDATOR),
                attribute_kind: Some(attribute::FOOD),
                work_range: Distance::new(0.5),
                ..Task::default()
            }],
            drop_sites: vec![ids::TOWN_CENTRE],
            ..ActionPart::default()
        }),
        ..villager_master()
    };
    let boar = Master {
        standing_sprite: Some(STANDING.id),
        combat_level: CombatLevel::ANIMAL,
        ..walker(48, class::PREDATOR)
    };
    let mut scene = Scene::open(GridSize::new(64, 64))
        .players(2)
        .graphic(STANDING)
        .object(SceneObject {
            ai: Some(idle(UnitAiKind::Civilian)),
            sprite: Some(STANDING.id),
            ..SceneObject::new(10041, player_id(1), hunter, Position::new(20.0, 20.0, 0.0))
        })
        .object(SceneObject {
            ai: Some(UnitAi {
                current_target: Some(object_id(10041)),
                ..hit_boar()
            }),
            sprite: Some(STANDING.id),
            seen_by: Some(VisionBits::visible(player_id(1))),
            ..SceneObject::new(9731, PlayerId::GAIA, boar, Position::new(20.9, 20.0, 0.0))
        });
    for other in others {
        scene = scene.object(SceneObject {
            seen_by: Some(VisionBits::visible(player_id(1))),
            ..other.clone()
        });
    }
    let mut world = scene.build();
    world.streams.get_mut(Stream::Ai).set_seed(0);
    let hunt = queued(OrderKind::Hunt, 9731);
    let order_outcome = run_ai(
        &mut world,
        (player_id(1), object_id(10041)),
        |ai, at, side, kind, rng| {
            ai.process_order(
                &hunt,
                Some((at, kind)),
                None,
                rng,
                side,
                AiLayer {
                    roles: &at.roles,
                    slots: at.slots,
                    unit_ai_tuning: &at.unit_ai_tuning,
                },
            )
        },
    );
    assert_eq!(order_outcome, Ok(OrderOutcome::Taken));
    world
}

/// MarvinBlue/426048731's villager 10041 in update 4839, hunting the boar 9731 (its Hunt in
/// state 12) that hits it: the boar is its target, so it calls nobody for help; it is under
/// attack, and the hunt order with a class-10 target and a Gaia attacker ignores the damage.
/// Then player 2's villager 9800, which is not its target, hits it: it calls for help and
/// evades (its hunt order back into the queue, its actions stopped, a run from its
/// attackers: its `MoveTo` at once).
#[test]
fn hunter_hit_by_its_boar_ignores_the_damage() {
    let hit = |caller: i32| Notify {
        caller: Some(object_id(caller)),
        recipient: object_id(10041),
        kind: NotifyKind::DAMAGED,
        params: [caller, 34, 40],
    };
    let enemy = SceneObject {
        ai: Some(UnitAi {
            current_target: Some(object_id(10041)),
            ..idle(UnitAiKind::Civilian)
        }),
        ..SceneObject::new(
            9800,
            player_id(2),
            walker(293, class::VILLAGER),
            Position::new(21.0, 21.0, 0.0),
        )
    };
    let mut hunter_world = hunter_world(&[enemy]);
    let damaged = |world: &mut World, notify: Notify| {
        world
            .parts
            .unit_ai
            .get_mut(object_id(10041))
            .unwrap()
            .post(notify, ObjectState::Alive)
            .unwrap();
        run_ai(
            world,
            (player_id(1), object_id(10041)),
            |ai, at, side, kind, rng| {
                ai.villager_damaged(notify, Millis::ZERO, at, kind, rng, side)
            },
        )
    };
    assert_eq!(damaged(&mut hunter_world, hit(9731)), Ok(GO_ON));
    let unit_ai = hunter_world.parts.unit_ai.get(object_id(10041)).unwrap();
    assert_eq!(unit_ai.attackers, Attackers::of_saved(&[9731]));
    assert_eq!(
        (unit_ai.current_order, unit_ai.current_target),
        (Some(OrderKind::Hunt), Some(object_id(9731)))
    );
    let hunter = hunter_world.players[1]
        .objects
        .by_id(object_id(10041))
        .unwrap();
    assert!(hunter.flags.contains(UnitFlags::UNDER_ATTACK));
    assert_eq!(
        hunter_world
            .parts
            .actions
            .of(object_id(10041))
            .top_action_kind(),
        Some(HUNT)
    );
    // another player's attacker that is not its target: the call for help, then the evasion
    assert_eq!(damaged(&mut hunter_world, hit(9800)), Ok(BREAK));
    let unit_ai = hunter_world.parts.unit_ai.get(object_id(10041)).unwrap();
    assert_eq!(unit_ai.attackers, Attackers::of_saved(&[9731, 9800]));
    assert_eq!(
        (unit_ai.current_order, unit_ai.current_target),
        (None, None)
    );
    assert!(
        unit_ai
            .orders
            .iter()
            .any(|queued_order| queued_order.order == OrderKind::Hunt)
    );
    assert_eq!(
        hunter_world
            .parts
            .actions
            .of(object_id(10041))
            .top_action_kind(),
        Some(MOVE_TO)
    );
}

#[test]
fn closest_valid_attacker_skips_classes_and_flips_coin_on_tie() {
    let mut unit_ai = hit_boar();
    unit_ai.attackers = Attackers::of_saved(&[1, 2, 3]);
    let at = Surroundings {
        units: vec![
            attacker(1, class::SIEGE, DistanceSquared::new(0.5)),
            attacker(2, class::VILLAGER, DistanceSquared::new(4.0)),
            attacker(3, class::VILLAGER, DistanceSquared::new(4.03)),
        ],
        roles: roles(),
        slots: slots(),
        unit_ai_tuning: tuning().unit_ai,
        ..Surroundings::default()
    };
    // 1 is of class 13; 3 lies within 0.0625 of 2: the coin decides
    let mut generator = rng();
    let mut coin = rng();
    let picked = unit_ai.closest_valid_attacker(&at, &mut generator);
    let expected = if coin.rand15() & 1 == 0 { 2 } else { 3 };
    assert_eq!(picked, ObjectId::of_field(expected));
}

/// The attackers' upkeep: of boar 9729's two attackers, the villager 10049 that no longer
/// targets it leaves first, then the dying 10050 (one an update); with none left the boar,
/// still under attack, no longer is on the next.
#[test]
fn an_attacker_that_no_longer_targets_the_unit_leaves_one_an_update() {
    let walked_off = SceneObject {
        ai: Some(idle(UnitAiKind::Civilian)),
        ..boar_villager(10049, 21.0, 9729)
    };
    let dead = SceneObject {
        state: ObjectState::Dying,
        ..boar_villager(10050, 21.0, 9729)
    };
    let attacked = UnitAi {
        attackers: Attackers::of_saved(&[10049, 10050]),
        ..hit_boar()
    };
    let mut boar_world_with =
        boar_world_with(9729, (CombatLevel::ANIMAL, attacked), &[walked_off, dead]);
    boar_world_with.players[0]
        .objects
        .by_id_mut(object_id(9729))
        .unwrap()
        .flags
        .insert(UnitFlags::UNDER_ATTACK);
    let upkeep = |world: &mut World| {
        run_ai(
            world,
            (PlayerId::GAIA, object_id(9729)),
            |ai, at, side, _, _| {
                ai.keep_attackers(at, side);
                Ok(())
            },
        )
        .unwrap();
        let boar = world.players[0].objects.by_id(object_id(9729)).unwrap();
        let ai = world.parts.unit_ai.get(object_id(9729)).unwrap();
        (
            ai.attackers.clone(),
            boar.flags.contains(UnitFlags::UNDER_ATTACK),
        )
    };
    assert_eq!(
        upkeep(&mut boar_world_with),
        (Attackers::of_saved(&[10050]), true)
    );
    assert_eq!(upkeep(&mut boar_world_with), (Attackers::default(), true));
    assert_eq!(upkeep(&mut boar_world_with), (Attackers::default(), false));
}

/// MarvinBlue/426048731's boar 9729 in updates 4157 to 4159: the hunter 10049 moves off, its
/// Hunt's teardown lets the boar go (notify 699 with the hunter's class); the boar's AI takes
/// it, as 10049 is one of its attackers, and its base handler drops the attacker. A 699 from
/// a unit that is not attacking it is ignored.
#[test]
fn released_attacker_leaves_the_set_through_notify_699() {
    let mut unit_ai = hit_boar();
    unit_ai.attackers = Attackers::of_saved(&[10049]);
    let released = |caller| Notify {
        caller,
        recipient: object_id(9729),
        kind: NotifyKind::NO_LONGER_ON_ME,
        params: [4, 0, 0],
    };
    unit_ai
        .post(released(Some(object_id(10050))), ObjectState::Alive)
        .unwrap();
    assert_eq!(unit_ai.notifies, Vec::new());
    unit_ai
        .post(released(Some(object_id(10049))), ObjectState::Alive)
        .unwrap();
    assert_eq!(unit_ai.notifies, vec![released(Some(object_id(10049)))]);
    let mut world = boar_world(9729, (CombatLevel::ANIMAL, unit_ai), &[(10049, 23.0)]);
    let run_ai = run_ai(
        &mut world,
        (PlayerId::GAIA, object_id(9729)),
        |ai, at, side, kind, rng| {
            ai.base_notify(
                released(Some(object_id(10049))),
                Millis::ZERO,
                at,
                kind,
                rng,
                side,
            )
        },
    );
    let ai = world.parts.unit_ai.get(object_id(9729)).unwrap();
    assert_eq!(
        (run_ai, ai.attackers.clone()),
        (Ok(GO_ON), Attackers::default())
    );
}

/// A builder walking to its foundation (busy: a `MoveTo` of its own) keeps its idle timer and
/// its secondary timer at 0 and redraws its look-around timeout; its state position follows
/// it, as it has an order. On the attack order, its target gone (the target block forgets
/// it), the civilian's processMisc stops the action and ends the order (slot 0xb0 with 0).
#[test]
fn builder_idle_timer_stays_0_and_its_look_around_is_redrawn() {
    let walking = Action {
        list: ActionList::default().with_move_to(Some(MoveToAction {
            point: Position::new(30.0, 88.0, 0.0),
            ..MoveToAction::sub_in(MoveTo::Moving)
        })),
        ..Action::default()
    };
    let building = UnitAi {
        current_order: Some(OrderKind::Build),
        current_action: Some(AiAction::Build),
        current_target: Some(object_id(10028)),
        idle_timer: Millis::new(120),
        secondary_timer: Millis::new(10),
        look_around_timer: Millis::new(990),
        ..idle(UnitAiKind::Civilian)
    };
    let (house, _) = house_and_tree();
    let foundation = SceneObject {
        desolid: true,
        state: ObjectState::Foundation,
        ..SceneObject::new(10028, player_id(1), house, Position::new(30.0, 88.0, 0.0))
    };
    let builder = (player_id(1), 9175);
    let mut world = builder_world(building.clone(), Some(walking.clone()), Some(foundation));
    world.streams.get_mut(Stream::Ai).set_seed(0);
    let (answer, unit_ai, draws) = update_of(&mut world, builder, Millis::new(50), WorldTime::ZERO);
    assert_eq!(answer, Ok(None));
    assert_eq!(
        (
            unit_ai.idle_timer,
            unit_ai.secondary_timer,
            unit_ai.look_around_timer
        ),
        (Millis::ZERO, Millis::ZERO, Millis::ZERO)
    );
    assert_eq!(
        (unit_ai.look_around_timeout, draws),
        (Millis::new(4500 + 7241 % 1000), 1)
    );
    assert_eq!(unit_ai.state_position, Point::new(29.5, 89.5));
    let attacking = UnitAi {
        current_order: Some(OrderKind::Attack),
        ..building
    };
    let mut world = builder_world(attacking, Some(walking), None);
    let (_, unit_ai, _) = update_of(&mut world, builder, Millis::new(50), WorldTime::ZERO);
    assert_eq!(
        world.parts.actions.of(object_id(9175)).top_action_kind(),
        None
    );
    assert_eq!(
        (
            unit_ai.current_order,
            unit_ai.last_order,
            unit_ai.current_action
        ),
        (None, Some(OrderKind::Attack), None)
    );
}

#[test]
fn work_notify_sets_the_order_and_redraws_timeout_left_by_order_711() {
    let mut unit_ai = UnitAi {
        current_order: Some(OrderKind::Build),
        current_action: Some(AiAction::Build),
        current_target: Some(object_id(10028)),
        adjusted_idle_timeout: Millis::new(750),
        idle_timeout: Millis::new(3000),
        secondary_timer: Millis::new(900),
        ..idle(UnitAiKind::Civilian)
    };
    let mut rng = rng();
    unit_ai
        .notify_work(
            Some(OrderKind::Hunt),
            Some(AiAction::Hunt),
            Some((object_id(9365), class::TREE)),
            Position::new(24.5, 97.5, 0.0),
            Notifies::Cleared,
            &mut rng,
        )
        .unwrap();
    assert_eq!(
        (
            unit_ai.current_order,
            unit_ai.current_action,
            unit_ai.current_target,
            rng.draws
        ),
        (
            Some(OrderKind::Hunt),
            Some(AiAction::Hunt),
            Some(object_id(9365)),
            0
        )
    );
    unit_ai.current_order = Some(OrderKind::Patrol);
    unit_ai
        .notify_work(
            Some(OrderKind::Hunt),
            Some(AiAction::Hunt),
            Some((object_id(9365), class::TREE)),
            Position::new(24.5, 97.5, 0.0),
            Notifies::Cleared,
            &mut rng,
        )
        .unwrap();
    assert_eq!((rng.draws, unit_ai.secondary_timer), (1, Millis::new(750)));
    assert_ne!(unit_ai.adjusted_idle_timeout, Millis::new(750));
}

/// records/425906646's update 1404: player 8's unit on the defend stance (1). Within 5 tiles of
/// its state position (from its footprint of 0.5) it goes on to the retarget check; 5.1 tiles
/// off it stops, forgets its target and walks back to its state position (`FUN_140c5e7d0`),
/// then goes on to the retarget check (its timer not used up).
#[test]
fn defending_unit_beyond_five_tiles_of_its_state_position_stops_to_walk_back() {
    let defender = |x: f32| {
        let unit_ai = UnitAi {
            stance: UnitStance::Defensive,
            current_order: None,
            current_action: Some(AiAction::Build),
            current_target: Some(object_id(4321)),
            state_position: Point::new(10.0, 10.0),
            retarget_timer: Millis::new(u32::MAX),
            ..idle(UnitAiKind::Soldier)
        };
        let unit = lone_unit(player_id(1), unit_ai, Position::new(x, 10.0, 0.0));
        let master = Master {
            radius: Footprint::new(0.5, 0.5),
            ..unit.master
        };
        lone_world(SceneObject { master, ..unit }, &[])
    };
    let mut world = defender(15.4);
    let (answer, _) = lone_retarget_check(&mut world);
    assert!(answer.is_ok());
    assert_eq!(lone_walks(&world), None);
    let mut world = defender(15.6);
    let (answer, unit_ai) = lone_retarget_check(&mut world);
    assert_eq!(answer, Ok(RetargetCheck::NotDue));
    assert_eq!(
        lone_walks(&world),
        Some((Position::new(10.0, 10.0, 0.0), Distance::new(1.0)))
    );
    assert_eq!(
        (
            unit_ai.current_action,
            unit_ai.current_target,
            unit_ai.last_target
        ),
        (Some(AiAction::Move), None, Some(object_id(4321)))
    );
}

/// Player 1's lone unit 50 of `world` runs its retarget check ([`UnitAi::retarget_check`]) at
/// time 0: its answer and its unit AI.
fn lone_retarget_check(world: &mut World) -> (GenieResult<RetargetCheck>, UnitAi) {
    let answer = run_ai(
        world,
        (player_id(1), object_id(50)),
        |ai, at, side, kind, rng| ai.retarget_check(WorldTime::ZERO, kind, at, rng, side),
    );
    (
        answer,
        world.parts.unit_ai.get(object_id(50)).unwrap().clone(),
    )
}

/// The target of the lone unit 50's Attack in `world`, none without one.
fn lone_attacks(world: &World) -> Option<ObjectId> {
    world
        .parts
        .actions
        .of(object_id(50))
        .list
        .attack()
        .and_then(|attack| attack.target)
        .map(AttackTargetId::id)
}

/// `owner`'s unit 50 at `at` of the [`lone_master`] of its unit AI `unit_ai`, on its sprite.
fn lone_unit(owner: PlayerId, unit_ai: UnitAi, at: Position) -> SceneObject {
    let master = Master {
        standing_sprite: Some(STANDING.id),
        ..lone_master(unit_ai.ai_kind)
    };
    SceneObject {
        ai: Some(unit_ai),
        sprite: Some(STANDING.id),
        ..SceneObject::new(50, owner, master, at)
    }
}

/// A map of 128 by 128 with the `unit` and the `others`, each seen by the unit's owner
/// ([`finish`] builds it).
fn lone_scene(unit: SceneObject, others: &[SceneObject]) -> Scene {
    let owner = unit.player;
    let mut scene = Scene::open(GridSize::new(128, 128))
        .players(2)
        .graphic(STANDING)
        .object(unit);
    for other in others {
        scene = scene.object(SceneObject {
            seen_by: Some(VisionBits::visible(owner)),
            ..other.clone()
        });
    }
    scene
}

/// The world of `scene`, its AI stream seeded with 0, as [`rng`] is.
fn finish(scene: Scene) -> World {
    let mut world = scene.build();
    world.streams.get_mut(Stream::Ai).set_seed(0);
    world
}

/// [`lone_scene`]'s world.
fn lone_world(unit: SceneObject, others: &[SceneObject]) -> World {
    finish(lone_scene(unit, others))
}

/// The `unit` on its way to `to` (its own `MoveTo`), so that a stop shows.
fn on_its_way(unit: SceneObject, to: Position) -> SceneObject {
    SceneObject {
        actions: Some(Action {
            list: ActionList::default().with_move_to(Some(MoveToAction {
                point: to,
                ..MoveToAction::sub_in(MoveTo::Moving)
            })),
            ..Action::default()
        }),
        ..unit
    }
}

/// The lone unit 50 of `world` holds no action.
fn lone_stopped(world: &World) -> bool {
    world
        .parts
        .actions
        .of(object_id(50))
        .top_action_kind()
        .is_none()
}

/// [`lone_world`] with player 1's unit at (30, 30), its unit AI `unit_ai`.
fn lone_unit_world(unit_ai: UnitAi) -> World {
    lone_world(
        lone_unit(player_id(1), unit_ai, Position::new(30.0, 30.0, 0.0)),
        &[],
    )
}

/// Gaia's animal 50 at `at`, its unit AI `unit_ai` ([`lone_unit`]).
fn animal(unit_ai: UnitAi, at: Position) -> SceneObject {
    lone_unit(PlayerId::GAIA, unit_ai, at)
}

/// [`lone_world`] with the `animal`, the `others` and the AI stream `stream`.
fn animal_world(animal: SceneObject, others: &[SceneObject], stream: Well512) -> World {
    let mut world = lone_world(animal, others);
    *world.streams.get_mut(Stream::Ai) = stream;
    world
}

/// The master of a lone unit of `kind`: a building (a moving object of speed 0 that may not
/// walk, master 70) for a building's or a tower's unit AI, a villager for a civilian's, a sheep
/// (class 58) for livestock, a boar (class 11, master 48), a deer (class 9, master 65, of line
/// of sight 2, search radius 1 and speed 1.2), a transport ship (class 20, master 545), else a
/// walker of class 6 (master 4).
fn lone_master(kind: UnitAiKind) -> Master {
    match kind {
        UnitAiKind::Building | UnitAiKind::Tower => {
            let (house, _) = house_and_tree();
            Master {
                terrain_restriction: Some(RestrictionId::named(7)),
                move_stats: Some(walking(Speed::ZERO)),
                ..house
            }
        }
        UnitAiKind::Civilian => villager_master(),
        UnitAiKind::Livestock => walker(594, class::LIVESTOCK),
        UnitAiKind::Boar => walker(48, class::CLASS_11),
        UnitAiKind::Hunted => Master {
            line_of_sight: Distance::new(2.0),
            action: Some(ActionPart {
                search_radius: Distance::new(1.0),
                ..ActionPart::default()
            }),
            move_stats: Some(walking(Speed::new(1.2))),
            ..walker(65, class::PREY)
        },
        UnitAiKind::Transport => walker(545, class::TRANSPORT),
        _ => walker(4, class::CLASS_6),
    }
}

/// `owner`'s unit `id` of `world` runs its full update ([`UnitAi::update`], `dt` at `now`):
/// its answer, its unit AI and the AI stream's draws.
fn update_of(
    world: &mut World,
    (owner, id): (PlayerId, i32),
    dt: Millis,
    now: WorldTime,
) -> (GenieResult<Option<AfterOrder>>, UnitAi, u32) {
    let before = world.streams.get_mut(Stream::Ai).draws;
    let answer = run_ai(world, (owner, object_id(id)), |ai, at, side, kind, rng| {
        ai.update(dt, now, kind, at, rng, side)
    });
    let draws = world.streams.get_mut(Stream::Ai).draws - before;
    (
        answer,
        world.parts.unit_ai.get(object_id(id)).unwrap().clone(),
        draws,
    )
}

/// [`update_of`] player 1's lone unit 50.
fn lone_step(
    world: &mut World,
    dt: Millis,
    now: WorldTime,
) -> (GenieResult<Option<AfterOrder>>, UnitAi, u32) {
    update_of(world, (player_id(1), 50), dt, now)
}

/// [`update_of`] Gaia's lone animal 50, `dt` at time 0.
fn animal_step(world: &mut World, dt: Millis) -> (GenieResult<Option<AfterOrder>>, UnitAi, u32) {
    update_of(world, (PlayerId::GAIA, 50), dt, WorldTime::ZERO)
}

/// The point and the range of the lone unit 50's `MoveTo` in `world`, none without one.
fn lone_walks(world: &World) -> Option<(Position, Distance)> {
    world
        .parts
        .actions
        .of(object_id(50))
        .list
        .move_to()
        .map(|move_to| (move_to.point, move_to.range))
}

#[test]
fn retarget_waits_for_the_timer() {
    let building = idle(UnitAiKind::Building);
    let mut world = lone_unit_world(building);
    let (_, unit_ai, draws) = lone_step(&mut world, Millis::new(50), WorldTime::from_millis(899));
    assert_eq!((unit_ai.retarget_timer, draws), (Millis::new(900), 0));
    let (_, unit_ai, _) = lone_step(&mut world, Millis::new(50), WorldTime::from_millis(900));
    // the first rand15 after seed 0 is 7241
    assert_eq!(
        (unit_ai.last_retarget_time, unit_ai.retarget_timer),
        (WorldTime::from_millis(900), Millis::new(750 + 7241 % 500))
    );
}

#[test]
fn last_retarget_ahead_of_the_world_time_wraps_and_retargets() {
    let tower = UnitAi {
        last_retarget_time: WorldTime::from_millis(400),
        ..idle(UnitAiKind::Tower)
    };
    let mut world = lone_unit_world(tower);
    let (_, unit_ai, draws) = lone_step(&mut world, Millis::new(50), WorldTime::from_millis(100));
    assert_eq!(
        (unit_ai.last_retarget_time, draws),
        (WorldTime::from_millis(100), 1)
    );
}

#[test]
fn villagers_and_livestock_never_retarget() {
    for kind in [UnitAiKind::Civilian, UnitAiKind::Livestock] {
        let mut world = lone_unit_world(idle(kind));
        let (_, unit_ai, draws) =
            lone_step(&mut world, Millis::new(50), WorldTime::from_millis(5000));
        assert_eq!(
            (unit_ai.retarget_timer, draws),
            (Millis::new(900), 0),
            "type {kind}"
        );
    }
}

#[test]
fn the_look_around_timeout_is_redrawn_once_exceeded() {
    let still = UnitAi {
        retarget_disabled: 1,
        ..idle(UnitAiKind::Civilian)
    };
    let mut world = lone_unit_world(still);
    let (_, unit_ai, draws) = lone_step(&mut world, Millis::new(1000), WorldTime::ZERO);
    assert_eq!((unit_ai.look_around_timer, draws), (Millis::new(1000), 0));
    let (_, unit_ai, _) = lone_step(&mut world, Millis::new(1), WorldTime::ZERO);
    assert_eq!(
        (unit_ai.look_around_timer, unit_ai.look_around_timeout),
        (Millis::ZERO, Millis::new(4500 + 7241 % 1000))
    );
}

#[test]
fn retarget_draws_before_look_around() {
    let soldier = UnitAi {
        look_around_timer: Millis::new(1000),
        ..idle(UnitAiKind::Soldier)
    };
    let mut world = lone_unit_world(soldier);
    let (_, unit_ai, _) = lone_step(&mut world, Millis::new(1), WorldTime::from_millis(900));
    assert_eq!(unit_ai.retarget_timer, Millis::new(750 + 7241 % 500));
    assert_eq!(
        unit_ai.look_around_timeout,
        Millis::new(4500 + 25269 % 1000)
    );
}

/// `FUN_140c64cf0`: the timer is a draw of the stream modulo half the span plus three
/// quarters of it; a span below 1000 is 1000.
#[test]
fn the_randomized_retarget_timer_draws_around_its_span() {
    let mut unit_ai = idle(UnitAiKind::Civilian);
    unit_ai.set_randomized_retarget_timer(Millis::new(4000), &mut rng());
    assert_eq!(unit_ai.retarget_timer, Millis::new(7241 % 2000 + 3000));
    unit_ai.set_randomized_retarget_timer(Millis::new(10), &mut rng());
    assert_eq!(unit_ai.retarget_timer, Millis::new(7241 % 500 + 750));
    unit_ai.set_randomized_retarget_timer(Millis::new(u32::MAX), &mut rng());
    assert_eq!(
        unit_ai.retarget_timer,
        Millis::new(7241_u32.wrapping_add(u32::MAX.wrapping_mul(3) >> 2))
    );
}

#[test]
fn the_idle_timer_restarts_after_its_timeout_and_the_secondary_timer_stops_at_0() {
    let waiting = UnitAi {
        secondary_timer: Millis::new(750),
        ..idle(UnitAiKind::Civilian)
    };
    let mut world = lone_unit_world(waiting);
    let (_, unit_ai, _) = lone_step(&mut world, Millis::new(3001), WorldTime::ZERO);
    assert_eq!(
        (unit_ai.idle_timer, unit_ai.secondary_timer),
        (Millis::ZERO, Millis::ZERO)
    );
}

/// A unit AI type the slice does not model: the ranged unit's, which only a load makes. Its
/// update stops at its retarget check, before its idle check. (The double's test for a unit
/// that is not idle tested its own fixture.)
#[test]
fn unknown_kinds_are_unsupported() {
    let mut world = lone_unit_world(idle(UnitAiKind::Ranged));
    let (answer, ..) = lone_step(&mut world, Millis::new(1), WorldTime::ZERO);
    assert_eq!(
        answer,
        Err(GenieError::unsupported("unit-AI type not modelled"))
    );
}

#[test]
fn new_building_unit_ai_draws_its_three_timers() {
    // UnitAIModule's constructor: last retarget r % 500, adjusted idle timeout r % 1500 + 2250,
    // retarget timer 750 + r % 500, in that order
    let mut generator = rng();
    let ai = UnitAi::constructed(
        UnitAiKind::Building,
        &placed(Position::new(30.0, 88.0, 0.0), class::BUILDING),
        &mut generator,
        &roles(),
    );
    let mut expected = rng();
    let (retarget_draw, idle_draw, timer_draw) =
        (expected.rand15(), expected.rand15(), expected.rand15());
    assert_eq!(generator.draws, 3);
    assert_eq!(
        ai,
        UnitAi {
            ai_kind: UnitAiKind::Building,
            current_order: None,
            current_action: None,
            current_target: None,
            target_class: None,
            target_point: Position::new(-1.0, -1.0, -1.0),
            desired_distance: Distance::new(2.0),
            defend_target: None,
            last_order: None,
            last_action: None,
            last_target: None,
            last_target_class: None,
            idle_timer: Millis::ZERO,
            adjusted_idle_timeout: Millis::new(idle_draw % 1500 + 2250),
            idle_timeout: Millis::new(3000),
            secondary_timer: Millis::new(750),
            look_around_timer: Millis::ZERO,
            look_around_timeout: Millis::new(1000),
            last_retarget_time: WorldTime::from_millis(retarget_draw % 500),
            retarget_timer: Millis::new(750 + timer_draw % 500),
            stance: UnitStance::Aggressive,
            formation_kind: FormationKind::Horde,
            state_position: Point::new(30.0, 88.0),
            retarget_disabled: 0,
            alert_mode: AlertMode::Calm,
            alert_object: None,
            time_since_enemy: Seconds::ZERO,
            patrol_path: None,
            patrol_waypoint: WaypointIndex::of_data(0),
            stop_after_kill: false,
            home: None,
            hunted: None,
            order_priority: OrderPriority::of_data(-1),
            last_update_time: Millis::ZERO,
            orders: Vec::new(),
            building_orders: Vec::new(),
            notifies: Vec::new(),
            notify_capacity: 10,
            processing: false,
            buffered: Vec::new(),
            ordered_on_target: false,
            attackers: Attackers::default(),
            order_history: Vec::new(),
            retries: Vec::new(),
            best_this_update: None,
        }
    );
}

/// The artifact's unit AI (14, `TribeArtifactUnitAIModule` `FUN_140c11340`, the rock church's): the
/// base's three draws, then `FUN_140c41bc0(5000)`: its idle timeout 5000 and a fourth draw for its
/// adjusted one (`generator % 2500 + 3750`), stance 0, five notify slots. The recording 426048706
/// holds rock churches of base 5000 with adjusted ones of 3881 and 6012.
#[test]
fn new_artifact_unit_ai_draws_four_timers() {
    let mut generator = rng();
    let ai = UnitAi::constructed(
        UnitAiKind::Artifact,
        &placed(Position::new(136.0, 100.0, 0.0), class::ARTIFACT),
        &mut generator,
        &roles(),
    );
    let mut expected = rng();
    let (retarget_draw, _, timer_draw, fourth_draw) = (
        expected.rand15(),
        expected.rand15(),
        expected.rand15(),
        expected.rand15(),
    );
    assert_eq!(generator.draws, 4);
    assert_eq!(
        (
            ai.ai_kind,
            ai.last_retarget_time,
            ai.retarget_timer,
            ai.idle_timeout,
            ai.adjusted_idle_timeout,
            ai.stance.get(),
            ai.notify_capacity,
        ),
        (
            UnitAiKind::Artifact,
            WorldTime::from_millis(retarget_draw % 500),
            Millis::new(750 + timer_draw % 500),
            Millis::new(5000),
            Millis::new(fourth_draw % 2500 + 3750),
            0,
            5
        )
    );
}

/// Player 1's scout cavalry 30 (master 448, class 12, line of sight 4) at (20.5, 92.5) on a
/// map of 120 by 120 tiles of land, its unit AI `ai`, its owner of `kind`; the player has
/// explored every tile but `unexplored`.
fn scouting(ai: UnitAi, kind: PlayerKind, unexplored: &[Tile]) -> World {
    let scout = Master {
        id: master_id(448),
        kind: MasterKind::COMBAT,
        class: class::CLASS_12,
        radius: Footprint::new(0.2, 0.2),
        height: Distance::new(1.0),
        hit_points: MasterHitPoints::of_data(45),
        line_of_sight: Distance::new(4.0),
        terrain_restriction: Some(RestrictionId::named(7)),
        move_stats: Some(walking(Speed::new(1.2))),
        ..Master::default()
    };
    Scene::open(GridSize::new(120, 120))
        .sight(TileSight::Explored)
        .sight_on(unexplored.iter().copied(), TileSight::Unknown)
        .kind(player_id(1), kind)
        .object(SceneObject {
            ai: Some(ai),
            ..SceneObject::new(30, player_id(1), scout, Position::new(20.5, 92.5, 0.0))
        })
        .build()
}

/// The point the Explore of scout 30 of `world` walks to, none without one.
fn exploring(world: &World) -> Option<Position> {
    world
        .parts
        .actions
        .of(object_id(30))
        .list
        .explore()
        .map(|explore_action| explore_action.point)
}

/// The base handler's case 0x202 for the auto scout ([`UnitAi::seek_unexplored`]): a point
/// its search finds (the unit AI's own, slot 0x90, without a town centre: an unexplored tile
/// within its line of sight and 3 more) it explores (the explore's action value the last, its
/// own back) and processing breaks; finding none, on the auto scout order the last search's
/// point (the flood through the explored tiles) is explored the same way, and with none left
/// the scout map is made; on another order the target goes and the unit stops. Another
/// explorer's search, a scout order's that finds none and a computer player's AI told of it
/// are not modelled.
#[test]
fn an_auto_scout_explores_what_its_searches_find_or_maps_what_it_saw() {
    let scout = |order, action| UnitAi {
        current_order: order,
        current_action: Some(action),
        current_target: Some(object_id(30)),
        ..idle(UnitAiKind::Hunted)
    };
    let auto = Some(OrderKind::AutoScout);
    let seek = |ai: UnitAi, kind: PlayerKind, unexplored: &[Tile]| {
        let mut scouting = scouting(ai, kind, unexplored);
        let notify = Notify::own(object_id(30), NotifyKind::REQUIRES_SEARCH, [605, 0, 0]);
        let run_ai = run_ai(
            &mut scouting,
            (player_id(1), object_id(30)),
            |ai, at, side, kind, rng| ai.seek_unexplored(notify, at, kind, rng, side),
        );
        (run_ai, scouting)
    };
    let actions = |world: &World| {
        let ai = world.parts.unit_ai.get(object_id(30)).unwrap();
        (ai.current_action, ai.last_action)
    };
    // the unit AI's search finds (22, 94) within the scout's square
    let (result, world) = seek(
        scout(auto, AiAction::AutoScout),
        PlayerKind::Human,
        &[Tile::new(22, 94), Tile::new(40, 92)],
    );
    assert_eq!(result, Ok(BREAK));
    assert_eq!(exploring(&world), Some(Position::new(22.0, 94.0, 0.0)));
    assert_eq!(
        actions(&world),
        (Some(AiAction::AutoScout), Some(AiAction::Explore))
    );
    // none within it: the last search's flood finds (40, 92); the last action the explore set
    // (the auto scout's) stays
    let (result, world) = seek(
        scout(auto, AiAction::AutoScout),
        PlayerKind::Human,
        &[Tile::new(40, 92)],
    );
    assert_eq!(result, Ok(BREAK));
    assert_eq!(exploring(&world), Some(Position::new(40.0, 92.0, 0.0)));
    assert_eq!(
        actions(&world),
        (Some(AiAction::AutoScout), Some(AiAction::AutoScout))
    );
    // every tile explored: the scout map from what the player sees
    let (result, world) = seek(scout(auto, AiAction::AutoScout), PlayerKind::Human, &[]);
    assert_eq!(result, Ok(BREAK));
    assert_eq!(exploring(&world), None);
    assert!(world.spatial.scout_map(player_id(1)).is_some());
    assert_eq!(
        seek(scout(auto, AiAction::AutoScout), PlayerKind::Computer, &[]).0,
        Err(GenieError::unsupported(
            "a computer player's AI told its scout found nothing (0x202)"
        ))
    );
    // on no order the target goes and the unit stops
    let (result, world) = seek(scout(None, AiAction::AutoScout), PlayerKind::Human, &[]);
    assert_eq!(result, Ok(GO_ON));
    let ai = world.parts.unit_ai.get(object_id(30)).unwrap();
    assert_eq!((ai.current_target, ai.current_action), (None, None));
    assert_eq!(
        seek(
            scout(Some(OrderKind::Scout), AiAction::AutoScout),
            PlayerKind::Human,
            &[]
        )
        .0,
        Err(GenieError::unsupported(
            "a scout that finds no undiscovered tile (the player's slot 0x2e8 with 0x211)"
        ))
    );
    assert_eq!(
        seek(scout(None, AiAction::Explore), PlayerKind::Human, &[]).0,
        Err(GenieError::unsupported(
            "an explorer's search for the closest undiscovered tile (slot 0x90)"
        ))
    );
    // a computer player's explorer finds no tile (slot 0x90 gives none for it): the target goes,
    // the action stops and its owner hears the notify (its AI module, which the world does not
    // hold here, would)
    let (result, world) = seek(scout(None, AiAction::Explore), PlayerKind::Computer, &[]);
    assert_eq!(result, Ok(GO_ON));
    let ai = world.parts.unit_ai.get(object_id(30)).unwrap();
    assert_eq!((ai.current_target, ai.current_action), (None, None));
}

/// MarvinBlue/426048706's rock church 7108's unit AI (type 0xe, the artifact's), idle.
fn church_ai() -> UnitAi {
    UnitAi {
        ai_kind: UnitAiKind::Artifact,
        current_order: None,
        current_action: None,
        current_target: None,
        idle_timeout: Millis::new(5000),
        adjusted_idle_timeout: Millis::new(3881),
        retarget_timer: Millis::new(1224),
        ..UnitAi::default()
    }
}

/// The rock church 7108 of `owner` (master 285, an artifact of radius 2, line of sight 5 and
/// search radius 5, its speed `speed`: none stands) at (136, 100) on a map of 200 by 200
/// tiles, under its Artifact (its default task), its unit AI `ai`; the `others` (id, owner,
/// x, y), walkers of class 6, each in Gaia's and player 1's lists; rocks on `rocks`; player 1
/// has only explored the tiles `fogged` and sees every other.
fn church_world(
    (owner, ai): (PlayerId, UnitAi),
    speed: Option<Speed>,
    others: &[(i32, PlayerId, f32, f32)],
    (rocks, fogged): (&[Tile], &[Tile]),
) -> World {
    let church = Master {
        id: master_id(285),
        kind: MasterKind::BUILDING,
        class: class::ARTIFACT,
        radius: Footprint::new(2.0, 2.0),
        height: Distance::new(2.0),
        hit_points: MasterHitPoints::of_data(1000),
        line_of_sight: Distance::new(5.0),
        terrain_restriction: Some(RestrictionId::named(7)),
        move_stats: speed.map(walking),
        action: Some(ActionPart {
            search_radius: Distance::new(5.0),
            ..ActionPart::default()
        }),
        ..Master::default()
    };
    let artifact = Action {
        list: ActionList::default().with_artifact(Some(Artifact::new(owner, None))),
        ..Action::default()
    };
    let mut scene = Scene::open(GridSize::new(200, 200))
        .players(3)
        .rocks(rocks.iter().copied())
        .sight_on(fogged.iter().copied(), TileSight::Explored)
        .object(SceneObject {
            ai: Some(ai),
            actions: Some(artifact),
            ..SceneObject::new(7108, owner, church, Position::new(136.0, 100.0, 0.0))
        });
    for &(id, player, x, y) in others {
        scene = scene.object(SceneObject {
            seen_by: Some(VisionBits::visible(PlayerId::GAIA) | VisionBits::visible(player_id(1))),
            ..SceneObject::new(
                id,
                player,
                walker(5, class::CLASS_6),
                Position::new(x, y, 0.0),
            )
        });
    }
    scene.build()
}

/// The owner of the rock church in `world`, and whether it still holds its Artifact.
fn church(world: &World) -> (PlayerId, bool) {
    let (owner, _) = world.object(object_id(7108)).expect("the church");
    let busy = world
        .parts
        .actions
        .of(object_id(7108))
        .list
        .artifact()
        .is_some();
    (owner, busy)
}

/// The artifact's processIdle (`FUN_140c1e410`): `ClosestObject` over its cared classes from
/// its place within its line of sight and a tile, without the reach test for a unit that
/// does not move; the object found takes it over for its owner (slot 0x70, 6: the list's loop
/// then files the church in its new owner's list) while the lists hold one player's objects at
/// most; two players, none found or its own owner's leave it (5). A moving one asks for the
/// reach test: a unit walled in by rocks is found by the church that stands only.
#[test]
fn the_artifact_goes_to_the_owner_of_the_closest_object_one_player_sees() {
    let gaia = PlayerId::GAIA;
    let idle_of = |speed, others: &[(i32, PlayerId, f32, f32)], rocks: &[Tile]| {
        let mut world = church_world((gaia, church_ai()), speed, others, (rocks, &[]));
        let took = run_ai(
            &mut world,
            (gaia, object_id(7108)),
            |ai, at, side, kind, rng| ai.process_idle(at, kind, rng, side),
        );
        (took, church(&world).0)
    };
    let two = player_id(2);
    assert_eq!(
        idle_of(None, &[(500, two, 140.0, 100.0)], &[]),
        (Ok(IdleOutcome::Acted), two)
    );
    // two players' objects, none, or Gaia's own
    for others in [
        &[(500, two, 140.0, 100.0), (501, player_id(3), 140.0, 101.0)][..],
        &[],
        &[(500, gaia, 140.0, 100.0)],
    ] {
        assert_eq!(idle_of(None, others, &[]), (Ok(IdleOutcome::Idle), gaia));
    }
    // player 2's unit walled in at (141, 100): reached only by a church that does not move
    let walled = [
        Tile::new(140, 99),
        Tile::new(141, 99),
        Tile::new(142, 99),
        Tile::new(140, 100),
        Tile::new(142, 100),
        Tile::new(140, 101),
        Tile::new(141, 101),
        Tile::new(142, 101),
    ];
    let inside = [(500, two, 141.5, 100.5)];
    assert_eq!(
        idle_of(None, &inside, &walled),
        (Ok(IdleOutcome::Acted), two)
    );
    assert_eq!(
        idle_of(Some(Speed::new(0.8)), &inside, &walled),
        (Ok(IdleOutcome::Idle), gaia)
    );
}

/// The artifact's slot 73 (`FUN_140c21d40`) in its full update: its processIdle every
/// update; a takeover forgets its target, stops its action (slot 0xb0 with 1) after the
/// ownership change, and its idle timer is 0.
#[test]
fn the_artifact_update_hands_it_over_then_stops() {
    let update = |others: &[(i32, PlayerId, f32, f32)]| {
        let ai = UnitAi {
            idle_timer: Millis::new(900),
            ..church_ai()
        };
        let mut world = church_world((PlayerId::GAIA, ai), None, others, (&[], &[]));
        let after_order = run_ai(
            &mut world,
            (PlayerId::GAIA, object_id(7108)),
            |ai, at, side, kind, rng| {
                ai.update(
                    Millis::new(100),
                    WorldTime::from_millis(5000),
                    kind,
                    at,
                    rng,
                    side,
                )
            },
        );
        assert_eq!(after_order, Ok(None));
        let ai = world.parts.unit_ai.get(object_id(7108)).unwrap();
        (church(&world), ai.current_target, ai.idle_timer)
    };
    assert_eq!(
        update(&[(500, player_id(2), 140.0, 100.0)]),
        ((player_id(2), false), None, Millis::ZERO)
    );
    // nobody near: nothing, the idle timer runs
    assert_eq!(
        update(&[]),
        ((PlayerId::GAIA, true), None, Millis::new(1000))
    );
}

/// The artifact's notify handler (`FUN_140c21f70`), here on player 1's church: the kinds it
/// lets be go on; a unit attacking it joins its attackers and puts it under attack; its own
/// action's completion or failure (with an order, the order's action) and any kind it does
/// not know end in its stop (its Artifact goes); a moved target it still sees goes on, one it
/// does not see ends.
#[test]
fn the_artifact_notifies_stop_it_or_go_on() {
    let one = player_id(1);
    let run = |ai: &UnitAi, kind: NotifyKind, params: [i32; 3], sight: TileSight| {
        let notify = Notify {
            caller: Some(object_id(500)),
            recipient: object_id(7108),
            kind,
            params,
        };
        let others = [
            (500, player_id(2), 140.0, 100.0),
            (9, player_id(2), 140.5, 104.5),
        ];
        let fogged: &[Tile] = match sight {
            TileSight::Explored => &[Tile::new(140, 104)],
            _ => &[],
        };
        let mut world = church_world((one, ai.clone()), None, &others, (&[], fogged));
        let run_ai = run_ai(
            &mut world,
            (one, object_id(7108)),
            |ai, at, side, kind, rng| ai.handle(notify, Millis::ZERO, at, kind, rng, side),
        );
        let unit = world.players[1].objects.by_id(object_id(7108)).unwrap();
        let under_attack = unit.flags.contains(UnitFlags::UNDER_ATTACK);
        let stopped = !church(&world).1;
        let after = world.parts.unit_ai.get(object_id(7108)).unwrap().clone();
        ((run_ai, stopped, under_attack), after)
    };
    let seen = TileSight::Seen;
    let mut ai = church_ai();
    for kind in [
        NotifyKind::DAMAGED,
        NotifyKind::from(0x1f7),
        NotifyKind::from(0x1ff),
        NotifyKind::REQUIRES_SEARCH,
        NotifyKind::OWNER_CHANGED,
        NotifyKind::from(0x25d),
        NotifyKind::from(0x268),
        NotifyKind::NO_LONGER_ON_ME,
        NotifyKind::from(0x2be),
    ] {
        assert_eq!(run(&ai, kind, [0; 3], seen).0, (Ok(GO_ON), false, false));
    }
    let (result, after) = run(&ai, NotifyKind::ATTACKED, [0; 3], seen);
    assert_eq!(result, (Ok(GO_ON), false, true));
    assert_eq!(after.attackers, Attackers::of_saved(&[500]));
    ai = after;
    ai.current_action = Some(AiAction::Move);
    ai.current_target = Some(object_id(9));
    let (result, after) = run(&ai, NotifyKind::ACTION_COMPLETED, [0x261, 0, 0], seen);
    assert_eq!(result, (Ok(GO_ON), false, false));
    ai = after;
    let (result, after) = run(&ai, NotifyKind::ACTION_FAILED, [0x262, 0, 0], seen);
    assert_eq!(result, (Ok(GO_ON), true, false));
    assert_eq!((after.current_action, after.current_target), (None, None));
    ai = after;
    // with an order: its action's (action + 100 = order)
    ai.current_order = Some(OrderKind::Move);
    ai.current_action = Some(AiAction::Move);
    let (result, after) = run(&ai, NotifyKind::ACTION_COMPLETED, [0x1, 0, 0], seen);
    assert_eq!(result, (Ok(GO_ON), true, false));
    ai = after;
    assert_eq!(
        run(&ai, NotifyKind::from(0x210), [0; 3], seen).0,
        (Ok(GO_ON), true, false)
    );
    // a moved target it sees goes on; one it does not see stops it
    ai.current_target = Some(object_id(9));
    assert_eq!(
        run(&ai, NotifyKind::TARGET_MOVED, [0; 3], seen).0,
        (Ok(GO_ON), false, false)
    );
    assert_eq!(
        run(&ai, NotifyKind::TARGET_MOVED, [0; 3], TileSight::Explored).0,
        (Ok(GO_ON), true, false)
    );
}

/// The tower's unit AI (15, `TribeTowerUnitAIModule` `FUN_140c12960`, a town centre's with its
/// volley missile): the base's three draws, then `FUN_140c41bc0(1000)`: its idle timeout 1000 and a
/// fourth draw for its adjusted idle timeout (`generator % 500 + 750`). records/428759682 update
/// 2019: the town centre 8768 a 621's completion makes holds base 1000, adjusted 818.
#[test]
fn new_tower_unit_ai_draws_four_timers() {
    let mut generator = rng();
    let ai = UnitAi::constructed(
        UnitAiKind::Tower,
        &placed(Position::new(54.0, 147.0, 0.0), class::TOWER),
        &mut generator,
        &roles(),
    );
    let mut expected = rng();
    let (retarget_draw, _, timer_draw, fourth_draw) = (
        expected.rand15(),
        expected.rand15(),
        expected.rand15(),
        expected.rand15(),
    );
    assert_eq!(generator.draws, 4);
    assert_eq!(
        (
            ai.ai_kind,
            ai.last_retarget_time,
            ai.retarget_timer,
            ai.idle_timeout,
            ai.adjusted_idle_timeout,
            ai.stance.get(),
        ),
        (
            UnitAiKind::Tower,
            WorldTime::from_millis(retarget_draw % 500),
            Millis::new(750 + timer_draw % 500),
            Millis::new(1000),
            Millis::new(fourth_draw % 500 + 750),
            0
        )
    );
}

/// A new object of class `class` at `position`, of master 83 (its own copy), a human
/// player's in the Dark Age with posture 0.
fn placed(position: Position, class: ClassId) -> NewObject {
    NewObject {
        position,
        master: ids::VILLAGER_MALE,
        copy_id: Some(ids::VILLAGER_MALE),
        class,
        ..NewObject::default()
    }
}

/// The base's three draws (`0x140c2e080`) and, for the types whose constructor sets its own
/// idle timeout (`FUN_140c41bc0`), slot 0x1a8's fourth (`0x140c41e00`: `r % (t / 2) + 3 t /
/// 4`); the hunted animal's slot 0x1a8 (`FUN_140c17170`) is 5/4 of its timeout without a
/// draw. Each type's stance (`+0x1a9`) and notify capacity (5 for 0xe and 0x13).
#[test]
fn each_unit_ai_constructor_draws_its_timers_and_sets_its_stance() {
    // (type, class, idle timeout, fourth draw, stance, notify capacity)
    let kinds: [(UnitAiKind, ClassId, u32, bool, u8, usize); 17] = [
        (UnitAiKind::Hunted, class::PREY, 4000, false, 3, 10),
        (UnitAiKind::Predator, class::PREDATOR, 3000, false, 0, 10),
        (UnitAiKind::Boar, class::PREDATOR, 3000, false, 0, 10),
        (UnitAiKind::Lion, class::PREDATOR, 6000, true, 0, 10),
        (UnitAiKind::Ranged, class::CLASS_0, 3000, false, 3, 10),
        (UnitAiKind::Ranged2, class::CLASS_0, 3000, false, 3, 10),
        (UnitAiKind::Civilian, class::VILLAGER, 3000, false, 0, 10),
        (UnitAiKind::Priest, class::MONK, 500, true, 1, 10),
        (UnitAiKind::Trade, class::TRADE_BOAT, 3000, false, 0, 10),
        (UnitAiKind::Transport, class::TRANSPORT, 3000, false, 0, 10),
        (UnitAiKind::Fishing, class::FISHING_SHIP, 3000, false, 0, 10),
        (UnitAiKind::Soldier, class::CLASS_6, 500, true, 0, 10),
        (UnitAiKind::Artifact, class::ARTIFACT, 5000, true, 0, 5),
        (UnitAiKind::Tower, class::TOWER, 1000, true, 0, 10),
        (UnitAiKind::Siege, class::SIEGE, 1000, true, 1, 10),
        (UnitAiKind::Building, class::BUILDING, 3000, false, 0, 10),
        (UnitAiKind::SiegeTower, class::CLASS_6, 3000, false, 0, 10),
    ];
    for (ai_kind, class, timeout, fourth, stance, capacity) in kinds {
        let mut generator = rng();
        let ai = UnitAi::constructed(
            ai_kind,
            &placed(Position::new(12.5, 40.0, 1.0), class),
            &mut generator,
            &roles(),
        );
        let mut expected = rng();
        let (retarget_draw, idle_draw, timer_draw) =
            (expected.rand15(), expected.rand15(), expected.rand15());
        let adjusted = if fourth {
            expected.rand15() % (timeout / 2) + timeout * 3 / 4
        } else if ai_kind == UnitAiKind::Hunted {
            timeout * 5 / 4
        } else {
            idle_draw % 1500 + 2250
        };
        assert_eq!(
            generator.draws,
            if fourth { 4 } else { 3 },
            "type {ai_kind}"
        );
        assert_eq!(
            (
                ai.ai_kind,
                ai.last_retarget_time,
                ai.retarget_timer,
                ai.idle_timeout,
                ai.adjusted_idle_timeout,
                ai.stance.get(),
                ai.notify_capacity,
                ai.state_position,
            ),
            (
                ai_kind,
                WorldTime::from_millis(retarget_draw % 500),
                Millis::new(750 + timer_draw % 500),
                Millis::new(timeout),
                Millis::new(adjusted),
                stance,
                capacity,
                Point::new(12.5, 40.0)
            ),
            "type {ai_kind}"
        );
    }
    let mut rng = rng();
    let livestock = UnitAi::constructed(
        UnitAiKind::Livestock,
        &placed(Position::from([1.0; 3]), class::LIVESTOCK),
        &mut rng,
        &roles(),
    );
    assert_eq!(
        (rng.draws, livestock.stance.get(), livestock.notify_capacity),
        (3, 1, 5)
    );
}

/// The hunted animal's constructor (`FUN_140c11920`): its origin is the object's place, its
/// leash the object's slot 0x7b0 (`+0xb74`); the king (class 59) takes stance 0, a deer
/// keeps the base's 3. The boar's (`FUN_140c11720`) origin is its place too, neither
/// furious nor sent home.
#[test]
fn new_hunted_animal_and_boar_keep_their_place_as_their_origin() {
    let mut deer = placed(Position::new(20.0, 30.0, 2.0), class::PREY);
    deer.hunt_range = Distance::new(7.5);
    let mut generator = rng();
    let ai = UnitAi::constructed(UnitAiKind::Hunted, &deer, &mut generator, &roles());
    assert_eq!(
        (ai.hunted, ai.stance.get(), ai.home),
        (
            Some(Hunted {
                origin: Position::new(20.0, 30.0, 2.0),
                leash: Distance::new(7.5),
            }),
            3,
            None
        )
    );
    let king = UnitAi::constructed(
        UnitAiKind::Hunted,
        &placed(Position::ORIGIN, class::KING),
        &mut rng(),
        &roles(),
    );
    assert_eq!(king.stance, UnitStance::Aggressive);
    let boar = UnitAi::constructed(
        UnitAiKind::Boar,
        &placed(Position::new(5.0, 6.0, 0.5), class::PREDATOR),
        &mut rng(),
        &roles(),
    );
    assert_eq!(
        (boar.home, boar.hunted, boar.stance.get()),
        (
            Some(Home {
                origin: Position::new(5.0, 6.0, 0.5),
                furious: false,
                homeward: false,
            }),
            None,
            0
        )
    );
}

/// The soldier's constructor (`FUN_140c12300`): a computer player's idle timeout is 1500
/// (the fourth draw `generator % 750 + 1125`); its stance by the rules of [`soldier_stance`].
#[test]
fn new_soldier_takes_its_stance_from_its_master_and_owner() {
    let mut computer = placed(Position::ORIGIN, class::CLASS_6);
    computer.player_kind = PlayerKind::Computer;
    computer.posture = Posture::Defensive;
    let mut generator = rng();
    let ai = UnitAi::constructed(UnitAiKind::Soldier, &computer, &mut generator, &roles());
    let mut expected = rng();
    let _ = (expected.rand15(), expected.rand15(), expected.rand15());
    assert_eq!(
        (ai.idle_timeout, ai.adjusted_idle_timeout, ai.stance.get()),
        (
            Millis::new(1500),
            Millis::new(expected.rand15() % 750 + 1125),
            1
        )
    );
    let stance = |setup: &dyn Fn(&mut NewObject)| {
        let mut new = placed(Position::ORIGIN, class::CLASS_6);
        setup(&mut new);
        soldier_stance(&new, &roles())
    };
    // the owner's posture, its low byte
    assert_eq!(stance(&|_| {}), UnitStance::Aggressive);
    assert_eq!(
        stance(&|new_object| new_object.posture = Posture::of_data(0x101)),
        UnitStance::Defensive
    );
    // the scout of the Dark Age stands its ground: flag 0x10, its own copy, age 0 (or -0)
    assert_eq!(
        stance(&|new_object| new_object.object_flags = ObjectFlags::SCOUT),
        UnitStance::StandGround
    );
    assert_eq!(
        stance(&|new_object| {
            new_object.object_flags = ObjectFlags::SCOUT;
            new_object.age = Amount::new(-0.0);
        }),
        UnitStance::StandGround
    );
    assert_eq!(
        stance(&|new_object| {
            new_object.object_flags = ObjectFlags::SCOUT;
            new_object.age = Amount::new(1.0);
        }),
        UnitStance::Aggressive
    );
    assert_eq!(
        stance(&|new_object| {
            new_object.object_flags = ObjectFlags::SCOUT;
            new_object.age = Amount::new(f32::NAN);
        }),
        UnitStance::Aggressive
    );
    assert_eq!(
        stance(&|new_object| {
            new_object.object_flags = ObjectFlags::SCOUT;
            new_object.copy_id = Some(ids::ID_448);
        }),
        UnitStance::Aggressive
    );
    assert_eq!(
        stance(&|new_object| {
            new_object.object_flags = ObjectFlags::SCOUT;
            new_object.player_kind = PlayerKind::Computer;
        }),
        UnitStance::Aggressive
    );
    // the demolition ships defend
    for master in [ids::ID_527, ids::ID_528, ids::ID_1104] {
        assert_eq!(
            stance(&|new_object| {
                new_object.master = master;
                new_object.copy_id = Some(master);
            }),
            UnitStance::Defensive
        );
    }
    assert_eq!(
        stance(&|new_object| {
            new_object.master = ids::ID_527;
            new_object.player_kind = PlayerKind::Computer;
        }),
        UnitStance::Aggressive
    );
    // a human player's petards and class 64 do not attack; a computer's take its posture
    assert_eq!(
        stance(&|new_object| new_object.class = class::CLASS_35),
        UnitStance::NoAttack
    );
    assert_eq!(
        stance(&|new_object| new_object.class = class::CLASS_64),
        UnitStance::NoAttack
    );
    assert_eq!(
        stance(&|new_object| {
            new_object.class = class::CLASS_35;
            new_object.player_kind = PlayerKind::Computer;
        }),
        UnitStance::Aggressive
    );
    // a hero defends: bit 0 or 3, not 3 with 7
    assert_eq!(
        stance(&|new_object| new_object.hero_flag = HeroFlags::FULL_HERO),
        UnitStance::Defensive
    );
    assert_eq!(
        stance(&|new_object| new_object.hero_flag = HeroFlags::DEFENSIVE),
        UnitStance::Defensive
    );
    assert_eq!(
        stance(&|new_object| new_object.hero_flag = HeroFlags::FULL_HERO | HeroFlags::INVERTED),
        UnitStance::Defensive
    );
    assert_eq!(
        stance(&|new_object| new_object.hero_flag = HeroFlags::DEFENSIVE | HeroFlags::INVERTED),
        UnitStance::Aggressive
    );
    assert_eq!(
        stance(&|new_object| new_object.hero_flag = HeroFlags::PROTECTED_FORMATION),
        UnitStance::Aggressive
    );
    // class 61 does not attack, unless a hero
    assert_eq!(
        stance(&|new_object| new_object.class = class::CLASS_61),
        UnitStance::NoAttack
    );
    assert_eq!(
        stance(&|new_object| {
            new_object.class = class::CLASS_61;
            new_object.hero_flag = HeroFlags::FULL_HERO;
        }),
        UnitStance::Defensive
    );
    assert_eq!(
        stance(&|new_object| {
            new_object.class = class::CLASS_61;
            new_object.player_kind = PlayerKind::Computer;
        }),
        UnitStance::NoAttack
    );
}

/// The siege unit's stance (`FUN_140c12040`): a computer player's and the trebuchets' (51,
/// 54) 0, the scorpion's (55) the owner's posture, any other's 1.
#[test]
fn new_siege_unit_takes_its_stance_from_its_class_and_owner() {
    let stance = |class: ClassId, player_kind: PlayerKind, posture: i32| {
        siege_stance(
            &NewObject {
                class,
                player_kind,
                posture: Posture::of_data(posture),
                ..NewObject::default()
            },
            &roles(),
        )
    };
    assert_eq!(
        stance(class::SIEGE, PlayerKind::Human, 0),
        UnitStance::Defensive
    );
    assert_eq!(
        stance(class::SIEGE, PlayerKind::Computer, 0),
        UnitStance::Aggressive
    );
    assert_eq!(
        stance(class::CLASS_51, PlayerKind::Human, 1),
        UnitStance::Aggressive
    );
    assert_eq!(
        stance(class::CLASS_54, PlayerKind::Human, 1),
        UnitStance::Aggressive
    );
    assert_eq!(
        stance(class::CLASS_55, PlayerKind::Human, 1),
        UnitStance::Defensive
    );
    assert_eq!(
        stance(class::CLASS_55, PlayerKind::Human, 0),
        UnitStance::Aggressive
    );
    assert_eq!(
        stance(class::CLASS_55, PlayerKind::Computer, 1),
        UnitStance::Aggressive
    );
    assert_eq!(
        stance(class::CLASS_6, PlayerKind::Human, 0),
        UnitStance::Defensive
    );
}

/// [`NewObject::of`]: the master's copy, class, flags and hero flag, and the work range of
/// its last task of action type 6 (0 without one).
#[test]
fn new_object_takes_its_hunt_range_from_its_master_last_task_of_kind_6() {
    let task = |action_kind, work_range| Task {
        action_kind,
        work_range,
        ..Task::default()
    };
    let made = Master {
        copy_id: Some(master_id(1240)),
        class: class::PREY,
        object_flags: ObjectFlags::SCOUT | ObjectFlags::GATHERED_BUILDING,
        creatable: Some(CreatablePart {
            hero_flag: HeroFlags::REGENERATES,
            ..CreatablePart::default()
        }),
        action: Some(ActionPart {
            tasks: vec![
                task(ActionKind::named(6), Distance::new(2.0)),
                task(ActionKind::COMBAT, Distance::new(9.0)),
                task(ActionKind::named(6), Distance::new(3.5)),
                task(GATHER, Distance::new(1.0)),
            ],
            ..ActionPart::default()
        }),
        ..Master::default()
    };
    let new = NewObject::of(Position::new(1.0, 2.0, 3.0), master_id(1239), &made);
    assert_eq!(
        new,
        NewObject {
            position: Position::new(1.0, 2.0, 3.0),
            master: master_id(1239),
            copy_id: Some(master_id(1240)),
            class: class::PREY,
            object_flags: ObjectFlags::SCOUT | ObjectFlags::GATHERED_BUILDING,
            hero_flag: HeroFlags::REGENERATES,
            hunt_range: Distance::new(3.5),
            // the owner's fields are a human player's until the caller's
            player_kind: PlayerKind::Human,
            ..NewObject::default()
        }
    );
    let none = NewObject::of(Position::ORIGIN, master_id(1), &Master::default());
    assert_eq!(none.hunt_range, Distance::ZERO);
}

#[test]
fn an_order_redraws_an_adjusted_idle_timeout_of_750_or_more_and_caps_the_secondary_timer() {
    // FUN_140e6cfc0 for each unit of an INTERACT, MOVE or RETREAT
    let mut unit_ai = UnitAi {
        idle_timeout: Millis::new(3000),
        adjusted_idle_timeout: Millis::new(2600),
        secondary_timer: Millis::new(900),
        patrol_path: Some(PatrolPath::default()),
        patrol_waypoint: WaypointIndex::of_data(3),
        ..idle(UnitAiKind::Soldier)
    };
    let mut generator = rng();
    unit_ai.reset_for_order(&mut generator);
    assert_eq!(
        (unit_ai.adjusted_idle_timeout, unit_ai.secondary_timer),
        (Millis::new(7241 % 1500 + 2250), Millis::new(750))
    );
    // FUN_140c68de0 first: the patrol path and its waypoint go
    assert_eq!(
        (unit_ai.patrol_path, unit_ai.patrol_waypoint.get()),
        (None, 0)
    );
    let mut low = UnitAi {
        idle_timeout: Millis::new(3000),
        adjusted_idle_timeout: Millis::new(749),
        ..idle(UnitAiKind::Soldier)
    };
    let mut rng = rng();
    low.reset_for_order(&mut rng);
    assert_eq!(
        (low.adjusted_idle_timeout, rng.draws),
        (Millis::new(749), 0)
    );
}

/// MarvinBlue/426048731's boar 9732 (type 4) at its origin (23.5, 105.5), as saved after 680.
fn boar() -> UnitAi {
    UnitAi {
        idle_timer: Millis::new(3000),
        adjusted_idle_timeout: Millis::new(3005),
        idle_timeout: Millis::new(3000),
        look_around_timeout: Millis::new(5426),
        retarget_timer: Millis::new(1205),
        target_class: None,
        target_point: Position::new(-1.0, -1.0, -1.0),
        last_order: None,
        last_action: None,
        last_target: None,
        last_target_class: None,
        stance: UnitStance::Aggressive,
        home: Some(Home {
            origin: Position::new(23.5, 105.5, 0.0),
            ..Home::default()
        }),
        ..idle(UnitAiKind::Boar)
    }
}

/// A seed whose idle check (a scaled draw, then rand15) wanders, or does not.
#[allow(clippy::maybe_infinite_iter)] // the search ends at the first seed that fits
fn seed_for(wanders: bool) -> Well512 {
    (0..)
        .map(|seed| {
            let mut rng = Well512::default();
            rng.set_seed(seed);
            rng
        })
        .find(|candidate| {
            let mut rng = candidate.clone();
            let _ = rng.below(81);
            (rng.rand15() % 7 == 0) == wanders
        })
        .unwrap()
}

#[test]
fn boar_at_home_draws_its_leash_and_mostly_stays() {
    // updates 766, 910 and 1039: processIdle draws `0..81` and a rand15, which is not a
    // multiple of 7; nothing moves
    let home = Position::new(23.5, 105.5, 0.0);
    let mut world = animal_world(animal(boar(), home), &[], seed_for(false));
    let (answer, unit_ai, draws) = animal_step(&mut world, Millis::new(10));
    assert_eq!((answer, lone_walks(&world)), (Ok(None), None));
    assert_eq!(
        (unit_ai.idle_timer, unit_ai.current_action, draws),
        (Millis::ZERO, None, 2)
    );
}

#[test]
fn boar_wanders_up_to_three_tiles_once_in_seven() {
    let stream = seed_for(true);
    let mut expected = stream.clone();
    let _ = expected.below(81);
    let _ = expected.rand15();
    let (dx, dy) = (expected.rand15() % 6, expected.rand15() % 6);
    let home = Position::new(23.5, 105.5, 0.0);
    let mut world = animal_world(animal(boar(), home), &[], stream);
    let (_, unit_ai, draws) = animal_step(&mut world, Millis::new(10));
    let point = Position::new(dx as f32 - 3.0 + 23.5, dy as f32 - 3.0 + 105.5, 0.0);
    assert_eq!(lone_walks(&world), Some((point, Distance::new(0.5))));
    // slot 0x130: the move action, no target, the point; the last action is the old one
    assert_eq!(
        (
            unit_ai.current_action,
            unit_ai.last_action,
            unit_ai.current_target,
            unit_ai.target_point
        ),
        (Some(AiAction::Move), None, None, point)
    );
    assert_eq!(draws, 4);
}

#[test]
fn boar_away_from_home_goes_back_and_furious_one_stays() {
    // more than 2 tiles from its origin and once in 7: home, at its own height
    let mut world = animal_world(
        animal(boar(), Position::new(26.0, 105.5, 0.5)),
        &[],
        seed_for(true),
    );
    let (_, _, draws) = animal_step(&mut world, Millis::new(10));
    assert_eq!(
        (lone_walks(&world).map(|(point, _)| point), draws),
        (Some(Position::new(23.5, 105.5, 0.5)), 2)
    );
    // beyond the leash (more than 144 + 80, squared) it heads home unless it already does
    let far = Position::new(43.5, 105.5, 0.0);
    let mut world = animal_world(animal(boar(), far), &[], seed_for(false));
    let (_, _, draws) = animal_step(&mut world, Millis::new(10));
    assert_eq!(
        (lone_walks(&world).map(|(point, _)| point), draws),
        (Some(Position::new(23.5, 105.5, 0.0)), 1)
    );
    let heading = UnitAi {
        target_point: Position::new(23.5, 0.0, 0.0),
        ..boar()
    };
    let mut world = animal_world(animal(heading, far), &[], seed_for(false));
    let (answer, unit_ai, _) = animal_step(&mut world, Millis::new(10));
    assert_eq!(
        (answer, unit_ai.current_action, lone_walks(&world)),
        (Ok(None), None, None)
    );
    // furious, or in mode 1, it does not draw at all; without a target it calms down after
    for (furious, mode_1) in [(true, false), (false, true)] {
        let mut angry = boar();
        angry.home.as_mut().unwrap().furious = furious;
        let mut world = animal_world(animal(angry, Position::new(23.5, 105.5, 0.0)), &[], rng());
        if mode_1 {
            world.mode_680 = Mode680::ONE;
        }
        let (answer, unit_ai, draws) = animal_step(&mut world, Millis::new(10));
        assert_eq!(
            (answer, draws, unit_ai.home.unwrap().furious),
            (Ok(None), 0, false)
        );
    }
}

#[test]
fn deer_adjusted_idle_timeout_is_five_quarters_of_its_idle_timeout() {
    // FUN_140c17170, the deer's and fox's slot 0x1a8: no draw
    let mut unit_ai = UnitAi {
        idle_timeout: Millis::new(4000),
        adjusted_idle_timeout: Millis::new(3000),
        ..idle(UnitAiKind::Hunted)
    };
    let mut rng = rng();
    unit_ai.reset_for_order(&mut rng);
    assert_eq!(
        (unit_ai.adjusted_idle_timeout, rng.draws),
        (Millis::new(5000), 0)
    );
}

#[test]
fn move_forgets_the_target_and_an_order_keeps_its_history() {
    // MOVE of 9185 in 1044: order 710, action 610, no target, the point on the ground
    let mut unit_ai = UnitAi {
        current_target: Some(object_id(9365)),
        target_class: class_of(15),
        ..idle(UnitAiKind::Soldier)
    };
    unit_ai.notify_move(true, None, Point::new(21.947_916, 92.489_586), &mut rng());
    assert_eq!(
        (
            unit_ai.current_order,
            unit_ai.current_action,
            unit_ai.current_target,
            unit_ai.target_class
        ),
        (Some(OrderKind::Move), Some(AiAction::Move), None, None)
    );
    assert_eq!(
        (
            unit_ai.last_order,
            unit_ai.last_action,
            unit_ai.last_target,
            unit_ai.last_target_class
        ),
        (None, None, None, None)
    );
    assert_eq!(
        unit_ai.target_point,
        Position::new(21.947_916, 92.489_586, 0.0)
    );
}

#[test]
fn an_idle_transport_ship_retargets_like_soldier() {
    // type 11, TribeTransportShipUnitAIModule: base slots 4 and 73, slot 72 0x140c21d20 draws
    // nothing
    let mut world = lone_unit_world(idle(UnitAiKind::Transport));
    let (answer, unit_ai, _) = lone_step(&mut world, Millis::new(50), WorldTime::from_millis(900));
    assert_eq!(answer, Ok(None));
    assert_eq!(
        (unit_ai.last_retarget_time, unit_ai.retarget_timer),
        (WorldTime::from_millis(900), Millis::new(750 + 7241 % 500))
    );
}

fn deer() -> UnitAi {
    UnitAi {
        idle_timer: Millis::new(5000),
        adjusted_idle_timeout: Millis::new(5000),
        idle_timeout: Millis::new(4000),
        look_around_timeout: Millis::new(5424),
        retarget_timer: Millis::new(1212),
        target_class: None,
        target_point: Position::new(-1.0, -1.0, -1.0),
        last_order: None,
        last_action: None,
        last_target: None,
        last_target_class: None,
        hunted: Some(Hunted {
            origin: Position::new(31.5, 106.5, 0.0),
            leash: Distance::new(0.0),
        }),
        ..idle(UnitAiKind::Hunted)
    }
}

/// A stream whose next rand15 is a multiple of 7 or not.
#[allow(clippy::maybe_infinite_iter)] // the search ends at the first seed that fits
fn wander_seed(wanders: bool) -> Well512 {
    (0..)
        .map(|seed| {
            let mut rng = Well512::default();
            rng.set_seed(seed);
            rng
        })
        .find(|rng| (rng.clone().rand15() % 7 == 0) == wanders)
        .unwrap()
}

/// A deer that sees nothing stays six times in seven (one draw), else wanders up to three
/// tiles off (two more draws). Its search reaches its line of sight plus 1 (3 tiles), its search
/// radius (1) being shorter: player 1's scout 3.5 tiles off is not seen.
#[test]
fn deer_that_sees_nothing_stays_six_times_in_seven_and_otherwise_wanders() {
    let home = Position::new(31.5, 106.5, 0.0);
    let far_scout = [deer_scout(Position::new(35.0, 106.5, 0.0))];
    let mut world = animal_world(animal(deer(), home), &far_scout, wander_seed(false));
    let (answer, _, draws) = animal_step(&mut world, Millis::new(10));
    assert_eq!((answer, lone_walks(&world), draws), (Ok(None), None, 1));

    let stream = wander_seed(true);
    let mut expected = stream.clone();
    let _ = expected.rand15();
    let step = |roll: u32| {
        if 3 <= roll {
            roll as i32 - 6
        } else {
            roll as i32 + 1
        }
    };
    let (dx, dy) = (step(expected.rand15() % 6), step(expected.rand15() % 6));
    let mut world = animal_world(animal(deer(), home), &[], stream);
    let (_, _, draws) = animal_step(&mut world, Millis::new(10));
    assert_eq!(
        lone_walks(&world).map(|(point, _)| point),
        Some(Position::new(31.5 + dx as f32, 106.5 + dy as f32, 0.0))
    );
    assert!(dx != 0 && dy != 0 && dx.abs() <= 3 && dy.abs() <= 3);
    assert_eq!(draws, 3);
}

/// Player 1's scout 9183 at `at` (a walker of class 12, master 448), which a deer evades.
fn deer_scout(at: Position) -> SceneObject {
    SceneObject::new(9183, player_id(1), walker(448, class::CLASS_12), at)
}

#[test]
fn deer_away_from_home_goes_back_and_one_beyond_its_leash_at_once() {
    let away = Position::new(34.5, 106.5, 0.5);
    let mut world = animal_world(animal(deer(), away), &[], wander_seed(true));
    let (_, _, draws) = animal_step(&mut world, Millis::new(10));
    assert_eq!(
        (lone_walks(&world).map(|(point, _)| point), draws),
        (Some(Position::new(31.5, 106.5, 0.5)), 1)
    );
    // a fox's leash of 2.5: 3 tiles off (2.8 from its footprint) it goes home without a
    // search or a draw
    let mut fox = deer();
    fox.hunted.as_mut().unwrap().leash = Distance::new(2.5);
    let mut world = animal_world(animal(fox, away), &[], wander_seed(false));
    let (_, _, draws) = animal_step(&mut world, Millis::new(10));
    assert_eq!(
        (lone_walks(&world).map(|(point, _)| point), draws),
        (Some(Position::new(31.5, 106.5, 0.5)), 0)
    );
}

/// A deer that sees an object runs from it by three times its speed (slot 0x150), forced
/// and running, and holds the move action and the point; an enemy among what it saw sets its
/// alert mode and starts its sighting time again (deer 9655 at update 2626).
#[test]
fn deer_that_sees_an_object_evades_it() {
    let startled = UnitAi {
        time_since_enemy: Seconds::new(12.0),
        ..deer()
    };
    let home = Position::new(31.5, 106.5, 0.0);
    let scout = [deer_scout(Position::new(33.5, 106.5, 0.0))];
    let mut world = animal_world(animal(startled, home), &scout, wander_seed(false));
    let (_, unit_ai, _) = animal_step(&mut world, Millis::new(10));
    let point = Position::new(31.5 - 1.2 * 3.0, 106.5, 0.0);
    assert_eq!(lone_walks(&world), Some((point, Distance::ZERO)));
    assert_eq!(
        (unit_ai.current_action, unit_ai.target_point),
        (Some(AiAction::Move), point)
    );
    assert_eq!(
        (unit_ai.alert_mode, unit_ai.time_since_enemy),
        (AlertMode::Alert, Seconds::ZERO)
    );
}

/// An animal whose wander ended (its unit AI on the move action, no `MoveTo` left) drops the
/// action and turns idle; its idle timer runs on. While it walks (its `MoveTo` of its own: busy),
/// its timers run and nothing else happens.
#[test]
fn an_animal_whose_wander_ended_drops_its_move_action_and_turns_idle() {
    let home = Position::new(31.5, 106.5, 0.0);
    let ended = UnitAi {
        current_action: Some(AiAction::Move),
        idle_timer: Millis::new(10),
        ..deer()
    };
    let mut world = animal_world(animal(ended, home), &[], wander_seed(false));
    let (_, unit_ai, _) = animal_step(&mut world, Millis::new(10));
    assert_eq!(
        (
            lone_walks(&world),
            unit_ai.current_action,
            unit_ai.idle_timer
        ),
        (None, None, Millis::new(20))
    );
    let wandering = UnitAi {
        current_action: Some(AiAction::Move),
        ..deer()
    };
    let walking = SceneObject {
        actions: Some(Action {
            list: ActionList::default().with_move_to(Some(MoveToAction {
                point: Position::new(33.5, 106.5, 0.0),
                ..MoveToAction::sub_in(MoveTo::Moving)
            })),
            ..Action::default()
        }),
        ..animal(wandering, home)
    };
    let mut world = animal_world(walking, &[], wander_seed(false));
    let (_, unit_ai, _) = animal_step(&mut world, Millis::new(10));
    assert_eq!(
        (
            unit_ai.current_action,
            unit_ai.idle_timer,
            unit_ai.state_position
        ),
        (Some(AiAction::Move), Millis::new(5010), Point::ORIGIN)
    );
}

/// `MarvinBlue/Some(Issuer::Player(player_id(4)))26048731`'s scout 9183 (soldier AI, type 13) as saved after 821: on its MOVE
/// order with five queued moves, its `MoveTo` about to arrive.
fn scout() -> UnitAi {
    let order = |x: f32, y: f32| QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::Move,
        priority: OrderPriority::of_data(100),
        target: None,
        target_owner: None,
        point: Position::new(x, y, 0.0),
        range: Distance::ZERO,
        shown: Point::new(x, y),
        ready: true,
        target_kind: None,
    };
    UnitAi {
        current_order: Some(OrderKind::Move),
        order_priority: OrderPriority::of_data(-1),
        current_action: Some(AiAction::Move),
        target_class: None,
        target_point: Position::new(93.583_336, 57.791_668, 0.0),
        last_order: Some(OrderKind::Move),
        last_action: Some(AiAction::Move),
        last_target: None,
        last_target_class: None,
        stance: UnitStance::StandGround,
        orders: vec![order(89.09375, 60.677_082), order(91.822_914, 52.489_582)],
        ..idle(UnitAiKind::Soldier)
    }
}

#[test]
fn between_turns_an_arrived_move_runs_the_next_queued_order() {
    // update 822: the MoveTo arrived (state 1, which slot 0x2d0 lets go), so FUN_140c68980
    // runs the queue: the first move becomes the AI's (order 710 at priority 100, action 610,
    // its point) and the unit's, forced and walking; the order leaves the queue
    let mut world = lone_world(lone_scout(scout(), MoveTo::Arrived), &[]);
    let (answer, unit_ai) = lone_off_turn(&mut world);
    assert_eq!(answer, Ok(()));
    assert_eq!(
        lone_walks(&world),
        Some((Position::new(89.09375, 60.677_082, 0.0), Distance::ZERO))
    );
    assert_eq!(
        (
            unit_ai.current_order,
            unit_ai.order_priority.get(),
            unit_ai.current_action
        ),
        (Some(OrderKind::Move), 100, Some(AiAction::Move))
    );
    assert_eq!(
        unit_ai.target_point,
        Position::new(89.09375, 60.677_082, 0.0)
    );
    assert_eq!(unit_ai.orders.len(), 1);
    assert_eq!(
        unit_ai.orders[0].point,
        Position::new(91.822_914, 52.489_582, 0.0)
    );
}

/// The scout 9183 (its unit AI `unit_ai`) as player 1's lone unit 50 at (93.5, 58.25), a
/// walker of class 12 (master 448), its `MoveTo` to its place in `state`.
fn lone_scout(unit_ai: UnitAi, state: MoveTo) -> SceneObject {
    let at = Position::new(93.5, 58.25, 0.0);
    let lone = lone_unit(player_id(1), unit_ai, at);
    SceneObject {
        master: Master {
            id: master_id(448),
            class: class::CLASS_12,
            ..lone.master
        },
        actions: Some(Action {
            list: ActionList::default().with_move_to(Some(MoveToAction {
                point: at,
                ..MoveToAction::sub_in(state)
            })),
            ..Action::default()
        }),
        ..lone
    }
}

/// Player 1's lone unit 50 of `world` runs its check between turns ([`UnitAi::off_turn`]): its
/// answer and its unit AI.
fn lone_off_turn(world: &mut World) -> (GenieResult<()>, UnitAi) {
    let answer = run_ai(world, (player_id(1), object_id(50)), UnitAi::off_turn);
    (
        answer,
        world.parts.unit_ai.get(object_id(50)).unwrap().clone(),
    )
}

/// Between turns a walking unit (its `MoveTo` moving: slot 0x2d0 holds it) or one with notifies
/// keeps its queue and its walk; an order not ready stays and the next one runs.
#[test]
fn between_turns_walking_unit_or_one_with_notifies_keeps_its_queue() {
    let at = Position::new(93.5, 58.25, 0.0);
    let mut world = lone_world(lone_scout(scout(), MoveTo::Moving), &[]);
    let (answer, unit_ai) = lone_off_turn(&mut world);
    assert_eq!(
        (answer, unit_ai.orders.len(), lone_walks(&world)),
        (Ok(()), 2, Some((at, Distance::ZERO)))
    );
    let told = UnitAi {
        notifies: vec![Notify::own(
            object_id(50),
            NotifyKind::ACTION_COMPLETED,
            [AiAction::Move.value(), 0, 0],
        )],
        ..scout()
    };
    let mut world = lone_world(lone_scout(told, MoveTo::Arrived), &[]);
    let (answer, unit_ai) = lone_off_turn(&mut world);
    assert_eq!(
        (answer, unit_ai.orders.len(), lone_walks(&world)),
        (Ok(()), 2, Some((at, Distance::ZERO)))
    );
    // an order not ready stays and the next one runs
    let mut waiting = scout();
    waiting.orders[0].ready = false;
    let mut world = lone_world(lone_scout(waiting, MoveTo::Arrived), &[]);
    let (_, unit_ai) = lone_off_turn(&mut world);
    assert_eq!(unit_ai.orders.len(), 1);
    assert_eq!(
        unit_ai.orders[0].point,
        Position::new(89.09375, 60.677_082, 0.0)
    );
}

#[test]
fn completed_move_on_its_order_ends_the_order_and_stops_the_unit() {
    // the base handler's 0x1fa: 610 + 100 = 710, so slot 0xb0 ends the order too; the
    // notifies go with the owner's time; the queue then runs (the unit is no longer busy)
    let arrived = UnitAi {
        orders: vec![],
        notifies: vec![Notify::own(
            object_id(50),
            NotifyKind::ACTION_COMPLETED,
            [AiAction::Move.value(), 0, 0],
        )],
        ..scout()
    };
    let scout_unit = SceneObject {
        actions: None,
        ..lone_scout(arrived, MoveTo::Arrived)
    };
    let mut world = lone_world(scout_unit, &[]);
    let (answer, unit_ai, _) = lone_step(&mut world, Millis::new(60), WorldTime::from_millis(900));
    assert_eq!((answer, lone_walks(&world)), (Ok(None), None));
    assert_eq!(
        (
            unit_ai.current_order,
            unit_ai.order_priority.get(),
            unit_ai.current_action
        ),
        (None, -1, None)
    );
    assert_eq!(
        (unit_ai.last_order, unit_ai.last_action),
        (Some(OrderKind::Move), Some(AiAction::Move))
    );
    assert_eq!(unit_ai.state_position, Point::new(93.5, 58.25));
    assert_eq!(
        (unit_ai.notifies.len(), unit_ai.last_update_time),
        (0, Millis::new(60))
    );
}

/// A map of 200 by 200 with Gaia's unit `id` at `at` (a walker of class 11, master 4, on its
/// sprite), its unit AI `ai`. The AI stream is seeded with 0, as [`rng`] is.
fn gaia_unit_world(id: i32, ai: UnitAi, at: Position) -> World {
    gaia_unit_world_with(id, ai, at, &[])
}

/// [`gaia_unit_world`] with the `others`, each in Gaia's lists.
fn gaia_unit_world_with(id: i32, ai: UnitAi, at: Position, others: &[SceneObject]) -> World {
    let animal = Master {
        standing_sprite: Some(STANDING.id),
        ..walker(4, class::CLASS_11)
    };
    let mut scene = Scene::open(GridSize::new(200, 200))
        .graphic(STANDING)
        .object(SceneObject {
            ai: Some(ai),
            sprite: Some(STANDING.id),
            ..SceneObject::new(id, PlayerId::GAIA, animal, at)
        });
    for other in others {
        scene = scene.object(SceneObject {
            seen_by: Some(VisionBits::visible(PlayerId::GAIA)),
            ..other.clone()
        });
    }
    let mut world = scene.build();
    world.streams.get_mut(Stream::Ai).set_seed(0);
    world
}

/// Gaia's unit `id` in `world` hears `notify` through its action-failed handler
/// ([`UnitAi::action_failed`]): its answer and its unit AI.
fn gaia_failed(world: &mut World, id: i32, notify: Notify) -> (GenieResult<u32>, UnitAi) {
    let run_ai = run_ai(
        world,
        (PlayerId::GAIA, object_id(id)),
        |ai, at, side, kind, rng| ai.action_failed(notify, at, kind, rng, side),
    );
    (
        run_ai,
        world.parts.unit_ai.get(object_id(id)).unwrap().clone(),
    )
}

#[test]
fn failed_action_stops_like_completed_one_and_an_order_own_failure_ends_it() {
    // records/425906646's update 1234: a Gaia animal's own MoveTo failed (state 13, notify
    // 0x1f9); without an order the failure of the current action stops it
    let moving = UnitAi {
        current_order: None,
        current_action: Some(AiAction::Move),
        current_target: Some(object_id(31234)),
        ..UnitAi::default()
    };
    let notify = Notify::own(
        object_id(9183),
        NotifyKind::ACTION_FAILED,
        [AiAction::Move.value(), 0, 0],
    );
    let mut world = gaia_unit_world(9183, moving, Position::new(10.0, 10.0, 0.0));
    let (result, unit_ai) = gaia_failed(&mut world, 9183, notify);
    assert_eq!(result, Ok(GO_ON));
    assert_eq!(
        (unit_ai.current_action, unit_ai.last_action),
        (None, Some(AiAction::Move))
    );
    assert_eq!(
        (unit_ai.current_target, unit_ai.last_target),
        (None, Some(object_id(31234)))
    );
    // on its order (610 + 100) the failure ends the order too and sets the state position
    let ordered = UnitAi {
        current_order: Some(OrderKind::Move),
        current_action: Some(AiAction::Move),
        ..UnitAi::default()
    };
    let mut world = gaia_unit_world(9183, ordered, Position::new(10.0, 12.0, 0.0));
    let (_, unit_ai) = gaia_failed(&mut world, 9183, notify);
    assert_eq!(
        (unit_ai.current_order, unit_ai.current_action),
        (None, None)
    );
    assert_eq!(unit_ai.state_position, Point::new(10.0, 12.0));
    // another action's failure goes on; a failed attack on the attack order of a unit AI
    // whose processIdle is not modelled (the ranged unit's) is not either
    let building = UnitAi {
        current_order: None,
        current_action: Some(AiAction::Build),
        ..UnitAi::default()
    };
    let mut world = gaia_unit_world(9183, building, Position::new(10.0, 12.0, 0.0));
    let (_, unit_ai) = gaia_failed(&mut world, 9183, notify);
    assert_eq!(unit_ai.current_action, Some(AiAction::Build));
    let ranged = UnitAi {
        ai_kind: UnitAiKind::Ranged,
        current_order: Some(OrderKind::Attack),
        ..UnitAi::default()
    };
    let attack = Notify::own(object_id(9183), NotifyKind::ACTION_FAILED, [600, 0, 0]);
    let mut world = gaia_unit_world(9183, ranged, Position::new(10.0, 12.0, 0.0));
    assert!(gaia_failed(&mut world, 9183, attack).0.is_err());
}

/// records/425906646's wolf 29827 (Gaia, a boar's unit AI) in update 3891: its Attack's target
/// went out of sight, which sent it home (`+0x1fd`); the failure's processIdle (slot 0x240)
/// walks it there (its `MoveTo` at once) and breaks off. A boar that stays forgets its target
/// and stops.
#[test]
fn boar_failed_attack_runs_its_idle_check() {
    let home = |homeward, furious| Home {
        origin: Position::new(171.5, 51.5, 0.0),
        furious,
        homeward,
    };
    let boar = |home| UnitAi {
        ai_kind: UnitAiKind::Boar,
        current_order: None,
        current_action: Some(AiAction::Attack),
        current_target: Some(object_id(31110)),
        home: Some(home),
        ..UnitAi::default()
    };
    let attack = Notify::own(object_id(29827), NotifyKind::ACTION_FAILED, [600, 0, 0]);
    let walks = |world: &World| {
        world
            .parts
            .actions
            .of(object_id(29827))
            .list
            .move_to()
            .map(|move_to_action| move_to_action.point)
    };
    let place = Position::new(169.5, 50.6, 0.0);
    let mut world = gaia_unit_world(29827, boar(home(true, false)), place);
    let (result, unit_ai) = gaia_failed(&mut world, 29827, attack);
    assert_eq!(result, Ok(PRODUCED));
    assert_eq!(walks(&world), Some(Position::new(171.5, 51.5, 0.0)));
    assert_eq!(
        (unit_ai.current_action, unit_ai.last_action),
        (Some(AiAction::Move), Some(AiAction::Attack))
    );
    assert!(unit_ai.home.is_some_and(|home| !home.homeward));
    let mut world = gaia_unit_world(29827, boar(home(false, true)), place);
    let (result, unit_ai) = gaia_failed(&mut world, 29827, attack);
    assert_eq!(result, Ok(GO_ON));
    assert_eq!(
        (unit_ai.current_target, unit_ai.last_target),
        (None, Some(object_id(31110)))
    );
    assert_eq!(walks(&world), None);
}

#[test]
fn an_animal_completed_wander_becomes_its_last_action() {
    // the fox 9851 in 718: its MoveTo's completion through the hunted animal's handler to
    // the base: no order, its action completed (its stop finds no action left)
    let arrived = UnitAi {
        current_action: Some(AiAction::Move),
        idle_timer: Millis::ZERO,
        notifies: vec![Notify::own(
            object_id(50),
            NotifyKind::ACTION_COMPLETED,
            [AiAction::Move.value(), 0, 0],
        )],
        ..deer()
    };
    let home = Position::new(31.5, 106.5, 0.0);
    let mut world = animal_world(animal(arrived, home), &[], wander_seed(false));
    let (answer, unit_ai, _) = animal_step(&mut world, Millis::new(60));
    assert_eq!(answer, Ok(None));
    assert_eq!(
        world.parts.actions.of(object_id(50)).top_action_kind(),
        None
    );
    assert_eq!(
        (unit_ai.current_action, unit_ai.last_action),
        (None, Some(AiAction::Move))
    );
}

#[test]
fn movement_completion_reaches_only_moving_unit_ai() {
    let completed = Notify::own(
        object_id(9181),
        NotifyKind::ACTION_COMPLETED,
        [AiAction::Move.value(), 0, 0],
    );
    let mut builder = UnitAi {
        current_order: Some(OrderKind::Build),
        current_action: Some(AiAction::Build),
        ..idle(UnitAiKind::Civilian)
    };
    builder.post(completed, ObjectState::Alive).unwrap();
    assert_eq!(builder.notifies, Vec::<Notify>::new());
    let mut wanderer = UnitAi {
        current_action: Some(AiAction::Move),
        ..deer()
    };
    wanderer.post(completed, ObjectState::Alive).unwrap();
    assert_eq!(wanderer.notifies, vec![completed]);
    // a unit not in state 2 tells its AI nothing
    let mut dying = UnitAi {
        current_action: Some(AiAction::Move),
        ..deer()
    };
    dying.post(completed, ObjectState::Dying).unwrap();
    assert_eq!(dying.notifies, Vec::<Notify>::new());
}

#[test]
fn livestock_ownership_notify_drops_its_action_value_and_breaks_off() {
    // FUN_140c26ac0: the notify ends the pass; the update's FUN_140c5dc10 clears the queue
    let sheep = UnitAi {
        current_action: Some(AiAction::Move),
        notifies: vec![
            Notify::own(object_id(50), NotifyKind::OWNER_CHANGED, [2, 0, 0]),
            Notify::own(
                object_id(50),
                NotifyKind::ACTION_COMPLETED,
                [AiAction::Move.value(), 0, 0],
            ),
        ],
        ..idle(UnitAiKind::Livestock)
    };
    let mut world = lone_unit_world(sheep);
    let (answer, unit_ai, _) = lone_step(&mut world, Millis::new(60), WorldTime::ZERO);
    assert_eq!((answer, lone_walks(&world)), (Ok(None), None));
    assert_eq!(
        (unit_ai.current_action, unit_ai.last_action),
        (None, Some(AiAction::Move))
    );
    assert_eq!(
        (unit_ai.notifies.len(), unit_ai.last_update_time),
        (0, Millis::new(60))
    );
}

/// Slot 0x28: a unit past state 2 takes no order; one in front goes before the first queued
/// order, any other after the last; an order not ready is what `FUN_140c4dfa0` looks for.
#[test]
fn an_order_for_the_queue_goes_after_the_last_or_before_the_first() {
    let order = |target, ready| QueuedOrder {
        target,
        order: OrderKind::OnTarget,
        ready,
        ..QueuedOrder::default()
    };
    let mut ai = UnitAi::default();
    assert_eq!(
        ai.queue_order(
            ObjectState::Dying,
            order(Some(object_id(1)), true),
            Dispatch::Queued,
            QueueEnd::Back
        ),
        Ok(Answer::NotTaken)
    );
    assert_eq!(ai.orders, Vec::<QueuedOrder>::new());
    for (target, end) in [
        (1, QueueEnd::Front),
        (2, QueueEnd::Back),
        (3, QueueEnd::Front),
    ] {
        assert_eq!(
            ai.queue_order(
                ObjectState::Alive,
                order(ObjectId::of_field(target), true),
                Dispatch::Queued,
                end
            ),
            Ok(Answer::Taken)
        );
    }
    let targets: Vec<Option<ObjectId>> = ai
        .orders
        .iter()
        .map(|queued_order| queued_order.target)
        .collect();
    assert_eq!(
        targets,
        [Some(object_id(3)), Some(object_id(1)), Some(object_id(2))]
    );
    assert!(!ai.has_unready_order());
    let _ = ai
        .queue_order(
            ObjectState::Alive,
            order(Some(object_id(4)), false),
            Dispatch::Queued,
            QueueEnd::Back,
        )
        .unwrap();
    assert!(ai.has_unready_order());
    assert!(
        ai.queue_order(
            ObjectState::Alive,
            order(Some(object_id(5)), true),
            Dispatch::Now,
            QueueEnd::Back
        )
        .is_err()
    );
}

#[test]
fn saved_order_and_notify_read_as_the_game_wrote_them() {
    let mut order = [0u8; 52];
    for (index, word) in [1i32, 710, 100, -1, -1].iter().enumerate() {
        order[4 * index..4 * index + 4].copy_from_slice(&word.to_le_bytes());
    }
    for (index, float) in [91.822_914_f32, 52.489_582, 0.0, 0.0, 91.822_914, 52.489_582]
        .iter()
        .enumerate()
    {
        order[20 + 4 * index..24 + 4 * index].copy_from_slice(&float.to_le_bytes());
    }
    order[44] = 1;
    order[48..52].copy_from_slice(&(-1i32).to_le_bytes());
    // a player's order in a unit's queue (the issuer 1, a player of the nine)
    let queued_order = QueuedOrder::from_bytes(&order, object_id(9183), 9).unwrap();
    assert_eq!(
        (
            queued_order.issuer,
            queued_order.order,
            queued_order.priority.get(),
            queued_order.point,
            queued_order.range,
            queued_order.ready,
            queued_order.target_kind
        ),
        (
            Some(Issuer::Player(player_id(1))),
            OrderKind::Move,
            100,
            Position::new(91.822_914, 52.489_582, 0.0),
            Distance::ZERO,
            true,
            None
        )
    );
    // an order value no order holds (MOVE's action value)
    order[4..8].copy_from_slice(&AiAction::Move.value().to_le_bytes());
    assert_eq!(
        QueuedOrder::from_bytes(&order, object_id(9183), 9).map_err(|error| error.kind()),
        Err(crate::ErrorKind::InvalidInput)
    );
    let mut notify = [0u8; 24];
    for (index, word) in [9851i32, 9851, 506, 610, 0, 0].iter().enumerate() {
        notify[4 * index..4 * index + 4].copy_from_slice(&word.to_le_bytes());
    }
    assert_eq!(
        Notify::from_bytes(&notify).unwrap(),
        Notify::own(
            object_id(9851),
            NotifyKind::ACTION_COMPLETED,
            [AiAction::Move.value(), 0, 0]
        )
    );
}

/// MarvinBlue/426048731's builders 9175 and 9177 in update 1497: their Builds ended (no
/// actions left) on their build order (702/602) with the house 10028 as their target.
fn builder_done() -> UnitAi {
    UnitAi {
        current_order: Some(OrderKind::Build),
        current_action: Some(AiAction::Build),
        current_target: Some(object_id(10028)),
        target_class: class_of(3),
        target_point: Position::new(30.0, 88.0, 0.0),
        idle_timeout: Millis::new(3000),
        ..idle(UnitAiKind::Civilian)
    }
}

/// A town centre (type 15, class 3, a tower's unit AI) of player 1 at (30, 30) on its make
/// action, a moving object of speed 0 on its sprite, its missile `missile` (none for −1) and
/// its firepower `firepower`; nothing else on the map.
fn making_world(ai: UnitAi, missile: i16, firepower: f32) -> World {
    let centre = Master {
        id: ids::TOWN_CENTRE,
        kind: MasterKind::BUILDING,
        class: class::BUILDING,
        radius: Footprint::new(2.0, 2.0),
        height: Distance::new(3.0),
        hit_points: MasterHitPoints::of_data(2400),
        line_of_sight: Distance::new(8.0),
        terrain_restriction: Some(RestrictionId::named(7)),
        move_stats: Some(walking(Speed::ZERO)),
        standing_sprite: Some(STANDING.id),
        combat: Some(CombatPart {
            weapon_range: Distance::new(6.0),
            missile: MasterField::of_data(missile),
            ..CombatPart::default()
        }),
        ..Master::default()
    };
    let mut world = Scene::open(GridSize::new(64, 64))
        .players(2)
        .graphic(STANDING)
        .object(SceneObject {
            ai: Some(ai),
            sprite: Some(STANDING.id),
            ..SceneObject::new(500, player_id(1), centre, Position::new(30.0, 30.0, 0.0))
        })
        .build();
    world.parts.combat_mut(object_id(500)).unwrap().firepower = Firepower::new(firepower);
    world
}

/// The town centre on its make action: with a target, or within its idle timeout, the idle
/// part does nothing and keeps the timer; past it without a target the timer restarts and
/// processIdle runs (the tower's search, which finds nothing). A building without a missile
/// and a single volley forgets its target, then searches; with units inside it shoots more and
/// keeps it.
#[test]
fn making_building_runs_process_idle_only_without_target() {
    let tc = |target| UnitAi {
        ai_kind: UnitAiKind::Tower,
        current_order: Some(OrderKind::Attack),
        current_action: Some(AiAction::Make),
        current_target: target,
        idle_timer: Millis::new(4000),
        adjusted_idle_timeout: Millis::new(1080),
        ..UnitAi::default()
    };
    let run = |ai: UnitAi, missile, firepower| {
        let mut world = making_world(ai, missile, firepower);
        let run_ai = run_ai(
            &mut world,
            (player_id(1), object_id(500)),
            |ai, at, side, kind, rng| ai.idle_part(true, at.group, at, kind, rng, side),
        );
        (run_ai, ai_500(&world).clone())
    };
    let (result, unit_ai) = run(tc(Some(object_id(29826))), 363, 1.0);
    assert_eq!((result, unit_ai.idle_timer), (Ok(()), Millis::new(4000)));
    let waiting = UnitAi {
        idle_timer: Millis::new(900),
        ..tc(None)
    };
    let (result, unit_ai) = run(waiting, 363, 1.0);
    assert_eq!((result, unit_ai.idle_timer), (Ok(()), Millis::new(900)));
    // past it without a target the timer restarts and processIdle runs: the tower's search
    let (result, unit_ai) = run(tc(None), 363, 1.0);
    assert_eq!((result, unit_ai.idle_timer), (Ok(()), Millis::ZERO));
    // a building without a missile and a single volley forgets its target, then searches
    let (result, unit_ai) = run(tc(Some(object_id(29826))), -1, 1.0);
    assert_eq!(result, Ok(()));
    assert_eq!(
        (
            unit_ai.current_target,
            unit_ai.last_target,
            unit_ai.idle_timer
        ),
        (None, Some(object_id(29826)), Millis::ZERO)
    );
    // with units inside it shoots more and keeps it
    let (result, unit_ai) = run(tc(Some(object_id(29826))), -1, 3.0);
    assert_eq!(result, Ok(()));
    assert_eq!(
        (unit_ai.current_target, unit_ai.idle_timer),
        (Some(object_id(29826)), Millis::new(4000))
    );
}

/// Unit 9 of `player` at (20, 20), a walker of class `class` (master 4), busy with its
/// `MoveTo` following its target, its unit AI `ai`; the target `target` (id, owner, class) at
/// (24, 20), alive, a walker of master 5.
fn busy_on_target(
    (player, players): (PlayerId, u8),
    class: ClassId,
    ai: UnitAi,
    (target, owner, target_class): (i32, PlayerId, ClassId),
) -> World {
    let follow = Action {
        list: ActionList::default().with_move_to(Some(MoveToAction {
            target: Some(object_id(target)),
            point: Position::new(24.0, 20.0, 0.0),
            ..MoveToAction::sub_in(MoveTo::Moving)
        })),
        ..Action::default()
    };
    let unit = SceneObject {
        ai: Some(ai),
        actions: Some(follow),
        ..SceneObject::new(9, player, walker(4, class), Position::new(20.0, 20.0, 0.0))
    };
    Scene::open(GridSize::new(64, 64))
        .players(players)
        .object(unit)
        .object(SceneObject::new(
            target,
            owner,
            walker(5, target_class),
            Position::new(24.0, 20.0, 0.0),
        ))
        .build()
}

/// [`busy_on_target`]'s unit runs its full update ([`UnitAi::update`], 10 ms at `now`): its
/// answer and the world.
fn update_busy(
    mut world: World,
    player: PlayerId,
    now: WorldTime,
) -> (GenieResult<Option<AfterOrder>>, World) {
    let after_order = run_ai(
        &mut world,
        (player, object_id(9)),
        |ai, at, side, kind, rng| ai.update(Millis::new(10), now, kind, at, rng, side),
    );
    (after_order, world)
}

/// The point and target of the `MoveTo` of unit 9 of `world`.
fn following(world: &World) -> Option<(Position, Option<ObjectId>)> {
    world
        .parts
        .actions
        .of(object_id(9))
        .list
        .move_to()
        .map(|move_to_action| (move_to_action.point, move_to_action.target))
}

/// The full update with a target on the orders and action values the recordings met (it
/// reads the target generally, whatever the order): a town centre's make action on the
/// attack order (6091, p6's 27171), a wolf's own attack, a building a MOVE took (no action
/// value, its research's 620), a builder and a hunter sent off a footprint, a hunt without
/// an order after `HuntObject`, a villager waiting for its foundation (734/634 with the
/// foundation), a boar moved aside off its attacker (-1/703).
#[test]
fn unit_ai_with_target_runs_its_update_on_any_order() {
    for (ai_kind, order, action) in [
        (
            UnitAiKind::Tower,
            Some(OrderKind::Attack),
            Some(AiAction::Make),
        ),
        (UnitAiKind::Lion, None, Some(AiAction::Attack)),
        (UnitAiKind::Building, Some(OrderKind::Move), None),
        (
            UnitAiKind::Building,
            Some(OrderKind::Move),
            Some(AiAction::Research),
        ),
        (
            UnitAiKind::Civilian,
            Some(OrderKind::Build),
            Some(AiAction::MoveAside),
        ),
        (
            UnitAiKind::Civilian,
            Some(OrderKind::Hunt),
            Some(AiAction::MoveAside),
        ),
        (UnitAiKind::Civilian, None, Some(AiAction::Hunt)),
        (
            UnitAiKind::Civilian,
            Some(OrderKind::Foundation),
            Some(AiAction::WaitSolidify),
        ),
        (UnitAiKind::Boar, None, Some(AiAction::MoveAside)),
    ] {
        let unit_ai = UnitAi {
            ai_kind,
            current_order: order,
            current_action: action,
            current_target: Some(object_id(14230)),
            adjusted_idle_timeout: Millis::new(9000),
            look_around_timeout: Millis::new(9000),
            retarget_timer: Millis::new(90_000),
            home: (ai_kind == UnitAiKind::Boar).then(Home::default),
            ..UnitAi::default()
        };
        let busy_on_target = busy_on_target(
            (player_id(1), 2),
            class::CLASS_6,
            unit_ai,
            (14230, player_id(2), class::PREDATOR),
        );
        let (after_order, world) =
            update_busy(busy_on_target, player_id(1), WorldTime::from_millis(100));
        assert_eq!(after_order, Ok(None), "{ai_kind} {order:?} {action:?}");
        // no call on the unit: it follows its target still
        assert_eq!(
            following(&world),
            Some((Position::new(24.0, 20.0, 0.0), Some(object_id(14230)))),
            "{ai_kind} {order:?} {action:?}"
        );
        assert_eq!(
            world
                .parts
                .unit_ai
                .get(object_id(9))
                .unwrap()
                .current_target,
            Some(object_id(14230)),
            "{ai_kind} {order:?} {action:?}"
        );
    }
}

/// MarvinBlue/426048731's builder 9175 of player 1 at (29.5, 89.5) on its sprite, its unit AI
/// `ai` and its actions `actions`, and the `target` 10028 at (30, 88), each in player 1's lists.
/// Its last human order time is past the cap.
fn builder_world(ai: UnitAi, actions: Option<Action>, target: Option<SceneObject>) -> World {
    let villager = Master {
        standing_sprite: Some(STANDING.id),
        ..villager_master()
    };
    let mut scene = Scene::open(GridSize::new(120, 120))
        .players(2)
        .graphic(STANDING)
        .object(SceneObject {
            ai: Some(ai),
            actions,
            sprite: Some(STANDING.id),
            ..SceneObject::new(9175, player_id(1), villager, Position::new(29.5, 89.5, 0.0))
        });
    if let Some(scene_object) = target {
        scene = scene.object(SceneObject {
            seen_by: Some(VisionBits::visible(player_id(1))),
            ..scene_object
        });
    }
    let mut world = scene.build();
    world.players[1]
        .objects
        .by_id_mut(object_id(9175))
        .unwrap()
        .last_human_order = WorldTime::from_millis(9000);
    world
}

/// Player 1's house 10028 at (30, 88), standing.
fn standing_house() -> SceneObject {
    let (house, _) = house_and_tree();
    SceneObject::new(10028, player_id(1), house, Position::new(30.0, 88.0, 0.0))
}

/// Player 2's walker 10028 (class 6, master 5) at (30, 88), dying.
fn dying_walker() -> SceneObject {
    SceneObject {
        state: ObjectState::Dying,
        ..SceneObject::new(
            10028,
            player_id(2),
            walker(5, class::CLASS_6),
            Position::new(30.0, 88.0, 0.0),
        )
    }
}

/// Builder 9175 of `world` runs its target block ([`UnitAi::target_block`]): its answer and its
/// unit AI.
fn builder_target_block(world: &mut World) -> (GenieResult<()>, UnitAi) {
    let run_ai = run_ai(
        world,
        (player_id(1), object_id(9175)),
        |ai, at, side, kind, rng| ai.target_block(at, kind, rng, side),
    );
    (
        run_ai,
        world.parts.unit_ai.get(object_id(9175)).unwrap().clone(),
    )
}

/// The target block: the house 10028 standing in state 2 keeps it (1497). A target no longer
/// in the world goes (its class and point stay); an attack action's idle timer then takes its
/// timeout. Inside another, a unit drops its target before the lookup.
#[test]
fn the_target_block_keeps_standing_target_and_drops_gone_one() {
    let mut world = builder_world(builder_done(), None, Some(standing_house()));
    let (result, unit_ai) = builder_target_block(&mut world);
    assert_eq!(
        (result, unit_ai.current_target),
        (Ok(()), Some(object_id(10028)))
    );
    assert_eq!(
        world.parts.actions.of(object_id(9175)).top_action_kind(),
        None
    );
    let attacking = UnitAi {
        current_action: Some(AiAction::Attack),
        ..builder_done()
    };
    let mut world = builder_world(attacking, None, None);
    let (result, unit_ai) = builder_target_block(&mut world);
    assert_eq!(result, Ok(()));
    assert_eq!(
        (unit_ai.current_target, unit_ai.target_class),
        (None, class_of(3))
    );
    assert_eq!(unit_ai.idle_timer, Millis::new(3000));
    // inside another
    let mut world = builder_world(builder_done(), None, Some(standing_house()));
    world.players[1]
        .objects
        .by_id_mut(object_id(9175))
        .unwrap()
        .life
        .inside = Some(ContainerId::new(object_id(10028)));
    let (result, unit_ai) = builder_target_block(&mut world);
    assert_eq!((result, unit_ai.current_target), (Ok(()), None));
}

/// A dead target: 0x25b needs a live one, so the target goes and the action stops (its
/// `MoveTo` too). A hunt goes on at its carcass. An attack whose target died waits while its
/// Attack swings on the kill (state 2); past it the villager's processIdle (the cap, then 5)
/// sends it running to the dead target's point. (The double's Attack the slice does not hold
/// is a case no world reaches.)
#[test]
fn dead_target_stops_an_action_that_needs_live_one() {
    let walking = Action {
        list: ActionList::default().with_move_to(Some(MoveToAction {
            point: Position::new(40.0, 89.5, 0.0),
            ..MoveToAction::sub_in(MoveTo::Moving)
        })),
        ..Action::default()
    };
    let healing = UnitAi {
        current_order: None,
        current_action: Some(AiAction::Heal),
        ..builder_done()
    };
    let mut world = builder_world(healing, Some(walking.clone()), Some(dying_walker()));
    let (result, unit_ai) = builder_target_block(&mut world);
    assert_eq!(result, Ok(()));
    assert_eq!(
        world.parts.actions.of(object_id(9175)).top_action_kind(),
        None
    );
    assert_eq!(
        (unit_ai.current_target, unit_ai.last_target),
        (None, Some(object_id(10028)))
    );
    assert_eq!(
        (unit_ai.current_action, unit_ai.last_action),
        (None, Some(AiAction::Heal))
    );
    // a hunt goes on at its carcass
    let hunting = UnitAi {
        current_order: Some(OrderKind::Hunt),
        current_action: Some(AiAction::Hunt),
        ..builder_done()
    };
    let mut walking_world = builder_world(hunting, Some(walking), Some(dying_walker()));
    let (result, unit_ai) = builder_target_block(&mut walking_world);
    assert_eq!(
        (result, unit_ai.current_target),
        (Ok(()), Some(object_id(10028)))
    );
    assert_eq!(
        walking_world
            .parts
            .actions
            .of(object_id(9175))
            .top_action_kind(),
        Some(MOVE_TO)
    );
    // the attack: its Attack swinging on the kill waits; done, the processIdle
    let attack = || UnitAi {
        current_action: Some(AiAction::Attack),
        ..builder_done()
    };
    let attacking = |state| Action {
        list: ActionList::default().with_attack(Some(own_attack(10028, state, Distance::new(1.0)))),
        ..Action::default()
    };
    let mut world = builder_world(
        attack(),
        Some(attacking(AttackState::Killed)),
        Some(dying_walker()),
    );
    let (result, unit_ai) = builder_target_block(&mut world);
    assert_eq!(
        (result, unit_ai.current_target),
        (Ok(()), Some(object_id(10028)))
    );
    let mut world = builder_world(
        attack(),
        Some(attacking(AttackState::Done)),
        Some(dying_walker()),
    );
    let (result, unit_ai) = builder_target_block(&mut world);
    assert_eq!(result, Ok(()));
    let walk = world
        .parts
        .actions
        .of(object_id(9175))
        .list
        .move_to()
        .map(|move_to_action| (move_to_action.point, move_to_action.range));
    assert_eq!(walk, Some((Position::new(30.0, 88.0, 0.0), Distance::ZERO)));
    assert_eq!(
        (unit_ai.current_order, unit_ai.last_order),
        (None, Some(OrderKind::Build))
    );
    let villager = world.players[1].objects.by_id(object_id(9175)).unwrap();
    assert_eq!(villager.last_human_order, HUMAN_ORDER_CAP);
}

/// Player 1's scout 9183 following the deer 9655 (FOLLOW at 2604): order 712, action 612,
/// the target and its place when FOLLOW ran, the constructor's desired distance.
fn follower() -> UnitAi {
    UnitAi {
        ai_kind: UnitAiKind::Soldier,
        current_order: Some(OrderKind::Follow),
        current_action: Some(AiAction::Follow),
        current_target: Some(object_id(9655)),
        target_class: class_of(9),
        target_point: Position::new(115.6998, 39.299_213, 0.0),
        desired_distance: Distance::new(2.0),
        stance: UnitStance::StandGround,
        ..UnitAi::default()
    }
}

/// [`follower`] as player 1's lone scout 50 at (110, 40) (a walker of class 12) on its way to
/// Gaia's deer 9655 (class 9, master 65) at `deer`, whose tile player 1 sees with `visible`.
fn follow_world(deer: Position, visible: bool) -> World {
    let scout = lone_unit(player_id(1), follower(), Position::new(110.0, 40.0, 0.0));
    let master = Master {
        class: class::CLASS_12,
        ..scout.master.clone()
    };
    let scout = on_its_way(SceneObject { master, ..scout }, deer);
    let prey = SceneObject::new(9655, PlayerId::GAIA, walker(65, class::PREY), deer);
    let mut scene = lone_scene(scout, &[prey]);
    if !visible {
        let tile = Tile::new(deer.x() as i32, deer.y() as i32);
        scene = scene.sight_on([tile], TileSight::Explored);
    }
    finish(scene)
}

/// Player 1's lone unit 50 of `world` runs its follow order's check ([`UnitAi::follow`]): its
/// answer and its unit AI.
fn lone_follow(world: &mut World) -> (GenieResult<MiscEnd>, UnitAi) {
    let answer = run_ai(
        world,
        (player_id(1), object_id(50)),
        |ai, at, side, _, rng| ai.follow(at, side, rng),
    );
    (
        answer,
        world.parts.unit_ai.get(object_id(50)).unwrap().clone(),
    )
}

/// The target and the range of the lone unit 50's `MoveTo` in `world`, none without one.
fn lone_walks_to(world: &World) -> Option<(Option<ObjectId>, Distance)> {
    world
        .parts
        .actions
        .of(object_id(50))
        .list
        .move_to()
        .map(|move_to| (move_to.target, move_to.range))
}

/// 2652: the deer is more than a tile off the point in y; beyond 2 the AI moves to it (a
/// `MoveTo` to the deer within 2). Less than a tile off on both axes it stays: its `MoveTo` to
/// the point stays.
#[test]
fn the_follow_order_moves_to_target_that_left_its_point() {
    let deer = Position::new(114.75516, 40.34795, 0.0);
    let mut world = follow_world(deer, true);
    let (answer, unit_ai) = lone_follow(&mut world);
    assert_eq!(answer, Ok(MiscEnd::Ends));
    assert_eq!(
        lone_walks_to(&world),
        Some((Some(object_id(9655)), Distance::new(2.0)))
    );
    assert_eq!(
        (unit_ai.current_action, unit_ai.last_action),
        (Some(AiAction::Move), Some(AiAction::Follow))
    );
    assert_eq!(
        (
            unit_ai.current_target,
            unit_ai.last_target,
            unit_ai.target_class
        ),
        (Some(object_id(9655)), Some(object_id(9655)), class_of(9))
    );
    assert_eq!(unit_ai.target_point, deer);
    let mut world = follow_world(Position::new(115.0, 40.2, 0.0), true);
    let (answer, unit_ai) = lone_follow(&mut world);
    assert_eq!(answer, Ok(MiscEnd::GoesOn));
    assert_eq!(lone_walks_to(&world).map(|(target, _)| target), Some(None));
    assert_eq!(
        unit_ai.target_point,
        Position::new(115.6998, 39.299_213, 0.0)
    );
}

/// The deer out of player 1's sight: the follow order ends, the scout stops.
#[test]
fn the_follow_order_ends_on_target_out_of_sight() {
    let mut world = follow_world(Position::new(114.75516, 40.34795, 0.0), false);
    let (answer, unit_ai) = lone_follow(&mut world);
    assert_eq!(answer, Ok(MiscEnd::Ends));
    assert!(lone_stopped(&world));
    assert_eq!(
        (
            unit_ai.current_order,
            unit_ai.current_action,
            unit_ai.current_target
        ),
        (None, None, None)
    );
    assert_eq!(
        (unit_ai.last_target, unit_ai.last_action),
        (Some(object_id(9655)), Some(AiAction::Follow))
    );
}

/// The base handler's case 0x1fc (a `MoveTo`'s moved target) on Gaia's scout 9183 following
/// the deer 9655: nothing without a saved patrol; with one processIdle runs (slot 0x240 with
/// 1; this Gaia soldier only caps its last human order time).
#[test]
fn moved_target_notify_runs_process_idle_with_saved_patrol() {
    let unit_ai = follower();
    let deer = SceneObject::new(
        9655,
        PlayerId::GAIA,
        walker(65, class_id(9)),
        Position::new(115.0, 40.2, 0.0),
    );
    let notify = Notify::own(object_id(9183), NotifyKind::TARGET_MOVED, [0, 0, 0]);
    let hears = |hearer: UnitAi| {
        let mut world = gaia_unit_world_with(
            9183,
            hearer,
            Position::new(110.0, 40.0, 0.0),
            from_ref(&deer),
        );
        world.players[0]
            .objects
            .by_id_mut(object_id(9183))
            .unwrap()
            .last_human_order = WorldTime::from_millis(9000);
        let answer = run_ai(
            &mut world,
            (PlayerId::GAIA, object_id(9183)),
            |ai, at, side, kind, rng| ai.base_notify(notify, Millis::ZERO, at, kind, rng, side),
        );
        let scout = world.players[0].objects.by_id(object_id(9183)).unwrap();
        (answer, scout.last_human_order)
    };
    assert_eq!(
        hears(unit_ai.clone()),
        (Ok(GO_ON), WorldTime::from_millis(9000))
    );
    // off its follow order the idle check caps it
    let patrolling = UnitAi {
        patrol_path: Some(PatrolPath::default()),
        current_order: None,
        ..unit_ai
    };
    assert_eq!(hears(patrolling), (Ok(GO_ON), HUMAN_ORDER_CAP));
}

/// The scout 9185 in update 1518: idle (its MOVE done), stance 2 at its state position.
fn idle_scout() -> UnitAi {
    UnitAi {
        idle_timer: Millis::new(504),
        adjusted_idle_timeout: Millis::new(503),
        stance: UnitStance::StandGround,
        state_position: Point::new(21.947_916, 92.489_586),
        ..idle(UnitAiKind::Soldier)
    }
}

/// [`idle_scout`]'s place (its state position).
const SCOUT_AT: Position = Position::new(21.947_916, 92.489_586, 0.0);

/// The scout 9185 (its unit AI `unit_ai`) as `owner`'s lone unit 50 at [`SCOUT_AT`]: a walker
/// of class 12 (master 448) of line of sight and search radius 4 and weapon range 1.
fn idle_scout_unit(owner: PlayerId, unit_ai: UnitAi) -> SceneObject {
    let scout = lone_unit(owner, unit_ai, SCOUT_AT);
    let master = Master {
        id: master_id(448),
        class: class::CLASS_12,
        line_of_sight: Distance::new(4.0),
        action: Some(ActionPart {
            search_radius: Distance::new(4.0),
            ..ActionPart::default()
        }),
        combat: Some(CombatPart {
            weapon_range: Distance::new(1.0),
            ..CombatPart::default()
        }),
        ..scout.master
    };
    SceneObject { master, ..scout }
}

/// [`lone_world`] with the `scout` and the `others`, the scout's last human order past the cap.
fn idle_scout_world(scout: SceneObject, others: &[SceneObject]) -> World {
    let owner = scout.player;
    let mut world = lone_world(scout, others);
    lone_past_cap(&mut world, owner);
    world
}

/// `owner`'s lone unit 50 of `world` gets its last human order time past the cap (9000), so
/// that the cap shows ([`UnitAi::cap_human_order`]).
fn lone_past_cap(world: &mut World, owner: PlayerId) {
    world.players[owner.index()]
        .objects
        .by_id_mut(object_id(50))
        .unwrap()
        .last_human_order = WorldTime::from_millis(9000);
}

/// `owner`'s idle walker `id` (class 6, master 5) `dx` tiles east of [`SCOUT_AT`], which a
/// scout's search may find.
fn scout_foe(id: i32, owner: PlayerId, dx: f32) -> SceneObject {
    SceneObject {
        ai: Some(idle(UnitAiKind::Soldier)),
        ..walker_unit(id, owner, Point::new(SCOUT_AT.x() + dx, SCOUT_AT.y()))
    }
}

/// The last human order time of `owner`'s lone unit 50 in `world`.
fn lone_human_order(world: &World, owner: PlayerId) -> WorldTime {
    world.players[owner.index()]
        .objects
        .by_id(object_id(50))
        .unwrap()
        .last_human_order
}

/// The scout's idle check caps its last human order time and searches for something to
/// attack: within its line of sight plus 1 (5 tiles), Gaia's objects too (not a scenario).
/// Standing its ground (stance 2), it attacks only within its weapon range (1): player 1's or
/// Gaia's walker 1 tile off it attacks (`AttackObject`), one 4.5 tiles off it finds and leaves,
/// one 6 tiles off it does not find. Away from its state position, a stance-2 soldier walks
/// back ("I'm in Defend or `StandGround`, returning to state pos.", `FUN_140c5e7d0`), which its
/// processIdle does not count as an action (5); one in a group stays.
#[test]
fn an_idle_soldier_caps_its_human_order_time_and_searches_for_something_to_attack() {
    let scout = (player_id(2), 50);
    let run = |others: &[SceneObject]| {
        let mut world = idle_scout_world(idle_scout_unit(player_id(2), idle_scout()), others);
        let (answer, unit_ai, _) = update_of(&mut world, scout, Millis::new(28), WorldTime::ZERO);
        assert_eq!(answer, Ok(None));
        (world, unit_ai)
    };
    let (world, unit_ai) = run(&[scout_foe(77, player_id(1), 6.0)]);
    assert_eq!(
        (found_ids(&world), lone_attacks(&world), lone_walks(&world)),
        (vec![], None, None)
    );
    assert_eq!(unit_ai.idle_timer, Millis::ZERO);
    assert_eq!(lone_human_order(&world, player_id(2)), HUMAN_ORDER_CAP);
    let (world, _) = run(&[scout_foe(77, player_id(1), 4.5)]);
    assert_eq!(
        (found_ids(&world), lone_attacks(&world)),
        (vec![object_id(77)], None)
    );
    for owner in [player_id(1), PlayerId::GAIA] {
        let (world, unit_ai) = run(&[scout_foe(77, owner, 1.0)]);
        assert_eq!(
            (lone_attacks(&world), unit_ai.current_action),
            (Some(object_id(77)), Some(AiAction::Attack)),
            "{owner:?}"
        );
    }
    let away = UnitAi {
        state_position: Point::new(25.0, 92.5),
        ..idle_scout()
    };
    let mut world = idle_scout_world(idle_scout_unit(player_id(2), away.clone()), &[]);
    let (_, unit_ai, _) = update_of(&mut world, scout, Millis::new(28), WorldTime::ZERO);
    assert_eq!(
        lone_walks(&world),
        Some((Position::new(25.0, 92.5, 0.0), Distance::new(1.0)))
    );
    assert_eq!(
        (unit_ai.last_action, unit_ai.current_action),
        (None, Some(AiAction::Move))
    );
    // in a group (its +0x6d0, a group the world need not hold) it stays
    let mut world = idle_scout_world(idle_scout_unit(player_id(2), away), &[]);
    world.players[2]
        .objects
        .by_id_mut(object_id(50))
        .unwrap()
        .group
        .id = Some(group_id(7));
    let (answer, ..) = update_of(&mut world, scout, Millis::new(28), WorldTime::ZERO);
    assert_eq!((answer, lone_walks(&world)), (Ok(None), None));
}

/// Gaia's soldier that takes owners by sight caps its last human order time and does not
/// search: player 1's walker 4.5 tiles off stays unattacked. The capture flag the unit AI reads
/// is the sight registry's (`Seer::capture`), so the soldier is in the registry.
#[test]
fn gaia_soldier_that_takes_owners_by_sight_does_not_search() {
    let scout = SceneObject {
        capture: CaptureKind::BySight,
        seen_by: Some(VisionBits::visible(PlayerId::GAIA)),
        ..idle_scout_unit(PlayerId::GAIA, idle_scout())
    };
    let near = [scout_foe(77, player_id(1), 4.5)];
    let mut world = idle_scout_world(scout, &near);
    let (answer, ..) = update_of(
        &mut world,
        (PlayerId::GAIA, 50),
        Millis::new(28),
        WorldTime::ZERO,
    );
    assert_eq!((answer, lone_attacks(&world)), (Ok(None), None));
    assert!(world.spatial.searched().is_none());
    assert_eq!(lone_human_order(&world, PlayerId::GAIA), HUMAN_ORDER_CAP);
}

/// Player 2's town centre 9156 in MarvinBlue/426048731's update 1506.
const TC: Attacker = Attacker {
    ai_kind: UnitAiKind::Tower,
    player: PlayerId::new(2),
    player_kind: PlayerKind::Human,
    state: ObjectState::Alive,
    held: None,
    top_action: None,
    master: ids::TOWN_CENTRE,
    kind: MasterKind::BUILDING,
    class: Some(class::BUILDING),
    attack_priority: AttackPriority::PRIORITY_0,
    missile: Some(MasterId::named(54)),
    range: Distance::new(6.0),
    min_range: Distance::ZERO,
    charge_kind: ChargeKind::NONE,
    area_effect: Distance::ZERO,
    blast_level: BlastLevel::NONE,
    los: Distance::new(8.0),
    position: Position::new(29.0, 94.0, 0.0),
    radius: Footprint::new(2.0, 2.0),
    speed: Speed::ZERO,
    scenario: false,
    firepower: Firepower::new(0.0),
    capture: CaptureKind::None,
};

/// Gaia's boar 9731 (master 48, class 10).
const BOAR: Target = Target {
    owner: PlayerId::new(0),
    class: class::PREDATOR,
    kind: MasterKind::COMBAT,
    master: MasterId::named(48),
    combat_level: CombatLevel::NONE,
    capture: CaptureKind::None,
    flags: AttackTargetFlags::HAS_AI
        .union(AttackTargetFlags::TARGETING.union(AttackTargetFlags::ATTACKABLE)),
    position: Position::new(30.0, 98.0, 0.0),
    radius: Footprint::new(0.5, 0.5),
};

/// A unit AI without an order or a target, in the aggressive stance.
const IDLE: Seeker = Seeker {
    stance: UnitStance::Aggressive,
    state_position: Point::new(0.0, 0.0),
    order: None,
    action: None,
    target: None,
    target_class: None,
    last_target_class: None,
    patrol_path: false,
    target_master_kind: None,
};

/// Slot 0xa0 by unit-AI type: the base's always, the building's never, the hunted
/// animal's only with an Attack (or a type 7) in its list, the predator's and lion's by
/// class under attack priority 0 (anything under 1 and 2), the civilian's but classes 1 and
/// 42; the priest's is not modelled.
#[test]
fn each_unit_ai_kind_has_its_own_attack_check() {
    let check =
        |ai_kind: UnitAiKind, top_action: Option<ActionKind>, priority: u8, class: ClassId| {
            let me = Attacker {
                ai_kind,
                top_action,
                attack_priority: AttackPriority::of_byte(priority),
                ..TC
            };
            can_attack(
                &me,
                &IDLE,
                &Target { class, ..BOAR },
                &roles(),
                &tuning().unit_ai,
            )
        };
    for unit_ai_kind in [
        UnitAiKind::Boar,
        UnitAiKind::Ranged,
        UnitAiKind::Ranged2,
        UnitAiKind::Trade,
        UnitAiKind::Transport,
        UnitAiKind::Fishing,
        UnitAiKind::Artifact,
        UnitAiKind::SiegeTower,
        UnitAiKind::Livestock,
    ] {
        assert_eq!(
            check(unit_ai_kind, None, 0, class::RELIC),
            Ok(true),
            "{unit_ai_kind}"
        );
    }
    assert_eq!(
        check(UnitAiKind::Building, None, 0, class::VILLAGER),
        Ok(false)
    );
    assert_eq!(
        check(UnitAiKind::Hunted, None, 0, class::VILLAGER),
        Ok(false)
    );
    assert_eq!(
        check(UnitAiKind::Hunted, Some(ATTACK), 0, class::VILLAGER),
        Ok(true)
    );
    assert_eq!(
        check(
            UnitAiKind::Hunted,
            Some(ActionKind::COMBAT),
            0,
            class::VILLAGER
        ),
        Ok(true)
    );
    // the predator's list and the lion's: 4 (villagers) yes, 10 no
    assert_eq!(
        (
            check(UnitAiKind::Predator, None, 0, class::VILLAGER),
            check(UnitAiKind::Predator, None, 0, class::PREDATOR)
        ),
        (Ok(true), Ok(false))
    );
    assert_eq!(
        check(UnitAiKind::Predator, None, 2, class::PREDATOR),
        Ok(true)
    );
    assert_eq!(
        (
            check(UnitAiKind::Lion, None, 0, class::PREY),
            check(UnitAiKind::Predator, None, 0, class::PREY)
        ),
        (Ok(true), Ok(false))
    );
    assert_eq!(
        check(UnitAiKind::Civilian, None, 0, class::RELIC),
        Ok(false)
    );
    assert_eq!(
        check(UnitAiKind::Civilian, None, 0, class::PREDATOR),
        Ok(true)
    );
    assert!(check(UnitAiKind::Priest, None, 0, class::VILLAGER).is_err());
}

#[test]
fn tower_passes_boar_but_not_wolf() {
    assert_eq!(
        can_attack(&TC, &IDLE, &BOAR, &roles(), &tuning().unit_ai),
        Ok(false)
    );
    let wolf = Target {
        combat_level: CombatLevel::MILITARY,
        ..BOAR
    };
    assert_eq!(
        can_attack(&TC, &IDLE, &wolf, &roles(), &tuning().unit_ai),
        Ok(true)
    );
    // Gaia's tower attacks nothing; a Gaia object outside classes 9 and 10 that takes owners
    // by sight neither
    assert_eq!(
        can_attack(
            &Attacker {
                player: PlayerId::new(0),
                ..TC
            },
            &IDLE,
            &wolf,
            &roles(),
            &tuning().unit_ai
        ),
        Ok(false)
    );
    let sheep = Target {
        class: class::LIVESTOCK,
        capture: CaptureKind::Livestock,
        ..BOAR
    };
    assert_eq!(
        can_attack(&TC, &IDLE, &sheep, &roles(), &tuning().unit_ai),
        Ok(false)
    );
    // a type whose attack check is not modelled (the priest's conversion test)
    assert!(
        can_attack(
            &Attacker {
                ai_kind: UnitAiKind::Priest,
                ..TC
            },
            &IDLE,
            &BOAR,
            &roles(),
            &tuning().unit_ai
        )
        .is_err()
    );
    // a tower without a missile is armed only by the firepower of the units inside it
    let bare = Attacker {
        missile: None,
        ..TC
    };
    let enemy = Target {
        owner: PlayerId::new(1),
        class: class::CLASS_6,
        ..BOAR
    };
    assert_eq!(
        can_attack(&bare, &IDLE, &enemy, &roles(), &tuning().unit_ai),
        Ok(false)
    );
    let manned = Attacker {
        firepower: Firepower::new(2.0),
        ..bare
    };
    assert_eq!(
        can_attack(&manned, &IDLE, &enemy, &roles(), &tuning().unit_ai),
        Ok(true)
    );
}

/// A scout cavalry (master 448, class 47) of player 1 in `StandGround` at its state position,
/// as records/425906646's 27272 at update 4268.
const SCOUT: Attacker = Attacker {
    ai_kind: UnitAiKind::Soldier,
    player: PlayerId::new(1),
    kind: MasterKind::COMBAT,
    class: Some(class::CLASS_47),
    master: ids::ID_448,
    missile: None,
    range: Distance::ZERO,
    los: Distance::new(4.0),
    position: Position::new(118.5, 183.5, 0.0),
    radius: Footprint::new(0.4, 0.4),
    speed: Speed::new(1.2),
    ..TC
};
const STAND_GROUND: Seeker = Seeker {
    stance: UnitStance::StandGround,
    state_position: Point::new(118.5, 183.5),
    ..IDLE
};

#[test]
fn soldier_standing_its_ground_attacks_within_its_range_of_its_state_position() {
    let villager = |x: f32| Target {
        owner: PlayerId::new(2),
        class: class::VILLAGER,
        master: ids::VILLAGER_MALE,
        position: Position::new(x, 183.5, 0.0),
        radius: Footprint::new(0.2, 0.2),
        ..BOAR
    };
    // the range against it plus 0.5 from the state position to its footprint
    let reach = |seeker: &Seeker, x: f32| {
        soldier_can_attack(&SCOUT, seeker, &villager(x), &roles(), &tuning().unit_ai)
    };
    assert_eq!(reach(&STAND_GROUND, 119.2), Ok(true));
    assert_eq!(reach(&STAND_GROUND, 119.3), Ok(false));
    // on order 0x2bd anywhere; in Defend 5 more
    let ordered = Seeker {
        order: Some(OrderKind::Defend),
        ..STAND_GROUND
    };
    assert_eq!(reach(&ordered, 130.0), Ok(true));
    let defend = Seeker {
        stance: UnitStance::Defensive,
        ..STAND_GROUND
    };
    assert_eq!(reach(&defend, 124.2), Ok(true));
    assert_eq!(reach(&defend, 124.3), Ok(false));
    // never in NoAttack, never an object its targeting flag leaves out, never class 1
    let no_attack = Seeker {
        stance: UnitStance::NoAttack,
        ..STAND_GROUND
    };
    assert_eq!(reach(&no_attack, 119.0), Ok(false));
    let untargeted = Target {
        flags: AttackTargetFlags::empty(),
        ..villager(119.0)
    };
    assert_eq!(
        soldier_can_attack(
            &SCOUT,
            &STAND_GROUND,
            &untargeted,
            &roles(),
            &tuning().unit_ai
        ),
        Ok(false)
    );
    let relic = Target {
        class: class::ARTIFACT,
        ..villager(119.0)
    };
    assert_eq!(
        soldier_can_attack(&SCOUT, &STAND_GROUND, &relic, &roles(), &tuning().unit_ai),
        Ok(false)
    );
}

/// The tower's idle check caps its last human order time and searches its cared classes
/// within its line of sight plus 1 (9, from its footprint): of player 1's villagers 8.5 and 11
/// tiles off and Gaia's sheep (class 58, none of its classes) 4 tiles off, it finds the first
/// villager alone.
#[test]
fn an_idle_tower_searches_with_its_classes() {
    let sheep = SceneObject {
        ai: Some(idle(UnitAiKind::Livestock)),
        ..SceneObject::new(
            78,
            PlayerId::GAIA,
            walker(594, class::LIVESTOCK),
            Position::new(30.0, 34.0, 0.0),
        )
    };
    let mut world = tower_world(
        idle_tower(UnitStance::Aggressive),
        &[tower_villager(9731, 8.5), tower_villager(9732, 11.0), sheep],
    );
    let (answer, ..) = update_of(
        &mut world,
        (player_id(2), 500),
        Millis::new(28),
        WorldTime::ZERO,
    );
    assert_eq!(answer.map(|_| ()), Ok(()));
    assert_eq!(found_ids(&world), vec![object_id(9731)]);
    assert_eq!(tower_human_order(&world), HUMAN_ORDER_CAP);
}

/// A tower's unit AI whose idle timer passed its timeout, in `stance`.
fn idle_tower(stance: UnitStance) -> UnitAi {
    UnitAi {
        idle_timer: Millis::new(504),
        adjusted_idle_timeout: Millis::new(503),
        stance,
        ..idle(UnitAiKind::Tower)
    }
}

/// Player 2's tower 500 (master 79, class 52, radius 1) at (30, 30) on its sprite, its unit AI
/// `unit_ai`, and the `others`, each seen by player 2; the tower is armed (its missile 363,
/// weapon range 8, line of sight 8) and a moving object of speed 0, as a building's master is.
/// Its last human order time is past the cap.
fn tower_world(unit_ai: UnitAi, others: &[SceneObject]) -> World {
    let tower = Master {
        id: master_id(79),
        kind: MasterKind::BUILDING,
        class: class::TOWER,
        radius: Footprint::new(1.0, 1.0),
        height: Distance::new(3.0),
        hit_points: MasterHitPoints::of_data(1020),
        line_of_sight: Distance::new(8.0),
        terrain_restriction: Some(RestrictionId::named(7)),
        move_stats: Some(walking(Speed::ZERO)),
        standing_sprite: Some(STANDING.id),
        combat: Some(CombatPart {
            weapon_range: Distance::new(8.0),
            missile: MasterField::of_data(363),
            ..CombatPart::default()
        }),
        ..Master::default()
    };
    let mut scene = Scene::open(GridSize::new(64, 64))
        .players(2)
        .graphic(STANDING)
        .object(SceneObject {
            ai: Some(unit_ai),
            sprite: Some(STANDING.id),
            ..SceneObject::new(500, player_id(2), tower, Position::new(30.0, 30.0, 0.0))
        });
    for other in others {
        scene = scene.object(SceneObject {
            seen_by: Some(VisionBits::visible(player_id(2))),
            ..other.clone()
        });
    }
    let mut world = scene.build();
    world.players[2]
        .objects
        .by_id_mut(object_id(500))
        .unwrap()
        .last_human_order = WorldTime::from_millis(9000);
    world
}

/// Player 1's idle villager `id` `dx` tiles east of [`tower_world`]'s tower.
fn tower_villager(id: i32, dx: f32) -> SceneObject {
    SceneObject {
        ai: Some(idle(UnitAiKind::Civilian)),
        ..SceneObject::new(
            id,
            player_id(1),
            villager_master(),
            Position::new(30.0 + dx, 30.0, 0.0),
        )
    }
}

/// The last human order time of [`tower_world`]'s tower.
fn tower_human_order(world: &World) -> WorldTime {
    world.players[2]
        .objects
        .by_id(object_id(500))
        .unwrap()
        .last_human_order
}

/// The ids the last search of `world` found, of every relation ([`Spatial::searched`]).
fn found_ids(world: &World) -> Vec<ObjectId> {
    world.spatial.searched().map_or_else(Vec::new, |lists| {
        lists
            .iter()
            .flat_map(|(_, records)| records.iter().map(|record| record.id))
            .collect()
    })
}

/// `FUN_140c4dcb0`: both targets and their classes go and the point is −1 on all three axes;
/// the order and action stay. MOVE's notify to a unit that took its `MoveTo` forgets the same
/// way before it takes the order's target.
#[test]
fn forgetting_the_targets_clears_the_current_and_the_last_and_the_point() {
    let mut unit_ai = UnitAi {
        current_order: Some(OrderKind::Attack),
        current_action: Some(AiAction::Attack),
        current_target: Some(object_id(12)),
        target_class: class_of(4),
        target_point: Position::new(3.0, 4.0, 0.5),
        last_target: Some(object_id(11)),
        last_target_class: class_of(9),
        ..idle(UnitAiKind::Soldier)
    };
    unit_ai.forget_targets();
    assert_eq!(
        (
            unit_ai.current_target,
            unit_ai.target_class,
            unit_ai.last_target,
            unit_ai.last_target_class
        ),
        (None, None, None, None)
    );
    assert_eq!(unit_ai.target_point, Position::new(-1.0, -1.0, -1.0));
    assert_eq!(
        (unit_ai.current_order, unit_ai.current_action),
        (Some(OrderKind::Attack), Some(AiAction::Attack))
    );
    let mut moved = UnitAi {
        current_target: Some(object_id(12)),
        target_class: class_of(4),
        last_target: Some(object_id(11)),
        last_target_class: class_of(9),
        ..idle(UnitAiKind::Soldier)
    };
    moved.notify_move(true, None, Point::new(5.0, 6.0), &mut rng());
    // the forget, then FUN_140c64be0 with no target: the forgotten -1 becomes the last
    assert_eq!(
        (
            moved.last_target,
            moved.last_target_class,
            moved.current_target
        ),
        (None, None, None)
    );
    assert_eq!(moved.target_point, Position::new(5.0, 6.0, 0.0));
}

/// `FUN_140c4eea0`: a master with a task of action type 0x99 refuses orders while its target
/// exists or it has an action value; with the second argument set a unit that passes takes
/// orders only with that task, and without it any unit that passes does.
#[test]
fn unit_takes_orders_by_its_0x99_task_and_with_task_only_needs_it() {
    let mut unit_ai = idle(UnitAiKind::Soldier);
    let plain = no_surroundings();
    let carter = Surroundings {
        flags: SurroundingFlags::TASK_0X99,
        roles: roles(),
        slots: slots(),
        unit_ai_tuning: tuning().unit_ai,
        ..Surroundings::default()
    };
    assert!(unit_ai.can_order(&plain, OrderTest::Any));
    assert!(!unit_ai.can_order(&plain, OrderTest::TaskOnly));
    assert!(unit_ai.can_order(&carter, OrderTest::Any));
    assert!(unit_ai.can_order(&carter, OrderTest::TaskOnly));
    unit_ai.current_action = Some(AiAction::Gather);
    assert!(!unit_ai.can_order(&carter, OrderTest::Any));
    assert!(!unit_ai.can_order(&carter, OrderTest::TaskOnly));
    assert!(unit_ai.can_order(&plain, OrderTest::Any));
    unit_ai.current_action = None;
    let targeted = Surroundings {
        target: Some(Seen::default()),
        ..carter
    };
    assert!(!unit_ai.can_order(&targeted, OrderTest::TaskOnly));
}

/// `FUN_140c1a760` at `LAB_140c1aa59`: an enemy building (master type `'P'`) passes the
/// tower's check once a builder started it; a foundation still desolid (`+0xc60`) does not.
/// The flag counts only for a building: a unit that holds it passes.
#[test]
fn tower_attacks_started_building_but_not_desolid_foundation() {
    let house = Target {
        owner: PlayerId::new(1),
        class: class::BUILDING,
        kind: MasterKind::BUILDING,
        master: ids::HOUSE,
        ..BOAR
    };
    assert_eq!(
        tower_can_attack(&TC, &house, &roles(), &tuning().unit_ai),
        Ok(true)
    );
    let foundation = Target {
        flags: AttackTargetFlags::DESOLID,
        ..house
    };
    assert_eq!(
        tower_can_attack(&TC, &foundation, &roles(), &tuning().unit_ai),
        Ok(false)
    );
    let unit = Target {
        owner: PlayerId::new(1),
        class: class::CLASS_6,
        flags: AttackTargetFlags::DESOLID,
        ..BOAR
    };
    assert_eq!(
        tower_can_attack(&TC, &unit, &roles(), &tuning().unit_ai),
        Ok(true)
    );
}

/// `FUN_140b43f30` on the attackers (`+0xc0`): an id joins the set in its sorted place and
/// the call answers 1; an id already in it changes nothing and answers 0.
#[test]
fn an_attacker_joins_the_sorted_set_once() {
    let mut unit_ai = idle(UnitAiKind::Boar);
    assert!(unit_ai.attackers.add(Some(object_id(30))));
    assert!(unit_ai.attackers.add(Some(object_id(10))));
    assert!(unit_ai.attackers.add(Some(object_id(20))));
    assert!(!unit_ai.attackers.add(Some(object_id(10))));
    assert!(unit_ai.attackers.add(None));
    assert_eq!(unit_ai.attackers, Attackers::of_saved(&[-1, 10, 20, 30]));
}

/// The soldier's slot 0xa8 (`FUN_140c1ae60`): a neutral object of a class in the mask
/// `0x200200800838001c` is passed over; at attack priority 3 only [`PRIORITY_3_CLASSES`]
/// pass; any other class passes, also one outside the mask's 62 bits.
#[test]
fn the_soldier_passes_over_neutrals_of_its_masked_classes() {
    for class in [2, 3, 4, 19, 20, 21, 27, 39, 49, 61].map(ClassId::of_master) {
        assert!(
            !soldier_attacks_neutral(AttackPriority::PRIORITY_0, class, &roles()),
            "class {class}"
        );
    }
    for class in [0, 6, 9, 15, 35, 52, 53, 60, 62, 63, -1].map(ClassId::of_master) {
        assert!(
            soldier_attacks_neutral(AttackPriority::PRIORITY_0, class, &roles()),
            "class {class}"
        );
    }
    assert!(soldier_attacks_neutral(
        AttackPriority::PRIORITY_3,
        class::TOWER,
        &roles()
    ));
    assert!(!soldier_attacks_neutral(
        AttackPriority::PRIORITY_3,
        class::CLASS_6,
        &roles()
    ));
    // a masked class stays passed over whatever the priority
    assert!(!soldier_attacks_neutral(
        AttackPriority::PRIORITY_3,
        class::WALL,
        &roles()
    ));
}

/// `FUN_140c42640`: an order goes after the last unless it goes in front of a queue that
/// holds one.
#[test]
fn an_inserted_order_goes_in_front_only_of_queue_that_holds_one() {
    let order = |target| QueuedOrder {
        target,
        order: OrderKind::Move,
        ready: true,
        ..QueuedOrder::default()
    };
    let mut unit_ai = idle(UnitAiKind::Soldier);
    unit_ai.insert_order(order(Some(object_id(1))), QueueEnd::Front);
    unit_ai.insert_order(order(Some(object_id(2))), QueueEnd::Back);
    unit_ai.insert_order(order(Some(object_id(3))), QueueEnd::Front);
    unit_ai.insert_order(order(Some(object_id(4))), QueueEnd::Back);
    let targets: Vec<Option<ObjectId>> = unit_ai
        .orders
        .iter()
        .map(|queued_order| queued_order.target)
        .collect();
    assert_eq!(
        targets,
        [
            Some(object_id(3)),
            Some(object_id(1)),
            Some(object_id(2)),
            Some(object_id(4))
        ]
    );
}

/// `FUN_140c55c90` immediate and in front: a builder (702/602 on foundation 60) told to
/// move at once keeps its build order: the build goes in front of the queue with the move's
/// shown point and ready flag (issuer the unit, owner −1, its range `+0x5c`), the move in
/// front of it; the target goes, the action stops and ends the build order (602 + 100), the
/// move runs (slot 0x230: its `MoveTo` to the point) and leaves the queue; the build waits
/// behind.
#[test]
fn an_immediate_order_in_front_keeps_the_current_order_behind_it() {
    let building = UnitAi {
        current_order: Some(OrderKind::Build),
        order_priority: OrderPriority::of_data(2),
        current_action: Some(AiAction::Build),
        current_target: Some(object_id(60)),
        target_class: class_of(3),
        target_point: Position::new(20.0, 21.0, 0.0),
        desired_distance: Distance::new(2.0),
        ..idle(UnitAiKind::Civilian)
    };
    let order = QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::Move,
        priority: OrderPriority::of_data(7),
        target: None,
        target_owner: None,
        point: Position::new(10.0, 11.0, 0.0),
        range: Distance::new(0.5),
        shown: Point::new(10.0, 11.0),
        ready: true,
        target_kind: None,
    };
    let builder = lone_unit(player_id(1), building, Position::new(5.0, 6.0, 0.0));
    let mut world = lone_world(on_its_way(builder, Position::new(20.0, 21.0, 0.0)), &[]);
    let answer = run_ai(
        &mut world,
        (player_id(1), object_id(50)),
        |ai, at, side, _, rng| {
            ai.order_now(
                OrderNow {
                    order,
                    keep_from: Some(object_id(50)),
                    place: Some((at.position.ground(), at.class)),
                },
                None,
                rng,
                side,
                AiLayer {
                    roles: &at.roles,
                    slots: at.slots,
                    unit_ai_tuning: &at.unit_ai_tuning,
                },
            )
        },
    );
    assert!(answer.is_ok());
    let kept = QueuedOrder {
        issuer: Some(Issuer::Object(object_id(50))),
        order: OrderKind::Build,
        priority: OrderPriority::of_data(2),
        target: Some(object_id(60)),
        target_owner: None,
        point: Position::new(20.0, 21.0, 0.0),
        range: Distance::new(2.0),
        shown: Point::new(10.0, 11.0),
        ready: true,
        target_kind: order_kind(3),
    };
    let unit_ai = world.parts.unit_ai.get(object_id(50)).unwrap();
    assert_eq!(unit_ai.orders, vec![kept]);
    assert_eq!(
        lone_walks(&world),
        Some((Position::new(10.0, 11.0, 0.0), Distance::new(0.5)))
    );
    assert_eq!(
        (unit_ai.current_order, unit_ai.order_priority.get()),
        (Some(OrderKind::Move), 7)
    );
    // the stop ended the build order; the move's FUN_140c64c10 made the forgotten -1 the
    // last target
    assert_eq!(
        (unit_ai.last_order, unit_ai.last_target),
        (Some(OrderKind::Build), None)
    );
    assert_eq!(unit_ai.state_position, Point::new(5.0, 6.0));
}

/// `FUN_140c55c90` immediate for an attack roundup (0x2d9): "We have an `AttackRoundup`, so
/// I'm not stopping the object.": the target stays and nothing stops; the notifies still go
/// and slot 0x230 runs the order in front: its roundup (slot 0xc0) attacks the new target,
/// and the tail makes the order 700. Outside the update (no surroundings) the case does not
/// run.
#[test]
fn an_immediate_attack_roundup_keeps_the_target_until_it_attacks_the_next() {
    let attacking = || UnitAi {
        current_order: Some(OrderKind::Attack),
        current_action: Some(AiAction::Attack),
        current_target: Some(object_id(50)),
        notifies: vec![Notify::own(object_id(9), NotifyKind::DAMAGED, [1, 5, 0])],
        ..idle(UnitAiKind::Soldier)
    };
    let roundup = QueuedOrder {
        order: OrderKind::AttackRoundup,
        priority: OrderPriority::of_data(100),
        target: Some(object_id(51)),
        ready: true,
        ..QueuedOrder::default()
    };
    let enemy = order_target(51, player_id(2), class::CLASS_6);
    let mut world = order_world(order_unit(order_soldier(), attacking()), from_ref(&enemy));
    let answer = run_ai(
        &mut world,
        (player_id(1), object_id(9)),
        |ai, at, side, _, rng| {
            ai.take_order_now(
                ObjectState::Alive,
                roundup,
                (Point::ORIGIN, Some(class::CLASS_6)),
                rng,
                side,
                AiLayer {
                    roles: &at.roles,
                    slots: at.slots,
                    unit_ai_tuning: &at.unit_ai_tuning,
                },
            )
        },
    );
    assert!(answer.is_err());
    assert_eq!(attacks_9(&world), None);
    let unit_ai = ai_9(&world);
    assert_eq!(
        (unit_ai.current_target, unit_ai.current_action),
        (Some(object_id(50)), Some(AiAction::Attack))
    );
    assert_eq!(unit_ai.notifies, Vec::new());
    let mut world = order_world(order_unit(order_soldier(), attacking()), &[enemy]);
    run_ai(
        &mut world,
        (player_id(1), object_id(9)),
        |ai, at, side, kind, rng| {
            ai.order_now(
                OrderNow {
                    order: roundup,
                    keep_from: None,
                    place: None,
                },
                Some((at, kind)),
                rng,
                side,
                AiLayer {
                    roles: &at.roles,
                    slots: at.slots,
                    unit_ai_tuning: &at.unit_ai_tuning,
                },
            )
        },
    )
    .unwrap();
    assert_eq!(attacks_9(&world), Some(object_id(51)));
    let unit_ai = ai_9(&world);
    assert_eq!(
        (
            unit_ai.current_target,
            unit_ai.last_target,
            unit_ai.current_action,
            unit_ai.last_action
        ),
        (
            Some(object_id(51)),
            Some(object_id(50)),
            Some(AiAction::Attack),
            Some(AiAction::Attack)
        )
    );
    assert_eq!(
        (unit_ai.current_order, unit_ai.order_priority.get()),
        (Some(OrderKind::Attack), 100)
    );
    // the old notifies go; the new Attack's start tells the AI its target is out of reach
    assert_eq!(
        unit_ai.notifies,
        [Notify::own(object_id(9), NotifyKind::OUT_OF_REACH, [0; 3])]
    );
    assert_eq!(unit_ai.orders, Vec::new());
}

/// `FUN_140c5e7d0`: a unit more than a tile (the squared footprint gap, `FUN_140a847c0`)
/// from its state position moves there at its height within 1.0, forced and running (the
/// unit's slot 0x340); the move action value becomes current, the old one the last, and the
/// target stays. Within a tile nothing happens. A gap that is not a number walks too
/// (`COMISS 1.0, gap; JNC`).
#[test]
fn unit_off_its_state_position_walks_back_and_keeps_its_target() {
    let attacking = UnitAi {
        current_action: Some(AiAction::Attack),
        current_target: Some(object_id(77)),
        state_position: Point::new(10.0, 10.0),
        ..idle(UnitAiKind::Soldier)
    };
    let walk_back = |unit_ai: UnitAi, x: f32| {
        let unit = lone_unit(player_id(1), unit_ai, Position::new(x, 10.0, 2.5));
        let master = Master {
            radius: Footprint::new(0.5, 0.5),
            ..unit.master
        };
        let mut world = lone_world(SceneObject { master, ..unit }, &[]);
        let answer = run_ai(
            &mut world,
            (player_id(1), object_id(50)),
            |ai, at, side, _, rng| ai.walk_back(at, side, rng),
        );
        let unit_ai = world.parts.unit_ai.get(object_id(50)).unwrap().clone();
        (answer, unit_ai, lone_walks(&world))
    };
    // 1.4 off, 0.9 off its footprint: within a tile
    let (answer, unit_ai, walk) = walk_back(attacking.clone(), 11.4);
    assert_eq!(
        (answer, unit_ai.current_action, walk),
        (Ok(WalkBack::Stays), Some(AiAction::Attack), None)
    );
    let (answer, unit_ai, walk) = walk_back(attacking, 11.6);
    assert_eq!(
        (answer, walk),
        (
            Ok(WalkBack::Walks),
            Some((Position::new(10.0, 10.0, 2.5), Distance::new(1.0)))
        )
    );
    assert_eq!(
        (unit_ai.last_action, unit_ai.current_action),
        (Some(AiAction::Attack), Some(AiAction::Move))
    );
    assert_eq!(unit_ai.current_target, Some(object_id(77)));
    let lost = UnitAi {
        state_position: Point::new(f32::NAN, 10.0),
        ..idle(UnitAiKind::Soldier)
    };
    assert_eq!(walk_back(lost, 10.0).0, Ok(WalkBack::Walks));
}

/// `FUN_140a847c0` clamps each axis by `MAXSS 0, gap`: a gap that is not a number stays one.
#[test]
fn the_point_gap_keeps_gap_that_is_not_number() {
    assert_eq!(
        point_gap_squared(
            Point::new(3.0, 0.0),
            Position::ORIGIN,
            Footprint::new(1.0, 1.0)
        ),
        DistanceSquared::new(4.0)
    );
    assert!(
        point_gap_squared(
            Point::new(f32::NAN, 0.0),
            Position::ORIGIN,
            Footprint::new(1.0, 1.0)
        )
        .get()
        .is_nan()
    );
}

/// `FUN_140c20db0` in `NoAttack` ("searching and returning to state pos."): the search alone
/// (slot 0x50 with mode 1), then off the patrol orders the walk back; processIdle stays 5. On
/// a patrol order it stays. A search that finds enemies (player 1's walker 4.5 tiles off) turns
/// the alert mode on and restarts the sighting time; the soldier still walks back.
#[test]
fn no_attack_soldier_searches_then_walks_back_to_its_state_position() {
    let peaceful = UnitAi {
        stance: UnitStance::NoAttack,
        state_position: Point::new(25.0, 92.5),
        ..idle_scout()
    };
    let scout = (player_id(2), 50);
    let mut world = idle_scout_world(idle_scout_unit(player_id(2), peaceful.clone()), &[]);
    let (answer, ..) = update_of(&mut world, scout, Millis::new(28), WorldTime::ZERO);
    assert_eq!(answer, Ok(None));
    assert!(world.spatial.searched().is_some());
    assert_eq!(
        lone_walks(&world),
        Some((Position::new(25.0, 92.5, 0.0), Distance::new(1.0)))
    );
    assert_eq!(lone_human_order(&world, player_id(2)), HUMAN_ORDER_CAP);
    let near = [scout_foe(77, player_id(1), 4.5)];
    let sighted_long_ago = UnitAi {
        time_since_enemy: Seconds::new(12.0),
        ..peaceful.clone()
    };
    let mut world = idle_scout_world(idle_scout_unit(player_id(2), sighted_long_ago), &near);
    let (answer, unit_ai, _) = update_of(&mut world, scout, Millis::new(28), WorldTime::ZERO);
    assert_eq!(answer, Ok(None));
    assert_eq!(
        (unit_ai.alert_mode, unit_ai.time_since_enemy),
        (AlertMode::Alert, Seconds::ZERO)
    );
    assert_eq!(
        lone_walks(&world),
        Some((Position::new(25.0, 92.5, 0.0), Distance::new(1.0)))
    );
    let patrolling = UnitAi {
        current_order: Some(OrderKind::Patrol),
        ..peaceful
    };
    let mut world = idle_scout_world(idle_scout_unit(player_id(2), patrolling), &[]);
    let answer = run_ai(
        &mut world,
        (player_id(2), object_id(50)),
        |ai, at, side, kind, rng| ai.no_attack_idle(at, kind, side, rng),
    );
    assert!(answer.is_ok());
    assert_eq!(lone_walks(&world), None);
}

/// The full update (`0x140c66b10`) of a unit in a group the world holds: its state position
/// follows it while its group has a command (`+0x3a8`); its idle check (slot 0x240) waits
/// while its group is not idle (`FUN_141017ad0`: its motion moves) or the unit moves (slot
/// 0x5c8); its order queue waits while the group is not idle; a group that patrols takes the
/// patrol's idle part, and its target's range test (`FUN_140c41690`) answers 0 for a human
/// player; action `0x2c0` (moved aside) ends, the last action keeping it, unless the group
/// moves. A group on a move that patrols is idle, so the moving patrol follows.
#[test]
fn grouped_unit_full_update_reads_its_group() {
    let ship = || UnitAi {
        idle_timer: Millis::new(3001),
        adjusted_idle_timeout: Millis::new(3000),
        ..idle(UnitAiKind::Transport)
    };
    let still = Some((GroupCommandKind::MOVE, BVector::ZERO, GroupFlags::empty()));
    let moving = Some((
        GroupCommandKind::MOVE,
        BVector::new(0.5, 0.0, 0.0),
        GroupFlags::empty(),
    ));
    let step = |world: &mut World| lone_step(world, Millis::new(28), WorldTime::ZERO);
    // an idle group of a still unit: the idle check runs, the state position follows
    let mut world = grouped_ship_world(ship(), still, (false, &[]));
    let (_, unit_ai, _) = step(&mut world);
    assert_eq!(lone_human_order(&world, player_id(1)), HUMAN_ORDER_CAP);
    assert_eq!(unit_ai.state_position, Point::new(30.0, 30.0));
    // a group not idle, or a moving unit: no idle check
    for (group, member_moving) in [(moving, false), (still, true)] {
        let mut world = grouped_ship_world(ship(), group, (member_moving, &[]));
        let (answer, unit_ai, _) = step(&mut world);
        assert_eq!(answer, Ok(None));
        assert_eq!(unit_ai.idle_timer, Millis::new(3029));
        assert_eq!(
            lone_human_order(&world, player_id(1)),
            WorldTime::from_millis(9000)
        );
    }
    // a group without a command: the state position stays
    let uncommanded = Some((GroupCommandKind::NONE, BVector::ZERO, GroupFlags::empty()));
    let mut world = grouped_ship_world(ship(), uncommanded, (false, &[]));
    let (_, unit_ai, _) = step(&mut world);
    assert_eq!(unit_ai.state_position, Point::ORIGIN);
    // the order queue waits for a group that is not idle
    let order = QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::Move,
        priority: OrderPriority::of_data(100),
        target: None,
        target_owner: None,
        point: Position::new(39.0, 39.0, 0.0),
        range: Distance::ZERO,
        shown: Point::new(39.0, 39.0),
        ready: true,
        target_kind: None,
    };
    let ordered = UnitAi {
        orders: vec![order],
        ..idle(UnitAiKind::Transport)
    };
    let mut world = grouped_ship_world(ordered, moving, (false, &[]));
    let (_, unit_ai, _) = step(&mut world);
    assert_eq!(unit_ai.orders.len(), 1);
    world.groups.get_mut(group_id(7)).unwrap().motion.velocity = BVector::ZERO;
    let (_, unit_ai, _) = step(&mut world);
    assert_eq!(unit_ai.orders.len(), 0);
    // a patrolling group: the patrol branch, its secondary timer run out, restarts it at 750
    // and runs processIdle (the ship's caps its last human order time)
    let patrolling = Some((
        GroupCommandKind::FOLLOW,
        BVector::new(0.5, 0.0, 0.0),
        GroupFlags::PATROL,
    ));
    let mut world = grouped_ship_world(idle(UnitAiKind::Transport), patrolling, (false, &[]));
    let (answer, unit_ai, _) = step(&mut world);
    assert_eq!(answer, Ok(None));
    assert_eq!(unit_ai.secondary_timer, Millis::new(750));
    assert_eq!(lone_human_order(&world, player_id(1)), HUMAN_ORDER_CAP);
    // a target of a unit in a patrolling group: FUN_140c41690 answers 0 for a human player
    let idle_patrol = Some((GroupCommandKind::MOVE, BVector::ZERO, GroupFlags::PATROL));
    let targeting = UnitAi {
        current_target: Some(object_id(5)),
        ..idle(UnitAiKind::Transport)
    };
    for (kind, answer) in [
        (PlayerKind::Human, Ok(())),
        (
            PlayerKind::Computer,
            Err(GenieError::unsupported(
                "the patrol range of a unit AI's target",
            )),
        ),
    ] {
        let target = [walker_unit(5, player_id(2), Point::new(32.0, 31.0))];
        let mut world = grouped_ship_world(targeting.clone(), idle_patrol, (false, &target));
        world.players[1].kind = kind;
        let blocked = run_ai(
            &mut world,
            (player_id(1), object_id(50)),
            |ai, at, side, player_kind, rng| ai.target_block(at, player_kind, rng, side),
        );
        assert_eq!(blocked, answer, "{kind:?}");
    }
    // action 0x2c0 ends unless the group moves
    for (group, after) in [
        (None, None),
        (still, None),
        (moving, Some(AiAction::GroupAside)),
    ] {
        let aside = UnitAi {
            current_action: Some(AiAction::GroupAside),
            last_action: Some(AiAction::Heal),
            ..idle(UnitAiKind::Transport)
        };
        let mut world = grouped_ship_world(aside, group, (false, &[]));
        let (_, unit_ai, _) = step(&mut world);
        assert_eq!(unit_ai.current_action, after);
        let last = Some(if after.is_none() {
            AiAction::GroupAside
        } else {
            AiAction::Heal
        });
        assert_eq!(unit_ai.last_action, last);
    }
}

/// Player 1's transport ship 50 at (30, 30) (its unit AI `unit_ai`) and the `others`
/// ([`lone_world`]), its last human order past the cap; in player 1's group 7 with `group`'s
/// command, its motion's velocity and its flags ([`join_group`]), none for no group; with
/// `member_moving` its own mover moves.
fn grouped_ship_world(
    unit_ai: UnitAi,
    group: Option<(GroupCommandKind, BVector, GroupFlags)>,
    (member_moving, others): (bool, &[SceneObject]),
) -> World {
    let ship = lone_unit(player_id(1), unit_ai, Position::new(30.0, 30.0, 0.0));
    let mut world = lone_world(ship, others);
    lone_past_cap(&mut world, player_id(1));
    if let Some(group) = group {
        join_group(&mut world, group);
    }
    if member_moving {
        world.parts.mover_mut(object_id(50)).unwrap().velocity = Some(Velocity {
            velocity: BVector::new(0.5, 0.0, 0.0),
            acceleration: BVector::ZERO,
        });
    }
    world
}

/// Player 1's lone unit 50 of `world` joins player 1's group 7, the group's command, its
/// motion's velocity and its flags `group`'s.
fn join_group(
    world: &mut World,
    (command, velocity, flags): (GroupCommandKind, BVector, GroupFlags),
) {
    world.players[1]
        .objects
        .by_id_mut(object_id(50))
        .unwrap()
        .group
        .id = Some(group_id(7));
    world.groups.groups.push(UnitGroup {
        id: group_id(7),
        owner: Some(player_id(1)),
        members: vec![Member {
            id: object_id(50),
            ..Member::default()
        }],
        current_command: command,
        flags,
        motion: Motion {
            velocity,
            ..Motion::default()
        },
        ..UnitGroup::default()
    });
}

/// The transport ship's processIdle (`0x140c21d20`): `FUN_140c41e50`, then 5. An idle ship
/// whose idle timer passed its timeout caps its last human order time and does nothing more.
/// The building's (`FUN_140c1e600`) and the livestock's (`FUN_140c1fea0`) return 5 alone.
#[test]
fn an_idle_transport_ship_check_caps_its_human_order_time_and_does_nothing() {
    let ship = UnitAi {
        idle_timer: Millis::new(3001),
        adjusted_idle_timeout: Millis::new(3000),
        ..idle(UnitAiKind::Transport)
    };
    let mut world = grouped_ship_world(ship, None, (false, &[]));
    let (answer, unit_ai, _) = lone_step(&mut world, Millis::new(28), WorldTime::ZERO);
    assert_eq!(
        (answer, lone_walks(&world), unit_ai.idle_timer),
        (Ok(None), None, Millis::ZERO)
    );
    assert_eq!(lone_human_order(&world, player_id(1)), HUMAN_ORDER_CAP);
    for ai_kind in [UnitAiKind::Building, UnitAiKind::Livestock] {
        let mut world = lone_unit_world(idle(ai_kind));
        lone_past_cap(&mut world, player_id(1));
        assert_eq!(
            lone_process_idle(&mut world, player_id(1)),
            (Ok(IdleOutcome::Idle), 0),
            "{ai_kind}"
        );
        assert_eq!(
            lone_human_order(&world, player_id(1)),
            WorldTime::from_millis(9000)
        );
    }
}

/// `owner`'s lone unit 50 of `world` runs its idle check ([`UnitAi::process_idle`]): its answer
/// and the AI stream's draws.
fn lone_process_idle(world: &mut World, owner: PlayerId) -> (GenieResult<IdleOutcome>, u32) {
    let before = world.streams.get_mut(Stream::Ai).draws;
    let answer = run_ai(world, (owner, object_id(50)), |ai, at, side, kind, rng| {
        ai.process_idle(at, kind, rng, side)
    });
    (answer, world.streams.get_mut(Stream::Ai).draws - before)
}

/// The target block's attack whose target died, for a villager: the civilian's processIdle
/// (`FUN_140c1e610`: the cap, then 5) produces nothing, so the AI moves to the dead target's
/// point within its weapon range (0.25) and the order ends.
#[test]
fn villager_whose_attack_target_died_walks_to_its_point() {
    let attacking = UnitAi {
        current_order: Some(OrderKind::Attack),
        order_priority: OrderPriority::of_data(100),
        current_action: Some(AiAction::Attack),
        current_target: Some(object_id(60)),
        target_point: Position::new(5.0, 6.0, 0.0),
        ..idle(UnitAiKind::Civilian)
    };
    let villager = lone_unit(player_id(1), attacking, Position::new(30.0, 30.0, 0.0));
    let master = Master {
        combat: Some(CombatPart {
            weapon_range: Distance::new(0.25),
            ..CombatPart::default()
        }),
        ..villager.master
    };
    let mut world = lone_world(SceneObject { master, ..villager }, &[]);
    lone_past_cap(&mut world, player_id(1));
    let answer = run_ai(
        &mut world,
        (player_id(1), object_id(50)),
        |ai, at, side, kind, rng| ai.next_after_kill(at, kind, rng, side),
    );
    assert_eq!(answer, Ok(()));
    assert_eq!(
        lone_walks(&world),
        Some((Position::new(5.0, 6.0, 0.0), Distance::new(0.25)))
    );
    assert_eq!(lone_human_order(&world, player_id(1)), HUMAN_ORDER_CAP);
    let unit_ai = world.parts.unit_ai.get(object_id(50)).unwrap();
    assert_eq!(
        (
            unit_ai.current_order,
            unit_ai.order_priority.get(),
            unit_ai.last_order
        ),
        (None, -1, Some(OrderKind::Attack))
    );
}

/// A wolf whose attack target died: its processIdle (`FUN_140c1fb70`) finds another villager
/// within its reach and calls `AttackObject` unforced, which fails while the attack action
/// value is still current; the 6 it gives all the same keeps it from walking to the dead
/// one's point.
#[test]
fn wolf_whose_attack_target_died_attacks_the_next_villager() {
    let unit_ai = UnitAi {
        current_action: Some(AiAction::Attack),
        current_target: Some(object_id(50)),
        target_point: Position::new(5.0, 6.0, 0.0),
        ..idle(UnitAiKind::Lion)
    };
    // villager 700 a gap of 1 off
    let mut world = wolf_world(unit_ai, &[(700, class::VILLAGER, 21.4)]);
    run_ai(
        &mut world,
        (PlayerId::GAIA, object_id(20)),
        |ai, at, side, kind, rng| ai.next_after_kill(at, kind, rng, side),
    )
    .unwrap();
    let wolf = world.players[0].objects.by_id(object_id(20)).unwrap();
    assert_eq!(wolf.last_human_order, HUMAN_ORDER_CAP);
    assert!(
        world
            .parts
            .actions
            .of(object_id(20))
            .list
            .attack()
            .is_none()
    );
    let unit_ai = world.parts.unit_ai.get(object_id(20)).unwrap();
    assert_eq!(
        (unit_ai.current_action, unit_ai.current_target),
        (Some(AiAction::Attack), Some(object_id(50)))
    );
}

/// The tower in `NoAttack` (stance 3, `FUN_140c564c0`): after the cap only the search (slot
/// 0x50 with mode 1, its cared classes), whose alert mode it sets; then 5. Player 1's villager
/// 4 tiles off is found and not attacked.
#[test]
fn no_attack_tower_only_searches() {
    let mut world = tower_world(
        idle_tower(UnitStance::NoAttack),
        &[tower_villager(9731, 4.0)],
    );
    let (answer, unit_ai, _) = update_of(
        &mut world,
        (player_id(2), 500),
        Millis::new(28),
        WorldTime::ZERO,
    );
    assert_eq!(answer, Ok(None));
    assert_eq!(found_ids(&world), vec![object_id(9731)]);
    assert_eq!(attacks_500(&world), None);
    assert_eq!(unit_ai.alert_mode, AlertMode::Alert);
    assert_eq!(tower_human_order(&world), HUMAN_ORDER_CAP);
}

/// The tower's processIdle in the idle part (`FUN_140c564c0` with 1): what
/// `bestUnitToAttack` found it attacks (slot 0xb8 forced: player 1's villager 9731 in its
/// range, its Attack at once; the tower is armed, its missile 363, and a moving object of
/// speed 0, as a building's master is); the idle part reads no result.
#[test]
fn an_idle_tower_attacks_what_its_search_found() {
    let mut world = tower_world(
        idle_tower(UnitStance::Aggressive),
        &[tower_villager(9731, 4.0)],
    );
    let (answer, unit_ai, _) = update_of(
        &mut world,
        (player_id(2), 500),
        Millis::new(28),
        WorldTime::ZERO,
    );
    assert_eq!(answer.map(|_| ()), Ok(()));
    assert_eq!(attacks_500(&world), Some(object_id(9731)));
    assert_eq!(
        (unit_ai.current_action, unit_ai.current_target),
        (Some(AiAction::Attack), Some(object_id(9731)))
    );
}

/// The hunted animal's processIdle for a king (class 59, `FUN_140c1eea0`): after the cap a
/// human player's king does nothing (5) and draws nothing; only a computer player's (not of
/// master 704, not inside another, without an order) has its unit's slot 0x7e8, which is not
/// modelled.
#[test]
fn human_player_king_idle_check_only_caps() {
    let king = UnitAi {
        hunted: None,
        ..deer()
    };
    let mut world = king_world(king.clone(), ids::KING, PlayerKind::Human);
    assert_eq!(
        lone_process_idle(&mut world, player_id(1)),
        (Ok(IdleOutcome::Idle), 0)
    );
    assert_eq!(lone_human_order(&world, player_id(1)), HUMAN_ORDER_CAP);
    let mut world = king_world(king.clone(), ids::KING, PlayerKind::Computer);
    assert_eq!(
        lone_process_idle(&mut world, player_id(1)).0,
        Err(GenieError::unsupported(
            "a computer player's king's idle check (FUN_140c16930, slot 0x7e8)"
        ))
    );
    // a computer player's king of master 704, or one with an order, does nothing either
    let mut world = king_world(king.clone(), ids::SHAH, PlayerKind::Computer);
    assert_eq!(
        lone_process_idle(&mut world, player_id(1)).0,
        Ok(IdleOutcome::Idle)
    );
    let ordered_king = UnitAi {
        current_order: Some(OrderKind::Move),
        ..king
    };
    let mut world = king_world(ordered_king, ids::KING, PlayerKind::Computer);
    assert_eq!(
        lone_process_idle(&mut world, player_id(1)).0,
        Ok(IdleOutcome::Idle)
    );
}

/// Player 1's king 50 (its unit AI `unit_ai`, class 59, master `master`) at (31.5, 106.5),
/// player 1 of type `kind`, the king's last human order past the cap.
fn king_world(unit_ai: UnitAi, master: MasterId, kind: PlayerKind) -> World {
    let king = lone_unit(player_id(1), unit_ai, Position::new(31.5, 106.5, 0.0));
    let master = Master {
        id: master,
        class: class::KING,
        ..king.master
    };
    let mut world = lone_world(SceneObject { master, ..king }, &[]);
    world.players[1].kind = kind;
    lone_past_cap(&mut world, player_id(1));
    world
}

/// Player 1's villager 10041 at (20, 20) on its sprite, of attack priority `priority`, that
/// hunts predators; Gaia's boar 9731 at (20.9, 20), player 2's soldier 9800 at (21, 21), Gaia's
/// relic 9801 at (19, 19) and player 2's house 9802 at (24, 20), each with its unit AI and in
/// player 1's lists.
fn civilian_world(priority: u8) -> World {
    let villager = Master {
        standing_sprite: Some(STANDING.id),
        creatable: Some(CreatablePart {
            attack_priority: AttackPriority::of_byte(priority),
            ..CreatablePart::default()
        }),
        action: Some(ActionPart {
            tasks: vec![Task {
                action_kind: HUNT,
                object_group: Some(class::PREDATOR),
                attribute_kind: Some(attribute::FOOD),
                work_range: Distance::new(0.5),
                ..Task::default()
            }],
            ..ActionPart::default()
        }),
        ..villager_master()
    };
    let (house, _) = house_and_tree();
    let seen = |ai, object: SceneObject| SceneObject {
        ai: Some(ai),
        seen_by: Some(VisionBits::visible(player_id(1))),
        ..object
    };
    Scene::open(GridSize::new(64, 64))
        .players(2)
        .graphic(STANDING)
        .object(SceneObject {
            ai: Some(idle(UnitAiKind::Civilian)),
            sprite: Some(STANDING.id),
            ..SceneObject::new(
                10041,
                player_id(1),
                villager,
                Position::new(20.0, 20.0, 0.0),
            )
        })
        .object(seen(
            hit_boar(),
            SceneObject::new(
                9731,
                PlayerId::GAIA,
                walker(48, class::PREDATOR),
                Position::new(20.9, 20.0, 0.0),
            ),
        ))
        .object(seen(
            idle(UnitAiKind::Soldier),
            SceneObject::new(
                9800,
                player_id(2),
                walker(5, class::CLASS_6),
                Position::new(21.0, 21.0, 0.0),
            ),
        ))
        .object(seen(
            idle(UnitAiKind::Artifact),
            SceneObject::new(
                9801,
                PlayerId::GAIA,
                walker(285, class::RELIC),
                Position::new(19.0, 19.0, 0.0),
            ),
        ))
        .object(seen(
            idle(UnitAiKind::Building),
            SceneObject::new(9802, player_id(2), house, Position::new(24.0, 20.0, 0.0)),
        ))
        .build()
}

/// The civilian's `AttackObject` (slot 0xb8, `FUN_140c17370`, forced): an object the world
/// does not hold gets nothing; an animal or a tree (classes 9, 10, 15) is hunted (slot 0xc8:
/// the villager stops, then hunts at once); any other goes to the base `AttackObject`, whose
/// civilian slot 0xa0 (`FUN_140c18b70`) refuses classes 1 and 42, and at attack priority 3 the
/// classes outside [`PRIORITY_3_CLASSES`]. Each attack is the villager's Attack at once.
#[test]
fn villager_hunts_an_animal_and_attacks_another_object() {
    let attack = |priority, target| {
        let mut world = civilian_world(priority);
        let run_ai = run_ai(
            &mut world,
            (player_id(1), object_id(10041)),
            |ai, at, side, _, rng| ai.civilian_attack(object_id(target), at, side, rng),
        );
        let action = world.parts.actions.of(object_id(10041));
        let attacks = action
            .list
            .attack()
            .and_then(|attack| attack.target)
            .map(AttackTargetId::id);
        let unit_ai = world.parts.unit_ai.get(object_id(10041)).unwrap();
        (
            run_ai,
            action.top_action_kind(),
            attacks,
            unit_ai.current_action,
            unit_ai.current_target,
        )
    };
    let (answer, top, ..) = attack(0, 9999);
    assert_eq!((answer, top), (Ok(Answer::NotTaken), None));
    assert_eq!(
        attack(0, 9731),
        (
            Ok(Answer::Taken),
            Some(HUNT),
            None,
            Some(AiAction::Hunt),
            Some(object_id(9731))
        )
    );
    assert_eq!(
        attack(0, 9800),
        (
            Ok(Answer::Taken),
            Some(ATTACK),
            Some(object_id(9800)),
            Some(AiAction::Attack),
            Some(object_id(9800))
        )
    );
    assert_eq!(attack(0, 9801).0, Ok(Answer::NotTaken));
    assert_eq!(attack(3, 9800).0, Ok(Answer::NotTaken));
    let (answer, _, attacks, ..) = attack(3, 9802);
    assert_eq!(
        (answer, attacks),
        (Ok(Answer::Taken), Some(object_id(9802)))
    );
}

/// Slot 0xa0 of the types whose `AttackObject` is the base's: the transport ship's and the
/// livestock's (`FUN_140c48400`) pass anything (unit 9's Attack at once on player 2's 9800),
/// the building's (`FUN_140c18b60`) nothing.
#[test]
fn ship_or_livestock_may_attack_anything_and_building_nothing() {
    for (ai_kind, attacks) in [
        (UnitAiKind::Transport, Some(object_id(9800))),
        (UnitAiKind::Livestock, Some(object_id(9800))),
        (UnitAiKind::Building, None),
    ] {
        let enemy = order_target(9800, player_id(2), class::CLASS_6);
        let mut world = order_world(order_unit(order_soldier(), idle(ai_kind)), &[enemy]);
        let run_ai = run_ai(
            &mut world,
            (player_id(1), object_id(9)),
            |ai, at, side, _, rng| {
                ai.attack_object(
                    object_id(9800),
                    OrderForce::Forced,
                    TargetRules::Own,
                    at,
                    (side, rng),
                )
            },
        );
        let answer = if attacks.is_some() {
            Answer::Taken
        } else {
            Answer::NotTaken
        };
        assert_eq!(
            (run_ai, attacks_9(&world)),
            (Ok(answer), attacks),
            "type {ai_kind}"
        );
    }
}

/// Unit `id` of `player` in `world` handles `notify` through its type's handler (slot 0x238,
/// [`UnitAi::handle`]).
fn hears(world: &mut World, (player, id): (PlayerId, i32), notify: Notify) -> GenieResult<u32> {
    run_ai(world, (player, object_id(id)), |ai, at, side, kind, rng| {
        ai.handle(notify, Millis::ZERO, at, kind, rng, side)
    })
}

/// The boar's handler (`FUN_140c25770`) for the completion of its move off a footprint
/// (0x1fa on action 0x2bf): its idle timeout becomes the adjusted one plus 1, the target goes
/// (`FUN_140c5e1e0`), its slot 0x1f0 (`FUN_140c16d50`) sends it home on its next idle check,
/// then the base handler stops the completed action.
#[test]
fn boar_whose_move_aside_completed_heads_home_next() {
    let unit_ai = UnitAi {
        current_action: Some(AiAction::MoveAside),
        current_target: Some(object_id(50)),
        target_class: class_of(4),
        adjusted_idle_timeout: Millis::new(3100),
        idle_timeout: Millis::new(3000),
        ..hit_boar()
    };
    let notify = Notify::own(
        object_id(9729),
        NotifyKind::ACTION_COMPLETED,
        [AiAction::MoveAside.value(), 0, 0],
    );
    let mut world = boar_world(9729, (CombatLevel::ANIMAL, unit_ai), &[]);
    assert_eq!(hears(&mut world, (PlayerId::GAIA, 9729), notify), Ok(GO_ON));
    let unit_ai = world.parts.unit_ai.get(object_id(9729)).unwrap();
    assert_eq!(unit_ai.idle_timeout, Millis::new(3101));
    assert_eq!(
        (unit_ai.current_target, unit_ai.last_target),
        (None, Some(object_id(50)))
    );
    assert_eq!(unit_ai.current_action, None);
    assert!(unit_ai.home.is_some_and(|home| home.homeward));
}

/// The lion's handler (`FUN_140c266f0`): a completed attack (0x1fa with 600) sets its idle
/// timeout to 1000 and redraws the adjusted one (`FUN_140c41bc0`, one draw of the AI stream),
/// drops the target and stops (slot 0xb0 with 1); the base handler then stops on the attack's
/// completion again.
#[test]
fn lion_completed_attack_resets_its_idle_timeout_and_stops() {
    let unit_ai = UnitAi {
        current_action: Some(AiAction::Attack),
        current_target: Some(object_id(700)),
        idle_timeout: Millis::new(6000),
        ..idle(UnitAiKind::Lion)
    };
    let notify = Notify::own(
        object_id(800),
        NotifyKind::ACTION_COMPLETED,
        [AiAction::Attack.value(), 0, 0],
    );
    let expected = adjusted_idle_timeout(Millis::new(1000), &mut rng());
    let mut world = gaia_unit_world(800, unit_ai, Position::new(10.0, 10.0, 0.0));
    let before = world.streams.get_mut(Stream::Ai).draws;
    assert_eq!(hears(&mut world, (PlayerId::GAIA, 800), notify), Ok(GO_ON));
    assert_eq!(world.streams.get_mut(Stream::Ai).draws - before, 1);
    let unit_ai = world.parts.unit_ai.get(object_id(800)).unwrap();
    assert_eq!(
        (unit_ai.idle_timeout, unit_ai.adjusted_idle_timeout),
        (Millis::new(1000), expected)
    );
    assert_eq!(
        (
            unit_ai.current_target,
            unit_ai.last_target,
            unit_ai.current_action
        ),
        (None, Some(object_id(700)), None)
    );
}

/// Gaia's lion 800 at (10, 10) (radius 0.5, line of sight 4, on its sprite), its unit AI `ai`,
/// and player 1's villager 700 (radius 0.5, its master's flags `flags`) at (x, 10), none
/// without x, in Gaia's lists.
fn lion_world(ai: UnitAi, villager: Option<(f32, ObjectFlags)>) -> World {
    let lion = Master {
        radius: Footprint::new(0.5, 0.5),
        line_of_sight: Distance::new(4.0),
        ..walker(126, class::PREDATOR)
    };
    let prey = villager.map(|(x, flags)| SceneObject {
        ai: Some(idle(UnitAiKind::Civilian)),
        ..SceneObject::new(
            700,
            player_id(1),
            Master {
                radius: Footprint::new(0.5, 0.5),
                object_flags: flags,
                ..walker(83, class::VILLAGER)
            },
            Position::new(x, 10.0, 0.0),
        )
    });
    let mut scene = Scene::open(GridSize::new(64, 64))
        .graphic(STANDING)
        .object(SceneObject {
            ai: Some(ai),
            sprite: Some(STANDING.id),
            ..SceneObject::new(
                800,
                PlayerId::GAIA,
                Master {
                    standing_sprite: Some(STANDING.id),
                    ..lion
                },
                Position::new(10.0, 10.0, 0.0),
            )
        });
    if let Some(scene_object) = prey {
        scene = scene.object(SceneObject {
            seen_by: Some(VisionBits::visible(PlayerId::GAIA)),
            ..scene_object
        });
    }
    let mut world = scene.build();
    world.streams.get_mut(Stream::Ai).set_seed(0);
    world
}

/// The lion's handler for a moved target (0x1fc): a target the world no longer holds, or
/// one beyond its line of sight (the footprints' squared gap against its sight squared,
/// `COMISS` then `JC`), or one whose master has flag 0x10 (`+0x654`), is dropped and the lion
/// stops before the base handler; one within its sight it attacks again (slot 0xb8 with the
/// target, forced: its Attack at once) and the processing breaks (3).
#[test]
fn lion_drops_moved_target_out_of_its_sight() {
    let lion = || UnitAi {
        current_action: Some(AiAction::Attack),
        current_target: Some(object_id(700)),
        ..idle(UnitAiKind::Lion)
    };
    let notify = Notify::own(object_id(800), NotifyKind::TARGET_MOVED, [0, 0, 0]);
    let run = |villager| {
        let mut world = lion_world(lion(), villager);
        let hears = hears(&mut world, (PlayerId::GAIA, 800), notify);
        let unit_ai = world.parts.unit_ai.get(object_id(800)).unwrap();
        (
            hears,
            gaia_attacks(&world, 800),
            unit_ai.current_target,
            unit_ai.current_action,
        )
    };
    assert_eq!(run(None), (Ok(GO_ON), None, None, None));
    // 5 tiles apart, 4 between the footprints: at its sight, attacked again
    assert_eq!(
        run(Some((15.0, ObjectFlags::NONE))),
        (
            Ok(BREAK),
            Some(object_id(700)),
            Some(object_id(700)),
            Some(AiAction::Attack)
        )
    );
    // a master with flag 0x10 is dropped at its sight too
    assert_eq!(
        run(Some((15.0, ObjectFlags::SCOUT))),
        (Ok(GO_ON), None, None, None)
    );
    // 6 apart, 5 between: beyond it
    assert_eq!(
        run(Some((16.0, ObjectFlags::NONE))),
        (Ok(GO_ON), None, None, None)
    );
}

/// Player 1's house 10 at (30, 30) (its owner of type `kind`), its unit AI `ai`, and player 2's
/// walkers 60, 61, 62 and 70 (class 6) around it, each in player 1's lists.
fn building_world(ai: UnitAi, kind: PlayerKind) -> World {
    let (house, _) = house_and_tree();
    let mut scene = Scene::open(GridSize::new(64, 64))
        .players(2)
        .kind(player_id(1), kind)
        .object(SceneObject {
            ai: Some(ai),
            ..SceneObject::new(10, player_id(1), house, Position::new(30.0, 30.0, 0.0))
        });
    for (id, x) in [(60, 33.0), (61, 34.0), (62, 35.0), (70, 36.0)] {
        scene = scene.object(SceneObject {
            seen_by: Some(VisionBits::visible(player_id(1))),
            ai: Some(idle(UnitAiKind::Soldier)),
            ..SceneObject::new(
                id,
                player_id(2),
                walker(5, class::CLASS_6),
                Position::new(x, 30.0, 0.0),
            )
        });
    }
    scene.build()
}

/// The building's handler (`FUN_140c22610`): an attack (600) adds its attacker and puts it
/// under attack; 699 drops one; damage (500) at no hit points ends the notifies (4), else
/// adds an attacker already known or its target, and a new one after it asks for help
/// (`AskForHelp`, `FUN_140c42ce0`); a computer player's AI hears of it (not modelled).
#[test]
fn building_counts_its_attackers() {
    let mut game = building_world(idle(UnitAiKind::Building), PlayerKind::Human);
    let house = (player_id(1), 10);
    let under_attack = |world: &World| {
        let house = world.players[1].objects.by_id(object_id(10)).unwrap();
        house.flags.contains(UnitFlags::UNDER_ATTACK)
    };
    let attackers = |world: &World| {
        world
            .parts
            .unit_ai
            .get(object_id(10))
            .unwrap()
            .attackers
            .clone()
    };
    let attacked = |caller| Notify::own(object_id(caller), NotifyKind::ATTACKED, [0; 3]);
    assert_eq!(hears(&mut game, house, attacked(70)), Ok(GO_ON));
    assert!(under_attack(&game));
    let _ = hears(&mut game, house, attacked(60));
    assert_eq!(attackers(&game), Attackers::of_saved(&[60, 70]));
    let released = |caller| Notify::own(object_id(caller), NotifyKind::NO_LONGER_ON_ME, [0; 3]);
    assert_eq!(hears(&mut game, house, released(70)), Ok(GO_ON));
    assert_eq!(attackers(&game), Attackers::of_saved(&[60]));
    assert_eq!(hears(&mut game, house, released(99)), Ok(GO_ON));
    assert_eq!(attackers(&game), Attackers::of_saved(&[60]));
    let hit =
        |caller: i32, hp| Notify::own(object_id(caller), NotifyKind::DAMAGED, [caller, hp, 0]);
    game.players[1]
        .objects
        .by_id_mut(object_id(10))
        .unwrap()
        .flags
        .remove(UnitFlags::UNDER_ATTACK);
    assert_eq!(hears(&mut game, house, hit(60, 5)), Ok(GO_ON));
    assert!(under_attack(&game));
    game.parts
        .unit_ai
        .get_mut(object_id(10))
        .unwrap()
        .current_target = Some(object_id(61));
    assert_eq!(hears(&mut game, house, hit(61, 5)), Ok(GO_ON));
    assert_eq!(attackers(&game), Attackers::of_saved(&[60, 61]));
    assert_eq!(hears(&mut game, house, hit(62, 5)), Ok(GO_ON));
    assert_eq!(attackers(&game), Attackers::of_saved(&[60, 61, 62]));
    let mut computer = building_world(
        UnitAi {
            attackers: Attackers::of_saved(&[60]),
            ..idle(UnitAiKind::Building)
        },
        PlayerKind::Computer,
    );
    assert!(hears(&mut computer, house, hit(60, 5)).is_err());
    game.parts
        .unit_ai
        .get_mut(object_id(10))
        .unwrap()
        .post(hit(60, 0), ObjectState::Alive)
        .unwrap();
    assert_eq!(hears(&mut game, house, hit(60, 0)), Ok(DEAD));
    assert_eq!(
        game.parts
            .unit_ai
            .get(object_id(10))
            .unwrap()
            .notifies
            .len(),
        0
    );
}

/// The tower's handler (`FUN_140c299f0`) for damage at no hit points: the owner's alert
/// (0x209, a human player's alert list), the notifies end and it gives up (4).
#[test]
fn destroyed_tower_gives_up() {
    let notify = Notify::own(object_id(60), NotifyKind::DAMAGED, [60, 0, 0]);
    let tower = UnitAi {
        notifies: vec![notify],
        ..idle(UnitAiKind::Tower)
    };
    let mut world = building_world(tower, PlayerKind::Human);
    assert_eq!(hears(&mut world, (player_id(1), 10), notify), Ok(DEAD));
    assert_eq!(
        world
            .parts
            .unit_ai
            .get(object_id(10))
            .unwrap()
            .notifies
            .len(),
        0
    );
    let mut world = building_world(idle(UnitAiKind::Tower), PlayerKind::Computer);
    assert!(hears(&mut world, (player_id(1), 10), notify).is_err());
}

/// A deer evading an object on its own point steps off by one tile down and left, the object
/// Gaia's wolf (no enemy); near the map's edge too.
#[test]
fn deer_evading_an_object_on_its_own_point_steps_off_by_one() {
    for (at, off) in [
        (
            Position::new(31.5, 106.5, 0.0),
            Position::new(30.5, 105.5, 0.0),
        ),
        (Position::new(3.5, 4.5, 0.0), Position::new(4.5, 5.5, 0.0)),
    ] {
        let wolf = [SceneObject::new(
            9183,
            PlayerId::GAIA,
            walker(126, class::PREDATOR),
            at,
        )];
        let mut world = animal_world(animal(deer(), at), &wolf, wander_seed(false));
        let _ = animal_step(&mut world, Millis::new(10));
        assert_eq!(
            lone_walks(&world).map(|(point, _)| point),
            Some(off),
            "{at:?}"
        );
    }
}

/// A walker of radius 0.2 and speed 1 on land: master `id` of class `class`.
fn walker(id: i16, class: ClassId) -> Master {
    Master {
        id: master_id(id),
        kind: MasterKind::COMBAT,
        class,
        radius: Footprint::new(0.2, 0.2),
        height: Distance::new(1.0),
        hit_points: MasterHitPoints::of_data(40),
        terrain_restriction: Some(RestrictionId::named(7)),
        move_stats: Some(walking(Speed::new(1.0))),
        ..Master::default()
    }
}

/// Unit 500 of `owner` at `at`, a walker of class `class` (master 4), its unit AI `ai`.
fn runner_unit(owner: PlayerId, class: ClassId, ai: UnitAi, at: Position) -> SceneObject {
    SceneObject {
        ai: Some(ai),
        ..SceneObject::new(500, owner, walker(4, class), at)
    }
}

/// A map of 64 by 64 tiles of land with a rock on each of `rocks`, the `runner` on it and
/// its owner of type `kind`; player 2's attackers (id, class, x) stand at (x, 30), each a
/// walker (of the master of its id) whose unit AI targets unit 500. The AI stream is seeded
/// with 0, as [`rng`] is.
fn chased(
    runner: SceneObject,
    kind: PlayerKind,
    rocks: &[Tile],
    attackers: &[(i32, ClassId, f32)],
) -> World {
    let mut scene = Scene::open(GridSize::new(64, 64))
        .players(2)
        .kind(runner.player, kind)
        .rocks(rocks.iter().copied())
        .object(runner);
    for &(id, class, x) in attackers {
        scene = scene.object(SceneObject {
            ai: Some(UnitAi {
                current_target: Some(object_id(500)),
                ..UnitAi::default()
            }),
            ..SceneObject::new(
                id,
                player_id(2),
                walker(i16::try_from(id).expect("a master's id"), class),
                Position::new(x, 30.0, 0.0),
            )
        });
    }
    let mut world = scene.build();
    world.streams.get_mut(Stream::Ai).set_seed(0);
    world
}

/// The point the `MoveTo` of unit 500 of `world` walks to, none without one.
fn walks_to(world: &World) -> Option<Position> {
    world
        .parts
        .actions
        .of(object_id(500))
        .list
        .move_to()
        .map(|move_to_action| move_to_action.point)
}

/// `evasiveMoveTo` (`FUN_140c4d140`): without `uncon` a unit with an action stays; with
/// `tries` a point the unit cannot stand at (a rock on its tile) moves by four draws (x's
/// sign, its amount, y's sign, its amount: `rand15 & 1`, `rand15 & 3`), held on the map, up to
/// five times; then the forced running move within 0 (the unit's `MoveTo` at once), the move
/// action value and the point.
#[test]
fn an_evasive_move_steps_off_point_the_unit_cannot_stand_at() {
    let evade = |ai: UnitAi, point: Position, flags: (OrderForce, BadPoint), rocks: &[Tile]| {
        let at = Position::new(10.0, 20.0, 0.0);
        let mut chased = chased(
            runner_unit(player_id(1), class::CLASS_6, ai, at),
            PlayerKind::Human,
            rocks,
            &[],
        );
        let run_ai = run_ai(
            &mut chased,
            (player_id(1), object_id(500)),
            |ai, at, side, _, rng| ai.evasive_move(point, flags, at, rng, side),
        );
        (run_ai, chased)
    };
    let busy = UnitAi {
        current_action: Some(AiAction::Attack),
        ..idle(UnitAiKind::Hunted)
    };
    let unforced = (OrderForce::Unforced, BadPoint::Moves);
    let (answer, world) = evade(busy, Position::new(12.0, 22.0, 0.0), unforced, &[]);
    assert_eq!((answer, walks_to(&world)), (Ok(Answer::NotTaken), None));
    let mut expected = rng();
    let step = |rng: &mut Well512, coordinate: f32| {
        let up = rng.rand15() & 1 != 0;
        let by = (rng.rand15() & 3) as f32;
        if up { coordinate + by } else { coordinate - by }
    };
    let ex = step(&mut expected, 12.0);
    let ey = step(&mut expected, 22.0);
    let (answer, mut world) = evade(
        idle(UnitAiKind::Hunted),
        Position::new(12.0, 22.0, 0.0),
        unforced,
        &[Tile::new(12, 22)],
    );
    assert_eq!(answer, Ok(Answer::Taken));
    assert_eq!(world.streams.get_mut(Stream::Ai).draws, 4);
    assert_eq!(walks_to(&world), Some(Position::new(ex, ey, 0.0)));
    let ai = world.parts.unit_ai.get(object_id(500)).unwrap();
    assert_eq!(
        (ai.current_action, ai.target_point),
        (Some(AiAction::Move), Position::new(ex, ey, 0.0))
    );
    // a point past the far edge is held a tile in, below 0 at 0
    let (_, world) = evade(
        idle(UnitAiKind::Hunted),
        Position::new(70.0, -3.0, 0.0),
        (OrderForce::Forced, BadPoint::Kept),
        &[],
    );
    assert_eq!(walks_to(&world), Some(Position::new(63.0, 0.0, 0.0)));
}

/// Slot 0x148 (`FUN_140c4d0d0`): unforced, a unit AI with an action value takes no move off
/// a footprint; forced, or without one, the unit's slot 0x340 gets the move
/// (`AiCommand::MoveAside`: its `MoveTo` to the point) and the action value becomes 0x2bf,
/// the old one the last.
#[test]
fn move_off_footprint_needs_force_or_no_action() {
    let aside = |force| AiMove {
        point: Position::new(4.0, 5.0, 0.0),
        range: Distance::ZERO,
        force,
        pace: Pace::Walk,
    };
    let gathering = UnitAi {
        current_action: Some(AiAction::Gather),
        current_target: Some(object_id(40)),
        ..idle(UnitAiKind::Civilian)
    };
    let move_aside = |unit_ai: UnitAi, force| {
        let mut world = lone_unit_world(unit_ai);
        let answer = run_ai(
            &mut world,
            (player_id(1), object_id(50)),
            |ai, _, side, _, rng| ai.move_aside(aside(force), side, rng),
        );
        let unit_ai = world.parts.unit_ai.get(object_id(50)).unwrap().clone();
        (answer, unit_ai, lone_walks(&world))
    };
    let to_point = Some((Position::new(4.0, 5.0, 0.0), Distance::ZERO));
    let (answer, unit_ai, walk) = move_aside(gathering.clone(), OrderForce::Unforced);
    assert_eq!(
        (answer, unit_ai.current_action, walk),
        (Ok(Answer::NotTaken), Some(AiAction::Gather), None)
    );
    let (answer, unit_ai, walk) = move_aside(gathering, OrderForce::Forced);
    assert_eq!((answer, walk), (Ok(Answer::Taken), to_point));
    assert_eq!(
        (
            unit_ai.last_action,
            unit_ai.current_action,
            unit_ai.current_target
        ),
        (
            Some(AiAction::Gather),
            Some(AiAction::MoveAside),
            Some(object_id(40))
        )
    );
    let (answer, _, walk) = move_aside(idle(UnitAiKind::Civilian), OrderForce::Unforced);
    assert_eq!((answer, walk), (Ok(Answer::Taken), to_point));
}

/// Slot 0x130 (`FUN_140c55620`): unforced, a unit AI with an action value takes no move;
/// forced, the action value becomes the move's (the old one the last) and `FUN_140c64c10`
/// with −1 moves the target to the last and takes the point, leaving the target class.
#[test]
fn an_ai_move_needs_force_or_no_action_and_keeps_the_target_class() {
    let ai_move = |force| AiMove {
        point: Position::new(4.0, 5.0, 0.5),
        range: Distance::new(0.5),
        force,
        pace: Pace::Walk,
    };
    let mut unit_ai = UnitAi {
        current_action: Some(AiAction::Attack),
        current_target: Some(object_id(40)),
        target_class: class_of(9),
        last_target: Some(object_id(30)),
        last_target_class: class_of(4),
        ..idle(UnitAiKind::Boar)
    };
    let before = unit_ai.clone();
    assert_eq!(unit_ai.move_to(ai_move(OrderForce::Unforced)), None);
    assert_eq!(unit_ai, before);
    assert_eq!(
        unit_ai.move_to(ai_move(OrderForce::Forced)),
        Some(ai_move(OrderForce::Forced))
    );
    assert_eq!(
        (unit_ai.last_action, unit_ai.current_action),
        (Some(AiAction::Attack), Some(AiAction::Move))
    );
    assert_eq!(
        (
            unit_ai.last_target,
            unit_ai.last_target_class,
            unit_ai.current_target,
            unit_ai.target_class
        ),
        (Some(object_id(40)), class_of(9), None, class_of(9))
    );
    assert_eq!(unit_ai.target_point, Position::new(4.0, 5.0, 0.5));
}

/// `FUN_140c64b60`: leaving a patrol order (711, 733) redraws an adjusted idle timeout of
/// exactly 750 and caps the secondary timer at 750 by a signed compare (a timer read as
/// negative stays); with its third argument 0 the last order stays.
#[test]
fn the_order_setter_caps_by_signed_compare_and_may_keep_the_last_order() {
    let mut unit_ai = UnitAi {
        current_order: Some(OrderKind::Patrol),
        last_order: Some(OrderKind::Wait),
        adjusted_idle_timeout: Millis::new(3000),
        secondary_timer: Millis::new(0x8000_0000),
        ..idle(UnitAiKind::Soldier)
    };
    unit_ai.set_order_with(Some(OrderKind::Attack), OrderHistory::Skipped, &mut rng());
    assert_eq!(
        (unit_ai.current_order, unit_ai.last_order),
        (Some(OrderKind::Attack), Some(OrderKind::Wait))
    );
    assert_eq!(unit_ai.secondary_timer, Millis::new(0x8000_0000));
    let mut unit_ai = UnitAi {
        current_order: Some(OrderKind::AttackMove),
        adjusted_idle_timeout: Millis::new(750),
        idle_timeout: Millis::new(3000),
        secondary_timer: Millis::new(900),
        ..idle(UnitAiKind::Soldier)
    };
    let mut rng = rng();
    unit_ai.set_order(None, &mut rng);
    assert_eq!(
        (unit_ai.current_order, unit_ai.last_order),
        (None, Some(OrderKind::AttackMove))
    );
    assert_eq!(unit_ai.secondary_timer, Millis::new(750));
    assert_eq!(rng.draws, 1);
}

/// The civilian's slot 73 (`FUN_140c21da0`): on the attack order without a target it stops
/// its action and ends the order (slot 0xb0 with 0: the state position takes its place);
/// with a target, or on another order, nothing.
#[test]
fn villager_on_the_attack_order_without_target_gives_it_up() {
    let misc = |unit_ai: UnitAi| {
        let villager = lone_unit(player_id(1), unit_ai, Position::new(12.0, 13.0, 0.0));
        let mut world = lone_world(on_its_way(villager, Position::new(20.0, 13.0, 0.0)), &[]);
        let answer = run_ai(
            &mut world,
            (player_id(1), object_id(50)),
            |ai, at, side, _, rng| ai.civilian_misc(at, side, rng),
        );
        assert_eq!(answer, Ok(()));
        let unit_ai = world.parts.unit_ai.get(object_id(50)).unwrap().clone();
        (lone_stopped(&world), unit_ai)
    };
    let (stopped, unit_ai) = misc(UnitAi {
        current_order: Some(OrderKind::Attack),
        order_priority: OrderPriority::of_data(100),
        current_action: Some(AiAction::Attack),
        ..idle(UnitAiKind::Civilian)
    });
    assert!(stopped);
    assert_eq!(
        (
            unit_ai.current_order,
            unit_ai.order_priority.get(),
            unit_ai.last_order,
            unit_ai.current_action
        ),
        (None, -1, Some(OrderKind::Attack), None)
    );
    assert_eq!(unit_ai.state_position, Point::new(12.0, 13.0));
    for (order, target) in [(OrderKind::Attack, 40), (OrderKind::Build, -1)] {
        let (stopped, unit_ai) = misc(UnitAi {
            current_order: Some(order),
            current_target: ObjectId::of_field(target),
            ..idle(UnitAiKind::Civilian)
        });
        assert_eq!((stopped, unit_ai.current_order), (false, Some(order)));
    }
}

/// The factory (`FUN_141215a90`): masters 1105, 1744 and 1746 by their id; then by class,
/// the classes without a unit AI, the civilian, the building (or the tower with a volley
/// missile, `+0x8d4`, and for class 52), the hunted animal, the soldier as the default and
/// the rest of the switch; class 10's type its master's combat level picks
/// (`FUN_140fde4f0`: the boar's for 1, else the lion's).
#[test]
fn the_factory_picks_the_unit_ai_kind_by_master_and_class() {
    assert_eq!(
        factory_kind(
            class::CLASS_6,
            master_id(1105),
            None,
            CombatLevel::NONE,
            &roles()
        ),
        Some(UnitAiKind::SiegeTower)
    );
    assert_eq!(
        factory_kind(
            class::CLASS_6,
            master_id(1744),
            None,
            CombatLevel::NONE,
            &roles()
        ),
        Some(UnitAiKind::Siege)
    );
    assert_eq!(
        factory_kind(
            class::CLASS_6,
            master_id(1746),
            None,
            CombatLevel::NONE,
            &roles()
        ),
        Some(UnitAiKind::Siege)
    );
    let by_class = [
        (ClassId::of_master(1), UnitAiKind::Artifact),
        (ClassId::of_master(2), UnitAiKind::Trade),
        (ClassId::of_master(19), UnitAiKind::Trade),
        (class::VILLAGER, UnitAiKind::Civilian),
        (class::PREY, UnitAiKind::Hunted),
        (class::KING, UnitAiKind::Hunted),
        (ClassId::of_master(13), UnitAiKind::Siege),
        (ClassId::of_master(51), UnitAiKind::Siege),
        (ClassId::of_master(54), UnitAiKind::Siege),
        (ClassId::of_master(55), UnitAiKind::Siege),
        (class::MONK, UnitAiKind::Priest),
        (class::CLASS_43, UnitAiKind::Priest),
        (class::CLASS_53, UnitAiKind::Priest),
        (ClassId::of_master(20), UnitAiKind::Transport),
        (ClassId::of_master(21), UnitAiKind::Fishing),
        (ClassId::of_master(29), UnitAiKind::Livestock),
        (ClassId::of_master(58), UnitAiKind::Livestock),
        (ClassId::of_master(52), UnitAiKind::Tower),
        (ClassId::of_master(6), UnitAiKind::Soldier),
        (ClassId::of_master(0), UnitAiKind::Soldier),
        (ClassId::of_master(64), UnitAiKind::Soldier),
    ];
    for (class, ai_kind) in by_class {
        assert_eq!(
            factory_kind(class, master_id(83), None, CombatLevel::NONE, &roles()),
            Some(ai_kind),
            "class {class}"
        );
    }
    for class in [3, 27, 39, 49, 60].map(ClassId::of_master) {
        assert_eq!(
            factory_kind(class, master_id(70), None, CombatLevel::NONE, &roles()),
            Some(UnitAiKind::Building)
        );
        assert_eq!(
            factory_kind(
                class,
                master_id(70),
                Some(master_id(5)),
                CombatLevel::NONE,
                &roles()
            ),
            Some(UnitAiKind::Tower)
        );
    }
    for class in
        [5, 7, 8, 11, 14, 15, 16, 30, 31, 32, 33, 34, 38, 42, 48, 63].map(ClassId::of_master)
    {
        assert_eq!(
            factory_kind(class, master_id(83), None, CombatLevel::NONE, &roles()),
            None
        );
    }
    // class 10: the boar's for combat level 1, else the lion's
    assert_eq!(
        factory_kind(
            class::PREDATOR,
            master_id(48),
            None,
            CombatLevel::ANIMAL,
            &roles()
        ),
        Some(UnitAiKind::Boar)
    );
    assert_eq!(
        factory_kind(
            class::PREDATOR,
            master_id(126),
            None,
            CombatLevel::NONE,
            &roles()
        ),
        Some(UnitAiKind::Lion)
    );
    assert_eq!(
        factory_kind(
            class::PREDATOR,
            master_id(126),
            None,
            CombatLevel::MILITARY,
            &roles()
        ),
        Some(UnitAiKind::Lion)
    );
}

/// The building's cared classes (`FUN_140c11470`) are the soldier's in another order.
#[test]
fn the_building_cares_about_the_soldier_classes_in_its_own_order() {
    let mut building = BUILDING_CARED.to_vec();
    let mut soldier = SOLDIER_CARED.to_vec();
    assert_ne!(building, soldier);
    building.sort_unstable();
    soldier.sort_unstable();
    assert_eq!(building, soldier);
}

/// The hunted animal's damage notify (`FUN_140c26420`) for a human player's king (class 59):
/// the owner's alert (0x201, a human player's alert list, not held), the attacker joins the
/// set, the unit is under attack and runs from its attackers (slot 0x160; class 59 not
/// unconditionally, so only without an action value), by twice its speed away from them;
/// processing breaks off (3). A computer player's AI would hear the alert (not modelled).
#[test]
fn human_player_king_runs_from_its_attacker() {
    let notify = Notify::own(object_id(77), NotifyKind::DAMAGED, [77, 40, 0]);
    let king = |kind| {
        let mut unit_ai = UnitAi {
            hunted: None,
            ..idle(UnitAiKind::Hunted)
        };
        unit_ai.post(notify, ObjectState::Alive).unwrap();
        let at = Position::new(20.0, 30.0, 0.0);
        let mut chased = chased(
            runner_unit(player_id(1), class::KING, unit_ai, at),
            kind,
            &[],
            &[(77, class::CLASS_6, 22.0)],
        );
        let run_ai = run_ai(
            &mut chased,
            (player_id(1), object_id(500)),
            |ai, at, side, kind, rng| ai.hunted_damaged(notify, Millis::ZERO, at, kind, rng, side),
        );
        (run_ai, chased)
    };
    let (result, world) = king(PlayerKind::Human);
    assert_eq!(result, Ok(BREAK));
    let ai = world.parts.unit_ai.get(object_id(500)).unwrap();
    assert_eq!(ai.attackers, Attackers::of_saved(&[77]));
    assert_eq!(walks_to(&world), Some(Position::new(18.0, 30.0, 0.0)));
    assert!(king(PlayerKind::Computer).0.is_err());
}

/// `runAwayFromAttackers` (`FUN_140c5e900`) beyond the king's run: stand ground and no
/// attack stay (Gaia's run all the same), and so does a unit with an action value unless
/// unconditionally; an attacker the world does not hold is left out; attackers that cancel
/// out send it a random way (two draws of the AI stream); a defensive unit (stance 1) runs at
/// most 5 tiles from its state position. A computer player's siege unit, a computer's unit
/// attacked by class 22 and a computer's transport ship are not modelled.
#[test]
fn unit_runs_from_its_attackers_within_its_leash() {
    let attackers = [
        (77, class::CLASS_6, 22.0),
        (78, class::CLASS_6, 18.0),
        (79, class::WARSHIP, 22.0),
    ];
    let run = |unit_ai: UnitAi, (owner, kind): (PlayerId, PlayerKind), class: ClassId, uncon| {
        let at = Position::new(20.0, 30.0, 0.0);
        let mut world = chased(
            runner_unit(owner, class, unit_ai, at),
            kind,
            &[],
            &attackers,
        );
        let answer = run_ai(
            &mut world,
            (owner, object_id(500)),
            |ai, at, side, kind, rng| ai.run_away(uncon, at, kind, rng, side),
        );
        (answer, walks_to(&world))
    };
    let human = (player_id(1), PlayerKind::Human);
    let gaia = (PlayerId::GAIA, PlayerKind::Gaia);
    let runner = |attackers: Vec<i32>| UnitAi {
        attackers: Attackers::of_saved(&attackers),
        ..idle(UnitAiKind::Hunted)
    };
    let forced = OrderForce::Forced;
    let away = |x| Some(Position::new(x, 30.0, 0.0));
    for stance in [UnitStance::StandGround, UnitStance::NoAttack] {
        let unit_ai = UnitAi {
            stance,
            ..runner(vec![77])
        };
        assert_eq!(
            run(unit_ai.clone(), human, class::CLASS_6, forced),
            (Ok(Answer::NotTaken), None)
        );
        assert_eq!(run(unit_ai, gaia, class::CLASS_6, forced).1, away(18.0));
    }
    let unit_ai = UnitAi {
        current_action: Some(AiAction::Attack),
        ..runner(vec![77])
    };
    assert_eq!(
        run(unit_ai, human, class::CLASS_6, OrderForce::Unforced),
        (Ok(Answer::NotTaken), None)
    );
    assert_eq!(
        run(runner(vec![77, 4242]), human, class::CLASS_6, forced).1,
        away(18.0)
    );
    // 77 and 78 on either side: the way two draws give
    let mut rng = rng();
    let mut part = || {
        let draw = rng.rand15() as i32 as f32;
        (draw + draw) / RUN_DRAW_SCALE - RUN_ONE
    };
    let (vx, vy) = (part(), part());
    let length = sqrt(vy * vy + vx * vx);
    let way = Position::new(20.0 - (vx / length) * 2.0, 30.0 - (vy / length) * 2.0, 0.0);
    assert_eq!(
        run(runner(vec![77, 78]), human, class::CLASS_6, forced).1,
        Some(way)
    );
    // a defensive unit with its state position 5 tiles east runs no farther than that
    let unit_ai = UnitAi {
        stance: UnitStance::Defensive,
        state_position: Point::new(25.0, 30.0),
        ..runner(vec![77])
    };
    assert_eq!(run(unit_ai, human, class::CLASS_6, forced).1, away(20.0));
    let computer = (player_id(1), PlayerKind::Computer);
    assert!(
        run(runner(vec![79]), computer, class::CLASS_6, forced)
            .0
            .is_err()
    );
    assert!(
        run(runner(vec![77]), computer, class::TRANSPORT, forced)
            .0
            .is_err()
    );
    assert!(
        run(idle(UnitAiKind::Siege), computer, class::CLASS_6, forced)
            .0
            .is_err()
    );
}

/// `FUN_140c64c10`: the target and its class become the last ones, the id and the point are
/// taken, and the class only when the world holds the object (else it stays).
#[test]
fn target_takes_its_class_only_from_an_object_the_world_holds() {
    let mut unit_ai = UnitAi {
        current_target: Some(object_id(40)),
        target_class: class_of(9),
        ..idle(UnitAiKind::Soldier)
    };
    unit_ai.target_at(Some(object_id(41)), None, Position::new(1.0, 2.0, 3.0));
    assert_eq!(
        (
            unit_ai.last_target,
            unit_ai.last_target_class,
            unit_ai.current_target,
            unit_ai.target_class
        ),
        (
            Some(object_id(40)),
            class_of(9),
            Some(object_id(41)),
            class_of(9)
        )
    );
    assert_eq!(unit_ai.target_point, Position::new(1.0, 2.0, 3.0));
    unit_ai.target_at(
        Some(object_id(42)),
        Some(class::VILLAGER),
        Position::new(5.0, 6.0, 0.0),
    );
    assert_eq!(
        (
            unit_ai.last_target,
            unit_ai.last_target_class,
            unit_ai.current_target,
            unit_ai.target_class
        ),
        (
            Some(object_id(41)),
            class_of(9),
            Some(object_id(42)),
            class_of(4)
        )
    );
}

/// Slot 0x38 (`FUN_140c556d0`) answers whether it took the notify: 0 outside state 2, for a
/// 699 from a unit that is not attacking it and for the invalidation of another action; 1
/// for a movement completion it ignores (not queued) and for one it queues.
#[test]
fn the_unit_ai_notify_answers_whether_it_took_it() {
    let mut unit_ai = UnitAi {
        current_action: Some(AiAction::Gather),
        attackers: Attackers::of_saved(&[70]),
        ..idle(UnitAiKind::Civilian)
    };
    let notify = |caller, kind, p0| Notify::own(caller, kind, [p0, 0, 0]);
    assert_eq!(
        unit_ai.accept(
            notify(object_id(70), NotifyKind::DAMAGED, 0),
            ObjectState::Foundation
        ),
        Answer::NotTaken
    );
    assert_eq!(
        unit_ai.accept(
            notify(object_id(71), NotifyKind::NO_LONGER_ON_ME, 0),
            ObjectState::Alive
        ),
        Answer::NotTaken
    );
    assert_eq!(
        unit_ai.accept(
            notify(
                object_id(70),
                NotifyKind::ACTION_INVALIDATED,
                AiAction::Attack.value()
            ),
            ObjectState::Alive
        ),
        Answer::NotTaken
    );
    assert_eq!(
        unit_ai.accept(
            notify(
                object_id(70),
                NotifyKind::ACTION_COMPLETED,
                AiAction::Move.value()
            ),
            ObjectState::Alive
        ),
        Answer::Taken
    );
    assert_eq!(unit_ai.notifies, Vec::new());
    assert_eq!(
        unit_ai.accept(
            notify(object_id(70), NotifyKind::NO_LONGER_ON_ME, 0),
            ObjectState::Alive
        ),
        Answer::Taken
    );
    assert_eq!(
        unit_ai.accept(
            notify(
                object_id(70),
                NotifyKind::ACTION_INVALIDATED,
                AiAction::Gather.value()
            ),
            ObjectState::Alive
        ),
        Answer::Taken
    );
    assert_eq!(unit_ai.notifies.len(), 2);
}

/// `0x140aa2a70` in state 3: a unit without an AI asks for its path even in range and keeps
/// its attack move; one off the patrol orders ends in range and stops attack-moving.
#[test]
fn unit_off_patrol_asks_plain_path() {
    let ask = UnitAi::path_ask(
        None,
        Point::new(20.0, 20.0),
        Point::new(30.0, 20.0),
        &tuning().unit_ai,
    );
    assert_eq!(
        ask,
        PathAsk {
            ends_in_range: false,
            ..PathAsk::default()
        }
    );
    let mut unit_ai = idle(UnitAiKind::Soldier);
    let ask = UnitAi::path_ask(
        Some(&mut unit_ai),
        Point::new(20.0, 20.0),
        Point::new(30.0, 20.0),
        &tuning().unit_ai,
    );
    assert_eq!(
        ask,
        PathAsk {
            attack_move: Some(false),
            ..PathAsk::default()
        }
    );
    // a saved patrol off the patrol orders stays, and so does the attack move
    unit_ai.patrol_path = Some(PatrolPath::default());
    let ask = UnitAi::path_ask(
        Some(&mut unit_ai),
        Point::new(20.0, 20.0),
        Point::new(30.0, 20.0),
        &tuning().unit_ai,
    );
    assert_eq!(
        (ask.attack_move, unit_ai.patrol_path.is_some()),
        (None, true)
    );
}

/// `0x140aa2a70` in state 3 on a patrol order: the path loops; without a saved patrol it goes
/// back and forth from 3 tiles (`SETBE`: a NaN distance too); order 733 attack-moves.
#[test]
fn patrol_order_path_loops_and_turns_back_from_three_tiles() {
    let mut unit_ai = UnitAi {
        current_order: Some(OrderKind::Patrol),
        ..idle(UnitAiKind::Soldier)
    };
    let ask = UnitAi::path_ask(
        Some(&mut unit_ai),
        Point::new(20.0, 20.0),
        Point::new(22.9, 20.0),
        &tuning().unit_ai,
    );
    assert_eq!(
        ask,
        PathAsk {
            ends_in_range: false,
            patrol: true,
            back_and_forth: false,
            resume: WaypointIndex::of_data(-1),
            attack_move: Some(false),
        }
    );
    let far = UnitAi::path_ask(
        Some(&mut unit_ai),
        Point::new(20.0, 20.0),
        Point::new(20.0, 23.0),
        &tuning().unit_ai,
    );
    assert!(far.back_and_forth);
    let nan = UnitAi::path_ask(
        Some(&mut unit_ai),
        Point::new(20.0, 20.0),
        Point::new(f32::NAN, 23.0),
        &tuning().unit_ai,
    );
    assert!(nan.back_and_forth);
    unit_ai.current_order = Some(OrderKind::AttackMove);
    let attack = UnitAi::path_ask(
        Some(&mut unit_ai),
        Point::new(20.0, 20.0),
        Point::new(22.9, 20.0),
        &tuning().unit_ai,
    );
    assert_eq!((attack.patrol, attack.attack_move), (true, Some(true)));
}

/// `0x140aa2a70` in state 3 on a patrol order with a saved patrol: its waypoint and its back
/// and forth (`getPatrolLinear`, flags bit 1) are the path's, and it goes
/// (`zeroPatrolInformation`), which then stops the attack move.
#[test]
fn saved_patrol_gives_its_waypoint_to_the_path_and_goes() {
    let mut unit_ai = UnitAi {
        current_order: Some(OrderKind::Patrol),
        patrol_path: Some(PatrolPath {
            flags: PathFlags::LOOPS | PathFlags::BACK_AND_FORTH,
            ..PatrolPath::default()
        }),
        patrol_waypoint: WaypointIndex::of_data(2),
        ..idle(UnitAiKind::Soldier)
    };
    let ask = UnitAi::path_ask(
        Some(&mut unit_ai),
        Point::new(20.0, 20.0),
        Point::new(20.5, 20.0),
        &tuning().unit_ai,
    );
    assert_eq!(
        (ask.resume.get(), ask.back_and_forth, ask.attack_move),
        (2, true, Some(false))
    );
    assert_eq!(
        (unit_ai.patrol_path, unit_ai.patrol_waypoint.get()),
        (None, 0)
    );
}

/// The soldier of the patrol tests: master 4 of class 6, a walker of line of sight 4 and
/// weapon range 1, standing on [`STANDING`].
fn soldier_master() -> Master {
    Master {
        line_of_sight: Distance::new(4.0),
        standing_sprite: Some(STANDING.id),
        combat: Some(CombatPart {
            weapon_range: Distance::new(1.0),
            ..CombatPart::default()
        }),
        ..walker(4, class::CLASS_6)
    }
}

/// A human player's soldier walking its patrol (711 with the move action) as the full update
/// finds it, its secondary timer `secondary`.
fn patroller(secondary: u32) -> UnitAi {
    UnitAi {
        current_order: Some(OrderKind::Patrol),
        current_action: Some(AiAction::Move),
        idle_timeout: Millis::new(3000),
        adjusted_idle_timeout: Millis::new(750),
        secondary_timer: Millis::new(secondary),
        ..idle(UnitAiKind::Soldier)
    }
}

/// Player 1's soldier 500 of master `soldier` at (20, 20) on a map of 64 by 64, on its
/// sprite, its unit AI `ai`, its top action a `MoveTo` to `walk` (its point and target), none
/// without one; the `others`, each in player 1's lists. The AI stream is seeded with 0, as
/// [`rng`] is.
fn soldier_world(
    (soldier, ai): (Master, UnitAi),
    walk: Option<(Point, Option<i32>)>,
    others: &[SceneObject],
) -> World {
    let actions = walk.map(|(to, target)| Action {
        list: ActionList::default().with_move_to(Some(MoveToAction {
            target: target.map(object_id),
            point: Position::new(to.x(), to.y(), 0.0),
            ..MoveToAction::sub_in(MoveTo::Moving)
        })),
        ..Action::default()
    });
    let mut scene = Scene::open(GridSize::new(64, 64))
        .players(2)
        .graphic(STANDING)
        .object(SceneObject {
            ai: Some(ai),
            actions,
            sprite: Some(STANDING.id),
            ..SceneObject::new(500, player_id(1), soldier, Position::new(20.0, 20.0, 0.0))
        });
    for other in others {
        scene = scene.object(SceneObject {
            seen_by: Some(VisionBits::visible(player_id(1))),
            ..other.clone()
        });
    }
    let mut world = scene.build();
    world.streams.get_mut(Stream::Ai).set_seed(0);
    world
}

/// Player 2's villager 77 at `at`, idle, for [`soldier_world`]'s others.
fn enemy_villager(at: Point) -> SceneObject {
    SceneObject {
        ai: Some(idle(UnitAiKind::Civilian)),
        ..SceneObject::new(
            77,
            player_id(2),
            villager_master(),
            Position::new(at.x(), at.y(), 0.0),
        )
    }
}

/// Object `id` of `owner` at `at`, a walker of master 5 and class 6, for [`soldier_world`]'s
/// others.
fn walker_unit(id: i32, owner: PlayerId, at: Point) -> SceneObject {
    SceneObject::new(
        id,
        owner,
        walker(5, class::CLASS_6),
        Position::new(at.x(), at.y(), 0.0),
    )
}

/// Unit 500's unit AI in `world`.
fn ai_500(world: &World) -> &UnitAi {
    world.parts.unit_ai.get(object_id(500)).unwrap()
}

/// The target of unit 500's Attack in `world`, none without one.
fn attacks_500(world: &World) -> Option<ObjectId> {
    world
        .parts
        .actions
        .of(object_id(500))
        .list
        .attack()
        .and_then(|attack| attack.target)
        .map(AttackTargetId::id)
}

/// Unit 500's full update in `world` ([`UnitAi::update`], 28 ms at time 0).
fn update_500(world: &mut World) -> GenieResult<Option<AfterOrder>> {
    run_ai(
        world,
        (player_id(1), object_id(500)),
        |ai, at, side, kind, rng| ai.update(Millis::new(28), WorldTime::ZERO, kind, at, rng, side),
    )
}

/// Unit 500's idle check in `world` ([`UnitAi::soldier_idle`]).
fn soldier_idle_500(world: &mut World) -> GenieResult<IdleOutcome> {
    run_ai(
        world,
        (player_id(1), object_id(500)),
        |ai, at, side, kind, rng| ai.soldier_idle(at, kind, rng, side),
    )
}

/// The idle part's patrol branch (`0x140c67f0b`) while the secondary timer runs: nothing
/// but the timer's count down; the busy check lets the walking patrol through.
#[test]
fn patrol_waits_for_its_secondary_timer() {
    let mut world = soldier_world(
        (soldier_master(), patroller(300)),
        Some((Point::new(30.0, 20.0), None)),
        &[enemy_villager(Point::new(22.0, 20.0))],
    );
    assert!(update_500(&mut world).is_ok());
    let unit_ai = ai_500(&world);
    assert_eq!(
        (unit_ai.secondary_timer, unit_ai.current_order),
        (Millis::new(272), Some(OrderKind::Patrol))
    );
    assert_eq!(attacks_500(&world), None);
}

/// The patrol branch with its secondary timer run out: it restarts at 750 (the adjusted
/// idle timeout of 750 redrawn), processIdle's search finds 77: the patrol is saved (the
/// unit's slot 0x468: its top path level's points, flags and waypoint), the attack command
/// given (its Attack at once), and the order goes without its history (`FUN_140c64b60` with
/// -1 and 0).
#[test]
fn patrol_saves_itself_and_attacks_what_it_finds() {
    let unit_ai = UnitAi {
        last_order: Some(OrderKind::Wait),
        ..patroller(0)
    };
    let mut world = soldier_world(
        (soldier_master(), unit_ai),
        Some((Point::new(30.0, 20.0), None)),
        &[enemy_villager(Point::new(22.0, 20.0))],
    );
    let points = vec![
        BVector::new(20.0, 0.0, 20.0),
        BVector::new(30.0, 0.0, 20.0),
        BVector::new(30.0, 0.0, 30.0),
    ];
    let flags = PathFlags::LOOPS | PathFlags::BACK_AND_FORTH;
    world.parts.mover_mut(object_id(500)).unwrap().paths = vec![PathData {
        points: points.clone(),
        has_path: true,
        path_flags: flags,
        waypoint: WaypointIndex::of_data(2),
        ..PathData::without_path()
    }];
    let before = world.streams.get_mut(Stream::Ai).draws;
    assert!(update_500(&mut world).is_ok());
    assert_eq!(attacks_500(&world), Some(object_id(77)));
    let unit_ai = ai_500(&world);
    assert_eq!(
        (unit_ai.current_order, unit_ai.last_order),
        (None, Some(OrderKind::Wait))
    );
    let saved = unit_ai
        .patrol_path
        .as_ref()
        .map(|patrol_path| (patrol_path.path.points.clone(), patrol_path.flags));
    assert_eq!(
        (saved, unit_ai.patrol_waypoint.get()),
        (Some((points, flags)), 2)
    );
    assert_eq!(unit_ai.secondary_timer, Millis::new(750));
    assert_ne!(unit_ai.adjusted_idle_timeout, Millis::new(750));
    assert!(before < world.streams.get_mut(Stream::Ai).draws);
}

/// The patrol branch passes over processIdle while the top action's target is a building
/// (slot 400: its walk to player 2's house 50); a unit target it does not.
#[test]
fn patrol_attacking_building_keeps_at_it() {
    let house = Master {
        id: master_id(70),
        kind: MasterKind::BUILDING,
        class: class::BUILDING,
        radius: Footprint::new(1.0, 1.0),
        height: Distance::new(2.0),
        hit_points: MasterHitPoints::of_data(550),
        ..Master::default()
    };
    let mut world = soldier_world(
        (soldier_master(), patroller(0)),
        Some((Point::new(30.0, 30.0), Some(50))),
        &[
            SceneObject::new(50, player_id(2), house, Position::new(30.0, 30.0, 0.0)),
            enemy_villager(Point::new(22.0, 20.0)),
        ],
    );
    assert!(update_500(&mut world).is_ok());
    assert_eq!(attacks_500(&world), None);
    assert_eq!(ai_500(&world).secondary_timer, Millis::new(750));
}

/// The soldier's idle check (`FUN_140c20db0`) takes its top action's target within its
/// range (slot 400, slot 0x1e8: the footprints' squared gap between the least range and the
/// weapon range squared) without a search; out of range it searches.
#[test]
fn soldier_idle_check_attacks_its_top_action_target_in_range() {
    let unit_ai = UnitAi {
        current_order: None,
        ..patroller(0)
    };
    let world = |soldier: Master, x: f32| {
        soldier_world(
            (soldier, unit_ai.clone()),
            Some((Point::new(x, 20.0), Some(50))),
            &[
                walker_unit(50, player_id(2), Point::new(x, 20.0)),
                enemy_villager(Point::new(20.0, 22.0)),
            ],
        )
    };
    let in_range = |world: &mut World| {
        run_ai(
            world,
            (player_id(1), object_id(500)),
            |_, at, side, _, _| UnitAi::top_target_in_range(at, side),
        )
    };
    assert_eq!(
        in_range(&mut world(soldier_master(), 21.0)),
        Ok(Some(object_id(50)))
    );
    let least = Master {
        combat: Some(CombatPart {
            weapon_range: Distance::new(1.0),
            min_range: Distance::new(1.0),
            ..CombatPart::default()
        }),
        ..soldier_master()
    };
    assert_eq!(in_range(&mut world(least, 21.0)), Ok(None));
    let mut game = world(soldier_master(), 21.0);
    assert_eq!(soldier_idle_500(&mut game), Ok(IdleOutcome::Acted));
    assert_eq!(attacks_500(&game), Some(object_id(50)));
    // too far: the search's 77
    let mut world = world(soldier_master(), 25.0);
    assert_eq!(soldier_idle_500(&mut world), Ok(IdleOutcome::Acted));
    assert_eq!(attacks_500(&world), Some(object_id(77)));
}

/// The soldier's idle check's other ways (`FUN_140c20db0`): master 706 checks nothing; with
/// retargeting off a current target the world holds is attacked without a search, the
/// order kept; a failed attack on a patrol drops the saved patrol and stops on the order (its
/// held current target, untargetable (`+0x689` off), refused by slot 0xa0); a computer player's
/// check and a healer's ally search are not modelled.
#[test]
fn soldier_idle_check_holds_its_target_with_retargeting_off() {
    let villager = enemy_villager(Point::new(22.0, 20.0));
    let walk = Some((Point::new(30.0, 20.0), None));
    let saboteur = Master {
        id: ids::SABOTEUR,
        ..soldier_master()
    };
    let mut villager_world = soldier_world((saboteur, patroller(0)), walk, from_ref(&villager));
    assert_eq!(soldier_idle_500(&mut villager_world), Ok(IdleOutcome::Idle));
    assert_eq!(attacks_500(&villager_world), None);
    let holding = |target: i32| UnitAi {
        current_order: None,
        retarget_disabled: 1,
        current_target: Some(object_id(target)),
        ..patroller(0)
    };
    let sixty = walker_unit(60, player_id(2), Point::new(23.0, 20.0));
    let mut world = soldier_world(
        (soldier_master(), holding(60)),
        walk,
        &[sixty, villager.clone()],
    );
    assert_eq!(soldier_idle_500(&mut world), Ok(IdleOutcome::Acted));
    assert_eq!(
        (attacks_500(&world), ai_500(&world).current_order),
        (Some(object_id(60)), None)
    );
    let mut world = soldier_world((soldier_master(), patroller(0)), walk, &[villager]);
    world.players[1].kind = PlayerKind::Computer;
    assert!(soldier_idle_500(&mut world).is_err());
    // the untargetable 61 held: refused, the patrol saved from its path goes
    let own = SceneObject {
        flags: UnitFlags::ATTACKABLE | UnitFlags::UNTARGETABLE,
        ..walker_unit(61, player_id(2), Point::new(21.0, 20.0))
    };
    let refused = UnitAi {
        current_order: Some(OrderKind::Patrol),
        ..holding(61)
    };
    let mut world = soldier_world((soldier_master(), refused), walk, &[own]);
    world.parts.mover_mut(object_id(500)).unwrap().paths = vec![PathData {
        points: vec![BVector::new(20.0, 0.0, 20.0), BVector::new(30.0, 0.0, 20.0)],
        has_path: true,
        waypoint: WaypointIndex::of_data(1),
        ..PathData::without_path()
    }];
    assert_eq!(soldier_idle_500(&mut world), Ok(IdleOutcome::Idle));
    let unit_ai = ai_500(&world);
    assert_eq!(
        (&unit_ai.patrol_path, unit_ai.patrol_waypoint.get()),
        (&None, 0)
    );
    assert_eq!(attacks_500(&world), None);
    let healer = Master {
        action: Some(ActionPart {
            tasks: vec![Task {
                action_kind: ActionKind::HEAL,
                ..Task::default()
            }],
            ..ActionPart::default()
        }),
        ..soldier_master()
    };
    let unit_ai = UnitAi {
        current_order: None,
        ..patroller(0)
    };
    let mut world = soldier_world((healer, unit_ai), walk, &[]);
    assert!(soldier_idle_500(&mut world).is_err());
}

/// A stop earlier in the update (a failed Enter's notify, slot 0xb0) empties the action list
/// at once: the soldier's idle check then finds no top action's target, in range as it was
/// at the update's start, and searches (426750657's scout inside its transport, update 467);
/// an effect that leaves the actions does not change it.
#[test]
fn soldier_stopped_in_the_update_finds_no_top_action_target() {
    let unit_ai = UnitAi {
        current_order: None,
        ..patroller(0)
    };
    let mut world = soldier_world(
        (soldier_master(), unit_ai),
        Some((Point::new(21.0, 20.0), Some(50))),
        &[walker_unit(50, player_id(2), Point::new(21.0, 20.0))],
    );
    let read = run_ai(
        &mut world,
        (player_id(1), object_id(500)),
        |ai, at, side, _, rng| {
            let first = UnitAi::top_target_in_range(at, side)?;
            side.effect_now(Effect::CapHumanOrder(WorldTime::from_millis(5000)), ai, rng)?;
            let capped = UnitAi::top_target_in_range(at, side)?;
            side.effect_now(Effect::Stop, ai, rng)?;
            let stopped = UnitAi::top_target_in_range(at, side)?;
            side.effect_now(Effect::CapHumanOrder(WorldTime::from_millis(5000)), ai, rng)?;
            Ok((
                first,
                capped,
                stopped,
                UnitAi::top_target_in_range(at, side)?,
            ))
        },
    );
    let fifty = Some(object_id(50));
    assert_eq!(read, Ok((fifty, fifty, None, None)));
}

/// The soldier's idle check with nothing to attack and a saved patrol, in no group:
/// `restorePatrolInformation` gives the unit its walk back onto the patrol (to its saved
/// waypoint), the order 711 (733 for a unit that attack-moves) and a secondary timer of 750;
/// processIdle produced.
#[test]
fn soldier_with_nothing_to_attack_goes_back_to_its_patrol() {
    let mut path = Path::default();
    let _ = path.push(BVector::new(20.0, 0.0, 20.0));
    let _ = path.push(BVector::new(30.0, 0.0, 20.0));
    let unit_ai = UnitAi {
        current_order: None,
        current_action: None,
        patrol_path: Some(PatrolPath {
            path,
            ..PatrolPath::default()
        }),
        patrol_waypoint: WaypointIndex::of_data(1),
        ..patroller(0)
    };
    let mut world = soldier_world((soldier_master(), unit_ai), None, &[]);
    assert_eq!(soldier_idle_500(&mut world), Ok(IdleOutcome::Acted));
    assert_eq!(walks_to(&world), Some(Position::new(30.0, 20.0, 0.0)));
    let unit_ai = ai_500(&world);
    assert_eq!(
        (unit_ai.current_order, unit_ai.secondary_timer),
        (Some(OrderKind::Patrol), Millis::new(750))
    );
    world
        .parts
        .unit_ai
        .get_mut(object_id(500))
        .unwrap()
        .current_order = None;
    world.parts.mover_mut(object_id(500)).unwrap().attack_moving = true;
    assert_eq!(soldier_idle_500(&mut world), Ok(IdleOutcome::Acted));
    assert_eq!(ai_500(&world).current_order, Some(OrderKind::AttackMove));
}

/// The order check of a unit no longer busy: the attack move (733) ends, its timers reset
/// (an adjusted idle timeout of 750 redrawn, the secondary timer capped at 750); the patrol
/// order (711) stays.
#[test]
fn an_attack_move_not_busy_ends_and_patrol_stays() {
    let at = no_surroundings();
    let mut unit_ai = UnitAi {
        current_order: Some(OrderKind::AttackMove),
        idle_timeout: Millis::new(3000),
        adjusted_idle_timeout: Millis::new(750),
        secondary_timer: Millis::new(900),
        ..idle(UnitAiKind::Soldier)
    };
    unit_ai.check_order(&at, &mut rng());
    assert_eq!(
        (unit_ai.current_order, unit_ai.secondary_timer),
        (None, Millis::new(750))
    );
    assert_ne!(unit_ai.adjusted_idle_timeout, Millis::new(750));
    let mut unit_ai = UnitAi {
        current_order: Some(OrderKind::Patrol),
        ..idle(UnitAiKind::Soldier)
    };
    unit_ai.check_order(&at, &mut rng());
    assert_eq!(unit_ai.current_order, Some(OrderKind::Patrol));
}

/// Slot 0xb0 (`FUN_140c64ed0`) on a patrol order: the order ends even when the stop keeps
/// orders, its secondary timer capped at 750; with an adjusted idle timeout of exactly 750
/// the redraw (the AI stream's) is not modelled here.
#[test]
fn stop_ends_patrol_order_with_its_secondary_timer_capped() {
    let stop = |unit_ai: UnitAi, place: Option<(Point, Option<ClassId>)>| {
        let soldier = lone_unit(player_id(1), unit_ai, Position::new(30.0, 30.0, 0.0));
        let mut world = lone_world(on_its_way(soldier, Position::new(40.0, 30.0, 0.0)), &[]);
        let answer = run_ai(
            &mut world,
            (player_id(1), object_id(50)),
            |ai, at, side, _, rng| ai.stop_with(StopOrder::Keep, place, side, rng, &at.roles),
        );
        let unit_ai = world.parts.unit_ai.get(object_id(50)).unwrap().clone();
        (answer, unit_ai, lone_stopped(&world))
    };
    let patrolling = UnitAi {
        current_order: Some(OrderKind::Patrol),
        current_action: Some(AiAction::Move),
        adjusted_idle_timeout: Millis::new(2600),
        secondary_timer: Millis::new(900),
        ..idle(UnitAiKind::Soldier)
    };
    let (answer, unit_ai, stopped) = stop(
        patrolling,
        Some((Point::new(3.0, 4.0), Some(class::CLASS_6))),
    );
    assert_eq!((answer, stopped), (Ok(()), true));
    assert_eq!(
        (
            unit_ai.current_order,
            unit_ai.last_order,
            unit_ai.secondary_timer
        ),
        (None, Some(OrderKind::Patrol), Millis::new(750))
    );
    assert_eq!(unit_ai.state_position, Point::new(3.0, 4.0));
    let attack_moving = UnitAi {
        current_order: Some(OrderKind::AttackMove),
        adjusted_idle_timeout: Millis::new(750),
        ..idle(UnitAiKind::Soldier)
    };
    assert!(stop(attack_moving, None).0.is_err());
}

/// The order's tail (`LAB_140c5d611`) and the Build's end (`FUN_140c64b60` with −1): leaving
/// a patrol order caps the secondary timer; staying on it does not.
#[test]
fn leaving_patrol_order_caps_its_secondary_timer() {
    let order = |order| QueuedOrder {
        order,
        priority: OrderPriority::of_data(100),
        ..QueuedOrder::default()
    };
    let patrolling = || UnitAi {
        current_order: Some(OrderKind::Patrol),
        adjusted_idle_timeout: Millis::new(2600),
        secondary_timer: Millis::new(900),
        ..idle(UnitAiKind::Soldier)
    };
    let tail = |unit_ai: &mut UnitAi, queued_order: QueuedOrder| {
        unit_ai.order_tail(
            Some(queued_order.order),
            queued_order.priority,
            Some(class::CLASS_6),
            TakenAs::Order,
            &mut rng(),
            &roles(),
        )
    };
    let mut unit_ai = patrolling();
    assert_eq!(
        tail(&mut unit_ai, order(OrderKind::Move)),
        OrderOutcome::Taken
    );
    assert_eq!(
        (unit_ai.current_order, unit_ai.secondary_timer),
        (Some(OrderKind::Move), Millis::new(750))
    );
    let mut unit_ai = patrolling();
    assert_eq!(
        tail(&mut unit_ai, order(OrderKind::Patrol)),
        OrderOutcome::Taken
    );
    assert_eq!(unit_ai.secondary_timer, Millis::new(900));
    let mut unit_ai = patrolling();
    unit_ai.end_order_and_action().unwrap();
    assert_eq!(
        (
            unit_ai.current_order,
            unit_ai.last_order,
            unit_ai.secondary_timer
        ),
        (None, Some(OrderKind::Patrol), Millis::new(750))
    );
}

/// A unit AI with an order on a target ready in its queue, its idle timer, look-around timer
/// and retarget timer all past their timeouts.
fn ordered(ai_kind: UnitAiKind) -> UnitAi {
    UnitAi {
        orders: vec![QueuedOrder {
            order: OrderKind::OnTarget,
            priority: OrderPriority::of_data(100),
            target: Some(object_id(77)),
            ready: true,
            ..QueuedOrder::default()
        }],
        idle_timer: Millis::new(5000),
        look_around_timer: Millis::new(2000),
        last_retarget_time: WorldTime::ZERO,
        retarget_timer: Millis::new(900),
        ..idle(ai_kind)
    }
}

/// `0x140c66b10` runs the order on a target inside the update (slot 0x230's case 0x2db), so
/// the update stops there: the order (on player 2's walker 77; the soldier's master has no task,
/// so it gives no action) and what the rest of the update reads of its start ([`AfterOrder`]:
/// its `busy` and group) go to the caller; slot 73, the idle part and the look-around check have
/// not run (no draw, the timers as they grew).
#[test]
fn full_update_stops_at_its_order_on_target() {
    let soldier = lone_unit(
        player_id(1),
        ordered(UnitAiKind::Soldier),
        Position::new(30.0, 30.0, 0.0),
    );
    let mut world = lone_world(soldier, &[order_foe()]);
    let (answer, unit_ai, draws) =
        lone_step(&mut world, Millis::new(100), WorldTime::from_millis(10_000));
    assert_eq!(
        answer,
        Ok(Some(AfterOrder {
            busy: false,
            group: None,
        }))
    );
    assert_eq!((lone_attacks(&world), draws), (None, 0));
    assert_eq!(
        (
            unit_ai.idle_timer,
            unit_ai.look_around_timer,
            unit_ai.retarget_timer,
            unit_ai.orders.len()
        ),
        (Millis::new(5100), Millis::new(2100), Millis::new(900), 0)
    );
}

/// Player 2's idle walker 77 (class 6, master 5) at (32, 30), [`ordered`]'s target.
fn order_foe() -> SceneObject {
    SceneObject {
        ai: Some(idle(UnitAiKind::Soldier)),
        ..walker_unit(77, player_id(2), Point::new(32.0, 30.0))
    }
}

/// Player 1's lone unit 50 of `world` runs the rest of its full update after the order on a
/// target ([`UnitAi::after_order`], `after`, at time 10 000): its answer, its unit AI and the
/// AI stream's draws.
fn lone_after_order(
    world: &mut World,
    owner: PlayerId,
    after: AfterOrder,
) -> (GenieResult<()>, UnitAi, u32) {
    let before = world.streams.get_mut(Stream::Ai).draws;
    let answer = run_ai(world, (owner, object_id(50)), |ai, at, side, kind, rng| {
        ai.after_order(after, WorldTime::from_millis(10_000), kind, at, rng, side)
    });
    let draws = world.streams.get_mut(Stream::Ai).draws - before;
    let unit_ai = world.parts.unit_ai.get(object_id(50)).unwrap().clone();
    (answer, unit_ai, draws)
}

/// The update's rest after the order (`0x140c66b10` from slot 73 on): the retarget check
/// reads the order's attack (its timer redrawn, then a human unit whose Attack waits to swing,
/// an attack's work state, stops there); the idle part sees the action, so an action of a
/// player above Gaia keeps the idle timer at 0; the look-around check redraws its timeout.
#[test]
fn the_rest_after_an_order_that_gave_an_action_reads_it() {
    let attacking = UnitAi {
        orders: Vec::new(),
        current_order: Some(OrderKind::Attack),
        current_action: Some(AiAction::Attack),
        current_target: Some(object_id(77)),
        ..ordered(UnitAiKind::Soldier)
    };
    let soldier = SceneObject {
        actions: Some(Action {
            list: ActionList::default().with_attack(Some(own_attack(
                77,
                AttackState::Ready,
                Distance::new(1.0),
            ))),
            ..Action::default()
        }),
        ..lone_unit(player_id(1), attacking, Position::new(30.0, 30.0, 0.0))
    };
    let mut world = lone_world(soldier, &[order_foe()]);
    let (answer, unit_ai, draws) =
        lone_after_order(&mut world, player_id(1), AfterOrder::default());
    assert_eq!(answer, Ok(()));
    let mut expected = rng();
    let (retarget, look) = (expected.rand15(), expected.rand15());
    assert_eq!(
        (lone_attacks(&world), lone_walks(&world)),
        (Some(object_id(77)), None)
    );
    assert_eq!(
        (
            unit_ai.last_retarget_time,
            unit_ai.retarget_timer,
            unit_ai.idle_timer,
            unit_ai.look_around_timer,
            unit_ai.look_around_timeout,
            draws
        ),
        (
            WorldTime::from_millis(10_000),
            Millis::new(750 + retarget % 500),
            Millis::ZERO,
            Millis::ZERO,
            Millis::new(4500 + look % 1000),
            2
        )
    );
}

/// An order that gave the AI no action (no task on its target): the idle timer past its
/// timeout restarts and slot 72 runs with 1 (the villager's: its cap of the last human order
/// time). With the update's `busy` and a target the order gave, it does not; the idle part's
/// end then keeps the timer of a player above Gaia at 0, and Gaia's unit keeps its timer.
#[test]
fn the_rest_after_an_order_that_gave_no_action_runs_the_idle_check() {
    let unordered = UnitAi {
        orders: Vec::new(),
        ..ordered(UnitAiKind::Civilian)
    };
    let at = Position::new(30.0, 30.0, 0.0);
    let mut world = lone_world(lone_unit(player_id(1), unordered.clone(), at), &[]);
    lone_past_cap(&mut world, player_id(1));
    let (answer, unit_ai, draws) =
        lone_after_order(&mut world, player_id(1), AfterOrder::default());
    assert_eq!(answer, Ok(()));
    assert_eq!((unit_ai.idle_timer, draws), (Millis::ZERO, 1));
    assert_eq!(lone_human_order(&world, player_id(1)), HUMAN_ORDER_CAP);
    let busy = AfterOrder {
        busy: true,
        group: None,
    };
    let targeting = UnitAi {
        current_target: Some(object_id(77)),
        ..unordered
    };
    for (owner, timer) in [
        (player_id(1), Millis::ZERO),
        (PlayerId::GAIA, Millis::new(5000)),
    ] {
        let mut world = lone_world(lone_unit(owner, targeting.clone(), at), &[order_foe()]);
        lone_past_cap(&mut world, owner);
        let (answer, unit_ai, _) = lone_after_order(&mut world, owner, busy);
        assert_eq!((answer, unit_ai.idle_timer), (Ok(()), timer), "{owner:?}");
        assert_eq!(
            lone_human_order(&world, owner),
            WorldTime::from_millis(9000)
        );
    }
}

/// The transport ship's handler (`FUN_140c2a580`): a notify other than damage and a
/// missile's warning is the base handler's, as it answers it: the same answer, unit AI and
/// actions as the base handler's on a ship alike.
#[test]
fn the_transport_ship_passes_other_notifies_to_the_base_handler() {
    let moving = UnitAi {
        current_action: Some(AiAction::Move),
        current_order: Some(OrderKind::Move),
        ..ship_ai()
    };
    let notify = Notify::own(
        object_id(500),
        NotifyKind::ACTION_COMPLETED,
        [AiAction::Move.value(), 0, 0],
    );
    let mut world = transport_world(moving.clone(), None, PlayerKind::Human);
    let hears = hears(&mut world, (player_id(1), 500), notify);
    let mut base = transport_world(moving, None, PlayerKind::Human);
    let run_ai = run_ai(
        &mut base,
        (player_id(1), object_id(500)),
        |ai, at, side, kind, rng| ai.base_notify(notify, Millis::ZERO, at, kind, rng, side),
    );
    assert_eq!(hears, run_ai);
    assert_eq!(ai_500(&world), ai_500(&base));
    assert_eq!(
        world.parts.actions.of(object_id(500)).top_action_kind(),
        base.parts.actions.of(object_id(500)).top_action_kind()
    );
}

/// Player 1's transport ship 500 (class 20, unit-AI type 0x13) at (20, 30), its unit AI `ai`
/// (which names its attacker) and actions `actions`, its owner of type `kind`: player 2's
/// warship 77 at (22, 30) targets it.
fn transport_world(ai: UnitAi, actions: Option<Action>, kind: PlayerKind) -> World {
    let ship = SceneObject {
        actions,
        ..runner_unit(
            player_id(1),
            class::TRANSPORT,
            ai,
            Position::new(20.0, 30.0, 0.0),
        )
    };
    chased(ship, kind, &[], &[(77, class::WARSHIP, 22.0)])
}

/// The transport ship's unit AI whose attackers hold warship 77.
fn ship_ai() -> UnitAi {
    UnitAi {
        attackers: Attackers::of_saved(&[77]),
        ..idle(UnitAiKind::Transport)
    }
}

/// [`transport_world`]'s ship handles `notify` from its unit AI's queue ([`UnitAi::handle`]): its
/// answer and the world.
fn ship_hears(
    notify: Notify,
    mut ai: UnitAi,
    actions: Option<Action>,
    kind: PlayerKind,
) -> (GenieResult<u32>, World) {
    ai.post(notify, ObjectState::Alive).unwrap();
    let mut world = transport_world(ai, actions, kind);
    let run_ai = run_ai(
        &mut world,
        (player_id(1), object_id(500)),
        |ai, at, side, kind, rng| ai.handle(notify, Millis::ZERO, at, kind, rng, side),
    );
    (run_ai, world)
}

/// Its damage notify: the attacker it knows joins no one new (no call for help), the unit is
/// under attack; without an order or an action it runs from its attackers by twice its speed
/// (slot 0x160 with 1) and breaks off (3). With an order it ignores the damage (2). At no hit
/// points left it gives up (4). A computer player's AI would hear the alert (not modelled).
#[test]
fn transport_ship_runs_from_its_attackers_without_an_order() {
    let notify = Notify::own(object_id(77), NotifyKind::DAMAGED, [77, 40, 0]);
    let human = PlayerKind::Human;
    let (heard, game) = ship_hears(notify, ship_ai(), None, human);
    assert_eq!(heard, Ok(BREAK));
    let ai = game.parts.unit_ai.get(object_id(500)).unwrap();
    assert_eq!(ai.attackers, Attackers::of_saved(&[77]));
    assert_eq!(walks_to(&game), Some(Position::new(18.0, 30.0, 0.0)));
    let ordered = UnitAi {
        current_order: Some(OrderKind::Move),
        ..ship_ai()
    };
    assert_eq!(
        {
            let (result, world) = ship_hears(notify, ordered, None, human);
            (result, walks_to(&world))
        },
        (Ok(GO_ON), None)
    );
    // busy with its top action in state 2 it runs too; in another state (its MoveTo's 4) it
    // stays
    let moving = Action {
        list: ActionList::default().with_move_to(Some(MoveToAction {
            point: Position::new(25.0, 30.0, 0.0),
            ..MoveToAction::sub_in(MoveTo::Moving)
        })),
        ..Action::default()
    };
    let (result, moving_world) = ship_hears(notify, ship_ai(), Some(moving), human);
    assert_eq!(
        (result, walks_to(&moving_world)),
        (Ok(BREAK), Some(Position::new(25.0, 30.0, 0.0)))
    );
    let dead = Notify::own(object_id(77), NotifyKind::DAMAGED, [77, 0, 0]);
    assert_eq!(ship_hears(dead, ship_ai(), None, human).0, Ok(DEAD));
    assert!(
        ship_hears(notify, ship_ai(), None, PlayerKind::Computer)
            .0
            .is_err()
    );
    // a new attacker that is not its target: the call for help (AskForHelp: the search finds
    // no helper in the registry, which holds no object), then it joins the set and the ship
    // runs
    let (result, world) = ship_hears(notify, idle(UnitAiKind::Transport), None, human);
    assert_eq!(
        (result, walks_to(&world)),
        (Ok(BREAK), Some(Position::new(18.0, 30.0, 0.0)))
    );
    let ai = world.parts.unit_ai.get(object_id(500)).unwrap();
    assert_eq!(ai.attackers, Attackers::of_saved(&[77]));
}

/// A missile's warning (`0x20f`): one draw, `r * 100 / 0x7fff` below the owner's
/// `chance_to_dodge_missiles` dodges it (2); else it is damage.
#[test]
fn transport_ship_dodges_missile_warning_by_its_owner_chance() {
    let percent = rng().rand15() as i32 * 100 / 0x7fff;
    let warning = Notify::own(object_id(77), NotifyKind::MISSILE_WARNING, [77, 40, 0]);
    let warned = |chance: i32| {
        let mut transport_world = transport_world(ship_ai(), None, PlayerKind::Human);
        transport_world.players[1].dodge_chance = Chance::of_data(u8::try_from(chance).unwrap());
        let answer = run_ai(
            &mut transport_world,
            (player_id(1), object_id(500)),
            |ai, at, side, kind, rng| ai.handle(warning, Millis::ZERO, at, kind, rng, side),
        );
        (answer, transport_world)
    };
    let (answer, mut world) = warned(percent + 1);
    assert_eq!(answer, Ok(GO_ON));
    assert_eq!(
        (world.streams.get_mut(Stream::Ai).draws, walks_to(&world)),
        (1, None)
    );
    let (answer, world) = warned(percent);
    assert_eq!(
        (answer, walks_to(&world)),
        (Ok(BREAK), Some(Position::new(18.0, 30.0, 0.0)))
    );
}

/// The siege tower (0x12): its idle check (`FUN_140c20690`) only caps the last human order
/// time; its damage notify (`FUN_140c28400`) is the transport ship's, but a human player's
/// tower in a group that is not idle (its motion moving) ignores the damage (2), and one in an
/// idle group runs.
#[test]
fn siege_tower_in_busy_group_ignores_its_damage() {
    let mut game = transport_world(idle(UnitAiKind::SiegeTower), None, PlayerKind::Human);
    game.players[1]
        .objects
        .by_id_mut(object_id(500))
        .unwrap()
        .last_human_order = WorldTime::from_millis(9000);
    let before = game.streams.get_mut(Stream::Ai).draws;
    let idle_outcome = run_ai(
        &mut game,
        (player_id(1), object_id(500)),
        |ai, at, side, kind, rng| ai.process_idle(at, kind, rng, side),
    );
    assert_eq!(idle_outcome, Ok(IdleOutcome::Idle));
    assert_eq!(game.streams.get_mut(Stream::Ai).draws - before, 0);
    let tower = game.players[1].objects.by_id(object_id(500)).unwrap();
    assert_eq!(tower.last_human_order, HUMAN_ORDER_CAP);
    let notify = Notify::own(object_id(77), NotifyKind::DAMAGED, [77, 40, 0]);
    let tower = |idle_group: bool| {
        let ai = UnitAi {
            ai_kind: UnitAiKind::SiegeTower,
            ..ship_ai()
        };
        let mut world = transport_world(ai, None, PlayerKind::Human);
        let speed = if idle_group { 0.0 } else { 1.0 };
        world.groups.groups.push(UnitGroup {
            id: group_id(7),
            owner: Some(player_id(1)),
            members: vec![Member {
                id: object_id(500),
                ..Member::default()
            }],
            motion: Motion {
                velocity: BVector::new(speed, 0.0, 0.0),
                ..Motion::default()
            },
            ..UnitGroup::default()
        });
        world.players[1]
            .objects
            .by_id_mut(object_id(500))
            .unwrap()
            .group
            .id = Some(group_id(7));
        let answer = run_ai(
            &mut world,
            (player_id(1), object_id(500)),
            |ai, at, side, kind, rng| ai.handle(notify, Millis::ZERO, at, kind, rng, side),
        );
        let attackers = world
            .parts
            .unit_ai
            .get(object_id(500))
            .unwrap()
            .attackers
            .clone();
        (answer, walks_to(&world), attackers)
    };
    assert_eq!(tower(false), (Ok(GO_ON), None, Attackers::of_saved(&[77])));
    let (answer, walk, _) = tower(true);
    assert_eq!(
        (answer, walk),
        (Ok(BREAK), Some(Position::new(18.0, 30.0, 0.0)))
    );
}

/// `AttackObject` (slot 0xb8) of the types whose slot 0xa0 is the base's (`FUN_140c48400`,
/// anything): the ranged units', the ships', the artifact's and the siege tower's attack a
/// unit the world holds (unit 9's Attack at once on player 2's 77); the AI takes the attack and
/// the target.
#[test]
fn the_base_attack_check_lets_every_kind_without_its_own_attack() {
    for ai_kind in [
        UnitAiKind::Ranged,
        UnitAiKind::Ranged2,
        UnitAiKind::Trade,
        UnitAiKind::Transport,
        UnitAiKind::Fishing,
        UnitAiKind::Artifact,
        UnitAiKind::SiegeTower,
    ] {
        let enemy = order_target(77, player_id(2), class::CLASS_6);
        let mut world = order_world(order_unit(order_soldier(), idle(ai_kind)), &[enemy]);
        let answer = run_ai(
            &mut world,
            (player_id(1), object_id(9)),
            |ai, at, side, _, rng| {
                ai.attack_object(
                    object_id(77),
                    OrderForce::Forced,
                    TargetRules::Own,
                    at,
                    (side, rng),
                )
            },
        );
        assert_eq!(answer, Ok(Answer::Taken), "type {ai_kind}");
        let unit_ai = ai_9(&world);
        assert_eq!(
            (
                attacks_9(&world),
                unit_ai.current_action,
                unit_ai.current_target
            ),
            (
                Some(object_id(77)),
                Some(AiAction::Attack),
                Some(object_id(77))
            ),
            "type {ai_kind}"
        );
    }
}

/// Player 1's ship 500 of class `class` (master 4, a walker of speed 1 and line of sight 4)
/// at (20, 30) on a map of 64 by 64, its unit AI `ai`, its owner of `kind`; the `others` (id,
/// owner, class, x), walkers at (x, 30), each in player 1's lists. The ship's last human order
/// time is past the cap.
fn ship_world(
    (class, ai): (ClassId, UnitAi),
    kind: PlayerKind,
    others: &[(i32, PlayerId, ClassId, f32)],
) -> World {
    let ship = Master {
        line_of_sight: Distance::new(4.0),
        ..walker(4, class)
    };
    let mut scene = Scene::open(GridSize::new(64, 64))
        .players(2)
        .kind(player_id(1), kind)
        .object(SceneObject {
            ai: Some(ai),
            ..SceneObject::new(500, player_id(1), ship, Position::new(20.0, 30.0, 0.0))
        });
    for &(id, owner, class, x) in others {
        scene = scene.object(SceneObject {
            seen_by: Some(VisionBits::visible(player_id(1))),
            ..SceneObject::new(
                id,
                owner,
                walker(i16::try_from(id).expect("a master's id"), class),
                Position::new(x, 30.0, 0.0),
            )
        });
    }
    let mut world = scene.build();
    world.players[1]
        .objects
        .by_id_mut(object_id(500))
        .unwrap()
        .last_human_order = WorldTime::from_millis(9000);
    world
}

/// The ships' idle check (`FUN_140c21930`, `FUN_140c1eab0`): a human player's only caps the
/// last human order time and searches nothing (5); a computer player's fishing ship looks for
/// the closest object of its classes, any relation, reached, its radius grown by 7 (12: an
/// object 11 tiles off is found, one 13 off is not), and moves away from it by seven times its
/// speed (6); a trade cart searches its own classes; finding nothing, 5.
#[test]
fn computer_player_ship_evades_the_closest_object_it_sees() {
    let idle_of = |class, ai: UnitAi, kind, others: &[(i32, PlayerId, ClassId, f32)]| {
        let mut ship_world = ship_world((class, ai), kind, others);
        let run_ai = run_ai(
            &mut ship_world,
            (player_id(1), object_id(500)),
            |ai, at, side, kind, rng| ai.process_idle(at, kind, rng, side),
        );
        (run_ai, ship_world)
    };
    let fishing = idle(UnitAiKind::Fishing);
    let warship = |x| [(77, player_id(2), class::WARSHIP, x)];
    let (idle_outcome, world) = idle_of(
        class::FISHING_SHIP,
        fishing.clone(),
        PlayerKind::Human,
        &warship(22.0),
    );
    assert_eq!(idle_outcome, Ok(IdleOutcome::Idle));
    assert!(world.spatial.searched().is_none());
    assert_eq!(walks_to(&world), None);
    let ship = world.players[1].objects.by_id(object_id(500)).unwrap();
    assert_eq!(ship.last_human_order, HUMAN_ORDER_CAP);
    let computer = PlayerKind::Computer;
    let (idle_outcome, world) = idle_of(
        class::FISHING_SHIP,
        fishing.clone(),
        computer,
        &warship(22.0),
    );
    assert_eq!(idle_outcome, Ok(IdleOutcome::Acted));
    assert_eq!(walks_to(&world), Some(Position::new(13.0, 30.0, 0.0)));
    // the radius: 12 from the footprints' gap (0.4 apart at the centres' 11.4 and 13.4)
    let (idle_outcome, world) = idle_of(
        class::FISHING_SHIP,
        fishing.clone(),
        computer,
        &warship(31.4),
    );
    assert_eq!(
        (idle_outcome, walks_to(&world).is_some()),
        (Ok(IdleOutcome::Acted), true)
    );
    let (idle_outcome, world) = idle_of(class::FISHING_SHIP, fishing, computer, &warship(33.4));
    assert_eq!(
        (idle_outcome, walks_to(&world)),
        (Ok(IdleOutcome::Idle), None)
    );
    // a trade cart (class 19) finding nothing
    let (idle_outcome, world) = idle_of(class::TRADE_CART, idle(UnitAiKind::Trade), computer, &[]);
    assert_eq!(idle_outcome, Ok(IdleOutcome::Idle));
    assert!(world.spatial.searched().is_some());
}

/// The trade ship's case 0x202 (`FUN_140c29bf0`) off its trade and explore actions: the
/// target goes, the action stops (its `MoveTo` too), and the processing goes on (2). On those
/// two actions the case is not modelled.
#[test]
fn trade_ship_whose_action_needs_search_stops() {
    let notify = Notify::own(
        object_id(500),
        NotifyKind::REQUIRES_SEARCH,
        [AiAction::Move.value(), 0, 0],
    );
    let walking = Action {
        list: ActionList::default().with_move_to(Some(MoveToAction {
            point: Position::new(30.0, 30.0, 0.0),
            ..MoveToAction::sub_in(MoveTo::Moving)
        })),
        ..Action::default()
    };
    let trading = |action| UnitAi {
        ai_kind: UnitAiKind::Trade,
        current_action: action,
        current_target: Some(object_id(12)),
        ..ship_ai()
    };
    let mut game = transport_world(
        trading(Some(AiAction::Move)),
        Some(walking),
        PlayerKind::Human,
    );
    assert_eq!(hears(&mut game, (player_id(1), 500), notify), Ok(GO_ON));
    let unit_ai = ai_500(&game);
    assert_eq!(
        (unit_ai.current_action, unit_ai.current_target),
        (None, None)
    );
    assert_eq!(
        game.parts.actions.of(object_id(500)).top_action_kind(),
        None
    );
    for action in [AiAction::Trade, AiAction::Explore].map(Some) {
        let mut world = transport_world(trading(action), None, PlayerKind::Human);
        assert!(hears(&mut world, (player_id(1), 500), notify).is_err());
    }
}

/// The trade ship's damage notify (`FUN_140c29bf0`): an attacker it knows raises no alert
/// and no call for help; without an order, off the move action, the action stops (the last
/// action the stopped one) and it runs from its attackers (3); on the move action it runs on
/// (no stop: the move stays the last action's successor).
#[test]
fn trade_ship_stops_and_runs_from_its_attackers() {
    let notify = Notify::own(object_id(77), NotifyKind::DAMAGED, [77, 40, 0]);
    let trade = |action| UnitAi {
        ai_kind: UnitAiKind::Trade,
        current_action: Some(action),
        ..ship_ai()
    };
    let (result, world) = ship_hears(notify, trade(AiAction::Gather), None, PlayerKind::Human);
    assert_eq!(
        (result, walks_to(&world)),
        (Ok(BREAK), Some(Position::new(18.0, 30.0, 0.0)))
    );
    let unit_ai = world.parts.unit_ai.get(object_id(500)).unwrap();
    assert_eq!(
        (
            unit_ai.current_action,
            unit_ai.last_action,
            unit_ai.orders.len()
        ),
        (Some(AiAction::Move), None, 0)
    );
    let (result, world) = ship_hears(notify, trade(AiAction::Move), None, PlayerKind::Human);
    assert_eq!(
        (result, walks_to(&world)),
        (Ok(BREAK), Some(Position::new(18.0, 30.0, 0.0)))
    );
    let unit_ai = world.parts.unit_ai.get(object_id(500)).unwrap();
    assert_eq!(
        (unit_ai.current_action, unit_ai.last_action),
        (Some(AiAction::Move), Some(AiAction::Move))
    );
}

/// The fishing ship's damage notify (`FUN_140c25ca0`): on the move action it neither stops
/// nor runs (3, nothing); off it, it stops and runs.
#[test]
fn fishing_ship_on_the_move_takes_its_damage_without_running() {
    let notify = Notify::own(object_id(77), NotifyKind::DAMAGED, [77, 40, 0]);
    let fishing = |action| UnitAi {
        ai_kind: UnitAiKind::Fishing,
        current_action: Some(action),
        ..ship_ai()
    };
    let (result, world) = ship_hears(notify, fishing(AiAction::Move), None, PlayerKind::Human);
    assert_eq!((result, walks_to(&world)), (Ok(BREAK), None));
    let (result, world) = ship_hears(notify, fishing(AiAction::Gather), None, PlayerKind::Human);
    assert_eq!(
        (result, walks_to(&world)),
        (Ok(BREAK), Some(Position::new(18.0, 30.0, 0.0)))
    );
}

/// The fishing ship's builder search (`FUN_140c25ca0`, case 0x202 on its build): its own
/// foundations (relation 3, state 0) of the classes 3, 27, 39, 52 and 49, in that order;
/// the nearest by `ClosestObject`'s distance (a tie keeps the first) that it reaches it
/// builds (3). Finding none, a human player's ship goes on (2); a computer player's posts
/// itself its build's completion: the build of foundation 42, the nearest of three (a
/// search for state 0 passes only a farm that has workers, `ClosestObject`'s unbuilt farm
/// test).
#[test]
fn fishing_ship_builds_the_nearest_foundation_it_reaches() {
    let builder = UnitAi {
        current_action: Some(AiAction::Build),
        ..idle(UnitAiKind::Fishing)
    };
    let notify = Notify::own(
        object_id(500),
        NotifyKind::REQUIRES_SEARCH,
        [AiAction::Build.value(), 0, 0],
    );
    let site = |id: i32, class, at| SceneObject {
        desolid: true,
        state: ObjectState::Foundation,
        seen_by: Some(VisionBits::visible(player_id(1))),
        ..SceneObject::new(
            id,
            player_id(1),
            Master {
                kind: MasterKind::BUILDING,
                radius: Footprint::new(1.0, 1.0),
                ..walker(i16::try_from(id).expect("a master's id"), class)
            },
            at,
        )
    };
    let ship = Master {
        line_of_sight: Distance::new(4.0),
        action: Some(ActionPart {
            tasks: vec![build_task()],
            ..ActionPart::default()
        }),
        ..walker(4, class::FISHING_SHIP)
    };
    let hear = |kind, sites: Vec<SceneObject>| {
        let mut scene = Scene::open(GridSize::new(64, 64))
            .kind(player_id(1), kind)
            .object(SceneObject {
                ai: Some(builder.clone()),
                ..SceneObject::new(
                    500,
                    player_id(1),
                    ship.clone(),
                    Position::new(20.0, 30.0, 0.0),
                )
            });
        for scene_object in sites {
            scene = scene.object(scene_object);
        }
        let mut build = scene.build();
        let run_ai = run_ai(
            &mut build,
            (player_id(1), object_id(500)),
            |ai, at, side, kind, rng| ai.handle(notify, Millis::ZERO, at, kind, rng, side),
        );
        (run_ai, build)
    };
    let at = |x, y| Position::new(x, y, 0.0);
    // the gaps 6.5, 3.5 and 3.5: the distances 6, 3 and 3
    let sites = vec![
        site(41, class::BUILDING, at(27.7, 30.0)),
        site(42, class::WALL, at(24.7, 30.0)),
        site(43, class::GATE, at(15.3, 30.0)),
    ];
    let (result, world) = hear(PlayerKind::Human, sites);
    assert_eq!(result, Ok(BREAK));
    assert_eq!(
        world
            .parts
            .actions
            .of(object_id(500))
            .list
            .build()
            .map(|build_action| build_action.target),
        Some(Some(FoundationId::new(object_id(42))))
    );
    // none found: the human's goes on, the computer's posts its completion
    let (result, world) = hear(PlayerKind::Human, Vec::new());
    assert_eq!(result, Ok(GO_ON));
    assert!(
        world
            .parts
            .actions
            .of(object_id(500))
            .list
            .build()
            .is_none()
    );
    let unit_ai = world.parts.unit_ai.get(object_id(500)).unwrap();
    assert_eq!((unit_ai.buffered.len(), unit_ai.notifies.len()), (0, 0));
    let (result, world) = hear(PlayerKind::Computer, Vec::new());
    assert_eq!(result, Ok(GO_ON));
    assert_eq!(
        world.parts.unit_ai.get(object_id(500)).unwrap().notifies,
        vec![Notify::own(
            object_id(500),
            NotifyKind::ACTION_COMPLETED,
            [AiAction::Build.value(), 0, 0]
        )]
    );
}

/// A siege unit of player 1 (a mangonel: class 13, master 280, range 7, radii 0.6) at its
/// state position.
const MANGONEL: Attacker = Attacker {
    ai_kind: UnitAiKind::Siege,
    player: PlayerId::new(1),
    kind: MasterKind::COMBAT,
    class: Some(class::SIEGE),
    master: MasterId::named(280),
    missile: Some(MasterId::named(656)),
    range: Distance::new(7.0),
    los: Distance::new(9.0),
    position: Position::new(20.0, 20.0, 0.0),
    radius: Footprint::new(0.6, 0.6),
    speed: Speed::new(0.6),
    ..TC
};

/// `FUN_14120e4e0` for a type-70 attacker: priority 3 spares every class but its six; a ranged
/// attacker with a blast whose level has neither `SPARES_FRIENDS` nor `SKIPS_FRIEND_CHECK` would
/// scan the blast for friends (not modelled); a melee attacker, one without a blast, or a blast
/// level with either bit spares nobody.
#[test]
fn an_attacker_spares_by_its_priority_and_its_blast() {
    let villager = Target {
        class: class::VILLAGER,
        ..BOAR
    };
    let tower = Target {
        class: class::TOWER,
        ..BOAR
    };
    let priority_3 = Attacker {
        attack_priority: AttackPriority::PRIORITY_3,
        ..MANGONEL
    };
    assert_eq!(spares_friends(&priority_3, &villager, &roles()), Ok(false));
    assert_eq!(spares_friends(&priority_3, &tower, &roles()), Ok(true));
    let blast = Attacker {
        area_effect: Distance::new(1.0),
        ..MANGONEL
    };
    assert!(spares_friends(&blast, &villager, &roles()).is_err());
    for level in [4, 8] {
        let attacker = Attacker {
            blast_level: BlastLevel::from_bits_retain(level),
            ..blast
        };
        assert_eq!(spares_friends(&attacker, &villager, &roles()), Ok(true));
    }
    let melee = Attacker {
        range: Distance::new(1.0),
        ..blast
    };
    assert_eq!(spares_friends(&melee, &villager, &roles()), Ok(true));
    assert_eq!(spares_friends(&MANGONEL, &villager, &roles()), Ok(true));
}

/// The siege unit's attack check (`FUN_140c19190`): never classes 1 and 42 nor in
/// `NoAttack`; standing its ground within its range of its state position less its own
/// diagonal (`powf`, `sqrtf`), a Gaia target's range 1.5 longer; defending within the range
/// plus 5; a Gaia unit only taking owners by sight; never from below state 2, a human
/// player's packed unit (unless any) or while packing; by priority 0 a ram only at siege and
/// buildings, a trebuchet only at buildings; priority 3 only its classes.
#[test]
fn siege_unit_attacks_by_its_stance_its_class_and_its_priority() {
    let villager = |x: f32| Target {
        owner: PlayerId::new(2),
        class: class::VILLAGER,
        master: ids::VILLAGER_MALE,
        position: Position::new(x, 20.0, 0.0),
        radius: Footprint::new(0.2, 0.2),
        ..BOAR
    };
    let seeker = |stance| Seeker {
        stance,
        state_position: Point::new(20.0, 20.0),
        ..IDLE
    };
    let check = |me: &Attacker, seeker: &Seeker, target: &Target| {
        siege_can_attack(
            me,
            seeker,
            target,
            TargetRules::Own,
            &roles(),
            &tuning().unit_ai,
        )
    };
    assert_eq!(
        check(&MANGONEL, &seeker(UnitStance::Aggressive), &villager(40.0)),
        Ok(true)
    );
    assert_eq!(
        check(&MANGONEL, &seeker(UnitStance::NoAttack), &villager(21.0)),
        Ok(false)
    );
    for class in [1, 42].map(ClassId::of_master) {
        let target = Target {
            class,
            ..villager(21.0)
        };
        assert_eq!(
            check(&MANGONEL, &seeker(UnitStance::Aggressive), &target),
            Ok(false)
        );
    }
    // the diagonal: sqrt(0.36 + 0.36) = 0.8485; a gap of 7.8 - 0.8485 is within 7
    let diagonal = sqrt(0.6f32 * 0.6 + 0.6 * 0.6);
    let x = 20.0 + 0.2 + 7.0 + diagonal;
    assert_eq!(
        check(
            &MANGONEL,
            &seeker(UnitStance::StandGround),
            &villager(x - 0.01)
        ),
        Ok(true)
    );
    assert_eq!(
        check(
            &MANGONEL,
            &seeker(UnitStance::StandGround),
            &villager(x + 0.01)
        ),
        Ok(false)
    );
    let gaia = Target {
        owner: PlayerId::new(0),
        ..villager(x + 1.4)
    };
    assert_eq!(
        check(&MANGONEL, &seeker(UnitStance::StandGround), &gaia),
        Ok(true)
    );
    let defend = seeker(UnitStance::Defensive);
    assert_eq!(check(&MANGONEL, &defend, &villager(x + 4.99)), Ok(true));
    assert_eq!(check(&MANGONEL, &defend, &villager(x + 5.01)), Ok(false));
    let patrolling = Seeker {
        order: Some(OrderKind::Patrol),
        ..defend
    };
    assert!(check(&MANGONEL, &patrolling, &villager(30.0)).is_err());
    // Gaia's unit, a low state, a packed unit, a packing one
    let gaia_unit = Attacker {
        player: PlayerId::new(0),
        capture: CaptureKind::BySight,
        ..MANGONEL
    };
    assert_eq!(
        check(&gaia_unit, &seeker(UnitStance::Aggressive), &villager(30.0)),
        Ok(false)
    );
    let gaia_taker = Attacker {
        capture: CaptureKind::None,
        ..gaia_unit
    };
    assert_eq!(
        check(
            &gaia_taker,
            &seeker(UnitStance::Aggressive),
            &villager(30.0)
        ),
        Ok(true)
    );
    let low = Attacker {
        state: ObjectState::Unfinished,
        ..MANGONEL
    };
    assert_eq!(
        check(&low, &seeker(UnitStance::Aggressive), &villager(30.0)),
        Ok(false)
    );
    let packed = Attacker {
        class: Some(class::CLASS_51),
        ..MANGONEL
    };
    assert_eq!(
        check(&packed, &seeker(UnitStance::Aggressive), &villager(30.0)),
        Ok(false)
    );
    assert_eq!(
        siege_can_attack(
            &packed,
            &seeker(UnitStance::Aggressive),
            &villager(30.0),
            TargetRules::Any,
            &roles(),
            &tuning().unit_ai
        ),
        Ok(true)
    );
    let packing = Attacker {
        top_action: Some(ActionKind::PACK),
        ..MANGONEL
    };
    assert_eq!(
        check(&packing, &seeker(UnitStance::Aggressive), &villager(30.0)),
        Ok(false)
    );
    // a ram attacks buildings and siege, a trebuchet buildings only
    let ram = Attacker {
        master: ids::ID_35,
        ..MANGONEL
    };
    let siege = Target {
        class: class::SIEGE,
        ..villager(30.0)
    };
    assert_eq!(
        check(&ram, &seeker(UnitStance::Aggressive), &villager(30.0)),
        Ok(false)
    );
    assert_eq!(
        check(&ram, &seeker(UnitStance::Aggressive), &siege),
        Ok(true)
    );
    let trebuchet = Attacker {
        class: Some(class::CLASS_54),
        ..MANGONEL
    };
    assert_eq!(
        check(&trebuchet, &seeker(UnitStance::Aggressive), &siege),
        Ok(false)
    );
    let house = Target {
        class: class::BUILDING,
        kind: MasterKind::BUILDING,
        ..villager(30.0)
    };
    assert_eq!(
        check(&trebuchet, &seeker(UnitStance::Aggressive), &house),
        Ok(true)
    );
    // masters 1744 and 1746 on a building: priority 1 or 4 and up only
    let elephant = |attack_priority| Attacker {
        master: ids::ID_1744,
        attack_priority: AttackPriority::of_byte(attack_priority),
        ..MANGONEL
    };
    let on_building = Seeker {
        target: Some(object_id(900)),
        target_master_kind: Some(MasterKind::BUILDING),
        ..seeker(UnitStance::Aggressive)
    };
    for (priority, attacks) in [(0, false), (1, true), (2, false), (3, false), (4, true)] {
        assert_eq!(
            check(&elephant(priority), &on_building, &villager(30.0)),
            Ok(attacks),
            "priority {priority}"
        );
    }
    let priority_3 = Attacker {
        attack_priority: AttackPriority::PRIORITY_3,
        ..MANGONEL
    };
    assert_eq!(
        check(
            &priority_3,
            &seeker(UnitStance::Aggressive),
            &villager(30.0)
        ),
        Ok(false)
    );
    assert_eq!(
        check(&priority_3, &seeker(UnitStance::Aggressive), &house),
        Ok(true)
    );
    // its neutral test spares the trade cart's class no more
    assert!(siege_attacks_neutral(
        AttackPriority::PRIORITY_0,
        class::TRADE_CART,
        &roles()
    ));
    assert!(!soldier_attacks_neutral(
        AttackPriority::PRIORITY_0,
        class::TRADE_CART,
        &roles()
    ));
    assert!(!siege_attacks_neutral(
        AttackPriority::PRIORITY_0,
        class::VILLAGER,
        &roles()
    ));
}

/// The mangonel of the siege tests: master 280 of class 13, line of sight 9, weapon range 7,
/// radius 0.6, speed 0.6, standing on [`STANDING`].
fn mangonel() -> Master {
    let walk = walker(280, class::SIEGE);
    Master {
        radius: Footprint::new(0.6, 0.6),
        line_of_sight: Distance::new(9.0),
        standing_sprite: Some(STANDING.id),
        move_stats: Some(MoveStats {
            radius: Radii::new(0.6, 0.6, 0.5),
            ..walking(Speed::new(0.6))
        }),
        combat: Some(CombatPart {
            weapon_range: Distance::new(7.0),
            ..CombatPart::default()
        }),
        ..walk
    }
}

/// Mangonel 500 of `owner` at (20, 20) ([`mangonel`]) on its sprite, its unit AI `ai`.
fn mangonel_unit(owner: PlayerId, ai: UnitAi) -> SceneObject {
    SceneObject {
        ai: Some(ai),
        sprite: Some(STANDING.id),
        ..SceneObject::new(500, owner, mangonel(), Position::new(20.0, 20.0, 0.0))
    }
}

/// A map of 64 by 64 with the mangonel `siege` on it ([`mangonel_unit`]) and player 2's
/// villagers `targets` (id, place), each in the mangonel's owner's lists; water on `water`.
fn siege_world(siege: SceneObject, targets: &[(i32, Point)], water: &[Tile]) -> World {
    let owner = siege.player;
    let mut scene = Scene::open(GridSize::new(64, 64))
        .players(2)
        .graphic(STANDING)
        .water(water.iter().copied())
        .object(siege);
    for &(id, at) in targets {
        scene = scene.object(SceneObject {
            seen_by: Some(VisionBits::visible(owner)),
            ai: Some(idle(UnitAiKind::Civilian)),
            ..SceneObject::new(
                id,
                player_id(2),
                villager_master(),
                Position::new(at.x(), at.y(), 0.0),
            )
        });
    }
    scene.build()
}

/// The target of mangonel 500's Attack in `world`, none without one.
fn siege_attacks(world: &World) -> Option<ObjectId> {
    world
        .parts
        .actions
        .of(object_id(500))
        .list
        .attack()
        .and_then(|attack| attack.target)
        .map(AttackTargetId::id)
}

/// The siege unit's idle check (`FUN_140c206b0`): it caps the last human order time, finds a
/// target with `bestUnitToAttack` (villager 77 within its range) and attacks it (forced: its
/// Attack at once), the order going (6); finding nothing, a unit in a group gives 5 and one
/// defending walks back to its state position. Gaia's that takes no owners by sight does
/// nothing and searches nothing.
#[test]
fn siege_unit_attacks_what_its_idle_search_finds() {
    let one = player_id(1);
    let idle_of = |world: &mut World, owner: PlayerId| {
        run_ai(world, (owner, object_id(500)), |ai, at, side, kind, rng| {
            ai.process_idle(at, kind, rng, side)
        })
    };
    let unit_ai = UnitAi {
        current_order: Some(OrderKind::Move),
        ..idle(UnitAiKind::Siege)
    };
    let mut world = siege_world(
        mangonel_unit(one, unit_ai),
        &[(77, Point::new(25.0, 20.0))],
        &[],
    );
    assert_eq!(idle_of(&mut world, one), Ok(IdleOutcome::Acted));
    assert_eq!(siege_attacks(&world), Some(object_id(77)));
    let unit_ai = world.parts.unit_ai.get(object_id(500)).unwrap();
    assert_eq!(
        (
            unit_ai.current_action,
            unit_ai.current_target,
            unit_ai.current_order
        ),
        (Some(AiAction::Attack), Some(object_id(77)), None)
    );
    // nothing found: in a group 5 and no walk; defending without a group walks back
    let defending = UnitAi {
        stance: UnitStance::Defensive,
        state_position: Point::new(10.0, 20.0),
        ..idle(UnitAiKind::Siege)
    };
    let mut world = siege_world(mangonel_unit(one, defending.clone()), &[], &[]);
    world.groups.groups.push(UnitGroup {
        id: group_id(7),
        owner: Some(player_id(1)),
        members: vec![Member {
            id: object_id(500),
            ..Member::default()
        }],
        ..UnitGroup::default()
    });
    world.players[1]
        .objects
        .by_id_mut(object_id(500))
        .unwrap()
        .group
        .id = Some(group_id(7));
    assert_eq!(idle_of(&mut world, one), Ok(IdleOutcome::Idle));
    assert_eq!(walks_to(&world), None);
    let mut world = siege_world(mangonel_unit(one, defending), &[], &[]);
    assert_eq!(idle_of(&mut world, one), Ok(IdleOutcome::Idle));
    assert!(walks_to(&world).is_some_and(|position| position.x() < 20.0));
    // Gaia's that does not take owners by sight (its seer's flag)
    let gaia = SceneObject {
        capture: CaptureKind::BySight,
        seen_by: Some(VisionBits::empty()),
        ..mangonel_unit(PlayerId::GAIA, idle(UnitAiKind::Siege))
    };
    let mut world = siege_world(gaia, &[(77, Point::new(25.0, 20.0))], &[]);
    assert_eq!(idle_of(&mut world, PlayerId::GAIA), Ok(IdleOutcome::Idle));
    assert!(world.spatial.searched().is_none());
}

/// With retargeting off and its current target held, a top target the siege unit reaches
/// within its range ends the check (5). Not reached, retargeting comes back on and the unit
/// attacks it (forced: its Attack replaces its walk): the mangonel stands at a lake's corner,
/// its target on an island within its range by the footprints' gap, but every land tile the
/// mangonel walks to lies beyond the planner's range (the radii and the weapon range from the
/// target's centre).
#[test]
fn siege_unit_with_retargeting_off_keeps_reachable_top_target() {
    let unit_ai = UnitAi {
        retarget_disabled: 1,
        current_target: Some(object_id(88)),
        ..idle(UnitAiKind::Siege)
    };
    let walk = Action {
        list: ActionList::default().with_move_to(Some(MoveToAction {
            target: Some(object_id(88)),
            point: Position::new(14.3, 14.3, 0.0),
            ..MoveToAction::sub_in(MoveTo::Moving)
        })),
        ..Action::default()
    };
    let check = |water: &[Tile]| {
        let siege = SceneObject {
            actions: Some(walk.clone()),
            ..mangonel_unit(player_id(1), unit_ai.clone())
        };
        let mut world = siege_world(siege, &[(88, Point::new(14.3, 14.3))], water);
        let idle_outcome = run_ai(
            &mut world,
            (player_id(1), object_id(500)),
            |ai, at, side, kind, rng| ai.process_idle(at, kind, rng, side),
        );
        let disabled = world
            .parts
            .unit_ai
            .get(object_id(500))
            .unwrap()
            .retarget_disabled;
        (idle_outcome, disabled, siege_attacks(&world))
    };
    assert_eq!(check(&[]), (Ok(IdleOutcome::Idle), 1, None));
    let lake: Vec<Tile> = (0..64)
        .flat_map(|y| (0..64).map(move |x| Tile::new(x, y)))
        .filter(|tile| (tile.x() < 20 || tile.y() < 20) && *tile != Tile::new(14, 14))
        .collect();
    assert_eq!(
        check(&lake),
        (Ok(IdleOutcome::Acted), 0, Some(object_id(88)))
    );
}

/// The siege unit's handler (`FUN_140c287e0`): a target under its least range makes it back
/// up (not modelled); a human player's executed action and damage are the base handler's; a
/// computer player's executed attack draws whether it keeps its distance, but not on a
/// building target; its run: a top target (its walk to villager 77) keeps it.
#[test]
fn siege_unit_hands_its_notifies_to_the_base_handler() {
    let hear = |ai: UnitAi, notify: Notify, kind: PlayerKind| {
        let mut world = siege_world(
            mangonel_unit(player_id(1), ai),
            &[(77, Point::new(25.0, 20.0))],
            &[],
        );
        world.players[1].kind = kind;
        let result = run_ai(
            &mut world,
            (player_id(1), object_id(500)),
            |ai, at, side, kind, rng| ai.handle(notify, Millis::ZERO, at, kind, rng, side),
        );
        (result, world.streams.get_mut(Stream::Ai).draws)
    };
    let unit_ai = idle(UnitAiKind::Siege);
    let under = Notify::own(object_id(500), NotifyKind::TOO_CLOSE, [77, 0, 0]);
    assert!(hear(unit_ai.clone(), under, PlayerKind::Human).0.is_err());
    let executed = Notify::own(object_id(500), NotifyKind::ACTION_EXECUTED, [600, 77, 0]);
    assert_eq!(
        hear(unit_ai.clone(), executed, PlayerKind::Human).0,
        Ok(GO_ON)
    );
    // a computer player's, off the attack action: one draw, then 2
    assert_eq!(
        hear(unit_ai.clone(), executed, PlayerKind::Computer),
        (Ok(GO_ON), 1)
    );
    // a building target: no draw, the base handler's
    let on_house = UnitAi {
        target_class: class_of(3),
        ..idle(UnitAiKind::Siege)
    };
    assert_eq!(
        hear(on_house, executed, PlayerKind::Computer),
        (Ok(GO_ON), 0)
    );
    // its run: a top target keeps it
    let walk = Action {
        list: ActionList::default().with_move_to(Some(MoveToAction {
            target: Some(object_id(77)),
            point: Position::new(25.0, 20.0, 0.0),
            ..MoveToAction::sub_in(MoveTo::Moving)
        })),
        ..Action::default()
    };
    let pinned = SceneObject {
        actions: Some(walk),
        ..mangonel_unit(player_id(1), unit_ai)
    };
    let mut world = siege_world(pinned, &[(77, Point::new(25.0, 20.0))], &[]);
    let answer = run_ai(
        &mut world,
        (player_id(1), object_id(500)),
        |ai, at, side, kind, rng| ai.run_away(OrderForce::Forced, at, kind, rng, side),
    );
    assert_eq!(answer, Ok(Answer::NotTaken));
    assert_eq!(walks_to(&world), Some(Position::new(25.0, 20.0, 0.0)));
}

/// The priest's slot 73 (`FUN_140c21f30`): the base's retarget check (a used timer redrawn: one
/// draw of the AI stream), but on the order on a target and for a computer player's priest
/// none.
#[test]
fn priest_retargets_but_on_an_order_on_target() {
    let priest = |order| UnitAi {
        current_order: order,
        retarget_timer: Millis::new(900),
        last_retarget_time: WorldTime::ZERO,
        adjusted_idle_timeout: Millis::new(9000),
        look_around_timeout: Millis::new(9000),
        ..idle(UnitAiKind::Priest)
    };
    let monk = Master {
        standing_sprite: Some(STANDING.id),
        ..walker(125, class::MONK)
    };
    for (order, player_kind, draws) in [
        (None, PlayerKind::Human, 1),
        (Some(OrderKind::OnTarget), PlayerKind::Human, 0),
        (None, PlayerKind::Computer, 0),
    ] {
        let mut world = order_world(order_unit(monk.clone(), priest(order)), &[]);
        world.players[1].kind = player_kind;
        let before = world.streams.get_mut(Stream::Ai).draws;
        let _ = run_ai(
            &mut world,
            (player_id(1), object_id(9)),
            |ai, at, side, kind, rng| {
                ai.update_tail(
                    UpdateTail {
                        retarget: Retarget::Priest,
                        world_time: WorldTime::from_millis(10_000),
                        player_kind: kind,
                        busy: false,
                        group: None,
                    },
                    at,
                    rng,
                    side,
                )
            },
        );
        assert_eq!(
            world.streams.get_mut(Stream::Ai).draws - before,
            draws,
            "order {order:?}, computer {player_kind}"
        );
    }
    assert!(matches!(
        retarget_kind(UnitAiKind::Priest),
        Ok(Retarget::Priest)
    ));
}

/// A target the tail lets be: a player's, targetable and attackable, outside a pasture.
const PLAIN: TargetFlags = TargetFlags {
    owner: PlayerId::new(2),
    kind: MasterKind::COMBAT,
    capture: CaptureKind::None,
    pasture: None,
    untargetable: false,
    attackable: true,
};

fn on_target(action: Option<AiAction>) -> UnitAi {
    UnitAi {
        current_action: action,
        current_target: Some(object_id(500)),
        target_class: class_of(6),
        target_point: Position::new(10.0, 10.0, 0.0),
        ..idle(UnitAiKind::Lion)
    }
}

/// Object `id` at `at` as `flags` make it: their owner, its master's kind (master `id`, a
/// walker of class 6), its flag bytes, its capture flag and its pasture.
fn target_object(id: i32, flags: TargetFlags, at: Position) -> SceneObject {
    let mut bits = UnitFlags::empty();
    bits.set(UnitFlags::UNTARGETABLE, flags.untargetable);
    bits.set(UnitFlags::ATTACKABLE, flags.attackable);
    let master = Master {
        kind: flags.kind,
        ..walker(i16::try_from(id).expect("a master's id"), class::CLASS_6)
    };
    SceneObject {
        flags: bits,
        capture: flags.capture,
        pasture: flags.pasture,
        ..SceneObject::new(id, flags.owner, master, at)
    }
}

/// Unit 40 of `player` at (14, 10), a walker of class 6 (master 4) whose `MoveTo` follows
/// object `follows` (none: no action), its unit AI [`on_target`]'s with `action`; object 500
/// at (10, 10) as `flags` make it ([`target_object`]).
fn tail_scene(
    player: PlayerId,
    action: Option<AiAction>,
    flags: TargetFlags,
    follows: Option<i32>,
) -> Scene {
    let follow = |id| Action {
        list: ActionList::default().with_move_to(Some(MoveToAction {
            target: Some(object_id(id)),
            point: Position::new(10.0, 10.0, 0.0),
            ..MoveToAction::sub_in(MoveTo::Moving)
        })),
        ..Action::default()
    };
    let unit = SceneObject {
        ai: Some(on_target(action)),
        actions: follows.map(follow),
        ..SceneObject::new(
            40,
            player,
            walker(4, class::CLASS_6),
            Position::new(14.0, 10.0, 0.0),
        )
    };
    Scene::open(GridSize::new(64, 64))
        .players(2)
        .object(unit)
        .object(target_object(500, flags, Position::new(10.0, 10.0, 0.0)))
}

/// The tail's checks of the top action's target ([`UnitAi::top_target_tail`]) on
/// [`tail_scene`]'s unit following object 500: the world after them.
fn tail(player: PlayerId, action: Option<AiAction>, flags: TargetFlags) -> World {
    let mut world = tail_scene(player, action, flags, Some(500)).build();
    run_ai(
        &mut world,
        (player, object_id(40)),
        |ai, at, side, _, rng| ai.top_target_tail(at, side, rng),
    )
    .unwrap();
    world
}

/// Whether unit 40 of `world` stopped: no action left.
fn stopped(world: &World) -> bool {
    world.parts.actions.of(object_id(40)).top_action().is_none()
}

/// Unit 40's unit AI in `world`.
fn ai_of(world: &World) -> &UnitAi {
    world.parts.unit_ai.get(object_id(40)).unwrap()
}

#[test]
fn the_tail_stops_gaia_unit_on_gaia_object_others_can_take_again() {
    // 0x140c68519: a Gaia unit, a Gaia target of type 70 or more, its +0x924 2: the target
    // goes (FUN_140c5e1e0) and the action stops (slot 0xb0 with 1)
    let again = TargetFlags {
        owner: PlayerId::new(0),
        capture: CaptureKind::Again,
        ..PLAIN
    };
    let game = tail(PlayerId::new(0), Some(AiAction::Attack), again);
    assert!(stopped(&game));
    let unit_ai = ai_of(&game);
    assert_eq!(
        (unit_ai.current_target, unit_ai.last_target),
        (None, Some(object_id(500)))
    );
    assert_eq!(
        (unit_ai.current_action, unit_ai.last_action),
        (None, Some(AiAction::Attack))
    );
    // a player's unit, a player's target, another capture flag or a type below 70 stays
    for (player, flags) in [
        (1, again),
        (
            0,
            TargetFlags {
                owner: PlayerId::new(1),
                ..again
            },
        ),
        (
            0,
            TargetFlags {
                capture: CaptureKind::BySight,
                ..again
            },
        ),
        (
            0,
            TargetFlags {
                kind: MasterKind::MISSILE,
                ..again
            },
        ),
    ] {
        let world = tail(PlayerId::new(player), Some(AiAction::Attack), flags);
        assert!(!stopped(&world), "{player} {flags:?}");
        assert_eq!(ai_of(&world).current_target, Some(object_id(500)));
    }
}

#[test]
fn the_tail_stops_work_on_target_without_its_targeting_flag() {
    // 0x140c685cf: outside a pasture, a target that cannot be targeted (+0x689 0) stops any
    // action but the trade
    let untargetable = TargetFlags {
        untargetable: true,
        ..PLAIN
    };
    let player = PlayerId::new(1);
    for action in [Some(AiAction::Gather), Some(AiAction::Attack), None] {
        let world = tail(player, action, untargetable);
        assert!(stopped(&world), "{action:?}");
        assert_eq!(ai_of(&world).current_target, None);
    }
    assert!(!stopped(&tail(player, Some(AiAction::Trade), untargetable)));
    // one in a pasture stays
    let penned = TargetFlags {
        pasture: Some(PastureId::new(object_id(77))),
        ..untargetable
    };
    assert!(!stopped(&tail(player, Some(AiAction::Gather), penned)));
    // one that cannot be attacked (+0x68a 0) stops only the attack
    let unattackable = TargetFlags {
        attackable: false,
        ..PLAIN
    };
    assert!(!stopped(&tail(
        player,
        Some(AiAction::Gather),
        unattackable
    )));
    assert!(stopped(&tail(player, Some(AiAction::Attack), unattackable)));
    // both: the second test reads the action value the first stop left (−1), so an
    // untargetable target stops it again, the second stop's last action the first's −1
    let both = TargetFlags {
        owner: PlayerId::new(0),
        capture: CaptureKind::Again,
        untargetable: true,
        ..PLAIN
    };
    let world = tail(PlayerId::new(0), Some(AiAction::Attack), both);
    assert!(stopped(&world));
    let ai = ai_of(&world);
    assert_eq!((ai.current_action, ai.last_action), (None, None));
    // an attackable-only failure after the first stop: the action value is −1, one stop
    let unattackable_again = TargetFlags {
        untargetable: false,
        attackable: false,
        ..both
    };
    let world = tail(PlayerId::new(0), Some(AiAction::Attack), unattackable_again);
    assert!(stopped(&world));
    let unit_ai = ai_of(&world);
    assert_eq!(
        (unit_ai.current_action, unit_ai.last_action),
        (None, Some(AiAction::Attack))
    );
}

/// The tail reads the top target as the update's calls left it: they ran at once on the unit
/// (here one call, then the tail).
#[test]
fn the_tail_reads_the_top_target_the_update_commands_left() {
    let untargetable = TargetFlags {
        untargetable: true,
        ..PLAIN
    };
    let player = PlayerId::new(1);
    let after = |action, follows: Option<i32>, pending: Option<Effect>| {
        let mut tail_scene = tail_scene(player, action, untargetable, follows)
            .object(target_object(
                600,
                untargetable,
                Position::new(12.0, 10.0, 0.0),
            ))
            .build();
        run_ai(
            &mut tail_scene,
            (player, object_id(40)),
            |ai, at, side, _, rng| {
                if let Some(effect) = pending {
                    side.effect_now(effect, ai, rng)?;
                }
                ai.top_target_tail(at, side, rng)
            },
        )
        .unwrap();
        tail_scene
    };
    // a move to a point, a stop or an explore left no object to test
    for pending in [
        Effect::Stop,
        Effect::Explore(Point::new(1.0, 2.0)),
        Effect::Move(AiMove {
            point: Position::new(1.0, 2.0, 0.0),
            range: Distance::ZERO,
            force: OrderForce::Forced,
            pace: Pace::Run,
        }),
    ] {
        let pending_world = after(Some(AiAction::Gather), None, Some(pending));
        assert_eq!(
            ai_of(&pending_world).current_action,
            Some(AiAction::Gather),
            "{pending:?}"
        );
    }
    // a move to an object holds it: the tail stops the unit on its untargetable target
    let to = Effect::MoveToObject {
        target: object_id(600),
        state: ObjectState::Alive,
        range: Distance::new(2.0),
    };
    let game = after(Some(AiAction::Move), None, Some(to));
    assert!(stopped(&game));
    assert_eq!(
        (ai_of(&game).current_action, ai_of(&game).current_target),
        (None, None)
    );
    // no top action, or one on an object the world no longer holds: nothing
    for follows in [None, Some(700)] {
        let world = after(Some(AiAction::Gather), follows, None);
        assert_eq!(ai_of(&world).current_action, Some(AiAction::Gather));
    }
}

#[test]
fn queued_builds_on_started_or_gone_foundations_leave_the_queue() {
    // 0x140c686e0: off the build order, each queued build order (the last first) on a
    // foundation the world does not hold or holds out of state 0 goes; other orders stay
    let order = |order: OrderKind, target: i32| QueuedOrder {
        order,
        target: ObjectId::of_field(target),
        ready: true,
        ..QueuedOrder::default()
    };
    let queue = vec![
        order(OrderKind::Build, 1),
        order(OrderKind::Build, 2),
        order(OrderKind::Move, 3),
        order(OrderKind::Build, 4),
        order(OrderKind::Build, 5),
    ];
    let left = |ai: UnitAi| {
        let (house, _) = house_and_tree();
        let at = |x| Position::new(x, 20.0, 0.0);
        let site = |id, x| SceneObject {
            desolid: true,
            state: ObjectState::Foundation,
            ..SceneObject::new(id, player_id(1), house.clone(), at(x))
        };
        let mut world = Scene::open(GridSize::new(64, 64))
            .object(SceneObject {
                ai: Some(ai),
                ..SceneObject::new(40, player_id(1), villager_master(), at(30.0))
            })
            .object(site(1, 10.0))
            .object(SceneObject::new(2, player_id(1), house.clone(), at(14.0)))
            .object(SceneObject::new(3, player_id(1), house.clone(), at(18.0)))
            .object(site(5, 22.0))
            .build();
        run_ai(
            &mut world,
            (player_id(1), object_id(40)),
            |ai, _, side, _, _| ai.drop_started_builds(side),
        )
        .unwrap();
        let orders = &world.parts.unit_ai.get(object_id(40)).unwrap().orders;
        orders
            .iter()
            .map(|queued_order| queued_order.target)
            .collect::<Vec<_>>()
    };
    let unit_ai = UnitAi {
        orders: queue.clone(),
        ..idle(UnitAiKind::Civilian)
    };
    assert_eq!(
        left(unit_ai),
        [Some(object_id(1)), Some(object_id(3)), Some(object_id(5))]
    );
    // on the build order the queue stays
    let unit_ai = UnitAi {
        current_order: Some(OrderKind::Build),
        orders: queue,
        ..idle(UnitAiKind::Civilian)
    };
    assert_eq!(left(unit_ai).len(), 5);
}

/// Slot 73's stand ground leash (0x140c57bc4): the squared gap from the unit's footprint to the
/// state position (4 tiles: 16) against the weapon range plus 0.25. A soldier of radius 0.2 at 4.2
/// tiles with a range of 15 and an action stops, forgets its target and walks back. Within the
/// range plus 0.25, without an action value, moving aside, on a patrol order or in a group that
/// patrols it stays; a gap that is not a number stays too (COMISS, JBE).
#[test]
fn unit_standing_its_ground_that_strayed_with_an_action_stops_and_walks_back() {
    let standing = |action| UnitAi {
        stance: UnitStance::StandGround,
        state_position: Point::new(10.0, 10.0),
        current_action: action,
        current_target: Some(object_id(500)),
        retarget_timer: Millis::new(100_000),
        ..idle(UnitAiKind::Soldier)
    };
    let world_of = |unit_ai: UnitAi, range: f32| {
        let soldier = lone_unit(player_id(1), unit_ai, Position::new(14.2, 10.0, 0.0));
        let master = Master {
            combat: Some(CombatPart {
                weapon_range: Distance::new(range),
                ..CombatPart::default()
            }),
            ..soldier.master
        };
        lone_world(SceneObject { master, ..soldier }, &[])
    };
    let mut world = world_of(standing(Some(AiAction::Attack)), 15.0);
    let (answer, unit_ai) = lone_retarget_check(&mut world);
    assert_eq!(answer, Ok(RetargetCheck::NotDue));
    assert_eq!(
        lone_walks(&world),
        Some((Position::new(10.0, 10.0, 0.0), Distance::new(WALK_BACK)))
    );
    assert_eq!(
        (unit_ai.current_target, unit_ai.current_action),
        (None, Some(AiAction::Move))
    );
    let patrol_group = (GroupCommandKind::MOVE, BVector::ZERO, GroupFlags::PATROL);
    for (unit_ai, range, group) in [
        (standing(Some(AiAction::Attack)), 15.75, None),
        (standing(None), 15.0, None),
        (standing(Some(AiAction::MoveAside)), 15.0, None),
        (standing(Some(AiAction::GroupAside)), 15.0, None),
        (
            UnitAi {
                current_order: Some(OrderKind::Patrol),
                ..standing(Some(AiAction::Attack))
            },
            15.0,
            None,
        ),
        (standing(Some(AiAction::Attack)), 15.0, Some(patrol_group)),
        (
            UnitAi {
                state_position: Point::new(f32::NAN, 10.0),
                ..standing(Some(AiAction::Attack))
            },
            15.0,
            None,
        ),
    ] {
        let action = unit_ai.current_action;
        let mut world = world_of(unit_ai, range);
        if let Some(group) = group {
            join_group(&mut world, group);
        }
        let (answer, unit_ai) = lone_retarget_check(&mut world);
        assert!(answer.is_ok(), "{action:?}");
        assert_eq!(
            (lone_walks(&world), unit_ai.current_target),
            (None, Some(object_id(500))),
            "{action:?} {range} {group:?}"
        );
    }
}

/// `FUN_14087e7c0` and `FUN_140f95e10`: the least distance to the lines through a path's
/// consecutive points (the lines, not the segments); a path of one point answers the largest
/// f32, an empty one is unreachable; a NaN distance is kept until the next one.
#[test]
fn patrol_distance_is_the_least_to_the_lines_through_its_points() {
    assert_eq!(
        line_distance(
            BVector::new(3.0, 0.0, 4.0),
            BVector::ZERO,
            BVector::new(10.0, 0.0, 0.0)
        ),
        Distance::new(4.0)
    );
    // beyond the segment's end the line still counts
    assert_eq!(
        line_distance(
            BVector::new(30.0, 0.0, 4.0),
            BVector::ZERO,
            BVector::new(10.0, 0.0, 0.0)
        ),
        Distance::new(4.0)
    );
    let path = [
        BVector::ZERO,
        BVector::new(10.0, 0.0, 0.0),
        BVector::new(10.0, 0.0, 10.0),
    ];
    assert_eq!(
        patrol_distance(&path, BVector::new(9.0, 0.0, 4.0)),
        Ok(Distance::new(1.0))
    );
    assert_eq!(
        patrol_distance(&path[..1], BVector::new(9.0, 0.0, 4.0)),
        Ok(Distance::new(PATROL_FARTHEST))
    );
    assert!(patrol_distance(&[], BVector::new(9.0, 0.0, 4.0)).is_err());
    // two equal points make a NaN, which the next line replaces
    let doubled = [BVector::ZERO, BVector::ZERO, BVector::new(10.0, 0.0, 0.0)];
    assert_eq!(
        patrol_distance(&doubled, BVector::new(3.0, 0.0, 4.0)),
        Ok(Distance::new(4.0))
    );
    assert!(
        patrol_distance(&doubled[..2], BVector::new(3.0, 0.0, 4.0))
            .unwrap()
            .get()
            .is_nan()
    );
}

/// processMisc's leash with patrol information (`+0x1c0`): from where the unit last moved to
/// (slot 0x520) to the saved patrol's path (`FUN_140f95e10`). Beyond stand ground's 0.5 or
/// the defend stance's 5 the unit stops, forgets its target and walks its patrol on (the
/// saved waypoint, range 0), its order the patrol's (10); within it stand ground goes on to
/// the retarget check (its state position's leash not read), the defend stance ends slot 73
/// (9) before it.
#[test]
fn unit_with_patrol_information_leashes_to_its_patrol_path() {
    let path = PatrolPath {
        path: Path {
            points: vec![BVector::new(0.0, 0.0, 10.0), BVector::new(20.0, 0.0, 10.0)],
            length: Distance::new(20.0),
        },
        flags: PathFlags::LOOPS | PathFlags::BACK_AND_FORTH,
    };
    let unit = |stance| UnitAi {
        stance,
        state_position: Point::new(10.0, 30.0),
        current_action: Some(AiAction::Attack),
        current_target: Some(object_id(500)),
        retarget_timer: Millis::ZERO,
        last_retarget_time: WorldTime::ZERO,
        patrol_path: Some(path.clone()),
        ..idle(UnitAiKind::Soldier)
    };
    // where the unit last moved to: its place, on the path's line, 1 or 6 off it
    let run = |stance, y: f32| {
        let soldier = lone_unit(player_id(1), unit(stance), Position::new(14.0, y, 0.0));
        let master = Master {
            combat: Some(CombatPart {
                weapon_range: Distance::new(15.0),
                ..CombatPart::default()
            }),
            ..soldier.master
        };
        let mut world = lone_world(SceneObject { master, ..soldier }, &[]);
        let answer = run_ai(
            &mut world,
            (player_id(1), object_id(50)),
            |ai, at, side, kind, rng| {
                ai.retarget_check(WorldTime::from_millis(1000), kind, at, rng, side)
            },
        );
        let unit_ai = world.parts.unit_ai.get(object_id(50)).unwrap().clone();
        (answer, unit_ai, lone_walks(&world).map(|(_, range)| range))
    };
    // stand ground within 0.5: no leash, on to the retarget check (the timer runs out)
    let (answer, unit_ai, walk) = run(UnitStance::StandGround, 10.0);
    assert_eq!((answer, walk), (Ok(RetargetCheck::Due), None));
    assert_eq!(unit_ai.last_retarget_time, WorldTime::from_millis(1000));
    // beyond it: stop, the target forgotten, the patrol walked on (10)
    let (answer, unit_ai, walk) = run(UnitStance::StandGround, 11.0);
    assert_eq!(
        (answer, walk),
        (Ok(RetargetCheck::NotDue), Some(Distance::ZERO))
    );
    assert_eq!(
        (
            unit_ai.current_target,
            unit_ai.current_order,
            unit_ai.secondary_timer
        ),
        (None, Some(OrderKind::Patrol), Millis::new(750))
    );
    // the defend stance within 5 ends slot 73 before the retarget check (9)
    let (answer, unit_ai, walk) = run(UnitStance::Defensive, 11.0);
    assert_eq!((answer, walk), (Ok(RetargetCheck::NotDue), None));
    assert_eq!(unit_ai.last_retarget_time, WorldTime::ZERO);
    // beyond 5 it walks the patrol on
    let (answer, _, walk) = run(UnitStance::Defensive, 16.0);
    assert_eq!(
        (answer, walk),
        (Ok(RetargetCheck::NotDue), Some(Distance::ZERO))
    );
}

#[test]
fn busy_villager_on_the_attack_order_runs_its_full_update() {
    // records (6)/426406723's villagers attacking (order 700, action 600, a target), busy: the
    // update runs whole (the civilian's processMisc, the idle part keeping the timer of a
    // player's unit with an action at 0)
    let unit_ai = UnitAi {
        current_order: Some(OrderKind::Attack),
        current_action: Some(AiAction::Attack),
        current_target: Some(object_id(18842)),
        target_class: class_of(10),
        idle_timer: Millis::new(400),
        ..idle(UnitAiKind::Civilian)
    };
    let busy_on_target = busy_on_target(
        (player_id(3), 3),
        class::VILLAGER,
        unit_ai,
        (18842, PlayerId::GAIA, class::PREDATOR),
    );
    let (after_order, world) =
        update_busy(busy_on_target, player_id(3), WorldTime::from_millis(1000));
    assert_eq!(after_order, Ok(None));
    assert_eq!(
        following(&world),
        Some((Position::new(24.0, 20.0, 0.0), Some(object_id(18842))))
    );
    let unit_ai = world.parts.unit_ai.get(object_id(9)).unwrap();
    assert_eq!(unit_ai.idle_timer, Millis::ZERO);
    assert_eq!(
        (
            unit_ai.current_order,
            unit_ai.current_action,
            unit_ai.current_target
        ),
        (
            Some(OrderKind::Attack),
            Some(AiAction::Attack),
            Some(object_id(18842))
        )
    );
}

/// An order of value `order` on `target` at (12.7, 30.2, 0.5) within 3, priority 40, ready.
fn queued(order: OrderKind, target: i32) -> QueuedOrder {
    QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order,
        priority: OrderPriority::of_data(40),
        target: ObjectId::of_field(target),
        target_owner: None,
        point: Position::new(12.7, 30.2, 0.5),
        range: Distance::new(3.0),
        shown: Point::new(12.5, 30.5),
        ready: true,
        target_kind: None,
    }
}

/// Player 1's unit 9 of master `master` at (5, 6), on its sprite, its unit AI `ai`.
fn order_unit(master: Master, ai: UnitAi) -> SceneObject {
    SceneObject {
        ai: Some(ai),
        sprite: Some(STANDING.id),
        ..SceneObject::new(9, player_id(1), master, Position::new(5.0, 6.0, 0.0))
    }
}

/// A map of 100 by 100 with unit 9 (`unit`, [`order_unit`]) on it and the `others`, each in
/// player 1's lists. The AI stream is seeded with 0, as [`rng`] is.
fn order_world(unit: SceneObject, others: &[SceneObject]) -> World {
    order_world_on(&[], unit, others)
}

/// [`order_world`] with water on each of `water`.
fn order_world_on(water: &[Tile], unit: SceneObject, others: &[SceneObject]) -> World {
    let mut scene = Scene::open(GridSize::new(100, 100))
        .players(2)
        .graphic(STANDING)
        .water(water.iter().copied())
        .object(unit);
    for other in others {
        scene = scene.object(SceneObject {
            seen_by: Some(VisionBits::visible(player_id(1))),
            ..other.clone()
        });
    }
    let mut world = scene.build();
    world.streams.get_mut(Stream::Ai).set_seed(0);
    world
}

/// A villager on its sprite whose tasks gather and hunt trees (wood, within 0.5).
fn order_gatherer() -> Master {
    let on_trees = |action_kind| Task {
        action_kind,
        object_group: Some(class::TREE),
        attribute_kind: Some(attribute::WOOD),
        work_range: Distance::new(0.5),
        ..Task::default()
    };
    Master {
        standing_sprite: Some(STANDING.id),
        action: Some(ActionPart {
            tasks: vec![on_trees(GATHER), on_trees(HUNT)],
            ..ActionPart::default()
        }),
        ..villager_master()
    }
}

/// Gaia's tree 70 (master 349) at (10, 20), holding 100 wood.
fn order_tree() -> SceneObject {
    resource(
        (70, 349, class::TREE),
        (MasterKind::TREE, GatherGroup::WOOD),
        (10.0, 20.0),
        (attribute::WOOD, 100.0),
    )
}

/// Unit 9's slot 0x230 on order `queued_order` in `world` ([`UnitAi::process_order`] inside its
/// update).
fn orders(world: &mut World, queued_order: &QueuedOrder) -> GenieResult<OrderOutcome> {
    run_ai(
        world,
        (player_id(1), object_id(9)),
        |ai, at, side, kind, rng| {
            ai.process_order(
                queued_order,
                Some((at, kind)),
                None,
                rng,
                side,
                AiLayer {
                    roles: &at.roles,
                    slots: at.slots,
                    unit_ai_tuning: &at.unit_ai_tuning,
                },
            )
        },
    )
}

/// Unit 9's unit AI in `world`.
fn ai_9(world: &World) -> &UnitAi {
    world.parts.unit_ai.get(object_id(9)).unwrap()
}

/// Unit 9's top action's kind in `world`, none without one.
fn top_9(world: &World) -> Option<ActionKind> {
    world.parts.actions.of(object_id(9)).top_action_kind()
}

/// The target of unit 9's Attack in `world`, none without one.
fn attacks_9(world: &World) -> Option<ObjectId> {
    world
        .parts
        .actions
        .of(object_id(9))
        .list
        .attack()
        .and_then(|attack| attack.target)
        .map(AttackTargetId::id)
}

/// The order tests' soldier: master 4 of class 6, standing on [`STANDING`], weapon range 1.
fn order_soldier() -> Master {
    Master {
        standing_sprite: Some(STANDING.id),
        combat: Some(CombatPart {
            weapon_range: Distance::new(1.0),
            ..CombatPart::default()
        }),
        ..walker(4, class::CLASS_6)
    }
}

/// Object `id` of `owner` and class `class` (master 5 of its class) at (10, 20), idle.
fn order_target(id: i32, owner: PlayerId, class: ClassId) -> SceneObject {
    SceneObject {
        ai: Some(idle(UnitAiKind::Civilian)),
        ..SceneObject::new(id, owner, walker(5, class), Position::new(10.0, 20.0, 0.0))
    }
}

/// The point unit 9's `MoveTo` of `world` walks to and its range, none without one.
fn walk_9(world: &World) -> Option<(Position, Distance)> {
    world
        .parts
        .actions
        .of(object_id(9))
        .list
        .move_to()
        .map(|move_to_action| (move_to_action.point, move_to_action.range))
}

#[test]
fn an_order_not_ready_stays_and_patrol_or_unnamed_orders_take_nothing() {
    let waiting = QueuedOrder {
        ready: false,
        ..queued(OrderKind::Move, -1)
    };
    let mut world = order_world(order_unit(order_soldier(), idle(UnitAiKind::Soldier)), &[]);
    assert_eq!(orders(&mut world, &waiting), Ok(OrderOutcome::NotReady));
    // "I don't know what to do with patrol orders"; the trade's and the hero power's orders,
    // which no case names
    for order in [OrderKind::Patrol, OrderKind::Trade, OrderKind::HeroPower] {
        assert_eq!(
            orders(&mut world, &queued(order, -1)),
            Ok(OrderOutcome::NotTaken),
            "{order:?}"
        );
    }
    assert_eq!(ai_9(&world), &idle(UnitAiKind::Soldier));
    assert_eq!(top_9(&world), None);
}

/// The idle part's patrol branch (`0x140c67f0b`): a patrol or an attack-move order patrols, and
/// so does a unit of a group that patrols, but not on the hero power's order (`0x326`).
#[test]
fn patrolling_group_unit_on_the_hero_power_does_not_patrol() {
    let group = GroupState::PATROL;
    let mut unit_ai = idle(UnitAiKind::Soldier);
    assert!(unit_ai.patrols(Some(group)) && !unit_ai.patrols(None));
    unit_ai.current_order = Some(OrderKind::HeroPower);
    assert!(!unit_ai.patrols(Some(group)));
    unit_ai.current_order = Some(OrderKind::AttackMove);
    assert!(unit_ai.patrols(None));
}

/// The attack order by the unit AI's type: a soldier's is the base's `AttackObject` through
/// its slot 0xa0 (its Attack at once), and a member of a group stops moving
/// (`FUN_140c17550`); a villager hunts an animal (class 9, its Hunt at once) and attacks
/// anything else; a unit of class 18 forgets the target at the tail (its class and point stay)
/// and ends the order, its priority kept. A target the world does not hold is not taken (the
/// real attack command answers no only for such a target).
#[test]
fn the_attack_order_attacks_by_the_unit_ai_kind_and_becomes_the_ai_s() {
    let enemy = order_target(70, player_id(2), class::CLASS_6);
    let mut world = order_world(
        order_unit(order_soldier(), idle(UnitAiKind::Soldier)),
        from_ref(&enemy),
    );
    assert_eq!(
        orders(&mut world, &queued(OrderKind::Attack, 70)),
        Ok(OrderOutcome::Taken)
    );
    assert_eq!(attacks_9(&world), Some(object_id(70)));
    let unit_ai = ai_9(&world);
    assert_eq!(
        (
            unit_ai.current_order,
            unit_ai.order_priority.get(),
            unit_ai.current_action
        ),
        (Some(OrderKind::Attack), 40, Some(AiAction::Attack))
    );
    assert_eq!(
        (
            unit_ai.current_target,
            unit_ai.target_class,
            unit_ai.target_point
        ),
        (
            Some(object_id(70)),
            class_of(6),
            Position::new(10.0, 20.0, 0.0)
        )
    );
    // a group's member walking stops
    let mut world = order_world(
        order_unit(order_soldier(), idle(UnitAiKind::Soldier)),
        from_ref(&enemy),
    );
    world.groups.groups.push(UnitGroup {
        id: group_id(7),
        owner: Some(player_id(1)),
        members: vec![Member {
            id: object_id(9),
            ..Member::default()
        }],
        ..UnitGroup::default()
    });
    world.players[1]
        .objects
        .by_id_mut(object_id(9))
        .unwrap()
        .group
        .id = Some(group_id(7));
    world.parts.mover_mut(object_id(9)).unwrap().velocity = Some(Velocity {
        velocity: BVector::new(0.0, 0.0, 0.5),
        acceleration: BVector::ZERO,
    });
    assert_eq!(
        orders(&mut world, &queued(OrderKind::Attack, 70)),
        Ok(OrderOutcome::Taken)
    );
    assert!(!world.parts.mover_of(object_id(9)).unwrap().is_moving());
    // a villager hunts an animal (class 9) and attacks anything else it may (class 6)
    let hunter = Master {
        standing_sprite: Some(STANDING.id),
        ..gatherer_master()
    };
    let deer = SceneObject {
        at: Position::new(10.0, 20.0, 0.0),
        ..gaia_deer(71, (10.0, 20.0))
    };
    let mut world = order_world(
        order_unit(hunter.clone(), idle(UnitAiKind::Civilian)),
        &[deer],
    );
    assert_eq!(
        orders(&mut world, &queued(OrderKind::Attack, 71)),
        Ok(OrderOutcome::Taken)
    );
    let unit_ai = ai_9(&world);
    assert_eq!(
        (unit_ai.current_order, unit_ai.current_action, top_9(&world)),
        (Some(OrderKind::Attack), Some(AiAction::Hunt), Some(HUNT))
    );
    let other = order_target(72, player_id(2), class::CLASS_6);
    let mut world = order_world(
        order_unit(hunter, idle(UnitAiKind::Civilian)),
        from_ref(&other),
    );
    assert_eq!(
        orders(&mut world, &queued(OrderKind::Attack, 72)),
        Ok(OrderOutcome::Taken)
    );
    assert_eq!(
        (ai_9(&world).current_action, attacks_9(&world)),
        (Some(AiAction::Attack), Some(object_id(72)))
    );
    // a unit of class 18: the tail forgets the target (its class and point stay) and ends the
    // order, the order's priority kept
    let monk = Master {
        standing_sprite: Some(STANDING.id),
        ..walker(125, class::MONK)
    };
    let mut other_world = order_world(order_unit(monk, idle(UnitAiKind::Boar)), from_ref(&other));
    assert_eq!(
        orders(&mut other_world, &queued(OrderKind::Attack, 72)),
        Ok(OrderOutcome::Taken)
    );
    let unit_ai = ai_9(&other_world);
    assert_eq!(
        (
            unit_ai.current_order,
            unit_ai.order_priority.get(),
            unit_ai.current_target,
            unit_ai.target_class
        ),
        (None, 40, None, class_of(6))
    );
    // a target the world does not hold: not taken
    let predator = Master {
        standing_sprite: Some(STANDING.id),
        ..walker(126, class::PREDATOR)
    };
    let mut world = order_world(order_unit(predator, idle(UnitAiKind::Boar)), &[other]);
    assert_eq!(
        orders(&mut world, &queued(OrderKind::Attack, 99)),
        Ok(OrderOutcome::NotTaken)
    );
    let unit_ai = ai_9(&world);
    assert_eq!(
        (unit_ai.current_order, unit_ai.current_action),
        (None, None)
    );
}

/// The defend order keeps near its target (within the order's range 3, its defend target) or,
/// without one, near its point at the unit's height; a target the world does not hold, or a
/// unit of master 128, takes none.
#[test]
fn the_defend_order_keeps_near_its_target_or_its_point() {
    let friend = order_target(70, player_id(1), class::CLASS_6);
    let moving = UnitAi {
        current_action: Some(AiAction::Move),
        ..idle(UnitAiKind::Soldier)
    };
    let mut friend_world = order_world(order_unit(order_soldier(), moving), from_ref(&friend));
    assert_eq!(
        orders(&mut friend_world, &queued(OrderKind::Defend, 70)),
        Ok(OrderOutcome::Taken)
    );
    let unit_ai = ai_9(&friend_world);
    assert_eq!(
        (
            unit_ai.current_order,
            unit_ai.current_action,
            unit_ai.last_action
        ),
        (
            Some(OrderKind::Defend),
            Some(AiAction::Defend),
            Some(AiAction::Move)
        )
    );
    assert_eq!(
        (
            unit_ai.current_target,
            unit_ai.target_class,
            unit_ai.target_point
        ),
        (
            Some(object_id(70)),
            class_of(6),
            Position::new(10.0, 20.0, 0.0)
        )
    );
    assert_eq!(
        (unit_ai.desired_distance, unit_ai.defend_target),
        (Distance::new(3.0), Some(object_id(70)))
    );
    // without a target: the point at the unit's height, no defend target
    let mut world = order_world(
        order_unit(order_soldier(), idle(UnitAiKind::Soldier)),
        from_ref(&friend),
    );
    assert_eq!(
        orders(&mut world, &queued(OrderKind::Defend, -1)),
        Ok(OrderOutcome::Taken)
    );
    let unit_ai = ai_9(&world);
    assert_eq!(
        (
            unit_ai.current_target,
            unit_ai.target_point,
            unit_ai.defend_target
        ),
        (None, Position::new(12.7, 30.2, 0.0), None)
    );
    // a target the world does not hold: not taken
    let mut world = order_world(
        order_unit(order_soldier(), idle(UnitAiKind::Soldier)),
        &[friend],
    );
    assert_eq!(
        orders(&mut world, &queued(OrderKind::Defend, 99)),
        Ok(OrderOutcome::NotTaken)
    );
    // master 128 takes no defend order
    let cart = Master {
        id: ids::TRADE_CART,
        ..order_soldier()
    };
    let mut world = order_world(order_unit(cart, idle(UnitAiKind::Soldier)), &[]);
    assert_eq!(
        orders(&mut world, &queued(OrderKind::Defend, -1)),
        Ok(OrderOutcome::NotTaken)
    );
    assert_eq!(ai_9(&world).current_action, None);
}

/// The heal, conversion and repair orders give the unit their commands: the heal and the
/// convert command are not modelled. The repair command on player 1's damaged house 70 takes
/// it (its Repair at once): the AI holds the order on the house, its class and place, the old
/// target its last. An undamaged house finds no repair task: nothing changes.
#[test]
fn the_heal_conversion_and_repair_orders_give_the_unit_commands() {
    let (house, _) = house_and_tree();
    let target = |hp| SceneObject {
        hp: HitPoints::new(hp),
        ..SceneObject::new(
            70,
            player_id(1),
            house.clone(),
            Position::new(10.0, 20.0, 0.0),
        )
    };
    let monk = Master {
        standing_sprite: Some(STANDING.id),
        ..walker(125, class::MONK)
    };
    for order in [OrderKind::Heal, OrderKind::Convert] {
        let mut world = order_world(
            order_unit(monk.clone(), idle(UnitAiKind::Priest)),
            &[target(300.0)],
        );
        assert_eq!(
            orders(&mut world, &queued(order, 70)),
            Err(UNMODELLED_COMMAND),
            "{order:?}"
        );
    }
    let repairer = Master {
        standing_sprite: Some(STANDING.id),
        action: Some(ActionPart {
            tasks: vec![Task {
                action_kind: REPAIR,
                work_val1: WorkValue::new(0.5),
                ..Task::default()
            }],
            ..ActionPart::default()
        }),
        ..villager_master()
    };
    let working = UnitAi {
        current_target: Some(object_id(5)),
        target_class: class_of(9),
        ..idle(UnitAiKind::Civilian)
    };
    let mut world = order_world(
        order_unit(repairer.clone(), working.clone()),
        &[target(300.0)],
    );
    assert_eq!(
        orders(&mut world, &queued(OrderKind::Repair, 70)),
        Ok(OrderOutcome::Taken)
    );
    let repairs = world
        .parts
        .actions
        .of(object_id(9))
        .list
        .repair()
        .map(|repair_action| repair_action.target);
    assert_eq!(repairs, Some(Some(RepairTargetId::new(object_id(70)))));
    let unit_ai = ai_9(&world);
    assert_eq!(
        (unit_ai.current_order, unit_ai.current_action),
        (Some(OrderKind::Repair), Some(AiAction::Repair))
    );
    assert_eq!(
        (
            unit_ai.current_target,
            unit_ai.target_class,
            unit_ai.target_point
        ),
        (
            Some(object_id(70)),
            class_of(3),
            Position::new(10.0, 20.0, 0.0)
        )
    );
    assert_eq!(
        (unit_ai.last_target, unit_ai.last_target_class),
        (Some(object_id(5)), class_of(9))
    );
    // undamaged: no repair task, nothing changes
    let mut world = order_world(order_unit(repairer, working.clone()), &[target(550.0)]);
    assert_eq!(
        orders(&mut world, &queued(OrderKind::Repair, 70)),
        Ok(OrderOutcome::NotTaken)
    );
    assert_eq!(ai_9(&world), &working);
    assert_eq!(top_9(&world), None);
}

#[test]
fn the_explore_orders_explore_their_point_cut_to_tiles() {
    for (order, action, last) in [
        (OrderKind::Explore, AiAction::Explore, None),
        (OrderKind::Scout, AiAction::Scout, Some(AiAction::Explore)),
        (
            OrderKind::AutoScout,
            AiAction::AutoScout,
            Some(AiAction::Explore),
        ),
    ] {
        let mut world = order_world(order_unit(order_soldier(), idle(UnitAiKind::Soldier)), &[]);
        assert_eq!(
            orders(&mut world, &queued(order, -1)),
            Ok(OrderOutcome::Taken)
        );
        // its Explore at once, to the point cut to its tile at the unit's height
        let explore = world
            .parts
            .actions
            .of(object_id(9))
            .list
            .explore()
            .map(|explore_action| explore_action.point);
        assert_eq!(explore, Some(Position::new(12.0, 30.0, 0.0)), "{order:?}");
        let unit_ai = ai_9(&world);
        assert_eq!(
            (
                unit_ai.current_order,
                unit_ai.current_action,
                unit_ai.last_action
            ),
            (Some(order), Some(action), last)
        );
        assert_eq!(
            unit_ai.target_point,
            Position::new(12.0, 30.0, tuning().unit_ai.explore_z)
        );
    }
    // a point that is not a number cuts to i32::MIN (CVTTSS2SI)
    let queued_order = QueuedOrder {
        point: Position::new(f32::NAN, 1.5, 0.0),
        ..queued(OrderKind::Explore, -1)
    };
    let mut world = order_world(order_unit(order_soldier(), idle(UnitAiKind::Soldier)), &[]);
    let _ = orders(&mut world, &queued_order);
    let explore = world
        .parts
        .actions
        .of(object_id(9))
        .list
        .explore()
        .map(|explore_action| explore_action.point);
    assert_eq!(explore, Some(Position::new(i32::MIN as f32, 1.0, 0.0)));
}

/// The stop order stops the unit (its actions gone at once), ends the order and its target,
/// and takes the unit's place as its state position.
#[test]
fn the_stop_order_stops_and_ends_the_order() {
    let unit_ai = UnitAi {
        current_order: Some(OrderKind::Move),
        order_priority: OrderPriority::of_data(100),
        current_action: Some(AiAction::Move),
        current_target: Some(object_id(70)),
        ..idle(UnitAiKind::Soldier)
    };
    let walk = Action {
        list: ActionList::default().with_move_to(Some(MoveToAction {
            point: Position::new(30.0, 6.0, 0.0),
            ..MoveToAction::sub_in(MoveTo::Moving)
        })),
        ..Action::default()
    };
    let unit = SceneObject {
        actions: Some(walk),
        ..order_unit(order_soldier(), unit_ai)
    };
    let mut world = order_world(unit, &[]);
    assert_eq!(
        orders(&mut world, &queued(OrderKind::Stop, -1)),
        Ok(OrderOutcome::Taken)
    );
    assert_eq!(top_9(&world), None);
    let unit_ai = ai_9(&world);
    assert_eq!(
        (
            unit_ai.current_order,
            unit_ai.order_priority.get(),
            unit_ai.current_action,
            unit_ai.current_target
        ),
        (None, -1, None, None)
    );
    assert_eq!(
        (unit_ai.last_order, unit_ai.last_target),
        (Some(OrderKind::Move), Some(object_id(70)))
    );
    assert_eq!(unit_ai.state_position, Point::new(5.0, 6.0));
}

/// The run-away order without attackers: a random direction (two draws), by twice the speed
/// (0 here: the unit's own place), through `evasiveMoveTo` (a forced, running move: its
/// `MoveTo` at once); standing its ground, a player's unit stays (not taken).
#[test]
fn the_run_away_order_runs_from_the_unit_attackers() {
    let still = Master {
        move_stats: Some(walking(Speed::ZERO)),
        ..order_soldier()
    };
    let mut world = order_world(order_unit(still.clone(), idle(UnitAiKind::Soldier)), &[]);
    let before = world.streams.get_mut(Stream::Ai).draws;
    assert_eq!(
        orders(&mut world, &queued(OrderKind::RunAway, -1)),
        Ok(OrderOutcome::Taken)
    );
    assert_eq!(world.streams.get_mut(Stream::Ai).draws - before, 2);
    assert_eq!(
        walk_9(&world),
        Some((Position::new(5.0, 6.0, 0.0), Distance::ZERO))
    );
    let unit_ai = ai_9(&world);
    assert_eq!(
        (unit_ai.current_order, unit_ai.current_action),
        (Some(OrderKind::RunAway), Some(AiAction::Move))
    );
    let standing = UnitAi {
        stance: UnitStance::StandGround,
        ..idle(UnitAiKind::Soldier)
    };
    let mut world = order_world(order_unit(still, standing), &[]);
    assert_eq!(
        orders(&mut world, &queued(OrderKind::RunAway, -1)),
        Ok(OrderOutcome::NotTaken)
    );
}

/// The gather and hunt orders work an object the world holds: the gather command (its Gather
/// at once) on tree 70; a gone target takes nothing. The hunt stops first, then hunts (its
/// Hunt at once); on a gone target the stop stays. The auto boar luring hunts, then holds
/// 0x2be either way.
#[test]
fn the_gather_and_hunt_orders_work_an_object_the_world_holds() {
    let tree = order_tree();
    let world = |ai: UnitAi| order_world(order_unit(order_gatherer(), ai), from_ref(&tree));
    let gather = OrderKind::Gather;
    let mut game = world(idle(UnitAiKind::Civilian));
    assert_eq!(
        orders(&mut game, &queued(gather, 70)),
        Ok(OrderOutcome::Taken)
    );
    let unit_ai = ai_9(&game);
    assert_eq!(
        (
            unit_ai.current_order,
            unit_ai.current_action,
            unit_ai.current_target,
            unit_ai.target_class,
            top_9(&game)
        ),
        (
            Some(gather),
            Some(AiAction::Gather),
            Some(object_id(70)),
            class_of(15),
            Some(GATHER)
        )
    );
    // gone: the gather command takes nothing
    let mut game = world(idle(UnitAiKind::Civilian));
    assert_eq!(
        orders(&mut game, &queued(gather, 99)),
        Ok(OrderOutcome::NotTaken)
    );
    assert_eq!(top_9(&game), None);
    // the hunt stops first, then hunts; on a gone target the stop stays
    let gathering = UnitAi {
        current_action: Some(AiAction::Gather),
        ..idle(UnitAiKind::Civilian)
    };
    let mut game = world(gathering.clone());
    assert_eq!(
        orders(&mut game, &queued(OrderKind::Hunt, 70)),
        Ok(OrderOutcome::Taken)
    );
    let unit_ai = ai_9(&game);
    assert_eq!(
        (unit_ai.current_order, unit_ai.current_action, top_9(&game)),
        (Some(OrderKind::Hunt), Some(AiAction::Hunt), Some(HUNT))
    );
    let mut gathering_world = world(gathering);
    assert_eq!(
        orders(&mut gathering_world, &queued(OrderKind::Hunt, 99)),
        Ok(OrderOutcome::NotTaken)
    );
    let unit_ai = ai_9(&gathering_world);
    assert_eq!(
        (unit_ai.current_order, unit_ai.current_action),
        (None, None)
    );
    // the auto boar luring hunts, then holds 0x2be either way
    let luring = OrderKind::AutoBoarLuring;
    let mut game = world(idle(UnitAiKind::Civilian));
    assert_eq!(
        orders(&mut game, &queued(luring, 70)),
        Ok(OrderOutcome::Taken)
    );
    let unit_ai = ai_9(&game);
    assert_eq!(
        (
            unit_ai.current_order,
            unit_ai.current_action,
            unit_ai.last_action
        ),
        (
            Some(luring),
            Some(AiAction::BoarLuring),
            Some(AiAction::Hunt)
        )
    );
    let mut world = world(idle(UnitAiKind::Civilian));
    assert_eq!(
        orders(&mut world, &queued(luring, 99)),
        Ok(OrderOutcome::NotTaken)
    );
    let unit_ai = ai_9(&world);
    assert_eq!(
        (unit_ai.current_order, unit_ai.current_action),
        (None, Some(AiAction::BoarLuring))
    );
}

/// The move orders move to their point within their range (a forced, walking move: its
/// `MoveTo` at once).
#[test]
fn the_move_orders_move_to_their_point_within_their_range() {
    for order in [OrderKind::Move, OrderKind::GroupGather] {
        let mut world = order_world(order_unit(order_soldier(), idle(UnitAiKind::Soldier)), &[]);
        assert_eq!(
            orders(&mut world, &queued(order, -1)),
            Ok(OrderOutcome::Taken)
        );
        assert_eq!(
            walk_9(&world),
            Some((Position::new(12.7, 30.2, 0.5), Distance::new(3.0))),
            "{order:?}"
        );
        let unit_ai = ai_9(&world);
        assert_eq!(
            (unit_ai.current_order, unit_ai.current_action),
            (Some(order), Some(AiAction::Move))
        );
    }
}

/// The follow order on object 70: within its range (the footprints' squared gap below 9) the AI
/// follows without a move; beyond it, it moves to the object (slot 0x138: its `MoveTo` at once,
/// on the object within the range), the follow its last action. A gone target is not taken. (A
/// gap that is not a number, which stays, needs a place no world holds.)
#[test]
fn the_follow_order_follows_and_moves_to_target_beyond_its_range() {
    let follow = queued(OrderKind::Follow, 70);
    let near = SceneObject {
        at: Position::new(6.0, 7.0, 0.0),
        ..order_target(70, player_id(1), class::CLASS_6)
    };
    let mut world = order_world(
        order_unit(order_soldier(), idle(UnitAiKind::Soldier)),
        &[near],
    );
    assert_eq!(orders(&mut world, &follow), Ok(OrderOutcome::Taken));
    assert_eq!(walk_9(&world), None);
    let unit_ai = ai_9(&world);
    assert_eq!(
        (
            unit_ai.current_order,
            unit_ai.current_action,
            unit_ai.current_target,
            unit_ai.desired_distance
        ),
        (
            Some(OrderKind::Follow),
            Some(AiAction::Follow),
            Some(object_id(70)),
            Distance::new(3.0)
        )
    );
    // beyond it: the move to the object
    let far = order_target(70, player_id(1), class::CLASS_6);
    let mut world = order_world(
        order_unit(order_soldier(), idle(UnitAiKind::Soldier)),
        &[far],
    );
    assert_eq!(orders(&mut world, &follow), Ok(OrderOutcome::Taken));
    let to = world
        .parts
        .actions
        .of(object_id(9))
        .list
        .move_to()
        .map(|move_to_action| (move_to_action.target, move_to_action.range));
    assert_eq!(to, Some((Some(object_id(70)), Distance::new(3.0))));
    let unit_ai = ai_9(&world);
    assert_eq!(
        (unit_ai.current_action, unit_ai.last_action),
        (Some(AiAction::Move), Some(AiAction::Follow))
    );
    // a gone target
    let mut world = order_world(order_unit(order_soldier(), idle(UnitAiKind::Soldier)), &[]);
    assert_eq!(
        orders(&mut world, &queued(OrderKind::Follow, 99)),
        Ok(OrderOutcome::NotTaken)
    );
    assert_eq!(walk_9(&world), None);
}

/// The transport, enter and unload orders give the unit their commands. The transport and the
/// unload over a wall are not modelled. The enter into player 1's house 70 (room for 5,
/// villagers): its Enter at once, walking to the house; the AI holds the house at (−1, −1, −1)
/// with its class. An object the world does not hold takes no enter (the civilian's own test,
/// then the command's: so the class the AI held for one, which `FUN_140c64c10` keeps, needs a
/// command no world answers). The unload: its Transport at once, sailing to the point at the
/// unit's height; the unit itself the target.
#[test]
fn the_transport_enter_and_unload_orders_give_the_unit_commands() {
    let ship = Master {
        standing_sprite: Some(STANDING.id),
        terrain_restriction: Some(RestrictionId::named(7)),
        ..walker(4, class::TRANSPORT)
    };
    for order in [OrderKind::Transport, OrderKind::UnloadOverWall] {
        let mut world = order_world(order_unit(ship.clone(), idle(UnitAiKind::Transport)), &[]);
        assert_eq!(
            orders(&mut world, &queued(order, -1)),
            Err(UNMODELLED_COMMAND),
            "{order:?}"
        );
    }
    // the enter
    let (house, _) = house_and_tree();
    let house = Master {
        garrison: GarrisonCapacity::of_data(5),
        building: Some(BuildingMaster {
            garrison_kind: GarrisonKinds::VILLAGERS,
            ..BuildingMaster::default()
        }),
        ..house
    };
    let villager = Master {
        standing_sprite: Some(STANDING.id),
        action: Some(ActionPart {
            tasks: vec![Task {
                action_kind: GARRISON,
                object_group: Some(class::BUILDING),
                ..Task::default()
            }],
            ..ActionPart::default()
        }),
        ..villager_master()
    };
    let target = SceneObject::new(70, player_id(1), house, Position::new(10.0, 20.0, 0.0));
    let enter = OrderKind::Enter;
    let mut world = order_world(
        order_unit(villager.clone(), idle(UnitAiKind::Soldier)),
        &[target],
    );
    assert_eq!(
        orders(&mut world, &queued(enter, 70)),
        Ok(OrderOutcome::Taken)
    );
    let walks = world
        .parts
        .actions
        .of(object_id(9))
        .list
        .enter()
        .and_then(|enter_action| {
            enter_action
                .state
                .move_to()
                .map(|move_to_action| move_to_action.target)
        });
    assert_eq!(walks, Some(Some(object_id(70))));
    let unit_ai = ai_9(&world);
    assert_eq!(
        (
            unit_ai.current_order,
            unit_ai.current_action,
            unit_ai.current_target,
            unit_ai.target_class,
            unit_ai.target_point
        ),
        (
            Some(enter),
            Some(AiAction::Enter),
            Some(object_id(70)),
            class_of(3),
            Position::from([-1.0; 3])
        )
    );
    for ai in [UnitAiKind::Civilian, UnitAiKind::Soldier] {
        let mut ai_world = order_world(order_unit(villager.clone(), idle(ai)), &[]);
        assert_eq!(
            orders(&mut ai_world, &queued(enter, 99)),
            Ok(OrderOutcome::NotTaken),
            "{ai:?}"
        );
        assert_eq!((ai_9(&ai_world), top_9(&ai_world)), (&idle(ai), None));
    }
    // the unload
    let mut world = order_world(order_unit(ship, idle(UnitAiKind::Transport)), &[]);
    assert_eq!(
        orders(&mut world, &queued(OrderKind::Unload, -1)),
        Ok(OrderOutcome::Taken)
    );
    let sails = world
        .parts
        .actions
        .of(object_id(9))
        .list
        .transport()
        .map(|transport_action| (transport_action.state.bare(), transport_action.point));
    assert_eq!(
        sails,
        Some((
            TransportState::SailToUnload { move_to: None },
            Position::new(12.7, 30.2, 0.0)
        ))
    );
    let unit_ai = ai_9(&world);
    assert_eq!(
        (
            unit_ai.current_order,
            unit_ai.current_action,
            unit_ai.current_target,
            unit_ai.target_class,
            unit_ai.target_point
        ),
        (
            Some(OrderKind::Unload),
            Some(AiAction::Unload),
            Some(object_id(9)),
            class_of(20),
            Position::new(5.0, 6.0, 0.0)
        )
    );
}

/// Seek and destroy attacks player 2's unit 70 (forced: its Attack at once), the AI holding it
/// at the order's tile at the explore height; explore and destroy explores the tile (its
/// Explore at once) without a target.
#[test]
fn seek_and_destroy_attacks_and_explore_and_destroy_explores_at_tiles() {
    let enemy = order_target(70, player_id(2), class::CLASS_6);
    let soldier = || order_unit(order_soldier(), idle(UnitAiKind::Soldier));
    let seek = OrderKind::SeekAndDestroy;
    let mut world = order_world(soldier(), &[enemy]);
    assert_eq!(
        orders(&mut world, &queued(seek, 70)),
        Ok(OrderOutcome::Taken)
    );
    assert_eq!(attacks_9(&world), Some(object_id(70)));
    let unit_ai = ai_9(&world);
    assert_eq!(
        (
            unit_ai.current_order,
            unit_ai.current_action,
            unit_ai.current_target,
            unit_ai.target_class
        ),
        (
            Some(seek),
            Some(AiAction::SeekAndDestroy),
            Some(object_id(70)),
            class_of(6)
        )
    );
    assert_eq!(
        unit_ai.target_point,
        Position::new(12.0, 30.0, tuning().unit_ai.explore_z)
    );
    let destroy = OrderKind::ExploreAndDestroy;
    let mut world = order_world(soldier(), &[]);
    assert_eq!(
        orders(&mut world, &queued(destroy, -1)),
        Ok(OrderOutcome::Taken)
    );
    let explore = world
        .parts
        .actions
        .of(object_id(9))
        .list
        .explore()
        .map(|explore_action| explore_action.point);
    assert_eq!(explore, Some(Position::new(12.0, 30.0, 0.0)));
    let unit_ai = ai_9(&world);
    assert_eq!(
        (
            unit_ai.current_order,
            unit_ai.current_action,
            unit_ai.current_target,
            unit_ai.target_point
        ),
        (
            Some(destroy),
            Some(AiAction::ExploreAndDestroy),
            None,
            Position::new(12.0, 30.0, tuning().unit_ai.explore_z)
        )
    );
}

/// Case 0x2d8: the action value 0x274 (the last stays), the patrol order's timers, then the
/// order; an adjusted idle timeout of 750 is redrawn (one draw of the AI stream).
#[test]
fn the_wait_order_waits_and_leaves_patrol_order() {
    let patrolling = UnitAi {
        current_order: Some(OrderKind::Patrol),
        current_action: Some(AiAction::Move),
        last_action: Some(AiAction::Heal),
        adjusted_idle_timeout: Millis::new(750),
        idle_timeout: Millis::new(3000),
        secondary_timer: Millis::new(900),
        ..idle(UnitAiKind::Soldier)
    };
    let wait = OrderKind::Wait;
    let mut world = order_world(order_unit(order_soldier(), patrolling), &[]);
    let before = world.streams.get_mut(Stream::Ai).draws;
    assert_eq!(
        orders(&mut world, &queued(wait, -1)),
        Ok(OrderOutcome::Taken)
    );
    assert_eq!(world.streams.get_mut(Stream::Ai).draws - before, 1);
    assert_eq!(top_9(&world), None);
    let unit_ai = ai_9(&world);
    assert_eq!(
        (
            unit_ai.current_order,
            unit_ai.order_priority.get(),
            unit_ai.current_action,
            unit_ai.last_action
        ),
        (Some(wait), 40, Some(AiAction::Wait), Some(AiAction::Heal))
    );
    assert_eq!(unit_ai.secondary_timer, Millis::new(750));
    assert_ne!(unit_ai.adjusted_idle_timeout, Millis::new(750));
}

/// The attack roundup on player 2's unit 70 passes its own target; with an action value it
/// must reach the target, which a lake around it parts from the unit; without one it attacks
/// (its Attack at once), and the order becomes 700, leaving a patrol order.
#[test]
fn the_attack_roundup_passes_its_own_target_and_one_it_cannot_reach() {
    let enemy = order_target(70, player_id(2), class::CLASS_6);
    let roundup = queued(OrderKind::AttackRoundup, 70);
    let own = UnitAi {
        current_target: Some(object_id(70)),
        ..idle(UnitAiKind::Soldier)
    };
    let mut order_world = order_world(order_unit(order_soldier(), own), from_ref(&enemy));
    assert_eq!(
        orders(&mut order_world, &roundup),
        Ok(OrderOutcome::NotTaken)
    );
    assert_eq!(attacks_9(&order_world), None);
    // with an action value it must reach the target: a lake around it
    let moving = UnitAi {
        current_action: Some(AiAction::Move),
        ..idle(UnitAiKind::Soldier)
    };
    let lake: Vec<Tile> = (8..=12)
        .flat_map(|x| [Tile::new(x, 18), Tile::new(x, 22)])
        .chain((19..=21).flat_map(|y| [Tile::new(8, y), Tile::new(12, y)]))
        .collect();
    let mut enemy_world =
        order_world_on(&lake, order_unit(order_soldier(), moving), from_ref(&enemy));
    assert_eq!(
        orders(&mut enemy_world, &roundup),
        Ok(OrderOutcome::NotTaken)
    );
    assert_eq!(attacks_9(&enemy_world), None);
    // without one it attacks, and the order becomes 700, leaving a patrol order
    let patrolling = UnitAi {
        current_order: Some(OrderKind::Patrol),
        secondary_timer: Millis::new(900),
        ..idle(UnitAiKind::Soldier)
    };
    let mut world = order_world_on(&lake, order_unit(order_soldier(), patrolling), &[enemy]);
    assert_eq!(orders(&mut world, &roundup), Ok(OrderOutcome::Taken));
    assert_eq!(attacks_9(&world), Some(object_id(70)));
    let unit_ai = ai_9(&world);
    assert_eq!(
        (
            unit_ai.current_order,
            unit_ai.current_action,
            unit_ai.secondary_timer
        ),
        (
            Some(OrderKind::Attack),
            Some(AiAction::Attack),
            Millis::new(750)
        )
    );
}

/// The order on tree 70 the world holds goes to the unit (its slot 0x128 at once: the villager
/// gathers it) and ends the full update before its rest. On a gone target without its type:
/// the move to the order's point (its `MoveTo` at once) and the order on a target the AI's.
#[test]
fn the_order_on_target_goes_to_the_caller_or_moves_to_gone_one_point() {
    let queued_order = queued(OrderKind::OnTarget, 70);
    let mut game = order_world(
        order_unit(order_gatherer(), idle(UnitAiKind::Civilian)),
        &[order_tree()],
    );
    assert_eq!(orders(&mut game, &queued_order), Ok(OrderOutcome::Taken));
    assert_eq!(top_9(&game), Some(GATHER));
    let ai = ai_9(&game);
    assert!(ai.ordered_on_target);
    // gone, without its type
    for order in [OrderKind::OnTarget, OrderKind::OnTarget2] {
        let mut world = order_world(
            order_unit(order_gatherer(), idle(UnitAiKind::Civilian)),
            &[],
        );
        assert_eq!(
            orders(&mut world, &queued(order, 99)),
            Ok(OrderOutcome::Taken)
        );
        assert_eq!(
            walk_9(&world),
            Some((Position::new(12.7, 30.2, 0.5), Distance::new(3.0)))
        );
        let unit_ai = ai_9(&world);
        assert_eq!(
            (unit_ai.current_order, unit_ai.order_priority.get()),
            (Some(order), 40)
        );
    }
}

/// A gone target with its type: without an owner the type is a class. A tree's (15, wood) is
/// searched for from the order's shown point; the unit works what it finds (slot 0x7e8: its
/// Gather at once, whose start gives its AI the gather) and the order is not taken; with none
/// to find nothing changes. With an owner the type is a master of that player's: a gold mine's
/// class (32, gold); a master the player does not hold gives nothing. A class of no kind (a
/// soldier's 6), or a unit AI type whose slot 0x1b0 gives none, takes nothing.
#[test]
fn gone_target_of_kind_sends_the_unit_to_the_closest_resource_of_its_kind() {
    let typed = QueuedOrder {
        target_kind: order_kind(15),
        ..queued(OrderKind::OnTarget, 99)
    };
    let working = UnitAi {
        current_target: Some(object_id(7)),
        ..idle(UnitAiKind::Civilian)
    };
    let gatherer = Master {
        standing_sprite: Some(STANDING.id),
        ..gatherer_master()
    };
    let villager = || order_unit(gatherer.clone(), working.clone());
    let gathers = |world: &World| {
        world
            .parts
            .actions
            .of(object_id(9))
            .list
            .gather()
            .and_then(|gather_action| gather_action.target)
            .map(GatherTargetId::id)
    };
    let tree = resource(
        (80, 349, class::TREE),
        (MasterKind::TREE, GatherGroup::WOOD),
        (13.5, 31.5),
        (attribute::WOOD, 100.0),
    );
    let mut world = order_world(villager(), &[tree]);
    assert_eq!(orders(&mut world, &typed), Ok(OrderOutcome::NotTaken));
    assert_eq!(gathers(&world), Some(object_id(80)));
    let unit_ai = ai_9(&world);
    assert_eq!(
        (
            unit_ai.current_order,
            unit_ai.current_action,
            unit_ai.current_target,
            unit_ai.last_target
        ),
        (
            Some(OrderKind::Gather),
            Some(AiAction::Gather),
            Some(object_id(80)),
            Some(object_id(7))
        )
    );
    let mut world = order_world(villager(), &[]);
    assert_eq!(orders(&mut world, &typed), Ok(OrderOutcome::NotTaken));
    assert_eq!((ai_9(&world), top_9(&world)), (&working, None));
    // with an owner: player 2's master
    let mine = resource(
        (81, 66, class::CLASS_32),
        (MasterKind::TREE, GatherGroup::GOLD),
        (13.5, 31.5),
        (attribute::GOLD, 800.0),
    );
    let owned = |master| QueuedOrder {
        target_kind: Some(OrderTargetKind::of_master(master_id(master))),
        target_owner: owner(2),
        ..queued(OrderKind::OnTarget, 99)
    };
    let mut world = order_world(villager(), from_ref(&mine));
    assert_eq!(orders(&mut world, &owned(66)), Ok(OrderOutcome::NotTaken));
    assert_eq!(gathers(&world), Some(object_id(81)));
    let mut world = order_world(villager(), from_ref(&mine));
    assert_eq!(orders(&mut world, &owned(67)), Ok(OrderOutcome::NotTaken));
    assert_eq!((ai_9(&world), top_9(&world)), (&working, None));
    // a class of no kind, or a unit AI type without one
    let soldier_kind = QueuedOrder {
        target_kind: order_kind(6),
        ..queued(OrderKind::OnTarget, 99)
    };
    let mut mine_world = order_world(villager(), from_ref(&mine));
    assert_eq!(
        orders(&mut mine_world, &soldier_kind),
        Ok(OrderOutcome::NotTaken)
    );
    assert_eq!(top_9(&mine_world), None);
    let soldier = Master {
        standing_sprite: Some(STANDING.id),
        ..gatherer_master()
    };
    let mut world = order_world(order_unit(soldier, idle(UnitAiKind::Soldier)), &[mine]);
    assert_eq!(orders(&mut world, &owned(66)), Ok(OrderOutcome::NotTaken));
    assert_eq!(top_9(&world), None);
}

#[test]
fn slot_0x1b0_gives_the_kind_of_resource_by_the_unit_ai_kind() {
    let kind = |ai_kind, class| idle(ai_kind).resource_kind(class_id(class), &roles());
    // the byte the game returns: −1 for none
    let group = |byte: i32| (0 <= byte).then(|| GatherGroup::try_from(byte).unwrap());
    for ai_kind in [UnitAiKind::Civilian, UnitAiKind::Building] {
        assert_eq!(
            [15, 7, 5, 31, 33, 8, 32, 63, 6].map(|class| kind(ai_kind, class)),
            [0, 1, 2, 2, 2, 3, 4, 4, -1].map(group)
        );
    }
    assert_eq!(
        [5, 31, 33, 63, 15, 32, 34].map(|class| kind(UnitAiKind::Fishing, class)),
        [2, 2, 2, 4, -1, -1, -1].map(group)
    );
    assert_eq!(
        [15, 5, 63].map(|class| kind(UnitAiKind::Soldier, class)),
        [None; 3]
    );
}

/// The attack move of a trebuchet (classes 51 and 54) attacks player 2's house 70: the packed
/// one (51) never attacks, so it takes nothing; the unpacked one's attack command is its work
/// order on the house (slot 0x7e8), whose attack task attacks it within its reach and packs
/// (not held) out of it. Any other class, or no target, moves to the order's point (its
/// `MoveTo` at once).
#[test]
fn the_attack_move_attacks_with_trebuchet_and_moves_otherwise() {
    let (house, _) = house_and_tree();
    let enemy = SceneObject {
        ai: Some(idle(UnitAiKind::Building)),
        ..SceneObject::new(70, player_id(2), house, Position::new(10.0, 20.0, 0.0))
    };
    let unit = |class, ai| {
        let master = Master {
            id: master_id(4),
            class,
            ..order_soldier()
        };
        order_unit(master, idle(ai))
    };
    let attack_move = queued(OrderKind::AttackMove, 70);
    let mut enemy_world = order_world(unit(class::CLASS_51, UnitAiKind::Siege), from_ref(&enemy));
    assert_eq!(
        orders(&mut enemy_world, &attack_move),
        Ok(OrderOutcome::NotTaken)
    );
    assert_eq!(
        (attacks_9(&enemy_world), ai_9(&enemy_world)),
        (None, &idle(UnitAiKind::Siege))
    );
    let trebuchet = |weapon_range| {
        let mut master = Master {
            id: master_id(4),
            class: class::CLASS_54,
            combat: Some(CombatPart {
                weapon_range: Distance::new(weapon_range),
                ..CombatPart::default()
            }),
            ..order_soldier()
        };
        master.action.get_or_insert_with(Default::default).tasks = vec![Task {
            action_kind: ActionKind::COMBAT,
            owner_kind: TargetOwner::ENEMY,
            object_group: Some(class_id(3)),
            ..Task::default()
        }];
        order_unit(master, idle(UnitAiKind::Siege))
    };
    let mut enemy_world = order_world(trebuchet(16.0), from_ref(&enemy));
    assert_eq!(
        orders(&mut enemy_world, &attack_move),
        Ok(OrderOutcome::Taken)
    );
    assert_eq!(attacks_9(&enemy_world), Some(object_id(70)));
    let mut enemy_world = order_world(trebuchet(1.0), from_ref(&enemy));
    assert_eq!(orders(&mut enemy_world, &attack_move), Err(PACK_OR_UNPACK));
    for (class, target) in [(class::CLASS_6, 70), (class::CLASS_51, -1)] {
        let mut world = order_world(unit(class, UnitAiKind::Soldier), from_ref(&enemy));
        assert_eq!(
            orders(&mut world, &queued(OrderKind::AttackMove, target)),
            Ok(OrderOutcome::Taken)
        );
        assert_eq!(
            walk_9(&world),
            Some((Position::new(12.7, 30.2, 0.5), Distance::new(3.0))),
            "{class:?}"
        );
        let unit_ai = ai_9(&world);
        assert_eq!(
            (unit_ai.current_order, unit_ai.current_action),
            (Some(OrderKind::AttackMove), Some(AiAction::Move))
        );
    }
}

#[test]
fn the_order_tail_takes_the_order_and_leaves_patrol_order() {
    let patrolling = |order| UnitAi {
        current_order: order,
        order_priority: OrderPriority::of_data(7),
        adjusted_idle_timeout: Millis::new(750),
        idle_timeout: Millis::new(3000),
        secondary_timer: Millis::new(2000),
        current_target: Some(object_id(70)),
        ..idle(UnitAiKind::Soldier)
    };
    // leaving a patrol order for another redraws a timeout of exactly 750 and caps the
    // secondary timer
    let mut unit_ai = patrolling(Some(OrderKind::Patrol));
    let mut generator = rng();
    assert_eq!(
        unit_ai.order_tail(
            Some(OrderKind::Move),
            OrderPriority::of_data(40),
            Some(class::CLASS_6),
            TakenAs::Order,
            &mut generator,
            &roles()
        ),
        OrderOutcome::Taken
    );
    assert_eq!(
        (
            unit_ai.current_order,
            unit_ai.order_priority.get(),
            unit_ai.secondary_timer,
            generator.draws
        ),
        (Some(OrderKind::Move), 40, Millis::new(750), 1)
    );
    // staying on it does nothing to the timers
    let mut unit_ai = patrolling(Some(OrderKind::AttackMove));
    let mut generator = rng();
    let _ = unit_ai.order_tail(
        Some(OrderKind::AttackMove),
        OrderPriority::of_data(40),
        Some(class::CLASS_6),
        TakenAs::Order,
        &mut generator,
        &roles(),
    );
    assert_eq!(
        (
            unit_ai.secondary_timer,
            unit_ai.adjusted_idle_timeout,
            generator.draws
        ),
        (Millis::new(2000), Millis::new(750), 0)
    );
    // the stop ends the order and its priority
    let mut unit_ai = patrolling(Some(OrderKind::Patrol));
    let _ = unit_ai.order_tail(
        Some(OrderKind::Stop),
        OrderPriority::of_data(40),
        Some(class::CLASS_6),
        TakenAs::Order,
        &mut rng(),
        &roles(),
    );
    assert_eq!(
        (
            unit_ai.current_order,
            unit_ai.order_priority.get(),
            unit_ai.secondary_timer
        ),
        (None, -1, Millis::new(750))
    );
    // the attack move's attack and the roundup become the attack order
    let mut unit_ai = patrolling(Some(OrderKind::AttackMove));
    let _ = unit_ai.order_tail(
        Some(OrderKind::AttackMove),
        OrderPriority::of_data(40),
        Some(class::CLASS_51),
        TakenAs::Attack,
        &mut rng(),
        &roles(),
    );
    assert_eq!(
        (unit_ai.current_order, unit_ai.secondary_timer),
        (Some(OrderKind::Attack), Millis::new(750))
    );
    // a class-18 unit's attack order: the target goes, the order is -1
    let mut unit_ai = patrolling(None);
    let _ = unit_ai.order_tail(
        Some(OrderKind::Attack),
        OrderPriority::of_data(40),
        Some(class::MONK),
        TakenAs::Order,
        &mut rng(),
        &roles(),
    );
    assert_eq!(
        (
            unit_ai.current_order,
            unit_ai.order_priority.get(),
            unit_ai.current_target
        ),
        (None, 40, None)
    );
}

/// Player 1's unit `id` at (20, 20) on its sprite (master `master`), its unit AI `ai`, on a
/// map of 200 by 200 with the `others`, each in player 1's lists. The AI stream is seeded with
/// 0, as [`rng`] is.
fn player_unit_world(id: i32, (master, ai): (Master, UnitAi), others: &[SceneObject]) -> World {
    let mut scene = Scene::open(GridSize::new(200, 200))
        .players(2)
        .graphic(STANDING)
        .object(SceneObject {
            ai: Some(ai),
            sprite: Some(STANDING.id),
            ..SceneObject::new(
                id,
                player_id(1),
                Master {
                    standing_sprite: Some(STANDING.id),
                    ..master
                },
                Position::new(20.0, 20.0, 0.0),
            )
        });
    for other in others {
        scene = scene.object(SceneObject {
            seen_by: Some(VisionBits::visible(player_id(1))),
            ..other.clone()
        });
    }
    let mut world = scene.build();
    world.streams.get_mut(Stream::Ai).set_seed(0);
    world
}

/// The villager's handler (`FUN_140c22bf0`) on the attack action (600): a moved target
/// (0x1fc) with `+0x1a8` set posts the Attack's failure to itself (the unit's slot 0x328:
/// 0x1f9 with 600, 0, 0), else an invalidation, a moved target or a search goes to the base
/// handler (a moved target without a saved patrol: nothing).
#[test]
fn villager_attack_target_that_moved_is_the_base_handler_or_its_failure() {
    let attacking = |stop_after_kill| UnitAi {
        current_order: Some(OrderKind::Attack),
        current_action: Some(AiAction::Attack),
        current_target: Some(object_id(18842)),
        target_class: class_of(47),
        stop_after_kill,
        ..idle(UnitAiKind::Civilian)
    };
    let run = |ai, notify| {
        let mut world = player_unit_world(6116, (villager_master(), ai), &[]);
        let hears = hears(&mut world, (player_id(1), 6116), notify);
        (
            hears,
            world.parts.unit_ai.get(object_id(6116)).unwrap().clone(),
        )
    };
    let moved = Notify::own(object_id(6116), NotifyKind::TARGET_MOVED, [0, 0, 0]);
    let (result, unit_ai) = run(attacking(true), moved);
    assert_eq!(result, Ok(GO_ON));
    assert_eq!(
        unit_ai.notifies,
        [Notify::own(
            object_id(6116),
            NotifyKind::ACTION_FAILED,
            [AiAction::Attack.value(), 0, 0]
        )]
    );
    let (result, unit_ai) = run(attacking(false), moved);
    assert_eq!(result, Ok(GO_ON));
    assert_eq!(unit_ai.notifies, Vec::new());
    assert_eq!(unit_ai.current_action, Some(AiAction::Attack));
    // the base handler's invalidation of the current action stops it
    let invalid = Notify::own(
        object_id(6116),
        NotifyKind::ACTION_INVALIDATED,
        [AiAction::Attack.value(), 0, 0],
    );
    let (result, unit_ai) = run(attacking(false), invalid);
    assert!(result.is_ok());
    assert_eq!(unit_ai.current_action, None);
}

/// Player 1's villager 20 at (20, 20) holding 10 wood on its Gather of Gaia's stone 77 at
/// (22, 20) (the gather order's Gather at once), and, `with_camp`, player 1's lumber camp 45
/// (master 562) at (26, 20), the drop-site table's building for a villager's wood, done in its
/// player's tracker.
fn other_load_world(with_camp: bool) -> World {
    let gatherer = gatherer_master();
    let villager = Master {
        standing_sprite: Some(STANDING.id),
        action: gatherer.action.clone().map(|action_part| ActionPart {
            drop_sites: vec![ids::TOWN_CENTRE, master_id(562)],
            ..action_part
        }),
        ..gatherer
    };
    let stone = resource(
        (77, 102, class_id(8)),
        (MasterKind::TREE, GatherGroup::STONE),
        (22.0, 20.0),
        (attribute::STONE, 100.0),
    );
    let camp = Master {
        id: master_id(562),
        kind: MasterKind::BUILDING,
        class: class::BUILDING,
        radius: Footprint::new(1.0, 1.0),
        height: Distance::new(2.0),
        hit_points: MasterHitPoints::of_data(600),
        ..Master::default()
    };
    let mut scene = Scene::open(GridSize::new(64, 64))
        .players(2)
        .graphic(STANDING)
        .object(SceneObject {
            ai: Some(idle(UnitAiKind::Civilian)),
            sprite: Some(STANDING.id),
            holds: Some((attribute::WOOD, Amount::new(10.0))),
            ..SceneObject::new(20, player_id(1), villager, Position::new(20.0, 20.0, 0.0))
        })
        .object(SceneObject {
            seen_by: Some(VisionBits::visible(player_id(1))),
            ..stone
        });
    if with_camp {
        scene = scene.object(SceneObject {
            seen_by: Some(VisionBits::visible(player_id(1))),
            ..SceneObject::new(45, player_id(1), camp, Position::new(26.0, 20.0, 0.0))
        });
    }
    let mut world = scene.build();
    world.model.drop_sites = Arc::new(vec![DropSite {
        building: master_id(562),
        worker_class: Some(class::VILLAGER),
        resource_kinds: vec![attribute::WOOD],
        ..DropSite::default()
    }]);
    if with_camp {
        let key = world.tracker_key(player_id(1), master_id(562));
        world.players[1].tracker.add(
            object_id(45),
            Key {
                pending: false,
                ..key
            },
        );
    }
    world.streams.get_mut(Stream::Ai).set_seed(0);
    let gather = queued(OrderKind::Gather, 77);
    let order_outcome = run_ai(
        &mut world,
        (player_id(1), object_id(20)),
        |ai, at, side, kind, rng| {
            ai.process_order(
                &gather,
                Some((at, kind)),
                None,
                rng,
                side,
                AiLayer {
                    roles: &at.roles,
                    slots: at.slots,
                    unit_ai_tuning: &at.unit_ai_tuning,
                },
            )
        },
    );
    assert_eq!(order_outcome, Ok(OrderOutcome::Taken));
    world
}

/// The villager's case 0x21c (`FUN_140c22bf0`), a load of another resource asked off: the
/// target of its top action (slot 0x190: stone 77), then the drop-site table's building
/// (`FUN_141658e20`: the lumber camp 45), which the unit works at once (its Gather walks to
/// the camp to drop the wood there, the camp one of the villager's drop sites) and whose way
/// back the caller queues (the order on stone 77); without a building nothing. The
/// processing goes on.
#[test]
fn villager_drops_another_resource_at_the_table_building() {
    let other = Notify::own(object_id(20), NotifyKind::OTHER_LOAD, [0; 3]);
    let mut world = other_load_world(true);
    assert_eq!(hears(&mut world, (player_id(1), 20), other), Ok(GO_ON));
    let gather = world.parts.actions.of(object_id(20)).list.gather().copied();
    assert_eq!(
        gather.map(|gather_action| (gather_action.carry_to, gather_action.state.bare())),
        Some((
            Some(DropSiteId::new(object_id(45))),
            GatherState::Drop { move_to: None }
        ))
    );
    let unit_ai = world.parts.unit_ai.get(object_id(20)).unwrap();
    let back: Vec<_> = unit_ai
        .orders
        .iter()
        .map(|queued_order| (queued_order.order, queued_order.target))
        .collect();
    assert_eq!(back, [(OrderKind::OnTarget, Some(object_id(77)))]);
    let mut world = other_load_world(false);
    assert_eq!(hears(&mut world, (player_id(1), 20), other), Ok(GO_ON));
    let action = world.parts.actions.of(object_id(20));
    assert_eq!(action.top_target(), Some(object_id(77)));
    assert_eq!(
        world.parts.unit_ai.get(object_id(20)).unwrap().orders,
        Vec::new()
    );
}

/// The villager's case 0x21d (`FUN_140c22bf0`): off the build or the hunt action the object
/// its action made (429381190's villager 15295 and its farm 15342), which the world holds, the
/// unit works at its place (slot 0x7e8: its Gather of the farm at once); off another action
/// nothing. The processing goes on.
#[test]
fn villager_works_the_pasture_it_built() {
    let farm = SceneObject {
        id: object_id(15342),
        at: Position::new(24.0, 20.0, 0.0),
        ..own_farm()
    };
    let built = |action| {
        Notify::own(
            object_id(15295),
            NotifyKind::PASTURE_BUILT,
            [action, 15342, 0],
        )
    };
    let run = |notify| {
        let mut world = player_unit_world(
            15295,
            (gatherer_master(), idle(UnitAiKind::Civilian)),
            from_ref(&farm),
        );
        let hears = hears(&mut world, (player_id(1), 15295), notify);
        (hears, world.parts.actions.of(object_id(15295)).top_target())
    };
    for action in [AiAction::Build.value(), AiAction::Hunt.value()] {
        assert_eq!(run(built(action)), (Ok(GO_ON), Some(object_id(15342))));
    }
    assert_eq!(run(built(AiAction::Gather.value())), (Ok(GO_ON), None));
}

/// The base handler's completion (`FUN_140c589c0` case 0x1fa; the villager's own case takes
/// it before) of an attack on the attack order 700: processIdle with 0; when it produced an
/// action the processing breaks (0xd), else the target goes and slot 0xb0 with 0 ends the
/// order. A unit without an order in a group with a command keeps its place as its state
/// position.
#[test]
fn an_attack_completed_on_the_attack_order_looks_for_the_next_or_ends_it() {
    let done = Notify::own(
        object_id(9),
        NotifyKind::ACTION_COMPLETED,
        [AiAction::Attack.value(), 0, 0],
    );
    // a transport ship's processIdle only caps its last human order time (5)
    let ship = UnitAi {
        current_order: Some(OrderKind::Attack),
        order_priority: OrderPriority::of_data(100),
        current_action: Some(AiAction::Attack),
        current_target: Some(object_id(70)),
        ..idle(UnitAiKind::Transport)
    };
    let transport = Master {
        id: master_id(4),
        class: class::TRANSPORT,
        ..order_soldier()
    };
    let mut world = order_world(order_unit(transport, ship), &[]);
    world.players[1]
        .objects
        .by_id_mut(object_id(9))
        .unwrap()
        .last_human_order = WorldTime::from_millis(9000);
    assert_eq!(hears(&mut world, (player_id(1), 9), done), Ok(GO_ON));
    let unit_ai = ai_9(&world);
    assert_eq!(
        (
            unit_ai.current_order,
            unit_ai.order_priority.get(),
            unit_ai.last_order
        ),
        (None, -1, Some(OrderKind::Attack))
    );
    assert_eq!(
        (
            unit_ai.current_target,
            unit_ai.last_target,
            unit_ai.current_action
        ),
        (None, Some(object_id(70)), None)
    );
    assert_eq!(unit_ai.state_position, Point::new(5.0, 6.0));
    let unit = world.players[1].objects.by_id(object_id(9)).unwrap();
    assert_eq!(unit.last_human_order, HUMAN_ORDER_CAP);
    // a boar beyond its leash heads home (6, its `MoveTo` at once): the processing breaks
    let boar = UnitAi {
        current_order: Some(OrderKind::Attack),
        current_action: Some(AiAction::Attack),
        current_target: Some(object_id(70)),
        ..boar()
    };
    let mut gaia_unit_world = gaia_unit_world(9, boar, Position::new(43.5, 105.5, 0.0));
    *gaia_unit_world.streams.get_mut(Stream::Ai) = seed_for(false);
    assert_eq!(
        hears(&mut gaia_unit_world, (PlayerId::GAIA, 9), done),
        Ok(PRODUCED)
    );
    assert!(
        gaia_unit_world
            .parts
            .actions
            .of(object_id(9))
            .list
            .move_to()
            .is_some()
    );
    // (its move forgot the target: FUN_140c64c10 with -1)
    let unit_ai = gaia_unit_world.parts.unit_ai.get(object_id(9)).unwrap();
    assert_eq!(
        (
            unit_ai.current_order,
            unit_ai.current_target,
            unit_ai.last_target
        ),
        (Some(OrderKind::Attack), None, Some(object_id(70)))
    );
    // without an order, in a group with a command: the state position, then the stop
    let attacking = UnitAi {
        current_action: Some(AiAction::Attack),
        current_target: Some(object_id(70)),
        ..idle(UnitAiKind::Soldier)
    };
    let mut world = order_world(order_unit(order_soldier(), attacking.clone()), &[]);
    world.groups.groups.push(UnitGroup {
        id: group_id(7),
        owner: Some(player_id(1)),
        members: vec![Member {
            id: object_id(9),
            ..Member::default()
        }],
        current_command: GroupCommandKind::MOVE,
        ..UnitGroup::default()
    });
    world.players[1]
        .objects
        .by_id_mut(object_id(9))
        .unwrap()
        .group
        .id = Some(group_id(7));
    assert_eq!(hears(&mut world, (player_id(1), 9), done), Ok(GO_ON));
    assert_eq!(ai_9(&world).state_position, Point::new(5.0, 6.0));
    let mut world = order_world(order_unit(order_soldier(), attacking), &[]);
    let _ = hears(&mut world, (player_id(1), 9), done);
    assert_eq!(ai_9(&world).state_position, Point::new(0.0, 0.0));
}

/// A computer player's packed siege unit 500 (class 51) at (20, 30) on its Attack of player
/// 2's object 90 at (25, 30), its attacker player 2's warship 77 at (22, 30), each in player 1's
/// lists.
fn packed_siege_world() -> World {
    let packed = UnitAi {
        current_action: Some(AiAction::Attack),
        attackers: Attackers::of_saved(&[77]),
        ..idle(UnitAiKind::Siege)
    };
    let attacking = Action {
        list: ActionList::default().with_attack(Some(own_attack(
            90,
            AttackState::Ready,
            Distance::new(5.0),
        ))),
        ..Action::default()
    };
    let siege = Master {
        id: master_id(4),
        class: class::CLASS_51,
        ..order_soldier()
    };
    let mut scene = Scene::open(GridSize::new(64, 64))
        .players(2)
        .kind(player_id(1), PlayerKind::Computer)
        .graphic(STANDING)
        .object(SceneObject {
            ai: Some(packed),
            actions: Some(attacking),
            sprite: Some(STANDING.id),
            ..SceneObject::new(500, player_id(1), siege, Position::new(20.0, 30.0, 0.0))
        });
    for (id, class, x) in [(77, class::WARSHIP, 22.0), (90, class::CLASS_6, 25.0)] {
        scene = scene.object(SceneObject {
            seen_by: Some(VisionBits::visible(player_id(1))),
            ai: Some(UnitAi {
                current_target: Some(object_id(500)),
                ..idle(UnitAiKind::Soldier)
            }),
            ..SceneObject::new(
                id,
                player_id(2),
                walker(i16::try_from(id).expect("a master's id"), class),
                Position::new(x, 30.0, 0.0),
            )
        });
    }
    scene.build()
}

/// The siege unit's handler (`FUN_140c287e0`) on a computer player's packed unit's damage
/// ([`UnitAi::packed_damaged`]): an attacker the world does not hold ends it. Else the unit
/// stops (slot 0xb0), which ends its top action, so its slot 0x160 finds no top target and a
/// computer player's unit lets the units inside it go first (`FUN_141225c40`, not modelled);
/// dead, its owner's AI would hear the alert (not modelled either).
#[test]
fn computer_packed_siege_unit_stops_and_runs_when_damaged() {
    let hit =
        |caller: i32, hp| Notify::own(object_id(caller), NotifyKind::DAMAGED, [caller, hp, 0]);
    let mut world = packed_siege_world();
    assert_eq!(
        hears(&mut world, (player_id(1), 500), hit(77, 40)),
        Err(GenieError::unsupported(
            "a computer player's siege unit running away (FUN_141225c40)"
        ))
    );
    let mut world = packed_siege_world();
    let before = ai_500(&world).clone();
    assert_eq!(
        hears(&mut world, (player_id(1), 500), hit(78, 40)),
        Ok(GO_ON)
    );
    assert_eq!(ai_500(&world), &before);
    assert_eq!(attacks_500(&world), Some(object_id(90)));
    let mut world = packed_siege_world();
    assert_eq!(
        hears(&mut world, (player_id(1), 500), hit(77, 0)),
        Err(GenieError::unsupported(
            "a computer player's AI hearing its unit's alert (slot 0x2e8, then slot 0x3c8)"
        ))
    );
}

/// The retarget tests' soldier ([`soldier_master`] of line of sight 6) on the attack order
/// against `target` (−1 for none).
fn retargeter(target: i32) -> (Master, UnitAi) {
    let soldier = Master {
        line_of_sight: Distance::new(6.0),
        ..soldier_master()
    };
    let unit_ai = UnitAi {
        current_order: Some(OrderKind::Attack),
        current_action: Some(AiAction::Attack),
        current_target: ObjectId::of_field(target),
        ..idle(UnitAiKind::Soldier)
    };
    (soldier, unit_ai)
}

/// Player 2's object `id` of master `master` at (x, y), its unit AI of `kind`.
fn enemy_unit(id: i32, master: Master, (x, y): (f32, f32), kind: UnitAiKind) -> SceneObject {
    SceneObject {
        ai: Some(idle(kind)),
        ..SceneObject::new(id, player_id(2), master, Position::new(x, y, 0.0))
    }
}

/// A building master `id` of class `class`: radius 1, a moving object of speed 0.
fn building_master(id: i16, class: ClassId) -> Master {
    Master {
        id: master_id(id),
        kind: MasterKind::BUILDING,
        class,
        radius: Footprint::new(1.0, 1.0),
        height: Distance::new(2.0),
        hit_points: MasterHitPoints::of_data(550),
        terrain_restriction: Some(RestrictionId::named(7)),
        move_stats: Some(walking(Speed::ZERO)),
        ..Master::default()
    }
}

/// The retarget's search ([`UnitAi::retarget_search`]) of unit 500 of `world`: its answer, its
/// Attack's target, and the objects the search's enemies list held.
fn retargets(
    world: &mut World,
    search: RetargetSearch,
) -> (GenieResult<u32>, Option<ObjectId>, Vec<ObjectId>) {
    let run_ai = run_ai(
        world,
        (player_id(1), object_id(500)),
        |ai, at, side, kind, rng| ai.retarget_search(search, kind, at, side, rng),
    );
    let seen = world
        .spatial
        .searched()
        .map(|by_relation| {
            by_relation[UnitRelation::Enemy]
                .iter()
                .map(|record| record.id)
                .collect()
        })
        .unwrap_or_default();
    (run_ai, attacks_500(world), seen)
}

/// The retarget's search for a better target ([`UnitAi::retarget_search`]): from the current
/// target's place (a sheep far from the soldier: the archer beside it is in the search's
/// lists, one beside the soldier is not); on the attack order a find of another attack
/// category than the live current target's ends it (9), as does any find without a building
/// target (another archer beside the current one). Without a target the search starts from
/// the unit. None attacks.
#[test]
fn retarget_search_ends_on_another_category_or_without_building_target() {
    let plain = RetargetSearch {
        building: false,
        work_state: true,
    };
    let sheep = SceneObject {
        capture: CaptureKind::Livestock,
        ..enemy_unit(
            25954,
            walker(594, class::LIVESTOCK),
            (30.0, 20.0),
            UnitAiKind::Livestock,
        )
    };
    let archer = |id: i32, at| enemy_unit(id, walker(7, class::CLASS_0), at, UnitAiKind::Ranged);
    let mut world = soldier_world(
        retargeter(25954),
        None,
        &[sheep.clone(), archer(30, (31.0, 20.0))],
    );
    assert_eq!(
        retargets(&mut world, plain),
        (Ok(9), None, vec![object_id(30)])
    );
    assert_eq!(ai_500(&world).alert_mode, AlertMode::Alert);
    let mut world = soldier_world(retargeter(25954), None, &[sheep, archer(30, (21.0, 20.0))]);
    assert_eq!(retargets(&mut world, plain), (Ok(9), None, vec![]));
    // the same category: no building target, so nothing more either
    let mut world = soldier_world(
        retargeter(30),
        None,
        &[archer(30, (30.0, 20.0)), archer(31, (31.0, 20.0))],
    );
    let (result, attacked, _) = retargets(&mut world, plain);
    assert_eq!((result, attacked), (Ok(9), None));
    // without a target: from the unit
    let mut world = soldier_world(retargeter(-1), None, &[archer(30, (22.0, 20.0))]);
    assert_eq!(
        retargets(&mut world, plain),
        (Ok(9), None, vec![object_id(30)])
    );
    let mut world = soldier_world(retargeter(-1), None, &[archer(30, (40.0, 20.0))]);
    assert_eq!(retargets(&mut world, plain), (Ok(9), None, vec![]));
}

/// With a building target the search goes on (off the attack order): a found building keeps
/// the current target (10), but a trebuchet's classes, or a find for a farm target that is no
/// farm; a found annex names its main building (house 40 out of the search's reach); a found
/// object other than the current target is attacked (slot 0xb8, not any; 10), the current
/// target itself is not (9). A current target gone, the search starts from the unit. A
/// computer player's search (on the attack order, the unit in stance 0) is not modelled.
#[test]
fn retarget_search_with_building_target_attacks_what_it_finds() {
    let building = RetargetSearch {
        building: true,
        work_state: false,
    };
    let archer = || {
        enemy_unit(
            30,
            walker(7, class::CLASS_0),
            (22.0, 21.0),
            UnitAiKind::Ranged,
        )
    };
    let house = |at| {
        enemy_unit(
            40,
            building_master(70, class::BUILDING),
            at,
            UnitAiKind::Building,
        )
    };
    let annex = |at| SceneObject {
        annex_of: Some(MainBuildingId::new(object_id(40))),
        ..enemy_unit(
            41,
            building_master(71, class::BUILDING),
            at,
            UnitAiKind::Building,
        )
    };
    let trebuchet = enemy_unit(
        42,
        building_master(42, class::CLASS_54),
        (23.0, 23.0),
        UnitAiKind::Siege,
    );
    let farm = enemy_unit(
        43,
        building_master(50, class::FARM),
        (23.0, 20.0),
        UnitAiKind::Building,
    );
    let attacked = |target: i32, others: &[SceneObject]| {
        let (soldier, mut unit_ai) = retargeter(target);
        unit_ai.current_order = None;
        let mut world = soldier_world((soldier, unit_ai), None, others);
        let (result, attacked, _) = retargets(&mut world, building);
        (result, attacked)
    };
    let near = (23.0, 20.0);
    let far = (40.0, 20.0);
    assert_eq!(
        attacked(40, &[house(near), archer()]),
        (Ok(10), Some(object_id(30)))
    );
    assert_eq!(attacked(40, &[house(near)]), (Ok(10), None));
    assert_eq!(attacked(30, &[house(far), annex(near)]), (Ok(10), None));
    assert_eq!(
        attacked(43, &[farm.clone(), house(far), annex((25.0, 20.0))]),
        (Ok(10), Some(object_id(40)))
    );
    assert_eq!(
        attacked(40, &[house(near), trebuchet]),
        (Ok(10), Some(object_id(42)))
    );
    assert_eq!(attacked(43, &[farm]), (Ok(10), None));
    assert_eq!(attacked(30, &[archer()]), (Ok(9), None));
    // a computer player's search
    let mut world = soldier_world(retargeter(40), None, &[house(near), archer()]);
    world.players[1].kind = PlayerKind::Computer;
    assert_eq!(
        retargets(&mut world, building).0,
        Err(GenieError::unsupported(
            "a computer player's bestUnitToAttack"
        ))
    );
}
