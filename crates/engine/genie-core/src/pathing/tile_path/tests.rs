use super::*;
use crate::map::Distance;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::Restriction;
use crate::map::TerrainId;
use crate::map::Tile;
use crate::map::spatial::Tiles;
use crate::pathing::tile_path::goal_marks::Mark;
use crate::testing::tuning;
use std::iter::from_fn;

#[test]
fn search_leaves_the_grid_as_it_was() {
    let terrain = vec![TerrainId::named(0); 64];
    let open = |_: Tile| false;
    let built = Planner::new(
        GridSize::new(8, 8),
        terrain.clone(),
        Restriction::new(vec![1.0]),
        &Tiles::with_blocked(GridSize::new(8, 8), open),
    )
    .unwrap();
    let mut searched = built.clone();
    searched.goal = PlanTile::new(3, 4);
    searched.goal_marked = true;
    assert!(searched != built && searched.same_grid(&built));
    let other = Planner::new(
        GridSize::new(8, 8),
        terrain,
        Restriction::new(vec![0.0]),
        &Tiles::with_blocked(GridSize::new(8, 8), open),
    )
    .unwrap();
    assert!(!other.same_grid(&built));
}

fn open_map(size: GridSize) -> Tiles {
    Tiles::with_blocked(size, |_| false)
}

/// `FUN_140f7c530` reads the start's cell only after the search over the levels found the
/// goal: a start off the map ends the search there, without the tile search.
#[test]
fn plan_from_off_the_map_reads_no_start_cell() {
    let mut planner = Planner::new(
        GridSize::new(16, 8),
        vec![TerrainId::named(0); 128],
        Restriction::new(vec![1.0]),
        &open_map(GridSize::new(16, 8)),
    )
    .unwrap();
    let req = Request {
        from: Point::new(-3.0, 2.0),
        to: Point::new(5.5, 2.5),
        radii: Distance::new(0.2),
        range: Distance::ZERO,
        closed: Vec::new(),
        full: true,
    };
    let mut out = Path::default();
    let found = planner
        .plan::<crate::GenieError>(&req, false, &mut out, None, &tuning().tile_path)
        .unwrap();
    assert_eq!(found, Found::OffMap);
}

#[test]
fn the_last_open_point_walks_the_path_back_to_tile_with_an_edge() {
    // water (terrain 0) left of x = 10, land (1, impassable) right of it
    let terrain: Vec<TerrainId> = (0..128)
        .map(|tile| TerrainId::named(u8::from(10 <= tile % 16)))
        .collect();
    let mut planner = Planner::new(
        GridSize::new(16, 8),
        terrain,
        Restriction::new(vec![1.0, 0.0]),
        &open_map(GridSize::new(16, 8)),
    )
    .unwrap();
    // a goal on the water: the path's end
    assert_eq!(
        planner
            .last_open_point(
                Point::new(2.5, 4.5),
                Point::new(7.5, 4.5),
                &tuning().tile_path
            )
            .unwrap(),
        Some(Point::new(7.5, 4.5))
    );
    // ashore: the path ends at the node nearest the goal, the last water tile
    assert_eq!(
        planner
            .last_open_point(
                Point::new(2.5, 4.5),
                Point::new(13.5, 4.5),
                &tuning().tile_path
            )
            .unwrap(),
        Some(Point::new(9.5, 4.5))
    );
    // a goal off the map (result 0): none
    assert_eq!(
        planner
            .last_open_point(
                Point::new(2.5, 4.5),
                Point::new(16.5, 4.5),
                &tuning().tile_path
            )
            .unwrap(),
        None
    );
}

/// The scan along a boundary for the tile nearest `at` with the edge: the first upward, then
/// downward while strictly nearer; neither: none.
#[test]
fn the_nearest_tile_with_the_edge_is_found_up_then_nearer_down() {
    let planner = Planner::new(
        GridSize::new(8, 8),
        vec![TerrainId::named(0); 64],
        Restriction::new(vec![1.0]),
        &open_map(GridSize::new(8, 8)),
    )
    .unwrap();
    // the tiles at the places `with` have the edge N
    let edges = |with: &'static [i16]| {
        move |coordinate: i16| {
            Ok(Cell {
                edges: if with.contains(&coordinate) {
                    CellEdges::N
                } else {
                    CellEdges::NONE
                },
                kind: CellKind::NONE,
            })
        }
    };
    // above at 7 (2 off), below at 4 (1 off): the nearer below
    assert_eq!(
        planner
            .nearest_on_boundary(5, 0, 7, CellEdges::N, edges(&[4, 7]))
            .unwrap(),
        Some(4)
    );
    // one off either way: a tie goes up
    assert_eq!(
        planner
            .nearest_on_boundary(5, 0, 7, CellEdges::N, edges(&[4, 6]))
            .unwrap(),
        Some(6)
    );
    // above at 6 only
    assert_eq!(
        planner
            .nearest_on_boundary(5, 0, 7, CellEdges::N, edges(&[6]))
            .unwrap(),
        Some(6)
    );
    // below at 1 only, beyond nothing found above
    assert_eq!(
        planner
            .nearest_on_boundary(5, 0, 7, CellEdges::N, edges(&[1]))
            .unwrap(),
        Some(1)
    );
    // another edge only: none
    let south = |_: i16| {
        Ok(Cell {
            edges: CellEdges::S,
            kind: CellKind::NONE,
        })
    };
    assert_eq!(
        planner
            .nearest_on_boundary(5, 0, 7, CellEdges::N, south)
            .unwrap(),
        None
    );
}

/// `FUN_140f78ba0` on an open map: the tiles' A* (`FUN_140f79250`, its neighbours by
/// `FUN_140f76840`) reaches the goal, and the path runs from the start to the goal point.
#[test]
fn the_tile_search_reaches_goal_on_an_open_map() {
    let mut planner = Planner::new(
        GridSize::new(16, 8),
        vec![TerrainId::named(0); 128],
        Restriction::new(vec![1.0]),
        &open_map(GridSize::new(16, 8)),
    )
    .unwrap();
    let req = Request {
        from: Point::new(1.5, 2.5),
        to: Point::new(6.5, 5.5),
        radii: Distance::new(0.2),
        range: Distance::ZERO,
        closed: Vec::new(),
        full: true,
    };
    let mut out = Path::default();
    assert_eq!(
        planner
            .search_on_tiles(&req, &mut out, &tuning().tile_path)
            .unwrap(),
        Found::Goal
    );
    assert_eq!(out.points.first(), Some(&BVector::new(1.5, 0.0, 2.5)));
    assert_eq!(out.points.last(), Some(&BVector::new(6.5, 0.0, 5.5)));
    // the search leaves its marks behind: the same search again finds the same path
    let mut again = Path::default();
    assert_eq!(
        planner
            .search_on_tiles(&req, &mut again, &tuning().tile_path)
            .unwrap(),
        Found::Goal
    );
    assert_eq!(again.points, out.points);
}

/// A wall across the map: the tile search ends at the tile nearest the goal (its centre),
/// on the start's side.
#[test]
fn the_tile_search_ends_nearest_the_goal_behind_wall() {
    let wall = |tile: Tile| tile.x() == 3;
    let mut planner = Planner::new(
        GridSize::new(8, 8),
        vec![TerrainId::named(0); 64],
        Restriction::new(vec![1.0]),
        &Tiles::with_blocked(GridSize::new(8, 8), wall),
    )
    .unwrap();
    let req = Request {
        from: Point::new(1.5, 2.5),
        to: Point::new(6.5, 2.5),
        radii: Distance::new(0.2),
        range: Distance::ZERO,
        closed: Vec::new(),
        full: true,
    };
    let mut out = Path::default();
    assert_eq!(
        planner
            .search_on_tiles(&req, &mut out, &tuning().tile_path)
            .unwrap(),
        Found::Nearest
    );
    assert_eq!(out.points.first(), Some(&BVector::new(1.5, 0.0, 2.5)));
    assert_eq!(out.points.last(), Some(&BVector::new(2.5, 0.0, 2.5)));
}

/// `FUN_140f7c530` for a human player's search (`full`): a short path over the levels is
/// searched again on the tiles; a long one has its start shortened over the tiles
/// (`shorter_start`); either way the path runs from the start to the goal.
#[test]
fn full_plan_searches_the_tiles_near_and_shortens_the_start_far() {
    let mut planner = Planner::new(
        GridSize::new(64, 8),
        vec![TerrainId::named(0); 512],
        Restriction::new(vec![1.0]),
        &open_map(GridSize::new(64, 8)),
    )
    .unwrap();
    for to in [Point::new(6.5, 5.5), Point::new(60.5, 5.5)] {
        let req = Request {
            from: Point::new(1.5, 2.5),
            to,
            radii: Distance::new(0.2),
            range: Distance::ZERO,
            closed: Vec::new(),
            full: true,
        };
        let mut out = Path::default();
        assert_eq!(
            planner
                .plan::<crate::GenieError>(&req, false, &mut out, None, &tuning().tile_path)
                .unwrap(),
            Found::Goal
        );
        assert_eq!(out.points.first(), Some(&BVector::new(1.5, 0.0, 2.5)));
        assert_eq!(out.points.last(), Some(&BVector::new(to.x(), 0.0, to.y())));
    }
}

#[test]
fn an_open_map_is_one_node_cell_per_eight_tiles() {
    let planner = Planner::new(
        GridSize::new(16, 8),
        vec![TerrainId::named(0); 128],
        Restriction::new(vec![1.0]),
        &open_map(GridSize::new(16, 8)),
    )
    .unwrap();
    let levels =
        [(16, 8), (8, 4), (4, 2), (2, 1)].map(|(width, height)| GridSize::new(width, height));
    assert_eq!(
        GridLevel::ALL.map(|grid_level| planner.level_size(grid_level)),
        levels
    );
    // level 3: two uniform node cells, joined across their shared edge only
    let node = |edges| Cell {
        edges,
        kind: CellKind::NODE,
    };
    assert_eq!(
        planner.level(GridLevel::new(3)).unwrap(),
        &[node(CellEdges::E), node(CellEdges::W)]
    );
    // an inner tile has all four edges; a corner tile two
    let tile = |x: usize, y: usize| planner.level(GridLevel::new(0)).unwrap()[y * 16 + x].bytes();
    assert_eq!(tile(3, 3), [CellEdges::EDGES.bits(), 0]);
    assert_eq!(tile(0, 0), [(CellEdges::E | CellEdges::S).bits(), 0]);
}

#[test]
fn padding_tiles_of_map_that_is_not_multiple_of_eight_lie_off_the_map() {
    let planner = Planner::new(
        GridSize::new(6, 6),
        vec![TerrainId::named(0); 36],
        Restriction::new(vec![1.0]),
        &open_map(GridSize::new(6, 6)),
    )
    .unwrap();
    let levels = [8, 4, 2, 1].map(|side| GridSize::new(side, side));
    assert_eq!(
        GridLevel::ALL.map(|grid_level| planner.level_size(grid_level)),
        levels
    );
    let tile = |x: usize, y: usize| planner.level(GridLevel::new(0)).unwrap()[y * 8 + x].bytes();
    assert_eq!(tile(6, 0), [0, 0xf]);
    assert_eq!(tile(5, 5), [(CellEdges::N | CellEdges::W).bits(), 0]);
    // the level-1 cell (3, 0) covers tiles 6 and 7: off the map, both of its children
    assert_eq!(
        planner.level(GridLevel::new(1)).unwrap()[3].kind.off_map(),
        CellKind::OFF_MAP
    );
}

#[test]
fn wall_splits_its_level_three_cell_into_its_children() {
    // a blocked column at x = 3 cuts the first 8 × 8 cell in two
    let wall = |tile: Tile| tile.x() == 3;
    let planner = Planner::new(
        GridSize::new(8, 8),
        vec![TerrainId::named(0); 64],
        Restriction::new(vec![1.0]),
        &Tiles::with_blocked(GridSize::new(8, 8), wall),
    )
    .unwrap();
    let node_or_mixed = CellKind::NODE | CellKind::MIXED;
    assert_eq!(
        planner.level(GridLevel::new(3)).unwrap()[0].kind & node_or_mixed,
        CellKind::MIXED
    );
    assert_eq!(planner.level(GridLevel::new(0)).unwrap()[3], Cell::EMPTY);
    assert_eq!(
        planner.level(GridLevel::new(0)).unwrap()[2].edges,
        CellEdges::W | CellEdges::S
    );
}

#[test]
fn path_skips_point_equal_to_its_neighbour() {
    let mut path = Path::default();
    assert!(path.push(BVector::new(1.0, 0.0, 2.0)));
    assert!(!path.push(BVector::new(1.0, 0.0, 2.0)));
    assert!(path.push(BVector::new(4.0, 0.0, 6.0)));
    path.measure();
    assert_eq!(path.length, Distance::new(5.0));
}

/// `FUN_140f761b0` with fewer than one point between `first` and `last`: no change, 0 fewer.
#[test]
fn stretch_without_points_between_is_left_as_it_is() {
    let mut path = Path {
        points: vec![
            BVector::ZERO,
            BVector::new(1.0, 0.0, 0.0),
            BVector::new(30.0, 0.0, 0.0),
        ],
        length: Distance::ZERO,
    };
    let before = path.points.clone();
    assert_eq!(straighten(&mut path, 0, 1, &tuning().tile_path), 0);
    assert_eq!(straighten(&mut path, 1, 1, &tuning().tile_path), 0);
    assert_eq!(path.points, before);
    // two apart: one point between, the line 30 long takes four points at 7.5 apart: 3 more
    assert_eq!(straighten(&mut path, 0, 2, &tuning().tile_path), -3);
}

/// MarvinBlue/426048731's 9520 in update 758 (the probe's log): the search's twelve tile
/// points become three where the walk is blocked at (103.5, 46.5) and beyond.
#[test]
fn the_smoothing_keeps_the_points_before_blocked_walk() {
    let points = [
        Point::new(109.5, 42.5),
        Point::new(108.5, 42.5),
        Point::new(108.5, 43.5),
        Point::new(107.5, 43.5),
        Point::new(107.5, 44.5),
        Point::new(106.5, 44.5),
        Point::new(105.5, 44.5),
        Point::new(104.5, 44.5),
        Point::new(103.5, 44.5),
        Point::new(103.5, 45.5),
        Point::new(103.5, 46.5),
        Point::new(103.604_164, 46.854_168),
    ];
    let mut path = Path {
        points: points
            .iter()
            .map(|point| BVector::new(point.x(), 0.0, point.y()))
            .collect(),
        length: Distance::ZERO,
    };
    // blocked: any walk from the start that ends beyond y = 44.5
    let mut walls = Walls::default();
    smooth(&mut path, &mut walls, &tuning().tile_path).unwrap();
    let ends: Vec<Point> = path.points.iter().map(|point| point.ground()).collect();
    assert_eq!(
        ends,
        [
            Point::new(109.5, 42.5),
            Point::new(103.5, 44.5),
            Point::new(103.604_164, 46.854_168)
        ]
    );
    assert_eq!(walls.passes, 1);
    // a path of two points tests nothing and ends no pass
    let mut short = Path {
        points: vec![BVector::ZERO, BVector::new(5.0, 0.0, 5.0)],
        length: Distance::ZERO,
    };
    smooth(&mut short, &mut walls, &tuning().tile_path).unwrap();
    assert_eq!((walls.tests, walls.passes), (10, 1));
}

/// The smoothing test's stand-in: any walk from (109.5, 42.5) that ends beyond y = 44.5 is
/// blocked; it counts the tests and the passes that ended.
#[derive(Default)]
struct Walls {
    tests: usize,
    passes: usize,
}

impl SegmentTest<()> for Walls {
    fn blocked(&mut self, from: Point, to: Point) -> Result<bool, ()> {
        self.tests += 1;
        Ok(from == Point::new(109.5, 42.5) && 44.5 < to.y())
    }

    fn pass_ends(&mut self) {
        self.passes += 1;
    }
}

/// `FUN_140f7be80` orders each pair of corners, holds the rectangle on the map (its low
/// corner at 0, its high one at `+0x404`/`+0x406`, the size less one) and closes, from each
/// level-3 cell over it, the node cells under it within the rectangle at their level
/// (`FUN_140f7e070`): a mixed cell's children in the order (2x, 2y), (2x + 1, 2y),
/// (2x + 1, 2y + 1), (2x, 2y + 1). Closing a closed cell or opening an open one changes nothing.
/// `FUN_140f95c00` appends each point through `FUN_140f95060` at the end (a point equal to
/// the last is left out), then takes the source's length or, at most 1e-7, measures anew:
/// `COMISS` with `JBE` (0x140f95d66), so an unordered (NaN) length measures too.
#[test]
fn an_appended_path_takes_the_source_length_or_measures_short_or_nan_one() {
    let mut path = Path::default();
    let _ = path.push(BVector::ZERO);
    let other = Path {
        points: vec![BVector::ZERO, BVector::new(3.0, 0.0, 4.0)],
        length: Distance::new(9.0),
    };
    path.append(&other);
    assert_eq!(
        path.points,
        [BVector::new(0.0, 0.0, 0.0), BVector::new(3.0, 0.0, 4.0)]
    );
    assert_eq!(path.length, Distance::new(9.0));
    for short in [1e-7, 0.0, f32::NAN] {
        let mut path = Path::default();
        path.append(&Path {
            length: Distance::new(short),
            ..other.clone()
        });
        assert_eq!(path.length, Distance::new(5.0), "{short}");
    }
}

#[test]
fn closed_rectangle_closes_the_node_cells_under_it() {
    // tile (0, 0) has no passable neighbour: its level-3, 2 and 1 cells are mixed, and the
    // four tiles of the level-1 cell are nodes
    let walls = |tile: Tile| tile == Tile::new(1, 0) || tile == Tile::new(0, 1);
    let mut planned = Planner::new(
        GridSize::new(16, 8),
        vec![TerrainId::named(0); 128],
        Restriction::new(vec![1.0]),
        &Tiles::with_blocked(GridSize::new(16, 8), walls),
    )
    .unwrap();
    let open = planned.clone();
    let node_or_mixed = CellKind::NODE | CellKind::MIXED;
    assert_eq!(
        planned.level(GridLevel::new(1)).unwrap()[0].kind & node_or_mixed,
        CellKind::MIXED
    );
    assert!(
        planned.level(GridLevel::new(0)).unwrap()[0]
            .kind
            .contains(CellKind::NODE)
    );
    // the corners swapped and the low y off the map
    planned
        .close_rect(PlanRect::new(5, 3, 1, -2), Mark::Set)
        .unwrap();
    assert_eq!(planned.region, PlanRect::new(1, 0, 5, 3));
    let closed = |planner: &Planner, level: u8, x: usize, y: usize| {
        let grid_level = GridLevel::new(level);
        let level_size = planner.level_size(grid_level).width() as usize;
        planner.level(grid_level).unwrap()[y * level_size + x]
            .edges
            .contains(CellEdges::CLOSED)
    };
    let expected: [(u8, usize, usize, bool); 14] = [
        (0, 0, 0, false),
        (0, 1, 0, true),
        (0, 1, 1, true),
        (0, 0, 1, false),
        (1, 0, 0, false),
        (1, 1, 0, true),
        (1, 1, 1, true),
        (1, 0, 1, true),
        (2, 0, 0, false),
        (2, 1, 0, true),
        (2, 1, 1, false),
        (2, 0, 1, false),
        (3, 0, 0, false),
        (3, 1, 0, false),
    ];
    for (level, x, y, shut) in expected {
        assert_eq!(
            closed(&planned, level, x, y),
            shut,
            "level {level} ({x}, {y})"
        );
    }
    // the closed bit is added once
    let once = planned.clone();
    planned
        .close_rect(PlanRect::new(1, 0, 5, 3), Mark::Set)
        .unwrap();
    assert_eq!(planned, once);
    assert_eq!(
        planned.level(GridLevel::new(0)).unwrap()[1].edges,
        CellEdges::CLOSED
    );
    // the high corner past the map's size holds at its last tile: everything reopens
    planned
        .close_rect(PlanRect::new(0, 0, 20, 9), Mark::Clear)
        .unwrap();
    assert_eq!(planned.region, PlanRect::new(0, 0, 15, 7));
    planned.region = open.region;
    assert_eq!(planned, open);
}

#[test]
fn the_open_list_pops_the_cheapest_and_the_older_of_equal_costs_first() {
    let node = |cost: f32| Node {
        cell: CellIndex::new(0),
        at: PlanTile::new(0, 0),
        entry: PlanTile::new(0, 0),
        total: Distance::new(cost),
        cost: Distance::ZERO,
        level: GridLevel::TILES,
        dir: CellEdges::NONE,
        shortcut: false,
        parent: None,
    };
    let mut nodes = Nodes::default();
    let ids = [5.5, 5.25, 5.25, 7.0, 6.5].map(|cost| nodes.push(node(cost)));
    let mut open = OpenList::default();
    for id in ids {
        open.push(id, &nodes).unwrap();
    }
    let order: Vec<NodeIndex> = from_fn(|| open.pop().unwrap()).collect();
    assert_eq!(order, [1, 2, 0, 4, 3].map(|index| ids[index]));
    assert!(open.is_empty());
}

/// A search's result keeps the game's code, and the tests its readers make by code: `result < 2`
/// reaches (off the map too), 1 and 2 lead on.
#[test]
fn search_result_reads_by_the_game_code() {
    let all = [Found::OffMap, Found::Goal, Found::Nearest, Found::Nothing];
    assert_eq!(all.map(Found::code), [0, 1, 2, 3]);
    assert_eq!(all.map(Found::reaches), [true, true, false, false]);
    assert_eq!(all.map(Found::leads), [false, true, true, false]);
}

/// A cell is the grid's two bytes: they read back as they were written, the low nibble of byte
/// 1 holds a bit per child off the map, and a mark is added and subtracted as the game's byte
/// arithmetic does (an added mark that is set already carries into the next bit).
#[test]
fn cell_keeps_the_game_two_bytes_and_byte_arithmetic() {
    let cell = Cell::from_bytes([0x3f, 0xf5]);
    assert_eq!(cell.bytes(), [0x3f, 0xf5]);
    assert!(
        cell.edges
            .contains(CellEdges::EDGES | CellEdges::OPEN | CellEdges::CLOSED)
    );
    assert_eq!(cell.kind.off_map(), CellKind::from_bits_retain(5));
    assert_eq!(
        CellKind::child_off_map(0) | CellKind::child_off_map(2),
        CellKind::from_bits_retain(5)
    );
    assert_eq!(CellEdges::EDGES.bits(), 0xf);
    let open = CellEdges::W.plus(CellEdges::OPEN);
    assert_eq!(open, CellEdges::W | CellEdges::OPEN);
    assert_eq!(open.minus(CellEdges::OPEN), CellEdges::W);
    // the game's add on a set mark: OPEN twice is CLOSED
    assert_eq!(open.plus(CellEdges::OPEN), CellEdges::W | CellEdges::CLOSED);
    assert_eq!(
        CellKind::NODE.plus(CellKind::GOAL).minus(CellKind::NODE),
        CellKind::GOAL
    );
    assert_eq!(Cell::default(), Cell::EMPTY);
}

/// A grid level: the tiles at 0, the top at 3, each a step up or down, read as the shift.
#[test]
fn grid_level_steps_between_the_tiles_and_the_top() {
    assert_eq!(GridLevel::ALL, [0, 1, 2, 3].map(GridLevel::new));
    assert_eq!(GridLevel::TILES.up().up().up(), GridLevel::TOP);
    assert_eq!(GridLevel::TOP.down(), GridLevel::new(2));
    assert_eq!(GridLevel::TOP.get(), 3);
    assert!(GridLevel::TILES < GridLevel::TOP);
}

/// The grid reads and changes a cell by its place, and gives a level's run of cells.
#[test]
fn the_grid_reads_cell_by_its_place() {
    let mut cells = Cells::cleared(4);
    cells.checked_mut(CellIndex::new(2)).unwrap().kind = CellKind::NODE;
    assert_eq!(
        cells.checked(CellIndex::new(2)).unwrap().kind,
        CellKind::NODE
    );
    assert_eq!(
        cells.run(CellIndex::new(1), 2).unwrap(),
        &[
            Cell::EMPTY,
            Cell {
                edges: CellEdges::NONE,
                kind: CellKind::NODE
            }
        ]
    );
}

/// A cell's place is the game's sum of the level's first cell, its width times y and x: a place
/// past a level's last row is the next level's cell, and a place before the grid or past its
/// end is an error where the game would read memory outside it, not a panic.
#[test]
fn cell_off_the_grid_is_an_error_and_row_past_its_level_the_next_level() {
    let mut planner = Planner::new(
        GridSize::new(8, 8),
        vec![TerrainId::named(0); 64],
        Restriction::new(vec![1.0]),
        &open_map(GridSize::new(8, 8)),
    )
    .unwrap();
    // level 0 holds 64 cells: row 8 is level 1's first cell
    assert_eq!(
        planner.index(GridLevel::TILES, PlanTile::new(0, 8)),
        planner.index(GridLevel::new(1), PlanTile::new(0, 0))
    );
    assert_eq!(
        planner.cell(GridLevel::TILES, PlanTile::new(0, 8)).unwrap(),
        planner.level(GridLevel::new(1)).unwrap()[0]
    );
    // the four levels hold 64 + 16 + 4 + 1 cells
    for (level, at) in [
        (GridLevel::TILES, PlanTile::new(-1, 0)),
        (GridLevel::TOP, PlanTile::new(1, 0)),
        (GridLevel::TOP, PlanTile::new(0, -86)),
    ] {
        let error = planner.cell(level, at).unwrap_err();
        assert_eq!(error.reason(), "a cell outside the planner's grid");
    }
    assert_eq!(
        planner
            .search_tiles(PlanTile::new(-1, 0), &tuning().tile_path)
            .unwrap_err()
            .reason(),
        "a cell outside the planner's grid"
    );
}

/// The grid's lookups refuse a place before its first cell or past its last, and a run that
/// ends past its last.
#[test]
fn the_grid_refuses_place_or_run_it_does_not_hold() {
    let mut cells = Cells::cleared(4);
    for place in [-1, 4] {
        assert!(cells.checked(CellIndex::new(place)).is_err());
        assert!(cells.checked_mut(CellIndex::new(place)).is_err());
    }
    assert!(cells.run(CellIndex::new(3), 2).is_err());
    assert!(cells.run(CellIndex::new(-1), 1).is_err());
    assert_eq!(cells.run(CellIndex::new(4), 0).unwrap(), []);
}

/// The pool hands out its nodes' places: a place another pool handed out is an error in this
/// one, and so is an open node the pool has not got.
#[test]
fn the_pool_refuses_node_it_does_not_hold() {
    let node = Node {
        cell: CellIndex::new(0),
        at: PlanTile::new(0, 0),
        entry: PlanTile::new(0, 0),
        total: Distance::new(1.0),
        cost: Distance::ZERO,
        level: GridLevel::TILES,
        dir: CellEdges::NONE,
        shortcut: false,
        parent: None,
    };
    let mut larger = Nodes::default();
    let _ = larger.push(node);
    let second = larger.push(node);
    let mut nodes = Nodes::default();
    let _ = nodes.push(node);
    assert_eq!(
        nodes.checked(second).unwrap_err().reason(),
        "a node the planner's pool does not hold"
    );
    assert!(nodes.checked_mut(second).is_err());
    let mut open = OpenList::default();
    assert!(open.push(second, &nodes).is_err());
    assert!(open.is_empty());
}

/// `FUN_140f7d160`: a tile is near a passable one when it or a neighbour (right, below, left,
/// above) has an edge; a neighbour off the map has none.
#[test]
fn tile_is_near_passable_when_it_or_neighbour_has_an_edge() {
    // the open tiles are x 0 and 1, y 0 to 3
    let blocked = |tile: Tile| 2 <= tile.x() || 4 <= tile.y();
    let planner = Planner::new(
        GridSize::new(8, 8),
        vec![TerrainId::named(0); 64],
        Restriction::new(vec![1.0]),
        &Tiles::with_blocked(GridSize::new(8, 8), blocked),
    )
    .unwrap();
    let near = |x: i16, y: i16| planner.near_passable(PlanTile::new(x, y)).unwrap();
    // itself, the left neighbour, the one above, a tile off the map with a neighbour on it
    assert!(near(0, 0) && near(2, 3) && near(0, 4) && near(-1, 2));
    // every neighbour blocked, and the corner's beyond the map
    assert!(!near(3, 3) && !near(7, 7));
}
