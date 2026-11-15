//! The Pack's tests on player 1's trebuchet 700 (master 42) and player 2's villager 800.

use super::*;
use crate::commands::QueueId;
use crate::construction::Annexes;
use crate::map::Distance;
use crate::map::GroundOffset;
use crate::map::Point;
use crate::map::Position;
use crate::movement::Speed;
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
use crate::objects::actions::Action as ListAction;
use crate::objects::actions::ActionList;
use crate::objects::layers::Action;
use crate::objects::layers::Combat;
use crate::objects::layers::Static;
use crate::objects::lifecycle::Lifecycle;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ActionPart;
use crate::objects::masters::BuildingMaster;
use crate::objects::masters::CombatPart;
use crate::objects::masters::CreatablePart;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::Masters;
use crate::objects::masters::TrainLocation;
use crate::objects::sprite::{ActiveSprite, Graphic};
use crate::objects::teardown::{HeadCode, TornDown};
use crate::objects::unit::Unit;
use crate::orders::Work;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::test_ids::object_id;
use crate::testing::class;
use crate::testing::ids;
use crate::testing::roles;
use crate::testing::slots;
use crate::unit_ai::AiAction;
use crate::unit_ai::Issuer;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::OrderKind;
use crate::unit_ai::OrderPriority;
use crate::unit_ai::OrderTargetKind;
use crate::unit_ai::QueuedOrder;
use crate::unit_ai::UnitAi;
use crate::world::FrameSeed;
use crate::world::Millis;
use crate::world::PlayState;
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

/// The trebuchet's construction sprite 137, which a Pack at work shows. The fixture's sprite of
/// that id is a stand-in ([`graphic`]): the `.dat`'s 137 has 3 frames of 32 angles and a frame
/// duration of 0.
const PACKING: SpriteId = SpriteId::named(137);

/// The trebuchet's standing sprite 1180 of the `.dat`, which a Pack's undoing shows.
const STANDING: SpriteId = SpriteId::named(1180);

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
/// its pack time 50 (its train location's), its work rate 4.5, its weapon range 16, its standing
/// sprite 1180 and its transform the packed trebuchet 331.
fn trebuchet_master() -> Master {
    Master {
        id: ids::ID_42,
        kind: MasterKind::BUILDING,
        class: class::CLASS_54,
        hit_points: MasterHitPoints::of_data(150),
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
            transform_unit: Some(ids::ID_331),
            ..BuildingMaster::default()
        }),
        ..Master::default()
    }
}

/// The packed trebuchet master 331 of build 158041's `.dat`: a building of class 51, 150 hit
/// points, its pack time 50, its standing sprite 2281 and its transform 42.
fn packed_master() -> Master {
    Master {
        id: ids::ID_331,
        class: class::CLASS_51,
        standing_sprite: Some(SpriteId::named(2281)),
        building: Some(BuildingMaster {
            transform_unit: Some(ids::ID_42),
            ..BuildingMaster::default()
        }),
        ..trebuchet_master()
    }
}

/// A Pack at work (state 6, status 1) on the villager, its timer `timer`.
fn working(timer: f32) -> PackAction {
    PackAction {
        state: PackState::Working,
        target: Some(TARGET),
        timer: Seconds::new(timer),
        status: PackStatus::Packing,
        unique_id: QueueId::of_data(-1),
    }
}

/// The trebuchet holding `held` alone.
fn packing(held: PackAction) -> ActionList {
    ActionList::default().with_pack(Some(held))
}

/// Player 1's trebuchet 700 (its masters 42 and 331) at (10, 10), alive with 120 hit points,
/// showing its construction sprite at frame 9, its action list `list`, its unit AI a building's;
/// player 2's villager 800 (master 83) at (20, 30) in `state`.
fn trebuchet_world(list: ActionList, state: ObjectState) -> World {
    let trebuchet = Unit {
        id: TREBUCHET,
        place: Static::at(Position::new(10.0, 10.0, 0.0)),
        life: Lifecycle {
            state: ObjectState::Alive,
            hp: HitPoints::new(120.0),
            master_id: ids::ID_42,
            ..Lifecycle::default()
        },
        sprite: Some(ActiveSprite::at_rest(graphic(PACKING), Frame::of_data(9))),
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
    world.model.masters = vec![
        Arc::new(Masters::default()),
        Arc::new(Masters::new([trebuchet_master(), packed_master()])),
        Arc::new(Masters::new([Master {
            id: ids::VILLAGER_MALE,
            kind: MasterKind::COMBAT,
            class: class::VILLAGER,
            ..Master::default()
        }])),
    ];
    world.model.graphics = Arc::new(Graphics::from(
        [PACKING, STANDING, SpriteId::named(2281)].map(|id| (id, graphic(id))),
    ));
    world
}

/// [`trebuchet_world`] with the villager alive.
fn world_with(list: ActionList) -> World {
    trebuchet_world(list, ObjectState::Alive)
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

/// The trebuchet's Pack, if it holds one.
fn held(world: &World) -> Option<PackAction> {
    world.parts.actions_of(TREBUCHET).list.pack().copied()
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

/// The notify the destructor posts once the timer reached the pack time.
fn completed() -> (NotifyKind, [i32; 3]) {
    (
        NotifyKind::ACTION_COMPLETED,
        [AiAction::OnTarget.value(), 0, 0],
    )
}

/// The orders in the trebuchet's unit AI's queue.
fn queued(world: &World) -> Vec<QueuedOrder> {
    world.parts.unit_ai.get(TREBUCHET).unwrap().orders.clone()
}

/// The trebuchet's Pack update (slot 0x30) in an update of `dt` ms.
fn updated(world: &mut World, dt: u32) -> GenieResult<HeadCode> {
    in_update(world, AT, dt, |unit, actions, step| {
        unit.pack_part(actions, step)
    })
}

/// Each state and status reads back from its byte; a byte without one is an error.
#[test]
fn its_states_and_statuses_read_back_from_their_bytes() {
    for byte in 0..=u8::MAX {
        match PackState::of_byte(byte) {
            Ok(state) => assert_eq!(state.get(), byte),
            Err(error) => {
                assert!(!matches!(byte, 0 | 1 | 6 | 13), "{byte}");
                assert_eq!(error.reason(), "no state of a Pack");
            }
        }
        match PackStatus::of_byte(byte) {
            Ok(status) => assert_eq!(status.get(), byte),
            Err(error) => {
                assert!(2 < byte, "{byte}");
                assert_eq!(error.reason(), "no status of a Pack");
            }
        }
    }
    assert_eq!(PackState::of_byte(13).unwrap(), PackState::Failed);
    assert_eq!(PackStatus::of_byte(2).unwrap(), PackStatus::Transformed);
    assert_eq!(PackProgress::of_data(-3).get(), -3);
}

/// The Pack is type 0x7c of the list, its state and target the list's reads.
#[test]
fn pack_is_kind_0x7c_of_the_list() {
    let listed = ListAction::from(working(3.0));
    assert_eq!(listed.kind(), ActionKind::PACK);
    assert_eq!(listed.state().get(), 6);
    assert_eq!(listed.target(), Some(TARGET));
}

/// A master's pack time is its first train location's time, 0 without one.
#[test]
fn pack_time_is_first_train_location_time() {
    assert_eq!(pack_time(&trebuchet_master()), WholeSeconds::of_data(50));
    let mut two = trebuchet_master();
    two.creatable
        .as_mut()
        .unwrap()
        .train_locations
        .push(TrainLocation {
            time: WholeSeconds::of_data(7),
            ..TrainLocation::default()
        });
    assert_eq!(pack_time(&two), WholeSeconds::of_data(50));
    assert_eq!(pack_time(&Master::default()), WholeSeconds::ZERO);
}

/// The work goes onto the timer; at the pack time or past it the timer reached it, a timer that
/// is not a number never does.
#[test]
fn work_reaches_the_pack_time_at_it_or_past_it() {
    let time = WholeSeconds::of_data(50);
    let mut pack = working(40.0);
    assert_eq!(pack.work(Seconds::new(9.5), time), PackTime::Left);
    assert_eq!(pack.timer.get(), 49.5);
    assert_eq!(pack.work(Seconds::new(0.5), time), PackTime::Reached);
    assert_eq!(
        working(40.0).work(Seconds::new(30.0), time),
        PackTime::Reached
    );
    assert_eq!(
        working(f32::NAN).work(Seconds::new(30.0), time),
        PackTime::Left
    );
    assert_eq!(
        working(0.0).work(Seconds::ZERO, WholeSeconds::ZERO),
        PackTime::Reached
    );
}

/// The frame runs down from the frames toward 0 as the timer runs up (`CVTTSS2SI`, its low
/// byte); none when that is not below the frames: at the timer 0, and past the pack time, where
/// the byte of −2 is 254. A timer that is not a number, and a pack time of 0, give frame 0.
#[test]
fn frame_runs_down_toward_the_pack_time() {
    let (time, frames) = (WholeSeconds::of_data(50), Frame::of_data(10));
    let frame = |timer: f32| working(timer).frame(time, frames).map(Frame::get);
    assert_eq!(frame(0.0), None);
    assert_eq!(frame(0.1), Some(9));
    assert_eq!(frame(25.0), Some(5));
    assert_eq!(frame(49.0), Some(0));
    assert_eq!(frame(50.0), Some(0));
    assert_eq!(frame(60.0), None);
    assert_eq!(frame(f32::NAN), Some(0));
    assert_eq!(
        working(3.0)
            .frame(WholeSeconds::ZERO, frames)
            .map(Frame::get),
        Some(0)
    );
}

/// The progress is all of it in state 1; else the timer's share of the pack time in percent,
/// truncated to its short: a NaN share is the word `0x80000000`, whose low half is 0.
#[test]
fn progress_is_timer_share_in_percent() {
    let time = WholeSeconds::of_data(50);
    let progress = |pack: PackAction, time| pack.progress(time).get();
    assert_eq!(progress(working(25.0), time), 50);
    assert_eq!(progress(working(12.4), time), 24);
    assert_eq!(progress(working(f32::NAN), time), 0);
    assert_eq!(progress(working(25.0), WholeSeconds::ZERO), 0);
    assert_eq!(progress(working(20_000.0), time), -25_536);
    let done = PackAction {
        state: PackState::Done,
        ..working(10.0)
    };
    assert_eq!(progress(done, time), 100);
    let made = PackAction {
        state: PackState::New,
        ..working(10.0)
    };
    assert_eq!(progress(made, time), 20);
}

/// Its stop (slot 0x38) is done in any state, its status and timer kept.
#[test]
fn halt_finishes_and_keeps_its_status() {
    for state in [
        PackState::New,
        PackState::Done,
        PackState::Working,
        PackState::Failed,
    ] {
        let mut pack = PackAction {
            state,
            ..working(3.0)
        };
        pack.halt();
        assert_eq!(
            pack,
            PackAction {
                state: PackState::Done,
                ..working(3.0)
            },
            "{state:?}"
        );
    }
}

/// The destructor undoes a Pack at work in a played game alone.
#[test]
fn undone_only_at_work_in_played_game() {
    for (status, game_state, undone) in [
        (PackStatus::Packing, PlayState::Playing, true),
        (PackStatus::Packing, PlayState::Over, false),
        (PackStatus::Idle, PlayState::Playing, false),
        (PackStatus::Transformed, PlayState::Playing, false),
    ] {
        let pack = PackAction {
            status,
            ..working(3.0)
        };
        assert_eq!(pack.is_undone_in(game_state), undone, "{status:?}");
    }
}

/// The update answers 1 in states 1 and 13 and 0 in state 0, none of them working.
#[test]
fn update_answers_by_its_state() {
    for (state, code) in [
        (PackState::Done, HeadCode::Deletes),
        (PackState::Failed, HeadCode::Deletes),
        (PackState::New, HeadCode::Keeps),
    ] {
        let pack = PackAction {
            state,
            ..working(3.0)
        };
        let mut world = world_with(packing(pack));
        assert_eq!(updated(&mut world, 2000), Ok(code), "{state:?}");
        assert_eq!(held(&world), Some(pack), "{state:?}");
        assert_eq!(shown(&world), Some((PACKING, 9)));
    }
}

/// An update of a building without a Pack keeps its list.
#[test]
fn update_without_pack_keeps_the_list() {
    let mut world = world_with(ActionList::default());
    assert_eq!(updated(&mut world, 2000), Ok(HeadCode::Keeps));
}

/// The update's check keeps a target up to state 2 and lets one in state 3 or later go, and one
/// the world does not hold: the Pack then holds none.
#[test]
fn update_lets_target_past_state_2_go() {
    for (state, kept) in [
        (ObjectState::Alive, true),
        (ObjectState::Foundation, true),
        (ObjectState::Dying, false),
        (ObjectState::Removed, false),
    ] {
        let made = PackAction {
            state: PackState::New,
            ..working(3.0)
        };
        let mut world = trebuchet_world(packing(made), state);
        assert_eq!(updated(&mut world, 2000), Ok(HeadCode::Keeps), "{state:?}");
        let target = held(&world).unwrap().target;
        assert_eq!(target, kept.then_some(TARGET), "{state:?}");
    }
    let mut lost = world_with(packing(PackAction {
        target: Some(object_id(801)),
        state: PackState::New,
        ..working(3.0)
    }));
    assert_eq!(updated(&mut lost, 2000), Ok(HeadCode::Keeps));
    assert_eq!(held(&lost).unwrap().target, None);
}

/// At work the building's work goes onto the timer: its work rate's powerup plus its master's,
/// over the update's seconds ((0.5 + 4.5) × 2); the sprite takes the timer's frame
/// ((50 − 30) × 10 / 50 = 4).
#[test]
fn update_at_work_adds_work_and_shows_its_frame() {
    let mut world = world_with(packing(working(20.0))).with_combats([(
        TREBUCHET,
        Combat {
            work_rate_powerup: WorkRate::new(0.5),
            ..Combat::default()
        },
    )]);
    assert_eq!(updated(&mut world, 2000), Ok(HeadCode::Keeps));
    assert_eq!(held(&world), Some(working(30.0)));
    assert_eq!(shown(&world), Some((PACKING, 4)));
}

/// A frame that the sprite shows already, or none below the frames, leaves the sprite as it is;
/// a building without a sprite fails.
#[test]
fn update_at_work_without_new_frame_keeps_the_sprite() {
    for timer in [0.0, 5.0] {
        let mut world = world_with(packing(working(timer)));
        world.players[1].objects[AT.1]
            .sprite
            .as_mut()
            .unwrap()
            .last_frame = Frame::of_data(3);
        assert_eq!(updated(&mut world, 0), Ok(HeadCode::Keeps));
        let sprite = trebuchet(&world).sprite.as_ref().unwrap();
        assert_eq!(
            (sprite.frame.get(), sprite.last_frame.get()),
            (9, 3),
            "{timer}"
        );
    }
    let mut bare = world_with(packing(working(5.0)));
    bare.players[1].objects[AT.1].sprite = None;
    assert!(updated(&mut bare, 2000).is_err());
}

/// The quick-build cheat's work is 1000, whatever the rate: the Pack is done at once.
#[test]
fn update_with_instant_work_reaches_the_pack_time() {
    let mut world = world_with(packing(working(0.0)));
    world.instant_work = 1;
    assert_eq!(updated(&mut world, 0), Ok(HeadCode::Deletes));
    assert_eq!(trebuchet(&world).life.master_id, ids::ID_331);
}

/// Once the timer reached the pack time the Pack is done: its unit AI takes the attack order on
/// the target first (700 at priority 0x32, the building's weapon range, at the target's place,
/// shown there, ready, queued in front), the building turns into the packed trebuchet, and its
/// action list goes with the Pack, whose destructor tells the unit AI its action completed and
/// lets the target go; done (1).
#[test]
fn update_at_pack_time_orders_the_attack_and_transforms() {
    let mut world = world_with(packing(working(49.0)));
    let first = QueuedOrder {
        priority: OrderPriority::of_data(1),
        ..QueuedOrder::default()
    };
    world.parts.unit_ai.get_mut(TREBUCHET).unwrap().orders = vec![first];
    assert_eq!(updated(&mut world, 2000), Ok(HeadCode::Deletes));
    assert_eq!(trebuchet(&world).life.master_id, ids::ID_331);
    assert_eq!(held(&world), None);
    assert!(world.parts.actions_of(TREBUCHET).list.is_empty());
    assert_eq!(heard(&world), [completed()]);
    let at = Position::new(20.0, 30.0, 0.5);
    assert_eq!(
        queued(&world),
        [
            QueuedOrder {
                issuer: Some(Issuer::Object(TREBUCHET)),
                order: OrderKind::Attack,
                priority: OrderPriority::of_data(0x32),
                target: Some(TARGET),
                target_owner: Some(PlayerId::new(2)),
                point: at,
                range: Distance::new(16.0),
                shown: Point::new(20.0, 30.0),
                ready: true,
                target_kind: Some(OrderTargetKind::of_master(ids::VILLAGER_MALE)),
            },
            first,
        ]
    );
}

/// The update writes the status 2 before the transform (`0x140ad6fb0`), whose list deletion runs
/// the destructor on the old master: the destructor leaves the building as the transform made
/// it and makes none of master 42's annexes again.
#[test]
fn update_at_pack_time_transforms_without_rebuilding_annexes() {
    let tower = annex_master(273, MasterKind::BUILDING);
    let mut annexed = trebuchet_master();
    annexed.building.as_mut().unwrap().annexes = [Some(tower.id), None, None, None];
    let mut world = world_with(packing(working(49.0)));
    world.model.masters[1] = Arc::new(Masters::new([annexed, packed_master(), tower]));
    world.players[1].attributes = vec![Amount::ZERO; 300];
    assert_eq!(updated(&mut world, 2000), Ok(HeadCode::Deletes));
    assert_eq!(trebuchet(&world).life.master_id, ids::ID_331);
    assert_eq!(
        world.parts.building_of(TREBUCHET).annexes,
        Annexes::default()
    );
    assert_eq!(world.players[1].objects.len(), 1);
    assert_eq!(heard(&world), [completed()]);
}

/// A Pack without a target turns the building without an order.
#[test]
fn update_at_pack_time_without_target_orders_nothing() {
    let mut world = world_with(packing(PackAction {
        target: None,
        ..working(49.0)
    }));
    assert_eq!(updated(&mut world, 2000), Ok(HeadCode::Deletes));
    assert_eq!(trebuchet(&world).life.master_id, ids::ID_331);
    assert_eq!(queued(&world), []);
    assert_eq!(heard(&world), [completed()]);
}

/// A building without a unit AI cannot take the attack order: the game calls through a null
/// pointer.
#[test]
fn update_at_pack_time_without_unit_ai_fails() {
    let mut world = world_with(packing(working(49.0)));
    world.parts.set_unit_ai(TREBUCHET, None);
    assert_eq!(
        updated(&mut world, 2000).unwrap_err().reason(),
        "a Pack's or Unpack's attack order through a null unit AI"
    );
}

/// A master whose transform its owner has not got, or that has none, leaves the building as it
/// is: the Pack is done (1) at its status, and the list's deletion then puts the building back.
#[test]
fn update_at_pack_time_without_transform_master_stays() {
    for transform in [Some(MasterId::named(273)), None] {
        let mut world = world_with(packing(working(49.0)));
        let masters = Arc::make_mut(&mut world.model.masters[1]);
        masters
            .get_mut(ids::ID_42)
            .unwrap()
            .building
            .as_mut()
            .unwrap()
            .transform_unit = transform;
        assert_eq!(updated(&mut world, 2000), Ok(HeadCode::Deletes));
        assert_eq!(trebuchet(&world).life.master_id, ids::ID_42);
        assert_eq!(
            held(&world),
            Some(PackAction {
                state: PackState::Done,
                ..working(58.0)
            })
        );
        assert_eq!(queued(&world), []);
    }
}

/// A building of a master without its building part has no Pack.
#[test]
fn update_of_master_without_building_part_fails() {
    let mut world = world_with(packing(working(3.0)));
    Arc::make_mut(&mut world.model.masters[1])
        .get_mut(ids::ID_42)
        .unwrap()
        .building = None;
    assert_eq!(
        updated(&mut world, 2000).unwrap_err().reason(),
        "a Pack or Unpack of a master without its building part"
    );
}

/// The list's update runs the Pack and deletes the done one: its destructor tells the unit AI
/// once the timer reached the pack time, and puts the building back at work in a played game.
#[test]
fn actions_update_deletes_done_pack_and_undoes_it() {
    let done = PackAction {
        state: PackState::Done,
        ..working(50.0)
    };
    let mut world = world_with(packing(done));
    let captured = in_update(&mut world, AT, 2000, |unit, actions, step| {
        unit.update_actions(actions, step, &roles(), slots())
    })
    .unwrap();
    assert!(captured.is_none());
    assert_eq!(held(&world), None);
    assert_eq!(heard(&world), [completed()]);
    assert_eq!(shown(&world).map(|(id, _)| id), Some(STANDING));
}

/// The list's deletion outside the step (`FUN_140a98620`, [`World::clear_actions`]) and the
/// head's from the step (`FUN_140a98e20`, [`Unit::delete_own_head`]) each run the destructor's
/// two checks: the notify once the timer reached the pack time, the standing sprite at work in a
/// played game.
#[test]
fn deleted_pack_tells_its_unit_ai_and_puts_building_back() {
    let deletions: [(&str, Deletion); 2] = [
        ("world", |world| world.clear_actions(AT.0, AT.1)),
        ("step", |world| delete_head_at(world, AT.0, AT.1)),
    ];
    for (path, delete) in deletions {
        for (timer, status, game_state, told, sprite) in [
            (
                50.0,
                PackStatus::Packing,
                PlayState::Playing,
                true,
                STANDING,
            ),
            (
                49.5,
                PackStatus::Packing,
                PlayState::Playing,
                false,
                STANDING,
            ),
            (
                f32::NAN,
                PackStatus::Packing,
                PlayState::Over,
                false,
                PACKING,
            ),
            (60.0, PackStatus::Idle, PlayState::Playing, true, PACKING),
            (
                60.0,
                PackStatus::Transformed,
                PlayState::Playing,
                true,
                PACKING,
            ),
        ] {
            let mut world = world_with(packing(PackAction {
                status,
                ..working(timer)
            }));
            world.game_state = game_state;
            delete(&mut world).unwrap();
            assert_eq!(held(&world), None, "{path}");
            let notifies = if told { vec![completed()] } else { Vec::new() };
            assert_eq!(heard(&world), notifies, "{path} {timer} {status:?}");
            let shown_id = shown(&world).map(|(id, _)| id);
            assert_eq!(shown_id, Some(sprite), "{path} {timer} {status:?}");
        }
    }
}

/// A Pack under a deleted head restarts as it was (slot 0x28 with 0 does nothing): its unit AI
/// hears nothing and the building keeps its sprite.
#[test]
fn pack_under_deleted_head_restarts_as_it_was() {
    let mut under = world_with(wait_over(packing(working(50.0))));
    delete_head_at(&mut under, AT.0, AT.1).unwrap();
    assert_eq!(held(&under), Some(working(50.0)));
    assert_eq!(heard(&under), []);
    assert_eq!(shown(&under), Some((PACKING, 9)));
}

/// A building without a unit AI hears nothing of its Pack's deletion, from the world and from
/// its step.
#[test]
fn deleted_pack_without_unit_ai_tells_nothing() {
    let mut world = world_with(packing(working(50.0)));
    world.parts.set_unit_ai(TREBUCHET, None);
    world.clear_actions(AT.0, AT.1).unwrap();
    assert_eq!(shown(&world).map(|(id, _)| id), Some(STANDING));
    let mut stepped = world_with(packing(working(50.0)));
    stepped.parts.set_unit_ai(TREBUCHET, None);
    delete_head_at(&mut stepped, AT.0, AT.1).unwrap();
    assert_eq!(held(&stepped), None);
}

/// The destructor (slot 0) hands the Pack over and lets its target go; its teardown outside a
/// unit's own deletion is refused.
#[test]
fn torn_pack_hands_itself_over_and_releases_its_target() {
    let mut world = world_with(ActionList::default());
    let pack = working(3.0);
    let torn = ListAction::from(pack)
        .tear_down(world.parts.motion_mut(TREBUCHET), Speed::ZERO)
        .unwrap();
    assert_eq!(
        torn,
        TornDown {
            releases: vec![TARGET],
            pack: Some(pack),
            ..TornDown::default()
        }
    );
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

/// A building whose master names annexes makes each again at its slot's place off its own,
/// links it in that slot and gives it its hit points; a building annex takes it as its main
/// building, an annex of another type holds no main.
#[test]
fn undone_pack_rebuilds_annexes() {
    let (tower, soldier) = (
        annex_master(273, MasterKind::BUILDING),
        annex_master(379, MasterKind::ANIMATED),
    );
    let mut annexed = trebuchet_master();
    let building = annexed.building.as_mut().unwrap();
    building.annexes = [Some(soldier.id), None, Some(tower.id), None];
    building.annex_offsets[0] = GroundOffset::new(-1.0, 0.5);
    building.annex_offsets[2] = GroundOffset::new(1.5, -2.0);
    let mut world = world_with(packing(working(3.0)));
    world.model.masters[1] = Arc::new(Masters::new([annexed, packed_master(), tower, soldier]));
    world.players[1].attributes = vec![Amount::ZERO; 300];
    world.clear_actions(AT.0, AT.1).unwrap();
    let annexes = world.parts.building_of(TREBUCHET).annexes;
    let [first, third] = annexes.ids().collect::<Vec<_>>()[..] else {
        panic!("two annexes: {annexes:?}");
    };
    assert_eq!(
        annexes,
        Annexes::new([Some(first), None, Some(third), None])
    );
    let made = |id| {
        let annex = world.players[1].objects.by_id(id).unwrap();
        (
            annex.position(),
            annex.life.hp,
            annex.life.master_id,
            world.parts.building_of(id).annex_of.map(RoleId::id),
        )
    };
    assert_eq!(
        made(first),
        (
            Position::new(9.0, 10.5, 0.0),
            HitPoints::new(120.0),
            MasterId::named(379),
            None
        )
    );
    assert_eq!(
        made(third),
        (
            Position::new(11.5, 8.0, 0.0),
            HitPoints::new(120.0),
            MasterId::named(273),
            Some(TREBUCHET)
        )
    );
    assert_eq!(heard(&world), []);
}

/// The object stop keeps a top Pack's share done in `+0x88a` and ends the Pack, which the list
/// keeps; under another action, or none, it writes 0. A unit AI's stop then deletes the list: at
/// work the Pack's destructor puts the building back.
#[test]
fn stop_keeps_pack_progress() {
    let mut world = world_with(packing(working(25.0)));
    world.parts.set_unit_ai(TREBUCHET, None);
    world.parts.actions_mut(TREBUCHET).unwrap().pack_progress = PackProgress::of_data(7);
    world.object_stop(AT.0, AT.1).unwrap();
    let actions = world.parts.actions_of(TREBUCHET);
    assert_eq!(actions.pack_progress, PackProgress::of_data(50));
    assert_eq!(
        held(&world),
        Some(PackAction {
            state: PackState::Done,
            ..working(25.0)
        })
    );
    world.object_stop(AT.0, AT.1).unwrap();
    assert_eq!(
        world.parts.actions_of(TREBUCHET).pack_progress,
        PackProgress::of_data(100)
    );
    for list in [wait_over(ActionList::default()), ActionList::default()] {
        let mut other = world_with(list);
        other.parts.actions_mut(TREBUCHET).unwrap().pack_progress = PackProgress::of_data(7);
        other.object_stop(AT.0, AT.1).unwrap();
        let actions = other.parts.actions_of(TREBUCHET);
        assert_eq!(actions.pack_progress, PackProgress::NONE);
        assert!(actions.list.is_empty());
    }
    let mut stopped = world_with(packing(working(25.0)));
    stopped.object_stop(AT.0, AT.1).unwrap();
    assert_eq!(
        stopped.parts.actions_of(TREBUCHET).pack_progress,
        PackProgress::of_data(50)
    );
    assert_eq!(held(&stopped), None);
    assert_eq!(shown(&stopped).map(|(id, _)| id), Some(STANDING));
}

/// `FUN_141227190`'s first test (`0x141227242`): a class-54 building under its Pack ignores a
/// work order. Without the Pack the order goes on, as it does for a class-51 building under a
/// Pack: to the reach against a target of a player it is not allied with, which the trebuchet,
/// placed on no map, has not got.
#[test]
fn work_order_on_class_54_under_its_pack_is_ignored() {
    let work = Work::on(Some(TARGET), Position::new(20.0, 30.0, 0.5));
    let mut world = world_with(packing(working(3.0)));
    assert_eq!(world.order_work(AT.0, AT.1, &work), Ok(()));
    assert_eq!(held(&world), Some(working(3.0)));
    let bare = world_with(ActionList::default());
    let mut packed = world_with(packing(working(3.0)));
    packed.players[1].objects[AT.1].life.master_id = ids::ID_331;
    for mut on in [bare, packed] {
        assert_eq!(
            on.order_work(AT.0, AT.1, &work).unwrap_err().reason(),
            "an ordered unit without a place"
        );
    }
}
