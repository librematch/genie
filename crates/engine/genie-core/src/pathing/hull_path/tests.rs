use super::*;
use crate::map::GridSize;
use crate::map::Position;
use crate::map::Radii;
use crate::map::Rect;
use crate::map::TerrainMask;
use crate::map::Tile;
use crate::map::TileRect;
use crate::map::spatial::{Obstruction, Tiles};
use crate::objects::ActionState;
use crate::objects::masters::BuildableOn;
use crate::objects::masters::ClassId;
use crate::objects::masters::ObstructionClass;
use crate::objects::masters::ObstructionType;
use crate::objects::masters::class;
use crate::objects::masters::ids;
use crate::pathing::ShortFound;
use crate::pathing::hull_path::rounds::Rounds;
use crate::players::PlayerId;
use crate::test_ids::object_id;

/// The unit square as `addShapeToHulls` makes a hull of it: high right, low right, low left,
/// high left.
fn square() -> Hull {
    Hull {
        points: vec![
            Point::new(1.0, 1.0),
            Point::new(1.0, 0.0),
            Point::new(0.0, 0.0),
            Point::new(0.0, 1.0),
        ],
        entrances: vec![None; 4],
        min: Point::new(0.0, 0.0),
        max: Point::new(1.0, 1.0),
        objects: Vec::new(),
    }
}

/// A hull's place is the value a search node holds for it.
#[test]
fn a_hull_s_node_slot_is_its_place() {
    let h = HullIndex::new(3);
    assert_eq!((h.index(), h.slot()), (3, 3));
}

#[test]
fn a_hull_holds_the_points_inside_it() {
    let h = square();
    assert!(h.inside(Point::new(0.5, 0.5)));
    assert!(!h.inside(Point::new(1.5, 0.5)));
    assert!(!h.inside(Point::new(0.5, -0.25)));
}

#[test]
fn a_walk_meets_the_nearest_side_it_crosses() {
    let h = square();
    // the side from point 2 (low left) to point 3 (high left)
    assert_eq!(
        h.intersection(false, Point::new(-1.0, 0.5), Point::new(2.0, 0.5)),
        Some((Point::new(0.0, 0.5), 3))
    );
    // the last side ends at point 0: it is side 0
    assert_eq!(
        h.intersection(false, Point::new(0.5, 2.0), Point::new(0.5, 0.5)),
        Some((Point::new(0.5, 1.0), 0))
    );
    assert_eq!(
        h.intersection(false, Point::new(2.0, 2.0), Point::new(3.0, 2.0)),
        None
    );
}

#[test]
fn the_gift_wrap_turns_clockwise_from_the_highest_point() {
    let mut h = square();
    h.merge(&[Point::new(2.0, 0.5)]).unwrap();
    assert_eq!(
        h.points,
        [
            Point::new(1.0, 1.0),
            Point::new(2.0, 0.5),
            Point::new(1.0, 0.0),
            Point::new(0.0, 0.0),
            Point::new(0.0, 1.0)
        ]
    );
    assert_eq!((h.min, h.max), (Point::new(0.0, 0.0), Point::new(2.0, 1.0)));
    // a point inside changes nothing
    let mut inside = square();
    inside.merge(&[Point::new(0.5, 0.5)]).unwrap();
    assert_eq!(inside.points, square().points);
}

#[test]
fn a_box_overlapping_a_hull_reports_its_corners_outside() {
    let h = square();
    let (overlap, outside) = h.shape_overlap(Point::new(0.5, 0.5), Point::new(1.5, 1.5));
    assert_eq!((overlap, outside), (2, [false, true, true, true]));
    assert_eq!(
        h.shape_overlap(Point::new(0.25, 0.25), Point::new(0.75, 0.75))
            .0,
        3
    );
    assert_eq!(
        h.shape_overlap(Point::new(2.0, 2.0), Point::new(3.0, 3.0))
            .0,
        0
    );
}

#[test]
fn a_ray_leaves_a_hull_by_the_side_it_meets() {
    let h = square();
    assert_eq!(
        h.ray_to_hull(Point::new(0.5, 0.5), 0),
        Some((Point::new(0.5, 0.0), 2))
    );
    assert_eq!(
        h.ray_to_hull(Point::new(0.5, 0.5), 3),
        Some((Point::new(1.0, 0.5), 1))
    );
}

/// `FUN_140f8ea30`: toward each side, the nearest box whose span holds the point: past it
/// (strictly for the box the point lies on, `own`) its far side, inside it its near side,
/// short of it nothing; out keeps the point's other coordinate. The span tests are
/// `COMISS`/`JA` skips (0x140f8ec1c, 0x140f8eb6e): an unordered coordinate passes them.
#[test]
fn a_ray_meets_the_nearest_box_side_toward_it() {
    let shape = |bounds: [f32; 4], id: RecordId| Shape {
        bounds: Rect::from(bounds),
        id,
    };
    let hull = Hull {
        objects: vec![
            shape([0.0, 0.0, 2.0, 2.0], RecordId::from(object_id(1))),
            shape([1.0, 3.0, 3.0, 5.0], RecordId::from(object_id(2))),
            shape([4.0, 1.0, 6.0, 2.0], RecordId::from(object_id(3))),
        ],
        ..Hull::default()
    };
    // down from above both: box 1's top (its side 0) is nearer than box 0's
    assert_eq!(
        hull.ray_to_box(Point::new(1.5, 6.0), 0, -1),
        Some((Point::new(1.5, 5.0), 1, 0))
    );
    // down from inside box 1: its bottom (side 2), nearer than box 0's top
    assert_eq!(
        hull.ray_to_box(Point::new(1.5, 4.0), 0, -1),
        Some((Point::new(1.5, 3.0), 1, 2))
    );
    // on box 1's top side: past it (its top), unless it is the point's own box (its bottom)
    assert_eq!(
        hull.ray_to_box(Point::new(1.5, 5.0), 0, -1),
        Some((Point::new(1.5, 5.0), 1, 0))
    );
    assert_eq!(
        hull.ray_to_box(Point::new(1.5, 5.0), 0, 1),
        Some((Point::new(1.5, 3.0), 1, 2))
    );
    // up from below box 0: its bottom; right from box 0's right side, own: box 2's left
    assert_eq!(
        hull.ray_to_box(Point::new(0.5, -1.0), 1, -1),
        Some((Point::new(0.5, 0.0), 0, 2))
    );
    assert_eq!(
        hull.ray_to_box(Point::new(2.0, 1.5), 3, 0),
        Some((Point::new(4.0, 1.5), 2, 3))
    );
    // left from inside box 2: its left side (side 3); from the right of it: its right
    assert_eq!(
        hull.ray_to_box(Point::new(5.0, 1.5), 2, -1),
        Some((Point::new(4.0, 1.5), 2, 3))
    );
    assert_eq!(
        hull.ray_to_box(Point::new(7.0, 1.5), 2, -1),
        Some((Point::new(6.0, 1.5), 2, 1))
    );
    // right from box 2's right: nothing beyond; an unknown side: nothing
    assert_eq!(hull.ray_to_box(Point::new(6.5, 1.5), 3, -1), None);
    assert_eq!(hull.ray_to_box(Point::new(1.5, 6.0), 4, -1), None);
    // an unordered x passes every span test of a vertical ray: the nearest top below
    let (at, k, side) = hull.ray_to_box(Point::new(f32::NAN, 6.0), 0, -1).unwrap();
    assert!(at.x().is_nan());
    assert_eq!((at.y(), k, side), (5.0, 1, 0));
}

fn placed(x: f32, y: f32, r: f32, kind: ObstructionType) -> Placed {
    Placed {
        position: Position::new(x, y, 0.0),
        radius: Footprint::new(r, r),
        class: if kind == ObstructionType::CIRCLE {
            class::VILLAGER
        } else {
            class::PREDATOR
        },
        player: PlayerId::new(1),
        master: ids::ID_0,
        obstruction_type: kind,
        buildable_on: BuildableOn::NO,
        tall: true,
        moves: false,
        unknown: None,
        obstruction: Some(Obstruction {
            kind,
            class: ObstructionClass::UNIT,
        }),
    }
}

#[test]
fn the_quick_path_goes_around_one_square_obstruction() {
    let mut w = Spatial::with_tiles(
        GridSize::new(20, 20),
        Tiles::new(GridSize::new(20, 20), vec![4; 400]),
    );
    // a villager, a tree in its way and its target past the tree
    w.insert(
        object_id(9171),
        placed(2.0, 5.5, 0.2, ObstructionType::CIRCLE),
    );
    w.insert(
        object_id(48),
        placed(5.5, 5.5, 0.5, ObstructionType::TYPE_2),
    );
    w.insert(
        object_id(9149),
        placed(10.5, 5.5, 0.5, ObstructionType::TYPE_2),
    );
    let mut pather = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        object_id(9171),
        Some(object_id(9149)),
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    let (result, path) = pather
        .find(
            Point::new(2.0, 5.5),
            Point::new(10.5, 5.5),
            Some(object_id(9149)),
            Distance::new(0.1),
        )
        .unwrap();
    assert_eq!(result, ShortFound::Reached);
    // by the tree's lower side: (half side + radius) √2 below its middle
    let [start, by, end] = path.points[..] else {
        panic!("{path:?}")
    };
    assert_eq!(start, Point::new(2.0, 5.5));
    assert!((by.x() - 5.5).abs() < 1e-5 && (by.y() - (5.5 - 0.7 * SQRT_2)).abs() < 1e-5);
    // the goal moved on by the range
    assert!((end.x() - 10.6).abs() < 1e-5 && (end.y() - 5.5).abs() < 1e-6);
}

/// `FUN_140f8af60`'s merge scan tests the other hull's sides k = 0..n-1 (`cmp edi, esi; jl`
/// at 0x140f8b2c4 with esi = n - 1): a box hull whose bottom side alone crosses the grown
/// hull merges into it.
#[test]
fn a_hull_crossing_only_the_other_s_last_tested_side_merges() {
    let mut w = Spatial::with_tiles(
        GridSize::new(20, 20),
        Tiles::new(GridSize::new(20, 20), vec![4; 400]),
    );
    let mut pather = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        ObjectId::NOBODY,
        None,
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    // a thin strip across the box's corner at (0, 0), a vertex of neither inside the other
    let strip = Hull {
        points: vec![
            Point::new(-0.9, 2.1),
            Point::new(2.1, -0.9),
            Point::new(2.0, -1.0),
            Point::new(-1.0, 2.0),
        ],
        entrances: vec![None; 4],
        min: Point::new(-1.0, -1.0),
        max: Point::new(2.1, 2.1),
        objects: Vec::new(),
    };
    // a new hull's order: max-max, max-min, min-min, min-max; the side min-min to max-min
    // (k = 2 of 4) is the only one the strip crosses that the scan tests
    let square = Hull {
        points: vec![
            Point::new(10.0, 10.0),
            Point::new(10.0, 0.0),
            Point::new(0.0, 0.0),
            Point::new(0.0, 10.0),
        ],
        entrances: vec![None; 4],
        min: Point::new(0.0, 0.0),
        max: Point::new(10.0, 10.0),
        objects: Vec::new(),
    };
    pather.hulls = vec![strip, square];
    // a box at the strip's far end goes into the strip, which then meets the square
    let hull = pather
        .add_shape(Point::new(-1.2, 1.9), Point::new(-0.8, 2.3))
        .unwrap();
    assert_eq!((hull, pather.hulls.len()), (HullIndex::new(0), 1));
    assert!(pather.hulls[0].points.contains(&Point::new(10.0, 10.0)));
}

/// Slot 0x3e8 (`FUN_14114f790`): the old short pather from the unit's place with its radius
/// and terrain mask, ignoring it and the target; a path found or none needed gives its
/// length, a target the world does not hold none.
#[test]
fn a_unit_s_short_reach_is_its_old_short_path_s_length() {
    let mut w = Spatial::with_tiles(
        GridSize::new(20, 20),
        Tiles::new(GridSize::new(20, 20), vec![4; 400]),
    );
    w.insert(
        object_id(9171),
        placed(2.0, 5.5, 0.2, ObstructionType::CIRCLE),
    );
    w.insert(
        object_id(48),
        placed(5.5, 5.5, 0.5, ObstructionType::TYPE_2),
    );
    w.insert(
        object_id(9149),
        placed(10.5, 5.5, 0.5, ObstructionType::TYPE_2),
    );
    let mut mover = crate::gathering::hunt::tests::walker().placed(Position::new(2.0, 5.5, 0.0));
    mover.stats.radius = Radii::new(0.2, 0.2, 0.2);
    mover.stats.restriction_mask = TerrainMask::of_bits(6);
    let me = crate::movement::Me {
        human_order: crate::world::WorldTime::ZERO,
        id: object_id(9171),
        action_state: ActionState::NO_ACTION,
        group: None,
    };
    let length = mover
        .short_reach(
            &me,
            &mut w,
            object_id(9149),
            Distance::new(0.1),
            crate::world::index::ObjectsRef::empty(),
        )
        .unwrap()
        .unwrap();
    let mut pather = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        object_id(9171),
        Some(object_id(9149)),
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    let (_, path) = pather
        .find(
            Point::new(2.0, 5.5),
            Point::new(10.5, 5.5),
            Some(object_id(9149)),
            Distance::new(0.1),
        )
        .unwrap();
    assert_eq!(length, path.length());
    let by = 5.5 - 0.7 * SQRT_2;
    let expected = (3.5f32 * 3.5 + (5.5 - by) * (5.5 - by)).sqrt()
        + (5.1f32 * 5.1 + (5.5 - by) * (5.5 - by)).sqrt();
    assert!(
        (length.get() - expected).abs() < 1e-4,
        "{length} {expected}"
    );
    assert_eq!(
        mover
            .short_reach(
                &me,
                &mut w,
                object_id(777),
                Distance::new(0.1),
                crate::world::index::ObjectsRef::empty()
            )
            .unwrap(),
        None
    );
}

/// An empty 20 × 20 map of terrain 4, and a pather on it with radius 0.2 and mask 6.
/// `FindShortPath` removes the manager's temporary tile records at its start and at its
/// exits (`FUN_1410561f0`), but the quick-only exit (slot 0x3e0's search) leaves the quick
/// path's records to its caller, which removes them ([`quick_reach_from`]).
#[test]
fn find_short_path_removes_its_tile_records_but_on_its_quick_only_exit() {
    // a wall across the map at x = 8 that the unit's mask lacks
    let mut classes = vec![4; 400];
    for y in 0..20 {
        classes[y * 20 + 8] = 1;
    }
    let mut w = Spatial::with_tiles(
        GridSize::new(20, 20),
        Tiles::new(GridSize::new(20, 20), classes),
    );
    w.sight = Some(crate::vision::visibility::Visibility::new(
        GridSize::new(20, 20),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        0,
        Vec::new(),
    ));
    let mut quick = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        ObjectId::NOBODY,
        None,
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    let (result, _) = quick
        .find_quick(
            Point::new(2.0, 5.5),
            Point::new(12.5, 5.5),
            None,
            Distance::ZERO,
        )
        .unwrap();
    assert_eq!(result, ShortFound::Blocked);
    assert!(
        w.tile_records().contains(&Tile::new(8, 5)),
        "{:?}",
        w.tile_records()
    );
    // a search starts without them and leaves none
    let mut full = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        ObjectId::NOBODY,
        None,
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    let (result, _) = full
        .find(
            Point::new(2.0, 5.5),
            Point::new(12.5, 5.5),
            None,
            Distance::ZERO,
        )
        .unwrap();
    assert_ne!(result, ShortFound::Reached);
    assert_eq!(w.tile_records(), Vec::<Tile>::new());
    // slot 0x3e0 removes what its quick path left
    assert_eq!(
        quick_reach_from(
            (&mut w, crate::world::index::ObjectsRef::empty()),
            ObjectId::NOBODY,
            Point::new(2.0, 5.5),
            None,
            TerrainMask::of_bits(6),
            Distance::new(0.2),
            Distance::ZERO
        )
        .unwrap(),
        None
    );
    assert_eq!(w.tile_records(), Vec::<Tile>::new());
}

/// The pool starts with the two lists' heads, the search's first; a node pushed takes the
/// next place; each list keeps its own closed nodes.
#[test]
fn the_node_pool_holds_the_heads_first_and_numbers_nodes_in_order() {
    let mut nodes = Nodes::heads();
    assert_eq!(
        (NodeList::SEARCH.head(), NodeList::WALKS.head()),
        (NodeIndex(0), NodeIndex(1))
    );
    let mut node = Node::head();
    node.cost = Distance::new(3.0);
    assert_eq!(nodes.push(node), NodeIndex(2));
    assert_eq!(
        nodes.checked(NodeIndex(2)).unwrap().cost,
        Distance::new(3.0)
    );
    assert_eq!(nodes[NodeIndex(2)].cost, Distance::new(3.0));
    nodes.checked_mut(NodeIndex(2)).unwrap().cost = Distance::new(4.0);
    assert_eq!(
        nodes.get(NodeIndex(2)).map(|n| n.cost),
        Some(Distance::new(4.0))
    );
    assert!(nodes.checked(NodeIndex(3)).is_err());
    assert!(nodes.get(NodeIndex(3)).is_none());
    let mut closed = ClosedLists::default();
    closed.of_mut(NodeList::WALKS).push(NodeIndex(2));
    assert_eq!(*closed.of(NodeList::WALKS), [NodeIndex(2)]);
    assert_eq!(*closed.of(NodeList::SEARCH), []);
}

/// A straight walk is the root of the squares' sum, y's first: the same either way, and
/// exact for a 3-4-5 triangle.
#[test]
fn a_walk_s_length_is_the_root_of_the_squares_sum() {
    let (a, b) = (Point::new(1.0, 2.0), Point::new(4.0, 6.0));
    assert_eq!(walk_length(a, b), Distance::new(5.0));
    assert_eq!(walk_length(b, a), Distance::new(5.0));
    assert_eq!(walk_length(a, a), Distance::ZERO);
    let (c, d) = (Point::new(0.1, 0.7), Point::new(2.3, -1.9));
    assert_eq!(
        walk_length(c, d).get().to_bits(),
        sqrt((0.7f32 - -1.9) * (0.7 - -1.9) + (0.1f32 - 2.3) * (0.1 - 2.3)).to_bits()
    );
}

fn open_world() -> Spatial {
    Spatial::with_tiles(
        GridSize::new(20, 20),
        Tiles::new(GridSize::new(20, 20), vec![4; 400]),
    )
}

/// List `l`'s open nodes from its head, each linked back to the one before.
fn open_nodes(p: &HullPather<'_>, l: NodeList) -> Vec<NodeIndex> {
    let mut out = Vec::new();
    let mut before = l.head();
    let mut n = p.nodes[l.head()].next;
    while let Some(i) = n {
        assert_eq!(p.nodes[i].prev, Some(before), "node {i:?}'s prev");
        out.push(i);
        before = i;
        n = p.nodes[i].next;
    }
    out
}

/// `MShortPather::addNode` (`FUN_140f8aa10`): the cost is the parent's plus the step, the
/// distance to the goal 0 without one. An open node goes after the open nodes whose cost
/// plus distance is smaller (`COMISS`/`JBE` at 0x140f8ae2d: before equal ones); with a hull
/// point (both `+0x28` and `+0x2c` at least 0, compared as integers), after the node before
/// the first larger one of the scan. An open node of the same hull point that costs more
/// closes and the new one takes its place; one that costs no more, or a closed one (all of
/// them), refuses the new node. Not open: straight into the closed list.
#[test]
fn a_node_joins_the_open_list_by_cost_and_distance_and_replaces_a_dearer_one() {
    let mut w = open_world();
    let mut p = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        ObjectId::NOBODY,
        None,
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    let goal = Some(Point::new(10.0, 0.0));
    let root = p
        .add_node(NewNode {
            list: NodeList::SEARCH,
            open: true,
            parent: None,
            position: Point::new(0.0, 0.0),
            goal,
            hull: -1,
            vertex: -1,
            direction: -1,
            by_place: false,
        })
        .unwrap()
        .unwrap();
    assert_eq!(
        (p.nodes[root].cost, p.nodes[root].to_goal),
        (Distance::ZERO, Distance::new(10.0))
    );
    // cost 5, distance √65: 13.06
    let a = p
        .add_node(NewNode {
            list: NodeList::SEARCH,
            open: true,
            parent: Some(root),
            position: Point::new(3.0, 4.0),
            goal,
            hull: 0,
            vertex: 1,
            direction: 7,
            by_place: false,
        })
        .unwrap()
        .unwrap();
    assert_eq!(
        (p.nodes[a].cost, p.nodes[a].parent),
        (Distance::new(5.0), Some(root))
    );
    assert_eq!(
        (p.nodes[a].hull, p.nodes[a].vertex, p.nodes[a].direction),
        (0, 1, 7)
    );
    // 6 + 4 = 10: the scan puts it after the node before `a`, past the root's equal 10
    let b = p
        .add_node(NewNode {
            list: NodeList::SEARCH,
            open: true,
            parent: Some(root),
            position: Point::new(6.0, 0.0),
            goal,
            hull: 0,
            vertex: 2,
            direction: -1,
            by_place: false,
        })
        .unwrap()
        .unwrap();
    assert_eq!(open_nodes(&p, NodeList::SEARCH), [root, b, a]);
    // no hull point: no scan, the walk stops at the first node of no smaller sum
    let c = p
        .add_node(NewNode {
            list: NodeList::SEARCH,
            open: true,
            parent: Some(root),
            position: Point::new(6.0, 0.0),
            goal,
            hull: -1,
            vertex: -1,
            direction: -1,
            by_place: false,
        })
        .unwrap()
        .unwrap();
    assert_eq!(open_nodes(&p, NodeList::SEARCH), [c, root, b, a]);
    // `a`'s hull point at no smaller cost: refused
    assert_eq!(
        p.add_node(NewNode {
            list: NodeList::SEARCH,
            open: true,
            parent: Some(root),
            position: Point::new(0.0, 5.0),
            goal,
            hull: 0,
            vertex: 1,
            direction: -1,
            by_place: false
        })
        .unwrap(),
        None
    );
    // cheaper (3): `a` closes, the new node goes in by its sum (3 + 7)
    let d = p
        .add_node(NewNode {
            list: NodeList::SEARCH,
            open: true,
            parent: Some(root),
            position: Point::new(3.0, 0.0),
            goal,
            hull: 0,
            vertex: 1,
            direction: -1,
            by_place: false,
        })
        .unwrap()
        .unwrap();
    assert_eq!(open_nodes(&p, NodeList::SEARCH), [d, c, root, b]);
    assert_eq!(*p.closed.of(NodeList::SEARCH), [a]);
    // a closed node of the hull point that costs no more refuses one (`b`, 6, a cost-7
    // node); a cheaper one is made
    p.close(NodeList::SEARCH, b).unwrap();
    assert_eq!(
        p.add_node(NewNode {
            list: NodeList::SEARCH,
            open: true,
            parent: Some(root),
            position: Point::new(0.0, 7.0),
            goal,
            hull: 0,
            vertex: 2,
            direction: -1,
            by_place: false
        })
        .unwrap(),
        None
    );
    let e = p
        .add_node(NewNode {
            list: NodeList::SEARCH,
            open: true,
            parent: Some(root),
            position: Point::new(5.0, 0.0),
            goal,
            hull: 0,
            vertex: 2,
            direction: -1,
            by_place: false,
        })
        .unwrap()
        .unwrap();
    assert_eq!(p.nodes[e].cost, Distance::new(5.0));
    // a node not open goes to the closed list without a test
    let f = p
        .add_node(NewNode {
            list: NodeList::SEARCH,
            open: false,
            parent: Some(e),
            position: Point::new(5.0, 0.0),
            goal: None,
            hull: 0,
            vertex: 2,
            direction: -1,
            by_place: false,
        })
        .unwrap()
        .unwrap();
    assert_eq!(
        (p.nodes[f].cost, p.nodes[f].to_goal),
        (Distance::new(5.0), Distance::ZERO)
    );
    assert_eq!(*p.closed.of(NodeList::SEARCH), [a, b, f]);
    assert!(!open_nodes(&p, NodeList::SEARCH).contains(&f));
}

/// `addNode` by place (its last argument set): the same place is within [`EPSILON`] on both
/// axes (`COMISS`/`JA`, an unordered difference is no match), with or without a hull point;
/// of the closed list, all but the last node count.
#[test]
fn a_node_by_place_matches_within_epsilon_and_skips_the_last_closed_node() {
    let mut w = open_world();
    let mut p = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        ObjectId::NOBODY,
        None,
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    let root = p
        .add_node(NewNode {
            list: NodeList::WALKS,
            open: true,
            parent: None,
            position: Point::new(0.0, 0.0),
            goal: None,
            hull: -1,
            vertex: -1,
            direction: -1,
            by_place: true,
        })
        .unwrap()
        .unwrap();
    let a = p
        .add_node(NewNode {
            list: NodeList::WALKS,
            open: true,
            parent: Some(root),
            position: Point::new(2.0, 0.0),
            goal: None,
            hull: -1,
            vertex: -1,
            direction: -1,
            by_place: true,
        })
        .unwrap()
        .unwrap();
    // within EPSILON of `a`, dearer: refused; cheaper (from a nearer parent): replaces it
    let near = Point::new(2.0 + EPSILON * 0.5, 0.0);
    assert_eq!(
        p.add_node(NewNode {
            list: NodeList::WALKS,
            open: true,
            parent: Some(a),
            position: near,
            goal: None,
            hull: -1,
            vertex: -1,
            direction: -1,
            by_place: true
        })
        .unwrap(),
        None
    );
    let q = p
        .add_node(NewNode {
            list: NodeList::WALKS,
            open: false,
            parent: None,
            position: Point::new(1.5, 0.0),
            goal: None,
            hull: -1,
            vertex: -1,
            direction: -1,
            by_place: false,
        })
        .unwrap()
        .unwrap();
    let b = p
        .add_node(NewNode {
            list: NodeList::WALKS,
            open: true,
            parent: Some(q),
            position: near,
            goal: None,
            hull: -1,
            vertex: -1,
            direction: -1,
            by_place: true,
        })
        .unwrap()
        .unwrap();
    assert_eq!(*p.closed.of(NodeList::WALKS), [q, a]);
    assert_eq!(open_nodes(&p, NodeList::WALKS), [root, b]);
    // a place EPSILON away is another place
    assert!(
        p.add_node(NewNode {
            list: NodeList::WALKS,
            open: true,
            parent: Some(a),
            position: Point::new(2.0 + EPSILON * 2.0, 0.0),
            goal: None,
            hull: -1,
            vertex: -1,
            direction: -1,
            by_place: true
        })
        .unwrap()
        .is_some()
    );
    // the closed list's last node does not count, the ones before it do
    p.empty(NodeList::WALKS).unwrap();
    let r = p
        .add_node(NewNode {
            list: NodeList::WALKS,
            open: true,
            parent: None,
            position: Point::new(9.0, 9.0),
            goal: None,
            hull: -1,
            vertex: -1,
            direction: -1,
            by_place: true,
        })
        .unwrap()
        .unwrap();
    p.close(NodeList::WALKS, r).unwrap();
    let s = p
        .add_node(NewNode {
            list: NodeList::WALKS,
            open: false,
            parent: None,
            position: Point::new(4.0, 4.0),
            goal: None,
            hull: -1,
            vertex: -1,
            direction: -1,
            by_place: false,
        })
        .unwrap()
        .unwrap();
    assert_eq!(*p.closed.of(NodeList::WALKS), [r, s]);
    assert!(
        p.add_node(NewNode {
            list: NodeList::WALKS,
            open: true,
            parent: Some(s),
            position: Point::new(4.0, 4.0),
            goal: None,
            hull: -1,
            vertex: -1,
            direction: -1,
            by_place: true
        })
        .unwrap()
        .is_some()
    );
    assert!(
        p.add_node(NewNode {
            list: NodeList::WALKS,
            open: true,
            parent: Some(s),
            position: Point::new(9.0, 9.0),
            goal: None,
            hull: -1,
            vertex: -1,
            direction: -1,
            by_place: true
        })
        .unwrap()
        .is_none()
    );
}

/// `moveToClosedList` (`FUN_140f8ce00`) unlinks the node (its own links stay) and appends it
/// to the closed list; `closeAllOpenNodes` (`FUN_140f8c130`) closes the open nodes in their
/// order and makes every closed node forget its hull point (`+0x28`, `+0x2c` -1); `emptyLists`
/// (`FUN_140f8c300`) drops the list's open and closed nodes.
#[test]
fn nodes_close_one_by_one_or_all_and_the_lists_empty() {
    let mut w = open_world();
    let mut p = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        ObjectId::NOBODY,
        None,
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    let goal = Some(Point::new(10.0, 0.0));
    let n: Vec<NodeIndex> = (0..4)
        .map(|k| {
            p.add_node(NewNode {
                list: NodeList::SEARCH,
                open: true,
                parent: None,
                position: Point::new(k as f32, 0.0),
                goal,
                hull: 0,
                vertex: k,
                direction: 2,
                by_place: false,
            })
            .unwrap()
            .unwrap()
        })
        .collect();
    // the nearest to the goal first
    assert_eq!(open_nodes(&p, NodeList::SEARCH), [n[3], n[2], n[1], n[0]]);
    p.close(NodeList::SEARCH, n[2]).unwrap();
    assert_eq!(open_nodes(&p, NodeList::SEARCH), [n[3], n[1], n[0]]);
    assert_eq!(
        (p.nodes[n[2]].prev, p.nodes[n[2]].next),
        (Some(n[3]), Some(n[1]))
    );
    // the last open node: no next to relink
    p.close(NodeList::SEARCH, n[0]).unwrap();
    assert_eq!(open_nodes(&p, NodeList::SEARCH), [n[3], n[1]]);
    assert_eq!(*p.closed.of(NodeList::SEARCH), [n[2], n[0]]);
    assert_eq!((p.nodes[n[2]].hull, p.nodes[n[2]].vertex), (0, 2));
    p.close_all(NodeList::SEARCH).unwrap();
    assert_eq!(open_nodes(&p, NodeList::SEARCH), []);
    assert_eq!(*p.closed.of(NodeList::SEARCH), [n[2], n[0], n[3], n[1]]);
    for &i in &n {
        assert_eq!((p.nodes[i].hull, p.nodes[i].vertex), (-1, -1));
        assert_eq!(p.nodes[i].direction, 2);
    }
    // list 1 is another list
    let other = p
        .add_node(NewNode {
            list: NodeList::WALKS,
            open: true,
            parent: None,
            position: Point::new(1.0, 1.0),
            goal,
            hull: 3,
            vertex: 3,
            direction: -1,
            by_place: false,
        })
        .unwrap()
        .unwrap();
    p.empty(NodeList::SEARCH).unwrap();
    assert_eq!(
        (
            open_nodes(&p, NodeList::SEARCH),
            p.closed.of(NodeList::SEARCH).len()
        ),
        (vec![], 0)
    );
    assert_eq!(open_nodes(&p, NodeList::WALKS), [other]);
    p.empty(NodeList::WALKS).unwrap();
    assert_eq!(open_nodes(&p, NodeList::WALKS), []);
}

/// `MShortPather::checkPointInsideHull` (`FUN_140f8be30`): the first hull with points that
/// holds the point, moved [`NUDGE`] of the way toward its bounding box's middle (a point on
/// a side counts as inside), or none.
#[test]
fn a_point_lies_in_the_first_hull_holding_it() {
    let mut w = open_world();
    let mut p = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        ObjectId::NOBODY,
        None,
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    let shifted = |dx: f32| Hull {
        points: square()
            .points
            .iter()
            .map(|q| q.with_x(q.x() + dx))
            .collect(),
        min: Point::new(dx, 0.0),
        max: Point::new(dx + 1.0, 1.0),
        ..square()
    };
    p.hulls = vec![Hull::default(), shifted(0.0), shifted(0.5)];
    assert_eq!(p.hull_of(Point::new(0.75, 0.5)), Some(HullIndex::new(1)));
    assert_eq!(p.hull_of(Point::new(1.25, 0.5)), Some(HullIndex::new(2)));
    // the left side of hull 1 and the right side of hull 2, nudged inward
    assert_eq!(p.hull_of(Point::new(0.0, 0.5)), Some(HullIndex::new(1)));
    assert_eq!(p.hull_of(Point::new(1.5, 0.5)), Some(HullIndex::new(2)));
    assert_eq!(p.hull_of(Point::new(1.75, 0.5)), None);
    assert_eq!(p.hull_of(Point::new(f32::NAN, 0.5)), None);
    p.hulls.clear();
    assert_eq!(p.hull_of(Point::new(0.75, 0.5)), None);
}

/// A tall object of class 10 (39 for a gate) without an obstruction record, owned by player 1.
fn tall(x: f32, y: f32, r: f32, class: ClassId) -> Placed {
    Placed {
        position: Position::new(x, y, 0.0),
        radius: Footprint::new(r, r),
        class,
        player: PlayerId::new(1),
        master: ids::ID_0,
        obstruction_type: ObstructionType::NONE,
        buildable_on: BuildableOn::NO,
        tall: true,
        moves: false,
        unknown: None,
        obstruction: None,
    }
}

/// The ids of each hull's boxes, hull by hull.
fn hull_ids(p: &HullPather<'_>) -> Vec<Vec<RecordId>> {
    p.hulls
        .iter()
        .map(|h| h.objects.iter().map(|o| o.id).collect())
        .collect()
}

/// `MShortPather::addTileObjects` (`FUN_140f8b460`): the tile's tall objects in its list's
/// order; slot 0x678 (`FUN_1411d4a30`) leaves out a gate the player may pass and, for player
/// -1, never does; the ignored and moving objects stay out unless included; a box at most
/// half a tile wide (`COMISS`/`JA`: an unordered one too) joins the given hull, a wider one
/// its own place; an id goes into a hull once.
#[test]
fn a_tile_s_tall_objects_go_into_the_hulls_unless_passed_or_left_out() {
    let mut objects = crate::world::index::TestObjects::default();
    let mut w = open_world();
    w.insert(object_id(10), tall(5.3, 5.3, 0.2, class::PREDATOR)); // the unit: ignored
    w.insert(object_id(1), tall(5.5, 5.5, 0.5, class::PREDATOR));
    w.insert(
        object_id(2),
        Placed {
            tall: false,
            ..tall(5.2, 5.2, 0.5, class::PREDATOR)
        },
    );
    w.insert(object_id(3), tall(5.5, 5.4, 0.5, class::GATE)); // a gate
    w.insert(object_id(5), tall(5.7, 5.7, 0.2, class::PREDATOR)); // moving
    objects.set_gait(
        object_id(5),
        Some(crate::map::spatial::Gait {
            moving: true,
            ..Default::default()
        }),
    );
    w.insert(object_id(6), tall(5.5, 5.5, 1.0, class::PREDATOR)); // wider than a tile
    w.insert(object_id(7), tall(8.5, 8.5, f32::NAN, class::PREDATOR));
    let far = Hull {
        points: vec![
            Point::new(1.0, 1.0),
            Point::new(1.0, 0.0),
            Point::new(0.0, 0.0),
            Point::new(0.0, 1.0),
        ],
        entrances: vec![None; 4],
        min: Point::new(0.0, 0.0),
        max: Point::new(1.0, 1.0),
        objects: Vec::new(),
    };
    // a gate for a player, without the world's slot 0x678
    let mut p = HullPather::new(
        (&mut w, objects.view()),
        object_id(10),
        Some(object_id(11)),
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    assert_eq!(p.player, Some(PlayerId::from(1)));
    p.hulls = vec![far.clone()];
    assert!(
        p.add_tile_objects(
            Tile::new(5, 5),
            Some(HullIndex::new(0)),
            Some(PlayerId::from(1))
        )
        .is_err()
    );
    // player 1 passes the gate
    let passes = BTreeSet::from([object_id(3)]);
    let mut p = HullPather::new(
        (&mut w, objects.view()),
        object_id(10),
        Some(object_id(11)),
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    )
    .with_gates(&passes);
    p.hulls = vec![far.clone()];
    p.add_tile_objects(
        Tile::new(5, 5),
        Some(HullIndex::new(0)),
        Some(PlayerId::from(1)),
    )
    .unwrap();
    assert_eq!(
        hull_ids(&p),
        [
            vec![RecordId::from(object_id(1))],
            vec![RecordId::from(object_id(6))]
        ]
    );
    assert_eq!(p.hulls[0].objects[0].bounds, Rect::new(4.8, 4.8, 6.2, 6.2));
    assert_eq!(p.hulls[1].objects[0].bounds, Rect::new(4.3, 4.3, 6.7, 6.7));
    // once per hull
    p.add_tile_objects(
        Tile::new(5, 5),
        Some(HullIndex::new(0)),
        Some(PlayerId::from(1)),
    )
    .unwrap();
    assert_eq!(
        hull_ids(&p),
        [
            vec![RecordId::from(object_id(1))],
            vec![RecordId::from(object_id(6))]
        ]
    );
    // player -1 passes no gate, and needs no slot 0x678; included objects go in
    let mut p = HullPather::new(
        (&mut w, objects.view()),
        object_id(10),
        Some(object_id(11)),
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    )
    .including(&[object_id(5), object_id(10)]);
    p.hulls = vec![far];
    p.add_tile_objects(Tile::new(5, 5), Some(HullIndex::new(0)), None)
        .unwrap();
    assert_eq!(
        hull_ids(&p),
        [
            vec![
                RecordId::from(object_id(10)),
                RecordId::from(object_id(1)),
                RecordId::from(object_id(3)),
                RecordId::from(object_id(5))
            ],
            vec![RecordId::from(object_id(6))]
        ]
    );
    // an unordered radius takes the given hull
    p.add_tile_objects(Tile::new(8, 8), Some(HullIndex::new(0)), None)
        .unwrap();
    assert_eq!(
        hull_ids(&p)[0],
        [
            RecordId::from(object_id(10)),
            RecordId::from(object_id(1)),
            RecordId::from(object_id(3)),
            RecordId::from(object_id(5)),
            RecordId::from(object_id(7))
        ]
    );
    // without a hull each box finds its own place: the wide box merges into the first's
    let mut p = HullPather::new(
        (&mut w, objects.view()),
        object_id(10),
        Some(object_id(11)),
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    )
    .with_gates(&passes);
    p.add_tile_objects(Tile::new(5, 5), None, Some(PlayerId::from(1)))
        .unwrap();
    assert_eq!(
        hull_ids(&p),
        [vec![
            RecordId::from(object_id(1)),
            RecordId::from(object_id(6))
        ]]
    );
}

/// `processCliffs` (`FUN_140f8e020`) over the frame two tiles about the area, held on the
/// map: rows y0 - 2..=y1 + 2 left and right of the area, then the rows above it, then those
/// below; each tile through `FUN_140f8a890`, which takes the objects of a cliff tile (word
/// `& 0x3f00` = 0x600) each to its own place, a gate too.
#[test]
fn the_cliffs_in_the_frame_about_the_area_go_into_the_hulls() {
    let mut tiles = Tiles::new(GridSize::new(20, 20), vec![4; 400]);
    let cliffs = [
        (3, 3),
        (6, 4),
        (6, 9),
        (9, 6),
        (6, 6),
        (2, 2),
        (8, 8),
        (19, 19),
    ];
    for (x, y) in cliffs {
        tiles.mark_for_test(Tile::new(x, y), 0x600);
    }
    // not a cliff: another class's bits
    tiles.mark_for_test(Tile::new(4, 6), 0x700);
    let mut w = Spatial::with_tiles(GridSize::new(20, 20), tiles);
    for (id, (x, y)) in (1..).zip(cliffs.into_iter().chain([(4, 6)])) {
        let class = if (x, y) == (9, 6) {
            class::GATE
        } else {
            class::PREDATOR
        };
        w.insert(
            object_id(id),
            tall(x as f32 + 0.5, y as f32 + 0.5, 0.1, class),
        );
    }
    let mut p = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        ObjectId::NOBODY,
        None,
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    p.area = TileRect::new(5, 5, 7, 7);
    p.process_cliffs().unwrap();
    // (3, 3) and (9, 6) by the side rows, (6, 4) above, (6, 9) below; (8, 8) is a corner of
    // the frame's side rows, after (9, 6)'s row
    assert_eq!(
        hull_ids(&p),
        [
            vec![RecordId::from(object_id(1))],
            vec![RecordId::from(object_id(4))],
            vec![RecordId::from(object_id(7))],
            vec![RecordId::from(object_id(2))],
            vec![RecordId::from(object_id(3))]
        ]
    );
    // at the map's edge the frame stops at the last tile
    let mut p = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        ObjectId::NOBODY,
        None,
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    p.area = TileRect::new(17, 17, 18, 18);
    p.process_cliffs().unwrap();
    assert_eq!(hull_ids(&p), [vec![RecordId::from(object_id(8))]]);
}

/// `processTileObstructions` (`FUN_140f8e1d0`): nothing off the map; a tile whose word holds a
/// bit the mask lacks (and not ignored) is a box `r` beyond the tile, into the given hull or
/// its own place, once per hull; then the tile's objects, then those of its building's anchor
/// tile when that lies outside the area.
#[test]
fn a_tile_s_obstructions_and_its_building_s_go_into_the_hulls() {
    let mut classes = vec![4; 400];
    classes[9 * 20 + 9] = 1;
    classes[3 * 20 + 3] = 1;
    let mut w = Spatial::with_tiles(
        GridSize::new(20, 20),
        Tiles::new(GridSize::new(20, 20), classes),
    );
    let mut v = crate::vision::visibility::Visibility::new(
        GridSize::new(20, 20),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        0,
        Vec::new(),
    );
    // a building of 2 × 2 tiles, 9..=10 each way, its anchor (10, 10)
    let _ = v.link_building(Position::new(10.0, 10.0, 0.0), Footprint::new(1.0, 1.0));
    assert_eq!(v.anchor_of(Tile::new(9, 9)), Some(Tile::new(10, 10)));
    w.sight = Some(v);
    w.insert(object_id(20), tall(10.0, 10.0, 1.0, class::PREDATOR)); // the building, on its anchor tile
    w.insert(object_id(21), tall(9.5, 9.5, 0.1, class::PREDATOR));
    let mut p = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        ObjectId::NOBODY,
        None,
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    p.process_tile(Tile::new(-1, 9), TileRect::new(5, 5, 9, 9), None)
        .unwrap();
    p.process_tile(Tile::new(20, 9), TileRect::new(5, 5, 9, 9), None)
        .unwrap();
    assert_eq!(p.hulls, []);
    // the tile's box, its object, then the anchor's building, the anchor outside the area
    p.process_tile(Tile::new(9, 9), TileRect::new(5, 5, 9, 9), None)
        .unwrap();
    assert_eq!(
        hull_ids(&p),
        [vec![
            tile_record_id(Tile::new(9, 9)),
            RecordId::from(object_id(21)),
            RecordId::from(object_id(20))
        ]]
    );
    assert_eq!(
        p.hulls[0].objects[0].bounds,
        Rect::new(8.8, 8.8, 10.2, 10.2)
    );
    // the anchor inside the area: its tile's own turn takes the building
    let mut p = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        ObjectId::NOBODY,
        None,
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    p.process_tile(Tile::new(9, 9), TileRect::new(5, 5, 10, 10), None)
        .unwrap();
    assert_eq!(
        hull_ids(&p),
        [vec![
            tile_record_id(Tile::new(9, 9)),
            RecordId::from(object_id(21))
        ]]
    );
    // into a given hull, once; an ignored tile's box stays out
    let mut p = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        ObjectId::NOBODY,
        None,
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    p.hulls = vec![Hull::default()];
    p.process_tile(
        Tile::new(3, 3),
        TileRect::new(0, 0, 5, 5),
        Some(HullIndex::new(0)),
    )
    .unwrap();
    p.process_tile(
        Tile::new(3, 3),
        TileRect::new(0, 0, 5, 5),
        Some(HullIndex::new(0)),
    )
    .unwrap();
    assert_eq!(hull_ids(&p), [vec![tile_record_id(Tile::new(3, 3))]]);
    p.ignore.push(tile_record_id(Tile::new(9, 9)));
    p.process_tile(
        Tile::new(9, 9),
        TileRect::new(5, 5, 10, 10),
        Some(HullIndex::new(0)),
    )
    .unwrap();
    assert_eq!(
        hull_ids(&p),
        [vec![
            tile_record_id(Tile::new(3, 3)),
            RecordId::from(object_id(21))
        ]]
    );
}

/// `MShortPather::buildHulls` (`FUN_140f8b8f0`): no hulls, the area held on the map, then row
/// by row: a tile the unit cannot cross (its word holds a bit the mask lacks, or the static
/// bit 14 without class 5) starts or continues a run; an open tile ends the run (one box from
/// its start to the open tile, then each run tile into its hull) before the open tile's own
/// obstructions go each to its own place; a run at the row's end ends at the area's last
/// column plus one.
#[test]
fn the_area_s_runs_of_blocked_tiles_make_one_hull_each() {
    let mut objects = crate::world::index::TestObjects::default();
    let mut classes = vec![4; 400];
    for x in [3, 4, 5, 9, 10] {
        classes[5 * 20 + x] = 1;
    }
    let mut tiles = Tiles::new(GridSize::new(20, 20), classes);
    // a gate's tile (class 5) is open, another static record's is not
    tiles.mark_for_test(Tile::new(7, 5), 0x4000 | 0x500);
    tiles.mark_for_test(Tile::new(8, 5), 0x4000 | 0x200);
    let mut w = Spatial::with_tiles(GridSize::new(20, 20), tiles);
    w.sight = Some(crate::vision::visibility::Visibility::new(
        GridSize::new(20, 20),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        0,
        Vec::new(),
    ));
    w.insert(object_id(30), tall(1.5, 5.5, 0.3, class::PREDATOR));
    w.insert(object_id(31), tall(4.5, 5.5, 0.2, class::PREDATOR));
    w.insert(object_id(32), tall(7.5, 5.5, 0.2, class::PREDATOR));
    // moving: out unless included (`FindShortPath`'s arguments 13 and 14, passed on)
    w.insert(object_id(33), tall(0.5, 5.5, 0.2, class::PREDATOR));
    objects.set_gait(
        object_id(33),
        Some(crate::map::spatial::Gait {
            moving: true,
            ..Default::default()
        }),
    );
    let mut p = HullPather::new(
        (&mut w, objects.view()),
        ObjectId::NOBODY,
        None,
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    p.hulls = vec![Hull::default()];
    p.area = TileRect::new(-1, 5, 9, 5);
    p.build_hulls().unwrap();
    assert_eq!(p.area, TileRect::new(0, 5, 9, 5));
    let t = |x: i32| tile_record_id(Tile::new(x, 5));
    // the run 8..10 (the area ends at 9) overlaps object 32's hull and joins it; its tile 8
    // is blocked by its static bit alone, which the tile's box test (the mask's byte) does
    // not see
    assert_eq!(
        hull_ids(&p),
        [
            vec![RecordId::from(object_id(30))],
            vec![t(3), t(4), RecordId::from(object_id(31)), t(5)],
            vec![RecordId::from(object_id(32)), t(9)]
        ]
    );
    assert_eq!(
        (p.hulls[1].min, p.hulls[1].max),
        (Point::new(2.8, 4.8), Point::new(6.2, 6.2))
    );
    let x0 = (7.5f32 - 0.2) - 0.2;
    assert_eq!(
        (p.hulls[2].min, p.hulls[2].max),
        (Point::new(x0, 4.8), Point::new(10.2, 6.2))
    );
    let mut p = HullPather::new(
        (&mut w, objects.view()),
        ObjectId::NOBODY,
        None,
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    )
    .including(&[object_id(33)]);
    p.area = TileRect::new(0, 5, 2, 5);
    p.build_hulls().unwrap();
    assert_eq!(
        hull_ids(&p),
        [
            vec![RecordId::from(object_id(33))],
            vec![RecordId::from(object_id(30))]
        ]
    );
}

/// `MShortPather::buildPath` (`FUN_140f8bcd0`): from the end back, a node whose walk to its
/// grandparent meets nothing (`FUN_141052b90`, `FUN_141052350` with flags 0x40) skips its
/// parent and tests again; one whose walk is blocked hands on to its parent. Then each node
/// from the end goes first into the path (`FUN_140f95a50`, `InsertWaypoint` at 0).
#[test]
fn a_path_skips_the_parents_its_nodes_see_past() {
    let mut classes = vec![4; 400];
    // blocks the walk (8, 5) -> (2, 2) alone
    classes[3 * 20 + 4] = 1;
    let mut w = Spatial::with_tiles(
        GridSize::new(20, 20),
        Tiles::new(GridSize::new(20, 20), classes),
    );
    let mut p = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        ObjectId::NOBODY,
        None,
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    let n0 = p
        .add_unrefused(UnrefusedNode {
            list: NodeList::SEARCH,
            open: false,
            parent: None,
            position: Point::new(2.0, 2.0),
            goal: None,
            hull: -1,
            direction: -1,
        })
        .unwrap();
    let n1 = p
        .add_unrefused(UnrefusedNode {
            list: NodeList::SEARCH,
            open: false,
            parent: Some(n0),
            position: Point::new(5.0, 2.0),
            goal: None,
            hull: -1,
            direction: -1,
        })
        .unwrap();
    let n2 = p
        .add_unrefused(UnrefusedNode {
            list: NodeList::SEARCH,
            open: false,
            parent: Some(n1),
            position: Point::new(5.0, 5.0),
            goal: None,
            hull: -1,
            direction: -1,
        })
        .unwrap();
    let n3 = p
        .add_unrefused(UnrefusedNode {
            list: NodeList::SEARCH,
            open: false,
            parent: Some(n2),
            position: Point::new(8.0, 5.0),
            goal: None,
            hull: -1,
            direction: -1,
        })
        .unwrap();
    let mut path = Waypoints::default();
    p.build_path(&mut path, n3).unwrap();
    // n3 sees n1 (n2 skipped), not n0; n1 has no grandparent
    assert_eq!(
        path.points,
        [
            Point::new(2.0, 2.0),
            Point::new(5.0, 2.0),
            Point::new(8.0, 5.0)
        ]
    );
    assert_eq!(p.nodes[n3].parent, Some(n1));
    assert_eq!(p.nodes[n1].parent, Some(n0));
    // a lone node is a path of one point
    let mut one = Waypoints::default();
    p.build_path(&mut one, n0).unwrap();
    assert_eq!(one.points, [Point::new(2.0, 2.0)]);
}

/// Slot 0x3e0 (`FUN_1411506e0`): the quick path alone from the unit's place to the target's,
/// ignoring both; a result below 2 reaches, with the path's length (its `param_4`); none for a
/// target the world does not hold or a quick path that is blocked.
#[test]
fn a_unit_s_quick_reach_is_its_quick_path_alone() {
    let mut w = open_world();
    w.insert(
        object_id(9171),
        placed(2.0, 5.5, 0.2, ObstructionType::CIRCLE),
    );
    w.insert(
        object_id(48),
        placed(5.5, 5.5, 0.5, ObstructionType::TYPE_2),
    );
    w.insert(
        object_id(9149),
        placed(10.5, 5.5, 0.5, ObstructionType::TYPE_2),
    );
    let by = 5.5 - 0.7 * SQRT_2;
    let length = quick_reach_from(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        object_id(9171),
        Point::new(2.0, 5.5),
        Some(object_id(9149)),
        TerrainMask::of_bits(6),
        Distance::new(0.2),
        Distance::new(0.1),
    )
    .unwrap()
    .unwrap();
    let expected = (3.5f32 * 3.5 + (5.5 - by) * (5.5 - by)).sqrt()
        + (5.1f32 * 5.1 + (5.5 - by) * (5.5 - by)).sqrt();
    assert!(
        (length.get() - expected).abs() < 1e-4,
        "{length} {expected}"
    );
    assert!(
        quick_reach(
            (&mut w, crate::world::index::ObjectsRef::empty()),
            object_id(9171),
            Some(object_id(9149)),
            TerrainMask::of_bits(6),
            Distance::new(0.2),
            Distance::new(0.1)
        )
        .unwrap()
    );
    // within range already: no path, length 0
    assert_eq!(
        quick_reach_from(
            (&mut w, crate::world::index::ObjectsRef::empty()),
            object_id(9171),
            Point::new(9.75, 5.5),
            Some(object_id(9149)),
            TerrainMask::of_bits(6),
            Distance::new(0.2),
            Distance::new(0.1)
        )
        .unwrap(),
        Some(Distance::ZERO)
    );
    // a target the world does not hold
    assert!(
        !quick_reach(
            (&mut w, crate::world::index::ObjectsRef::empty()),
            object_id(9171),
            Some(object_id(777)),
            TerrainMask::of_bits(6),
            Distance::new(0.2),
            Distance::new(0.1)
        )
        .unwrap()
    );
    // a second obstruction in the way: the quick path is blocked (3)
    w.insert(
        object_id(49),
        placed(7.5, 5.5, 0.5, ObstructionType::TYPE_2),
    );
    assert_eq!(
        quick_reach_from(
            (&mut w, crate::world::index::ObjectsRef::empty()),
            object_id(9171),
            Point::new(2.0, 5.5),
            Some(object_id(9149)),
            TerrainMask::of_bits(6),
            Distance::new(0.2),
            Distance::new(0.1)
        )
        .unwrap(),
        None
    );
}

/// `FUN_140f84e40`: no hulls, both open lists empty behind their heads, empty closed lists,
/// the area -1, bounds 0, four empty exit and entrance paths; the iteration cap 0x40. With
/// `FUN_140f89e40`'s ignore list: the unit and the target.
#[test]
fn a_new_pather_holds_the_constructor_s_state() {
    let mut w = open_world();
    w.insert(
        object_id(9171),
        placed(2.0, 5.5, 0.2, ObstructionType::CIRCLE),
    );
    let p = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        object_id(9171),
        Some(object_id(9149)),
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    assert_eq!(ITERATION_CAP, 0x40);
    assert_eq!(
        (p.player, p.ignore.clone()),
        (
            Some(PlayerId::from(1)),
            vec![
                RecordId::from(object_id(9171)),
                RecordId::from(object_id(9149))
            ]
        )
    );
    assert_eq!(p.hulls, []);
    assert_eq!(p.nodes.0.len(), 2);
    for head in &p.nodes.0 {
        assert_eq!((head.parent, head.next, head.prev), (None, None, None));
    }
    assert_eq!(p.closed.0, [Vec::<NodeIndex>::new(), Vec::new()]);
    assert_eq!((p.area, p.bounds), (NO_AREA, Rect::default()));
    assert_eq!(p.exits, vec![Waypoints::default(); PATHS]);
    assert_eq!(p.entrances, vec![Waypoints::default(); PATHS]);
    assert_eq!(
        (p.buffer, p.include.len(), p.gates.is_none()),
        (None, 0, true)
    );
}

/// The pather's float tests as the game compiles them: the area test skips by `COMISS`/`JA`
/// (an unordered place is inside, 0x140f86a44), the target's gap clamps each axis by `MAXSS`
/// (an unordered one is 0, 0x140f86bcf), a walk within a hull picks its sides by `JA`/`SETBE`/
/// `CMOVA` (an unordered start is right of and below the box, 0x140f8d1fc) and keeps a side
/// point unless it is farther than the reach (`JA`, 0x140f8d620), and the end's partway test
/// takes an unordered distance as nearer the goal (`JC`, 0x140f876ca).
#[test]
fn the_pather_s_float_tests_take_the_game_s_nan_paths() {
    let mut w = open_world();
    let mut p = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        ObjectId::NOBODY,
        None,
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    p.area = TileRect::new(2, 2, 5, 5);
    assert!(p.in_area(Point::new(f32::NAN, 3.0)));
    assert!(p.in_area(Point::new(2.2, 5.8)));
    assert!(!p.in_area(Point::new(2.1, 3.0)));
    let t = Target {
        at: Point::new(10.0, 10.0),
        radius: Footprint::new(0.5, 0.5),
        moves: false,
    };
    let gy = (13.7f32 - 10.0).abs() - (0.5 + 0.2);
    assert_eq!(
        p.target_gap(&t, Point::new(f32::NAN, 13.7)),
        Distance::new(gy)
    );
    assert_eq!(p.target_gap(&t, Point::new(10.2, 10.3)), Distance::ZERO);
    // an unordered start: the box's right and top sides (case 8); an unordered reach leaves
    // no side point out
    let b = Rect::new(4.0, 4.0, 6.0, 6.0);
    p.hulls = vec![Hull::default()];
    let root = p
        .add_unrefused(UnrefusedNode {
            list: NodeList::WALKS,
            open: true,
            parent: None,
            position: Point::new(5.0, 5.0),
            goal: None,
            hull: -1,
            direction: -1,
        })
        .unwrap();
    assert!(
        p.side_points(
            HullIndex::new(0),
            root,
            b,
            Point::new(f32::NAN, f32::NAN),
            0.5,
            None
        )
        .unwrap()
    );
    let mut at: Vec<Point> = open_nodes(&p, NodeList::WALKS)
        .iter()
        .filter(|&&i| i != root)
        .map(|&i| p.nodes[i].at)
        .collect();
    at.sort_by(|a, b| a.get().partial_cmp(&b.get()).unwrap());
    let n = NUDGE;
    let mut expected = vec![
        Point::new(6.0 + n, 4.0 + n),
        Point::new(6.0 + n, (4.0 + n) + 1.0),
        Point::new(6.0 - n, 6.0 + n),
        Point::new((6.0 - n) - 1.0, 6.0 + n),
    ];
    expected.sort_by(|a, b| a.get().partial_cmp(&b.get()).unwrap());
    assert_eq!(at, expected);
    p.empty(NodeList::WALKS).unwrap();
    let root = p
        .add_unrefused(UnrefusedNode {
            list: NodeList::WALKS,
            open: true,
            parent: None,
            position: Point::new(5.0, 3.0),
            goal: None,
            hull: -1,
            direction: -1,
        })
        .unwrap();
    let reach = Some((Point::new(5.0, 3.0), f32::NAN));
    assert!(
        p.side_points(HullIndex::new(0), root, b, Point::new(5.0, 3.0), 0.5, reach)
            .unwrap()
    );
    assert_eq!(open_nodes(&p, NodeList::WALKS).len(), 3);
    // partway: an unordered end is nearer the goal than the start, and not within range
    let mut path = Waypoints {
        points: vec![Point::new(1.0, 1.0), Point::new(f32::NAN, 2.0)],
        cost: Distance::ZERO,
    };
    p.start = Point::new(1.0, 1.0);
    assert_eq!(
        p.partway(
            &mut path,
            Point::new(1.0, 1.0),
            Point::new(9.0, 9.0),
            Distance::new(0.5)
        ),
        ShortFound::Partial
    );
    assert_eq!(path.points.len(), 2);
}

/// A 20 × 20 map whose tiles (6, 4..=6) or (8, 4..=6) are of a terrain the mask 6 lacks (a
/// wall), with the map's links, and the unit 9171 and its target 9149 (a tree's size) at
/// (12.5, 5.5).
fn walled(wall_x: usize, unit: Point) -> Spatial {
    let mut classes = vec![4; 400];
    for y in 4..=6 {
        classes[y * 20 + wall_x] = 1;
    }
    let mut w = Spatial::with_tiles(
        GridSize::new(20, 20),
        Tiles::new(GridSize::new(20, 20), classes),
    );
    w.sight = Some(crate::vision::visibility::Visibility::new(
        GridSize::new(20, 20),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        0,
        Vec::new(),
    ));
    w.insert(
        object_id(9171),
        placed(unit.x(), unit.y(), 0.2, ObstructionType::CIRCLE),
    );
    w.insert(
        object_id(9149),
        placed(12.5, 5.5, 0.5, ObstructionType::TYPE_2),
    );
    w
}

/// `FindShortPath` (`FUN_140f85580`) when the quick path meets more than one obstruction:
/// the area about the walk (2 tiles each way), its hulls (the wall's three rows merged, the
/// target's marked tile a hull of its own), the target's entrance paths joined to its hull,
/// then the search from the start: its walk crosses the wall's hull, whose points it follows
/// both ways; the first node within range of the target's grown box ends it, and the path
/// skips every node that sees past its parent.
#[test]
fn the_hull_search_goes_around_a_wall_to_the_target() {
    let mut w = walled(6, Point::new(2.0, 5.5));
    let mut p = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        object_id(9171),
        Some(object_id(9149)),
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    let (result, path) = p
        .find(
            Point::new(2.0, 5.5),
            Point::new(12.5, 5.5),
            Some(object_id(9149)),
            Distance::new(0.1),
        )
        .unwrap();
    assert_eq!(p.area, TileRect::new(0, 3, 14, 7));
    assert_eq!(
        p.hulls[0].points,
        [
            Point::new(7.2, 7.2),
            Point::new(7.2, 3.8),
            Point::new(5.8, 3.8),
            Point::new(5.8, 7.2)
        ]
    );
    assert_eq!(p.hulls[0].objects.len(), 3);
    // the target's hull took the four entrance ends on its left side, by their distance from
    // the side's first point (11.8, 4.8)
    let x = 11.8 - NUDGE;
    let e = |i: usize| Some(EntranceIndex(i));
    assert_eq!(
        p.hulls[1].entrances,
        [None, None, None, e(2), e(0), e(1), e(3), None]
    );
    assert_eq!(p.hulls[1].points[3].x(), x);
    assert_eq!(result, ShortFound::Reached);
    assert_eq!(
        path.points[..3],
        [
            Point::new(2.0, 5.5),
            Point::new(5.8, 7.2),
            Point::new(7.2, 7.2)
        ]
    );
    let end = path.points[3];
    assert_eq!((path.points.len(), end.x()), (4, x));
    assert!((end.y() - (6.2 - NUDGE - 0.35)).abs() < 1e-5);
}

/// A start inside a hull: `pathWithinHull` out of it (`FUN_140f8cf00`) gives the exit paths,
/// whose nodes but the last go closed and the last open into the search's list; the goal's
/// hull its entrance paths (`FUN_140f88ee0`) with the target.
#[test]
fn a_start_inside_a_hull_leaves_it_by_its_exit_paths() {
    let mut w = walled(8, Point::new(4.9, 5.5));
    w.insert(
        object_id(48),
        placed(5.5, 5.5, 0.5, ObstructionType::TYPE_2),
    );
    let mut p = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        object_id(9171),
        Some(object_id(9149)),
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    let (result, path) = p
        .find(
            Point::new(4.9, 5.5),
            Point::new(12.5, 5.5),
            Some(object_id(9149)),
            Distance::new(0.1),
        )
        .unwrap();
    let ends: Vec<Point> = p.exits.iter().map(|e| e.points[2]).collect();
    let n = 4.8 - NUDGE;
    assert_eq!(
        ends,
        [
            Point::new(n, 5.5),
            Point::new(n, 5.541_421_4),
            Point::new(n, 5.458_578_6),
            Point::new(n, 5.6)
        ]
    );
    let starts: Vec<Point> = p.entrances.iter().map(|e| e.points[0]).collect();
    assert_eq!(starts, [Point::new(12.5, 5.5); 4]);
    assert_eq!(result, ShortFound::Reached);
    assert_eq!(
        path.points[..5],
        [
            Point::new(4.9, 5.5),
            Point::new(4.8, 5.6),
            Point::new(4.8, 6.2),
            Point::new(7.8, 7.2),
            Point::new(9.2, 7.2)
        ]
    );
}

/// `pathWithinHull` out of a hull (`FUN_140f8cf00`). Without a goal: from inside a box, the
/// 16 first steps (each 2r, held at the hull's side) and the points of the box's sides the
/// start's region picks, within 2r; each ray that leaves the boxes ends a way out at the
/// hull's side, nudged out, kept by its end (result 5 from inside a box). With a goal: a node
/// that sees the goal ends the walk there (0).
#[test]
fn a_walk_out_of_a_hull_keeps_its_ways_out_or_reaches_the_goal() {
    let mut w = walled(8, Point::new(4.9, 5.5));
    let box_hull = |p: &mut HullPather<'_>| {
        p.area = TileRect::new(0, 0, 19, 19);
        let h = p
            .add_shape(Point::new(4.8, 4.8), Point::new(6.2, 6.2))
            .unwrap();
        p.hulls[h.index()].objects.push(Shape {
            bounds: Rect::new(4.8, 4.8, 6.2, 6.2),
            id: RecordId::from(object_id(48)),
        });
        h
    };
    let mut p = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        object_id(9171),
        Some(object_id(9149)),
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    let h = box_hull(&mut p);
    let mut paths = vec![Waypoints::default(); PATHS];
    let mut count = 0;
    let walk = p
        .walk_out(WalkOut {
            hull: h,
            start: Point::new(4.9, 5.5),
            goal: None,
            target: None,
            range: Distance::new(0.1),
            nudge: true,
            paths: &mut paths,
            most: PATHS,
            count: &mut count,
        })
        .unwrap();
    assert_eq!(
        (walk.result, walk.left_boxes, count),
        (WalkResult::WaysFromBox, true, 4)
    );
    let n = 4.8 - NUDGE;
    assert_eq!(paths[0].points, [Point::new(4.9, 5.5), Point::new(n, 5.5)]);
    assert_eq!(paths[3].points, [Point::new(4.9, 5.5), Point::new(n, 5.6)]);
    assert!(paths[0].cost < paths[1].cost && paths[1].cost <= paths[3].cost);
    // toward a goal it sees from its first step
    let mut p = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        object_id(9171),
        Some(object_id(9149)),
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    let h = box_hull(&mut p);
    let mut paths = vec![Waypoints::default(); 1];
    let mut count = 0;
    let goal = Some(Point::new(3.0, 5.5));
    let walk = p
        .walk_out(WalkOut {
            hull: h,
            start: Point::new(4.9, 5.5),
            goal,
            target: None,
            range: Distance::new(0.1),
            nudge: false,
            paths: &mut paths,
            most: 1,
            count: &mut count,
        })
        .unwrap();
    assert_eq!(
        (walk.result, walk.left_boxes, count),
        (WalkResult::Reached, false, 0)
    );
    assert_eq!(
        paths[0].points,
        [Point::new(4.9, 5.5), Point::new(3.0, 5.5)]
    );
}

/// The rays of a walk out's node ([`HullPather::cast_rays`]) at (4.2, 5.5), inside a hull of
/// (4, 4) to (7, 7) and left of its box: the ray to the right meets the box and the walk goes
/// on from its side; the other three leave the boxes and end on the hull's edge. Without a goal
/// each of those is a way out nudged past the edge, kept while fewer than `most` are; toward a
/// goal the walk goes on from each end too, keeping none.
#[test]
fn a_walk_out_s_rays_end_on_the_hull_s_edge_as_ways_out() {
    let mut w = walled(8, Point::new(4.9, 5.5));
    let mut rays = |goal: Option<Point>, most| {
        let mut p = HullPather::new(
            (&mut w, crate::world::index::ObjectsRef::empty()),
            object_id(9171),
            Some(object_id(9149)),
            TerrainMask::of_bits(6),
            Distance::new(0.2),
        );
        p.area = TileRect::new(0, 0, 19, 19);
        let h = p
            .add_shape(Point::new(4.0, 4.0), Point::new(7.0, 7.0))
            .unwrap();
        p.hulls[h.index()].objects.push(Shape {
            bounds: Rect::new(4.8, 4.8, 6.2, 6.2),
            id: RecordId::from(object_id(48)),
        });
        p.empty(NodeList::WALKS).unwrap();
        let root = p
            .add_unrefused(UnrefusedNode {
                list: NodeList::WALKS,
                open: true,
                parent: None,
                position: Point::new(4.2, 5.5),
                goal,
                hull: -1,
                direction: -1,
            })
            .unwrap();
        p.close(NodeList::WALKS, root).unwrap();
        let mut paths = vec![Waypoints::default(); PATHS];
        let mut count = 0;
        let left = p
            .cast_rays(h, root, goal, true, &mut paths, (&mut count, most))
            .unwrap();
        let ends: Vec<Point> = paths[..count]
            .iter()
            .map(|w| *w.points.last().unwrap())
            .collect();
        let mut open = 0;
        let mut next = p.nodes[NodeList::WALKS.head()].next;
        while let Some(n) = next {
            open += 1;
            next = p.nodes[n].next;
        }
        (left, ends, open)
    };
    let n = NUDGE;
    let (left, ends, open) = rays(None, PATHS);
    assert_eq!((left, open), (true, 1));
    assert_eq!(
        ends,
        [
            Point::new(4.2, 4.0 - n),
            Point::new(4.2, 7.0 + n),
            Point::new(4.0 - n, 5.5)
        ]
    );
    assert_eq!(
        rays(None, 2).1,
        [Point::new(4.2, 4.0 - n), Point::new(4.2, 7.0 + n)]
    );
    let (left, ends, open) = rays(Some(Point::new(3.0, 5.5)), PATHS);
    assert_eq!((left, ends.len(), open), (true, 0, 4));
}

/// `pathWithinHull` out of a hull (`FUN_140f8cf00`) from (4.2, 5.5), inside the hull of (4, 4)
/// to (7, 7) and outside its box: without a goal its ways out are the answer (result 4: the
/// start was in no box), none kept is result 2; toward a goal within the unit's radius and range
/// of the start the start ends the walk (0).
#[test]
fn a_walk_out_from_outside_its_boxes_ends_by_its_ways_out_or_at_a_goal_in_range() {
    let mut w = walled(8, Point::new(4.9, 5.5));
    let mut walk = |goal: Option<Point>, range, most| {
        let mut p = HullPather::new(
            (&mut w, crate::world::index::ObjectsRef::empty()),
            object_id(9171),
            Some(object_id(9149)),
            TerrainMask::of_bits(6),
            Distance::new(0.2),
        );
        p.area = TileRect::new(0, 0, 19, 19);
        let h = p
            .add_shape(Point::new(4.0, 4.0), Point::new(7.0, 7.0))
            .unwrap();
        p.hulls[h.index()].objects.push(Shape {
            bounds: Rect::new(4.8, 4.8, 6.2, 6.2),
            id: RecordId::from(object_id(48)),
        });
        let mut paths = vec![Waypoints::default(); PATHS];
        let mut count = 0;
        let walk = p
            .walk_out(WalkOut {
                hull: h,
                start: Point::new(4.2, 5.5),
                goal,
                target: None,
                range,
                nudge: true,
                paths: &mut paths,
                most,
                count: &mut count,
            })
            .unwrap();
        (walk.result, walk.left_boxes, count, paths[0].points.clone())
    };
    let (result, left, count, _) = walk(None, Distance::new(0.1), PATHS);
    assert_eq!((result, left), (WalkResult::Ways, true));
    assert!(0 < count);
    assert_eq!(
        walk(None, Distance::new(0.1), 0),
        (WalkResult::NoWay, false, 0, vec![])
    );
    let (result, _, count, points) = walk(Some(Point::new(4.4, 5.5)), Distance::new(0.5), 1);
    assert_eq!(
        (result, count, points),
        (WalkResult::Reached, 0, vec![Point::new(4.2, 5.5)])
    );
}

/// `pathWithinHull` into a hull (`FUN_140f88ee0`): the walk out of the hull from the goal
/// inside a box toward the start, its ways out nudged back in by [`NUDGE`], each with the hull
/// side it leaves by (result 5: the goal is inside a box).
#[test]
fn a_walk_into_a_hull_keeps_its_entrances_and_their_sides() {
    let mut w = walled(8, Point::new(4.9, 5.5));
    let mut p = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        object_id(9171),
        Some(object_id(9149)),
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    p.area = TileRect::new(0, 0, 19, 19);
    let h = p
        .add_shape(Point::new(4.8, 4.8), Point::new(6.2, 6.2))
        .unwrap();
    p.hulls[h.index()].objects.push(Shape {
        bounds: Rect::new(4.8, 4.8, 6.2, 6.2),
        id: RecordId::from(object_id(48)),
    });
    let mut paths = vec![Waypoints::default(); PATHS];
    let mut sides = [0usize; PATHS];
    let mut count = 0;
    let walk = p
        .walk_in(WalkIn {
            hull: h,
            goal: Point::new(5.0, 5.5),
            start: Point::new(2.0, 5.5),
            target: None,
            paths: &mut paths,
            sides_out: &mut sides,
            most: PATHS,
            count: &mut count,
        })
        .unwrap();
    assert_eq!(
        (walk.result, walk.left_boxes, count),
        (WalkResult::WaysFromBox, true, 4)
    );
    // all four leave by side 3, the hull's left (its points 2 to 3)
    assert_eq!(sides, [3; 4]);
    let x = 4.8 + NUDGE;
    // the world holds no record in the box: the end sees the goal, its parent is skipped
    assert_eq!(paths[0].points, [Point::new(5.0, 5.5), Point::new(x, 5.5)]);
}

/// `FindShortPath` with a goal off the map less the radius: its target must stand on it and
/// the goal lie on the map (`FUN_140f89d80` with the map's bounds), else 4.
#[test]
fn a_goal_at_the_map_s_edge_needs_its_target_on_it() {
    let mut w = open_world();
    w.insert(
        object_id(9171),
        placed(15.0, 5.5, 0.2, ObstructionType::CIRCLE),
    );
    w.insert(
        object_id(9149),
        placed(19.9, 5.5, 0.5, ObstructionType::TYPE_2),
    );
    w.insert(
        object_id(9150),
        placed(20.5, 7.5, 0.5, ObstructionType::TYPE_2),
    );
    let mut p = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        object_id(9171),
        Some(object_id(9149)),
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    let (result, path) = p
        .find(
            Point::new(15.0, 5.5),
            Point::new(19.9, 5.5),
            Some(object_id(9149)),
            Distance::ZERO,
        )
        .unwrap();
    assert_eq!(
        (result, path.points),
        (
            ShortFound::Reached,
            vec![Point::new(15.0, 5.5), Point::new(19.9, 5.5)]
        )
    );
    assert_eq!(p.bounds, Rect::new(0.2, 0.2, 19.8, 19.8));
    // no target, a target elsewhere, a goal off the map
    assert_eq!(
        p.find(
            Point::new(15.0, 5.5),
            Point::new(19.9, 5.5),
            None,
            Distance::ZERO
        )
        .unwrap()
        .0,
        ShortFound::Invalid
    );
    assert_eq!(
        p.find(
            Point::new(15.0, 5.5),
            Point::new(19.9, 6.5),
            Some(object_id(9149)),
            Distance::ZERO
        )
        .unwrap()
        .0,
        ShortFound::Invalid
    );
    assert_eq!(
        p.find(
            Point::new(15.0, 5.5),
            Point::new(20.5, 7.5),
            Some(object_id(9150)),
            Distance::ZERO
        )
        .unwrap()
        .0,
        ShortFound::Invalid
    );
    assert_eq!(p.bounds, Rect::new(0.0, 0.0, 20.0, 20.0));
}

/// `FindShortPath`'s argument 11: the last search's area and hulls stay, none are built; with
/// no area yet the search makes its own.
#[test]
fn a_kept_search_reuses_the_last_area_and_hulls() {
    let mut w = walled(6, Point::new(2.0, 5.5));
    let mut p = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        object_id(9171),
        Some(object_id(9149)),
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    let first = p
        .find_keeping(
            Point::new(2.0, 5.5),
            Point::new(12.5, 5.5),
            Some(object_id(9149)),
            Distance::new(0.1),
        )
        .unwrap();
    assert_eq!(p.area, TileRect::new(0, 3, 14, 7));
    // one pather borrows the world at a time: the kept state goes on to the next
    let kept = p.kept();
    let mut fresh = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        object_id(9171),
        Some(object_id(9149)),
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    assert_eq!(
        first,
        fresh
            .find(
                Point::new(2.0, 5.5),
                Point::new(12.5, 5.5),
                Some(object_id(9149)),
                Distance::new(0.1)
            )
            .unwrap()
    );
    let mut p = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        object_id(9171),
        Some(object_id(9149)),
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    )
    .with_kept(kept);
    // the hulls gone: a kept search builds none and finds no way around the wall
    p.hulls.clear();
    p.area = TileRect::new(0, 2, 14, 8);
    let (result, _) = p
        .find_keeping(
            Point::new(2.0, 5.5),
            Point::new(12.5, 5.5),
            Some(object_id(9149)),
            Distance::new(0.1),
        )
        .unwrap();
    assert_eq!(
        (result, p.area, p.hulls.len()),
        (ShortFound::Blocked, TileRect::new(0, 2, 14, 8), 0)
    );
    // not kept: the area and the hulls anew
    let (result, _) = p
        .find(
            Point::new(2.0, 5.5),
            Point::new(12.5, 5.5),
            Some(object_id(9149)),
            Distance::new(0.1),
        )
        .unwrap();
    assert_eq!(
        (result, p.area, p.hulls.len()),
        (ShortFound::Reached, TileRect::new(0, 3, 14, 7), 2)
    );
}

/// `FindShortPath`'s search meets a node beyond the area: the area grows by two tiles each
/// way on the map (at most 100 tiles, `GROWTHS` times), its tiles' obstructions join the
/// hulls and the search starts again in it.
#[test]
fn a_node_beyond_the_area_grows_it_once() {
    let mut classes = vec![4; 400];
    for y in 3..=8 {
        classes[y * 20 + 6] = 1;
    }
    let mut w = Spatial::with_tiles(
        GridSize::new(20, 20),
        Tiles::new(GridSize::new(20, 20), classes),
    );
    w.sight = Some(crate::vision::visibility::Visibility::new(
        GridSize::new(20, 20),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        0,
        Vec::new(),
    ));
    w.insert(
        object_id(9171),
        placed(5.0, 5.5, 0.2, ObstructionType::CIRCLE),
    );
    w.insert(
        object_id(9149),
        placed(8.5, 5.5, 0.5, ObstructionType::TYPE_2),
    );
    let mut p = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        object_id(9171),
        Some(object_id(9149)),
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    let (result, path) = p
        .find(
            Point::new(5.0, 5.5),
            Point::new(8.5, 5.5),
            Some(object_id(9149)),
            Distance::new(0.1),
        )
        .unwrap();
    // the first area (3..=10, 3..=7) held the wall's rows 3..=7; the corner (5.8, 2.8) lies
    // beyond it: grown, the area took row 8 into the wall's hull
    assert_eq!(p.area, TileRect::new(1, 1, 12, 9));
    assert_eq!(
        (p.hulls[0].min, p.hulls[0].max),
        (Point::new(5.8, 2.8), Point::new(7.2, 9.2))
    );
    assert_eq!(result, ShortFound::Reached);
    assert_eq!(
        path.points,
        [
            Point::new(5.0, 5.5),
            Point::new(5.8, 2.8),
            Point::new(7.2, 2.8),
            Point::new(7.8, 4.8)
        ]
    );
}

/// `FUN_140f89e40` with any list: the ids in turn (unknown ones too), then the tile of each
/// listed object of radii 0.5 × 0.5 on a tile's centre whose word holds a terrain class the
/// mask lacks; one off the centre, of another size or on an open tile adds none.
#[test]
fn the_ignore_list_takes_the_ids_then_the_tiles_of_one_tile_objects_on_a_centre() {
    let mut classes = vec![4; 400];
    classes[3 * 20 + 3] = 1;
    classes[7 * 20 + 7] = 1;
    let mut w = Spatial::with_tiles(
        GridSize::new(20, 20),
        Tiles::new(GridSize::new(20, 20), classes),
    );
    w.insert(object_id(1), placed(3.5, 3.5, 0.5, ObstructionType::TYPE_2));
    w.insert(object_id(2), placed(5.5, 5.5, 0.5, ObstructionType::TYPE_2));
    w.insert(object_id(3), placed(3.5, 3.6, 0.5, ObstructionType::TYPE_2));
    w.insert(object_id(4), placed(7.5, 7.5, 0.4, ObstructionType::TYPE_2));
    let p = HullPather::for_list(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        &[
            object_id(1),
            object_id(2),
            object_id(3),
            object_id(777),
            object_id(4),
        ],
        TerrainMask::of_bits(6),
        Distance::new(0.6),
        Some(PlayerId::from(2)),
    );
    assert_eq!(
        p.ignore,
        [
            RecordId::from(object_id(1)),
            RecordId::from(object_id(2)),
            RecordId::from(object_id(3)),
            RecordId::from(object_id(777)),
            RecordId::from(object_id(4)),
            tile_record_id(Tile::new(3, 3))
        ]
    );
    assert_eq!(
        (p.player, p.mask, p.radius),
        (
            Some(PlayerId::from(2)),
            TerrainMask::of_bits(6),
            Distance::new(0.6)
        )
    );
    // an empty list ignores nothing
    assert_eq!(
        HullPather::for_list(
            (&mut w, crate::world::index::ObjectsRef::empty()),
            &[],
            TerrainMask::of_bits(6),
            Distance::new(0.6),
            Some(PlayerId::from(2))
        )
        .ignore,
        Vec::<RecordId>::new()
    );
    // slot 0x3e8's pather: the unit and its target
    let q = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        object_id(2),
        Some(object_id(1)),
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    assert_eq!(
        q.ignore,
        [
            RecordId::from(object_id(2)),
            RecordId::from(object_id(1)),
            tile_record_id(Tile::new(3, 3))
        ]
    );
}

/// `FindShortPath` without a target (argument 7 -1): past the quick path, the goal's point
/// query (`FUN_141051900` with flags 0x20) finds no record holding the goal, and the search
/// ends at a node within the range and the radius of the goal (squared, [`NUDGE`] more):
/// found, a path (1).
#[test]
fn a_walk_to_a_point_goes_around_a_wall_without_a_target() {
    let mut w = walled(6, Point::new(2.0, 5.5));
    let mut p = HullPather::for_list(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        &[object_id(9171)],
        TerrainMask::of_bits(6),
        Distance::new(0.2),
        Some(PlayerId::from(1)),
    );
    let (result, path) = p
        .find(
            Point::new(2.0, 5.5),
            Point::new(10.5, 5.5),
            None,
            Distance::ZERO,
        )
        .unwrap();
    assert_eq!(result, ShortFound::Reached);
    assert_eq!(
        path.points,
        [
            Point::new(2.0, 5.5),
            Point::new(5.8, 7.2),
            Point::new(7.2, 7.2),
            Point::new(10.5, 5.5)
        ]
    );
    // a target elsewhere than on the goal is none (its id -1) as well
    let mut q = HullPather::for_list(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        &[object_id(9171)],
        TerrainMask::of_bits(6),
        Distance::new(0.2),
        Some(PlayerId::from(1)),
    );
    assert_eq!(
        q.find(
            Point::new(2.0, 5.5),
            Point::new(10.5, 5.5),
            Some(object_id(9149)),
            Distance::ZERO
        )
        .unwrap(),
        (ShortFound::Reached, path)
    );
}

/// The goal's point query finds one object whose box, grown by half the radius, holds the
/// goal: it is the target (its id still -1), so the node within range of its box ends the
/// search without finding (2: partway). An ignored object does not count; two objects
/// holding the goal need the grid's order, which the slice does not hold.
#[test]
fn a_goal_on_an_object_takes_it_as_the_target_but_finds_no_path() {
    let mut w = walled(6, Point::new(2.0, 5.5));
    w.insert(
        object_id(48),
        placed(10.5, 5.5, 0.5, ObstructionType::TYPE_2),
    );
    let mut p = HullPather::for_list(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        &[object_id(9171)],
        TerrainMask::of_bits(6),
        Distance::new(0.2),
        Some(PlayerId::from(1)),
    );
    let (result, path) = p
        .find(
            Point::new(2.0, 5.5),
            Point::new(10.6, 5.5),
            None,
            Distance::ZERO,
        )
        .unwrap();
    assert_eq!(result, ShortFound::Partial);
    let end = *path.points.last().unwrap();
    let t = Target {
        at: Point::new(10.5, 5.5),
        radius: Footprint::new(0.5, 0.5),
        moves: false,
    };
    assert!(p.target_gap(&t, end) <= Distance::new(NUDGE), "{path:?}");
    // ignored, the object is no target: the search ends on the goal itself
    let mut q = HullPather::for_list(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        &[object_id(9171), object_id(48)],
        TerrainMask::of_bits(6),
        Distance::new(0.2),
        Some(PlayerId::from(1)),
    );
    let (result, path) = q
        .find(
            Point::new(2.0, 5.5),
            Point::new(10.6, 5.5),
            None,
            Distance::ZERO,
        )
        .unwrap();
    assert_eq!(
        (result, path.points.last()),
        (ShortFound::Reached, Some(&Point::new(10.6, 5.5)))
    );
    // a second object holding the goal
    w.insert(
        object_id(49),
        placed(11.5, 5.5, 0.5, ObstructionType::TYPE_2),
    );
    let mut r = HullPather::for_list(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        &[object_id(9171)],
        TerrainMask::of_bits(6),
        Distance::new(0.2),
        Some(PlayerId::from(1)),
    );
    assert!(
        r.find(
            Point::new(2.0, 5.5),
            Point::new(11.0, 5.5),
            None,
            Distance::ZERO
        )
        .is_err()
    );
}

/// The line tests (`FUN_141052350`, `FUN_141051900` with flags 0x40) leave a moving record
/// out unless `FindShortPath`'s include list (its arguments 13 and 14) holds it: an included
/// moving unit in the quick path's walk is the square it goes around; ignored, it is out all
/// the same.
#[test]
fn an_included_moving_unit_blocks_the_line_tests() {
    let mut objects = crate::world::index::TestObjects::default();
    let mut w = open_world();
    w.insert(
        object_id(9171),
        placed(2.0, 5.5, 0.6, ObstructionType::CIRCLE),
    );
    w.insert(
        object_id(50),
        placed(6.0, 5.5, 0.3, ObstructionType::CIRCLE),
    );
    objects.set_gait(
        object_id(50),
        Some(crate::map::spatial::Gait {
            moving: true,
            ..Default::default()
        }),
    );
    let mut p = HullPather::for_list(
        (&mut w, objects.view()),
        &[object_id(9171)],
        TerrainMask::of_bits(6),
        Distance::new(0.6),
        Some(PlayerId::from(1)),
    );
    p.buffer = Some(Distance::new(0.3));
    assert!(
        !p.blocked(Point::new(2.0, 5.5), Point::new(10.0, 5.5))
            .unwrap()
    );
    let (result, path) = p
        .find(
            Point::new(2.0, 5.5),
            Point::new(10.0, 5.5),
            None,
            Distance::ZERO,
        )
        .unwrap();
    assert_eq!(
        (result, path.points),
        (
            ShortFound::Reached,
            vec![Point::new(2.0, 5.5), Point::new(10.0, 5.5)]
        )
    );
    let mut p = HullPather::for_list(
        (&mut w, objects.view()),
        &[object_id(9171)],
        TerrainMask::of_bits(6),
        Distance::new(0.6),
        Some(PlayerId::from(1)),
    )
    .including(&[object_id(50)]);
    p.buffer = Some(Distance::new(0.3));
    assert!(
        p.blocked(Point::new(2.0, 5.5), Point::new(10.0, 5.5))
            .unwrap()
    );
    assert_eq!(
        p.hits(Point::new(2.0, 5.5), Point::new(10.0, 5.5))
            .unwrap()
            .len(),
        1
    );
    let (result, path) = p
        .find(
            Point::new(2.0, 5.5),
            Point::new(10.0, 5.5),
            None,
            Distance::ZERO,
        )
        .unwrap();
    let off = (0.3 + 0.6) * SQRT_2;
    let [start, by, end] = path.points[..] else {
        panic!("{path:?}")
    };
    assert_eq!(
        (result, start, end),
        (
            ShortFound::Reached,
            Point::new(2.0, 5.5),
            Point::new(10.0, 5.5)
        )
    );
    assert!((by.x() - 6.0).abs() < 1e-5 && (by.y() - (5.5 - off)).abs() < 1e-5);
    let mut q = HullPather::for_list(
        (&mut w, objects.view()),
        &[object_id(9171), object_id(50)],
        TerrainMask::of_bits(6),
        Distance::new(0.6),
        Some(PlayerId::from(1)),
    )
    .including(&[object_id(50)]);
    q.buffer = Some(Distance::new(0.3));
    assert!(
        !q.blocked(Point::new(2.0, 5.5), Point::new(10.0, 5.5))
            .unwrap()
    );
}

/// The manager keeps one pather: a search's area and hulls ([`KeptHulls`]) carried into the
/// next pather make its kept search (argument 11) the one search the game's lasting pather
/// makes; the constructor's state has no area, so a kept search builds its own.
#[test]
fn the_last_search_s_area_and_hulls_carry_into_a_kept_search() {
    let mut w = walled(6, Point::new(2.0, 5.5));
    let mut first = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        object_id(9171),
        Some(object_id(9149)),
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    let found = first
        .find(
            Point::new(2.0, 5.5),
            Point::new(12.5, 5.5),
            Some(object_id(9149)),
            Distance::new(0.1),
        )
        .unwrap();
    let kept = first.kept();
    assert_eq!(
        (kept.area, kept.hulls.len()),
        (TileRect::new(0, 3, 14, 7), 2)
    );
    // a kept search from another start reuses them as they are
    let mut next = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        object_id(9171),
        Some(object_id(9149)),
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    )
    .with_kept(kept.clone());
    let from_kept = next.find_keeping(
        Point::new(3.0, 5.5),
        Point::new(12.5, 5.5),
        Some(object_id(9149)),
        Distance::new(0.1),
    );
    let next_area = next.kept().area;
    let mut same = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        object_id(9171),
        Some(object_id(9149)),
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    same.area = kept.area;
    same.hulls = kept.hulls;
    assert_eq!(
        from_kept,
        same.find_keeping(
            Point::new(3.0, 5.5),
            Point::new(12.5, 5.5),
            Some(object_id(9149)),
            Distance::new(0.1)
        )
    );
    assert_eq!(next_area, TileRect::new(0, 3, 14, 7));
    // the constructor's state: no area, no hulls
    let none = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        object_id(9171),
        Some(object_id(9149)),
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    )
    .kept();
    assert_eq!((none.area, none.hulls.len()), (NO_AREA, 0));
    let mut fresh = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        object_id(9171),
        Some(object_id(9149)),
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    )
    .with_kept(none);
    assert_eq!(
        fresh
            .find_keeping(
                Point::new(2.0, 5.5),
                Point::new(12.5, 5.5),
                Some(object_id(9149)),
                Distance::new(0.1)
            )
            .unwrap(),
        found
    );
}

#[test]
fn a_unit_within_range_needs_no_path() {
    let mut w = Spatial::with_tiles(
        GridSize::new(20, 20),
        Tiles::new(GridSize::new(20, 20), vec![4; 400]),
    );
    w.insert(
        object_id(9171),
        placed(9.75, 5.5, 0.2, ObstructionType::CIRCLE),
    );
    w.insert(
        object_id(9149),
        placed(10.5, 5.5, 0.5, ObstructionType::TYPE_2),
    );
    let mut pather = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        object_id(9171),
        Some(object_id(9149)),
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    let (result, path) = pather
        .find(
            Point::new(9.75, 5.5),
            Point::new(10.5, 5.5),
            Some(object_id(9149)),
            Distance::new(0.1),
        )
        .unwrap();
    assert_eq!(
        (result, path.length()),
        (ShortFound::InRange, Distance::ZERO)
    );
}

/// The triangle hull (5, 6), (5, 4), (4, 5) crossed on its edge 1 by a node at `at` (the
/// start's child, its walk to (9, 5)), with an open node of no cost on each point of `taken`
/// first: the nodes [`HullPather::around_hull`] makes, each its place, parent and point, and
/// the crossing node's index.
fn around_triangle(at: Point, taken: &[i32]) -> (Vec<(Point, Option<usize>, i32)>, usize) {
    let mut w = open_world();
    let mut p = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        object_id(9171),
        None,
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    p.hulls.push(Hull {
        points: vec![
            Point::new(5.0, 6.0),
            Point::new(5.0, 4.0),
            Point::new(4.0, 5.0),
        ],
        entrances: vec![None; 3],
        min: Point::new(4.0, 4.0),
        max: Point::new(5.0, 6.0),
        objects: Vec::new(),
    });
    let to = Point::new(9.0, 5.0);
    let root = p
        .add_unrefused(UnrefusedNode {
            list: NodeList::SEARCH,
            open: true,
            parent: None,
            position: Point::new(2.0, 5.0),
            goal: Some(to),
            hull: -1,
            direction: -1,
        })
        .unwrap();
    for &v in taken {
        let point = p.hulls[0].points[v as usize];
        let _ = p
            .add_node(NewNode {
                list: NodeList::SEARCH,
                open: true,
                parent: None,
                position: point,
                goal: Some(to),
                hull: 0,
                vertex: v,
                direction: -1,
                by_place: false,
            })
            .unwrap();
    }
    let node = p
        .add_unrefused(UnrefusedNode {
            list: NodeList::SEARCH,
            open: false,
            parent: Some(root),
            position: at,
            goal: Some(to),
            hull: 0,
            direction: -1,
        })
        .unwrap();
    p.around_hull(node, HullIndex::new(0), 1, to).unwrap();
    // the places as numbers, for the tests' sums
    let made = p.nodes.0[node.0 + 1..]
        .iter()
        .map(|n| (n.at, n.parent.map(|i| i.0), n.vertex))
        .collect();
    (made, node.0)
}

/// `FindShortPath` around a crossed hull: from the crossed edge 1 both ways, its start's point
/// 0 back and its end's point 1 on, half the points each; a hull of an odd count adds its last
/// point on the cheaper way, the forward way on a tie, the way left when the other's point was
/// refused (an open node there costs no more), none when both were.
#[test]
fn a_crossed_hull_s_points_go_both_ways_and_an_odd_one_the_cheaper() {
    let (made, n) = around_triangle(Point::new(4.5, 5.6), &[]);
    let a = Some(n + 1);
    assert_eq!(
        made,
        [
            (Point::new(5.0, 6.0), Some(n), 0),
            (Point::new(5.0, 4.0), Some(n), 1),
            (Point::new(4.0, 5.0), a, 2)
        ]
    );
    let (made, n) = around_triangle(Point::new(4.5, 5.0), &[]);
    assert_eq!(made[2], (Point::new(4.0, 5.0), Some(n + 2), 2));
    let (made, n) = around_triangle(Point::new(4.5, 5.6), &[1]);
    assert_eq!(
        made,
        [
            (Point::new(5.0, 6.0), Some(n), 0),
            (Point::new(4.0, 5.0), Some(n + 1), 2)
        ]
    );
    let (made, n) = around_triangle(Point::new(4.5, 5.6), &[0]);
    assert_eq!(
        made,
        [
            (Point::new(5.0, 4.0), Some(n), 1),
            (Point::new(4.0, 5.0), Some(n + 1), 2)
        ]
    );
    assert_eq!(around_triangle(Point::new(4.5, 5.6), &[0, 1]).0, []);
}

/// `FindShortPath`'s end of a found path ([`HullPather::found_end`]): without an entrance the
/// path stands; with one, the entrance's points but its last (the node's own) follow
/// backwards; a walk in that ended in a box keeps its last leg only when the leg meets nothing
/// or the target alone, else the last point goes and the path ends partway.
#[test]
fn a_found_path_follows_its_entrance_and_keeps_a_last_leg_on_the_target() {
    let mut w = walled(8, Point::new(2.0, 5.5));
    let mut p = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        object_id(9171),
        None,
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    p.entrances[0].points = vec![
        Point::new(12.5, 5.5),
        Point::new(11.9, 5.5),
        Point::new(11.7, 5.5),
    ];
    let q = HullQuery {
        from: Point::new(2.0, 5.5),
        to: Point::new(12.5, 5.5),
        target: None,
        target_id: Some(object_id(9149)),
        range: Distance::new(0.1),
    };
    let walk = || Waypoints {
        points: vec![Point::new(2.0, 5.5), Point::new(11.7, 5.5)],
        cost: Distance::ZERO,
    };
    let mut rounds = Rounds::new(Distance::ZERO);
    let mut path = walk();
    assert_eq!(p.found_end(&q, &rounds, &mut path), Ok(true));
    assert_eq!(path, walk());
    rounds.entrance = Some(EntranceIndex(0));
    let through = [
        Point::new(2.0, 5.5),
        Point::new(11.7, 5.5),
        Point::new(11.9, 5.5),
        Point::new(12.5, 5.5),
    ];
    let mut path = walk();
    assert_eq!(p.found_end(&q, &rounds, &mut path), Ok(true));
    assert_eq!(path.points, through);
    // ended in a box: the last leg meets the target 9149 alone
    rounds.entered_box = true;
    let mut path = walk();
    assert_eq!(p.found_end(&q, &rounds, &mut path), Ok(true));
    assert_eq!(path.points, through);
    let other = HullQuery {
        target_id: None,
        ..q
    };
    let mut path = walk();
    assert_eq!(p.found_end(&other, &rounds, &mut path), Ok(false));
    assert_eq!(path.points, through[..3]);
}

/// The search's end ([`HullPather::path_end`]) unfound, from (2, 5) to (9, 5): a node at
/// (6, 5) nearer the goal than the walk within the start's hull is the path's end (a walk
/// without points is unordered, the node's too); a walk nearer ends the path; without a node
/// the walk is the path, or none (3) without one. Each ends partway (2).
#[test]
fn an_unfound_search_ends_at_the_node_or_the_walk_nearer_the_goal() {
    let mut w = open_world();
    let mut p = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        object_id(9171),
        None,
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    p.start = Point::new(2.0, 5.0);
    let q = HullQuery {
        from: Point::new(2.0, 5.0),
        to: Point::new(9.0, 5.0),
        target: None,
        target_id: None,
        range: Distance::new(0.1),
    };
    let root = p
        .add_unrefused(UnrefusedNode {
            list: NodeList::SEARCH,
            open: false,
            parent: None,
            position: Point::new(2.0, 5.0),
            goal: Some(q.to),
            hull: -1,
            direction: -1,
        })
        .unwrap();
    let node = p
        .add_unrefused(UnrefusedNode {
            list: NodeList::SEARCH,
            open: false,
            parent: Some(root),
            position: Point::new(6.0, 5.0),
            goal: Some(q.to),
            hull: -1,
            direction: -1,
        })
        .unwrap();
    let mut rounds = Rounds::new(Distance::new(7.0));
    rounds.within = true;
    rounds.best_node = Some(node);
    let walked = |x: &[Point]| Waypoints {
        points: x.to_vec(),
        cost: Distance::ZERO,
    };
    let to_node = [Point::new(2.0, 5.0), Point::new(6.0, 5.0)];
    for walk in [&[Point::new(2.0, 5.0), Point::new(4.0, 5.0)][..], &[]] {
        let mut path = walked(walk);
        assert_eq!(p.path_end(&q, &rounds, &mut path), Ok(ShortFound::Partial));
        assert_eq!(path.points, to_node);
    }
    let mut path = walked(&[Point::new(2.0, 5.0), Point::new(8.0, 5.0)]);
    assert_eq!(p.path_end(&q, &rounds, &mut path), Ok(ShortFound::Partial));
    assert_eq!(path.points, [Point::new(2.0, 5.0), Point::new(8.0, 5.0)]);
    rounds.best_node = None;
    let mut path = walked(&[Point::new(2.0, 5.0), Point::new(8.0, 5.0)]);
    assert_eq!(p.path_end(&q, &rounds, &mut path), Ok(ShortFound::Partial));
    rounds.within = false;
    assert_eq!(p.path_end(&q, &rounds, &mut path), Ok(ShortFound::Blocked));
}

/// A start and a goal in the same hull (the object 48's grown box): the round walks within the
/// hull toward the goal first ([`HullPather::leave_start_hull`]). A goal no object holds is
/// reached (1); a goal the object holds makes it the target (the goal query's, id -1), whose
/// box the walk reaches (2). Without the target the walk to that goal ends partway on the
/// hull's side (`within`), and the search goes on from the hull's ways out.
#[test]
fn a_goal_in_the_start_s_hull_is_walked_to_within_it() {
    let mut w = walled(8, Point::new(4.9, 5.5));
    w.insert(
        object_id(48),
        placed(5.5, 5.5, 0.5, ObstructionType::TYPE_2),
    );
    let mut p = HullPather::new(
        (&mut w, crate::world::index::ObjectsRef::empty()),
        object_id(9171),
        None,
        TerrainMask::of_bits(6),
        Distance::new(0.2),
    );
    let side = Point::new(4.799_9, 5.499_899);
    let (result, path) = p
        .find(
            Point::new(4.9, 5.5),
            Point::new(4.85, 6.15),
            None,
            Distance::ZERO,
        )
        .unwrap();
    assert_eq!(
        (result, path.points),
        (
            ShortFound::Reached,
            vec![Point::new(4.9, 5.5), side, Point::new(4.85, 6.15)]
        )
    );
    let to = Point::new(6.1, 5.5);
    let (result, path) = p
        .find(Point::new(4.9, 5.5), to, None, Distance::ZERO)
        .unwrap();
    assert_eq!(
        (result, path.points),
        (ShortFound::Partial, vec![Point::new(4.9, 5.5), side])
    );
    let q = HullQuery {
        from: Point::new(4.9, 5.5),
        to,
        target: None,
        target_id: None,
        range: Distance::ZERO,
    };
    let mut rounds = Rounds::new(Distance::ZERO);
    let mut path = Waypoints::default();
    assert_eq!(p.round_start(&q, true, &mut rounds, &mut path), Ok(None));
    assert!(rounds.within);
    assert_eq!(path.points, [Point::new(4.9, 5.5), side]);
}
