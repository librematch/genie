//! The mule cart's searches for resources and its work command toward the one it found.

use super::*;
use crate::map::spatial::Placed;
use crate::objects::masters::{ActionKind, ClassId, Master, Masters, Task};
use crate::players::PlayerKind;
use crate::test_ids::class_id;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::testing::ids;
use crate::world::tests::cart_world;
use std::sync::Arc;

/// The cart's list index in [`cart_world`]: after the forager, the house's foundation and the
/// town centre.
const CART: ListIndex = ListIndex::new(3);

/// The cart 750's unit AI's current target (`+0x48`); none without one or without a unit AI.
fn cart_target(world: &World) -> Option<ObjectId> {
    world
        .parts
        .unit_ai
        .get(object_id(750))
        .and_then(|unit_ai| unit_ai.current_target)
}

/// The cart's master's tasks are `tasks`: each its action type and target class.
fn cart_tasks(world: &mut World, tasks: &[(ActionKind, i16)]) {
    for masters in &mut world.model.masters {
        let Some(cart) = Arc::make_mut(masters).get_mut(ids::MASTER_1808) else {
            continue;
        };
        let action = cart.action.get_or_insert_with(Default::default);
        let template = action.tasks[0];
        action.tasks = tasks
            .iter()
            .map(|&(action_kind, class)| Task {
                action_kind,
                object_group: Some(class_id(class)),
                ..template
            })
            .collect();
    }
}

/// The bushes (master 59) gather as group `kind`.
fn bushes_in(world: &mut World, kind: GatherGroup) {
    for masters in &mut world.model.masters {
        if let Some(bush) = Arc::make_mut(masters).get_mut(master_id(59)) {
            bush.gather_group = kind;
        }
    }
}

/// Slot 0x1e8's groups (`FUN_140fdd5c0` with 1): the cart's task of class 7 searches group 1,
/// 8 group 3, 15 group 0, 32 and 63 group 4; a class of none of them (16) searches nothing.
#[test]
fn cart_find_searches_gather_group_of_its_task_class() {
    let found = |class: i16, kind: GatherGroup| {
        let mut world = cart_world();
        cart_tasks(&mut world, &[(CART_TASK, class)]);
        bushes_in(&mut world, kind);
        world
            .cart_find(PlayerId::new(1), CART)
            .map(|(found, _)| found)
    };
    let bush = Ok(Some(object_id(600)));
    assert_eq!(found(7, GatherGroup::FORAGE), bush);
    assert_eq!(found(8, GatherGroup::STONE), bush);
    assert_eq!(found(15, GatherGroup::WOOD), bush);
    assert_eq!(found(32, GatherGroup::GOLD), bush);
    assert_eq!(found(63, GatherGroup::GOLD), bush);
    assert_eq!(found(8, GatherGroup::FORAGE), Ok(None));
    assert_eq!(found(16, GatherGroup::WOOD), Ok(None));
}

/// `FUN_1415cd2a0` answers each task in turn with the target class of the last task of type
/// 0x99 so far: a task of another type before it searches nothing, one after it the cart task's
/// group again, never its own class's.
#[test]
fn cart_find_reads_class_of_last_cart_task_so_far() {
    let found = |tasks: &[(ActionKind, i16)]| {
        let mut world = cart_world();
        cart_tasks(&mut world, tasks);
        world
            .cart_find(PlayerId::new(1), CART)
            .map(|(found, _)| found)
    };
    let gather = ActionKind::named(5);
    assert_eq!(found(&[(gather, 7), (CART_TASK, 8)]), Ok(None));
    assert_eq!(found(&[(CART_TASK, 8), (gather, 7)]), Ok(None));
    assert_eq!(
        found(&[(CART_TASK, 8), (CART_TASK, 7)]),
        Ok(Some(object_id(600)))
    );
}

/// Bush 601 as the stone mine of master 60 (class 8, group 3) at `x`.
fn stone_at(world: &mut World, x: f32) {
    for masters in &mut world.model.masters {
        let stone = Master {
            id: master_id(60),
            class: ClassId::named(8),
            gather_group: GatherGroup::STONE,
            ..masters.get(master_id(59)).unwrap().clone()
        };
        *masters = Arc::new(Masters::new(masters.iter().cloned().chain([stone])));
    }
    let mine = world
        .live_object_mut(PlayerId::new(0), object_id(601))
        .unwrap();
    mine.life.master_id = master_id(60);
    mine.place_for_test(Position::new(x, 89.5, 0.0));
    let placed = *world.spatial.get(object_id(601)).unwrap();
    world.spatial.insert(
        object_id(601),
        Placed {
            position: Position::new(x, 89.5, 0.0),
            master: master_id(60),
            class: ClassId::named(8),
            ..placed
        },
    );
}

/// Of the resources each group's search found, the cart takes the nearest by the footprints'
/// squared gap (`FUN_14118f8c0`, the first of equals) with its place: bush 600 at 32.5 before the
/// stone at 36.5, the stone at 28.5 before the bush.
#[test]
fn cart_find_takes_nearest_of_found_resources() {
    let found = |x: f32| {
        let mut world = cart_world();
        cart_tasks(&mut world, &[(CART_TASK, 7), (CART_TASK, 8)]);
        stone_at(&mut world, x);
        world.cart_find(PlayerId::new(1), CART)
    };
    assert_eq!(
        found(36.5),
        Ok((Some(object_id(600)), Position::new(32.5, 89.5, 0.0)))
    );
    assert_eq!(
        found(28.5),
        Ok((Some(object_id(601)), Position::new(28.5, 89.5, 0.0)))
    );
}

/// Slot 0x1e8 ([`World::cart_work_search`]): the search, then the work command (slot 0x140)
/// toward what it found, which the unit AI takes as its target; with nothing found the work
/// command (its command count raised) goes to the cart's own place without a target
/// ([`World::cart_work`]).
#[test]
fn cart_work_search_sends_cart_to_work_on_what_it_found() {
    let mut world = cart_world();
    assert_eq!(world.cart_work_search(PlayerId::new(1), CART), Ok(()));
    assert_eq!(cart_target(&world), Some(object_id(600)));
    let mut world = cart_world();
    let commands = world.parts.actions_of(object_id(750)).commands;
    assert_eq!(
        world.cart_work(
            PlayerId::new(1),
            CART,
            (None, Position::new(24.5, 89.5, 0.0))
        ),
        Ok(())
    );
    assert_eq!(cart_target(&world), None);
    assert_ne!(world.parts.actions_of(object_id(750)).commands, commands);
}

/// A completed cart (`FUN_1411c6bd0`'s tail) with a unit AI: a human player's without a target
/// runs slot 0x1e8, a computer player's does not; with a target (the work command's, or one it
/// had) it moves to it (slot 0x138). All of it runs in the completion's `set_state` from state 0,
/// the cart's state its own again after.
#[test]
fn completed_cart_searches_for_human_and_moves_to_its_target() {
    let completed = |setup: fn(&mut World)| {
        let mut world = cart_world();
        setup(&mut world);
        let result = world.cart_search(PlayerId::new(1), object_id(750), ObjectState::Foundation);
        let cart = world.live_object(PlayerId::new(1), object_id(750)).unwrap();
        (
            result,
            cart_target(&world),
            cart.life.state,
            world.parts.actions_of(object_id(750)).list.is_empty(),
        )
    };
    assert_eq!(
        completed(|_| ()),
        (Ok(()), Some(object_id(600)), ObjectState::Alive, false)
    );
    assert_eq!(
        completed(|world| world.players[1].kind = PlayerKind::Computer),
        (Ok(()), None, ObjectState::Alive, true)
    );
    assert_eq!(
        completed(|world| {
            world
                .parts
                .unit_ai
                .get_mut(object_id(750))
                .unwrap()
                .current_target = Some(object_id(601));
        }),
        (Ok(()), Some(object_id(601)), ObjectState::Alive, false)
    );
    // without a unit AI nothing happens
    assert_eq!(
        completed(|world| world.parts.set_unit_ai(object_id(750), None)),
        (Ok(()), None, ObjectState::Alive, true)
    );
}
