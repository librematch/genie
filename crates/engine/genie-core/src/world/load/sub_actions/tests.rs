use crate::combat::AttackTargetId;
use crate::combat::attack::ATTACK;
use crate::combat::attack::AttackState;
use crate::garrison::ENTER;
use crate::gathering::make::MAKE;
use crate::map::Distance;
use crate::movement::explore::EXPLORE;
use crate::movement::fly::FLY;
use crate::movement::move_to::MOVE_TO;
use crate::movement::move_to::MoveTo;
use crate::objects::Graphics;
use crate::objects::SpriteId;
use crate::objects::Voice;
use crate::objects::WorkRate;
use crate::objects::layers::{UnreadDefault, Unstartable};
use crate::objects::masters::ActionKind;
use crate::objects::masters::ActionPart;
use crate::objects::masters::FlyMode;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::Task;
use crate::objects::masters::TaskId;
use crate::objects::masters::TaskIndex;
use crate::objects::masters::TaskSprites;
use crate::ownership::artifact::ARTIFACT_KIND;
use crate::players::Amount;
use crate::players::CivId;
use crate::production::{MakeObjState, MakeTechState, Making};
use crate::technology::TechId;
use crate::test_ids::object_id;
use crate::testing::attribute;
use crate::testing::ids::VILLAGER_MALE;
use crate::world::load::fixtures::{
    action_list, building_mut, building_object, combat_object, static_mut,
};
use crate::world::load::sub_actions::{
    DefaultStart, default_task, gather_point, own_build_task, own_fly_task, production,
    saved_attack,
};
use genie_import::dat::{DefaultAction, MasterTable};
use genie_save::generated::world::{
    Action, PlayerObjects, RgeAction, RgeActionAttack, RgeActionByKind, RgeActionMoveTo, RgeObject,
    TribeActionMakeObj, TribeActionMakeTech, TribeBuildingObjectEntry1,
};

/// A make action (type 102) of master 83, `work_done` 0.5, making (state 6), paid.
fn make() -> Action {
    Action {
        tribe_action_make_obj: Some(Box::new(TribeActionMakeObj {
            rge_action: RgeAction {
                state: 6,
                sub_action_value: 1,
                ..RgeAction::default()
            },
            obj_id: 83,
            work_done: 0.5,
            ..TribeActionMakeObj::default()
        })),
        ..Action::default()
    }
}

/// A queue slot of master 83, two of them, its first cost 50 food.
fn slot() -> TribeBuildingObjectEntry1 {
    let mut costs = [0; 24];
    costs[4..8].copy_from_slice(&50.0_f32.to_le_bytes());
    TribeBuildingObjectEntry1 {
        production_queue_master_unit_id: 83,
        production_queue_unit_count: 2,
        production_queue_costs: costs,
        ..TribeBuildingObjectEntry1::default()
    }
}

#[test]
fn building_production_holds_its_slots_and_make_action() {
    let mut town_centre = building_object(10, 109);
    let tribe_building_object = building_mut(&mut town_centre);
    tribe_building_object.production_queue = vec![slot(), slot()];
    tribe_building_object.production_queue_count = 1;
    tribe_building_object.production_queue_enabled = 1;
    tribe_building_object.production_queue_actions =
        action_list(vec![(ActionKind::MAKE_OBJECT, make())]);
    let production = production(&town_centre).unwrap().unwrap();
    assert_eq!(production.queue.slots.len(), 2);
    assert_eq!(production.queue.slots[1].serial.get(), 1);
    assert_eq!(
        production.queue.slots[0].costs[0],
        (Some(attribute::FOOD), Amount::new(50.0))
    );
    assert_eq!(
        (
            production.queue.count.get(),
            production.queue.next_serial.get()
        ),
        (1, 2)
    );
    assert!(production.queue.enabled);
    let Some(Making::Unit(made)) = production.action else {
        panic!("a make action")
    };
    assert_eq!(
        (made.state, made.paid, made.master),
        (MakeObjState::Making, true, Some(VILLAGER_MALE))
    );
}

#[test]
fn research_is_production_action_too() {
    let mut university = building_object(11, 209);
    let research = Action {
        tribe_action_make_tech: Some(Box::new(TribeActionMakeTech {
            tech_id: 22,
            research_progress: 0.25,
            ..TribeActionMakeTech::default()
        })),
        ..Action::default()
    };
    building_mut(&mut university).production_queue_actions =
        action_list(vec![(ActionKind::MAKE_TECH, research)]);
    let production = production(&university).unwrap().unwrap();
    assert!(
        matches!(production.action, Some(Making::Research(make_tech)) if make_tech.tech == TechId::try_from(22).unwrap() && !make_tech.paid && make_tech.state == MakeTechState::New)
    );
}

/// A make action saved in a state no make action has (2) fails the read.
#[test]
fn make_action_in_state_of_none_fails() {
    let mut barracks = building_object(11, 12);
    let mut made = make();
    if let Some(tribe_action_make_obj) = made.tribe_action_make_obj.as_deref_mut() {
        tribe_action_make_obj.rge_action.state = 2;
    }
    building_mut(&mut barracks).production_queue_actions =
        action_list(vec![(ActionKind::MAKE_OBJECT, made)]);
    assert!(production(&barracks).is_err());
}

/// A research saved in a state no research has (2) fails the read.
#[test]
fn research_in_state_of_none_fails() {
    let mut university = building_object(11, 209);
    let research = Action {
        tribe_action_make_tech: Some(Box::new(TribeActionMakeTech {
            rge_action: RgeAction {
                state: 2,
                ..RgeAction::default()
            },
            tech_id: 22,
            ..TribeActionMakeTech::default()
        })),
        ..Action::default()
    };
    building_mut(&mut university).production_queue_actions =
        action_list(vec![(ActionKind::MAKE_TECH, research)]);
    assert!(production(&university).is_err());
}

/// A building's gather point takes its saved state: 0 is none, 1 and 0xff read as theirs, a
/// value past a byte fails the read.
#[test]
fn gather_point_takes_its_saved_state() {
    use crate::commands::GatherPointState;
    let with = |state: i32| {
        let mut barracks = building_object(11, 12);
        building_mut(&mut barracks).gather_point_exists = state;
        gather_point(&barracks).map(|gather_point| {
            gather_point.map(|barracks_gather_point| barracks_gather_point.state)
        })
    };
    assert_eq!(with(0), Ok(None));
    assert_eq!(with(1), Ok(Some(GatherPointState::Point)));
    assert_eq!(with(0xff), Ok(Some(GatherPointState::Kept)));
    assert!(with(0x100).is_err());
    assert_eq!(gather_point(&combat_object(5, 83)), Ok(None));
}

/// No building, or one without slots, action or local lists, has no production; its local
/// lists alone make one.
#[test]
fn building_without_anything_to_make_has_no_production() {
    assert!(production(&combat_object(5, 83)).unwrap().is_none());
    let mut house = building_object(12, 70);
    assert!(production(&house).unwrap().is_none());
    building_mut(&mut house).value = vec![3];
    let house_production = production(&house).unwrap().unwrap();
    assert_eq!(
        house_production.disabled_techs,
        [TechId::try_from(3).unwrap()]
    );
}

/// A make or research action without its part, or another action, is not modelled.
#[test]
fn another_production_action_is_unsupported() {
    for (kind, action) in [
        (ActionKind::MAKE_OBJECT, Action::default()),
        (ActionKind::MAKE_TECH, Action::default()),
        (EXPLORE, make()),
    ] {
        let mut building = building_object(13, 109);
        building_mut(&mut building).production_queue_actions = action_list(vec![(kind, action)]);
        assert!(production(&building).is_err(), "type {kind}");
    }
}

/// A combat object `id` of master 83 owned by Gaia.
fn livestock() -> genie_save::Object {
    combat_object(5, 83)
}

fn start(table: &MasterTable) -> Result<Option<DefaultStart>, Unstartable> {
    let mut object = livestock();
    let rge_static_object = static_mut(&mut object).clone();
    default_task(
        &object,
        &rge_static_object,
        &PlayerObjects::default(),
        CivId::GAIA,
        Some(table),
        None,
    )
}

/// The table's default action: livestock's Artifact, a farm's Make, none, a type no factory
/// makes (an animal's Explore, 6); any other is reported.
#[test]
fn the_default_task_starts_what_the_table_names() {
    let table = |action| {
        MasterTable::of_master(
            CivId::GAIA,
            MasterId::of_data(83).unwrap(),
            None,
            FlyMode::of_data(0),
        )
        .with_default_action(CivId::GAIA, MasterId::of_data(83).unwrap(), action)
        .unwrap()
    };
    let artifact = ARTIFACT_KIND;
    assert!(matches!(
        start(&table(DefaultAction::Kind(artifact))),
        Ok(Some(DefaultStart::Artifact(started))) if started.gaia
    ));
    let make = MAKE;
    assert!(matches!(
        start(&table(DefaultAction::Kind(make))),
        Ok(Some(DefaultStart::Make))
    ));
    assert!(matches!(start(&table(DefaultAction::None)), Ok(None)));
    assert!(matches!(
        start(&table(DefaultAction::Kind(ActionKind::HUNT_RANGE))),
        Ok(None)
    ));
    let enter = ENTER;
    assert!(matches!(
        start(&table(DefaultAction::Kind(enter))),
        Err(Unstartable::Unported(action_kind)) if action_kind == enter
    ));
    assert!(matches!(
        start(&table(DefaultAction::NoSuchTask)),
        Err(Unstartable::Unread(UnreadDefault::NoSuchTask))
    ));
    // a bird's Fly reads its master's default task as it starts, not at the load
    let fly = FLY;
    assert!(matches!(
        start(&table(DefaultAction::Kind(fly))),
        Ok(Some(DefaultStart::Fly))
    ));
    // a master the table does not have
    assert!(matches!(
        start(&MasterTable::default()),
        Err(Unstartable::Unread(UnreadDefault::NotInTable))
    ));
}

/// Only action objects have a default task; without a table it is unknown.
#[test]
fn static_object_has_no_default_task() {
    let tree = genie_save::Object {
        rge_object: Some(Box::new(RgeObject {
            rge_static_object: Some(Box::default()),
            ..RgeObject::default()
        })),
        ..genie_save::Object::default()
    };
    let static_part = tree.static_part().unwrap().clone();
    let objects = PlayerObjects::default();
    assert!(matches!(
        default_task(&tree, &static_part, &objects, CivId::GAIA, None, None),
        Ok(None)
    ));
    let object = livestock();
    let rge_static_object = object.static_part().unwrap().clone();
    assert!(matches!(
        default_task(
            &object,
            &rge_static_object,
            &objects,
            CivId::GAIA,
            None,
            None
        ),
        Err(Unstartable::Unread(UnreadDefault::NoMasterTable))
    ));
}

/// An object of its own master starts that master's default task, the task at its index, with
/// no table; with the record unread, the start fails.
#[test]
fn own_master_default_task_is_its_own() {
    let mut object = livestock();
    static_mut(&mut object).rge_master_static_object = Some(Box::default());
    let rge_static_object = static_mut(&mut object).clone();
    let objects = PlayerObjects::default();
    let sprite = SpriteId::named(1200);
    let own = |default_task| Master {
        action: Some(ActionPart {
            default_task,
            tasks: vec![Task {
                action_kind: ARTIFACT_KIND,
                sprites: TaskSprites {
                    moving: Some(sprite),
                    ..TaskSprites::default()
                },
                ..Task::default()
            }],
            ..ActionPart::default()
        }),
        ..Master::default()
    };
    let start = |own: Option<&Master>| {
        default_task(
            &object,
            &rge_static_object,
            &objects,
            CivId::GAIA,
            None,
            own,
        )
    };
    assert!(matches!(
        start(Some(&own(TaskIndex::of_data(0)))),
        Ok(Some(DefaultStart::Artifact(started))) if started.task.and_then(|task| task.sprites.moving) == Some(sprite)
    ));
    assert!(matches!(start(Some(&own(None))), Ok(None)));
    assert!(matches!(
        start(Some(&own(TaskIndex::of_data(1)))),
        Err(Unstartable::Unread(UnreadDefault::NoSuchTask))
    ));
    assert!(matches!(
        start(None),
        Err(Unstartable::Unread(UnreadDefault::OwnMaster))
    ));
}

/// An Attack (type 9) of range 5 in state `state`, its sub-actions `subs`.
fn attack(state: u8, subs: Vec<(ActionKind, Action)>) -> Action {
    Action {
        rge_action_by_kind: Some(Box::new(RgeActionByKind {
            rge_action_attack: Some(Box::new(RgeActionAttack {
                rge_action: RgeAction {
                    state,
                    target_id: 700,
                    sub_actions: action_list(subs),
                    ..RgeAction::default()
                },
                range: 5.0,
                ..RgeActionAttack::default()
            })),
            ..RgeActionByKind::default()
        })),
        ..Action::default()
    }
}

/// A quiet `MoveTo` (type 1) in state 4 toward (3, 4).
fn move_to() -> Action {
    Action {
        rge_action_by_kind: Some(Box::new(RgeActionByKind {
            rge_action_move_to: Some(Box::new(RgeActionMoveTo {
                rge_action: RgeAction {
                    state: 4,
                    target_x: 3.0,
                    target_y: 4.0,
                    short_val_2: -1,
                    // quiet (`+0x60` 1), as the Attack makes it
                    sub_action_value: 1,
                    ..RgeAction::default()
                },
                ..RgeActionMoveTo::default()
            })),
            ..RgeActionByKind::default()
        })),
        ..Action::default()
    }
}

/// The Attack's target, state and range; in states 5 and 11 its `MoveTo`'s point is where it
/// walks, and the `MoveTo` comes with its state.
#[test]
fn saved_attack_reads_its_target_range_and_move_to() {
    let graphics = Graphics::new();
    let graphics_attack =
        saved_attack(&attack(5, vec![(MOVE_TO, move_to())]), Some(&graphics)).unwrap();
    assert_eq!(
        (graphics_attack.target, graphics_attack.state.bare()),
        (
            Some(AttackTargetId::new(object_id(700))),
            AttackState::GotoSeen { move_to: None }
        )
    );
    // the MoveTo is quiet and holds its own state, point and range
    let move_to_action = graphics_attack.state.move_to().copied().unwrap();
    assert_eq!(
        (
            move_to_action.state,
            move_to_action.point.x(),
            move_to_action.point.y(),
            move_to_action.voice
        ),
        (MoveTo::Moving, 3.0, 4.0, Voice::Quiet)
    );
    assert_eq!(graphics_attack.range.get().to_bits(), 5.0_f32.to_bits());
    let plain = saved_attack(&attack(2, vec![]), Some(&graphics)).unwrap();
    assert_eq!(plain.state, AttackState::Killed);
    // a MoveTo belongs to a walk: in any other state it is no state of the game
    let stray = saved_attack(&attack(6, vec![(MOVE_TO, move_to())]), Some(&graphics)).unwrap_err();
    assert_eq!(
        (stray.kind(), stray.reason()),
        (
            crate::ErrorKind::InvalidInput,
            "a MoveTo of an Attack that does not walk"
        )
    );
    assert!(
        saved_attack(
            &attack(2, vec![(ATTACK, attack(1, vec![]))]),
            Some(&graphics)
        )
        .is_err()
    );
    assert!(saved_attack(&attack(2, vec![]), None).is_err());
    assert!(saved_attack(&move_to(), Some(&graphics)).is_err());
    assert!(saved_attack(&Action::default(), Some(&graphics)).is_err());
}

/// An object of its own master reads its saved actions' Fly and Build tasks from that master
/// (the object's master pointer, `+0xa0`): the first of the type, the Build with the master's
/// work rate; a master without one is reported.
#[test]
fn own_master_gives_the_fly_and_build_tasks() {
    let task = |id, action_kind| Task {
        id: TaskId::of_task(id),
        action_kind,
        work_range: Distance::new(1.5),
        ..Task::default()
    };
    let own = Master {
        action: Some(ActionPart {
            tasks: vec![
                task(0, ARTIFACT_KIND),
                task(1, FLY),
                task(2, ActionKind::BUILD),
            ],
            work_rate: WorkRate::new(0.75),
            ..ActionPart::default()
        }),
        ..Master::default()
    };
    assert_eq!(own_fly_task(&own).map(|fly| fly.id), Ok(TaskId::of_task(1)));
    let build = own_build_task(&own).unwrap();
    assert_eq!(
        (build.id, build.range, build.work_rate),
        (TaskId::of_task(2), Distance::new(1.5), WorkRate::new(0.75))
    );
    let none = Master::default();
    assert_eq!(
        own_fly_task(&none).map_err(|error| error.reason()),
        Err("a bird whose master has no Fly task")
    );
    assert_eq!(
        own_build_task(&none)
            .map(|_| ())
            .map_err(|error| error.reason()),
        Err("a builder whose master has no Build task")
    );
}
