//! The Unpack's tests on player 1's packed trebuchet 700 (masters 331 and 42) and player 2's
//! villager 800.

use super::*;
use crate::construction::Annexes;
use crate::gathering::hunt::tests::walker;
use crate::gathering::make::MakeAction;
use crate::map::GroundOffset;
use crate::map::spatial::Placed;
use crate::movement::Me;
use crate::movement::MoveStats;
use crate::movement::Mover;
use crate::movement::PathFlags;
use crate::movement::Speed;
use crate::movement::TopSpeed;
use crate::movement::WaypointLevel;
use crate::movement::move_to::{MOVE_TO, MoveTo};
use crate::objects::ActionState;
use crate::objects::FacetCount;
use crate::objects::Graphics;
use crate::objects::HitPoints;
use crate::objects::ListIndex;
use crate::objects::MasterHitPoints;
use crate::objects::ObjectState;
use crate::objects::RoleId;
use crate::objects::SpeedMultiplier;
use crate::objects::SpriteFlags;
use crate::objects::SpriteId;
use crate::objects::WorkRate;
use crate::objects::WorkerCount;
use crate::objects::actions::Action as ListAction;
use crate::objects::actions::ActionList;
use crate::objects::layers::Action;
use crate::objects::layers::Combat;
use crate::objects::layers::Static;
use crate::objects::lifecycle::Lifecycle;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ActionPart;
use crate::objects::masters::BuildableOn;
use crate::objects::masters::BuildingMaster;
use crate::objects::masters::CombatPart;
use crate::objects::masters::CreatablePart;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::Masters;
use crate::objects::masters::MovingPart;
use crate::objects::masters::ObstructionKind;
use crate::objects::masters::Task;
use crate::objects::masters::TrainLocation;
use crate::objects::sprite::{ActiveSprite, Graphic};
use crate::objects::teardown::{HeadCode, TornDown};
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::test_ids::object_id;
use crate::testing::attribute;
use crate::testing::class;
use crate::testing::ids;
use crate::unit_ai::Issuer;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::OrderKind;
use crate::unit_ai::OrderPriority;
use crate::unit_ai::OrderTargetKind;
use crate::unit_ai::PatrolPath;
use crate::unit_ai::QueuedOrder;
use crate::unit_ai::UnitAi;
use crate::world::FrameSeed;
use crate::world::Millis;
use crate::world::Player;
use crate::world::World;
use crate::world::WorldTime;
use crate::world::tests::{delete_head_at, in_update, wait_over};
use std::sync::Arc;

/// The trebuchet's id.
const TREBUCHET: ObjectId = object_id(700);

/// The trebuchet's place: player 1's first object.
const AT: (PlayerId, ListIndex) = (PlayerId::new(1), ListIndex::new(0));

/// The target's id: player 2's villager.
const TARGET: ObjectId = object_id(800);

/// The construction sprite 137 of both trebuchet masters, which `set_state(6)` shows. The
/// fixture's sprite of that id is a stand-in ([`graphic`]).
const CONSTRUCTION: SpriteId = SpriteId::named(137);

/// The unpacked trebuchet's standing sprite 1180 of the `.dat`, which the Unpack's end shows.
const STANDING: SpriteId = SpriteId::named(1180);

/// The packed trebuchet's standing sprite 2281 of the `.dat`.
const PACKED_STANDING: SpriteId = SpriteId::named(2281);

/// The packed trebuchet's walking sprite 2255 of the `.dat`.
const WALKING: SpriteId = SpriteId::named(2255);

/// The range of an Unpack made toward a target: the unpacked trebuchet's weapon range less 0.1
/// (`FUN_140ae58a0`).
const RANGE: f32 = 15.9;

/// A sprite of 10 frames, animated, of id `id`.
fn graphic(id: SpriteId) -> Graphic {
    Graphic {
        id,
        frames: Frame::of_data(10),
        facets: FacetCount::of_data(1),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::ANIMATED,
    }
}

/// The trebuchet master 42 of build 158041's `.dat`: a building of class 54, 150 hit points,
/// radii 0.5, its unpack time 50 (its train location's), its work rate 4.5, its weapon range 16,
/// its standing sprite 1180, its construction sprite 137, its transform the packed trebuchet 331;
/// it does not walk (speed 0).
fn trebuchet_master() -> Master {
    Master {
        id: ids::ID_42,
        kind: MasterKind::BUILDING,
        class: class::CLASS_54,
        hit_points: MasterHitPoints::of_data(150),
        radius: Footprint::new(0.5, 0.5),
        standing_sprite: Some(STANDING),
        action: Some(ActionPart {
            work_rate: WorkRate::new(4.5),
            ..ActionPart::default()
        }),
        combat: Some(CombatPart {
            weapon_range: Distance::new(16.0),
            ..CombatPart::default()
        }),
        creatable: Some(CreatablePart {
            train_locations: vec![TrainLocation {
                time: WholeSeconds::of_data(50),
                ..TrainLocation::default()
            }],
            ..CreatablePart::default()
        }),
        building: Some(BuildingMaster {
            construction_sprite: Some(CONSTRUCTION),
            transform_unit: Some(ids::ID_331),
            ..BuildingMaster::default()
        }),
        move_stats: Some(MoveStats {
            speed: Speed::ZERO,
            ..walker().stats
        }),
        ..Master::default()
    }
}

/// The packed trebuchet master 331 of build 158041's `.dat`: a building of class 51, its standing
/// sprite 2281, its walking sprite 2255, speed 0.8, its transform 42; else as master 42.
fn packed_master() -> Master {
    Master {
        id: ids::ID_331,
        class: class::CLASS_51,
        standing_sprite: Some(PACKED_STANDING),
        building: Some(BuildingMaster {
            construction_sprite: Some(CONSTRUCTION),
            transform_unit: Some(ids::ID_42),
            ..BuildingMaster::default()
        }),
        moving: Some(MovingPart {
            move_sprite: Some(WALKING),
            ..MovingPart::default()
        }),
        move_stats: Some(walker().stats),
        ..trebuchet_master()
    }
}

/// An Unpack toward the villager in `state`, within [`RANGE`], its point (12, 13), its timer
/// `timer`, not at work yet.
fn unpack_in(state: UnpackState, timer: f32) -> UnpackAction {
    UnpackAction {
        state,
        target: Some(TARGET),
        point: Point::new(12.0, 13.0),
        timer: Seconds::new(timer),
        unique_id: QueueId::of_data(-1),
        status: UnpackStatus::Idle,
        range: Distance::new(RANGE),
    }
}

/// An Unpack at work (state 6, status 1) on the villager, its timer `timer`.
fn working(timer: f32) -> UnpackAction {
    UnpackAction {
        status: UnpackStatus::Unpacking,
        ..unpack_in(UnpackState::Working, timer)
    }
}

/// An Unpack walking (state 11) toward the villager, its quiet `MoveTo` to it in `state`.
fn walking(state: MoveTo) -> UnpackAction {
    let move_to = MoveToAction {
        target: Some(TARGET),
        point: Position::new(20.0, 30.0, 0.5),
        ..MoveToAction::sub_in(state)
    };
    unpack_in(
        UnpackState::Moving {
            move_to: Some(move_to),
        },
        0.0,
    )
}

/// An Unpack walking (state 11) to its point (12, 13) within `range`, its quiet `MoveTo` there
/// within `range` too in `state`.
fn walking_to_point(state: MoveTo, range: f32) -> UnpackAction {
    let move_to = MoveToAction {
        point: Position::new(12.0, 13.0, 0.0),
        range: Distance::new(range),
        ..MoveToAction::sub_in(state)
    };
    UnpackAction {
        target: None,
        range: Distance::new(range),
        ..unpack_in(
            UnpackState::Moving {
                move_to: Some(move_to),
            },
            0.0,
        )
    }
}

/// The trebuchet holding `held` alone.
fn unpacking(held: UnpackAction) -> ActionList {
    ActionList::default().with_unpack(Some(held))
}

/// Player 1's trebuchet 700 (its masters 42 and 331) of master `master` at (10, 10), alive with
/// 120 hit points, showing the construction sprite at frame 9, walking ([`walker`]), its action
/// list `list`, its unit AI a default one; player 2's villager 800 (master 83, radii 0.2) at
/// (20, 30) in `state`.
fn trebuchet_world(master: MasterId, list: ActionList, state: ObjectState) -> World {
    let trebuchet = Unit {
        id: TREBUCHET,
        place: Static::at(Position::new(10.0, 10.0, 0.0)),
        life: Lifecycle {
            state: ObjectState::Alive,
            hp: HitPoints::new(120.0),
            master_id: master,
            ..Lifecycle::default()
        },
        sprite: Some(ActiveSprite::at_rest(
            graphic(CONSTRUCTION),
            Frame::of_data(9),
        )),
        ..Unit::default()
    };
    let villager = Unit {
        id: TARGET,
        place: Static::at(Position::new(20.0, 30.0, 0.5)),
        life: Lifecycle {
            state,
            master_id: ids::VILLAGER_MALE,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let owner = |objects: Vec<Unit>| Player {
        objects: objects.into(),
        attributes: vec![Amount::ZERO; 300],
        ..Player::default()
    };
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(1),
        vec![Millis::ZERO, Millis::ZERO, Millis::ZERO],
        vec![
            Player::default(),
            owner(vec![trebuchet]),
            owner(vec![villager]),
        ],
    )
    .with_actions([(
        TREBUCHET,
        Action {
            list,
            ..Action::default()
        },
    )])
    .with_ais([(TREBUCHET, UnitAi::default())]);
    world.parts.put_mover(TREBUCHET, Some(walker()));
    world.model.masters = vec![
        Arc::new(Masters::default()),
        Arc::new(Masters::new([trebuchet_master(), packed_master()])),
        Arc::new(Masters::new([Master {
            id: ids::VILLAGER_MALE,
            kind: MasterKind::COMBAT,
            class: class::VILLAGER,
            radius: Footprint::new(0.2, 0.2),
            ..Master::default()
        }])),
    ];
    world.model.graphics = Arc::new(Graphics::from(
        [CONSTRUCTION, STANDING, PACKED_STANDING, WALKING].map(|id| (id, graphic(id))),
    ));
    world
}

/// [`trebuchet_world`] of the unpacked master 42 with the villager alive: an Unpack at work.
fn world_with(list: ActionList) -> World {
    trebuchet_world(ids::ID_42, list, ObjectState::Alive)
}

/// [`trebuchet_world`] of the packed master 331 with the villager alive: an Unpack walking.
fn packed_with(list: ActionList) -> World {
    trebuchet_world(ids::ID_331, list, ObjectState::Alive)
}

/// A stand-in annex master `id` of type `kind` and 200 hit points, a building part for a
/// building. The tests take 273 for a building and 379 for an animated object: the `.dat` leaves
/// both empty in every civilisation.
fn annex_master(id: i16, kind: MasterKind) -> Master {
    Master {
        id: MasterId::named(id),
        kind,
        class: class::BUILDING,
        hit_points: MasterHitPoints::of_data(200),
        building: (kind == MasterKind::BUILDING).then(BuildingMaster::default),
        ..Master::default()
    }
}

/// A deletion of the trebuchet's actions, from the world or from its step.
type Deletion = fn(&mut World) -> GenieResult<()>;

/// The trebuchet's Unpack, if it holds one.
fn held(world: &World) -> Option<UnpackAction> {
    world.parts.actions_of(TREBUCHET).list.unpack().copied()
}

/// The trebuchet.
fn trebuchet(world: &World) -> &Unit {
    &world.players[1].objects[AT.1]
}

/// The trebuchet's sprite and its frame.
fn shown(world: &World) -> Option<(SpriteId, i16)> {
    trebuchet(world)
        .sprite
        .as_ref()
        .map(|sprite| (sprite.graphic.id, sprite.frame.get()))
}

/// The trebuchet's mover's wait to move (`+0x7f9`), which a stop clears.
fn waiting(world: &World) -> u8 {
    world
        .parts
        .mover_of(TREBUCHET)
        .map_or(0, |mover| mover.waiting)
}

/// The orders in the trebuchet's unit AI's queue.
fn queued(world: &World) -> Vec<QueuedOrder> {
    world.parts.unit_ai.get(TREBUCHET).unwrap().orders.clone()
}

/// The notifies the trebuchet's unit AI heard.
fn heard(world: &World) -> Vec<(NotifyKind, [i32; 3])> {
    world
        .parts
        .unit_ai
        .get(TREBUCHET)
        .unwrap()
        .notifies
        .iter()
        .map(|notify| (notify.kind, notify.params))
        .collect()
}

/// The trebuchet's Unpack update (slot 0x30) in an update of `dt` ms.
fn updated(world: &mut World, dt: u32) -> GenieResult<HeadCode> {
    in_update(world, AT, dt, |unit, actions, step| {
        let me = Me {
            human_order: WorldTime::ZERO,
            id: unit.id,
            group: unit.group.id,
            action_state: actions
                .top_action()
                .map_or(ActionState::NO_ACTION, |top_action| top_action.state),
            top_speed: TopSpeed::Own,
        };
        unit.unpack_part(actions, step, me)
    })
}

/// Each state and status reads back from its byte; a byte without one is an error, as is a
/// `MoveTo` of a state that does not walk.
#[test]
fn its_states_and_statuses_read_back_from_their_bytes() {
    for byte in 0..=u8::MAX {
        match UnpackState::of_byte(byte, None) {
            Ok(state) => assert_eq!(state.get(), byte),
            Err(error) => {
                assert!(!matches!(byte, 0 | 1 | 6 | 11 | 13), "{byte}");
                assert_eq!(error.reason(), "no state of an Unpack");
            }
        }
        match UnpackStatus::of_byte(byte) {
            Ok(status) => assert_eq!(status.get(), byte),
            Err(error) => {
                assert!(2 < byte, "{byte}");
                assert_eq!(error.reason(), "no status of an Unpack");
            }
        }
    }
    let walk = MoveToAction::sub_in(MoveTo::Moving);
    assert_eq!(
        UnpackState::of_byte(11, Some(walk)),
        Ok(UnpackState::Moving {
            move_to: Some(walk)
        })
    );
    assert_eq!(
        UnpackState::of_byte(11, None),
        Ok(UnpackState::Moving { move_to: None })
    );
    assert_eq!(
        UnpackState::of_byte(6, Some(walk)).unwrap_err().reason(),
        "a MoveTo of an Unpack that does not walk"
    );
    assert_eq!(UnpackState::of_byte(13, None), Ok(UnpackState::Failed));
    assert_eq!(UnpackStatus::of_byte(2), Ok(UnpackStatus::Unpacked));
    assert_eq!(UnpackState::Working.to_string(), "6");
}

/// The Unpack is type 0x7d of the list, its state and target the list's reads; its walk's
/// `MoveTo` is its sub-action, which a task change and an idle update reach.
#[test]
fn unpack_is_kind_0x7d_of_the_list() {
    let mut listed = ListAction::from(walking(MoveTo::Moving));
    assert_eq!(listed.kind(), ActionKind::UNPACK);
    assert_eq!(listed.state().get(), 11);
    assert_eq!(listed.target(), Some(TARGET));
    assert_eq!(listed.second_target(), None);
    assert!(listed.sub_action_mut().is_some());
    assert!(ListAction::from(working(3.0)).sub_action_mut().is_none());
    let mut layer = Action::default();
    layer.list.put(listed);
    let sub = layer.top_sub_action().unwrap();
    assert_eq!((sub.kind, sub.state.get()), (MOVE_TO, 4));
    assert_eq!(layer.top_action_kind(), Some(ActionKind::UNPACK));
}

/// Without a target the reach is the gaps to the point past the unit's radii (each below 0 taken
/// as 0) against the range: (2 − 0.5)² + (3 − 0.5)² = 8.5 is within 3 and out of 2.9; a gap that
/// is not a number stays one (`MAXSS`) and is out, as is a range that is not a number.
#[test]
fn reach_point_measures_the_clamped_gaps() {
    let radius = Footprint::new(0.5, 0.5);
    let at = Position::new(10.0, 10.0, 0.0);
    let reach = |range: f32, point: Point| {
        UnpackAction {
            range: Distance::new(range),
            point,
            ..working(0.0)
        }
        .reach_point(at, radius)
    };
    assert_eq!(reach(3.0, Point::new(12.0, 13.0)), Reach::Within);
    assert_eq!(reach(2.9, Point::new(12.0, 13.0)), Reach::Out);
    assert_eq!(reach(0.0, Point::new(10.2, 9.7)), Reach::Within);
    assert_eq!(reach(0.0, Point::new(10.2, 10.6)), Reach::Out);
    assert_eq!(reach(100.0, Point::new(f32::NAN, 10.0)), Reach::Out);
    assert_eq!(reach(f32::NAN, Point::new(10.0, 10.0)), Reach::Out);
    let nan_radius = working(0.0).reach_point(at, Footprint::new(f32::NAN, 0.0));
    assert_eq!(nan_radius, Reach::Out);
}

/// To a target the reach is the squared footprint gap against the squared range, at most it
/// within; a gap that is not a number is out.
#[test]
fn reach_target_compares_the_squared_gap() {
    let unpack = working(0.0);
    assert_eq!(unpack.reach_target(RANGE * RANGE), Reach::Within);
    assert_eq!(unpack.reach_target(252.82), Reach::Out);
    assert_eq!(unpack.reach_target(0.0), Reach::Within);
    assert_eq!(unpack.reach_target(f32::NAN), Reach::Out);
}

/// The work goes onto the timer; at the unpack time or past it the timer reached it, a timer that
/// is not a number never does.
#[test]
fn work_reaches_the_unpack_time_at_it_or_past_it() {
    let time = WholeSeconds::of_data(50);
    let mut unpack = working(40.0);
    assert_eq!(unpack.work(Seconds::new(9.5), time), PackTime::Left);
    assert_eq!(unpack.timer.get(), 49.5);
    assert_eq!(unpack.work(Seconds::new(0.5), time), PackTime::Reached);
    assert_eq!(
        working(f32::NAN).work(Seconds::new(30.0), time),
        PackTime::Left
    );
}

/// The frame runs up from 0 toward the frames as the timer runs up (`CVTTSS2SI`, its low byte);
/// none when that is not below the frames: at the unpack time, and past it. A timer that is not
/// a number gives frame 0, as does an unpack time of 0 (the infinite quotient's indefinite
/// integer `0x80000000` has a low byte of 0); a negative one gives a byte past the frames.
#[test]
fn frame_runs_up_toward_the_unpack_time() {
    let (time, frames) = (WholeSeconds::of_data(50), Frame::of_data(10));
    let frame = |timer: f32| working(timer).frame(time, frames).map(Frame::get);
    assert_eq!(frame(0.0), Some(0));
    assert_eq!(frame(4.9), Some(0));
    assert_eq!(frame(5.0), Some(1));
    assert_eq!(frame(49.9), Some(9));
    assert_eq!(frame(50.0), None);
    assert_eq!(frame(f32::NAN), Some(0));
    assert_eq!(frame(-5.0), None);
    assert_eq!(
        working(3.0)
            .frame(WholeSeconds::ZERO, frames)
            .map(Frame::get),
        Some(0)
    );
}

/// The progress is all of it in state 1; else the timer's share of the unpack time in percent,
/// truncated to its short.
#[test]
fn progress_is_timer_share_in_percent() {
    let time = WholeSeconds::of_data(50);
    let progress = |unpack: UnpackAction, time| unpack.progress(time).get();
    assert_eq!(progress(working(25.0), time), 50);
    assert_eq!(progress(working(12.4), time), 24);
    assert_eq!(progress(working(f32::NAN), time), 0);
    assert_eq!(progress(working(25.0), WholeSeconds::ZERO), 0);
    assert_eq!(progress(working(20_000.0), time), -25_536);
    let done = UnpackAction {
        state: UnpackState::Done,
        ..working(10.0)
    };
    assert_eq!(progress(done, time), 100);
    assert_eq!(progress(walking(MoveTo::Moving), time), 0);
}

/// Its stop (slot 0x38) is done in any state, its status kept; a walk's `MoveTo` goes and stops
/// the unit.
#[test]
fn halt_finishes_and_drops_its_walk() {
    for state in [
        UnpackState::New,
        UnpackState::Done,
        UnpackState::Working,
        UnpackState::Failed,
    ] {
        let mut unpack = UnpackAction {
            state,
            ..working(3.0)
        };
        assert_eq!(unpack.halt(), TornWalk::Untouched, "{state:?}");
        assert_eq!(
            unpack,
            UnpackAction {
                state: UnpackState::Done,
                ..working(3.0)
            }
        );
    }
    let mut walked = walking(MoveTo::Moving);
    assert_eq!(walked.halt(), TornWalk::Stops);
    assert_eq!(walked.state, UnpackState::Done);
    let mut empty = unpack_in(UnpackState::Moving { move_to: None }, 0.0);
    assert_eq!(empty.halt(), TornWalk::Untouched);
}

/// The destructor turns the building back at work in a played game alone.
#[test]
fn undone_only_at_work_in_played_game() {
    for (status, game_state, undone) in [
        (UnpackStatus::Unpacking, PlayState::Playing, true),
        (UnpackStatus::Unpacking, PlayState::Over, false),
        (UnpackStatus::Idle, PlayState::Playing, false),
        (UnpackStatus::Unpacked, PlayState::Playing, false),
    ] {
        assert_eq!(status.is_undone_in(game_state), undone, "{status:?}");
    }
}

/// The update answers 1 in states 1 and 13 and 0 in state 0, none of them working; a building
/// without an Unpack keeps its list.
#[test]
fn update_answers_by_its_state() {
    for (state, code) in [
        (UnpackState::Done, HeadCode::Deletes),
        (UnpackState::Failed, HeadCode::Deletes),
        (UnpackState::New, HeadCode::Keeps),
    ] {
        let unpack = unpack_in(state, 3.0);
        let mut world = world_with(unpacking(unpack));
        assert_eq!(updated(&mut world, 2000), Ok(code), "{state:?}");
        assert_eq!(held(&world), Some(unpack), "{state:?}");
        assert_eq!(shown(&world), Some((CONSTRUCTION, 9)));
    }
    let mut bare = world_with(ActionList::default());
    assert_eq!(updated(&mut bare, 2000), Ok(HeadCode::Keeps));
}

/// The update's check keeps a target up to state 2 and lets one in state 3 or later go, and one
/// the world does not hold: the Unpack then holds none.
#[test]
fn update_lets_target_past_state_2_go() {
    for (state, kept) in [
        (ObjectState::Alive, true),
        (ObjectState::Foundation, true),
        (ObjectState::Dying, false),
        (ObjectState::Removed, false),
    ] {
        let made = unpack_in(UnpackState::New, 3.0);
        let mut world = trebuchet_world(ids::ID_42, unpacking(made), state);
        assert_eq!(updated(&mut world, 2000), Ok(HeadCode::Keeps), "{state:?}");
        let target = held(&world).unwrap().target;
        assert_eq!(target, kept.then_some(TARGET), "{state:?}");
    }
    let mut lost = world_with(unpacking(UnpackAction {
        target: Some(object_id(801)),
        ..unpack_in(UnpackState::New, 3.0)
    }));
    assert_eq!(updated(&mut lost, 2000), Ok(HeadCode::Keeps));
    assert_eq!(held(&lost).unwrap().target, None);
}

/// At work the building's work goes onto the timer: its work rate's powerup plus its master's,
/// over the update's seconds ((0.5 + 4.5) × 2); the sprite takes the timer's frame
/// (10 × 30 / 50 = 6). The quick-build cheat's work is 1000, whatever the rate: done at once.
#[test]
fn update_at_work_adds_work_and_shows_its_frame() {
    let mut world = world_with(unpacking(working(20.0))).with_combats([(
        TREBUCHET,
        Combat {
            work_rate_powerup: WorkRate::new(0.5),
            ..Combat::default()
        },
    )]);
    assert_eq!(updated(&mut world, 2000), Ok(HeadCode::Keeps));
    assert_eq!(held(&world), Some(working(30.0)));
    assert_eq!(shown(&world), Some((CONSTRUCTION, 6)));
    let mut instant = world_with(unpacking(working(0.0)));
    instant.instant_work = 1;
    assert_eq!(updated(&mut instant, 0), Ok(HeadCode::Deletes));
    assert_eq!(held(&instant).unwrap().timer, Seconds::new(1000.0));
    let mut bare = world_with(unpacking(working(5.0)));
    bare.players[1].objects[AT.1].sprite = None;
    assert!(updated(&mut bare, 2000).is_err());
}

/// Once the timer reached the unpack time the building takes its standing sprite, the Unpack is
/// done at status 2 (1), and its unit AI takes the attack order on the target first (700 at
/// priority 0x32, the building's weapon range, at the target's place, shown there, ready, queued
/// in front). Without a target it orders nothing; without a unit AI it fails.
#[test]
fn update_at_unpack_time_stands_and_orders_the_attack() {
    let mut world = world_with(unpacking(working(49.0)));
    let first = QueuedOrder {
        priority: OrderPriority::of_data(1),
        ..QueuedOrder::default()
    };
    world.parts.unit_ai.get_mut(TREBUCHET).unwrap().orders = vec![first];
    assert_eq!(updated(&mut world, 2000), Ok(HeadCode::Deletes));
    assert_eq!(trebuchet(&world).life.master_id, ids::ID_42);
    assert_eq!(shown(&world).map(|(id, _)| id), Some(STANDING));
    assert_eq!(
        held(&world),
        Some(UnpackAction {
            state: UnpackState::Done,
            status: UnpackStatus::Unpacked,
            ..working(58.0)
        })
    );
    assert_eq!(
        queued(&world),
        [
            QueuedOrder {
                issuer: Some(Issuer::Object(TREBUCHET)),
                order: OrderKind::Attack,
                priority: OrderPriority::of_data(0x32),
                target: Some(TARGET),
                target_owner: Some(PlayerId::new(2)),
                point: Position::new(20.0, 30.0, 0.5),
                range: Distance::new(16.0),
                shown: Point::new(20.0, 30.0),
                ready: true,
                target_kind: Some(OrderTargetKind::of_master(ids::VILLAGER_MALE)),
            },
            first,
        ]
    );
    let mut aimless = world_with(unpacking(UnpackAction {
        target: None,
        ..working(49.0)
    }));
    assert_eq!(updated(&mut aimless, 2000), Ok(HeadCode::Deletes));
    assert_eq!(queued(&aimless), []);
    assert_eq!(held(&aimless).unwrap().status, UnpackStatus::Unpacked);
    let mut deaf = world_with(unpacking(working(49.0)));
    deaf.parts.set_unit_ai(TREBUCHET, None);
    assert_eq!(
        updated(&mut deaf, 2000).unwrap_err().reason(),
        "a Pack's or Unpack's attack order through a null unit AI"
    );
}

/// The Unpack's end makes its master's annexes at their slots' places off the building, links
/// each in its slot and gives it the building's hit points; a building annex takes the building
/// as its main.
#[test]
fn update_at_unpack_time_makes_the_annexes() {
    let (tower, soldier) = (
        annex_master(273, MasterKind::BUILDING),
        annex_master(379, MasterKind::ANIMATED),
    );
    let mut annexed = trebuchet_master();
    let building = annexed.building.as_mut().unwrap();
    building.annexes = [None, Some(tower.id), None, Some(soldier.id)];
    building.annex_offsets[1] = GroundOffset::new(1.0, -1.0);
    building.annex_offsets[3] = GroundOffset::new(-0.5, 0.5);
    let mut world = world_with(unpacking(working(49.0)));
    world.model.masters[1] = Arc::new(Masters::new([annexed, packed_master(), tower, soldier]));
    assert_eq!(updated(&mut world, 2000), Ok(HeadCode::Deletes));
    let annexes = world.parts.building_of(TREBUCHET).annexes;
    let [second, fourth] = annexes.ids().collect::<Vec<_>>()[..] else {
        panic!("two annexes: {annexes:?}");
    };
    assert_eq!(
        annexes,
        Annexes::new([None, Some(second), None, Some(fourth)])
    );
    let made = |id| {
        let annex = world.players[1].objects.by_id(id).unwrap();
        (
            annex.position(),
            annex.life.hp,
            world.parts.building_of(id).annex_of.map(RoleId::id),
        )
    };
    assert_eq!(
        made(second),
        (
            Position::new(11.0, 9.0, 0.0),
            HitPoints::new(120.0),
            Some(TREBUCHET)
        )
    );
    assert_eq!(
        made(fourth),
        (Position::new(9.5, 10.5, 0.0), HitPoints::new(120.0), None)
    );
}

/// A walk that arrived (1) unpacks (`set_state(6)`): the `MoveTo` goes and stops the unit, the
/// building takes the unpacked master and its construction sprite (its standing one for none),
/// and the Unpack, back at its list's head, starts at work from the unit's pack progress; the
/// update answers 0.
#[test]
fn arrived_walk_unpacks_the_building() {
    let mut world = packed_with(unpacking(walking(MoveTo::Arrived)));
    world.parts.actions_mut(TREBUCHET).unwrap().pack_progress = PackProgress::of_data(7);
    assert_eq!(waiting(&world), 1);
    assert_eq!(updated(&mut world, 2000), Ok(HeadCode::Keeps));
    assert_eq!(waiting(&world), 0);
    assert_eq!(trebuchet(&world).life.master_id, ids::ID_42);
    assert_eq!(shown(&world).map(|(id, _)| id), Some(CONSTRUCTION));
    assert_eq!(
        held(&world),
        Some(UnpackAction {
            state: UnpackState::Working,
            timer: Seconds::new(7.0),
            status: UnpackStatus::Unpacking,
            ..walking(MoveTo::Arrived)
        })
    );
    assert_eq!(world.parts.actions_of(TREBUCHET).list.iter().count(), 1);
    let mut plain = packed_with(unpacking(walking(MoveTo::Arrived)));
    Arc::make_mut(&mut plain.model.masters[1])
        .get_mut(ids::ID_42)
        .unwrap()
        .building
        .as_mut()
        .unwrap()
        .construction_sprite = None;
    assert_eq!(updated(&mut plain, 2000), Ok(HeadCode::Keeps));
    assert_eq!(shown(&plain).map(|(id, _)| id), Some(STANDING));
}

/// `set_state(6)` takes the Unpack out of its list: the action below it restarts (a Make takes
/// its task's resource, 0 of it, `FUN_140a98e90`), then the transform's list deletion takes it,
/// and the Unpack alone goes back to the head.
#[test]
fn unpacking_restarts_then_deletes_the_actions_below() {
    let food = Task {
        attribute_kind: Some(attribute::FOOD),
        ..Task::default()
    };
    let mut world = packed_with(ActionList::default().with_make(Some(MakeAction::new(food))));
    world.players[1].objects[AT.1].life.amount = Amount::new(5.0);
    world
        .parts
        .actions_mut(TREBUCHET)
        .unwrap()
        .list
        .push_head(ListAction::from(walking(MoveTo::Arrived)));
    assert_eq!(updated(&mut world, 2000), Ok(HeadCode::Keeps));
    let list = &world.parts.actions_of(TREBUCHET).list;
    assert_eq!(list.iter().count(), 1);
    assert_eq!(held(&world).unwrap().state, UnpackState::Working);
    let life = &trebuchet(&world).life;
    assert_eq!(
        (life.attribute_kind, life.amount),
        (Some(attribute::FOOD), Amount::ZERO)
    );
}

/// A master whose transform its owner has not got, or that has none, fails the Unpack
/// (`set_state(13)`): the building stays.
#[test]
fn unpacking_without_transform_master_fails() {
    for transform in [Some(MasterId::named(273)), None] {
        let mut world = packed_with(unpacking(walking(MoveTo::Arrived)));
        Arc::make_mut(&mut world.model.masters[1])
            .get_mut(ids::ID_331)
            .unwrap()
            .building
            .as_mut()
            .unwrap()
            .transform_unit = transform;
        assert_eq!(updated(&mut world, 2000), Ok(HeadCode::Keeps));
        assert_eq!(trebuchet(&world).life.master_id, ids::ID_331);
        assert_eq!(
            held(&world),
            Some(UnpackAction {
                state: UnpackState::Failed,
                ..walking(MoveTo::Arrived)
            })
        );
        assert_eq!(shown(&world).map(|(id, _)| id), Some(CONSTRUCTION));
        assert_eq!(waiting(&world), 0);
    }
}

/// A unit in state 3 or later takes no action back: its Unpack's destructor runs (it lets the
/// target go) and the list stays empty, but the unit still shows the construction sprite.
#[test]
fn dying_unit_deletes_its_unpack_at_work() {
    let mut world = packed_with(unpacking(walking(MoveTo::Arrived)));
    world.players[1].objects[AT.1].life.state = ObjectState::Dying;
    world.players[1].objects[AT.1].sprite =
        Some(ActiveSprite::at_rest(graphic(WALKING), Frame::of_data(0)));
    world.players[2].objects[ListIndex::new(0)].workers = WorkerCount::of_data(1);
    assert_eq!(updated(&mut world, 2000), Ok(HeadCode::Keeps));
    assert_eq!(held(&world), None);
    assert!(world.parts.actions_of(TREBUCHET).list.is_empty());
    assert_eq!(shown(&world).map(|(id, _)| id), Some(CONSTRUCTION));
    let villager = &world.players[2].objects[ListIndex::new(0)];
    assert_eq!(villager.workers.get(), 0);
}

/// A walk on its way (0) or failed (3) unpacks within reach of its target; out of it a failed one
/// fails the Unpack (13, the unit stopped) and one on its way goes on (11).
#[test]
fn walk_tests_its_reach_to_the_target() {
    for (state, range, after) in [
        (MoveTo::Waiting, 22.0, UnpackState::Working),
        (MoveTo::Failed, 22.0, UnpackState::Working),
        (MoveTo::Failed, RANGE, UnpackState::Failed),
    ] {
        let mut world = packed_with(unpacking(UnpackAction {
            range: Distance::new(range),
            ..walking(state)
        }));
        assert_eq!(updated(&mut world, 2000), Ok(HeadCode::Keeps), "{state:?}");
        assert_eq!(held(&world).unwrap().state, after, "{state:?} {range}");
        assert_eq!(waiting(&world), 0, "{state:?} {range}");
    }
    let mut on_way = packed_with(unpacking(walking(MoveTo::Waiting)));
    assert_eq!(updated(&mut on_way, 100), Ok(HeadCode::Keeps));
    let walked = held(&on_way).unwrap();
    assert!(matches!(
        walked.state,
        UnpackState::Moving {
            move_to: Some(MoveToAction {
                state: MoveTo::Waiting,
                ..
            })
        }
    ));
    assert_eq!(waiting(&on_way), 1);
    assert_eq!(trebuchet(&on_way).life.master_id, ids::ID_331);
}

/// Without a target a walk on its way tests its reach to the point: within it unpacks, out of
/// it goes on; a failed one unpacks where it stands, out of reach too.
#[test]
fn walk_tests_its_reach_to_the_point() {
    for (state, range, after) in [
        (MoveTo::Waiting, 3.0, 6),
        (MoveTo::Waiting, 2.9, 11),
        (MoveTo::Failed, 0.0, 6),
    ] {
        let mut world = packed_with(unpacking(walking_to_point(state, range)));
        assert_eq!(updated(&mut world, 100), Ok(HeadCode::Keeps), "{state:?}");
        assert_eq!(
            held(&world).unwrap().state.get(),
            after,
            "{state:?} {range}"
        );
    }
}

/// A walk whose `MoveTo` let its dead target go (5) changes nothing of the Unpack, whose own check
/// lets the target go.
#[test]
fn walk_whose_target_died_goes_on() {
    let mut world = trebuchet_world(
        ids::ID_331,
        unpacking(walking(MoveTo::Moving)),
        ObjectState::Removed,
    );
    assert_eq!(updated(&mut world, 100), Ok(HeadCode::Keeps));
    let walked = held(&world).unwrap();
    assert_eq!(walked.target, None);
    assert_eq!(walked.state.get(), 11);
    assert_eq!(trebuchet(&world).life.master_id, ids::ID_331);
}

/// A walk in state 3 asks what the unit's AI asks of its path: a point in range ends the walk at
/// once for a unit AI on no patrol, its path unasked, and the units in the way go to the AI after
/// a start or a move; on a patrol the walk asks a looping path. A wait posts none.
#[test]
fn walk_start_reads_its_unit_ai_and_posts_units_in_the_way() {
    let in_the_way = |world: &mut World| {
        let mover: &mut Mover = world.parts.mover_mut(TREBUCHET).unwrap();
        mover.in_the_way = vec![object_id(901)];
    };
    let loops = |world: &World| {
        world
            .parts
            .mover_of(TREBUCHET)
            .unwrap()
            .paths
            .iter()
            .any(|path| path.path_flags.contains(PathFlags::LOOPS))
    };
    let mut world = packed_with(unpacking(walking_to_point(MoveTo::Start, 10.0)));
    in_the_way(&mut world);
    assert_eq!(updated(&mut world, 100), Ok(HeadCode::Keeps));
    assert_eq!(heard(&world), [(NotifyKind::IN_THE_WAY, [901, -1, -1])]);
    assert_eq!(held(&world).unwrap().state.get(), 6);
    assert_eq!(
        world.parts.mover_of(TREBUCHET).unwrap().paths,
        walker().paths
    );
    let mut patrolling = packed_with(unpacking(walking_to_point(MoveTo::Start, 10.0)));
    patrolling
        .parts
        .unit_ai
        .get_mut(TREBUCHET)
        .unwrap()
        .current_order = Some(OrderKind::Patrol);
    assert_eq!(updated(&mut patrolling, 100), Ok(HeadCode::Keeps));
    assert!(loops(&patrolling));
    assert!(!loops(&world));
    // a walk on its way follows the path the patrol's start asked
    let mut on_way = packed_with(unpacking(walking_to_point(MoveTo::Moving, 2.9)));
    in_the_way(&mut on_way);
    on_way.parts.mover_mut(TREBUCHET).unwrap().paths =
        patrolling.parts.mover_of(TREBUCHET).unwrap().paths.clone();
    assert_eq!(updated(&mut on_way, 100), Ok(HeadCode::Keeps));
    assert_eq!(heard(&on_way), [(NotifyKind::IN_THE_WAY, [901, -1, -1])]);
    let mut waits = packed_with(unpacking(walking_to_point(MoveTo::Waiting, 2.9)));
    in_the_way(&mut waits);
    assert_eq!(updated(&mut waits, 100), Ok(HeadCode::Keeps));
    assert_eq!(heard(&waits), []);
}

/// A walk to its target in state 3 asks the unit AI's patrol turn of the target's place, which
/// its start takes as its point, not of the point it last walked to: on a patrol without a saved
/// one, the villager 22.4 off on the map goes back and forth though the old point (11, 11) lay
/// 1.4 off. A target past state 6, on the map still, ends the walk's update before its start: no
/// path is asked, and the saved patrol stays.
#[test]
fn walk_start_to_target_asks_of_its_place() {
    // the top level's flags, which the ask gives (a planned level's are a new path's)
    let top_flags = |world: &World| {
        world
            .parts
            .mover_of(TREBUCHET)
            .unwrap()
            .paths
            .iter()
            .find(|path| path.level == WaypointLevel::Top)
            .map(|path| path.path_flags)
    };
    let patrol = |world: &mut World, saved: Option<PatrolPath>| {
        let ai = world.parts.unit_ai.get_mut(TREBUCHET).unwrap();
        ai.current_order = Some(OrderKind::Patrol);
        ai.patrol_path = saved;
    };
    let stale = UnpackAction {
        state: UnpackState::Moving {
            move_to: Some(MoveToAction {
                target: Some(TARGET),
                point: Position::new(11.0, 11.0, 0.0),
                ..MoveToAction::sub_in(MoveTo::Start)
            }),
        },
        ..walking(MoveTo::Start)
    };
    let on_map = |world: &mut World| {
        let villager = &world.players[2].objects[ListIndex::new(0)];
        let placed = Placed {
            position: villager.position(),
            radius: Footprint::new(0.2, 0.2),
            class: class::VILLAGER,
            player: PlayerId::new(2),
            master: ids::VILLAGER_MALE,
            obstruction_kind: ObstructionKind::NONE,
            buildable_on: BuildableOn::NO,
            tall: false,
            moves: false,
            unknown: None,
            obstruction: None,
        };
        world.spatial.insert(TARGET, placed);
    };
    let mut world = packed_with(unpacking(stale));
    on_map(&mut world);
    patrol(&mut world, None);
    assert_eq!(updated(&mut world, 100), Ok(HeadCode::Keeps));
    let flags = top_flags(&world).unwrap();
    assert!(flags.contains(PathFlags::LOOPS | PathFlags::BACK_AND_FORTH));
    let walked = held(&world).unwrap();
    assert_eq!(
        walked.state.move_to().map(|move_to| move_to.point),
        Some(Position::new(20.0, 30.0, 0.5))
    );
    let mut gone = trebuchet_world(
        ids::ID_331,
        unpacking(walking(MoveTo::Start)),
        ObjectState::Removed,
    );
    on_map(&mut gone);
    patrol(&mut gone, Some(PatrolPath::default()));
    assert_eq!(updated(&mut gone, 100), Ok(HeadCode::Keeps));
    let ai = gone.parts.unit_ai.get(TREBUCHET).unwrap();
    assert_eq!(ai.patrol_path, Some(PatrolPath::default()));
    assert_eq!(
        gone.parts.mover_of(TREBUCHET).unwrap().paths,
        walker().paths
    );
}

/// The list's deletion outside the step (`FUN_140a98620`, [`World::clear_actions`]) and the
/// head's from the step (`FUN_140a98e20`, [`Unit::delete_own_head`]) each run the destructor:
/// at work in a played game it turns the building into its master's transform; in a game that
/// is over, or at another status, the building stays. Its target goes either way.
#[test]
fn deleted_unpack_turns_the_building_back() {
    let deletions: [(&str, Deletion); 2] = [
        ("world", |world| world.clear_actions(AT.0, AT.1)),
        ("step", |world| delete_head_at(world, AT.0, AT.1)),
    ];
    for (path, delete) in deletions {
        for (status, game_state, master) in [
            (UnpackStatus::Unpacking, PlayState::Playing, ids::ID_331),
            (UnpackStatus::Unpacking, PlayState::Over, ids::ID_42),
            (UnpackStatus::Idle, PlayState::Playing, ids::ID_42),
            (UnpackStatus::Unpacked, PlayState::Playing, ids::ID_42),
        ] {
            let mut world = world_with(unpacking(UnpackAction {
                status,
                ..working(3.0)
            }));
            world.players[2].objects[ListIndex::new(0)].workers = WorkerCount::of_data(1);
            world.game_state = game_state;
            delete(&mut world).unwrap();
            assert_eq!(held(&world), None, "{path}");
            let unit = trebuchet(&world);
            assert_eq!(unit.life.master_id, master, "{path} {status:?}");
            let villager = &world.players[2].objects[ListIndex::new(0)];
            assert_eq!(villager.workers.get(), 0, "{path} {status:?}");
        }
    }
}

/// A destructor whose master's transform its owner has not got leaves the building.
#[test]
fn deleted_unpack_without_transform_master_stays() {
    let mut world = world_with(unpacking(working(3.0)));
    Arc::make_mut(&mut world.model.masters[1])
        .get_mut(ids::ID_42)
        .unwrap()
        .building
        .as_mut()
        .unwrap()
        .transform_unit = Some(MasterId::named(273));
    world.clear_actions(AT.0, AT.1).unwrap();
    assert_eq!(trebuchet(&world).life.master_id, ids::ID_42);
    let mut stepped = world_with(unpacking(working(3.0)));
    Arc::make_mut(&mut stepped.model.masters[1])
        .get_mut(ids::ID_42)
        .unwrap()
        .building
        .as_mut()
        .unwrap()
        .transform_unit = None;
    delete_head_at(&mut stepped, AT.0, AT.1).unwrap();
    assert_eq!(trebuchet(&stepped).life.master_id, ids::ID_42);
}

/// An Unpack under a deleted head restarts as it was (slot 0x28 with 0 does nothing).
#[test]
fn unpack_under_deleted_head_restarts_as_it_was() {
    let mut under = world_with(wait_over(unpacking(working(30.0))));
    delete_head_at(&mut under, AT.0, AT.1).unwrap();
    assert_eq!(held(&under), Some(working(30.0)));
    assert_eq!(shown(&under), Some((CONSTRUCTION, 9)));
}

/// The destructor (slot 0) hands the Unpack's status over, lets its target go, and its walk's
/// `MoveTo` stops the unit; its teardown outside a unit's own deletion is refused.
#[test]
fn torn_unpack_hands_its_status_over_and_releases_its_target() {
    let mut world = packed_with(ActionList::default());
    let torn = ListAction::from(walking(MoveTo::Moving))
        .tear_down(world.parts.motion_mut(TREBUCHET), Speed::ZERO)
        .unwrap();
    assert_eq!(
        torn,
        TornDown {
            releases: vec![TARGET],
            unpack: Some(UnpackStatus::Idle),
            ..TornDown::default()
        }
    );
    assert_eq!(waiting(&world), 0);
    assert!(!torn.is_empty());
    let refused = in_update(&mut world, AT, 0, |unit, _, step| {
        let by = unit.releaser(step)?;
        torn.let_go(step, unit, by)
    });
    assert_eq!(
        refused.unwrap_err().reason(),
        "a torn-down Pack or Unpack whose own part its unit's deletion did not run"
    );
}

/// The object stop keeps a top Unpack's share done in `+0x88a` and ends the Unpack, which the
/// list keeps, its walk's `MoveTo` stopping the unit.
#[test]
fn stop_keeps_unpack_progress() {
    let mut world = world_with(unpacking(working(25.0)));
    world.parts.set_unit_ai(TREBUCHET, None);
    world.object_stop(AT.0, AT.1).unwrap();
    assert_eq!(
        world.parts.actions_of(TREBUCHET).pack_progress,
        PackProgress::of_data(50)
    );
    assert_eq!(
        held(&world),
        Some(UnpackAction {
            state: UnpackState::Done,
            ..working(25.0)
        })
    );
    let mut walked = packed_with(unpacking(walking(MoveTo::Moving)));
    walked.parts.set_unit_ai(TREBUCHET, None);
    walked.object_stop(AT.0, AT.1).unwrap();
    assert_eq!(held(&walked).unwrap().state, UnpackState::Done);
    assert_eq!(waiting(&walked), 0);
}
