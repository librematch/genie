use crate::combat::AttackCount;
use crate::combat::missile::ShooterId;
use crate::construction::Annexes;
use crate::garrison::NestedCount;
use crate::movement::explore::EXPLORE;
use crate::objects::ObjectId;
use crate::objects::layers::{DefaultTask, UnreadDefault, Unstartable};
use crate::objects::masters::FlyMode;
use crate::objects::masters::MasterId;
use crate::orders::town_bell::TownBell;
use crate::ownership::artifact::ARTIFACT_KIND;
use crate::players::CivId;
use crate::technology::TechId;
use crate::test_ids::object_id;
use crate::world::load::fixtures::{
    action_list, building_mut, building_object, combat_master, combat_mut, combat_object,
    missile_tuning, static_mut, with_masters,
};
use crate::world::load::object::building::annexes;
use crate::world::load::object::object as saved_unit_object;
use crate::world::load::object::object;
use genie_import::dat::{DefaultAction, MasterTable};
use genie_save::generated::world::{
    Action, PlayerObjects, RgeAction, RgeActionByKind, RgeMissileObject, RgeObject,
};

/// A two-player game's players with Gaia.
const PLAYERS: usize = 3;

/// An object without a static part is no unit the slice can run.
#[test]
fn an_object_without_its_static_part_is_unmodelled() {
    let object = object(
        &genie_save::Object::default(),
        &PlayerObjects::default(),
        &[],
        CivId::GAIA,
        None,
        PLAYERS,
        &missile_tuning(),
    );
    assert_eq!(object.unit.id, ObjectId::UNSAVED);
    assert!(object.parts.unread.is_some());
}

/// A saved building's gather point and production lists are its object's parts, not its
/// row's; a gather state the import cannot read leaves the part out and is the row's
/// unmodelled part. A building without a successor master record has no dead master.
#[test]
fn saved_building_parts_are_its_object_s() {
    use crate::commands::GatherPointState;
    let whole = |state: i32| {
        let mut barracks = building_object(11, 12);
        let tribe_building_object = building_mut(&mut barracks);
        tribe_building_object.gather_point_exists = state;
        tribe_building_object.gather_point_x = 3.5;
        tribe_building_object.gather_point_target = -1;
        tribe_building_object.value = vec![3];
        object(
            &barracks,
            &PlayerObjects::default(),
            &[],
            CivId::GAIA,
            None,
            PLAYERS,
            &missile_tuning(),
        )
    };
    let object = whole(1);
    let gather_point = object.parts.gather_point.unwrap();
    assert_eq!(
        (
            gather_point.state,
            gather_point.point.x(),
            gather_point.target
        ),
        (GatherPointState::Point, 3.5, None)
    );
    assert!(object.parts.dead_master.is_none());
    let disabled = object
        .parts
        .production
        .map(|production| production.disabled_techs);
    assert_eq!(disabled, Some(vec![TechId::try_from(3).unwrap()]));
    let object = whole(0x100);
    assert!(object.parts.gather_point.is_none());
    assert!(object.parts.unread.is_some());
}

/// A save from the start: livestock's default task is the Artifact, which exists from the
/// load on and starts again when its action list empties; a dead sheep (state 5) holds none.
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
        DefaultAction::Kind(ARTIFACT_KIND),
    )
    .unwrap();
    let mut sheep = combat_object(5, 83);
    static_mut(&mut sheep).object_state = 2;
    let object = object(
        &sheep,
        &PlayerObjects::default(),
        &[],
        CivId::GAIA,
        Some(&table),
        PLAYERS,
        &missile_tuning(),
    );
    assert_eq!(object.unit.id, object_id(5));
    assert!(object.parts.actions.list.artifact().is_some());
    assert_eq!(object.parts.actions.default_task, DefaultTask::Artifact);
    // a combat object's mover needs its master, which this save does not hold
    assert!(object.parts.unread.is_some());
    let mut dead = combat_object(6, 83);
    static_mut(&mut dead).object_state = 5;
    let object = saved_unit_object(
        &dead,
        &PlayerObjects::default(),
        &[],
        CivId::GAIA,
        Some(&table),
        PLAYERS,
        &missile_tuning(),
    );
    assert!(object.parts.actions.list.artifact().is_none());
    assert_eq!(object.parts.actions.default_task, DefaultTask::Artifact);
}

/// Without a table the default task is unknown: reported, as the object's.
#[test]
fn without_table_the_default_task_is_reported() {
    let object = object(
        &combat_object(5, 83),
        &PlayerObjects::default(),
        &[],
        CivId::GAIA,
        None,
        PLAYERS,
        &missile_tuning(),
    );
    assert_eq!(
        object.parts.actions.default_task,
        DefaultTask::Fails(Unstartable::Unread(UnreadDefault::NoMasterTable))
    );
    assert!(object.parts.actions.list.artifact().is_none());
}

/// A saved action the slice does not model stops the object.
#[test]
fn saved_action_the_slice_does_not_model_is_reported() {
    let mut scout = combat_object(5, 83);
    let explore = Action {
        rge_action_by_kind: Some(Box::new(RgeActionByKind {
            rge_action_explore: Some(Box::new(RgeAction::default())),
            ..RgeActionByKind::default()
        })),
        ..Action::default()
    };
    combat_mut(&mut scout)
        .rge_combat_object
        .rge_action_object
        .actions = action_list(vec![(EXPLORE, explore)]);
    let object = object(
        &scout,
        &PlayerObjects::default(),
        &[],
        CivId::GAIA,
        None,
        PLAYERS,
        &missile_tuning(),
    );
    assert!(object.parts.unread.is_some());
    assert_ne!(object.parts.actions.default_task, DefaultTask::Fly);
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
    let object = object(
        &arrow,
        &PlayerObjects::default(),
        &[],
        CivId::GAIA,
        None,
        PLAYERS,
        &missile_tuning(),
    );
    let (unit, flying) = (object.unit, object.parts.missile.unwrap());
    assert_eq!(flying.fired_from, Some(ShooterId::new(object_id(900))));
    assert!(flying.flying);
    assert_eq!(flying.step.x().to_bits(), 0.5_f32.to_bits());
    assert_eq!(object.parts.combat.attack_count, AttackCount::of_data(2));
    assert_eq!(unit.id, object_id(6));
}

/// A builder's Build takes its master's Build task with the builder's work-rate powerup (its
/// combat layer's) added to the master's work rate.
#[test]
fn build_work_rate_takes_the_builder_powerup() {
    use crate::objects::masters::ActionKind;
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
            task_kind: 1,
            task: Task {
                action_kind: ActionKind::BUILD.get(),
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
    let object = object(
        &builder,
        &with_masters(vec![master]),
        &[],
        CivId::GAIA,
        None,
        PLAYERS,
        &missile_tuning(),
    );
    let build_action = object.parts.actions.list.build().copied().unwrap();
    assert_eq!(
        build_action.task.work_rate.get().to_bits(),
        2.5_f32.to_bits()
    );
    assert_eq!(
        object.parts.combat.work_rate_powerup.get().to_bits(),
        0.5_f32.to_bits()
    );
}

/// The town bell's back-to-work records import with their actions as the object's part; an
/// action value none names is the object's unmodelled part, and the town bell stays a new
/// unit's.
#[test]
fn the_back_to_work_actions_import_as_their_kinds() {
    use crate::unit_ai::AiAction;
    let mut object = combat_object(5, 4);
    let tribe_combat_object = combat_mut(&mut object);
    tribe_combat_object.town_bell_flag = 1;
    tribe_combat_object.back_to_work1_town_bell_action = 0x261;
    let whole = |object: &genie_save::Object| {
        saved_unit_object(
            object,
            &PlayerObjects::default(),
            &[],
            CivId::GAIA,
            None,
            PLAYERS,
            &missile_tuning(),
        )
    };
    let bell = whole(&object).parts.town_bell;
    assert!(bell.flag.is_sheltered());
    assert_eq!(
        (bell.work.action, bell.before_building.action),
        (Some(AiAction::Gather), None)
    );
    combat_mut(&mut object).back_to_work2_town_bell_action = 0x2c6;
    let whole = whole(&object);
    assert_eq!(
        whole.parts.unread.map(|unread| unread.error().kind()),
        Some(crate::ErrorKind::InvalidInput)
    );
    assert_eq!(whole.parts.town_bell, TownBell::default());
}

/// A combat object's `inside_garrison_count` (`+0x9a8`, read at `0x141220585` into the
/// object's byte) imports as its count of what the units inside it hold; an object without the
/// TRIBE combat part (below type 70) holds none.
#[test]
fn inside_garrison_count_imports_as_nested_count() {
    let mut object = combat_object(5, 4);
    combat_mut(&mut object).inside_garrison_count = 3;
    let whole = |object: &genie_save::Object| {
        saved_unit_object(
            object,
            &PlayerObjects::default(),
            &[],
            CivId::GAIA,
            None,
            PLAYERS,
            &missile_tuning(),
        )
    };
    assert_eq!(whole(&object).parts.combat.nested, NestedCount::of_data(3));
    object.tribe_combat_object = None;
    assert_eq!(whole(&object).parts.combat.nested, NestedCount::ZERO);
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
