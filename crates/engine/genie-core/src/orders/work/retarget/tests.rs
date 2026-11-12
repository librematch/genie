//! The work command's notify once the action on top took the new target
//! ([`World::notify_retaken`]): the order and action of the action on top, an Attack's retarget
//! time, a unit without a unit AI and an empty action list.

use super::*;
use crate::combat::attack::AttackState;
use crate::combat::attack::tests::own_attack;
use crate::map::Distance;
use crate::test_ids::object_id;
use crate::testing::class;
use crate::unit_ai::OrderForce;
use crate::world::WorldTime;
use crate::world::tests::forage_world;

/// The forager's player.
const PLAYER: PlayerId = PlayerId::new(1);
/// The forager's slot.
const SLOT: ListIndex = ListIndex::FIRST;
/// The forager 9175 of [`forage_world`].
const FORAGER: ObjectId = object_id(9175);
/// The bush 600 of [`forage_world`] (class 7).
const BUSH: ObjectId = object_id(600);
/// The work's point.
const POINT: Position = Position::new(40.5, 80.5, 0.0);
/// The world's time of the notify.
const NOW: WorldTime = WorldTime::from_millis(3500);

/// The forager's unit AI's order, action, target, point and last retarget time.
type Told = (
    Option<OrderKind>,
    Option<AiAction>,
    Option<ObjectId>,
    Position,
    WorldTime,
);

/// What the forager's unit AI heard: [`Told`].
fn told(world: &World) -> Told {
    let ai = world.parts.unit_ai.get(FORAGER).unwrap();
    (
        ai.current_order,
        ai.current_action,
        ai.current_target,
        ai.target_point,
        ai.last_retarget_time,
    )
}

/// [`forage_world`] at [`NOW`], its forager under an Attack on the bush.
fn attacking() -> World {
    let mut world = forage_world();
    world
        .parts
        .actions
        .of_mut(FORAGER)
        .unwrap()
        .list
        .put(Action::Attack(own_attack(
            600,
            AttackState::Move { move_to: None },
            Distance::ZERO,
        )));
    world.world_time = NOW;
    world
}

/// An Attack on top (`FUN_14106e4e0`'s case 9): the unit AI hears the attack order and action on
/// the target at the point, and its last retarget time (`+0xbc`, `FUN_140c41bd0`) becomes the
/// world's time.
#[test]
fn notify_retaken_attack_sets_last_retarget_time() {
    let mut world = attacking();
    assert_eq!(
        world.notify_retaken(
            PLAYER,
            SLOT,
            Some((BUSH, class::CLASS_7)),
            POINT,
            Notifies::Cleared
        ),
        Ok(())
    );
    assert_eq!(
        told(&world),
        (
            Some(OrderKind::Attack),
            Some(AiAction::Attack),
            Some(BUSH),
            POINT,
            NOW
        )
    );
}

/// A Gather on top: the unit AI hears the gather order and action, and its last retarget time
/// stays.
#[test]
fn notify_retaken_gather_keeps_last_retarget_time() {
    let mut world = forage_world();
    assert_eq!(
        world.gather_command(PLAYER, SLOT, BUSH, OrderForce::Forced),
        Ok(Answer::Taken)
    );
    let before = told(&world).4;
    world.world_time = NOW;
    assert_eq!(
        world.notify_retaken(PLAYER, SLOT, None, POINT, Notifies::Cleared),
        Ok(())
    );
    assert_ne!(before, NOW);
    assert_eq!(
        told(&world),
        (
            Some(OrderKind::Gather),
            Some(AiAction::Gather),
            None,
            POINT,
            before
        )
    );
}

/// A unit without a unit AI hears nothing: the notify does nothing and answers.
#[test]
fn notify_retaken_without_unit_ai_does_nothing() {
    let mut world = attacking();
    let _ = world.lend_unit_ai(FORAGER);
    assert_eq!(
        world.notify_retaken(PLAYER, SLOT, None, POINT, Notifies::Cleared),
        Ok(())
    );
    assert!(world.parts.unit_ai.get(FORAGER).is_none());
}

/// An empty action list, whose head the game reads through a null pointer, is an error.
#[test]
fn notify_retaken_errors_on_empty_action_list() {
    let mut world = forage_world();
    assert!(world.parts.actions_of(FORAGER).list.head().is_none());
    assert_eq!(
        world
            .notify_retaken(PLAYER, SLOT, None, POINT, Notifies::Cleared)
            .map_err(|error| error.reason()),
        Err("a work command's retake that left no action on top")
    );
}
