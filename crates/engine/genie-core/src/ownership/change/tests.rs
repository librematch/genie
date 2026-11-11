//! An ownership change: a building's with its annexes, garrison, storage and records, an
//! object's to Gaia, and the attributes and the dead master it takes along.

use super::*;
use crate::commands::{GatherPoint, GatherPointState};
use crate::construction::BuilderCount;
use crate::construction::MainBuildingId;
use crate::construction::build::Construction;
use crate::garrison::ContainerId;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Snow;
use crate::map::Tile;
use crate::movement::Speed;
use crate::objects::MasterHitPoints;
use crate::objects::masters::{BuildingMaster, MasterId, Masters};
use crate::objects::masters::{ObstructionClass, ObstructionKind};
use crate::players::tracker::ListKey;
use crate::technology::{ResearchCap, ResearchState, TechDef, TechState};
use crate::test_ids::object_id;
use crate::test_ids::tech_id;
use crate::testing::attribute;
use crate::testing::class;
use crate::testing::ids;
use crate::testing::scene::{Scene, SceneObject, walking};
use crate::vision::VisionBits;
use crate::vision::ghost::{Cell, Watched};
use crate::world::WholeSeconds;
use std::sync::Arc;

/// The building's master in [`hands_scene`]: the castle's id.
const BUILDING: MasterId = ids::CASTLE;
/// A stack unit, a static object: the market's id.
const STACK: MasterId = ids::MARKET;
/// A villager, a combat object of class 4: the male villager's id.
const VILLAGER: MasterId = ids::VILLAGER_MALE;
/// The master of [`sheep_master`]: the `.dat`'s 705, a livestock master (class 58).
const SHEEP: MasterId = MasterId::named(705);

/// The building's master: a building of footprint 1.5 whose record is a building's, storing wood
/// 5 while built (flag 4), stone 7 for a new owner (flag 8) and gold 3 as any object does
/// (flag 2).
fn building_master() -> Master {
    Master {
        id: BUILDING,
        kind: MasterKind::BUILDING,
        class: class::BUILDING,
        radius: Footprint::new(1.5, 1.5),
        height: Distance::new(2.0),
        line_of_sight: Distance::new(2.0),
        hit_points: MasterHitPoints::of_data(100),
        obstruction_kind: ObstructionKind::KIND_2,
        obstruction_class: ObstructionClass::BUILDING,
        storage: vec![
            (Some(attribute::WOOD), Amount::new(5.0), STORAGE_WHILE_BUILT),
            (Some(attribute::STONE), Amount::new(7.0), STORAGE_8),
            (Some(attribute::GOLD), Amount::new(3.0), 2),
        ],
        building: Some(BuildingMaster::default()),
        ..Master::default()
    }
}

/// A villager of player 1 that stands at `at`.
fn villager(id: i32, at: Position) -> SceneObject {
    SceneObject::new(
        id,
        PlayerId::new(1),
        Master {
            id: VILLAGER,
            kind: MasterKind::COMBAT,
            class: class::VILLAGER,
            radius: Footprint::new(0.2, 0.2),
            height: Distance::new(1.0),
            move_stats: Some(walking(Speed::new(1.0))),
            ..Master::default()
        },
        at,
    )
}

/// A 20 × 20 scene of two players: player 1's `building` (id 50 of [`building_master`] at
/// (10, 10), its owner's tracker filing it) and the scene's `others`; every player holds 300
/// attributes.
fn hands_scene(building: SceneObject, others: Vec<SceneObject>) -> World {
    let master = building.master.id;
    let scene = others.into_iter().fold(
        Scene::open(GridSize::new(20, 20))
            .players(2)
            .object(building),
        Scene::object,
    );
    let mut world = scene.build();
    for player in &mut world.players {
        player.attributes = vec![Amount::ZERO; 300];
    }
    world
        .track_add(PlayerId::new(1), object_id(50), master)
        .unwrap();
    world
}

/// Player 1's building 50 of [`building_master`] changed by `change`, at (10, 10).
fn building(change: impl FnOnce(&mut SceneObject)) -> SceneObject {
    let mut object = SceneObject::new(
        50,
        PlayerId::new(1),
        building_master(),
        Position::new(10.0, 10.0, 0.0),
    );
    change(&mut object);
    object
}

/// Every table of `world` holds `masters` besides its own.
fn with_masters(world: &mut World, masters: &[Master]) {
    for table in &mut world.model.masters {
        *table = Arc::new(Masters::new(
            table.iter().cloned().chain(masters.iter().cloned()),
        ));
    }
}

/// Player `player`'s table of `world` without master `id`.
fn without_master(world: &mut World, player: usize, id: MasterId) {
    let table = &mut world.model.masters[player];
    *table = Arc::new(Masters::new(
        table.iter().filter(|master| master.id != id).cloned(),
    ));
}

/// Building `id`'s construction: built for `progress` seconds of its 10.
fn construction(world: &mut World, id: ObjectId, progress: f32) {
    world.parts.building_mut(id).unwrap().construction = Some(Construction {
        progress: Seconds::new(progress),
        desolid: false,
        builders: BuilderCount::of_data(0),
        hit_points: MasterHitPoints::of_data(100),
        build_time: WholeSeconds::of_data(10),
        record: Obstruction {
            kind: ObstructionKind::KIND_2,
            class: ObstructionClass::BUILDING,
        },
        snow: Snow::None,
        sprite_override: false,
    });
}

/// Player `player`'s attribute `attribute`.
fn attribute_of(world: &World, player: usize, attribute: AttributeId) -> Amount {
    world.players[player].attributes[attribute.index()]
}

/// The ids of player `player`'s live list, in its order.
fn ids(world: &World, player: usize) -> Vec<ObjectId> {
    world.players[player]
        .objects
        .iter()
        .map(|unit| unit.id)
        .collect()
}

/// The owners that object `id`'s records in the sight registry name (`+0x8b0`..`+0x8f0`), and
/// the owner its place names.
fn owners(world: &World, id: i32) -> (Vec<PlayerId>, Option<PlayerId>) {
    let registry = world
        .spatial
        .sight
        .as_ref()
        .and_then(|visibility| visibility.registry.as_ref())
        .unwrap();
    let records = registry
        .lists
        .iter()
        .flatten()
        .flatten()
        .filter(|record| record.id == object_id(id))
        .map(|record| record.owner)
        .collect();
    (
        records,
        world.spatial.get(object_id(id)).map(|placed| placed.player),
    )
}

/// Where player `player`'s tracker files object 50 of master `master`: its done list, else its
/// pending one.
fn filed(world: &World, player: usize, master: MasterId) -> (bool, bool) {
    let tracker = &world.players[player].tracker;
    let held = |key: ListKey| tracker.list(key).contains(&Some(object_id(50)));
    (
        held(ListKey::master(master)),
        held(ListKey::master(master).pending()),
    )
}

/// `FUN_1411cd860` on a built building (state 2): it goes to player 2's list and tracker, done
/// (its build time at most its build points); player 1 loses the storage of flag 4 and, through
/// the combat object's change, of flag 2; player 2 gains those of flags 4 and 8 and of flag 2;
/// the held population headroom (attribute 4) moves too; its gather point goes.
#[test]
fn built_building_moves_to_new_owner_with_its_storage() {
    let mut world = hands_scene(
        building(|object| object.holds = Some((attribute::HEADROOM, Amount::new(10.0)))),
        Vec::new(),
    );
    construction(&mut world, object_id(50), 10.0);
    world
        .set_part(
            object_id(50),
            Part::GatherPoint(Some(GatherPoint {
                state: GatherPointState::Point,
                point: Position::new(5.0, 5.0, 0.0),
                target: None,
                target_master: None,
            })),
        )
        .unwrap();
    world
        .building_changes_hands(PlayerId::new(1), object_id(50), PlayerId::new(2), false)
        .unwrap();
    assert_eq!(
        (ids(&world, 1), ids(&world, 2)),
        (vec![], vec![object_id(50)])
    );
    let resources = |player| {
        [
            attribute::WOOD,
            attribute::STONE,
            attribute::GOLD,
            attribute::HEADROOM,
        ]
        .map(|kind| attribute_of(&world, player, kind))
    };
    assert_eq!(resources(1), [-5.0, 0.0, -3.0, -10.0].map(Amount::new));
    assert_eq!(resources(2), [5.0, 7.0, 3.0, 10.0].map(Amount::new));
    assert_eq!(
        (filed(&world, 1, BUILDING), filed(&world, 2, BUILDING)),
        ((false, false), (true, false))
    );
    assert_eq!(world.parts.gather_point.get(object_id(50)), None);
}

/// `FUN_1411cd860` on a foundation (state 0): no storage of flags 4 and 8 and no held attribute
/// moves, but the combat object's change moves flag 2's; not built, it waits in player 2's
/// pending lists.
#[test]
fn foundation_moves_only_storage_of_any_object() {
    let mut world = hands_scene(
        building(|object| {
            object.state = ObjectState::Foundation;
            object.holds = Some((attribute::HEADROOM, Amount::new(10.0)));
        }),
        Vec::new(),
    );
    construction(&mut world, object_id(50), 2.0);
    world
        .building_changes_hands(PlayerId::new(1), object_id(50), PlayerId::new(2), false)
        .unwrap();
    let resources = |player| {
        [
            attribute::WOOD,
            attribute::STONE,
            attribute::GOLD,
            attribute::HEADROOM,
        ]
        .map(|kind| attribute_of(&world, player, kind))
    };
    assert_eq!(resources(1), [0.0, 0.0, -3.0, 0.0].map(Amount::new));
    assert_eq!(resources(2), [0.0, 0.0, 3.0, 0.0].map(Amount::new));
    assert_eq!(filed(&world, 2, BUILDING), (false, true));
}

/// `FUN_1411cd860`'s completion in the new owner's tracker tests the build points against the
/// build time (`COMISS`): build points of NaN do not complete it.
#[test]
fn building_of_nan_build_points_waits_in_new_tracker() {
    let mut world = hands_scene(building(|_| {}), Vec::new());
    construction(&mut world, object_id(50), f32::NAN);
    world
        .building_changes_hands(PlayerId::new(1), object_id(50), PlayerId::new(2), false)
        .unwrap();
    assert_eq!(filed(&world, 2, BUILDING), (false, true));
}

/// Without its master in the new owner's table the combat object's change (`FUN_14120f800`)
/// gives its records to the new owner and stops: the building stays player 1's, which loses the
/// storage of flag 4 and gains back those of flags 4 and 8. Built, it completes in player 2's
/// tracker all the same (`FUN_1411cd860` tests the build points alone), whose key of a master
/// player 2 lacks does not wait (`FUN_14104eac0`): it stays pending in player 1's.
#[test]
fn building_new_owner_lacks_master_of_stays_with_old_owner() {
    let mut world = hands_scene(
        building(|object| object.seen_by = Some(VisionBits::visible(PlayerId::new(1)))),
        Vec::new(),
    );
    construction(&mut world, object_id(50), 10.0);
    without_master(&mut world, 2, BUILDING);
    world
        .building_changes_hands(PlayerId::new(1), object_id(50), PlayerId::new(2), false)
        .unwrap();
    assert_eq!(
        (ids(&world, 1), ids(&world, 2)),
        (vec![object_id(50)], vec![])
    );
    assert_eq!(
        [attribute::WOOD, attribute::STONE, attribute::GOLD]
            .map(|kind| attribute_of(&world, 1, kind)),
        [0.0, 7.0, 0.0].map(Amount::new)
    );
    assert_eq!(
        owners(&world, 50),
        (vec![PlayerId::new(2)], Some(PlayerId::new(1)))
    );
    assert_eq!(
        (filed(&world, 1, BUILDING), filed(&world, 2, BUILDING)),
        ((false, true), (false, false))
    );
}

/// [`hands_scene`] whose building's master makes a stack unit `stack` (`+0x986`) and researches
/// technology 0 (`+0x98c`) when built, each player's tree holding it available.
fn stack_scene(id: MasterId, stack: MasterId) -> World {
    let mut world = hands_scene(
        building(|object| {
            object.master.id = id;
            if let Some(building_master) = object.master.building.as_mut() {
                building_master.stack_unit = Some(stack);
                building_master.tech = Some(tech_id(0));
            }
        }),
        Vec::new(),
    );
    let stacked = |id| Master {
        id,
        kind: MasterKind::STATIC,
        class: class::CLASS_0,
        ..Master::default()
    };
    let mangrove = world.model.slots.master.pasture_mangrove;
    with_masters(&mut world, &[stacked(STACK), stacked(mangrove)]);
    world.model.techs = Arc::new(vec![TechDef::default()]);
    for player in &mut world.players {
        player.techs = vec![TechState {
            state: ResearchState::Available,
            research_cap: ResearchCap::of_data(-1),
            building_master: None,
            ..TechState::default()
        }];
    }
    world
}

/// The masters of the objects player `player` holds after building 50.
fn made_masters(world: &World, player: usize) -> Vec<MasterId> {
    world.players[player]
        .objects
        .iter()
        .filter(|unit| unit.id != object_id(50))
        .map(|unit| unit.life.master_id)
        .collect()
}

/// `FUN_1411cd860` on a built building: its master's stack unit comes at its place for the new
/// owner (the master's slot 0x40) and the new owner researches its master's technology
/// (`FUN_1413cdcd0`).
#[test]
fn built_building_makes_stack_unit_and_research_for_new_owner() {
    let mut world = stack_scene(BUILDING, STACK);
    world
        .building_changes_hands(PlayerId::new(1), object_id(50), PlayerId::new(2), false)
        .unwrap();
    assert_eq!(
        (made_masters(&world, 1), made_masters(&world, 2)),
        (vec![], vec![STACK])
    );
    let stack = world.players[2]
        .objects
        .iter()
        .find(|unit| unit.life.master_id == STACK)
        .unwrap();
    assert_eq!(stack.position(), Position::new(10.0, 10.0, 0.0));
    assert_eq!(
        (
            world.players[1].techs[0].state,
            world.players[2].techs[0].state
        ),
        (ResearchState::Available, ResearchState::Researched)
    );
}

/// A pasture (master 1889, `0x67fd`) on rice terrain (`+0xcd0`) makes the mangrove's stack unit
/// (1893, `0x765`) instead of its own; off rice it makes its own.
#[test]
fn pasture_on_rice_makes_mangrove_stack_unit() {
    let made = |rice| {
        let mut world = stack_scene(ids::PASTURE_1889, STACK);
        world.parts.building_mut(object_id(50)).unwrap().rice = rice;
        world
            .building_changes_hands(PlayerId::new(1), object_id(50), PlayerId::new(2), false)
            .unwrap();
        made_masters(&world, 2)
    };
    assert_eq!(made(true), [ids::PASTURE_MANGROVE]);
    assert_eq!(made(false), [STACK]);
}

/// `FUN_1411cd860` changes its annexes of the building type first, each out of its old owner's
/// watched list (`+0xca4`, `FUN_141091310`) and into no other; the building itself, watched,
/// leaves player 1's watched list and joins player 2's at its tile (`FUN_141090430`).
#[test]
fn annexes_change_hands_first_leaving_watched_list() {
    let mut world = hands_scene(
        building(|_| {}),
        vec![SceneObject {
            annex_of: Some(MainBuildingId::new(object_id(50))),
            ..SceneObject::new(
                51,
                PlayerId::new(1),
                building_master(),
                Position::new(12.0, 10.0, 0.0),
            )
        }],
    );
    world.players[1].watched = [50, 51]
        .map(|id| Watched {
            id: object_id(id),
            cell: Cell::Tile(0),
            bits: VisionBits::NONE,
        })
        .to_vec();
    world
        .building_changes_hands(PlayerId::new(1), object_id(50), PlayerId::new(2), true)
        .unwrap();
    assert_eq!(
        (ids(&world, 1), ids(&world, 2)),
        (vec![], vec![object_id(51), object_id(50)])
    );
    let watched = |player: usize| {
        world.players[player]
            .watched
            .iter()
            .map(|watched| watched.id)
            .collect::<Vec<_>>()
    };
    assert_eq!((watched(1), watched(2)), (vec![], vec![object_id(50)]));
}

/// An annex that is no building (`+0xf` not `'P'`) does not change hands with its main
/// building.
#[test]
fn annex_not_building_stays_with_old_owner() {
    let mut world = hands_scene(
        building(|_| {}),
        vec![SceneObject {
            annex_of: Some(MainBuildingId::new(object_id(50))),
            ..SceneObject::new(
                51,
                PlayerId::new(1),
                Master {
                    id: STACK,
                    kind: MasterKind::STATIC,
                    class: class::CLASS_0,
                    ..Master::default()
                },
                Position::new(12.0, 10.0, 0.0),
            )
        }],
    );
    world
        .building_changes_hands(PlayerId::new(1), object_id(50), PlayerId::new(2), false)
        .unwrap();
    assert_eq!(
        (ids(&world, 1), ids(&world, 2)),
        (vec![object_id(51)], vec![object_id(50)])
    );
}

/// A building asleep cannot change hands: its place is no live one.
#[test]
fn asleep_building_does_not_change_hands() {
    let mut world = hands_scene(building(|_| {}), Vec::new());
    let asleep = world
        .swap_remove_live(PlayerId::new(1), ListIndex::FIRST)
        .unwrap();
    let _ = world.push_asleep(PlayerId::new(1), asleep).unwrap();
    assert_eq!(
        world.building_changes_hands(PlayerId::new(1), object_id(50), PlayerId::new(2), false),
        Err(HANDS_LEFT)
    );
}

/// Below state 3 the building's record goes and comes again (`FUN_141055b40`,
/// `FUN_141056460`), as its master has it; from state 3 on the records stay as they are.
#[test]
fn building_below_state_3_takes_its_record_again() {
    let record = |state| {
        let mut world = hands_scene(building(|object| object.state = state), Vec::new());
        world.spatial.drop_record(object_id(50));
        world
            .building_changes_hands(PlayerId::new(1), object_id(50), PlayerId::new(2), false)
            .unwrap();
        world
            .spatial
            .get(object_id(50))
            .and_then(|placed| placed.obstruction)
    };
    assert_eq!(
        record(ObjectState::Alive),
        Some(Obstruction {
            kind: ObstructionKind::KIND_2,
            class: ObstructionClass::BUILDING,
        })
    );
    assert_eq!(record(ObjectState::Dying), None);
}

/// [`World::record_again`] of a master whose record is of class 0 (`FUN_141056460`'s first test)
/// adds no record.
#[test]
fn record_again_of_unrecorded_master_adds_none() {
    let mut world = hands_scene(building(|_| {}), Vec::new());
    world.spatial.drop_record(object_id(50));
    let master = Master {
        obstruction_class: ObstructionClass::NONE,
        ..building_master()
    };
    world.record_again(object_id(50), &master);
    assert_eq!(
        world
            .spatial
            .get(object_id(50))
            .and_then(|placed| placed.obstruction),
        None
    );
}

/// The units inside the building leave it toward no point (`FUN_1411e7220` with −1, −1), from
/// the last, and stay their owner's.
#[test]
fn garrison_leaves_building_that_changes_hands() {
    let mut world = hands_scene(
        building(|_| {}),
        vec![
            villager(60, Position::new(10.0, 10.0, 0.0)),
            villager(61, Position::new(10.0, 10.0, 0.0)),
        ],
    );
    for id in [60, 61] {
        world.object_mut(object_id(id)).unwrap().1.life.inside =
            Some(ContainerId::new(object_id(50)));
        world
            .spatial
            .go_inside(object_id(id), Position::new(10.0, 10.0, 0.0));
        world
            .object_mut(object_id(50))
            .unwrap()
            .1
            .garrison
            .push(object_id(id));
    }
    world
        .building_changes_hands(PlayerId::new(1), object_id(50), PlayerId::new(2), false)
        .unwrap();
    let out = |id| {
        let (player, unit) = world.object(object_id(id)).unwrap();
        (player, unit.life.inside)
    };
    assert_eq!(
        (out(60), out(61)),
        ((PlayerId::new(1), None), (PlayerId::new(1), None))
    );
    assert_eq!(world.object(object_id(50)).unwrap().1.garrison, Vec::new());
}

/// A foundation's footprint sight (state 0) leaves the old owner's map (slot 0x1d8) and comes on
/// the new owner's (slot 0x1d0).
#[test]
fn foundation_sight_moves_to_new_owner_map() {
    let mut world = hands_scene(
        building(|object| {
            object.state = ObjectState::Foundation;
            object.seen_by = Some(VisionBits::NONE);
        }),
        Vec::new(),
    );
    let placed = world.spatial.get(object_id(50)).copied();
    world.foundation_sight_on(object_id(50), placed).unwrap();
    let seen = |world: &World, player: usize| {
        let visibility = world.spatial.sight.as_ref().unwrap();
        let tile = GridSize::new(20, 20).index(Tile::new(10, 10)).unwrap();
        visibility.maps[player].counts[tile]
    };
    assert_eq!((seen(&world, 1), seen(&world, 2)), (1, 0));
    world
        .building_changes_hands(PlayerId::new(1), object_id(50), PlayerId::new(2), false)
        .unwrap();
    assert_eq!((seen(&world, 1), seen(&world, 2)), (0, 1));
}

/// A sheep's master (705, class 58, line of sight 3): storage flags 1, 4, 8, 16 and 32 on food,
/// wood, stone, gold and attribute 4; its death spawns master `dead`.
fn sheep_master(dead: Option<MasterId>) -> Master {
    Master {
        id: SHEEP,
        kind: MasterKind::COMBAT,
        class: class::LIVESTOCK,
        radius: Footprint::new(0.2, 0.2),
        height: Distance::new(1.0),
        line_of_sight: Distance::new(3.0),
        death_spawns: [dead, None],
        storage: vec![
            (Some(attribute::FOOD), Amount::new(1.0), 1),
            (Some(attribute::WOOD), Amount::new(2.0), 4),
            (Some(attribute::STONE), Amount::new(3.0), 8),
        ],
        ..Master::default()
    }
}

/// A 20 × 20 scene of two players: player 1's sheep 9 of `master` at (5, 5), in the sight
/// registry's list of player 1, and its object 10 after it.
fn sheep_scene(master: Master) -> World {
    let mut world = Scene::open(GridSize::new(20, 20))
        .players(2)
        .object(SceneObject {
            seen_by: Some(VisionBits::visible(PlayerId::new(1))),
            ..SceneObject::new(
                9,
                PlayerId::new(1),
                master.clone(),
                Position::new(5.0, 5.0, 0.0),
            )
        })
        .object(SceneObject::new(
            10,
            PlayerId::new(1),
            Master {
                id: STACK,
                kind: MasterKind::STATIC,
                class: class::CLASS_0,
                ..Master::default()
            },
            Position::new(7.0, 5.0, 0.0),
        ))
        .build();
    for player in &mut world.players {
        player.attributes = vec![Amount::ZERO; 300];
    }
    world
        .track_add(PlayerId::new(1), object_id(9), master.id)
        .unwrap();
    world
}

/// [`World::give_gaia`]: once Gaia has its master the object leaves its owner's list, the list's
/// last object taking its place, for the end of Gaia's, with its trackers; the old owner loses no
/// storage of flags 1, 4 and 8, and Gaia gains flag 1's.
#[test]
fn object_given_to_gaia_joins_gaia_list_end() {
    let mut world = sheep_scene(sheep_master(None));
    world.give_gaia(PlayerId::new(1), ListIndex::FIRST).unwrap();
    assert_eq!(
        (ids(&world, 1), ids(&world, 0)),
        (vec![object_id(10)], vec![object_id(9)])
    );
    let key = |world: &World, player: usize| {
        world.players[player]
            .tracker
            .list(ListKey::master(SHEEP))
            .to_vec()
    };
    assert_eq!(
        (key(&world, 1), key(&world, 0)),
        (vec![], vec![Some(object_id(9))])
    );
    assert_eq!(
        [0, 1].map(
            |player| [attribute::FOOD, attribute::WOOD, attribute::STONE]
                .map(|kind| attribute_of(&world, player, kind))
        ),
        [
            [1.0, 0.0, 0.0].map(Amount::new),
            [0.0, 0.0, 0.0].map(Amount::new)
        ]
    );
    assert_eq!(
        world.spatial.get(object_id(9)).map(|placed| placed.player),
        Some(PlayerId::GAIA)
    );
}

/// Without its master in Gaia's table [`World::give_gaia`] gives Gaia only its records: the
/// object stays where it is in its owner's list, with no dead master filed.
#[test]
fn object_gaia_lacks_master_of_stays_with_owner() {
    let dead = Master {
        id: STACK,
        kind: MasterKind::STATIC,
        ..Master::default()
    };
    let mut world = sheep_scene(sheep_master(Some(dead.id)));
    without_master(&mut world, 0, SHEEP);
    world.give_gaia(PlayerId::new(1), ListIndex::FIRST).unwrap();
    assert_eq!(
        (ids(&world, 1), ids(&world, 0)),
        (vec![object_id(9), object_id(10)], vec![])
    );
    assert_eq!(
        owners(&world, 9),
        (vec![PlayerId::GAIA], Some(PlayerId::new(1)))
    );
    assert_eq!(world.parts.dead_master.get(object_id(9)), None);
}

/// The object's death spawn (`+0x5e4`) of a static type is copied from its old owner's table and
/// filed as its dead master once it changed hands (`FUN_14120f800`'s switch, `param_1[0xd7]`).
#[test]
fn object_given_to_gaia_files_copy_of_its_death_spawn() {
    let mut world = sheep_scene(sheep_master(Some(STACK)));
    world.give_gaia(PlayerId::new(1), ListIndex::FIRST).unwrap();
    assert_eq!(
        world
            .parts
            .dead_master
            .get(object_id(9))
            .map(|master| master.id),
        Some(STACK)
    );
}

/// The combat object's change sees the new owner's map only for an object in state 2 outside
/// any other (slots 0x1d8 and 0x1d0 behind `state == 2 && +0x2f0 == 0`): a dying sheep and one
/// inside another change hands without sight on Gaia's map.
#[test]
fn object_dying_or_inside_changes_hands_blind() {
    let gaia_sight = |change: fn(&mut SceneObject)| {
        let mut sheep = SceneObject::new(
            9,
            PlayerId::new(1),
            sheep_master(None),
            Position::new(5.0, 5.0, 0.0),
        );
        sheep.seen_by = Some(VisionBits::NONE);
        change(&mut sheep);
        let mut world = Scene::open(GridSize::new(20, 20)).object(sheep).build();
        for player in &mut world.players {
            player.attributes = vec![Amount::ZERO; 300];
        }
        world.give_gaia(PlayerId::new(1), ListIndex::FIRST).unwrap();
        let tile = GridSize::new(20, 20).index(Tile::new(5, 5)).unwrap();
        world.spatial.sight.as_ref().unwrap().maps[0].counts[tile]
    };
    assert_eq!(gaia_sight(|_| {}), 1);
    assert_eq!(gaia_sight(|sheep| sheep.state = ObjectState::Dying), 0);
    assert_eq!(
        gaia_sight(|sheep| sheep.inside = Some(ContainerId::new(object_id(77)))),
        0
    );
}

/// [`World::hand_over_attributes`] by the storage's flags: the old owner loses flags 2, 16 and
/// 32 (`FUN_1411a56c0` with 1), the new owner gains flags 1, 2 and 32 (`FUN_141193860`); flags
/// 4 and 8, a building's, move with neither.
#[test]
fn hand_over_moves_storage_by_its_flags() {
    let mut world = sheep_scene(sheep_master(None));
    let own = Master {
        storage: [1, 2, 4, 8, 16, 32]
            .into_iter()
            .zip(0..)
            .map(|(flag, kind)| (Some(AttributeId::named(kind)), Amount::new(1.0), flag))
            .collect(),
        ..sheep_master(None)
    };
    world
        .hand_over_attributes(PlayerId::new(1), PlayerId::new(2), Some(&own))
        .unwrap();
    let moved = |player| {
        (0..6)
            .map(|kind| attribute_of(&world, player, AttributeId::named(kind)).get())
            .collect::<Vec<_>>()
    };
    assert_eq!(moved(1), [0.0, -1.0, 0.0, 0.0, -1.0, -1.0]);
    assert_eq!(moved(2), [1.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
}

/// [`dead_master_of`] reads the death spawn of the object's own master before its old owner's
/// table's, and makes none for an old owner the tables do not hold.
#[test]
fn dead_master_reads_own_master_first() {
    let mut world = sheep_scene(sheep_master(None));
    world
        .set_part(
            object_id(9),
            Part::OwnMaster(Some(Box::new(sheep_master(Some(STACK))))),
        )
        .unwrap();
    let unit = world.object(object_id(9)).unwrap().1.clone();
    let masters = MasterTables::new(&world.model.masters, &world.parts.own_master);
    assert_eq!(
        dead_master_of(&unit, 1, masters, None).map(|master| master.id),
        Some(STACK)
    );
    assert_eq!(dead_master_of(&unit, 5, masters, None), None);
}

/// The combat object's change to an owner whose table lacks its master gives the records the
/// new owner and changes nothing else: no master of its own, no notify.
#[test]
fn change_to_owner_lacking_master_moves_records_only() {
    let mut world = sheep_scene(sheep_master(None));
    without_master(&mut world, 2, SHEEP);
    let mut unit = world.take_out(PlayerId::new(1), ListIndex::FIRST).unwrap();
    let mut actions = Action::default();
    let changed = change_hands(
        (&mut unit, &mut actions, None),
        (1, PlayerId::new(2)),
        MasterTables::new(&world.model.masters, &world.parts.own_master),
        &mut world.spatial,
        world.world_time,
        &world.model.roles,
        world.model.tuning.visibility,
    );
    assert_eq!(changed, Ok(None));
    assert_eq!(
        owners(&world, 9),
        (vec![PlayerId::new(2)], Some(PlayerId::new(1)))
    );
}
