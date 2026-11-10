use super::*;
#[cfg(test)]
use crate::garrison::transport::TRANSPORT;
#[cfg(test)]
use crate::gathering::gather::GATHER;
use crate::objects::WorkerCount;
use crate::objects::masters::ActionPart;
use crate::objects::masters::CombatLevel;
use crate::objects::masters::GarrisonCapacity;
use crate::objects::masters::MasterKind;
use crate::objects::masters::TaskTerrain;
#[cfg(test)]
use crate::objects::masters::WorkValue;
#[cfg(test)]
use crate::orders::GARRISON;
use crate::players::CivId;
use crate::players::attribute::AttributeId;
use crate::players::relation_row;
use crate::players::treaty::{NomadTreaty, TreatyMinutes, TreatyMonitor, TreatyState};
use crate::test_ids::carrying;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
#[cfg(test)]
use crate::test_ids::{class_of, opt_master};
use crate::testing::attribute;
use crate::testing::class;
use crate::testing::ids;
use crate::testing::roles;
use crate::testing::slots;
use crate::unit_ai::task_finder::target_steps::{drop_off_finding, garrison_into, repair_finding};
use crate::unit_ai::task_finder::world::TaskWorld;
use crate::world::GameMode;
use crate::world::player::Player;
use std::sync::LazyLock;

/// The tests' drop site: a building of master 562 takes food from any worker.
static FOOD_AT_562: LazyLock<[DropSite; 1]> = LazyLock::new(|| {
    [DropSite {
        building: master_id(562),
        targets: vec![DropTarget {
            class: ClassId::named(99),
            workers: WorkerCount::of_data(0),
            state: None,
            attribute: Some(attribute::FOOD),
        }],
        ..DropSite::default()
    }]
});

/// Every player on its own (the world holds no player, so none has an ally); a building of
/// master 562 takes food ([`FOOD_AT_562`]).
fn alone() -> TaskWorld<'static> {
    TaskWorld {
        players: &[],
        treaty: None,
        drop_sites: &*FOOD_AT_562,
    }
}

fn task(action_kind: ActionKind, object_group: i16, object_id: i16, work_val1: f32) -> Task {
    Task {
        action_kind,
        object_group: class_of(object_group),
        object_id: opt_master(object_id),
        work_val1: WorkValue::new(work_val1),
        ..Task::default()
    }
}

fn master(id: MasterId, kind: MasterKind, class: ClassId, tasks: Vec<Task>) -> Master {
    Master {
        id,
        kind,
        class,
        action: Some(ActionPart {
            tasks,
            ..ActionPart::default()
        }),
        ..Master::default()
    }
}

fn actor(own: &Master, held: Held) -> Actor<'_> {
    Actor {
        id: object_id(9181),
        master: own,
        player: PlayerId::new(2),
        held,
        attribute_95: Amount::ZERO,
        civ: CivId::new(1),
    }
}

fn building(site: &Master, player: PlayerId, state: ObjectState) -> TaskTarget<'_> {
    TaskTarget {
        id: object_id(10028),
        master: site,
        player,
        state,
        held: carrying(None, 0.0),
        hp: HitPoints::new(550.0),
        max_hp: HitPoints::new(550.0),
        attackable: true,
        pasture: None,
        top_action: None,
    }
}

/// Players 0 to 4, of whom 1 and 3 are allies both ways and player 1 counts 4 an ally, 4
/// does not count 1: their relation rows (0 an ally, 3 an enemy).
static TEAMS: LazyLock<Vec<Player>> = LazyLock::new(|| {
    [
        [0, 3, 3, 3, 3],
        [3, 0, 3, 0, 0],
        [3, 3, 0, 3, 3],
        [3, 0, 3, 0, 3],
        [3, 3, 3, 3, 0],
    ]
    .map(|row| Player {
        relations: relation_row(row),
        ..Player::default()
    })
    .into()
});

/// The players of [`TEAMS`], no drop sites.
fn teams() -> TaskWorld<'static> {
    TaskWorld {
        players: &TEAMS,
        treaty: None,
        drop_sites: &[],
    }
}

/// A treaty monitor of the world (`+0x820`) in state `state`.
fn monitor(state: TreatyState) -> TreatyMonitor {
    TreatyMonitor {
        state,
        ..TreatyMonitor::new(
            TreatyMinutes::of_byte(20),
            NomadTreaty::NotForced,
            GameMode::RandomMap,
        )
    }
}

/// `FUN_141663dc0`: its own player counts, Gaia never; past the row (the world's player count)
/// no player counts; relation 0 counts; relation 4 only with the ally flag; relation 1 only with
/// the flag while the world's treaty holds (its monitor in state 1), never without a monitor or
/// in another state; relation 3 and an unnamed one never.
#[test]
fn ally_flag_counts_neutral_only_while_treaty_holds() {
    let holds = monitor(TreatyState::Holds);
    let ended = monitor(TreatyState::Ended);
    let new = monitor(TreatyState::New);
    let treaties = [None, Some(&holds), Some(&ended), Some(&new)];
    let row = relation_row([3, 0, 1, 4, 3, 2]);
    let me = PlayerId::new(1);
    let ally_test = |treaty, to: u8, test| allied(&row, treaty, me, PlayerId::new(to), test);
    for treaty in treaties {
        for test in [AllyTest::WithRelation4, AllyTest::AlliesOnly] {
            assert!(ally_test(treaty, 1, test), "its own player");
            assert!(!ally_test(treaty, 0, test), "Gaia");
            assert!(!ally_test(treaty, 4, test), "relation 3");
            assert!(!ally_test(treaty, 5, test), "an unnamed relation");
            assert!(!ally_test(treaty, 6, test), "at the player count");
            assert!(!ally_test(treaty, 255, test), "past the player count");
        }
        assert!(
            ally_test(treaty, 3, AllyTest::WithRelation4),
            "relation 4 with the flag"
        );
        assert!(
            !ally_test(treaty, 3, AllyTest::AlliesOnly),
            "relation 4 without"
        );
        assert!(
            !ally_test(treaty, 2, AllyTest::AlliesOnly),
            "relation 1 without the flag"
        );
    }
    assert!(ally_test(Some(&holds), 2, AllyTest::WithRelation4));
    assert!(!ally_test(None, 2, AllyTest::WithRelation4));
    assert!(!ally_test(Some(&ended), 2, AllyTest::WithRelation4));
    assert!(!ally_test(Some(&new), 2, AllyTest::WithRelation4));
    // its own player counts even past its row, as the game compares `to` with its id first
    assert!(allied(
        &row,
        None,
        PlayerId::new(7),
        PlayerId::new(7),
        AllyTest::AlliesOnly
    ));
}

/// `FUN_141663d90`: no player counts for no one, its own player not either; a player is
/// [`allied`] on its id.
#[test]
fn ally_test_on_no_player_is_false() {
    let holds = monitor(TreatyState::Holds);
    let row = relation_row([3, 0, 1]);
    let me = PlayerId::new(1);
    for test in [AllyTest::WithRelation4, AllyTest::AlliesOnly] {
        assert!(!allied_player(&row, Some(&holds), me, None, test));
        assert!(allied_player(&row, None, me, Some(me), test));
    }
    assert!(allied_player(
        &row,
        Some(&holds),
        me,
        Some(PlayerId::new(2)),
        AllyTest::WithRelation4
    ));
    assert!(!allied_player(
        &row,
        None,
        me,
        Some(PlayerId::new(2)),
        AllyTest::WithRelation4
    ));
}

/// `FUN_1411a8620`, a task's own test on a target: a holder's task wants the target to hold
/// some of its resource (either of its two, or any for a task of neither); types 3 and 106
/// never apply, attacks (7, 9) want an attackable target; then the owner type says whose
/// target, and the unit itself is never its own target but for type 4.
#[test]
fn task_applies_by_what_the_target_holds_and_whose_it_is() {
    let own = Master {
        combat_level: CombatLevel::NONE,
        ..Master::default()
    };
    let fighter = Master {
        combat_level: CombatLevel::ANIMAL,
        ..Master::default()
    };
    let me = |master| Actor {
        id: object_id(9181),
        player: PlayerId::new(1),
        ..actor(master, carrying(None, 0.0))
    };
    let of = |owner_kind| Task {
        owner_kind,
        ..Task::default()
    };
    let at = |player, held| TaskTarget {
        held,
        ..building(&own, player, ObjectState::Alive)
    };
    let applies_to =
        |task: &Task, master, target: &TaskTarget<'_>| applies(task, &me(master), target, &teams());
    // the holder's test (−1 for no attribute)
    let kind = AttributeId::of_data;
    let holder = |attribute_kind, attribute_kind3| Task {
        holding_attr: 1,
        attribute_kind,
        attribute_kind3,
        ..of(TargetOwner::ANY)
    };
    for (task, held, takes) in [
        (holder(kind(5), kind(-1)), carrying(kind(-1), 3.0), false),
        (holder(kind(5), kind(-1)), carrying(kind(5), 0.0), false),
        (holder(kind(5), kind(-1)), carrying(kind(6), 3.0), false),
        (holder(kind(5), kind(-1)), carrying(kind(5), 3.0), true),
        (holder(kind(5), kind(6)), carrying(kind(6), 3.0), true),
        (holder(kind(-1), kind(-1)), carrying(kind(9), 3.0), true),
    ] {
        assert_eq!(
            applies_to(&task, &own, &at(PlayerId::new(1), held)),
            takes,
            "{held:?}"
        );
    }
    // the types that never apply, and the attacks on what cannot be attacked
    for (action_kind, attackable, takes) in [
        (ENTER, true, false),
        (REPAIR, true, false),
        (ActionKind::COMBAT, false, false),
        (ATTACK, false, false),
        (ATTACK, true, true),
    ] {
        let task = Task {
            action_kind,
            ..of(TargetOwner::ANY)
        };
        let target = TaskTarget {
            attackable,
            ..at(PlayerId::new(1), carrying(None, 0.0))
        };
        assert_eq!(applies_to(&task, &own, &target), takes, "{action_kind}");
    }
    // the owner types, by the target's player
    for (owner_kind, player, master, takes) in [
        (1, 1, &own, true),
        (1, 2, &own, false),
        (2, 2, &own, true),
        (2, 0, &own, false),
        (2, 3, &own, false),
        (2, 1, &own, false),
        (3, 0, &own, true),
        (3, 2, &own, false),
        (4, 0, &own, true),
        (4, 3, &own, true),
        (4, 4, &own, false),
        (4, 2, &own, false),
        (5, 1, &own, false),
        (5, 3, &own, false),
        (5, 2, &own, true),
        (5, 0, &own, false),
        (5, 0, &fighter, true),
        (6, 2, &own, true),
        (6, 1, &own, false),
        (0, 1, &own, true),
    ] {
        let target = at(PlayerId::new(player), carrying(None, 0.0));
        let got = applies_to(&of(TargetOwner::of_data(owner_kind)), master, &target);
        assert_eq!(got, takes, "type {owner_kind} on player {player}");
    }
    // the unit itself: only type 4 takes it
    let itself = TaskTarget {
        id: object_id(9181),
        ..at(PlayerId::new(1), carrying(None, 0.0))
    };
    assert!(applies_to(&of(TargetOwner::FRIENDLY), &own, &itself));
    assert!(!applies_to(&of(TargetOwner::OWN), &own, &itself));
}

fn find<'master>(
    list: &'master Master,
    held: Held,
    task_target: &TaskTarget<'_>,
) -> GenieResult<Option<&'master Task>> {
    let on = TaskOn {
        target: Some(task_target),
        general: Some(task_target),
        terrain: None,
    };
    find_task(
        list,
        &actor(list, held),
        &on,
        None,
        &alone(),
        slots(),
        &roles(),
    )
}

/// `FUN_140fdd950`: an entry for the worker class with resource types, for the civ, whose
/// basic resources or targets take the kind.
#[test]
fn the_table_gives_the_buildings_that_take_kind_from_class() {
    let entry = |building, class, kinds: Vec<AttributeId>, civs: Vec<u8>, attribute| DropSite {
        building,
        worker_class: class,
        civs: civs.into_iter().map(CivId::new).collect(),
        resource_kinds: kinds,
        targets: vec![DropTarget {
            class: class::TREE,
            workers: WorkerCount::NO_CAP,
            state: None,
            attribute,
        }],
        ..DropSite::default()
    };
    let table = [
        entry(
            ids::TOWN_CENTRE,
            Some(class::VILLAGER),
            attribute::RESOURCES.to_vec(),
            vec![],
            Some(attribute::MEAT),
        ),
        // no resource types: never listed
        entry(
            master_id(68),
            Some(class::VILLAGER),
            vec![],
            vec![],
            Some(attribute::WOOD),
        ),
        entry(
            master_id(562),
            Some(class::VILLAGER),
            vec![attribute::WOOD],
            vec![],
            Some(attribute::WOOD),
        ),
        // another civ's
        entry(
            master_id(1754),
            Some(class::VILLAGER),
            vec![attribute::WOOD],
            vec![17],
            Some(attribute::WOOD),
        ),
        entry(
            ids::ID_45,
            Some(class::SIEGE),
            vec![attribute::FOOD],
            vec![],
            Some(AttributeId::named(17)),
        ),
    ];
    assert_eq!(
        table_drop_buildings(
            &table,
            Some(attribute::WOOD),
            Some(class::VILLAGER),
            CivId::new(1)
        ),
        [ids::TOWN_CENTRE, master_id(562)]
    );
    assert_eq!(
        table_drop_buildings(
            &table,
            Some(attribute::WOOD),
            Some(class::VILLAGER),
            CivId::new(17)
        ),
        [ids::TOWN_CENTRE, master_id(562), master_id(1754)]
    );
    // a kind of 4 or more by the targets' attribute types; −1 is not a basic resource
    assert_eq!(
        table_drop_buildings(
            &table,
            Some(attribute::MEAT),
            Some(class::VILLAGER),
            CivId::new(1)
        ),
        [ids::TOWN_CENTRE]
    );
    assert_eq!(
        table_drop_buildings(&table, Some(AttributeId::named(17)), None, CivId::new(1)),
        [ids::ID_45]
    );
    assert_eq!(
        table_drop_buildings(&table, None, Some(class::VILLAGER), CivId::new(1)),
        Vec::<MasterId>::new()
    );
}

/// `FUN_1412784d0` for a master with object flag 0x10: no task on prey (class 9) whose master
/// stores something (`+0x610` above 0); prey without storage takes its task as any target.
#[test]
fn flag_0x10_master_has_no_task_on_prey_that_stores_food() {
    let hunt = task(HUNT, class::PREY.get(), -1, 1.0);
    let hunter = Master {
        object_flags: ObjectFlags::SCOUT,
        ..master(
            master_id(122),
            MasterKind::COMBAT,
            class::VILLAGER,
            vec![hunt],
        )
    };
    let deer = Master {
        storage: vec![(Some(attribute::FOOD), Amount::new(100.0), 1)],
        ..master(master_id(65), MasterKind::COMBAT, class::PREY, vec![])
    };
    let target = |master| TaskTarget {
        state: ObjectState::Alive,
        ..building(master, PlayerId::new(0), ObjectState::Alive)
    };
    assert_eq!(find(&hunter, carrying(None, 0.0), &target(&deer)), Ok(None));
    let eaten = Master {
        storage: vec![(Some(attribute::FOOD), Amount::ZERO, 1)],
        ..deer.clone()
    };
    assert_eq!(
        find(&hunter, carrying(None, 0.0), &target(&eaten)),
        Ok(Some(&hunt))
    );
    let plain = Master {
        object_flags: ObjectFlags::NONE,
        ..hunter
    };
    assert_eq!(
        find(&plain, carrying(None, 0.0), &target(&deer)),
        Ok(Some(&hunt))
    );
}

#[test]
fn build_task_for_the_foundation_master_or_class_needs_work_value() {
    let house = master(ids::HOUSE, MasterKind::BUILDING, class::BUILDING, vec![]);
    let site = building(&house, PlayerId::new(2), ObjectState::Foundation);
    let by = |tasks: Vec<Task>| {
        let builder = master(ids::BUILDER, MasterKind::COMBAT, class::VILLAGER, tasks);
        find(&builder, carrying(None, 0.0), &site).map(Option::<&Task>::copied)
    };
    let generic = task(ActionKind::BUILD, -1, -1, 0.0);
    assert_eq!(by(vec![generic]), Ok(Some(generic)));
    assert_eq!(
        by(vec![task(ActionKind::BUILD, 3, -1, 2.0), generic]),
        Ok(Some(task(ActionKind::BUILD, 3, -1, 2.0)))
    );
    assert_eq!(
        by(vec![task(ActionKind::BUILD, 3, -1, 0.0), generic]),
        Ok(None)
    );
    assert_eq!(
        by(vec![
            generic,
            task(ActionKind::BUILD, -1, ids::HOUSE.get(), 0.0)
        ]),
        Ok(None)
    );
}

#[test]
fn unit_carrying_what_finished_drop_site_takes_drops_it_off_there() {
    let camp = master(
        master_id(562),
        MasterKind::BUILDING,
        class::BUILDING,
        vec![],
    );
    let chop = Task {
        attribute_kind: Some(attribute::FOOD),
        ..task(GATHER, 15, -1, 0.6)
    };
    let lumberjack = master(
        master_id(123),
        MasterKind::COMBAT,
        class::VILLAGER,
        vec![task(ActionKind::BUILD, -1, -1, 1.0), chop],
    );
    assert_eq!(
        find(
            &lumberjack,
            carrying(Some(attribute::FOOD), 10.0),
            &building(&camp, PlayerId::new(2), ObjectState::Alive)
        ),
        Ok(Some(&chop))
    );
    // nothing carried: the general search, which has no task for the camp
    assert_eq!(
        find(
            &lumberjack,
            carrying(None, 0.0),
            &building(&camp, PlayerId::new(2), ObjectState::Alive)
        ),
        Ok(None)
    );
    // the camp still a foundation: building it
    assert_eq!(
        find(
            &lumberjack,
            carrying(Some(attribute::FOOD), 10.0),
            &building(&camp, PlayerId::new(2), ObjectState::Foundation)
        ),
        Ok(Some(&task(ActionKind::BUILD, -1, -1, 1.0)))
    );
}

/// The repair branch (`LAB_1412796b4`): a damaged building of the player's own (or Gaia's)
/// takes the repair task for its master first, else the last for its class with a work value,
/// else the last for any target; under the repair filter with none the finder ends; a whole
/// building or another player's has none.
#[test]
fn damaged_building_takes_the_repair_task_for_its_master_or_class() {
    let house = master(ids::HOUSE, MasterKind::BUILDING, class::BUILDING, vec![]);
    let damaged = TaskTarget {
        hp: HitPoints::new(300.0),
        ..building(&house, PlayerId::new(2), ObjectState::Alive)
    };
    let by = |tasks: Vec<Task>, task_target: &TaskTarget<'_>, filter: Option<ActionKind>| {
        let builder = master(ids::BUILDER, MasterKind::COMBAT, class::VILLAGER, tasks);
        find_with(&builder, task_target, filter).map(Option::<&Task>::copied)
    };
    let any = task(REPAIR, -1, -1, 0.0);
    let class3 = task(REPAIR, 3, -1, 2.0);
    let own = task(REPAIR, -1, ids::HOUSE.get(), 0.0);
    assert_eq!(by(vec![any], &damaged, None), Ok(Some(any)));
    assert_eq!(by(vec![class3, any], &damaged, None), Ok(Some(class3)));
    assert_eq!(by(vec![class3, any, own], &damaged, None), Ok(Some(own)));
    // a class task without a work value leaves no task, not the any one
    assert_eq!(
        by(vec![task(REPAIR, 3, -1, 0.0), any], &damaged, Some(REPAIR)),
        Ok(None)
    );
    // Gaia's damaged building too; whole or another player's, none
    let gaia = TaskTarget {
        player: PlayerId::new(0),
        ..damaged
    };
    assert_eq!(by(vec![any], &gaia, Some(REPAIR)), Ok(Some(any)));
    assert_eq!(
        by(
            vec![any],
            &building(&house, PlayerId::new(2), ObjectState::Alive),
            Some(REPAIR)
        ),
        Ok(None)
    );
    let foreign = TaskTarget {
        player: PlayerId::new(3),
        ..damaged
    };
    assert_eq!(by(vec![any], &foreign, Some(REPAIR)), Ok(None));
}

/// A transport of player `player` (object flag 0) in state 2 of class `class`.
fn transport(ship: &Master, player: PlayerId) -> TaskTarget<'_> {
    TaskTarget {
        id: object_id(7420),
        hp: HitPoints::new(100.0),
        max_hp: HitPoints::new(100.0),
        ..building(ship, player, ObjectState::Alive)
    }
}

fn find_with<'master>(
    list: &'master Master,
    task_target: &TaskTarget<'_>,
    filter: Option<ActionKind>,
) -> GenieResult<Option<&'master Task>> {
    let on = TaskOn {
        target: Some(task_target),
        general: Some(task_target),
        terrain: None,
    };
    find_task(
        list,
        &actor(list, carrying(None, 0.0)),
        &on,
        filter,
        &alone(),
        slots(),
        &roles(),
    )
}

#[test]
fn the_garrison_filter_goes_on_to_transport_class_task() {
    // FUN_1412784d0 with filter 3 on a transport ship (545, class 20, object flag 0): no
    // building's task, so LAB_1412796b4's first task of action 3 on class 20
    let ship = Master {
        object_flags: ObjectFlags::GARRISON,
        garrison: GarrisonCapacity::of_data(5),
        ..master(master_id(545), MasterKind::COMBAT, class::TRANSPORT, vec![])
    };
    let enter = task(ENTER, class::TRANSPORT.get(), -1, 0.0);
    let villager = master(
        ids::VILLAGER_FEMALE,
        MasterKind::COMBAT,
        class::VILLAGER,
        vec![task(ENTER, 3, -1, 0.0), enter],
    );
    assert_eq!(
        find_with(&villager, &transport(&ship, PlayerId::new(2)), Some(ENTER)),
        Ok(Some(&enter))
    );
    // any filter takes it too, another not
    assert_eq!(
        find_with(&villager, &transport(&ship, PlayerId::new(2)), None),
        Ok(Some(&enter))
    );
    // no task on its class: none with the garrison filter, and none with any filter (the
    // finder ends before the general search, which would give the militia's attack)
    let attack = Task {
        owner_kind: TargetOwner::ANY,
        ..task(ActionKind::COMBAT, class::TRANSPORT.get(), -1, 0.0)
    };
    let militia = master(
        master_id(74),
        MasterKind::COMBAT,
        class::CLASS_6,
        vec![attack],
    );
    assert_eq!(
        find_with(&militia, &transport(&ship, PlayerId::new(2)), Some(ENTER)),
        Ok(None)
    );
    assert_eq!(
        find_with(&militia, &transport(&ship, PlayerId::new(2)), None),
        Ok(None)
    );
    // an enemy's transport is no garrison: the general search with any filter
    assert_eq!(
        find_with(&militia, &transport(&ship, PlayerId::new(1)), None),
        Ok(Some(&attack))
    );
    // a target without object flag 0 has no task with the garrison filter
    let galley = Master {
        object_flags: ObjectFlags::NONE,
        ..ship
    };
    assert_eq!(
        find_with(
            &villager,
            &transport(&galley, PlayerId::new(2)),
            Some(ENTER)
        ),
        Ok(None)
    );
}

#[test]
fn villager_garrisons_only_transport_class_without_the_garrison_filter() {
    // LAB_1412796b4: a villager takes a target with object flag 0 not of class 20 (a ram,
    // class 13) only under the garrison filter (or mode 1, not modelled)
    let ram = Master {
        object_flags: ObjectFlags::GARRISON,
        garrison: GarrisonCapacity::of_data(4),
        ..master(ids::ID_35, MasterKind::COMBAT, class::SIEGE, vec![])
    };
    let enter = task(ENTER, class::SIEGE.get(), -1, 0.0);
    let villager = master(
        ids::VILLAGER_FEMALE,
        MasterKind::COMBAT,
        class::VILLAGER,
        vec![enter],
    );
    assert_eq!(
        find_with(&villager, &transport(&ram, PlayerId::new(2)), Some(ENTER)),
        Ok(Some(&enter))
    );
    assert_eq!(
        find_with(&villager, &transport(&ram, PlayerId::new(2)), None),
        Ok(None)
    );
    let infantry = master(
        master_id(74),
        MasterKind::COMBAT,
        class::CLASS_6,
        vec![enter],
    );
    assert_eq!(
        find_with(&infantry, &transport(&ram, PlayerId::new(2)), None),
        Ok(Some(&enter))
    );
    // a dead ram (state 3) is none of its garrisons
    let dead = TaskTarget {
        state: ObjectState::Dying,
        ..transport(&ram, PlayerId::new(2))
    };
    assert_eq!(find_with(&infantry, &dead, Some(ENTER)), Ok(None));
}

#[test]
fn the_general_search_takes_task_for_the_target_class_whose_owner_kind_fits() {
    let house = master(ids::HOUSE, MasterKind::BUILDING, class::BUILDING, vec![]);
    let attack = Task {
        owner_kind: TargetOwner::ENEMY,
        ..task(ActionKind::COMBAT, class::BUILDING.get(), -1, 0.0)
    };
    let militia = master(
        master_id(74),
        MasterKind::COMBAT,
        class::CLASS_6,
        vec![attack],
    );
    assert_eq!(
        find(
            &militia,
            carrying(None, 0.0),
            &building(&house, PlayerId::new(1), ObjectState::Alive)
        ),
        Ok(Some(&attack))
    );
    assert_eq!(
        find(
            &militia,
            carrying(None, 0.0),
            &building(&house, PlayerId::new(2), ObjectState::Alive)
        ),
        Ok(None)
    );
    let rubble = TaskTarget {
        attackable: false,
        ..building(&house, PlayerId::new(1), ObjectState::Alive)
    };
    assert_eq!(find(&militia, carrying(None, 0.0), &rubble), Ok(None));
}
/// The finder's head: the garrison filter without a garrison task answers none on a target
/// that is no transport and goes on for a transport; a flag-0x10 master has none on prey
/// that stores something; the relic carrier takes task 0x87 at its monastery (master 109);
/// a monk's conversion under attribute 95 is not modelled; anything else goes on.
#[test]
fn the_finder_head_answers_the_garrison_filter_prey_and_relics() {
    let house = master(ids::HOUSE, MasterKind::BUILDING, class::BUILDING, vec![]);
    let villager = master(
        ids::VILLAGER_MALE,
        MasterKind::COMBAT,
        class::VILLAGER,
        vec![task(ActionKind::ACTION_135, -1, -1, 0.0)],
    );
    let villager_actor = actor(&villager, carrying(None, 0.0));
    let task_target = building(&house, PlayerId::new(2), ObjectState::Alive);
    assert_eq!(
        finder_head(
            &villager,
            &villager_actor,
            Some(&task_target),
            None,
            &alone(),
            &roles()
        ),
        Ok(Finding::Next)
    );
    assert_eq!(
        finder_head(
            &villager,
            &villager_actor,
            Some(&task_target),
            Some(ENTER),
            &alone(),
            &roles()
        ),
        Ok(Finding::Answer(None))
    );
    let ship = Master {
        object_flags: ObjectFlags::GARRISON,
        ..master(master_id(545), MasterKind::COMBAT, class::TRANSPORT, vec![])
    };
    assert_eq!(
        finder_head(
            &villager,
            &villager_actor,
            Some(&transport(&ship, PlayerId::new(2))),
            Some(ENTER),
            &alone(),
            &roles()
        ),
        Ok(Finding::Next)
    );
    let deer = Master {
        storage: vec![(Some(attribute::FOOD), Amount::new(100.0), 0)],
        ..master(master_id(65), MasterKind::COMBAT, class::PREY, vec![])
    };
    let wolf = Master {
        object_flags: ObjectFlags::SCOUT,
        ..master(master_id(126), MasterKind::COMBAT, class::PREDATOR, vec![])
    };
    let prey = building(&deer, PlayerId::new(0), ObjectState::Alive);
    assert_eq!(
        finder_head(
            &wolf,
            &actor(&wolf, carrying(None, 0.0)),
            Some(&prey),
            None,
            &alone(),
            &roles()
        ),
        Ok(Finding::Answer(None))
    );
    let monastery = master(
        ids::TOWN_CENTRE,
        MasterKind::BUILDING,
        class::BUILDING,
        vec![],
    );
    let carrier = actor(&villager, carrying(Some(attribute::RELIC), 1.0));
    assert_eq!(
        finder_head(
            &villager,
            &carrier,
            Some(&building(&monastery, PlayerId::new(2), ObjectState::Alive)),
            None,
            &alone(),
            &roles()
        ),
        Ok(Finding::Answer(Some(&villager.action_part().tasks[0])))
    );
    let monk = master(ids::ID_448, MasterKind::COMBAT, class::MONK, vec![]);
    let converting = Actor {
        attribute_95: Amount::new(2.0),
        ..actor(&monk, carrying(None, 0.0))
    };
    let peasant = building(&villager, PlayerId::new(3), ObjectState::Alive);
    assert!(finder_head(&monk, &converting, Some(&peasant), None, &alone(), &roles()).is_err());
}

/// The general search's last step (`FUN_1411a8ba0`): without a task for the target, or
/// without a target, the first task for the terrain under the point (the transport's beach
/// task, 12 on terrain 2), whatever the filter the finder's tail passes (−1); none without a
/// terrain (a point off the map, or x or y −1).
#[test]
fn point_on_terrain_takes_the_task_for_that_terrain() {
    let beach = Task {
        terrain: TaskTerrain::of_data(2),
        ..task(TRANSPORT, -1, -1, 0.0)
    };
    let ship = master(
        master_id(545),
        MasterKind::COMBAT,
        class::TRANSPORT,
        vec![task(ActionKind::named(109), -1, -1, 0.0), beach],
    );
    let actor = actor(&ship, carrying(None, 0.0));
    let found = |task_target: Option<&TaskTarget<'_>>, terrain, filter| {
        let on = TaskOn {
            target: task_target,
            general: task_target,
            terrain,
        };
        find_task(&ship, &actor, &on, filter, &alone(), slots(), &roles())
            .map(|found| found.map(|task| task.action_kind))
    };
    assert_eq!(
        found(None, Some(TerrainId::named(2)), None),
        Ok(Some(TRANSPORT))
    );
    assert_eq!(
        found(None, Some(TerrainId::named(2)), Some(TRANSPORT)),
        Ok(Some(TRANSPORT))
    );
    assert_eq!(found(None, Some(TerrainId::named(1)), None), Ok(None));
    assert_eq!(found(None, None, None), Ok(None));
    let villager = master(
        ids::VILLAGER_MALE,
        MasterKind::COMBAT,
        class::VILLAGER,
        vec![],
    );
    let peasant = building(&villager, PlayerId::new(2), ObjectState::Alive);
    assert_eq!(
        found(Some(&peasant), Some(TerrainId::named(2)), None),
        Ok(Some(TRANSPORT))
    );
}

/// The finder's steps on a target: its player's foundation takes the build task; master 199
/// is not modelled; a dock or market below state 3 takes the first trade task (0x6f) for its
/// master or class the filter takes, and without one goes on; a transport's garrison without
/// a task ends the finder (none); anything else goes on.
#[test]
fn the_finder_steps_on_target_answer_or_go_on() {
    let house = master(ids::HOUSE, MasterKind::BUILDING, class::BUILDING, vec![]);
    let builder = master(
        ids::BUILDER,
        MasterKind::COMBAT,
        class::VILLAGER,
        vec![task(ActionKind::BUILD, -1, -1, 1.0)],
    );
    let builder_actor = actor(&builder, carrying(None, 0.0));
    assert_eq!(
        on_target(
            &builder,
            &builder_actor,
            &building(&house, PlayerId::new(2), ObjectState::Foundation),
            None,
            &alone(),
            &roles()
        ),
        Ok(Finding::Answer(Some(&builder.action_part().tasks[0])))
    );
    assert_eq!(
        on_target(
            &builder,
            &builder_actor,
            &building(&house, PlayerId::new(2), ObjectState::Alive),
            None,
            &alone(),
            &roles()
        ),
        Ok(Finding::Next)
    );
    let m199 = master(
        ids::FISH_TRAP,
        MasterKind::BUILDING,
        class::BUILDING,
        vec![],
    );
    assert!(
        on_target(
            &builder,
            &builder_actor,
            &building(&m199, PlayerId::new(2), ObjectState::Alive),
            None,
            &alone(),
            &roles()
        )
        .is_err()
    );
    let m45 = master(ids::ID_45, MasterKind::COMBAT, class::CLASS_6, vec![]);
    assert_eq!(
        on_target(
            &builder,
            &builder_actor,
            &building(&m45, PlayerId::new(2), ObjectState::Alive),
            None,
            &alone(),
            &roles()
        ),
        Ok(Finding::Next)
    );
    let trader = master(
        master_id(128),
        MasterKind::COMBAT,
        class::TRADE_CART,
        vec![
            task(ActionKind::TRADE, 7, -1, 1.0),
            task(ActionKind::TRADE, -1, ids::MARKET.get(), 1.0),
            task(ActionKind::TRADE, 6, -1, 1.0),
        ],
    );
    let cart = actor(&trader, carrying(None, 0.0));
    let dock = building(&m45, PlayerId::new(2), ObjectState::Alive);
    assert_eq!(
        on_target(&trader, &cart, &dock, None, &alone(), &roles()),
        Ok(Finding::Answer(Some(&trader.action_part().tasks[2])))
    );
    assert_eq!(
        on_target(
            &trader,
            &cart,
            &dock,
            Some(ActionKind::TRADE),
            &alone(),
            &roles()
        ),
        Ok(Finding::Answer(Some(&trader.action_part().tasks[2])))
    );
    assert_eq!(
        on_target(&trader, &cart, &dock, Some(GATHER), &alone(), &roles()),
        Ok(Finding::Next)
    );
    let market = master(ids::MARKET, MasterKind::BUILDING, class::CLASS_7, vec![]);
    assert_eq!(
        on_target(
            &trader,
            &cart,
            &building(&market, PlayerId::new(2), ObjectState::Alive),
            None,
            &alone(),
            &roles()
        ),
        Ok(Finding::Answer(Some(&trader.action_part().tasks[0])))
    );
    // state 3 and up: no trade
    assert_eq!(
        on_target(
            &trader,
            &cart,
            &building(&m45, PlayerId::new(2), ObjectState::Dying),
            None,
            &alone(),
            &roles()
        ),
        Ok(Finding::Next)
    );
    let soldier = master(master_id(4), MasterKind::COMBAT, class::CLASS_6, vec![]);
    let ship = Master {
        object_flags: ObjectFlags::GARRISON,
        ..master(master_id(545), MasterKind::COMBAT, class::TRANSPORT, vec![])
    };
    let sailor = actor(&soldier, carrying(None, 0.0));
    assert_eq!(
        on_target(
            &soldier,
            &sailor,
            &transport(&ship, PlayerId::new(2)),
            None,
            &alone(),
            &roles()
        ),
        Ok(Finding::Answer(None))
    );
}

/// The drop-off step: a lumberjack carrying wood to its finished camp takes its chop task;
/// carrying nothing goes on; without a task for the load, a camp at another work answers
/// none, an idle one goes on.
#[test]
fn the_drop_off_step_takes_the_task_for_the_load() {
    fn drop<'master>(
        list: &'master Master,
        held: Held,
        task_target: &TaskTarget<'_>,
    ) -> Finding<'master> {
        drop_off_finding(list, &actor(list, held), task_target, None, &alone())
    }
    let camp = master(
        master_id(562),
        MasterKind::BUILDING,
        class::BUILDING,
        vec![],
    );
    let chop = Task {
        attribute_kind: Some(attribute::FOOD),
        ..task(GATHER, 15, -1, 0.6)
    };
    let lumberjack = master(
        master_id(123),
        MasterKind::COMBAT,
        class::VILLAGER,
        vec![chop],
    );
    let site = building(&camp, PlayerId::new(2), ObjectState::Alive);
    assert_eq!(
        drop(&lumberjack, carrying(Some(attribute::FOOD), 10.0), &site),
        Finding::Answer(Some(&lumberjack.action_part().tasks[0]))
    );
    assert_eq!(drop(&lumberjack, carrying(None, 0.0), &site), Finding::Next);
    let idle = master(master_id(123), MasterKind::COMBAT, class::VILLAGER, vec![]);
    assert_eq!(
        drop(&idle, carrying(Some(attribute::FOOD), 10.0), &site),
        Finding::Next
    );
    let busy = TaskTarget {
        top_action: Some(ActionKind::named(0x10)),
        ..site
    };
    assert_eq!(
        drop(&idle, carrying(Some(attribute::FOOD), 10.0), &busy),
        Finding::Answer(None)
    );
}

/// The garrison into a transport: its class's task of action 3; without one the garrison
/// filter has none and any filter ends the finder after the repair; a villager goes on for
/// a transport of another class.
#[test]
fn the_garrison_into_transport_takes_its_class_task() {
    fn into<'master>(
        list: &'master Master,
        task_target: &TaskTarget<'_>,
        filter: Option<ActionKind>,
    ) -> (Finding<'master>, bool) {
        garrison_into(
            list,
            &actor(list, carrying(None, 0.0)),
            task_target,
            filter,
            &alone(),
            &roles(),
        )
    }
    let ship = Master {
        object_flags: ObjectFlags::GARRISON,
        ..master(master_id(545), MasterKind::COMBAT, class::TRANSPORT, vec![])
    };
    let boarding = task(GARRISON, class::TRANSPORT.get(), -1, 0.0);
    let soldier = master(
        master_id(4),
        MasterKind::COMBAT,
        class::CLASS_6,
        vec![boarding],
    );
    let bare = master(master_id(4), MasterKind::COMBAT, class::CLASS_6, vec![]);
    let task_target = transport(&ship, PlayerId::new(2));
    assert_eq!(
        into(&soldier, &task_target, None),
        (
            Finding::Answer(Some(&soldier.action_part().tasks[0])),
            false
        )
    );
    assert_eq!(
        into(&bare, &task_target, Some(ENTER)),
        (Finding::Answer(None), false)
    );
    assert_eq!(into(&bare, &task_target, None), (Finding::Next, true));
    let barge = Master {
        object_flags: ObjectFlags::GARRISON,
        ..master(ids::ID_546, MasterKind::COMBAT, class::CLASS_6, vec![])
    };
    let villager = master(
        ids::VILLAGER_MALE,
        MasterKind::COMBAT,
        class::VILLAGER,
        vec![],
    );
    assert_eq!(
        into(&villager, &transport(&barge, PlayerId::new(2)), None),
        (Finding::Next, false)
    );
}

/// The repair step: a damaged building takes the repair task for any target; a whole one
/// goes on; under the repair filter a class's task without a work value leaves none.
#[test]
fn the_repair_step_takes_damaged_building_task() {
    fn repair<'master>(
        list: &'master Master,
        task_target: &TaskTarget<'_>,
        filter: Option<ActionKind>,
    ) -> Finding<'master> {
        repair_finding(
            list,
            &actor(list, carrying(None, 0.0)),
            task_target,
            filter,
            &alone(),
            &roles(),
        )
    }
    let house = master(ids::HOUSE, MasterKind::BUILDING, class::BUILDING, vec![]);
    let damaged = TaskTarget {
        hp: HitPoints::new(300.0),
        ..building(&house, PlayerId::new(2), ObjectState::Alive)
    };
    let any = master(
        ids::BUILDER,
        MasterKind::COMBAT,
        class::VILLAGER,
        vec![task(REPAIR, -1, -1, 0.0)],
    );
    assert_eq!(
        repair(&any, &damaged, None),
        Finding::Answer(Some(&any.action_part().tasks[0]))
    );
    assert_eq!(
        repair(
            &any,
            &building(&house, PlayerId::new(2), ObjectState::Alive),
            None
        ),
        Finding::Next
    );
    let weak = master(
        ids::BUILDER,
        MasterKind::COMBAT,
        class::VILLAGER,
        vec![task(REPAIR, class::BUILDING.get(), -1, 0.0)],
    );
    assert_eq!(repair(&weak, &damaged, Some(REPAIR)), Finding::Answer(None));
    assert_eq!(repair(&weak, &damaged, None), Finding::Next);
}

/// The finder's tail: task 13 under its filter alone for an own target; a villager on
/// livestock takes task 110 for class 58; a monk's conversion, task 0x84, healing, task
/// 0x88, class 53's conversion and master 1105 are not modelled.
#[test]
fn the_finder_tail_takes_task_13_livestock_or_the_general_search() {
    fn tail<'master>(
        list: &'master Master,
        task_target: &TaskTarget<'_>,
        filter: Option<ActionKind>,
    ) -> GenieResult<Option<&'master Task>> {
        let on = TaskOn {
            target: Some(task_target),
            general: Some(task_target),
            terrain: None,
        };
        finder_tail(
            list,
            &actor(list, carrying(None, 0.0)),
            &on,
            filter,
            &alone(),
            slots(),
            &roles(),
        )
    }
    let house = master(ids::HOUSE, MasterKind::BUILDING, class::BUILDING, vec![]);
    let own = building(&house, PlayerId::new(2), ObjectState::Alive);
    let thirteen = master(
        master_id(9),
        MasterKind::COMBAT,
        class::CLASS_6,
        vec![task(ActionKind::GUARD, -1, -1, 0.0)],
    );
    assert_eq!(
        tail(&thirteen, &own, Some(ActionKind::GUARD)),
        Ok(Some(&thirteen.action_part().tasks[0]))
    );
    assert_eq!(tail(&thirteen, &own, None), Ok(None));
    let sheep = master(master_id(594), MasterKind::COMBAT, class::LIVESTOCK, vec![]);
    let shepherd = master(
        ids::VILLAGER_MALE,
        MasterKind::COMBAT,
        class::VILLAGER,
        vec![task(HUNT, class::LIVESTOCK.get(), -1, 0.0)],
    );
    assert_eq!(
        tail(
            &shepherd,
            &building(&sheep, PlayerId::new(0), ObjectState::Alive),
            None
        ),
        Ok(Some(&shepherd.action_part().tasks[0]))
    );
    for list in [
        master(master_id(125), MasterKind::COMBAT, class::MONK, vec![]),
        master(
            master_id(9),
            MasterKind::COMBAT,
            class::CLASS_6,
            vec![task(ActionKind::HEAL, -1, -1, 0.0)],
        ),
        master(master_id(9), MasterKind::COMBAT, class::CLASS_43, vec![]),
        master(master_id(9), MasterKind::COMBAT, class::CLASS_53, vec![]),
        master(ids::ID_1105, MasterKind::COMBAT, class::SIEGE, vec![]),
    ] {
        assert!(tail(&list, &own, None).is_err(), "{}", list.id);
    }
    let relic_cart = master(
        master_id(9),
        MasterKind::COMBAT,
        class::CLASS_6,
        vec![task(ActionKind::PICK_UP, -1, -1, 0.0)],
    );
    let c42 = master(master_id(1), MasterKind::COMBAT, class::RELIC, vec![]);
    assert!(
        tail(
            &relic_cart,
            &building(&c42, PlayerId::new(2), ObjectState::Alive),
            None
        )
        .is_err()
    );
}
