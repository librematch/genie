//! The work command's Build and Repair, their retakes and targets, and the unit AI's repair,
//! gather and hunt commands.

use super::*;
use crate::construction::BuilderCount;
use crate::construction::build::BuildState;
use crate::construction::repair::REPAIR;
use crate::construction::repair::REPAIR_TARGET_UNHELD;
use crate::gathering::DropSiteId;
use crate::gathering::GatherTargetId;
use crate::gathering::HuntTargetId;
use crate::gathering::gather::{GatherAction, GatherState};
use crate::gathering::hunt::{HuntAction, HuntState};
use crate::map::Distance;
use crate::objects::ObjectState;
use crate::objects::WorkerCount;
use crate::objects::masters::ActionKind;
use crate::objects::masters::MasterId;
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::players::PlayerKind;
use crate::test_ids::object_id;
use crate::testing::class;
use crate::testing::ids;
use crate::unit_ai::NotifyKind;
use crate::world::Seconds;
use crate::world::tests::{
    forage_world, house_builder, hunt_world, repair_world, repairer_world, sent_off_world,
};

/// The house 10028 of [`repair_world`] and [`sent_off_world`].
const HOUSE: ObjectId = object_id(10028);
/// The villager 9175 at slot 0 of player 1.
const VILLAGER: ObjectId = object_id(9175);
/// The player of the worlds' villager.
const PLAYER: PlayerId = PlayerId::new(1);
/// The villager's slot.
const SLOT: ListIndex = ListIndex::FIRST;

/// The task of `action_kind` of player 1's master `master` in `world`.
fn task_of(world: &World, master: MasterId, action_kind: ActionKind) -> Task {
    world.model.masters[1]
        .get(master)
        .unwrap()
        .action_part()
        .tasks
        .iter()
        .find(|task| task.action_kind == action_kind)
        .copied()
        .unwrap()
}

/// The villager's Repair.
fn repair_of(world: &World) -> Option<RepairAction> {
    world.parts.actions_of(VILLAGER).list.repair().copied()
}

/// Object `id`'s workers.
fn workers(world: &World, id: ObjectId) -> WorkerCount {
    world.object(id).unwrap().1.workers
}

/// The house's repairers or builders.
fn builders(world: &World) -> BuilderCount {
    world.parts.repairers_of(HOUSE)
}

/// The villager's unit AI's order, action and target.
fn told(world: &World) -> (Option<OrderKind>, Option<AiAction>, Option<ObjectId>) {
    let ai = world.parts.unit_ai.get(VILLAGER).unwrap();
    (ai.current_order, ai.current_action, ai.current_target)
}

/// The work command's Build (`FUN_140aca4b0` case 0x65) of a villager without a unit AI: no
/// notify, but the Build takes the foundation (slot 0x178, a worker more) and replaces the
/// villager's actions (slot 0x808), walking to it (state 4).
#[test]
fn build_start_without_unit_ai_installs_build_on_foundation() {
    let mut world = sent_off_world(vec![]);
    world.object_mut(VILLAGER).unwrap().1.life.master_id = ids::BUILDER;
    world.parts.set_unit_ai(VILLAGER, None);
    let task = task_of(&world, ids::BUILDER, ActionKind::BUILD);
    world
        .start_build(PLAYER, SLOT, HOUSE, &task, Position::ORIGIN, Notifies::Kept)
        .unwrap();
    let build = world
        .parts
        .actions_of(VILLAGER)
        .list
        .build()
        .copied()
        .unwrap();
    assert_eq!(
        (build.target, build.state.bare()),
        (
            Some(FoundationId::new(HOUSE)),
            BuildState::Move { move_to: None }
        )
    );
    assert_eq!(workers(&world, HOUSE), WorkerCount::of_data(1));
}

/// The work command's Build of a builder in state 3 (`Dying`): the Build takes its foundation
/// (slot 0x178), but the install (slot 0x808, `FUN_140a977e0`) deletes it unstarted, which lets
/// the foundation go again: no action, no worker.
#[test]
fn build_start_of_dying_builder_deletes_new_build_and_lets_foundation_go() {
    let mut world = sent_off_world(vec![]);
    let villager = world.object_mut(VILLAGER).unwrap().1;
    (villager.life.master_id, villager.life.state) = (ids::BUILDER, ObjectState::Dying);
    world.parts.set_unit_ai(VILLAGER, None);
    let task = task_of(&world, ids::BUILDER, ActionKind::BUILD);
    world
        .start_build(PLAYER, SLOT, HOUSE, &task, Position::ORIGIN, Notifies::Kept)
        .unwrap();
    assert!(world.parts.actions_of(VILLAGER).list.is_empty());
    assert_eq!(workers(&world, HOUSE), WorkerCount::ZERO);
}

/// The notifies the villager's unit AI heard.
fn heard(world: &World) -> Vec<(NotifyKind, [i32; 3])> {
    world
        .parts
        .unit_ai
        .get(VILLAGER)
        .unwrap()
        .notifies
        .iter()
        .map(|notify| (notify.kind, notify.params))
        .collect()
}

/// The work command's Build on a foundation of another player (the house given to Gaia): the
/// constructor (`FUN_140aad4e0`) compares the builder's player's id with the foundation's
/// (`+0x718`), so it holds no target and leaves its point at (0, 0, 0) (`FUN_140a73e90`): nothing
/// is taken. The install's start (slot 0x28 with 1, `0x140aadd80` without a target) waits for
/// work (`set_state(3)`): the unit AI, with no order queued, hears notify 0x202 with the build's
/// action value.
#[test]
fn build_start_on_foundation_of_another_player_waits_for_work() {
    let mut world = sent_off_world(vec![]);
    world.object_mut(VILLAGER).unwrap().1.life.master_id = ids::BUILDER;
    let house = world.players[1]
        .objects
        .swap_remove(ListIndex::new(1))
        .unwrap();
    world.players[0].objects.push(house);
    world.reindex();
    let task = task_of(&world, ids::BUILDER, ActionKind::BUILD);
    world
        .start_build(PLAYER, SLOT, HOUSE, &task, Position::ORIGIN, Notifies::Kept)
        .unwrap();
    let build = world
        .parts
        .actions_of(VILLAGER)
        .list
        .build()
        .copied()
        .unwrap();
    assert_eq!(
        (build.target, build.state, build.point),
        (None, BuildState::Idle, Position::ORIGIN)
    );
    assert_eq!(workers(&world, HOUSE), WorkerCount::ZERO);
    assert_eq!(
        heard(&world),
        [(NotifyKind::REQUIRES_SEARCH, [AiAction::Build.value(), 0, 0])]
    );
}

/// The work command's Build on its player's foundation past state 6: the constructor's
/// `FUN_140a7a230` holds none and takes nothing, but the point takes the foundation's place; the
/// install's start then waits for work (`set_state(3)`, notify 0x202). One in state 6 is held
/// and taken, and the start walks to it (`set_state(4)`).
#[test]
fn build_start_on_foundation_past_state_6_waits_for_work() {
    for state in [ObjectState::Undead, ObjectState::Removed, ObjectState::Gone] {
        let mut world = sent_off_world(vec![]);
        world.object_mut(VILLAGER).unwrap().1.life.master_id = ids::BUILDER;
        world.object_mut(HOUSE).unwrap().1.life.state = state;
        let task = task_of(&world, ids::BUILDER, ActionKind::BUILD);
        world
            .start_build(PLAYER, SLOT, HOUSE, &task, Position::ORIGIN, Notifies::Kept)
            .unwrap();
        let build = world
            .parts
            .actions_of(VILLAGER)
            .list
            .build()
            .copied()
            .unwrap();
        let waits = [(NotifyKind::REQUIRES_SEARCH, [AiAction::Build.value(), 0, 0])];
        let (target, built, taken, told) = if state == ObjectState::Undead {
            (
                Some(FoundationId::new(HOUSE)),
                BuildState::Move { move_to: None },
                WorkerCount::ONE,
                &[][..],
            )
        } else {
            (None, BuildState::Idle, WorkerCount::ZERO, &waits[..])
        };
        assert_eq!(
            (build.target, build.state.bare(), build.point),
            (target, built, Position::new(30.0, 88.0, 0.0)),
            "{state:?}"
        );
        assert_eq!(workers(&world, HOUSE), taken, "{state:?}");
        assert_eq!(heard(&world), told, "{state:?}");
    }
}

/// The work command's Repair (`FUN_140aca4b0` case 0x6a) of a villager without a unit AI: no
/// notify, but the Repair takes its target and replaces the villager's actions, walking to it.
#[test]
fn repair_start_without_unit_ai_installs_repair_on_target() {
    let mut world = repair_world();
    world.parts.set_unit_ai(VILLAGER, None);
    let task = task_of(&world, ids::BUILDER, REPAIR);
    world
        .start_repair(PLAYER, SLOT, HOUSE, &task, Position::ORIGIN, Notifies::Kept)
        .unwrap();
    let repair = repair_of(&world).unwrap();
    assert_eq!(
        (repair.target, repair.state.bare()),
        (
            Some(RepairTargetId::new(HOUSE)),
            RepairState::Move { move_to: None }
        )
    );
    assert_eq!(workers(&world, HOUSE), WorkerCount::of_data(1));
}

/// The work command's Repair of a repairer in state 3: the install deletes the new Repair
/// unstarted (`FUN_140a977e0`), its target let go again.
#[test]
fn repair_start_of_dying_repairer_deletes_new_repair_and_lets_target_go() {
    let mut world = repair_world();
    world.parts.set_unit_ai(VILLAGER, None);
    world.object_mut(VILLAGER).unwrap().1.life.state = ObjectState::Dying;
    let task = task_of(&world, ids::BUILDER, REPAIR);
    world
        .start_repair(PLAYER, SLOT, HOUSE, &task, Position::ORIGIN, Notifies::Kept)
        .unwrap();
    assert_eq!(repair_of(&world), None);
    assert_eq!(workers(&world, HOUSE), WorkerCount::ZERO);
}

/// The work command's Repair on a target past state 6: the constructor's `FUN_140a7a230` holds
/// none, and its `FUN_140ad9120` reads the null target's master (`0x140ad9161`): the game faults
/// before the take and the unit AI's notify ([`REPAIR_TARGET_UNHELD`]). One in state 6 is held,
/// taken and walked to.
#[test]
fn repair_start_on_target_past_state_6_faults_before_take() {
    for state in [ObjectState::Undead, ObjectState::Removed, ObjectState::Gone] {
        let mut world = repair_world();
        world.object_mut(HOUSE).unwrap().1.life.state = state;
        let task = task_of(&world, ids::BUILDER, REPAIR);
        let started =
            world.start_repair(PLAYER, SLOT, HOUSE, &task, Position::ORIGIN, Notifies::Kept);
        if state == ObjectState::Undead {
            assert_eq!(started, Ok(()));
            assert_eq!(
                repair_of(&world).map(|repair| (repair.target, repair.state.bare())),
                Some((
                    Some(RepairTargetId::new(HOUSE)),
                    RepairState::Move { move_to: None }
                ))
            );
            assert_eq!(workers(&world, HOUSE), WorkerCount::ONE);
        } else {
            assert_eq!(started, Err(REPAIR_TARGET_UNHELD), "{state:?}");
            assert_eq!(repair_of(&world), None, "{state:?}");
            assert_eq!(workers(&world, HOUSE), WorkerCount::ZERO, "{state:?}");
            assert_eq!(told(&world), (None, None, None), "{state:?}");
        }
    }
}

/// [`repair_world`] with the villager's Repair on the house walking to it (state 4), counted
/// among its repairers (`+0x68`), the house counting it.
fn counted_repairer() -> World {
    let mut world = repair_world();
    let task = task_of(&world, ids::BUILDER, REPAIR);
    world
        .start_repair(PLAYER, SLOT, HOUSE, &task, Position::ORIGIN, Notifies::Kept)
        .unwrap();
    world
        .parts
        .actions
        .of_mut(VILLAGER)
        .unwrap()
        .list
        .repair_mut()
        .unwrap()
        .counted = true;
    world
        .parts
        .building_mut(HOUSE)
        .unwrap()
        .construction
        .as_mut()
        .unwrap()
        .builders = BuilderCount::of_data(1);
    world
}

/// `FUN_140adb480` on the Repair's own target while it walks there (state 4): a human player's
/// repairer walks again (`set_state(4)`), leaving the repairers it was counted among; a computer
/// player's keeps its walk and its count.
#[test]
fn repair_retake_of_own_target_walks_human_repairer_again() {
    let retaken = |kind: PlayerKind| {
        let mut world = counted_repairer();
        world.players[1].kind = kind;
        let repair = repair_of(&world).unwrap();
        let class = world
            .repair_retarget((PLAYER, SLOT), repair, HOUSE)
            .unwrap();
        (class, repair_of(&world).unwrap().counted, builders(&world))
    };
    assert_eq!(
        retaken(PlayerKind::Human),
        (class::BUILDING, false, BuilderCount::of_data(0))
    );
    assert_eq!(
        retaken(PlayerKind::Computer),
        (class::BUILDING, true, BuilderCount::of_data(1))
    );
}

/// The work command's retake of the Repair on top (slot 0x828 after slot 0x48) tells the unit AI
/// the repair order and action on the target (order 0x2ce, action 0x26a) even where the Repair
/// keeps its state (a repairer facing its target, state 6).
#[test]
fn repair_retake_tells_unit_ai_repair_order_on_target() {
    let mut world = counted_repairer();
    world
        .parts
        .actions
        .of_mut(VILLAGER)
        .unwrap()
        .list
        .repair_mut()
        .unwrap()
        .state = RepairState::Face;
    let ai = world.parts.unit_ai.get_mut(VILLAGER).unwrap();
    (ai.current_order, ai.current_action, ai.current_target) = (None, None, None);
    let repair = repair_of(&world).unwrap();
    world
        .retake_repair(
            (PLAYER, SLOT),
            repair,
            (HOUSE, Position::ORIGIN, Notifies::Kept),
        )
        .unwrap();
    assert_eq!(repair_of(&world), Some(repair));
    assert_eq!(
        told(&world),
        (Some(OrderKind::Repair), Some(AiAction::Repair), Some(HOUSE))
    );
}

/// The unit AI's repair command (slot 0x378) with the Repair on top retakes it on the target
/// ([`RepairOrder::Retake`], `FUN_140adb480`) without telling the unit AI, which set its order
/// itself.
#[test]
fn repair_command_retakes_top_repair_without_notify() {
    let mut world = counted_repairer();
    world
        .parts
        .unit_ai
        .get_mut(VILLAGER)
        .unwrap()
        .current_target = None;
    let repair = repair_of(&world).unwrap();
    world
        .repair_order(
            PLAYER,
            SLOT,
            RepairOrder::Retake {
                target: HOUSE,
                repair,
            },
        )
        .unwrap();
    assert_eq!(
        (
            repair_of(&world).map(|repair_action| repair_action.counted),
            builders(&world)
        ),
        (Some(false), BuilderCount::of_data(0))
    );
    assert_eq!(
        world.parts.unit_ai.get(VILLAGER).unwrap().current_target,
        None
    );
}

/// The unit AI's repair command without a Repair on top switches the villager to the masters it
/// names (slot 0x90 with 0), then a Repair on the task found replaces its actions; without a task
/// it only switches. A villager in state 3 takes its new Repair, but the install (slot 0x808,
/// `FUN_140a977e0`) deletes it, the house let go again.
#[test]
fn repair_command_switches_master_then_starts_repair_on_task_found() {
    let started = |found: bool| {
        let mut world = repairer_world(vec![], true);
        let task = task_of(&world, ids::VILLAGER_REPAIRER_MALE, REPAIR);
        world
            .repair_order(
                PLAYER,
                SLOT,
                RepairOrder::Start {
                    target: HOUSE,
                    switches: [Some(ids::VILLAGER_REPAIRER_MALE), None],
                    task: found.then_some(task),
                },
            )
            .unwrap();
        (
            world.object(VILLAGER).unwrap().1.life.master_id,
            repair_of(&world).map(|repair_action| repair_action.target),
        )
    };
    assert_eq!(
        started(true),
        (
            ids::VILLAGER_REPAIRER_MALE,
            Some(Some(RepairTargetId::new(HOUSE)))
        )
    );
    assert_eq!(started(false), (ids::VILLAGER_REPAIRER_MALE, None));
    let mut world = repairer_world(vec![], true);
    world.object_mut(VILLAGER).unwrap().1.life.state = ObjectState::Dying;
    let task = task_of(&world, ids::VILLAGER_REPAIRER_MALE, REPAIR);
    world
        .repair_order(
            PLAYER,
            SLOT,
            RepairOrder::Start {
                target: HOUSE,
                switches: [Some(ids::VILLAGER_REPAIRER_MALE), None],
                task: Some(task),
            },
        )
        .unwrap();
    assert_eq!(
        (repair_of(&world), workers(&world, HOUSE)),
        (None, WorkerCount::ZERO)
    );
}

/// `FUN_140ab1b30` on the Build's own foundation while a computer player's builder walks there
/// (state 4) keeps the Build as it is, and answers the foundation's class.
#[test]
fn build_retake_keeps_computer_builder_walking_to_its_foundation() {
    let mut world = house_builder();
    world.players[1].kind = PlayerKind::Computer;
    let build = world
        .parts
        .actions_of(VILLAGER)
        .list
        .build()
        .copied()
        .unwrap();
    assert_eq!(
        world.build_take((PLAYER, SLOT), build, HOUSE),
        Ok(class::BUILDING)
    );
    assert_eq!(
        world.parts.actions_of(VILLAGER).list.build().copied(),
        Some(build)
    );
}

/// The work command's retake of the Build on top (`LAB_1410706a8`: slot 0x48, then slot 0x828)
/// tells the unit AI the build order and action on the foundation (order 702, action 602).
#[test]
fn build_retake_tells_unit_ai_build_order_on_foundation() {
    let mut world = house_builder();
    let ai = world.parts.unit_ai.get_mut(VILLAGER).unwrap();
    (ai.current_order, ai.current_action, ai.current_target) = (None, None, None);
    let build = world
        .parts
        .actions_of(VILLAGER)
        .list
        .build()
        .copied()
        .unwrap();
    world
        .retake_build(
            (PLAYER, SLOT),
            build,
            (HOUSE, Position::ORIGIN, Notifies::Kept),
        )
        .unwrap();
    assert_eq!(
        told(&world),
        (Some(OrderKind::Build), Some(AiAction::Build), Some(HOUSE))
    );
}

/// The Repair's constructor (`FUN_140ad7fa0`) takes its target (slot 0x178) and answers the
/// Repair with the target's class, its place the point; it installs and starts nothing: the
/// villager's actions stay, and the Repair waits for the install's start (state 3, where the
/// game's constructor leaves 0).
#[test]
fn new_repair_takes_target_and_installs_nothing() {
    let mut world = repair_world();
    let task = task_of(&world, ids::BUILDER, REPAIR);
    let (repair, class) = world.new_repair(PLAYER, SLOT, HOUSE, &task).unwrap();
    assert_eq!(
        (repair.target, repair.state, repair.point, class),
        (
            Some(RepairTargetId::new(HOUSE)),
            RepairState::Idle,
            Position::new(30.0, 88.0, 0.0),
            class::BUILDING
        )
    );
    assert_eq!(repair_of(&world), None);
    assert_eq!(workers(&world, HOUSE), WorkerCount::of_data(1));
}

/// The bush 600 of [`forage_world`].
const BUSH: ObjectId = object_id(600);
/// The bush 601 of [`forage_world`].
const OTHER_BUSH: ObjectId = object_id(601);
/// The deer 500 and 501 of [`hunt_world`].
const DEER: [ObjectId; 2] = [object_id(500), object_id(501)];
/// The town centre 700 of [`forage_world`] and [`hunt_world`], player 1's.
const TOWN_CENTRE: ObjectId = object_id(700);

/// The villager's Gather.
fn gather_of(world: &World) -> Option<GatherAction> {
    world.parts.actions_of(VILLAGER).list.gather().copied()
}

/// The villager's Hunt.
fn hunt_of(world: &World) -> Option<HuntAction> {
    world.parts.actions_of(VILLAGER).list.hunt().copied()
}

/// The unit's gather command (slot 0x368, `FUN_141085030`) and its hunt command (slot 0x360,
/// `FUN_1412151f0`) answer 0 for an object the world does not hold (`FUN_1415417a0`) and for
/// one slot 0x820 finds no task on (the town centre 700); the hunt's for none too. The
/// villager's actions stay as they were.
#[test]
fn gathering_commands_refuse_object_not_held_or_without_task() {
    for target in [object_id(99), object_id(700)] {
        let mut world = forage_world();
        let before = world.parts.actions_of(VILLAGER).clone();
        assert_eq!(
            world.gather_command(PLAYER, SLOT, target, OrderForce::Forced),
            Ok(Answer::NotTaken),
            "{target:?}"
        );
        assert_eq!(world.parts.actions_of(VILLAGER), &before);
    }
    for target in [Some(object_id(99)), Some(object_id(700)), None] {
        let mut world = hunt_world();
        let before = world.parts.actions_of(VILLAGER).clone();
        assert_eq!(
            world.hunt_command(PLAYER, SLOT, target, OrderForce::Forced),
            Ok(Answer::NotTaken),
            "{target:?}"
        );
        assert_eq!(world.parts.actions_of(VILLAGER), &before);
    }
}

/// The gather command of a forager in state 3 on the bush 600 and the hunt command of a hunter
/// in state 3 on the deer 500: the new action takes its target (slot 0x178), the install (slot
/// 0x808) deletes it unstarted (`FUN_140a977e0`), which lets the target go again; the command
/// answers 1, no action stays.
#[test]
fn gathering_commands_of_dying_unit_delete_new_action_and_let_target_go() {
    let mut world = forage_world();
    world.object_mut(VILLAGER).unwrap().1.life.state = ObjectState::Dying;
    assert_eq!(
        world.gather_command(PLAYER, SLOT, BUSH, OrderForce::Forced),
        Ok(Answer::Taken)
    );
    assert!(world.parts.actions_of(VILLAGER).list.is_empty());
    assert_eq!(workers(&world, BUSH), WorkerCount::ZERO);
    let mut world = hunt_world();
    world.object_mut(VILLAGER).unwrap().1.life.state = ObjectState::Dying;
    assert_eq!(
        world.hunt_command(PLAYER, SLOT, Some(DEER[0]), OrderForce::Forced),
        Ok(Answer::Taken)
    );
    assert!(world.parts.actions_of(VILLAGER).list.is_empty());
    assert_eq!(workers(&world, DEER[0]), WorkerCount::ZERO);
}

/// [`forage_world`]'s villager on the bush 600 (the gather command's new Gather), at work
/// (state 6) with its work done this update (`+0x80`).
fn working_gatherer() -> World {
    let mut world = forage_world();
    assert_eq!(
        world.gather_command(PLAYER, SLOT, BUSH, OrderForce::Forced),
        Ok(Answer::Taken)
    );
    let gather = world
        .parts
        .actions
        .of_mut(VILLAGER)
        .unwrap()
        .list
        .gather_mut()
        .unwrap();
    (gather.state, gather.worked) = (GatherState::Work, true);
    world
}

/// The gather command unforced on the target of the Gather on top ("I'm already gathering Unit
/// #%d.") answers 1 at once: the Gather stays as it was. Forced, the Gather on the task found
/// takes the target (its slot 0x48), which a Gather at work keeps but for its work done
/// (`+0x80`), cleared; 1 again.
#[test]
fn gather_command_on_top_gather_target_answers_at_once_only_unforced() {
    let mut world = working_gatherer();
    let gather = gather_of(&world).unwrap();
    assert_eq!(
        world.gather_command(PLAYER, SLOT, BUSH, OrderForce::Unforced),
        Ok(Answer::Taken)
    );
    assert_eq!(gather_of(&world), Some(gather));
    assert_eq!(
        world.gather_command(PLAYER, SLOT, BUSH, OrderForce::Forced),
        Ok(Answer::Taken)
    );
    assert_eq!(
        gather_of(&world),
        Some(GatherAction {
            worked: false,
            ..gather
        })
    );
}

/// The gather command unforced on an object other than the Gather's target answers 1 through
/// slot 0x820: it finds the Gather's task on the bush 601, and the Gather on top takes the bush
/// (its slot 0x48).
#[test]
fn gather_command_unforced_on_another_target_retargets_top_gather() {
    let mut world = working_gatherer();
    let gather = gather_of(&world).unwrap();
    assert_eq!(
        world.gather_command(PLAYER, SLOT, OTHER_BUSH, OrderForce::Unforced),
        Ok(Answer::Taken)
    );
    let retargeted = gather_of(&world).unwrap();
    assert_ne!(retargeted, gather);
    assert_eq!(
        (retargeted.target, retargeted.task.id),
        (Some(GatherTargetId::new(OTHER_BUSH)), gather.task.id)
    );
}

/// A recyclable object's id (a negative one, [`ObjectId::is_recyclable`]).
const RECYCLED: ObjectId = object_id(-602);

/// `world` with a copy of object `original` under the id [`RECYCLED`]: in its owner's live list,
/// on the map at its place.
fn recycled_copy(world: &mut World, original: ObjectId) {
    let (owner, unit) = world.object(original).unwrap();
    let copy = Unit {
        id: RECYCLED,
        ..unit.clone()
    };
    let record = *world.spatial.get(original).unwrap();
    let _ = world.push_live(owner, copy).unwrap();
    world.spatial.insert(RECYCLED, record);
}

/// The gather command unforced on the Gather's own target with a recyclable id does not answer
/// at once: `FUN_140a78450` finds no object for an id below 0, so the top action's target never
/// equals the command's object. The Gather on the task found takes the target again (its slot
/// 0x48), which clears its work done (`+0x80`); 1.
#[test]
fn gather_command_unforced_on_recyclable_top_target_retakes_it() {
    let mut world = working_gatherer();
    recycled_copy(&mut world, BUSH);
    let gather = world
        .parts
        .actions
        .of_mut(VILLAGER)
        .unwrap()
        .list
        .gather_mut()
        .unwrap();
    gather.target = Some(GatherTargetId::new(RECYCLED));
    let gather = *gather;
    assert_eq!(
        world.gather_command(PLAYER, SLOT, RECYCLED, OrderForce::Unforced),
        Ok(Answer::Taken)
    );
    assert_eq!(
        gather_of(&world),
        Some(GatherAction {
            worked: false,
            ..gather
        })
    );
}

/// The hunt command's new Hunt (`FUN_140ac2dd0`) holds a deer up to state 6: the deer is taken
/// (slot 0x178, a worker), its place is the point and the start walks to the dead deer
/// (`set_state` 4). A deer past state 6 `FUN_140a7a230` drops: nothing is taken, the point and
/// the target type stay −1, and the install's start (slot 5 with no target) searches at once
/// (`set_state(3)`): the unit AI hears notify 0x202 with the hunt, no class and the closest drop
/// site (the town centre 700), and the timer starts at −10 s.
#[test]
fn hunt_command_on_deer_past_state_6_searches_without_it() {
    for state in [ObjectState::Undead, ObjectState::Removed, ObjectState::Gone] {
        let mut world = hunt_world();
        world.object_mut(DEER[0]).unwrap().1.life.state = state;
        assert_eq!(
            world.hunt_command(PLAYER, SLOT, Some(DEER[0]), OrderForce::Forced),
            Ok(Answer::Taken),
            "{state:?}"
        );
        let hunt = hunt_of(&world).unwrap();
        let heard: Vec<_> = world
            .parts
            .unit_ai
            .get(VILLAGER)
            .unwrap()
            .notifies
            .iter()
            .map(|notify| (notify.kind, notify.params))
            .collect();
        if state == ObjectState::Undead {
            assert_eq!(
                (
                    hunt.target,
                    hunt.point,
                    hunt.state.bare(),
                    workers(&world, DEER[0])
                ),
                (
                    Some(HuntTargetId::new(DEER[0])),
                    Position::new(32.5, 89.5, 0.0),
                    HuntState::Walk { move_to: None },
                    WorkerCount::ONE
                )
            );
            assert_eq!(heard, Vec::new());
        } else {
            assert_eq!(
                (
                    hunt.target,
                    hunt.point,
                    hunt.target_class,
                    hunt.state,
                    hunt.timer,
                    workers(&world, DEER[0])
                ),
                (
                    None,
                    Position::new(-1.0, -1.0, -1.0),
                    None,
                    HuntState::Search,
                    Seconds::new(-10.0),
                    WorkerCount::ZERO
                ),
                "{state:?}"
            );
            assert_eq!(
                heard,
                [(
                    NotifyKind::REQUIRES_SEARCH,
                    [AiAction::Hunt.value(), -1, TOWN_CENTRE.get()]
                )],
                "{state:?}"
            );
        }
    }
}

/// [`hunt_world`]'s villager hunting the deer 500 (the hunt command's new Hunt), facing it
/// (state 6).
fn deer_hunter() -> World {
    let mut world = hunt_world();
    assert_eq!(
        world.hunt_command(PLAYER, SLOT, Some(DEER[0]), OrderForce::Forced),
        Ok(Answer::Taken)
    );
    world
        .parts
        .actions
        .of_mut(VILLAGER)
        .unwrap()
        .list
        .hunt_mut()
        .unwrap()
        .state = HuntState::Face;
    world
}

/// The hunt command on the target of the Hunt on top answers 1 and the Hunt stays as it was:
/// unforced at once, forced through the Hunt's slot 0x48 (slot 9, [`World::retarget_hunt`]),
/// for which its own target changes nothing, where a new Hunt would start over.
#[test]
fn hunt_command_on_top_hunt_target_keeps_hunt() {
    for force in [OrderForce::Unforced, OrderForce::Forced] {
        let mut world = deer_hunter();
        let hunt = hunt_of(&world).unwrap();
        assert_eq!(
            world.hunt_command(PLAYER, SLOT, Some(DEER[0]), force),
            Ok(Answer::Taken),
            "{force:?}"
        );
        assert_eq!(hunt_of(&world), Some(hunt), "{force:?}");
    }
}

/// The hunt command on another target, forced or not, with a Hunt on top: the Hunt's slot 0x48
/// (slot 9, [`World::retarget_hunt`]) takes the deer 501 and chases it (its Attack, state 12),
/// keeping its task (`+0x48`, here a work range of 0.75), which a new Hunt would take anew from
/// the master (0.5); 1.
#[test]
fn hunt_command_with_top_hunt_retargets_it() {
    for force in [OrderForce::Unforced, OrderForce::Forced] {
        let mut world = deer_hunter();
        world
            .parts
            .actions
            .of_mut(VILLAGER)
            .unwrap()
            .list
            .hunt_mut()
            .unwrap()
            .task
            .range = Distance::new(0.75);
        assert_eq!(
            world.hunt_command(PLAYER, SLOT, Some(DEER[1]), force),
            Ok(Answer::Taken),
            "{force:?}"
        );
        let hunt = hunt_of(&world).unwrap();
        assert_eq!(
            (hunt.target, hunt.state.bare(), hunt.task.range),
            (
                Some(HuntTargetId::new(DEER[1])),
                HuntState::Attack { attack: None },
                Distance::new(0.75)
            ),
            "{force:?}"
        );
    }
}

/// The hunt command unforced on the Hunt's own target with a recyclable id does not answer at
/// once (`FUN_140a78450` finds no object for an id below 0): the Hunt takes the target again (its
/// slot 0x48, slot 9). A loaded hunter's target of its own player's drop site (a town centre)
/// takes the Hunt there to unload (`+0x34`) without a target, where the answer at once would
/// leave the Hunt as it was.
#[test]
fn hunt_command_unforced_on_recyclable_top_target_retargets_hunt() {
    let mut world = deer_hunter();
    recycled_copy(&mut world, TOWN_CENTRE);
    world.object_mut(VILLAGER).unwrap().1.life.amount = Amount::new(5.0);
    world
        .parts
        .actions
        .of_mut(VILLAGER)
        .unwrap()
        .list
        .hunt_mut()
        .unwrap()
        .target = Some(HuntTargetId::new(RECYCLED));
    assert_eq!(
        world.hunt_command(PLAYER, SLOT, Some(RECYCLED), OrderForce::Unforced),
        Ok(Answer::Taken)
    );
    let hunt = hunt_of(&world).unwrap();
    assert_eq!(
        (hunt.target, hunt.carry_to),
        (None, Some(DropSiteId::new(RECYCLED)))
    );
}

/// [`deer_hunter`] whose Hunt also carries to the town centre 700 (`+0x28`), which it took.
fn deer_hunter_with_drop_site() -> World {
    let mut world = deer_hunter();
    let by = world.releaser(PLAYER, SLOT).unwrap();
    world.take(TOWN_CENTRE, by).unwrap();
    world
        .parts
        .actions
        .of_mut(VILLAGER)
        .unwrap()
        .list
        .hunt_mut()
        .unwrap()
        .carry_to = Some(DropSiteId::new(TOWN_CENTRE));
    world
}

/// The workers of the deer and of the town centre.
fn hunt_workers(world: &World) -> [WorkerCount; 3] {
    [DEER[0], DEER[1], TOWN_CENTRE].map(|id| workers(world, id))
}

/// The Hunt's slot 9 on another deer lets its old target go and takes the new one (slot 0x68,
/// `FUN_140a7a230`), and lets its second target go (`FUN_140a7a190` with none).
#[test]
fn hunt_retarget_lets_old_targets_go_and_takes_new_one() {
    let mut world = deer_hunter_with_drop_site();
    let [one, none] = [WorkerCount::of_data(1), WorkerCount::ZERO];
    assert_eq!(hunt_workers(&world), [one, none, one]);
    assert_eq!(
        world.hunt_command(PLAYER, SLOT, Some(DEER[1]), OrderForce::Forced),
        Ok(Answer::Taken)
    );
    let hunt = hunt_of(&world).unwrap();
    assert_eq!(
        (hunt.target, hunt.carry_to, hunt_workers(&world)),
        (Some(HuntTargetId::new(DEER[1])), None, [none, one, none])
    );
}

/// A loaded hunter sent to its town centre takes it as its second target (`FUN_140a7a190`),
/// whatever the town centre's state, and lets its deer go (slot 0x68 with none). Sent there again
/// it takes it no second time: the second target is the town centre already.
#[test]
fn loaded_hunter_sent_to_its_town_centre_takes_it_once_whatever_its_state() {
    for state in [ObjectState::Alive, ObjectState::Removed] {
        let mut world = deer_hunter();
        world.object_mut(VILLAGER).unwrap().1.life.amount = Amount::new(5.0);
        world.object_mut(TOWN_CENTRE).unwrap().1.life.state = state;
        let [one, none] = [WorkerCount::of_data(1), WorkerCount::ZERO];
        for _ in 0..2 {
            assert_eq!(
                world.hunt_command(PLAYER, SLOT, Some(TOWN_CENTRE), OrderForce::Forced),
                Ok(Answer::Taken),
                "{state:?}"
            );
            let hunt = hunt_of(&world).unwrap();
            assert_eq!(
                (hunt.target, hunt.carry_to, hunt.state.bare()),
                (
                    None,
                    Some(DropSiteId::new(TOWN_CENTRE)),
                    HuntState::Drop { move_to: None }
                ),
                "{state:?}"
            );
            assert_eq!(hunt_workers(&world), [none, none, one], "{state:?}");
        }
    }
}

/// The Hunt's slot 9 on a deer past state 6 holds no target (`FUN_140a7a230`) and takes none;
/// `set_state(4)` fails the Hunt (`set_state(13)`), and the hunter takes the sprite the hunt
/// stands with (`FUN_140ac5210`: its master's standing sprite, as it carries nothing). Its old
/// deer and the town centre are let go.
#[test]
fn hunt_retarget_on_deer_past_state_6_fails_hunt_with_standing_sprite() {
    use crate::objects::FacetCount;
    use crate::objects::Frame;
    use crate::objects::SpeedMultiplier;
    use crate::objects::SpriteFlags;
    use crate::objects::SpriteId;
    use crate::objects::masters::{Master, Masters};
    use crate::objects::sprite::Graphic;
    use std::sync::Arc;
    let standing = Graphic {
        id: SpriteId::named(410),
        frames: Frame::of_data(10),
        facets: FacetCount::of_data(8),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::ANIMATED,
    };
    let mut world = deer_hunter_with_drop_site();
    let hunter = world.object(VILLAGER).unwrap().1.life.master_id;
    for masters in &mut world.model.masters {
        *masters = Arc::new(Masters::new(masters.iter().cloned().map(|master| {
            if master.id == hunter {
                Master {
                    standing_sprite: Some(standing.id),
                    ..master
                }
            } else {
                master
            }
        })));
    }
    let _ = Arc::make_mut(&mut world.model.graphics).insert(standing.id, standing);
    world.object_mut(DEER[1]).unwrap().1.life.state = ObjectState::Removed;
    assert_eq!(
        world.hunt_command(PLAYER, SLOT, Some(DEER[1]), OrderForce::Forced),
        Ok(Answer::Taken)
    );
    let hunt = hunt_of(&world).unwrap();
    assert_eq!(
        (hunt.target, hunt.carry_to, hunt.state, hunt_workers(&world)),
        (None, None, HuntState::Failed, [WorkerCount::ZERO; 3])
    );
    let sprite = world.object(VILLAGER).unwrap().1.sprite;
    assert_eq!(
        sprite.map(|active_sprite| active_sprite.graphic.id),
        Some(standing.id)
    );
}

/// A building target that the load could not read whole stops the Hunt's slot 9 with the
/// import's error: its action list may hold the Unpack the slot tests. The Hunt stays as it was.
/// The slot reads no other object's action list: an unread deer is retargeted.
#[test]
fn hunt_retarget_on_unread_building_stops_with_import_error() {
    let why = GenieError::unsupported("a pack");
    let mut world = deer_hunter();
    world.set_unread(TOWN_CENTRE, why.clone());
    let hunt = hunt_of(&world);
    assert_eq!(
        world.hunt_command(PLAYER, SLOT, Some(TOWN_CENTRE), OrderForce::Forced),
        Err(why.clone())
    );
    assert_eq!(hunt_of(&world), hunt);
    world.set_unread(DEER[1], why);
    assert_eq!(
        world.hunt_command(PLAYER, SLOT, Some(DEER[1]), OrderForce::Forced),
        Ok(Answer::Taken)
    );
    assert_eq!(
        hunt_of(&world).and_then(|hunt_action| hunt_action.target),
        Some(HuntTargetId::new(DEER[1]))
    );
}
