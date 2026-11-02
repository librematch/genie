use crate::fixtures::{action_list, building_mut, building_object, combat_object, static_mut};
use crate::masters::{DefaultAction, MasterTable};
use crate::sub_actions::{DefaultStart, default_task, gather_point, production, saved_attack};
use crate::test_ids::VILLAGER_MALE;
use crate::test_ids::attribute;
use crate::test_ids::object_id;
use genie_core::combat::AttackTargetId;
use genie_core::combat::attack::ATTACK;
use genie_core::combat::attack::AttackState;
use genie_core::garrison::ENTER;
use genie_core::gathering::make::MAKE;
use genie_core::movement::explore::EXPLORE;
use genie_core::movement::fly::FLY;
use genie_core::movement::move_to::MOVE_TO;
use genie_core::movement::move_to::MoveTo;
use genie_core::objects::Graphics;
use genie_core::objects::Voice;
use genie_core::objects::layers::{UnreadDefault, Unstartable};
use genie_core::objects::masters::ActionKind;
use genie_core::objects::masters::FlyMode;
use genie_core::objects::masters::MasterId;
use genie_core::ownership::artifact::ARTIFACT_TYPE;
use genie_core::players::Amount;
use genie_core::players::CivId;
use genie_core::production::{MakeObjState, MakeTechState, Making};
use genie_core::technology::TechId;
use genie_save::generated::world::{
    Action, PlayerObjects, RgeAction, RgeActionAttack, RgeActionByType, RgeActionMoveTo, RgeObject,
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
    let b = building_mut(&mut town_centre);
    b.production_queue = vec![slot(), slot()];
    b.production_queue_count = 1;
    b.production_queue_enabled = 1;
    b.production_queue_actions = action_list(vec![(ActionKind::MAKE_OBJECT, make())]);
    let p = production(&town_centre).unwrap().unwrap();
    assert_eq!(p.queue.slots.len(), 2);
    assert_eq!(p.queue.slots[1].serial.get(), 1);
    assert_eq!(
        p.queue.slots[0].costs[0],
        (Some(attribute::FOOD), Amount::new(50.0))
    );
    assert_eq!((p.queue.count.get(), p.queue.next_serial.get()), (1, 2));
    assert!(p.queue.enabled);
    let Some(Making::Unit(made)) = p.action else {
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
    let p = production(&university).unwrap().unwrap();
    assert!(
        matches!(p.action, Some(Making::Research(r)) if r.tech == TechId::try_from(22).unwrap() && !r.paid && r.state == MakeTechState::New)
    );
}

/// A make action saved in a state no make action has (2) fails the read.
#[test]
fn make_action_in_state_of_none_fails() {
    let mut barracks = building_object(11, 12);
    let mut made = make();
    if let Some(m) = made.tribe_action_make_obj.as_deref_mut() {
        m.rge_action.state = 2;
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
    use genie_core::commands::GatherPointState;
    let with = |state: i32| {
        let mut barracks = building_object(11, 12);
        building_mut(&mut barracks).gather_point_exists = state;
        gather_point(&barracks).map(|g| g.map(|g| g.state))
    };
    assert_eq!(with(0), Ok(None));
    assert_eq!(with(1), Ok(Some(GatherPointState::POINT)));
    assert_eq!(with(0xff), Ok(Some(GatherPointState::KEPT)));
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
    let p = production(&house).unwrap().unwrap();
    assert_eq!(p.disabled_techs, [TechId::try_from(3).unwrap()]);
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
    let s = static_mut(&mut object).clone();
    default_task(
        &object,
        &s,
        &PlayerObjects::default(),
        CivId::GAIA,
        Some(table),
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
    let artifact = ARTIFACT_TYPE;
    assert!(matches!(
        start(&table(DefaultAction::Type(artifact))),
        Ok(Some(DefaultStart::Artifact(a))) if a.gaia
    ));
    let make = MAKE;
    assert!(matches!(
        start(&table(DefaultAction::Type(make))),
        Ok(Some(DefaultStart::Make))
    ));
    assert!(matches!(start(&table(DefaultAction::None)), Ok(None)));
    assert!(matches!(
        start(&table(DefaultAction::Type(ActionKind::HUNT_RANGE))),
        Ok(None)
    ));
    let enter = ENTER;
    assert!(matches!(
        start(&table(DefaultAction::Type(enter))),
        Err(Unstartable::Unported(t)) if t == enter
    ));
    assert!(matches!(
        start(&table(DefaultAction::NoSuchTask)),
        Err(Unstartable::Unread(UnreadDefault::NoSuchTask))
    ));
    // a bird's Fly reads its master's default task as it starts, not at the load
    let fly = FLY;
    assert!(matches!(
        start(&table(DefaultAction::Type(fly))),
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
    let s = tree.static_part().unwrap().clone();
    let objects = PlayerObjects::default();
    assert!(matches!(
        default_task(&tree, &s, &objects, CivId::GAIA, None),
        Ok(None)
    ));
    let object = livestock();
    let s = object.static_part().unwrap().clone();
    assert!(matches!(
        default_task(&object, &s, &objects, CivId::GAIA, None),
        Err(Unstartable::Unread(UnreadDefault::NoMasterTable))
    ));
}

/// An Attack (type 9) of range 5 in state `state`, its sub-actions `subs`.
fn attack(state: u8, subs: Vec<(ActionKind, Action)>) -> Action {
    Action {
        rge_action_by_type: Some(Box::new(RgeActionByType {
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
            ..RgeActionByType::default()
        })),
        ..Action::default()
    }
}

/// A `MoveTo` (type 1) in state 4 toward (3, 4).
fn move_to() -> Action {
    Action {
        rge_action_by_type: Some(Box::new(RgeActionByType {
            rge_action_move_to: Some(Box::new(RgeActionMoveTo {
                rge_action: RgeAction {
                    state: 4,
                    target_x: 3.0,
                    target_y: 4.0,
                    short_val_2: -1,
                    ..RgeAction::default()
                },
                ..RgeActionMoveTo::default()
            })),
            ..RgeActionByType::default()
        })),
        ..Action::default()
    }
}

/// The Attack's target, state and range; in states 5 and 11 its `MoveTo`'s point is where it
/// walks, and the `MoveTo` comes with its state.
#[test]
fn saved_attack_reads_its_target_range_and_move_to() {
    let graphics = Graphics::new();
    let a = saved_attack(&attack(5, vec![(MOVE_TO, move_to())]), Some(&graphics)).unwrap();
    assert_eq!(
        (a.target, a.state.bare()),
        (
            Some(AttackTargetId::new(object_id(700))),
            AttackState::GotoSeen { move_to: None }
        )
    );
    // the MoveTo is quiet and holds its own state, point and range
    let m = a.state.move_to().copied().unwrap();
    assert_eq!(
        (m.state, m.point.x(), m.point.y(), m.voice),
        (MoveTo::Moving, 3.0, 4.0, Voice::Quiet)
    );
    assert_eq!(a.range.get().to_bits(), 5.0_f32.to_bits());
    let plain = saved_attack(&attack(2, vec![]), Some(&graphics)).unwrap();
    assert_eq!(plain.state, AttackState::Killed);
    // a MoveTo belongs to a walk: in any other state it is no state of the game
    let stray = saved_attack(&attack(6, vec![(MOVE_TO, move_to())]), Some(&graphics)).unwrap_err();
    assert_eq!(
        (stray.kind(), stray.reason()),
        (
            genie_core::ErrorKind::InvalidInput,
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
