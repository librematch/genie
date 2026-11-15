//! The install of a unit's own `MoveTo` through slot 0x808: a unit in state 3 or later keeps
//! none (its teardown stops the unit and lets the target go), and the start's gap and top speed.

use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Position;
use crate::map::spatial::Placed;
use crate::movement::Speed;
use crate::movement::move_to::MoveTo;
use crate::objects::ListIndex;
use crate::objects::ObjectState;
use crate::objects::actions::ActionList;
use crate::objects::layers::{Animated, Motion};
use crate::objects::masters::BuildableOn;
use crate::objects::masters::ObstructionKind;
use crate::players::PlayerId;
use crate::test_ids::object_id;
use crate::testing::class;
use crate::testing::ids;
use crate::testing::static_roles;
use crate::unit_ai::AiMove;
use crate::unit_ai::Answer;
use crate::unit_ai::OrderForce;
use crate::unit_ai::Pace;
use crate::world::World;
use crate::world::tests::{town_workers_and_holding, villager_speeds, walking_villager_holding};
use std::sync::Arc;

/// The point the villager is moved to.
const POINT: Position = Position::new(15.0, 15.0, 0.0);

/// The walking villager 40 of [`walking_villager_holding`] with an empty list, in `state`, and
/// town centre 9150 on the map (radii 2, at 20, 20).
fn walker_in(state: ObjectState) -> World {
    let mut world = walking_villager_holding(ActionList::default());
    world.players[1].objects[ListIndex::new(2)].life.state = state;
    world.spatial.insert(
        object_id(9150),
        Placed {
            position: Position::new(20.0, 20.0, 0.0),
            radius: Footprint::new(2.0, 2.0),
            class: class::BUILDING,
            player: PlayerId::new(1),
            master: ids::TOWN_CENTRE,
            obstruction_kind: ObstructionKind::NONE,
            buildable_on: BuildableOn::NO,
            tall: true,
            moves: false,
            unknown: None,
            obstruction: None,
        },
    );
    world
}

/// What the villager kept after a move: whether its list holds an action, its animation speed
/// (0 once a teardown stopped it) and the workers of town centres 9149 and 9150.
fn kept(world: &World) -> (bool, Speed, (i32, i32)) {
    let (first, second, _, _) = town_workers_and_holding(world);
    (
        !world.parts.actions_of(object_id(40)).list.is_empty(),
        villager_speeds(world).1,
        (first, second),
    )
}

/// The unit AI's move of the villager to [`POINT`], forced, walking.
const AI_MOVE: AiMove = AiMove {
    point: POINT,
    range: Distance::ZERO,
    force: OrderForce::Forced,
    pace: Pace::Walk,
};

/// `FUN_14106d340` to a point: below state 3 the install hands the new `MoveTo` back to start
/// (state 3) and hold; in state 3 the list's add deletes it unstarted, and its teardown
/// (`FUN_140aa1f30`) stops the unit.
#[test]
fn point_move_in_state_3_deletes_the_new_move_to_and_stops_the_unit() {
    let mut world = walker_in(ObjectState::Alive);
    assert_eq!(
        world.move_to_point(PlayerId::new(1), ListIndex::new(2), POINT),
        Ok(Answer::NotTaken)
    );
    assert_eq!(kept(&world), (true, Speed::new(1.0), (1, 1)));
    let mut world = walker_in(ObjectState::Dying);
    assert_eq!(
        world.move_to_point(PlayerId::new(1), ListIndex::new(2), POINT),
        Ok(Answer::NotTaken)
    );
    assert_eq!(kept(&world), (false, Speed::ZERO, (1, 1)));
}

/// `FUN_14106d340` toward an object: the `MoveTo`'s constructor (`FUN_140aa0df0`) takes town
/// centre 9150 before the install; in state 3 the list's add deletes it unstarted, and its
/// teardown lets the centre go and stops the unit.
#[test]
fn object_move_in_state_3_lets_the_target_of_the_new_move_to_go() {
    let mut world = walker_in(ObjectState::Alive);
    assert_eq!(
        world.move_to_object(PlayerId::new(1), ListIndex::new(2), object_id(9150), POINT),
        Ok(Answer::NotTaken)
    );
    assert_eq!(kept(&world), (true, Speed::new(1.0), (1, 2)));
    let mut world = walker_in(ObjectState::Dying);
    assert_eq!(
        world.move_to_object(PlayerId::new(1), ListIndex::new(2), object_id(9150), POINT),
        Ok(Answer::NotTaken)
    );
    assert_eq!(kept(&world), (false, Speed::ZERO, (1, 1)));
}

/// Slot 0x340's move (`FUN_1410866e0`) from the unit's step and from the world (the move off a
/// footprint): in state 3 the install deletes the new `MoveTo` unstarted, which stops the unit;
/// either way the move answers 1.
#[test]
fn ai_point_move_in_state_3_deletes_the_new_move_to() {
    for (state, held) in [
        (ObjectState::Alive, (true, Speed::new(1.0), (1, 1))),
        (ObjectState::Dying, (false, Speed::ZERO, (1, 1))),
    ] {
        let mut stepped = walker_in(state);
        let answer = stepped.run_on_step(
            PlayerId::new(1),
            ListIndex::new(2),
            |unit, actions, step| {
                unit.ai_move(actions, AI_MOVE, class::VILLAGER, step, static_roles())
            },
        );
        assert_eq!(answer, Ok(Answer::Taken));
        let mut aside = walker_in(state);
        assert_eq!(
            aside.move_aside(PlayerId::new(1), ListIndex::new(2), AI_MOVE),
            Ok(Answer::Taken)
        );
        assert_eq!((kept(&stepped), kept(&aside)), (held, held), "{state:?}");
    }
}

/// Slot 0x348 (`FUN_141086440`) toward town centre 9150 from the unit's step: the constructor
/// takes it; in state 3 the install deletes the new `MoveTo` unstarted, which lets it go and
/// stops the unit.
#[test]
fn ai_object_move_in_state_3_lets_the_target_of_the_new_move_to_go() {
    for (state, held) in [
        (ObjectState::Alive, (true, Speed::new(1.0), (1, 2))),
        (ObjectState::Dying, (false, Speed::ZERO, (1, 1))),
    ] {
        let mut world = walker_in(state);
        world
            .run_on_step(
                PlayerId::new(1),
                ListIndex::new(2),
                |unit, actions, step| {
                    unit.move_to_object(
                        actions,
                        (object_id(9150), ObjectState::Alive),
                        Distance::ZERO,
                        step,
                        static_roles(),
                    )
                },
            )
            .unwrap();
        assert_eq!(kept(&world), held, "{state:?}");
    }
}

/// The `MoveTo` the villager holds after a move toward town centre 9150 from the world, and the
/// centre's workers.
fn moved_to_centre(mut world: World) -> (Option<MoveTo>, i32) {
    assert_eq!(
        world.move_to_object(PlayerId::new(1), ListIndex::new(2), object_id(9150), POINT),
        Ok(Answer::NotTaken)
    );
    let held = world
        .parts
        .actions_of(object_id(40))
        .list
        .move_to()
        .map(|move_to| move_to.state);
    (held, town_workers_and_holding(&world).1)
}

/// [`moved_to_centre`] by slot 0x348 from the villager's step.
fn ai_moved_to_centre(mut world: World) -> (Option<MoveTo>, i32) {
    world
        .run_on_step(
            PlayerId::new(1),
            ListIndex::new(2),
            |unit, actions, step| {
                unit.move_to_object(
                    actions,
                    (object_id(9150), ObjectState::Alive),
                    Distance::ZERO,
                    step,
                    static_roles(),
                )
            },
        )
        .unwrap();
    let held = world
        .parts
        .actions_of(object_id(40))
        .list
        .move_to()
        .map(|move_to| move_to.state);
    (held, town_workers_and_holding(&world).1)
}

/// [`walker_in`] alive, its villager's master of radii 8 (`+0x5d8`, `+0x5dc`) where its walker
/// keeps 0.2.
fn wide_walker() -> World {
    let mut world = walker_in(ObjectState::Alive);
    for masters in &mut world.model.masters {
        if let Some(master) = Arc::make_mut(masters).get_mut(ids::VILLAGER_MALE) {
            master.radius = Footprint::new(8.0, 8.0);
        }
    }
    world
}

/// The start measures its gap (`FUN_14118f680`) from the unit's master's radii, from the world
/// and from the step: radii 8 reach the centre's footprint (state 1), the walker's 0.2 do not (3).
#[test]
fn object_move_start_measures_from_the_master_radii() {
    let walking = (Some(MoveTo::Start), 2);
    let arrived = (Some(MoveTo::Arrived), 2);
    assert_eq!(moved_to_centre(walker_in(ObjectState::Alive)), walking);
    assert_eq!(ai_moved_to_centre(walker_in(ObjectState::Alive)), walking);
    assert_eq!(moved_to_centre(wide_walker()), arrived);
    assert_eq!(ai_moved_to_centre(wide_walker()), arrived);
}

/// [`walker_in`] alive, its villager without movement state.
fn unmoving_villager() -> World {
    let mut world = walker_in(ObjectState::Alive);
    world.set_motion(object_id(40), Some(Motion::Animated(Animated::default())));
    world
}

/// A unit without movement state has no top speed (slot 0x208, `0x141071810`): its `MoveTo`
/// toward an object fails at its start (state 13), its target held, from the world and from
/// the step.
#[test]
fn object_move_of_unit_without_movement_fails_at_its_start() {
    let failed = (Some(MoveTo::Failed), 2);
    assert_eq!(moved_to_centre(unmoving_villager()), failed);
    assert_eq!(ai_moved_to_centre(unmoving_villager()), failed);
}
