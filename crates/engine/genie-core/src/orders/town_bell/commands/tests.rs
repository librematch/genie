//! The town bell's commands: its ring, its end and the back-to-work command.

use super::*;
use crate::garrison::tests::{
    ARCHER, TC, VILLAGER, building_at, inside_tc, let_out_world, sized_world, tracked, walker,
};
use crate::map::{Point, Position};
use crate::objects::masters::{BuildingMaster, Master, MasterId, Masters};
use crate::orders::town_bell::BackToWork;
use crate::orders::town_bell::tests::{bell, set_bell};
use crate::test_ids::object_id;
use crate::test_ids::player_id;
use crate::testing::class;
use std::sync::Arc;

/// The building villager `id`'s Enter takes it to, if it has one.
fn enter(world: &World, id: i32) -> Option<ObjectId> {
    world
        .parts
        .actions_of(object_id(id))
        .list
        .enter()
        .and_then(|enter_action| enter_action.target)
        .map(ContainerId::id)
}

#[test]
fn the_bell_sends_the_villagers_in_reach_to_the_buildings_in_reach_and_flags_them() {
    // FUN_14163a650: villager 1000 is within 25 tiles of town centre 100, 1001 is not,
    // archer 1002 is no villager; the centre takes villagers and is in reach of itself
    let mut world = sized_world(60, 60);
    walker(
        &mut world,
        PlayerId::new(1),
        1000,
        VILLAGER,
        Point::new(5.5, 5.5),
    );
    walker(
        &mut world,
        PlayerId::new(1),
        1001,
        VILLAGER,
        Point::new(45.5, 45.5),
    );
    walker(
        &mut world,
        PlayerId::new(1),
        1002,
        ARCHER,
        Point::new(6.5, 5.5),
    );
    world
        .town_bell(player_id(1), object_id(100), Bell::Ring)
        .unwrap();
    assert_eq!(
        [1000, 1001, 1002].map(|id| enter(&world, id)),
        [Some(object_id(100)), None, None]
    );
    assert!(bell(&world, 100).flag.is_sheltered());
}

#[test]
fn the_bell_is_its_owners_or_any_players_with_allied_control() {
    let mut world = let_out_world();
    walker(
        &mut world,
        PlayerId::new(1),
        1000,
        VILLAGER,
        Point::new(5.5, 5.5),
    );
    // player 2 rings player 1's centre: refused
    world
        .town_bell(player_id(2), object_id(100), Bell::Ring)
        .unwrap();
    assert_eq!(enter(&world, 1000), None);
    world.settings.flags.set(OptionFlags::CHEATS_ENABLED, true);
    world
        .town_bell(player_id(2), object_id(100), Bell::Ring)
        .unwrap();
    assert_eq!(enter(&world, 1000), Some(object_id(100)));
    // an object that is no town centre rings nothing; nor does an id the world lacks
    let mut world = let_out_world();
    walker(
        &mut world,
        PlayerId::new(1),
        1000,
        VILLAGER,
        Point::new(5.5, 5.5),
    );
    world
        .town_bell(player_id(1), object_id(1000), Bell::Ring)
        .unwrap();
    world
        .town_bell(player_id(1), object_id(4242), Bell::Ring)
        .unwrap();
    assert_eq!(enter(&world, 1000), None);
}

#[test]
fn the_bells_end_lets_the_villagers_out_and_sends_the_others_back_by_the_tracker() {
    // FUN_14163ab80: the centre drops its flag and lets villager 1000 out toward its
    // record; villager 1001 outside, filed under class 4, walks back to its record's
    // point (slot 0x340); 1002 has no flag and stays; 1003 is not filed and stays
    let mut game = let_out_world();
    set_bell(&mut game, 100, |town_bell| {
        town_bell.flag = BellShelter::Sheltered;
    });
    inside_tc(&mut game, PlayerId::new(1), 1000, VILLAGER);
    walker(
        &mut game,
        PlayerId::new(1),
        1001,
        VILLAGER,
        Point::new(5.5, 5.5),
    );
    walker(
        &mut game,
        PlayerId::new(1),
        1002,
        VILLAGER,
        Point::new(6.5, 5.5),
    );
    walker(
        &mut game,
        PlayerId::new(1),
        1003,
        VILLAGER,
        Point::new(7.5, 5.5),
    );
    for (id, point) in [
        (1000, Point::new(4.5, 4.5)),
        (1001, Point::new(3.5, 3.5)),
        (1002, Point::new(2.5, 2.5)),
    ] {
        set_bell(&mut game, id, |town_bell| {
            town_bell.flag = if id == 1002 {
                BellShelter::Free
            } else {
                BellShelter::Sheltered
            };
            town_bell.work = BackToWork {
                point,
                ..BackToWork::NONE
            };
        });
    }
    set_bell(&mut game, 1003, |town_bell| {
        town_bell.flag = BellShelter::Sheltered;
    });
    set_bell(&mut game, 1003, |town_bell| {
        town_bell.work.point = Point::new(1.5, 1.5);
    });
    tracked(&mut game, VILLAGER, &[1000, 1001, 1002]);
    game.town_bell(player_id(1), object_id(100), Bell::End)
        .unwrap();
    assert!(!bell(&game, 100).flag.is_sheltered());
    let object = game.object(object_id(1000)).unwrap().1;
    assert_eq!(
        (
            object.life.inside,
            game.parts.town_bell_of(object.id).flag.is_sheltered()
        ),
        (None, false)
    );
    let goal = |world: &World, id: i32| {
        let unit = world.object(object_id(id)).unwrap().1;
        (
            world
                .parts
                .actions_of(object_id(id))
                .list
                .move_to()
                .copied()
                .and_then(|move_to_action| move_to_action.goal()),
            world.parts.town_bell_of(unit.id).flag.is_sheltered(),
        )
    };
    assert_eq!(
        goal(&game, 1001),
        (Some(Position::new(3.5, 3.5, 0.0)), false)
    );
    assert_eq!(goal(&game, 1002), (None, false));
    assert_eq!(goal(&game, 1003), (None, true));
    // a record without a point on an axis (an exact -1) keeps the unit and its flag
    let mut world = let_out_world();
    walker(
        &mut world,
        PlayerId::new(1),
        1001,
        VILLAGER,
        Point::new(5.5, 5.5),
    );
    set_bell(&mut world, 1001, |town_bell| {
        town_bell.flag = BellShelter::Sheltered;
        town_bell.work.point = Point::new(3.5, -1.0);
    });
    tracked(&mut world, VILLAGER, &[1001]);
    world
        .town_bell(player_id(1), object_id(100), Bell::End)
        .unwrap();
    assert_eq!(goal(&world, 1001), (None, true));
}
#[test]
fn all_back_to_work_lets_the_villagers_out_and_sends_the_others_back_leaving_the_flag() {
    // ALL_BACK_TO_WORK (do_command 0x8c, FUN_141617420): as the bell's end, but with no
    // bell flag to test or drop: villager 1000 comes out of the centre toward its
    // record, 1001 outside, filed under class 4, walks back to its record's point
    let world = || {
        let mut let_out_world = let_out_world();
        set_bell(&mut let_out_world, 100, |town_bell| {
            town_bell.flag = BellShelter::Sheltered;
        });
        inside_tc(&mut let_out_world, PlayerId::new(1), 1000, VILLAGER);
        walker(
            &mut let_out_world,
            PlayerId::new(1),
            1001,
            VILLAGER,
            Point::new(5.5, 5.5),
        );
        for (id, point) in [(1000, Point::new(4.5, 4.5)), (1001, Point::new(3.5, 3.5))] {
            set_bell(&mut let_out_world, id, |town_bell| {
                town_bell.flag = BellShelter::Sheltered;
                town_bell.work = BackToWork {
                    point,
                    ..BackToWork::NONE
                };
            });
        }
        tracked(&mut let_out_world, VILLAGER, &[1000, 1001]);
        let_out_world
    };
    let mut game = world();
    game.all_back_to_work(player_id(1), object_id(100)).unwrap();
    assert!(bell(&game, 100).flag.is_sheltered());
    let object = game.object(object_id(1000)).unwrap().1;
    assert_eq!(
        (
            object.life.inside,
            game.parts.town_bell_of(object.id).flag.is_sheltered()
        ),
        (None, false)
    );
    let unit = game.object(object_id(1001)).unwrap().1;
    assert_eq!(
        (
            game.parts
                .actions_of(object_id(1001))
                .list
                .move_to()
                .copied()
                .and_then(|move_to_action| move_to_action.goal()),
            game.parts.town_bell_of(unit.id).flag.is_sheltered()
        ),
        (Some(Position::new(3.5, 3.5, 0.0)), false)
    );
    // another player's command does nothing without the game's cheats
    let mut world = world();
    world
        .all_back_to_work(player_id(2), object_id(100))
        .unwrap();
    assert_eq!(
        world.object(object_id(1000)).unwrap().1.life.inside,
        Some(ContainerId::new(object_id(100)))
    );
}

/// The mule cart's master (1808), which answers the bell besides the villagers.
const MULE_CART: MasterId = MasterId::named(1808);
/// A villager below the combat type (type 30), which the bell passes over.
const VILLAGER_WALKER: MasterId = MasterId::named(97);
/// A house: a building that takes no units in.
const HOUSE: MasterId = MasterId::named(70);

/// [`sized_world`] of `width` × `height` tiles whose tables also hold the mule cart (a villager's
/// copy of class 0), [`VILLAGER_WALKER`] (one of type 30) and [`HOUSE`].
fn bell_world(width: i32, height: i32) -> World {
    let mut world = sized_world(width, height);
    for masters in &mut world.model.masters {
        let villager = masters.get(VILLAGER).unwrap().clone();
        let mule_cart = Master {
            id: MULE_CART,
            class: class::CLASS_0,
            ..villager.clone()
        };
        let walker = Master {
            id: VILLAGER_WALKER,
            kind: MasterKind::MOVING,
            ..villager
        };
        let house = Master {
            id: HOUSE,
            kind: MasterKind::BUILDING,
            class: class::BUILDING,
            building: Some(BuildingMaster::default()),
            ..Master::default()
        };
        *masters = Arc::new(Masters::new(
            masters.iter().cloned().chain([mule_cart, walker, house]),
        ));
    }
    world
}

/// Unit `id` of player 1 and master `master` inside building `building` at its place (its
/// garrison's last), its town-bell record's point `point`, flagged by the bell.
fn sheltered(world: &mut World, building: i32, id: i32, master: MasterId, point: Point) {
    let at = world.object(object_id(building)).unwrap().1.position();
    walker(world, PlayerId::new(1), id, master, at.ground());
    let (_, unit) = world.object_mut(object_id(id)).unwrap();
    unit.life.inside = Some(ContainerId::new(object_id(building)));
    world.spatial.go_inside(object_id(id), at);
    world
        .object_mut(object_id(building))
        .unwrap()
        .1
        .garrison
        .push(object_id(id));
    set_bell(world, id, |town_bell| {
        town_bell.flag = BellShelter::Sheltered;
        town_bell.work.point = point;
    });
}

/// The building unit `id` is inside.
fn inside(world: &World, id: i32) -> Option<ObjectId> {
    world
        .object(object_id(id))
        .unwrap()
        .1
        .life
        .inside
        .map(ContainerId::id)
}

/// The point unit `id`'s `MoveTo` walks to.
fn goal(world: &World, id: i32) -> Option<Position> {
    world
        .parts
        .actions_of(object_id(id))
        .list
        .move_to()
        .copied()
        .and_then(|move_to_action| move_to_action.goal())
}

/// Whether building `id` holds the bell's flag (`+0x9dc`).
fn flagged(world: &World, id: i32) -> bool {
    bell(world, id).flag.is_sheltered()
}

/// `FUN_14163a650` sends a mule cart (master 1808, `0x710`) within the reach to the buildings
/// whatever its class, as it does a villager; one beyond the reach stays.
#[test]
fn bell_sends_mule_carts_in_reach() {
    let mut world = bell_world(60, 60);
    walker(
        &mut world,
        PlayerId::new(1),
        1000,
        MULE_CART,
        Point::new(5.5, 5.5),
    );
    walker(
        &mut world,
        PlayerId::new(1),
        1001,
        MULE_CART,
        Point::new(45.5, 45.5),
    );
    world
        .town_bell(player_id(1), object_id(100), Bell::Ring)
        .unwrap();
    assert_eq!(
        [1000, 1001].map(|id| enter(&world, id)),
        [Some(object_id(100)), None]
    );
}

/// `FUN_14163a650` sends only the villagers in state 2 outside every other object whose type is
/// 70 (`'F'`): a dying villager, a villager of type 30 and one inside the town centre stay,
/// while a live one beside them goes.
#[test]
fn bell_passes_over_dying_inside_and_below_combat_kind_villagers() {
    let mut world = bell_world(60, 60);
    for (id, master, x) in [
        (1000, VILLAGER, 5.5),
        (1001, VILLAGER_WALKER, 6.5),
        (1002, VILLAGER, 7.5),
    ] {
        walker(&mut world, PlayerId::new(1), id, master, Point::new(x, 5.5));
    }
    world.object_mut(object_id(1000)).unwrap().1.life.state = ObjectState::Dying;
    inside_tc(&mut world, PlayerId::new(1), 1003, VILLAGER);
    world
        .town_bell(player_id(1), object_id(100), Bell::Ring)
        .unwrap();
    assert_eq!(
        [1000, 1001, 1002, 1003].map(|id| enter(&world, id)),
        [None, None, Some(object_id(100)), None]
    );
}

/// `FUN_14163a650` flags the buildings in state 2 within the reach that take villagers
/// (`FUN_1411ccb60` with 1): a second town centre in reach takes the flag; one beyond the reach,
/// a dying one and a house in reach, which takes no units, do not.
#[test]
fn bell_flags_live_buildings_in_reach_that_take_villagers() {
    let mut world = bell_world(80, 80);
    building_at(&mut world, 101, TC, Point::new(16.0, 10.0), None);
    building_at(&mut world, 102, TC, Point::new(60.0, 60.0), None);
    building_at(&mut world, 103, TC, Point::new(10.0, 16.0), None);
    building_at(&mut world, 104, HOUSE, Point::new(4.0, 10.0), None);
    world.object_mut(object_id(103)).unwrap().1.life.state = ObjectState::Dying;
    world
        .town_bell(player_id(1), object_id(100), Bell::Ring)
        .unwrap();
    assert_eq!(
        [100, 101, 102, 103, 104].map(|id| flagged(&world, id)),
        [true, true, false, false, false]
    );
}

/// `FUN_14163ab80` drops the flag of each flagged building below state 3 that takes villagers: a
/// foundation's goes; a dying building keeps its own.
#[test]
fn bells_end_drops_flags_of_buildings_below_state_3() {
    let mut world = bell_world(60, 60);
    building_at(&mut world, 101, TC, Point::new(16.0, 10.0), None);
    building_at(&mut world, 102, TC, Point::new(10.0, 16.0), None);
    world.object_mut(object_id(101)).unwrap().1.life.state = ObjectState::Foundation;
    world.object_mut(object_id(102)).unwrap().1.life.state = ObjectState::Dying;
    for id in [100, 101, 102] {
        set_bell(&mut world, id, |town_bell| {
            town_bell.flag = BellShelter::Sheltered;
        });
    }
    world
        .town_bell(player_id(1), object_id(100), Bell::End)
        .unwrap();
    assert_eq!(
        [100, 101, 102].map(|id| flagged(&world, id)),
        [false, false, true]
    );
}

/// `FUN_14163ab80` drops a flagged building's flag beyond the reach too, but lets out only the
/// units of one within it (`fVar13 <= 625.0`).
#[test]
fn bells_end_beyond_reach_drops_flag_and_keeps_units_inside() {
    let mut world = bell_world(80, 80);
    building_at(&mut world, 101, TC, Point::new(60.0, 60.0), None);
    set_bell(&mut world, 101, |town_bell| {
        town_bell.flag = BellShelter::Sheltered;
    });
    sheltered(&mut world, 101, 1000, VILLAGER, Point::new(55.5, 55.5));
    world
        .town_bell(player_id(1), object_id(100), Bell::End)
        .unwrap();
    assert!(!flagged(&world, 101));
    assert_eq!(inside(&world, 1000), Some(object_id(101)));
}

/// `FUN_14163ab80` lets out of a flagged building in reach the units of type 70 and up
/// (`0x45 < type`) that are villagers (class 4) or mule carts (1808): an archer and a villager of
/// type 30 stay inside.
#[test]
fn bells_end_lets_out_villagers_and_mule_carts_of_combat_kind_only() {
    let mut world = bell_world(40, 40);
    set_bell(&mut world, 100, |town_bell| {
        town_bell.flag = BellShelter::Sheltered;
    });
    for (id, master) in [
        (1000, VILLAGER),
        (1001, ARCHER),
        (1002, MULE_CART),
        (1003, VILLAGER_WALKER),
    ] {
        sheltered(&mut world, 100, id, master, Point::new(4.5, 4.5));
    }
    world
        .town_bell(player_id(1), object_id(100), Bell::End)
        .unwrap();
    assert_eq!(
        [1000, 1001, 1002, 1003].map(|id| inside(&world, id)),
        [None, Some(object_id(100)), None, Some(object_id(100))]
    );
}

/// The bell's end sends back the units its owner's tracker files under master 1808
/// (`FUN_14104f960`) as it does those under class 4.
#[test]
fn bells_end_sends_tracked_mule_carts_back() {
    let mut world = bell_world(40, 40);
    walker(
        &mut world,
        PlayerId::new(1),
        1001,
        MULE_CART,
        Point::new(5.5, 5.5),
    );
    set_bell(&mut world, 1001, |town_bell| {
        town_bell.flag = BellShelter::Sheltered;
        town_bell.work.point = Point::new(3.5, 3.5);
    });
    tracked(&mut world, MULE_CART, &[1001]);
    world
        .town_bell(player_id(1), object_id(100), Bell::End)
        .unwrap();
    assert_eq!(goal(&world, 1001), Some(Position::new(3.5, 3.5, 0.0)));
}

/// The bell's end on an object that is no town centre, and on an id the world does not hold,
/// ends nothing: the flags stay and the units stay inside.
#[test]
fn bells_end_of_no_town_centre_ends_nothing() {
    let mut world = bell_world(40, 40);
    set_bell(&mut world, 100, |town_bell| {
        town_bell.flag = BellShelter::Sheltered;
    });
    sheltered(&mut world, 100, 1000, VILLAGER, Point::new(4.5, 4.5));
    for id in [1000, 4242] {
        world
            .town_bell(player_id(1), object_id(id), Bell::End)
            .unwrap();
    }
    assert!(flagged(&world, 100));
    assert_eq!(inside(&world, 1000), Some(object_id(100)));
}

/// `ALL_BACK_TO_WORK` (`FUN_141617420`) measures the reach from the commanded object, whatever it
/// is: from a villager beyond the reach of the town centre the units inside stay, while the units
/// its owner's tracker files outside go back. An id the world does not hold does nothing.
#[test]
fn all_back_to_work_measures_reach_from_commanded_object() {
    let mut world = bell_world(80, 80);
    sheltered(&mut world, 100, 1000, VILLAGER, Point::new(4.5, 4.5));
    walker(
        &mut world,
        PlayerId::new(1),
        1001,
        VILLAGER,
        Point::new(60.5, 60.5),
    );
    set_bell(&mut world, 1001, |town_bell| {
        town_bell.flag = BellShelter::Sheltered;
        town_bell.work.point = Point::new(58.5, 58.5);
    });
    tracked(&mut world, VILLAGER, &[1000, 1001]);
    world
        .all_back_to_work(player_id(1), object_id(4242))
        .unwrap();
    assert_eq!(goal(&world, 1001), None);
    world
        .all_back_to_work(player_id(1), object_id(1001))
        .unwrap();
    assert_eq!(inside(&world, 1000), Some(object_id(100)));
    assert_eq!(goal(&world, 1001), Some(Position::new(58.5, 58.5, 0.0)));
}

/// `ALL_BACK_TO_WORK` lets the units out of buildings in state 2 only: a dying town centre keeps
/// its villager inside.
#[test]
fn all_back_to_work_passes_over_buildings_not_alive() {
    let mut world = bell_world(40, 40);
    sheltered(&mut world, 100, 1000, VILLAGER, Point::new(4.5, 4.5));
    world.object_mut(object_id(100)).unwrap().1.life.state = ObjectState::Dying;
    world
        .all_back_to_work(player_id(1), object_id(100))
        .unwrap();
    assert_eq!(inside(&world, 1000), Some(object_id(100)));
}
