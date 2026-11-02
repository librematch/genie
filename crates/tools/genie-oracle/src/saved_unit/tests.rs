use crate::fixtures::{
    action_list, building_mut, building_object, combat_master, combat_mut, combat_object,
    with_masters,
};
use crate::masters::{DefaultAction, MasterTable};
use crate::saved_unit::object as saved_unit_object;
use crate::saved_unit::{annexes, object};
use crate::test_ids::object_id;
use genie_core::combat::AttackCount;
use genie_core::combat::missile::ShooterId;
use genie_core::construction::Annexes;
use genie_core::movement::explore::EXPLORE;
use genie_core::objects::ObjectId;
use genie_core::objects::layers::{DefaultTask, UnreadDefault, Unstartable};
use genie_core::objects::masters::FlyMode;
use genie_core::objects::masters::MasterId;
use genie_core::orders::town_bell::TownBell;
use genie_core::ownership::artifact::ARTIFACT_TYPE;
use genie_core::players::CivId;
use genie_core::technology::TechId;
use genie_save::generated::world::{
    Action, PlayerObjects, RgeAction, RgeActionByType, RgeMissileObject, RgeObject,
};

/// A two-player game's players with Gaia.
const PLAYERS: usize = 3;

/// An object without a static part is no unit the slice can run.
#[test]
fn an_object_without_its_static_part_is_unmodelled() {
    let u = object(
        &genie_save::Object::default(),
        &PlayerObjects::default(),
        &[],
        CivId::GAIA,
        None,
        PLAYERS,
    );
    assert_eq!(u.unit.id, ObjectId::UNSAVED);
    assert!(u.parts.unread.is_some());
}

/// A saved building's gather point and production lists are its object's parts, not its
/// row's; a gather state the import cannot read leaves the part out and is the row's
/// unmodelled part. The dead master is the import's gap.
#[test]
fn saved_building_parts_are_its_object_s() {
    use genie_core::commands::GatherPointState;
    let whole = |state: i32| {
        let mut barracks = building_object(11, 12);
        let b = building_mut(&mut barracks);
        b.gather_point_exists = state;
        b.gather_point_x = 3.5;
        b.gather_point_target = -1;
        b.value = vec![3];
        object(
            &barracks,
            &PlayerObjects::default(),
            &[],
            CivId::GAIA,
            None,
            PLAYERS,
        )
    };
    let o = whole(1);
    let g = o.parts.gather_point.unwrap();
    assert_eq!(
        (g.state, g.point.x(), g.target),
        (GatherPointState::POINT, 3.5, None)
    );
    assert!(o.parts.dead_master.is_none());
    let disabled = o.parts.production.map(|p| p.disabled_techs);
    assert_eq!(disabled, Some(vec![TechId::try_from(3).unwrap()]));
    let o = whole(0x100);
    assert!(o.parts.gather_point.is_none());
    assert!(o.parts.unread.is_some());
}

/// A save from the start: livestock's default task is the Artifact, which exists from the
/// load on and starts again when its action list empties.
#[test]
fn livestock_starts_with_its_artifact() {
    let table = MasterTable::of_master(
        CivId::GAIA,
        MasterId::of_data(83).unwrap(),
        None,
        FlyMode::of_data(0),
    )
    .with_default_action(
        CivId::GAIA,
        MasterId::of_data(83).unwrap(),
        DefaultAction::Type(ARTIFACT_TYPE),
    )
    .unwrap();
    let sheep = combat_object(5, 83);
    let u = object(
        &sheep,
        &PlayerObjects::default(),
        &[],
        CivId::GAIA,
        Some(&table),
        PLAYERS,
    );
    assert_eq!(u.unit.id, object_id(5));
    assert!(u.parts.actions.list.artifact().is_some());
    assert_eq!(u.parts.actions.default_task, DefaultTask::Artifact);
    // a combat object's mover needs its master, which this save does not hold
    assert!(u.parts.unread.is_some());
}

/// Without a table the default task is unknown: reported, as the object's.
#[test]
fn without_table_the_default_task_is_reported() {
    let u = object(
        &combat_object(5, 83),
        &PlayerObjects::default(),
        &[],
        CivId::GAIA,
        None,
        PLAYERS,
    );
    assert_eq!(
        u.parts.actions.default_task,
        DefaultTask::Fails(Unstartable::Unread(UnreadDefault::NoMasterTable))
    );
    assert!(u.parts.actions.list.artifact().is_none());
}

/// A saved action the slice does not model stops the object.
#[test]
fn saved_action_the_slice_does_not_model_is_reported() {
    let mut scout = combat_object(5, 83);
    let explore = Action {
        rge_action_by_type: Some(Box::new(RgeActionByType {
            rge_action_explore: Some(Box::new(RgeAction::default())),
            ..RgeActionByType::default()
        })),
        ..Action::default()
    };
    combat_mut(&mut scout)
        .rge_combat_object
        .rge_action_object
        .actions = action_list(vec![(EXPLORE, explore)]);
    let u = object(
        &scout,
        &PlayerObjects::default(),
        &[],
        CivId::GAIA,
        None,
        PLAYERS,
    );
    assert!(u.parts.unread.is_some());
    assert_ne!(u.parts.actions.default_task, DefaultTask::Fly);
}

/// A missile without its action (it met its target) flies on with its own fields.
#[test]
fn missile_without_its_action_keeps_flying() {
    let mut missile = RgeMissileObject {
        max_range: 7.0,
        fired_from_id: 900,
        ..RgeMissileObject::default()
    };
    missile.rge_combat_object.attack_count = 2;
    missile
        .rge_combat_object
        .rge_action_object
        .rge_moving_object
        .velocity_x = 0.5;
    missile
        .rge_combat_object
        .rge_action_object
        .rge_moving_object
        .rge_animated_object
        .rge_static_object
        .id = 6;
    let arrow = genie_save::Object {
        rge_object: Some(Box::new(RgeObject {
            rge_missile_object: Some(Box::new(missile)),
            ..RgeObject::default()
        })),
        ..genie_save::Object::default()
    };
    let o = object(
        &arrow,
        &PlayerObjects::default(),
        &[],
        CivId::GAIA,
        None,
        PLAYERS,
    );
    let (u, flying) = (o.unit, o.parts.missile.unwrap());
    assert_eq!(flying.fired_from, Some(ShooterId::new(object_id(900))));
    assert!(flying.flying);
    assert_eq!(flying.step.x().to_bits(), 0.5_f32.to_bits());
    assert_eq!(o.parts.combat.attack_count, AttackCount::of_data(2));
    assert_eq!(u.id, object_id(6));
}

/// A builder's Build takes its master's Build task with the builder's work-rate powerup (its
/// combat layer's) added to the master's work rate.
#[test]
fn build_work_rate_takes_the_builder_powerup() {
    use genie_core::objects::masters::ActionKind;
    use genie_save::generated::world::{Task, TaskList, TaskListEntry1, TribeActionBuild};
    let mut master = combat_master(83);
    let acting = &mut master
        .tribe_master_combat_object
        .as_deref_mut()
        .expect("a combat master")
        .rge_master_combat_object
        .rge_master_action_object;
    acting.work_rate = 2.0;
    acting.tasks = Some(Box::new(TaskList {
        list_num: 1,
        list: vec![TaskListEntry1 {
            task_type: 1,
            task: Task {
                action_type: ActionKind::BUILD.get(),
                ..Task::default()
            },
        }],
    }));
    let mut builder = combat_object(5, 83);
    let combat = combat_mut(&mut builder);
    combat.work_rate_powerup = 0.5;
    let build = Action {
        tribe_action_build: Some(Box::new(TribeActionBuild {
            rge_action: RgeAction {
                state: 4,
                target_id: 30,
                target2_id: -1,
                ..RgeAction::default()
            },
            added_to_num_builders: 1,
        })),
        ..Action::default()
    };
    combat.rge_combat_object.rge_action_object.actions =
        action_list(vec![(ActionKind::BUILD, build)]);
    let o = object(
        &builder,
        &with_masters(vec![master]),
        &[],
        CivId::GAIA,
        None,
        PLAYERS,
    );
    let b = o.parts.actions.list.build().copied().unwrap();
    assert_eq!(b.task.work_rate.get().to_bits(), 2.5_f32.to_bits());
    assert_eq!(
        o.parts.combat.work_rate_powerup.get().to_bits(),
        0.5_f32.to_bits()
    );
}

/// The town bell's back-to-work records import with their actions as the object's part; an
/// action value none names is the object's unmodelled part, and the town bell stays a new
/// unit's.
#[test]
fn the_back_to_work_actions_import_as_their_kinds() {
    use genie_core::unit_ai::AiAction;
    let mut object = combat_object(5, 4);
    let c = combat_mut(&mut object);
    c.town_bell_flag = 1;
    c.back_to_work1_town_bell_action = 0x261;
    let whole = |object: &genie_save::Object| {
        saved_unit_object(
            object,
            &PlayerObjects::default(),
            &[],
            CivId::GAIA,
            None,
            PLAYERS,
        )
    };
    let bell = whole(&object).parts.town_bell;
    assert!(bell.flag);
    assert_eq!(
        (bell.work.action, bell.before_building.action),
        (Some(AiAction::Gather), None)
    );
    combat_mut(&mut object).back_to_work2_town_bell_action = 0x2c6;
    let o = whole(&object);
    assert_eq!(
        o.parts.unread.map(|e| e.error().kind()),
        Some(genie_core::ErrorKind::InvalidInput)
    );
    assert_eq!(o.parts.town_bell, TownBell::default());
}

/// The save's linked object ids fill a main building's four annex slots in their order, −1 an
/// empty slot.
#[test]
fn the_linked_object_ids_fill_the_annex_slots_in_order() {
    assert_eq!(
        annexes(&[12, -1, 14, -1]),
        Annexes::new([Some(object_id(12)), None, Some(object_id(14)), None])
    );
    assert_eq!(annexes(&[]), Annexes::NONE);
}
