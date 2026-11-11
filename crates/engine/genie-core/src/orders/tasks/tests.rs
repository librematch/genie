//! The work command's Build and Repair, their retakes and targets, and the unit AI's repair
//! command.

use super::*;
use crate::construction::BuilderCount;
use crate::construction::build::BuildState;
use crate::construction::repair::REPAIR;
use crate::objects::WorkerCount;
use crate::objects::masters::ActionKind;
use crate::objects::masters::MasterId;
use crate::players::PlayerKind;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::testing::class;
use crate::testing::ids;
use crate::world::tests::{house_builder, repair_world, repairer_world, sent_off_world};

/// The house 10028 of [`repair_world`] and [`sent_off_world`], and its point.
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
/// it only switches.
#[test]
fn repair_command_switches_master_then_starts_repair_on_task_found() {
    let started = |found: bool| {
        let mut world = repairer_world(vec![], true);
        let task = task_of(&world, master_id(156), REPAIR);
        world
            .repair_order(
                PLAYER,
                SLOT,
                RepairOrder::Start {
                    target: HOUSE,
                    switches: [Some(master_id(156)), None],
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
        (master_id(156), Some(Some(RepairTargetId::new(HOUSE))))
    );
    assert_eq!(started(false), (master_id(156), None));
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
/// Repair with the target's class; it installs nothing: the villager's actions stay.
#[test]
fn new_repair_takes_target_and_installs_nothing() {
    let mut world = repair_world();
    let task = task_of(&world, ids::BUILDER, REPAIR);
    let (repair, class) = world.new_repair(PLAYER, SLOT, HOUSE, &task).unwrap();
    assert_eq!(
        (repair.target, repair.state.bare(), class),
        (
            Some(RepairTargetId::new(HOUSE)),
            RepairState::Move { move_to: None },
            class::BUILDING
        )
    );
    assert_eq!(repair_of(&world), None);
    assert_eq!(workers(&world, HOUSE), WorkerCount::of_data(1));
}
