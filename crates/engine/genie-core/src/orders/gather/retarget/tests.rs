//! The Gather's slot 0x48 on a command's new target (`0x140a903a0`, [`World::retarget_gather`]):
//! a walk to a point, the target it keeps, its player's foundation, the building test by the main
//! building, the drop test and its errors.

use super::*;
use crate::GenieError;
use crate::construction::FoundationId;
use crate::construction::MainBuildingId;
use crate::gathering::DropSiteId;
use crate::gathering::GatherTargetId;
use crate::gathering::gather::GatherAction;
use crate::map::Footprint;
use crate::map::spatial::Placed;
use crate::objects::WorkerCount;
use crate::objects::actions::Action;
use crate::objects::layers::{Building, Static};
use crate::objects::lifecycle::Lifecycle;
use crate::objects::masters::{ClassId, Master, Masters};
use crate::objects::unit::Unit;
use crate::players::attribute::AttributeId;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::testing::class;
use crate::testing::ids;
use crate::unit_ai::AiAction;
use crate::unit_ai::Notifies;
use crate::unit_ai::OrderForce;
use crate::unit_ai::OrderKind;
use crate::world::parts::Object;
use crate::world::tests::forage_world;
use std::sync::Arc;

/// The forager's player.
const PLAYER: PlayerId = PlayerId::new(1);
/// The forager's slot.
const SLOT: ListIndex = ListIndex::FIRST;
/// The forager 9175 of [`forage_world`].
const FORAGER: ObjectId = object_id(9175);
/// The bush its Gather works.
const BUSH: ObjectId = object_id(600);
/// Its player's house foundation (10028, state 0).
const HOUSE: ObjectId = object_id(10028);
/// Its player's town centre (700, master 109, a drop site of the forager's master).
const TOWN_CENTRE: ObjectId = object_id(700);
/// Its player's farm (800, master 50, class 49, state 2).
const FARM: ObjectId = object_id(800);
/// A master no building of the game has, for an annex: class 3, no drop site.
const ANNEX_MASTER: MasterId = MasterId::named(99);
/// Where a work command without a target sends the forager.
const POINT: Position = Position::new(40.5, 80.5, 0.0);

/// A building master `id` of class `class`, type 80, its radii 1.5.
fn building_master(id: MasterId, class: ClassId) -> Master {
    Master {
        id,
        kind: MasterKind::BUILDING,
        class,
        radius: Footprint::new(1.5, 1.5),
        ..Master::default()
    }
}

/// Building `id` of master `master` in `state` at (26.5, 89.5), the annex of `annex_of`.
fn building(
    id: ObjectId,
    master: MasterId,
    state: ObjectState,
    annex_of: Option<ObjectId>,
) -> Object {
    Object::with_building(
        Unit {
            id,
            place: Static::at(Position::new(26.5, 89.5, 0.0)),
            life: Lifecycle {
                state,
                master_id: master,
                ..Lifecycle::default()
            },
            pasture: None,
            my_pasture: None,
            ..Unit::default()
        },
        Building {
            annex_of: annex_of.map(MainBuildingId::new),
            ..Default::default()
        },
    )
}

/// [`forage_world`]'s forager on the bush (the gather command's new Gather), its work done this
/// update (`+0x80`); both players' tables hold a farm (50, class 49), a pasture (1897, class 3)
/// and [`ANNEX_MASTER`]; its player holds the farm [`FARM`].
fn gathering() -> World {
    let mut world = forage_world();
    for masters in &mut world.model.masters {
        *masters = Arc::new(Masters::new(masters.iter().cloned().chain([
            building_master(ids::FARM, class::FARM),
            building_master(ids::PASTURE, class::BUILDING),
            building_master(ANNEX_MASTER, class::BUILDING),
        ])));
    }
    let mut world = holding(
        world,
        PLAYER,
        building(FARM, ids::FARM, ObjectState::Alive, None),
    );
    assert_eq!(
        world.gather_command(PLAYER, SLOT, BUSH, OrderForce::Forced),
        Ok(Answer::Taken)
    );
    gather_mut(&mut world).worked = true;
    world
}

/// The forager's Gather.
fn gather_of(world: &World) -> Option<GatherAction> {
    world.parts.actions_of(FORAGER).list.gather().copied()
}

/// The forager's Gather to change.
fn gather_mut(world: &mut World) -> &mut GatherAction {
    world
        .parts
        .actions
        .of_mut(FORAGER)
        .unwrap()
        .list
        .gather_mut()
        .unwrap()
}

/// Object `id`'s workers.
fn workers(world: &World, id: ObjectId) -> WorkerCount {
    world.object(id).unwrap().1.workers
}

/// `world` holding building `object` of `player`, of master `master` and class `class`, on the
/// map at (26.5, 89.5) with its radii 1.5.
fn holding(mut world: World, player: PlayerId, object: Object) -> World {
    let (id, master) = (object.unit.id, object.unit.life.master_id);
    let class = world
        .master(player, master)
        .map(|building_master| building_master.class)
        .unwrap();
    let _ = world.push_live(player, object).unwrap();
    let placed = *world.spatial.get(TOWN_CENTRE).unwrap();
    world.spatial.insert(
        id,
        Placed {
            position: Position::new(26.5, 89.5, 0.0),
            radius: Footprint::new(1.5, 1.5),
            class,
            player,
            master,
            ..placed
        },
    );
    world
}

/// Its player's farm in state 2 (class 49) the Gather takes: the target, walking there with its
/// place as the point; the bush is let go and the farm taken. The work done (`+0x80`) clears.
#[test]
fn gather_takes_its_player_farm_and_walks_there() {
    let mut world = gathering();
    assert_eq!(
        world.retarget_gather(PLAYER, SLOT, Some(FARM), POINT),
        Ok(Answer::Taken)
    );
    let gather_action = gather_of(&world).unwrap();
    assert_eq!(
        (
            gather_action.target,
            gather_action.state.bare(),
            gather_action.point,
            gather_action.worked
        ),
        (
            Some(GatherTargetId::new(FARM)),
            GatherState::Walk { move_to: None },
            Position::new(26.5, 89.5, 0.0),
            false
        )
    );
    assert_eq!(
        (workers(&world, BUSH), workers(&world, FARM)),
        (WorkerCount::of_data(0), WorkerCount::of_data(1))
    );
}

/// Without a target (`0x140a90440`) the Gather walks to the point it is given: no target (the
/// bush let go), `set_state(15)` and the point; 1. The work done (`+0x80`) clears.
#[test]
fn gather_without_target_walks_to_the_point() {
    let mut world = gathering();
    assert_eq!(
        world.retarget_gather(PLAYER, SLOT, None, POINT),
        Ok(Answer::Taken)
    );
    let gather_action = gather_of(&world).unwrap();
    assert_eq!(
        (
            gather_action.target,
            gather_action.carry_to,
            gather_action.state.bare(),
            gather_action.point,
            gather_action.worked
        ),
        (
            None,
            None,
            GatherState::Moved { move_to: None },
            POINT,
            false
        )
    );
    assert_eq!(workers(&world, BUSH), WorkerCount::of_data(0));
}

/// The test of its present target comes before the building test: gathering the town centre
/// (state 7) the Gather keeps it, though slot 0x48 refuses a building other than a farm; 1, only
/// the work done cleared. In state 3 (not kept) the building test refuses it: 0.
#[test]
fn gather_keeps_its_target_before_the_building_test() {
    let mut world = gathering();
    gather_mut(&mut world).target = Some(GatherTargetId::new(TOWN_CENTRE));
    gather_mut(&mut world).state = GatherState::Work;
    let before = gather_of(&world).unwrap();
    assert_eq!(
        world.retarget_gather(PLAYER, SLOT, Some(TOWN_CENTRE), POINT),
        Ok(Answer::Taken)
    );
    assert_eq!(
        gather_of(&world),
        Some(GatherAction {
            worked: false,
            ..before
        })
    );
    gather_mut(&mut world).state = GatherState::Search;
    assert_eq!(
        world.retarget_gather(PLAYER, SLOT, Some(TOWN_CENTRE), POINT),
        Ok(Answer::NotTaken)
    );
    assert_eq!(
        gather_of(&world),
        Some(GatherAction {
            state: GatherState::Search,
            worked: false,
            ..before
        })
    );
}

/// Its player's foundation (the house 10028, state 0) or pasture (1897 in state 2, of a class
/// slot 0x48 would refuse) the gatherer builds: slot 0x808 with none empties its actions, the
/// Gather too (the bush let go), and slot 0x7e8 works the building at its place; 1. (A
/// villager's work on a pasture reads its player's pasture workers, attribute 290.)
#[test]
fn gather_builds_its_player_foundation_or_pasture() {
    let pasture = object_id(810);
    let mut with_pasture = holding(
        gathering(),
        PLAYER,
        building(pasture, ids::PASTURE, ObjectState::Alive, None),
    );
    with_pasture.players[1].attributes = vec![Amount::ZERO; 300];
    for (world, target) in [(gathering(), HOUSE), (with_pasture, pasture)] {
        let mut world = world;
        assert_eq!(
            world.retarget_gather(PLAYER, SLOT, Some(target), POINT),
            Ok(Answer::Taken),
            "{target:?}"
        );
        assert_eq!(gather_of(&world), None, "{target:?}");
        assert_eq!(workers(&world, BUSH), WorkerCount::of_data(0), "{target:?}");
    }
    // the house: the work command switches the forager to the builder (118) and its Build
    let mut world = gathering();
    let _ = world.retarget_gather(PLAYER, SLOT, Some(HOUSE), POINT);
    assert_eq!(
        (
            world.players[1].objects[SLOT].life.master_id,
            world
                .parts
                .actions_of(FORAGER)
                .list
                .build()
                .and_then(|build_action| build_action.target)
        ),
        (ids::BUILDER, Some(FoundationId::new(HOUSE)))
    );
}

/// Another player's foundation is not built: a house is no farm, so slot 0x48 refuses it (0) and
/// the Gather stays.
#[test]
fn gather_refuses_another_player_foundation() {
    let foundation = object_id(820);
    let mut world = holding(
        gathering(),
        PlayerId::new(0),
        building(foundation, ids::HOUSE, ObjectState::Foundation, None),
    );
    assert_eq!(
        world.retarget_gather(PLAYER, SLOT, Some(foundation), POINT),
        Ok(Answer::NotTaken)
    );
    assert_eq!(
        gather_of(&world).map(|gather_action| gather_action.target),
        Some(Some(GatherTargetId::new(BUSH)))
    );
}

/// An annex is tested by its main building's master (`+0xbb8`): one of class 49 whose main
/// building is the town centre is refused (0); one of class 3 whose main building is the farm is
/// taken, its own class the target type.
#[test]
fn gather_tests_annex_by_its_main_building_class() {
    let (farm_annex, annex) = (object_id(830), object_id(831));
    let mut world = holding(
        holding(
            gathering(),
            PLAYER,
            building(farm_annex, ids::FARM, ObjectState::Alive, Some(TOWN_CENTRE)),
        ),
        PLAYER,
        building(annex, ANNEX_MASTER, ObjectState::Alive, Some(FARM)),
    );
    assert_eq!(
        world.retarget_gather(PLAYER, SLOT, Some(farm_annex), POINT),
        Ok(Answer::NotTaken)
    );
    assert_eq!(
        world.retarget_gather(PLAYER, SLOT, Some(annex), POINT),
        Ok(Answer::Taken)
    );
    let gather_action = gather_of(&world).unwrap();
    assert_eq!(
        (gather_action.target, gather_action.target_class),
        (Some(GatherTargetId::new(annex)), class::BUILDING.or_none())
    );
}

/// An annex is built by its main building (`+0xbb8`): an alive annex of its player's house
/// foundation answers 1, the Gather goes, and slot 0x7e8 works the house at the house's place
/// (30, 88), not the annex at its own (26.5, 89.5): the Build's target and the unit AI's target
/// and point.
#[test]
fn gather_builds_annex_main_foundation_at_its_place() {
    let annex = object_id(832);
    let mut world = holding(
        gathering(),
        PLAYER,
        building(annex, ANNEX_MASTER, ObjectState::Alive, Some(HOUSE)),
    );
    assert_eq!(
        world.retarget_gather(PLAYER, SLOT, Some(annex), POINT),
        Ok(Answer::Taken)
    );
    assert_eq!(gather_of(&world), None);
    let built = world
        .parts
        .actions_of(FORAGER)
        .list
        .build()
        .and_then(|build_action| build_action.target);
    let ai = world.parts.unit_ai.get(FORAGER).unwrap();
    assert_eq!(
        (built, ai.current_target, ai.target_point),
        (
            Some(FoundationId::new(HOUSE)),
            Some(HOUSE),
            Position::new(30.0, 88.0, 0.0)
        )
    );
}

/// The build test reads the main building's state and master, not the annex's: an annex in
/// state 0 and an annex of the pasture master 1897, both of the alive farm, are gathered (the
/// Gather's target), not built.
#[test]
fn gather_tests_main_building_state_and_master_not_annex() {
    for (annex, master, state) in [
        (object_id(833), ANNEX_MASTER, ObjectState::Foundation),
        (object_id(834), ids::PASTURE, ObjectState::Alive),
    ] {
        let mut world = holding(
            gathering(),
            PLAYER,
            building(annex, master, state, Some(FARM)),
        );
        assert_eq!(
            world.retarget_gather(PLAYER, SLOT, Some(annex), POINT),
            Ok(Answer::Taken),
            "{annex:?}"
        );
        assert_eq!(
            gather_of(&world).map(|gather_action| gather_action.target),
            Some(Some(GatherTargetId::new(annex))),
            "{annex:?}"
        );
        assert_eq!(
            world.parts.actions_of(FORAGER).list.build(),
            None,
            "{annex:?}"
        );
    }
}

/// The build test compares the main building's player (`+0x138`), not the annex's: an annex of
/// the gatherer's player on another player's house foundation is not built but refused, as a
/// house is no farm (0), and the Gather stays on the bush.
#[test]
fn gather_compares_main_building_player() {
    let (foundation, annex) = (object_id(820), object_id(835));
    let mut world = holding(
        holding(
            gathering(),
            PlayerId::new(0),
            building(foundation, ids::HOUSE, ObjectState::Foundation, None),
        ),
        PLAYER,
        building(annex, ANNEX_MASTER, ObjectState::Alive, Some(foundation)),
    );
    assert_eq!(
        world.retarget_gather(PLAYER, SLOT, Some(annex), POINT),
        Ok(Answer::NotTaken)
    );
    assert_eq!(
        gather_of(&world).map(|gather_action| gather_action.target),
        Some(Some(GatherTargetId::new(BUSH)))
    );
}

/// The drop test (`FUN_14109d4f0`) reads an annex's main building's master: a loaded forager
/// whose master drops at farms carries its load to an annex of the farm (`set_state(8)`, the
/// annex its second target). With object flag 0x20 the annex is tested as itself: its own master
/// is no drop site, so the forager walks to it.
#[test]
fn gather_drop_test_reads_main_building_master() {
    let annex = object_id(831);
    let loaded = |flags| {
        let mut world = holding(
            gathering(),
            PLAYER,
            building(annex, ANNEX_MASTER, ObjectState::Alive, Some(FARM)),
        );
        for masters in &mut world.model.masters {
            let mut table = (**masters).clone();
            table
                .get_mut(master_id(120))
                .and_then(|forager| forager.action.as_mut())
                .unwrap()
                .drop_sites
                .push(ids::FARM);
            table.get_mut(ANNEX_MASTER).unwrap().object_flags = flags;
            *masters = Arc::new(table);
        }
        let forager = &mut world.players[1].objects[SLOT];
        (forager.life.attribute_kind, forager.life.amount) =
            (Some(AttributeId::named(17)), Amount::new(6.0));
        world
    };
    let mut world = loaded(ObjectFlags::NONE);
    assert_eq!(
        world.retarget_gather(PLAYER, SLOT, Some(annex), POINT),
        Ok(Answer::Taken)
    );
    let gather_action = gather_of(&world).unwrap();
    assert_eq!(
        (
            gather_action.target,
            gather_action.carry_to,
            gather_action.state.bare()
        ),
        (
            None,
            Some(DropSiteId::new(annex)),
            GatherState::Drop { move_to: None }
        )
    );
    let mut world = loaded(ObjectFlags::GATHERED_BUILDING);
    assert_eq!(
        world.retarget_gather(PLAYER, SLOT, Some(annex), POINT),
        Ok(Answer::Taken)
    );
    let gather_action = gather_of(&world).unwrap();
    assert_eq!(
        (
            gather_action.target,
            gather_action.carry_to,
            gather_action.state.bare()
        ),
        (
            Some(GatherTargetId::new(annex)),
            None,
            GatherState::Walk { move_to: None }
        )
    );
}

/// A building the load could not read whole may be under its Unpack: slot 0x48 stops with the
/// import's error. An annex whose main building is gone and a target the world does not hold are
/// errors too.
#[test]
fn gather_retarget_errors_on_unread_building_gone_main_or_unheld_target() {
    let why = GenieError::unsupported("a pack");
    let mut world = gathering();
    world.set_unread(FARM, why.clone());
    assert_eq!(
        world.retarget_gather(PLAYER, SLOT, Some(FARM), POINT),
        Err(why)
    );
    let orphan = object_id(840);
    let mut world = holding(
        gathering(),
        PLAYER,
        building(orphan, ids::FARM, ObjectState::Alive, Some(object_id(4444))),
    );
    assert_eq!(
        world
            .retarget_gather(PLAYER, SLOT, Some(orphan), POINT)
            .map_err(|error| error.reason()),
        Err(MAIN_BUILDING_GONE.reason())
    );
    assert_eq!(
        world
            .retarget_gather(PLAYER, SLOT, Some(object_id(4444)), POINT)
            .map_err(|error| error.reason()),
        Err(GATHER_TARGET_NOT_HELD.reason())
    );
}

/// The work command's retake without a target (a task for a terrain): the Gather walks to the
/// work's point and its unit AI hears the gather order and action on none at that point. On its
/// player's foundation the gatherer builds it, and the unit AI hears the order of the action on
/// top now, on the foundation at the work's point.
#[test]
fn work_retake_tells_unit_ai_of_action_on_top() {
    let mut world = gathering();
    assert_eq!(
        world.retarget_gather_work(PLAYER, SLOT, None, POINT, Notifies::Cleared),
        Ok(Answer::Taken)
    );
    let told = |world: &World| {
        let ai = world.parts.unit_ai.get(FORAGER).unwrap();
        (
            ai.current_order,
            ai.current_action,
            ai.current_target,
            ai.target_point,
        )
    };
    assert_eq!(
        told(&world),
        (Some(OrderKind::Gather), Some(AiAction::Gather), None, POINT)
    );
    let mut world = gathering();
    assert_eq!(
        world.retarget_gather_work(PLAYER, SLOT, Some(HOUSE), POINT, Notifies::Cleared),
        Ok(Answer::Taken)
    );
    let head = world
        .parts
        .actions_of(FORAGER)
        .list
        .head()
        .map(Action::kind);
    assert_eq!(head, Some(ActionKind::BUILD));
    assert_eq!(
        told(&world),
        (
            Some(OrderKind::Build),
            Some(AiAction::Build),
            Some(HOUSE),
            POINT
        )
    );
}
