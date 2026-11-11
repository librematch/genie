//! The unit AI's order slots, each called by its own name.

use super::*;
use crate::construction::RepairTargetId;
use crate::garrison::transport::TransportState;
use crate::gathering::gather::GATHER;
use crate::gathering::hunt::HUNT;
use crate::movement::move_to::MOVE_TO;
use crate::objects::masters::Master;
use crate::test_ids::class_of;
use crate::test_ids::object_id;
use crate::test_ids::player_id;
use crate::testing::ai::run_ai;
use crate::testing::scene::SceneObject;
use crate::testing::tuning;
use crate::unit_ai::Notify;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::OrderKind;
use crate::unit_ai::OrderPriority;
use crate::unit_ai::tests::{
    ai_9, attacks_9, idle, order_garrisoner, order_gatherer, order_house, order_repairer,
    order_shelter, order_ship, order_soldier, order_target, order_tree, order_unit, order_walk,
    order_world, queued, top_9, walk_9,
};
use crate::world::World;
use std::slice::from_ref;

/// Runs `slot` on unit 9's unit AI inside its update in `world` ([`run_ai`]): the AI, what it
/// reads of its unit and the world, its side and the AI stream.
fn slot_9<R>(
    world: &mut World,
    slot: impl FnOnce(&mut UnitAi, &Surroundings, &mut UnitView<'_, '_>, &mut Well512) -> GenieResult<R>,
) -> GenieResult<R> {
    run_ai(
        world,
        (player_id(1), object_id(9)),
        |ai, at, side, _, rng| slot(ai, at, side, rng),
    )
}

/// A unit AI of `ai_kind` without an action value, its last action the heal's, its target
/// object 5 of class 9 at (1, 2).
fn resting(ai_kind: UnitAiKind) -> UnitAi {
    UnitAi {
        last_action: Some(AiAction::Heal),
        current_target: Some(object_id(5)),
        target_class: class_of(9),
        target_point: Position::new(1.0, 2.0, 0.0),
        ..idle(ai_kind)
    }
}

/// [`resting`] with the move's action value.
fn moving(ai_kind: UnitAiKind) -> UnitAi {
    UnitAi {
        current_action: Some(AiAction::Move),
        ..resting(ai_kind)
    }
}

/// The two ways a slot takes an order from a unit AI of `ai_kind`: forced with an action value ([`moving`], whose move becomes the last action) and unforced without one
/// ([`resting`], whose none becomes the last).
fn takers(ai_kind: UnitAiKind) -> [(OrderForce, UnitAi, Option<AiAction>); 2] {
    [
        (OrderForce::Forced, moving(ai_kind), Some(AiAction::Move)),
        (OrderForce::Unforced, resting(ai_kind), None),
    ]
}

/// A unit AI's action value and its last.
const fn actions(unit_ai: &UnitAi) -> (Option<AiAction>, Option<AiAction>) {
    (unit_ai.current_action, unit_ai.last_action)
}

/// What a unit AI holds of its target: the target, its class and point, and the last target
/// and class.
#[derive(Debug, PartialEq)]
struct Targets {
    target: Option<ObjectId>,
    class: Option<ClassId>,
    point: Position,
    last: Option<ObjectId>,
    last_class: Option<ClassId>,
}

impl Targets {
    /// Those `unit_ai` holds.
    const fn of(unit_ai: &UnitAi) -> Self {
        Self {
            target: unit_ai.current_target,
            class: unit_ai.target_class,
            point: unit_ai.target_point,
            last: unit_ai.last_target,
            last_class: unit_ai.last_target_class,
        }
    }

    /// Those of a slot that took object `target` of class `class` at `point` from [`resting`]:
    /// its target 5 and class 9 become the last ones.
    fn taken(target: i32, class: i16, point: Position) -> Self {
        Self {
            target: Some(object_id(target)),
            class: class_of(class),
            point,
            last: Some(object_id(5)),
            last_class: class_of(9),
        }
    }

    /// Those of a slot that took no target at `point` from [`resting`] (`FUN_140c64c10` with
    /// −1): its class 9 stays, and its target 5 and class 9 become the last ones.
    fn pointed(point: Position) -> Self {
        Self {
            target: None,
            class: class_of(9),
            point,
            last: Some(object_id(5)),
            last_class: class_of(9),
        }
    }
}

/// An explore slot: its unit AI, the tile, its force, the side, the AI stream and the tuning.
type ExploreSlot = fn(
    &mut UnitAi,
    Tile,
    OrderForce,
    &mut UnitView<'_, '_>,
    &mut Well512,
    &UnitAiTuning,
) -> GenieResult<Answer>;

/// The point the Explore of unit 9 in `world` goes to, none without one.
fn explores_9(world: &World) -> Option<Position> {
    world
        .parts
        .actions
        .of(object_id(9))
        .list
        .explore()
        .map(|explore_action| explore_action.point)
}

/// Unit 9 of master `master` and unit AI `unit_ai` at (5, 6) at height 1.5.
fn raised_unit(master: Master, unit_ai: UnitAi) -> SceneObject {
    SceneObject {
        at: Position::new(5.0, 6.0, 1.5),
        ..order_unit(master, unit_ai)
    }
}

/// Slot 0x170 (`FUN_140c4ccc0`) on player 1's swordsman 70, forced with an action value or
/// unforced without one: the defend's action value (`0x259`), the old one the last; 70 the
/// target at its place with its class (`FUN_140c64c10`), the old target and class the last ones;
/// the range the desired distance and 70 the defend target. Unforced with an action value, or on
/// an object the world does not hold, the AI stays as it was.
#[test]
fn defend_object_defends_object_the_world_holds_forced_or_without_action_value() {
    let defend = |unit_ai: UnitAi, target: i32, force: OrderForce| {
        let mut world = order_world(
            order_unit(order_soldier(), unit_ai),
            &[order_target(70, player_id(1))],
        );
        let answer = slot_9(&mut world, |ai, at, side, _| {
            ai.defend_object(object_id(target), Distance::new(3.0), force, at, side)
        });
        (answer, world)
    };
    for (force, unit_ai, last) in takers(UnitAiKind::Soldier) {
        let (answer, world) = defend(unit_ai, 70, force);
        assert_eq!(answer, Ok(Answer::Taken), "{force:?}");
        let unit_ai = ai_9(&world);
        assert_eq!(actions(unit_ai), (Some(AiAction::Defend), last));
        assert_eq!(
            Targets::of(unit_ai),
            Targets::taken(70, 6, Position::new(10.0, 20.0, 0.0))
        );
        assert_eq!(
            (unit_ai.desired_distance, unit_ai.defend_target),
            (Distance::new(3.0), Some(object_id(70)))
        );
    }
    for (target, force) in [(70, OrderForce::Unforced), (99, OrderForce::Forced)] {
        let (answer, world) = defend(moving(UnitAiKind::Soldier), target, force);
        assert_eq!(answer, Ok(Answer::NotTaken), "{target} {force:?}");
        assert_eq!(ai_9(&world), &moving(UnitAiKind::Soldier));
    }
}

/// Slot 0x178 (`FUN_140c4ce30`), forced with an action value or unforced without one: the
/// defend's action value (`0x259`), the old one the last; no target, at the point at the unit's
/// height (`FUN_140c64c10` with −1, which leaves the class), the old target and class the last
/// ones; the range the desired distance and no defend target. Unforced with an action value
/// the AI stays as it was.
#[test]
fn defend_point_defends_point_at_unit_height_without_target() {
    let at = Surroundings {
        position: Position::new(5.0, 6.0, 1.5),
        ..Surroundings::default()
    };
    let point = Point::new(12.7, 30.2);
    for (force, unit_ai, last) in takers(UnitAiKind::Soldier) {
        let mut unit_ai = UnitAi {
            defend_target: Some(object_id(70)),
            ..unit_ai
        };
        assert_eq!(
            unit_ai.defend_point(point, Distance::new(3.0), force, &at),
            Answer::Taken,
            "{force:?}"
        );
        assert_eq!(actions(&unit_ai), (Some(AiAction::Defend), last));
        assert_eq!(
            Targets::of(&unit_ai),
            Targets::pointed(Position::new(12.7, 30.2, 1.5))
        );
        assert_eq!(
            (unit_ai.desired_distance, unit_ai.defend_target),
            (Distance::new(3.0), None)
        );
    }
    let mut unit_ai = moving(UnitAiKind::Soldier);
    assert_eq!(
        unit_ai.defend_point(point, Distance::new(3.0), OrderForce::Unforced, &at),
        Answer::NotTaken
    );
    assert_eq!(unit_ai, moving(UnitAiKind::Soldier));
}

/// The work slots unforced on their own action value (heal 0xe0 `FUN_140c4e010` on `0x25b`,
/// conversion 0xd8 `FUN_140c4c9f0` on `0x25c`, repair 0xe8 `FUN_140c5e3a0` on `0x26a`) take
/// nothing before the unit's command: the AI stays as it was and the unit gets no action.
#[test]
fn work_object_unforced_on_its_own_action_value_gives_no_command() {
    let house = object_id(70);
    for (action, command) in [
        (AiAction::Heal, UnitCommand::Heal(house)),
        (AiAction::Convert, UnitCommand::Convert(house)),
        (AiAction::Repair, UnitCommand::Repair(house)),
    ] {
        let unit_ai = UnitAi {
            current_action: Some(action),
            ..resting(UnitAiKind::Civilian)
        };
        let mut world = order_world(
            order_unit(order_repairer(), unit_ai.clone()),
            &[order_house(300.0)],
        );
        let answer = slot_9(&mut world, |ai, at, side, _| {
            ai.work_object(house, OrderForce::Unforced, (action, command), (at, side))
        });
        assert_eq!(answer, Ok(Answer::NotTaken), "{action:?}");
        assert_eq!((ai_9(&world), top_9(&world)), (&unit_ai, None));
    }
}

/// The repair slot 0xe8 (`FUN_140c5e3a0`) forced on its own action value or unforced on
/// another (the gather's): the unit's repair command (slot 0x378) on player 1's damaged house
/// 70 takes it (its Repair at once), then the repair's action value (`0x26a`), the old one the
/// last, and 70 the target at its place with its class, the old target and class the last
/// ones. An undamaged house gives the unit no repair task (its 0): the AI stays as it was.
#[test]
fn work_object_takes_target_once_the_unit_command_takes_it() {
    let repair = |unit_ai: UnitAi, hp: f32, force: OrderForce| {
        let mut world = order_world(order_unit(order_repairer(), unit_ai), &[order_house(hp)]);
        let answer = slot_9(&mut world, |ai, at, side, _| {
            let house = object_id(70);
            let work = (AiAction::Repair, UnitCommand::Repair(house));
            ai.work_object(house, force, work, (at, side))
        });
        (answer, world)
    };
    for (force, action) in [
        (OrderForce::Forced, AiAction::Repair),
        (OrderForce::Unforced, AiAction::Gather),
    ] {
        let unit_ai = UnitAi {
            current_action: Some(action),
            ..resting(UnitAiKind::Civilian)
        };
        let (answer, world) = repair(unit_ai, 300.0, force);
        assert_eq!(answer, Ok(Answer::Taken), "{force:?}");
        let repairs = world
            .parts
            .actions
            .of(object_id(9))
            .list
            .repair()
            .map(|repair_action| repair_action.target);
        assert_eq!(repairs, Some(Some(RepairTargetId::new(object_id(70)))));
        let unit_ai = ai_9(&world);
        assert_eq!(actions(unit_ai), (Some(AiAction::Repair), Some(action)));
        assert_eq!(
            Targets::of(unit_ai),
            Targets::taken(70, 3, Position::new(10.0, 20.0, 0.0))
        );
    }
    let (answer, world) = repair(moving(UnitAiKind::Civilian), 550.0, OrderForce::Forced);
    assert_eq!(answer, Ok(Answer::NotTaken));
    assert_eq!(
        (ai_9(&world), top_9(&world)),
        (&moving(UnitAiKind::Civilian), None)
    );
}

/// `GatherObject` (slot 0xd0, `FUN_140c4dd50`) forced on Gaia's oak 70: the unit's gather
/// command (slot 0x368: its Gather at once), then the gather's action value (`0x261`), the old
/// one the last, and 70 the target at its place with its class, the old target and class the
/// last ones. An object the world does not hold is not taken, and nothing changes.
#[test]
fn gather_order_gathers_object_the_world_holds() {
    let gather = |target| {
        let mut world = order_world(
            order_unit(order_gatherer(), moving(UnitAiKind::Civilian)),
            &[order_tree()],
        );
        let answer = slot_9(&mut world, |ai, at, side, rng| {
            ai.gather_order(object_id(target), at, side, rng)
        });
        (answer, world)
    };
    let (answer, world) = gather(70);
    assert_eq!(answer, Ok(Answer::Taken));
    assert_eq!(top_9(&world), Some(GATHER));
    let unit_ai = ai_9(&world);
    assert_eq!(
        actions(unit_ai),
        (Some(AiAction::Gather), Some(AiAction::Move))
    );
    assert_eq!(
        Targets::of(unit_ai),
        Targets::taken(70, 15, Position::new(10.0, 20.0, 0.0))
    );
    let (answer, world) = gather(99);
    assert_eq!(answer, Ok(Answer::NotTaken));
    assert_eq!(
        (ai_9(&world), top_9(&world)),
        (&moving(UnitAiKind::Civilian), None)
    );
}

/// `HuntObject` (slot 0xc8, `FUN_140c4e220`) forced on Gaia's oak 70 by a unit on its walk: the
/// action stops first (slot 0xb0 with 1), then the unit's hunt command (slot 0x360: its Hunt at
/// once, which replaces the walk), the hunt's action value (`0x265`) with the none the stop
/// left the last, and 70 the target at its place with its class, the old target and class the
/// last ones. On an object the world does not hold, or on none, the stop stays (its walk gone,
/// the move the last action, the target kept) and the order is not taken.
#[test]
fn hunt_order_stops_then_hunts_object_the_world_holds() {
    let hunt = |target| {
        let walker = SceneObject {
            actions: Some(order_walk()),
            ..order_unit(order_gatherer(), moving(UnitAiKind::Civilian))
        };
        let mut world = order_world(walker, &[order_tree()]);
        let answer = slot_9(&mut world, |ai, at, side, rng| {
            ai.hunt_order(target, at, side, rng)
        });
        (answer, world)
    };
    let (answer, world) = hunt(Some(object_id(70)));
    assert_eq!(answer, Ok(Answer::Taken));
    assert_eq!(top_9(&world), Some(HUNT));
    let unit_ai = ai_9(&world);
    assert_eq!(actions(unit_ai), (Some(AiAction::Hunt), None));
    assert_eq!(
        Targets::of(unit_ai),
        Targets::taken(70, 15, Position::new(10.0, 20.0, 0.0))
    );
    for target in [Some(object_id(99)), None] {
        let (answer, world) = hunt(target);
        assert_eq!(answer, Ok(Answer::NotTaken), "{target:?}");
        assert_eq!(top_9(&world), None);
        let unit_ai = ai_9(&world);
        assert_eq!(actions(unit_ai), (None, Some(AiAction::Move)));
        assert_eq!(
            Targets::of(unit_ai),
            Targets::of(&moving(UnitAiKind::Civilian))
        );
    }
}

/// Slot 0x168 (`FUN_140c4dab0`) on player 1's swordsman 70 at (6, 7), forced with an action
/// value or unforced without one: the follow's action value (`0x264`), the old one the last;
/// 70 the target at its place with its class (`FUN_140c64c10`), the old target and class the
/// last ones; the range the desired distance. Within the range (the footprints' squared gap at
/// most its square) the AI holds the follow and gives no move.
#[test]
fn follow_object_within_its_range_follows_without_move() {
    let near = SceneObject {
        at: Position::new(6.0, 7.0, 0.0),
        ..order_target(70, player_id(1))
    };
    for (force, unit_ai, last) in takers(UnitAiKind::Soldier) {
        let mut world = order_world(order_unit(order_soldier(), unit_ai), from_ref(&near));
        let answer = slot_9(&mut world, |ai, at, side, rng| {
            ai.follow_object(object_id(70), Distance::new(3.0), force, at, side, rng)
        });
        assert_eq!(answer, Ok(Answer::Taken), "{force:?}");
        assert_eq!(walk_9(&world), None);
        let unit_ai = ai_9(&world);
        assert_eq!(actions(unit_ai), (Some(AiAction::Follow), last));
        assert_eq!(
            Targets::of(unit_ai),
            Targets::taken(70, 6, Position::new(6.0, 7.0, 0.0))
        );
        assert_eq!(unit_ai.desired_distance, Distance::new(3.0));
    }
}

/// Slot 0x168 (`FUN_140c4dab0`) on player 1's swordsman 70 at (10, 20), beyond the range of 3:
/// the AI moves to it (slot 0x138: its `MoveTo` at once, on 70 within the range), the follow
/// its last action. A range that is not a number gives no move (`COMISS gap, range²; JBE`:
/// the unordered compare skips it), the follow held. Unforced with an action value, or on an
/// object the world does not hold, the AI stays as it was.
#[test]
fn follow_object_beyond_its_range_moves_to_target() {
    let follow = |unit_ai: UnitAi, target: i32, range: f32, force: OrderForce| {
        let mut world = order_world(
            order_unit(order_soldier(), unit_ai),
            &[order_target(70, player_id(1))],
        );
        let answer = slot_9(&mut world, |ai, at, side, rng| {
            ai.follow_object(
                object_id(target),
                Distance::new(range),
                force,
                at,
                side,
                rng,
            )
        });
        (answer, world)
    };
    let (answer, world) = follow(resting(UnitAiKind::Soldier), 70, 3.0, OrderForce::Forced);
    assert_eq!(answer, Ok(Answer::Taken));
    let to = world
        .parts
        .actions
        .of(object_id(9))
        .list
        .move_to()
        .map(|move_to_action| (move_to_action.target, move_to_action.range));
    assert_eq!(to, Some((Some(object_id(70)), Distance::new(3.0))));
    assert_eq!(
        actions(ai_9(&world)),
        (Some(AiAction::Move), Some(AiAction::Follow))
    );
    let (answer, world) = follow(
        resting(UnitAiKind::Soldier),
        70,
        f32::NAN,
        OrderForce::Forced,
    );
    assert_eq!(answer, Ok(Answer::Taken));
    assert_eq!(walk_9(&world), None);
    let unit_ai = ai_9(&world);
    assert_eq!(actions(unit_ai), (Some(AiAction::Follow), None));
    assert!(unit_ai.desired_distance.get().is_nan());
    for (target, force) in [(70, OrderForce::Unforced), (99, OrderForce::Forced)] {
        let (answer, world) = follow(moving(UnitAiKind::Soldier), target, 3.0, force);
        assert_eq!(answer, Ok(Answer::NotTaken), "{target} {force:?}");
        assert_eq!(
            (ai_9(&world), walk_9(&world)),
            (&moving(UnitAiKind::Soldier), None)
        );
    }
}

/// Slot 0x110 on player 1's house 70, which has room for villagers, forced with an action
/// value or unforced without one, for the base's AI (`FUN_140c4d050`, a soldier's) and the
/// civilian's (`FUN_140c1c680`, whose own lookup finds the house first): the unit's enter
/// command (slot 0x3a0: its Enter at once, walking to the house), then the enter's action value
/// (`0x269`), the old one the last, and 70 the target at (−1, −1, −1) with its class
/// (`FUN_140c64c10`), the old target and class the last ones.
#[test]
fn enter_object_enters_once_the_unit_command_takes_it() {
    for ai_kind in [UnitAiKind::Soldier, UnitAiKind::Civilian] {
        for (force, unit_ai, last) in takers(ai_kind) {
            let mut world =
                order_world(order_unit(order_garrisoner(), unit_ai), &[order_shelter()]);
            let answer = slot_9(&mut world, |ai, at, side, _| {
                ai.enter_object(object_id(70), force, (at, side))
            });
            assert_eq!(answer, Ok(Answer::Taken), "{ai_kind:?} {force:?}");
            let walks =
                world
                    .parts
                    .actions
                    .of(object_id(9))
                    .list
                    .enter()
                    .and_then(|enter_action| {
                        enter_action
                            .state
                            .move_to()
                            .map(|move_to_action| move_to_action.target)
                    });
            assert_eq!(walks, Some(Some(object_id(70))));
            let unit_ai = ai_9(&world);
            assert_eq!(actions(unit_ai), (Some(AiAction::Enter), last));
            assert_eq!(
                Targets::of(unit_ai),
                Targets::taken(70, 3, Position::from([-1.0; 3]))
            );
        }
    }
}

/// Slot 0x110 that takes nothing, for the base's AI and the civilian's: unforced with an action
/// value (before the unit's command), and forced on player 1's swordsman 71, which the unit has
/// no task to enter (the enter command's 0); the AI stays as it was and the unit gets no
/// action. An object the world does not hold the civilian's own lookup refuses before the
/// command, which would refuse it too (the base's AI): no world tells the two apart.
#[test]
fn enter_object_unforced_with_action_value_or_refused_takes_nothing() {
    for ai_kind in [UnitAiKind::Soldier, UnitAiKind::Civilian] {
        for (target, force) in [
            (70, OrderForce::Unforced),
            (71, OrderForce::Forced),
            (99, OrderForce::Forced),
        ] {
            let mut world = order_world(
                order_unit(order_garrisoner(), moving(ai_kind)),
                &[order_shelter(), order_target(71, player_id(1))],
            );
            let answer = slot_9(&mut world, |ai, at, side, _| {
                ai.enter_object(object_id(target), force, (at, side))
            });
            assert_eq!(answer, Ok(Answer::NotTaken), "{ai_kind:?} {target}");
            assert_eq!((ai_9(&world), top_9(&world)), (&moving(ai_kind), None));
        }
    }
}

/// Slot 0x128 (`FUN_140c65400`) unforced with an action value takes nothing before the unit's
/// transport command (slot 0x3b8): the AI stays as it was. (The command itself is not modelled,
/// so the slot's other branches wait for it.)
#[test]
fn transport_to_unforced_with_action_value_gives_no_command() {
    let mut world = order_world(order_unit(order_ship(), moving(UnitAiKind::Transport)), &[]);
    let answer = slot_9(&mut world, |ai, _, side, _| {
        ai.transport_to(Position::new(12.7, 30.2, 0.0), OrderForce::Unforced, side)
    });
    assert_eq!(answer, Ok(Answer::NotTaken));
    assert_eq!(ai_9(&world), &moving(UnitAiKind::Transport));
}

/// Slot 0x118 (`FUN_140c660d0`) of the transport ship 9 at height 1.5, forced with an action
/// value or unforced without one: the unit's unload command at the point at the ship's height
/// (slot 0x3a8: its Transport at once, sailing there to unload), then the unload's action value
/// (`0x26d`), the old one the last, and the ship itself the target at its place with its class,
/// the old target and class the last ones.
#[test]
fn unload_at_sails_to_point_at_unit_height_and_targets_the_unit() {
    for (force, unit_ai, last) in takers(UnitAiKind::Transport) {
        let mut world = order_world(raised_unit(order_ship(), unit_ai), &[]);
        let answer = slot_9(&mut world, |ai, at, side, _| {
            ai.unload_at(Point::new(12.7, 30.2), (force, false), (at, side))
        });
        assert_eq!(answer, Ok(Answer::Taken), "{force:?}");
        let sails = world
            .parts
            .actions
            .of(object_id(9))
            .list
            .transport()
            .map(|transport_action| (transport_action.state.bare(), transport_action.point));
        assert_eq!(
            sails,
            Some((
                TransportState::SailToUnload { move_to: None },
                Position::new(12.7, 30.2, 1.5)
            ))
        );
        let unit_ai = ai_9(&world);
        assert_eq!(actions(unit_ai), (Some(AiAction::Unload), last));
        assert_eq!(
            Targets::of(unit_ai),
            Targets::taken(9, 20, Position::new(5.0, 6.0, 1.5))
        );
    }
}

/// Slots 0x118 (`FUN_140c660d0`) and 0x120 (`FUN_140c66330`, over a wall) unforced with an
/// action value take nothing before the unit's unload command: the AI stays as it was and the
/// ship gets no action. (The unload over a wall is not modelled, so its other branches wait
/// for it.)
#[test]
fn unload_at_unforced_with_action_value_gives_no_command() {
    for over_wall in [false, true] {
        let mut world = order_world(order_unit(order_ship(), moving(UnitAiKind::Transport)), &[]);
        let answer = slot_9(&mut world, |ai, at, side, _| {
            ai.unload_at(
                Point::new(12.7, 30.2),
                (OrderForce::Unforced, over_wall),
                (at, side),
            )
        });
        assert_eq!(answer, Ok(Answer::NotTaken), "{over_wall}");
        assert_eq!(
            (ai_9(&world), top_9(&world)),
            (&moving(UnitAiKind::Transport), None)
        );
    }
}

/// Slot 0x180 (`FUN_140c64ab0`) on player 2's swordsman 70 from tile (12, 30), forced with an
/// action value or unforced without one: the unit's attack command (slot 0x338: its Attack at
/// once), then seek and destroy's action value (`0x26e`), the old one the last, and 70 the
/// target at the tile (as floats) at height 1 with its class (`FUN_140c64c10`), the old target
/// and class the last ones. Unforced with an action value, or on an object the world does
/// not hold (the attack command's 0), the AI stays as it was.
#[test]
fn seek_and_destroy_attacks_and_holds_target_at_its_tile() {
    let seek = |unit_ai: UnitAi, target: i32, force: OrderForce| {
        let mut world = order_world(
            order_unit(order_soldier(), unit_ai),
            &[order_target(70, player_id(2))],
        );
        let answer = slot_9(&mut world, |ai, at, side, rng| {
            ai.seek_and_destroy(object_id(target), Tile::new(12, 30), force, at, side, rng)
        });
        (answer, world)
    };
    for (force, unit_ai, last) in takers(UnitAiKind::Soldier) {
        let (answer, world) = seek(unit_ai, 70, force);
        assert_eq!(answer, Ok(Answer::Taken), "{force:?}");
        assert_eq!(attacks_9(&world), Some(object_id(70)));
        let unit_ai = ai_9(&world);
        assert_eq!(actions(unit_ai), (Some(AiAction::SeekAndDestroy), last));
        let tile_point = Position::new(12.0, 30.0, tuning().unit_ai.explore_z);
        assert_eq!(Targets::of(unit_ai), Targets::taken(70, 6, tile_point));
    }
    for (target, force) in [(70, OrderForce::Unforced), (99, OrderForce::Forced)] {
        let (answer, world) = seek(moving(UnitAiKind::Soldier), target, force);
        assert_eq!(answer, Ok(Answer::NotTaken), "{target} {force:?}");
        assert_eq!(
            (ai_9(&world), attacks_9(&world)),
            (&moving(UnitAiKind::Soldier), None)
        );
    }
}

/// The explore slots to tile (12, 30) of a unit at height 1.5 without an Explore on top, forced
/// with an action value or unforced without one: slot 0x188 (`FUN_140c4d9b0`, explore and
/// destroy, `0x26f`) and slot 0x100 (`FUN_140c4d910`, the explore, `0x25d`). The unit's explore
/// (slot 0x398: its Explore at once to the tile's corner at the unit's height), then the slot's
/// action value, the old one the last, and no target at the tile at height 1 (`FUN_140c64c10`
/// with −1, which leaves the class), the old target and class the last ones. Unforced with an
/// action value the AI stays as it was and the unit gets no Explore.
#[test]
fn explore_slots_explore_tile_corner_without_target() {
    let explore = |unit_ai: UnitAi, slot: ExploreSlot, force: OrderForce| {
        let mut world = order_world(raised_unit(order_soldier(), unit_ai), &[]);
        let answer = slot_9(&mut world, |ai, at, side, rng| {
            slot(ai, Tile::new(12, 30), force, side, rng, &at.unit_ai_tuning)
        });
        (answer, world)
    };
    let slots: [(AiAction, ExploreSlot); 2] = [
        (AiAction::ExploreAndDestroy, UnitAi::explore_and_destroy),
        (AiAction::Explore, UnitAi::explore_at),
    ];
    for (action, slot) in slots {
        for (force, unit_ai, last) in takers(UnitAiKind::Soldier) {
            let (answer, world) = explore(unit_ai, slot, force);
            assert_eq!(answer, Ok(Answer::Taken), "{action:?} {force:?}");
            assert_eq!(explores_9(&world), Some(Position::new(12.0, 30.0, 1.5)));
            let unit_ai = ai_9(&world);
            assert_eq!(actions(unit_ai), (Some(action), last));
            let tile_point = Position::new(12.0, 30.0, tuning().unit_ai.explore_z);
            assert_eq!(Targets::of(unit_ai), Targets::pointed(tile_point));
        }
        let (answer, world) = explore(moving(UnitAiKind::Soldier), slot, OrderForce::Unforced);
        assert_eq!(answer, Ok(Answer::NotTaken), "{action:?}");
        assert_eq!(
            (ai_9(&world), explores_9(&world)),
            (&moving(UnitAiKind::Soldier), None)
        );
    }
}

/// Slot 0x28 (`FUN_140c55c90`) with Immediate set and inFront clear, from outside the unit AI's
/// update, for unit 9 on its walk with a damage notify and target 5 of class 9: the order goes
/// in front of the queue, the target goes (`FUN_140c5e1e0`: its class with it), the unit stops
/// (slot 0xb0: its walk gone), its notifies end (`FUN_140c5dc10` with 0), and the order runs
/// at once (slot 0x230: the move's `MoveTo` to its point, its `FUN_140c64c10` making the none
/// the last target), which leaves the queue. An order not ready stays in front after the stop.
/// A dying unit (state 3) takes nothing.
#[test]
fn take_order_now_with_stops_and_runs_order_at_once() {
    let walking = || UnitAi {
        current_order: Some(OrderKind::Move),
        notifies: vec![Notify::own(object_id(9), NotifyKind::DAMAGED, [1, 5, 0])],
        ..moving(UnitAiKind::Soldier)
    };
    let take = |order: QueuedOrder, state: ObjectState| {
        let walker = SceneObject {
            actions: Some(order_walk()),
            state,
            ..order_unit(order_soldier(), walking())
        };
        let mut world = order_world(walker, &[]);
        let answer = run_ai(
            &mut world,
            (player_id(1), object_id(9)),
            |ai, at, side, player_kind, rng| {
                ai.take_order_now_with(order, (at, side, player_kind), rng)
            },
        );
        (answer, world)
    };
    let move_order = queued(OrderKind::Move, -1);
    let (answer, world) = take(move_order, ObjectState::Alive);
    assert_eq!(answer, Ok(()));
    assert_eq!(
        walk_9(&world),
        Some((Position::new(12.7, 30.2, 0.5), Distance::new(3.0)))
    );
    let unit_ai = ai_9(&world);
    assert_eq!(unit_ai.orders, Vec::new());
    assert_eq!(unit_ai.notifies, Vec::new());
    assert_eq!(
        (unit_ai.current_order, unit_ai.order_priority),
        (Some(OrderKind::Move), OrderPriority::of_data(40))
    );
    // the target went before the move's `FUN_140c64c10` made the none the last
    assert_eq!(
        (
            unit_ai.current_target,
            unit_ai.target_class,
            unit_ai.last_target,
            unit_ai.last_target_class
        ),
        (None, None, None, None)
    );
    let waiting = QueuedOrder {
        ready: false,
        ..move_order
    };
    let (answer, world) = take(waiting, ObjectState::Alive);
    assert_eq!(answer, Ok(()));
    assert_eq!((walk_9(&world), top_9(&world)), (None, None));
    assert_eq!(ai_9(&world).orders, vec![waiting]);
    let (answer, world) = take(move_order, ObjectState::Dying);
    assert_eq!(answer, Ok(()));
    assert_eq!(top_9(&world), Some(MOVE_TO));
    assert_eq!(ai_9(&world), &walking());
}

/// Slot 0x28 (`FUN_140c55c90`) with Immediate set for an attack roundup (0x2d9) on player 2's
/// swordsman 51 by unit 9 attacking 50: "We have an `AttackRoundup`, so I'm not stopping the
/// object": the target stays until the roundup (slot 0xc0) attacks 51 (its Attack at once),
/// so 50 becomes the last target; the tail makes the order 700.
#[test]
fn take_order_now_with_attack_roundup_keeps_target_until_it_attacks() {
    let attacking = UnitAi {
        current_order: Some(OrderKind::Attack),
        current_action: Some(AiAction::Attack),
        current_target: Some(object_id(50)),
        ..idle(UnitAiKind::Soldier)
    };
    let roundup = queued(OrderKind::AttackRoundup, 51);
    let mut world = order_world(
        order_unit(order_soldier(), attacking),
        &[order_target(51, player_id(2))],
    );
    let answer = run_ai(
        &mut world,
        (player_id(1), object_id(9)),
        |ai, at, side, player_kind, rng| {
            ai.take_order_now_with(roundup, (at, side, player_kind), rng)
        },
    );
    assert_eq!(answer, Ok(()));
    assert_eq!(attacks_9(&world), Some(object_id(51)));
    let unit_ai = ai_9(&world);
    assert_eq!(
        (unit_ai.current_target, unit_ai.last_target),
        (Some(object_id(51)), Some(object_id(50)))
    );
    assert_eq!(unit_ai.current_order, Some(OrderKind::Attack));
    assert_eq!(unit_ai.orders, Vec::new());
}
