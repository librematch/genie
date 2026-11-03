use crate::map::Distance;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::Tile;
use crate::map::spatial::TileSight;
use crate::math::rng::Well512;
use crate::movement::explore::InfluenceMap;
use crate::pathing::zones::ZoneId;
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::players::ai_module::search::{DEAD_END, EXPLORED, PUNTED, UNEXPLORED};
use crate::players::ai_module::{
    AiModule, AiModules, InfoModule, SLOTS, SearchUnit, TacticalModule,
};
use crate::testing::scene::{Scene, scout_area};
use crate::vision::SightCentre;
use crate::vision::SightTiles;
use crate::vision::TileCount;
use crate::vision::VisionBits;
use crate::world::Millis;
use crate::world::WorldTime;
use crate::world::{FrameSeed, World};

fn module() -> AiModule {
    AiModule {
        info: InfoModule {
            size: GridSize::new(4, 3),
            path_map: InfluenceMap::new(GridSize::new(4, 3), 0xff),
            quadrants: [[0; 3]; 16],
            explorer: None,
            search: Vec::new(),
        },
        tactical: TacticalModule {
            slot_times: vec![7, 8],
        },
    }
}

#[test]
fn an_explored_tile_on_the_map_writes_zero_into_the_path_map() {
    let mut m = module();
    m.explored_tile(Tile::new(2, 1));
    assert_eq!(m.info.path_map.get(Tile::new(2, 1)), Some(0));
    assert_eq!(m.info.path_map.get(Tile::new(1, 1)), Some(0xff));
}

#[test]
fn an_explored_tile_off_the_map_changes_nothing() {
    let mut m = module();
    let before = m.clone();
    for (x, y) in [(-1, 0), (0, -1), (4, 0), (0, 3)] {
        m.explored_tile(Tile::new(x, y));
    }
    assert_eq!(m, before);
}

#[test]
fn the_hooks_bound_is_the_modules_size_not_the_path_maps() {
    // FUN_140b8ab60 tests +0x10518/+0x1051c, then the grid lets a tile off its own size be
    let mut m = module();
    m.info.size = GridSize::new(10, 10);
    let before = m.info.path_map.clone();
    m.explored_tile(Tile::new(8, 8));
    assert_eq!(m.info.path_map, before);
}

#[test]
fn slot_past_the_saved_words_or_the_slots_is_minus_one() {
    let t = module().tactical;
    assert_eq!(t.slot(0), 7);
    assert_eq!(t.slot(1), 8);
    assert_eq!(t.slot(2), -1);
    assert_eq!(t.slot(SLOTS as u32), -1);
    assert_eq!(t.slot(u32::MAX), -1);
}

#[test]
fn the_table_holds_module_by_player() {
    let mut t = AiModules::default();
    assert!(t.get(PlayerId::new(3)).is_none());
    t.set(PlayerId::new(3), module());
    assert!(t.get(PlayerId::new(3)).is_some());
    assert!(t.get(PlayerId::new(2)).is_none());
    t.get_mut(PlayerId::new(3)).unwrap().info.explorer = None;
}

#[test]
fn the_worlds_sight_hands_its_explored_tiles_to_the_players_module() {
    use crate::vision::visibility::{Visibility, VisibleMap};
    let mut t = World::new(
        WorldTime::ZERO,
        FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO; 3],
        Vec::new(),
    );
    let n = 16 * 16;
    let mut v = Visibility::new(
        GridSize::new(16, 16),
        vec![
            VisibleMap {
                counts: vec![-1; n],
                explored: TileCount::ZERO,
            };
            3
        ],
        vec![VisionBits::NONE; n],
        Vec::new(),
        0,
        vec![PlayerKind::Human, PlayerKind::Computer, PlayerKind::Human],
    );
    assert_eq!(
        v.explore(
            PlayerId::new(1),
            Tile::new(3, 2),
            SightTiles::new(0),
            SightCentre::Tile
        ),
        Ok(TileCount::ONE)
    );
    assert_eq!(
        v.explore(
            PlayerId::new(2),
            Tile::new(5, 5),
            SightTiles::new(0),
            SightCentre::Tile
        ),
        Ok(TileCount::ONE)
    );
    t.spatial.sight = Some(v);
    let mut m = module();
    m.info.size = GridSize::new(16, 16);
    m.info.path_map = InfluenceMap::new(GridSize::new(16, 16), 0xff);
    t.ai.set(PlayerId::new(1), m);
    assert!(t.take_sight_unsupported().is_none());
    assert_eq!(
        t.ai.get(PlayerId::new(1))
            .unwrap()
            .info
            .path_map
            .get(Tile::new(3, 2)),
        Some(0)
    );
    assert_eq!(
        t.ai.get(PlayerId::new(1))
            .unwrap()
            .info
            .path_map
            .get(Tile::new(5, 5)),
        Some(0xff)
    );
}

/// The explorer of a search: civilian or not, at the scene's scout's place `at`.
fn unit(civilian: bool, at: Point) -> SearchUnit {
    SearchUnit {
        position: at,
        civilian,
    }
}

fn info(size: GridSize, fill: u8) -> InfoModule {
    InfoModule {
        size,
        path_map: InfluenceMap::new(size, fill),
        quadrants: [[0; 3]; 16],
        explorer: None,
        search: Vec::new(),
    }
}

/// `scene`'s world with its scout at `at`, line of sight `sight`, which sees nothing now but
/// `seen` (the rest explored).
fn searched(scene: Scene, seen: &[Tile], at: Point, sight: f32) -> World {
    scene
        .sight(TileSight::Explored)
        .sight_on(seen.iter().copied(), TileSight::Seen)
        .scout(at.with_height(0.0), Distance::new(sight))
        .build()
}

/// The search from `start` on `w`'s scout for `unit`, with module `m`.
fn search(
    m: &mut InfoModule,
    w: &mut World,
    unit: &SearchUnit,
    start: Point,
    rng: &mut Well512,
) -> Option<Tile> {
    let (mut area, size, _) = scout_area(w);
    m.closest_unexplored_tile(unit, start, &mut area, (size, rng))
        .unwrap()
}

/// A closed ring of rocks about the tiles from `low` to `high`, which no path crosses.
fn walled(scene: Scene, low: Tile, high: Tile) -> Scene {
    let (x0, y0, x1, y1) = (low.x() - 1, low.y() - 1, high.x() + 1, high.y() + 1);
    let ring: Vec<Tile> = (x0..=x1)
        .flat_map(|x| [Tile::new(x, y0), Tile::new(x, y1)])
        .chain((y0 + 1..y1).flat_map(|y| [Tile::new(x0, y), Tile::new(x1, y)]))
        .collect();
    scene.rocks(ring)
}

/// The scout's place in the corner tile.
const CORNER: Point = Point::new(0.5, 0.5);

/// The search starts at the unit's tile and finds the first tile a civilian never searched:
/// an explored tile (0) is passed through, the picked tile becomes explored. (Row 0 and column
/// 0 are left out: a tile's corner on the map's edge is never clear, the obstruction manager's
/// edge records hold it.)
#[test]
fn civilians_flood_picks_the_first_tile_no_search_touched() {
    let size = GridSize::new(6, 6);
    let mut m = info(size, EXPLORED);
    m.path_map.set(Tile::new(3, 1), UNEXPLORED);
    m.path_map.set(Tile::new(1, 3), UNEXPLORED);
    let mut w = searched(Scene::open(size), &[], CORNER, 2.0);
    let picked = search(
        &mut m,
        &mut w,
        &unit(true, CORNER),
        CORNER,
        &mut Well512::default(),
    );
    assert_eq!(picked, Some(Tile::new(3, 1)));
    assert_eq!(m.path_map.get(Tile::new(3, 1)), Some(EXPLORED));
}

/// Another unit takes a tile that is not explored: a punted tile (1) counts for it, not for a
/// civilian.
#[test]
fn another_units_flood_takes_punted_tile_civilian_passes() {
    let size = GridSize::new(6, 6);
    let mut m = info(size, EXPLORED);
    m.path_map.set(Tile::new(2, 1), PUNTED);
    let mut w = searched(Scene::open(size), &[], CORNER, 2.0);
    let mut rng = Well512::default();
    let picked = search(&mut m, &mut w, &unit(false, CORNER), CORNER, &mut rng);
    assert_eq!(picked, Some(Tile::new(2, 1)));
    let mut civilian = info(size, EXPLORED);
    civilian.path_map.set(Tile::new(2, 1), PUNTED);
    let none = search(&mut civilian, &mut w, &unit(true, CORNER), CORNER, &mut rng);
    assert_eq!(none, None);
}

/// A tile the player sees now is passed through; an unexplored tile of another zone (the water
/// about the row, or at its end) or where the unit cannot stand (a rock) ends its branch.
#[test]
fn seen_tile_is_passed_through_but_another_zone_ends_the_branch() {
    let at = Point::new(0.5, 1.5);
    let row = |middle: &str| {
        let scene = Scene::rows(&["~~~~", middle, "~~~~", "~~~~"]);
        let mut m = info(GridSize::new(4, 4), UNEXPLORED);
        for x in 0..3 {
            m.path_map.set(Tile::new(x, 1), EXPLORED);
        }
        let mut w = searched(scene, &[Tile::new(0, 1), Tile::new(1, 1)], at, 4.0);
        search(&mut m, &mut w, &unit(true, at), at, &mut Well512::default())
    };
    assert_eq!(row("...."), Some(Tile::new(3, 1)));
    assert_eq!(row("...~"), None);
    assert_eq!(row("...#"), None);
}

/// An explored tile of another zone is passed through, an unexplored one ends its branch:
/// (3, 1) lies in the scout's zone by the land below, which the rocks close to the flood, and
/// the flood meets it only through the water at (2, 1).
#[test]
fn another_zone_ends_the_branch_only_where_the_tile_is_wanted() {
    let at = Point::new(0.5, 1.5);
    let size = GridSize::new(4, 4);
    let module = |water: u8| {
        let mut m = info(size, UNEXPLORED);
        for t in [[0, 1], [1, 1], [3, 2], [0, 3], [1, 3], [2, 3], [3, 3]] {
            m.path_map.set(Tile::new(t[0], t[1]), EXPLORED);
        }
        m.path_map.set(Tile::new(2, 1), water);
        m
    };
    let seen = [Tile::new(0, 1), Tile::new(1, 1)];
    let scene = || Scene::rows(&["~~~~", "..~.", "###.", "...."]);
    let mut w = searched(scene(), &seen, at, 4.0);
    let mut m = module(EXPLORED);
    let picked = search(&mut m, &mut w, &unit(true, at), at, &mut Well512::default());
    assert_eq!(picked, Some(Tile::new(3, 1)));
    let mut w = searched(scene(), &seen, at, 4.0);
    let mut m = module(UNEXPLORED);
    let picked = search(&mut m, &mut w, &unit(true, at), at, &mut Well512::default());
    assert_eq!(picked, None);
}

/// The zones' low bytes only (`FUN_140b71990`): on a map 4 wide and 6 high the labelling
/// starts patches only in its 4 by 4 square, and its fill (`FUN_141413190`, each row's `height`
/// tiles from `row * width`) reaches the land below the water up to index 18: (1, 4) keeps the
/// unreached zone 512, whose low byte is the scout's zone 0's, and its unexplored tile counts.
/// The islet at (1, 2), zone 2, does not.
#[test]
fn the_flood_compares_the_zones_low_bytes() {
    let scene = || Scene::rows(&["....", "~~~~", "~.~~", "~~~~", "....", "...."]);
    let size = GridSize::new(4, 6);
    let module = |open: Tile| {
        let mut m = info(size, EXPLORED);
        m.path_map.set(open, UNEXPLORED);
        m
    };
    let mut w = searched(scene(), &[], CORNER, 5.0);
    {
        let (area, _, _) = scout_area(&mut w);
        assert_eq!(area.own_zone(), ZoneId::new(0));
        assert_eq!(area.zone(Tile::new(1, 4)), ZoneId::new(0x200));
        assert_eq!(area.zone(Tile::new(1, 2)), ZoneId::new(2));
    }
    let mut m = module(Tile::new(1, 4));
    let picked = search(
        &mut m,
        &mut w,
        &unit(true, CORNER),
        CORNER,
        &mut Well512::default(),
    );
    assert_eq!(picked, Some(Tile::new(1, 4)));
    let mut m = module(Tile::new(1, 2));
    let mut w = searched(scene(), &[], CORNER, 5.0);
    let picked = search(
        &mut m,
        &mut w,
        &unit(true, CORNER),
        CORNER,
        &mut Well512::default(),
    );
    assert_eq!(picked, None);
}

/// A tile the unit cannot reach (walled in by rocks beyond its reach of 1) is punted (1) and the
/// search starts again from the unit's own place, with a fresh flood, which picks the next
/// tile.
#[test]
fn an_unreachable_tile_is_punted_and_the_search_starts_from_the_unit() {
    let size = GridSize::new(12, 12);
    let mut m = info(size, EXPLORED);
    m.path_map.set(Tile::new(4, 4), UNEXPLORED);
    m.path_map.set(Tile::new(11, 1), UNEXPLORED);
    let scene = walled(Scene::open(size), Tile::new(3, 3), Tile::new(5, 5));
    let mut w = searched(scene, &[], CORNER, 2.0);
    // from (8, 8) the flood meets (4, 4) first, then (11, 1) after the restart from (0, 0)
    let picked = search(
        &mut m,
        &mut w,
        &unit(true, CORNER),
        Point::new(8.5, 8.5),
        &mut Well512::default(),
    );
    assert_eq!(m.path_map.get(Tile::new(4, 4)), Some(PUNTED));
    assert_eq!(picked, Some(Tile::new(11, 1)));
    assert_eq!(m.path_map.get(Tile::new(11, 1)), Some(EXPLORED));
}

/// A search that already starts at the unit's place punts to a corner of the best quadrant: 16
/// draws for the scores and one for the corner.
#[test]
fn punt_from_the_units_place_picks_corner_of_the_lowest_scored_quadrant() {
    let size = GridSize::new(8, 8);
    let mut m = info(size, EXPLORED);
    m.path_map.set(Tile::new(4, 4), UNEXPLORED);
    // quadrant 6 (column 1, row 2) scores far below the others
    for (i, q) in m.quadrants.iter_mut().enumerate() {
        q[0] = if i == 6 { 0 } else { 1000 };
    }
    let scene = walled(Scene::open(size), Tile::new(3, 3), Tile::new(5, 5));
    let mut w = searched(scene, &[], CORNER, 2.0);
    let mut rng = Well512::default();
    let mut shadow = rng.clone();
    let picked = search(&mut m, &mut w, &unit(true, CORNER), CORNER, &mut rng).unwrap();
    for _ in 0..16 {
        let _ = shadow.rand15();
    }
    let corner = shadow.rand15() & 3;
    // column 1 starts at x 2, row 2 at y 4; each quadrant is 2 wide and high
    let expected = [
        Tile::new(2, 4),
        Tile::new(2, 5),
        Tile::new(3, 5),
        Tile::new(3, 4),
    ][corner as usize];
    assert_eq!(picked, expected);
    assert_eq!(rng.rand15(), shadow.rand15());
}

/// A flood that meets nothing marks every searched tile of the unit's zone it does not see
/// now as a dead end (2), and gives none: (0, 1) is seen, (1, 0) is water, (2, 2) was never
/// searched (a rock stands on it).
#[test]
fn an_exhausted_flood_marks_the_dead_ends() {
    let size = GridSize::new(3, 3);
    let mut m = info(size, EXPLORED);
    m.path_map.set(Tile::new(1, 1), PUNTED);
    m.path_map.set(Tile::new(2, 2), UNEXPLORED);
    let mut w = searched(
        Scene::rows(&[".~.", "...", "..#"]),
        &[Tile::new(0, 1)],
        CORNER,
        2.0,
    );
    let picked = search(
        &mut m,
        &mut w,
        &unit(true, CORNER),
        CORNER,
        &mut Well512::default(),
    );
    assert_eq!(picked, None);
    assert_eq!(m.path_map.get(Tile::new(0, 1)), Some(EXPLORED));
    assert_eq!(m.path_map.get(Tile::new(1, 0)), Some(EXPLORED));
    assert_eq!(m.path_map.get(Tile::new(2, 2)), Some(UNEXPLORED));
    assert_eq!(m.path_map.get(Tile::new(0, 0)), Some(DEAD_END));
    assert_eq!(m.path_map.get(Tile::new(1, 1)), Some(DEAD_END));
}

#[test]
fn tile_uncovered_is_false_off_the_map_and_for_unexplored_and_dead_ends() {
    let mut m = info(GridSize::new(3, 3), EXPLORED);
    m.path_map.set(Tile::new(0, 0), UNEXPLORED);
    m.path_map.set(Tile::new(1, 0), DEAD_END);
    m.path_map.set(Tile::new(2, 0), PUNTED);
    assert!(!m.tile_uncovered(Tile::new(0, 0)).unwrap());
    assert!(!m.tile_uncovered(Tile::new(1, 0)).unwrap());
    assert!(m.tile_uncovered(Tile::new(2, 0)).unwrap());
    assert!(m.tile_uncovered(Tile::new(0, 1)).unwrap());
    for (x, y) in [(-1, 0), (0, -1), (3, 0), (0, 3)] {
        assert!(!m.tile_uncovered(Tile::new(x, y)).unwrap());
    }
}

/// A ring's points that lie on the map join the stack, from the angle 0 on; the top of an
/// empty stack and its pop are (-1, -1).
#[test]
fn ring_pushes_the_points_on_the_map_and_the_stack_gives_minus_one_when_empty() {
    let mut m = info(GridSize::new(20, 20), UNEXPLORED);
    assert_eq!(m.top_point(), Tile::new(-1, -1));
    assert_eq!(m.pop_point(), Tile::new(-1, -1));
    m.push_ring(Distance::new(5.0), Point::new(10.7, 10.2));
    assert_eq!(m.search.len(), 8);
    // the centre truncates to (10, 10); angle 0 is 5 east, π/2's sine is 1
    assert_eq!(m.search[0], Tile::new(15, 10));
    assert_eq!(m.search[2], Tile::new(10, 15));
    assert_eq!(m.top_point(), m.search[7]);
    let top = m.pop_point();
    assert_eq!(m.search.len(), 7);
    assert_ne!(top, Tile::new(-1, -1));
    // a ring at a corner keeps only the points on the map
    let mut corner = info(GridSize::new(20, 20), UNEXPLORED);
    corner.push_ring(Distance::new(5.0), Point::new(1.0, 1.0));
    assert!(corner.search.len() < 8);
    assert!(corner.search.iter().all(|p| 0 <= p.x() && 0 <= p.y()));
}
