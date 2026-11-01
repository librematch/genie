use crate::GenieError;
use crate::map::Distance;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::Tile;
use crate::math::rng::Well512;
use crate::movement::explore::Area;
use crate::movement::explore::InfluenceMap;
use crate::objects::ObjectId;
use crate::players::PlayerId;
use crate::players::ai_module::search::{DEAD_END, EXPLORED, PUNTED, UNEXPLORED};
use crate::players::ai_module::{
    AiModule, AiModules, InfoModule, SLOTS, SearchArea, SearchUnit, TacticalModule,
};
use crate::vision::SightTiles;
use crate::vision::VisionBits;
use crate::world::Millis;
use crate::world::WorldTime;

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
fn a_slot_past_the_saved_words_or_the_slots_is_minus_one() {
    let t = module().tactical;
    assert_eq!(t.slot(0), 7);
    assert_eq!(t.slot(1), 8);
    assert_eq!(t.slot(2), -1);
    assert_eq!(t.slot(SLOTS as u32), -1);
    assert_eq!(t.slot(u32::MAX), -1);
}

#[test]
fn the_table_holds_a_module_by_player() {
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
    let mut t = crate::world::World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(1),
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
                explored: 0,
            };
            3
        ],
        vec![VisionBits::NONE; n],
        Vec::new(),
        0,
        vec![false, true, false],
    );
    assert_eq!(
        v.explore(PlayerId::new(1), Tile::new(3, 2), SightTiles::new(0), false),
        Ok(1)
    );
    assert_eq!(
        v.explore(PlayerId::new(2), Tile::new(5, 5), SightTiles::new(0), false),
        Ok(1)
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

/// A map of `size` by `size` tiles for the search: what it sees now, which tiles are in the
/// unit's zone, where the unit could stand and which it reaches.
struct Grid {
    seen: Vec<[i32; 2]>,
    other_zone: Vec<[i32; 2]>,
    blocked: Vec<[i32; 2]>,
    unreachable: Vec<[i32; 2]>,
    visits: std::cell::RefCell<Vec<[i32; 2]>>,
}

impl Grid {
    fn new(_size: i32) -> Self {
        Self {
            seen: Vec::new(),
            other_zone: Vec::new(),
            blocked: Vec::new(),
            unreachable: Vec::new(),
            visits: std::cell::RefCell::new(Vec::new()),
        }
    }
}

impl Area for Grid {
    fn explored(&self, _: Tile) -> Result<bool, GenieError> {
        Ok(false)
    }
    fn zone(&mut self, tile: Tile) -> crate::pathing::zones::ZoneId {
        // the byte past the zone's own one is not read
        crate::pathing::zones::ZoneId::new(if self.other_zone.contains(&tile.get()) {
            0x108
        } else {
            0x207
        })
    }
    fn own_zone(&mut self) -> crate::pathing::zones::ZoneId {
        crate::pathing::zones::ZoneId::new(0x307)
    }
    fn clear(&mut self, tile: Tile) -> Result<bool, GenieError> {
        Ok(!self.blocked.contains(&tile.get()))
    }
    fn reaches(&mut self, tile: Tile) -> Result<bool, GenieError> {
        Ok(!self.unreachable.contains(&tile.get()))
    }
    fn distance(&self, _: Tile) -> f32 {
        0.0
    }
}

impl SearchArea for Grid {
    fn visible_now(&self, tile: Tile) -> bool {
        self.visits.borrow_mut().push(tile.get());
        self.seen.contains(&tile.get())
    }
}

fn unit(civilian: bool) -> SearchUnit {
    SearchUnit {
        id: ObjectId::try_from(30).unwrap(),
        position: Point::new(0.5, 0.5),
        civilian,
    }
}

fn info(size: i32, fill: u8) -> InfoModule {
    InfoModule {
        size: GridSize::new(size, size),
        path_map: InfluenceMap::new(GridSize::new(size, size), fill),
        quadrants: [[0; 3]; 16],
        explorer: None,
        search: Vec::new(),
    }
}

/// The search starts at the unit's tile and finds the first tile a civilian never searched:
/// an explored tile (0) is passed through, the picked tile becomes explored.
#[test]
fn a_civilians_flood_picks_the_first_tile_no_search_touched() {
    let mut m = info(6, EXPLORED);
    m.path_map.set(Tile::new(3, 0), UNEXPLORED);
    m.path_map.set(Tile::new(0, 3), UNEXPLORED);
    let mut area = Grid::new(6);
    let mut rng = Well512::default();
    let picked = m
        .closest_unexplored_tile(
            &unit(true),
            Point::new(0.5, 0.5),
            &mut area,
            (GridSize::new(6, 6), &mut rng),
        )
        .unwrap();
    assert_eq!(picked, Some(Tile::new(3, 0)));
    assert_eq!(m.path_map.get(Tile::new(3, 0)), Some(EXPLORED));
}

/// Another unit takes a tile that is not explored: a punted tile (1) counts for it, not for a
/// civilian.
#[test]
fn another_units_flood_takes_a_punted_tile_a_civilian_passes() {
    let mut m = info(6, EXPLORED);
    m.path_map.set(Tile::new(2, 0), PUNTED);
    let mut rng = Well512::default();
    let picked = m
        .closest_unexplored_tile(
            &unit(false),
            Point::new(0.5, 0.5),
            &mut Grid::new(6),
            (GridSize::new(6, 6), &mut rng),
        )
        .unwrap();
    assert_eq!(picked, Some(Tile::new(2, 0)));
    let mut civilian = info(6, EXPLORED);
    civilian.path_map.set(Tile::new(2, 0), PUNTED);
    let none = civilian
        .closest_unexplored_tile(
            &unit(true),
            Point::new(0.5, 0.5),
            &mut Grid::new(6),
            (GridSize::new(6, 6), &mut rng),
        )
        .unwrap();
    assert_eq!(none, None);
}

/// A tile the player sees now is passed through; one of another zone, or where the unit
/// cannot stand, ends its branch (its neighbours are not met through it). The map is one row.
#[test]
fn a_seen_tile_is_passed_through_but_another_zone_ends_the_branch() {
    let row = |zone: Option<[i32; 2]>, blocked: Option<[i32; 2]>| {
        let mut m = info(4, EXPLORED);
        m.path_map.set(Tile::new(3, 0), UNEXPLORED);
        let mut area = Grid::new(4);
        area.seen = vec![[0, 0], [1, 0]];
        area.other_zone.extend(zone);
        area.blocked.extend(blocked);
        let mut rng = Well512::default();
        m.closest_unexplored_tile(
            &unit(true),
            Point::new(0.5, 0.5),
            &mut area,
            (GridSize::new(4, 1), &mut rng),
        )
        .unwrap()
    };
    assert_eq!(row(None, None), Some(Tile::new(3, 0)));
    assert_eq!(row(Some([3, 0]), None), None);
    assert_eq!(row(None, Some([3, 0])), None);
    // a wanted tile in another zone ends the branch; an unwanted one does not
    let mut m = info(4, EXPLORED);
    m.path_map.set(Tile::new(3, 0), UNEXPLORED);
    let mut area = Grid::new(4);
    area.other_zone = vec![[2, 0]];
    let mut rng = Well512::default();
    let picked = m
        .closest_unexplored_tile(
            &unit(true),
            Point::new(0.5, 0.5),
            &mut area,
            (GridSize::new(4, 1), &mut rng),
        )
        .unwrap();
    assert_eq!(picked, Some(Tile::new(3, 0)));
}

/// The start tile is not marked met: a neighbour meets it again.
#[test]
fn the_start_tile_is_met_twice() {
    let mut m = info(3, EXPLORED);
    let area = &mut Grid::new(3);
    let mut rng = Well512::default();
    let _ = m
        .closest_unexplored_tile(
            &unit(true),
            Point::new(1.5, 1.5),
            area,
            (GridSize::new(3, 3), &mut rng),
        )
        .unwrap();
    // the flood found nothing, so the dead ends' pass met every tile once more
    let visits = area.visits.borrow();
    assert_eq!(visits.iter().filter(|&&t| t == [1, 1]).count() - 1, 2);
    assert_eq!(visits.iter().filter(|&&t| t == [0, 0]).count() - 1, 1);
}

/// A tile the unit cannot reach is punted (1) and the search starts again from the unit's own
/// place, with a fresh flood, which picks the next tile.
#[test]
fn an_unreachable_tile_is_punted_and_the_search_starts_from_the_unit() {
    let mut m = info(6, EXPLORED);
    m.path_map.set(Tile::new(2, 2), UNEXPLORED);
    m.path_map.set(Tile::new(5, 5), UNEXPLORED);
    let mut area = Grid::new(6);
    area.unreachable = vec![[2, 2]];
    let mut rng = Well512::default();
    // from (3, 3) the flood meets (2, 2) first; (5, 5) after the restart from (0, 0)... both
    // are searched by a civilian, the nearer first
    let picked = m
        .closest_unexplored_tile(
            &unit(true),
            Point::new(3.5, 3.5),
            &mut area,
            (GridSize::new(6, 6), &mut rng),
        )
        .unwrap();
    assert_eq!(m.path_map.get(Tile::new(2, 2)), Some(PUNTED));
    assert_eq!(picked, Some(Tile::new(5, 5)));
    assert_eq!(m.path_map.get(Tile::new(5, 5)), Some(EXPLORED));
}

/// A search that already starts at the unit's place punts to a corner of the best quadrant: 16
/// draws for the scores and one for the corner.
#[test]
fn a_punt_from_the_units_place_picks_a_corner_of_the_lowest_scored_quadrant() {
    let mut m = info(8, EXPLORED);
    m.path_map.set(Tile::new(4, 4), UNEXPLORED);
    // quadrant 6 (column 1, row 2) scores far below the others
    for (i, q) in m.quadrants.iter_mut().enumerate() {
        q[0] = if i == 6 { 0 } else { 1000 };
    }
    let mut area = Grid::new(8);
    area.unreachable = vec![[4, 4]];
    let mut rng = Well512::default();
    let mut shadow = rng.clone();
    let picked = m
        .closest_unexplored_tile(
            &unit(true),
            Point::new(0.5, 0.5),
            &mut area,
            (GridSize::new(8, 8), &mut rng),
        )
        .unwrap()
        .unwrap();
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
/// now as a dead end (2), and gives none.
#[test]
fn an_exhausted_flood_marks_the_dead_ends() {
    let mut m = info(3, EXPLORED);
    m.path_map.set(Tile::new(1, 1), PUNTED);
    m.path_map.set(Tile::new(2, 2), UNEXPLORED);
    let mut area = Grid::new(3);
    area.seen = vec![[0, 1]];
    area.other_zone = vec![[1, 0]];
    area.blocked = vec![[2, 2]];
    let mut rng = Well512::default();
    let picked = m
        .closest_unexplored_tile(
            &unit(true),
            Point::new(0.5, 0.5),
            &mut area,
            (GridSize::new(3, 3), &mut rng),
        )
        .unwrap();
    assert_eq!(picked, None);
    // (0, 1) is seen, (1, 0) lies in another zone, (2, 2) was never searched
    assert_eq!(m.path_map.get(Tile::new(0, 1)), Some(EXPLORED));
    assert_eq!(m.path_map.get(Tile::new(1, 0)), Some(EXPLORED));
    assert_eq!(m.path_map.get(Tile::new(2, 2)), Some(UNEXPLORED));
    assert_eq!(m.path_map.get(Tile::new(0, 0)), Some(DEAD_END));
    assert_eq!(m.path_map.get(Tile::new(1, 1)), Some(DEAD_END));
}

#[test]
fn tile_uncovered_is_false_off_the_map_and_for_unexplored_and_dead_ends() {
    let mut m = info(3, EXPLORED);
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
fn a_ring_pushes_the_points_on_the_map_and_the_stack_gives_minus_one_when_empty() {
    let mut m = info(20, UNEXPLORED);
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
    let mut corner = info(20, UNEXPLORED);
    corner.push_ring(Distance::new(5.0), Point::new(1.0, 1.0));
    assert!(corner.search.len() < 8);
    assert!(corner.search.iter().all(|p| 0 <= p.x() && 0 <= p.y()));
}
