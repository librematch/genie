use super::*;
use crate::combat::attack::AttackState;
use crate::construction::build::BuildState;
use crate::construction::repair::RepairState;
use crate::gathering::gather::GatherState;
use crate::gathering::hunt::HuntState;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Point;
use crate::map::Position;
use crate::movement::Speed;
use crate::objects::Graphics;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::SpeedMultiplier;
use crate::objects::SpriteFlags;
use crate::objects::SpriteId;
use crate::objects::WorkRate;
use crate::objects::actions::ActionList;
use crate::objects::masters::CostAmount;
use crate::objects::masters::TaskId;
use crate::objects::masters::TaskSprites;
use crate::objects::masters::action_type;
use crate::objects::masters::class;
use crate::objects::masters::ids;
use crate::objects::masters::master_type;
use crate::objects::masters::{Master, Masters};
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::Relation;
use crate::players::attribute;
use crate::players::attribute::AttributeId;
use crate::players::relation_row;
use crate::production::LocationButton;
use crate::production::QueueButton;
use crate::production::QueuedUnits;
use crate::production::UnitIndex;
use crate::production::{
    Entry, MakeObj, MakeObjState, MakeTech, MakeTechState, Making, Production, Queue,
};
use crate::technology::CostPaid;
use crate::technology::ResearchState;
use crate::test_ids::costs;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::test_ids::object_ids;
use crate::test_ids::player_id;
use crate::test_ids::tech_id;
use crate::unit_ai::ActionKind;
use crate::unit_ai::OrderKind;
use crate::unit_ai::UnitAi;
use crate::world::Millis;
use crate::world::Player;
use crate::world::Seconds;
use crate::world::WholeSeconds;

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
        serial: crate::production::EntrySerial::new(0),
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
        count: crate::production::QueueLength::of_data(entries.len() as i16),
        total: entries.iter().map(|e| e.count).sum(),
        next_serial: crate::production::EntrySerial::new(10),
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
        combat: crate::objects::layers::Combat {
            ai: Some(UnitAi {
                current_order: None,
                current_action: Some(ActionKind::Make),
                current_target: None,
                ..UnitAi::default()
            }),
            ..Default::default()
        },
        building: crate::objects::layers::Building {
            production: Some(Production {
                queue: queue(&[villager(Some(ids::VILLAGER_MALE), 1, 46.0)]),
                action: Some(Making::Unit(MakeObj {
                    state: MakeObjState::Making,
                    progress: Seconds::new(3.0),
                    ..MakeObj::for_entry(&villager(Some(ids::VILLAGER_MALE), 1, 46.0))
                })),
                ..Production::default()
            }),
            ..Default::default()
        },
        ..Unit::default()
    };
    let player = |objects| Player {
        objects,
        attributes: attributes.clone(),
        relations: relation_row([0, 0, 3]),
        ..Player::default()
    };
    let mut t = World::new(
        WorldTime::from_millis(61_000),
        crate::world::FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO; 3],
        vec![
            Player::default(),
            player(vec![tc].into()),
            player(vec![].into()),
        ],
    );
    let masters = Masters::new([
        Master {
            id: ids::TOWN_CENTRE,
            class: class::BUILDING,
            kind: master_type::BUILDING,
            ..Master::default()
        },
        Master {
            id: ids::VILLAGER_MALE,
            class: class::VILLAGER,
            kind: master_type::COMBAT,
            cost: costs(&[(attribute::FOOD, 46, true), (attribute::HEADROOM, 1, false)]),
            storage: vec![
                (Some(attribute::HEADROOM), Amount::new(-1.0), 2),
                (Some(attribute::POPULATION), Amount::new(1.0), 2),
            ],
            ..Master::default()
        },
    ]);
    t.players[1].objects[ListIndex::new(0)].life.master_id = ids::TOWN_CENTRE;
    t.masters = vec![masters.clone(), masters.clone(), masters];
    t.victory_conquest = true;
    t
}

/// The action object's stop (`FUN_14106fa60`) keeps the action list when the top action's
/// slot 0x38 answers true and the unit is no trade cart with goods (`FUN_14106a940`): a
/// human hunter's Hunt halts (state 2) and stays. A unit AI then stops (slot 0xb0) as the
/// top action is no trade (0x6f), and its stopAction empties the list.
/// A defeat (`FUN_141639860`) adds its event to the history: the player with the most of
/// its own attribute 300 + id, Gaia and the defeated player left out; a resigned player and
/// a history without entries get none.
#[test]
fn a_defeat_names_the_player_with_the_most_of_its_own_attribute() {
    let mut t = game();
    let other = t.players[1].clone();
    t.players.push(other);
    t.reindex();
    t.players[2].attributes.resize(310, Amount::ZERO);
    t.players[3].attributes.resize(310, Amount::ZERO);
    t.players[1].attributes.resize(310, Amount::ZERO);
    t.players[0].attributes = vec![Amount::new(9.0); 310];
    t.players[2].attributes[302] = Amount::new(3.0);
    t.players[3].attributes[303] = Amount::new(7.9);
    t.players[1].attributes[301] = Amount::new(50.0);
    t.players[1].history.entries = vec![
        crate::players::history::HistoryEntry {
            civilian: crate::players::history::Population::of_data(1),
            military: crate::players::history::Population::ZERO,
        };
        4
    ];
    t.world_time = WorldTime::from_millis(5_000);
    t.defeat_event(PlayerId::new(1)).unwrap();
    assert_eq!(
        t.players[1].history.events,
        [crate::players::history::Event {
            kind: crate::players::history::EventKind::DEFEAT,
            time_slice: crate::players::history::TimeSlice::of_data(4),
            world_time: WorldTime::from_millis(5_000),
            data1: 3.0,
            data2: 7.0,
            data3: 0.0,
        }]
    );
    t.players[2].resigned = true;
    t.players[2].history.entries = vec![crate::players::history::HistoryEntry {
        civilian: crate::players::history::Population::of_data(1),
        military: crate::players::history::Population::ZERO,
    }];
    t.defeat_event(PlayerId::new(2)).unwrap();
    assert_eq!(t.players[2].history.events, []);
}

/// Slot 0x38 of the actions with their own ([`World::halt_other`]), each true: the Gather
/// idles (state 2), the Build and the Repair halt (state 2), the Attack ends (state 1, its
/// animation at speed 1, standing) and the Artifact checks again (state 6); a kind it does
/// not model is refused.
#[test]
fn each_action_with_its_own_stop_halts_and_answers_true() {
    let mut t = game();
    let mut add = |unit: Unit| {
        let mut unit = unit;
        unit.life.master_id = ids::TOWN_CENTRE;
        let _ = t.push_live(PlayerId::new(1), unit).unwrap();
        t.players[1].objects.last_place().unwrap()
    };
    let task = crate::gathering::hunt::HuntTask {
        id: TaskId::test(0),
        range: Distance::new(0.25),
        resource: Some(AttributeId::numbered(17)),
        sprites: TaskSprites::NONE,
        work_value: crate::objects::masters::WorkValue::new(1.0),
        productivity: None,
        kind4: None,
    };
    let bush = crate::gathering::gather::Resource {
        id: ObjectId::NOBODY,
        position: Position::new(3.5, 3.5, 0.0),
        class: class::CLASS_7,
        master: master_id(59),
        player: PlayerId::new(0),
        workers: crate::objects::WorkerCount::of_data(0),
        drop_site: false,
        gap: Distance::new(1.2),
        state: ObjectState::Alive,
        radius: Footprint::new(0.5, 0.5),
    };
    let rate = crate::gathering::gather::StopRate {
        work_rate: WorkRate::new(1.0),
        capacity: Amount::new(10.0),
        productivity: Amount::new(1.0),
    };
    let gather = add(Unit {
        id: object_id(30),
        actions: crate::objects::layers::ActionLayer {
            list: ActionList::default().with_gather(Some(
                crate::gathering::gather::GatherAction::new(task, &bush, false, rate).unwrap(),
            )),
            ..Default::default()
        },
        ..Unit::default()
    });
    let build_task = crate::construction::build::BuildTask {
        range: Distance::ZERO,
        work_value: crate::objects::masters::WorkValue::new(1.0),
        work_rate: WorkRate::new(1.0),
        resource: None,
        id: TaskId::of_task(-1),
        moving: None,
    };
    let build = add(Unit {
        id: object_id(31),
        actions: crate::objects::layers::ActionLayer {
            list: ActionList::default().with_build(Some(
                crate::construction::build::tests::started_on(
                    None,
                    Position::new(3.5, 3.5, 0.0),
                    Distance::new(1.0),
                    build_task,
                ),
            )),
            ..Default::default()
        },
        ..Unit::default()
    });
    let repair_task = crate::construction::repair::RepairTask {
        id: TaskId::test(0),
        work_value: crate::objects::masters::WorkValue::new(1.0),
        moving: None,
        proceeding: None,
    };
    let repair = add(Unit {
        id: object_id(32),
        actions: crate::objects::layers::ActionLayer {
            list: ActionList::default().with_repair(Some(
                crate::construction::repair::RepairAction::start(
                    crate::movement::move_to::WalkTo {
                        id: None,
                        at: Position::new(3.5, 3.5, 0.0),
                        gap: Distance::new(crate::movement::move_to::NO_TARGET_GAP),
                        state: None,
                    },
                    repair_task,
                    Distance::ZERO,
                    &crate::movement::move_to::MoveToReads {
                        sprites: crate::movement::move_to::SpriteReads {
                            defaults: crate::movement::move_to::MoveDefaults::default(),
                            graphics: &Graphics::new(),
                        },
                        state: ObjectState::Alive,
                        speed: Speed::new(1.0),
                        at: Position::new(3.5, 3.5, 0.0),
                        ground: Footprint::new(0.2, 0.2),
                    },
                )
                .unwrap(),
            )),
            ..Default::default()
        },
        ..Unit::default()
    });
    let mut attack =
        crate::combat::attack::tests::own_attack(-1, AttackState::Done, Distance::new(1.0));
    attack.arms.idle_sprite = Some(crate::objects::sprite::Graphic {
        id: SpriteId::named(1),
        frames: crate::objects::Frame::of_data(10),
        facets: crate::objects::FacetCount::of_data(8),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::NONE,
    });
    let attack = add(Unit {
        id: object_id(33),
        actions: crate::objects::layers::ActionLayer {
            list: ActionList::default().with_attack(Some(attack)),
            ..Default::default()
        },
        body: crate::objects::layers::Body::Moving(crate::gathering::hunt::tests::walker()),
        ..Unit::default()
    });
    let artifact = add(Unit {
        id: object_id(34),
        actions: crate::objects::layers::ActionLayer {
            list: ActionList::default()
                .with_artifact(Some(crate::ownership::artifact::Artifact::new(false, None))),
            ..Default::default()
        },
        ..Unit::default()
    });
    assert_eq!(
        t.halt_other(PlayerId::new(1), gather, crate::gathering::gather::GATHER),
        Ok(true)
    );
    assert_eq!(
        t.halt_other(PlayerId::new(1), build, action_type::BUILD),
        Ok(true)
    );
    assert_eq!(
        t.halt_other(
            PlayerId::new(1),
            repair,
            crate::construction::repair::REPAIR
        ),
        Ok(true)
    );
    assert_eq!(
        t.halt_other(PlayerId::new(1), attack, crate::combat::attack::ATTACK),
        Ok(true)
    );
    let kind = crate::ownership::artifact::ARTIFACT_TYPE;
    assert_eq!(t.halt_other(PlayerId::new(1), artifact, kind), Ok(true));
    let objects = &t.players[1].objects;
    assert_eq!(
        objects[gather].actions.list.gather().map(|g| g.state),
        Some(GatherState::Idle)
    );
    assert_eq!(
        objects[build].actions.list.build().map(|b| b.state),
        Some(BuildState::Halted)
    );
    assert_eq!(
        objects[repair].actions.list.repair().map(|r| r.state),
        Some(RepairState::Halted)
    );
    let a = &objects[attack];
    assert_eq!(
        a.actions.list.attack().map(|a| a.state),
        Some(AttackState::Done)
    );
    assert_eq!(a.body.anim_speed(), Some(Speed::new(1.0)));
    assert!(a.sprite.is_some());
    assert_eq!(
        objects[artifact].actions.list.artifact().map(|a| a.state),
        Some(crate::ownership::artifact::ArtifactState::Check)
    );
    assert!(
        t.halt_other(PlayerId::new(1), gather, ActionType::named(99))
            .is_err()
    );
}

#[test]
fn a_stopped_hunter_keeps_its_halted_hunt() {
    use crate::gathering::hunt::{HuntAction, HuntTask, Lead};
    let hunter = Unit {
        id: object_id(20),
        combat: crate::objects::layers::Combat {
            ai: Some(UnitAi {
                current_order: Some(OrderKind::Hunt),
                current_action: Some(ActionKind::Hunt),
                current_target: None,
                ..UnitAi::default()
            }),
            ..Default::default()
        },
        actions: crate::objects::layers::ActionLayer {
            list: ActionList::default().with_hunt(Some(HuntAction {
                target: None,
                state: HuntState::Walk,
                point: Position::from([-1.0; 3]),
                target_class: None,
                task: HuntTask {
                    id: TaskId::test(0),
                    range: Distance::ZERO,
                    resource: None,
                    sprites: TaskSprites::NONE,
                    work_value: crate::objects::masters::WorkValue::new(0.0),
                    productivity: None,
                    kind4: None,
                },
                attack: None,
                move_to: None,
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
        ..Unit::default()
    };
    let mut t = game();
    let mut hunter = hunter;
    // game()'s villager master
    hunter.life.master_id = ids::TOWN_CENTRE;
    hunter.my_pasture = None;
    let _ = t.push_live(PlayerId::new(1), hunter).unwrap();
    let idx = t.players[1].objects.last_place().unwrap();
    let mut without_ai = t.clone();
    // the unit AI's stopAction (slot 0x3c0) then empties the list
    t.object_stop(PlayerId::new(1), idx).unwrap();
    let u = &t.players[1].objects[idx];
    assert_eq!(u.actions.list.hunt().copied(), None);
    assert_eq!(u.combat.ai.as_ref().unwrap().current_action, None);
    // without a unit AI the halted Hunt stays
    without_ai.players[1].objects[idx].combat.ai = None;
    without_ai.object_stop(PlayerId::new(1), idx).unwrap();
    let u = &without_ai.players[1].objects[idx];
    assert_eq!(
        u.actions.list.hunt().map(|h| h.state),
        Some(HuntState::Idle)
    );
    // its target went through slot 0x68 (FUN_140ac7980), which clears the lead
    assert_eq!(u.actions.list.hunt().map(|h| h.lead), Some(Lead::NEW));
}

/// records/425906646's player 2 cancels the second villager of its town centre at update
/// 3699 (`UNIT_ORDER` action 4, index 1): that one's 46 food come back and its population
/// leaves attribute 80; the first keeps its make action. A cancel of the first drops the make
/// action too; an index past the queue does nothing.
#[test]
fn a_cancel_gives_back_one_queued_unit() {
    let mut t = game();
    let tc = t.players[1].objects[ListIndex::new(0)]
        .building
        .production
        .as_mut()
        .unwrap();
    tc.queue = queue(&[
        villager(Some(ids::VILLAGER_MALE), 1, 46.0),
        villager(Some(ids::VILLAGER_MALE), 1, 45.0),
    ]);
    t.players[1].attributes[80] = Amount::new(2.0);
    t.cancel_order(&object_ids([9]), UnitIndex::of_data(1), false)
        .unwrap();
    let p = &t.players[1];
    assert_eq!(
        (p.attributes[0], p.attributes[80]),
        (Amount::new(45.0), Amount::new(1.0))
    );
    let production = p.objects[ListIndex::new(0)]
        .building
        .production
        .as_ref()
        .unwrap();
    let q = &production.queue;
    assert_eq!((q.count.get(), q.total.get(), q.changes.get()), (1, 1, 1));
    assert!(production.action.is_some());
    assert_eq!(p.tracker.queued, -1);
    t.cancel_order(&object_ids([9]), UnitIndex::of_data(5), false)
        .unwrap();
    assert_eq!(t.players[1].attributes[0], Amount::new(45.0));
    t.cancel_order(&object_ids([9]), UnitIndex::of_data(0), false)
        .unwrap();
    let p = &t.players[1];
    assert_eq!(
        (p.attributes[0], p.attributes[80], p.attributes[81]),
        (Amount::new(91.0), Amount::ZERO, Amount::ZERO)
    );
    let production = p.objects[ListIndex::new(0)]
        .building
        .production
        .as_ref()
        .unwrap();
    assert!(production.queue.entries().is_empty() && production.action.is_none());
    assert!(
        t.cancel_order(&object_ids([9]), UnitIndex::of_data(0), true)
            .is_ok()
    );
}

/// `TRIBE_Building_Object::update` after its combat update (`0x1411ca3e2`): a building past
/// state 2 cancels its whole queue (`FUN_1411cff00`): 429063959's player 3 deleted its town
/// centre with four villagers queued at update 550, and the game gave their 200 food back as
/// the town centre died. Each later update counts the queue's changes on; a building in state
/// 2 keeps its queue for its production step.
#[test]
fn a_dying_building_cancels_its_queue_in_its_update() {
    let mut t = game();
    let tc = t.players[1].objects[ListIndex::new(0)]
        .building
        .production
        .as_mut()
        .unwrap();
    tc.queue = queue(&[villager(Some(ids::VILLAGER_MALE), 4, 50.0)]);
    t.players[1].attributes[80] = Amount::new(4.0);
    t.players[1].objects[ListIndex::new(0)].life.state = ObjectState::Dying;
    t.building_production(PlayerId::new(1), ListIndex::new(0), Seconds::new(0.05))
        .unwrap();
    let p = &t.players[1];
    assert_eq!(
        (p.attributes[0], p.attributes[80]),
        (Amount::new(200.0), Amount::ZERO)
    );
    let q = &p.objects[ListIndex::new(0)]
        .building
        .production
        .as_ref()
        .unwrap()
        .queue;
    assert_eq!((q.count.get(), q.total.get(), q.changes.get()), (0, 0, 1));
    t.building_production(PlayerId::new(1), ListIndex::new(0), Seconds::new(0.05))
        .unwrap();
    let q = &t.players[1].objects[ListIndex::new(0)]
        .building
        .production
        .as_ref()
        .unwrap()
        .queue;
    assert_eq!((q.count.get(), q.changes.get()), (0, 2));
    let mut t = game();
    let tc = t.players[1].objects[ListIndex::new(0)]
        .building
        .production
        .as_mut()
        .unwrap();
    tc.queue = queue(&[villager(Some(ids::VILLAGER_MALE), 4, 50.0)]);
    t.players[1].attributes[80] = Amount::new(4.0);
    t.players[1].objects[ListIndex::new(0)].life.state = ObjectState::Alive;
    // a disabled queue (+0xbf6): the production step does nothing
    t.players[1].objects[ListIndex::new(0)]
        .building
        .production
        .as_mut()
        .unwrap()
        .queue
        .enabled = false;
    t.building_production(PlayerId::new(1), ListIndex::new(0), Seconds::new(0.05))
        .unwrap();
    let p = &t.players[1];
    assert_eq!(
        p.objects[ListIndex::new(0)]
            .building
            .production
            .as_ref()
            .unwrap()
            .queue
            .count,
        crate::production::QueueLength::of_data(1)
    );
    assert_eq!(p.attributes[0], Amount::ZERO);
}

/// One unit of an entry of three goes: the entry keeps two, and the first unit's cancel
/// drops the make action though the entry stays (`FUN_1411d66b0`).
#[test]
fn a_cancel_of_the_first_unit_drops_the_make_action_of_an_entry_that_stays() {
    let mut t = game();
    let tc = t.players[1].objects[ListIndex::new(0)]
        .building
        .production
        .as_mut()
        .unwrap();
    tc.queue = queue(&[villager(Some(ids::VILLAGER_MALE), 3, 46.0)]);
    t.players[1].attributes[80] = Amount::new(3.0);
    t.cancel_queued(
        PlayerId::new(1),
        ListIndex::new(0),
        UnitIndex::of_data(0),
        false,
    )
    .unwrap();
    let p = &t.players[1];
    let production = p.objects[ListIndex::new(0)]
        .building
        .production
        .as_ref()
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
        (p.attributes[80], p.attributes[81]),
        (Amount::new(2.0), Amount::ZERO)
    );
}

/// `FUN_1411d66b0` with `all` on the first villager of [3 male, 4 female]: the three go
/// (`FUN_1411d61e0`), the first entry with them, and the make action with it (the cancel
/// started at unit 0); the female entry moved to the first slot gives two more, five in
/// all. Each unit gives its 46 food back and leaves attribute 80; the tracker's counter
/// loses two per unit (the game counts a cancel of several twice).
#[test]
fn a_cancel_of_all_takes_five_villagers_of_both_sexes() {
    let mut t = game();
    let tc = t.players[1].objects[ListIndex::new(0)]
        .building
        .production
        .as_mut()
        .unwrap();
    tc.queue = queue(&[
        villager(Some(ids::VILLAGER_MALE), 3, 46.0),
        villager(Some(ids::VILLAGER_FEMALE), 4, 46.0),
    ]);
    t.players[1].attributes[80] = Amount::new(7.0);
    t.masters[1] = Masters::new([
        t.master(PlayerId::new(1), ids::TOWN_CENTRE)
            .unwrap()
            .clone(),
        t.master(PlayerId::new(1), ids::VILLAGER_MALE)
            .unwrap()
            .clone(),
        Master {
            id: ids::VILLAGER_FEMALE,
            ..t.master(PlayerId::new(1), ids::VILLAGER_MALE)
                .unwrap()
                .clone()
        },
    ]);
    t.cancel_order(&object_ids([9]), UnitIndex::of_data(0), true)
        .unwrap();
    let p = &t.players[1];
    assert_eq!(
        (p.attributes[0], p.attributes[80]),
        (Amount::new(5.0 * 46.0), Amount::new(2.0))
    );
    let production = p.objects[ListIndex::new(0)]
        .building
        .production
        .as_ref()
        .unwrap();
    let q = &production.queue;
    assert_eq!(
        (
            q.count.get(),
            q.total.get(),
            q.slots[0].master,
            q.slots[0].count.get()
        ),
        (1, 2, Some(master_id(293)), 2)
    );
    assert_eq!(q.changes, crate::production::QueueChanges::of_data(2));
    assert!(production.action.is_none());
    assert_eq!(p.tracker.queued, -10);
    // from the second unit the make action goes on for the other sex
    let mut t2 = game();
    t2.masters = t.masters.clone();
    let tc = t2.players[1].objects[ListIndex::new(0)]
        .building
        .production
        .as_mut()
        .unwrap();
    tc.queue = queue(&[
        villager(Some(ids::VILLAGER_MALE), 3, 46.0),
        villager(Some(ids::VILLAGER_FEMALE), 4, 46.0),
    ]);
    t2.players[1].attributes[80] = Amount::new(7.0);
    t2.cancel_order(&object_ids([9]), UnitIndex::of_data(1), true)
        .unwrap();
    let production = t2.players[1].objects[ListIndex::new(0)]
        .building
        .production
        .as_ref()
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
fn a_cancel_of_all_goes_on_into_villagers_and_stops_at_a_research() {
    let archer = |count| Entry {
        master: Some(master_id(4)),
        costs: [
            (Some(attribute::WOOD), Amount::new(25.0)),
            (Some(attribute::GOLD), Amount::new(45.0)),
            (Some(attribute::HEADROOM), Amount::new(1.0)),
        ],
        ..villager(Some(master_id(4)), count, 0.0)
    };
    let mut t = game();
    let ms = &mut t.masters[1];
    *ms = Masters::new(ms.iter().cloned().chain([Master {
        id: master_id(4),
        kind: master_type::COMBAT,
        cost: costs(&[
            (attribute::WOOD, 25, true),
            (attribute::GOLD, 45, true),
            (attribute::HEADROOM, 1, false),
        ]),
        storage: vec![(Some(attribute::HEADROOM), Amount::new(-1.0), 2)],
        ..Master::default()
    }]));
    t.players[1].attributes[80] = Amount::new(4.0);
    let tc = t.players[1].objects[ListIndex::new(0)]
        .building
        .production
        .as_mut()
        .unwrap();
    tc.queue = queue(&[archer(2), villager(Some(ids::VILLAGER_MALE), 2, 46.0)]);
    t.cancel_queued(
        PlayerId::new(1),
        ListIndex::new(0),
        UnitIndex::of_data(0),
        true,
    )
    .unwrap();
    let p = &t.players[1];
    assert_eq!(
        p.resources(),
        crate::test_ids::amounts([92.0, 50.0, 0.0, 90.0])
    );
    assert_eq!(p.attributes[80], Amount::ZERO);
    assert_eq!(
        p.objects[ListIndex::new(0)]
            .building
            .production
            .as_ref()
            .unwrap()
            .queue
            .count,
        crate::production::QueueLength::of_data(0)
    );
    // [2 villagers, a research]: the research moved into the slot ends it
    let mut t = game();
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
    let tc = t.players[1].objects[ListIndex::new(0)]
        .building
        .production
        .as_mut()
        .unwrap();
    tc.queue = queue(&[villager(Some(ids::VILLAGER_MALE), 2, 46.0), research]);
    t.players[1].attributes[80] = Amount::new(2.0);
    t.cancel_queued(
        PlayerId::new(1),
        ListIndex::new(0),
        UnitIndex::of_data(0),
        true,
    )
    .unwrap();
    let q = &t.players[1].objects[ListIndex::new(0)]
        .building
        .production
        .as_ref()
        .unwrap()
        .queue;
    assert_eq!(
        (q.count.get(), q.slots[0].tech, t.players[1].attributes[0]),
        (1, Some(tech_id(22)), Amount::new(92.0))
    );
}

/// Player 1's town centre researching technology 22 three times over (one entry), the
/// research running with `progress` done: each paid 50 gold, and the `.dat` lists 20 wood
/// the building pays (flag 2).
fn researching(progress: f32) -> World {
    use crate::technology::{Cost, Location, TechDef, TechState};
    let mut t = game();
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
    let tc = t.players[1].objects[ListIndex::new(0)]
        .building
        .production
        .as_mut()
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
        pending: crate::technology::ResearchCount::of_data(3),
        research_done: Seconds::new(progress),
        research_cap: crate::technology::ResearchCap::of_data(-1),
        overrides: [crate::technology::CostOverride::UNSET; 3],
        locations: vec![Location {
            building: Some(ids::TOWN_CENTRE),
            time: WholeSeconds::of_data(25),
            button: LocationButton::of_data(1),
            adjustment: Seconds::ZERO,
        }],
        ..TechState::default()
    };
    t.players[1].techs = techs;
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
    t.techs = defs;
    t
}

/// `FUN_1411d66b0` on the first research with `all`: the running research loses its
/// progress and the technology its work done; `FUN_1411d61e0` with five takes the entry's
/// three: 150 gold back (`FUN_1413d15e0`), the technology in state 1 with none queued, the
/// 20 wood the building paid to its store (`FUN_1411c7a00`), 22 off its disabled list
/// (`FUN_1411c5250`); the emptied first entry drops the research, whose destructor tells
/// the centre's unit AI it failed.
#[test]
fn a_cancel_of_all_researches_gives_back_the_entry_s_three() {
    let mut t = researching(10.0);
    t.cancel_order(&object_ids([9]), UnitIndex::of_data(0), true)
        .unwrap();
    let p = &t.players[1];
    assert_eq!(p.attributes[3], Amount::new(150.0));
    let s = &p.techs[22];
    assert_eq!(
        (s.state, s.pending.get(), s.research_done),
        (ResearchState::Available, 0, Seconds::ZERO)
    );
    let tc = &p.objects[ListIndex::new(0)];
    assert_eq!(
        tc.combat.store,
        crate::test_ids::adds([(attribute::WOOD, 20.0)])
    );
    let production = tc.building.production.as_ref().unwrap();
    assert_eq!(production.disabled_techs, [tech_id(5)]);
    assert_eq!(
        (production.queue.count.get(), production.queue.total.get()),
        (0, 0)
    );
    assert!(production.action.is_none());
    assert_eq!(tc.combat.ai.as_ref().unwrap().notifies.len(), 1);
    // one research of the second unit: 50 gold, the technology in state 1 though two stay
    // queued, the research going on
    let mut t = researching(10.0);
    t.cancel_order(&object_ids([9]), UnitIndex::of_data(1), false)
        .unwrap();
    let p = &t.players[1];
    assert_eq!(
        (
            p.attributes[3],
            p.techs[22].state,
            p.techs[22].pending.get()
        ),
        (Amount::new(50.0), ResearchState::Available, 2)
    );
    let production = p.objects[ListIndex::new(0)]
        .building
        .production
        .as_ref()
        .unwrap();
    assert_eq!(production.queue.slots[0].count, QueuedUnits::of_data(2));
    assert!(production.action.is_some());
}

/// `FUN_1411cff00` gives a research entry back as a whole; the research's destructor
/// (`FUN_140ad0be0`) then counts a stacking technology whose progress reached its time as
/// researched once more and queued once less, in state 1 below its cap.
#[test]
fn a_queue_s_cancel_gives_back_its_researches_and_the_destructor_counts_a_done_one() {
    let mut t = researching(30.0);
    let s = &mut t.players[1].techs[22];
    (s.stack, s.research_cap) = (true, crate::technology::ResearchCap::of_data(5));
    t.cancel_queue(PlayerId::new(1), ListIndex::new(0)).unwrap();
    let p = &t.players[1];
    assert_eq!(p.attributes[3], Amount::new(150.0));
    let s = &p.techs[22];
    assert_eq!(
        (
            s.state,
            s.times_researched.get(),
            s.pending.get(),
            s.research_done
        ),
        (ResearchState::Available, 1, -1, Seconds::new(30.0))
    );
    let production = p.objects[ListIndex::new(0)]
        .building
        .production
        .as_ref()
        .unwrap();
    assert_eq!(
        (production.queue.count.get(), production.queue.changes.get()),
        (0, 1)
    );
    // not done: state 1, nothing counted
    let mut t = researching(10.0);
    let s = &mut t.players[1].techs[22];
    (s.stack, s.research_cap) = (true, crate::technology::ResearchCap::of_data(5));
    t.cancel_queue(PlayerId::new(1), ListIndex::new(0)).unwrap();
    let s = &t.players[1].techs[22];
    assert_eq!(
        (s.state, s.times_researched.get(), s.pending.get()),
        (ResearchState::Available, 0, 0)
    );
}

/// `0x140ad2a20` after a research completes: attribute 269 (the Spanish's 300, cut by
/// `CVTTSS2SI`, its low short above 0) names the effect of the world's table (`+0x128`, slot
/// 0x10 `FUN_141462440`) the player gets with the building as its object: effect 300 adds 20
/// gold; an id the table does not hold does nothing.
#[test]
fn a_research_gives_the_effect_attribute_269_names() {
    use crate::technology::EffectCommand;
    let run = |reward: f32| {
        let mut t = researching(25.0);
        t.players[1].attributes.resize(300, Amount::ZERO);
        t.players[1].attributes[269] = Amount::new(reward);
        t.effects = vec![Vec::new(); 301];
        t.effects[300] = vec![EffectCommand::of_data(1, 3, 1, -1, 20.0).unwrap()];
        let gold = t.players[1].attributes[3];
        t.produce(PlayerId::new(1), ListIndex::new(0), Seconds::new(1.0))
            .unwrap();
        t.players[1].attributes[3] - gold
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
fn a_cancel_in_game_mode_16_clears_the_military_flag() {
    let run = |age: f32| {
        let mut t = game();
        t.settings.game_mode = crate::world::GameMode::of_data(16);
        t.players[1].attributes[6] = Amount::new(age);
        t.players[1].one_military_queued = true;
        let ms = &mut t.masters[1];
        *ms = Masters::new(ms.iter().cloned().chain([Master {
            id: master_id(4),
            class: class::CLASS_6,
            kind: master_type::COMBAT,
            ..Master::default()
        }]));
        let tc = t.players[1].objects[ListIndex::new(0)]
            .building
            .production
            .as_mut()
            .unwrap();
        tc.queue = queue(&[
            villager(Some(ids::VILLAGER_MALE), 1, 46.0),
            villager(Some(master_id(4)), 1, 0.0),
        ]);
        t.cancel_order(&object_ids([9]), UnitIndex::of_data(1), false)
            .unwrap();
        t.players[1].one_military_queued
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
fn a_resignation_cancels_the_queues_and_the_last_side_wins() {
    let mut t = game();
    t.resign(player_id(1), false).unwrap();
    let p = &t.players[1];
    assert_eq!(
        (p.game_status, p.resigned, p.dropped),
        (GameStatus::LOST, true, false)
    );
    assert_eq!(p.attributes[0], Amount::new(46.0));
    assert_eq!(
        (p.attributes[80], p.attributes[81]),
        (Amount::ZERO, Amount::ZERO)
    );
    let tc = &p.objects[ListIndex::new(0)];
    let production = tc.building.production.as_ref().unwrap();
    assert!(production.queue.entries().is_empty() && production.action.is_none());
    assert_eq!(
        (production.queue.total.get(), production.queue.changes.get()),
        (0, 1)
    );
    let ai = tc.combat.ai.as_ref().unwrap();
    assert_eq!(
        (ai.current_action, ai.last_action),
        (None, Some(ActionKind::Make))
    );
    assert_eq!(ai.notifies.len(), 1);
    assert_eq!(t.game_state, crate::world::PlayState::of_data(0));
    t.victory_check().unwrap();
    assert_eq!(t.game_state, crate::world::PlayState::of_data(1));
    assert_eq!(
        (t.players[1].game_status, t.players[2].game_status),
        (GameStatus::LOST, GameStatus::WON)
    );
}

#[test]
fn a_make_action_the_queue_did_not_pay_for_refunds_its_master_s_costs() {
    let mut t = game();
    if let Some(Some(Making::Unit(make))) = t.players[1].objects[ListIndex::new(0)]
        .building
        .production
        .as_mut()
        .map(|q| q.action.as_mut())
    {
        make.paid = false;
    }
    t.resign(player_id(1), false).unwrap();
    assert_eq!(t.players[1].attributes[0], Amount::new(92.0));
}

#[test]
fn early_in_the_game_a_resignation_leaves_the_objects_alone() {
    let mut t = game();
    t.world_time = WorldTime::from_millis(60_000);
    t.resign(player_id(1), true).unwrap();
    assert_eq!((t.players[1].resigned, t.players[1].dropped), (true, true));
    assert_eq!(t.players[1].attributes[0], Amount::ZERO);
}

#[test]
fn one_side_left_after_a_resignation_ends_a_conquest_game() {
    let mut c = Count::default();
    c.add(
        PlayerId::new(1),
        GameStatus::LOST,
        0,
        (Relation::ALLY, Relation::ALLY),
    );
    c.add(
        PlayerId::new(2),
        GameStatus::PLAYING,
        0,
        (Relation::ALLY, Relation::ALLY),
    );
    assert_eq!((c.playing, c.side, c.lost, c.won), (1, 1, 1, 0));
    // two players still playing, enemies: two sides
    let mut c = Count::default();
    c.add(
        PlayerId::new(1),
        GameStatus::PLAYING,
        0,
        (Relation::ALLY, Relation::ALLY),
    );
    c.add(
        PlayerId::new(2),
        GameStatus::PLAYING,
        0,
        (Relation::ENEMY, Relation::ENEMY),
    );
    assert_eq!((c.playing, c.side), (2, -1));
    // allies with the allied victory are one side
    let mut c = Count::default();
    c.add(
        PlayerId::new(1),
        GameStatus::PLAYING,
        1,
        (Relation::ALLY, Relation::ALLY),
    );
    c.add(
        PlayerId::new(2),
        GameStatus::PLAYING,
        1,
        (Relation::ALLY, Relation::ALLY),
    );
    assert_eq!((c.playing, c.side), (2, 1));
}

/// A game status keeps the game's byte; won and lost are the ended ones.
#[test]
fn a_game_status_keeps_its_byte_and_knows_the_ended_ones() {
    assert_eq!(GameStatus::of_data(2), GameStatus::LOST);
    assert_eq!(GameStatus::WON.get(), 1);
    assert_eq!(GameStatus::default(), GameStatus::PLAYING);
    assert!(GameStatus::WON.ended() && GameStatus::LOST.ended());
    assert!(!GameStatus::PLAYING.ended() && !GameStatus::of_data(3).ended());
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
