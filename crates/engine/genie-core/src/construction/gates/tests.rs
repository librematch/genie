//! The gates: whom a gate lets pass, the units between its posts, a player's gate list, and the
//! gates that open for a player's update and close after it.

use crate::construction::gates::{gate_units, lets_pass};
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Position;
use crate::map::Tile;
use crate::map::spatial::{BLOCKED, Obstruction, Placed, Spatial, Tiles};
use crate::movement::Speed;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::BuildableOn;
use crate::objects::masters::BuildingMaster;
use crate::objects::masters::ClassId;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObstructionClass;
use crate::objects::masters::ObstructionKind;
use crate::players::ByPlayer;
use crate::players::PlayerId;
use crate::players::Relation;
use crate::players::relation_row;
use crate::test_ids::{master_id, object_id};
use crate::testing::class;
use crate::testing::ids;
use crate::testing::roles;
use crate::testing::scene::{Scene, SceneObject};
use crate::world::Gate;
use crate::world::World;
use crate::world::objects_ref::ObjectsRef;

/// Player `index`.
const fn player(index: u8) -> PlayerId {
    PlayerId::new(index)
}

/// The record of [`gate_master`]'s gates.
const GATE_RECORD: Obstruction = Obstruction {
    kind: ObstructionKind::KIND_2,
    class: ObstructionClass::GATE,
};

/// A gate's master `id` (class 39, type 80) transforming into `transform_unit` (none: into
/// nothing), 2 high, of radii 1 by 0.5 and of [`GATE_RECORD`]'s obstruction kind and class.
fn gate_master(id: i16, transform_unit: Option<MasterId>) -> Master {
    Master {
        id: master_id(id),
        kind: MasterKind::BUILDING,
        class: class::GATE,
        radius: Footprint::new(1.0, 0.5),
        height: Distance::new(2.0),
        obstruction_kind: GATE_RECORD.kind,
        obstruction_class: GATE_RECORD.class,
        building: Some(BuildingMaster {
            transform_unit,
            ..BuildingMaster::default()
        }),
        ..Master::default()
    }
}

/// The gate 50 of player 2 at (10, 10.5), of master 64 transforming into 63.
const GATE: ObjectId = object_id(50);

/// Player `owner`'s gate `id` of `master` at (`x`, 10.5).
fn gate(id: i32, owner: PlayerId, master: Master, x: f32) -> SceneObject {
    SceneObject::new(id, owner, master, Position::new(x, 10.5, 0.0))
}

/// A 20-tile map of five players and player 2's [`GATE`]: players 1 and 2 count each other
/// allies; player 2 counts player 3 an ally, not player 3 it; player 4 counts player 2 an ally,
/// not player 2 it; players 2 and 5 stand to each other in relation 4 (the stances' while the
/// world's `+0x820` object is in state 1). Every other relation is an enemy's.
fn gate_scene() -> Scene {
    Scene::open(GridSize::new(20, 20))
        .players(5)
        .relation(player(1), player(2), Relation::Ally)
        .relation(player(2), player(1), Relation::Ally)
        .relation(player(2), player(3), Relation::Ally)
        .relation(player(4), player(2), Relation::Ally)
        .relation(player(2), player(5), Relation::Relation4)
        .relation(player(5), player(2), Relation::Relation4)
        .object(gate(
            50,
            player(2),
            gate_master(64, Some(master_id(63))),
            10.0,
        ))
}

/// `FUN_1411d4a30` on [`gate_scene`]'s gate as it stands in `world`: whether it lets a unit of
/// each player pass, Gaia first.
fn passes(world: &World) -> Vec<bool> {
    let (_, gate) = world.object(GATE).unwrap();
    let master = gate.master(world.master_tables(), player(2)).unwrap();
    let layer = world.parts.building_of(GATE);
    PlayerId::each(world.players.len())
        .map(|unit_owner| {
            lets_pass(
                (gate, layer, master),
                player(2),
                unit_owner,
                &world.spatial,
                world.treaty.as_ref(),
                &roles(),
            )
        })
        .collect()
}

/// `FUN_1411d4a30` asks both sides with the ally flag (`FUN_141663dc0` for the gate's owner,
/// `FUN_141663d90` for the unit's player): the gate lets its own player pass, an ally both ways
/// and a player of relation 4 both ways; a player only one side counts an ally, an enemy and
/// Gaia stay out. A foundation (state 0, below 3) lets them pass as a standing gate does.
#[test]
fn gate_lets_pass_player_allied_both_ways() {
    let mut world = gate_scene().build();
    assert_eq!(passes(&world), [false, true, true, false, false, true]);
    world.players[2].objects[ListIndex::new(0)].life.state = ObjectState::Foundation;
    assert_eq!(passes(&world), [false, true, true, false, false, true]);
}

/// A player's gate list (`+0x1360`, `FUN_141616200` adds a new object of class 39 whose master's
/// `transform_unit` is not −1 at its front): its gates whose masters transform, the newest first;
/// a wall and a gate whose master transforms into nothing are not on it.
#[test]
fn gate_list_holds_the_transforming_gates_newest_first() {
    let wall = Master {
        class: class::WALL,
        ..gate_master(117, Some(master_id(63)))
    };
    let world = gate_scene()
        .object(gate(51, player(2), wall, 4.0))
        .object(gate(
            52,
            player(2),
            gate_master(64, Some(master_id(63))),
            14.0,
        ))
        .object(gate(53, player(2), gate_master(65, None), 17.0))
        .build();
    assert_eq!(
        world.gate_list(player(2)),
        Ok(vec![ListIndex::new(2), ListIndex::new(0)])
    );
    assert_eq!(world.gate_list(player(1)), Ok(vec![]));
}

/// Whether the tile under [`GATE`]'s west half is marked blocked.
fn blocked(world: &World) -> bool {
    world
        .spatial
        .tiles()
        .word(Tile::new(9, 10))
        .is_some_and(|word| word & BLOCKED != 0)
}

/// [`GATE`]'s record.
fn record(world: &World) -> Option<Obstruction> {
    world
        .spatial
        .get(GATE)
        .and_then(|placed| placed.obstruction)
}

/// `BaseWorld::update`'s loop before a player's objects (slot 0x318) runs for a player past Gaia
/// alone: Gaia's update opens no gate, and its close after it closes none of those open.
#[test]
fn gaia_update_opens_and_closes_no_gate() {
    let mut world = gate_scene().build();
    let mut open = vec![false; 6];
    world.open_gates(PlayerId::GAIA, &mut open).unwrap();
    assert_eq!(
        (open.as_slice(), record(&world)),
        ([false; 6].as_slice(), Some(GATE_RECORD))
    );
    world.open_gates(player(1), &mut open).unwrap();
    world.close_gates(PlayerId::GAIA, &mut open).unwrap();
    assert_eq!(
        (open.as_slice(), record(&world), blocked(&world)),
        (
            [false, true, true, false, false, false].as_slice(),
            None,
            false
        )
    );
}

/// `BaseWorld::update` opens the gates of the players that count the updating player an ally
/// and whom it counts one (`FUN_141663dc0` both ways), relation 4 too: player 5's update opens
/// player 2's gate; player 3's (whom player 2 counts an ally unanswered) and player 4's (who
/// counts player 2 one unanswered) open only their own gates, of which they have none.
#[test]
fn player_update_opens_only_the_gates_of_players_allied_both_ways() {
    let opened = |updating: u8| {
        let mut world = gate_scene().build();
        let mut open = vec![false; 6];
        world.open_gates(player(updating), &mut open).unwrap();
        (open, record(&world), blocked(&world))
    };
    assert_eq!(
        opened(5),
        (vec![false, false, true, false, false, true], None, false)
    );
    assert_eq!(
        opened(3),
        (
            vec![false, false, false, true, false, false],
            Some(GATE_RECORD),
            true
        )
    );
    assert_eq!(
        opened(4),
        (
            vec![false, false, false, false, true, false],
            Some(GATE_RECORD),
            true
        )
    );
}

/// A gate of obstruction kind 10 opens with its tiles' own marks (`FUN_141055b40`'s
/// `FUN_140f65580`), which the slice does not model.
#[test]
fn gate_of_obstruction_kind_10_opening_is_not_modelled() {
    let mut master = gate_master(64, Some(master_id(63)));
    master.obstruction_kind = ObstructionKind::KIND_10;
    let mut world = Scene::open(GridSize::new(20, 20))
        .object(gate(50, player(1), master, 10.0))
        .build();
    let mut open = vec![false; 2];
    assert_eq!(
        world
            .open_gates(player(1), &mut open)
            .map_err(|error| error.reason()),
        Err("a gate of obstruction kind 10 opens (FUN_140f65580)")
    );
}

/// A player's gates open for the updating player as a whole (slot 0x318 counts them open,
/// `+0x1384`, and so does the update's flag), whether each gate lets it pass or not: a locked
/// gate keeps its record and its marks, and its player counts open, so a later player's opening
/// within the update passes it over.
#[test]
fn locked_gate_keeps_its_record_while_its_player_counts_open() {
    let mut world = gate_scene().build();
    world.parts.building_mut(GATE).unwrap().gate = Some(Gate {
        locked: true,
        ..Gate::default()
    });
    let mut open = vec![false; 6];
    world.open_gates(player(1), &mut open).unwrap();
    assert_eq!(
        (open, record(&world), blocked(&world)),
        (
            vec![false, true, true, false, false, false],
            Some(GATE_RECORD),
            true
        )
    );
}

/// `BaseWorld::update`'s close after the last player (no next player to keep them open for)
/// closes every open player's gates: player 5's own update opened player 2's gate, and its close
/// gives the gate its record and its marks again.
#[test]
fn last_player_close_closes_every_open_gate() {
    let mut world = gate_scene().build();
    let mut open = vec![false; 6];
    world.open_gates(player(5), &mut open).unwrap();
    world.close_gates(player(5), &mut open).unwrap();
    assert_eq!(
        (open, record(&world), blocked(&world)),
        (vec![false; 6], Some(GATE_RECORD), true)
    );
}

/// A gate part of obstruction kind 0 (the gate's middle) gets no record when its gates close
/// (`FUN_141056460`'s first test, [`Obstruction::recorded`]); the gate beside it gets its own
/// back.
#[test]
fn gate_part_of_no_obstruction_kind_gets_no_record_when_closed() {
    let mut middle = gate_master(91, Some(master_id(88)));
    middle.obstruction_kind = ObstructionKind::NONE;
    let mut world = gate_scene()
        .object(gate(54, player(2), middle, 15.0))
        .build();
    let mut open = vec![false; 6];
    world.open_gates(player(2), &mut open).unwrap();
    // player 3, next, counts player 2 an enemy: player 2's gates close
    world.close_gates(player(2), &mut open).unwrap();
    assert_eq!(open, [false; 6]);
    assert_eq!(record(&world), Some(GATE_RECORD));
    assert_eq!(
        world
            .spatial
            .get(object_id(54))
            .map(|placed| placed.obstruction),
        Some(None)
    );
}

/// Player 1's gate 27745 with its posts 27746 at (18.5, 20) and 27747 at (21.5, 20) on a 40-tile
/// map of three players, each the others' enemy.
fn posts() -> Spatial {
    let mut spatial = Spatial::with_tiles(
        GridSize::new(40, 40),
        Tiles::new(GridSize::new(40, 40), vec![1; 1600]),
    );
    spatial.relations = ByPlayer::new(vec![
        relation_row([0, 3, 3]),
        relation_row([3, 0, 3]),
        relation_row([3, 3, 0]),
    ]);
    for (id, x) in [(27745, 20.0), (27746, 18.5), (27747, 21.5)] {
        spatial.insert(
            object_id(id),
            placed(x, class::GATE, player(1), Speed::ZERO),
        );
    }
    spatial
}

/// The record of an object of `class` and `player` at (`x`, 20) moving at `speed`: radii 0.2, a
/// unit's obstruction.
fn placed(x: f32, class: ClassId, player: PlayerId, speed: Speed) -> Placed {
    Placed {
        position: Position::new(x, 20.0, 0.0),
        radius: Footprint::new(0.2, 0.2),
        class,
        player,
        master: ids::ID_0,
        obstruction_kind: ObstructionKind::KIND_2,
        buildable_on: BuildableOn::NO,
        tall: true,
        moves: Speed::ZERO < speed,
        unknown: None,
        obstruction: Some(Obstruction {
            kind: ObstructionKind::KIND_2,
            class: ObstructionClass::UNIT,
        }),
    }
}

/// [`gate_units`] of [`posts`]'s gate.
fn units_in(spatial: &mut Spatial) -> Result<(Vec<ObjectId>, usize), &'static str> {
    gate_units(
        spatial,
        object_id(27745),
        &[object_id(27746), object_id(27747)],
        ObjectsRef::empty(),
    )
    .map_err(|error| error.reason())
}

/// The record of a unit of `class` and `player` at (`x`, 20) moving at `speed` as [`placed`]'s,
/// a circle.
fn circle(x: f32, class: ClassId, player: PlayerId, speed: Speed) -> Placed {
    let kind = ObstructionKind::CIRCLE;
    Placed {
        obstruction_kind: kind,
        obstruction: Some(Obstruction {
            kind,
            class: ObstructionClass::UNIT,
        }),
        ..placed(x, class, player, speed)
    }
}

/// records/425906646's gate 27745 (annexes 27746 and 27747) in update 2865, player 1's as
/// [`posts`] holds it: an ally walking between the posts opens it; a standing Gaia animal does
/// not count, an enemy counts but is no ally, a wall next to it neither.
#[test]
fn gate_counts_the_moving_units_between_its_posts() {
    let mut spatial = posts();
    spatial.insert(
        object_id(1),
        circle(19.6, class::VILLAGER, player(1), Speed::new(0.8)),
    );
    assert_eq!(units_in(&mut spatial), Ok((vec![object_id(1)], 1)));
    spatial.insert(
        object_id(2),
        circle(20.4, class::PREDATOR, PlayerId::GAIA, Speed::ZERO),
    );
    spatial.insert(
        object_id(3),
        circle(20.8, class::CLASS_6, player(2), Speed::new(0.9)),
    );
    spatial.insert(
        object_id(4),
        placed(21.0, class::WALL, player(1), Speed::ZERO),
    );
    assert_eq!(
        units_in(&mut spatial),
        Ok((vec![object_id(1), object_id(3)], 1))
    );
}

/// `FUN_1411d0570`'s count: a unit of a player, standing or not, counts; a building (class 3)
/// only while it moves, a tower (52) never; a moving Gaia unit counts, but is no ally of the
/// gate's player (`FUN_141663d90`).
#[test]
fn gate_counts_moving_buildings_and_gaia_but_no_tower() {
    let mut spatial = posts();
    for (id, x, class, owner, speed) in [
        (1, 19.0, class::VILLAGER, player(1), Speed::ZERO),
        (2, 19.5, class::BUILDING, player(1), Speed::ZERO),
        (3, 20.0, class::BUILDING, player(1), Speed::new(0.5)),
        (4, 20.5, class::TOWER, player(1), Speed::ZERO),
        (5, 21.0, class::PREY, PlayerId::GAIA, Speed::new(1.2)),
    ] {
        spatial.insert(object_id(id), placed(x, class, owner, speed));
    }
    assert_eq!(
        units_in(&mut spatial),
        Ok((vec![object_id(1), object_id(3), object_id(5)], 2))
    );
}

/// The gate's query lists at most 10 records (`FUN_141051900`'s list); one that finds more is
/// not modelled.
#[test]
fn gate_query_of_more_than_ten_records_is_not_modelled() {
    let mut spatial = posts();
    for index in 0..11 {
        spatial.insert(
            object_id(index + 1),
            placed(
                18.75 + 0.25 * index as f32,
                class::VILLAGER,
                player(1),
                Speed::new(0.8),
            ),
        );
    }
    assert_eq!(
        units_in(&mut spatial),
        Err("a gate's query of more than 10 records")
    );
}

/// The query runs from the gate's first post to its second, for the gate's player: a gate
/// without its two annexes (the game then skips the query and reads its counts unset), a post
/// or a gate without a place is not modelled.
#[test]
fn gate_without_its_annexes_or_places_is_not_modelled() {
    let mut spatial = posts();
    assert_eq!(
        gate_units(
            &mut spatial,
            object_id(27745),
            &[object_id(27746)],
            ObjectsRef::empty(),
        )
        .map_err(|error| error.reason()),
        Err("a gate without its two annexes")
    );
    spatial.remove(object_id(27747));
    assert_eq!(
        units_in(&mut spatial),
        Err("a gate's annex without a place")
    );
    let mut spatial = posts();
    spatial.remove(object_id(27745));
    assert_eq!(units_in(&mut spatial), Err("a gate without a place"));
}
