//! The spawn search's later rings: the edge's points, the footprint's points and the tile of
//! the fewest objects.

use super::*;
use crate::map::Footprint;
use crate::map::Position;
use crate::testing::tuning;

/// A building of radius `radius` at (10, 10) without a gather point or walls.
fn site(radius: f32) -> Site {
    Site {
        position: Position::new(10.0, 10.0, 0.0),
        radius: Footprint::new(radius, radius),
        goal: None,
        walls: [false; 4],
        closest_goal: None,
    }
}

/// The points of the outer ring about a building of radius 1 at (10, 10): the sides at 1.5 from
/// its centre, each from a step past its corner (11 to 14).
const OUTER: [Point; 8] = [
    Point::new(9.5, 11.5),
    Point::new(10.5, 11.5),
    Point::new(11.5, 10.5),
    Point::new(11.5, 9.5),
    Point::new(10.5, 8.5),
    Point::new(9.5, 8.5),
    Point::new(8.5, 9.5),
    Point::new(8.5, 10.5),
];

/// The inner ring's points about a building of radius 1 at (10, 10): the top side (15), the
/// left side and the right side from a step below the top (16, 17), the bottom side from a step
/// past the left (18).
const INNER: [Point; 5] = [
    Point::new(9.5, 10.5),
    Point::new(10.5, 10.5),
    Point::new(9.5, 9.5),
    Point::new(10.5, 9.5),
    Point::new(10.5, 9.5),
];

/// The outer ring of `site` where `free` says which points are free: its answer and the points
/// it tried, with their test.
fn outer(site: &Site, free: impl Fn(Point) -> bool) -> (Option<Point>, Vec<(Point, SpawnTest)>) {
    let mut tried = Vec::new();
    let answer = outer_ring(
        site,
        &mut |point, spawn_test| {
            tried.push((point, spawn_test));
            Ok(if free(point) {
                Tested::Free
            } else {
                Tested::Blocked { stays: true }
            })
        },
        &tuning().spawn,
    )
    .unwrap();
    (answer, tried)
}

/// The inner ring of `site` for a unit of step 0.5, `free` saying which points are free and
/// `objects` each tile's object count (none off the map): its answer and the points it tried.
fn inner(
    site: &Site,
    free: impl Fn(Point) -> bool,
    objects: impl Fn(Tile) -> Option<u8>,
) -> (Option<Point>, Vec<(Point, SpawnTest)>) {
    let mut tried = Vec::new();
    let answer = inner_ring(
        site,
        GroundOffset::new(0.5, 0.5),
        &mut |point, spawn_test| {
            tried.push((point, spawn_test));
            Ok(if free(point) {
                Tested::Free
            } else {
                Tested::Blocked { stays: false }
            })
        },
        &|tile| Ok(objects(tile)),
        &tuning().spawn,
    )
    .unwrap();
    (answer, tried)
}

/// The four points `FUN_1411d0b20` tries at `at` for a step of 0.5: itself, below, below right,
/// right.
fn candidates(at: Point) -> [Point; 4] {
    let (x, y) = (at.x(), at.y());
    [
        at,
        Point::new(x, y - 0.5),
        Point::new(x + 0.5, y - 0.5),
        Point::new(x + 0.5, y - 0.5 + 0.5),
    ]
}

#[test]
fn outer_ring_walks_the_four_sides_beyond_the_footprint() {
    let (answer, tried) = outer(&site(1.0), |_| false);
    assert_eq!(answer, None);
    assert_eq!(tried, OUTER.map(|point| (point, SpawnTest::Outer)).to_vec());
}

#[test]
fn outer_ring_takes_its_first_free_point() {
    let (answer, tried) = outer(&site(1.0), |point| point == Point::new(11.5, 9.5));
    assert_eq!(answer, Some(Point::new(11.5, 9.5)));
    assert_eq!(tried.len(), 4);
}

/// A building at a NaN place: every side's first compare fails (`JBE` before each loop), so
/// neither ring tries a point.
#[test]
fn rings_about_nan_place_try_nothing() {
    let nan = Site {
        position: Position::new(f32::NAN, f32::NAN, 0.0),
        ..site(1.0)
    };
    assert_eq!(outer(&nan, |_| true), (None, Vec::new()));
    assert_eq!(inner(&nan, |_| true, |_| Some(0)), (None, Vec::new()));
}

#[test]
fn inner_ring_tries_each_point_with_its_three_neighbours() {
    let (answer, tried) = inner(&site(1.0), |_| false, |_| None);
    assert_eq!(answer, None);
    let expected: Vec<_> = INNER
        .iter()
        .flat_map(|&point| candidates(point))
        .map(|point| (point, SpawnTest::Inner))
        .collect();
    assert_eq!(tried, expected);
}

/// The third candidate of the first point (a step below and right of it) is free: that one.
#[test]
fn inner_ring_takes_its_first_free_candidate() {
    let (answer, tried) = inner(
        &site(1.0),
        |point| point == Point::new(10.0, 10.0),
        |_| None,
    );
    assert_eq!(answer, Some(Point::new(10.0, 10.0)));
    assert_eq!(tried.len(), 3);
}

/// Nothing free: the first point whose tile holds the fewest objects, a tile of fewer than 255
/// (a later tile of the same count is not kept); off the map none.
#[test]
fn inner_ring_keeps_the_first_point_of_the_fewest_objects() {
    let counts = |tile: Tile| match (tile.x(), tile.y()) {
        (9, 10) => Some(3),
        (10, 9 | 10) => Some(1),
        (9, 9) => Some(2),
        _ => None,
    };
    assert_eq!(
        inner(&site(1.0), |_| false, counts).0,
        Some(Point::new(10.5, 10.5))
    );
    assert_eq!(inner(&site(1.0), |_| false, |_| Some(255)).0, None);
    assert_eq!(
        inner(&site(1.0), |_| false, |_| Some(254)).0,
        Some(INNER[0])
    );
}

/// A building narrower than twice the margin: its inner sides end before they start.
#[test]
fn inner_ring_of_small_building_has_no_points() {
    assert_eq!(inner(&site(0.4), |_| true, |_| Some(0)), (None, Vec::new()));
}

/// A NaN step leaves the first point's own test as it is; its neighbours are NaN, and its tile
/// is the point's.
#[test]
fn inner_spot_of_nan_step_keeps_the_point_by_its_tile() {
    let mut kept = None;
    let mut tried = Vec::new();
    let free = inside_spot(
        Point::new(9.5, 10.5),
        GroundOffset::new(f32::NAN, f32::NAN),
        &mut kept,
        &mut |point, _| {
            tried.push(point);
            Ok(Tested::Blocked { stays: false })
        },
        &|tile| Ok((tile == Tile::new(9, 10)).then_some(7)),
    )
    .unwrap();
    assert_eq!(free, None);
    assert_eq!(tried.first(), Some(&Point::new(9.5, 10.5)));
    assert_eq!(
        kept,
        Some(Kept {
            point: Point::new(9.5, 10.5),
            objects: 7
        })
    );
}
