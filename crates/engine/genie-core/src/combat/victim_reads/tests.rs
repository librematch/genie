//! What a hit reads of its victim: its relations to the attacker, its place, its master's
//! values and its own, and its hit point transforms.

use crate::GenieError;
use crate::combat::abilities::REMOVAL_REWARD;
use crate::combat::attack::AttackState;
use crate::combat::attack::tests::own_attack;
use crate::combat::damage::{Allied, BONUS_DAMAGE, ElevationBonus, Victim};
use crate::combat::hp_transforms::HP_TRANSFORM;
use crate::combat::powerup::{InfantryChange, InfantryCount};
use crate::combat::stingers::STINGER;
use crate::construction::MainBuildingId;
use crate::garrison::ContainerId;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Position;
use crate::map::Restriction;
use crate::map::TerrainFlags;
use crate::map::terrain::TileTerrain;
use crate::map::terrain::{Terrain, TerrainKind};
use crate::objects::HitPoints;
use crate::objects::MasterHitPoints;
use crate::objects::layers::Action;
use crate::objects::lifecycle::MasterData;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ActionPart;
use crate::objects::masters::CombatPart;
use crate::objects::masters::MasterKind;
use crate::objects::masters::RestrictionField;
use crate::objects::masters::WorkValue;
use crate::objects::masters::{Master, Task};
use crate::players::Amount;
use crate::players::AmountRate;
use crate::players::PlayerId;
use crate::players::Relation;
use crate::players::attribute::AttributeId;
use crate::test_ids::{master_id, object_id};
use crate::testing::attribute;
use crate::testing::class;
use crate::testing::scene::{Scene, SceneObject};
use crate::world::Unread;
use crate::world::World;
use crate::world::parts::Part;

/// A guard's master (4, class 0, type 70): 40 hit points, radii 0.3 and 1.2 high, a first
/// storage of 5 food, a decay of 0.25 a second and no terrain defense, with `tasks`.
fn guard(tasks: Vec<Task>) -> Master {
    Master {
        id: master_id(4),
        kind: MasterKind::COMBAT,
        class: class::CLASS_0,
        hit_points: MasterHitPoints::of_data(40),
        radius: Footprint::new(0.3, 0.3),
        height: Distance::new(1.2),
        storage: vec![(Some(attribute::FOOD), Amount::new(5.0), 0)],
        life: Some(MasterData {
            decay: AmountRate::new(0.25),
            ..MasterData::default()
        }),
        combat: Some(CombatPart {
            terrain_bonus: RestrictionField::NONE,
            ..CombatPart::default()
        }),
        action: Some(ActionPart {
            tasks,
            ..ActionPart::default()
        }),
        ..Master::default()
    }
}

/// Player 1's object 7 of `object`'s master and values at (1.5, 1.5) on a 4-tile map of two
/// players, player 2 counting player 1 as an ally of relation 4 and player 1 player 2 as an
/// enemy; every player's 500 attributes 0.
fn world_of(object: SceneObject) -> World {
    let mut world = Scene::open(GridSize::new(4, 4))
        .players(2)
        .relation(PlayerId::new(2), PlayerId::new(1), Relation::Relation4)
        .object(object)
        .build();
    for player in &mut world.players {
        player.attributes = vec![Amount::ZERO; 500];
    }
    world
}

/// Player 1's guard 7 of master `master` at (1.5, 1.5).
fn object_of(master: Master) -> SceneObject {
    SceneObject::new(7, PlayerId::new(1), master, Position::new(1.5, 1.5, 0.0))
}

/// Object `id` as a hit of player 2 reads it.
fn read(world: &World, id: i32) -> Victim {
    world
        .victim(object_id(id), PlayerId::new(2))
        .unwrap()
        .expect("the world holds the object")
}

/// A task of type `kind` of work range `range`.
fn task(kind: ActionKind, range: f32) -> Task {
    Task {
        action_kind: kind,
        work_range: Distance::new(range),
        ..Task::default()
    }
}

/// A hit point transform into master `into` below 10 hit points, enabled by attribute
/// `enablement`, its threshold raised by attribute `addend`.
fn transform_into(into: i16, enablement: Option<AttributeId>, addend: Option<AttributeId>) -> Task {
    Task {
        action_kind: HP_TRANSFORM,
        work_val1: WorkValue::new(f32::from(into)),
        work_val2: WorkValue::new(-10.0),
        attribute_kind2: enablement,
        attribute_kind3: addend,
        ..Task::default()
    }
}

/// The hit reads the relations of the attacker's player and its own both ways (relation 4 an
/// ally's): player 2 counts player 1 as allied, player 1 not player 2. The object's place is a
/// guess once the slice stops it (its place unknown) or holds it in no tile's list.
#[test]
fn victim_reads_relations_both_ways_and_whether_its_place_is_guessed() {
    let mut world = world_of(object_of(guard(Vec::new())));
    let victim = read(&world, 7);
    assert_eq!(
        (victim.allied, victim.uncertain),
        (
            Allied {
                by_attacker: true,
                by_target: false,
            },
            false
        )
    );
    world.spatial.freeze(object_id(7));
    assert!(read(&world, 7).uncertain, "a frozen place");
    world.spatial.remove(object_id(7));
    assert!(read(&world, 7).uncertain, "no place");
    assert_eq!(world.victim(object_id(8), PlayerId::new(2)), Ok(None));
}

/// The hit reads the object's own values (its hit points, what it holds, its place) and its
/// master's (type, class, radii, height, first storage, decay, hit points), and its place in its
/// tile's list: the second object on its tile.
#[test]
fn victim_reads_its_own_values_and_its_master() {
    let world = world_of(SceneObject {
        hp: HitPoints::new(33.5),
        holds: Some((attribute::FOOD, Amount::new(12.0))),
        ..object_of(guard(Vec::new()))
    });
    let victim = read(&world, 7);
    assert_eq!(
        (victim.id, victim.hp, victim.amount, victim.position),
        (
            object_id(7),
            HitPoints::new(33.5),
            Amount::new(12.0),
            Position::new(1.5, 1.5, 0.0)
        )
    );
    assert_eq!(
        (victim.kind, victim.class, victim.master_id, victim.player),
        (
            MasterKind::COMBAT,
            class::CLASS_0,
            master_id(4),
            PlayerId::new(1)
        )
    );
    assert_eq!(
        (victim.radius, victim.height, victim.storage, victim.decay),
        (
            Footprint::new(0.3, 0.3),
            Distance::new(1.2),
            Amount::new(5.0),
            AmountRate::new(0.25)
        )
    );
    assert_eq!(victim.master_hp, MasterHitPoints::of_data(40));
    let shared = Scene::open(GridSize::new(4, 4))
        .players(2)
        .object(SceneObject::new(
            6,
            PlayerId::new(2),
            guard(Vec::new()),
            Position::new(1.25, 1.25, 0.0),
        ))
        .object(object_of(guard(Vec::new())))
        .build();
    assert_eq!(read(&shared, 7).tile_place, 1);
}

/// `FUN_14120d780` reads its own player's attributes 211 and 212 only past 212 attributes
/// (`0xd4 <` the count); the target's 272 and 273 always, none it lacks.
#[test]
fn victim_reads_its_attack_elevation_only_past_attribute_212() {
    let mut world = world_of(object_of(guard(Vec::new())));
    let mut attributes = vec![Amount::ZERO; 212];
    attributes[211] = Amount::new(0.25);
    world.players[1].attributes = attributes;
    let victim = read(&world, 7);
    assert_eq!(
        (victim.attack_elevation, victim.elevation),
        (ElevationBonus::default(), ElevationBonus::default())
    );
    let mut attributes = vec![Amount::ZERO; 274];
    attributes[211] = Amount::new(0.25);
    attributes[212] = Amount::new(0.5);
    attributes[272] = Amount::new(0.125);
    attributes[273] = Amount::new(0.75);
    world.players[1].attributes = attributes;
    let victim = read(&world, 7);
    assert_eq!(
        (victim.attack_elevation, victim.elevation),
        (
            ElevationBonus {
                higher: 0.25,
                lower: 0.5,
            },
            ElevationBonus {
                higher: 0.125,
                lower: 0.75,
            }
        )
    );
}

/// A combat object's slot 0xf0 (`FUN_141082530`, types 50 and up) reads its armor; an object of
/// a type below has none.
#[test]
fn victim_below_the_combat_kinds_has_no_armor() {
    let moving = Master {
        kind: MasterKind::MOVING,
        ..guard(Vec::new())
    };
    assert_eq!(read(&world_of(object_of(moving)), 7).armored, None);
    let rge_combat = Master {
        kind: MasterKind::RGE_COMBAT,
        ..guard(Vec::new())
    };
    assert!(read(&world_of(object_of(rge_combat)), 7).armored.is_some());
}

/// The terrain defense of the object's tile (`FUN_141082530`): its master's restriction's value
/// for the tile's terrain; 1 for a master without a restriction and for an object inside
/// another; none without the map's terrain.
#[test]
fn victim_terrain_defense_reads_its_tile_unless_it_has_none() {
    let defended = Master {
        combat: Some(CombatPart {
            terrain_bonus: RestrictionField::of_data(0),
            ..CombatPart::default()
        }),
        ..guard(Vec::new())
    };
    let defense = |world: &World| read(world, 7).armored.and_then(|armored| armored.terrain);
    let mut world = world_of(object_of(defended.clone()));
    let mut terrain = Terrain::new(
        GridSize::new(4, 4),
        vec![TileTerrain::of_save(0, 255, -1); 16],
        vec![TerrainKind {
            shown: true,
            flags: TerrainFlags::LAND,
            forest: false,
        }],
        vec![Restriction::new(vec![0.5])],
    );
    terrain.set_elevations(vec![0; 16]);
    world.spatial.terrain = Some(terrain);
    assert_eq!(defense(&world), Some(0.5));
    world.players[1].objects.iter_mut().for_each(|unit| {
        unit.life.inside = Some(ContainerId::new(object_id(9)));
    });
    assert_eq!(defense(&world), Some(1.0), "inside another");
    world.spatial.terrain = None;
    assert_eq!(
        defense(&world),
        Some(1.0),
        "inside another, without the terrain"
    );
    let mut outside = world_of(object_of(defended));
    outside.spatial.terrain = None;
    assert_eq!(defense(&outside), None);
    assert_eq!(
        defense(&world_of(object_of(guard(Vec::new())))),
        Some(1.0),
        "no restriction"
    );
}

/// Slot 0x1b8, the infantry inside: a combat unit's or a building's count (types 70 and 80); 0
/// for any other (RGE's `FUN_14106a6b0`).
#[test]
fn victim_counts_infantry_inside_only_for_combat_units_and_buildings() {
    let infantry = |kind| {
        let mut world = world_of(object_of(Master {
            kind,
            ..guard(Vec::new())
        }));
        world
            .parts
            .combat_mut(object_id(7))
            .unwrap()
            .infantry_inside = InfantryCount::ZERO.changed(InfantryChange::Entered);
        read(&world, 7).source.infantry_inside.get()
    };
    assert_eq!(
        [
            MasterKind::COMBAT,
            MasterKind::BUILDING,
            MasterKind::RGE_COMBAT
        ]
        .map(infantry),
        [1, 1, 0]
    );
}

/// The hit's object is busy with an action on its list or with notifies the import could not
/// read; idle without either.
#[test]
fn victim_is_busy_with_an_action_or_unread_notifies() {
    let busy = |world: &World| read(world, 7).armored.map(|armored| armored.busy);
    let mut world = world_of(object_of(guard(Vec::new())));
    assert_eq!(busy(&world), Some(false));
    world
        .set_part(
            object_id(7),
            Part::Unread(Some(Unread::new(GenieError::unsupported(
                "a test's notify",
            )))),
        )
        .unwrap();
    assert_eq!(busy(&world), Some(true), "unread notifies");
    let mut actions = Action::default();
    actions
        .list
        .set_attack(Some(own_attack(8, AttackState::Ready, Distance::new(1.0))));
    let attacking = world_of(SceneObject {
        actions: Some(actions),
        ..object_of(guard(Vec::new()))
    });
    assert_eq!(busy(&attacking), Some(true), "an attack");
}

/// The hit reads the master's tasks by their types in their order: its bonus damage (0xa0), a
/// removal reward (0xa1) of a work range above 1e-7 as its kill task, a loot task (0x9a) as its
/// kill bonus, and its stingers (0x9d) that its player's attribute enables.
#[test]
fn victim_reads_the_master_tasks_by_their_kinds() {
    let bonus = |range| task(BONUS_DAMAGE, range);
    let stinger = |enablement| Task {
        attribute_kind2: enablement,
        ..task(STINGER, 0.0)
    };
    let tasks = vec![
        bonus(1.0),
        task(REMOVAL_REWARD, 1e-7),
        stinger(Some(AttributeId::named(250))),
        bonus(2.0),
        stinger(None),
    ];
    let mut world = world_of(object_of(guard(tasks)));
    let victim = read(&world, 7);
    assert_eq!(victim.bonus_tasks, [bonus(1.0), bonus(2.0)]);
    assert_eq!(victim.stinger_tasks.len(), 1, "the enabled stinger");
    assert_eq!(
        (
            victim.armored.map(|armored| armored.kill_tasks),
            victim.source.kill_bonus_tasks
        ),
        (Some(false), false)
    );
    world.players[1].attributes[250] = Amount::new(1.0);
    assert_eq!(read(&world, 7).stinger_tasks.len(), 2);
    let rewarding = world_of(object_of(guard(vec![
        task(REMOVAL_REWARD, 1e-6),
        task(ActionKind::LOOT, 0.0),
    ])));
    let victim = read(&rewarding, 7);
    assert_eq!(
        (
            victim.armored.map(|armored| armored.kill_tasks),
            victim.source.kill_bonus_tasks
        ),
        (Some(true), true)
    );
}

/// A building's read: its annexes (`+0xbc0`..) in their slots and an annex's main building
/// (`+0xbb8`); an object of another type has no building part.
#[test]
fn building_victim_reads_its_annexes_and_an_annex_its_main_building() {
    let building = Master {
        kind: MasterKind::BUILDING,
        ..guard(Vec::new())
    };
    let at = Position::new(1.5, 1.5, 0.0);
    let annex = |id| SceneObject {
        annex_of: Some(MainBuildingId::new(object_id(7))),
        ..SceneObject::new(id, PlayerId::new(1), building.clone(), at)
    };
    let world = Scene::open(GridSize::new(4, 4))
        .players(2)
        .object(SceneObject::new(7, PlayerId::new(1), building.clone(), at))
        .object(annex(21))
        .object(annex(22))
        .build();
    let main = read(&world, 7).building.expect("a building");
    assert_eq!(
        (main.main, main.annexes),
        (None, vec![object_id(21), object_id(22)])
    );
    let annex_part = read(&world, 22).building.expect("a building");
    assert_eq!(
        (annex_part.main, annex_part.annexes),
        (Some(MainBuildingId::new(object_id(7))), Vec::new())
    );
    assert_eq!(
        read(&world_of(object_of(guard(Vec::new()))), 7).building,
        None
    );
}

/// The hit's transforms read each master a transform gives in turn, once: the guard's (4) into
/// 5, 5's into 6 and back into 4, 6's none; each read as the object with that master.
#[test]
fn victim_transforms_read_each_master_they_give_once() {
    let next = |id, tasks| Master {
        id: master_id(id),
        ..guard(tasks)
    };
    let mut world = Scene::open(GridSize::new(4, 4))
        .players(2)
        .object(object_of(next(4, vec![transform_into(5, None, None)])))
        .object(SceneObject::new(
            8,
            PlayerId::new(2),
            next(
                5,
                vec![transform_into(6, None, None), transform_into(4, None, None)],
            ),
            Position::new(2.5, 2.5, 0.0),
        ))
        .object(SceneObject::new(
            9,
            PlayerId::new(2),
            next(6, Vec::new()),
            Position::new(2.5, 1.5, 0.0),
        ))
        .build();
    world.players[1].attributes = vec![Amount::ZERO; 500];
    let victim = read(&world, 7);
    let given = |current, hp| {
        victim
            .transforms
            .first_holding(master_id(current), HitPoints::new(hp))
            .map(|master| (master.id, master.reads.master_id))
    };
    assert_eq!(given(4, 5.0), Some((master_id(5), master_id(5))));
    assert_eq!(given(5, 5.0), Some((master_id(6), master_id(6))));
    assert_eq!(given(6, 5.0), None, "6 has no transform");
    assert_eq!(given(4, 15.0), None, "above the threshold's size");
}

/// A transform whose enablement its player's attribute fails gives no master; an object of a
/// type that runs no transforms (below 70) has none; a transform's attribute its player does
/// not hold fails the read.
#[test]
fn victim_transforms_skip_the_disabled_and_the_kinds_without_them() {
    let gated = vec![transform_into(5, Some(AttributeId::named(250)), None)];
    let into_five = Master {
        id: master_id(5),
        ..guard(Vec::new())
    };
    let world_with = |guard_master: Master| {
        let mut world = Scene::open(GridSize::new(4, 4))
            .players(2)
            .object(object_of(guard_master))
            .object(SceneObject::new(
                8,
                PlayerId::new(2),
                into_five.clone(),
                Position::new(2.5, 2.5, 0.0),
            ))
            .build();
        world.players[1].attributes = vec![Amount::ZERO; 500];
        world
    };
    let mut world = world_with(guard(gated));
    let holding = |world: &World| {
        read(world, 7)
            .transforms
            .first_holding(master_id(4), HitPoints::new(5.0))
            .map(|master| master.id)
    };
    assert_eq!(holding(&world), None, "disabled");
    world.players[1].attributes[250] = Amount::new(1.0);
    assert_eq!(holding(&world), Some(master_id(5)));
    let moving = world_with(Master {
        kind: MasterKind::RGE_COMBAT,
        ..guard(vec![transform_into(5, None, None)])
    });
    assert_eq!(holding(&moving), None, "type 50");
    let lacking = world_with(guard(vec![transform_into(
        5,
        None,
        Some(AttributeId::named(600)),
    )]));
    assert!(lacking.victim(object_id(7), PlayerId::new(2)).is_err());
}
