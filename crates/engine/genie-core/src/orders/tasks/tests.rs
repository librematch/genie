//! The work command's Build and Repair, their retakes and targets, and the unit AI's repair,
//! gather and hunt commands.

use super::*;
use crate::construction::BuilderCount;
use crate::construction::build::BuildState;
use crate::construction::repair::REPAIR;
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
