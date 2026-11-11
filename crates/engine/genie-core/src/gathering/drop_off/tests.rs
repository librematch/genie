//! A gatherer's walk after a drop and the drop sites it walks to.

use super::*;
use crate::gathering::gather::GatherState;
use crate::gathering::gather::tests::one_frame;
use crate::gathering::{DropSiteId, GatherTargetId};
use crate::map::Footprint;
use crate::map::Point;
use crate::map::spatial::Placed;
use crate::math::rng::Stream;
use crate::movement::TopSpeed;
use crate::movement::footprint_gap;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::SpriteId;
use crate::objects::WorkerCount;
use crate::objects::masters::MasterKind;
use crate::objects::masters::{AutoGatherMode, ClassId, GatherGroup, OwnerMasters, TaskSprites};
use crate::ownership::CaptureKind;
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::players::attribute::AttributeId;
use crate::test_ids::class_id;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::test_ids::{order_kind, owner, player_id};
use crate::testing::attribute;
use crate::testing::roles;
use crate::unit_ai::{AiAction, Issuer, OrderPriority, QueuedOrder};
use crate::vision::visibility::Seer;
use crate::world::World;
use crate::world::WorldTime;
use crate::world::tests::{dropped_without_bush, forage_world, foraging, in_update};
use std::sync::Arc;

/// The forager 9175 of [`forage_world`].
fn forager(world: &World) -> &Unit {
    &world.players[1].objects[ListIndex::new(0)]
}

/// The forager's Gather.
fn gather_of(world: &World) -> Option<GatherAction> {
    world
        .parts
        .actions_of(forager(world).id)
        .list
        .gather()
        .copied()
}

/// The workers of object `id`.
fn workers(world: &World, id: ObjectId) -> WorkerCount {
    world.object(id).map(|(_, unit)| unit.workers).unwrap()
}

/// The bushes (master 59) gather by themselves as group 1, the forager sees 4 tiles, and bush 601
/// stands at (25.5, 89.5), beside the town centre: the unit AI's search after the drop finds it.
fn bush_beside_centre(world: &mut World) {
    for masters in &mut world.model.masters {
        let bush = Arc::make_mut(masters).get_mut(master_id(59)).unwrap();
        (bush.auto_gather, bush.gather_group) = (AutoGatherMode::ON, GatherGroup::FORAGE);
        Arc::make_mut(masters)
            .get_mut(master_id(120))
            .unwrap()
            .line_of_sight = Distance::new(4.0);
    }
    world
        .live_object_mut(PlayerId::new(0), object_id(601))
        .unwrap()
        .place_for_test(Position::new(25.5, 89.5, 0.0));
    let placed = *world.spatial.get(object_id(601)).unwrap();
    world.spatial.insert(
        object_id(601),
        Placed {
            position: Position::new(25.5, 89.5, 0.0),
            ..placed
        },
    );
}

/// The forager's unit AI's target class (`+0x58`) is `class`.
fn ai_target_class(world: &mut World, class: Option<ClassId>) {
    let id = forager(world).id;
    world.parts.unit_ai.get_mut(id).unwrap().target_class = class;
}

/// The forager's Gather's target class (`mTargetType`) is `class`.
fn gather_target_class(world: &mut World, class: Option<ClassId>) {
    let id = forager(world).id;
    world
        .parts
        .actions
        .of_mut(id)
        .unwrap()
        .list
        .gather_mut()
        .unwrap()
        .target_class = class;
}

/// After the drop without a target the unit AI's search finds bush 601 and the Gather takes it
/// (slot 0x48): the Gather's targets that changed are let go and taken, the town centre's worker
/// leaving and the bush counting one; the Gather walks to the bush (state 4).
#[test]
fn retarget_after_drop_lets_drop_site_go_and_takes_next_bush() {
    let world = dropped_without_bush(|world| {
        bush_beside_centre(world);
        ai_target_class(world, None);
    });
    let gather_action = gather_of(&world).unwrap();
    assert_eq!(
        (
            gather_action.state.bare(),
            gather_action.target,
            gather_action.carry_to
        ),
        (
            GatherState::Walk { move_to: None },
            Some(GatherTargetId::new(object_id(601))),
            None
        )
    );
    assert_eq!(
        (
            workers(&world, object_id(601)),
            workers(&world, object_id(700))
        ),
        (WorkerCount::of_data(1), WorkerCount::of_data(0))
    );
}

/// The search after the drop asks the gather group of the unit AI's target class (its `+0x58`,
/// `FUN_140c4cc50`), else of the Gather's target type: a class of no group (99) finds nothing,
/// and the Gather walks back to its point (state 11).
#[test]
fn after_drop_search_takes_unit_ai_target_class_before_gather_target_class() {
    let after_drop = |ai_class: Option<ClassId>, gather_class: Option<ClassId>| {
        let world = dropped_without_bush(|world| {
            bush_beside_centre(world);
            ai_target_class(world, ai_class);
            gather_target_class(world, gather_class);
        });
        gather_of(&world).map(|gather_action| (gather_action.state.bare(), gather_action.target))
    };
    let found = Some((
        GatherState::Walk { move_to: None },
        Some(GatherTargetId::new(object_id(601))),
    ));
    let none_found = Some((GatherState::WalkBack { move_to: None }, None));
    assert_eq!(after_drop(Some(class_id(7)), Some(class_id(99))), found);
    assert_eq!(after_drop(None, Some(class_id(7))), found);
    assert_eq!(
        after_drop(Some(class_id(99)), Some(class_id(7))),
        none_found
    );
    assert_eq!(after_drop(None, Some(class_id(99))), none_found);
}

/// A forager without a unit AI (`+0x6b0` null) skips the orders and the search after the drop:
/// with its point it walks back there to search (state 11) though a bush stands near.
#[test]
fn forager_without_unit_ai_walks_back_after_drop() {
    let world = dropped_without_bush(|world| {
        bush_beside_centre(world);
        let id = forager(world).id;
        world.parts.set_unit_ai(id, None);
    });
    let gather_action = gather_of(&world).unwrap();
    assert_eq!(
        (gather_action.state.bare(), gather_action.target),
        (GatherState::WalkBack { move_to: None }, None)
    );
    assert_eq!(workers(&world, object_id(601)), WorkerCount::of_data(0));
}

/// The sprite an idling forager shows (`set_state(2)`'s `FUN_140a8b6b0`): the task's carrying
/// sprite while it holds at least 1 (truncated) of the task's resource, else its master's
/// standing sprite; none where the sprites' table lacks it.
#[test]
fn idle_forager_shows_carrying_sprite_only_holding_task_resource() {
    let shown = |held: (Option<AttributeId>, f32), carrying: Option<i16>, standing: i16| {
        let mut world = foraging();
        world.model.graphics = Arc::new(Graphics::from(
            [1, 4].map(|id| (SpriteId::named(id), one_frame(id))),
        ));
        for masters in &mut world.model.masters {
            Arc::make_mut(masters)
                .get_mut(master_id(120))
                .unwrap()
                .standing_sprite = Some(SpriteId::named(standing));
        }
        in_update(
            &mut world,
            (PlayerId::new(1), ListIndex::new(0)),
            50,
            |unit, actions, step| {
                (unit.life.attribute_kind, unit.life.amount) = (held.0, Amount::new(held.1));
                let mut gather = *actions.list.gather().unwrap();
                gather.task.sprites = TaskSprites {
                    carrying: carrying.map(SpriteId::named),
                    ..TaskSprites::NONE
                };
                unit.idle_sprite(&gather, step)
            },
        )
        .unwrap()
        .map(|graphic| graphic.id)
    };
    let berries = Some(AttributeId::named(17));
    let (carrying, standing) = (Some(SpriteId::named(4)), Some(SpriteId::named(1)));
    assert_eq!(shown((berries, 1.0), Some(4), 1), carrying);
    assert_eq!(shown((berries, 0.99), Some(4), 1), standing);
    assert_eq!(shown((Some(attribute::STONE), 5.0), Some(4), 1), standing);
    assert_eq!(shown((berries, 5.0), None, 1), standing);
    // a standing sprite the table lacks shows none
    assert_eq!(shown((berries, 0.0), Some(4), 2), None);
}

/// The Gather's `set_state(8)` on a forager carrying 10 berries without a second target: its
/// master's drop site, the town centre 700, becomes the second target (slot 0x178: a worker),
/// and a quiet `MoveTo` walks there (state 8); again with the centre its second target the
/// centre is not taken twice.
#[test]
fn gather_drop_off_takes_its_drop_site_once() {
    let mut world = foraging();
    let drop_off = |world: &mut World| {
        in_update(
            world,
            (PlayerId::new(1), ListIndex::new(0)),
            50,
            |unit, actions, step| {
                (unit.life.attribute_kind, unit.life.amount) =
                    (Some(AttributeId::named(17)), Amount::new(10.0));
                let mut gather = *actions.list.gather().unwrap();
                unit.gather_drop_off(&mut gather, actions, step, &roles())
            },
        )
    };
    assert_eq!(drop_off(&mut world), Ok(()));
    let gather_action = gather_of(&world).unwrap();
    assert_eq!(
        (
            gather_action.state.bare(),
            gather_action.carry_to,
            gather_action
                .state
                .move_to()
                .and_then(|move_to_action| move_to_action.target)
        ),
        (
            GatherState::Drop { move_to: None },
            Some(DropSiteId::new(object_id(700))),
            Some(object_id(700))
        )
    );
    assert_eq!(workers(&world, object_id(700)), WorkerCount::of_data(1));
    assert_eq!(drop_off(&mut world), Ok(()));
    assert_eq!(workers(&world, object_id(700)), WorkerCount::of_data(1));
}

/// `f` with [`forage_world`]'s forager, its action layer and mover, the world a drop-site search
/// reads (its player `player_kind`'s, no tile planner), its player's objects and the objects by
/// id.
fn drop_search<R>(
    world: &mut World,
    player_kind: PlayerKind,
    search: impl FnOnce(
        &Unit,
        (&Action, Option<&Mover>),
        &mut DropWorld<'_>,
        OthersRef<'_>,
        ObjectsRef<'_>,
    ) -> R,
) -> R {
    let mut drop_world = DropWorld {
        top_speed: TopSpeed::Own,
        masters: OwnerMasters::new(
            &world.parts.own_master,
            world.model.masters.get(1).map(|masters| &**masters),
        ),
        spatial: &mut world.spatial,
        zones: &mut world.zones,
        pathing: &mut world.pathing,
        object: world.streams.get_mut(Stream::Object),
        world_time: WorldTime::ZERO,
        player_kind,
        groups: &world.groups,
    };
    let others = OthersRef::of(&world.players, &world.index, PlayerId::new(1)).unwrap();
    let unit = &world.players[1].objects[ListIndex::new(0)];
    search(
        unit,
        (
            world.parts.actions_of(unit.id),
            world.parts.mover_of(unit.id),
        ),
        &mut drop_world,
        others,
        ObjectsRef::new(
            &world.players,
            &world.index,
            &world.model.masters,
            &world.parts,
            &world.model.roles,
            &world.model.tuning,
            world.treaty.as_ref(),
        ),
    )
}

/// The drop site of `set_state(8)` ([`Unit::drop_site`]) for the forager with the second target
/// `carry_to`, by a human player: that target with the footprints' gap to it; without one its
/// master's drop sites searched (the search by path finds none without a tile planner, the one
/// by distance the town centre); none for a master without drop sites.
#[test]
fn drop_site_keeps_second_target_or_searches_master_drop_sites() {
    let site = |world: &mut World, carry_to: Option<DropSiteId>| {
        drop_search(
            world,
            PlayerKind::Human,
            |unit, (actions, mover), drop_world, others, by_id| {
                unit.drop_site(
                    (actions, mover),
                    drop_world,
                    others,
                    carry_to,
                    ActionState::NO_ACTION,
                    by_id,
                )
            },
        )
    };
    let gap = footprint_gap(
        Position::new(29.5, 89.5, 0.0),
        Footprint::new(0.2, 0.2),
        Position::new(20.0, 89.5, 0.0),
        Footprint::new(2.0, 2.0),
    );
    let centre = Some(DropSiteId::new(object_id(700)));
    let mut world = forage_world();
    assert_eq!(site(&mut world, centre), Ok((centre, gap)));
    assert_eq!(site(&mut world, None), Ok((centre, gap)));
    for masters in &mut world.model.masters {
        Arc::make_mut(masters)
            .get_mut(master_id(120))
            .unwrap()
            .action
            .as_mut()
            .unwrap()
            .drop_sites
            .clear();
    }
    assert_eq!(site(&mut world, None), Ok((None, Distance::ZERO)));
}

/// `FUN_1411671e0` from the forager (its argument 9, `FUN_141659cf0`): a human player's search
/// measures by path, which without a tile planner keeps the largest float and finds no drop
/// site; a computer player's by the squared distance, which finds the town centre.
#[test]
fn drop_site_from_measures_human_by_path_and_computer_by_distance() {
    let site = |player_kind: PlayerKind| {
        let mut world = forage_world();
        drop_search(
            &mut world,
            player_kind,
            |unit, (actions, mover), drop_world, others, by_id| {
                unit.drop_site_from(
                    (actions, mover),
                    drop_world,
                    others,
                    ActionState::NO_ACTION,
                    SitePlace::Outside,
                    by_id,
                )
            },
        )
    };
    assert_eq!(site(PlayerKind::Human), Ok(None));
    assert_eq!(
        site(PlayerKind::Computer),
        Ok(Some(DropSiteId::new(object_id(700))))
    );
}

/// A queued order of `order` on `target`, as player 1 gave it.
fn queued(order: OrderKind, target: Option<ObjectId>) -> QueuedOrder {
    QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order,
        priority: OrderPriority::of_data(100),
        target,
        target_owner: owner(0),
        point: Position::new(36.5, 89.5, 0.0),
        range: Distance::ZERO,
        shown: Point::new(36.5, 89.5),
        ready: true,
        target_kind: order_kind(7),
    }
}

/// Bush `id` (master 59, Gaia's) in `state`: the object's own state (`+0x640`) and its sight's
/// record of it, as a state change writes both.
fn bush_in_state(world: &mut World, id: ObjectId, state: ObjectState) {
    world
        .live_object_mut(PlayerId::new(0), id)
        .unwrap()
        .life
        .state = state;
    world.spatial.sight.as_mut().unwrap().add_seer(
        id,
        Seer {
            player: PlayerId::new(0),
            master: master_id(59),
            los: Distance::ZERO,
            los_powerup: Distance::ZERO,
            state,
            explored: true,
            building: None,
            umv: 0,
            capture: CaptureKind::None,
            kind: MasterKind::MOVING,
        },
    );
}

/// The unit AI's queued orders after the drop (`FUN_140c5de60`, from the queue's end): an
/// order to gather, to hunt or on a target (0x2c5, 0x2c9, 0x2db) leaves where the world holds
/// no target (602) or its target is past state 5 (bush 601 undead; bush 600 dead stays); an
/// order of another kind stays. With orders left the unit AI ends its order and action
/// (`FUN_140c64b60`, `FUN_140c64b50`) and the Gather ends; with none it goes on.
#[test]
fn after_drop_orders_leave_on_targets_gone_and_end_gather_with_orders_left() {
    let after_drop = |orders: Vec<QueuedOrder>| {
        let mut world = foraging();
        bush_in_state(&mut world, object_id(600), ObjectState::Dead);
        bush_in_state(&mut world, object_id(601), ObjectState::Undead);
        let id = forager(&world).id;
        let ai = world.parts.unit_ai.get_mut(id).unwrap();
        (ai.orders, ai.current_order, ai.current_action) =
            (orders, Some(OrderKind::Gather), Some(AiAction::Gather));
        let ended = in_update(
            &mut world,
            (PlayerId::new(1), ListIndex::new(0)),
            50,
            |unit, _, step| unit.after_drop_orders(step),
        );
        let ai = world.parts.unit_ai.get(id).unwrap();
        let left: Vec<_> = ai
            .orders
            .iter()
            .map(|queued_order| (queued_order.order, queued_order.target))
            .collect();
        (ended, left, (ai.current_order, ai.current_action))
    };
    let gone = [
        queued(OrderKind::Gather, Some(object_id(602))),
        queued(OrderKind::Hunt, Some(object_id(601))),
        queued(OrderKind::OnTarget, None),
    ];
    assert_eq!(
        after_drop(gone.to_vec()),
        (
            Ok(ActionEnd::GoesOn),
            vec![],
            (Some(OrderKind::Gather), Some(AiAction::Gather))
        )
    );
    let mut kept = gone.to_vec();
    kept.insert(1, queued(OrderKind::Gather, Some(object_id(600))));
    kept.push(queued(OrderKind::Move, Some(object_id(602))));
    assert_eq!(
        after_drop(kept),
        (
            Ok(ActionEnd::Ends),
            vec![
                (OrderKind::Gather, Some(object_id(600))),
                (OrderKind::Move, Some(object_id(602)))
            ],
            (None, None)
        )
    );
}
