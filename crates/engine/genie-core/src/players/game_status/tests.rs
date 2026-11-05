use super::*;
use crate::ErrorKind;
use crate::combat::attack::AttackState;
use crate::combat::attack::tests::own_attack;
use crate::commands::Departure;
use crate::construction::build::BuildState;
use crate::construction::build::BuildTask;
use crate::construction::build::tests::started_on;
use crate::construction::repair::REPAIR;
use crate::construction::repair::RepairState;
use crate::construction::repair::{RepairAction, RepairTask};
use crate::gathering::PastureId;
use crate::gathering::gather::GatherState;
use crate::gathering::gather::{GatherAction, Resource, StopRate};
use crate::gathering::hunt::HUNT;
use crate::gathering::hunt::HuntState;
use crate::gathering::hunt::HuntTask;
use crate::gathering::hunt::tests::walker;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Point;
use crate::map::Position;
use crate::movement::Speed;
use crate::movement::move_to::{MoveDefaults, MoveToReads, NO_TARGET_GAP, SpriteReads, WalkTo};
use crate::movement::move_to::{MoveTo, MoveToAction};
use crate::objects::Graphics;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::SpeedMultiplier;
use crate::objects::SpriteFlags;
use crate::objects::SpriteId;
use crate::objects::WorkRate;
use crate::objects::actions::Action as ActionsAction;
use crate::objects::actions::ActionList;
use crate::objects::layers::{Action, Motion};
use crate::objects::masters::CostAmount;
use crate::objects::masters::CreatablePart;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::TaskId;
use crate::objects::masters::TaskSprites;
use crate::objects::masters::WorkValue;
use crate::objects::masters::{Master, Masters};
use crate::objects::sprite::Graphic;
use crate::objects::unit::Unit;
use crate::objects::{FacetCount, Frame, WorkerCount};
use crate::ownership::artifact::{Artifact, ArtifactState};
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::players::Relation;
use crate::players::attribute::AttributeId;
use crate::players::game_status::stops::StopList;
use crate::players::history::{Event, EventKind, HistoryEntry, Population, TimeSlice};
use crate::players::relation_row;
use crate::production::Batch;
use crate::production::LocationButton;
use crate::production::QueueButton;
use crate::production::QueuedUnits;
use crate::production::UnitIndex;
use crate::production::{
    Entry, MakeObj, MakeObjState, MakeTech, MakeTechState, Making, Production, Queue,
};
use crate::production::{EntrySerial, QueueChanges, QueueLength};
use crate::technology::CostPaid;
use crate::technology::ResearchState;
use crate::technology::{CostOverride, ResearchCap, ResearchCount};
use crate::test_ids::costs;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::test_ids::object_ids;
use crate::test_ids::player_id;
use crate::test_ids::task_id;
use crate::test_ids::task_key;
use crate::test_ids::tech_id;
use crate::test_ids::{adds, amounts};
use crate::testing::attribute;
use crate::testing::class;
use crate::testing::ids;
use crate::testing::roles;
use crate::testing::tuning;
use crate::unit_ai::AiAction;
use crate::unit_ai::OrderKind;
use crate::unit_ai::UnitAi;
use crate::world::Departures;
use crate::world::Millis;
use crate::world::Player;
use crate::world::Seconds;
use crate::world::WholeSeconds;
use crate::world::parts::Object;
use crate::world::parts::Part;
use crate::world::settings::FarmMode;
use crate::world::{FrameSeed, GameMode, PlayState};
use std::sync::Arc;

/// An entry of `count` villagers of master `master` that paid `food` each.
fn villager(master: Option<MasterId>, count: i16, food: f32) -> Entry {
    Entry {
        master,
        count: QueuedUnits::of_data(count),
        tech: None,
        costs: [
            (Some(attribute::FOOD), Amount::new(food)),
            (None, Amount::new(0.0)),
            (Some(attribute::HEADROOM), Amount::new(1.0)),
        ],
        serial: EntrySerial::new(0),
        button: QueueButton::NONE,
        train_time: WholeSeconds::of_data(25),
    }
}

/// A queue of ten slots holding `entries`.
fn queue(entries: &[Entry]) -> Queue {
    let mut slots = vec![Entry::default(); 10];
    slots[..entries.len()].copy_from_slice(entries);
    Queue {
        slots,
        count: QueueLength::of_data(entries.len() as i16),
        total: entries.iter().map(|entry| entry.count).sum(),
        next_serial: EntrySerial::new(10),
        ..Queue::default()
    }
}

/// Gaia and two players; player 1's town centre (class 3) makes a villager its queue paid
/// 46 food for, a minute and more into the game.
fn game() -> World {
    let mut attributes = vec![Amount::ZERO; 250];
    (attributes[80], attributes[81]) = (Amount::new(1.0), Amount::new(1.0));
    let tc = Unit {
        id: object_id(9),
        ..Unit::default()
    };
    let production = Production {
        queue: queue(&[villager(Some(ids::VILLAGER_MALE), 1, 46.0)]),
        action: Some(Making::Unit(MakeObj {
            state: MakeObjState::Making,
            progress: Seconds::new(3.0),
            ..MakeObj::for_entry(&villager(Some(ids::VILLAGER_MALE), 1, 46.0))
        })),
        ..Production::default()
    };
    let player = |objects| Player {
        objects,
        attributes: attributes.clone(),
        relations: relation_row([0, 0, 3]),
        ..Player::default()
    };
    let mut world = World::new(
        WorldTime::from_millis(61_000),
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO; 3],
        vec![
            Player::default(),
            player(vec![tc].into()),
            player(vec![].into()),
        ],
    )
    .with_ais([(
        object_id(9),
        UnitAi {
            current_order: None,
            current_action: Some(AiAction::Make),
            current_target: None,
            ..UnitAi::default()
        },
    )]);
    let masters = Masters::new([
        Master {
            id: ids::TOWN_CENTRE,
            class: class::BUILDING,
            kind: MasterKind::BUILDING,
            ..Master::default()
        },
        Master {
            id: ids::VILLAGER_MALE,
            class: class::VILLAGER,
            kind: MasterKind::COMBAT,
            storage: vec![
                (Some(attribute::HEADROOM), Amount::new(-1.0), 2),
                (Some(attribute::POPULATION), Amount::new(1.0), 2),
            ],
            creatable: Some(CreatablePart {
                cost: costs(&[(attribute::FOOD, 46, true), (attribute::HEADROOM, 1, false)]),
                ..CreatablePart::default()
            }),
            ..Master::default()
        },
    ]);
    world.players[1].objects[ListIndex::new(0)].life.master_id = ids::TOWN_CENTRE;
    world
        .set_part(object_id(9), Part::Production(Some(Box::new(production))))
        .unwrap();
    world.model.masters = vec![
        Arc::new(masters.clone()),
        Arc::new(masters.clone()),
        Arc::new(masters),
    ];
    world.victory_conquest = true;
    world
}

/// The action object's stop (`FUN_14106fa60`) keeps the action list when the top action's
/// slot 0x38 answers true and the unit is no trade cart with goods (`FUN_14106a940`): a
/// human hunter's Hunt halts (state 2) and stays. A unit AI then stops (slot 0xb0) as the
/// top action is no trade (0x6f), and its stopAction empties the list.
/// A defeat (`FUN_141639860`) adds its event to the history: the player with the most of
/// its own attribute 300 + id, Gaia and the defeated player left out; a resigned player and
/// a history without entries get none.
#[test]
fn defeat_names_the_player_with_the_most_of_its_own_attribute() {
    let mut world = game();
    let other = world.players[1].clone();
    world.players.push(other);
    world.reindex();
    world.players[2].attributes.resize(310, Amount::ZERO);
    world.players[3].attributes.resize(310, Amount::ZERO);
    world.players[1].attributes.resize(310, Amount::ZERO);
    world.players[0].attributes = vec![Amount::new(9.0); 310];
    world.players[2].attributes[302] = Amount::new(3.0);
    world.players[3].attributes[303] = Amount::new(7.9);
    world.players[1].attributes[301] = Amount::new(50.0);
    world.players[1].history.entries = vec![
        HistoryEntry {
            civilian: Population::of_data(1),
            military: Population::ZERO,
        };
        4
    ];
    world.world_time = WorldTime::from_millis(5_000);
    world.defeat_event(PlayerId::new(1)).unwrap();
    assert_eq!(
        world.players[1].history.events,
        [Event {
            kind: EventKind::Defeat,
            time_slice: TimeSlice::of_data(4),
            world_time: WorldTime::from_millis(5_000),
            data1: 3.0,
            data2: 7.0,
            data3: 0.0,
        }]
    );
    world.players[2].departures.set(Departures::RESIGNED, true);
    world.players[2].history.entries = vec![HistoryEntry {
        civilian: Population::of_data(1),
        military: Population::ZERO,
    }];
    world.defeat_event(PlayerId::new(2)).unwrap();
    assert_eq!(world.players[2].history.events, []);
}

/// Slot 0x38 of the actions with their own ([`World::halt_other`]), each true: the Gather
/// idles (state 2), the Build and the Repair halt (state 2), the Attack ends (state 1, its
/// animation at speed 1, standing) and the Artifact checks again (state 6); a kind whose stop
/// [`World::halt_top_action`] answers itself is an error (the Transport's is
/// `garrison::tests::stopped_transport_keeps_its_idle_transport`).
#[test]
fn each_action_with_its_own_stop_halts_and_answers_true() {
    let mut game = game();
    let mut add = |object: Object| {
        let mut object = object;
        object.unit.life.master_id = ids::TOWN_CENTRE;
        let _ = game.push_live(PlayerId::new(1), object).unwrap();
        game.players[1].objects.last_place().unwrap()
    };
    let task = HuntTask {
        id: task_id(0),
        key: task_key(HUNT),
        range: Distance::new(0.25),
        resource: Some(AttributeId::named(17)),
        sprites: TaskSprites::NONE,
        work_value: WorkValue::new(1.0),
        productivity: None,
        kind4: None,
    };
    let bush = Resource {
        id: ObjectId::NOBODY,
        position: Position::new(3.5, 3.5, 0.0),
        class: class::CLASS_7,
        master: master_id(59),
        player: PlayerId::new(0),
        workers: WorkerCount::of_data(0),
        drop_site: false,
        gap: Distance::new(1.2),
        state: ObjectState::Alive,
        radius: Footprint::new(0.5, 0.5),
    };
    let rate = StopRate {
        work_rate: WorkRate::new(1.0),
        capacity: Amount::new(10.0),
        productivity: Amount::new(1.0),
    };
    let gather = add(Object::with_actions(
        Unit {
            id: object_id(30),
            ..Unit::default()
        },
        Action {
            list: ActionList::default().with_gather(Some(
                GatherAction::new(task, &bush, FarmMode::Fixed, rate, &roles()).unwrap(),
            )),
            ..Default::default()
        },
    ));
    let build_task = BuildTask {
        range: Distance::ZERO,
        work_value: WorkValue::new(1.0),
        work_rate: WorkRate::new(1.0),
        resource: None,
        id: TaskId::of_task(-1),
        key: task_key(ActionKind::BUILD),
        sprites: TaskSprites::NONE,
    };
    let build = add(Object::with_actions(
        Unit {
            id: object_id(31),
            ..Unit::default()
        },
        Action {
            list: ActionList::default().with_build(Some(started_on(
                None,
                Position::new(3.5, 3.5, 0.0),
                Distance::new(1.0),
                build_task,
            ))),
            ..Default::default()
        },
    ));
    let repair_task = RepairTask {
        id: task_id(0),
        key: task_key(REPAIR),
        work_value: WorkValue::new(1.0),
        sprites: TaskSprites::NONE,
    };
    let repair = add(Object::with_actions(
        Unit {
            id: object_id(32),
            ..Unit::default()
        },
        Action {
            list: ActionList::default().with_repair(Some(
                RepairAction::start(
                    WalkTo {
                        id: None,
                        at: Position::new(3.5, 3.5, 0.0),
                        gap: Distance::new(NO_TARGET_GAP),
                        state: None,
                    },
                    repair_task,
                    Distance::ZERO,
                    &MoveToReads {
                        sprites: SpriteReads {
                            defaults: MoveDefaults::default(),
                            graphics: &Graphics::new(),
                        },
                        state: ObjectState::Alive,
                        speed: Speed::new(1.0),
                        at: Position::new(3.5, 3.5, 0.0),
                        ground: Footprint::new(0.2, 0.2),
                    },
                    &tuning().move_to,
                )
                .unwrap(),
            )),
            ..Default::default()
        },
    ));
    let mut attack = own_attack(-1, AttackState::Done, Distance::new(1.0));
    attack.arms.idle_sprite = Some(Graphic {
        id: SpriteId::named(1),
        frames: Frame::of_data(10),
        facets: FacetCount::of_data(8),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::NONE,
    });
    let attack = add(Object::with_actions(
        Unit {
            id: object_id(33),
            ..Unit::default()
        },
        Action {
            list: ActionList::default().with_attack(Some(attack)),
            ..Default::default()
        },
    ));
    let artifact = add(Object::with_actions(
        Unit {
            id: object_id(34),
            ..Unit::default()
        },
        Action {
            list: ActionList::default().with_artifact(Some(Artifact::new(PlayerId::new(1), None))),
            ..Default::default()
        },
    ));
    game.set_motion(object_id(33), Some(Motion::Moving(walker())));
    let head = |world: &World, list_index: ListIndex| {
        world
            .parts
            .actions_of(world.players[1].objects[list_index].id)
            .list
            .head()
            .cloned()
            .unwrap()
    };
    for list_index in [gather, build, repair, attack, artifact] {
        let top = head(&game, list_index);
        assert_eq!(
            game.halt_other(PlayerId::new(1), list_index, top),
            Ok(StopList::Keeps)
        );
    }
    let objects = &game.players[1].objects;
    assert_eq!(
        game.parts
            .actions_of(objects[gather].id)
            .list
            .gather()
            .map(|gather_action| gather_action.state),
        Some(GatherState::Idle)
    );
    assert_eq!(
        game.parts
            .actions_of(objects[build].id)
            .list
            .build()
            .map(|build_action| build_action.state),
        Some(BuildState::Halted)
    );
    assert_eq!(
        game.parts
            .actions_of(objects[repair].id)
            .list
            .repair()
            .map(|repair_action| repair_action.state),
        Some(RepairState::Halted)
    );
    let unit = &objects[attack];
    assert_eq!(
        game.parts
            .actions_of(unit.id)
            .list
            .attack()
            .map(|attack| attack.state),
        Some(AttackState::Done)
    );
    assert_eq!(game.parts.anim_speed_of(unit.id), Some(Speed::new(1.0)));
    assert!(unit.sprite.is_some());
    assert_eq!(
        game.parts
            .actions_of(objects[artifact].id)
            .list
            .artifact()
            .map(|artifact| artifact.state),
        Some(ArtifactState::Check)
    );
    // a kind whose stop halt_top_action answers itself never reaches it
    let walk = ActionsAction::MoveTo(MoveToAction::sub_in(MoveTo::Moving));
    assert_eq!(
        game.halt_other(PlayerId::new(1), gather, walk)
            .map_err(|error| error.kind()),
        Err(ErrorKind::Unreachable)
    );
}

#[test]
fn stopped_hunter_keeps_its_halted_hunt() {
    use crate::gathering::hunt::{HuntAction, HuntTask, Lead};
    let hunter = Object::with_actions(
        Unit {
            id: object_id(20),
            ..Unit::default()
        },
        Action {
            list: ActionList::default().with_hunt(Some(HuntAction {
                target: None,
                state: HuntState::Walk { move_to: None },
                point: Position::from([-1.0; 3]),
                target_class: None,
                task: HuntTask {
                    id: task_id(0),
                    key: task_key(HUNT),
                    range: Distance::ZERO,
                    resource: None,
                    sprites: TaskSprites::NONE,
                    work_value: WorkValue::new(0.0),
                    productivity: None,
                    kind4: None,
                },
                drop_site: None,
                carry_to: None,
                timer: Seconds::ZERO,
                lead: Lead {
                    moved: true,
                    still_time: 2,
                    start: Point::new(3.0, 4.0),
                },
            })),
            ..Default::default()
        },
    );
    let mut world = game();
    let mut hunter = hunter;
    // game()'s villager master
    hunter.unit.life.master_id = ids::TOWN_CENTRE;
    hunter.unit.my_pasture = None;
    let ai = UnitAi {
        current_order: Some(OrderKind::Hunt),
        current_action: Some(AiAction::Hunt),
        current_target: None,
        ..UnitAi::default()
    };
    let _ = world
        .push_live(PlayerId::new(1), Object::with_ai(hunter, ai))
        .unwrap();
    let idx = world.players[1].objects.last_place().unwrap();
    // a computer player's stopped hunt (set_state 1) and a stopped hunter that tends a pasture
    // are not modelled
    let mut computer = world.clone();
    computer.players[1].kind = PlayerKind::Computer;
    assert!(computer.object_stop(PlayerId::new(1), idx).is_err());
    let mut tending = world.clone();
    tending.players[1].objects[idx].my_pasture = Some(PastureId::new(object_id(50)));
    assert!(tending.object_stop(PlayerId::new(1), idx).is_err());
    let mut without_ai = world.clone();
    // the unit AI's stopAction (slot 0x3c0) then empties the list
    world.object_stop(PlayerId::new(1), idx).unwrap();
    let players = &world.players[1].objects[idx];
    assert_eq!(
        world.parts.actions_of(players.id).list.hunt().copied(),
        None
    );
    assert_eq!(
        world.parts.unit_ai.get(players.id).unwrap().current_action,
        None
    );
    // without a unit AI the halted Hunt stays
    without_ai
        .parts
        .set_unit_ai(without_ai.players[1].objects[idx].id, None);
    without_ai.object_stop(PlayerId::new(1), idx).unwrap();
    let unit = &without_ai.players[1].objects[idx];
    assert_eq!(
        without_ai
            .parts
            .actions_of(unit.id)
            .list
            .hunt()
            .map(|hunt_action| hunt_action.state),
        Some(HuntState::Idle)
    );
    // its target went through slot 0x68 (FUN_140ac7980), which clears the lead
    assert_eq!(
        without_ai
            .parts
            .actions_of(unit.id)
            .list
            .hunt()
            .map(|hunt_action| hunt_action.lead),
        Some(Lead::NEW)
    );
}

/// records/425906646's player 2 cancels the second villager of its town centre at update
/// 3699 (`UNIT_ORDER` action 4, index 1): that one's 46 food come back and its population
/// leaves attribute 80; the first keeps its make action. A cancel of the first drops the make
/// action too; an index past the queue does nothing.
#[test]
fn cancel_gives_back_one_queued_unit() {
    let mut world = game();
    let tc = world
        .parts
        .production
        .get_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap();
    tc.queue = queue(&[
        villager(Some(ids::VILLAGER_MALE), 1, 46.0),
        villager(Some(ids::VILLAGER_MALE), 1, 45.0),
    ]);
    world.players[1].attributes[80] = Amount::new(2.0);
    world
        .cancel_order(&object_ids([9]), UnitIndex::of_data(1), Batch::One)
        .unwrap();
    let players = &world.players[1];
    assert_eq!(
        (players.attributes[0], players.attributes[80]),
        (Amount::new(45.0), Amount::new(1.0))
    );
    let production = world
        .parts
        .production
        .get(players.objects[ListIndex::new(0)].id)
        .unwrap();
    let queue = &production.queue;
    assert_eq!(
        (queue.count.get(), queue.total.get(), queue.changes.get()),
        (1, 1, 1)
    );
    assert!(production.action.is_some());
    assert_eq!(players.tracker.queued, -1);
    world
        .cancel_order(&object_ids([9]), UnitIndex::of_data(5), Batch::One)
        .unwrap();
    assert_eq!(world.players[1].attributes[0], Amount::new(45.0));
    world
        .cancel_order(&object_ids([9]), UnitIndex::of_data(0), Batch::One)
        .unwrap();
    let player = &world.players[1];
    assert_eq!(
        (
            player.attributes[0],
            player.attributes[80],
            player.attributes[81]
        ),
        (Amount::new(91.0), Amount::ZERO, Amount::ZERO)
    );
    let production = world
        .parts
        .production
        .get(player.objects[ListIndex::new(0)].id)
        .unwrap();
    assert!(production.queue.entries().is_empty() && production.action.is_none());
    assert!(
        world
            .cancel_order(&object_ids([9]), UnitIndex::of_data(0), Batch::Five)
            .is_ok()
    );
}

/// `TRIBE_Building_Object::update` after its combat update (`0x1411ca3e2`): a building past
/// state 2 cancels its whole queue (`FUN_1411cff00`): 429063959's player 3 deleted its town
/// centre with four villagers queued at update 550, and the game gave their 200 food back as
/// the town centre died. Each later update counts the queue's changes on; a building in state
/// 2 keeps its queue for its production step.
#[test]
fn dying_building_cancels_its_queue_in_its_update() {
    let mut world = game();
    let tc = world
        .parts
        .production
        .get_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap();
    tc.queue = queue(&[villager(Some(ids::VILLAGER_MALE), 4, 50.0)]);
    world.players[1].attributes[80] = Amount::new(4.0);
    world.players[1].objects[ListIndex::new(0)].life.state = ObjectState::Dying;
    world
        .building_production(PlayerId::new(1), ListIndex::new(0), Seconds::new(0.05))
        .unwrap();
    let players = &world.players[1];
    assert_eq!(
        (players.attributes[0], players.attributes[80]),
        (Amount::new(200.0), Amount::ZERO)
    );
    let production = &world
        .parts
        .production
        .get(players.objects[ListIndex::new(0)].id)
        .unwrap()
        .queue;
    assert_eq!(
        (
            production.count.get(),
            production.total.get(),
            production.changes.get()
        ),
        (0, 0, 1)
    );
    world
        .building_production(PlayerId::new(1), ListIndex::new(0), Seconds::new(0.05))
        .unwrap();
    let production_queue = &world
        .parts
        .production
        .get(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .queue;
    assert_eq!(
        (production_queue.count.get(), production_queue.changes.get()),
        (0, 2)
    );
    let mut world = game();
    let tc = world
        .parts
        .production
        .get_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap();
    tc.queue = queue(&[villager(Some(ids::VILLAGER_MALE), 4, 50.0)]);
    world.players[1].attributes[80] = Amount::new(4.0);
    world.players[1].objects[ListIndex::new(0)].life.state = ObjectState::Alive;
    // a disabled queue (+0xbf6): the production step does nothing
    world
        .parts
        .production
        .get_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .queue
        .enabled = false;
    world
        .building_production(PlayerId::new(1), ListIndex::new(0), Seconds::new(0.05))
        .unwrap();
    let player = &world.players[1];
    assert_eq!(
        world
            .parts
            .production
            .get(player.objects[ListIndex::new(0)].id)
            .unwrap()
            .queue
            .count,
        QueueLength::of_data(1)
    );
    assert_eq!(player.attributes[0], Amount::ZERO);
}

/// One unit of an entry of three goes: the entry keeps two, and the first unit's cancel
/// drops the make action though the entry stays (`FUN_1411d66b0`).
#[test]
fn cancel_of_the_first_unit_drops_the_make_action_of_an_entry_that_stays() {
    let mut world = game();
    let tc = world
        .parts
        .production
        .get_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap();
    tc.queue = queue(&[villager(Some(ids::VILLAGER_MALE), 3, 46.0)]);
    world.players[1].attributes[80] = Amount::new(3.0);
    world
        .cancel_queued(
            PlayerId::new(1),
            ListIndex::new(0),
            UnitIndex::of_data(0),
            Batch::One,
        )
        .unwrap();
    let player = &world.players[1];
    let production = world
        .parts
        .production
        .get(player.objects[ListIndex::new(0)].id)
        .unwrap();
    assert_eq!(
        (
            production.queue.count.get(),
            production.queue.slots[0].count.get()
        ),
        (1, 2)
    );
    assert!(production.action.is_none());
    assert_eq!(
        (player.attributes[80], player.attributes[81]),
        (Amount::new(2.0), Amount::ZERO)
    );
}

/// `FUN_1411d66b0` with `all` on the first villager of [3 male, 4 female]: the three go
/// (`FUN_1411d61e0`), the first entry with them, and the make action with it (the cancel
/// started at unit 0); the female entry moved to the first slot gives two more, five in
/// all. Each unit gives its 46 food back and leaves attribute 80; the tracker's counter
/// loses two per unit (the game counts a cancel of several twice).
#[test]
fn cancel_of_all_takes_five_villagers_of_both_sexes() {
    let mut world = game();
    let tc = world
        .parts
        .production
        .get_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap();
    tc.queue = queue(&[
        villager(Some(ids::VILLAGER_MALE), 3, 46.0),
        villager(Some(ids::VILLAGER_FEMALE), 4, 46.0),
    ]);
    world.players[1].attributes[80] = Amount::new(7.0);
    world.model.masters[1] = Arc::new(Masters::new([
        world
            .master(PlayerId::new(1), ids::TOWN_CENTRE)
            .unwrap()
            .clone(),
        world
            .master(PlayerId::new(1), ids::VILLAGER_MALE)
            .unwrap()
            .clone(),
        Master {
            id: ids::VILLAGER_FEMALE,
            ..world
                .master(PlayerId::new(1), ids::VILLAGER_MALE)
                .unwrap()
                .clone()
        },
    ]));
    world
        .cancel_order(&object_ids([9]), UnitIndex::of_data(0), Batch::Five)
        .unwrap();
    let player = &world.players[1];
    assert_eq!(
        (player.attributes[0], player.attributes[80]),
        (Amount::new(5.0 * 46.0), Amount::new(2.0))
    );
    let production = world
        .parts
        .production
        .get(player.objects[ListIndex::new(0)].id)
        .unwrap();
    let production_queue = &production.queue;
    assert_eq!(
        (
            production_queue.count.get(),
            production_queue.total.get(),
            production_queue.slots[0].master,
            production_queue.slots[0].count.get()
        ),
        (1, 2, Some(master_id(293)), 2)
    );
    assert_eq!(production_queue.changes, QueueChanges::of_data(2));
    assert!(production.action.is_none());
    assert_eq!(player.tracker.queued, -10);
    // from the second unit the make action goes on for the other sex
    let mut t2 = game();
    t2.model.masters = world.model.masters.clone();
    let tc = t2
        .parts
        .production
        .get_mut(t2.players[1].objects[ListIndex::new(0)].id)
        .unwrap();
    tc.queue = queue(&[
        villager(Some(ids::VILLAGER_MALE), 3, 46.0),
        villager(Some(ids::VILLAGER_FEMALE), 4, 46.0),
    ]);
    t2.players[1].attributes[80] = Amount::new(7.0);
    t2.cancel_order(&object_ids([9]), UnitIndex::of_data(1), Batch::Five)
        .unwrap();
    let production = t2
        .parts
        .production
        .get(t2.players[1].objects[ListIndex::new(0)].id)
        .unwrap();
    assert_eq!(
        (
            production.queue.count.get(),
            production.queue.slots[0].count.get()
        ),
        (1, 2)
    );
    assert!(production.action.is_some());
}

/// The kind is the first entry's master or a villager's: all of two archers takes the
/// villagers after them too; a research shifted into the slot ends the loop.
#[test]
fn cancel_of_all_goes_on_into_villagers_and_stops_at_research() {
    let archer = |count| Entry {
        master: Some(master_id(4)),
        costs: [
            (Some(attribute::WOOD), Amount::new(25.0)),
            (Some(attribute::GOLD), Amount::new(45.0)),
            (Some(attribute::HEADROOM), Amount::new(1.0)),
        ],
        ..villager(Some(master_id(4)), count, 0.0)
    };
    let mut world = game();
    let ms = &mut world.model.masters[1];
    *ms = Arc::new(Masters::new(ms.iter().cloned().chain([Master {
        id: master_id(4),
        kind: MasterKind::COMBAT,
        storage: vec![(Some(attribute::HEADROOM), Amount::new(-1.0), 2)],
        creatable: Some(CreatablePart {
            cost: costs(&[
                (attribute::WOOD, 25, true),
                (attribute::GOLD, 45, true),
                (attribute::HEADROOM, 1, false),
            ]),
            ..CreatablePart::default()
        }),
        ..Master::default()
    }])));
    world.players[1].attributes[80] = Amount::new(4.0);
    let tc = world
        .parts
        .production
        .get_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap();
    tc.queue = queue(&[archer(2), villager(Some(ids::VILLAGER_MALE), 2, 46.0)]);
    world
        .cancel_queued(
            PlayerId::new(1),
            ListIndex::new(0),
            UnitIndex::of_data(0),
            Batch::Five,
        )
        .unwrap();
    let player = &world.players[1];
    assert_eq!(player.resources(), amounts([92.0, 50.0, 0.0, 90.0]));
    assert_eq!(player.attributes[80], Amount::ZERO);
    assert_eq!(
        world
            .parts
            .production
            .get(player.objects[ListIndex::new(0)].id)
            .unwrap()
            .queue
            .count,
        QueueLength::of_data(0)
    );
    // [2 villagers, a research]: the research moved into the slot ends it
    let mut world = game();
    let research = Entry {
        master: None,
        tech: Some(tech_id(22)),
        costs: [
            (Some(attribute::GOLD), Amount::new(50.0)),
            (None, Amount::new(0.0)),
            (None, Amount::new(0.0)),
        ],
        ..villager(None, 1, 0.0)
    };
    let tc = world
        .parts
        .production
        .get_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap();
    tc.queue = queue(&[villager(Some(ids::VILLAGER_MALE), 2, 46.0), research]);
    world.players[1].attributes[80] = Amount::new(2.0);
    world
        .cancel_queued(
            PlayerId::new(1),
            ListIndex::new(0),
            UnitIndex::of_data(0),
            Batch::Five,
        )
        .unwrap();
    let queue = &world
        .parts
        .production
        .get(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .queue;
    assert_eq!(
        (
            queue.count.get(),
            queue.slots[0].tech,
            world.players[1].attributes[0]
        ),
        (1, Some(tech_id(22)), Amount::new(92.0))
    );
}

/// Player 1's town centre researching technology 22 three times over (one entry), the
/// research running with `progress` done: each paid 50 gold, and the `.dat` lists 20 wood
/// the building pays (flag 2).
fn researching(progress: f32) -> World {
    use crate::technology::{Cost, Location, TechDef, TechState};
    let mut world = game();
    let entry = Entry {
        master: None,
        tech: Some(tech_id(22)),
        costs: [
            (Some(attribute::GOLD), Amount::new(50.0)),
            (None, Amount::new(0.0)),
            (None, Amount::new(0.0)),
        ],
        ..villager(None, 3, 0.0)
    };
    let tc = world
        .parts
        .production
        .get_mut(world.players[1].objects[ListIndex::new(0)].id)
        .unwrap();
    tc.queue = queue(&[entry]);
    tc.action = Some(Making::Research(MakeTech {
        state: MakeTechState::Researching,
        paid: true,
        tech: tech_id(22),
        master: -1,
        progress: Seconds::new(progress),
    }));
    tc.disabled_techs = vec![tech_id(22), tech_id(5)];
    let mut techs = vec![TechState::default(); 23];
    techs[22] = TechState {
        state: ResearchState::Researching,
        allow_multiple: true,
        pending: ResearchCount::of_data(3),
        research_done: Seconds::new(progress),
        research_cap: ResearchCap::of_data(-1),
        overrides: [CostOverride::UNSET; 3],
        locations: vec![Location {
            building: Some(ids::TOWN_CENTRE),
            time: WholeSeconds::of_data(25),
            button: LocationButton::of_data(1),
            adjustment: Seconds::ZERO,
        }],
        ..TechState::default()
    };
    world.players[1].techs = techs;
    let mut defs = vec![TechDef::default(); 23];
    let cost = |attribute, amount, paid| Cost {
        attribute,
        amount,
        paid,
    };
    defs[22] = TechDef {
        costs: [
            cost(
                Some(attribute::GOLD),
                CostAmount::of_data(50),
                CostPaid::PLAYER,
            ),
            cost(
                Some(attribute::WOOD),
                CostAmount::of_data(20),
                CostPaid::BUILDING,
            ),
            cost(None, CostAmount::of_data(0), CostPaid::UNPAID),
        ],
        locations: vec![(MasterId::of_data(109), WholeSeconds::of_data(25))],
        ..TechDef::default()
    };
    world.model.techs = Arc::new(defs);
    world
}

/// `FUN_1411d66b0` on the first research with `all`: the running research loses its
/// progress and the technology its work done; `FUN_1411d61e0` with five takes the entry's
/// three: 150 gold back (`FUN_1413d15e0`), the technology in state 1 with none queued, the
/// 20 wood the building paid to its store (`FUN_1411c7a00`), 22 off its disabled list
/// (`FUN_1411c5250`); the emptied first entry drops the research, whose destructor tells
/// the centre's unit AI it failed.
#[test]
fn cancel_of_all_researches_gives_back_the_entry_three() {
    let mut world = researching(10.0);
    world
        .cancel_order(&object_ids([9]), UnitIndex::of_data(0), Batch::Five)
        .unwrap();
    let players = &world.players[1];
    assert_eq!(players.attributes[3], Amount::new(150.0));
    let tech_state = &players.techs[22];
    assert_eq!(
        (
            tech_state.state,
            tech_state.pending.get(),
            tech_state.research_done
        ),
        (ResearchState::Available, 0, Seconds::ZERO)
    );
    let tc = &players.objects[ListIndex::new(0)];
    assert_eq!(
        world.parts.combat_of(tc.id).store,
        adds([(attribute::WOOD, 20.0)])
    );
    let production = world.parts.production.get(tc.id).unwrap();
    assert_eq!(production.disabled_techs, [tech_id(5)]);
    assert_eq!(
        (production.queue.count.get(), production.queue.total.get()),
        (0, 0)
    );
    assert!(production.action.is_none());
    assert_eq!(world.parts.unit_ai.get(tc.id).unwrap().notifies.len(), 1);
    // one research of the second unit: 50 gold, the technology in state 1 though two stay
    // queued, the research going on
    let mut world = researching(10.0);
    world
        .cancel_order(&object_ids([9]), UnitIndex::of_data(1), Batch::One)
        .unwrap();
    let player = &world.players[1];
    assert_eq!(
        (
            player.attributes[3],
            player.techs[22].state,
            player.techs[22].pending.get()
        ),
        (Amount::new(50.0), ResearchState::Available, 2)
    );
    let production = world
        .parts
        .production
        .get(player.objects[ListIndex::new(0)].id)
        .unwrap();
    assert_eq!(production.queue.slots[0].count, QueuedUnits::of_data(2));
    assert!(production.action.is_some());
}

/// `FUN_1411cff00` gives a research entry back as a whole; the research's destructor
/// (`FUN_140ad0be0`) then counts a stacking technology whose progress reached its time as
/// researched once more and queued once less, in state 1 below its cap.
#[test]
fn queue_cancel_gives_back_its_researches_and_the_destructor_counts_done_one() {
    let mut world = researching(30.0);
    let tech_state = &mut world.players[1].techs[22];
    (tech_state.stack, tech_state.research_cap) = (true, ResearchCap::of_data(5));
    world
        .cancel_queue(PlayerId::new(1), ListIndex::new(0))
        .unwrap();
    let player = &world.players[1];
    assert_eq!(player.attributes[3], Amount::new(150.0));
    let techs = &player.techs[22];
    assert_eq!(
        (
            techs.state,
            techs.times_researched.get(),
            techs.pending.get(),
            techs.research_done
        ),
        (ResearchState::Available, 1, -1, Seconds::new(30.0))
    );
    let production = world
        .parts
        .production
        .get(player.objects[ListIndex::new(0)].id)
        .unwrap();
    assert_eq!(
        (production.queue.count.get(), production.queue.changes.get()),
        (0, 1)
    );
    // not done: state 1, nothing counted
    let mut world = researching(10.0);
    let players = &mut world.players[1].techs[22];
    (players.stack, players.research_cap) = (true, ResearchCap::of_data(5));
    world
        .cancel_queue(PlayerId::new(1), ListIndex::new(0))
        .unwrap();
    let tech_state = &world.players[1].techs[22];
    assert_eq!(
        (
            tech_state.state,
            tech_state.times_researched.get(),
            tech_state.pending.get()
        ),
        (ResearchState::Available, 0, 0)
    );
}

/// `0x140ad2a20` after a research completes: attribute 269 (the Spanish's 300, cut by
/// `CVTTSS2SI`, its low short above 0) names the effect of the world's table (`+0x128`, slot
/// 0x10 `FUN_141462440`) the player gets with the building as its object: effect 300 adds 20
/// gold; an id the table does not hold does nothing.
#[test]
fn research_gives_the_effect_attribute_269_names() {
    use crate::technology::{Effect, EffectCommand, Effects};
    let run = |reward: f32| {
        let mut world = researching(25.0);
        world.players[1].attributes.resize(300, Amount::ZERO);
        world.players[1].attributes[269] = Amount::new(reward);
        let mut effects = vec![Effect::default(); 301];
        effects[300].commands = vec![EffectCommand::of_data(1, 3, 1, -1, 20.0).unwrap()];
        world.model.effects = Arc::new(Effects::new(effects));
        let gold = world.players[1].attributes[3];
        world
            .produce(PlayerId::new(1), ListIndex::new(0), Seconds::new(1.0))
            .unwrap();
        world.players[1].attributes[3] - gold
    };
    assert_eq!(run(300.0), Amount::new(20.0));
    assert_eq!(run(300.9), Amount::new(20.0));
    assert_eq!(run(0.0), Amount::ZERO);
    assert_eq!(run(301.0), Amount::ZERO);
    // the short of 65836 is 300
    assert_eq!(run(65_836.0), Amount::new(20.0));
}

/// Game mode 16's tail: with attribute 6 at most 1 a cancel whose slot at the unit index
/// holds a military master clears the player's flag (`+0x1378`); above 1 it stays.
#[test]
fn cancel_in_game_mode_16_clears_the_military_flag() {
    let run = |age: f32| {
        let mut world = game();
        world.settings.game_mode = GameMode::of_data(16);
        world.players[1].attributes[6] = Amount::new(age);
        world.players[1].one_military_queued = true;
        let ms = &mut world.model.masters[1];
        *ms = Arc::new(Masters::new(ms.iter().cloned().chain([Master {
            id: master_id(4),
            class: class::CLASS_6,
            kind: MasterKind::COMBAT,
            ..Master::default()
        }])));
        let tc = world
            .parts
            .production
            .get_mut(world.players[1].objects[ListIndex::new(0)].id)
            .unwrap();
        tc.queue = queue(&[
            villager(Some(ids::VILLAGER_MALE), 1, 46.0),
            villager(Some(master_id(4)), 1, 0.0),
        ]);
        world
            .cancel_order(&object_ids([9]), UnitIndex::of_data(1), Batch::One)
            .unwrap();
        world.players[1].one_military_queued
    };
    assert!(!run(1.0));
    assert!(run(2.0));
    assert!(run(f32::NAN));
}

/// MarvinBlue/426048731's player 1 resigns in its last update: its town centre gives back
/// the villager queued (the queue's 46 food; the make action made with the queue's payment
/// gives nothing more) and its population leaves attributes 80 and 81; the make action's end
/// notifies the centre's unit AI, which stops. The victory check then ends the game:
/// player 2, the one side left, wins.
#[test]
fn resignation_cancels_the_queues_and_the_last_side_wins() {
    let mut world = game();
    world.resign(player_id(1), Departure::Resigned).unwrap();
    let player = &world.players[1];
    assert_eq!(
        (
            player.game_status,
            player.departures.contains(Departures::RESIGNED),
            player.departures.contains(Departures::DROPPED)
        ),
        (GameStatus::Lost, true, false)
    );
    assert_eq!(player.attributes[0], Amount::new(46.0));
    assert_eq!(
        (player.attributes[80], player.attributes[81]),
        (Amount::ZERO, Amount::ZERO)
    );
    let tc = &player.objects[ListIndex::new(0)];
    let production = world.parts.production.get(tc.id).unwrap();
    assert!(production.queue.entries().is_empty() && production.action.is_none());
    assert_eq!(
        (production.queue.total.get(), production.queue.changes.get()),
        (0, 1)
    );
    let ai = world.parts.unit_ai.get(tc.id).unwrap();
    assert_eq!(
        (ai.current_action, ai.last_action),
        (None, Some(AiAction::Make))
    );
    assert_eq!(ai.notifies.len(), 1);
    assert_eq!(world.game_state, PlayState::of_data(0));
    world.victory_check().unwrap();
    assert_eq!(world.game_state, PlayState::of_data(1));
    assert_eq!(
        (world.players[1].game_status, world.players[2].game_status),
        (GameStatus::Lost, GameStatus::Won)
    );
}

#[test]
fn make_action_the_queue_did_not_pay_for_refunds_its_master_costs() {
    let mut world = game();
    if let Some(Some(Making::Unit(make))) = world
        .parts
        .production
        .get_mut(world.players[1].objects[ListIndex::new(0)].id)
        .map(|production| production.action.as_mut())
    {
        make.paid = false;
    }
    world.resign(player_id(1), Departure::Resigned).unwrap();
    assert_eq!(world.players[1].attributes[0], Amount::new(92.0));
}

#[test]
fn early_in_the_game_resignation_leaves_the_objects_alone() {
    let mut world = game();
    world.world_time = WorldTime::from_millis(60_000);
    world.resign(player_id(1), Departure::Dropped).unwrap();
    assert_eq!(
        (
            world.players[1].departures.contains(Departures::RESIGNED),
            world.players[1].departures.contains(Departures::DROPPED)
        ),
        (true, true)
    );
    assert_eq!(world.players[1].attributes[0], Amount::ZERO);
}

#[test]
fn one_side_left_after_resignation_ends_conquest_game() {
    let mut count = Count::default();
    count.add(
        PlayerId::new(1),
        GameStatus::Lost,
        0,
        (Relation::Ally, Relation::Ally),
    );
    count.add(
        PlayerId::new(2),
        GameStatus::Playing,
        0,
        (Relation::Ally, Relation::Ally),
    );
    assert_eq!(
        (count.playing, count.side, count.lost, count.won),
        (1, 1, 1, 0)
    );
    // two players still playing, enemies: two sides
    let mut count = Count::default();
    count.add(
        PlayerId::new(1),
        GameStatus::Playing,
        0,
        (Relation::Ally, Relation::Ally),
    );
    count.add(
        PlayerId::new(2),
        GameStatus::Playing,
        0,
        (Relation::Enemy, Relation::Enemy),
    );
    assert_eq!((count.playing, count.side), (2, -1));
    // allies with the allied victory are one side
    let mut count = Count::default();
    count.add(
        PlayerId::new(1),
        GameStatus::Playing,
        1,
        (Relation::Ally, Relation::Ally),
    );
    count.add(
        PlayerId::new(2),
        GameStatus::Playing,
        1,
        (Relation::Ally, Relation::Ally),
    );
    assert_eq!((count.playing, count.side), (2, 1));
}

/// A game status keeps the game's byte; won and lost are the ended ones.
#[test]
fn game_status_keeps_its_byte_and_knows_the_ended_ones() {
    assert_eq!(GameStatus::of_data(2), GameStatus::Lost);
    assert_eq!(GameStatus::Won.get(), 1);
    assert_eq!(GameStatus::default(), GameStatus::Playing);
    assert!(GameStatus::Won.ended() && GameStatus::Lost.ended());
    assert!(!GameStatus::Playing.ended() && !GameStatus::of_data(3).ended());
}

/// The victory state and the fealty keep their saved values: met is 2, a fealty names a player
/// above 0.
#[test]
fn the_victory_state_and_the_fealty_keep_their_values() {
    assert_eq!(VictoryState::of_data(2), VictoryState::MET);
    assert_eq!(VictoryState::default().get(), 0);
    assert!(Fealty::of_data(3).names_player());
    assert!(!Fealty::of_data(0).names_player() && !Fealty::of_data(-1).names_player());
    assert_eq!(Fealty::of_data(-1).get(), -1);
}

/// Each named value and one the set names nowhere go to their variant and back.
#[test]
fn every_game_status_value_maps_to_its_variant_and_back() {
    for value in [0, 1, 2, 3] {
        assert_eq!(GameStatus::of_raw(value).raw(), value);
    }
    assert!(matches!(GameStatus::of_raw(3), GameStatus::Other(_)));
}
