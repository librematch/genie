use super::*;
use crate::GenieResult;
use crate::commands::GatherPoint;
use crate::commands::GatherPointState;
use crate::construction::MainBuildingId;
use crate::gathering::hunt::tests::walker;
use crate::map::Distance;
use crate::map::GridSize;
use crate::map::GroundOffset;
use crate::map::Radii;
use crate::map::Restriction;
use crate::map::TerrainFlags;
use crate::map::TerrainId;
use crate::map::Tile;
use crate::map::TileOffset;
use crate::map::TileRect;
use crate::map::spatial::Placed;
use crate::map::spatial::TileSight;
use crate::movement::Speed;
use crate::movement::move_to::TargetMoved;
use crate::objects::HitPoints;
use crate::objects::ListIndex;
use crate::objects::MasterHitPoints;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::SpeedMultiplier;
use crate::objects::SpriteFlags;
use crate::objects::SpriteId;
use crate::objects::UpdateCode;
use crate::objects::Voice;
use crate::objects::facing::{SpriteFacets, angle_of_facet};
use crate::objects::layers::Static;
use crate::objects::masters::AnimatedPart;
use crate::objects::masters::BuildableOn;
use crate::objects::masters::FogVisibility;
use crate::objects::masters::HillMode;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObstructionClass;
use crate::objects::masters::ObstructionKind;
use crate::objects::masters::PlacementTerrains;
use crate::objects::masters::RecycleMode;
use crate::objects::masters::{Master, Masters};
use crate::objects::sprite::Graphic;
use crate::objects::unit::Unit;
use crate::objects::{FacetCount, Frame};
use crate::orders::OrderSource;
use crate::orders::town_bell::BellShelter;
use crate::players::Amount;
use crate::players::PlayerKind;
use crate::production::spawn::NearFlags;
use crate::production::spawn::PlacementRadii;
use crate::production::spawn::SiteChecks;
use crate::production::spawn::ZoneTest;
use crate::production::spawn::new_unit::SpawnOutcome;
use crate::production::spawn::objects::RECYCLABLE_NEW_OBJECT;
use crate::production::spawn::placement_test::UNLISTED_TERRAIN;
use crate::production::{Entry, MakeObj, MakeObjState, Making, Production, SPOT_RETRY};
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::test_ids::player_id;
use crate::testing::attribute;
use crate::testing::class;
use crate::testing::ids;
use crate::testing::roles;
use crate::testing::scene::{Scene, SceneObject, moving_master, walking};
use crate::testing::tuning;
use crate::unit_ai::AiAction;
use crate::unit_ai::Issuer;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::OrderKind;
use crate::unit_ai::UnitAi;
use crate::unit_ai::UnitAiKind;
use crate::unit_ai::{OrderPriority, QueuedOrder};
use crate::vision::VisionBits;
use crate::world::FeatureFlags;
use crate::world::Millis;
use crate::world::Seconds;
use crate::world::WorldTime;
use crate::world::parts::Part;
use crate::world::tests::forage_world;
use crate::world::{FrameSeed, Player};
use std::sync::Arc;

/// records/425906646's update 1883: a TC's gather point on a tree that is gone; its new
/// villager takes the closest tree of Gaia's (by centre) within its sight of the point,
/// measured from the tree's footprint, else none (it walks to the point).
#[test]
fn gather_point_whose_tree_is_gone_takes_the_closest_tree_within_sight() {
    let unit = |id, master, x: f32, y: f32| Unit {
        id,
        place: Static::at(Position::new(x, y, 0.0)),
        life: Lifecycle {
            master_id: master,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let mut world = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            Player {
                sleeping: vec![
                    unit(object_id(10), master_id(349), 20.5, 20.5),
                    unit(object_id(11), master_id(349), 22.5, 20.5),
                ]
                .into(),
                ..Player::default()
            },
            Player {
                objects: vec![unit(object_id(99), ids::VILLAGER_MALE, 18.0, 18.0)].into(),
                ..Player::default()
            },
        ],
    );
    let tree = Master {
        id: master_id(349),
        class: class::TREE,
        radius: Footprint::new(0.5, 0.5),
        ..Master::default()
    };
    let villager = Master {
        id: ids::VILLAGER_MALE,
        class: class::VILLAGER,
        line_of_sight: Distance::new(4.0),
        ..Master::default()
    };
    world.model.masters = vec![
        Arc::new(Masters::new([tree.clone()])),
        Arc::new(Masters::new([villager, tree])),
    ];
    let gather_point = GatherPoint {
        state: GatherPointState::Point,
        point: Position::new(21.0, 21.0, 0.0),
        target: Some(object_id(5)),
        target_master: Some(master_id(349)),
    };
    assert_eq!(
        world
            .lost_gather_target(PlayerId::new(1), ListIndex::new(0), gather_point)
            .unwrap(),
        Some(object_id(10))
    );
    // 7 tiles from the closest tree's footprint, beyond the villager's sight of 4
    let far = GatherPoint {
        point: Position::new(30.0, 21.0, 0.0),
        ..gather_point
    };
    assert_eq!(
        world
            .lost_gather_target(PlayerId::new(1), ListIndex::new(0), far)
            .unwrap(),
        None
    );
}

/// The hill test (mode `mode`) of tile `at` in the box whose corner is (2, 5), the corner's
/// elevation 3, on a 10 by 10 map at elevation 3 with the tiles `raised` at their own: the
/// tile's shape, as the map's load makes it of the elevations about it, and whether it fails.
fn hill_test(mode: u8, at: [i32; 2], raised: &[([i32; 2], u8)]) -> (u8, bool) {
    let scene = raised.iter().fold(
        Scene::open(GridSize::new(10, 10)).elevation(3),
        |scene, &(tile, elevation)| scene.elevation_on([Tile::from(tile)], elevation),
    );
    let world = scene.build();
    let map = WorldMap { turns: &world };
    let tile = Tile::from(at);
    let shape = map.shape(tile).unwrap();
    let fails = hill_fails(
        HillMode::of_data(mode),
        &map,
        tile,
        Tile::new(2, 5),
        Some(3),
    )
    .unwrap();
    (shape, fails)
}

/// Mode 1 takes flat ground and the four side slopes (shapes 5 to 8) but no saddle or corner,
/// mode 2 flat ground alone, mode 3 a tile of the box's first column or last row within one
/// step of the corner's elevation; the other modes take any tile.
#[test]
fn hill_mode_takes_flat_ground_side_slopes_or_an_edge_within_one_step() {
    let at = [3, 3];
    // mode 1: flat, the tile west of it one step up (5), the one east (8); a saddle (2: north
    // east up, west and south down) and a corner (9: south west up) fail
    assert_eq!(hill_test(1, at, &[]), (0, false));
    assert_eq!(hill_test(1, at, &[([2, 3], 4)]), (5, false));
    assert_eq!(hill_test(1, at, &[([4, 3], 4)]), (8, false));
    assert_eq!(
        hill_test(1, at, &[([4, 2], 4), ([2, 3], 2), ([3, 4], 2)]),
        (2, true)
    );
    assert_eq!(hill_test(1, at, &[([2, 4], 4)]), (9, true));
    // mode 2: flat alone; north up (6) fails
    assert_eq!(hill_test(2, at, &[]), (0, false));
    assert_eq!(hill_test(2, at, &[([3, 2], 4)]), (6, true));
    // mode 3: (2, 3) on the first column within one step passes, two steps up fails; (4, 5) on
    // the last row two steps down fails; (4, 4) on neither passes at any elevation
    assert!(!hill_test(3, [2, 3], &[([2, 3], 4)]).1);
    assert!(hill_test(3, [2, 3], &[([2, 3], 5)]).1);
    assert!(hill_test(3, [4, 5], &[([4, 5], 1)]).1);
    assert!(!hill_test(3, [4, 4], &[([4, 4], 7)]).1);
    // the other modes take any tile: a corner (12: south east up) at elevation 9
    assert_eq!(hill_test(0, at, &[([3, 3], 9), ([4, 4], 10)]), (12, false));
}

/// MarvinBlue/426048731's TC 9149 at (104, 47), radii 2: its gather point (107.2451,
/// 46.075863) lies right and below, so the right side's lower half (6) comes first among the
/// sides the first pass tries; the villager (radius 0.2) steps by 0.5 from (106.5, 47.5).
/// The sheep 9520 at (106.68, 47.15) blocks the first two candidates and stays (livestock in
/// state 2), so each ends its spot; (106.5, 46.5) is free.
#[test]
fn the_tc_spawns_its_villager_below_the_sheep_on_its_right() {
    let site = Site {
        position: Position::new(104.0, 47.0, 0.0),
        radius: Footprint::new(2.0, 2.0),
        goal: Some(Point::new(107.2451, 46.075_863)),
        walls: [false; 4],
        closest_goal: None,
    };
    let mut tried = Vec::new();
    let spawn_point = spawn_point(
        &site,
        Footprint::new(0.2, 0.2),
        &mut |point, _| {
            tried.push(point);
            Ok(if point.y() > 46.9 {
                Tested::Blocked { stays: true }
            } else {
                Tested::Free
            })
        },
        &|_| Ok(None),
        &tuning().spawn,
    )
    .unwrap();
    assert_eq!(spawn_point, Some(Point::new(106.5, 46.5)));
    assert_eq!(
        tried,
        [
            Point::new(106.5, 47.5),
            Point::new(106.5, 47.0),
            Point::new(106.5, 46.5)
        ]
    );
}

#[test]
fn blocker_that_moves_lets_the_spot_try_its_next_candidate() {
    let spot = Spot {
        at: Point::new(1.0, 1.0),
        step: GroundOffset::new(0.5, 0.0),
        dir: TileOffset::new(1, 0),
        count: 1,
    };
    let mut calls = 0;
    let point = try_spot(&spot, &mut |_| {
        calls += 1;
        Ok(if calls == 1 {
            Tested::Blocked { stays: false }
        } else {
            Tested::Free
        })
    })
    .unwrap();
    assert_eq!(point, Some(Point::new(1.5, 1.0)));
}

fn near_sheep(state: ObjectState) -> Near {
    Near {
        id: object_id(9520),
        position: Position::new(106.68481, 47.14652, 0.0),
        radius: Footprint::new(0.35, 0.35),
        flags: NearFlags::TALL | NearFlags::MOVING,
        kind: MasterKind::COMBAT,
        class: class::LIVESTOCK,
        master: master_id(705),
        buildable_on: BuildableOn::NO,
        obstruction_kind: ObstructionKind::CIRCLE,
        state,
        player: PlayerId::new(0),
        fog_visibility: FogVisibility::INVISIBLE,
        fog_mask: None,
        hp: HitPoints::new(7.0),
        outline: Radii::ZERO,
        main_building: None,
    }
}

#[test]
fn livestock_stays_alive_tree_only_once_dead() {
    assert!(near_sheep(ObjectState::Alive).stays(&roles()));
    assert!(!near_sheep(ObjectState::Unfinished).stays(&roles()));
    let tree = Near {
        class: class::TREE,
        kind: MasterKind::STATIC,
        ..near_sheep(ObjectState::Alive)
    };
    assert!(!tree.stays(&roles()));
    assert!(
        Near {
            state: ObjectState::Dying,
            ..tree
        }
        .stays(&roles())
    );
}

/// The sheep 9520's master (705): livestock of radii 0.35 that walks, 7 hit points.
fn sheep_master() -> Master {
    Master {
        id: master_id(705),
        kind: MasterKind::COMBAT,
        class: class::LIVESTOCK,
        radius: Footprint::new(0.35, 0.35),
        height: Distance::new(0.5),
        obstruction_kind: ObstructionKind::CIRCLE,
        fog_visibility: FogVisibility::INVISIBLE,
        hit_points: MasterHitPoints::of_data(7),
        ..moving_master(walking(Speed::new(0.7)))
    }
}

/// Gaia's sheep 9520 at (106.68, 47.15), alive: the world's object that reads as
/// [`near_sheep`].
fn sheep() -> SceneObject {
    SceneObject::new(
        9520,
        PlayerId::GAIA,
        sheep_master(),
        Position::new(106.68481, 47.14652, 0.0),
    )
}

/// A house's master: a building of radii 1 and obstruction type 2, without a fog visibility.
fn house_master() -> Master {
    Master {
        id: ids::HOUSE,
        kind: MasterKind::BUILDING,
        class: class::BUILDING,
        radius: Footprint::new(1.0, 1.0),
        height: Distance::new(2.0),
        obstruction_kind: ObstructionKind::KIND_2,
        obstruction_class: ObstructionClass::UNIT,
        fog_visibility: FogVisibility::INVISIBLE,
        hit_points: MasterHitPoints::of_data(550),
        ..Master::default()
    }
}

/// In the fog, of the objects without a fog visibility only the player's own block a BUILD,
/// and another player's building whose registry mask meets both the player's masks: player
/// 1 explored the map and sees none of it; player 2's house 500 at (20.5, 20.5) was last seen
/// by the players of its mask.
#[test]
fn fogged_building_blocks_build_by_its_visibility_mask() {
    let site = BuildSite {
        player: PlayerId::new(1),
        checks: SiteChecks::HUMAN,
        radius: Footprint::new(1.0, 1.0),
        tall: false,
        class: class::BUILDING,
        obstructs: true,
        restriction: &Restriction::new(vec![1.0, 0.0]),
        hill_mode: HillMode::ANY,
        placement: PlacementTerrains::NONE,
        passed: None,
    };
    let test = |owner, seen_by| {
        let house = SceneObject {
            seen_by,
            ..SceneObject::new(500, owner, house_master(), Position::new(20.5, 20.5, 0.0))
        };
        let world = Scene::open(GridSize::new(120, 120))
            .sight(TileSight::Explored)
            .players(2)
            .object(house)
            .build();
        build_placement(
            &site,
            &WorldMap { turns: &world },
            Point::new(21.0, 21.0),
            &roles(),
            &tuning().spawn,
        )
        .unwrap()
        .code()
    };
    let (one, two) = (PlayerId::new(1), PlayerId::new(2));
    // player 1 saw the house: its mask meets what player 1 sees and explored, it blocks
    assert_eq!(
        test(
            two,
            Some(VisionBits::visible(one) | VisionBits::explored(one))
        ),
        6
    );
    // seen by player 2 alone (player 1's explored bit kept): another player's house in the
    // fog passes
    assert_eq!(
        test(
            two,
            Some(VisionBits::visible(two) | VisionBits::explored(one))
        ),
        0
    );
    // outside the registry neither, but the player's own does
    assert_eq!(test(two, None), 0);
    assert_eq!(test(one, None), 6);
}

/// The world's sheep reads as [`near_sheep`] on its tile; it blocks a villager (radius 0.2) at the
/// points whose box overlaps its own.
#[test]
fn the_sheep_blocks_the_villager_where_their_boxes_overlap() {
    let villager = Placing {
        player: PlayerId::new(1),
        radius: Footprint::new(0.2, 0.2),
        clearance: Footprint::new(0.2, 0.2),
        tall: true,
        class: class::VILLAGER,
        restriction: &Restriction::new(vec![1.0, 0.0]),
        hill_mode: HillMode::ANY,
        placement: PlacementTerrains::NONE,
    };
    let world = Scene::open(GridSize::new(120, 120)).object(sheep()).build();
    let map = WorldMap { turns: &world };
    assert_eq!(
        map.objects(Tile::new(106, 47)),
        Ok(vec![near_sheep(ObjectState::Alive)])
    );
    let at = |x, y| {
        placement(
            &villager,
            &map,
            Point::new(x, y),
            COUNTED,
            &roles(),
            &tuning().spawn,
        )
        .unwrap()
    };
    assert_eq!(at(106.5, 47.5), Tested::Blocked { stays: true });
    assert_eq!(at(106.5, 47.0), Tested::Blocked { stays: true });
    assert_eq!(at(106.5, 46.5), Tested::Free);
}

/// Two objects on one tile that block the point differently, a sheep that stays (state 2) and
/// a soldier that does not: the first of the tile's list (the one placed first) reports
/// (the game's scan, `FUN_141264db0`), whichever stays.
#[test]
fn the_first_object_of_tile_list_reports_the_block() {
    let villager = Placing {
        player: PlayerId::new(1),
        radius: Footprint::new(0.2, 0.2),
        clearance: Footprint::new(0.2, 0.2),
        tall: true,
        class: class::VILLAGER,
        restriction: &Restriction::new(vec![1.0, 0.0]),
        hill_mode: HillMode::ANY,
        placement: PlacementTerrains::NONE,
    };
    let soldier = || {
        let master = Master {
            id: master_id(4),
            class: class::CLASS_6,
            ..sheep_master()
        };
        SceneObject::new(9521, PlayerId::GAIA, master, sheep().at)
    };
    let at = |first: SceneObject, second: SceneObject| {
        let world = Scene::open(GridSize::new(120, 120))
            .object(first)
            .object(second)
            .build();
        placement(
            &villager,
            &WorldMap { turns: &world },
            Point::new(106.5, 47.5),
            COUNTED,
            &roles(),
            &tuning().spawn,
        )
        .unwrap()
    };
    assert_eq!(at(sheep(), soldier()), Tested::Blocked { stays: true });
    assert_eq!(at(soldier(), sheep()), Tested::Blocked { stays: false });
}

/// The villager the sheep tests place: radius and clearance 0.2, tall, on a map whose terrain 0
/// it may stand on.
fn placed_villager(restriction: &Restriction) -> Placing<'_> {
    Placing {
        player: PlayerId::new(1),
        radius: Footprint::new(0.2, 0.2),
        clearance: Footprint::new(0.2, 0.2),
        tall: true,
        class: class::VILLAGER,
        restriction,
        hill_mode: HillMode::ANY,
        placement: PlacementTerrains::NONE,
    }
}

/// The master's slot 0x50 passes over its call's object (`[0x58]`, the inner ring's building)
/// in both scans: the sheep that blocks the villager at (106.5, 47.5) is not there.
#[test]
fn placement_passes_over_the_object_of_its_call() {
    let restriction = Restriction::new(vec![1.0, 0.0]);
    let villager = placed_villager(&restriction);
    let world = Scene::open(GridSize::new(120, 120)).object(sheep()).build();
    let at = |passed| {
        placement(
            &villager,
            &WorldMap { turns: &world },
            Point::new(106.5, 47.5),
            PlacementCall { passed, ..COUNTED },
            &roles(),
            &tuning().spawn,
        )
        .unwrap()
    };
    assert_eq!(at(None), Tested::Blocked { stays: true });
    assert_eq!(at(Some(object_id(9520))), Tested::Free);
}

/// A mule cart's spot (`[0x60]` 1): the slot's own scan passes over the units, so a soldier
/// first on the tile (which `FUN_141123f80`'s scan still meets) no longer reports the block,
/// and the house after it, which stays, does.
#[test]
fn placement_of_mule_cart_passes_over_the_units_in_its_own_scan() {
    let restriction = Restriction::new(vec![1.0, 0.0]);
    let villager = placed_villager(&restriction);
    let soldier = Master {
        id: master_id(4),
        class: class::CLASS_6,
        ..sheep_master()
    };
    let world = Scene::open(GridSize::new(120, 120))
        .object(SceneObject::new(
            9521,
            PlayerId::GAIA,
            soldier,
            Position::new(106.6, 47.2, 0.0),
        ))
        .object(SceneObject::new(
            500,
            PlayerId::GAIA,
            house_master(),
            Position::new(106.5, 47.9, 0.0),
        ))
        .build();
    let at = |units| {
        placement(
            &villager,
            &WorldMap { turns: &world },
            Point::new(106.5, 47.5),
            PlacementCall { units, ..COUNTED },
            &roles(),
            &tuning().spawn,
        )
        .unwrap()
    };
    assert_eq!(at(UnitScan::Counted), Tested::Blocked { stays: false });
    assert_eq!(at(UnitScan::Passed), Tested::Blocked { stays: true });
}

/// The master's slot 0x50 with `[0x50]` set (`param_11`, every call of genie's) answers 0 for a
/// master whose radii are both at most 0, whatever `FUN_141123f80` found: off the map, on a
/// terrain its restriction forbids, on a map its player never explored, in a hill mode and on
/// the sheep.
#[test]
fn master_without_radii_is_free_wherever_it_stands() {
    let passable = Restriction::new(vec![1.0, 0.0]);
    let forbidden = Restriction::new(vec![0.0, 0.0]);
    let sheep_world = Scene::open(GridSize::new(120, 120)).object(sheep()).build();
    let unexplored = Scene::open(GridSize::new(120, 120))
        .sight(TileSight::Unknown)
        .build();
    let at = |placing: &Placing<'_>, world: &World, x, y| {
        placement(
            placing,
            &WorldMap { turns: world },
            Point::new(x, y),
            COUNTED,
            &roles(),
            &tuning().spawn,
        )
        .unwrap()
    };
    let villager_of = |restriction, radius| Placing {
        radius,
        ..placed_villager(restriction)
    };
    for radius in [Footprint::ZERO, Footprint::new(-0.5, 0.0)] {
        let villager = villager_of(&passable, radius);
        assert_eq!(at(&villager, &sheep_world, -5.0, 47.5), Tested::Free);
        assert_eq!(at(&villager, &sheep_world, 106.5, 47.5), Tested::Free);
        assert_eq!(at(&villager, &unexplored, 60.5, 60.5), Tested::Free);
        assert_eq!(
            at(&villager_of(&forbidden, radius), &sheep_world, 60.5, 60.5),
            Tested::Free
        );
        let on_hills = Placing {
            hill_mode: HillMode::ONE_STEP,
            ..villager
        };
        assert_eq!(at(&on_hills, &sheep_world, 60.5, 60.5), Tested::Free);
    }
}

/// A NaN radius is not at most 0: the slot's `COMISS` of `0.0 < rx` jumps on the unordered
/// case (`JC`) into its scan, so it answers `FUN_141123f80`'s code, here the map's edge (7).
#[test]
fn master_of_nan_radius_answers_the_static_test() {
    let restriction = Restriction::new(vec![1.0, 0.0]);
    let villager = Placing {
        radius: Footprint::new(f32::NAN, 0.0),
        ..placed_villager(&restriction)
    };
    let world = Scene::open(GridSize::new(120, 120)).build();
    assert_eq!(
        placement(
            &villager,
            &WorldMap { turns: &world },
            Point::new(60.5, -1.0),
            COUNTED,
            &roles(),
            &tuning().spawn,
        ),
        Ok(Tested::Blocked { stays: false })
    );
}

/// A unit of radii 1 in hill mode `hill_mode` on terrain 0, which `restriction` rates: at
/// (50, 50) its box is the tiles 49 and 50 of both axes, whose first column's last row is
/// (49, 50).
fn hill_unit(restriction: &Restriction, hill_mode: HillMode) -> Placing<'_> {
    Placing {
        radius: Footprint::new(1.0, 1.0),
        clearance: Footprint::new(1.0, 1.0),
        hill_mode,
        ..placed_villager(restriction)
    }
}

/// The spawn's placement test of `placing` at (50, 50) under the call `call`, on a map of 120
/// by 120 tiles at elevation 3 that `scene` changes.
fn on_hills(
    placing: &Placing<'_>,
    call: PlacementCall,
    scene: impl FnOnce(Scene) -> Scene,
) -> GenieResult<Tested> {
    let world = scene(Scene::open(GridSize::new(120, 120)).elevation(3)).build();
    placement(
        placing,
        &WorldMap { turns: &world },
        Point::new(50.0, 50.0),
        call,
        &roles(),
        &tuning().spawn,
    )
}

/// The tiles of column 48 from row 40 to row 59 lie a step up: the box's first column (49)
/// takes the side slope with its west up (shape 5).
fn west_slope(scene: Scene) -> Scene {
    scene.elevation_on((40..60).map(|y| Tile::new(48, y)), 4)
}

/// Hill mode 1 takes the box's side slopes (`FUN_141123f80`'s `+0x604` 1): its first column on
/// the west slope passes, a corner (shape 9: the tile south west of (49, 50) up) fails (code 3).
#[test]
fn placement_in_hill_mode_1_takes_side_slopes_but_no_corner() {
    let restriction = Restriction::new(vec![1.0, 0.0]);
    let unit = hill_unit(&restriction, HillMode::SIDE_SLOPES);
    let corner = |scene: Scene| scene.elevation_on([Tile::new(48, 51)], 4);
    let shape = |scene: fn(Scene) -> Scene| {
        let world = scene(Scene::open(GridSize::new(120, 120)).elevation(3)).build();
        WorldMap { turns: &world }.shape(Tile::new(49, 50))
    };
    assert_eq!((shape(west_slope), shape(corner)), (Some(5), Some(9)));
    assert_eq!(on_hills(&unit, COUNTED, |scene| scene), Ok(Tested::Free));
    assert_eq!(on_hills(&unit, COUNTED, west_slope), Ok(Tested::Free));
    assert_eq!(
        on_hills(&unit, COUNTED, corner),
        Ok(Tested::Blocked { stays: false })
    );
}

/// Hill mode 2 takes flat ground alone: the box on the west slope fails (code 3).
#[test]
fn placement_in_hill_mode_2_takes_flat_ground_alone() {
    let restriction = Restriction::new(vec![1.0, 0.0]);
    let unit = hill_unit(&restriction, HillMode::FLAT);
    assert_eq!(on_hills(&unit, COUNTED, |scene| scene), Ok(Tested::Free));
    assert_eq!(
        on_hills(&unit, COUNTED, west_slope),
        Ok(Tested::Blocked { stays: false })
    );
}

/// Hill mode 3 measures the box's first column and last row against the elevation of the
/// first column's last row (49, 50), read before the tiles: (50, 50) a step above that corner
/// passes and (49, 49) a step below it too, though two below (50, 50); (49, 49) two steps above
/// the corner fails (code 3); the tile (50, 49), on neither, passes at any elevation.
#[test]
fn placement_in_hill_mode_3_keeps_the_edge_within_one_step_of_its_corner() {
    let restriction = Restriction::new(vec![1.0, 0.0]);
    let unit = hill_unit(&restriction, HillMode::ONE_STEP);
    let raised = |tiles: &'static [([i32; 2], u8)]| {
        move |scene: Scene| {
            tiles.iter().fold(scene, |scene, &(tile, elevation)| {
                scene.elevation_on([Tile::from(tile)], elevation)
            })
        }
    };
    assert_eq!(
        on_hills(&unit, COUNTED, raised(&[([49, 50], 4), ([50, 50], 5)])),
        Ok(Tested::Free)
    );
    assert_eq!(
        on_hills(&unit, COUNTED, raised(&[([49, 49], 5)])),
        Ok(Tested::Blocked { stays: false })
    );
    assert_eq!(
        on_hills(&unit, COUNTED, raised(&[([50, 49], 9)])),
        Ok(Tested::Free)
    );
}

/// `FUN_141123f80` tests the box's tiles column by column, each tile's terrain before its hill
/// mode: (49, 50) in a corner fails before the terrain 5, which the restriction does not list,
/// of (50, 49) is read; that terrain on (49, 50) itself is read first.
#[test]
fn placement_tests_the_hill_mode_by_columns_after_the_terrain() {
    let restriction = Restriction::new(vec![1.0, 0.0]);
    let unit = hill_unit(&restriction, HillMode::SIDE_SLOPES);
    let unlisted_on = |tile: Tile| {
        move |scene: Scene| {
            scene
                .elevation_on([Tile::new(48, 51)], 4)
                .terrain_on([tile], TerrainId::named(5))
        }
    };
    assert_eq!(
        on_hills(&unit, COUNTED, unlisted_on(Tile::new(50, 49))),
        Ok(Tested::Blocked { stays: false })
    );
    assert_eq!(
        on_hills(&unit, COUNTED, unlisted_on(Tile::new(49, 50)))
            .unwrap_err()
            .reason(),
        UNLISTED_TERRAIN.reason()
    );
}

/// A tile the hill mode refuses ends `FUN_141123f80` before its zone test and its scan: a zone
/// mode raises nothing, and the sheep on the box, which only that scan counts where the slot's
/// own passes over the units (`[0x60]` 1), no longer reports the block.
#[test]
fn placement_hill_failure_skips_the_zone_test_and_the_static_scan() {
    let restriction = Restriction::new(vec![1.0, 0.0]);
    let unit = hill_unit(&restriction, HillMode::FLAT);
    let zoned = PlacementCall {
        zones: ZoneTest::Tested,
        ..COUNTED
    };
    assert_eq!(
        on_hills(&unit, zoned, west_slope),
        Ok(Tested::Blocked { stays: false })
    );
    let sheep_on_box = |scene: Scene| {
        scene.object(SceneObject {
            at: Position::new(50.3, 50.3, 0.0),
            ..sheep()
        })
    };
    let passing_units = PlacementCall {
        units: UnitScan::Passed,
        ..COUNTED
    };
    assert_eq!(
        on_hills(&unit, passing_units, sheep_on_box),
        Ok(Tested::Blocked { stays: true })
    );
    assert_eq!(
        on_hills(&unit, passing_units, |scene| west_slope(sheep_on_box(
            scene
        ))),
        Ok(Tested::Blocked { stays: false })
    );
}

/// A call of `[0x60]` 2: the slot's own scan passes over the units in motion (their slot 0x1f8,
/// the animation speed `+0x7b8`, above 0), so a walking soldier first on the tile no longer
/// reports the block and the house after it does; a soldier at rest and one of a NaN speed
/// (`COMISS`, `JA` at 0x14126523f) still report it.
#[test]
fn placement_passing_units_in_motion_passes_over_walking_soldier_only() {
    let restriction = Restriction::new(vec![1.0, 0.0]);
    let villager = placed_villager(&restriction);
    let at = |speed: f32| {
        let soldier = Master {
            id: master_id(4),
            class: class::CLASS_6,
            ..sheep_master()
        };
        let mut world = Scene::open(GridSize::new(120, 120))
            .object(SceneObject::new(
                9521,
                PlayerId::GAIA,
                soldier,
                Position::new(106.6, 47.2, 0.0),
            ))
            .object(SceneObject::new(
                500,
                PlayerId::GAIA,
                house_master(),
                Position::new(106.5, 47.9, 0.0),
            ))
            .build();
        world
            .parts
            .set_anim_speed(object_id(9521), Speed::new(speed));
        placement(
            &villager,
            &WorldMap { turns: &world },
            Point::new(106.5, 47.5),
            PlacementCall {
                units: UnitScan::InMotionPassed,
                ..COUNTED
            },
            &roles(),
            &tuning().spawn,
        )
        .unwrap()
    };
    assert_eq!(at(0.7), Tested::Blocked { stays: true });
    assert_eq!(at(0.0), Tested::Blocked { stays: false });
    assert_eq!(at(f32::NAN), Tested::Blocked { stays: false });
}

/// A call of `[0x40]` 1 measures by the master's clearance radii (`+0x5fc`, `+0x600`) in the
/// slot's gate and `FUN_141123f80`'s box: the villager's clearance of 0.6 meets the sheep from
/// (106.5, 46.5), which its radius of 0.2 misses, and leaves the map at x 0.5. A clearance of 0
/// is free on the sheep's own spot, where a box of no size still overlaps the sheep: only the
/// gate on the chosen radii frees it.
#[test]
fn placement_by_clearance_measures_the_clearance_radii() {
    let restriction = Restriction::new(vec![1.0, 0.0]);
    let world = Scene::open(GridSize::new(120, 120)).object(sheep()).build();
    let at = |clearance, radii, x, y| {
        placement(
            &Placing {
                clearance,
                ..placed_villager(&restriction)
            },
            &WorldMap { turns: &world },
            Point::new(x, y),
            PlacementCall { radii, ..COUNTED },
            &roles(),
            &tuning().spawn,
        )
        .unwrap()
    };
    let wide = Footprint::new(0.6, 0.6);
    assert_eq!(at(wide, PlacementRadii::Size, 106.5, 46.5), Tested::Free);
    assert_eq!(
        at(wide, PlacementRadii::Clearance, 106.5, 46.5),
        Tested::Blocked { stays: true }
    );
    assert_eq!(at(wide, PlacementRadii::Size, 0.5, 60.5), Tested::Free);
    assert_eq!(
        at(wide, PlacementRadii::Clearance, 0.5, 60.5),
        Tested::Blocked { stays: false }
    );
    let (sheep_x, sheep_y) = (sheep().at.x(), sheep().at.y());
    assert_eq!(
        at(Footprint::ZERO, PlacementRadii::Size, sheep_x, sheep_y),
        Tested::Blocked { stays: true }
    );
    assert_eq!(
        at(Footprint::ZERO, PlacementRadii::Clearance, sheep_x, sheep_y),
        Tested::Free
    );
}

/// `FUN_141123f80`'s scan measures by the clearance of a call of `[0x40]` 1: with the slot's own
/// scan passing over the units (`[0x60]` 1), only that scan's box of 0.6 finds the sheep from
/// (106.5, 46.5), where the radius of 0.2 finds nothing.
#[test]
fn placement_by_clearance_measures_static_scan_by_the_clearance() {
    let restriction = Restriction::new(vec![1.0, 0.0]);
    let villager = Placing {
        clearance: Footprint::new(0.6, 0.6),
        ..placed_villager(&restriction)
    };
    let world = Scene::open(GridSize::new(120, 120)).object(sheep()).build();
    let at = |radii| {
        placement(
            &villager,
            &WorldMap { turns: &world },
            Point::new(106.5, 46.5),
            PlacementCall {
                radii,
                units: UnitScan::Passed,
                ..COUNTED
            },
            &roles(),
            &tuning().spawn,
        )
        .unwrap()
    };
    assert_eq!(at(PlacementRadii::Size), Tested::Free);
    assert_eq!(
        at(PlacementRadii::Clearance),
        Tested::Blocked { stays: true }
    );
}

/// The slot's own scan measures by the clearance of a call of `[0x40]` 1: on a box that meets a
/// water tile, which the villager's restriction forbids, `FUN_141123f80` answers 2 without its
/// scan, and only the own scan's box of 0.6 makes the sheep the blocker from (106.5, 46.5), so
/// the block stays.
#[test]
fn placement_by_clearance_measures_own_scan_by_the_clearance() {
    let restriction = Restriction::new(vec![1.0, 0.0]);
    let villager = Placing {
        clearance: Footprint::new(0.6, 0.6),
        ..placed_villager(&restriction)
    };
    let world = Scene::open(GridSize::new(120, 120))
        .water([Tile::new(106, 46)])
        .object(sheep())
        .build();
    let at = |radii| {
        placement(
            &villager,
            &WorldMap { turns: &world },
            Point::new(106.5, 46.5),
            PlacementCall { radii, ..COUNTED },
            &roles(),
            &tuning().spawn,
        )
        .unwrap()
    };
    assert_eq!(at(PlacementRadii::Size), Tested::Blocked { stays: false });
    assert_eq!(
        at(PlacementRadii::Clearance),
        Tested::Blocked { stays: true }
    );
}

/// Under the feature flags (`DAT_143f01833`, [`FeatureFlags::On`]) the slot's own scan passes
/// over a building annexed to its call's object (`[0x58]`, the annex's `+0xbb8`): the house 501,
/// an annex of the building 600 and first on the tile, reports the block without the flags or
/// without a passed object; with both, the soldier after it does.
#[test]
fn placement_under_feature_flags_passes_over_annex_of_passed_building() {
    let restriction = Restriction::new(vec![1.0, 0.0]);
    let villager = placed_villager(&restriction);
    let soldier = Master {
        id: master_id(4),
        class: class::CLASS_6,
        ..sheep_master()
    };
    let mut world = Scene::open(GridSize::new(120, 120))
        .object(SceneObject::new(
            600,
            PlayerId::GAIA,
            house_master(),
            Position::new(20.5, 20.5, 0.0),
        ))
        .object(SceneObject {
            annex_of: Some(MainBuildingId::new(object_id(600))),
            ..SceneObject::new(
                501,
                PlayerId::GAIA,
                house_master(),
                Position::new(106.5, 47.9, 0.0),
            )
        })
        .object(SceneObject::new(
            9521,
            PlayerId::GAIA,
            soldier,
            Position::new(106.6, 47.2, 0.0),
        ))
        .build();
    let at = |world: &World, passed| {
        placement(
            &villager,
            &WorldMap { turns: world },
            Point::new(106.5, 47.5),
            PlacementCall { passed, ..COUNTED },
            &roles(),
            &tuning().spawn,
        )
        .unwrap()
    };
    let main = Some(object_id(600));
    assert_eq!(at(&world, main), Tested::Blocked { stays: true });
    world.feature_flags = FeatureFlags::On;
    assert_eq!(at(&world, None), Tested::Blocked { stays: true });
    assert_eq!(at(&world, main), Tested::Blocked { stays: false });
}

/// Under the feature flags the slot's test of an annex needs its call's object (`[0x58]`,
/// `TEST RBP, RBP` at 0x14126520c): without one, the house 501, no annex and first on the
/// tile, still reports the block, as it does beside a passed object it is no annex of.
#[test]
fn placement_under_feature_flags_without_passed_object_counts_building_that_is_no_annex() {
    let restriction = Restriction::new(vec![1.0, 0.0]);
    let villager = placed_villager(&restriction);
    let soldier = Master {
        id: master_id(4),
        class: class::CLASS_6,
        ..sheep_master()
    };
    let mut world = Scene::open(GridSize::new(120, 120))
        .object(SceneObject::new(
            501,
            PlayerId::GAIA,
            house_master(),
            Position::new(106.5, 47.9, 0.0),
        ))
        .object(SceneObject::new(
            9521,
            PlayerId::GAIA,
            soldier,
            Position::new(106.6, 47.2, 0.0),
        ))
        .build();
    world.feature_flags = FeatureFlags::On;
    let at = |passed| {
        placement(
            &villager,
            &WorldMap { turns: &world },
            Point::new(106.5, 47.5),
            PlacementCall { passed, ..COUNTED },
            &roles(),
            &tuning().spawn,
        )
        .unwrap()
    };
    assert_eq!(at(None), Tested::Blocked { stays: true });
    assert_eq!(at(Some(object_id(600))), Tested::Blocked { stays: true });
}

/// A master without a height (`+0x5e0` at most 0) is free on the sheep: each object's test in
/// both scans needs that height, so the call's `[0x48]` (`param_10`), which lets the scans run
/// without it, changes nothing.
#[test]
fn master_without_height_is_free_on_the_sheep() {
    let restriction = Restriction::new(vec![1.0, 0.0]);
    let villager = Placing {
        tall: false,
        ..placed_villager(&restriction)
    };
    let world = Scene::open(GridSize::new(120, 120)).object(sheep()).build();
    assert_eq!(
        placement(
            &villager,
            &WorldMap { turns: &world },
            Point::new(106.5, 47.5),
            COUNTED,
            &roles(),
            &tuning().spawn,
        ),
        Ok(Tested::Free)
    );
}

/// `FUN_140ad0240` at the forage world's town centre 700: a villager (master 83) spawns beside
/// it and stands idle; attribute 81 lowers by its population and the centre's unit AI hears
/// that its make action completed (0x1fa with 0x26b). With the centre's command queue the
/// new villager's unit AI takes each order behind its own (priority 100, ready, at the
/// centre's height). A computer player's spawn, one without the players' sight and a master
/// that does not move are not modelled.
#[test]
fn made_villager_spawns_beside_its_town_centre_and_takes_its_queue() {
    let world = || {
        let mut forage_world = forage_world();
        // the villager's standing sprite (a graphic of ten frames, eight facets)
        for masters in &mut forage_world.model.masters {
            Arc::make_mut(masters)
                .get_mut(ids::VILLAGER_MALE)
                .unwrap()
                .standing_sprite = Some(SpriteId::named(300));
        }
        let _ = Arc::make_mut(&mut forage_world.model.graphics).insert(
            SpriteId::named(300),
            Graphic {
                id: SpriteId::named(300),
                frames: Frame::of_data(10),
                facets: FacetCount::of_data(8),
                speed_multiplier: SpeedMultiplier::ONE,
                frame_duration: Seconds::new(0.1),
                replay_delay: Seconds::ZERO,
                flags: SpriteFlags::ANIMATED,
            },
        );
        let players = &mut forage_world.players[1];
        players.attributes[attribute::IN_PRODUCTION.index()] = Amount::new(3.0);
        forage_world.parts.set_unit_ai(
            players.objects[ListIndex::new(2)].id,
            Some(UnitAi {
                ai_kind: UnitAiKind::Predator,
                current_order: None,
                current_action: None,
                current_target: None,
                ..UnitAi::default()
            }),
        );
        forage_world
    };
    let mut game = world();
    let before = game.players[1].objects.len();
    assert_eq!(
        game.spawn(
            PlayerId::new(1),
            ListIndex::new(2),
            ids::VILLAGER_MALE,
            Amount::new(1.0),
        )
        .unwrap(),
        SpawnOutcome::Made
    );
    let player = &game.players[1];
    assert_eq!(player.objects.len(), before + 1);
    let made = player.objects.last().unwrap();
    assert_eq!(made.life.master_id, ids::VILLAGER_MALE);
    assert_eq!(
        player.attributes[attribute::IN_PRODUCTION.index()],
        Amount::new(2.0)
    );
    let completed = game
        .parts
        .unit_ai
        .get(player.objects[ListIndex::new(2)].id)
        .unwrap()
        .notifies
        .iter()
        .any(|notify| {
            notify.kind == NotifyKind::ACTION_COMPLETED
                && notify.params == [AiAction::Make.value(), 0, 0]
        });
    assert!(completed);
    // the centre's command queue goes to the new unit
    let mut game = world();
    let order = QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::Move,
        priority: OrderPriority::of_data(5),
        target: None,
        target_owner: None,
        point: Position::new(40.0, 80.0, 7.0),
        range: Distance::new(2.0),
        shown: Point::new(40.0, 80.0),
        ready: false,
        target_kind: None,
    };
    game.parts
        .unit_ai
        .get_mut(game.players[1].objects[ListIndex::new(2)].id)
        .unwrap()
        .building_orders
        .push(order);
    assert_eq!(
        game.spawn(
            PlayerId::new(1),
            ListIndex::new(2),
            ids::VILLAGER_MALE,
            Amount::new(1.0),
        )
        .unwrap(),
        SpawnOutcome::Made
    );
    let made = game.players[1].objects.last().unwrap();
    let queued = game
        .parts
        .unit_ai
        .get(made.id)
        .map(|ai| ai.orders.clone())
        .unwrap_or_default();
    assert_eq!(
        queued
            .iter()
            .map(|queued_order| (
                queued_order.order,
                queued_order.priority.get(),
                queued_order.point,
                queued_order.range,
                queued_order.ready
            ))
            .collect::<Vec<_>>(),
        [(
            OrderKind::Move,
            100,
            Position::new(40.0, 80.0, 0.0),
            Distance::ZERO,
            true
        )]
    );
    // an order on an object the world holds that the new unit cannot enter (the centre has no
    // room) goes to it as it is
    let mut game = world();
    let centre = game.players[1].objects[ListIndex::new(2)].id;
    game.parts
        .unit_ai
        .get_mut(centre)
        .unwrap()
        .building_orders
        .push(QueuedOrder {
            target: Some(centre),
            ..queued_move()
        });
    assert_eq!(
        game.spawn(
            PlayerId::new(1),
            ListIndex::new(2),
            ids::VILLAGER_MALE,
            Amount::new(1.0),
        )
        .unwrap(),
        SpawnOutcome::Made
    );
    let made = game.players[1].objects.last().unwrap();
    let targets: Vec<_> = game
        .parts
        .unit_ai
        .get(made.id)
        .map(|ai| ai.orders.iter().map(|order| order.target).collect())
        .unwrap_or_default();
    assert_eq!(targets, [Some(centre)]);
    let mut game = world();
    game.players[1].kind = PlayerKind::Computer;
    assert!(
        game.spawn(
            PlayerId::new(1),
            ListIndex::new(2),
            ids::VILLAGER_MALE,
            Amount::new(1.0)
        )
        .is_err()
    );
    let mut game = world();
    game.spatial.sight = None;
    assert!(
        game.spawn(
            PlayerId::new(1),
            ListIndex::new(2),
            ids::VILLAGER_MALE,
            Amount::new(1.0)
        )
        .is_err()
    );
    let mut world = world();
    assert!(
        world
            .spawn(
                PlayerId::new(1),
                ListIndex::new(2),
                master_id(59),
                Amount::new(1.0)
            )
            .is_err()
    );
}

/// [`forage_world`] with its town centre 700 (list index 2 of player 1) making a villager (its
/// standing sprite a graphic of ten frames): the centre of radius 0.4 (no point inside it for
/// the spawn search's inner ring) on a map where the villager may stand nowhere.
fn walled_in_centre() -> World {
    let mut world = forage_world();
    for masters in &mut world.model.masters {
        let masters = Arc::make_mut(masters);
        if let Some(centre) = masters.get_mut(ids::TOWN_CENTRE) {
            centre.radius = Footprint::new(0.4, 0.4);
        }
        if let Some(villager) = masters.get_mut(ids::VILLAGER_MALE) {
            villager.standing_sprite = Some(SpriteId::named(300));
        }
    }
    let _ = Arc::make_mut(&mut world.model.graphics).insert(
        SpriteId::named(300),
        Graphic {
            id: SpriteId::named(300),
            frames: Frame::of_data(10),
            facets: FacetCount::of_data(8),
            speed_multiplier: SpeedMultiplier::ONE,
            frame_duration: Seconds::new(0.1),
            replay_delay: Seconds::ZERO,
            flags: SpriteFlags::ANIMATED,
        },
    );
    set_standing(&mut world, 0.0);
    world.players[1].attributes[attribute::IN_PRODUCTION.index()] = Amount::new(1.0);
    let centre = world.players[1].objects[ListIndex::new(2)].id;
    if let Some(&placed) = world.spatial.get(centre) {
        world.spatial.insert(
            centre,
            Placed {
                radius: Footprint::new(0.4, 0.4),
                ..placed
            },
        );
    }
    let make = MakeObj {
        state: MakeObjState::Making,
        master: Some(ids::VILLAGER_MALE),
        ..MakeObj::for_entry(&Entry::default())
    };
    world
        .set_part(
            centre,
            Part::Production(Some(Box::new(Production {
                action: Some(Making::Unit(make)),
                ..Production::default()
            }))),
        )
        .unwrap();
    world
}

/// Every restriction of `world`'s map takes `value` on its terrain.
fn set_standing(world: &mut World, value: f32) {
    if let Some(terrain) = world.spatial.terrain.as_mut() {
        for restriction in &mut terrain.restrictions {
            *restriction = Restriction::new(vec![value]);
        }
    }
}

/// The make action of [`walled_in_centre`]'s town centre.
fn centre_make(world: &World) -> MakeObj {
    let centre = world.players[1].objects[ListIndex::new(2)].id;
    match world
        .parts
        .production
        .get(centre)
        .and_then(|production| production.action)
    {
        Some(Making::Unit(make)) => make,
        _ => panic!("the centre's make action"),
    }
}

/// `FUN_140ad0240` where the spot search finds nothing: no unit, attribute 81 as it was.
#[test]
fn spawn_without_spot_makes_nothing() {
    let mut world = walled_in_centre();
    let before = world.players[1].objects.len();
    assert_eq!(
        world
            .spawn(
                PlayerId::new(1),
                ListIndex::new(2),
                ids::VILLAGER_MALE,
                Amount::new(1.0)
            )
            .unwrap(),
        SpawnOutcome::NoSpot
    );
    let player = &world.players[1];
    assert_eq!(player.objects.len(), before);
    assert_eq!(
        player.attributes[attribute::IN_PRODUCTION.index()],
        Amount::new(1.0)
    );
}

/// The make action's update (`0x140ad0490`) whose unit finds no spot: `set_state(10)` with its
/// timer at 5 s; each update takes dt off it, and at 0 the spawn tries again, which on open
/// ground makes the villager and ends the action (state 1).
#[test]
fn make_action_without_spot_waits_five_seconds_and_tries_again() {
    let mut world = walled_in_centre();
    let before = world.players[1].objects.len();
    let step = |world: &mut World, seconds| {
        world
            .update_production_action(PlayerId::new(1), ListIndex::new(2), Seconds::new(seconds))
            .unwrap()
    };
    assert_eq!(step(&mut world, 0.5), UpdateCode::UnderWay);
    let make = centre_make(&world);
    assert_eq!((make.state, make.timer), (MakeObjState::NoSpot, SPOT_RETRY));
    assert_eq!(step(&mut world, 2.0), UpdateCode::UnderWay);
    assert_eq!(centre_make(&world).timer, Seconds::new(3.0));
    set_standing(&mut world, 1.0);
    assert_eq!(step(&mut world, 2.5), UpdateCode::UnderWay);
    assert_eq!(world.players[1].objects.len(), before);
    assert_eq!(step(&mut world, 0.5), UpdateCode::Done);
    assert_eq!(centre_make(&world).state, MakeObjState::Done);
    assert_eq!(world.players[1].objects.len(), before + 1);
}

/// A NaN timer of a make action in state 10 waits (`COMISS 0, timer`; `JC`).
#[test]
fn make_action_of_nan_timer_waits() {
    let mut world = walled_in_centre();
    set_standing(&mut world, 1.0);
    let centre = world.players[1].objects[ListIndex::new(2)].id;
    if let Some(Some(Making::Unit(make))) = world
        .parts
        .production
        .get_mut(centre)
        .map(|production| production.action.as_mut())
    {
        make.state = MakeObjState::NoSpot;
        make.timer = Seconds::new(f32::NAN);
    }
    let before = world.players[1].objects.len();
    assert_eq!(
        world
            .update_production_action(PlayerId::new(1), ListIndex::new(2), Seconds::new(1.0))
            .unwrap(),
        UpdateCode::UnderWay
    );
    assert_eq!(world.players[1].objects.len(), before);
}

/// A new unit's top speed (slot 65, `FUN_14114d1d0`) is the one of the sprite it holds, its
/// standing one: its master's speed times that sprite's factor, not the master's speed with its
/// running sprite (the deer 9655 of MarvinBlue/426048731 ran at the running speed).
#[test]
fn made_unit_moves_at_the_speed_of_its_standing_sprite() {
    let mut world = forage_world();
    for masters in &mut world.model.masters {
        let villager = Arc::make_mut(masters).get_mut(ids::VILLAGER_MALE).unwrap();
        villager.standing_sprite = Some(SpriteId::named(300));
        let moving = villager.move_stats.as_mut().unwrap();
        moving.base = Speed::new(0.8);
        moving.speed = Speed::new(1.6);
    }
    let _ = Arc::make_mut(&mut world.model.graphics).insert(
        SpriteId::named(300),
        Graphic {
            id: SpriteId::named(300),
            frames: Frame::of_data(10),
            facets: FacetCount::of_data(8),
            speed_multiplier: SpeedMultiplier::of_data(0.5),
            frame_duration: Seconds::new(0.1),
            replay_delay: Seconds::ZERO,
            flags: SpriteFlags::ANIMATED,
        },
    );
    world.players[1].attributes[attribute::IN_PRODUCTION.index()] = Amount::new(1.0);
    assert_eq!(
        world
            .spawn(
                PlayerId::new(1),
                ListIndex::new(2),
                ids::VILLAGER_MALE,
                Amount::new(1.0),
            )
            .unwrap(),
        SpawnOutcome::Made
    );
    let made = world.players[1].objects.last().unwrap();
    assert_eq!(made.life.master_id, ids::VILLAGER_MALE);
    assert_eq!(
        world.parts.mover_of(made.id).unwrap().stats.speed,
        Speed::new(0.4)
    );
}

/// The combat setup (`FUN_14121df60`) of a new unit: its charge starts at its master's
/// `max_charge` (`+0x9e0` from `+0x918`), its decay timer (`+0x99c`) at its player's salvage
/// decay rate (attribute 65) times 1000, its powerup update offset (`+0xa54`) is the world time
/// plus the "setup" draw mod 100, and a monk (class 18) holds the faith (attribute 34 at 100),
/// a villager none.
#[test]
fn made_unit_takes_its_master_charge_decay_powerup_offset_and_faith() {
    use crate::combat::Charge;
    use crate::gathering::DecayTimer;
    let made = |class| {
        let mut world = forage_world();
        for masters in &mut world.model.masters {
            let villager = Arc::make_mut(masters).get_mut(ids::VILLAGER_MALE).unwrap();
            villager.standing_sprite = Some(SpriteId::named(300));
            villager.class = class;
            villager
                .creatable
                .get_or_insert_with(Default::default)
                .max_charge = Charge::new(3.0);
        }
        let _ = Arc::make_mut(&mut world.model.graphics).insert(
            SpriteId::named(300),
            Graphic {
                id: SpriteId::named(300),
                frames: Frame::of_data(10),
                facets: FacetCount::of_data(8),
                speed_multiplier: SpeedMultiplier::ONE,
                frame_duration: Seconds::new(0.1),
                replay_delay: Seconds::ZERO,
                flags: SpriteFlags::ANIMATED,
            },
        );
        world.players[1].attributes[attribute::IN_PRODUCTION.index()] = Amount::new(1.0);
        world.players[1].attributes[attribute::SALVAGE_DECAY_RATE.index()] = Amount::new(2.5);
        let now = world.world_time;
        assert_eq!(
            world
                .spawn(
                    PlayerId::new(1),
                    ListIndex::new(2),
                    ids::VILLAGER_MALE,
                    Amount::new(1.0),
                )
                .unwrap(),
            SpawnOutcome::Made
        );
        let unit = world.players[1].objects.last().unwrap();
        let combat = world.parts.combat_of(unit.id);
        (
            combat.charge,
            combat.decay_timer,
            combat.powerup_update.since(now) < Millis::new(100),
            unit.life.attribute_kind,
            unit.life.amount,
        )
    };
    assert_eq!(
        made(class::MONK),
        (
            Charge::new(3.0),
            DecayTimer::of_data(2500),
            true,
            Some(attribute::FAITH),
            Amount::new(100.0)
        )
    );
    assert_eq!(
        made(class::VILLAGER),
        (
            Charge::new(3.0),
            DecayTimer::of_data(2500),
            true,
            None,
            Amount::ZERO
        )
    );
}

/// The powerup update offset is the world time plus the draw mod 100 milliseconds.
#[test]
fn powerup_update_offset_is_the_time_plus_the_draw_mod_100() {
    use crate::production::spawn::combat_setup::powerup_update_after;
    assert_eq!(
        powerup_update_after(WorldTime::from_millis(1234), 0x7fff),
        WorldTime::from_millis(1234 + 0x7fff % 100)
    );
    assert_eq!(
        powerup_update_after(WorldTime::from_millis(50), 199),
        WorldTime::from_millis(149)
    );
}

/// `FUN_140ace9c0` sends the new unit to its building's gather point only for a state of 1
/// (`+0xc30`); a state of 0xff (SET GATHER STATE's) leaves it standing, which the unit's wait
/// to move (`+0x7f9`) shows.
#[test]
fn only_gather_point_of_state_1_sends_the_new_unit() {
    let waits = |state| {
        let mut world = forage_world();
        for masters in &mut world.model.masters {
            Arc::make_mut(masters)
                .get_mut(ids::VILLAGER_MALE)
                .unwrap()
                .standing_sprite = Some(SpriteId::named(300));
        }
        let _ = Arc::make_mut(&mut world.model.graphics).insert(
            SpriteId::named(300),
            Graphic {
                id: SpriteId::named(300),
                frames: Frame::of_data(10),
                facets: FacetCount::of_data(8),
                speed_multiplier: SpeedMultiplier::ONE,
                frame_duration: Seconds::new(0.1),
                replay_delay: Seconds::ZERO,
                flags: SpriteFlags::ANIMATED,
            },
        );
        let player = &mut world.players[1];
        player.attributes[attribute::IN_PRODUCTION.index()] = Amount::new(3.0);
        world.parts.set_unit_ai(
            player.objects[ListIndex::new(2)].id,
            Some(UnitAi {
                ai_kind: UnitAiKind::Predator,
                current_order: None,
                current_action: None,
                current_target: None,
                ..UnitAi::default()
            }),
        );
        let at = player.objects[ListIndex::new(2)].id;
        let gather = Some(GatherPoint {
            state,
            point: Position::new(40.0, 80.0, 0.0),
            target: None,
            target_master: None,
        });
        world.set_part(at, Part::GatherPoint(gather)).unwrap();
        assert_eq!(
            world
                .spawn(
                    PlayerId::new(1),
                    ListIndex::new(2),
                    ids::VILLAGER_MALE,
                    Amount::new(1.0),
                )
                .unwrap(),
            SpawnOutcome::Made
        );
        world.players[1]
            .objects
            .last()
            .and_then(|unit| world.parts.mover_of(unit.id).map(|mover| mover.waiting))
    };
    assert_eq!(waits(GatherPointState::Point), Some(1));
    assert_eq!(waits(GatherPointState::Kept), Some(0));
}

/// Every point of `site`'s search for a unit of radius 0.5 (a step of 0.5, no gap) with its
/// test, all blocked by objects that stay: each spot's first candidate in turn, then the later
/// rings' points; no tile on the map, so none is kept.
fn all_blocked(site: &Site) -> (Vec<(Point, SpawnTest)>, GenieResult<Option<Point>>) {
    let mut tried = Vec::new();
    let spawn_point = spawn_point(
        site,
        Footprint::new(0.5, 0.5),
        &mut |point, spawn_test| {
            tried.push((point, spawn_test));
            Ok(Tested::Blocked { stays: true })
        },
        &|_| Ok(None),
        &tuning().spawn,
    );
    (tried, spawn_point)
}

/// The points of `tried` its test `spawn_test` asked about, in order.
fn tried_by(tried: &[(Point, SpawnTest)], spawn_test: SpawnTest) -> Vec<Point> {
    tried
        .iter()
        .filter(|&&(_, test)| test == spawn_test)
        .map(|&(point, _)| point)
        .collect()
}

/// The arguments of the spawn's placement test for a unit other than the mule cart.
const COUNTED: PlacementCall = PlacementCall {
    explored: ExploredTest::Tested,
    hills: HillTest::Tested,
    radii: PlacementRadii::Size,
    zones: ZoneTest::Skipped,
    passed: None,
    units: UnitScan::Counted,
};

/// `FUN_1411d12d0` without a gather point: one pass over the eight segments of an open
/// building's frame (left 8.5, right 11.5, bottom 8.5, top 11.5): the top side's left quarter
/// and a half more (1), the left side's top part (2), the top's right part (3), the left's
/// lower part (4), the right side's top half (5) and lower half (6), the bottom side's left
/// half (7) and right half (8). Then the later rings: the edge's eight points, then the
/// footprint's five, each with its three neighbours; none is free, and no tile is on the map.
#[test]
fn spawn_search_without_gather_point_walks_the_eight_segments_once() {
    let site = Site {
        position: Position::new(10.0, 10.0, 0.0),
        radius: Footprint::new(1.0, 1.0),
        goal: None,
        walls: [false; 4],
        closest_goal: None,
    };
    let (tried, point) = all_blocked(&site);
    let top = [
        Point::new(8.5, 11.5),
        Point::new(9.0, 11.5),
        Point::new(9.5, 11.5),
    ];
    let left = [
        Point::new(8.5, 11.5),
        Point::new(8.5, 11.0),
        Point::new(8.5, 10.5),
    ];
    let top_right = [
        Point::new(9.75, 11.5),
        Point::new(10.25, 11.5),
        Point::new(10.75, 11.5),
        Point::new(11.25, 11.5),
    ];
    let left_low = [
        Point::new(8.5, 10.25),
        Point::new(8.5, 9.75),
        Point::new(8.5, 9.25),
        Point::new(8.5, 8.75),
    ];
    let right = [Point::new(11.5, 11.5), Point::new(11.5, 11.0)];
    let right_low = [
        Point::new(11.5, 10.5),
        Point::new(11.5, 10.0),
        Point::new(11.5, 9.5),
        Point::new(11.5, 9.0),
    ];
    let bottom = [Point::new(8.5, 8.5), Point::new(9.0, 8.5)];
    let bottom_right = [
        Point::new(9.5, 8.5),
        Point::new(10.0, 8.5),
        Point::new(10.5, 8.5),
        Point::new(11.0, 8.5),
    ];
    let segments: [&[Point]; 8] = [
        &top,
        &left,
        &top_right,
        &left_low,
        &right,
        &right_low,
        &bottom,
        &bottom_right,
    ];
    assert_eq!(tried_by(&tried, SpawnTest::Spot), segments.concat());
    let kinds: Vec<_> = tried.iter().map(|&(_, spawn_test)| spawn_test).collect();
    let spots = segments.concat().len();
    assert_eq!(
        kinds,
        [
            vec![SpawnTest::Spot; spots],
            vec![SpawnTest::Outer; 8],
            vec![SpawnTest::Inner; 20]
        ]
        .concat()
    );
    assert_eq!(
        tried_by(&tried, SpawnTest::Outer).first(),
        Some(&Point::new(9.5, 11.5))
    );
    assert_eq!(point, Ok(None));
}

/// With a gather point away from the walled corners (top right, bottom left) the first pass
/// tries nothing; the second takes the open top left corner's two segments (1, 2), then the
/// bottom side from the right (9) and the right side from the bottom (10) as one segment.
#[test]
fn second_pass_between_walled_corners_tries_the_bottom_right_corner() {
    let site = Site {
        position: Position::new(10.0, 10.0, 0.0),
        radius: Footprint::new(1.0, 1.0),
        goal: Some(Point::new(20.0, 5.0)),
        walls: [false, true, false, true],
        closest_goal: None,
    };
    let (tried, point) = all_blocked(&site);
    assert_eq!(
        tried_by(&tried, SpawnTest::Spot),
        [
            Point::new(8.5, 11.5),
            Point::new(9.0, 11.5),
            Point::new(9.5, 11.5),
            Point::new(8.5, 11.5),
            Point::new(8.5, 11.0),
            Point::new(8.5, 10.5),
            Point::new(11.5, 8.5),
            Point::new(11.0, 8.5),
            Point::new(11.5, 8.5),
            Point::new(11.5, 9.0)
        ]
    );
    assert_eq!(point, Ok(None));
}

/// A queued move of the centre's, which a new unit's notified walk forgets.
fn queued_move() -> QueuedOrder {
    QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::Move,
        priority: OrderPriority::of_data(5),
        target: None,
        target_owner: None,
        point: Position::new(40.0, 80.0, 7.0),
        range: Distance::new(2.0),
        shown: Point::new(40.0, 80.0),
        ready: false,
        target_kind: None,
    }
}

/// `FUN_140ace9c0` with a point: the new villager waits to move, takes the move order and
/// action with no target at the point, leaves the town bell and walks there (a `MoveTo` of
/// range 0); a new unit's queue empties, an unloaded unit's stays.
#[test]
fn gather_point_without_target_walks_the_new_unit_there() {
    use crate::movement::move_to::{MoveTo, MoveToAction, MoveWait};
    let walked = |source| {
        let mut world = forage_world();
        let unit = &mut world.players[1].objects[ListIndex::new(0)];
        world
            .parts
            .set_town_bell_flag(unit.id, BellShelter::Sheltered);
        world
            .parts
            .unit_ai
            .get_mut(unit.id)
            .unwrap()
            .orders
            .push(queued_move());
        world
            .gather_move(
                PlayerId::new(1),
                ListIndex::new(0),
                Position::new(40.0, 80.0, 0.0),
                source,
            )
            .unwrap();
        world
            .swap_remove_live(PlayerId::new(1), ListIndex::new(0))
            .unwrap()
    };
    let object = walked(OrderSource::Direct);
    assert_eq!(object.parts.mover().unwrap().waiting, 1);
    assert!(!object.parts.town_bell.flag.is_sheltered());
    let ai = object.parts.unit_ai.as_ref().unwrap();
    assert_eq!(
        (ai.current_order, ai.current_action, ai.current_target),
        (Some(OrderKind::Move), Some(AiAction::Move), None)
    );
    assert_eq!(ai.target_point, Position::new(40.0, 80.0, 0.0));
    assert_eq!(ai.orders, []);
    assert_eq!(
        object.parts.actions.list.move_to().copied(),
        Some(MoveToAction {
            sprite: None,
            state: MoveTo::Start,
            point: Position::new(40.0, 80.0, 0.0),
            target: None,
            range: Distance::ZERO,
            voice: Voice::Loud,
            on_target_moved: TargetMoved::Follows,
            wait: MoveWait::default(),
        })
    );
    let unloaded = walked(OrderSource::Queued);
    assert_eq!(
        unloaded.parts.unit_ai.as_ref().unwrap().orders,
        [queued_move()]
    );
    assert_eq!(
        unloaded.parts.actions.list.move_to().copied(),
        object.parts.actions.list.move_to().copied()
    );
}

/// A unit whose master does not move (the work command's slot 0xe8, `+0x740` not above 0; a
/// NaN does not move) only waits to move, whatever its own top speed; one whose master moves
/// walks without a top speed of its own. One that already acts, or one not in state 2, is not
/// modelled.
#[test]
fn gather_point_walk_waits_without_master_speed_and_stops_at_the_unmodelled() {
    let walk = |master_speed: f32, own: Speed| {
        let mut game = forage_world();
        let unit = &game.players[1].objects[ListIndex::new(0)];
        let (id, master_id) = (unit.id, unit.life.master_id);
        for masters in &mut game.model.masters {
            if let Some(master) = Arc::make_mut(masters).get_mut(master_id) {
                master.animated = Some(AnimatedPart {
                    max_speed: Speed::new(master_speed),
                });
            }
        }
        game.parts.mover_mut(id).unwrap().stats.speed = own;
        game.gather_move(
            PlayerId::new(1),
            ListIndex::new(0),
            Position::new(40.0, 80.0, 0.0),
            OrderSource::Direct,
        )
        .unwrap();
        assert_eq!(game.parts.mover_of(id).unwrap().waiting, 1);
        (
            game.parts.actions_of(id).list.move_to().is_some(),
            game.parts.unit_ai.get(id).unwrap().current_order,
        )
    };
    assert_eq!(walk(0.0, Speed::new(0.8)), (false, None));
    assert_eq!(walk(f32::NAN, Speed::new(0.8)), (false, None));
    assert_eq!(walk(0.8, Speed::ZERO), (true, Some(OrderKind::Move)));
    // a second walk finds the first's `MoveTo`
    let mut game = forage_world();
    game.gather_move(
        PlayerId::new(1),
        ListIndex::new(0),
        Position::new(40.0, 80.0, 0.0),
        OrderSource::Direct,
    )
    .unwrap();
    let reason = |world: &mut World| {
        world
            .gather_move(
                PlayerId::new(1),
                ListIndex::new(0),
                Position::new(41.0, 80.0, 0.0),
                OrderSource::Direct,
            )
            .unwrap_err()
            .reason()
    };
    assert_eq!(
        reason(&mut game),
        "a gather point's move of a unit with an action"
    );
    let mut world = forage_world();
    world.players[1].objects[ListIndex::new(0)].life.state = ObjectState::Dying;
    assert_eq!(
        reason(&mut world),
        "a gather point's move of a unit not in state 2"
    );
}

/// [`crate::world::tests::forage_world`] with the remains masters 800 (static, its storage of
/// flag 2 given to its owner and of flag 0 held, its death sprite 20 of eight facets starting
/// on a random one) and 801 (moving, its standing sprite 21 of eight facets).
fn remains_world() -> World {
    use crate::objects::masters::{Master, Masters};
    let mut world = forage_world();
    let graphic = |id, flags| Graphic {
        id,
        frames: Frame::of_data(10),
        facets: FacetCount::of_data(8),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags,
    };
    for (id, flags) in [
        (20, SpriteFlags::RANDOM_FIRST_FRAME),
        (21, SpriteFlags::ANIMATED),
    ] {
        let id = SpriteId::named(id);
        let _ = Arc::make_mut(&mut world.model.graphics).insert(id, graphic(id, flags));
    }
    let static_remains = Master {
        id: master_id(800),
        kind: MasterKind::STATIC,
        death_sprite: Some(SpriteId::named(20)),
        standing_sprite: None,
        storage: vec![
            (Some(attribute::HEADROOM), Amount::new(1.0), 2),
            (Some(attribute::POPULATION), Amount::new(5.0), 0),
        ],
        ..Master::default()
    };
    let moving_remains = Master {
        id: master_id(801),
        kind: MasterKind::MOVING,
        standing_sprite: Some(SpriteId::named(21)),
        ..moving_master(walker().stats)
    };
    for masters in &mut world.model.masters {
        *masters = Arc::new(Masters::new(
            masters
                .iter()
                .cloned()
                .chain([static_remains.clone(), moving_remains.clone()]),
        ));
    }
    world
}

/// The master's slot 0x40 for remains (`FUN_1411a45a0`, `FUN_141141640`): a static object gives
/// its owner its storage of flag 2, holds its entry of flag 0 and dies on its death sprite's
/// eight facets; moving remains face their dead object's facet (with their own sprite's
/// facets). Another type, a static object with hit points, a recyclable master, moving remains
/// without a facet or without a moving master are not modelled.
#[test]
fn remains_take_their_storage_and_face_their_dead_object_facet() {
    use crate::objects::lifecycle::Class;
    let made = |world: &World, id| world.model.masters[1].get(id).unwrap().clone();
    let mut game = remains_world();
    let food = game.players[1].attributes[4];
    let list_index = game
        .plain_object(
            PlayerId::new(1),
            master_id(800),
            &made(&game, master_id(800)),
            Position::new(30.5, 85.5, 0.0),
            Facing::Dead(None),
        )
        .unwrap();
    let players = &game.players[1].objects[list_index];
    assert_eq!(
        (
            players.life.class,
            players.life.attribute_kind,
            players.life.amount
        ),
        (Class::Static, Some(attribute::POPULATION), Amount::new(5.0))
    );
    assert_eq!(
        (players.life.death_facets, players.sprite),
        (Some(FacetCount::of_data(8)), None)
    );
    assert_eq!(game.players[1].attributes[4], food + Amount::new(1.0));
    let list_index = game
        .plain_object(
            PlayerId::new(1),
            master_id(801),
            &made(&game, master_id(801)),
            Position::new(31.5, 85.5, 0.0),
            Facing::Dead(Some(Facet::of_data(2))),
        )
        .unwrap();
    let unit = &game.players[1].objects[list_index];
    let facets = SpriteFacets {
        count: FacetCount::of_data(8),
        turns: false,
    };
    assert_eq!(
        game.parts.mover_of(unit.id).map(|mover| mover.angle),
        Some(angle_of_facet(Facet::of_data(2), facets))
    );
    let reason = |change: &dyn Fn(&mut Master), facing| {
        let mut world = remains_world();
        let mut master = made(&world, master_id(801));
        change(&mut master);
        world
            .plain_object(
                PlayerId::new(1),
                master.id,
                &master,
                Position::new(31.5, 85.5, 0.0),
                facing,
            )
            .unwrap_err()
            .reason()
    };
    let unchanged = |_: &mut Master| {};
    assert_eq!(
        reason(&|master| master.kind = MasterKind::COMBAT, Facing::Drawn),
        "remains of a type other than 10 and 20 to 39"
    );
    assert_eq!(
        reason(
            &|master| {
                master.kind = MasterKind::STATIC;
                master.hit_points = MasterHitPoints::of_data(5);
            },
            Facing::Drawn
        ),
        "remains with hit points (the sleeping list)"
    );
    assert_eq!(
        reason(
            &|master| master.recyclable = RecycleMode::RECYCLED,
            Facing::Drawn
        ),
        RECYCLABLE_NEW_OBJECT.reason()
    );
    assert_eq!(
        reason(&unchanged, Facing::Dead(None)),
        "moving remains of an object without a facet the slice holds"
    );
    assert_eq!(
        reason(&|master| master.move_stats = None, Facing::Drawn),
        "moving remains without a moving master"
    );
}

/// A map of 120 by 120 tiles of land that player 1 sees as `sight`, without objects.
fn plain(sight: TileSight) -> World {
    Scene::open(GridSize::new(120, 120)).sight(sight).build()
}

/// Player 1's house site (class 3, radii 1, obstructing, no height) on terrain 0, which `restriction`
/// rates; `human` or not.
const fn house_site(restriction: &Restriction, human: bool) -> BuildSite<'_> {
    BuildSite {
        player: PlayerId::new(1),
        checks: if human {
            SiteChecks::HUMAN
        } else {
            SiteChecks::empty()
        },
        radius: Footprint::new(1.0, 1.0),
        tall: false,
        class: class::BUILDING,
        obstructs: true,
        restriction,
        hill_mode: HillMode::ANY,
        placement: PlacementTerrains::NONE,
        passed: None,
    }
}

/// BUILD's static test: off the map 7, a forbidden terrain 2, a human who explored none of
/// the tiles 5 (a computer passes); a box that passes answers none; a zone mode is not
/// modelled.
#[test]
fn build_static_test_answers_the_map_edge_terrain_and_exploration() {
    use crate::production::spawn::build_placement_test::build_static_test;
    let open_values = Restriction::new(vec![1.0]);
    let open = house_site(&open_values, true);
    let seen = plain(TileSight::Seen);
    let flat = WorldMap { turns: &seen };
    assert_eq!(
        build_static_test(&open, &flat, Point::new(50.0, 50.0), &tuning().spawn)
            .map(|placement_code| placement_code.map(PlacementCode::code)),
        Ok(None)
    );
    assert_eq!(
        build_static_test(&open, &flat, Point::new(0.5, 50.0), &tuning().spawn)
            .map(|placement_code| placement_code.map(PlacementCode::code)),
        Ok(Some(7))
    );
    assert_eq!(
        build_static_test(&open, &flat, Point::new(119.5, 50.0), &tuning().spawn)
            .map(|placement_code| placement_code.map(PlacementCode::code)),
        Ok(Some(7))
    );
    let forbidden_values = Restriction::new(vec![0.0]);
    let forbidden = house_site(&forbidden_values, true);
    assert_eq!(
        build_static_test(&forbidden, &flat, Point::new(50.0, 50.0), &tuning().spawn)
            .map(|placement_code| placement_code.map(PlacementCode::code)),
        Ok(Some(2))
    );
    let unknown = plain(TileSight::Unknown);
    let dark = WorldMap { turns: &unknown };
    assert_eq!(
        build_static_test(&open, &dark, Point::new(50.0, 50.0), &tuning().spawn)
            .map(|placement_code| placement_code.map(PlacementCode::code)),
        Ok(Some(5))
    );
    let computer_values = Restriction::new(vec![1.0]);
    let computer = house_site(&computer_values, false);
    assert_eq!(
        build_static_test(&computer, &dark, Point::new(50.0, 50.0), &tuning().spawn)
            .map(|placement_code| placement_code.map(PlacementCode::code)),
        Ok(None)
    );
    let zoned = BuildSite {
        checks: SiteChecks::ZONES,
        ..open
    };
    assert_eq!(
        build_static_test(&zoned, &flat, Point::new(50.0, 50.0), &tuning().spawn)
            .map(|placement_code| placement_code.map(PlacementCode::code))
            .unwrap_err()
            .reason(),
        "BUILD's placement test in a zone mode"
    );
}

/// BUILD's hill mode 3 measures the box's first column and last row against the elevation of
/// the first column's last row (49, 50) of the house's box at (50, 50), as the spawn's does:
/// (50, 50) a step above that corner passes, (49, 49) two steps above it answers 3.
#[test]
fn build_static_test_measures_hill_mode_3_by_the_corner() {
    use crate::production::spawn::build_placement_test::build_static_test;
    let values = Restriction::new(vec![1.0]);
    let site = BuildSite {
        hill_mode: HillMode::ONE_STEP,
        ..house_site(&values, true)
    };
    let code = |raised: &[([i32; 2], u8)]| {
        let world = raised
            .iter()
            .fold(
                Scene::open(GridSize::new(120, 120)).elevation(3),
                |scene, &(tile, elevation)| scene.elevation_on([Tile::from(tile)], elevation),
            )
            .build();
        build_static_test(
            &site,
            &WorldMap { turns: &world },
            Point::new(50.0, 50.0),
            &tuning().spawn,
        )
        .map(|placement_code| placement_code.map(PlacementCode::code))
    };
    assert_eq!(code(&[([49, 50], 4), ([50, 50], 5)]), Ok(None));
    assert_eq!(code(&[([49, 49], 5)]), Ok(Some(3)));
}

/// BUILD's zone test runs behind the gate of `COMISS 0, rx; JC; COMISS 0, ry; JNC`
/// (0x141124d87): a NaN in either radius takes it (an error, as the zone mode is not
/// modelled), radii both at most 0 pass over it.
#[test]
fn nan_radius_takes_the_build_zone_test() {
    use crate::production::spawn::build_placement_test::build_static_test;
    let values = Restriction::new(vec![1.0]);
    let seen = plain(TileSight::Seen);
    let flat = WorldMap { turns: &seen };
    let zone_test = |radius| {
        let zoned = BuildSite {
            checks: SiteChecks::ZONES,
            radius,
            ..house_site(&values, false)
        };
        build_static_test(&zoned, &flat, Point::new(50.0, 50.0), &tuning().spawn)
            .map(|placement_code| placement_code.map(PlacementCode::code))
            .map_err(|error| error.reason())
    };
    for radius in [Footprint::new(f32::NAN, 0.0), Footprint::new(0.0, f32::NAN)] {
        assert_eq!(
            zone_test(radius),
            Err("BUILD's placement test in a zone mode"),
            "{radius:?}"
        );
    }
    for radius in [Footprint::ZERO, Footprint::new(-0.5, 0.0)] {
        assert_eq!(zone_test(radius), Ok(None), "{radius:?}");
    }
}

/// BUILD's scan reads 8 tiles either way of the point, from 0 to the map's last tile.
#[test]
fn build_scan_reads_eight_tiles_either_way_within_the_map() {
    use crate::production::spawn::build_placement_test::build_scan_range;
    assert_eq!(build_scan_range(50.5, 120, &tuning().spawn), (42, 58));
    assert_eq!(build_scan_range(2.0, 120, &tuning().spawn), (0, 10));
    assert_eq!(build_scan_range(115.0, 120, &tuning().spawn), (107, 119));
}

/// BUILD's sight test: in sight any object but one of fog visibility 3; in the fog one with a
/// fog visibility, and of those without only the player's own; on a tile never explored any;
/// a fogged object of fog visibility 3 is not modelled.
#[test]
fn build_scan_counts_what_the_player_sees_of_tile() {
    use crate::production::spawn::build_placement_test::build_sees;
    let site_values = Restriction::new(vec![1.0]);
    let site = house_site(&site_values, true);
    let seen = plain(TileSight::Seen);
    let flat = WorldMap { turns: &seen };
    let explored = plain(TileSight::Explored);
    let fogged = WorldMap { turns: &explored };
    let unknown = plain(TileSight::Unknown);
    let dark = WorldMap { turns: &unknown };
    let near_of = |player, fog_visibility| Near {
        player,
        fog_visibility,
        ..near_sheep(ObjectState::Alive)
    };
    let sees =
        |map: &WorldMap<'_>, near: Near, sight| build_sees(&site, map, &near, sight, &roles());
    assert_eq!(
        sees(
            &flat,
            near_of(PlayerId::new(2), FogVisibility::INVISIBLE),
            (true, true)
        ),
        Ok(true)
    );
    assert_eq!(
        sees(
            &flat,
            near_of(PlayerId::new(2), FogVisibility::ONLY_IN_FOG),
            (true, true)
        ),
        Ok(false)
    );
    assert_eq!(
        sees(
            &fogged,
            near_of(PlayerId::new(2), FogVisibility::VISIBLE),
            (true, false)
        ),
        Ok(true)
    );
    assert_eq!(
        sees(
            &fogged,
            near_of(PlayerId::new(2), FogVisibility::INVISIBLE),
            (true, false)
        ),
        Ok(false)
    );
    assert_eq!(
        sees(
            &fogged,
            near_of(PlayerId::new(1), FogVisibility::INVISIBLE),
            (true, false)
        ),
        Ok(true)
    );
    assert_eq!(
        sees(
            &dark,
            near_of(PlayerId::new(2), FogVisibility::INVISIBLE),
            (false, false)
        ),
        Ok(true)
    );
    assert_eq!(
        sees(
            &fogged,
            near_of(PlayerId::new(2), FogVisibility::ONLY_IN_FOG),
            (true, false)
        )
        .unwrap_err()
        .reason(),
        "a fogged object of fog visibility 3 near a BUILD"
    );
}

/// BUILD's scan ignores a dead house (a unit only past state 6), a moving unit where the site
/// does not obstruct, a wall under the player's own gate and another player's desolid house
/// (not one of class 51); it keeps a standing house.
#[test]
fn build_scan_ignores_the_dead_the_moving_own_walls_and_foreign_foundations() {
    use crate::production::spawn::build_placement_test::build_ignores;
    let site_values = Restriction::new(vec![1.0]);
    let site = house_site(&site_values, true);
    let seen = plain(TileSight::Seen);
    let flat = WorldMap { turns: &seen };
    let house = Near {
        kind: MasterKind::BUILDING,
        class: class::BUILDING,
        flags: NearFlags::empty(),
        player: PlayerId::new(2),
        state: ObjectState::Alive,
        ..near_sheep(ObjectState::Alive)
    };
    let ignores = |site: &BuildSite<'_>, near: Near| build_ignores(site, &flat, &near, &roles());
    assert!(!ignores(&site, house));
    assert!(ignores(
        &site,
        Near {
            state: ObjectState::Dying,
            ..house
        }
    ));
    let unit = Near {
        kind: MasterKind::COMBAT,
        state: ObjectState::Undead,
        ..house
    };
    assert!(!ignores(&site, unit));
    assert!(ignores(
        &site,
        Near {
            state: ObjectState::Removed,
            ..unit
        }
    ));
    let walking = Near {
        flags: NearFlags::MOVING,
        ..unit
    };
    assert!(!ignores(&site, walking));
    let open = BuildSite {
        obstructs: false,
        ..site
    };
    assert!(ignores(&open, walking));
    let gate = BuildSite {
        class: class::GATE,
        ..site
    };
    let own_wall = Near {
        class: class::WALL,
        player: PlayerId::new(1),
        ..house
    };
    assert!(ignores(&gate, own_wall));
    assert!(!ignores(
        &gate,
        Near {
            player: PlayerId::new(2),
            ..own_wall
        }
    ));
    let foundation = Near {
        flags: NearFlags::DESOLID,
        ..house
    };
    assert!(ignores(&site, foundation));
    assert!(!ignores(
        &site,
        Near {
            class: class::CLASS_51,
            ..foundation
        }
    ));
}

/// BUILD's scan passes over a moving unit of the player or Gaia but not another's, one
/// buildable on, and a felled tree off a forest but not on one or with more hit points.
#[test]
fn build_scan_passes_over_moving_friends_buildable_ground_and_felled_trees() {
    use crate::production::spawn::build_placement_test::build_passes;
    let site_values = Restriction::new(vec![1.0]);
    let site = house_site(&site_values, true);
    let seen = plain(TileSight::Seen);
    let flat = WorldMap { turns: &seen };
    let walker = Near {
        kind: MasterKind::COMBAT,
        class: class::CLASS_6,
        flags: NearFlags::MOVING,
        player: PlayerId::new(1),
        state: ObjectState::Alive,
        ..near_sheep(ObjectState::Unfinished)
    };
    let passes = |near: Near| build_passes(&site, &flat, &near, &roles());
    assert!(passes(walker));
    assert!(passes(Near {
        player: PlayerId::new(0),
        ..walker
    }));
    assert!(!passes(Near {
        player: PlayerId::new(2),
        ..walker
    }));
    assert!(!passes(Near {
        flags: NearFlags::empty(),
        ..walker
    }));
    assert!(!passes(Near {
        state: ObjectState::Dying,
        ..walker
    }));
    let still = Near {
        flags: NearFlags::empty(),
        ..walker
    };
    assert!(passes(Near {
        buildable_on: BuildableOn::YES,
        ..still
    }));
    let stump = Near {
        class: class::TREE,
        kind: MasterKind::STATIC,
        hp: HitPoints::new(1.0),
        ..still
    };
    assert!(passes(stump));
    assert!(!passes(Near {
        flags: NearFlags::FOREST_TILE,
        ..stump
    }));
    assert!(!passes(Near {
        hp: HitPoints::new(2.0),
        ..stump
    }));
}

/// What blocks a foundation at (21, 21): another player's house whose box overlaps it; not a
/// razed one, a moving unit of the player, an object built on, a felled tree off a forest, a
/// wall under the player's own gate, another's desolid foundation or an object beyond the
/// box; an object of obstruction type 11 is not modelled.
#[test]
fn build_blockers_are_the_standing_objects_whose_box_overlaps() {
    use crate::production::spawn::build_placement_test::blocks_build;
    let site_values = Restriction::new(vec![1.0]);
    let site = house_site(&site_values, true);
    let seen = plain(TileSight::Seen);
    let flat = WorldMap { turns: &seen };
    let house = Near {
        id: object_id(500),
        position: Position::new(20.5, 20.5, 0.0),
        radius: Footprint::new(1.0, 1.0),
        kind: MasterKind::BUILDING,
        class: class::BUILDING,
        master: ids::HOUSE,
        obstruction_kind: ObstructionKind::KIND_2,
        flags: NearFlags::empty(),
        player: PlayerId::new(2),
        state: ObjectState::Alive,
        ..near_sheep(ObjectState::Alive)
    };
    let blocks = |site: &BuildSite<'_>, near: Near| {
        blocks_build(site, &flat, &near, Point::new(21.0, 21.0), &roles())
    };
    assert!(blocks(&site, house));
    for passed in [
        Near {
            state: ObjectState::Dying,
            ..house
        },
        Near {
            kind: MasterKind::COMBAT,
            player: PlayerId::new(1),
            flags: NearFlags::MOVING,
            ..house
        },
        Near {
            buildable_on: BuildableOn::YES,
            ..house
        },
        Near {
            class: class::TREE,
            kind: MasterKind::STATIC,
            hp: HitPoints::new(1.0),
            ..house
        },
        Near {
            flags: NearFlags::DESOLID,
            ..house
        },
        Near {
            position: Position::new(30.5, 20.5, 0.0),
            ..house
        },
    ] {
        assert!(!blocks(&site, passed), "{passed:?}");
    }
    let gate = BuildSite {
        class: class::GATE,
        ..site
    };
    let wall = Near {
        class: class::WALL,
        player: PlayerId::new(1),
        ..house
    };
    assert!(!blocks(&gate, wall));
    assert!(blocks(&site, wall));
    // obstruction types 11 to 13 take the outline's box: one that does not reach the site, or
    // a negative height, passes; a height of 0 counts as 1 for a building
    let outlined = |outline| Near {
        position: Position::new(22.6, 21.0, 0.0),
        obstruction_kind: ObstructionKind::KIND_11,
        outline,
        ..house
    };
    assert!(blocks(&site, outlined(Radii::new(1.0, 1.0, 2.0))));
    assert!(!blocks(&site, outlined(Radii::new(0.4, 0.4, 2.0))));
    assert!(blocks(&site, outlined(Radii::new(1.0, 1.0, 0.0))));
    assert!(!blocks(&site, outlined(Radii::new(1.0, 1.0, -1.0))));
}

/// The Build's test ([`SiteChecks::SCANS`]) runs the static scans for a site with a height: they
/// find player 2's dying house 600 at (21.5, 21.5) (state 4) that BUILD's own scan passes over,
/// but not the test's object (`p12`), a desolid foundation, or anything for a site without a
/// height.
#[test]
fn the_build_static_scans_find_dying_house_build_passes_over() {
    let house = |desolid| SceneObject {
        state: ObjectState::DyingSprite,
        desolid,
        ..SceneObject::new(
            600,
            PlayerId::new(2),
            house_master(),
            Position::new(21.5, 21.5, 0.0),
        )
    };
    let build_values = Restriction::new(vec![1.0, 0.0]);
    let build = house_site(&build_values, false);
    let test = |site: &BuildSite<'_>, house: SceneObject| {
        let world = Scene::open(GridSize::new(120, 120))
            .players(2)
            .object(house)
            .build();
        build_placement(
            site,
            &WorldMap { turns: &world },
            Point::new(21.0, 21.0),
            &roles(),
            &tuning().spawn,
        )
        .unwrap()
        .code()
    };
    assert_eq!(test(&build, house(false)), 0);
    let scans = BuildSite {
        tall: true,
        checks: SiteChecks::SCANS,
        ..build
    };
    assert_eq!(test(&scans, house(false)), 6);
    assert_eq!(
        test(
            &BuildSite {
                passed: Some(PassedObject {
                    id: object_id(600),
                    annexes: &[],
                }),
                ..scans
            },
            house(false)
        ),
        0
    );
    assert_eq!(test(&scans, house(true)), 0);
    assert_eq!(
        test(
            &BuildSite {
                tall: false,
                ..scans
            },
            house(false)
        ),
        0
    );
}

/// The Build's test of a site whose radii are both at most 0 answers 0 whatever
/// `FUN_141123f80` found (`XOR AL, AL` at 0x141264f09), here off the map (7), which BUILD's
/// test without the scans keeps; a NaN in either radius is not at most 0 and keeps the code.
#[test]
fn build_scans_free_site_without_radii_off_the_map() {
    let values = Restriction::new(vec![1.0]);
    let world = Scene::open(GridSize::new(120, 120)).build();
    let test = |checks, radius, x, y| {
        let site = BuildSite {
            checks,
            radius,
            tall: true,
            ..house_site(&values, false)
        };
        build_placement(
            &site,
            &WorldMap { turns: &world },
            Point::new(x, y),
            &roles(),
            &tuning().spawn,
        )
        .unwrap()
        .code()
    };
    for radius in [Footprint::ZERO, Footprint::new(-0.5, 0.0)] {
        assert_eq!(test(SiteChecks::SCANS, radius, -5.0, 50.0), 0, "{radius:?}");
        assert_eq!(
            test(SiteChecks::empty(), radius, -5.0, 50.0),
            7,
            "{radius:?}"
        );
    }
    assert_eq!(
        test(SiteChecks::SCANS, Footprint::new(f32::NAN, 0.0), 50.0, -1.0),
        7
    );
    assert_eq!(
        test(SiteChecks::SCANS, Footprint::new(0.0, f32::NAN), -1.0, 50.0),
        7
    );
}

/// The slot's own scan (`FUN_141264db0`) runs whatever `FUN_141123f80` answered: a house that
/// overlaps a site past the map's west edge answers 6 in the Build's test, while BUILD's test
/// keeps the edge's 7, and so does the Build's test of a site without a height or without the
/// house.
#[test]
fn build_own_scan_answers_over_the_map_edge() {
    let values = Restriction::new(vec![1.0]);
    let test = |checks, tall, houses: &[SceneObject]| {
        let world = houses
            .iter()
            .cloned()
            .fold(
                Scene::open(GridSize::new(120, 120)).players(2),
                Scene::object,
            )
            .build();
        let site = BuildSite {
            checks,
            tall,
            ..house_site(&values, false)
        };
        build_placement(
            &site,
            &WorldMap { turns: &world },
            Point::new(0.5, 21.0),
            &roles(),
            &tuning().spawn,
        )
        .unwrap()
        .code()
    };
    let house = [SceneObject::new(
        600,
        PlayerId::new(2),
        house_master(),
        Position::new(1.5, 21.5, 0.0),
    )];
    assert_eq!(test(SiteChecks::SCANS, true, &house), 6);
    assert_eq!(test(SiteChecks::empty(), true, &house), 7);
    assert_eq!(test(SiteChecks::SCANS, false, &house), 7);
    assert_eq!(test(SiteChecks::SCANS, true, &[]), 7);
}

/// Player 2's house 600 at (60.5, 60.5) and its annex, the house 601 at `annex_at`, on a map of
/// 120 by 120 tiles.
fn annexed_house(annex_at: Position) -> World {
    Scene::open(GridSize::new(120, 120))
        .players(2)
        .object(SceneObject::new(
            600,
            PlayerId::new(2),
            house_master(),
            Position::new(60.5, 60.5, 0.0),
        ))
        .object(SceneObject {
            annex_of: Some(MainBuildingId::new(object_id(600))),
            ..SceneObject::new(601, PlayerId::new(2), house_master(), annex_at)
        })
        .build()
}

/// The static scans pass over the test's object (`p12`) alone (`FUN_141123f80`'s
/// `lVar16 != param_12`, `FUN_141264db0`'s `plVar7 != param_12`), not its annexes, which only the
/// building master's own scan passes over (`FUN_14123d090`): player 2's house 601 at
/// (21.5, 21.5), an annex of the house 600 the Build's test passes over, answers 6 there, under
/// the feature flags too (they spare an annex in the slot's own scan only); BUILD's test, without
/// the static scans, passes over it, and finds it once the annexes are not passed.
#[test]
fn build_static_scans_count_annex_of_passed_building() {
    let values = Restriction::new(vec![1.0]);
    let mut world = annexed_house(Position::new(21.5, 21.5, 0.0));
    let test = |world: &World, checks, passed| {
        let site = BuildSite {
            checks,
            tall: true,
            passed,
            ..house_site(&values, false)
        };
        build_placement(
            &site,
            &WorldMap { turns: world },
            Point::new(21.0, 21.0),
            &roles(),
            &tuning().spawn,
        )
        .unwrap()
        .code()
    };
    let annexes = [object_id(601)];
    let main = Some(PassedObject {
        id: object_id(600),
        annexes: &annexes,
    });
    let main_alone = Some(PassedObject {
        id: object_id(600),
        annexes: &[],
    });
    assert_eq!(test(&world, SiteChecks::SCANS, main), 6);
    assert_eq!(test(&world, SiteChecks::empty(), main), 0);
    assert_eq!(test(&world, SiteChecks::empty(), main_alone), 6);
    world.feature_flags = FeatureFlags::On;
    assert_eq!(test(&world, SiteChecks::SCANS, main), 6);
}

/// Under the feature flags (`DAT_143f01833`) the slot's own scan passes over a building annexed
/// to the test's object (its `+0xbb8`, `p12` not null) in the Build's test as in the spawn's:
/// player 2's house 601, an annex of the house 600, overlaps a site past the map's west edge and
/// answers 6 without the flags; with them the edge's 7 stays, whatever annexes the test passes
/// over, unless the test has no object.
#[test]
fn build_own_scan_under_feature_flags_passes_over_annex_of_passed_building() {
    let values = Restriction::new(vec![1.0]);
    let mut world = annexed_house(Position::new(1.5, 21.5, 0.0));
    let test = |world: &World, passed| {
        let site = BuildSite {
            checks: SiteChecks::SCANS,
            tall: true,
            passed,
            ..house_site(&values, false)
        };
        build_placement(
            &site,
            &WorldMap { turns: world },
            Point::new(0.5, 21.0),
            &roles(),
            &tuning().spawn,
        )
        .unwrap()
        .code()
    };
    let annexes = [object_id(601)];
    let main = Some(PassedObject {
        id: object_id(600),
        annexes: &annexes,
    });
    let main_alone = Some(PassedObject {
        id: object_id(600),
        annexes: &[],
    });
    assert_eq!(test(&world, main), 6);
    world.feature_flags = FeatureFlags::On;
    assert_eq!(test(&world, main), 7);
    assert_eq!(test(&world, main_alone), 7);
    assert_eq!(test(&world, None), 6);
}

/// A NaN in either radius passes the static scans' gates (`JC` at 0x141124d8b and
/// 0x141264f01), and their overlap tests (`JBE` at 0x141125240 and 0x1412654a1) find nothing for
/// it: the dying house 600 at (21.5, 21.5) that both find for radii 1 does not block it.
#[test]
fn nan_radius_static_scans_find_nothing() {
    let values = Restriction::new(vec![1.0, 0.0]);
    let world = Scene::open(GridSize::new(120, 120))
        .players(2)
        .object(SceneObject {
            state: ObjectState::DyingSprite,
            ..SceneObject::new(
                600,
                PlayerId::new(2),
                house_master(),
                Position::new(21.5, 21.5, 0.0),
            )
        })
        .build();
    let test = |radius| {
        let site = BuildSite {
            checks: SiteChecks::SCANS,
            radius,
            tall: true,
            ..house_site(&values, false)
        };
        build_placement(
            &site,
            &WorldMap { turns: &world },
            Point::new(21.0, 21.0),
            &roles(),
            &tuning().spawn,
        )
        .unwrap()
        .code()
    };
    assert_eq!(test(Footprint::new(1.0, 1.0)), 6);
    assert_eq!(test(Footprint::new(f32::NAN, 1.0)), 0);
    assert_eq!(test(Footprint::new(1.0, f32::NAN)), 0);
}

/// A 3×3 map whose tile `index` (row by row) is of terrain `kinds[index]` and whose terrain `kind`
/// has the flags `flags[kind]`, for the placement terrains' test ([`placement_terrains`]).
fn terrains(kinds: [u8; 9], flags: Vec<TerrainFlags>) -> World {
    let scene = (0..9).fold(
        Scene::open(GridSize::new(3, 3)).terrains(flags),
        |scene, index| {
            let tile = Tile::new(index % 3, index / 3);
            scene.terrain_on([tile], TerrainId::named(kinds[index as usize]))
        },
    );
    scene.build()
}

/// The point's tile must be of one of the master's terrains or share shallow water or shallows
/// with them; a terrain of none beside a set one reads flags the map has not
/// got, which is not modelled.
#[test]
fn placement_terrain_takes_its_own_tile_or_one_of_shared_flag() {
    use crate::production::spawn::terrains::point_terrain_allows;
    // terrains 0 (land), 1 and 2 (shallow water), 3 (shallows)
    let world = terrains(
        [0, 1, 2, 3, 0, 0, 0, 0, 0],
        vec![
            TerrainFlags::NONE,
            TerrainFlags::SHALLOW_WATER,
            TerrainFlags::SHALLOW_WATER,
            TerrainFlags::SHALLOWS,
        ],
    );
    let map = WorldMap { turns: &world };
    let terrain = |id: u8| Some(TerrainId::named(id));
    let at = |index: i32| Tile::new(index % 3, index / 3);
    assert_eq!(
        point_terrain_allows(&map, [terrain(1), terrain(3)], at(1)),
        Ok(true)
    );
    assert_eq!(
        point_terrain_allows(&map, [terrain(1), terrain(3)], at(2)),
        Ok(true)
    );
    assert_eq!(
        point_terrain_allows(&map, [terrain(2), terrain(2)], at(3)),
        Ok(false)
    );
    assert_eq!(
        point_terrain_allows(&map, [terrain(3), terrain(3)], at(3)),
        Ok(true)
    );
    assert_eq!(
        point_terrain_allows(&map, [terrain(1), terrain(2)], at(0)),
        Ok(false)
    );
    assert!(point_terrain_allows(&map, [terrain(1), None], at(0)).is_err());
}

/// The side terrains: the master's own, then every other terrain with ice (then beach) that one
/// of its own has, in the terrains' order.
#[test]
fn side_terrains_take_the_others_of_shared_flag() {
    use crate::production::spawn::terrains::side_terrains;
    let world = terrains(
        [0; 9],
        vec![
            TerrainFlags::NONE,
            TerrainFlags::ICE,
            TerrainFlags::ICE,
            TerrainFlags::BEACH,
            TerrainFlags::ICE | TerrainFlags::BEACH,
        ],
    );
    let map = WorldMap { turns: &world };
    let terrain = |id: u8| Some(TerrainId::named(id));
    let ids = |bytes: &[u8]| {
        bytes
            .iter()
            .copied()
            .map(TerrainId::named)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        side_terrains(&map, [terrain(0), terrain(0)]),
        Ok(ids(&[0, 0]))
    );
    assert_eq!(
        side_terrains(&map, [terrain(1), terrain(0)]),
        Ok(ids(&[1, 0, 2, 4]))
    );
    assert_eq!(
        side_terrains(&map, [terrain(3), terrain(3)]),
        Ok(ids(&[3, 3, 4]))
    );
    assert!(side_terrains(&map, [terrain(1), None]).is_err());
}

/// The placement terrains of a master: with terrains the point's tile must be of one and the box
/// shrinks to it; with side terrains a tile beside the box must be of one.
#[test]
fn master_placement_terrains_test_the_point_and_the_tiles_beside() {
    // the centre (1, 1) of terrain 5, the tile right of it (2, 1) of terrain 6
    let world = terrains([0, 0, 0, 0, 5, 6, 0, 0, 0], vec![TerrainFlags::NONE; 8]);
    let map = WorldMap { turns: &world };
    let terrain = |id: u8| Some(TerrainId::named(id));
    let centre = Point::new(1.5, 1.5);
    let whole = TileRect::new(0, 0, 2, 2);
    let test = |placement| placement_terrains(placement, &map, centre, whole);
    assert_eq!(test(PlacementTerrains::NONE), Ok(true));
    let on = |first, second| PlacementTerrains {
        on: [first, second],
        ..PlacementTerrains::NONE
    };
    assert_eq!(test(on(terrain(5), terrain(5))), Ok(true));
    assert_eq!(test(on(terrain(6), terrain(6))), Ok(false));
    let beside = |first, second| PlacementTerrains {
        beside: [first, second],
        ..PlacementTerrains::NONE
    };
    // the whole map's box has no tile beside it; the centre's tile has 6 to its right
    assert_eq!(test(beside(terrain(6), terrain(6))), Ok(false));
    assert_eq!(
        test(PlacementTerrains {
            beside: [terrain(6), terrain(6)],
            on: [terrain(5), terrain(5)],
        }),
        Ok(true)
    );
    assert_eq!(
        test(PlacementTerrains {
            beside: [terrain(7), terrain(7)],
            on: [terrain(5), terrain(5)],
        }),
        Ok(false)
    );
}

/// The building of the closest spot's tests: at (10, 10), of x radius 1 (its y radius 3 is not
/// read).
fn closest_site() -> Site {
    Site {
        position: Position::new(10.0, 10.0, 0.0),
        radius: Footprint::new(1.0, 3.0),
        goal: None,
        closest_goal: None,
        walls: [false; 4],
    }
}

/// Every point of `FUN_1411c52f0`'s square about [`closest_site`] for a unit of radii `unit`
/// toward `goal`, in the order it tests them, all blocked.
fn closest_tried(unit: Footprint, goal: Point) -> Vec<Point> {
    let mut tried = Vec::new();
    let found = closest_spot(
        &closest_site(),
        unit,
        goal,
        &mut |point, spawn_test| {
            assert_eq!(spawn_test, SpawnTest::Closest);
            tried.push(point);
            Ok(Tested::Blocked { stays: true })
        },
        &tuning().spawn,
    );
    assert_eq!(found, Ok(None));
    tried
}

/// `FUN_1411c52f0` about a building at (10, 10) of x radius 1 (its y radius not read) for a unit
/// of radius 0.5: the square at 1 + 0.5 + the gap from the centre, each side walked in halves (a
/// step of the unit's radius): the bottom from the left, the right from the top, the top from
/// the right, the left from the bottom. Sorted by the squared distance from the goal far to the
/// right, the right side comes first; a corner two sides share keeps the walk's order
/// (`std::sort`'s insertion sort below 33 points).
#[test]
fn closest_spot_tries_square_points_nearest_goal_first() {
    let reach = 1.0 + 0.5 + tuning().spawn.gap;
    let (left, right, low, high) = (10.0 - reach, 10.0 + reach, 10.0 - reach, 10.0 + reach);
    let middle = |from: f32, to: f32| (to - from) * 0.5 + from;
    let tried = closest_tried(Footprint::new(0.5, 0.5), Point::new(100.0, 10.3));
    assert_eq!(
        tried,
        [
            Point::new(right, middle(high, low)),
            Point::new(right, high),
            Point::new(right, high),
            Point::new(right, low),
            Point::new(right, low),
            Point::new(middle(right, left), high),
            Point::new(middle(left, right), low),
            Point::new(left, middle(low, high)),
            Point::new(left, high),
            Point::new(left, high),
            Point::new(left, low),
            Point::new(left, low),
        ]
    );
}

/// The step is the unit's x radius but at least 0.1 (`std::max`; a NaN radius takes 0.1), and a
/// side's shares go on while at most 1: a step of 0.1 sums to 1.0000001 after ten points, so
/// each side has ten; a step of 2 leaves each side its first corner alone.
#[test]
fn closest_spot_steps_by_unit_radius_but_at_least_tenth() {
    let goal = Point::new(100.0, 10.3);
    assert_eq!(closest_tried(Footprint::new(0.05, 0.05), goal).len(), 40);
    assert_eq!(
        closest_tried(Footprint::new(f32::NAN, 0.05), goal).len(),
        40
    );
    let reach = 1.0 + 2.0 + tuning().spawn.gap;
    let (low, high) = (10.0 - reach, 10.0 + reach);
    assert_eq!(
        closest_tried(Footprint::new(2.0, 0.05), goal),
        [
            Point::new(high, high),
            Point::new(high, high),
            Point::new(low, low),
            Point::new(low, low),
        ]
    );
}

/// The first free point of the sorted square ends the test with it.
#[test]
fn closest_spot_takes_first_free_point() {
    let mut calls = 0;
    let found = closest_spot(
        &closest_site(),
        Footprint::new(0.5, 0.5),
        Point::new(100.0, 10.3),
        &mut |point, _| {
            calls += 1;
            Ok(if point.y() < 9.0 {
                Tested::Free
            } else {
                Tested::Blocked { stays: true }
            })
        },
        &tuning().spawn,
    );
    let reach = 1.0 + 0.5 + tuning().spawn.gap;
    assert_eq!(found, Ok(Some(Point::new(10.0 + reach, 10.0 - reach))));
    assert_eq!(calls, 4);
}

/// Under the world's feature flags with a goal (`FUN_1411d12d0` at 0x1411d1343), the search
/// tests the closest spot's square first, then, all of it blocked, walks its segments and rings
/// as without the flags; a free point of the square ends the search there.
#[test]
fn spawn_search_under_feature_flags_tests_closest_square_first() {
    let goal = Point::new(100.0, 10.3);
    let site = Site {
        closest_goal: Some(goal),
        ..closest_site()
    };
    let (tried, found) = all_blocked(&site);
    let (without, _) = all_blocked(&closest_site());
    assert_eq!(found, Ok(None));
    let square = closest_tried(Footprint::new(0.5, 0.5), goal);
    assert_eq!(tried_by(&tried, SpawnTest::Closest), square);
    assert_eq!(tried.get(square.len()..), Some(&without[..]));
    let mut asked = Vec::new();
    let found = spawn_point(
        &site,
        Footprint::new(0.5, 0.5),
        &mut |_, spawn_test| {
            asked.push(spawn_test);
            Ok(Tested::Free)
        },
        &|_| Ok(None),
        &tuning().spawn,
    );
    assert_eq!(found, Ok(square.first().copied()));
    assert_eq!(asked, [SpawnTest::Closest]);
}

/// The gather point of the goals' tests, on `target`.
const fn goal_gather(target: Option<ObjectId>) -> GatherPoint {
    GatherPoint {
        state: GatherPointState::Point,
        point: Position::new(50.0, 60.0, 0.0),
        target,
        target_master: None,
    }
}

/// `FUN_1411d12d0`'s goal: the caller's point; without its x the first location of the building
/// unit AI's command queue when the building has no gather point, or its gather point when that
/// is not on the building; else the caller's point stands (a coordinate of [`NONE`] none). A
/// NaN x is not none (`UCOMISS`, `JP` at 0x1411d1774): the caller's point stands, its bits kept.
#[test]
fn search_goal_is_caller_point_queue_or_gather_point() {
    let building = object_id(700);
    let none = Point::new(NONE, NONE);
    let queued = Some(Point::new(30.0, 40.0));
    let given = Point::new(5.0, 6.0);
    assert_eq!(
        search_goal(given, Some(goal_gather(None)), building, queued),
        Some(given)
    );
    let half = Point::new(5.0, NONE);
    assert_eq!(search_goal(half, None, building, queued), Some(half));
    assert_eq!(search_goal(none, None, building, queued), queued);
    assert_eq!(search_goal(none, None, building, None), None);
    assert_eq!(
        search_goal(none, Some(goal_gather(None)), building, queued),
        Some(Point::new(50.0, 60.0))
    );
    assert_eq!(
        search_goal(none, Some(goal_gather(Some(building))), building, queued),
        None
    );
    let only_y = Point::new(NONE, 6.0);
    assert_eq!(
        search_goal(only_y, Some(goal_gather(Some(building))), building, queued),
        Some(only_y)
    );
    assert_eq!(
        search_goal(
            Point::new(f32::NAN, NONE),
            Some(goal_gather(None)),
            building,
            queued
        )
        .map(|goal| goal.get().map(f32::to_bits)),
        Some([f32::NAN.to_bits(), NONE.to_bits()])
    );
}

/// `FUN_1411c52f0`'s goal: none without a gather point (`+0xc30` 0) or with one on the building
/// (`+0xc40`); else each coordinate the caller's, or where it is none ([`NONE`]) the first
/// location of the building's unit AI's command queue, or without a queue the gather point's. A
/// NaN coordinate is not none (`UCOMISS`, `JP` at 0x1411c53e0 for x and 0x1411c540b for y): it
/// stays, its bits kept, and the other coordinate is filled.
#[test]
fn closest_goal_needs_gather_point_off_building_and_fills_each_coordinate() {
    let building = object_id(700);
    let none = Point::new(NONE, NONE);
    let queued = Some(Point::new(30.0, 40.0));
    assert_eq!(closest_goal(none, None, building, queued), None);
    assert_eq!(
        closest_goal(none, Some(goal_gather(Some(building))), building, queued),
        None
    );
    assert_eq!(
        closest_goal(none, Some(goal_gather(Some(object_id(9)))), building, None),
        Some(Point::new(50.0, 60.0))
    );
    assert_eq!(
        closest_goal(none, Some(goal_gather(None)), building, queued),
        Some(Point::new(30.0, 40.0))
    );
    assert_eq!(
        closest_goal(
            Point::new(5.0, NONE),
            Some(goal_gather(None)),
            building,
            queued
        ),
        Some(Point::new(5.0, 40.0))
    );
    assert_eq!(
        closest_goal(
            Point::new(NONE, 6.0),
            Some(goal_gather(None)),
            building,
            None
        ),
        Some(Point::new(50.0, 6.0))
    );
    let bits_of = |from| {
        closest_goal(from, Some(goal_gather(None)), building, queued)
            .map(|goal| goal.get().map(f32::to_bits))
    };
    assert_eq!(
        bits_of(Point::new(f32::NAN, NONE)),
        Some([f32::NAN.to_bits(), 40.0_f32.to_bits()])
    );
    assert_eq!(
        bits_of(Point::new(NONE, f32::NAN)),
        Some([30.0_f32.to_bits(), f32::NAN.to_bits()])
    );
}

/// The call's `[0x28]` (`param_6`) and `[0x30]` (`param_7`): `FUN_141123f80` needs a tile of the
/// box the player explored (its code 5) and tests the master's hill mode (its code 3) only where
/// the call asks for each. `FUN_1411c52f0`'s call asks for neither: a villager on a map its
/// player never explored is free there, and so is one of hill mode 2 on a slope. Without the hill
/// test the port reads no base elevation, which a map without its elevations cannot give.
#[test]
fn placement_tests_explored_tiles_and_hill_mode_only_where_its_call_asks() {
    let restriction = Restriction::new(vec![1.0, 0.0]);
    let villager = placed_villager(&restriction);
    let unexplored = Scene::open(GridSize::new(120, 120))
        .sight(TileSight::Unknown)
        .build();
    let at = |call| {
        placement(
            &villager,
            &WorldMap { turns: &unexplored },
            Point::new(60.5, 60.5),
            call,
            &roles(),
            &tuning().spawn,
        )
    };
    let closest = PlacementCall {
        explored: ExploredTest::Skipped,
        hills: HillTest::Skipped,
        ..COUNTED
    };
    let explored = PlacementCall {
        explored: ExploredTest::Tested,
        ..closest
    };
    let hills = PlacementCall {
        hills: HillTest::Tested,
        ..closest
    };
    assert_eq!(at(explored), Ok(Tested::Blocked { stays: false }));
    assert_eq!(at(hills), Ok(Tested::Free));
    assert_eq!(at(closest), Ok(Tested::Free));
    let flat = hill_unit(&restriction, HillMode::FLAT);
    assert_eq!(
        on_hills(&flat, hills, west_slope),
        Ok(Tested::Blocked { stays: false })
    );
    assert_eq!(on_hills(&flat, explored, west_slope), Ok(Tested::Free));
    let one_step = hill_unit(&restriction, HillMode::ONE_STEP);
    let mut without_elevations = Scene::open(GridSize::new(120, 120)).build();
    without_elevations
        .spatial
        .terrain
        .as_mut()
        .unwrap()
        .elevations
        .clear();
    let elevations_unknown = |call| {
        placement(
            &one_step,
            &WorldMap {
                turns: &without_elevations,
            },
            Point::new(50.0, 50.0),
            call,
            &roles(),
            &tuning().spawn,
        )
    };
    assert_eq!(elevations_unknown(closest), Ok(Tested::Free));
    assert_eq!(
        elevations_unknown(hills).map_err(|error| error.reason()),
        Err("a hill mode where the elevations are not known")
    );
}

/// The forage world's town centre 700 (at (20, 89.5), radii 2) with its gather point at (40, 80)
/// on `target` (state 0xff, so the new unit stays where it spawns), under the world's feature
/// flags `feature_flags`; the villager it spawns has its standing sprite.
fn centre_with_gather_point(feature_flags: FeatureFlags, target: Option<ObjectId>) -> World {
    let mut world = forage_world();
    world.feature_flags = feature_flags;
    for masters in &mut world.model.masters {
        Arc::make_mut(masters)
            .get_mut(ids::VILLAGER_MALE)
            .unwrap()
            .standing_sprite = Some(SpriteId::named(300));
    }
    let _ = Arc::make_mut(&mut world.model.graphics).insert(
        SpriteId::named(300),
        Graphic {
            id: SpriteId::named(300),
            frames: Frame::of_data(10),
            facets: FacetCount::of_data(8),
            speed_multiplier: SpeedMultiplier::ONE,
            frame_duration: Seconds::new(0.1),
            replay_delay: Seconds::ZERO,
            flags: SpriteFlags::ANIMATED,
        },
    );
    world.players[1].attributes[attribute::IN_PRODUCTION.index()] = Amount::new(3.0);
    let centre = world.players[1].objects[ListIndex::new(2)].id;
    assert_eq!(centre, object_id(700));
    let gather = Some(GatherPoint {
        state: GatherPointState::Kept,
        point: Position::new(40.0, 80.0, 0.0),
        target,
        target_master: None,
    });
    world.set_part(centre, Part::GatherPoint(gather)).unwrap();
    world
}

/// The place of the villager the town centre of `world` ([`centre_with_gather_point`]) spawns.
fn spawned(mut world: World) -> Position {
    assert_eq!(
        world
            .spawn(
                PlayerId::new(1),
                ListIndex::new(2),
                ids::VILLAGER_MALE,
                Amount::new(1.0),
            )
            .unwrap(),
        SpawnOutcome::Made
    );
    world.players[1].objects.last().unwrap().position()
}

/// `FUN_140ad0240` under the world's feature flags: the spawn search's first test
/// (`FUN_1411c52f0`) puts the villager on the square about the centre (2 + its radius + the gap
/// out) at the point nearest the gather point, or nearest the first location of the centre's
/// command queue where it has one (a building's or a tower's unit AI), which is free on the
/// open map, whether the player explored it or not and whatever the villager's hill mode (its
/// call tests neither). Without the flags, or with the gather point on the centre, the search
/// walks its segments.
#[test]
fn feature_flags_spawn_villager_on_square_point_nearest_gather_point() {
    let radius = forage_world()
        .master(PlayerId::new(1), ids::VILLAGER_MALE)
        .unwrap()
        .radius;
    let site = Site {
        position: Position::new(20.0, 89.5, 0.0),
        radius: Footprint::new(2.0, 2.0),
        goal: None,
        walls: [false; 4],
        closest_goal: None,
    };
    let nearest = closest_spot(
        &site,
        radius,
        Point::new(40.0, 80.0),
        &mut |_, _| Ok(Tested::Free),
        &tuning().spawn,
    )
    .unwrap()
    .unwrap();
    let flagged = spawned(centre_with_gather_point(FeatureFlags::On, None));
    assert_eq!(flagged.ground(), nearest);
    let nearest_queued = closest_spot(
        &site,
        radius,
        Point::new(10.0, 95.0),
        &mut |_, _| Ok(Tested::Free),
        &tuning().spawn,
    )
    .unwrap()
    .unwrap();
    assert_ne!(nearest_queued, nearest);
    // the building unit AI's slots 0x218 and 0x220 and the tower unit AI's twins read the queue
    for ai_kind in [UnitAiKind::Building, UnitAiKind::Tower] {
        let mut queued = centre_with_gather_point(FeatureFlags::On, None);
        queued.parts.set_unit_ai(
            object_id(700),
            Some(UnitAi {
                ai_kind,
                building_orders: vec![QueuedOrder {
                    point: Position::new(10.0, 95.0, 0.0),
                    ..queued_move()
                }],
                ..UnitAi::default()
            }),
        );
        assert_eq!(spawned(queued).ground(), nearest_queued, "{ai_kind:?}");
    }
    let plain = spawned(centre_with_gather_point(FeatureFlags::Off, None));
    assert_ne!(plain.ground(), nearest);
    let on_centre = Some(object_id(700));
    assert_eq!(
        spawned(centre_with_gather_point(FeatureFlags::On, on_centre)),
        spawned(centre_with_gather_point(FeatureFlags::Off, on_centre))
    );
    let mut unexplored = centre_with_gather_point(FeatureFlags::On, None);
    unexplored
        .spatial
        .sight
        .as_mut()
        .unwrap()
        .unified
        .fill(VisionBits::empty());
    assert_eq!(spawned(unexplored).ground(), nearest);
    let mut on_hills = centre_with_gather_point(FeatureFlags::On, None);
    for masters in &mut on_hills.model.masters {
        Arc::make_mut(masters)
            .get_mut(ids::VILLAGER_MALE)
            .unwrap()
            .hill_mode = HillMode::ONE_STEP;
    }
    assert_eq!(spawned(on_hills).ground(), nearest);
}
