//! The unit's task search (slot 0x820, [`World::find_work_task`]).

use super::*;
use crate::combat::attack::AttackState;
use crate::combat::attack::tests::own_attack;
use crate::map::Distance;
use crate::map::GridSize;
use crate::map::Tile;
use crate::objects::MasterHitPoints;
use crate::objects::actions::ActionList;
use crate::objects::layers::Action;
use crate::objects::masters::ActionPart;
use crate::objects::masters::ClassId;
use crate::objects::masters::Master;
use crate::objects::masters::MasterKind;
use crate::objects::masters::Masters;
use crate::objects::masters::TaskGroup;
use crate::objects::masters::TaskId;
use crate::objects::masters::TaskTerrain;
use crate::objects::masters::WorkValue;
use crate::orders::GARRISON;
use crate::test_ids::object_id;
use crate::testing::class;
use crate::testing::ids;
use crate::testing::scene::{PLAYER, Scene, SceneObject, WATER};
use crate::world::index::Place;
use std::sync::Arc;

/// The unit that searches (object 7).
const SEARCHER: ObjectId = object_id(7);

/// Its player's house foundation (object 8).
const FOUNDATION: ObjectId = object_id(8);

/// A Gaia object of a class no task names (object 10).
const STONE: ObjectId = object_id(10);

/// A villager master `id` (type 70, class 4) of swap group `group` with `tasks`.
fn villager(id: i16, group: TaskGroup, tasks: Vec<Task>) -> Master {
    Master {
        id: MasterId::named(id),
        kind: MasterKind::COMBAT,
        class: class::VILLAGER,
        hit_points: MasterHitPoints::of_data(25),
        action: Some(ActionPart {
            task_by_group: group,
            tasks,
            ..ActionPart::default()
        }),
        ..Master::default()
    }
}

/// A Build task (101) `id` for any target, with a work value.
fn build(id: i16) -> Task {
    Task {
        id: TaskId::of_task(id),
        action_kind: ActionKind::BUILD,
        work_val1: WorkValue::new(1.0),
        ..Task::default()
    }
}

/// A house of 550 hit points.
fn house() -> Master {
    Master {
        id: ids::HOUSE,
        kind: MasterKind::BUILDING,
        class: class::BUILDING,
        hit_points: MasterHitPoints::of_data(550),
        ..Master::default()
    }
}

/// A stone of a class no task names.
fn stone() -> Master {
    Master {
        id: MasterId::named(99),
        kind: MasterKind::STATIC,
        class: ClassId::named(99),
        ..Master::default()
    }
}

/// The searcher, of master `own` and attacking the foundation, its player's foundation, and
/// the stone on land beside water (tile 4, 4); the searcher's player holds the masters `masters`
/// and the house.
fn search_world(own: Master, masters: Vec<Master>) -> World {
    let attacking = Action {
        list: ActionList::default().with_attack(Some(own_attack(
            FOUNDATION.get(),
            AttackState::New,
            Distance::new(1.0),
        ))),
        ..Action::default()
    };
    let mut world = Scene::open(GridSize::new(8, 8))
        .water([Tile::new(4, 4)])
        .object(SceneObject {
            actions: Some(attacking),
            ..SceneObject::new(SEARCHER.get(), PLAYER, own, Position::new(2.5, 2.5, 0.0))
        })
        .object(SceneObject {
            state: ObjectState::Foundation,
            desolid: true,
            ..SceneObject::new(FOUNDATION.get(), PLAYER, house(), FOUNDATION_PLACE)
        })
        .object(SceneObject::new(
            STONE.get(),
            PlayerId::GAIA,
            stone(),
            Position::new(2.5, 5.5, 0.0),
        ))
        .build();
    world.model.masters[1] = Arc::new(Masters::new(masters.into_iter().chain([house()])));
    world
}

/// What slot 0x820 of the searcher finds on `target` at `point` with `filter` and `swap`: the
/// task's id, and the searcher's master after it and whether it still attacks.
fn search(
    world: &mut World,
    target: ObjectId,
    point: Position,
    filter: Option<ActionKind>,
    swap: MasterSwap,
) -> (Option<TaskId>, MasterId, bool) {
    let Some(Place::Live(player, idx)) = world.place(SEARCHER) else {
        panic!("the searcher lives");
    };
    let found = world
        .find_work_task(player, idx, target, point, filter, swap)
        .unwrap();
    let unit = world.players.live(player, idx).unwrap();
    let attacks = world.parts.actions_of(unit.id).list.attack().is_some();
    (found.map(|task| task.id), unit.life.master_id, attacks)
}

/// The foundation's place.
const FOUNDATION_PLACE: Position = Position::new(5.5, 5.5, 0.0);

/// The unit's own master's list answers with the caller's filter: no other master is asked and
/// the unit keeps its master and its actions, also with the switch.
#[test]
fn own_task_keeps_the_master_also_with_the_switch() {
    let own = villager(83, TaskGroup::MALE_VILLAGERS, vec![build(1)]);
    let builder = villager(118, TaskGroup::MALE_VILLAGERS, vec![build(2)]);
    let mut world = search_world(own.clone(), vec![own, builder]);
    assert_eq!(
        search(
            &mut world,
            FOUNDATION,
            FOUNDATION_PLACE,
            None,
            MasterSwap::Change
        ),
        (Some(TaskId::of_task(1)), MasterId::named(83), true)
    );
}

/// Under the garrison filter its own list has no task on the foundation; the swap group asks
/// its own master first again, with any filter, and its Build answers. With the switch the unit
/// takes its own master (slot 0x90 with 0): its actions go.
#[test]
fn own_master_answers_without_the_filter_and_the_switch_clears_the_actions() {
    let own = villager(83, TaskGroup::MALE_VILLAGERS, vec![build(1)]);
    let builder = villager(118, TaskGroup::MALE_VILLAGERS, vec![build(2)]);
    let mut world = search_world(own.clone(), vec![own.clone(), builder.clone()]);
    assert_eq!(
        search(
            &mut world,
            FOUNDATION,
            FOUNDATION_PLACE,
            Some(GARRISON),
            MasterSwap::Keep
        ),
        (Some(TaskId::of_task(1)), MasterId::named(83), true)
    );
    let mut world = search_world(own.clone(), vec![own, builder]);
    assert_eq!(
        search(
            &mut world,
            FOUNDATION,
            FOUNDATION_PLACE,
            Some(GARRISON),
            MasterSwap::Change
        ),
        (Some(TaskId::of_task(1)), MasterId::named(83), false)
    );
}

/// Without a task of its own, the swap group asks its player's masters by id, of the unit's
/// class, type and group only: the first with a task gives it, and the unit takes that master
/// only with the switch.
#[test]
fn swap_group_takes_the_first_master_of_its_class_kind_and_group() {
    let own = villager(83, TaskGroup::MALE_VILLAGERS, Vec::new());
    let masters = || {
        vec![
            own.clone(),
            Master {
                class: ClassId::named(99),
                ..villager(90, TaskGroup::MALE_VILLAGERS, vec![build(90)])
            },
            Master {
                kind: MasterKind::BUILDING,
                ..villager(91, TaskGroup::MALE_VILLAGERS, vec![build(91)])
            },
            villager(92, TaskGroup::FEMALE_VILLAGERS, vec![build(92)]),
            villager(118, TaskGroup::MALE_VILLAGERS, vec![build(118)]),
            villager(119, TaskGroup::MALE_VILLAGERS, vec![build(119)]),
        ]
    };
    let mut world = search_world(own.clone(), masters());
    assert_eq!(
        search(
            &mut world,
            FOUNDATION,
            FOUNDATION_PLACE,
            None,
            MasterSwap::Keep
        ),
        (Some(TaskId::of_task(118)), MasterId::named(83), true)
    );
    let mut world = search_world(own.clone(), masters());
    assert_eq!(
        search(
            &mut world,
            FOUNDATION,
            FOUNDATION_PLACE,
            None,
            MasterSwap::Change
        ),
        (Some(TaskId::of_task(118)), MasterId::named(118), false)
    );
}

/// A master without a swap group asks no other master: no task, and nothing changes.
#[test]
fn master_without_swap_group_asks_no_other() {
    let own = villager(83, TaskGroup::NONE, Vec::new());
    let builder = villager(118, TaskGroup::NONE, vec![build(118)]);
    let mut world = search_world(own.clone(), vec![own, builder]);
    assert_eq!(
        search(
            &mut world,
            FOUNDATION,
            FOUNDATION_PLACE,
            None,
            MasterSwap::Change
        ),
        (None, MasterId::named(83), true)
    );
}

/// The terrain the search reads is the one under the point its caller passes, not under the
/// target: a task for water answers on the stone only at a point on the water, and a point of
/// −1 has no terrain.
#[test]
fn the_point_its_caller_passes_names_the_terrain() {
    let wet = Task {
        id: TaskId::of_task(3),
        action_kind: ActionKind::named(99),
        terrain: TaskTerrain::of_data(i16::from(WATER.get())),
        ..Task::default()
    };
    let own = villager(83, TaskGroup::NONE, vec![wet]);
    let mut world = search_world(own.clone(), vec![own]);
    let found = |world: &mut World, point| search(world, STONE, point, None, MasterSwap::Keep).0;
    assert_eq!(
        found(&mut world, Position::new(4.5, 4.5, 0.0)),
        Some(TaskId::of_task(3))
    );
    assert_eq!(found(&mut world, Position::new(2.5, 5.5, 0.0)), None);
    assert_eq!(found(&mut world, Position::new(-1.0, -1.0, -1.0)), None);
}
