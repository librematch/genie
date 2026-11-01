use super::*;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::TerrainMask;
use crate::test_ids::object_id;

/// A search on an empty mesh toward a point target at `at` within `range` (fixed point), its
/// start node at the origin with cost 0.
fn search_toward(mesh: &Mesh, at: FixedPoint, range: i32) -> Search<'_> {
    let goal = Goal {
        at,
        r: FixedDistance::ZERO,
        rx: 0,
        ry: 0,
        rrx: 0,
        rry: 0,
        range: FixedDistance::new(range),
        range2: (i64::from(range) * i64::from(range)) >> 22,
    };
    let mut s = Search {
        ctx: Ctx {
            mesh,
            ignore: &[],
            d: FixedDistance::ZERO,
        },
        goal,
        bounds: FixedRect::new(0, 0, 64 * ONE, 64 * ONE),
        open: Vec::new(),
        arena: Vec::new(),
        polys: Vec::new(),
        goal_node: NONE,
        best: 0,
    };
    let _ = s.add(0, NONE, FixedPoint::new(0, 0), Kind::Start);
    s
}

/// `FUN_140fa3860`: a node within range becomes the goal while it is cheaper than the goal so
/// far; any other node becomes the best when it lies nearer than the best's estimate, and a
/// node no nearer adds nothing.
#[test]
fn a_goal_candidate_in_range_and_cheaper_becomes_the_goal_else_maybe_the_best() {
    let mesh = Mesh::new(64);
    let mut s = search_toward(&mesh, FixedPoint::new(10 * ONE, 10 * ONE), 2 * ONE);
    assert!(
        s.goal_test(Corner {
            cost: 5 * ONE,
            parent: 0,
            dir: WalkDir::Forward,
            low: 0,
            high: 0,
            poly: 0,
            point: FixedPoint::new(11 * ONE, 10 * ONE)
        })
        .unwrap()
    );
    assert_eq!(s.goal_node, 1);
    // in range but dearer: the best instead, as it lies nearer than the start's estimate
    assert!(
        !s.goal_test(Corner {
            cost: 6 * ONE,
            parent: 0,
            dir: WalkDir::Forward,
            low: 0,
            high: 0,
            poly: 0,
            point: FixedPoint::new(11 * ONE, 10 * ONE)
        })
        .unwrap()
    );
    assert_eq!((s.goal_node, s.best), (1, 2));
    // out of range and farther than the best's estimate (one tile): nothing
    assert!(
        !s.goal_test(Corner {
            cost: ONE,
            parent: 0,
            dir: WalkDir::Forward,
            low: 0,
            high: 0,
            poly: 0,
            point: FixedPoint::new(30 * ONE, 30 * ONE)
        })
        .unwrap()
    );
    assert_eq!((s.arena.len(), s.best), (3, 2));
}

/// `FUN_140fa8210`: the goal's foot on an axis edge that spans it, reached at the edge's
/// start cost plus the walk along the edge, is a goal candidate; an edge that does not span
/// the goal gives none.
#[test]
fn the_goal_s_foot_on_a_spanning_edge_is_a_goal_candidate() {
    let mesh = Mesh::new(64);
    let mut s = search_toward(&mesh, FixedPoint::new(10 * ONE, 10 * ONE), 2 * ONE);
    s.foot_test(Foot {
        poly: 0,
        start: FixedPoint::new(9 * ONE, 12 * ONE),
        end: FixedPoint::new(9 * ONE, 20 * ONE),
        x_extent: 0,
        low: 0,
        high: 0,
        dir: WalkDir::Forward,
        start_cost: ONE,
        parent: 0,
    })
    .unwrap();
    assert_eq!(s.arena.len(), 1);
    s.foot_test(Foot {
        poly: 0,
        start: FixedPoint::new(15 * ONE, 9 * ONE),
        end: FixedPoint::new(5 * ONE, 9 * ONE),
        x_extent: 1,
        low: 0,
        high: 0,
        dir: WalkDir::Forward,
        start_cost: ONE,
        parent: 0,
    })
    .unwrap();
    let goal = s.arena[s.goal_node as usize];
    assert_eq!(
        (goal.at, goal.g),
        (FixedPoint::new(10 * ONE, 9 * ONE), 6 * ONE)
    );
}

/// `FUN_140fa8350`: a junction moved `d` into the free space beside each side, the flag
/// turning it along the side.
#[test]
fn a_contact_point_moves_off_its_side() {
    let p = FixedPoint::new(100, 200);
    let moved: Vec<FixedPoint> = ShapeSide::ALL
        .into_iter()
        .flat_map(|side| [0, 1].map(|flag| contact_point(p, side, FixedDistance::new(3), flag)))
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

/// `FUN_140fa39f0`: a point goes onto a side of the rectangle shrunk by `d` (the side's
/// coordinate set, the other held between the shrunk corners).
#[test]
fn a_point_snaps_onto_a_side_within_its_corners() {
    let r = FixedRect::new(0, 0, 100, 50);
    let snap = |p: FixedPoint, side: usize| {
        let mut q = p;
        snap_to_side(&mut q, r, ShapeSide::ALL[side], FixedDistance::new(5));
        q
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
fn a_ray_crosses_the_side_it_faces_and_reports_how_far() {
    // a ray from (0, 1) to (4, 1) against the box (2, 0)-(3, 2): its left side at x = 2
    let r = FixedRect::new(2 * ONE, 0, 3 * ONE, 2 * ONE);
    let mut t = T_START;
    let hit = ray_rect(
        FixedPoint::new(0, ONE),
        FixedPoint::new(4 * ONE, ONE),
        r,
        &mut t,
    );
    assert_eq!((hit, t), (Some((1, ShapeSide::LowX)), ONE / 2));
    // a ray going the other way meets the right side
    let mut t = T_START;
    let hit = ray_rect(
        FixedPoint::new(4 * ONE, ONE),
        FixedPoint::new(0, ONE),
        r,
        &mut t,
    );
    assert_eq!((hit, t), (Some((1, ShapeSide::HighX)), ONE / 4));
    // a ray beside the box meets nothing
    let mut t = T_START;
    assert_eq!(
        ray_rect(
            FixedPoint::new(0, 3 * ONE),
            FixedPoint::new(4 * ONE, 3 * ONE),
            r,
            &mut t
        ),
        None
    );
}

/// `FUN_140faa4e0` on the y sides: a ray along y within the box's open x span meets the
/// bottom going up and the top going down, nearer than `t` and from at or outside the side;
/// a slanted ray meets them as `FUN_140fa9f10` does.
#[test]
fn a_ray_along_y_crosses_the_bottom_or_the_top() {
    let r = FixedRect::new(2 * ONE, 0, 3 * ONE, 2 * ONE);
    let x = 2 * ONE + ONE / 2;
    let cast = |a: FixedPoint, b: FixedPoint, t: i32| {
        let mut t = t;
        let hit = ray_rect(a, b, r, &mut t);
        (hit, t)
    };
    // up from y = -1 to 3: the bottom at a quarter of the way
    assert_eq!(
        cast(
            FixedPoint::new(x, -ONE),
            FixedPoint::new(x, 3 * ONE),
            T_START
        ),
        (Some((1, ShapeSide::LowY)), ONE / 4)
    );
    // down from 3 to -1: the top at a quarter
    assert_eq!(
        cast(
            FixedPoint::new(x, 3 * ONE),
            FixedPoint::new(x, -ONE),
            T_START
        ),
        (Some((1, ShapeSide::HighY)), ONE / 4)
    );
    // a nearer hit already: none
    assert_eq!(
        cast(
            FixedPoint::new(x, -ONE),
            FixedPoint::new(x, 3 * ONE),
            ONE / 8
        ),
        (None, ONE / 8)
    );
    // from inside the box, or beside it (x on its edge): none
    assert_eq!(
        cast(
            FixedPoint::new(x, ONE),
            FixedPoint::new(x, 3 * ONE),
            T_START
        ),
        (None, T_START)
    );
    assert_eq!(
        cast(
            FixedPoint::new(3 * ONE, -ONE),
            FixedPoint::new(3 * ONE, 3 * ONE),
            T_START
        ),
        (None, T_START)
    );
    assert_eq!(
        cast(FixedPoint::new(x, ONE), FixedPoint::new(x, -ONE), T_START),
        (None, T_START)
    );
    // a slanted ray up through the bottom: the segment test against it
    let (a, b) = (
        FixedPoint::new(2 * ONE + ONE / 4, -ONE),
        FixedPoint::new(2 * ONE + 3 * ONE / 4, 3 * ONE),
    );
    let mut t = T_START;
    let k = segment_hit(
        a,
        b,
        FixedPoint::new(2 * ONE, 0),
        FixedPoint::new(3 * ONE, 0),
        &mut t,
    );
    assert_ne!(k, 0);
    assert_eq!(cast(a, b, T_START), (Some((k, ShapeSide::LowY)), t));
    // and down through the top
    let mut t = T_START;
    let k = segment_hit(
        b,
        a,
        FixedPoint::new(2 * ONE, 2 * ONE),
        FixedPoint::new(3 * ONE, 2 * ONE),
        &mut t,
    );
    assert_ne!(k, 0);
    assert_eq!(cast(b, a, T_START), (Some((k, ShapeSide::HighY)), t));
}

/// `FUN_140fa16d0`: a segment through the box keeps the part inside, each end moved to the
/// edge it crossed (`(dy * (edge - x) >> 22 << 22) / dx`, truncated); a segment beside the
/// box is none, one inside it stays; the box's corners may come in any order.
#[test]
fn a_segment_clipped_to_the_box_keeps_its_part_inside() {
    let b = FixedRect::new(0, 0, 2 * ONE, 2 * ONE);
    assert_eq!(
        clip(FixedPoint::new(-ONE, ONE), FixedPoint::new(3 * ONE, ONE), b),
        Some((FixedPoint::new(0, ONE), FixedPoint::new(2 * ONE, ONE)))
    );
    // first the left edge, then the bottom, then the right edge for the far end
    assert_eq!(
        clip(
            FixedPoint::new(-ONE, -ONE),
            FixedPoint::new(3 * ONE, 2 * ONE),
            b
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
            b
        ),
        None
    );
    let inside = (FixedPoint::new(ONE / 2, ONE), FixedPoint::new(ONE, ONE / 2));
    assert_eq!(clip(inside.0, inside.1, b), Some(inside));
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
fn a_start_deep_in_blocked_terrain_is_blocked_unless_in_range() {
    let blocked = |_: u32, _: u32| 1;
    let free = |_: u32, _: u32| 0;
    let mut q = Query {
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
    assert!(pather.start_blocked(&mut mesh, &blocked, &q).unwrap());
    let n = mesh.shapes.len() as u32;
    assert!(0 < n && (0..n).all(|id| !mesh.is_shape(ShapeId::new(id))));
    assert!(!pather.start_blocked(&mut mesh, &free, &q).unwrap());
    q.range = Distance::new(3.0);
    assert!(!pather.start_blocked(&mut mesh, &blocked, &q).unwrap());
}

/// `FUN_140f9e540`'s entry tests as compiled: the target's gap clamps each axis by `MAXSS`
/// (`FUN_14118faf0`: an unordered one is 0) and is in range by `COMISS`/`JC` (0x140f9e5f5);
/// each end lies off the map by `COMISS`/`JA` below 0 and `JNC` at the size (0x140f9e67c..
/// 0x140f9e6da): an unordered coordinate is on it.
#[test]
fn the_entry_s_float_tests_take_the_game_s_nan_paths() {
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
    let t = Target {
        id: object_id(9),
        position: Point::new(5.0, 5.0),
        half_size: Footprint::new(0.5, 0.5),
    };
    assert_eq!(
        call(Point::new(f32::NAN, 5.0), Some(t)).entry(),
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
fn around(walls: &[[f32; 4]], from: Point, to: Point, ignored: bool) -> (u8, Vec<Point>) {
    let fixed = |v: f32| (v * ONE as f32) as i32;
    let mut mesh = Mesh::new(32);
    let ids: Vec<ShapeId> = walls
        .iter()
        .map(|w| {
            mesh.add(FixedRect::new(
                fixed(w[0]),
                fixed(w[1]),
                fixed(w[2]),
                fixed(w[3]),
            ))
        })
        .collect();
    let q = Query {
        from: FixedPoint::new(fixed(from.x()), fixed(from.y())),
        to: FixedPoint::new(fixed(to.x()), fixed(to.y())),
        radius: Distance::new(0.25),
        target_size: Footprint::new(0.0, 0.0),
        range: Distance::ZERO,
        blocked: 0,
    };
    let ignore = if ignored { ids } else { Vec::new() };
    let found = Pather::default()
        .search(&mut mesh, &|_, _| 0, &q, &ignore)
        .unwrap();
    let tiles = |v: i32| v as f32 / ONE as f32;
    let points = found
        .points
        .iter()
        .map(|p| Point::new(tiles(p.x()), tiles(p.y())))
        .collect();
    (found.result, points)
}

/// A shape keeps the unit's centre `d` (half a tile less the radius) inside its rectangle: the
/// search walks round a wall (10.25..10.75 by 4.25..11.75 for the centre) at its nearer end
/// (3.25 tiles down against 4.25 up), over its two corners; ignored, the wall leaves the path
/// direct (the start, the start node's own point, the goal).
#[test]
fn a_path_bends_round_a_wall_at_its_nearer_end() {
    let wall = [[10.0, 4.0, 11.0, 12.0]];
    assert_eq!(
        around(&wall, Point::new(6.5, 8.5), Point::new(14.5, 8.5), false),
        (
            1,
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
            1,
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
fn a_path_leaves_a_u_by_its_open_side() {
    let u = [
        [8.0, 4.0, 9.0, 12.0],
        [4.0, 4.0, 9.0, 5.0],
        [4.0, 11.0, 9.0, 12.0],
    ];
    assert_eq!(
        around(&u, Point::new(6.5, 8.5), Point::new(14.5, 8.5), false),
        (
            1,
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
fn a_goal_shut_in_a_box_gives_a_partial_path_to_its_nearest_point() {
    let walls = [
        [10.0, 6.0, 15.0, 7.0],
        [10.0, 10.0, 15.0, 11.0],
        [10.0, 6.0, 11.0, 11.0],
        [14.0, 6.0, 15.0, 11.0],
    ];
    assert_eq!(
        around(&walls, Point::new(6.5, 8.5), Point::new(12.5, 8.5), false),
        (2, vec![Point::new(6.5, 8.5), Point::new(10.25, 8.5)])
    );
    assert_eq!(
        around(&walls, Point::new(6.5, 8.5), Point::new(12.5, 8.5), true).0,
        1
    );
}

/// Two staggered walls: the path reaches the goal over one corner of the second wall (a
/// shortest path bends only at corners); the first wall's lower end lies off it.
#[test]
fn a_path_past_staggered_walls_bends_only_at_their_corners() {
    let walls = [[10.0, 4.0, 11.0, 9.0], [12.0, 8.0, 13.0, 14.0]];
    let (result, points) = around(&walls, Point::new(6.5, 6.5), Point::new(16.5, 12.5), false);
    assert_eq!(result, 1);
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
    assert!(between.iter().all(|p| corners.contains(p)), "{points:?}");
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
    assert_eq!(found.map(|f| f.result), Ok(0));
    let found = pather.search(&mut mesh, &words, &range_query(Distance::new(below)), &[]);
    assert_eq!(found.map(|f| f.result), Ok(1));
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
    let n = vertices.len();
    Poly {
        vertices,
        sides,
        shapes: vec![None; n],
        costs: vec![0; n],
        flag: false,
    }
}

/// `FUN_140fa77b0`: the outline's edge whose side is `side` and that holds the point, the
/// first vertex of its run (a run's edges are one); none for a point on no such edge, an
/// outline of no vertices or no shape; a vertex of a shape whose overlap with the shape asked
/// holds the point answers whatever the side.
#[test]
fn the_edge_holding_a_point_is_the_first_vertex_of_its_side_s_run() {
    let mut mesh = Mesh::new(64);
    mesh.shapes = vec![
        FixedRect::new(0, 0, 4 * ONE, 4 * ONE),
        FixedRect::new(3 * ONE, 3 * ONE, 6 * ONE, 6 * ONE),
    ];
    let edge = |poly: &Poly, p: FixedPoint, side: usize| {
        Search::edge_holding(
            poly,
            p,
            ShapeSide::ALL[side],
            &mesh,
            Some(ShapeId::new(0)),
            FixedDistance::ZERO,
        )
    };
    let poly = square(false);
    assert_eq!(edge(&poly, FixedPoint::new(2 * ONE, 0), 0), Some(0));
    assert_eq!(edge(&poly, FixedPoint::new(4 * ONE, 2 * ONE), 1), Some(1));
    assert_eq!(edge(&poly, FixedPoint::new(2 * ONE, 4 * ONE), 2), Some(2));
    assert_eq!(edge(&poly, FixedPoint::new(0, 2 * ONE), 3), Some(3));
    assert_eq!(edge(&poly, FixedPoint::new(2 * ONE, 0), 1), None);
    assert_eq!(edge(&poly, FixedPoint::new(2 * ONE, 2 * ONE), 0), None);
    let run = square(true);
    assert_eq!(edge(&run, FixedPoint::new(3 * ONE, 0), 0), Some(0));
    assert_eq!(edge(&run, FixedPoint::new(4 * ONE, 2 * ONE), 1), Some(2));
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
    assert_eq!(edge(&corner, FixedPoint::new(4 * ONE, 4 * ONE), 0), Some(2));
}

/// `FUN_140fa77b0`'s vertex test: the vertex at the point, of a shape whose overlap with the
/// shape asked (shrunk by `d`) holds it; not another vertex, a vertex of no shape, an empty
/// overlap or one shrunk past the point.
#[test]
fn a_vertex_holds_a_point_in_its_shape_s_overlap_with_the_other() {
    let mut mesh = Mesh::new(64);
    mesh.shapes = vec![
        FixedRect::new(0, 0, 4 * ONE, 4 * ONE),
        FixedRect::new(2 * ONE, 2 * ONE, 6 * ONE, 6 * ONE),
        FixedRect::new(5 * ONE, 5 * ONE, 8 * ONE, 8 * ONE),
    ];
    let p = FixedPoint::new(3 * ONE, 3 * ONE);
    let held = |vc: FixedPoint, vs: Option<u32>, d| {
        Search::vertex_in_overlap(&mesh, (p, vc, vs.map(ShapeId::new)), ShapeId::new(0), d)
    };
    assert!(held(p, Some(1), FixedDistance::ZERO));
    assert!(held(p, Some(1), FixedDistance::new(ONE / 2)));
    assert!(!held(
        FixedPoint::new(3 * ONE, 2 * ONE),
        Some(1),
        FixedDistance::ZERO
    ));
    assert!(!held(p, None, FixedDistance::ZERO));
    assert!(!held(p, Some(2), FixedDistance::ZERO));
    assert!(!held(p, Some(1), FixedDistance::TILE));
}

/// `FUN_140fa77b0`'s edge test on each side: the point on the edge's line between its ends;
/// whether an end counts follows the side before the run and the one after it.
#[test]
fn a_point_lies_on_an_edge_between_ends_its_neighbour_sides_open_or_close() {
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
    for (side, from, to, p, prev, next, on) in cases {
        assert_eq!(
            Search::on_edge(sides[side], (from, to), p, (sides[prev], sides[next])),
            on,
            "side {side} at {p:?}, sides {prev} and {next}"
        );
    }
}

/// The escape candidates a search holds open: each one's place, cost and shape side.
fn opened(s: &Search<'_>) -> Vec<(FixedPoint, i32, Kind)> {
    s.open.iter().map(|n| (n.at, n.g, n.kind)).collect()
}

/// Shape 0 over tiles (2, 2) to (6, 6), and a search of doubled radius two tiles (`r` one
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
fn a_start_in_range_free_or_held_answers_by_its_escape_candidates() {
    let mesh = shape_at_2_2();
    let mut s = search_toward(&mesh, FixedPoint::new(12 * ONE, 12 * ONE), 2 * ONE);
    assert_eq!(s.start(FixedPoint::new(11 * ONE, 12 * ONE)).unwrap(), 3);
    assert_eq!((s.arena.len(), s.open.len()), (1, 0));
    assert_eq!(s.start(FixedPoint::new(10 * ONE, 10 * ONE)).unwrap(), 0);
    assert_eq!((s.arena.len(), s.open.len()), (2, 1));
    assert_eq!(
        (s.open[0].at, s.open[0].kind),
        (FixedPoint::new(10 * ONE, 10 * ONE), Kind::Start)
    );
    let mut s = search_toward(&mesh, FixedPoint::new(12 * ONE, 12 * ONE), 0);
    s.goal.r = FixedDistance::TILE;
    assert_eq!(s.start(FixedPoint::new(4 * ONE, 3 * ONE)).unwrap(), 1);
    assert_eq!(s.open.len(), 1);
    let n = s.open[0];
    // the touching point on side 1 has the lowest estimate toward (12, 12)
    assert_eq!((n.at, n.g), (FixedPoint::new(6 * ONE, 3 * ONE), 2 * ONE));
    assert_eq!(n.f, s.goal.heuristic(n.at) + n.g);
    assert_eq!(s.arena[n.idx as usize].f, n.f);
    let mut s = search_toward(&mesh, FixedPoint::new(12 * ONE, 12 * ONE), 0);
    s.goal.r = FixedDistance::new(ONE / 8);
    assert_eq!(s.start(FixedPoint::new(4 * ONE, 4 * ONE)).unwrap(), 2);
}

/// `FUN_140fa4410`'s scan: every corner tile of the box twice the radius out is read once;
/// each holding shape 0 gives its four sides' candidates again, as the game scans a tile's
/// shapes without a seen list; an ignored shape gives none.
#[test]
fn the_start_s_scan_reads_each_corner_tile_once_and_skips_ignored_shapes() {
    let mesh = shape_at_2_2();
    let p = FixedPoint::new(4 * ONE, 3 * ONE);
    let mut s = search_toward(&mesh, FixedPoint::new(12 * ONE, 12 * ONE), 0);
    s.goal.r = FixedDistance::TILE;
    s.escape_candidates(p, 0).unwrap();
    let mut one = search_toward(&mesh, FixedPoint::new(12 * ONE, 12 * ONE), 0);
    one.goal.r = FixedDistance::TILE;
    for k in 0..4 {
        one.escape_side(
            p,
            0,
            (ShapeId::new(0), ShapeSide::ALL[k]),
            4 * i64::from(ONE),
        )
        .unwrap();
    }
    let tiles = [(2, 1), (6, 1), (6, 5), (2, 5)];
    let holding = tiles
        .iter()
        .filter(|&&(x, y)| mesh.tiles[y * 16 + x].contains(&ShapeId::new(0)))
        .count();
    assert!(0 < holding);
    assert_eq!(s.open.len(), holding * one.open.len());
    let ignore = [true];
    let mut s = search_toward(&mesh, FixedPoint::new(12 * ONE, 12 * ONE), 0);
    s.ctx.ignore = &ignore;
    s.goal.r = FixedDistance::TILE;
    s.escape_candidates(p, 0).unwrap();
    assert_eq!(s.open.len(), 0);
}

/// One side of shape 0 seen from (4, 3) with a circle of two tiles: side 0, one tile away,
/// gives the two points where the circle crosses it and the start's foot; side 1, two tiles
/// away, the touching point twice and the foot there; side 2, three tiles away, nothing. A
/// corner within the circle is a candidate; a crossing another shape holds is not.
#[test]
fn a_shape_s_side_gives_the_circle_s_crossings_the_foot_and_a_near_corner() {
    let mesh = shape_at_2_2();
    let side = |shape, side: usize| Kind::Side {
        shape: ShapeId::new(shape),
        side: ShapeSide::ALL[side],
    };
    let rr2 = 4 * i64::from(ONE);
    let mut s = search_toward(&mesh, FixedPoint::new(12 * ONE, 12 * ONE), 0);
    s.escape_side(
        FixedPoint::new(4 * ONE, 3 * ONE),
        0,
        (ShapeId::new(0), ShapeSide::ALL[0]),
        rr2,
    )
    .unwrap();
    let mut got = opened(&s);
    got.sort_by_key(|&(at, _, _)| at.get());
    assert_eq!(got.len(), 3);
    assert_eq!(
        (got[1].0, got[1].1, got[1].2),
        (FixedPoint::new(4 * ONE, 2 * ONE), ONE, side(0, 0))
    );
    for &(at, g, kind) in [&got[0], &got[2]] {
        assert_eq!((at.y(), kind), (2 * ONE, side(0, 0)));
        // √3 tiles either side of the foot, two tiles from the start
        assert!(((at.x() - 4 * ONE).abs() - 7_264_748).abs() <= 2, "{at:?}");
        assert!((g - 2 * ONE).abs() <= 2, "{g}");
    }
    let mut s = search_toward(&mesh, FixedPoint::new(12 * ONE, 12 * ONE), 0);
    s.escape_side(
        FixedPoint::new(4 * ONE, 3 * ONE),
        0,
        (ShapeId::new(0), ShapeSide::ALL[1]),
        rr2,
    )
    .unwrap();
    assert_eq!(
        opened(&s),
        vec![(FixedPoint::new(6 * ONE, 3 * ONE), 2 * ONE, side(0, 1)); 3]
    );
    let mut s = search_toward(&mesh, FixedPoint::new(12 * ONE, 12 * ONE), 0);
    s.escape_side(
        FixedPoint::new(4 * ONE, 3 * ONE),
        0,
        (ShapeId::new(0), ShapeSide::ALL[2]),
        rr2,
    )
    .unwrap();
    assert_eq!(opened(&s), vec![]);
    // from (3, 3) the corner (2, 2) lies √2 away
    let mut s = search_toward(&mesh, FixedPoint::new(12 * ONE, 12 * ONE), 0);
    s.escape_side(
        FixedPoint::new(3 * ONE, 3 * ONE),
        0,
        (ShapeId::new(0), ShapeSide::ALL[0]),
        rr2,
    )
    .unwrap();
    assert!(
        opened(&s).iter().any(
            |&(at, g, _)| at == FixedPoint::new(2 * ONE, 2 * ONE) && (g - 5_931_642).abs() <= 2
        )
    );
    // shape 1 holds the crossing right of the foot; its contact at (5, 2) is a candidate
    let mut mesh = shape_at_2_2();
    let _ = mesh.add(FixedRect::new(5 * ONE, 0, 9 * ONE, 3 * ONE));
    let mut s = search_toward(&mesh, FixedPoint::new(12 * ONE, 12 * ONE), 0);
    s.escape_side(
        FixedPoint::new(4 * ONE, 3 * ONE),
        0,
        (ShapeId::new(0), ShapeSide::ALL[0]),
        rr2,
    )
    .unwrap();
    let got = opened(&s);
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
fn the_start_s_foot_on_a_side_lies_between_its_corners_in_the_box_and_free() {
    let mesh = shape_at_2_2();
    let corners = |k: usize| {
        let r = mesh.shapes[0];
        (
            shrunk_corner(r, [0, 1, 3, 0][k], FixedDistance::ZERO),
            shrunk_corner(r, [1, 2, 2, 3][k], FixedDistance::ZERO),
        )
    };
    let side = |side: usize| Kind::Side {
        shape: ShapeId::new(0),
        side: ShapeSide::ALL[side],
    };
    let mut s = search_toward(&mesh, FixedPoint::new(12 * ONE, 12 * ONE), 0);
    let p = FixedPoint::new(3 * ONE, 5 * ONE / 2);
    for k in 0..4 {
        s.escape_foot(p, 0, (ShapeId::new(0), ShapeSide::ALL[k]), corners(k))
            .unwrap();
    }
    let mut got = opened(&s);
    got.sort_by_key(|&(_, g, _)| g);
    assert_eq!(
        got,
        vec![
            (FixedPoint::new(3 * ONE, 2 * ONE), ONE / 2, side(0)),
            (FixedPoint::new(2 * ONE, 5 * ONE / 2), ONE, side(3)),
            (FixedPoint::new(6 * ONE, 5 * ONE / 2), 3 * ONE, side(1)),
            (FixedPoint::new(3 * ONE, 6 * ONE), 7 * ONE / 2, side(2)),
        ]
    );
    let mut s = search_toward(&mesh, FixedPoint::new(12 * ONE, 12 * ONE), 0);
    s.escape_foot(
        FixedPoint::new(2 * ONE, 5 * ONE / 2),
        0,
        (ShapeId::new(0), ShapeSide::ALL[0]),
        corners(0),
    )
    .unwrap();
    s.escape_foot(
        FixedPoint::new(3 * ONE, 6 * ONE),
        0,
        (ShapeId::new(0), ShapeSide::ALL[1]),
        corners(1),
    )
    .unwrap();
    assert_eq!(opened(&s), vec![]);
    s.bounds = FixedRect::new(0, 0, 64 * ONE, ONE);
    s.escape_foot(p, 0, (ShapeId::new(0), ShapeSide::ALL[0]), corners(0))
        .unwrap();
    let mut held = shape_at_2_2();
    let _ = held.add(FixedRect::new(0, 0, 4 * ONE, 4 * ONE));
    let mut t = search_toward(&held, FixedPoint::new(12 * ONE, 12 * ONE), 0);
    t.escape_foot(p, 0, (ShapeId::new(0), ShapeSide::ALL[0]), corners(0))
        .unwrap();
    assert_eq!((opened(&s), opened(&t)), (vec![], vec![]));
}

/// A side's contacts: one wider than the search's shrink, with a shape not ignored, at a free
/// point in the box within the circle is a candidate; one on a side of its own number counts
/// only when its other shape holds the point nudged into it, and costs 0x32 more.
#[test]
fn a_side_s_contacts_become_candidates_when_wide_free_and_near() {
    use crate::pathing::mesh::Contact;
    let mut mesh = shape_at_2_2();
    let _ = mesh.add(FixedRect::new(5 * ONE, 0, 9 * ONE, 3 * ONE));
    let _ = mesh.add(FixedRect::new(4 * ONE, 2 * ONE, 7 * ONE, 4 * ONE));
    let contact = |other, side: usize, at: FixedPoint, half| Contact {
        other: ShapeId::new(other),
        side: ShapeSide::ALL[side],
        at,
        along: at.x(),
        flag: 0,
        half,
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
    let mut s = search_toward(&mesh, FixedPoint::new(12 * ONE, 12 * ONE), 0);
    s.ctx.ignore = &ignore;
    s.escape_contacts(
        FixedPoint::new(4 * ONE, 3 * ONE),
        0,
        (ShapeId::new(0), ShapeSide::ALL[0]),
        4 * i64::from(ONE),
    )
    .unwrap();
    let side = Kind::Side {
        shape: ShapeId::new(0),
        side: ShapeSide::LowY,
    };
    // 3/4 and 1 tile off: 5/4 tiles
    let mut got = opened(&s);
    got.sort_by_key(|&(_, g, _)| g);
    assert_eq!(
        got,
        vec![(near, 5 * ONE / 4, side), (near, 5 * ONE / 4 + 0x32, side)]
    );
}
