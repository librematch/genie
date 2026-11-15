//! `UNIT_ORDER`'s Housing and the work command's on the house of `construction::housing`'s tests,
//! and an action object's Housing task on `world::tests`'s mule cart.

use super::*;
use crate::commands::{Shift, UnitList, UnitOrderAction, UnitOrderCommand};
use crate::construction::housing::tests::{
    AT, HOUSE_ID, ORDER, held, holding, house_master, house_with, house_world, housing, wood,
};
use crate::construction::housing::{HousingAction, HousingState};
use crate::garrison::GarrisonScope;
use crate::map::Point;
use crate::map::Position;
use crate::map::spatial::Placed;
use crate::objects::actions::ActionList;
use crate::objects::lifecycle::Lifecycle;
use crate::objects::masters::Master;
use crate::objects::masters::Masters;
use crate::objects::masters::TargetOwner;
use crate::objects::masters::Task;
use crate::objects::unit::Unit;
use crate::orders::MasterSwap;
use crate::orders::Work;
use crate::orders::work::WORK_WITHOUT_UNIT_AI;
use crate::players::Amount;
use crate::players::PlayerKind;
use crate::production::UnitIndex;
use crate::production::{Entry, MakeObj, MakeTech, MakeTechState, Making, Production, QueueLength};
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::test_ids::object_ids;
use crate::test_ids::tech_id;
use crate::testing::class;
use crate::testing::ids;
use crate::testing::slots;
use crate::unit_ai::AiAction;
use crate::unit_ai::Answer;
use crate::unit_ai::Notifies;
use crate::unit_ai::OrderKind;
use crate::unit_ai::UnitAi;
use crate::world::Seconds;
use crate::world::tests::cart_world;
use std::iter::once;
use std::sync::Arc;

/// The order to the house alone.
fn order(world: &mut World) -> GenieResult<()> {
    world.housing_order(&object_ids([700]), ORDER)
}

/// The Housing on top of the house in its state and timer, if any.
fn working(world: &World) -> Option<(HousingState, f32)> {
    held(world).map(|housing_action| (housing_action.state, housing_action.timer.get()))
}

/// The house with the production part `production`, its owner of kind `kind`, holding no
/// headroom but 100 of nothing.
fn producing(kind: PlayerKind, production: Production) -> World {
    let mut world = house_world(house_master(), (None, 100.0), ActionList::default(), 120.0);
    world.players[1].kind = kind;
    world.parts.production = once((HOUSE_ID, production)).collect();
    world
}

/// A production whose queue holds `count` entries and that makes `making`.
fn production(count: i16, making: Option<Making>) -> Production {
    let mut production = Production {
        action: making,
        ..Production::default()
    };
    production.queue.count = QueueLength::of_data(count);
    production
}

/// A research in progress.
fn research() -> Making {
    Making::Research(MakeTech {
        state: MakeTechState::Researching,
        paid: false,
        tech: tech_id(22),
        master: -1,
        progress: Seconds::ZERO,
    })
}

/// The villager master 83 (type 70, class 4): below the buildings' type.
fn villager() -> Master {
    Master {
        id: ids::VILLAGER_MALE,
        kind: MasterKind::COMBAT,
        class: class::VILLAGER,
        ..Master::default()
    }
}

/// The new Housing pays 50 of the owner's 120 wood and goes to work on attribute 62, with the
/// order's queue id.
#[test]
fn housing_order_installs_paid_housing() {
    let mut world = house_with(ActionList::default(), 120.0);
    order(&mut world).unwrap();
    assert_eq!(
        (held(&world), wood(&world)),
        (Some(housing(HousingState::Working, 20.0)), 70.0)
    );
}

/// Without the wood the new Housing fails at its start: it stays on top in state 13, and the
/// wood stays.
#[test]
fn housing_order_without_wood_leaves_failed_housing() {
    let mut world = house_with(ActionList::default(), 10.0);
    order(&mut world).unwrap();
    assert_eq!(
        (working(&world), wood(&world)),
        (Some((HousingState::Failed, 0.0)), 10.0)
    );
}

/// The guard: a building takes the order below 45 (`COMISS`, `JBE`: a NaN does not), at work
/// or not; at 45 or more it takes it only holding no headroom and not at work.
#[test]
fn housing_order_takes_buildings_below_full_or_without_headroom() {
    let headroom = Some(slots().attribute.headroom);
    let wood_kind = Some(slots().attribute.wood);
    let at_work = || holding(housing(HousingState::Working, 4.0));
    for (holds, list, takes) in [
        ((headroom, 44.9), ActionList::default(), true),
        ((headroom, 45.0), ActionList::default(), false),
        ((headroom, f32::NAN), ActionList::default(), false),
        ((headroom, 50.0), at_work(), false),
        ((headroom, 5.0), at_work(), true),
        ((wood_kind, 100.0), ActionList::default(), true),
        ((None, 100.0), ActionList::default(), true),
        ((None, 100.0), at_work(), false),
        ((None, 0.0), at_work(), true),
    ] {
        let mut world = house_world(house_master(), holds, list, 120.0);
        order(&mut world).unwrap();
        assert_eq!(
            working(&world) == Some((HousingState::Working, 20.0)),
            takes,
            "{holds:?}"
        );
    }
}

/// A Housing at work that the order replaces gives its cost back before the new one pays: the
/// wood comes out even.
#[test]
fn housing_order_again_refunds_the_old_housing() {
    let mut world = house_with(holding(housing(HousingState::Working, 4.0)), 120.0);
    order(&mut world).unwrap();
    assert_eq!(
        (held(&world), wood(&world)),
        (Some(housing(HousingState::Working, 20.0)), 120.0)
    );
}

/// The types that keep a building from the order, the order's own compares (`0x14161ea3e`): the
/// Make (0x66), the research (0x67), the Housing (0x7b), the Pack (0x7c) and the Unpack
/// (0x7d), which genie's list does not hold yet (the task pack-unit-order).
#[test]
fn order_busy_kinds_are_the_order_compares() {
    assert_eq!(
        ORDER_BUSY.map(ActionKind::get),
        [0x66, 0x67, 0x7b, 0x7c, 0x7d]
    );
}

/// The work a building's owner reads: a computer's production first (its make 0x66, its research
/// 0x67, a queue that holds an entry 0x66), else the top action's type; a human's production
/// counts for nothing.
#[test]
fn building_work_kind_reads_computer_production_first() {
    let make = Making::Unit(MakeObj::for_entry(&Entry::default()));
    for (kind, production, work) in [
        (
            PlayerKind::Computer,
            production(0, Some(make)),
            Some(ActionKind::MAKE_OBJECT),
        ),
        (
            PlayerKind::Computer,
            production(0, Some(research())),
            Some(ActionKind::MAKE_TECH),
        ),
        (
            PlayerKind::Computer,
            production(1, None),
            Some(ActionKind::MAKE_OBJECT),
        ),
        (PlayerKind::Computer, production(0, None), None),
        (PlayerKind::Human, production(1, Some(research())), None),
    ] {
        let world = producing(kind, production);
        assert_eq!(
            world.building_work_kind(AT.0, AT.1).unwrap(),
            work,
            "{kind:?} {work:?}"
        );
    }
    let at_work = house_with(holding(housing(HousingState::Working, 4.0)), 120.0);
    assert_eq!(
        at_work.building_work_kind(AT.0, AT.1).unwrap(),
        Some(ActionKind::HOUSING)
    );
}

/// A computer's building whose queue holds an entry is busy and holds 45 or more of nothing: it
/// takes no Housing; a human's takes it.
#[test]
fn computer_building_with_queue_entry_takes_no_housing() {
    for (kind, takes) in [(PlayerKind::Computer, false), (PlayerKind::Human, true)] {
        let mut world = producing(kind, production(1, None));
        order(&mut world).unwrap();
        assert_eq!(held(&world).is_some(), takes, "{kind:?}");
    }
}

/// A building the load could not read whole stops at the work's read.
#[test]
fn housing_order_of_unread_building_fails() {
    let mut world = house_world(house_master(), (None, 100.0), ActionList::default(), 120.0);
    world.parts.unread =
        once((HOUSE_ID, Unread::new(GenieError::invalid_input("unread")))).collect();
    assert_eq!(order(&mut world).unwrap_err().reason(), "unread");
}

/// An object below the buildings' type holding less than 45 faults in the game; one holding 45
/// or more takes nothing.
#[test]
fn housing_order_of_no_building_faults_below_full() {
    let mut world = house_world(villager(), (None, 0.0), ActionList::default(), 120.0);
    assert_eq!(
        order(&mut world).unwrap_err().reason(),
        HOUSING_WITHOUT_BUILDING.reason()
    );
    let mut full = house_world(villager(), (None, 45.0), ActionList::default(), 120.0);
    order(&mut full).unwrap();
    assert_eq!(held(&full), None);
}

/// An object not in state 2 (a foundation, a dying building) and an id the world lacks take
/// nothing; a sleeping object is a gap.
#[test]
fn housing_order_skips_objects_not_alive() {
    for state in [ObjectState::Foundation, ObjectState::Dying] {
        let mut world = house_with(ActionList::default(), 120.0);
        world.players[1].objects[AT.1].life.state = state;
        order(&mut world).unwrap();
        assert_eq!((held(&world), wood(&world)), (None, 120.0), "{state:?}");
    }
    let mut lacking = house_with(ActionList::default(), 120.0);
    lacking.housing_order(&object_ids([701]), ORDER).unwrap();
    assert_eq!(held(&lacking), None);
    let mut asleep = house_with(ActionList::default(), 120.0);
    let house = asleep.players[1].objects.swap_remove(AT.1).unwrap();
    asleep.players[1].sleeping.push(house);
    asleep.reindex();
    assert_eq!(order(&mut asleep), Err(ORDER_ASLEEP));
}

/// The command's run routes case 3 to the Housing; a byte the switch has no case for does
/// nothing.
#[test]
fn unit_order_runs_housing_and_ignores_other_bytes() {
    let command = |action| UnitOrderCommand {
        player: PlayerId::new(1),
        units: UnitList {
            count: 1,
            ids: object_ids([700]),
        },
        target: None,
        point: Point::default(),
        queue_id: ORDER,
        shift: Shift::default(),
        scope: GarrisonScope::default(),
        action,
        param: UnitIndex::default(),
        human: false,
    };
    let mut world = house_with(ActionList::default(), 120.0);
    command(UnitOrderAction::Housing).run(&mut world).unwrap();
    assert_eq!(held(&world), Some(housing(HousingState::Working, 20.0)));
    let mut other = house_with(ActionList::default(), 120.0);
    command(UnitOrderAction::Other(9)).run(&mut other).unwrap();
    assert_eq!((held(&other), wood(&other)), (None, 120.0));
}

/// The mule cart's list index in [`cart_world`]: after the forager, the house's foundation and
/// the town centre.
const CART: ListIndex = ListIndex::new(3);

/// The mule cart of [`cart_world`].
const CART_ID: ObjectId = object_id(750);

/// The second house of player 1 that the work command names (object 701).
const NAMED: ObjectId = object_id(701);

/// The work command's point.
const POINT: Position = Position::new(12.5, 14.5, 0.0);

/// A stand-in master 273 (an id that build 158041's `.dat` leaves empty in every civilisation)
/// of the house master's kind and class, whose one task is a Housing (type 0x7b) on its own
/// player's objects of the house master 70: no master of the `.dat` holds a task of that type.
fn housing_master() -> Master {
    let mut stand_in = house_master();
    stand_in.id = master_id(273);
    stand_in.action.as_mut().unwrap().tasks = vec![Task {
        action_kind: ActionKind::HOUSING,
        object_id: Some(ids::HOUSE),
        owner_kind: TargetOwner::OWN,
        ..Task::default()
    }];
    stand_in
}

/// The house of the stand-in master [`housing_master`] with the unit AI `ai` (none: without one)
/// and the action list `list`, holding 5 headroom, its owner 120 wood, beside a second house 701
/// of player 1 (master 70).
fn work_world(ai: Option<UnitAi>, list: ActionList) -> World {
    let mut world = house_world(
        housing_master(),
        (Some(slots().attribute.headroom), 5.0),
        list,
        120.0,
    );
    world.players[1].objects.push(Unit {
        id: NAMED,
        life: Lifecycle {
            state: ObjectState::Alive,
            master_id: ids::HOUSE,
            ..Lifecycle::default()
        },
        ..Unit::default()
    });
    world.reindex();
    world.parts.set_unit_ai(HOUSE_ID, ai);
    world
}

/// The house's unit AI on an attack order and action, with no target.
fn attacking() -> UnitAi {
    UnitAi {
        current_order: Some(OrderKind::Attack),
        current_action: Some(AiAction::Attack),
        ..UnitAi::default()
    }
}

/// The stand-in master's Housing task.
fn housing_task() -> Task {
    housing_master().action_part().tasks[0]
}

/// The work command's factory (slot 0x10) on the Housing task, naming `target` at the point.
fn work(world: &mut World, target: Option<ObjectId>) -> GenieResult<()> {
    world.start_task(
        AT.0,
        AT.1,
        (target, POINT),
        &housing_task(),
        Notifies::Cleared,
    )
}

/// The work command (`FUN_14106ff70`) on the second house: the general search finds the house
/// master's Housing task by the target's master (`FUN_1411a8ba0`), nothing on top retakes it, and
/// the factory's Housing (case 0x7b) is installed and paid, its queue id −1.
#[test]
fn work_command_on_house_installs_its_master_housing() {
    let mut world = work_world(Some(attacking()), ActionList::default());
    world
        .work_command(AT.0, AT.1, &Work::on(Some(NAMED), POINT))
        .unwrap();
    assert_eq!(
        (
            held(&world).map(|housing_action| housing_action.unique_id),
            wood(&world)
        ),
        (Some(QueueId::NONE), 70.0)
    );
}

/// The factory's case 0x7b: a new Housing with the queue id −1 (`FUN_140ac1b80` with
/// `0xffffffff`) pays 50 of the owner's 120 wood at its start and goes to work on attribute 62.
#[test]
fn work_command_housing_task_installs_paid_housing_without_queue_id() {
    let mut world = work_world(Some(attacking()), ActionList::default());
    work(&mut world, Some(NAMED)).unwrap();
    assert_eq!(
        (held(&world), wood(&world)),
        (
            Some(HousingAction {
                state: HousingState::Working,
                timer: Seconds::new(20.0),
                unique_id: QueueId::NONE,
            }),
            70.0
        )
    );
}

/// The unit AI hears of the new Housing (slot 0x828 with 0x7b, which `FUN_14106e4e0`'s switch
/// does not name): no order and no action, the old ones its last, and the named house with its
/// class at the work's point; a work on a point names no target.
#[test]
fn work_command_housing_tells_unit_ai_no_order_and_no_action() {
    let told = |target| {
        let mut world = work_world(Some(attacking()), ActionList::default());
        work(&mut world, target).unwrap();
        let ai = world.parts.unit_ai.get(HOUSE_ID).unwrap().clone();
        (
            (ai.current_order, ai.last_order),
            (ai.current_action, ai.last_action),
            (ai.current_target, ai.target_class, ai.target_point),
        )
    };
    let was = (Some(OrderKind::Attack), Some(AiAction::Attack));
    assert_eq!(
        told(Some(NAMED)),
        (
            (None, was.0),
            (None, was.1),
            (Some(NAMED), Some(class::BUILDING), POINT)
        )
    );
    assert_eq!(told(None).2, (None, None, POINT));
}

/// The work command clears its unit AI's `+0x1d5` after the install through a pointer it does
/// not test: an object without a unit AI faults there, its Housing installed and paid.
#[test]
fn work_command_housing_without_unit_ai_faults_after_install() {
    let mut world = work_world(None, ActionList::default());
    assert_eq!(
        work(&mut world, Some(NAMED)).map_err(|error| error.reason()),
        Err(WORK_WITHOUT_UNIT_AI.reason())
    );
    assert_eq!(
        (working(&world), wood(&world)),
        (Some((HousingState::Working, 20.0)), 70.0)
    );
}

/// [`cart_world`]'s moving mule cart 750 under a stand-in master 382 of an action object (type
/// 40; an id that build 158041's `.dat` leaves empty in every civilisation) of the cart master's
/// speed and footprint, whose one task is the stand-in house's Housing task; its owner holds 120
/// wood.
fn action_object_world() -> World {
    let mut world = cart_world();
    world.players[1].attributes[slots().attribute.wood.index()] = Amount::new(120.0);
    let stand_in = master_id(382);
    for masters in &mut world.model.masters {
        let mut action_object = masters.get(ids::MASTER_1808).unwrap().clone();
        action_object.id = stand_in;
        action_object.kind = MasterKind::ACTION;
        action_object.action.as_mut().unwrap().tasks = vec![housing_task()];
        *masters = Arc::new(Masters::new(masters.iter().cloned().chain([action_object])));
    }
    world.players[1].objects[CART].life.master_id = stand_in;
    let placed = *world.spatial.get(CART_ID).unwrap();
    world.spatial.change_master(
        CART_ID,
        Placed {
            master: stand_in,
            ..placed
        },
    );
    world
}

/// An action object's list is an `RGE_Action_List` (slot 0x840, `FUN_14106c380`), whose factory
/// (`FUN_140a97e00`) makes no action of a Housing task: the work command moves the object to the
/// point (slot 0x138) instead, and its owner pays for no Housing; types 50 and 60 move too. An
/// action object holds no unit AI (only a `TRIBE_Combat_Object` and a `TRIBE_Building_Object`
/// save one), so slot 0x828 (`FUN_14106e4e0`, its `+0x6b0` null) tells nothing. A combat
/// object's or a building's `TRIBE_Action_List` (types 70 and 80, slot 0x840 `FUN_141210890`),
/// beside the unit AI it holds, takes a Housing there.
#[test]
fn work_command_housing_task_of_action_object_moves_without_housing() {
    let worked = |kind: MasterKind| {
        let mut world = action_object_world();
        for masters in &mut world.model.masters {
            Arc::make_mut(masters).get_mut(master_id(382)).unwrap().kind = kind;
        }
        // only the TRIBE classes (types 70 and 80) hold a unit AI
        if kind < MasterKind::COMBAT {
            world.parts.set_unit_ai(CART_ID, None);
        }
        let result = world.start_task(
            PlayerId::new(1),
            CART,
            (None, POINT),
            &housing_task(),
            Notifies::Cleared,
        );
        let list = &world.parts.actions_of(CART_ID).list;
        (
            result.map_err(|error| error.reason()),
            (list.housing().is_some(), list.move_to().is_some()),
            world.parts.unit_ai.get(CART_ID).is_some(),
            wood(&world),
        )
    };
    for action_object in [
        MasterKind::ACTION,
        MasterKind::RGE_COMBAT,
        MasterKind::MISSILE,
    ] {
        assert_eq!(worked(action_object), (Ok(()), (false, true), false, 120.0));
    }
    for tribe_object in [MasterKind::COMBAT, MasterKind::BUILDING] {
        assert_eq!(worked(tribe_object), (Ok(()), (true, false), true, 70.0));
    }
}

/// A Housing on top holds no task (`+0x48`, which its constructor leaves null), so the work's
/// retake on a Housing task answers 0 (`FUN_140a98710`'s none); the factory's new Housing then
/// replaces it, the old one's cost given back before the new one pays.
#[test]
fn work_command_on_housing_replaces_the_housing_on_top() {
    let mut world = work_world(
        Some(attacking()),
        holding(housing(HousingState::Working, 4.0)),
    );
    assert_eq!(
        world.retake_task(
            AT.0,
            AT.1,
            (Some(NAMED), POINT, Notifies::Cleared),
            &housing_task(),
            MasterSwap::Keep
        ),
        Ok(Answer::NotTaken)
    );
    work(&mut world, Some(NAMED)).unwrap();
    assert_eq!(
        (working(&world), wood(&world)),
        (Some((HousingState::Working, 20.0)), 120.0)
    );
}
