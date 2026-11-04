use super::*;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::TerrainMask;
use crate::pathing::FixedDistanceSquared;
use crate::pathing::ShortFound;
use crate::test_ids::object_id;

/// A search on an empty mesh toward a point target at `at` within `range` (fixed point), its
/// start node at the origin with cost 0.
fn search_toward(mesh: &Mesh, at: FixedPoint, range: i32) -> Search<'_> {
    let goal = Goal {
        at,
        radius: FixedDistance::ZERO,
        rx: FixedDistance::ZERO,
        ry: FixedDistance::ZERO,
        rrx: FixedDistance::ZERO,
        rry: FixedDistance::ZERO,
        range: FixedDistance::new(range),
        range2: FixedDistanceSquared::of(FixedDistance::new(range)),
    };
    let mut search = Search {
        ctx: Ctx {
            mesh,
            ignore: &[],
            clearance: FixedDistance::ZERO,
        },
        goal,
        bounds: FixedRect::new(0, 0, 64 * ONE, 64 * ONE),
        open: Vec::new(),
        arena: Vec::new(),
        polys: Vec::new(),
        goal_node: None,
        best: NodeIndex::START,
    };
    let origin = FixedPoint::new(0, 0);
    search.arena.push(Node {
        cost: FixedDistance::ZERO,
        total: search.goal.heuristic(origin),
        idx: NodeIndex::START,
        parent: None,
        at: origin,
        kind: Kind::Start,
    });
    search
}

/// A forward run of polygon 0 over its vertex 0, for the goal and foot tests.
const RUN_0: Run = Run {
    poly: PolyIndex::of(0),
    dir: WalkDir::Forward,
    lo: VertexIndex::FIRST,
    hi: VertexIndex::FIRST,
};

/// A node's index is its place in the arena, the start's 0; a goal node beats only a cheaper
/// one, and anything beats none.
#[test]
fn node_index_is_its_arena_place_and_any_cost_beats_no_goal() {
    let mesh = Mesh::new(64);
    let mut search = search_toward(&mesh, FixedPoint::new(10 * ONE, 10 * ONE), 2 * ONE);
    assert_eq!(NodeIndex::next_of(&search.arena), NodeIndex::new(1));
    assert_eq!(NodeIndex::START.get(), 0);
    assert!(search.beats_goal(FixedDistance::new(i32::MAX)).unwrap());
    let node = search.add(
        FixedDistance::TILE,
        NodeIndex::START,
        FixedPoint::new(ONE, 0),
        Kind::Waypoint,
    );
    assert_eq!(
        (node.idx, node.parent),
        (NodeIndex::new(1), Some(NodeIndex::START))
    );
    search.goal_node = Some(node.idx);
    assert!(search.beats_goal(FixedDistance::new(ONE - 1)).unwrap());
    assert!(!search.beats_goal(FixedDistance::TILE).unwrap());
    search.node_mut(node.idx).unwrap().cost = FixedDistance::ZERO;
    assert!(!search.beats_goal(FixedDistance::ZERO).unwrap());
    // a goal node the arena does not hold fails
    search.goal_node = Some(NodeIndex::new(7));
    assert!(search.beats_goal(FixedDistance::ZERO).is_err());
}

/// A vertex index steps round its polygon (`next_in`, `prev_in`) or along its list (`succ`,
/// `pred`), and reads the polygon's columns.
#[test]
fn vertex_index_steps_round_its_polygon_and_reads_its_columns() {
    let (first, last) = (VertexIndex::FIRST, VertexIndex::new(3));
    assert_eq!((last.next_in(4), first.prev_in(4)), (first, last));
    assert_eq!(
        (first.next_in(4), last.prev_in(4)),
        (VertexIndex::new(1), VertexIndex::new(2))
    );
    assert_eq!(
        (last.succ(), last.pred()),
        (VertexIndex::new(4), VertexIndex::new(2))
    );
    assert!(first < last && last.get() == 3);
    let mut poly = square(false);
    assert_eq!(poly.count(), 4);
    assert_eq!(poly.vertex(VertexIndex::new(2)).unwrap(), poly.vertices[2]);
    assert_eq!(poly.side(VertexIndex::new(1)).unwrap(), poly.sides[1]);
    assert_eq!(poly.shape(last).unwrap(), poly.shapes[3]);
    assert_eq!(poly.cost(first).unwrap(), poly.costs[0]);
    *poly.cost_mut(first).unwrap() = FixedDistance::TILE;
    assert_eq!(poly.costs[0], FixedDistance::TILE);
    assert!(poly.vertex(last.succ()).is_err());
}

/// A polygon's index is its place in the search's list.
#[test]
fn polygon_index_reads_the_search_list() {
    let mesh = Mesh::new(64);
    let mut search = search_toward(&mesh, FixedPoint::new(10 * ONE, 10 * ONE), 2 * ONE);
    assert!(search.poly(PolyIndex::of(0)).is_err());
    search.polys.push(Poly::default());
    search.polys.push(Poly::default());
    assert_eq!(PolyIndex::of(1).get(), 1);
    search.poly_mut(PolyIndex::of(1)).unwrap().flag = true;
    assert!(search.poly(PolyIndex::of(1)).unwrap().flag);
    assert!(!search.poly(PolyIndex::of(0)).unwrap().flag);
}

/// `FUN_140fa3860`: a node within range becomes the goal while it is cheaper than the goal so
/// far; any other node becomes the best when it lies nearer than the best's estimate, and a
/// node no nearer adds nothing.
#[test]
fn goal_candidate_in_range_and_cheaper_becomes_the_goal_else_maybe_the_best() {
    let mesh = Mesh::new(64);
    let mut search = search_toward(&mesh, FixedPoint::new(10 * ONE, 10 * ONE), 2 * ONE);
    assert!(
        search
            .goal_test(Corner {
                cost: FixedDistance::new(5 * ONE),
                parent: NodeIndex::START,
                run: RUN_0,
                point: FixedPoint::new(11 * ONE, 10 * ONE)
            })
            .unwrap()
    );
    assert_eq!(search.goal_node, Some(NodeIndex::new(1)));
    // in range but dearer: the best instead, as it lies nearer than the start's estimate
    assert!(
        !search
            .goal_test(Corner {
                cost: FixedDistance::new(6 * ONE),
                parent: NodeIndex::START,
                run: RUN_0,
                point: FixedPoint::new(11 * ONE, 10 * ONE)
            })
            .unwrap()
    );
    assert_eq!(
        (search.goal_node, search.best),
        (Some(NodeIndex::new(1)), NodeIndex::new(2))
    );
    // out of range and farther than the best's estimate (one tile): nothing
    assert!(
        !search
            .goal_test(Corner {
                cost: FixedDistance::TILE,
                parent: NodeIndex::START,
                run: RUN_0,
                point: FixedPoint::new(30 * ONE, 30 * ONE)
            })
            .unwrap()
    );
    assert_eq!((search.arena.len(), search.best), (3, NodeIndex::new(2)));
}

/// `FUN_140fa8210`: the goal's foot on an axis edge that spans it, reached at the edge's
/// start cost plus the walk along the edge, is a goal candidate; an edge that does not span
/// the goal gives none.
#[test]
fn the_goal_foot_on_spanning_edge_is_goal_candidate() {
    let mesh = Mesh::new(64);
    let mut search = search_toward(&mesh, FixedPoint::new(10 * ONE, 10 * ONE), 2 * ONE);
    search
        .foot_test(Foot {
            start: FixedPoint::new(9 * ONE, 12 * ONE),
            end: FixedPoint::new(9 * ONE, 20 * ONE),
            extent_x: 0,
            run: RUN_0,
            start_cost: FixedDistance::TILE,
            parent: NodeIndex::START,
        })
        .unwrap();
    assert_eq!(search.arena.len(), 1);
    search
        .foot_test(Foot {
            start: FixedPoint::new(15 * ONE, 9 * ONE),
            end: FixedPoint::new(5 * ONE, 9 * ONE),
            extent_x: 1,
            run: RUN_0,
            start_cost: FixedDistance::TILE,
            parent: NodeIndex::START,
        })
        .unwrap();
    let goal = *search.node(search.goal_node.unwrap()).unwrap();
    assert_eq!(
        (goal.at, goal.cost),
        (
            FixedPoint::new(10 * ONE, 9 * ONE),
            FixedDistance::new(6 * ONE)
        )
    );
}

/// `FUN_140fa8350`: a junction moved `clearance` into the free space beside each side, a contact at
/// its side's start turning it along the side.
#[test]
fn contact_point_moves_off_its_side() {
    let fixed_point = FixedPoint::new(100, 200);
    let moved: Vec<FixedPoint> = ShapeSide::ALL
        .into_iter()
        .flat_map(|side| {
            [false, true]
                .map(|start| contact_point(fixed_point, side, FixedDistance::new(3), start))
        })
        .collect();
    assert_eq!(
        moved,
        [
            FixedPoint::new(97, 203),
            FixedPoint::new(103, 203),
            FixedPoint::new(97, 197),
            FixedPoint::new(97, 203),
            FixedPoint::new(103, 197),
            FixedPoint::new(97, 197),
            FixedPoint::new(103, 203),
            FixedPoint::new(103, 197),
        ]
    );
}

/// `FUN_140fa39f0`: a point goes onto a side of the rectangle shrunk by `clearance` (the side's
/// coordinate set, the other held between the shrunk corners).
#[test]
fn point_snaps_onto_side_within_its_corners() {
    let fixed_rect = FixedRect::new(0, 0, 100, 50);
    let snap = |point: FixedPoint, side: usize| {
        let mut fixed_point = point;
        snap_to_side(
            &mut fixed_point,
            fixed_rect,
            ShapeSide::ALL[side],
            FixedDistance::new(5),
        );
        fixed_point
    };
    assert_eq!(snap(FixedPoint::new(-20, 30), 0), FixedPoint::new(5, 5));
    assert_eq!(snap(FixedPoint::new(200, 30), 0), FixedPoint::new(95, 5));
    assert_eq!(snap(FixedPoint::new(40, 30), 0), FixedPoint::new(40, 5));
    assert_eq!(snap(FixedPoint::new(40, -9), 1), FixedPoint::new(95, 5));
    assert_eq!(snap(FixedPoint::new(40, 99), 1), FixedPoint::new(95, 45));
    assert_eq!(snap(FixedPoint::new(40, 20), 2), FixedPoint::new(40, 45));
    assert_eq!(snap(FixedPoint::new(40, 20), 3), FixedPoint::new(5, 20));
}

#[test]
fn the_integer_square_root_is_exact_below_and_truncates_between_squares() {
    assert_eq!(isqrt(0), 0);
    assert_eq!(isqrt(1), 1);
    assert_eq!(isqrt(15), 3);
    assert_eq!(isqrt(16), 4);
    assert_eq!(isqrt(u64::from(u32::MAX) * u64::from(u32::MAX)), u32::MAX);
    // a tile squared in fixed point (2^44) gives a tile
    assert_eq!(isqrt(1 << 44), ONE as u32);
}

#[test]
fn ray_crosses_the_side_it_faces_and_reports_how_far() {
    // a ray from (0, 1) to (4, 1) against the box (2, 0)-(3, 2): its left side at x = 2
    let fixed_rect = FixedRect::new(2 * ONE, 0, 3 * ONE, 2 * ONE);
    let mut ray_fraction = RayFraction::START;
    let hit = ray_rect(
        FixedPoint::new(0, ONE),
        FixedPoint::new(4 * ONE, ONE),
        fixed_rect,
        &mut ray_fraction,
    );
    assert_eq!(
        (hit, ray_fraction),
        (
            Some((Meet::Cross, ShapeSide::LowX)),
            RayFraction::new(ONE / 2)
        )
    );
    // a ray going the other way meets the right side
    let mut ray_fraction = RayFraction::START;
    let hit = ray_rect(
        FixedPoint::new(4 * ONE, ONE),
        FixedPoint::new(0, ONE),
        fixed_rect,
        &mut ray_fraction,
    );
    assert_eq!(
        (hit, ray_fraction),
        (
            Some((Meet::Cross, ShapeSide::HighX)),
            RayFraction::new(ONE / 4)
        )
    );
    // a ray beside the box meets nothing
    let mut ray_fraction = RayFraction::START;
    assert_eq!(
        ray_rect(
            FixedPoint::new(0, 3 * ONE),
            FixedPoint::new(4 * ONE, 3 * ONE),
            fixed_rect,
            &mut ray_fraction
        ),
        None
    );
}

/// `FUN_140faa4e0` on the y sides: a ray along y within the box's open x span meets the bottom
/// going up and the top going down, nearer than `ray_fraction` and from at or outside the side; a
/// slanted ray meets them as `FUN_140fa9f10` does.
#[test]
fn ray_crosses_the_bottom_or_the_top_along_y() {
    let fixed_rect = FixedRect::new(2 * ONE, 0, 3 * ONE, 2 * ONE);
    let x = 2 * ONE + ONE / 2;
    let cast = |from: FixedPoint, to: FixedPoint, fraction: RayFraction| {
        let mut ray_fraction = fraction;
        let hit = ray_rect(from, to, fixed_rect, &mut ray_fraction);
        (hit, ray_fraction)
    };
    // up from y = -1 to 3: the bottom at a quarter of the way
    assert_eq!(
        cast(
            FixedPoint::new(x, -ONE),
            FixedPoint::new(x, 3 * ONE),
            RayFraction::START
        ),
        (
            Some((Meet::Cross, ShapeSide::LowY)),
            RayFraction::new(ONE / 4)
        )
    );
    // down from 3 to -1: the top at a quarter
    assert_eq!(
        cast(
            FixedPoint::new(x, 3 * ONE),
            FixedPoint::new(x, -ONE),
            RayFraction::START
        ),
        (
            Some((Meet::Cross, ShapeSide::HighY)),
            RayFraction::new(ONE / 4)
        )
    );
    // a nearer hit already: none
    assert_eq!(
        cast(
            FixedPoint::new(x, -ONE),
            FixedPoint::new(x, 3 * ONE),
            RayFraction::new(ONE / 8)
        ),
        (None, RayFraction::new(ONE / 8))
    );
    // from inside the box, or beside it (x on its edge): none
    assert_eq!(
        cast(
            FixedPoint::new(x, ONE),
            FixedPoint::new(x, 3 * ONE),
            RayFraction::START
        ),
        (None, RayFraction::START)
    );
    assert_eq!(
        cast(
            FixedPoint::new(3 * ONE, -ONE),
            FixedPoint::new(3 * ONE, 3 * ONE),
            RayFraction::START
        ),
        (None, RayFraction::START)
    );
    assert_eq!(
        cast(
            FixedPoint::new(x, ONE),
            FixedPoint::new(x, -ONE),
            RayFraction::START
        ),
        (None, RayFraction::START)
    );
    // a slanted ray up through the bottom: the segment test against it
    let (from, to) = (
        FixedPoint::new(2 * ONE + ONE / 4, -ONE),
        FixedPoint::new(2 * ONE + 3 * ONE / 4, 3 * ONE),
    );
    let mut ray_fraction = RayFraction::START;
    let Some(meet) = segment_hit(
        from,
        to,
        FixedPoint::new(2 * ONE, 0),
        FixedPoint::new(3 * ONE, 0),
        &mut ray_fraction,
    ) else {
        panic!("the ray meets the bottom");
    };
    assert_eq!(
        cast(from, to, RayFraction::START),
        (Some((meet, ShapeSide::LowY)), ray_fraction)
    );
    // and down through the top
    let mut ray_fraction = RayFraction::START;
    let Some(meet) = segment_hit(
        to,
        from,
        FixedPoint::new(2 * ONE, 2 * ONE),
        FixedPoint::new(3 * ONE, 2 * ONE),
        &mut ray_fraction,
    ) else {
        panic!("the ray meets the top");
    };
    assert_eq!(
        cast(to, from, RayFraction::START),
        (Some((meet, ShapeSide::HighY)), ray_fraction)
    );
}

/// `FUN_140faa380`'s four outcomes: a proper crossing (its parameter into `ray_fraction`), a touch
/// at the ray's end, a touch at the ray's start on the segment's end (`ray_fraction` 0), and none
/// for a parallel segment or one beyond `ray_fraction`.
#[test]
fn segment_meets_ray_by_crossing_or_touching() {
    let point_at = |x: i32, y: i32| FixedPoint::new(x * ONE, y * ONE);
    let (start, end) = (point_at(2, 0), point_at(2, 2));
    let mut ray_fraction = RayFraction::START;
    assert_eq!(
        segment_hit(
            point_at(0, 1),
            point_at(4, 1),
            start,
            end,
            &mut ray_fraction
        ),
        Some(Meet::Cross)
    );
    assert_eq!(ray_fraction, RayFraction::new(ONE / 2));
    let mut ray_fraction = RayFraction::START;
    assert_eq!(
        segment_hit(
            point_at(0, 1),
            point_at(2, 1),
            start,
            end,
            &mut ray_fraction
        ),
        Some(Meet::Touch)
    );
    assert_eq!(ray_fraction, RayFraction::new(ONE));
    let mut ray_fraction = RayFraction::START;
    assert_eq!(
        segment_hit(start, point_at(4, 1), start, end, &mut ray_fraction),
        Some(Meet::TouchAtStart)
    );
    assert_eq!(ray_fraction, RayFraction::new(0));
    let mut ray_fraction = RayFraction::START;
    assert_eq!(
        segment_hit(
            point_at(0, 1),
            point_at(4, 1),
            point_at(0, 2),
            point_at(4, 2),
            &mut ray_fraction
        ),
        None
    );
    let mut ray_fraction = RayFraction::new(ONE / 4);
    assert_eq!(
        segment_hit(
            point_at(0, 1),
            point_at(4, 1),
            start,
            end,
            &mut ray_fraction
        ),
        None
    );
    assert_eq!(ray_fraction, RayFraction::new(ONE / 4));
}

/// `FUN_140fa16d0`: a segment through the box keeps the part inside, each end moved to the
/// edge it crossed (`(dy * (edge - x) >> 22 << 22) / dx`, truncated); a segment beside the
/// box is none, one inside it stays; the box's corners may come in any order.
#[test]
fn segment_clipped_to_the_box_keeps_its_part_inside() {
    let fixed_rect = FixedRect::new(0, 0, 2 * ONE, 2 * ONE);
    assert_eq!(
        clip(
            FixedPoint::new(-ONE, ONE),
            FixedPoint::new(3 * ONE, ONE),
            fixed_rect
        ),
        Some((FixedPoint::new(0, ONE), FixedPoint::new(2 * ONE, ONE)))
    );
    // first the left edge, then the bottom, then the right edge for the far end
    assert_eq!(
        clip(
            FixedPoint::new(-ONE, -ONE),
            FixedPoint::new(3 * ONE, 2 * ONE),
            fixed_rect
        ),
        Some((
            FixedPoint::new(1_398_101, 0),
            FixedPoint::new(2 * ONE, 5_242_880)
        ))
    );
    assert_eq!(
        clip(
            FixedPoint::new(3 * ONE, 0),
            FixedPoint::new(4 * ONE, ONE),
            fixed_rect
        ),
        None
    );
    let inside = (FixedPoint::new(ONE / 2, ONE), FixedPoint::new(ONE, ONE / 2));
    assert_eq!(clip(inside.0, inside.1, fixed_rect), Some(inside));
    assert_eq!(
        clip(
            FixedPoint::new(-ONE, ONE),
            FixedPoint::new(3 * ONE, ONE),
            FixedRect::new(2 * ONE, 2 * ONE, 0, 0)
        ),
        Some((FixedPoint::new(0, ONE), FixedPoint::new(2 * ONE, ONE)))
    );
}

/// `FUN_140faa170` (through `FUN_140f9d3e0`): blocked when the start, out of range, lies in
/// a shape with fewer than two escape candidates; a free start or one in range is not. The
/// terrain's temporary shapes leave the mesh after it.
#[test]
fn start_deep_in_blocked_terrain_is_blocked_unless_in_range() {
    let blocked = |_: u32, _: u32| 1;
    let free = |_: u32, _: u32| 0;
    let mut query = Query {
        from: FixedPoint::new(5 * ONE + ONE / 2, 5 * ONE + ONE / 2),
        to: FixedPoint::new(8 * ONE + ONE / 2, 5 * ONE + ONE / 2),
        radius: Distance::new(0.25),
        target_size: Footprint::new(0.0, 0.0),
        range: Distance::ZERO,
        blocked: 1,
    };
    let mut mesh = Mesh::new(16);
    let mut pather = Pather::default();
    // each row of blocked tiles is a shape half a tile wider all round: the start sits 0.75
    // tiles inside row 5's, beyond twice the radius from its sides
    assert!(pather.start_blocked(&mut mesh, &blocked, &query).unwrap());
    let count = mesh.shapes.len() as u32;
    assert!(0 < count && (0..count).all(|id| !mesh.is_shape(ShapeId::new(id))));
    assert!(!pather.start_blocked(&mut mesh, &free, &query).unwrap());
    query.range = Distance::new(3.0);
    assert!(!pather.start_blocked(&mut mesh, &blocked, &query).unwrap());
}

/// `FUN_140f9e540`'s entry tests as compiled: the target's gap clamps each axis by `MAXSS`
/// (`FUN_14118faf0`: an unordered one is 0) and is in range by `COMISS`/`JC` (0x140f9e5f5);
/// each end lies off the map by `COMISS`/`JA` below 0 and `JNC` at the size (0x140f9e67c..
/// 0x140f9e6da): an unordered coordinate is on it.
#[test]
fn the_entry_float_tests_take_the_game_nan_paths() {
    let call = |from: Point, target: Option<Target>| Call {
        from,
        to: Point::new(7.0, 5.0),
        radius: Distance::new(0.2),
        range: Distance::ZERO,
        target,
        listed: Vec::new(),
        map: GridSize::new(16, 16),
        mask: TerrainMask::of_bits(6),
    };
    let target = Target {
        id: object_id(9),
        position: Point::new(5.0, 5.0),
        half_size: Footprint::new(0.5, 0.5),
    };
    assert_eq!(
        call(Point::new(f32::NAN, 5.0), Some(target)).entry(),
        Entry::InRange
    );
    assert!(matches!(
        call(Point::new(f32::NAN, 5.0), None).entry(),
        Entry::Search { .. }
    ));
    assert_eq!(call(Point::new(-0.1, 5.0), None).entry(), Entry::Invalid);
    assert_eq!(call(Point::new(16.0, 5.0), None).entry(), Entry::Invalid);
    assert_eq!(call(Point::new(5.0, 16.0), None).entry(), Entry::Invalid);
    assert!(matches!(
        call(Point::new(0.0, 15.9), None).entry(),
        Entry::Search { .. }
    ));
}

/// A search on a 32-tile mesh holding `walls` (tiles, `[x0, y0, x1, y1]`) from `from` to `to`
/// (tiles) with a radius of a quarter tile; the path's points in tiles. With `ignored`, every
/// wall is ignored.
fn around(walls: &[[f32; 4]], from: Point, to: Point, ignored: bool) -> (ShortFound, Vec<Point>) {
    let fixed = |value: f32| (value * ONE as f32) as i32;
    let mut mesh = Mesh::new(32);
    let ids: Vec<ShapeId> = walls
        .iter()
        .map(|rect| {
            mesh.add(FixedRect::new(
                fixed(rect[0]),
                fixed(rect[1]),
                fixed(rect[2]),
                fixed(rect[3]),
            ))
        })
        .collect();
    let query = Query {
        from: FixedPoint::new(fixed(from.x()), fixed(from.y())),
        to: FixedPoint::new(fixed(to.x()), fixed(to.y())),
        radius: Distance::new(0.25),
        target_size: Footprint::new(0.0, 0.0),
        range: Distance::ZERO,
        blocked: 0,
    };
    let ignore = if ignored { ids } else { Vec::new() };
    let found = Pather::default()
        .search(&mut mesh, &|_, _| 0, &query, &ignore)
        .unwrap();
    let tiles = |value: i32| value as f32 / ONE as f32;
    let points = found
        .points
        .iter()
        .map(|fixed_point| Point::new(tiles(fixed_point.x()), tiles(fixed_point.y())))
        .collect();
    (found.result, points)
}

/// A shape keeps the unit's centre `clearance` (half a tile less the radius) inside its rectangle:
/// the search walks round a wall (10.25..10.75 by 4.25..11.75 for the centre) at its nearer end
/// (3.25 tiles down against 4.25 up), over its two corners; ignored, the wall leaves the path
/// direct (the start, the start node's own point, the goal).
#[test]
fn path_bends_round_wall_at_its_nearer_end() {
    let wall = [[10.0, 4.0, 11.0, 12.0]];
    assert_eq!(
        around(&wall, Point::new(6.5, 8.5), Point::new(14.5, 8.5), false),
        (
            ShortFound::Reached,
            vec![
                Point::new(6.5, 8.5),
                Point::new(10.25, 11.75),
                Point::new(10.75, 11.75),
                Point::new(14.5, 8.5)
            ]
        )
    );
    assert_eq!(
        around(&wall, Point::new(6.5, 8.5), Point::new(14.5, 8.5), true),
        (
            ShortFound::Reached,
            vec![
                Point::new(6.5, 8.5),
                Point::new(6.5, 8.5),
                Point::new(14.5, 8.5)
            ]
        )
    );
}

/// A start inside a U open away from the goal: the path leaves by the open side round the
/// nearer arm's end (the lower, 2.75 tiles down against 3.75 up) and runs along it to the
/// corner it shares with the U's back.
#[test]
fn path_leaves_horseshoe_by_its_open_side() {
    let horseshoe = [
        [8.0, 4.0, 9.0, 12.0],
        [4.0, 4.0, 9.0, 5.0],
        [4.0, 11.0, 9.0, 12.0],
    ];
    assert_eq!(
        around(
            &horseshoe,
            Point::new(6.5, 8.5),
            Point::new(14.5, 8.5),
            false
        ),
        (
            ShortFound::Reached,
            vec![
                Point::new(6.5, 8.5),
                Point::new(4.25, 11.25),
                Point::new(4.25, 11.75),
                Point::new(8.75, 11.75),
                Point::new(14.5, 8.5)
            ]
        )
    );
}

/// A goal shut inside a box: the path is partial (result 2) and ends at the point nearest the
/// goal the unit's centre reaches, on the box's outer side.
#[test]
fn goal_shut_in_box_gives_partial_path_to_its_nearest_point() {
    let walls = [
        [10.0, 6.0, 15.0, 7.0],
        [10.0, 10.0, 15.0, 11.0],
        [10.0, 6.0, 11.0, 11.0],
        [14.0, 6.0, 15.0, 11.0],
    ];
    assert_eq!(
        around(&walls, Point::new(6.5, 8.5), Point::new(12.5, 8.5), false),
        (
            ShortFound::Partial,
            vec![Point::new(6.5, 8.5), Point::new(10.25, 8.5)]
        )
    );
    assert_eq!(
        around(&walls, Point::new(6.5, 8.5), Point::new(12.5, 8.5), true).0,
        ShortFound::Reached
    );
}

/// Two staggered walls: the path reaches the goal over one corner of the second wall (a
/// shortest path bends only at corners); the first wall's lower end lies off it.
#[test]
fn path_past_staggered_walls_bends_only_at_their_corners() {
    let walls = [[10.0, 4.0, 11.0, 9.0], [12.0, 8.0, 13.0, 14.0]];
    let (result, points) = around(&walls, Point::new(6.5, 6.5), Point::new(16.5, 12.5), false);
    assert_eq!(result, ShortFound::Reached);
    assert_eq!(points.first(), Some(&Point::new(6.5, 6.5)));
    assert_eq!(points.last(), Some(&Point::new(16.5, 12.5)));
    let corners = [
        Point::new(12.25, 8.25),
        Point::new(12.75, 8.25),
        Point::new(12.25, 13.75),
        Point::new(12.75, 13.75),
    ];
    let between = &points[1..points.len() - 1];
    assert!(2 < points.len(), "{points:?}");
    assert!(
        between.iter().all(|point| corners.contains(point)),
        "{points:?}"
    );
}

/// A query from (5, 5) to (7, 5) with a radius of a quarter tile and range `range`.
fn range_query(range: Distance) -> Query {
    Query {
        from: FixedPoint::new(5 * ONE, 5 * ONE),
        to: FixedPoint::new(7 * ONE, 5 * ONE),
        radius: Distance::new(0.25),
        target_size: Footprint::new(0.0, 0.0),
        range,
        blocked: 0,
    }
}

#[test]
fn the_search_truncates_its_radius_and_range_to_fixed_point() {
    // FUN_140f9d300: (int)(longlong)(range * 2^22). The goal lies 1.75 tiles beyond the
    // radius: a range of 1.75 holds it at once (result 0); the float just below truncates to
    // 0x6fffff and searches
    let words = |_: u32, _: u32| 0;
    let below = f32::from_bits(1.75f32.to_bits() - 1);
    let mut mesh = Mesh::new(16);
    let mut pather = Pather::default();
    let found = pather.search(&mut mesh, &words, &range_query(Distance::new(1.75)), &[]);
    assert_eq!(
        found.map(|short_path| short_path.result),
        Ok(ShortFound::InRange)
    );
    let found = pather.search(&mut mesh, &words, &range_query(Distance::new(below)), &[]);
    assert_eq!(
        found.map(|short_path| short_path.result),
        Ok(ShortFound::Reached)
    );
}

/// A square outline of four tiles' side at the origin, its sides 0 to 3 in turn (with `run`,
/// side 0 split at (2, 0) into a run of two edges), its vertices of no shape.
fn square(run: bool) -> Poly {
    let mut vertices = vec![
        FixedPoint::new(0, 0),
        FixedPoint::new(4 * ONE, 0),
        FixedPoint::new(4 * ONE, 4 * ONE),
        FixedPoint::new(0, 4 * ONE),
    ];
    let mut sides = ShapeSide::ALL.to_vec();
    if run {
        vertices.insert(1, FixedPoint::new(2 * ONE, 0));
        sides.insert(1, ShapeSide::LowY);
    }
    let count = vertices.len();
    Poly {
        vertices,
        sides,
        shapes: vec![None; count],
        costs: vec![FixedDistance::ZERO; count],
        flag: false,
    }
}

/// `FUN_140fa77b0`: the outline's edge whose side is `side` and that holds the point, the
/// first vertex of its run (a run's edges are one); none for a point on no such edge, an
/// outline of no vertices or no shape; a vertex of a shape whose overlap with the shape asked
/// holds the point answers whatever the side.
#[test]
fn the_edge_holding_point_is_the_first_vertex_of_its_side_run() {
    let mut mesh = Mesh::new(64);
    mesh.shapes = vec![
        FixedRect::new(0, 0, 4 * ONE, 4 * ONE),
        FixedRect::new(3 * ONE, 3 * ONE, 6 * ONE, 6 * ONE),
    ];
    let edge = |poly: &Poly, fixed_point: FixedPoint, side: usize| {
        Search::edge_holding(
            poly,
            fixed_point,
            ShapeSide::ALL[side],
            &mesh,
            Some(ShapeId::new(0)),
            FixedDistance::ZERO,
        )
    };
    let poly = square(false);
    assert_eq!(
        edge(&poly, FixedPoint::new(2 * ONE, 0), 0),
        Some(VertexIndex::new(0))
    );
    assert_eq!(
        edge(&poly, FixedPoint::new(4 * ONE, 2 * ONE), 1),
        Some(VertexIndex::new(1))
    );
    assert_eq!(
        edge(&poly, FixedPoint::new(2 * ONE, 4 * ONE), 2),
        Some(VertexIndex::new(2))
    );
    assert_eq!(
        edge(&poly, FixedPoint::new(0, 2 * ONE), 3),
        Some(VertexIndex::new(3))
    );
    assert_eq!(edge(&poly, FixedPoint::new(2 * ONE, 0), 1), None);
    assert_eq!(edge(&poly, FixedPoint::new(2 * ONE, 2 * ONE), 0), None);
    let run = square(true);
    assert_eq!(
        edge(&run, FixedPoint::new(3 * ONE, 0), 0),
        Some(VertexIndex::new(0))
    );
    assert_eq!(
        edge(&run, FixedPoint::new(4 * ONE, 2 * ONE), 1),
        Some(VertexIndex::new(2))
    );
    assert_eq!(
        Search::edge_holding(
            &poly,
            FixedPoint::new(2 * ONE, 0),
            ShapeSide::LowY,
            &mesh,
            None,
            FixedDistance::ZERO
        ),
        None
    );
    let empty = Poly {
        vertices: Vec::new(),
        sides: Vec::new(),
        shapes: Vec::new(),
        costs: Vec::new(),
        flag: false,
    };
    assert_eq!(edge(&empty, FixedPoint::new(2 * ONE, 0), 0), None);
    let mut corner = square(false);
    corner.shapes[2] = Some(ShapeId::new(1));
    assert_eq!(
        edge(&corner, FixedPoint::new(4 * ONE, 4 * ONE), 0),
        Some(VertexIndex::new(2))
    );
}

/// `FUN_140fa77b0`'s vertex test: the vertex at the point, of a shape whose overlap with the shape
/// asked (shrunk by `fixed_distance`) holds it; not another vertex, a vertex of no shape, an empty
/// overlap or one shrunk past the point.
#[test]
fn vertex_holds_point_in_its_shape_overlap_with_the_other() {
    let mut mesh = Mesh::new(64);
    mesh.shapes = vec![
        FixedRect::new(0, 0, 4 * ONE, 4 * ONE),
        FixedRect::new(2 * ONE, 2 * ONE, 6 * ONE, 6 * ONE),
        FixedRect::new(5 * ONE, 5 * ONE, 8 * ONE, 8 * ONE),
    ];
    let fixed_point = FixedPoint::new(3 * ONE, 3 * ONE);
    let held = |vc: FixedPoint, vs: Option<u32>, fixed_distance| {
        Search::vertex_in_overlap(
            &mesh,
            (fixed_point, vc, vs.map(ShapeId::new)),
            ShapeId::new(0),
            fixed_distance,
        )
    };
    assert!(held(fixed_point, Some(1), FixedDistance::ZERO));
    assert!(held(fixed_point, Some(1), FixedDistance::new(ONE / 2)));
    assert!(!held(
        FixedPoint::new(3 * ONE, 2 * ONE),
        Some(1),
        FixedDistance::ZERO
    ));
    assert!(!held(fixed_point, None, FixedDistance::ZERO));
    assert!(!held(fixed_point, Some(2), FixedDistance::ZERO));
    assert!(!held(fixed_point, Some(1), FixedDistance::TILE));
}

/// `FUN_140fa77b0`'s edge test on each side: the point on the edge's line between its ends;
/// whether an end counts follows the side before the run and the one after it.
#[test]
fn point_lies_on_an_edge_between_ends_its_neighbour_sides_open_or_close() {
    let (lo, hi, mid) = (0, 4 * ONE, 2 * ONE);
    // (side, from, to, point, side before, side after, on the edge)
    let cases: [(
        usize,
        FixedPoint,
        FixedPoint,
        FixedPoint,
        usize,
        usize,
        bool,
    ); 19] = [
        (
            0,
            FixedPoint::new(lo, lo),
            FixedPoint::new(hi, lo),
            FixedPoint::new(mid, lo),
            3,
            1,
            true,
        ),
        (
            0,
            FixedPoint::new(lo, lo),
            FixedPoint::new(hi, lo),
            FixedPoint::new(mid, ONE),
            3,
            1,
            false,
        ),
        (
            0,
            FixedPoint::new(lo, lo),
            FixedPoint::new(hi, lo),
            FixedPoint::new(lo, lo),
            1,
            1,
            true,
        ),
        (
            0,
            FixedPoint::new(lo, lo),
            FixedPoint::new(hi, lo),
            FixedPoint::new(lo, lo),
            3,
            1,
            false,
        ),
        (
            0,
            FixedPoint::new(lo, lo),
            FixedPoint::new(hi, lo),
            FixedPoint::new(hi, lo),
            3,
            3,
            true,
        ),
        (
            0,
            FixedPoint::new(lo, lo),
            FixedPoint::new(hi, lo),
            FixedPoint::new(hi, lo),
            3,
            1,
            false,
        ),
        (
            1,
            FixedPoint::new(hi, lo),
            FixedPoint::new(hi, hi),
            FixedPoint::new(hi, lo),
            2,
            2,
            true,
        ),
        (
            1,
            FixedPoint::new(hi, lo),
            FixedPoint::new(hi, hi),
            FixedPoint::new(hi, lo),
            0,
            2,
            false,
        ),
        (
            1,
            FixedPoint::new(hi, lo),
            FixedPoint::new(hi, hi),
            FixedPoint::new(hi, hi),
            0,
            1,
            true,
        ),
        (
            1,
            FixedPoint::new(hi, lo),
            FixedPoint::new(hi, hi),
            FixedPoint::new(hi, hi),
            0,
            2,
            false,
        ),
        (
            1,
            FixedPoint::new(hi, lo),
            FixedPoint::new(hi, hi),
            FixedPoint::new(mid, mid),
            0,
            2,
            false,
        ),
        (
            2,
            FixedPoint::new(hi, hi),
            FixedPoint::new(lo, hi),
            FixedPoint::new(lo, hi),
            1,
            1,
            true,
        ),
        (
            2,
            FixedPoint::new(hi, hi),
            FixedPoint::new(lo, hi),
            FixedPoint::new(lo, hi),
            1,
            3,
            false,
        ),
        (
            2,
            FixedPoint::new(hi, hi),
            FixedPoint::new(lo, hi),
            FixedPoint::new(hi, hi),
            3,
            3,
            true,
        ),
        (
            2,
            FixedPoint::new(hi, hi),
            FixedPoint::new(lo, hi),
            FixedPoint::new(hi, hi),
            1,
            3,
            false,
        ),
        (
            3,
            FixedPoint::new(lo, hi),
            FixedPoint::new(lo, lo),
            FixedPoint::new(lo, lo),
            0,
            2,
            true,
        ),
        (
            3,
            FixedPoint::new(lo, hi),
            FixedPoint::new(lo, lo),
            FixedPoint::new(lo, lo),
            0,
            0,
            false,
        ),
        (
            3,
            FixedPoint::new(lo, hi),
            FixedPoint::new(lo, lo),
            FixedPoint::new(lo, hi),
            0,
            0,
            true,
        ),
        (
            3,
            FixedPoint::new(lo, hi),
            FixedPoint::new(lo, lo),
            FixedPoint::new(lo, hi),
            2,
            0,
            false,
        ),
    ];
    let sides = ShapeSide::ALL;
    for (side, from, to, fixed_point, prev, next, on) in cases {
        assert_eq!(
            Search::on_edge(
                sides[side],
                (from, to),
                fixed_point,
                (sides[prev], sides[next])
            ),
            on,
            "side {side} at {fixed_point:?}, sides {prev} and {next}"
        );
    }
}

/// The escape candidates a search holds open: each one's place, cost and shape side.
fn opened(search: &Search<'_>) -> Vec<(FixedPoint, i32, Kind)> {
    search
        .open
        .iter()
        .map(|node| (node.at, node.cost.get(), node.kind))
        .collect()
}

/// Shape 0 over tiles (2, 2) to (6, 6), and a search of doubled radius two tiles (`radius` one
/// tile, `rr2` four) toward (12, 12).
fn shape_at_2_2() -> Mesh {
    let mut mesh = Mesh::new(16);
    let _ = mesh.add(FixedRect::new(2 * ONE, 2 * ONE, 6 * ONE, 6 * ONE));
    mesh
}

/// `FUN_140fa4410`: a start within range answers 3 with only its node made; a free start
/// opens its node and answers 0; a held start with escape candidates keeps open only the one
/// of the lowest estimate, its estimate taken again (1), and with fewer than two answers 2.
#[test]
fn start_in_range_free_or_held_answers_by_its_escape_candidates() {
    let mesh = shape_at_2_2();
    let mut search = search_toward(&mesh, FixedPoint::new(12 * ONE, 12 * ONE), 2 * ONE);
    assert_eq!(
        search.start(FixedPoint::new(11 * ONE, 12 * ONE)).unwrap(),
        Started::InRange
    );
    assert_eq!((search.arena.len(), search.open.len()), (1, 0));
    assert_eq!(
        search.start(FixedPoint::new(10 * ONE, 10 * ONE)).unwrap(),
        Started::Free
    );
    assert_eq!((search.arena.len(), search.open.len()), (2, 1));
    assert_eq!(
        (search.open[0].at, search.open[0].kind),
        (FixedPoint::new(10 * ONE, 10 * ONE), Kind::Start)
    );
    let mut search = search_toward(&mesh, FixedPoint::new(12 * ONE, 12 * ONE), 0);
    search.goal.radius = FixedDistance::TILE;
    assert_eq!(
        search.start(FixedPoint::new(4 * ONE, 3 * ONE)).unwrap(),
        Started::Escaped
    );
    assert_eq!(search.open.len(), 1);
    let node = search.open[0];
    // the touching point on side 1 has the lowest estimate toward (12, 12)
    assert_eq!(
        (node.at, node.cost),
        (
            FixedPoint::new(6 * ONE, 3 * ONE),
            FixedDistance::new(2 * ONE)
        )
    );
    assert_eq!(
        node.total,
        search.goal.heuristic(node.at).wrapping_add(node.cost)
    );
    assert_eq!(search.node(node.idx).unwrap().total, node.total);
    let mut search = search_toward(&mesh, FixedPoint::new(12 * ONE, 12 * ONE), 0);
    search.goal.radius = FixedDistance::new(ONE / 8);
    assert_eq!(
        search.start(FixedPoint::new(4 * ONE, 4 * ONE)).unwrap(),
        Started::Stuck
    );
}

/// `FUN_140fa4410`'s scan: every corner tile of the box twice the radius out is read once;
/// each holding shape 0 gives its four sides' candidates again, as the game scans a tile's
/// shapes without a seen list; an ignored shape gives none.
#[test]
fn the_start_scan_reads_each_corner_tile_once_and_skips_ignored_shapes() {
    let mesh = shape_at_2_2();
    let fixed_point = FixedPoint::new(4 * ONE, 3 * ONE);
    let mut search = search_toward(&mesh, FixedPoint::new(12 * ONE, 12 * ONE), 0);
    search.goal.radius = FixedDistance::TILE;
    search
        .escape_candidates(fixed_point, NodeIndex::START)
        .unwrap();
    let mut one = search_toward(&mesh, FixedPoint::new(12 * ONE, 12 * ONE), 0);
    one.goal.radius = FixedDistance::TILE;
    for index in 0..4 {
        one.escape_side(
            fixed_point,
            NodeIndex::START,
            (ShapeId::new(0), ShapeSide::ALL[index]),
            FixedDistanceSquared::of(FixedDistance::new(2 * ONE)),
        )
        .unwrap();
    }
    let tiles = [(2, 1), (6, 1), (6, 5), (2, 5)];
    let holding = tiles
        .iter()
        .filter(|&&(x, y)| mesh.tiles[y * 16 + x].contains(&ShapeId::new(0)))
        .count();
    assert!(0 < holding);
    assert_eq!(search.open.len(), holding * one.open.len());
    let ignore = [true];
    let mut search = search_toward(&mesh, FixedPoint::new(12 * ONE, 12 * ONE), 0);
    search.ctx.ignore = &ignore;
    search.goal.radius = FixedDistance::TILE;
    search
        .escape_candidates(fixed_point, NodeIndex::START)
        .unwrap();
    assert_eq!(search.open.len(), 0);
}

/// One side of shape 0 seen from (4, 3) with a circle of two tiles: side 0, one tile away,
/// gives the two points where the circle crosses it and the start's foot; side 1, two tiles
/// away, the touching point twice and the foot there; side 2, three tiles away, nothing. A
/// corner within the circle is a candidate; a crossing another shape holds is not.
#[test]
fn shape_side_gives_the_circle_crossings_the_foot_and_near_corner() {
    let mesh = shape_at_2_2();
    let side = |shape, side: usize| Kind::Side {
        shape: ShapeId::new(shape),
        side: ShapeSide::ALL[side],
    };
    let rr2 = FixedDistanceSquared::of(FixedDistance::new(2 * ONE));
    let mut search = search_toward(&mesh, FixedPoint::new(12 * ONE, 12 * ONE), 0);
    search
        .escape_side(
            FixedPoint::new(4 * ONE, 3 * ONE),
            NodeIndex::START,
            (ShapeId::new(0), ShapeSide::ALL[0]),
            rr2,
        )
        .unwrap();
    let mut got = opened(&search);
    got.sort_by_key(|&(at, _, _)| at.get());
    assert_eq!(got.len(), 3);
    assert_eq!(
        (got[1].0, got[1].1, got[1].2),
        (FixedPoint::new(4 * ONE, 2 * ONE), ONE, side(0, 0))
    );
    for &(at, cost, kind) in [&got[0], &got[2]] {
        assert_eq!((at.y(), kind), (2 * ONE, side(0, 0)));
        // √3 tiles either side of the foot, two tiles from the start
        assert!(((at.x() - 4 * ONE).abs() - 7_264_748).abs() <= 2, "{at:?}");
        assert!((cost - 2 * ONE).abs() <= 2, "{cost}");
    }
    let mut search = search_toward(&mesh, FixedPoint::new(12 * ONE, 12 * ONE), 0);
    search
        .escape_side(
            FixedPoint::new(4 * ONE, 3 * ONE),
            NodeIndex::START,
            (ShapeId::new(0), ShapeSide::ALL[1]),
            rr2,
        )
        .unwrap();
    assert_eq!(
        opened(&search),
        vec![(FixedPoint::new(6 * ONE, 3 * ONE), 2 * ONE, side(0, 1)); 3]
    );
    let mut search = search_toward(&mesh, FixedPoint::new(12 * ONE, 12 * ONE), 0);
    search
        .escape_side(
            FixedPoint::new(4 * ONE, 3 * ONE),
            NodeIndex::START,
            (ShapeId::new(0), ShapeSide::ALL[2]),
            rr2,
        )
        .unwrap();
    assert_eq!(opened(&search), vec![]);
    // from (3, 3) the corner (2, 2) lies √2 away
    let mut search = search_toward(&mesh, FixedPoint::new(12 * ONE, 12 * ONE), 0);
    search
        .escape_side(
            FixedPoint::new(3 * ONE, 3 * ONE),
            NodeIndex::START,
            (ShapeId::new(0), ShapeSide::ALL[0]),
            rr2,
        )
        .unwrap();
    assert!(
        opened(&search)
            .iter()
            .any(|&(at, cost, _)| at == FixedPoint::new(2 * ONE, 2 * ONE)
                && (cost - 5_931_642).abs() <= 2)
    );
    // shape 1 holds the crossing right of the foot; its contact at (5, 2) is a candidate
    let mut mesh = shape_at_2_2();
    let _ = mesh.add(FixedRect::new(5 * ONE, 0, 9 * ONE, 3 * ONE));
    let mut search = search_toward(&mesh, FixedPoint::new(12 * ONE, 12 * ONE), 0);
    search
        .escape_side(
            FixedPoint::new(4 * ONE, 3 * ONE),
            NodeIndex::START,
            (ShapeId::new(0), ShapeSide::ALL[0]),
            rr2,
        )
        .unwrap();
    let got = opened(&search);
    assert_eq!(got.len(), 3);
    assert!(got.iter().all(|&(at, _, _)| at.x() <= 5 * ONE));
    assert!(
        got.iter()
            .any(|&(at, _, _)| at == FixedPoint::new(5 * ONE, 2 * ONE))
    );
}

/// The start's foot on a side: strictly between the side's corners, in the search's box and
/// free, at the perpendicular distance; not past either corner, outside the box or held.
#[test]
fn the_start_foot_on_side_lies_between_its_corners_in_the_box_and_free() {
    let mesh = shape_at_2_2();
    let corners = |side: usize| {
        let fixed_rect = mesh.shapes[0];
        (
            shrunk_corner(fixed_rect, [0, 1, 3, 0][side], FixedDistance::ZERO),
            shrunk_corner(fixed_rect, [1, 2, 2, 3][side], FixedDistance::ZERO),
        )
    };
    let side = |side: usize| Kind::Side {
        shape: ShapeId::new(0),
        side: ShapeSide::ALL[side],
    };
    let mut search = search_toward(&mesh, FixedPoint::new(12 * ONE, 12 * ONE), 0);
    let fixed_point = FixedPoint::new(3 * ONE, 5 * ONE / 2);
    for index in 0..4 {
        search
            .escape_foot(
                fixed_point,
                NodeIndex::START,
                (ShapeId::new(0), ShapeSide::ALL[index]),
                corners(index),
            )
            .unwrap();
    }
    let mut got = opened(&search);
    got.sort_by_key(|&(_, cost, _)| cost);
    assert_eq!(
        got,
        vec![
            (FixedPoint::new(3 * ONE, 2 * ONE), ONE / 2, side(0)),
            (FixedPoint::new(2 * ONE, 5 * ONE / 2), ONE, side(3)),
            (FixedPoint::new(6 * ONE, 5 * ONE / 2), 3 * ONE, side(1)),
            (FixedPoint::new(3 * ONE, 6 * ONE), 7 * ONE / 2, side(2)),
        ]
    );
    let mut search = search_toward(&mesh, FixedPoint::new(12 * ONE, 12 * ONE), 0);
    search
        .escape_foot(
            FixedPoint::new(2 * ONE, 5 * ONE / 2),
            NodeIndex::START,
            (ShapeId::new(0), ShapeSide::ALL[0]),
            corners(0),
        )
        .unwrap();
    search
        .escape_foot(
            FixedPoint::new(3 * ONE, 6 * ONE),
            NodeIndex::START,
            (ShapeId::new(0), ShapeSide::ALL[1]),
            corners(1),
        )
        .unwrap();
    assert_eq!(opened(&search), vec![]);
    search.bounds = FixedRect::new(0, 0, 64 * ONE, ONE);
    search
        .escape_foot(
            fixed_point,
            NodeIndex::START,
            (ShapeId::new(0), ShapeSide::ALL[0]),
            corners(0),
        )
        .unwrap();
    let mut held = shape_at_2_2();
    let _ = held.add(FixedRect::new(0, 0, 4 * ONE, 4 * ONE));
    let mut held_search = search_toward(&held, FixedPoint::new(12 * ONE, 12 * ONE), 0);
    held_search
        .escape_foot(
            fixed_point,
            NodeIndex::START,
            (ShapeId::new(0), ShapeSide::ALL[0]),
            corners(0),
        )
        .unwrap();
    assert_eq!((opened(&search), opened(&held_search)), (vec![], vec![]));
}

/// A side's contacts: one wider than the search's shrink, with a shape not ignored, at a free
/// point in the box within the circle is a candidate; one on a side of its own number counts
/// only when its other shape holds the point nudged into it, and costs 0x32 more.
#[test]
fn side_contacts_become_candidates_when_wide_free_and_near() {
    use crate::pathing::mesh::Contact;
    let mut mesh = shape_at_2_2();
    let _ = mesh.add(FixedRect::new(5 * ONE, 0, 9 * ONE, 3 * ONE));
    let _ = mesh.add(FixedRect::new(4 * ONE, 2 * ONE, 7 * ONE, 4 * ONE));
    let contact = |other, side: usize, at: FixedPoint, half| Contact {
        other: ShapeId::new(other),
        side: ShapeSide::ALL[side],
        at,
        along: at.x(),
        at_side_start: false,
        half: FixedDistance::new(half),
    };
    let near = FixedPoint::new(19 * ONE / 4, 2 * ONE);
    mesh.sides[0][0] = vec![
        contact(1, 3, near, ONE),
        contact(1, 3, near, 0),
        contact(3, 3, near, ONE),
        contact(1, 3, FixedPoint::new(-ONE, 2 * ONE), ONE),
        contact(2, 0, near, ONE),
        contact(1, 0, near, ONE),
        contact(1, 3, FixedPoint::new(6 * ONE, ONE), ONE),
        contact(1, 3, FixedPoint::new(ONE / 2, 2 * ONE), ONE),
    ];
    let ignore = [false, false, false, true];
    let mut search = search_toward(&mesh, FixedPoint::new(12 * ONE, 12 * ONE), 0);
    search.ctx.ignore = &ignore;
    search
        .escape_contacts(
            FixedPoint::new(4 * ONE, 3 * ONE),
            NodeIndex::START,
            (ShapeId::new(0), ShapeSide::ALL[0]),
            FixedDistanceSquared::of(FixedDistance::new(2 * ONE)),
        )
        .unwrap();
    let side = Kind::Side {
        shape: ShapeId::new(0),
        side: ShapeSide::LowY,
    };
    // 3/4 and 1 tile off: 5/4 tiles
    let mut got = opened(&search);
    got.sort_by_key(|&(_, cost, _)| cost);
    assert_eq!(
        got,
        vec![(near, 5 * ONE / 4, side), (near, 5 * ONE / 4 + 0x32, side)]
    );
}
