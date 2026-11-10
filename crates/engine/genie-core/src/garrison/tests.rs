use super::*;
use crate::GenieResult;
use crate::combat::Firepower;
use crate::combat::MissileCount;
use crate::commands::GatherPointState;
use crate::commands::UnloadPick;
use crate::commands::{GatherPoint, UnloadCommand};
use crate::construction::MainBuildingId;
use crate::garrison::EnterKind;
use crate::garrison::GarrisonScope;
use crate::garrison::GarrisonSwitch;
use crate::garrison::LetOut;
use crate::garrison::ShelterReach;
use crate::garrison::transport::TransportMode;
use crate::garrison::transport::TransportState;
use crate::gathering::hunt::tests::walker as tests_walker;
use crate::groups::{Member, UnitGroup};
use crate::map::Distance;
use crate::map::DistanceSquared;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::Position;
use crate::map::Radii;
use crate::map::Restriction;
use crate::map::RestrictionId;
use crate::map::TerrainFlags;
use crate::map::TerrainId;
use crate::map::TerrainMask;
use crate::map::Tile;
use crate::map::spatial::{Obstruction, Placed, Spatial};
use crate::map::terrain::TileTerrain;
use crate::map::terrain::{Terrain, TerrainKind};
use crate::math::Angle;
use crate::math::fmath::BVector;
use crate::math::rng::Stream;
use crate::movement::MoveAlgorithm;
use crate::movement::Speed;
use crate::movement::TopSpeed;
use crate::movement::TurnRate;
use crate::movement::move_to::MoveToAction;
use crate::movement::move_to::MoveWaitState;
use crate::movement::move_to::SubCode;
use crate::movement::move_to::{ListUpdate, MoveWait, TargetMoved, update_sub_actions};
use crate::movement::{Me, MoveStats, Mover, SubstituteCount, WaitDelays};
use crate::objects::ActionState;
use crate::objects::HitPoints;
use crate::objects::ListIndex;
use crate::objects::MasterHitPoints;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::SpeedMultiplier;
use crate::objects::SpriteFlags;
use crate::objects::SpriteId;
use crate::objects::Voice;
use crate::objects::facing::SpriteFacets;
use crate::objects::layers::Animated;
use crate::objects::layers::CommandCount;
use crate::objects::layers::Speedup;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ActionPart;
use crate::objects::masters::AnimatedPart;
use crate::objects::masters::BuildableOn;
use crate::objects::masters::BuildingMaster;
use crate::objects::masters::ClassAmount;
use crate::objects::masters::ClassId;
use crate::objects::masters::CombatPart;
use crate::objects::masters::CreatablePart;
use crate::objects::masters::FlyMode;
use crate::objects::masters::GarrisonCapacity;
use crate::objects::masters::GarrisonKinds;
use crate::objects::masters::MasterField;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::MovingPart;
use crate::objects::masters::ObjectFlags;
use crate::objects::masters::ObstructionClass;
use crate::objects::masters::ObstructionKind;
use crate::objects::masters::TaskTerrain;
use crate::objects::sprite::Graphic;
use crate::objects::teardown::HeadCode;
use crate::objects::unit::Unit;
use crate::objects::{FacetCount, Frame, WorkerCount};
use crate::orders::Dispatch;
use crate::orders::OrderSource;
use crate::orders::Work;
use crate::orders::town_bell::BellShelter;
use crate::orders::town_bell::{BackToWork, TownBell};
use crate::ownership::CaptureKind;
use crate::pathing::Pathing;
use crate::players::Amount;
use crate::players::ByPlayer;
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::players::Relation;
use crate::players::attribute::AttributeId;
use crate::players::relation_row;
use crate::test_ids::carrying;
use crate::test_ids::class_of;
use crate::test_ids::group_id;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::test_ids::object_id_list;
use crate::test_ids::object_ids;
use crate::test_ids::owner;
use crate::test_ids::player_id;
use crate::test_ids::task_id;
use crate::test_ids::unit_list;
use crate::testing::class;
use crate::testing::ids;
use crate::testing::roles;
use crate::testing::static_roles;
use crate::testing::tuning;
use crate::unit_ai::AiAction;
use crate::unit_ai::Answer;
use crate::unit_ai::Issuer;
use crate::unit_ai::Notifies;
use crate::unit_ai::OrderKind;
use crate::unit_ai::OrderTargetKind;
use crate::unit_ai::UnitAiKind;
use crate::unit_ai::UnitStance;
use crate::unit_ai::task_finder::{DropSite, DropTarget};
use crate::unit_ai::{NotifyKind, OrderPriority, QueuedOrder, UnitAi};
use crate::vision::SightLayer;
use crate::vision::TileCount;
use crate::vision::VisionBits;
use crate::vision::registry::{Registry, UnitUpdate};
use crate::vision::visibility::{Seer, Visibility, VisibleMap};
use crate::vision::{SightCentre, SightShape};
use crate::world::FrameSeed;
use crate::world::Millis;
use crate::world::Player;
use crate::world::Seconds;
use crate::world::WorldTime;
use crate::world::index::ObjectIndex;
use crate::world::objects_ref::ObjectsRef;
use crate::world::parts::Object;
use crate::world::parts::Part;
use crate::world::parts::Parts;
use crate::world::settings::OptionFlags;
use crate::world::tests::in_update;
use std::sync::Arc;

/// An Enter task of work range `range` and class `class` without sprites.
pub(crate) fn enter_task(range: Distance, class: ClassId) -> Task {
    Task {
        work_range: range,
        object_group: Some(class),
        ..Task::default()
    }
}

pub(crate) const TC: MasterId = MasterId::named(109);
pub(crate) const VILLAGER: MasterId = MasterId::named(83);
pub(crate) const ARCHER: MasterId = MasterId::named(4);
const SHIP: MasterId = MasterId::named(545);
const GATE: MasterId = MasterId::named(88);

fn task(action_kind: ActionKind, object_group: i16) -> Task {
    Task {
        action_kind,
        object_group: class_of(object_group),
        ..Task::default()
    }
}

/// A town centre (garrison type 11, room for 15), a villager and an archer that may enter
/// buildings, a transport ship (object flag 0, room for 5) and a gate.
fn garrison_masters() -> Masters {
    Masters::new([
        Master {
            id: TC,
            kind: MasterKind::BUILDING,
            class: class::TOWER,
            hit_points: MasterHitPoints::of_data(2400),
            garrison: GarrisonCapacity::of_data(15),
            building: Some(BuildingMaster {
                garrison_kind: (GarrisonKinds::VILLAGERS
                    | GarrisonKinds::INFANTRY
                    | GarrisonKinds::MONKS),
                ..BuildingMaster::default()
            }),
            ..Master::default()
        },
        Master {
            id: VILLAGER,
            kind: MasterKind::COMBAT,
            class: class::VILLAGER,
            copy_id: Some(VILLAGER),
            standing_sprite: None,
            action: Some(ActionPart {
                tasks: vec![task(GARRISON, 3), task(GARRISON, 20)],
                ..ActionPart::default()
            }),
            moving: Some(MovingPart {
                // no walking or standing sprite: a MoveTo the villager makes takes none (the
                // world's sprites' table holds none)
                move_sprite: None,
                ..MovingPart::default()
            }),
            ..Master::default()
        },
        Master {
            id: ARCHER,
            kind: MasterKind::COMBAT,
            class: class::CLASS_0,
            action: Some(ActionPart {
                tasks: vec![task(ActionKind::COMBAT, 3)],
                ..ActionPart::default()
            }),
            ..Master::default()
        },
        Master {
            id: SHIP,
            kind: MasterKind::COMBAT,
            class: class::TRANSPORT,
            object_flags: ObjectFlags::GARRISON,
            garrison: GarrisonCapacity::of_data(1),
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
            id: GATE,
            kind: MasterKind::BUILDING,
            class: class::GATE,
            hit_points: MasterHitPoints::of_data(2750),
            garrison: GarrisonCapacity::of_data(0),
            ..Master::default()
        },
    ])
}

pub(crate) fn unit(id: i32, master: MasterId, hp: HitPoints) -> Unit {
    let mut unit = Unit {
        id: object_id(id),
        pasture: None,
        my_pasture: None,
        ..Unit::default()
    };
    unit.life.master_id = master;
    unit.life.hp = hp;
    unit.life.state = ObjectState::Alive;
    unit
}

/// Gaia and three players; players 1 and 2 allied both ways, player 3 an enemy. Player 1
/// holds town centre 100, ship 200 and gate 300 (an annex of the town centre).
fn garrison_world() -> World {
    let mut players: Vec<Player> = (0..4).map(|_| Player::default()).collect();
    players[1].relations = relation_row([1, 0, 0, 3]);
    players[2].relations = relation_row([1, 0, 0, 3]);
    players[3].relations = relation_row([1, 3, 3, 0]);
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO; 4],
        players,
    );
    world.model.masters = (0..4).map(|_| garrison_masters()).map(Arc::new).collect();
    let _ = world
        .push_live(PlayerId::new(1), unit(100, TC, HitPoints::new(2400.0)))
        .unwrap();
    let _ = world
        .push_live(PlayerId::new(1), unit(200, SHIP, HitPoints::new(100.0)))
        .unwrap();
    let gate = unit(300, GATE, HitPoints::new(2750.0));
    let _ = world.push_live(PlayerId::new(1), gate).unwrap();
    world.parts.building_mut(object_id(300)).unwrap().annex_of =
        Some(MainBuildingId::new(object_id(100)));
    world
}

/// UNLOAD's class groups (flag 2) as `FUN_14118a610` tests them; group 3 is the bit mask
/// `0x200801000801000` over classes below 58.
#[test]
fn the_unload_groups() {
    let group = |unload_group| {
        (0..64)
            .filter(|&master| {
                unload_group_takes(unload_group, ClassId::of_master(master), &roles())
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(group(1), [4, 19]);
    assert_eq!(group(3), [12, 23, 36, 47, 57]);
    assert_eq!(group(6), [18, 42, 43]);
    assert_eq!(group(4), [0i16; 0]);
}

/// Unit `idx` of player 1 moves at `speed`.
fn walks_at(world: &mut World, idx: ListIndex, speed: f32) {
    let mut walker = tests_walker();
    walker.stats = MoveStats {
        speed: Speed::new(speed),
        base: Speed::new(speed),
        ..standing()
    };
    world
        .parts
        .put_mover(world.players[1].objects[idx].id, Some(walker));
}

/// [`garrison_world`] with villager 9149 of player 1, and its index.
fn walk_again_world() -> (World, ListIndex) {
    let mut world = garrison_world();
    let _ = world
        .push_live(PlayerId::new(1), unit(9149, VILLAGER, HitPoints::new(25.0)))
        .unwrap();
    let idx = world.players[1].objects.last_place().unwrap();
    (world, idx)
}

/// What the reach tile test reads, for unit 1 between (10.5, 10.5) and (11.5, 11.5).
fn walls<'world>(spatial: &'world mut Spatial, stats: &'world MoveStats) -> Walls<'world> {
    Walls {
        me: Point::new(10.5, 10.5),
        them: Point::new(11.5, 11.5),
        spatial,
        id: object_id(1),
        stats,
    }
}

#[test]
fn an_enter_walks_to_its_building_unless_it_is_in_reach() {
    // MarvinBlue/426048731's 9171 garrisons the town centre at (104, 47) from 3.5 tiles off:
    // set_state(4)'s MoveTo to it (FUN_140aa0df0), quiet, starts in state 3
    let (mut world, list_index) = walk_again_world();
    // the villager walks at 0.8
    walks_at(&mut world, list_index, 0.8);
    let tc = EnterTarget {
        id: ContainerId::new(object_id(100)),
        state: ObjectState::Alive,
        allied: true,
        room: true,
        position: Position::new(104.0, 47.0, 0.0),
        in_reach: false,
        kind: EnterKind::Building,
    };
    let start = |gap| WalkStart { gap };
    let mut far = EnterAction::new(
        ContainerId::new(object_id(100)),
        Position::new(104.0, 47.0, 0.0),
        &enter_task(Distance::new(1.0), class::BUILDING),
    );
    assert_eq!(
        world.enter_walk_again(
            PlayerId::new(1),
            list_index,
            &mut far,
            &tc,
            start(Distance::new(3.5))
        ),
        Ok(Walking::Walk(None))
    );
    assert_eq!(
        (
            far.state.bare(),
            far.state
                .move_to()
                .copied()
                .map(|move_to_action| move_to_action.state)
        ),
        (EnterState::Walk { move_to: None }, Some(MoveTo::Start))
    );
    // within ARRIVED of its footprint the MoveTo starts arrived (state 1)
    let mut close = EnterAction::new(
        ContainerId::new(object_id(100)),
        Position::new(104.0, 47.0, 0.0),
        &enter_task(Distance::new(1.0), class::BUILDING),
    );
    assert_eq!(
        world.enter_walk_again(
            PlayerId::new(1),
            list_index,
            &mut close,
            &tc,
            start(Distance::ZERO)
        ),
        Ok(Walking::Walk(None))
    );
    assert_eq!(
        close
            .state
            .move_to()
            .copied()
            .map(|move_to_action| move_to_action.state),
        Some(MoveTo::Arrived)
    );
    // 10049 of update 4409 stands within the task's range: it enters at once
    let mut near = EnterAction::new(
        ContainerId::new(object_id(100)),
        Position::new(104.0, 47.0, 0.0),
        &enter_task(Distance::new(1.0), class::BUILDING),
    );
    let in_reach = EnterTarget {
        in_reach: true,
        ..tc
    };
    assert_eq!(
        world.enter_walk_again(
            PlayerId::new(1),
            list_index,
            &mut near,
            &in_reach,
            start(Distance::new(0.6))
        ),
        Ok(Walking::Enter)
    );
    assert_eq!(near.state.move_to().copied(), None);
    // the reach is the footprints' squared gap against the range squared (COMISS/JBE: a NaN
    // gap is in); a target with speed within a range of 1 (COMISS/JC: a NaN range is not
    // tested) asks the tiles about the two: on a walled map a reach that skips them is in
    let masters = [Masters::default(), garrison_masters()].map(Arc::new);
    let blocked = [(10, 11), (11, 10)];
    let (mut walled, stats) = walled_map(GridSize::new(100, 100), &blocked, &blocked, &[(10, 11)]);
    let (mut open, _) = walled_map(GridSize::new(100, 100), &[(10, 11)], &blocked, &[]);
    let mut reach = |range, gap, speed| {
        EnterAction::in_reach(
            range,
            gap,
            speed,
            &mut walls(&mut walled, &stats),
            ObjectsRef::of_masters(&masters),
        )
    };
    assert_eq!(
        reach(Distance::new(1.0), DistanceSquared::new(1.0), Speed::ZERO),
        Ok(true)
    );
    assert_eq!(
        reach(Distance::new(1.0), DistanceSquared::new(1.01), Speed::ZERO),
        Ok(false)
    );
    assert_eq!(
        reach(
            Distance::new(1.0),
            DistanceSquared::new(f32::NAN),
            Speed::ZERO
        ),
        Ok(true)
    );
    assert_eq!(
        reach(
            Distance::new(1.5),
            DistanceSquared::new(0.5),
            Speed::new(0.8)
        ),
        Ok(true)
    );
    assert_eq!(
        reach(
            Distance::new(f32::NAN),
            DistanceSquared::new(0.5),
            Speed::new(0.8)
        ),
        Ok(true)
    );
    assert_eq!(
        reach(
            Distance::new(1.0),
            DistanceSquared::new(0.5),
            Speed::new(f32::NAN)
        ),
        Ok(true)
    );
    assert_eq!(
        reach(
            Distance::new(1.0),
            DistanceSquared::new(0.5),
            Speed::new(0.8)
        ),
        Ok(false)
    );
    assert_eq!(
        EnterAction::in_reach(
            Distance::new(1.0),
            DistanceSquared::new(0.5),
            Speed::new(0.8),
            &mut walls(&mut open, &stats),
            ObjectsRef::of_masters(&masters)
        ),
        Ok(true)
    );
}

#[test]
fn an_enter_of_unit_without_speed_starts_failed_beyond_arrived() {
    // `0x140aa2100`: beyond ARRIVED of its target a MoveTo without speed starts in state 13
    let (mut world, list_index) = walk_again_world();
    walks_at(&mut world, list_index, 0.0);
    let tc = EnterTarget {
        id: ContainerId::new(object_id(100)),
        state: ObjectState::Alive,
        allied: true,
        room: true,
        position: Position::new(104.0, 47.0, 0.0),
        in_reach: false,
        kind: EnterKind::Building,
    };
    let mut enter_action = EnterAction::new(
        ContainerId::new(object_id(100)),
        Position::new(104.0, 47.0, 0.0),
        &enter_task(Distance::new(1.0), class::BUILDING),
    );
    assert_eq!(
        world.enter_walk_again(
            PlayerId::new(1),
            list_index,
            &mut enter_action,
            &tc,
            WalkStart {
                gap: Distance::new(3.5)
            }
        ),
        Ok(Walking::Walk(None))
    );
    assert_eq!(
        enter_action
            .state
            .move_to()
            .copied()
            .map(|move_to_action| move_to_action.state),
        Some(MoveTo::Failed)
    );
}

#[test]
fn transport_enter_walks_to_it_and_sends_it_where_they_meet() {
    // FUN_140a87320 case 4 on a transport: the point and code of FUN_140a85f30. An idle ship
    // meets the villager on the shore on the way (code 1): the transport is sent there at its
    // own height; one under a MoveTo (state 3) where it goes (code 0): not sent. The MoveTo
    // goes to the transport within 0
    let mut world = shore_world(Point::new(5.5, 12.5), Point::new(5.5, 30.5));
    let list_index = villager(&world);
    let ship = EnterTarget {
        id: ContainerId::new(object_id(200)),
        state: ObjectState::Alive,
        allied: true,
        room: true,
        position: Position::new(5.5, 30.5, 0.25),
        in_reach: false,
        kind: EnterKind::Transport,
    };
    let start = WalkStart {
        gap: Distance::new(6.0),
    };
    let mut enter = EnterAction::new(
        ContainerId::new(object_id(200)),
        Position::new(5.5, 30.5, 0.0),
        &enter_task(Distance::new(1.0), class::TRANSPORT),
    );
    assert_eq!(
        world.enter_walk_again(PlayerId::new(1), list_index, &mut enter, &ship, start),
        Ok(Walking::Walk(Some(Position::new(5.5, 17.5, 0.25))))
    );
    assert_eq!(
        (
            enter.point,
            enter
                .state
                .move_to()
                .copied()
                .map(|move_to_action| move_to_action.state)
        ),
        (Position::new(5.5, 17.5, 0.0), Some(MoveTo::Start))
    );
    assert_eq!(enter.walk_range(EnterKind::Transport), Distance::ZERO);
    assert_eq!(enter.walk_range(EnterKind::Building), Distance::new(1.0));
    world
        .parts
        .actions
        .of_mut(object_id(200))
        .unwrap()
        .list
        .set_move_to(Some(MoveToAction {
            sprite: None,
            state: MoveTo::Start,
            point: Position::new(9.5, 19.5, 0.0),
            target: None,
            range: Distance::ZERO,
            voice: Voice::Loud,
            on_target_moved: TargetMoved::Follows,
            wait: MoveWait::default(),
        }));
    assert_eq!(
        world.enter_walk_again(PlayerId::new(1), list_index, &mut enter, &ship, start),
        Ok(Walking::Walk(None))
    );
    assert_eq!(enter.point, Position::new(9.5, 19.5, 0.0));
}

/// Villager 1000's index in player 1's list.
fn villager(world: &World) -> ListIndex {
    world.players[1]
        .objects
        .position_of(object_id(1000))
        .expect("the villager")
}

/// [`shore_world`] with villager 1000 at `at` under `enter`, ship 200 at `ship`; the
/// villager's master has no sprites (the world's sprites' table is empty), its walk 16 facets.
fn entering(at: Point, ship: Point, enter: EnterAction) -> World {
    let mut world = shore_world(at, ship);
    for masters in &mut world.model.masters {
        if let Some(master) = Arc::make_mut(masters).get_mut(VILLAGER) {
            master.standing_sprite = None;
            let moving = master.moving.get_or_insert_with(Default::default);
            (moving.move_sprite, moving.run_sprite) = (None, None);
        }
    }
    // its walk turns by 16 facets
    if let Some(mover) = world.parts.mover_mut(object_id(1000)) {
        mover.sprite_facets = Some(SpriteFacets {
            count: FacetCount::of_data(16),
            turns: false,
        });
    }
    world
        .parts
        .actions
        .of_mut(object_id(1000))
        .unwrap()
        .list
        .set_enter(Some(enter));
    world
}

/// An Enter on ship 200 at `point` whose `MoveTo` is in `state`.
fn ship_enter(point: Position, state: MoveTo) -> EnterAction {
    let mut enter_action = EnterAction::new(
        ContainerId::new(object_id(200)),
        point,
        &enter_task(Distance::new(1.0), class::TRANSPORT),
    )
    .walking(state);
    // a transport's MoveTo walks to it within 0
    if let Some(move_to_action) = enter_action.state.walk_mut().and_then(Option::as_mut) {
        move_to_action.range = Distance::ZERO;
    }
    enter_action
}

/// One update of `dt` ms of villager 1000's Enter (its part of the unit's update,
/// [`Unit::enter_part`]) and its code.
fn enter_update(world: &mut World, dt: u32) -> GenieResult<HeadCode> {
    let list_index = villager(world);
    in_update(
        world,
        (PlayerId::new(1), list_index),
        dt,
        |unit, actions, step| {
            let me = Me {
                human_order: WorldTime::ZERO,
                id: unit.id,
                group: unit.group.id,
                action_state: actions
                    .top_action()
                    .map_or(ActionState::NO_ACTION, |top_action| top_action.state),
                top_speed: TopSpeed::Own,
            };
            unit.enter_part(actions, step, me, &roles())
        },
    )
}

/// Villager 1000's Enter.
fn enter_of(world: &World) -> EnterAction {
    world
        .parts
        .actions_of(object_id(1000))
        .list
        .enter()
        .copied()
        .expect("its Enter")
}

/// Ship 200's `MoveTo` goal, if it has one.
fn ship_goal(world: &World) -> Option<Position> {
    world
        .parts
        .actions_of(object_id(200))
        .list
        .move_to()
        .and_then(MoveToAction::goal)
}

/// The notifies 0x1fa (`NOTIFY_ACTION_COMPLETED`) villager 1000's unit AI heard.
fn completed(world: &World) -> usize {
    world
        .parts
        .unit_ai
        .get(world.object(object_id(1000)).unwrap().1.id)
        .unwrap()
        .notifies
        .iter()
        .filter(|notify| notify.kind == NotifyKind::ACTION_COMPLETED)
        .count()
}

#[test]
fn transport_that_moved_off_the_point_makes_the_enter_walk_again() {
    // FUN_140a87a80 state 4: FUN_140a7bbf0 (not on the point) and FUN_140a7bd10 (more than
    // half a tile off: COMISS/JA) set_state(4): the ship meets the villager on the shore (code
    // 1) and is sent there at once (slot 0x3d8, then slot 0x340); then the new MoveTo updates
    let mut game = entering(
        Point::new(5.5, 12.5),
        Point::new(5.5, 30.5),
        ship_enter(Position::new(5.5, 28.0, 0.0), MoveTo::Arrived),
    );
    assert_eq!(enter_update(&mut game, 500), Ok(HeadCode::Keeps));
    assert_eq!(ship_goal(&game), Some(Position::new(5.5, 17.5, 0.0)));
    let enter = enter_of(&game);
    assert_eq!(
        (enter.state.bare(), enter.point),
        (
            EnterState::Walk { move_to: None },
            Position::new(5.5, 17.5, 0.0)
        )
    );
    // within half a tile on each axis it does not walk again, and a NaN is not off: code 1
    // out of reach waits (set_state(2))
    for y in [30.0, f32::NAN] {
        let mut world = entering(
            Point::new(5.5, 12.5),
            Point::new(5.5, 30.5),
            ship_enter(Position::new(5.5, y, 0.0), MoveTo::Arrived),
        );
        assert_eq!(enter_update(&mut world, 500), Ok(HeadCode::Keeps));
        assert_eq!(ship_goal(&world), None);
        let enter_action = enter_of(&world);
        assert_eq!(
            (
                enter_action.state.bare(),
                enter_action.timer,
                enter_action.state.move_to().copied()
            ),
            (EnterState::Wait, Seconds::new(1.0), None)
        );
    }
    // a transport that cannot reach the point (where it last moved to is off the map):
    // set_state(13), not modelled
    let mut world = entering(
        Point::new(5.5, 12.5),
        Point::new(5.5, 30.5),
        ship_enter(Position::new(5.5, 28.0, 0.0), MoveTo::Arrived),
    );
    world.parts.mover_mut(object_id(200)).unwrap().moved_at = Position::new(-1.0, 30.5, 0.0);
    assert_eq!(
        enter_update(&mut world, 500).map_err(|error| error.reason().to_owned()),
        Err(UNREACHED.reason().to_owned())
    );
}

/// The Enter's waiting `MoveTo` (state 2, after the motion step gave up): the world's dt
/// onto its timer (code 0: nothing else happens); at its delay it asks again.
#[test]
fn an_enter_waiting_move_to_asks_again_at_its_delay() {
    let mut ship_enter = ship_enter(Position::new(5.5, 30.5, 0.0), MoveTo::Waiting);
    if let Some(move_to_action) = ship_enter.state.walk_mut().and_then(Option::as_mut) {
        move_to_action.wait = MoveWait {
            state: MoveWaitState::Waiting,
            timer: Seconds::ZERO,
            delay: Seconds::new(1.0),
        };
    }
    let mut world = entering(Point::new(5.5, 12.5), Point::new(5.5, 30.5), ship_enter);
    assert_eq!(enter_update(&mut world, 500), Ok(HeadCode::Keeps));
    let enter = enter_of(&world);
    assert_eq!(
        (
            enter.state.bare(),
            enter
                .state
                .move_to()
                .copied()
                .map(|move_to_action| (move_to_action.state, move_to_action.wait.timer))
        ),
        (
            EnterState::Walk { move_to: None },
            Some((MoveTo::Waiting, Seconds::new(0.5)))
        )
    );
    world
        .parts
        .actions
        .of_mut(object_id(1000))
        .unwrap()
        .list
        .enter_mut()
        .unwrap()
        .state
        .walk_mut()
        .and_then(Option::as_mut)
        .unwrap()
        .wait
        .timer = Seconds::new(1.0);
    assert_eq!(enter_update(&mut world, 500), Ok(HeadCode::Keeps));
    let enter_action = enter_of(&world);
    assert_eq!(
        enter_action
            .state
            .move_to()
            .copied()
            .map(|move_to_action| (move_to_action.state, move_to_action.wait.timer)),
        Some((MoveTo::Start, Seconds::ZERO))
    );
}

#[test]
fn walk_again_into_reach_enters_and_the_update_enters_again() {
    // the set_state(4) of a moved transport finds the unit in reach: FUN_140a867b0 at once (the
    // unit inside, the ship's list holding it, notify 0x1fa), then the update's code 1 (no
    // MoveTo) in reach enters again (notify 0x1fa and set_state(1), then its own)
    let mut world = entering(
        Point::new(5.5, 29.5),
        Point::new(5.5, 30.5),
        ship_enter(Position::new(5.5, 28.0, 0.0), MoveTo::Arrived),
    );
    assert_eq!(enter_update(&mut world, 500), Ok(HeadCode::Keeps));
    assert_eq!(
        world.object(object_id(1000)).unwrap().1.life.inside,
        Some(container_id(200))
    );
    assert_eq!(
        world.object(object_id(200)).unwrap().1.garrison,
        [object_id(1000)]
    );
    assert_eq!(
        (enter_of(&world).state, completed(&world)),
        (EnterState::Done, 2)
    );
    // without the walk again, code 1 in reach enters once
    let mut world = entering(
        Point::new(5.5, 29.5),
        Point::new(5.5, 30.5),
        ship_enter(Position::new(5.5, 30.5, 0.0), MoveTo::Arrived),
    );
    assert_eq!(enter_update(&mut world, 500), Ok(HeadCode::Keeps));
    assert_eq!(
        world.object(object_id(1000)).unwrap().1.life.inside,
        Some(container_id(200))
    );
    assert_eq!(
        (enter_of(&world).state, completed(&world)),
        (EnterState::Done, 1)
    );
}

#[test]
fn failed_move_walks_again_once_then_waits_and_wait_walks_again() {
    // code 3: +0x68 lets one set_state(4); the next waits a second (set_state(2)), whose end
    // walks again; the town centre is a building: no meeting
    let failed = EnterAction::new(
        ContainerId::new(object_id(100)),
        Position::new(10.0, 10.0, 0.0),
        &enter_task(Distance::new(1.0), class::BUILDING),
    )
    .walking(MoveTo::Failed);
    let mut world = entering(Point::new(5.5, 5.5), Point::new(5.5, 30.5), failed);
    assert_eq!(enter_update(&mut world, 500), Ok(HeadCode::Keeps));
    let enter_action = enter_of(&world);
    assert_eq!(
        (
            enter_action.retry,
            enter_action
                .state
                .move_to()
                .copied()
                .map(|move_to_action| move_to_action.state),
            enter_action.state.bare()
        ),
        (
            false,
            Some(MoveTo::Start),
            EnterState::Walk { move_to: None }
        )
    );
    world
        .parts
        .actions
        .of_mut(object_id(1000))
        .unwrap()
        .list
        .enter_mut()
        .unwrap()
        .state
        .walk_mut()
        .and_then(Option::as_mut)
        .unwrap()
        .state = MoveTo::Failed;
    assert_eq!(enter_update(&mut world, 500), Ok(HeadCode::Keeps));
    let enter = enter_of(&world);
    assert_eq!(
        (
            enter.state.bare(),
            enter.state.move_to().copied(),
            enter.timer
        ),
        (EnterState::Wait, None, Seconds::new(1.0))
    );
    // the wait counts down by dt (0.5): at 0 it walks again
    assert_eq!(enter_update(&mut world, 500), Ok(HeadCode::Keeps));
    assert_eq!(
        (enter_of(&world).state.bare(), enter_of(&world).timer),
        (EnterState::Wait, Seconds::new(0.5))
    );
    assert_eq!(enter_update(&mut world, 500), Ok(HeadCode::Keeps));
    let enter_action = enter_of(&world);
    assert_eq!(
        (
            enter_action.state.bare(),
            enter_action
                .state
                .move_to()
                .copied()
                .map(|move_to_action| move_to_action.state)
        ),
        (EnterState::Walk { move_to: None }, Some(MoveTo::Start))
    );
}

/// Master `master`'s room becomes `units` in every player's table.
pub(crate) fn give_room(world: &mut World, master: MasterId, units: u8) {
    for masters in &mut world.model.masters {
        if let Some(found) = Arc::make_mut(masters).get_mut(master) {
            found.garrison = GarrisonCapacity::of_data(units);
        }
    }
}

/// Puts unit `id` of player `player_id` inside object `container`, as one unit of what it
/// holds (slot 0x498 counts the objects whose `+0x2f0` names it).
fn held_by(world: &mut World, player_id: PlayerId, id: i32, container: i32) {
    let mut held = unit(id, VILLAGER, HitPoints::new(25.0));
    held.life.inside = Some(container_id(container));
    let _ = world.push_live(player_id, held).unwrap();
}

/// The update's room test (`FUN_140a87a80`'s start, on what `FUN_140a85f30` reads: the target's
/// slot 0x198 with the unit, `FUN_1412169b0`) counts what the entering unit holds: town centre
/// 100 of room 2 with a villager inside has room for villager 1000 (1 + 1), and none once a
/// unit is inside the villager (1 + 1 + 1), which is state 11, not modelled; slot 0x198 with
/// no unit would find room for one more (1 + 1).
#[test]
fn enter_update_room_counts_what_entering_unit_holds() {
    let walking = || {
        let failed = EnterAction::new(
            ContainerId::new(object_id(100)),
            Position::new(10.0, 10.0, 0.0),
            &enter_task(Distance::new(1.0), class::BUILDING),
        )
        .walking(MoveTo::Failed);
        let mut world = entering(Point::new(5.5, 5.5), Point::new(5.5, 30.5), failed);
        give_room(&mut world, TC, 2);
        inside_tc(&mut world, PlayerId::new(1), 1001, VILLAGER);
        world
    };
    let mut world = walking();
    assert_eq!(enter_update(&mut world, 500), Ok(HeadCode::Keeps));
    let mut holding = walking();
    held_by(&mut holding, PlayerId::new(1), 1002, 1000);
    assert_eq!(
        enter_update(&mut holding, 500).map_err(|error| error.reason().to_owned()),
        Err("an Enter whose building has no room (state 11)".to_owned())
    );
}

/// A transport target, room for the unit, 1.0 tiles off its point on y.
fn moved_ship(y: f32) -> EnterTarget {
    EnterTarget {
        id: ContainerId::new(object_id(7420)),
        state: ObjectState::Alive,
        allied: true,
        room: true,
        position: Position::new(82.5, y, 0.0),
        in_reach: false,
        kind: EnterKind::Transport,
    }
}

/// The Enter's test of its target (`FUN_140a87a80`'s start): a target gone, dead (state 3 or
/// more), of an unallied player or without room is state 11, which is not modelled; a held
/// one passes.
#[test]
fn an_enter_holds_only_live_allied_target_with_room() {
    let ship = moved_ship(89.5);
    let reason = |enter_target| EnterAction::held_target(enter_target).unwrap_err().reason();
    assert_eq!(reason(None), "an Enter whose building is gone (state 11)");
    let died = "an Enter whose building died or is not allied (state 11)";
    assert_eq!(
        reason(Some(EnterTarget {
            state: ObjectState::Dying,
            ..ship
        })),
        died
    );
    let unallied = EnterTarget {
        allied: false,
        ..ship
    };
    assert_eq!(reason(Some(unallied)), died);
    let full = EnterTarget {
        room: false,
        ..ship
    };
    assert_eq!(
        reason(Some(full)),
        "an Enter whose building has no room (state 11)"
    );
    assert_eq!(EnterAction::held_target(Some(ship)), Ok(ship));
}

/// State 4's test of a moved target (`FUN_140a7bbf0`, `FUN_140a7bd10`): a target on the point,
/// or off it by at most half a tile on every axis, stays; more than half a tile on one axis
/// moved; a NaN offset compares as not moved (`COMISS`/`JA`).
#[test]
fn an_enter_target_moved_only_beyond_half_tile() {
    let enter = EnterAction::new(
        ContainerId::new(object_id(7420)),
        Position::new(82.5, 89.5, 0.0),
        &enter_task(Distance::new(1.0), class::TRANSPORT),
    );
    let at = |y: f32| EnterTarget {
        position: Position::new(82.5, y, 0.0),
        ..moved_ship(y)
    };
    assert!(!enter.target_moved(&at(89.5)));
    assert!(!enter.target_moved(&at(90.0)));
    assert!(enter.target_moved(&at(90.1)));
    assert!(enter.target_moved(&at(88.9)));
    assert!(!enter.target_moved(&at(f32::NAN)));
}

#[test]
fn meeting_point_is_the_shore_on_the_way_else_beach_under_the_unit() {
    // FUN_140a85f30's branch by the target: an idle transport (state 0 or 2) with room and
    // speed meets the unit at the shore; without room or speed (or with a negative one; a NaN
    // has some) at its place; moving (state 4) where its action goes, but a siege weapon at its
    // place; another state at its place
    let ship = MeetingTarget {
        room: true,
        speed: Speed::new(1.1),
        state: ActionState::NO_ACTION,
        class: class::TRANSPORT,
        position: Point::new(79.5, 92.5),
        action_point: Err(UNPLACED),
    };
    assert_eq!(meet_at(&ship, &roles()), MeetAt::Shore);
    let idle = MeetingTarget {
        state: ActionState::new(2),
        ..ship.clone()
    };
    assert_eq!(meet_at(&idle, &roles()), MeetAt::Shore);
    for meeting_target in [
        MeetingTarget {
            room: false,
            ..ship.clone()
        },
        MeetingTarget {
            speed: Speed::ZERO,
            ..ship.clone()
        },
        MeetingTarget {
            speed: Speed::new(-1.0),
            ..ship.clone()
        },
    ] {
        assert_eq!(meet_at(&meeting_target, &roles()), MeetAt::Place);
    }
    let nan = MeetingTarget {
        speed: Speed::new(f32::NAN),
        ..ship.clone()
    };
    assert_eq!(meet_at(&nan, &roles()), MeetAt::Shore);
    let moving = MeetingTarget {
        state: ActionState::new(4),
        action_point: Ok(Point::new(60.0, 70.0)),
        ..ship
    };
    assert_eq!(meet_at(&moving, &roles()), MeetAt::ActionPoint);
    let siege = MeetingTarget {
        class: class::SIEGE,
        ..moving.clone()
    };
    assert_eq!(meet_at(&siege, &roles()), MeetAt::Place);
    assert_eq!(
        meet_at(
            &MeetingTarget {
                state: ActionState::new(7),
                ..moving
            },
            &roles()
        ),
        MeetAt::Place
    );
    // the shore on the way: from the ship's tile (5, 30) toward the villager's (5, 12) the
    // first beach (5, 17), at its centre, code 1
    let meet = |world: &mut World| {
        let list_index = villager(world);
        world.enter_meeting(
            PlayerId::new(1),
            list_index,
            ContainerId::new(object_id(200)),
        )
    };
    let mut world = shore_world(Point::new(5.5, 12.5), Point::new(5.5, 30.5));
    assert_eq!(meet(&mut world), Ok((Point::new(5.5, 17.5), 1)));
    // no shore on the way (the two on one tile): a beach under the unit gives its place rounded
    // (floorf of x + 0.5), code 1
    let mut world = shore_world(Point::new(5.6, 17.25), Point::new(5.4, 17.5));
    assert_eq!(meet(&mut world), Ok((Point::new(6.0, 17.0), 1)));
    // neither: the ship's place, code 0
    let mut world = shore_world(Point::new(5.6, 12.25), Point::new(5.4, 12.5));
    assert_eq!(meet(&mut world), Ok((Point::new(5.4, 12.5), 0)));
}

#[test]
fn the_landing_walk_steps_twice_per_tile_of_the_longer_axis() {
    // FUN_141157840 from (79, 92) to (83, 86): the y axis is longer (6), 12 steps of
    // (1/3, -1/2) in floats (79 + 3 steps is 80.00001); the tiles in the order they are
    // first reached
    let tiles = |from, to| {
        LandingWalk::new(from, to)
            .map(|(tile, _)| tile)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        tiles(Tile::new(79, 92), Tile::new(83, 86)),
        [
            Tile::new(79, 91),
            Tile::new(80, 90),
            Tile::new(80, 89),
            Tile::new(81, 89),
            Tile::new(81, 88),
            Tile::new(82, 87),
            Tile::new(82, 86),
            Tile::new(83, 86),
        ]
    );
    // each with the float place reached there
    let at = LandingWalk::new(Tile::new(79, 92), Tile::new(83, 86))
        .find(|&(tile, _)| tile == Tile::new(81, 89));
    assert_eq!(at, Some((Tile::new(81, 89), Point::new(81.000_015, 89.0))));
    // equal axes: x is the longer (`<=`); one tile: none
    assert_eq!(
        tiles(Tile::new(0, 0), Tile::new(-2, 2)),
        [Tile::new(0, 0), Tile::new(-1, 1), Tile::new(-2, 2)]
    );
    assert_eq!(tiles(Tile::new(5, 5), Tile::new(5, 5)), Vec::<Tile>::new());
}

/// A map of `size` tiles whose `blocked` tiles hold a terrain class outside the walker's mask
/// (bit 1), with a town centre (a building) on each tile of `buildings` and a villager (no
/// building) on each of `others`, and the walker (id 1, x radius 0.2, mask 1).
fn walled_map(
    size: GridSize,
    blocked: &[(i32, i32)],
    buildings: &[(i32, i32)],
    others: &[(i32, i32)],
) -> (Spatial, MoveStats) {
    use crate::map::spatial::{Placed, Spatial, Tiles};
    let (width, height) = (size.width(), size.height());
    let mut words = vec![0u8; (width * height) as usize];
    for &(x, y) in blocked {
        words[(y * width + x) as usize] = 0x02;
    }
    let mut spatial = Spatial::with_tiles(size, Tiles::new(GridSize::new(width, height), words));
    let at = |(x, y): (i32, i32), master: MasterId| Placed {
        position: Position::new(x as f32 + 0.5, y as f32 + 0.5, 0.0),
        radius: Footprint::new(0.2, 0.2),
        class: class::CLASS_0,
        player: PlayerId::new(1),
        master,
        obstruction_kind: ObstructionKind::NONE,
        buildable_on: BuildableOn::NO,
        tall: false,
        moves: false,
        unknown: None,
        obstruction: None,
    };
    let placed = buildings
        .iter()
        .map(|&tile| at(tile, TC))
        .chain(others.iter().map(|&tile| at(tile, VILLAGER)));
    for (id, placed) in (10..).zip(placed) {
        spatial.insert(object_id(id), placed);
    }
    let stats = MoveStats {
        fly_mode: FlyMode::GROUND,
        speed: Speed::new(0.8),
        base: Speed::new(0.8),
        turn_rate: TurnRate::MAX,
        turn_radius: Distance::ZERO,
        turn_radius_speed: Speed::MAX,
        radius: Radii::new(0.2, 0.2, 0.5),
        restriction: RestrictionId::named(0),
        restriction_mask: TerrainMask::of_bits(1),
        turn_time: Seconds::ZERO,
        move_algorithm: MoveAlgorithm::NONE,
        min_collision: 1.0,
        fast_turn: false,
        facets: None,
        weapon_range: Distance::ZERO,
    };
    (spatial, stats)
}

#[test]
fn the_reach_tile_test_counts_buildings_where_the_unit_cannot_stand() {
    // FUN_140a85c30 for range <= 1 and a target with speed: the box of the two tiles (one
    // tile and the next when it spans more than one), the two tiles left out
    let masters = [Masters::default(), garrison_masters()].map(Arc::new);
    let walled = |spatial: &mut Spatial, stats, me, them| {
        walled_off(
            me,
            them,
            spatial,
            (object_id(1), stats),
            ObjectsRef::of_masters(&masters),
        )
    };
    let blocked = [(10, 11), (11, 10)];
    let (mut spatial, stats) = walled_map(GridSize::new(100, 100), &blocked, &blocked, &[(10, 11)]);
    assert_eq!(
        walled(
            &mut spatial,
            &stats,
            Point::new(10.5, 10.5),
            Point::new(11.5, 11.5)
        ),
        Ok(true)
    );
    // the unit's and the target's own tiles do not count, nor what is not a building
    let (mut spatial, stats) = walled_map(
        GridSize::new(100, 100),
        &blocked,
        &[(10, 10), (10, 11)],
        &[(11, 10)],
    );
    assert_eq!(
        walled(
            &mut spatial,
            &stats,
            Point::new(10.5, 10.5),
            Point::new(11.5, 11.5)
        ),
        Ok(false)
    );
    // a tile the unit could stand on does not count
    let (mut spatial, stats) = walled_map(GridSize::new(100, 100), &[(10, 11)], &blocked, &[]);
    assert_eq!(
        walled(
            &mut spatial,
            &stats,
            Point::new(10.5, 10.5),
            Point::new(11.5, 11.5)
        ),
        Ok(false)
    );
    // far apart: the first tile of the box and the next, within the map
    let far = [(95, 2), (96, 3)];
    let (mut spatial, stats) = walled_map(GridSize::new(97, 100), &far, &far, &[]);
    assert_eq!(
        walled(
            &mut spatial,
            &stats,
            Point::new(98.5, 2.5),
            Point::new(95.5, 9.5)
        ),
        Ok(true)
    );
    let (mut spatial, stats) = walled_map(GridSize::new(97, 100), &far, &[(95, 2), (95, 4)], &[]);
    assert_eq!(
        walled(
            &mut spatial,
            &stats,
            Point::new(98.5, 2.5),
            Point::new(95.5, 9.5)
        ),
        Ok(false)
    );
    // a master the slice lacks on a counted tile fails
    let masters = [Masters::default(), Masters::default()].map(Arc::new);
    let (mut spatial, stats) = walled_map(GridSize::new(100, 100), &blocked, &blocked, &[]);
    assert!(
        walled_off(
            Point::new(10.5, 10.5),
            Point::new(11.5, 11.5),
            &mut spatial,
            (object_id(1), &stats),
            ObjectsRef::of_masters(&masters)
        )
        .is_err()
    );
    // unless the object has a master of its own: its master pointer reads that one
    // (`MasterTables::of_placed`)
    let town_centre = garrison_masters().get(TC).cloned().unwrap();
    let parts = Parts {
        own_master: [10, 11]
            .map(|id| (object_id(id), town_centre.clone()))
            .into_iter()
            .collect(),
        ..Parts::default()
    };
    let (mut spatial, stats) = walled_map(GridSize::new(100, 100), &blocked, &blocked, &[]);
    assert_eq!(
        walled_off(
            Point::new(10.5, 10.5),
            Point::new(11.5, 11.5),
            &mut spatial,
            (object_id(1), &stats),
            ObjectsRef::new(
                &[],
                ObjectIndex::scanning(),
                &masters,
                &parts,
                static_roles(),
                tuning(),
                None,
            )
        ),
        Ok(true)
    );
}

#[test]
fn town_center_takes_villagers_infantry_cavalry_and_monks() {
    // a town center's garrison_type 11: bits 0, 1 and 3
    assert!(garrison_kind_takes(
        GarrisonKinds::VILLAGERS | GarrisonKinds::INFANTRY | GarrisonKinds::MONKS,
        class::VILLAGER,
        ids::VILLAGER_MALE,
        GarrisonSwitch::Garrison,
        &roles()
    ));
    assert!(garrison_kind_takes(
        GarrisonKinds::VILLAGERS | GarrisonKinds::INFANTRY | GarrisonKinds::MONKS,
        class::CLASS_6,
        master_id(74),
        GarrisonSwitch::Garrison,
        &roles()
    ));
    assert!(!garrison_kind_takes(
        GarrisonKinds::VILLAGERS | GarrisonKinds::INFANTRY | GarrisonKinds::MONKS,
        class::CLASS_12,
        master_id(38),
        GarrisonSwitch::Garrison,
        &roles()
    ));
    assert!(garrison_kind_takes(
        GarrisonKinds::VILLAGERS | GarrisonKinds::INFANTRY | GarrisonKinds::MONKS,
        class::MONK,
        master_id(125),
        GarrisonSwitch::Garrison,
        &roles()
    ));
    // master 1811 is no infantry, but a monk
    assert!(garrison_kind_takes(
        GarrisonKinds::VILLAGERS | GarrisonKinds::INFANTRY | GarrisonKinds::MONKS,
        class::CLASS_6,
        ids::WARRIOR_PRIEST,
        GarrisonSwitch::Garrison,
        &roles()
    ));
    assert!(!garrison_kind_takes(
        GarrisonKinds::INFANTRY,
        class::CLASS_6,
        ids::WARRIOR_PRIEST,
        GarrisonSwitch::Garrison,
        &roles()
    ));
    // a tower's 3 takes no monk; the task finder takes no class 42
    assert!(!garrison_kind_takes(
        GarrisonKinds::VILLAGERS | GarrisonKinds::INFANTRY,
        class::MONK,
        master_id(125),
        GarrisonSwitch::Garrison,
        &roles()
    ));
    assert!(garrison_kind_takes(
        GarrisonKinds::MONKS,
        class::RELIC,
        master_id(1),
        GarrisonSwitch::Garrison,
        &roles()
    ));
    assert!(!garrison_kind_takes(
        GarrisonKinds::MONKS,
        class::RELIC,
        master_id(1),
        GarrisonSwitch::TaskFinder,
        &roles()
    ));
}

#[test]
fn villager_inside_town_center_adds_one_missile_to_its_volleys() {
    // MarvinBlue/426048731's town center 9149 (the tick oracle, update 4439): pierce 5 by its
    // volley missile 54 every 2 s; its 7 villagers made its firepower 1 + 7
    let arrow = Master {
        id: master_id(54),
        kind: MasterKind::MISSILE,
        combat: Some(CombatPart {
            weapons: vec![ClassAmount::new(3, 5)],
            ..CombatPart::default()
        }),
        ..Master::default()
    };
    let tc = Master {
        id: ids::TOWN_CENTRE,
        kind: MasterKind::BUILDING,
        class: class::BUILDING,
        creatable: Some(CreatablePart {
            volley_missile: Some(master_id(54)),
            volley_fire: Firepower::new(1.0),
            max_volley: MissileCount::of_data(11),
            ..CreatablePart::default()
        }),
        combat: Some(CombatPart {
            reload: Seconds::new(2.0),
            weapon_range: Distance::new(6.0),
            weapons: vec![ClassAmount::new(3, 5)],
            ..CombatPart::default()
        }),
        ..Master::default()
    };
    let villager = Master {
        id: ids::VILLAGER_MALE,
        kind: MasterKind::COMBAT,
        class: class::VILLAGER,
        combat: Some(CombatPart {
            reload: Seconds::new(2.0),
            weapons: vec![
                ClassAmount::new(11, 3),
                ClassAmount::new(4, 3),
                ClassAmount::new(13, 6),
                ClassAmount::new(24, 0),
                ClassAmount::new(17, 0),
            ],
            garrison_firepower: Firepower::new(-2.5),
            ..CombatPart::default()
        }),
        ..Master::default()
    };
    let masters = Masters::new([arrow, tc.clone(), villager.clone()]);
    let rate = attack_rate(&tc, &masters, Seconds::ZERO, &roles(), &tuning().garrison);
    assert_eq!(rate, DamageRate::new(2.5));
    // a garrison firepower at most 0 is its magnitude
    assert_eq!(
        firepower(&villager, &masters, Seconds::ZERO),
        DamageRate::new(2.5)
    );
    assert_eq!(
        share(&villager, &masters, Seconds::ZERO, rate, &roles()),
        Ok(Firepower::new(1.0))
    );
    assert_eq!(
        share(
            &villager,
            &masters,
            Seconds::ZERO,
            DamageRate::new(0.0),
            &roles()
        ),
        Ok(Firepower::new(0.0))
    );
    // one above 0 times the melee, pierce, class 31 and 38 amounts over the reload
    let armed = Master {
        combat: Some(CombatPart {
            garrison_firepower: Firepower::new(0.5),
            ..villager.combat.clone().unwrap_or_default()
        }),
        ..villager
    };
    assert_eq!(
        firepower(&armed, &masters, Seconds::ZERO),
        DamageRate::new(0.75)
    );
}

/// A town centre's master as `villager_inside_town_center_adds_one_missile_to_its_volleys`
/// has it.
fn town_center() -> Master {
    Master {
        id: ids::TOWN_CENTRE,
        kind: MasterKind::BUILDING,
        class: class::BUILDING,
        creatable: Some(CreatablePart {
            volley_missile: Some(master_id(54)),
            volley_fire: Firepower::new(1.0),
            ..CreatablePart::default()
        }),
        combat: Some(CombatPart {
            reload: Seconds::new(2.0),
            weapon_range: Distance::new(6.0),
            weapons: vec![ClassAmount::new(3, 5)],
            ..CombatPart::default()
        }),
        ..Master::default()
    }
}

#[test]
fn an_attack_rate_needs_reload_and_range_that_compare_above_their_limits() {
    // FUN_14120dc10: COMISS reload, 0 and COMISS range, 1.0, each with JBE: an unordered
    // value gives no rate, as one at the limit does
    let arrow = Master {
        id: master_id(54),
        kind: MasterKind::MISSILE,
        combat: Some(CombatPart {
            weapons: vec![
                ClassAmount::new(3, 5),
                ClassAmount::new(4, 7),
                ClassAmount::new(38, 2),
            ],
            ..CombatPart::default()
        }),
        ..Master::default()
    };
    let masters = Masters::new([arrow, town_center()]);
    assert_eq!(
        attack_rate(
            &town_center(),
            &masters,
            Seconds::ZERO,
            &roles(),
            &tuning().garrison
        ),
        DamageRate::new(3.5)
    );
    for (reload, range) in [(f32::NAN, 6.0), (2.0, f32::NAN), (0.0, 6.0), (2.0, 1.0)] {
        let tc = Master {
            combat: Some(CombatPart {
                reload: Seconds::new(reload),
                weapon_range: Distance::new(range),
                ..town_center().combat.clone().unwrap_or_default()
            }),
            ..town_center()
        };
        assert_eq!(
            attack_rate(&tc, &masters, Seconds::ZERO, &roles(), &tuning().garrison)
                .get()
                .to_bits(),
            0.0f32.to_bits()
        );
    }
    // classes 4 and 6 sum their own melee, pierce, 31 and 38 amounts, not the missile's
    let infantry = Master {
        class: class::CLASS_6,
        combat: Some(CombatPart {
            weapons: vec![
                ClassAmount::new(4, 4),
                ClassAmount::new(3, 1),
                ClassAmount::new(31, 2),
                ClassAmount::new(38, 1),
                ClassAmount::new(11, 9),
            ],
            ..town_center().combat.unwrap_or_default()
        }),
        ..town_center()
    };
    assert_eq!(
        attack_rate(
            &infantry,
            &masters,
            Seconds::new(0.5),
            &roles(),
            &tuning().garrison
        ),
        DamageRate::new(8.0 / 1.5)
    );
}

#[test]
fn volley_missile_slot_the_table_holds_is_taken_even_when_it_is_empty() {
    // FUN_14120e1a0: the volley missile when 0 <= id < the player's master count, else the
    // missile; a null slot or one without weapons adds nothing, with no fallback
    let missile = Master {
        id: ids::FARM,
        kind: MasterKind::MISSILE,
        combat: Some(CombatPart {
            weapons: vec![ClassAmount::new(3, 6)],
            ..CombatPart::default()
        }),
        ..Master::default()
    };
    let volley = Master {
        id: master_id(54),
        kind: MasterKind::MISSILE,
        combat: Some(CombatPart {
            weapons: vec![
                ClassAmount::new(3, 2),
                ClassAmount::new(38, 1),
                ClassAmount::new(4, 9),
            ],
            ..CombatPart::default()
        }),
        ..Master::default()
    };
    let unarmed = Master {
        id: master_id(55),
        kind: MasterKind::MISSILE,
        ..Master::default()
    };
    let archer = |volley_missile| Master {
        id: master_id(4),
        kind: MasterKind::COMBAT,
        creatable: Some(CreatablePart {
            volley_fire: Firepower::new(3.0),
            volley_missile,
            ..CreatablePart::default()
        }),
        combat: Some(CombatPart {
            reload: Seconds::new(2.0),
            weapons: vec![ClassAmount::new(3, 4)],
            missile: MasterField::of(Some(ids::FARM)),
            garrison_firepower: Firepower::new(1.0),
            ..CombatPart::default()
        }),
        ..Master::default()
    };
    let masters = Masters::new([missile, volley, unarmed]);
    // (2 + 1) * 2 extra missiles, plus its own 4, over its reload 2
    assert_eq!(
        firepower(&archer(Some(master_id(54))), &masters, Seconds::ZERO),
        DamageRate::new(5.0)
    );
    // no volley missile: the missile's (6 * 2 + 4) / 2
    assert_eq!(
        firepower(&archer(None), &masters, Seconds::ZERO),
        DamageRate::new(8.0)
    );
    // a volley missile without weapons adds nothing
    assert_eq!(
        firepower(&archer(Some(master_id(55))), &masters, Seconds::ZERO),
        DamageRate::new(2.0)
    );
    // a null slot of the player's table (id 52 below its last master) adds nothing either
    assert_eq!(
        firepower(&archer(Some(master_id(52))), &masters, Seconds::ZERO),
        DamageRate::new(2.0)
    );
    // no missile: the weapons' own
    assert_eq!(
        firepower(&archer(None), &masters, Seconds::ZERO),
        DamageRate::new(8.0)
    );
}

#[test]
fn the_extra_missiles_truncate_and_multiply_as_the_game_does() {
    // FUN_14120e1a0: CVTTSS2SI R8D gives i32::MIN past the i32 range, and IMUL EAX, R8D
    // wraps: amount 1 adds -2^31, amount 2 adds 0
    let volley = |amount| Master {
        id: master_id(54),
        kind: MasterKind::MISSILE,
        combat: Some(CombatPart {
            weapons: vec![ClassAmount::new(3, amount)],
            ..CombatPart::default()
        }),
        ..Master::default()
    };
    let tower = Master {
        id: master_id(79),
        kind: MasterKind::COMBAT,
        creatable: Some(CreatablePart {
            volley_fire: Firepower::new(1e10),
            volley_missile: Some(master_id(54)),
            ..CreatablePart::default()
        }),
        combat: Some(CombatPart {
            reload: Seconds::new(1.0),
            garrison_firepower: Firepower::new(1.0),
            ..CombatPart::default()
        }),
        ..Master::default()
    };
    let masters = Masters::new([volley(1)]);
    assert_eq!(
        firepower(&tower, &masters, Seconds::ZERO),
        DamageRate::new(-2_147_483_648.0)
    );
    let masters = Masters::new([volley(2)]);
    assert_eq!(
        firepower(&tower, &masters, Seconds::ZERO),
        DamageRate::new(0.0)
    );
    // the f32 truncation toward zero: 3.9 - 1 gives 2 extra missiles
    let masters = Masters::new([volley(3)]);
    let three = Master {
        creatable: Some(CreatablePart {
            volley_fire: Firepower::new(3.9),
            ..tower.creatable.clone().unwrap_or_default()
        }),
        ..tower
    };
    assert_eq!(
        firepower(&three, &masters, Seconds::ZERO),
        DamageRate::new(6.0)
    );
}

#[test]
fn share_is_at_least_zero_and_transforming_master_takes_its_other_form_as_its_floor() {
    // FUN_141213520: MAXSS(own / rate, floor), the floor 0.0 unless object flag 3 names (in
    // its upper half, +0x656) a master of type 70: that master's firepower over the rate
    let foot = Master {
        id: master_id(1120),
        kind: MasterKind::COMBAT,
        class: class::CLASS_0,
        object_flags: ObjectFlags::from_bits_retain(8 | (1122 << 16)),
        combat: Some(CombatPart {
            reload: Seconds::new(2.0),
            weapons: vec![ClassAmount::new(3, 2)],
            garrison_firepower: Firepower::new(1.0),
            ..CombatPart::default()
        }),
        ..Master::default()
    };
    let mounted = Master {
        id: master_id(1122),
        kind: MasterKind::COMBAT,
        class: class::CLASS_36,
        combat: Some(CombatPart {
            reload: Seconds::new(2.0),
            weapons: vec![ClassAmount::new(3, 6)],
            garrison_firepower: Firepower::new(1.0),
            ..CombatPart::default()
        }),
        ..Master::default()
    };
    let masters = Masters::new([foot.clone(), mounted.clone()]);
    // its own 1.0 against the other form's 3.0, over a rate of 2
    assert_eq!(
        share(
            &foot,
            &masters,
            Seconds::ZERO,
            DamageRate::new(2.0),
            &roles()
        ),
        Ok(Firepower::new(1.5))
    );
    // the other form's lower share leaves its own
    assert_eq!(
        share(
            &mounted,
            &masters,
            Seconds::ZERO,
            DamageRate::new(2.0),
            &roles()
        ),
        Ok(Firepower::new(1.5))
    );
    let reversed = Master {
        object_flags: ObjectFlags::from_bits_retain(8 | (1120 << 16)),
        ..mounted.clone()
    };
    assert_eq!(
        share(
            &reversed,
            &masters,
            Seconds::ZERO,
            DamageRate::new(2.0),
            &roles()
        ),
        Ok(Firepower::new(1.5))
    );
    // a named master not of type 70, or none, gives the floor 0.0; flag 3 clear names none
    let building = Master {
        kind: MasterKind::BUILDING,
        ..mounted
    };
    let masters_80 = Masters::new([foot.clone(), building]);
    assert_eq!(
        share(
            &foot,
            &masters_80,
            Seconds::ZERO,
            DamageRate::new(2.0),
            &roles()
        ),
        Ok(Firepower::new(0.5))
    );
    let unflagged = Master {
        object_flags: ObjectFlags::from_bits_retain(1122 << 16),
        ..foot.clone()
    };
    assert_eq!(
        share(
            &unflagged,
            &masters,
            Seconds::ZERO,
            DamageRate::new(2.0),
            &roles()
        ),
        Ok(Firepower::new(0.5))
    );
    // a negative quotient and a NaN one give the floor 0.0
    assert_eq!(
        share(
            &unflagged,
            &masters,
            Seconds::ZERO,
            DamageRate::new(-2.0),
            &roles()
        ),
        Ok(Firepower::new(0.0))
    );
    assert_eq!(
        share(
            &unflagged,
            &masters,
            Seconds::ZERO,
            DamageRate::new(f32::NAN),
            &roles()
        ),
        Ok(Firepower::new(0.0))
    );
    // _fdclass: a rate of ±0 gives +0.0; class 42 gives 1.0 whatever the rate
    assert_eq!(
        share(
            &foot,
            &masters,
            Seconds::ZERO,
            DamageRate::new(-0.0),
            &roles()
        )
        .map(|firepower| firepower.get().to_bits()),
        Ok(0)
    );
    let relic_like = Master {
        class: class::RELIC,
        ..foot
    };
    assert_eq!(
        share(
            &relic_like,
            &masters,
            Seconds::ZERO,
            DamageRate::new(f32::NAN),
            &roles()
        ),
        Ok(Firepower::new(1.0))
    );
}

#[test]
fn garrison_task_compares_the_bytes_the_game_passes_widened() {
    // FUN_1415c9240: the task's ushorts +0xe and +0x10 against the byte arguments
    let tasks = [task(GARRISON, 20), task(GARRISON, -1)];
    // class 276 passes its low byte 20; a task of 276 never matches a byte
    assert!(has_garrison_task(
        &tasks[..1],
        GARRISON,
        ClassId::of_master(276)
    ));
    assert!(has_garrison_task(&tasks[..1], GARRISON, class::TRANSPORT));
    assert!(!has_garrison_task(
        &[task(GARRISON, 276)],
        GARRISON,
        ClassId::of_master(276)
    ));
    // a class of -1 passes 0xff, which a task of 0xffff does not match
    assert!(!has_garrison_task(&tasks[1..], GARRISON, class::NO_CLASS));
    assert!(has_garrison_task(
        &[task(GARRISON, 255)],
        GARRISON,
        class::NO_CLASS
    ));
    assert!(!has_garrison_task(
        &[task(ActionKind::COMBAT, 3)],
        GARRISON,
        class::BUILDING
    ));
}

#[test]
fn building_takes_unit_in_by_its_player_state_hit_points_room_and_garrison_kind() {
    // FUN_14120eb10
    let mut garrison_world = garrison_world();
    let takes = |world: &World, building, player_id, master_id, others, unchecked| {
        world
            .takes_in_as(
                ContainerId::new(object_id(building)),
                player_id,
                Some(master_id),
                others,
                unchecked,
            )
            .unwrap()
    };
    assert!(takes(
        &garrison_world,
        100,
        PlayerId::new(1),
        VILLAGER,
        true,
        true
    ));
    assert!(takes(
        &garrison_world,
        100,
        PlayerId::new(1),
        VILLAGER,
        true,
        false
    ));
    // a master the player does not have, a building the world does not hold
    assert!(!takes(
        &garrison_world,
        100,
        PlayerId::new(1),
        master_id(999),
        true,
        true
    ));
    assert!(!takes(
        &garrison_world,
        101,
        PlayerId::new(1),
        VILLAGER,
        true,
        true
    ));
    // an ally's building only with param_4 1; an enemy's never
    assert!(takes(
        &garrison_world,
        100,
        PlayerId::new(2),
        VILLAGER,
        true,
        true
    ));
    assert!(!takes(
        &garrison_world,
        100,
        PlayerId::new(2),
        VILLAGER,
        false,
        true
    ));
    assert!(!takes(
        &garrison_world,
        100,
        PlayerId::new(3),
        VILLAGER,
        true,
        true
    ));
    // no garrison task on buildings; garrison type 11 has no bit for class 0
    assert!(!takes(
        &garrison_world,
        100,
        PlayerId::new(1),
        ARCHER,
        true,
        true
    ));
    // the gate (an annex) asks its main building, but its own master must be of type 80
    assert!(takes(
        &garrison_world,
        300,
        PlayerId::new(1),
        VILLAGER,
        true,
        true
    ));
    // a unit takes nothing in through this path: the archer is no transport
    assert!(!takes(
        &garrison_world,
        200,
        PlayerId::new(1),
        ARCHER,
        true,
        true
    ));
    // hit points at most a fifth of the master's (COMISD, JBE: NaN too)
    for hp in [480.0, f32::NAN] {
        garrison_world.players[1].objects[ListIndex::new(0)].life.hp = HitPoints::new(hp);
        assert!(!takes(
            &garrison_world,
            100,
            PlayerId::new(1),
            VILLAGER,
            true,
            true
        ));
        assert!(!takes(
            &garrison_world,
            300,
            PlayerId::new(1),
            VILLAGER,
            true,
            true
        ));
    }
    garrison_world.players[1].objects[ListIndex::new(0)].life.hp = HitPoints::new(480.5);
    assert!(takes(
        &garrison_world,
        100,
        PlayerId::new(1),
        VILLAGER,
        true,
        true
    ));
    // state 2 only
    garrison_world.players[1].objects[ListIndex::new(0)]
        .life
        .state = ObjectState::Dying;
    assert!(!takes(
        &garrison_world,
        100,
        PlayerId::new(1),
        VILLAGER,
        true,
        true
    ));
    garrison_world.players[1].objects[ListIndex::new(0)]
        .life
        .state = ObjectState::Alive;
    // full: only the room test (param_5 0) refuses
    for index in 0..15 {
        let mut unit = unit(1000 + index, VILLAGER, HitPoints::new(25.0));
        unit.life.inside = Some(ContainerId::new(object_id(100)));
        let _ = garrison_world.push_live(PlayerId::new(1), unit).unwrap();
    }
    assert!(takes(
        &garrison_world,
        100,
        PlayerId::new(1),
        VILLAGER,
        true,
        true
    ));
    assert!(!takes(
        &garrison_world,
        100,
        PlayerId::new(1),
        VILLAGER,
        true,
        false
    ));
}

#[test]
fn transport_takes_combat_unit_with_task_on_its_class_and_room() {
    // FUN_14120eb10's object-flag-0 branch: FUN_1415c9240 with 3 and the transport's class
    let mut garrison_world = garrison_world();
    let takes = |world: &World, master_id, unchecked| {
        world.takes_in_as(
            ContainerId::new(object_id(200)),
            PlayerId::new(1),
            Some(master_id),
            true,
            unchecked,
        )
    };
    assert_eq!(takes(&garrison_world, VILLAGER, false), Ok(true));
    assert_eq!(takes(&garrison_world, ARCHER, true), Ok(false));
    let mut unit = unit(1000, VILLAGER, HitPoints::new(25.0));
    unit.life.inside = Some(ContainerId::new(object_id(200)));
    let _ = garrison_world.push_live(PlayerId::new(1), unit).unwrap();
    assert_eq!(takes(&garrison_world, VILLAGER, false), Ok(false));
    assert_eq!(takes(&garrison_world, VILLAGER, true), Ok(true));
    // a unit of another type with no class flags 0x10002 (FUN_1411205e0) goes on as for a
    // building: the transport is of type 70, so it takes none
    assert_eq!(takes(&garrison_world, TC, true), Ok(false));
    // a unit of a type below 70 whose class has them (13: 0x3) takes the transport's branch
    let mut masters = garrison_masters();
    masters = Masters::new(masters.iter().cloned().chain([Master {
        id: master_id(1500),
        kind: MasterKind::MISSILE,
        class: class::SIEGE,
        action: Some(ActionPart {
            tasks: vec![task(GARRISON, 20)],
            ..ActionPart::default()
        }),
        ..Master::default()
    }]));
    garrison_world.model.masters = (0..4).map(|_| masters.clone()).map(Arc::new).collect();
    assert_eq!(takes(&garrison_world, master_id(1500), true), Ok(true));
    // full: no room for one more
    assert_eq!(takes(&garrison_world, master_id(1500), false), Ok(false));
}

#[test]
fn the_class_flags_are_the_images_table_by_class() {
    // FUN_1411205e0 with DAT_1439e37b0: villagers (4, 0x10163), infantry (6, 0x10103) and
    // siege (13, 0x3) garrison a transport; ships (20, 21, 22) do not
    for class in [0, 4, 6, 13, 18, 44, 58] {
        assert!(
            class_has_flags(ClassId::of_master(class), ClassFlags::TRANSPORTED),
            "class {class}"
        );
    }
    for class in [2, 20, 21, 22, 52, 60, 63] {
        assert!(
            !class_has_flags(ClassId::of_master(class), ClassFlags::TRANSPORTED),
            "class {class}"
        );
    }
    // other flags of the same words; a class outside the table has none (JS, JGE)
    assert!(class_has_flags(
        class::CLASS_17,
        ClassFlags::from_bits_retain(0x10)
    ));
    assert!(class_has_flags(
        class::FISHING_SHIP,
        ClassFlags::from_bits_retain(0x100)
    ));
    assert!(!class_has_flags(class::NO_CLASS, ClassFlags::all()));
    assert!(!class_has_flags(class::CLASS_64, ClassFlags::all()));
    assert!(class_has_flags(class::CLASS_61, ClassFlags::all()));
}

#[test]
fn room_test_counts_the_unit_and_what_it_holds_or_one_for_none() {
    // FUN_1412169b0: slot 0x498 of the building plus that of the unit and 1
    let mut garrison_world = garrison_world();
    let _ = garrison_world
        .push_live(PlayerId::new(1), unit(1000, VILLAGER, HitPoints::new(25.0)))
        .unwrap();
    let room = |world: &World, unit| {
        let (bp, building) = world.object(object_id(200)).unwrap();
        world.has_room(bp, building, unit).unwrap()
    };
    assert!(room(&garrison_world, None));
    assert!(room(&garrison_world, Some(object_id(1000))));
    // a unit holding one more needs room for two
    let mut held = unit(1001, ARCHER, HitPoints::new(25.0));
    held.life.inside = Some(ContainerId::new(object_id(1000)));
    let _ = garrison_world.push_live(PlayerId::new(1), held).unwrap();
    assert!(!room(&garrison_world, Some(object_id(1000))));
    assert!(room(&garrison_world, None));
}

#[test]
fn unload_picks_the_units_its_flag_names_of_any_player() {
    // FUN_14118a610 on the units inside town centre 100: player 1's villager 1000, archer
    // 1001 and the class-42 1002, and player 2's (an ally's) villager 2000
    let mut garrison_world = garrison_world();
    let mut relic = garrison_masters();
    relic = Masters::new(relic.iter().cloned().chain([Master {
        id: ids::RELIC,
        kind: MasterKind::COMBAT,
        class: class::RELIC,
        ..Master::default()
    }]));
    garrison_world.model.masters = (0..4).map(|_| relic.clone()).map(Arc::new).collect();
    for (player, id, master_id) in [
        (1, 1000, VILLAGER),
        (1, 1001, ARCHER),
        (1, 1002, ids::RELIC),
        (2, 2000, VILLAGER),
        (3, 3000, ARCHER),
    ] {
        let mut unit = unit(id, master_id, HitPoints::new(25.0));
        unit.life.inside = (player != 3).then_some(ContainerId::new(object_id(100)));
        let _ = garrison_world
            .push_live(PlayerId::new(player), unit)
            .unwrap();
    }
    let picks = |world: &World, flag, param| {
        [1000, 1001, 1002, 2000]
            .into_iter()
            .filter(|&id| {
                let (player_id, unit) = world.object(object_id(id)).unwrap();
                world
                    .unload_picks(player_id, unit, UnloadPick::of_packet(flag, param))
                    .unwrap()
            })
            .collect::<Vec<i32>>()
    };
    assert_eq!(picks(&garrison_world, 0, -1), [1000, 1001, 2000]);
    assert_eq!(picks(&garrison_world, 1, 2), [2000]);
    assert_eq!(picks(&garrison_world, 2, 1), [1000, 2000]);
    assert_eq!(picks(&garrison_world, 2, 6), [1002]);
    assert_eq!(picks(&garrison_world, 3, 1001), [1001]);
    // flag 4: the named object's master (a villager names every villager of its player)
    assert_eq!(picks(&garrison_world, 4, 1000), [1000]);
    assert_eq!(picks(&garrison_world, 4, 2000), [2000]);
    assert_eq!(picks(&garrison_world, 4, 1001), [1001]);
    assert_eq!(picks(&garrison_world, 4, 9999), [0i32; 0]);
    // flag 5: every unit but it and class 42, of its player or with no named object
    assert_eq!(picks(&garrison_world, 5, 1001), [1000]);
    assert_eq!(picks(&garrison_world, 5, 9999), [1000, 1001, 2000]);
    // flag 6: not of its master, never class 42, a villager not for a villager
    assert_eq!(picks(&garrison_world, 6, 1001), [1000]);
    assert_eq!(picks(&garrison_world, 6, 1000), [1001]);
    assert_eq!(picks(&garrison_world, 6, 3000), [0i32; 0]);
    assert_eq!(picks(&garrison_world, 7, 1000), [0i32; 0]);
}

/// A unit's movement as the spot test reads it: terrain restriction 0, no speed.
pub(crate) fn standing() -> MoveStats {
    MoveStats {
        fly_mode: FlyMode::GROUND,
        speed: Speed::ZERO,
        base: Speed::ZERO,
        turn_rate: TurnRate::ZERO,
        turn_radius: Distance::ZERO,
        turn_radius_speed: Speed::MAX,
        radius: Radii::new(0.2, 0.2, 2.0),
        restriction: RestrictionId::named(0),
        restriction_mask: TerrainMask::ALL,
        turn_time: Seconds::ZERO,
        move_algorithm: MoveAlgorithm::NONE,
        min_collision: 0.0,
        fast_turn: false,
        facets: None,
        weapon_range: Distance::ZERO,
    }
}

/// [`garrison_world`] on a flat 20×20 map every player sees, the town centre at (10, 10)
/// with its footprint on the map, the villager master able to stand beside it.
pub(crate) fn let_out_world() -> World {
    sized_world(20, 20)
}

/// [`let_out_world`] on a map of `width` × `height` tiles.
pub(crate) fn sized_world(width: i32, height: i32) -> World {
    let mut world = garrison_world();
    let mut masters = garrison_masters();
    if let Some(tc) = masters.get_mut(TC) {
        tc.radius = Footprint::new(2.0, 2.0);
        tc.height = Distance::new(1.0);
        tc.creatable
            .get_or_insert_with(Default::default)
            .volley_fire = Firepower::new(1.0);
        // no stack unit, head or annexes: the tracker files it under its own id
        if let Some(building_master) = tc.building.as_mut() {
            building_master.stack_unit = None;
            building_master.head_unit = None;
            building_master.annexes = [None; 4];
        }
    }
    if let Some(master) = masters.get_mut(VILLAGER) {
        master.radius = Footprint::new(0.2, 0.2);
        master.height = Distance::new(1.0);
        master.move_stats = Some(standing());
    }
    world.model.masters = (0..4).map(|_| masters.clone()).map(Arc::new).collect();
    let tiles = (width * height) as usize;
    world.spatial = Spatial::new(GridSize::new(width, height));
    let mut terrain = Terrain::new(
        GridSize::new(width, height),
        vec![TileTerrain::of_save(0, 255, -1); tiles],
        vec![
            TerrainKind {
                shown: true,
                flags: TerrainFlags::NONE,
                forest: false,
            };
            1
        ],
        vec![Restriction::new(vec![1.0])],
    );
    terrain.set_elevations(vec![0; tiles]);
    world.spatial.terrain = Some(terrain);
    let seen = (0..4).fold(0u64, |bits, player| {
        bits | (1 << player) | (1 << (32 + player))
    });
    world.spatial.sight = Some(Visibility::new(
        GridSize::new(width, height),
        (0..4)
            .map(|_| VisibleMap {
                counts: vec![0; tiles],
                explored: TileCount::ZERO,
            })
            .collect(),
        vec![VisionBits::from_bits_retain(seen); tiles],
        Vec::new(),
        0,
        vec![PlayerKind::Human; 4],
    ));
    let tc = &mut world.players[1].objects[ListIndex::new(0)];
    tc.place_for_test(Position::new(10.0, 10.0, 0.0));
    let id = tc.id;
    world.parts.combat_mut(id).unwrap().firepower = Firepower::new(1.0);
    world.spatial.insert(
        object_id(100),
        Placed {
            position: Position::new(10.0, 10.0, 0.0),
            radius: Footprint::new(2.0, 2.0),
            class: class::TOWER,
            player: PlayerId::new(1),
            master: TC,
            obstruction_kind: ObstructionKind::KIND_2,
            buildable_on: BuildableOn::NO,
            tall: true,
            moves: false,
            unknown: None,
            obstruction: Some(Obstruction {
                kind: ObstructionKind::KIND_2,
                class: ObstructionClass::UNIT,
            }),
        },
    );
    world
}

/// `FUN_1411921d0`, the enter itself: a member of a unit group (`+0x6d0`) leaves the group
/// the world holds (removeUnit with the building and 0, `FUN_14102c160`) and stands (slot
/// 0x560) between leaving the mesh and joining the building.
/// A new unit whose building's gather point is the building itself (`FUN_140ace9c0`): with
/// fewer than 10 inside, or fewer than its capacity, its slot 0x1a0 forced (1:
/// `FUN_1411921d0` passes over the room test, slot 0x198) takes it in at once: no notify,
/// no state change, no move; the building's sight takes its circle of 5 and the villager's
/// town-bell record its work.
#[test]
fn gather_point_on_its_own_building_takes_the_new_unit_in() {
    let mut world = let_out_world();
    walker(
        &mut world,
        PlayerId::new(1),
        1000,
        VILLAGER,
        Point::new(12.5, 10.0),
    );
    let idx = world.players[1].objects.last_place().unwrap();
    let state = world.players[1].objects[idx].life.state;
    let gather_point = GatherPoint {
        state: GatherPointState::Point,
        point: Position::new(10.0, 10.0, 0.0),
        target: Some(object_id(100)),
        target_master: Some(TC),
    };
    world
        .gather(
            PlayerId::new(1),
            idx,
            gather_point,
            object_id(100),
            OrderSource::Direct,
        )
        .unwrap();
    let unit = &world.players[1].objects[idx];
    assert_eq!(
        (
            unit.life.inside,
            unit.life.state,
            world.parts.actions_of(unit.id).list.move_to().is_some(),
            world.parts.actions_of(unit.id).list.enter().is_some()
        ),
        (Some(container_id(100)), state, false, false)
    );
    assert_eq!(
        world.parts.town_bell_of(unit.id).work.target,
        Some(object_id(100))
    );
    assert_eq!(
        world.object(object_id(100)).unwrap().1.garrison,
        [object_id(1000)]
    );
}

#[test]
fn grouped_unit_that_enters_building_leaves_its_group() {
    let mut world = let_out_world();
    let mut unit = unit(400, VILLAGER, HitPoints::new(25.0));
    unit.place_for_test(Position::new(12.5, 10.0, 0.0));
    let mut walker = tests_walker();
    walker.stats = standing();
    world.parts.put_mover(unit.id, Some(walker));
    unit.group.id = Some(group_id(7));
    let _ = world.push_live(PlayerId::new(1), unit).unwrap();
    world
        .parts
        .set_town_bell_flag(object_id(400), BellShelter::Sheltered);
    world.spatial.insert(
        object_id(400),
        Placed {
            position: Position::new(12.5, 10.0, 0.0),
            radius: Footprint::new(0.2, 0.2),
            class: class::VILLAGER,
            player: PlayerId::new(1),
            master: VILLAGER,
            obstruction_kind: ObstructionKind::NONE,
            buildable_on: BuildableOn::NO,
            tall: true,
            moves: false,
            unknown: None,
            obstruction: None,
        },
    );
    let member = |id| Member {
        id,
        ..Member::default()
    };
    world.groups.groups.push(UnitGroup {
        id: group_id(7),
        owner: Some(PlayerId::new(1)),
        members: vec![member(object_id(401)), member(object_id(400))],
        ..UnitGroup::default()
    });
    let idx = world.players[1].objects.last_place().unwrap();
    let garrison = Task {
        work_range: Distance::new(1.0),
        ..task(GARRISON, 3)
    };
    world
        .start_enter(
            PlayerId::new(1),
            idx,
            object_id(100),
            &garrison,
            Position::new(10.0, 10.0, 0.0),
            Notifies::Kept,
        )
        .unwrap();
    let players = &world.players[1].objects[idx];
    assert_eq!(
        (players.life.inside, players.group.id),
        (Some(container_id(100)), None)
    );
    assert_eq!(
        world.parts.mover_of(players.id).unwrap().animated.speed,
        Speed::new(0.0)
    );
    let members: Vec<ObjectId> = world.groups.groups[0]
        .members
        .iter()
        .map(|member| member.id)
        .collect();
    assert_eq!(members, [object_id(401)]);
}

/// Villager 400 (with a unit AI) beside town centre 100, in reach of its garrison task (range
/// 1), on [`let_out_world`]; `setup` changes the world, then the villager's Enter starts
/// ([`World::start_enter`]). The result and the world after it.
fn enters_the_centre(setup: impl FnOnce(&mut World)) -> (GenieResult<()>, World) {
    let mut world = let_out_world();
    let mut unit = unit(400, VILLAGER, HitPoints::new(25.0));
    unit.place_for_test(Position::new(12.5, 10.0, 0.0));
    let mut walker = tests_walker();
    walker.stats = standing();
    world.parts.put_mover(unit.id, Some(walker));
    let villager_ai = UnitAi {
        ai_kind: UnitAiKind::Civilian,
        current_order: None,
        current_action: None,
        current_target: None,
        ..UnitAi::default()
    };
    let _ = world
        .push_live(PlayerId::new(1), Object::with_ai(unit, villager_ai))
        .unwrap();
    world.players[1].attributes = vec![Amount::ZERO; 250];
    let placed = Placed {
        position: Position::new(12.5, 10.0, 0.0),
        radius: Footprint::new(0.2, 0.2),
        class: class::VILLAGER,
        player: PlayerId::new(1),
        master: VILLAGER,
        obstruction_kind: ObstructionKind::NONE,
        buildable_on: BuildableOn::NO,
        tall: true,
        moves: false,
        unknown: None,
        obstruction: None,
    };
    world.spatial.insert(object_id(400), placed);
    setup(&mut world);
    let idx = world.players[1].objects.last_place().unwrap();
    let garrison = Task {
        work_range: Distance::new(1.0),
        ..task(GARRISON, 3)
    };
    let start_enter = world.start_enter(
        PlayerId::new(1),
        idx,
        object_id(100),
        &garrison,
        Position::new(10.0, 10.0, 0.0),
        Notifies::Kept,
    );
    (start_enter, world)
}

/// The enter into a building (`FUN_140a867b0`): a villager carrying food into a town centre
/// that takes it as a drop site gives the load first (food and the food gathered, attribute
/// 166) and carries nothing after; without the drop-site entry it keeps the load. A centre at
/// a fifth of its hit points or less (480 of 2400) and a master that transforms into the
/// centre (task 0x88) are not modelled, nor is the Enter's state 11 for a full centre: full
/// by the start's room test (slot 0x198 with the unit, `FUN_1412169b0`), which counts what the
/// villager holds too.
#[test]
fn an_enter_gives_its_load_to_drop_site_and_needs_room() {
    let with_food = |world: &mut World| {
        let players = world.players[1].objects.last_mut().unwrap();
        (players.life.attribute_kind, players.life.amount) =
            (Some(AttributeId::named(17)), Amount::new(6.0));
    };
    let drop_site = |world: &mut World| {
        world.model.drop_sites = Arc::new(vec![DropSite {
            building: TC,
            worker_class: Some(class::VILLAGER),
            targets: vec![DropTarget {
                class: class::CLASS_7,
                workers: WorkerCount::of_data(-1),
                state: None,
                attribute: Some(AttributeId::named(17)),
            }],
            ..DropSite::default()
        }]);
    };
    let (result, game) = enters_the_centre(|world| {
        with_food(world);
        drop_site(world);
    });
    assert_eq!(result, Ok(()));
    let object = game.object(object_id(400)).unwrap().1;
    assert_eq!(object.life.inside, Some(container_id(100)));
    assert_eq!(object.life.held(), carrying(None, 0.0));
    let player = &game.players[1];
    assert_eq!(
        (player.attributes[0], player.attributes[166]),
        (Amount::new(6.0), Amount::new(6.0))
    );
    let (result, with_food_world) = enters_the_centre(with_food);
    assert_eq!(result, Ok(()));
    let unit = with_food_world.object(object_id(400)).unwrap().1;
    assert_eq!(
        (unit.life.inside, unit.life.amount),
        (Some(container_id(100)), Amount::new(6.0))
    );
    let reason = |setup: fn(&mut World)| enters_the_centre(setup).0.unwrap_err().reason();
    let weak: fn(&mut World) =
        |world| world.object_mut(object_id(100)).unwrap().1.life.hp = HitPoints::new(480.0);
    assert_eq!(
        reason(weak),
        "an enter into a building at a fifth of its hit points (state 13)"
    );
    let transforms: fn(&mut World) = |world| {
        for masters in &mut world.model.masters {
            let master = Arc::make_mut(masters).get_mut(VILLAGER).unwrap();
            master
                .action
                .get_or_insert_with(Default::default)
                .tasks
                .push(Task {
                    object_id: Some(TC),
                    ..task(ActionKind::DEPOSIT, -1)
                });
        }
    };
    assert_eq!(
        reason(transforms),
        "an enter of a master that transforms (task 0x88)"
    );
    let full: fn(&mut World) = |world| give_room(world, TC, 0);
    assert_eq!(
        reason(full),
        "an Enter into a building without room (state 11)"
    );
    // the start's slot 0x198 with the unit counts what the unit holds: a centre of room 2 with
    // an ally's villager inside takes villager 400 (1 + 1), and not with a unit inside it
    let crowded: fn(&mut World) = |world| {
        give_room(world, TC, 2);
        inside_tc(world, PlayerId::new(2), 2000, VILLAGER);
    };
    assert_eq!(enters_the_centre(crowded).0, Ok(()));
    let holding: fn(&mut World) = |world| {
        give_room(world, TC, 2);
        inside_tc(world, PlayerId::new(2), 2000, VILLAGER);
        held_by(world, PlayerId::new(2), 2001, 400);
    };
    assert_eq!(
        reason(holding),
        "an Enter into a building without room (state 11)"
    );
}

/// Puts unit `id` of player `player_id` and master `master_id` inside the town centre.
pub(crate) fn inside_tc(world: &mut World, player_id: PlayerId, id: i32, master_id: MasterId) {
    let mut unit = unit(id, master_id, HitPoints::new(25.0));
    unit.place_for_test(Position::new(10.0, 10.0, 0.0));
    unit.life.inside = Some(ContainerId::new(object_id(100)));
    let _ = world.push_live(player_id, unit).unwrap();
    world.spatial.insert(
        object_id(id),
        Placed {
            position: Position::new(10.0, 10.0, 0.0),
            radius: Footprint::new(0.2, 0.2),
            class: class::VILLAGER,
            player: player_id,
            master: master_id,
            obstruction_kind: ObstructionKind::NONE,
            buildable_on: BuildableOn::NO,
            tall: true,
            moves: false,
            unknown: None,
            obstruction: None,
        },
    );
    world
        .spatial
        .go_inside(object_id(id), Position::new(10.0, 10.0, 0.0));
    world.players[1].objects[ListIndex::new(0)]
        .garrison
        .push(object_id(id));
}

#[test]
fn back_to_work_lets_an_allys_villager_out_toward_its_record() {
    // FUN_141625590 -> FUN_1412081e0 -> FUN_1411e7220 on player 2's villager 2000 inside
    // player 1's town centre: its own player's master and orders, the town bell cleared
    let mut world = let_out_world();
    inside_tc(&mut world, PlayerId::new(2), 2000, VILLAGER);
    let players = &mut world.players[2].objects[ListIndex::new(0)];
    world
        .parts
        .set_town_bell_flag(players.id, BellShelter::Sheltered);
    world.parts.town_bell_of(players.id).work = BackToWork {
        point: Point::new(4.5, 4.5),
        ..BackToWork::NONE
    };
    // the building's last unit inside: the volleys go back to the master's volley fire
    world
        .parts
        .combat_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .firepower = Firepower::new(2.0);
    world
        .go_back_to_work(player_id(1), &object_ids([100]))
        .unwrap();
    let unit = &world.players[2].objects[ListIndex::new(0)];
    assert_eq!(unit.life.inside, None);
    assert!(!world.parts.town_bell_of(unit.id).flag.is_sheltered());
    assert_eq!(
        world.players[1].objects[ListIndex::new(0)].garrison.len(),
        0
    );
    assert_eq!(
        world
            .parts
            .combat_of(world.players[1].objects[ListIndex::new(0)].id)
            .firepower,
        Firepower::new(1.0)
    );
    assert!(world.spatial.get_known(object_id(2000)).unwrap().is_some());
    // FUN_141217910's reset: COMISS volley, firepower; JBE keeps it while units stay inside
    // and the firepower is not below the volley fire (a NaN one too)
    let mut world = let_out_world();
    inside_tc(&mut world, PlayerId::new(1), 1000, VILLAGER);
    inside_tc(&mut world, PlayerId::new(2), 2000, VILLAGER);
    let mut bell = world.parts.town_bell_of(object_id(2000));
    bell.work.point = Point::new(4.5, 4.5);
    world.parts.set_town_bell(object_id(2000), bell);
    world
        .parts
        .combat_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .firepower = Firepower::new(f32::NAN);
    assert_eq!(
        world
            .back_to_work(Some((PlayerId::new(1), ListIndex::new(0))), object_id(2000))
            .unwrap(),
        WorkReturn::Went
    );
    assert!(
        world
            .parts
            .combat_of(world.players[1].objects[ListIndex::new(0)].id)
            .firepower
            .get()
            .is_nan()
    );
    assert_eq!(
        world.players[1].objects[ListIndex::new(0)].garrison,
        [object_id(1000)]
    );
    world
        .parts
        .combat_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .firepower = Firepower::new(0.5);
    let mut bell = world.parts.town_bell_of(object_id(1000));
    bell.work.point = Point::new(4.5, 15.5);
    world.parts.set_town_bell(object_id(1000), bell);
    assert_eq!(
        world
            .back_to_work(Some((PlayerId::new(1), ListIndex::new(0))), object_id(1000))
            .unwrap(),
        WorkReturn::Went
    );
    assert_eq!(
        world
            .parts
            .combat_of(world.players[1].objects[ListIndex::new(0)].id)
            .firepower,
        Firepower::new(1.0)
    );
    // a unit not a villager stays inside
    let mut world = let_out_world();
    inside_tc(&mut world, PlayerId::new(2), 2001, ARCHER);
    world
        .go_back_to_work(player_id(1), &object_ids([100]))
        .unwrap();
    assert_eq!(
        world.players[2].objects[ListIndex::new(0)].life.inside,
        Some(container_id(100))
    );
}

/// `FUN_1412081e0` with no record object the world holds but a recorded action and target
/// class: the unit AI takes the action and the unit asks it to search (notify 0x202 with the
/// action, the class and the old target); without the action or the class it asks nothing.
/// Either way the unit leaves toward the record's point.
#[test]
fn back_to_work_without_its_object_asks_the_unit_ai_to_search() {
    use crate::orders::town_bell::BackToWork;
    for (action, class, searches) in [
        (Some(AiAction::Gather), 5, true),
        (None, 5, false),
        (Some(AiAction::Gather), -1, false),
    ] {
        let mut world = let_out_world();
        inside_tc(&mut world, PlayerId::new(1), 1000, VILLAGER);
        world
            .parts
            .set_unit_ai(object_id(1000), Some(UnitAi::default()));
        let bell = TownBell {
            work: BackToWork {
                target: Some(object_id(7)),
                target_class: class_of(class),
                action,
                point: Point::new(4.5, 4.5),
                ..BackToWork::NONE
            },
            ..world.parts.town_bell_of(object_id(1000))
        };
        world.parts.set_town_bell(object_id(1000), bell);
        assert_eq!(
            world
                .back_to_work(Some((PlayerId::new(1), ListIndex::new(0))), object_id(1000))
                .unwrap(),
            WorkReturn::Went
        );
        let ai = world.parts.unit_ai.get(object_id(1000)).unwrap();
        let heard: Vec<_> = ai
            .notifies
            .iter()
            .map(|notify| (notify.kind, notify.params))
            .collect();
        if searches {
            assert_eq!(ai.current_action, Some(AiAction::Gather));
            assert_eq!(
                heard,
                [(
                    NotifyKind::REQUIRES_SEARCH,
                    [AiAction::Gather.value(), 5, 7]
                )]
            );
        } else {
            assert_eq!(
                (ai.current_action, heard),
                (None, Vec::new()),
                "{action:?} {class}"
            );
        }
    }
}

#[test]
fn unit_ais_test_takes_building_or_transport_that_takes_its_unit_in() {
    // FUN_140c40e00: type below 70 none, else FUN_14120eb10 with 1, 1
    let mut garrison_world = garrison_world();
    let mut masters = garrison_masters();
    masters = Masters::new(masters.iter().cloned().chain([Master {
        id: master_id(59),
        kind: MasterKind::MISSILE,
        class: class::BUILDING,
        garrison: GarrisonCapacity::of_data(5),
        ..Master::default()
    }]));
    garrison_world.model.masters = (0..4).map(|_| masters.clone()).map(Arc::new).collect();
    let _ = garrison_world
        .push_live(
            PlayerId::new(1),
            unit(400, master_id(59), HitPoints::new(10.0)),
        )
        .unwrap();
    let takes = |world: &World, building, player_id, master_id| {
        let (bp, bu) = world.object(object_id(building)).unwrap();
        world.ai_takes(bp, bu, player_id, master_id)
    };
    assert_eq!(
        takes(&garrison_world, 100, PlayerId::new(1), VILLAGER),
        Ok(true)
    );
    assert_eq!(
        takes(&garrison_world, 100, PlayerId::new(2), VILLAGER),
        Ok(true)
    );
    assert_eq!(
        takes(&garrison_world, 100, PlayerId::new(1), ARCHER),
        Ok(false)
    );
    // the transport (type 70) by its own branch
    assert_eq!(
        takes(&garrison_world, 200, PlayerId::new(1), VILLAGER),
        Ok(true)
    );
    // an object below type 70
    assert_eq!(
        takes(&garrison_world, 400, PlayerId::new(1), VILLAGER),
        Ok(false)
    );
}

/// A villager of player `player_id` with a unit AI and speed at (`x`, `y`).
pub(crate) fn walker(
    world: &mut World,
    player_id: PlayerId,
    id: i32,
    master_id: MasterId,
    at: Point,
) {
    let mut unit = unit(id, master_id, HitPoints::new(25.0));
    unit.place_for_test(at.with_height(0.0));
    let villager_ai = UnitAi {
        ai_kind: UnitAiKind::Civilian,
        current_order: None,
        current_action: None,
        current_target: None,
        ..UnitAi::default()
    };
    world.parts.put_mover(
        unit.id,
        Some(Mover {
            forward: BVector::new(1.0, 0.0, 0.0),
            right: BVector::new(0.0, 0.0, -1.0),
            velocity: None,
            paths: vec![],
            factor: 1.0,
            stats: MoveStats {
                speed: Speed::new(0.8),
                base: Speed::new(0.8),
                ..standing()
            },
            waiting: 0,
            wait_delays: WaitDelays::of_data(0),
            attack_moving: false,
            last_move: WorldTime::ZERO,
            moved_at: Position::ORIGIN,
            substitute_count: SubstituteCount::of_data(0),
            speedup: Speedup::NONE,
            ignore_units: Vec::new(),
            user_waypoints: Vec::new(),
            last_turn: Seconds::ZERO,
            angle: Angle::ZERO,
            animated: Animated::default(),
            pauses: Vec::new(),
            in_the_way: Vec::new(),
            sprite_facets: None,
        }),
    );
    let _ = world
        .push_live(player_id, Object::with_ai(unit, villager_ai))
        .unwrap();
    world.spatial.insert(
        object_id(id),
        Placed {
            position: at.with_height(0.0),
            radius: Footprint::new(0.2, 0.2),
            class: class::VILLAGER,
            player: player_id,
            master: master_id,
            obstruction_kind: ObstructionKind::NONE,
            buildable_on: BuildableOn::NO,
            tall: true,
            moves: true,
            unknown: None,
            obstruction: None,
        },
    );
}

/// A building of player 1 of master `master_id` (radius 2) at `at`, `annex_of` its main building.
pub(crate) fn building_at(
    world: &mut World,
    id: i32,
    master_id: MasterId,
    at: Point,
    annex_of: Option<ObjectId>,
) {
    let mut unit = unit(id, master_id, HitPoints::new(2400.0));
    unit.place_for_test(at.with_height(0.0));
    let _ = world.push_live(PlayerId::new(1), unit).unwrap();
    world.parts.building_mut(object_id(id)).unwrap().annex_of = annex_of.map(MainBuildingId::new);
    world.spatial.insert(
        object_id(id),
        Placed {
            position: at.with_height(0.0),
            radius: Footprint::new(2.0, 2.0),
            class: class::TOWER,
            player: PlayerId::new(1),
            master: master_id,
            obstruction_kind: ObstructionKind::KIND_2,
            buildable_on: BuildableOn::NO,
            tall: true,
            moves: false,
            unknown: None,
            obstruction: Some(Obstruction {
                kind: ObstructionKind::KIND_2,
                class: ObstructionClass::UNIT,
            }),
        },
    );
}

/// Player 1's tracker files `ids` under master `master_id`'s key, done, in that order.
pub(crate) fn tracked(world: &mut World, master_id: MasterId, ids: &[i32]) {
    let key = world.tracker_key(PlayerId::new(1), master_id);
    for &id in ids {
        world.players[1].tracker.add(object_id(id), key);
        world.players[1].tracker.complete(object_id(id), key);
    }
}

#[test]
fn the_shelters_near_target_follow_its_owners_tracker_and_count_annexes_and_gates() {
    // FUN_14161e370 with +0x1e: the town centres' list (109) in the tracker's order; 102 is
    // beyond ten tiles, annex 104 is no building but pushes its count (one unit inside)
    let mut world = sized_world(40, 40);
    building_at(&mut world, 101, TC, Point::new(15.0, 10.0), None);
    building_at(&mut world, 102, TC, Point::new(30.0, 30.0), None);
    building_at(
        &mut world,
        104,
        TC,
        Point::new(10.0, 15.0),
        Some(object_id(101)),
    );
    let mut inside = unit(500, VILLAGER, HitPoints::new(25.0));
    inside.life.inside = Some(ContainerId::new(object_id(104)));
    let _ = world.push_live(PlayerId::new(1), inside).unwrap();
    tracked(&mut world, TC, &[104, 102, 101, 100]);
    assert_eq!(
        world
            .shelters_near(player_id(1), object_id(100), TC)
            .unwrap(),
        Shelters {
            buildings: container_ids([101, 100]),
            transports: Vec::new(),
            counts: vec![InsideCount::ONE, InsideCount::ZERO, InsideCount::ZERO],
        }
    );
    // the gates' list: no gate is a building, each pushes its count
    building_at(&mut world, 301, GATE, Point::new(10.0, 6.0), None);
    world
        .spatial
        .insert(object_id(300), *world.spatial.get(object_id(301)).unwrap());
    tracked(&mut world, GATE, &[300, 301]);
    assert_eq!(
        world
            .shelters_near(player_id(1), object_id(301), GATE)
            .unwrap(),
        Shelters {
            buildings: Vec::new(),
            transports: Vec::new(),
            counts: vec![InsideCount::ZERO, InsideCount::ZERO],
        }
    );
}

#[test]
fn garrison_near_its_target_reads_the_count_list_at_its_pairs_index() {
    // the town centres take one unit; the list [104, 101, 100] gives the buildings
    // [101, 100] and the counts [1, 0, 0]: the villager's nearest pair (101, index 0) reads
    // the annex's count 1, which is full, so it goes to 100 (index 1, count 0)
    let world = |with_annex: bool| {
        let mut sized_world = sized_world(40, 40);
        for masters in &mut sized_world.model.masters {
            if let Some(tc) = Arc::make_mut(masters).get_mut(TC) {
                tc.garrison = GarrisonCapacity::of_data(1);
            }
        }
        building_at(&mut sized_world, 101, TC, Point::new(16.0, 10.0), None);
        building_at(
            &mut sized_world,
            104,
            TC,
            Point::new(10.0, 15.0),
            Some(object_id(101)),
        );
        let mut inside = unit(500, VILLAGER, HitPoints::new(25.0));
        inside.life.inside = Some(ContainerId::new(object_id(104)));
        let _ = sized_world.push_live(PlayerId::new(1), inside).unwrap();
        let listed: &[i32] = if with_annex {
            &[104, 101, 100]
        } else {
            &[101, 100]
        };
        tracked(&mut sized_world, TC, listed);
        walker(
            &mut sized_world,
            PlayerId::new(1),
            1000,
            VILLAGER,
            Point::new(19.5, 10.5),
        );
        sized_world
            .garrison_order(
                player_id(1),
                &object_ids([1000]),
                Some(object_id(100)),
                Dispatch::Now,
                GarrisonScope::Nearby,
            )
            .unwrap();
        sized_world
            .parts
            .actions_of(object_id(1000))
            .list
            .enter()
            .map(|enter_action| enter_action.target)
    };
    assert_eq!(world(true), Some(Some(container_id(100))));
    assert_eq!(world(false), Some(Some(container_id(101))));
}

#[test]
fn garrison_through_the_unit_ais_queues_their_enter_orders() {
    // FUN_141621220 with param_7: slot 0x30 into slot 0x28 with 0x2cd, issued by the unit
    let mut world = let_out_world();
    walker(
        &mut world,
        PlayerId::new(1),
        1000,
        VILLAGER,
        Point::new(5.5, 5.5),
    );
    world
        .garrison_order(
            player_id(1),
            &object_ids([1000]),
            Some(object_id(100)),
            Dispatch::Queued,
            GarrisonScope::Target,
        )
        .unwrap();
    let unit = world.object(object_id(1000)).unwrap().1;
    assert_eq!(
        world
            .parts
            .actions_of(object_id(1000))
            .list
            .enter()
            .copied(),
        None
    );
    assert_eq!(
        world.parts.unit_ai.get(unit.id).unwrap().orders,
        [QueuedOrder {
            issuer: Some(Issuer::Object(object_id(1000))),
            order: OrderKind::Enter,
            priority: OrderPriority::of_data(100),
            target: Some(object_id(100)),
            target_owner: owner(1),
            point: Position::new(10.0, 10.0, 0.0),
            range: Distance::ZERO,
            shown: Point::new(10.0, 10.0),
            ready: true,
            target_kind: Some(OrderTargetKind::of_master(TC)),
        }]
    );
}

/// [`let_out_world`] with ship 200 at (`x`, `y`) on the map.
fn ship_at(world: &mut World, at: Point) {
    let ship = world.object_mut(object_id(200)).expect("the ship").1;
    ship.place_for_test(at.with_height(0.0));
    world.spatial.insert(
        object_id(200),
        Placed {
            position: at.with_height(0.0),
            radius: Footprint::new(0.5, 0.5),
            class: class::TRANSPORT,
            player: PlayerId::new(1),
            master: SHIP,
            obstruction_kind: ObstructionKind::NONE,
            buildable_on: BuildableOn::NO,
            tall: true,
            moves: true,
            unknown: None,
            obstruction: None,
        },
    );
}

#[test]
fn garrison_into_transport_pairs_the_units_whose_class_has_the_flags() {
    // FUN_141621220: a villager (class 4, flags 0x10163) with a garrison task on the ship's
    // class 20 pairs with it and, through the unit AIs, queues its enter order on it; the
    // archer (class 0) has no such task, and the ship itself (class 20) no class flags
    let mut let_out_world = let_out_world();
    ship_at(&mut let_out_world, Point::new(5.5, 12.5));
    walker(
        &mut let_out_world,
        PlayerId::new(1),
        1000,
        VILLAGER,
        Point::new(5.5, 5.5),
    );
    walker(
        &mut let_out_world,
        PlayerId::new(1),
        1001,
        ARCHER,
        Point::new(6.5, 5.5),
    );
    let_out_world
        .garrison_order(
            player_id(1),
            &object_ids([1000, 1001, 200]),
            Some(object_id(200)),
            Dispatch::Queued,
            GarrisonScope::Target,
        )
        .unwrap();
    let orders = |world: &World, id| {
        world
            .parts
            .unit_ai
            .get(world.object(object_id(id)).unwrap().1.id)
            .map(|ai| {
                ai.orders
                    .iter()
                    .map(|queued_order| (queued_order.order, queued_order.target))
                    .collect::<Vec<_>>()
            })
    };
    assert_eq!(
        orders(&let_out_world, 1000),
        Some(vec![(OrderKind::Enter, Some(object_id(200)))])
    );
    assert_eq!(orders(&let_out_world, 1001), Some(vec![]));
    // a transport beyond the town bell's reach (param_6 1) pairs nothing
    let mut world = sized_world(40, 40);
    ship_at(&mut world, Point::new(5.5, 35.5));
    walker(
        &mut world,
        PlayerId::new(1),
        1000,
        VILLAGER,
        Point::new(5.5, 5.5),
    );
    let shelters = Shelters {
        buildings: Vec::new(),
        transports: container_ids([200]),
        counts: vec![InsideCount::ZERO],
    };
    world
        .shelter(
            PlayerId::new(1),
            &object_id_list([1000]),
            &shelters,
            ShelterReach::TownBell,
            Dispatch::Queued,
        )
        .unwrap();
    assert_eq!(orders(&world, 1000), Some(vec![]));
    world
        .shelter(
            PlayerId::new(1),
            &object_id_list([1000]),
            &shelters,
            ShelterReach::Far,
            Dispatch::Queued,
        )
        .unwrap();
    assert_eq!(
        orders(&world, 1000),
        Some(vec![(OrderKind::Enter, Some(object_id(200)))])
    );
    // with no unit the transport is never asked
    assert_eq!(
        world.garrison_order(
            player_id(1),
            &[],
            Some(object_id(200)),
            Dispatch::Now,
            GarrisonScope::Target
        ),
        Ok(())
    );
}

/// [`sized_world`] 40 × 40 with a beach row (terrain 1, flags 0x10) at y = 17 and water
/// (terrain 2) from y = 18 on, every terrain passable for restriction 0; the villager's
/// garrison task on transports (class 20) has range 1; ship 200 (speed 1.1, type 70) at
/// `ship` and villager 1000 at `at`, each where it last moved to its place.
fn shore_world(at: Point, ship: Point) -> World {
    let mut world = sized_world(40, 40);
    for masters in &mut world.model.masters {
        if let Some(master) = Arc::make_mut(masters).get_mut(VILLAGER) {
            master.action.get_or_insert_with(Default::default).tasks = vec![
                task(GARRISON, 3),
                Task {
                    work_range: Distance::new(1.0),
                    ..task(GARRISON, 20)
                },
            ];
        }
    }
    let tiles = (0..40 * 40)
        .map(|tile| match tile / 40 {
            17 => (1, 255, -1),
            y if 18 <= y => (2, 255, -1),
            _ => (0, 255, -1),
        })
        .map(|(kind, restore, layer)| TileTerrain::of_save(kind, restore, layer))
        .collect();
    let kind = |flags| TerrainKind {
        shown: true,
        flags,
        forest: false,
    };
    let mut terrain = Terrain::new(
        GridSize::new(40, 40),
        tiles,
        vec![
            kind(TerrainFlags::LAND),
            kind(TerrainFlags::BEACH),
            kind(TerrainFlags::WATER),
        ],
        vec![Restriction::new(vec![1.0, 1.0, 1.0])],
    );
    terrain.set_elevations(vec![0; 40 * 40]);
    world.spatial.terrain = Some(terrain);
    ship_at(&mut world, ship);
    let mut mover = tests_walker();
    mover.stats = MoveStats {
        speed: Speed::new(1.1),
        base: Speed::new(1.1),
        ..standing()
    };
    mover.moved_at = ship.with_height(0.0);
    world.parts.put_mover(object_id(200), Some(mover));
    walker(&mut world, PlayerId::new(1), 1000, VILLAGER, at);
    world.parts.mover_mut(object_id(1000)).unwrap().moved_at = at.with_height(0.0);
    world
}

#[test]
fn villager_sent_into_ship_meets_it_at_the_shore_on_the_way() {
    // FUN_14161e370 (no +0x1e) -> FUN_141621220's transports -> slot 0x7e8 with filter 3 ->
    // the finder's LAB_1412796b4 -> the Enter: FUN_140a85f30 walks from the ship's tile
    // (5, 30) to the villager's (5, 12) and meets it on the beach tile (5, 17), code 1: the
    // ship reaches within 1 of (5, 17) (slot 0x3d8, the long pather's answer) and moves
    // there (slot 0x340: a MoveTo, range 0, with the master's running sprite 2001)
    let mut world = shore_world(Point::new(5.5, 12.5), Point::new(5.5, 30.5));
    let running = Graphic {
        id: SpriteId::named(2001),
        frames: Frame::of_data(10),
        facets: FacetCount::of_data(16),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::ANIMATED,
    };
    let _ = Arc::make_mut(&mut world.model.graphics).insert(SpriteId::named(2001), running);
    for masters in &mut world.model.masters {
        if let Some(master) = Arc::make_mut(masters).get_mut(SHIP) {
            master
                .moving
                .get_or_insert_with(Default::default)
                .run_sprite = Some(SpriteId::named(2001));
        }
    }
    world
        .garrison_order(
            player_id(1),
            &object_ids([1000]),
            Some(object_id(200)),
            Dispatch::Now,
            GarrisonScope::Target,
        )
        .unwrap();
    let ship = world
        .parts
        .actions_of(object_id(200))
        .list
        .move_to()
        .copied()
        .unwrap();
    assert_eq!(
        (
            ship.goal(),
            ship.range,
            ship.sprite.map(|graphic| graphic.id),
            ObjectId::field(ship.target)
        ),
        (
            Some(Position::new(5.5, 17.5, 0.0)),
            Distance::ZERO,
            Some(SpriteId::named(2001)),
            -1
        )
    );
    let enter = world
        .parts
        .actions_of(object_id(1000))
        .list
        .enter()
        .copied()
        .unwrap();
    assert_eq!(
        (
            enter.target,
            enter.point,
            enter.state.bare(),
            enter
                .state
                .move_to()
                .copied()
                .map(|move_to_action| move_to_action.state)
        ),
        (
            Some(container_id(200)),
            Position::new(5.5, 17.5, 0.0),
            EnterState::Walk { move_to: None },
            Some(MoveTo::Start)
        )
    );
    // the unit AI holds the enter order and action, the ship its target
    let ai = world.parts.unit_ai.get(object_id(1000)).cloned().unwrap();
    assert_eq!(
        (ai.current_order, ai.current_action, ai.current_target),
        (
            Some(OrderKind::Enter),
            Some(AiAction::Enter),
            Some(object_id(200))
        )
    );
}

#[test]
fn villager_meets_moving_ship_where_it_goes() {
    // FUN_140a85f30 on a ship under a MoveTo (state 3): its goal, code 0, no order
    let mut world = shore_world(Point::new(5.5, 12.5), Point::new(5.5, 30.5));
    world
        .parts
        .actions
        .of_mut(object_id(200))
        .unwrap()
        .list
        .set_move_to(Some(MoveToAction {
            sprite: None,
            state: MoveTo::Start,
            point: Position::new(9.5, 19.5, 0.0),
            target: None,
            range: Distance::ZERO,
            voice: Voice::Loud,
            on_target_moved: TargetMoved::Follows,
            wait: MoveWait::default(),
        }));
    world
        .garrison_order(
            player_id(1),
            &object_ids([1000]),
            Some(object_id(200)),
            Dispatch::Now,
            GarrisonScope::Target,
        )
        .unwrap();
    let enter = world
        .parts
        .actions_of(object_id(1000))
        .list
        .enter()
        .copied()
        .unwrap();
    assert_eq!(
        (
            enter.point,
            enter
                .state
                .move_to()
                .copied()
                .map(|move_to_action| move_to_action.state)
        ),
        (Position::new(9.5, 19.5, 0.0), Some(MoveTo::Start))
    );
    assert_eq!(
        world
            .parts
            .actions_of(object_id(200))
            .list
            .move_to()
            .copied()
            .unwrap()
            .goal(),
        Some(Position::new(9.5, 19.5, 0.0))
    );
    // the same order again: the work command finds the Enter's own task, whose slot 9 (the
    // base's, 0x140a7bf40) takes no new target, so a new Enter replaces it
    world
        .parts
        .actions
        .of_mut(object_id(1000))
        .unwrap()
        .list
        .enter_mut()
        .unwrap()
        .retry = false;
    world
        .garrison_order(
            player_id(1),
            &object_ids([1000]),
            Some(object_id(200)),
            Dispatch::Now,
            GarrisonScope::Target,
        )
        .unwrap();
    let enter = world
        .parts
        .actions_of(object_id(1000))
        .list
        .enter()
        .copied()
        .unwrap();
    assert_eq!((enter.target, enter.retry), (Some(container_id(200)), true));
}

#[test]
fn villager_meets_sailing_transport_where_it_sails() {
    // FUN_140a85f30 on a ship whose Transport sails (state 11): the Transport's point (its
    // +0x38, +0x3c, FUN_140a7be40 and FUN_140a7be50), code 0
    let mut world = shore_world(Point::new(5.5, 12.5), Point::new(5.5, 30.5));
    world
        .parts
        .actions
        .of_mut(object_id(200))
        .unwrap()
        .list
        .set_transport(Some(transport::TransportAction {
            state: TransportState::Sail {
                move_to: Some(MoveToAction::sub_in(MoveTo::Start)),
            },
            point: Position::new(9.5, 19.5, 0.0),
            ..transport::TransportAction::new(Position::new(5.5, 30.5, 0.0))
        }));
    world
        .garrison_order(
            player_id(1),
            &object_ids([1000]),
            Some(object_id(200)),
            Dispatch::Now,
            GarrisonScope::Target,
        )
        .unwrap();
    let enter = world
        .parts
        .actions_of(object_id(1000))
        .list
        .enter()
        .copied()
        .unwrap();
    assert_eq!(
        (
            enter.point,
            enter
                .state
                .move_to()
                .copied()
                .map(|move_to_action| move_to_action.state)
        ),
        (Position::new(9.5, 19.5, 0.0), Some(MoveTo::Start))
    );
}

/// A 30 × 20 map, water (terrain 1, flags 1) west of x = 15 and land (terrain 0, flags 8)
/// east of it, with the planners and zone maps of land units (restriction 0) and ships
/// (restriction 1). The ship 200 (a transport's unit AI, type 11; speed 1.1) floats at
/// `ship` with villager 1000 inside; the streams are seeded.
fn coast_world(ship: Point) -> World {
    let mut world = sized_world(30, 20);
    let sails = MoveStats {
        speed: Speed::new(1.1),
        base: Speed::new(1.1),
        radius: Radii::new(0.5, 0.5, 2.0),
        restriction: RestrictionId::named(1),
        ..standing()
    };
    for masters in &mut world.model.masters {
        if let Some(master) = Arc::make_mut(masters).get_mut(SHIP) {
            master.terrain_restriction = Some(RestrictionId::named(1));
            master.radius = Footprint::new(0.5, 0.5);
            master.height = Distance::new(1.0);
            master.garrison = GarrisonCapacity::of_data(5);
            master.move_stats = Some(sails);
        }
        if let Some(master) = Arc::make_mut(masters).get_mut(VILLAGER) {
            master.terrain_restriction = Some(RestrictionId::named(0));
        }
    }
    let tiles: Vec<(u8, u8, i16)> = (0..30 * 20)
        .map(|tile| {
            if tile % 30 < 15 {
                (1, 255, -1)
            } else {
                (0, 255, -1)
            }
        })
        .collect();
    let kinds: Vec<TerrainId> = tiles.iter().map(|tile| TerrainId::named(tile.0)).collect();
    let kind = |flags| TerrainKind {
        shown: true,
        flags,
        forest: false,
    };
    let restrictions = vec![
        Restriction::new(vec![1.0, 0.0]),
        Restriction::new(vec![0.0, 1.0]),
    ];
    let mut terrain = Terrain::new(
        GridSize::new(30, 20),
        tiles
            .into_iter()
            .map(|(kind, restore, layer)| TileTerrain::of_save(kind, restore, layer))
            .collect(),
        vec![kind(TerrainFlags::SHALLOWS), kind(TerrainFlags::WATER)],
        restrictions.clone(),
    );
    terrain.set_elevations(vec![0; 30 * 20]);
    world.spatial.terrain = Some(terrain);
    world.pathing = Pathing::new(GridSize::new(30, 20), kinds, restrictions, 0);
    world.streams.seed_all(7);
    ship_at(&mut world, ship);
    let mut mover = tests_walker();
    mover.stats = sails;
    mover.moved_at = ship.with_height(0.0);
    world.parts.set_unit_ai(
        object_id(200),
        Some(UnitAi {
            ai_kind: UnitAiKind::Transport,
            current_order: None,
            current_action: None,
            current_target: None,
            ..UnitAi::default()
        }),
    );
    world.parts.put_mover(object_id(200), Some(mover));
    let unit = world.object_mut(object_id(200)).unwrap().1;
    unit.garrison = vec![object_id(1000)];
    walker(&mut world, PlayerId::new(1), 1000, VILLAGER, ship);
    world.object_mut(object_id(1000)).unwrap().1.life.inside =
        Some(ContainerId::new(object_id(200)));
    world
        .spatial
        .go_inside(object_id(1000), ship.with_height(0.0));
    world
}

/// The first place about `center` within `radius` (`find_drop_location`'s draws on a copy of
/// the Object stream) where a unit of radius 0.2 stands on the land (x 15 and up) and clear
/// of the ship.
fn ashore(world: &mut World, center: Point, radius: f32) -> Point {
    let mut rng = world.streams.get_mut(Stream::Object).clone();
    let gap = radius + (0.2 + 0.01);
    let along =
        |draw: u32, centre: f32| ((draw as f32 * (radius + radius)) / 32767.0 - radius) + centre;
    let mut place = Point::ORIGIN;
    loop {
        place = match (rng.rand15() as i32 * 4) / 0x7fff {
            0 => Point::new(center.x() + gap, along(rng.rand15(), center.y())),
            1 => Point::new(center.x() - gap, along(rng.rand15(), center.y())),
            2 => Point::new(along(rng.rand15(), center.x()), center.y() + gap),
            3 => Point::new(along(rng.rand15(), center.x()), center.y() - gap),
            _ => place,
        };
        if 15.0 <= place.x() - 0.2 {
            return place;
        }
    }
}

/// The object stop (slot 0x148) of a ship sailing to unload: its Transport's slot 0x38 (slot 7,
/// `FUN_140aa4620`: `set_state(2)`, true) keeps the action list, the Transport idle, its
/// `MoveTo` gone. (A unit AI's stop, which a STOP runs after it, would clear the list.)
#[test]
fn stopped_transport_keeps_its_idle_transport() {
    let mut world = coast_world(Point::new(10.5, 10.5));
    UnloadCommand {
        player: player_id(1),
        units: unit_list(object_ids([200])),
        point: Point::new(17.5, 10.5),
        pick: UnloadPick::of_packet(0, -1),
        sound: false,
    }
    .run(&mut world)
    .unwrap();
    world.parts.set_unit_ai(object_id(200), None);
    let list_index = world.players[1]
        .objects
        .position_of(object_id(200))
        .unwrap();
    world.object_stop(PlayerId::new(1), list_index).unwrap();
    let transport = world
        .parts
        .actions_of(object_id(200))
        .list
        .transport()
        .copied()
        .expect("the Transport");
    assert_eq!(
        (transport.state.bare(), transport.state.move_to().is_some()),
        (TransportState::Idle, false)
    );
}

/// UNLOAD of a ship off the shore (`FUN_14161f410`'s last branch): its unit AI takes the
/// unload order on itself at once (0x2d1 -> slot 0x118 -> slot 0x3a8 `FUN_14108d050`): a new
/// Transport (`FUN_140aa3ab0`) at the ship's place starts (`set_state(11)`), then its work at
/// the point: no tile of the point's zone (the land, on the ship's zone map) lies within 2 of
/// the ship's tile, so it sails there to unload (`set_state(4)`), toward the last water tile of
/// the planner's path to the point (`FUN_140f7c390`), its `MoveTo` starting; the unit AI holds
/// the unload's action value with the ship itself as its target.
#[test]
fn an_unload_command_sends_transport_toward_the_shore() {
    let mut world = coast_world(Point::new(10.5, 10.5));
    UnloadCommand {
        player: player_id(1),
        units: unit_list(object_ids([200])),
        point: Point::new(17.5, 10.5),
        pick: UnloadPick::of_packet(0, -1),
        sound: false,
    }
    .run(&mut world)
    .unwrap();
    let ship = world.object(object_id(200)).unwrap().1;
    let tr = world
        .parts
        .actions_of(object_id(200))
        .list
        .transport()
        .copied()
        .expect("the Transport");
    assert_eq!(
        (
            tr.state.bare(),
            tr.mode,
            tr.point,
            tr.state
                .move_to()
                .copied()
                .map(|move_to_action| move_to_action.state),
            tr.retarget
        ),
        (
            TransportState::SailToUnload { move_to: None },
            TransportMode::Work,
            Position::new(14.5, 10.5, 0.0),
            Some(MoveTo::Start),
            false
        )
    );
    let ai = world.parts.unit_ai.get(ship.id).unwrap();
    assert_eq!(
        (ai.current_order, ai.current_action, ai.current_target),
        (
            Some(OrderKind::Unload),
            Some(AiAction::Unload),
            Some(object_id(200))
        )
    );
    // the villager inside stays there
    assert_eq!(
        world.object(object_id(1000)).unwrap().1.life.inside,
        Some(container_id(200))
    );
    // a second unload: the immediate order stops the unit AI's action first (slot 0xb0, the
    // unit's stop tears the Transport down), so the command makes a new one alike
    UnloadCommand {
        player: player_id(1),
        units: unit_list(object_ids([200])),
        point: Point::new(17.5, 10.5),
        pick: UnloadPick::of_packet(0, -1),
        sound: false,
    }
    .run(&mut world)
    .unwrap();
    let again = world
        .parts
        .actions_of(object_id(200))
        .list
        .transport()
        .copied()
        .unwrap();
    assert_eq!(
        (again.state.bare(), again.point),
        (tr.state.bare(), tr.point)
    );
    // slot 0x3a8 with the Transport on top: "Telling existing action to work" at the ship's
    // own place, whose zone lies under the ship: it unloads there (set_state(6)), its MoveTo
    // gone
    let idx = world.players[1]
        .objects
        .position_of(object_id(200))
        .unwrap();
    world
        .transport_order(PlayerId::new(1), idx, Position::new(17.5, 10.5, 0.0))
        .unwrap();
    let tr = world
        .parts
        .actions_of(object_id(200))
        .list
        .transport()
        .copied()
        .unwrap();
    assert_eq!(
        (tr.state.bare(), tr.point, tr.state.move_to().copied()),
        (TransportState::Unload, Position::new(10.5, 10.5, 0.0), None)
    );
}

/// [`coast_world`] with the ship's task 7 of type 12 for the land (terrain 0; the game's data
/// has it for a beach), the market (84) and its building 4000 of player 1 on the land, and the
/// ship's place in its player's list.
fn transport_task_world() -> (World, ListIndex) {
    let mut world = coast_world(Point::new(10.5, 10.5));
    for masters in &mut world.model.masters {
        let market = Master {
            id: ids::MARKET,
            ..masters.get(TC).expect("the town centre").clone()
        };
        let mut all: Vec<Master> = masters.iter().cloned().collect();
        all.push(market);
        *masters = Arc::new(Masters::new(all));
        if let Some(master) = Arc::make_mut(masters).get_mut(SHIP) {
            master
                .action
                .get_or_insert_with(Default::default)
                .tasks
                .push(Task {
                    id: task_id(7),
                    action_kind: transport::TRANSPORT,
                    object_group: None,
                    object_id: None,
                    terrain: TaskTerrain::of_data(0),
                    ..Task::default()
                });
        }
    }
    building_at(&mut world, 4000, ids::MARKET, Point::new(18.5, 12.5), None);
    let idx = world.players[1]
        .objects
        .position_of(object_id(200))
        .unwrap();
    (world, idx)
}

/// A work order (slot 0x7e8) on `target` (−1 for none) at `point`, as INTERACT gives it.
fn work_at(target: i32, point: Position) -> Work {
    Work {
        target: ObjectId::of_field(target),
        point,
        disable_retargeting: false,
        filter: None,
    }
}

/// The work command (`FUN_14106ff70`) of a transport on a point: the finder's general search
/// gives the task for the terrain under the point (`FUN_1411a8ba0`), whose Transport
/// (`FUN_140aa3ab0` on the task at the point) starts (`set_state(11)`: it sails toward the
/// shore on the way) and whose notify (`FUN_14106e4e0` kind 0xc) gives the unit AI the transport
/// order and action without a target. A second work on the same task retakes it (its slot
/// 0x48: the work at the new point, `+0x34` 1, sailing there for another object's order);
/// water under the point gives no task, so the ship just moves.
#[test]
fn work_command_on_land_point_makes_transport_that_sails_there() {
    let (mut world, idx) = transport_task_world();
    world
        .order_work(
            PlayerId::new(1),
            idx,
            &work_at(-1, Position::new(17.5, 10.5, 0.0)),
        )
        .unwrap();
    let ship = world.object(object_id(200)).unwrap().1;
    let tr = world
        .parts
        .actions_of(object_id(200))
        .list
        .transport()
        .copied()
        .expect("the Transport");
    assert_eq!(
        (
            tr.state.bare(),
            tr.mode,
            tr.task.map(|task| task.id),
            tr.point,
            tr.state
                .move_to()
                .copied()
                .map(|move_to_action| move_to_action.state)
        ),
        (
            TransportState::Sail { move_to: None },
            TransportMode::New,
            Some(task_id(7)),
            Position::new(14.5, 10.5, 0.0),
            Some(MoveTo::Start)
        )
    );
    let ai = world.parts.unit_ai.get(ship.id).unwrap();
    assert_eq!(
        (ai.current_order, ai.current_action, ai.current_target),
        (Some(OrderKind::Transport), Some(AiAction::Transport), None)
    );
    world
        .order_work(
            PlayerId::new(1),
            idx,
            &work_at(-1, Position::new(18.5, 11.5, 0.0)),
        )
        .unwrap();
    let again = world
        .parts
        .actions_of(object_id(200))
        .list
        .transport()
        .copied()
        .unwrap();
    assert_eq!(
        (again.state.bare(), again.mode, again.task),
        (
            TransportState::Sail { move_to: None },
            TransportMode::Work,
            tr.task
        )
    );
    // water: no task, no Transport
    let (mut world, idx) = transport_task_world();
    world
        .order_work(
            PlayerId::new(1),
            idx,
            &work_at(-1, Position::new(5.5, 10.5, 0.0)),
        )
        .unwrap();
    assert_eq!(
        world
            .parts
            .actions_of(object_id(200))
            .list
            .transport()
            .copied(),
        None
    );
}

/// The work command's retake (`LAB_1410706a8`) passes over a trade boat or cart (classes 2 and
/// 19, `FUN_141195490`, `FUN_141195400`) ordered to its own player's market: the factory makes
/// a new Transport (`+0x34` −1, not the retake's 1), whose notify names the market.
#[test]
fn trade_boat_work_at_its_own_market_makes_new_transport() {
    let (mut world, idx) = transport_task_world();
    for masters in &mut world.model.masters {
        if let Some(master) = Arc::make_mut(masters).get_mut(SHIP) {
            master.class = class::TRADE_BOAT;
        }
    }
    world
        .order_work(
            PlayerId::new(1),
            idx,
            &work_at(-1, Position::new(17.5, 10.5, 0.0)),
        )
        .unwrap();
    world
        .order_work(
            PlayerId::new(1),
            idx,
            &work_at(4000, Position::new(18.5, 12.5, 0.0)),
        )
        .unwrap();
    let ship = world.object(object_id(200)).unwrap().1;
    assert_eq!(
        world
            .parts
            .actions_of(object_id(200))
            .list
            .transport()
            .map(|tr| tr.mode),
        Some(TransportMode::New)
    );
    let ai = world.parts.unit_ai.get(ship.id).unwrap();
    assert_eq!(
        (ai.current_target, ai.target_class),
        (Some(object_id(4000)), class_of(52))
    );
    // a transport ship (class 20) retakes it on the same work
    let (mut world, idx) = transport_task_world();
    world
        .order_work(
            PlayerId::new(1),
            idx,
            &work_at(-1, Position::new(17.5, 10.5, 0.0)),
        )
        .unwrap();
    world
        .order_work(
            PlayerId::new(1),
            idx,
            &work_at(4000, Position::new(18.5, 12.5, 0.0)),
        )
        .unwrap();
    assert_eq!(
        world
            .parts
            .actions_of(object_id(200))
            .list
            .transport()
            .map(|tr| tr.mode),
        Some(TransportMode::Work)
    );
}

/// A ship beside the shore takes the unload order and unloads at once (`set_state(6)`: the
/// land within 2 of its tile, the point within 2 of its footprint); its unload
/// (`FUN_140aa3ba0` -> `FUN_141191b40`) puts the villager ashore east of it (the Object
/// stream's `find_drop_location`), out of the ship's list, its unit AI's state position the
/// spot; with none left inside the owner hears nothing, and the Transport idles.
#[test]
fn transport_beside_the_shore_puts_its_units_ashore() {
    let mut world = coast_world(Point::new(14.5, 10.5));
    UnloadCommand {
        player: player_id(1),
        units: unit_list(object_ids([200])),
        point: Point::new(15.5, 10.5),
        pick: UnloadPick::of_packet(0, -1),
        sound: false,
    }
    .run(&mut world)
    .unwrap();
    let tr = world
        .parts
        .actions_of(object_id(200))
        .list
        .transport()
        .copied()
        .unwrap();
    assert_eq!(
        (tr.state.bare(), tr.point, tr.state.move_to().copied()),
        (TransportState::Unload, Position::new(15.5, 10.5, 0.0), None)
    );
    let spot = ashore(&mut world, Point::new(14.5, 10.5), 0.5);
    let idx = world.players[1]
        .objects
        .position_of(object_id(200))
        .unwrap();
    world.transport_unload(PlayerId::new(1), idx).unwrap();
    let unit = world.object(object_id(1000)).unwrap().1;
    assert_eq!(unit.life.inside, None);
    assert_eq!(unit.position(), spot.with_height(0.0));
    assert_eq!(
        world.parts.unit_ai.get(unit.id).unwrap().state_position,
        spot
    );
    assert_eq!(
        world
            .spatial
            .get(object_id(1000))
            .map(|placed| placed.position),
        Some(spot.with_height(0.0))
    );
    let ship = world.object(object_id(200)).unwrap().1;
    assert_eq!(ship.garrison, Vec::<ObjectId>::new());
    assert_eq!(
        world
            .parts
            .actions_of(object_id(200))
            .list
            .transport()
            .map(|transport_action| transport_action.state.bare()),
        Some(TransportState::Idle)
    );
    // a move to a point: the idle Transport takes it (its slot 8, 0x140aa3e40) and sails
    // there, its MoveTo starting; no MoveTo of the unit's own
    let taken = world
        .move_to_point(PlayerId::new(1), idx, Position::new(5.5, 5.5, 0.0))
        .unwrap();
    let tr = world
        .parts
        .actions_of(object_id(200))
        .list
        .transport()
        .copied()
        .unwrap();
    assert_eq!(taken, Answer::Taken);
    assert_eq!(
        (
            tr.state.bare(),
            tr.mode,
            tr.point,
            tr.state
                .move_to()
                .copied()
                .map(|move_to_action| move_to_action.state),
            world
                .parts
                .actions_of(object_id(200))
                .list
                .move_to()
                .copied()
        ),
        (
            TransportState::Sail { move_to: None },
            TransportMode::Move,
            Position::new(5.5, 5.5, 0.0),
            Some(MoveTo::Start),
            None
        )
    );
}

/// No drop location on the land: the point moves (the Action stream's "Unload" draws) until
/// a draw leaves the water, and the Transport sails there to unload (`set_state(4)`, `+0x68`):
/// its update's end leaves it so.
#[test]
fn transport_without_drop_location_sails_on_to_new_point() {
    let mut world = coast_world(Point::new(14.5, 10.5));
    // the villager's master stands nowhere (restriction 0 refuses both terrains)
    if let Some(terrain) = world.spatial.terrain.as_mut() {
        terrain.restrictions[0] = Restriction::new(vec![0.0, 0.0]);
    }
    UnloadCommand {
        player: player_id(1),
        units: unit_list(object_ids([200])),
        point: Point::new(15.5, 10.5),
        pick: UnloadPick::of_packet(0, -1),
        sound: false,
    }
    .run(&mut world)
    .unwrap();
    let idx = world.players[1]
        .objects
        .position_of(object_id(200))
        .unwrap();
    let mut action = world.streams.get_mut(Stream::Action).clone();
    world.transport_unload(PlayerId::new(1), idx).unwrap();
    assert_eq!(
        world.object(object_id(1000)).unwrap().1.life.inside,
        Some(container_id(200))
    );
    // the first draws: from the old point (15.5, 10.5), r / 0xccc tiles less 5
    let x = (action.rand15() / 0xccc) as f32 + 15.5 - 5.0;
    let y = (action.rand15() / 0xccc) as f32 + 10.5 - 5.0;
    let tr = world
        .parts
        .actions_of(object_id(200))
        .list
        .transport()
        .copied()
        .unwrap();
    assert!(tr.retarget);
    assert_eq!(
        tr.state.bare(),
        TransportState::SailToUnload { move_to: None }
    );
    if 15.0 <= x {
        // the first draw is on land: the MoveTo's point is the shore on the way to it
        assert_eq!(
            tr.state
                .move_to()
                .copied()
                .map(|move_to_action| move_to_action.state),
            Some(MoveTo::Start)
        );
        assert!(tr.point.x() < 15.0, "{:?} from ({x}, {y})", tr.point);
    }
}

/// UNLOAD of the villager inside the ship (`FUN_14161f410` with a container of type 70): it
/// goes ashore about the ship within twice its radii (the Object stream's
/// `find_drop_location`, any zone), leaves it for the place and takes its stance again
/// there; the ship's own Transport stays untouched.
#[test]
fn unit_inside_transport_unloaded_goes_ashore_about_it() {
    let mut world = coast_world(Point::new(14.5, 10.5));
    let spot = ashore(&mut world, Point::new(14.5, 10.5), 1.0);
    UnloadCommand {
        player: player_id(1),
        units: unit_list(object_ids([1000])),
        point: Point::new(-1.0, -1.0),
        pick: UnloadPick::of_packet(0, -1),
        sound: false,
    }
    .run(&mut world)
    .unwrap();
    let unit = world.object(object_id(1000)).unwrap().1;
    assert_eq!(unit.life.inside, None);
    assert_eq!(unit.position(), spot.with_height(0.0));
    assert_eq!(
        world.parts.unit_ai.get(unit.id).unwrap().state_position,
        spot
    );
    assert!(!world.parts.town_bell_of(unit.id).flag.is_sheltered());
    let ship = world.object(object_id(200)).unwrap().1;
    assert_eq!(ship.garrison, Vec::<ObjectId>::new());
    assert_eq!(
        world
            .parts
            .actions_of(object_id(200))
            .list
            .transport()
            .copied(),
        None
    );
    // flag 3 names the unit inside: the ship lets it out the same way
    let mut world = coast_world(Point::new(14.5, 10.5));
    let spot = ashore(&mut world, Point::new(14.5, 10.5), 1.0);
    UnloadCommand {
        player: player_id(1),
        units: unit_list(object_ids([200])),
        point: Point::new(-1.0, -1.0),
        pick: UnloadPick::of_packet(3, 1000),
        sound: false,
    }
    .run(&mut world)
    .unwrap();
    assert_eq!(
        world.object(object_id(1000)).unwrap().1.position(),
        spot.with_height(0.0)
    );
    // a named unit elsewhere stays
    let mut world = coast_world(Point::new(14.5, 10.5));
    UnloadCommand {
        player: player_id(1),
        units: unit_list(object_ids([200])),
        point: Point::new(-1.0, -1.0),
        pick: UnloadPick::of_packet(3, 100),
        sound: false,
    }
    .run(&mut world)
    .unwrap();
    assert_eq!(
        world.object(object_id(1000)).unwrap().1.life.inside,
        Some(container_id(200))
    );
}

/// UNLOAD of the issuer's villager inside an ally's ship (426107079's player 3 at update 409):
/// the check keeps the ally's ship the command ties to the issuer's unit (flag 3,
/// `FUN_140e6f620`), the command's list holds any player's object (`FUN_140e6b750`), and the
/// villager goes ashore about it. Without the alliance both ways the ship goes from the list.
#[test]
fn unit_inside_an_ally_transport_unloaded_goes_ashore() {
    let ally = |both_ways: bool| {
        let mut coast_world = coast_world(Point::new(14.5, 10.5));
        let list_index = coast_world.players[1]
            .objects
            .position_of(object_id(200))
            .unwrap();
        let ship = coast_world
            .remove_live(PlayerId::new(1), list_index)
            .unwrap();
        let _ = coast_world.push_live(PlayerId::new(2), ship).unwrap();
        coast_world.players[1].relations = relation_row([3, 0, 0, 3]);
        coast_world.players[2].relations = ByPlayer::new(vec![
            Relation::Enemy,
            if both_ways {
                Relation::Relation4
            } else {
                Relation::Enemy
            },
            Relation::Ally,
            Relation::Enemy,
        ]);
        coast_world
    };
    let mut world = ally(true);
    let spot = ashore(&mut world, Point::new(14.5, 10.5), 1.0);
    UnloadCommand {
        player: player_id(1),
        units: unit_list(object_ids([200])),
        point: Point::new(-1.0, -1.0),
        pick: UnloadPick::of_packet(3, 1000),
        sound: false,
    }
    .run(&mut world)
    .unwrap();
    let unit = world.object(object_id(1000)).unwrap().1;
    assert_eq!(unit.life.inside, None);
    assert_eq!(unit.position(), spot.with_height(0.0));
    let mut world = ally(false);
    UnloadCommand {
        player: player_id(1),
        units: unit_list(object_ids([200])),
        point: Point::new(-1.0, -1.0),
        pick: UnloadPick::of_packet(3, 1000),
        sound: false,
    }
    .run(&mut world)
    .unwrap();
    assert_eq!(
        world.object(object_id(1000)).unwrap().1.life.inside,
        Some(container_id(200))
    );
}

#[test]
fn villager_beside_ship_enters_it_at_once() {
    // in reach (gap 0.8, range 1) of a ship with speed: the tiles (5, 19) and (5, 20) are
    // the two left out, no building between; FUN_140a867b0 -> FUN_1411921d0 -> the ship's
    // slot 0x1c0 (FUN_14120b790): inside at the ship's place; FUN_1412067f0 with 3 records
    // the Enter (the ship at its place, the enter action) and leaves the flag clear
    let mut world = shore_world(Point::new(5.5, 19.0), Point::new(5.5, 20.5));
    world
        .garrison_order(
            player_id(1),
            &object_ids([1000]),
            Some(object_id(200)),
            Dispatch::Now,
            GarrisonScope::Target,
        )
        .unwrap();
    let object = world.object(object_id(1000)).unwrap().1;
    assert_eq!(object.life.inside, Some(container_id(200)));
    assert_eq!(object.position().ground(), Point::new(5.5, 20.5));
    assert_eq!(
        world
            .parts
            .actions_of(object_id(1000))
            .list
            .enter()
            .map(|enter_action| enter_action.state),
        Some(EnterState::Done)
    );
    assert!(!world.parts.town_bell_of(object.id).flag.is_sheltered());
    assert_eq!(
        world.parts.town_bell_of(object.id).work,
        BackToWork {
            target: Some(object_id(200)),
            target_class: None,
            action: Some(AiAction::Enter),
            target2: None,
            point: Point::new(5.5, 20.5),
        }
    );
    assert_eq!(
        world.object(object_id(200)).unwrap().1.garrison,
        [object_id(1000)]
    );
    // the transport's count takes it: no room for another
    let (bp, unit) = world.object(object_id(200)).unwrap();
    assert!(!world.has_room(bp, unit, None).unwrap());
}

#[test]
fn villager_enters_an_allys_ship_with_its_owners_masters() {
    // player 2's ship beside player 1's villager: FUN_14120eb10 lets an ally's transport take
    // it (both ways allied), and the enter adds the villager to the ship's list
    let mut world = shore_world(Point::new(5.5, 19.0), Point::new(5.5, 20.5));
    let players = world.players[1]
        .objects
        .position_of(object_id(200))
        .unwrap();
    let ship = world.remove_live(PlayerId::new(1), players).unwrap();
    let _ = world.push_live(PlayerId::new(2), ship).unwrap();
    let mut placed = *world.spatial.get(object_id(200)).unwrap();
    placed.player = PlayerId::new(2);
    world.spatial.insert(object_id(200), placed);
    world
        .garrison_order(
            player_id(1),
            &object_ids([1000]),
            Some(object_id(200)),
            Dispatch::Now,
            GarrisonScope::Target,
        )
        .unwrap();
    assert_eq!(
        world.object(object_id(1000)).unwrap().1.life.inside,
        Some(container_id(200))
    );
    assert_eq!(
        world.object(object_id(200)).unwrap().1.garrison,
        [object_id(1000)]
    );
    // an enemy's ship takes no one in (player 3)
    let mut world = shore_world(Point::new(5.5, 19.0), Point::new(5.5, 20.5));
    let list_index = world.players[1]
        .objects
        .position_of(object_id(200))
        .unwrap();
    let ship = world.remove_live(PlayerId::new(1), list_index).unwrap();
    let _ = world.push_live(PlayerId::new(3), ship).unwrap();
    world
        .garrison_order(
            player_id(1),
            &object_ids([1000]),
            Some(object_id(200)),
            Dispatch::Now,
            GarrisonScope::Target,
        )
        .unwrap();
    assert_eq!(
        world
            .parts
            .actions_of(object_id(1000))
            .list
            .enter()
            .copied(),
        None
    );
}

#[test]
fn seek_shelter_sends_the_players_villagers_and_checked_units_to_its_buildings() {
    // FUN_140e6a3a0 -> FUN_141622450 -> FUN_141621220 with no transports: the command's
    // check (FUN_140e558b0) drops another player's unit unless players may order each
    // other's (+0x1074); a villager walks to the town centre's Enter
    let mut game = let_out_world();
    walker(
        &mut game,
        PlayerId::new(1),
        1000,
        VILLAGER,
        Point::new(5.5, 5.5),
    );
    walker(
        &mut game,
        PlayerId::new(2),
        2000,
        VILLAGER,
        Point::new(5.5, 6.5),
    );
    walker(
        &mut game,
        PlayerId::new(1),
        1001,
        ARCHER,
        Point::new(6.5, 5.5),
    );
    game.seek_shelter(player_id(1), &object_ids([1000, 2000, 1001, 4242]))
        .unwrap();
    let enter = |world: &World, id: ObjectId| {
        world
            .parts
            .actions_of(id)
            .list
            .enter()
            .map(|enter_action| enter_action.target)
    };
    assert_eq!(enter(&game, object_id(1000)), Some(Some(container_id(100))));
    // the ally's villager was dropped by the check; the archer is no villager
    assert_eq!(
        (enter(&game, object_id(2000)), enter(&game, object_id(1001))),
        (None, None)
    );
    // with +0x1074 the ally's villager is the player's to send; its own player's town
    // centres are not asked, only the commanding player's
    let mut world = let_out_world();
    world.settings.flags.set(OptionFlags::CHEATS_ENABLED, true);
    walker(
        &mut world,
        PlayerId::new(2),
        2000,
        VILLAGER,
        Point::new(5.5, 6.5),
    );
    world
        .seek_shelter(player_id(1), &object_ids([2000]))
        .unwrap();
    assert_eq!(
        enter(&world, object_id(2000)),
        Some(Some(container_id(100)))
    );
    // a building that takes no villagers (garrison type bit 0) is not asked
    let mut world = let_out_world();
    for masters in &mut world.model.masters {
        if let Some(building_master) = Arc::make_mut(masters)
            .get_mut(TC)
            .and_then(|tc| tc.building.as_mut())
        {
            building_master.garrison_kind = GarrisonKinds::INFANTRY;
        }
    }
    walker(
        &mut world,
        PlayerId::new(1),
        1000,
        VILLAGER,
        Point::new(5.5, 5.5),
    );
    world
        .seek_shelter(player_id(1), &object_ids([1000]))
        .unwrap();
    assert_eq!(enter(&world, object_id(1000)), None);
}

/// UNLOAD of a building (`FUN_14161f410`'s last branch): each unit inside comes out toward
/// the point, its orders gone and, a unit (type 70), no longer under the town bell; an id the
/// world no longer holds is passed over. A building with orders of its own to hand on is not
/// modelled.
#[test]
fn unload_of_building_lets_every_unit_inside_out() {
    let mut world = let_out_world();
    inside_tc(&mut world, PlayerId::new(1), 1000, VILLAGER);
    inside_tc(&mut world, PlayerId::new(1), 1001, VILLAGER);
    world.players[1].objects[ListIndex::new(0)]
        .garrison
        .insert(0, object_id(1500));
    for id in [1000, 1001] {
        world
            .parts
            .set_town_bell_flag(object_id(id), BellShelter::Sheltered);
        world.parts.set_unit_ai(
            object_id(id),
            Some(UnitAi {
                orders: vec![QueuedOrder::default()],
                ..UnitAi::default()
            }),
        );
    }
    UnloadCommand {
        player: player_id(1),
        units: unit_list(object_ids([100])),
        point: Point::new(4.5, 4.5),
        pick: UnloadPick::of_packet(0, -1),
        sound: false,
    }
    .run(&mut world)
    .unwrap();
    assert_eq!(
        world.players[1].objects[ListIndex::new(0)].garrison,
        [object_id(1500)]
    );
    for id in [1000, 1001] {
        let unit = world.object(object_id(id)).unwrap().1;
        assert_eq!(unit.life.inside, None, "{id}");
        assert!(
            !world.parts.town_bell_of(unit.id).flag.is_sheltered(),
            "{id}"
        );
        assert!(
            world.parts.unit_ai.get(unit.id).unwrap().orders.is_empty(),
            "{id}"
        );
    }
    let mut world = let_out_world();
    inside_tc(&mut world, PlayerId::new(1), 1000, VILLAGER);
    world.parts.set_unit_ai(
        world.players[1].objects[ListIndex::new(0)].id,
        Some(UnitAi {
            building_orders: vec![QueuedOrder::default()],
            ..UnitAi::default()
        }),
    );
    assert!(
        UnloadCommand {
            player: player_id(1),
            units: unit_list(object_ids([100])),
            point: Point::new(4.5, 4.5),
            pick: UnloadPick::of_packet(0, -1),
            sound: false
        }
        .run(&mut world)
        .is_err()
    );
}

#[test]
fn unload_lets_unit_out_of_the_building_it_is_inside() {
    // FUN_14161f410 on a unit inside a building: FUN_1411e7220 toward the point, +0x9dc
    // cleared, its orders gone; the building's own unload of the others is not run
    let mut world = let_out_world();
    inside_tc(&mut world, PlayerId::new(1), 1000, VILLAGER);
    inside_tc(&mut world, PlayerId::new(1), 1001, VILLAGER);
    world
        .parts
        .set_town_bell_flag(object_id(1000), BellShelter::Sheltered);
    world
        .parts
        .set_unit_ai(object_id(1000), Some(UnitAi::default()));
    UnloadCommand {
        player: player_id(1),
        units: unit_list(object_ids([1000])),
        point: Point::new(4.5, 4.5),
        pick: UnloadPick::of_packet(0, -1),
        sound: false,
    }
    .run(&mut world)
    .unwrap();
    let unit = world.object(object_id(1000)).unwrap().1;
    assert_eq!(unit.life.inside, None);
    assert!(!world.parts.town_bell_of(unit.id).flag.is_sheltered());
    assert_eq!(
        world.players[1].objects[ListIndex::new(0)].garrison,
        [object_id(1001)]
    );
}

/// `FUN_1411e7220` toward no point from a building without a gather point (nor a command
/// queue): the unit comes out, its unit AI stops (slot 0xb0 with 1: its stopAction), forgets
/// its target and takes its stance again at its new place (slot 0x1c0 with `+0x1a9`).
#[test]
fn unit_let_out_toward_nothing_stops_and_keeps_its_stance() {
    let mut world = let_out_world();
    inside_tc(&mut world, PlayerId::new(1), 1000, VILLAGER);
    world.parts.set_unit_ai(
        object_id(1000),
        Some(UnitAi {
            current_action: Some(AiAction::Gather),
            current_target: Some(object_id(77)),
            stance: UnitStance::Defensive,
            ..UnitAi::default()
        }),
    );
    assert_eq!(
        world.let_out(
            PlayerId::new(1),
            ListIndex::new(0),
            object_id(1000),
            Point::new(NO_POINT, NO_POINT),
            (None, false)
        ),
        Ok(LetOut::Out)
    );
    let unit = world.object(object_id(1000)).unwrap().1;
    assert_eq!(unit.life.inside, None);
    let ai = world.parts.unit_ai.get(unit.id).unwrap();
    assert_eq!(
        (
            ai.current_action,
            ai.current_target,
            ai.stance.get(),
            ai.last_action
        ),
        (None, None, 1, Some(AiAction::Gather))
    );
    assert_eq!(ai.state_position, unit.position().ground());
}

/// [`let_out_world`] with villager 1000 inside town centre 100 on a map where the villager may
/// stand nowhere (its restriction's value 0 on the only terrain), the centre's master of radius
/// `radius`.
fn walled_in_world(radius: f32) -> World {
    let mut world = let_out_world();
    for masters in &mut world.model.masters {
        if let Some(tc) = Arc::make_mut(masters).get_mut(TC) {
            tc.radius = Footprint::new(radius, radius);
        }
    }
    if let Some(terrain) = world.spatial.terrain.as_mut() {
        terrain.restrictions[0] = Restriction::new(vec![0.0]);
    }
    inside_tc(&mut world, PlayerId::new(1), 1000, VILLAGER);
    world
}

/// `FUN_1411e7220` where no spot beside the centre (radius 2 at (10, 10)) is free: the search's
/// inner ring keeps its first point, (8.5, 11.5), whose tile (8, 11) holds no object, and the
/// villager comes out there, inside the centre's footprint.
#[test]
fn walled_in_unit_comes_out_inside_the_footprint() {
    let mut world = walled_in_world(2.0);
    assert_eq!(
        world.let_out(
            PlayerId::new(1),
            ListIndex::new(0),
            object_id(1000),
            Point::new(NO_POINT, NO_POINT),
            (None, false)
        ),
        Ok(LetOut::Out)
    );
    let unit = world.object(object_id(1000)).unwrap().1;
    assert_eq!(
        (unit.life.inside, unit.position().ground()),
        (None, Point::new(8.5, 11.5))
    );
}

/// `FUN_1411e7220` on a map whose terrain the villager may stand on (0) covers only the
/// centre's footprint (its tiles 8 to 11) but its corner tile (8, 11): the inner ring's points
/// pass over the centre (`FUN_1411d0b20`'s `[0x58]`), so its second point, (9.5, 11.5), is
/// free; were the centre counted, the first point (8.5, 11.5) would be kept.
#[test]
fn walled_in_unit_takes_the_first_free_point_inside_the_footprint() {
    let mut world = let_out_world();
    let size = GridSize::new(20, 20);
    let tiles = (0..20)
        .flat_map(|y| (0..20).map(move |x| (x, y)))
        .map(|(x, y)| {
            let open = (8..12).contains(&x) && (8..12).contains(&y) && (x, y) != (8, 11);
            TileTerrain::of_save(u8::from(!open), 255, -1)
        })
        .collect();
    let kind = TerrainKind {
        shown: true,
        flags: TerrainFlags::NONE,
        forest: false,
    };
    let mut terrain = Terrain::new(
        size,
        tiles,
        vec![kind; 2],
        vec![Restriction::new(vec![1.0, 0.0])],
    );
    terrain.set_elevations(vec![0; 400]);
    world.spatial.terrain = Some(terrain);
    inside_tc(&mut world, PlayerId::new(1), 1000, VILLAGER);
    assert_eq!(
        world.let_out(
            PlayerId::new(1),
            ListIndex::new(0),
            object_id(1000),
            Point::new(NO_POINT, NO_POINT),
            (None, false)
        ),
        Ok(LetOut::Out)
    );
    let unit = world.object(object_id(1000)).unwrap().1;
    assert_eq!(unit.position().ground(), Point::new(9.5, 11.5));
}

/// `FUN_1411e7220` toward no point: the first spot (the top side's left part, at (7.5, 12.5))
/// meets a villager 1002 (which moves off) first in its tile's list and a building 101 (which
/// stays) after it. The villager's first candidate reports the villager and tries the next one
/// up, (7.5, 13.0). A mule cart's spot test passes over the units in the slot's own scan
/// (`FUN_1411c5b60`'s `[0x60]`): the building reports, the spot ends, and the mule cart takes
/// the next spot clear of both, (8.5, 12.5).
#[test]
fn mule_cart_spot_ends_at_building_behind_unit() {
    let let_out_from = |master: fn(&World) -> MasterId| {
        let mut world = let_out_world();
        let mule_cart = world.model.slots.master.mule_cart;
        for masters in &mut world.model.masters {
            let villager = masters.get(VILLAGER).cloned().unwrap();
            *masters = Arc::new(Masters::new(masters.iter().cloned().chain([Master {
                id: mule_cart,
                ..villager
            }])));
        }
        let made = master(&world);
        inside_tc(&mut world, PlayerId::new(1), 1000, made);
        let tc = *world.spatial.get(object_id(100)).unwrap();
        let mut walker = unit(1002, VILLAGER, HitPoints::new(25.0));
        walker.place_for_test(Position::new(7.5, 12.4, 0.0));
        let _ = world.push_live(PlayerId::new(1), walker).unwrap();
        world.spatial.insert(
            object_id(1002),
            Placed {
                position: Position::new(7.5, 12.4, 0.0),
                radius: Footprint::new(0.35, 0.35),
                class: class::VILLAGER,
                master: VILLAGER,
                obstruction_kind: ObstructionKind::NONE,
                obstruction: None,
                ..tc
            },
        );
        let mut building = unit(101, TC, HitPoints::new(2400.0));
        building.place_for_test(Position::new(7.9, 12.3, 0.0));
        let _ = world.push_live(PlayerId::new(1), building).unwrap();
        world.spatial.insert(
            object_id(101),
            Placed {
                position: Position::new(7.9, 12.3, 0.0),
                radius: Footprint::new(0.3, 0.3),
                ..tc
            },
        );
        assert_eq!(
            world.let_out(
                PlayerId::new(1),
                ListIndex::new(0),
                object_id(1000),
                Point::new(NO_POINT, NO_POINT),
                (None, false)
            ),
            Ok(LetOut::Out)
        );
        world.object(object_id(1000)).unwrap().1.position().ground()
    };
    assert_eq!(let_out_from(|_| VILLAGER), Point::new(7.5, 13.0));
    assert_eq!(
        let_out_from(|world| world.model.slots.master.mule_cart),
        Point::new(8.5, 12.5)
    );
}

/// `FUN_1411e7220` where the search finds nothing (a centre narrower than the inner ring's
/// margin: no point inside it): the game's 0 before anything changes, the unit with units
/// inside too (its refusal comes with slot 0x1a8, after the spot).
#[test]
fn unit_without_spot_stays_inside() {
    let mut world = walled_in_world(0.4);
    world.players[1]
        .objects
        .iter_mut()
        .find(|unit| unit.id == object_id(1000))
        .unwrap()
        .garrison
        .push(object_id(1001));
    assert_eq!(
        world.let_out(
            PlayerId::new(1),
            ListIndex::new(0),
            object_id(1000),
            Point::new(NO_POINT, NO_POINT),
            (None, false)
        ),
        Ok(LetOut::Stayed)
    );
    let unit = world.object(object_id(1000)).unwrap().1;
    assert_eq!(unit.life.inside, Some(ContainerId::new(object_id(100))));
    assert_eq!(
        world.players[1].objects[ListIndex::new(0)].garrison,
        [object_id(1000)]
    );
}

/// `FUN_1411e7220` on a unit that is not inside (an effect's new unit the building could not
/// take in, `FUN_1416136f0`): slot 0x1a8 does nothing (the building's list and volleys
/// unchanged), slot 0x58 moves it from where it stands to a spot beside the building.
#[test]
fn unit_on_the_map_let_out_moves_beside_the_building() {
    let mut world = let_out_world();
    let tc_at = world.players[1].objects[ListIndex::new(0)].position();
    walker(&mut world, PlayerId::new(1), 1000, VILLAGER, tc_at.ground());
    let firepower = world
        .parts
        .combat_of(world.players[1].objects[ListIndex::new(0)].id)
        .firepower;
    assert_eq!(
        world.let_out(
            PlayerId::new(1),
            ListIndex::new(0),
            object_id(1000),
            Point::new(NO_POINT, NO_POINT),
            (None, false)
        ),
        Ok(LetOut::Out)
    );
    let unit = world.object(object_id(1000)).unwrap().1;
    assert_ne!(unit.position().ground(), tc_at.ground());
    assert_eq!(
        world.spatial.get(object_id(1000)).unwrap().position,
        unit.position()
    );
    let tc = &world.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        (tc.garrison.len(), world.parts.combat_of(tc.id).firepower),
        (0, firepower)
    );
}

/// `FUN_1411e7220` writes the unit's x, y and z (`FUN_14119d610`) before its slot 0x58 reads
/// them. A unit on the map (an effect's new unit) still leaves its old tile's list by the tile
/// it holds (`FUN_1411a58b0`), but every read of its place reads the spot: its old tile keeps
/// its tall count (`FUN_140f5f560` writes the old list's count at the spot's tile, whose list
/// counts again), its sight leaves from the spot before it comes there (slots 0x1d8 and 0x1d0
/// at the spot: the tiles where it stood stay seen), and its registry records stay where it
/// stood (`FUN_14108c320` sees no move).
#[test]
fn unit_on_the_map_let_out_leaves_its_counts_where_it_stood() {
    let mut world = let_out_world();
    let tc_at = world.players[1].objects[ListIndex::new(0)].position();
    walker(&mut world, PlayerId::new(1), 1000, VILLAGER, tc_at.ground());
    let mut registry = Registry::new(4, 65);
    registry
        .update_unit(UnitUpdate {
            id: object_id(1000),
            owner: PlayerId::new(1),
            position: tc_at.ground(),
            radii: Footprint::new(0.2, 0.2),
            class: class::VILLAGER,
            old: VisionBits::NONE,
            new: VisionBits::visible(PlayerId::new(2)),
        })
        .unwrap();
    let visibility = world.spatial.sight.as_mut().unwrap();
    visibility.registry = Some(registry);
    visibility.add_seer(
        object_id(1000),
        Seer {
            player: PlayerId::new(1),
            master: VILLAGER,
            los: Distance::new(1.0),
            los_powerup: Distance::ZERO,
            state: ObjectState::Alive,
            explored: true,
            building: None,
            umv: 0,
            capture: CaptureKind::None,
            kind: MasterKind::COMBAT,
        },
    );
    let _ = visibility
        .sight_on(
            object_id(1000),
            tc_at,
            Footprint::new(0.2, 0.2),
            SightShape::Circle,
            SightCentre::Tile,
            SightLayer {
                roles: &roles(),
                visibility_tuning: tuning().visibility,
            },
        )
        .unwrap();
    let centre = Tile::new(10, 10);
    let tall_before = world.spatial.tiles().tall(centre);
    assert_eq!(tall_before, Some(2));
    assert_eq!(
        world.let_out(
            PlayerId::new(1),
            ListIndex::new(0),
            object_id(1000),
            Point::new(NO_POINT, NO_POINT),
            (None, false)
        ),
        Ok(LetOut::Out)
    );
    let spot = world.object(object_id(1000)).unwrap().1.position();
    let spot_tile = Tile::new(spot.x() as i32, spot.y() as i32);
    assert_ne!(spot_tile, centre);
    assert_eq!(world.spatial.tile_list(centre), [object_id(100)]);
    assert_eq!(world.spatial.tile_list(spot_tile), [object_id(1000)]);
    assert_eq!(
        (
            world.spatial.tiles().tall(centre),
            world.spatial.tiles().tall(spot_tile)
        ),
        (tall_before, Some(1))
    );
    let visibility = world.spatial.sight.as_ref().unwrap();
    let count = |tile: Tile| visibility.maps[1].counts[(tile.y() * 20 + tile.x()) as usize];
    assert_eq!((count(centre), count(spot_tile)), (1, 1));
    let records = &visibility.registry.as_ref().unwrap().lists[2][class::VILLAGER.index().unwrap()];
    assert_eq!(
        records
            .iter()
            .map(|record| (record.id, record.at))
            .collect::<Vec<_>>(),
        [(object_id(1000), tc_at.ground())]
    );
}

#[test]
fn buildings_count_takes_relics_in_and_leaves_raiders_out() {
    // FUN_1411d0ce0 = FUN_141213500 (the +0x2f8 count and +0x9a8) less +0xc2c, which counts
    // the units of type 70 that came in with +0x9a0 set (FUN_1411cb850)
    let mut world = garrison_world();
    let mut masters = garrison_masters();
    masters = Masters::new(masters.iter().cloned().chain([Master {
        id: ids::RELIC,
        kind: MasterKind::ANIMATED,
        class: class::ARTIFACT,
        ..Master::default()
    }]));
    world.model.masters = (0..4).map(|_| masters.clone()).map(Arc::new).collect();
    for (player, id, master_id) in [
        (0, 900, master_id(285)),
        (1, 1000, VILLAGER),
        (2, 2000, VILLAGER),
    ] {
        let mut unit = unit(id, master_id, HitPoints::new(25.0));
        unit.life.inside = Some(ContainerId::new(object_id(100)));
        let _ = world.push_live(PlayerId::new(player), unit).unwrap();
    }
    assert_eq!(world.garrisoned(object_id(100)), InsideCount::new(3));
    // a raider inside a building: the building's +0xc2c, not the unit's countdown, leaves it out
    world
        .parts
        .combat_mut(object_id(1000))
        .unwrap()
        .raider_countdown = RaiderCountdown::of_data(20);
    assert_eq!(world.garrisoned(object_id(100)), InsideCount::new(3));
    world
        .parts
        .building_mut(object_id(100))
        .unwrap()
        .garrison_counts
        .raiders = InsideCount::ONE;
    assert_eq!(world.garrisoned(object_id(100)), InsideCount::new(2));
    // a transport has no +0xc2c term
    let mut unit = unit(3000, VILLAGER, HitPoints::new(25.0));
    unit.life.inside = Some(ContainerId::new(object_id(200)));
    let _ = world.push_live(PlayerId::new(1), unit).unwrap();
    world
        .parts
        .combat_mut(object_id(3000))
        .unwrap()
        .raider_countdown = RaiderCountdown::of_data(20);
    assert_eq!(world.garrisoned(object_id(200)), InsideCount::ONE);
}

/// [`coast_world`] with its ship 200 at `ship`, the mesh built, after UNLOAD to `to`; `set` then
/// changes the ship's Transport, and one update (dt 100) runs.
fn transport_ticked(
    ship: Point,
    to: Point,
    set: impl FnOnce(&mut transport::TransportAction),
) -> World {
    let mut world = coast_world(ship);
    world.build_mesh().unwrap();
    UnloadCommand {
        player: player_id(1),
        units: unit_list(object_ids([200])),
        point: to,
        pick: UnloadPick::of_packet(0, -1),
        sound: false,
    }
    .run(&mut world)
    .unwrap();
    set(world
        .parts
        .actions
        .of_mut(object_id(200))
        .unwrap()
        .list
        .transport_mut()
        .unwrap());
    world.tick(Millis::new(100));
    assert_eq!(world.players[1].unsupported, None);
    world
}

/// The unload is the Transport's own call (`FUN_140aa3ba0` in its state 6): the villager is
/// ashore and the Transport idles while the ship's update still runs, before its unit AI part.
#[test]
fn transport_unloads_inside_its_own_update() {
    let mut world = coast_world(Point::new(14.5, 10.5));
    world.build_mesh().unwrap();
    UnloadCommand {
        player: player_id(1),
        units: unit_list(object_ids([200])),
        point: Point::new(15.5, 10.5),
        pick: UnloadPick::of_packet(0, -1),
        sound: false,
    }
    .run(&mut world)
    .unwrap();
    let list_index = world.players[1]
        .objects
        .position_of(object_id(200))
        .unwrap();
    let seen = in_update(
        &mut world,
        (PlayerId::new(1), list_index),
        100,
        |unit, actions, step| {
            let me = Me {
                human_order: WorldTime::ZERO,
                id: unit.id,
                group: unit.group.id,
                action_state: actions
                    .top_action()
                    .map_or(ActionState::NO_ACTION, |top_action| top_action.state),
                top_speed: TopSpeed::Own,
            };
            let transport = *actions.list.transport().unwrap();
            let _ = unit.transport_update(transport, actions, step, me, &roles())?;
            let inside = step
                .world
                .object(object_id(1000))
                .map(|(_, unit)| unit.life.inside);
            Ok((
                inside,
                actions
                    .list
                    .transport()
                    .map(|transport_action| transport_action.state),
            ))
        },
    );
    assert_eq!(seen, Ok((Some(None), Some(TransportState::Idle))));
}

/// The Transport's update in the ship's (`transport_update`): beside the shore (state 6, the
/// land within 2 of its tile) the update unloads: the villager goes ashore and the Transport
/// idles. A failed one (state 13) tells
/// the unit AI (0x1f9 with the transport action value), idles and leaves the list; a done one
/// (state 1) leaves it quietly.
#[test]
fn transport_update_unloads_beside_the_shore_and_ends_when_done_or_failed() {
    let world = transport_ticked(Point::new(14.5, 10.5), Point::new(15.5, 10.5), |_| ());
    let ship = world.object(object_id(200)).unwrap().1;
    assert_eq!(
        world
            .parts
            .actions_of(object_id(200))
            .list
            .transport()
            .map(|transport_action| transport_action.state.bare()),
        Some(TransportState::Idle)
    );
    assert_eq!(ship.garrison, Vec::<ObjectId>::new());
    assert_eq!(world.object(object_id(1000)).unwrap().1.life.inside, None);
    let failed = |tr: &mut transport::TransportAction| tr.state = TransportState::Failed;
    let world = transport_ticked(Point::new(10.5, 10.5), Point::new(17.5, 10.5), failed);
    let ship = world.object(object_id(200)).unwrap().1;
    assert_eq!(
        world
            .parts
            .actions_of(object_id(200))
            .list
            .transport()
            .copied(),
        None
    );
    let heard: Vec<_> = world
        .parts
        .unit_ai
        .get(ship.id)
        .unwrap()
        .notifies
        .iter()
        .map(|notify| (notify.kind, notify.params))
        .collect();
    assert_eq!(
        heard,
        [(
            NotifyKind::ACTION_FAILED,
            [AiAction::Transport.value(), 0, 0]
        )]
    );
    // off the shore its MoveTo starts: the walk the unit AI asked for moves on
    let world = transport_ticked(Point::new(10.5, 10.5), Point::new(17.5, 10.5), |_| ());
    let tr = world
        .parts
        .actions_of(object_id(200))
        .list
        .transport()
        .copied()
        .unwrap();
    assert_eq!(
        (
            tr.state.bare(),
            tr.state
                .move_to()
                .copied()
                .map(|move_to_action| move_to_action.state)
        ),
        (
            TransportState::SailToUnload { move_to: None },
            Some(MoveTo::Moving)
        )
    );
    // an unload far from the point's land cannot unload: the owner hears it (a human's
    // interface only) and the Transport idles with the villager inside
    let far = |tr: &mut transport::TransportAction| {
        (tr.state, tr.retarget) = (TransportState::Unload, false);
        tr.point = Position::new(17.5, 10.5, 0.0);
    };
    let world = transport_ticked(Point::new(10.5, 10.5), Point::new(17.5, 10.5), far);
    let tr = world
        .parts
        .actions_of(object_id(200))
        .list
        .transport()
        .copied()
        .unwrap();
    assert_eq!(tr.state.bare(), TransportState::Idle);
    assert_eq!(
        world.object(object_id(1000)).unwrap().1.life.inside,
        Some(container_id(200))
    );
    // a computer player's AI would hear it, which is not modelled
    let mut coast_world = coast_world(Point::new(10.5, 10.5));
    coast_world.build_mesh().unwrap();
    coast_world.players[1].kind = PlayerKind::Computer;
    UnloadCommand {
        player: player_id(1),
        units: unit_list(object_ids([200])),
        point: Point::new(17.5, 10.5),
        pick: UnloadPick::of_packet(0, -1),
        sound: false,
    }
    .run(&mut coast_world)
    .unwrap();
    far(coast_world
        .parts
        .actions
        .of_mut(object_id(200))
        .unwrap()
        .list
        .transport_mut()
        .unwrap());
    coast_world.tick(Millis::new(100));
    let stop = coast_world.players[1]
        .unsupported
        .as_ref()
        .map(|(_, _, error)| error.reason());
    assert_eq!(
        stop,
        Some("a computer player's AI hearing its unit's alert (slot 0x2e8, then slot 0x3c8)")
    );
    // the list holds the Transport as the update left it before the error (idle), which the
    // stop's context reads
    let tr = coast_world
        .parts
        .actions_of(object_id(200))
        .list
        .transport()
        .copied();
    assert_eq!(tr.map(|tr| tr.state.bare()), Some(TransportState::Idle));
    let done = |tr: &mut transport::TransportAction| tr.state = TransportState::Done;
    let world = transport_ticked(Point::new(10.5, 10.5), Point::new(17.5, 10.5), done);
    let ship = world.object(object_id(200)).unwrap().1;
    assert_eq!(
        world
            .parts
            .actions_of(object_id(200))
            .list
            .transport()
            .copied(),
        None
    );
    assert_eq!(world.parts.unit_ai.get(ship.id).unwrap().notifies, []);
    assert_eq!(
        world.object(object_id(1000)).unwrap().1.life.inside,
        Some(container_id(200))
    );
}

/// [`let_out_world`] with villager 1000 (its unit AI on target 77, stance 1) inside town centre
/// 100 and a second centre 101 of player 1 at (16, 4), which the villager's garrison task
/// takes; `setup` changes the world before the villager is let out toward `point` with
/// `work_on` ([`World::let_out`]).
fn let_out_beside_second_centre(
    setup: impl FnOnce(&mut World),
    point: Point,
    work_on: Option<ObjectId>,
) -> (GenieResult<LetOut>, World) {
    let mut world = let_out_world();
    inside_tc(&mut world, PlayerId::new(1), 1000, VILLAGER);
    world.parts.set_unit_ai(
        object_id(1000),
        Some(UnitAi {
            current_action: Some(AiAction::Gather),
            current_target: Some(object_id(77)),
            stance: UnitStance::Defensive,
            ..UnitAi::default()
        }),
    );
    let mut second = unit(101, TC, HitPoints::new(2400.0));
    second.place_for_test(Position::new(16.0, 4.0, 0.0));
    let _ = world.push_live(PlayerId::new(1), second).unwrap();
    let placed = *world.spatial.get(object_id(100)).unwrap();
    world.spatial.insert(
        object_id(101),
        Placed {
            position: Position::new(16.0, 4.0, 0.0),
            ..placed
        },
    );
    setup(&mut world);
    let let_out = world.let_out(
        PlayerId::new(1),
        ListIndex::new(0),
        object_id(1000),
        point,
        (work_on, false),
    );
    (let_out, world)
}

/// The end of a let-out with an object to work on ([`World::came_out`]): `GO_BACK_TO_WORK`'s
/// object (the second town centre) gets the unit's work command at the point given (slot
/// 0x128, `+0xa14` 0: the unit AI keeps its target). Without the garrison filter (3) the
/// villager's tasks give no action on a centre.
#[test]
fn unit_let_out_works_its_record_at_the_point_given() {
    let (let_out, game) =
        let_out_beside_second_centre(|_| (), Point::new(4.5, 4.5), Some(object_id(101)));
    assert_eq!(let_out, Ok(LetOut::Out));
    let object = game.object(object_id(1000)).unwrap().1;
    assert_eq!(object.life.inside, None);
    assert_eq!(
        game.parts
            .unit_ai
            .get(object.id)
            .map(|unit_ai| unit_ai.current_target),
        Some(Some(object_id(77)))
    );
    assert_eq!(game.parts.actions_of(object_id(1000)).top_action(), None);
}

/// Town centre 100's gather point on `target` at (16, 4), of state 1.
fn gather_on(target: i32, target_master: MasterId) -> impl FnOnce(&mut World) {
    move |world: &mut World| {
        let id = world.players[1].objects[ListIndex::new(0)].id;
        world
            .set_part(
                id,
                Part::GatherPoint(Some(GatherPoint {
                    state: GatherPointState::Point,
                    target: Some(object_id(target)),
                    point: Position::new(16.0, 4.0, 0.0),
                    target_master: Some(target_master),
                })),
            )
            .unwrap();
    }
}

/// `FUN_1411e7220` with a gather point on the second centre (a building, type 80) and no point
/// given: the unit AI forgets its target 77 (`FUN_140c5e1e0`), the unit waits to move (slot
/// 0x2a0 with 1), and its master's task finder (`+0x788`'s slot 0x10 with filter 3) finds its
/// garrison task on the centre, so slot 0x7e8 with filter 3 works it outside slot 0x128: the
/// villager enters the centre, its work's notify (`+0xa14` 1) tells the unit AI its target, and
/// no command is counted (slot 0x128's `FUN_14106c590`).
#[test]
fn gather_point_on_building_sends_let_out_unit_into_it() {
    let building = |world: &mut World| {
        moving_villager(world);
        gather_on(101, TC)(world);
    };
    let (let_out, world) =
        let_out_beside_second_centre(building, Point::new(NO_POINT, NO_POINT), None);
    assert_eq!(let_out, Ok(LetOut::Out));
    let unit = world.object(object_id(1000)).unwrap().1;
    assert_eq!(unit.life.inside, None);
    assert_eq!(
        world
            .parts
            .unit_ai
            .get(unit.id)
            .map(|unit_ai| unit_ai.current_target),
        Some(Some(object_id(101)))
    );
    assert_eq!(
        world.parts.motion.mover(unit.id).map(|mover| mover.waiting),
        Some(1)
    );
    let actions = world.parts.actions_of(object_id(1000));
    assert_eq!(
        (
            actions.list.enter().map(|enter_action| enter_action.target),
            actions.commands
        ),
        (Some(Some(container_id(101))), CommandCount::ZERO)
    );
}

/// The villager's master of every player moves (speed 1), and villager 1000 with it.
fn moving_villager(world: &mut World) {
    let mut walker = tests_walker();
    walker.stats = standing();
    world.parts.put_mover(object_id(1000), Some(walker));
    for masters in &mut world.model.masters {
        if let Some(villager) = Arc::make_mut(masters).get_mut(VILLAGER) {
            villager.animated = Some(AnimatedPart {
                max_speed: Speed::new(1.0),
            });
        }
    }
}

/// `FUN_1411e7220` with a gather point on a transport ship (type 70), on which the villager has
/// a garrison task: the task finder runs only for a target of type 80, so slot 0x128 works the
/// ship with any task (the finder lets a transport through to its garrison task): the villager
/// enters the ship, and slot 0x128 counts its command (`FUN_14106c590`), which the finder's slot
/// 0x7e8 would not.
#[test]
fn gather_point_on_unit_skips_the_task_finder() {
    let ship = |world: &mut World| {
        moving_villager(world);
        let mut ship = unit(102, SHIP, HitPoints::new(100.0));
        ship.place_for_test(Position::new(16.0, 4.0, 0.0));
        let _ = world.push_live(PlayerId::new(1), ship).unwrap();
        let placed = *world.spatial.get(object_id(100)).unwrap();
        world.spatial.insert(
            object_id(102),
            Placed {
                position: Position::new(16.0, 4.0, 0.0),
                radius: Footprint::new(0.5, 0.5),
                class: class::TRANSPORT,
                master: SHIP,
                obstruction: None,
                ..placed
            },
        );
        gather_on(102, SHIP)(world);
    };
    let (let_out, world) = let_out_beside_second_centre(ship, Point::new(NO_POINT, NO_POINT), None);
    assert_eq!(let_out, Ok(LetOut::Out));
    assert_eq!(world.object(object_id(1000)).unwrap().1.life.inside, None);
    let actions = world.parts.actions_of(object_id(1000));
    assert_eq!(
        (
            actions.list.enter().map(|enter_action| enter_action.target),
            actions.commands
        ),
        (Some(Some(container_id(102))), CommandCount::of_data(1))
    );
}

/// `FUN_1411e7220` with a gather point on a gate (a building, type 80, with no room): its
/// master's task finder (filter 3) finds no garrison task there (`0x1411e75ce`'s `JZ`), so slot
/// 0x128 works the gate at the gather point with any task (`+0xa14` 0) and counts the command
/// (`FUN_14106c590`), where the finder's slot 0x7e8 would neither count nor move. The villager
/// has no task on a gate: its work command tells its unit AI of a move to the gate (slot 0x828)
/// and moves there (slot 0x138, a `MoveTo` that counts the command again), after the unit waits
/// to move (slot 0x2a0 with 1).
#[test]
fn gather_point_on_building_without_garrison_task_works_it() {
    let gate = |world: &mut World| {
        moving_villager(world);
        let mut gate = unit(103, GATE, HitPoints::new(2750.0));
        gate.place_for_test(Position::new(16.0, 4.0, 0.0));
        let _ = world.push_live(PlayerId::new(1), gate).unwrap();
        let placed = *world.spatial.get(object_id(100)).unwrap();
        world.spatial.insert(
            object_id(103),
            Placed {
                position: Position::new(16.0, 4.0, 0.0),
                radius: Footprint::new(0.5, 0.5),
                class: class::GATE,
                master: GATE,
                obstruction: None,
                ..placed
            },
        );
        gather_on(103, GATE)(world);
    };
    let (let_out, world) = let_out_beside_second_centre(gate, Point::new(NO_POINT, NO_POINT), None);
    assert_eq!(let_out, Ok(LetOut::Out));
    let unit = world.object(object_id(1000)).unwrap().1;
    assert_eq!(unit.life.inside, None);
    assert_eq!(
        world
            .parts
            .unit_ai
            .get(unit.id)
            .map(|unit_ai| (unit_ai.current_order, unit_ai.current_target)),
        Some((Some(OrderKind::Move), Some(object_id(103))))
    );
    assert_eq!(
        world.parts.motion.mover(unit.id).map(|mover| mover.waiting),
        Some(1)
    );
    let actions = world.parts.actions_of(object_id(1000));
    assert_eq!(
        (
            actions
                .list
                .move_to()
                .map(|move_to| (move_to.point, move_to.target)),
            actions.list.enter().copied(),
            actions.commands
        ),
        (
            Some((Position::new(16.0, 4.0, 0.0), Some(object_id(103)))),
            None,
            CommandCount::of_data(2)
        )
    );
}

/// `FUN_1411e7220` with a gather point whose target the world no longer holds: the game looks
/// for no other (`FUN_1415417a0` finds none), so the unit moves to the point, though Gaia's
/// centre 300 of the target's master stands there, which a new unit's walk would take
/// (`FUN_140ace9c0`'s search, [`World::lost_gather_target`]).
#[test]
fn gather_point_with_lost_target_moves_let_out_unit_to_it() {
    let lost = |world: &mut World| {
        moving_villager(world);
        let mut centre = unit(300, TC, HitPoints::new(2400.0));
        centre.place_for_test(Position::new(16.0, 4.0, 0.0));
        let _ = world.push_live(PlayerId::GAIA, centre).unwrap();
        gather_on(555, TC)(world);
    };
    let (let_out, world) = let_out_beside_second_centre(lost, Point::new(NO_POINT, NO_POINT), None);
    assert_eq!(let_out, Ok(LetOut::Out));
    let actions = world.parts.actions_of(object_id(1000));
    assert_eq!(
        actions
            .list
            .move_to()
            .map(|move_to| (move_to.point, move_to.target)),
        Some((Position::new(16.0, 4.0, 0.0), None))
    );
}

/// A gather point of state 0xff (SET GATHER STATE's) replaces the point a unit is let out toward
/// (`FUN_1411e7220`'s `+0xc30 == 0xff`) and sends it no target; one of state 1 sends a unit
/// without a point to its target, one of state 0 or none leaves the point.
#[test]
fn gather_point_of_state_0xff_replaces_the_point_of_let_out() {
    use crate::garrison::let_out::{LetOutAim, let_out_aim};
    let at = |state, target| {
        Some(GatherPoint {
            state,
            target,
            point: Position::new(16.0, 4.0, 0.0),
            target_master: Some(TC),
        })
    };
    let given = Point::new(4.5, 4.5);
    let none = Point::new(NO_POINT, NO_POINT);
    let kept = at(GatherPointState::Kept, Some(object_id(101)));
    assert_eq!(
        let_out_aim(kept, given, object_id(100)),
        LetOutAim::Point(Point::new(16.0, 4.0))
    );
    assert_eq!(
        let_out_aim(kept, none, object_id(100)),
        LetOutAim::Point(Point::new(16.0, 4.0))
    );
    let one = at(GatherPointState::Point, Some(object_id(101)));
    assert_eq!(
        let_out_aim(one, none, object_id(100)),
        LetOutAim::Target(one.unwrap())
    );
    assert_eq!(
        let_out_aim(one, given, object_id(100)),
        LetOutAim::Point(given)
    );
    // on the building itself, or without a gather point, the point stays
    assert_eq!(
        let_out_aim(
            at(GatherPointState::Point, Some(object_id(100))),
            none,
            object_id(100)
        ),
        LetOutAim::Point(none)
    );
    assert_eq!(
        let_out_aim(None, given, object_id(100)),
        LetOutAim::Point(given)
    );
}

/// Slot 0x120 of a building told its player's new relation ([`World::notify_of_relation`]):
/// player 1's town centre 100 holding player 2's villager 2000 keeps it while they stay
/// allies (relation 0) and lets it out toward no point once they are enemies (3); a
/// relation toward a player with no unit inside lets nobody out. Told from the villager's
/// side (its player's relation toward the building's owner), it leaves the same way.
#[test]
fn building_lets_out_the_units_of_player_no_longer_its_ally() {
    let held = |world: &World| {
        (
            world.players[2].objects[ListIndex::new(0)].life.inside,
            world.players[1].objects[ListIndex::new(0)].garrison.clone(),
        )
    };
    let mut world = let_out_world();
    inside_tc(&mut world, PlayerId::new(2), 2000, VILLAGER);
    world
        .notify_of_relation(
            PlayerId::new(1),
            object_id(100),
            PlayerId::new(2),
            Relation::Ally,
        )
        .unwrap();
    world
        .notify_of_relation(
            PlayerId::new(1),
            object_id(100),
            PlayerId::new(3),
            Relation::Enemy,
        )
        .unwrap();
    assert_eq!(
        held(&world),
        (Some(container_id(100)), vec![object_id(2000)])
    );
    world
        .notify_of_relation(
            PlayerId::new(1),
            object_id(100),
            PlayerId::new(2),
            Relation::Enemy,
        )
        .unwrap();
    assert_eq!(held(&world), (None, vec![]));
    let mut world = let_out_world();
    inside_tc(&mut world, PlayerId::new(2), 2000, VILLAGER);
    world
        .notify_of_relation(
            PlayerId::new(2),
            object_id(2000),
            PlayerId::new(1),
            Relation::Ally,
        )
        .unwrap();
    assert_eq!(held(&world).0, Some(container_id(100)));
    world
        .notify_of_relation(
            PlayerId::new(2),
            object_id(2000),
            PlayerId::new(1),
            Relation::Enemy,
        )
        .unwrap();
    assert_eq!(held(&world), (None, vec![]));
}

/// A siege tower (class 13) unloads on a new relation, not modelled: the unit inside one of
/// the other player's, and the tower holding the other player's units, both stop there.
#[test]
fn siege_tower_unload_on_new_relation_is_not_modelled() {
    let mut world = let_out_world();
    for masters in &mut world.model.masters {
        if let Some(ship) = Arc::make_mut(masters).get_mut(SHIP) {
            ship.class = class::SIEGE;
        }
    }
    inside_tc(&mut world, PlayerId::new(2), 2000, VILLAGER);
    world.players[2].objects[ListIndex::new(0)].life.inside =
        Some(ContainerId::new(object_id(200)));
    world.players[1].objects[ListIndex::new(0)].garrison.clear();
    world.players[1].objects[ListIndex::new(1)]
        .garrison
        .push(object_id(2000));
    let unload = Err(SIEGE_UNLOAD);
    assert_eq!(
        world.notify_of_relation(
            PlayerId::new(2),
            object_id(2000),
            PlayerId::new(1),
            Relation::Enemy
        ),
        unload
    );
    assert_eq!(
        world.notify_of_relation(
            PlayerId::new(1),
            object_id(200),
            PlayerId::new(2),
            Relation::Enemy
        ),
        unload
    );
    assert_eq!(
        world.notify_of_relation(
            PlayerId::new(1),
            object_id(200),
            PlayerId::new(2),
            Relation::Ally
        ),
        Ok(())
    );
}

/// A human player's market (84), now an ally of a computer player, tells it of its tile (the
/// computer's tile list); a computer's market, or a human ally, hears nothing.
#[test]
fn market_tells_computer_ally_of_its_tile() {
    let market = |ally: PlayerKind, own: PlayerKind| {
        let mut world = let_out_world();
        for masters in &mut world.model.masters {
            let mut market = masters.get(TC).unwrap().clone();
            market.id = ids::MARKET;
            *masters = Arc::new(Masters::new(masters.iter().cloned().chain([market])));
        }
        world.players[1].objects[ListIndex::new(0)].life.master_id = ids::MARKET;
        world.players[1].objects[ListIndex::new(0)].place_for_test(Position::new(10.5, 12.5, 0.0));
        world.players[2].kind = ally;
        world.players[1].kind = own;
        let mut visibility = Visibility::new(
            GridSize::new(20, 20),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            0,
            Vec::new(),
        );
        visibility.new_tiles = vec![Vec::new(); 4];
        world.spatial.sight = Some(visibility);
        world
            .notify_of_relation(
                PlayerId::new(1),
                object_id(100),
                PlayerId::new(2),
                Relation::Ally,
            )
            .unwrap();
        world.spatial.sight.unwrap().new_tiles[2].clone()
    };
    let (human, computer) = (PlayerKind::Human, PlayerKind::Computer);
    assert_eq!(market(computer, human), [Tile::new(10, 12)]);
    assert_eq!(market(human, human), Vec::<Tile>::new());
    assert_eq!(market(computer, computer), Vec::<Tile>::new());
}

#[test]
fn the_sub_action_list_update_of_an_empty_list_is_1_and_of_an_arrived_move_to_too() {
    // FUN_140a992b0: "There is no list." answers 1; an arrived MoveTo's update ends with 1
    let mut world = entering(
        Point::new(5.5, 5.5),
        Point::new(5.5, 30.5),
        ship_enter(Position::new(5.5, 30.5, 0.0), MoveTo::Arrived),
    );
    let list_index = villager(&world);
    let mut update = |list: &mut Option<MoveToAction>| {
        in_update(
            &mut world,
            (PlayerId::new(1), list_index),
            0,
            |unit, _, step| {
                let me = Me {
                    human_order: WorldTime::ZERO,
                    id: unit.id,
                    group: unit.group.id,
                    action_state: ActionState::NO_ACTION,
                    top_speed: TopSpeed::Own,
                };
                update_sub_actions(list, step, unit, me, ListUpdate::default())
            },
        )
    };
    assert_eq!(update(&mut None), Ok(SubCode::Done));
    let mut arrived = enter_of_move(MoveTo::Arrived);
    assert_eq!(update(&mut arrived), Ok(SubCode::Done));
}

/// A quiet `MoveTo` to ship 200 in `state`.
fn enter_of_move(state: MoveTo) -> Option<MoveToAction> {
    ship_enter(Position::new(5.5, 30.5, 0.0), state)
        .state
        .move_to()
        .copied()
}

/// Object `value` as a container.
fn container_id(value: i32) -> ContainerId {
    ContainerId::new(object_id(value))
}

/// Objects `values` as containers.
fn container_ids<const N: usize>(values: [i32; N]) -> Vec<ContainerId> {
    values.into_iter().map(container_id).collect()
}
