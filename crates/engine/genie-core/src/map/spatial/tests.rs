use super::*;
use crate::construction::FoundationId;
use crate::map::GridSize;
use crate::map::TerrainMask;
use crate::map::Tile;
use crate::map::{Rect, Segment};
use crate::movement::Speed;
use crate::objects::HitPoints;
use crate::objects::ObjectState;
use crate::objects::masters::BuildableOn;
use crate::objects::masters::MasterKind;
use crate::objects::masters::class;
use crate::objects::masters::ids;
use crate::objects::masters::ids::FISH_TRAP;
use crate::objects::masters::master_type;
use crate::ownership::CaptureKind;
use crate::players::ByPlayer;
use crate::players::Relation;
use crate::players::relation_row;
use crate::players::relations;
use crate::test_ids::class_id;
use crate::test_ids::{master_id, object_id};
use crate::vision::VisionBits;
use crate::vision::registry::UnitUpdate;

/// The record cells (FD-030-D03) give every query the objects whose record is filed in a
/// cell it visits, as the full scan's cell test does, in id order: on and off the map, on
/// every level, while objects come, move, change masters, get and drop records and go.
#[test]
fn the_record_cells_answer_as_the_full_scan_does() {
    let mut s = Spatial::new(GridSize::new(120, 96));
    let mut seed = 0x2545_f491_u32;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    let kinds = [1u8, 2, 3, 5, 10, 11, 13].map(ObstructionType::from);
    let placed = |position: Point, r: f32, kind: Option<ObstructionType>| Placed {
        position: position.with_height(0.0),
        radius: Footprint::new(r, r * 0.5),
        class: class::CLASS_0,
        player: PlayerId::new(1),
        master: master_id(4),
        obstruction_type: ObstructionType::NONE,
        buildable_on: BuildableOn::NO,
        tall: false,
        moves: false,
        unknown: None,
        obstruction: kind.map(|kind| Obstruction {
            kind,
            class: ObstructionClass::NONE,
        }),
    };
    let scan = |s: &Spatial, rect: Rect| -> Vec<ObjectId> {
        s.objects
            .iter()
            .filter(|(_, p)| p.obstruction.is_some_and(|o| s.visits(p, o.kind, rect)))
            .map(|(id, _)| id)
            .collect()
    };
    let at = |next: &mut dyn FnMut() -> u32| (next() % 560) as f32 * 0.25 - 8.0;
    for step in 0..6000 {
        let id = (next() % 300) as i32;
        let position = Point::new(at(&mut next), at(&mut next));
        let r = (next() % 48) as f32 * 0.25;
        let kind = (next() % 8 != 0).then(|| kinds[(next() % 7) as usize]);
        match next() % 7 {
            0 | 1 => s.insert(object_id(id), placed(position, r, kind)),
            2 | 3 => s
                .set_position(object_id(id), position.with_height(0.0))
                .unwrap(),
            4 => {
                let here = s
                    .objects
                    .get(object_id(id))
                    .map_or(position, |p| p.position.ground());
                s.change_master(object_id(id), placed(here, r, kind));
            }
            5 => match kind {
                Some(kind) => s.add_record(
                    object_id(id),
                    Obstruction {
                        kind,
                        class: ObstructionClass::NONE,
                    },
                ),
                None => s.drop_record(object_id(id)),
            },
            _ => s.remove(object_id(id)),
        }
        let (x, y) = (at(&mut next), at(&mut next));
        let (w, h) = ((next() % 160) as f32 * 0.5, (next() % 160) as f32 * 0.5);
        let rect = Rect::new(x, y, x + w, y + h);
        assert_eq!(
            s.visited_records(rect),
            scan(&s, rect),
            "step {step}, {rect:?}"
        );
    }
}

#[test]
fn a_query_takes_its_kinds_of_players_from_the_save() {
    // player 2's +0x278 as loaded: itself 1, Gaia 0, player 1 an ally (2) whatever its
    // diplomacy byte (2 here, which no derivation knew)
    let mut s = Spatial::new(GridSize::new(10, 10));
    s.relations = ByPlayer::new(vec![
        relation_row([1, 4, 4]),
        relation_row([1, 0, 4]),
        relation_row([1, 2, 0]),
    ]);
    s.unit_diplomacy = ByPlayer::new(vec![
        relations([1, 4, 4]),
        relations([0, 1, 4]),
        relations([0, 2, 1]),
    ]);
    assert_eq!(s.query_kinds(PlayerId::new(2), 3), Ok(relations([0, 2, 1])));
    assert_eq!(s.query_kinds(PlayerId::new(2), 2), Ok(relations([0, 2])));
    assert!(s.query_kinds(PlayerId::new(3), 3).is_err());
}

/// `FUN_1410576a0`: counter-clockwise 1, clockwise -1, on the segment 0, beyond its end 1;
/// a NaN falls through every COMISS/JA to 0.
#[test]
fn ccw_answers_as_the_game_s_compares() {
    assert_eq!(
        ccw(
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, 1.0)
        ),
        1
    );
    assert_eq!(
        ccw(
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, -1.0)
        ),
        -1
    );
    assert_eq!(
        ccw(
            Point::new(0.0, 0.0),
            Point::new(2.0, 0.0),
            Point::new(1.0, 0.0)
        ),
        0
    );
    assert_eq!(
        ccw(
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(2.0, 0.0)
        ),
        1
    );
    assert_eq!(
        ccw(
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(-1.0, 0.0)
        ),
        -1
    );
    assert_eq!(
        ccw(
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(f32::NAN, 1.0)
        ),
        0
    );
}

/// The walk from (`x0`, `y0`) to (`x1`, `y1`).
const fn walk(x0: f32, y0: f32, x1: f32, y1: f32) -> Segment {
    Segment {
        from: Point::new(x0, y0),
        to: Point::new(x1, y1),
    }
}

/// `FUN_141052e10`: a walk from a NaN point meets the box's first side already (ccw answers
/// 0 for a NaN, so the segments meet); a walk inside it meets it; one past it does not.
#[test]
fn a_walk_meets_a_box_by_its_sides_or_from_inside() {
    let b = Rect::new(0.0, 0.0, 1.0, 1.0);
    assert!(box_meets_segment(
        b,
        Distance::ZERO,
        walk(f32::NAN, 0.5, 5.0, 5.0)
    ));
    assert!(box_meets_segment(
        b,
        Distance::ZERO,
        walk(0.5, 0.5, 0.6, 0.6)
    ));
    assert!(!box_meets_segment(
        b,
        Distance::ZERO,
        walk(3.0, 3.0, 5.0, 5.0)
    ));
}

/// `FUN_141051630` (and `FUN_141053130`): a circle record's box is read as it stands, so a
/// box whose x1 lies below its x0 holds no walk inside it; `FUN_1410512f0` orders it first.
#[test]
fn a_circle_record_s_box_is_read_unordered_and_another_s_ordered() {
    let b = Rect::new(2.0, 0.0, 1.0, 1.0);
    let inside = walk(1.4, 0.4, 1.6, 0.6);
    assert!(record_meets_segment(
        ObstructionType::TYPE_2,
        b,
        Distance::ZERO,
        inside
    ));
    assert!(!record_meets_segment(
        ObstructionType::CIRCLE,
        b,
        Distance::ZERO,
        inside
    ));
    assert!(record_meets_segment(
        ObstructionType::CIRCLE,
        Rect::new(1.0, 0.0, 2.0, 1.0),
        Distance::ZERO,
        inside
    ));
    // grown, the edges and the inside tests take the grow on the box as it stands: the
    // edges then run at x 1.25 and 1.75, the inside tests hold nothing
    assert!(!record_meets_segment(
        ObstructionType::CIRCLE,
        b,
        Distance::new(0.25),
        inside
    ));
    assert!(record_meets_segment(
        ObstructionType::CIRCLE,
        b,
        Distance::new(0.25),
        walk(1.0, 0.5, 1.5, 0.5)
    ));
    assert!(record_meets_segment(
        ObstructionType::TYPE_2,
        b,
        Distance::new(0.25),
        inside
    ));
}

fn villager(x: f32, y: f32) -> Placed {
    Placed {
        position: Position::new(x, y, 0.0),
        radius: Footprint::new(0.2, 0.2),
        class: class::VILLAGER,
        player: PlayerId::new(2),
        master: ids::ID_0,
        obstruction_type: ObstructionType::NONE,
        buildable_on: BuildableOn::NO,
        tall: true,
        moves: false,
        unknown: None,
        obstruction: Some(Obstruction {
            kind: ObstructionType::CIRCLE,
            class: ObstructionClass::UNIT,
        }),
    }
}

#[test]
fn a_search_near_the_last_reads_its_answer_until_a_unit_ai_lets_it_go() {
    use crate::unit_ai::{Pick, Search};
    use crate::vision::registry::Registry;
    let mut s = Spatial::new(GridSize::new(120, 120));
    // player 2 counts player 1 its enemy (3)
    s.relations = ByPlayer::new(vec![
        relation_row([0, 3, 3]),
        relation_row([0, 0, 3]),
        relation_row([0, 3, 0]),
    ]);
    s.unit_diplomacy = ByPlayer::new(vec![
        relations([1, 4, 4]),
        relations([0, 1, 4]),
        relations([0, 4, 1]),
    ]);
    let mut r = Registry::new(3, 65);
    // a villager of player 1 at (12, 10), which player 2 sees
    r.update_unit(UnitUpdate {
        id: object_id(1),
        owner: PlayerId::new(1),
        position: Point::new(12.0, 10.0),
        radii: Footprint::new(0.2, 0.2),
        class: class::VILLAGER,
        old: VisionBits::NONE,
        new: VisionBits::from_bits_retain(1 << 2),
    })
    .unwrap();
    let mut v = Visibility::new(
        GridSize::new(120, 120),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        0,
        Vec::new(),
    );
    v.registry = Some(r);
    s.sight = Some(v);
    let villagers = [class::VILLAGER];
    let look = |at| Search {
        at,
        radius: Distance::new(3.0),
        classes: &villagers,
        size: Footprint::new(0.2, 0.2),
        pick: Pick::Look,
        memory: crate::unit_ai::Memory::default(),
    };
    let mut never = crate::unit_ai::NoAttack;
    let mut reach = |_: &mut Spatial, _: ObjectId, _: Distance| Ok(true);
    let still = |_: ObjectId| false;
    let none = |_: &Placed| Ok(false);
    let no_farm = |_: ObjectId| None;
    let mut search = |s: &mut Spatial, at| {
        let f = s
            .search(SearchQuery {
                searcher: object_id(9),
                player: PlayerId::new(2),
                search: &look(at),
                attack: &mut never,
                reach: &mut reach,
                moving: &still,
                shuns: &none,
                farm: &no_farm,
            })
            .unwrap();
        (f.enemies, f.fresh)
    };
    assert_eq!(search(&mut s, Point::new(10.0, 10.0)), (true, true));
    // 4 tiles on (16 squared, within 25) the query is skipped: the enemy stays found
    assert_eq!(search(&mut s, Point::new(10.0, 14.0)), (true, false));
    // a unit AI's full update lets the cache go: the query runs, and finds nothing there
    s.forget_search();
    assert_eq!(search(&mut s, Point::new(10.0, 14.0)), (false, true));
    // 6 tiles from the last query (36 squared) it runs again
    assert_eq!(search(&mut s, Point::new(10.0, 8.0)), (true, true));
}

#[test]
fn a_rectangle_leaving_the_map_meets_the_edge_records_where_its_diagonal_crosses() {
    let mut s = Spatial::new(GridSize::new(120, 120));
    assert_eq!(
        s.edge_records(Rect::new(0.0, 0.0, 120.0, 120.0), 25),
        Vec::<Hit>::new()
    );
    // the diagonal (-0.3, 50) -> (0.3, 50.6) crosses x 0 at y 50.3: the left edge's tile (0, 50)
    let left = s.edge_records(Rect::new(-0.3, 50.0, 0.3, 50.6), 25);
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].id.value() as u32, 0xff00_0000 | 0x32);
    assert_eq!(left[0].bounds, Rect::new(0.0, 50.0, 1.0, 51.0));
    // past the bottom right corner: (119.5, 119.7) -> (120.5, 120.7) crosses x 120 at y 120.2
    // and y 120 at x 119.8
    let corner = s.edge_records(Rect::new(119.5, 119.7, 120.5, 120.7), 25);
    let ids: Vec<u32> = corner.iter().map(|h| h.id.value() as u32).collect();
    assert_eq!(ids, [0xff07_8078, 0xff07_7078]);
    // a list with room for one stops after the first edge
    assert_eq!(
        s.edge_records(Rect::new(119.5, 119.7, 120.5, 120.7), 1),
        corner[..1]
    );
    // an infinite end: the crossing's terms give NaN, which CVTTSS2SI makes i32::MIN
    let far = s.edge_records(Rect::new(-0.5, 10.0, f32::INFINITY, 10.5), 25);
    assert_eq!(far.len(), 2);
    assert_eq!(far[0].id.value() as u32, 0xff00_000a);
    assert_eq!(
        far[0].bounds,
        Rect::new(-2_147_483_648.0, 10.0, -2_147_483_648.0, 11.0)
    );
    // a unit of radius 0.3 cannot stand 0.2 from the edge
    assert!(
        !s.point_clear(
            object_id(1),
            Point::new(0.2, 50.0),
            Distance::new(0.3),
            TerrainMask::ALL,
            crate::world::index::ObjectsRef::empty()
        )
        .unwrap()
    );
    assert!(
        s.point_clear(
            object_id(1),
            Point::new(0.4, 50.0),
            Distance::new(0.3),
            TerrainMask::ALL,
            crate::world::index::ObjectsRef::empty()
        )
        .unwrap()
    );
}

/// `FormationTransform`'s line query: a standing villager on the line is hit, but not when
/// ignored or moving; a tile the mask lacks is hit once the formation put its record (its
/// box, [`Spatial::put_tile_records`]); a line leaving the map meets the edge; the buffer
/// the formation finds grows the records.
#[test]
fn a_still_line_meets_standing_records_and_the_temporary_tiles() {
    let mut objects = crate::world::index::TestObjects::default();
    let mut classes = vec![1u8; 100];
    classes[2 * 10 + 7] = 2;
    let mut s = Spatial::with_tiles(
        GridSize::new(10, 10),
        Tiles::new(crate::map::GridSize::new(10, 10), classes),
    );
    s.insert(object_id(7), villager(4.0, 5.0));
    let line = |s: &Spatial, ignore: &[RecordId], objects: &crate::world::index::TestObjects| {
        s.still_line_hit(
            Point::new(1.0, 5.0),
            Point::new(8.0, 5.0),
            Distance::ZERO,
            ignore,
            objects.view(),
        )
        .unwrap()
    };
    assert!(line(&s, &[], &objects));
    assert!(!line(&s, &[RecordId::from(object_id(7))], &objects));
    objects.set_gait(
        object_id(7),
        Some(Gait {
            moving: true,
            ..Gait::default()
        }),
    );
    assert!(!line(&s, &[], &objects));
    // the class-2 tile (7, 2) once its record is put, for a mask that lacks its class
    let across = |s: &Spatial| {
        s.still_line_hit(
            Point::new(7.5, 1.0),
            Point::new(7.5, 4.0),
            Distance::ZERO,
            &[],
            objects.view(),
        )
        .unwrap()
    };
    assert!(!across(&s));
    s.put_tile_records(
        Point::new(6.0, 1.0),
        Point::new(8.0, 3.0),
        Distance::ZERO,
        TerrainMask::of_bits(3),
    );
    assert!(!across(&s));
    s.put_tile_records(
        Point::new(6.0, 1.0),
        Point::new(8.0, 3.0),
        Distance::ZERO,
        TerrainMask::of_bits(1),
    );
    assert!(across(&s));
    s.clear_tile_records();
    assert!(!across(&s));
    assert!(
        s.still_line_hit(
            Point::new(1.0, 5.0),
            Point::new(10.5, 5.0),
            Distance::ZERO,
            &[],
            objects.view()
        )
        .unwrap()
    );
    // a line 0.3 beside the standing villager (radius 0.2): met only with a buffer of 0.2
    let mut t = Spatial::with_tiles(
        GridSize::new(10, 10),
        Tiles::new(crate::map::GridSize::new(10, 10), vec![1; 100]),
    );
    t.insert(object_id(8), villager(4.0, 5.0));
    assert!(
        !t.still_line_hit(
            Point::new(1.0, 5.3),
            Point::new(8.0, 5.3),
            Distance::ZERO,
            &[],
            objects.view()
        )
        .unwrap()
    );
    assert!(
        t.still_line_hit(
            Point::new(1.0, 5.3),
            Point::new(8.0, 5.3),
            Distance::new(0.2),
            &[],
            objects.view()
        )
        .unwrap()
    );
}

/// `FUN_141158940`: a villager's record holds (4, 5.5) for anyone but the ignored, and an
/// empty list ignores the unit itself; a tile of a class outside the mask, or off the map,
/// is never clear.
#[test]
fn a_point_is_clear_among_the_ignored_objects() {
    let mut classes = vec![1u8; 100];
    classes[5 * 10 + 4] = 2;
    let mut s = Spatial::with_tiles(
        GridSize::new(10, 10),
        Tiles::new(crate::map::GridSize::new(10, 10), classes),
    );
    s.insert(object_id(7), villager(4.0, 6.5));
    assert!(
        !s.point_clear_among(
            object_id(1),
            Point::new(4.0, 6.5),
            Distance::new(0.1),
            TerrainMask::of_bits(1),
            &[],
            crate::world::index::ObjectsRef::empty()
        )
        .unwrap()
    );
    assert!(
        s.point_clear_among(
            object_id(1),
            Point::new(4.0, 6.5),
            Distance::new(0.1),
            TerrainMask::of_bits(1),
            &[object_id(3), object_id(7)],
            crate::world::index::ObjectsRef::empty()
        )
        .unwrap()
    );
    assert!(
        s.point_clear_among(
            object_id(7),
            Point::new(4.0, 6.5),
            Distance::new(0.1),
            TerrainMask::of_bits(1),
            &[],
            crate::world::index::ObjectsRef::empty()
        )
        .unwrap()
    );
    assert!(
        !s.point_clear_among(
            object_id(7),
            Point::new(4.0, 6.5),
            Distance::new(0.1),
            TerrainMask::of_bits(1),
            &[object_id(3)],
            crate::world::index::ObjectsRef::empty()
        )
        .unwrap()
    );
    // the class-2 tile (4, 5) and a point off the map
    assert!(
        !s.point_clear_among(
            object_id(7),
            Point::new(4.5, 5.5),
            Distance::new(0.1),
            TerrainMask::of_bits(1),
            &[],
            crate::world::index::ObjectsRef::empty()
        )
        .unwrap()
    );
    assert!(
        s.point_clear_among(
            object_id(7),
            Point::new(4.5, 5.5),
            Distance::new(0.1),
            TerrainMask::of_bits(3),
            &[],
            crate::world::index::ObjectsRef::empty()
        )
        .unwrap()
    );
    assert!(
        !s.point_clear_among(
            object_id(7),
            Point::new(-0.5, 5.5),
            Distance::new(0.1),
            TerrainMask::of_bits(3),
            &[],
            crate::world::index::ObjectsRef::empty()
        )
        .unwrap()
    );
    // CVTTSS2SI: a NaN coordinate is i32::MIN, off the map, not tile 0
    assert!(
        !s.point_clear_among(
            object_id(7),
            Point::new(f32::NAN, 5.5),
            Distance::new(0.1),
            TerrainMask::of_bits(1),
            &[],
            crate::world::index::ObjectsRef::empty()
        )
        .unwrap()
    );
}

/// `FUN_1411570c0`'s query with its tile records: on a 10 by 10 map of terrain class 1 with
/// a tile of class 2 at (5, 5), the line (2, 5.5) -> (8, 5.5) of a unit whose mask holds only
/// class 1 meets the tile's temporary record and the villager on its way; without the mask
/// and with the villager ignored it meets nothing; a line whose grown rectangle leaves the
/// map meets the map edge's record first.
#[test]
fn a_line_meets_the_records_and_the_tiles_its_mask_lacks() {
    let mut classes = vec![1u8; 100];
    classes[5 * 10 + 5] = 2;
    let mut s = Spatial::with_tiles(
        GridSize::new(10, 10),
        Tiles::new(crate::map::GridSize::new(10, 10), classes),
    );
    s.insert(object_id(7), villager(4.0, 5.5));
    s.set_manager_buffer(Some(Distance::new(0.2)));
    let hits = s
        .line_query(
            Point::new(2.0, 5.5),
            Point::new(8.0, 5.5),
            Distance::new(0.2),
            Some(TerrainMask::of_bits(1)),
            &[],
        )
        .unwrap();
    assert_eq!(
        hits,
        [
            tile_record_id(Tile::new(5, 5)),
            RecordId::from(object_id(7))
        ]
    );
    // the routine sets the ignore list, resets the buffer and removes its records
    assert_eq!(s.manager_ignore(), Vec::<RecordId>::new());
    assert_eq!(s.manager_buffer(), None);
    assert_eq!(s.tile_records(), Vec::<Tile>::new());
    let none = s
        .line_query(
            Point::new(2.0, 5.5),
            Point::new(8.0, 5.5),
            Distance::new(0.2),
            None,
            &[RecordId::from(object_id(7))],
        )
        .unwrap();
    assert_eq!(none, Vec::<RecordId>::new());
    assert_eq!(s.manager_ignore(), [RecordId::from(object_id(7))]);
    // a record still put counts without the routine's own; the pather's test leaves them out
    s.put_tile_records(
        Point::new(5.5, 5.5),
        Point::new(5.5, 5.5),
        Distance::ZERO,
        TerrainMask::of_bits(1),
    );
    let kept = s
        .line_query(
            Point::new(2.0, 5.5),
            Point::new(8.0, 5.5),
            Distance::new(0.2),
            None,
            &[RecordId::from(object_id(7))],
        )
        .unwrap();
    assert_eq!(kept, [tile_record_id(Tile::new(5, 5))]);
    let movers = s
        .line_hits(
            Point::new(2.0, 5.5),
            Point::new(8.0, 5.5),
            Distance::new(0.2),
            false,
            &[RecordId::from(object_id(7))],
        )
        .unwrap();
    assert_eq!(movers, Vec::<RecordId>::new());
    s.clear_tile_records();
    let edge = s
        .line_query(
            Point::new(0.1, 5.5),
            Point::new(3.0, 5.5),
            Distance::new(0.2),
            None,
            &[RecordId::from(object_id(7))],
        )
        .unwrap();
    assert_eq!(edge.len(), 1);
    assert_eq!(edge[0].value() as u32 & 0xff00_0000, 0xff00_0000);
}

/// `FUN_1410562d0`'s buffer stays with the obstruction manager until the next call: the
/// stand tests and the walk's query set one and reset it after; a point query counts the
/// temporary tile records still put (the group's clear spot leaves one, `FUN_141007630`).
#[test]
fn the_manager_keeps_its_buffer_and_point_queries_meet_tile_records() {
    let mut classes = vec![1u8; 100];
    classes[5 * 10 + 4] = 2;
    let mut s = Spatial::with_tiles(
        GridSize::new(10, 10),
        Tiles::new(crate::map::GridSize::new(10, 10), classes),
    );
    assert_eq!(
        (s.manager_buffer(), s.manager_grow()),
        (None, Distance::ZERO)
    );
    s.set_manager_buffer(Some(Distance::new(0.1)));
    assert_eq!(s.manager_grow(), Distance::new(0.1));
    assert!(
        s.point_clear(
            object_id(1),
            Point::new(6.5, 6.5),
            Distance::new(0.2),
            TerrainMask::of_bits(1),
            crate::world::index::ObjectsRef::empty()
        )
        .unwrap()
    );
    assert_eq!(s.manager_buffer(), None);
    s.set_manager_buffer(Some(Distance::new(0.1)));
    let _ = s
        .walk_hits(WalkQuery {
            objects: crate::world::index::ObjectsRef::empty(),
            start: Point::new(2.0, 2.0),
            end: Point::new(3.0, 2.0),
            rect: Rect::new(2.0, 2.0, 3.0, 2.0),
            grow: Distance::new(0.2),
            ally_factor: 1.0,
            player: None,
            ignore: &[],
        })
        .unwrap();
    assert_eq!(s.manager_buffer(), None);
    // a point query near tile (4, 5) meets its record once it is put, within the grow
    assert!(
        !s.point_hit(
            Point::new(3.95, 5.5),
            Distance::new(0.1),
            &[],
            true,
            crate::world::index::ObjectsRef::empty()
        )
        .unwrap()
    );
    s.put_tile_records(
        Point::new(4.5, 5.5),
        Point::new(4.5, 5.5),
        Distance::ZERO,
        TerrainMask::of_bits(1),
    );
    assert!(
        s.point_hit(
            Point::new(3.95, 5.5),
            Distance::new(0.1),
            &[],
            true,
            crate::world::index::ObjectsRef::empty()
        )
        .unwrap()
    );
    assert!(
        !s.point_hit(
            Point::new(3.85, 5.5),
            Distance::new(0.1),
            &[],
            true,
            crate::world::index::ObjectsRef::empty()
        )
        .unwrap()
    );
    assert!(
        !s.point_hit(
            Point::new(3.95, 5.5),
            Distance::new(0.1),
            &[tile_record_id(Tile::new(4, 5))],
            true,
            crate::world::index::ObjectsRef::empty()
        )
        .unwrap()
    );
    // a stand test counts it too
    assert!(
        !s.point_clear_among(
            object_id(1),
            Point::new(3.95, 5.5),
            Distance::new(0.1),
            TerrainMask::of_bits(1),
            &[],
            crate::world::index::ObjectsRef::empty()
        )
        .unwrap()
    );
    s.clear_tile_records();
    assert!(
        s.point_clear_among(
            object_id(1),
            Point::new(3.95, 5.5),
            Distance::new(0.1),
            TerrainMask::of_bits(1),
            &[],
            crate::world::index::ObjectsRef::empty()
        )
        .unwrap()
    );
}

/// The walk's query beyond the map's edge meets the edge's record first (`FUN_141057290`),
/// then the objects as before.
#[test]
fn a_walk_past_the_maps_edge_meets_its_record_first() {
    let mut s = Spatial::new(GridSize::new(10, 10));
    s.insert(object_id(7), villager(1.0, 5.5));
    let (from, to) = (Point::new(0.1, 5.5), Point::new(2.0, 5.5));
    let rect = Rect::new(0.1, 5.5, 2.0, 5.5);
    let hits = s
        .walk_hits(WalkQuery {
            objects: crate::world::index::ObjectsRef::empty(),
            start: from,
            end: to,
            rect,
            grow: Distance::new(0.2),
            ally_factor: 1.0,
            player: Some(PlayerId::new(2)),
            ignore: &[],
        })
        .unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].value() as u32 & 0xff00_0000, 0xff00_0000);
    assert_eq!(hits[1], RecordId::from(object_id(7)));
}

#[test]
fn a_units_box_is_its_position_plus_and_minus_its_radii() {
    // MarvinBlue/426048731 tick 680: 9177's record box in the game's obstruction manager
    assert_eq!(
        villager(28.0, 89.0).bounds(),
        Rect::new(27.8, 88.8, 28.2, 89.2)
    );
}

#[test]
fn a_query_finds_the_boxes_its_rectangle_meets_but_not_the_ignored_or_unobstructing() {
    let mut s = Spatial::new(GridSize::new(120, 120));
    s.insert(object_id(9177), villager(28.0, 89.0));
    s.insert(object_id(9175), villager(30.0, 89.0));
    let mut foundation = villager(30.0, 88.0);
    foundation.radius = Footprint::new(1.0, 1.0);
    foundation.obstruction = None;
    foundation.class = class::BUILDING;
    s.insert(object_id(10028), foundation);
    // 9177's first step: from (28, 89) toward (28.0143, 88.9928), grown by its radius
    let step = Rect::new(28.0, 88.99284, 28.01431, 89.0);
    assert_eq!(
        s.obstruction_candidates(step, Distance::new(0.2), &[RecordId::from(object_id(9177))]),
        Vec::new()
    );
    assert_eq!(
        s.obstruction_candidates(step, Distance::new(0.2), &[]),
        vec![object_id(9177)]
    );
    // 9175's box grown by 0.2 starts at x 29.6
    assert_eq!(
        s.obstruction_candidates(
            Rect::new(29.0, 89.0, 29.59, 89.0),
            Distance::new(0.2),
            &[RecordId::from(object_id(9177))]
        ),
        Vec::new()
    );
    assert_eq!(
        s.obstruction_candidates(
            Rect::new(29.0, 89.0, 29.61, 89.0),
            Distance::new(0.2),
            &[RecordId::from(object_id(9177))]
        ),
        vec![object_id(9175)]
    );
}

#[test]
fn a_record_filed_in_a_cell_the_query_does_not_visit_is_not_found() {
    // records/425906646's update 553: the smoothing of 27272's plan (radius 0.25) tests
    // (32.25, 154.59375) -> (32.5, 151.5); bush 13594's box (31, 154)-(32, 155) grown by 0.25
    // touches the start, but it is filed in level 5's cell (3, 19) (tiles 24..31) and the
    // query's tiles are 32..32
    let mut s = Spatial::with_tiles(
        GridSize::new(220, 220),
        Tiles::new(crate::map::GridSize::new(220, 220), vec![1; 220 * 220]),
    );
    assert_eq!(s.grid, Grid { levels: 6 });
    let mut bush = villager(31.5, 154.5);
    bush.radius = Footprint::new(0.5, 0.5);
    bush.player = PlayerId::new(0);
    bush.obstruction = Some(Obstruction {
        kind: ObstructionType::TYPE_2,
        class: ObstructionClass::RESOURCE,
    });
    s.insert(object_id(13594), bush);
    assert_eq!(s.filed(&bush, ObstructionType::TYPE_2), (5, 3, 19));
    let (from, to) = (Point::new(32.25, 154.59375), Point::new(32.5, 151.5));
    assert!(!s.segment_blocked(from, to, Distance::new(0.25), TerrainMask::ALL, &[]));
    // a start a hair to the left visits tile 31's cell: the box blocks
    assert!(s.segment_blocked(
        Point::new(32.2, 154.59375),
        to,
        Distance::new(0.25),
        TerrainMask::ALL,
        &[]
    ));
    // off a half tile, the box's far edge is its tile: (31.6 ± 0.5) spans tiles 31 and 32,
    // which share only level 2's cells of 64 tiles
    bush.position = Position::new(31.6, 154.5, 0.0);
    assert_eq!(s.filed(&bush, ObstructionType::TYPE_2), (2, 0, 2));
}

#[test]
fn a_record_leaving_a_tile_another_static_record_stands_on_leaves_its_marks() {
    // records/425906646: Gaia's state-8 bush 0 shares tile (140, 3) with bush 808; its
    // removal (FUN_141055980) stops at the tile, which stays marked, and the planners hear
    // nothing
    let mut s = Spatial::with_tiles(
        GridSize::new(220, 220),
        Tiles::new(crate::map::GridSize::new(220, 220), vec![1; 220 * 220]),
    );
    let mut bush = villager(140.5, 3.5);
    bush.radius = Footprint::new(0.5, 0.5);
    bush.player = PlayerId::new(0);
    bush.obstruction = Some(Obstruction {
        kind: ObstructionType::TYPE_2,
        class: ObstructionClass::RESOURCE,
    });
    s.insert(object_id(808), bush);
    s.insert(object_id(0), bush);
    let _ = s.take_changes();
    s.remove(object_id(0));
    assert!(
        s.tiles()
            .word(Tile::new(140, 3))
            .is_some_and(|w| w & BLOCKED != 0)
    );
    assert_eq!(s.take_changes(), Vec::<PlanRect>::new());
    // alone, it clears its tile
    s.remove(object_id(808));
    assert_eq!(s.tiles().word(Tile::new(140, 3)), Some(1));
    assert_eq!(s.take_changes(), vec![PlanRect::new(140, 3, 140, 3)]);
}

/// `FUN_141055b40` by the master's obstruction type: type 12 keeps all (false); type 1 clears
/// its tiles and keeps its record (true); a type 2 foundation without its record still clears
/// its player's marks (false); a unit (type 5) drops its record (true).
#[test]
fn a_record_goes_by_its_master_s_obstruction_type() {
    let mut s = Spatial::with_tiles(
        GridSize::new(20, 20),
        Tiles::new(crate::map::GridSize::new(20, 20), vec![1; 400]),
    );
    let mut rock = villager(5.5, 5.5);
    rock.radius = Footprint::new(0.5, 0.5);
    rock.obstruction_type = ObstructionType::TYPE_1;
    rock.obstruction = Some(Obstruction {
        kind: ObstructionType::TYPE_1,
        class: ObstructionClass::RESOURCE,
    });
    s.insert(object_id(1), rock);
    assert!(
        s.tiles()
            .word(Tile::new(5, 5))
            .is_some_and(|w| w & BLOCKED != 0)
    );
    assert!(s.dropped_record(object_id(1)));
    assert_eq!(s.tiles().word(Tile::new(5, 5)), Some(1));
    assert_eq!(
        s.get(object_id(1)).and_then(|p| p.obstruction),
        rock.obstruction
    );
    let mut other = rock;
    other.obstruction_type = ObstructionType::TYPE_12;
    other.position = Position::new(8.5, 8.5, 0.0);
    s.insert(object_id(2), other);
    assert!(!s.dropped_record(object_id(2)));
    assert!(
        s.tiles()
            .word(Tile::new(8, 8))
            .is_some_and(|w| w & BLOCKED != 0)
    );
    // a foundation of player 2 without its record on a tile player 2 marked
    let mut wall = rock;
    wall.obstruction_type = ObstructionType::TYPE_2;
    wall.obstruction = Some(Obstruction {
        kind: ObstructionType::TYPE_2,
        class: ObstructionClass::RESOURCE,
    });
    wall.position = Position::new(12.5, 12.5, 0.0);
    s.insert(object_id(3), wall);
    let mut site = wall;
    site.obstruction = None;
    s.insert(object_id(4), site);
    s.objects.get_mut(object_id(3)).unwrap().obstruction = None;
    assert!(!s.dropped_record(object_id(4)));
    assert_eq!(s.tiles().word(Tile::new(12, 12)), Some(1));
    let unit = villager(15.5, 15.5);
    let mut unit5 = unit;
    unit5.obstruction_type = ObstructionType::CIRCLE;
    s.insert(object_id(5), unit5);
    assert!(s.dropped_record(object_id(5)));
    assert_eq!(s.get(object_id(5)).and_then(|p| p.obstruction), None);
    assert!(!s.dropped_record(object_id(5)));
}

#[test]
fn a_walk_hits_the_boxes_its_step_crosses_and_a_moving_ally_s_box_shrinks() {
    let mut objects = crate::world::index::TestObjects::default();
    let mut s = Spatial::new(GridSize::new(120, 120));
    s.relations = ByPlayer::new(vec![
        relation_row([1, 4, 4]),
        relation_row([1, 0, 4]),
        relation_row([1, 4, 0]),
    ]);
    s.insert(object_id(9175), villager(30.0, 89.0));
    let (from, to) = (Point::new(29.5, 88.0), Point::new(29.5, 88.3));
    let rect = Rect::new(29.5, 88.0, 29.5, 88.3);
    // the box (29.8..30.2, 88.8..89.2) grown by 0.25 reaches 88.55, past the step's end
    assert_eq!(
        s.walk_hits(WalkQuery {
            objects: objects.view(),
            start: from,
            end: to,
            rect,
            grow: Distance::new(0.25),
            ally_factor: 0.5,
            player: Some(PlayerId::new(2)),
            ignore: &[]
        }),
        Ok(vec![])
    );
    // grown by 0.4 it reaches x 29.4: the step crosses it once it gets to y 88.4
    let longer = (Point::new(29.5, 88.0), Point::new(29.5, 88.5));
    let rect = Rect::new(29.5, 88.0, 29.5, 88.5);
    assert_eq!(
        s.walk_hits(WalkQuery {
            objects: objects.view(),
            start: longer.0,
            end: longer.1,
            rect,
            grow: Distance::new(0.4),
            ally_factor: 1.0,
            player: Some(PlayerId::new(2)),
            ignore: &[]
        }),
        Ok(vec![RecordId::from(object_id(9175))])
    );
    assert_eq!(
        s.walk_hits(WalkQuery {
            objects: objects.view(),
            start: longer.0,
            end: longer.1,
            rect,
            grow: Distance::new(0.4),
            ally_factor: 1.0,
            player: Some(PlayerId::new(2)),
            ignore: &[RecordId::from(object_id(9175))]
        }),
        Ok(vec![])
    );
    // moving, of the walker's player, it grows by 0.4 × 0.5 only
    objects.set_gait(
        object_id(9175),
        Some(Gait {
            moving: true,
            ..Gait::default()
        }),
    );
    assert_eq!(
        s.walk_hits(WalkQuery {
            objects: objects.view(),
            start: longer.0,
            end: longer.1,
            rect,
            grow: Distance::new(0.4),
            ally_factor: 0.5,
            player: Some(PlayerId::new(2)),
            ignore: &[]
        }),
        Ok(vec![])
    );
    // an enemy's moving box grows in full
    set_relation(&mut s, (2, 1), Relation::ENEMY);
    assert_eq!(
        s.walk_hits(WalkQuery {
            objects: objects.view(),
            start: longer.0,
            end: longer.1,
            rect,
            grow: Distance::new(0.4),
            ally_factor: 0.5,
            player: Some(PlayerId::new(1)),
            ignore: &[]
        }),
        Ok(vec![RecordId::from(object_id(9175))])
    );
}

/// MarvinBlue/426048731's Town Centre 9157 of player 2 (class 3) and a tree of Gaia (class 1),
/// with the words the game's obstruction manager holds for their tiles.
#[test]
fn a_static_record_marks_its_tiles_with_its_class_and_player() {
    let mut s = Spatial::with_tiles(
        GridSize::new(4, 4),
        Tiles::new(crate::map::GridSize::new(4, 4), vec![4; 16]),
    );
    let centre = Placed {
        position: Position::new(2.0, 2.0, 0.0),
        radius: Footprint::new(1.0, 1.0),
        class: class::BUILDING,
        player: PlayerId::new(2),
        master: ids::ID_0,
        obstruction_type: ObstructionType::NONE,
        buildable_on: BuildableOn::NO,
        tall: true,
        moves: false,
        unknown: None,
        obstruction: Some(Obstruction {
            kind: ObstructionType::TYPE_2,
            class: ObstructionClass::BUILDING,
        }),
    };
    s.insert(object_id(9157), centre);
    let tree = Placed {
        position: Position::new(0.5, 0.5, 0.0),
        radius: Footprint::new(0.5, 0.5),
        class: class::TREE,
        player: PlayerId::new(0),
        master: ids::ID_0,
        obstruction_type: ObstructionType::NONE,
        buildable_on: BuildableOn::NO,
        tall: true,
        moves: false,
        unknown: None,
        obstruction: Some(Obstruction {
            kind: ObstructionType::TYPE_2,
            class: ObstructionClass::RESOURCE,
        }),
    };
    s.insert(object_id(48), tree);
    let t = s.tiles();
    assert_eq!(t.word(Tile::new(1, 1)), Some(0x202_4304));
    assert_eq!(t.word(Tile::new(2, 2)), Some(0x202_4304));
    assert_eq!(t.word(Tile::new(3, 3)), Some(4));
    assert_eq!(t.word(Tile::new(0, 0)), Some(0x80_4104));
    assert_eq!(
        (t.objects(Tile::new(2, 2)), t.tall(Tile::new(2, 2))),
        (Some(1), Some(1))
    );
    s.remove(object_id(9157));
    assert_eq!(s.tiles().word(Tile::new(1, 1)), Some(4));
    assert_eq!(s.tiles().objects(Tile::new(2, 2)), Some(0));
}

#[test]
fn a_unit_counts_on_the_tile_its_centre_is_on_and_marks_nothing() {
    let mut s = Spatial::with_tiles(
        GridSize::new(4, 4),
        Tiles::new(crate::map::GridSize::new(4, 4), vec![4; 16]),
    );
    s.insert(object_id(9177), villager(1.9, 1.0));
    assert_eq!(s.tiles().objects(Tile::new(1, 1)), Some(1));
    assert_eq!(s.tiles().word(Tile::new(1, 1)), Some(4));
    s.set_position(object_id(9177), Position::new(2.05, 1.0, 0.0))
        .unwrap();
    assert_eq!(
        (
            s.tiles().objects(Tile::new(1, 1)),
            s.tiles().objects(Tile::new(2, 1))
        ),
        (Some(0), Some(1))
    );
    assert_eq!(s.tiles().tall(Tile::new(2, 1)), Some(1));
}

#[test]
fn a_tile_s_list_keeps_the_game_s_order() {
    let mut s = Spatial::with_tiles(
        GridSize::new(4, 4),
        Tiles::new(crate::map::GridSize::new(4, 4), vec![4; 16]),
    );
    for (id, x) in [(9171, 1.1), (10029, 1.5), (9173, 1.9)] {
        s.insert(object_id(id), villager(x, 1.0));
    }
    assert_eq!(
        s.tiles().list(Tile::new(1, 1)),
        [object_id(9171), object_id(10029), object_id(9173)]
    );
    assert_eq!(s.tile_place(object_id(9173)), 2);
    // the leaving object's slot goes to the last
    s.set_position(object_id(9171), Position::new(2.1, 1.0, 0.0))
        .unwrap();
    assert_eq!(
        s.tiles().list(Tile::new(1, 1)),
        [object_id(9173), object_id(10029)]
    );
    assert_eq!(
        (s.tile_place(object_id(9173)), s.tile_place(object_id(9171))),
        (0, 0)
    );
    // a move within the tile and a master change keep the slot
    s.set_position(object_id(9173), Position::new(1.2, 1.0, 0.0))
        .unwrap();
    s.change_master(object_id(9173), villager(1.2, 1.0));
    assert_eq!(
        s.tiles().list(Tile::new(1, 1)),
        [object_id(9173), object_id(10029)]
    );
    s.set_position(object_id(9171), Position::new(1.0, 1.0, 0.0))
        .unwrap();
    assert_eq!(
        s.tiles().list(Tile::new(1, 1)),
        [object_id(9173), object_id(10029), object_id(9171)]
    );
    assert_eq!(
        (
            s.tiles().objects(Tile::new(1, 1)),
            s.tiles().tall(Tile::new(1, 1))
        ),
        (Some(3), Some(3))
    );
    assert_eq!(
        s.force_tile_lists(&[(
            Tile::new(1, 1),
            vec![object_id(9171), object_id(9173), object_id(10029)]
        )]),
        Vec::new()
    );
    assert_eq!(
        s.tiles().list(Tile::new(1, 1)),
        [object_id(9171), object_id(9173), object_id(10029)]
    );
}

/// `FUN_140f5f0a0`: a tile of 250 objects takes no more and keeps its tall count;
/// `FUN_140f5f560`: an object not on the list leaves nothing changed (no recount).
#[test]
fn a_full_tile_takes_no_object_and_one_not_on_it_leaves_its_count() {
    let mut s = Spatial::with_tiles(
        GridSize::new(4, 4),
        Tiles::new(crate::map::GridSize::new(4, 4), vec![4; 16]),
    );
    for id in 0..250 {
        s.insert(object_id(id), villager(1.5, 1.5));
    }
    assert_eq!(s.tiles().objects(Tile::new(1, 1)), Some(250));
    assert_eq!(s.tiles().tall(Tile::new(1, 1)), Some(250));
    // object 0 is no longer tall; the game does not redo the count
    s.set_tall(object_id(0), false);
    s.insert(object_id(250), villager(1.5, 1.5));
    assert_eq!(s.tiles().objects(Tile::new(1, 1)), Some(250));
    assert!(!s.tiles().list(Tile::new(1, 1)).contains(&object_id(250)));
    assert_eq!(s.tiles().tall(Tile::new(1, 1)), Some(250));
    // 250 leaves a tile whose list never held it: no recount
    s.remove(object_id(250));
    assert_eq!(s.tiles().tall(Tile::new(1, 1)), Some(250));
    // one that is on it leaves: the count is redone over the list, 0 not tall
    s.remove(object_id(249));
    assert_eq!(s.tiles().tall(Tile::new(1, 1)), Some(248));
}

/// A ghost enters and leaves a tile's list as any object does (`FUN_140f5f0a0`,
/// `FUN_140f5f560`): the tile's tall count is redone over the list.
#[test]
fn a_ghost_entering_or_leaving_a_tile_redoes_its_tall_count() {
    let mut s = Spatial::with_tiles(
        GridSize::new(4, 4),
        Tiles::new(crate::map::GridSize::new(4, 4), vec![4; 16]),
    );
    s.insert(object_id(1), villager(1.5, 1.5));
    s.insert(object_id(2), villager(1.5, 1.5));
    s.set_tall(object_id(1), false);
    assert_eq!(s.tiles().tall(Tile::new(1, 1)), Some(2));
    s.insert_ghost(object_id(-5), Position::new(1.5, 1.5, 0.0));
    assert_eq!(
        s.tiles().list(Tile::new(1, 1)),
        [object_id(1), object_id(2), object_id(-5)]
    );
    assert_eq!(s.tiles().tall(Tile::new(1, 1)), Some(1));
    s.set_tall(object_id(2), false);
    s.remove_ghost(object_id(-5), Position::new(1.5, 1.5, 0.0));
    assert_eq!(s.tiles().tall(Tile::new(1, 1)), Some(0));
}

/// The house foundation 10028 at (30, 88) of MarvinBlue/426048731 with its builder 9175 on its
/// footprint at (30, 89), on open land (class 4) and a villager's mask (restriction 7: 6).
fn site() -> (Spatial, Placed) {
    let mut s = Spatial::with_tiles(
        GridSize::new(40, 120),
        Tiles::new(crate::map::GridSize::new(40, 120), vec![4; 40 * 120]),
    );
    s.insert(object_id(9175), villager(30.0, 89.0));
    let mut house = villager(30.0, 88.0);
    house.radius = Footprint::new(1.0, 1.0);
    house.class = class::BUILDING;
    house.master = ids::HOUSE;
    house.obstruction_type = ObstructionType::TYPE_2;
    house.obstruction = None;
    s.insert(object_id(10028), house);
    (s, house)
}

#[test]
fn a_tile_is_passable_when_its_class_is_in_the_mask_and_nothing_blocks_it() {
    let (mut s, _) = site();
    assert_eq!(
        s.passable(
            Tile::new(29, 89),
            TerrainMask::of_bits(6),
            object_id(1),
            Distance::new(0.2),
            crate::world::index::ObjectsRef::empty()
        ),
        Ok(true)
    );
    assert_eq!(
        s.passable(
            Tile::new(29, 89),
            TerrainMask::of_bits(1),
            object_id(1),
            Distance::new(0.2),
            crate::world::index::ObjectsRef::empty()
        ),
        Ok(false)
    );
    assert_eq!(
        s.passable(
            Tile::new(-1, 89),
            TerrainMask::of_bits(6),
            object_id(1),
            Distance::new(0.2),
            crate::world::index::ObjectsRef::empty()
        ),
        Ok(false)
    );
    let mut tree = villager(29.5, 90.5);
    tree.radius = Footprint::new(0.5, 0.5);
    tree.obstruction = Some(Obstruction {
        kind: ObstructionType::TYPE_2,
        class: ObstructionClass::RESOURCE,
    });
    s.insert(object_id(48), tree);
    assert_eq!(
        s.passable(
            Tile::new(29, 90),
            TerrainMask::of_bits(6),
            object_id(1),
            Distance::new(0.2),
            crate::world::index::ObjectsRef::empty()
        ),
        Ok(false)
    );
}

/// A player who lost while the game goes on sees the map revealed: its masks take the ended
/// bits (and the unified map's every word, here without the players' sight).
/// `FUN_1412847b0`: the mask goes into every word of the unified map; `FUN_141639860` (the
/// end of the game for a player) also gives the player's masks the ended bits.
#[test]
fn revealing_the_map_sets_the_mask_in_every_word() {
    let revealed = VisionBits::NO_TILE | VisionBits::NO_TILE_EXPLORED;
    let words = [0, 1 << 1, 1 << 33].map(VisionBits::from_bits_retain);
    let mut s = sighted(words.to_vec());
    s.reveal_map(revealed);
    assert_eq!(
        s.sight.as_ref().unwrap().unified,
        words.map(|w| w | revealed)
    );
    let mut s = sighted(vec![VisionBits::NONE; 2]);
    s.reveal_to(PlayerId::new(1));
    assert!(
        s.sight
            .as_ref()
            .unwrap()
            .unified
            .iter()
            .all(|&w| w == VisionBits::ENDED | VisionBits::ENDED_EXPLORED)
    );
    // player 1 lost: its masks take the ended bits; the tile tests mask them out
    // (OBJECT_MASK holds neither ENDED nor ENDED_EXPLORED)
    assert_eq!(
        s.vision.get(PlayerId::new(1)),
        Some(&crate::vision::own_masks(PlayerId::new(1), true))
    );
    assert_eq!(
        s.tile_sight(PlayerId::new(1), Tile::new(0, 0)),
        Some(TileSight::UNKNOWN)
    );
}

#[test]
fn a_revealed_map_gives_the_player_the_ended_bits() {
    let mut s = Spatial {
        vision: ByPlayer::new(vec![
            crate::vision::own_masks(PlayerId::new(0), false),
            crate::vision::own_masks(PlayerId::new(1), false),
        ]),
        ..Spatial::default()
    };
    s.reveal_to(PlayerId::new(1));
    assert_eq!(
        s.vision.get(PlayerId::new(1)),
        Some(&crate::vision::own_masks(PlayerId::new(1), true))
    );
    assert_eq!(
        s.vision.get(PlayerId::new(0)),
        Some(&crate::vision::own_masks(PlayerId::new(0), false))
    );
}

/// A map of two tiles holding `words`, which players 0 and 1 see with their own masks.
fn sighted(words: Vec<VisionBits>) -> Spatial {
    let n = words.len() as i32;
    Spatial {
        sight: Some(Visibility::new(
            GridSize::new(n, 1),
            Vec::new(),
            words,
            Vec::new(),
            0,
            Vec::new(),
        )),
        vision: ByPlayer::new(vec![
            crate::vision::own_masks(PlayerId::new(0), false),
            crate::vision::own_masks(PlayerId::new(1), false),
        ]),
        ..Spatial::default()
    }
}

/// `FUN_141663500`: 0xf seen, 0x80 explored, 0 neither; a word revealed to all (bit 29 with
/// an explored hit or bit 61) is seen, one explored for all (bit 61) explored.
#[test]
fn a_player_s_tile_test_answers_seen_explored_or_neither_and_reads_the_revealed_bits() {
    let (seen, explored) = (
        VisionBits::from_bits_retain(1 << 1),
        VisionBits::from_bits_retain(1 << 33),
    );
    let s = sighted(vec![
        seen,
        explored,
        VisionBits::NO_TILE_EXPLORED,
        VisionBits::NO_TILE | VisionBits::NO_TILE_EXPLORED,
        VisionBits::NO_TILE,
        VisionBits::NO_TILE | explored,
    ]);
    let answers = |p: PlayerId| {
        (0..7)
            .map(|x| s.tile_sight(p, Tile::new(x, 0)).map(TileSight::get))
            .collect::<Vec<_>>()
    };
    let some = |v: [u8; 7]| v.map(Some).to_vec();
    assert_eq!(
        answers(PlayerId::new(1)),
        some([0xf, 0x80, 0x80, 0xf, 0, 0xf, 0])
    );
    assert_eq!(answers(PlayerId::new(0)), some([0, 0, 0x80, 0xf, 0, 0, 0]));
    assert_eq!(
        s.tile_sight(PlayerId::new(1), Tile::new(-1, 0)),
        Some(TileSight::UNKNOWN)
    );
    assert_eq!(
        s.tile_sight(PlayerId::new(1), Tile::new(0, 1)),
        Some(TileSight::UNKNOWN)
    );
    assert_eq!(
        Spatial::default().tile_sight(PlayerId::new(1), Tile::new(0, 0)),
        None
    );
    // sees_tile is its 0xf, explored any answer
    assert!(
        s.sees_tile(PlayerId::new(0), Tile::new(3, 0))
            && !s.sees_tile(PlayerId::new(0), Tile::new(2, 0))
    );
    assert_eq!(s.explored(PlayerId::new(0), Tile::new(2, 0)), Ok(true));
    assert_eq!(s.explored(PlayerId::new(0), Tile::new(4, 0)), Ok(false));
}

/// `FUN_141285350` and `FUN_140c68a00`: the word under the registry's mask only, so the
/// revealed bits 29 and 61 count for nothing.
#[test]
fn the_visible_map_s_tile_test_reads_only_the_players_bits() {
    let (seen, explored) = (
        VisionBits::from_bits_retain(1 << 1),
        VisionBits::from_bits_retain(1 << 33),
    );
    let s = sighted(vec![
        seen,
        explored,
        VisionBits::NO_TILE | VisionBits::NO_TILE_EXPLORED,
        VisionBits::NONE,
    ]);
    let scout = (0..5)
        .map(|x| {
            s.scout_sight(PlayerId::new(1), Tile::new(x, 0))
                .map(TileSight::get)
        })
        .collect::<Vec<_>>();
    assert_eq!(scout, [0xf, 0x80, 0, 0, 0].map(Some).to_vec());
    let seen_now = (0..4)
        .map(|x| {
            s.visibility(PlayerId::new(1), Tile::new(x, 0))
                .map(TileSight::get)
        })
        .collect::<Vec<_>>();
    assert_eq!(seen_now, [0xf, 0x80, 0, 0].map(Some).to_vec());
    assert!(
        s.visible(PlayerId::new(1), Tile::new(0, 0))
            && !s.visible(PlayerId::new(1), Tile::new(1, 0))
    );
    assert_eq!(
        Spatial::default().scout_sight(PlayerId::new(1), Tile::new(0, 0)),
        None
    );
    assert_eq!(
        Spatial::default().visibility(PlayerId::new(1), Tile::new(0, 0)),
        None
    );
}

#[test]
fn the_free_spot_beside_a_footprint_is_the_closest_empty_tile_around_it() {
    // the game sends 9175 to (29.5, 89.5): (30, 89) is as close but holds 9175 itself
    let (mut s, house) = site();
    assert_eq!(
        s.spot_beside(
            Position::new(30.0, 89.0, 0.0),
            (object_id(1), 0.2),
            TerrainMask::of_bits(6),
            (house.position, house.radius),
            crate::world::index::ObjectsRef::empty()
        ),
        Ok(Some(Point::new(29.5, 89.5)))
    );
}

#[test]
fn the_footprint_s_occupants_are_the_objects_whose_centre_is_on_it() {
    let (mut s, house) = site();
    assert_eq!(
        s.footprint_occupants(
            FoundationId::new(object_id(10028)),
            &house,
            true,
            &BTreeMap::new()
        ),
        Ok(vec![object_id(9175)])
    );
    s.insert(object_id(9177), villager(28.0, 89.0));
    assert_eq!(
        s.footprint_occupants(
            FoundationId::new(object_id(10028)),
            &house,
            true,
            &BTreeMap::new()
        ),
        Ok(vec![object_id(9175)])
    );
    assert!(!s.foreign_marks(&house));
}

/// `FUN_14123bb20` passes over an object in state 7 or later, a building from state 3 on, and
/// a desolid building foundation unless its player and the site's count each other allied
/// (`FUN_141663d90` with the ally flag, both ways): 428016030's palisade foundation 33965
/// (player 3) shares its tile with player 8's foundation 33990, and its builder 31368 starts
/// at once in update 1305. An object the view does not look at (the owner's sleeping
/// objects: types below 30, alive) counts.
#[test]
fn a_foundation_not_allied_both_ways_or_a_dead_object_occupies_no_footprint() {
    use crate::construction::build::ClearLook;
    let (mut s, house) = site();
    // player 2 counts player 3 allied, player 3 counts player 2 neutral
    s.relations = ByPlayer::new(vec![
        relation_row([0; 4]),
        relation_row([0; 4]),
        relation_row([0, 3, 1, 0]),
        relation_row([0, 3, 3, 1]),
    ]);
    let mut wall = villager(30.5, 88.5);
    wall.player = PlayerId::new(3);
    wall.radius = Footprint::new(0.5, 0.5);
    s.insert(object_id(33990), wall);
    let look = |state: ObjectState, kind: MasterKind, desolid: bool| ClearLook {
        hp: HitPoints::new(1.0),
        height: Distance::new(1.0),
        roo_called: false,
        kind,
        state,
        desolid,
    };
    let looks = |wall: ClearLook| {
        BTreeMap::from([
            (
                object_id(9175),
                look(ObjectState::Alive, master_type::COMBAT, false),
            ),
            (object_id(33990), wall),
        ])
    };
    let occupants = |s: &Spatial, wall| {
        s.footprint_occupants(
            FoundationId::new(object_id(10028)),
            &house,
            true,
            &looks(wall),
        )
    };
    assert_eq!(
        occupants(
            &s,
            look(ObjectState::Foundation, master_type::BUILDING, true)
        ),
        Ok(vec![object_id(9175)])
    );
    // allied both ways, the foundation occupies it; built (not desolid) too
    set_relation(&mut s, (3, 2), Relation::ALLY);
    assert_eq!(
        occupants(
            &s,
            look(ObjectState::Foundation, master_type::BUILDING, true)
        ),
        Ok(vec![object_id(9175), object_id(33990)])
    );
    set_relation(&mut s, (3, 2), Relation::ENEMY);
    assert_eq!(
        occupants(&s, look(ObjectState::Alive, master_type::BUILDING, false)),
        Ok(vec![object_id(9175), object_id(33990)])
    );
    // a building from state 3 on, any object from state 7 on, passes
    assert_eq!(
        occupants(&s, look(ObjectState::Dying, master_type::BUILDING, false)),
        Ok(vec![object_id(9175)])
    );
    assert_eq!(
        occupants(&s, look(ObjectState::Undead, master_type::COMBAT, false)),
        Ok(vec![object_id(9175), object_id(33990)])
    );
    assert_eq!(
        occupants(&s, look(ObjectState::Removed, master_type::COMBAT, false)),
        Ok(vec![object_id(9175)])
    );
    // the view does not look at it: it counts
    assert_eq!(
        s.footprint_occupants(
            FoundationId::new(object_id(10028)),
            &house,
            true,
            &BTreeMap::new()
        ),
        Ok(vec![object_id(9175), object_id(33990)])
    );
}

/// `FUN_1411d4b80` reads a tile's players as the low 32 bits of `word >> 23` and casts the
/// footprint's edges by `CVTTSS2SI`: a NaN place casts to `i32::MIN`, so no tile is read.
#[test]
fn foreign_marks_read_the_players_in_32_bits_and_skip_a_nan_place() {
    let (mut s, mut house) = site();
    // player 2 of 30.0, 88.0: tile (29, 87) marked by player 3, then a bit past 32 players
    s.tiles.mark_for_test(Tile::new(29, 87), 0x80_0000 << 3);
    assert!(s.foreign_marks(&house));
    let (mut t, _) = site();
    t.tiles.mark_for_test(Tile::new(29, 87), 1 << 60);
    assert!(!t.foreign_marks(&house));
    // a NaN place reads nothing, not tile 0 (here marked by player 3)
    s.tiles.mark_for_test(Tile::new(0, 0), 0x80_0000 << 3);
    house.position = Position::new(f32::NAN, f32::NAN, 0.0);
    assert!(!s.foreign_marks(&house));
}

#[test]
fn a_walk_meets_a_static_box_in_its_way_but_not_one_beside_it() {
    // tiles of class 1 everywhere: a unit with mask 1 crosses them all
    let mut s = Spatial::with_tiles(
        GridSize::new(16, 16),
        Tiles::new(crate::map::GridSize::new(16, 16), vec![1; 256]),
    );
    let mut house = villager(8.0, 8.0);
    house.radius = Footprint::new(1.0, 1.0);
    house.obstruction = Some(Obstruction {
        kind: ObstructionType::TYPE_2,
        class: ObstructionClass::BUILDING,
    });
    s.insert(object_id(7), house);
    assert!(s.segment_blocked(
        Point::new(4.0, 8.0),
        Point::new(12.0, 8.0),
        Distance::new(0.2),
        TerrainMask::of_bits(1),
        &[]
    ));
    assert!(!s.segment_blocked(
        Point::new(4.0, 11.0),
        Point::new(12.0, 11.0),
        Distance::new(0.2),
        TerrainMask::of_bits(1),
        &[]
    ));
    // within its radius of the box, and the box ignored
    assert!(s.segment_blocked(
        Point::new(4.0, 9.1),
        Point::new(12.0, 9.1),
        Distance::new(0.2),
        TerrainMask::of_bits(1),
        &[]
    ));
    assert!(!s.segment_blocked(
        Point::new(4.0, 8.0),
        Point::new(12.0, 8.0),
        Distance::new(0.2),
        TerrainMask::of_bits(1),
        &[RecordId::from(object_id(7))]
    ));
    // a moving unit's record never blocks the walk
    s.insert(object_id(8), villager(6.0, 3.0));
    assert!(!s.segment_blocked(
        Point::new(4.0, 3.0),
        Point::new(12.0, 3.0),
        Distance::new(0.2),
        TerrainMask::of_bits(1),
        &[]
    ));
}

#[test]
fn a_walk_meets_a_tile_whose_terrain_the_unit_cannot_cross() {
    let mut classes = vec![1; 256];
    classes[5 * 16 + 9] = 2;
    let mut s = Spatial::with_tiles(
        GridSize::new(16, 16),
        Tiles::new(crate::map::GridSize::new(16, 16), classes),
    );
    assert!(s.segment_blocked(
        Point::new(2.0, 5.5),
        Point::new(14.0, 5.5),
        Distance::new(0.2),
        TerrainMask::of_bits(1),
        &[]
    ));
    assert!(!s.segment_blocked(
        Point::new(2.0, 5.5),
        Point::new(14.0, 5.5),
        Distance::new(0.2),
        TerrainMask::of_bits(3),
        &[]
    ));
    // a walk whose grown box leaves the map
    assert!(s.segment_blocked(
        Point::new(0.1, 5.5),
        Point::new(14.0, 5.5),
        Distance::new(0.2),
        TerrainMask::of_bits(3),
        &[]
    ));
}

/// The old short pather's walks within one `FindShortPath` meet the tile records of its
/// earlier walks: the walk from (6.5, 6.5) with a buffer of 0.5 puts no record on tile
/// (5, 5), but touches the record an earlier walk put there, until the records are cleared.
#[test]
fn a_pather_walk_meets_the_tile_records_of_earlier_walks() {
    let mut classes = vec![1; 64 * 64];
    classes[5 * 64 + 5] = 0;
    let mut s = Spatial::with_tiles(
        GridSize::new(64, 64),
        Tiles::new(crate::map::GridSize::new(64, 64), classes),
    );
    let (a, b) = (Point::new(6.5, 6.5), Point::new(7.5, 7.5));
    assert!(
        !s.pather_blocked(
            a,
            b,
            Some(Distance::new(0.5)),
            TerrainMask::of_bits(1),
            &[],
            crate::world::index::ObjectsRef::empty()
        )
        .unwrap()
    );
    s.clear_tile_records();
    assert!(
        s.pather_blocked(
            Point::new(5.5, 4.0),
            Point::new(5.5, 7.0),
            Some(Distance::new(0.5)),
            TerrainMask::of_bits(1),
            &[],
            crate::world::index::ObjectsRef::empty()
        )
        .unwrap()
    );
    assert_eq!(s.tile_records(), vec![Tile::new(5, 5)]);
    let hits = s
        .pather_hits(
            a,
            b,
            Some(Distance::new(0.5)),
            TerrainMask::of_bits(1),
            &[],
            crate::world::index::ObjectsRef::empty(),
        )
        .unwrap();
    assert_eq!(
        hits,
        [Hit {
            id: tile_record_id(Tile::new(5, 5)),
            bounds: Rect::new(5.0, 5.0, 6.0, 6.0)
        }]
    );
    // the pather's ignore list holds tiles by their record's id
    assert!(
        !s.pather_blocked(
            a,
            b,
            Some(Distance::new(0.5)),
            TerrainMask::of_bits(1),
            &[tile_record_id(Tile::new(5, 5))],
            crate::world::index::ObjectsRef::empty()
        )
        .unwrap()
    );
    s.clear_tile_records();
    assert!(
        !s.pather_blocked(
            a,
            b,
            Some(Distance::new(0.5)),
            TerrainMask::of_bits(1),
            &[],
            crate::world::index::ObjectsRef::empty()
        )
        .unwrap()
    );
}

/// The old short pather's query on a walk whose grown box leaves the map: the first hit is
/// the map's edge record (`FUN_141052350`), the hits list it before the records; a box the
/// walk passes by is no hit, and a record of kind 11 or a gate's meeting the walk is not
/// modelled.
#[test]
fn a_pather_walk_off_the_map_meets_its_edge_first() {
    let mut s = Spatial::with_tiles(
        GridSize::new(16, 16),
        Tiles::new(crate::map::GridSize::new(16, 16), vec![1; 256]),
    );
    let mut house = villager(4.0, 8.0);
    house.radius = Footprint::new(1.0, 1.0);
    house.obstruction = Some(Obstruction {
        kind: ObstructionType::TYPE_2,
        class: ObstructionClass::BUILDING,
    });
    s.insert(object_id(7), house);
    let mut beside = villager(12.0, 2.0);
    beside.obstruction = Some(Obstruction {
        kind: ObstructionType::TYPE_2,
        class: ObstructionClass::BUILDING,
    });
    s.insert(object_id(8), beside);
    let (a, b) = (Point::new(0.2, 8.0), Point::new(6.0, 8.0));
    let edge = s.edge_records(Rect::new(-0.3, 7.5, 6.5, 8.5), QUERY_HITS);
    assert_ne!(edge.len(), 0);
    assert!(
        s.pather_blocked(
            a,
            b,
            Some(Distance::new(0.5)),
            TerrainMask::of_bits(1),
            &[],
            crate::world::index::ObjectsRef::empty()
        )
        .unwrap()
    );
    s.clear_tile_records();
    let hits = s
        .pather_hits(
            a,
            b,
            Some(Distance::new(0.5)),
            TerrainMask::of_bits(1),
            &[],
            crate::world::index::ObjectsRef::empty(),
        )
        .unwrap();
    assert_eq!(hits.len(), edge.len() + 1);
    assert_eq!(hits[..edge.len()], edge[..]);
    assert_eq!(hits[edge.len()].id, RecordId::from(object_id(7)));
    s.clear_tile_records();
    // within the map, the house alone; the box at (12, 2) is passed by
    let hits = s
        .pather_hits(
            Point::new(1.0, 8.0),
            Point::new(6.0, 8.0),
            Some(Distance::new(0.5)),
            TerrainMask::of_bits(1),
            &[],
            crate::world::index::ObjectsRef::empty(),
        )
        .unwrap();
    assert_eq!(
        hits.iter().map(|h| h.id).collect::<Vec<_>>(),
        [RecordId::from(object_id(7))]
    );
    s.clear_tile_records();
    let mut own = villager(4.0, 8.0);
    own.obstruction = Some(Obstruction {
        kind: ObstructionType::TYPE_11,
        class: ObstructionClass::BUILDING,
    });
    s.insert(object_id(7), own);
    assert!(
        s.pather_hits(
            Point::new(1.0, 8.0),
            Point::new(6.0, 8.0),
            Some(Distance::new(0.5)),
            TerrainMask::of_bits(1),
            &[],
            crate::world::index::ObjectsRef::empty()
        )
        .is_err()
    );
    let mut gate = villager(4.0, 8.0);
    gate.class = class::GATE;
    gate.obstruction = Some(Obstruction {
        kind: ObstructionType::TYPE_2,
        class: ObstructionClass::BUILDING,
    });
    s.insert(object_id(7), gate);
    assert!(
        s.pather_hits(
            Point::new(1.0, 8.0),
            Point::new(6.0, 8.0),
            Some(Distance::new(0.5)),
            TerrainMask::of_bits(1),
            &[],
            crate::world::index::ObjectsRef::empty()
        )
        .is_err()
    );
}

/// 426327574's transport at update 309: the smoothing's walk (77.5, 90.5) -> (76.5, 91.5)
/// puts tile (78, 90)'s record, and the next walk from (77.5, 91.5) touches its grown box.
#[test]
fn a_tile_record_blocks_later_walks_until_the_records_are_cleared() {
    let mut classes = vec![1; 128 * 128];
    classes[90 * 128 + 78] = 0;
    classes[87 * 128 + 78] = 0;
    let mut s = Spatial::with_tiles(
        GridSize::new(128, 128),
        Tiles::new(crate::map::GridSize::new(128, 128), classes),
    );
    let (first, second) = (
        (Point::new(77.5, 90.5), Point::new(76.5, 91.5)),
        (Point::new(77.5, 91.5), Point::new(76.5, 92.5)),
    );
    // alone, the second walk's tiles stop at y = 91 and its test removes its records
    assert!(!s.segment_blocked(
        second.0,
        second.1,
        Distance::new(0.5),
        TerrainMask::of_bits(1),
        &[]
    ));
    assert_eq!(s.tile_records(), Vec::<Tile>::new());
    s.put_tile_records(
        first.0,
        first.1,
        Distance::new(0.5),
        TerrainMask::of_bits(1),
    );
    assert_eq!(s.tile_records(), vec![Tile::new(78, 90)]);
    assert!(s.segment_meets(second.0, second.1, Distance::new(0.5), &[]));
    // the record's id in the ignore list leaves it out
    assert!(!s.segment_meets(
        second.0,
        second.1,
        Distance::new(0.5),
        &[tile_record_id(Tile::new(78, 90))]
    ));
    // a tile that holds a record takes no second
    s.put_tile_records(
        second.0,
        Point::new(79.0, 89.0),
        Distance::new(0.5),
        TerrainMask::of_bits(1),
    );
    assert_eq!(s.tile_records(), vec![Tile::new(78, 90)]);
    s.clear_tile_records();
    assert!(!s.segment_meets(second.0, second.1, Distance::new(0.5), &[]));
    // tile (78, 87)'s grown box touches (77.5, 88.5), but its cell (y 80..87) is not one the
    // walk from y 88.5 visits (y from 88)
    s.put_tile_records(
        Point::new(78.5, 86.5),
        Point::new(78.5, 87.5),
        Distance::ZERO,
        TerrainMask::of_bits(1),
    );
    assert_eq!(s.tile_records(), vec![Tile::new(78, 87)]);
    assert!(!s.segment_meets(
        Point::new(77.5, 88.5),
        Point::new(76.5, 89.5),
        Distance::new(0.5),
        &[]
    ));
    assert!(s.segment_meets(
        Point::new(77.5, 88.0),
        Point::new(76.5, 89.5),
        Distance::new(0.5),
        &[]
    ));
    s.clear_tile_records();
    assert_eq!(s.tile_records(), Vec::<Tile>::new());
}

#[test]
fn a_frozen_object_may_be_anywhere_its_speed_could_take_it() {
    let (mut s, house) = site();
    let mut deer = villager(34.0, 88.0);
    deer.moves = true;
    // its master's speed, a tile a second
    let deer_speed = |_, _| Speed::new(1.0);
    s.insert(object_id(9656), deer);
    s.freeze(object_id(9656));
    // frozen at once, the deer is where the save has it
    assert!(
        s.footprint_occupants(
            FoundationId::new(object_id(10028)),
            &house,
            true,
            &BTreeMap::new()
        )
        .is_ok()
    );
    assert!(s.get_known(object_id(9656)).is_ok());
    // its box reaches x 33.8; 3 s later it may stand on the footprint, which ends at 31
    s.grow_unknown(Seconds::new(2.5), deer_speed);
    assert!(
        s.footprint_occupants(
            FoundationId::new(object_id(10028)),
            &house,
            true,
            &BTreeMap::new()
        )
        .is_ok()
    );
    s.grow_unknown(Seconds::new(0.5), deer_speed);
    let e = s
        .footprint_occupants(
            FoundationId::new(object_id(10028)),
            &house,
            true,
            &BTreeMap::new(),
        )
        .unwrap_err();
    assert_eq!(e.context().next(), Some(("object", "9656")));
    let e = s.get_known(object_id(9656)).unwrap_err();
    // the error names the object and its player
    let context: Vec<_> = e.context().take(2).collect();
    assert_eq!(context, [("object", "9656"), ("player", "2")]);
}

#[test]
fn a_frozen_object_that_does_not_move_is_known_unless_a_stopped_unit_may_reach_it() {
    let mut s = Spatial::new(GridSize::new(64, 64));
    let tree = Placed {
        player: PlayerId::new(0),
        moves: false,
        ..villager(24.5, 97.5 - 60.0)
    };
    s.insert(object_id(9365), tree);
    s.freeze(object_id(9365));
    s.reach = Distance::new(1.0);
    let near = Rect::new(24.0, 37.0, 24.6, 38.0);
    assert!(s.check_known(near).is_ok());
    // a stopped villager 10 tiles off, 0.8 tiles a second (every master's speed here: the tree
    // does not move, so its place stays known)
    let villager_speed = |_, _| Speed::new(0.8);
    let mut walker = villager(34.5, 37.5);
    walker.player = PlayerId::new(1);
    walker.moves = true;
    s.insert(object_id(9200), walker);
    s.freeze(object_id(9200));
    s.grow_unknown(Seconds::new(10.0), villager_speed);
    assert!(s.check_known(near).is_ok());
    s.grow_unknown(Seconds::new(2.0), villager_speed);
    assert!(s.check_known(near).is_err());
}

/// `FUN_1410562e0`'s list stays with the obstruction manager: the walk's query and the stand
/// tests set it (`FUN_14114cc60`, `FUN_141158720`, `FUN_141158940`), and it holds until the
/// next set; the objects' part of the point query leaves the edge's records out.
#[test]
fn the_manager_keeps_the_last_ignore_list_set() {
    let mut s = Spatial::new(GridSize::new(10, 10));
    assert_eq!(s.manager_ignore(), Vec::<RecordId>::new());
    let rect = Rect::new(2.0, 5.0, 3.0, 5.0);
    let _ = s
        .walk_hits(WalkQuery {
            objects: crate::world::index::ObjectsRef::empty(),
            start: Point::new(2.0, 5.0),
            end: Point::new(3.0, 5.0),
            rect,
            grow: Distance::new(0.2),
            ally_factor: 1.0,
            player: None,
            ignore: &[RecordId::from(object_id(7)), RecordId::from(object_id(8))],
        })
        .unwrap();
    assert_eq!(
        s.manager_ignore(),
        [RecordId::from(object_id(7)), RecordId::from(object_id(8))]
    );
    assert!(
        s.point_clear(
            object_id(3),
            Point::new(4.5, 4.5),
            Distance::new(0.1),
            TerrainMask::ALL,
            crate::world::index::ObjectsRef::empty()
        )
        .unwrap()
    );
    assert_eq!(s.manager_ignore(), [RecordId::from(object_id(3))]);
    assert!(
        s.point_clear_among(
            object_id(4),
            Point::new(4.5, 4.5),
            Distance::new(0.1),
            TerrainMask::ALL,
            &[],
            crate::world::index::ObjectsRef::empty()
        )
        .unwrap()
    );
    assert_eq!(s.manager_ignore(), [RecordId::from(object_id(4))]);
    // a point off the map's tiles sets nothing
    assert!(
        !s.point_clear(
            object_id(5),
            Point::new(-1.0, 4.5),
            Distance::new(0.1),
            TerrainMask::ALL,
            crate::world::index::ObjectsRef::empty()
        )
        .unwrap()
    );
    assert_eq!(s.manager_ignore(), [RecordId::from(object_id(4))]);
    s.set_manager_ignore(&[]);
    assert_eq!(s.manager_ignore(), Vec::<RecordId>::new());
    assert!(
        s.point_hit(
            Point::new(0.05, 4.5),
            Distance::new(0.1),
            &[],
            false,
            crate::world::index::ObjectsRef::empty()
        )
        .unwrap()
    );
    assert!(
        !s.object_holds(
            Point::new(0.05, 4.5),
            Distance::new(0.1),
            &[],
            false,
            crate::world::index::ObjectsRef::empty()
        )
        .unwrap()
    );
}

/// A `ClosestObject` filter that keeps everything: every argument −1, no reach test.
const fn any_filter() -> Filter {
    Filter {
        group: None,
        relation: None,
        state: None,
        master: None,
        workers: crate::objects::WorkerCount::of_data(-1),
        spared: None,
        reach: false,
    }
}

/// The sight of a combat unit of player 2 in `state`.
const fn seer_in(state: ObjectState) -> crate::vision::visibility::Seer {
    crate::vision::visibility::Seer {
        player: PlayerId::new(2),
        master: ids::ID_0,
        los: Distance::new(4.0),
        state,
        explored: true,
        building: None,
        umv: 0,
        capture: CaptureKind::NONE,
        kind: master_type::COMBAT,
    }
}

/// The nearest so far gives way to a nearer record, to an equally near one of a lower id and
/// after a NaN distance; it stays before a farther record and an equally near one of a higher
/// id.
#[test]
fn closest_object_keeps_the_nearest_record_and_on_a_tie_the_lower_id() {
    use crate::map::spatial::search::nearer;
    let record = |id, distance| Record {
        id,
        distance,
        owner: PlayerId::new(1),
        at: Point::new(0.0, 0.0),
        radius: Footprint::new(0.5, 0.5),
    };
    assert_eq!(
        nearer(None, &record(object_id(5), Distance::new(3.0))),
        Some((object_id(5), 3.0))
    );
    assert_eq!(
        nearer(
            Some((object_id(5), 3.0)),
            &record(object_id(6), Distance::new(2.0))
        ),
        Some((object_id(6), 2.0))
    );
    assert_eq!(
        nearer(
            Some((object_id(5), 3.0)),
            &record(object_id(4), Distance::new(3.0))
        ),
        Some((object_id(4), 3.0))
    );
    assert_eq!(
        nearer(
            Some((object_id(5), 3.0)),
            &record(object_id(6), Distance::new(3.0))
        ),
        Some((object_id(5), 3.0))
    );
    assert_eq!(
        nearer(
            Some((object_id(5), 3.0)),
            &record(object_id(4), Distance::new(4.0))
        ),
        Some((object_id(5), 3.0))
    );
    assert_eq!(
        nearer(
            Some((object_id(5), f32::NAN)),
            &record(object_id(6), Distance::new(4.0))
        ),
        Some((object_id(6), 4.0))
    );
}

/// `ClosestObject`'s tests past the relation: the filter's class, master and state keep or
/// pass an object; a workers cap and the unbuilt farm test fail; livestock counts when the
/// searcher's player owns it or it is not in state 2, and not while it moves; a searcher of
/// class 10 passes what it shuns.
#[test]
fn closest_object_keeps_what_its_filter_and_its_searcher_take() {
    use crate::map::spatial::search::{Searcher, closest_kept};
    let (still, moving) = (|_: ObjectId| false, |_: ObjectId| true);
    let (takes, shuns) = (|_: &Placed| Ok(false), |_: &Placed| Ok(true));
    let fails = |_: &Placed| Err(GenieError::unsupported("a test's shunning"));
    let farm = |_: ObjectId| None;
    let checks: ClosestChecks<'_> = (&still, &takes, &farm);
    let p = villager(1.0, 1.0);
    let kept = |filter, p: &Placed, state, searcher, checks| {
        closest_kept(
            object_id(7),
            (&seer_in(state), p),
            filter,
            (PlayerId::new(1), searcher),
            checks,
        )
    };
    let plain = Searcher::default();
    let with = |f: fn(&mut Filter)| {
        let mut filter = any_filter();
        f(&mut filter);
        filter
    };
    assert!(kept(any_filter(), &p, ObjectState::Alive, plain, checks).unwrap());
    assert!(
        kept(
            with(|f| f.group = Some(class_id(4))),
            &p,
            ObjectState::Alive,
            plain,
            checks
        )
        .unwrap()
    );
    assert!(
        !kept(
            with(|f| f.group = Some(class_id(3))),
            &p,
            ObjectState::Alive,
            plain,
            checks
        )
        .unwrap()
    );
    assert!(
        !kept(
            with(|f| f.master = Some(master_id(5))),
            &p,
            ObjectState::Alive,
            plain,
            checks
        )
        .unwrap()
    );
    assert!(
        !kept(
            with(|f| f.state = Some(ObjectState::Dying)),
            &p,
            ObjectState::Alive,
            plain,
            checks
        )
        .unwrap()
    );
    assert_eq!(
        kept(
            with(|f| f.workers = crate::objects::WorkerCount::of_data(1)),
            &p,
            ObjectState::Alive,
            plain,
            checks
        )
        .unwrap_err()
        .reason(),
        "ClosestObject's workers cap (+0x644)"
    );
    assert_eq!(
        kept(
            with(|f| f.state = Some(ObjectState::Foundation)),
            &p,
            ObjectState::Foundation,
            plain,
            checks
        )
        .unwrap_err()
        .reason(),
        "ClosestObject's unbuilt farm test"
    );
    let livestock = with(|f| f.group = Some(class_id(58)));
    let mut sheep = Placed {
        class: class::LIVESTOCK,
        ..p
    };
    assert!(!kept(livestock, &sheep, ObjectState::Alive, plain, checks).unwrap());
    assert!(kept(livestock, &sheep, ObjectState::Unfinished, plain, checks).unwrap());
    sheep.player = PlayerId::new(1);
    assert!(kept(livestock, &sheep, ObjectState::Alive, plain, checks).unwrap());
    assert!(
        !kept(
            livestock,
            &sheep,
            ObjectState::Alive,
            plain,
            (&moving, &takes, &farm)
        )
        .unwrap()
    );
    let wolf = Searcher {
        class_10: true,
        ..plain
    };
    assert!(kept(any_filter(), &p, ObjectState::Alive, wolf, checks).unwrap());
    assert!(
        !kept(
            any_filter(),
            &p,
            ObjectState::Alive,
            wolf,
            (&still, &shuns, &farm)
        )
        .unwrap()
    );
    assert!(
        kept(
            any_filter(),
            &p,
            ObjectState::Alive,
            plain,
            (&still, &shuns, &farm)
        )
        .unwrap()
    );
    assert!(
        kept(
            any_filter(),
            &p,
            ObjectState::Alive,
            wolf,
            (&still, &fails, &farm)
        )
        .is_err()
    );
}

/// A farm `ClosestObject` found: a free one the searcher gathers from (a fish trap for a
/// fishing ship, any other for a villager) counts; a worked one only with a master asked for;
/// a pasture's livestock never; a pasture and a farm the caller does not hold fail.
#[test]
fn closest_object_takes_a_free_farm_its_searcher_gathers_from() {
    use crate::map::spatial::search::{Searcher, closest_kept, farm_kept};
    let filter = Filter {
        group: Some(class_id(49)),
        ..any_filter()
    };
    let farm_of = |master| Placed {
        class: class::FARM,
        master,
        ..villager(1.0, 1.0)
    };
    let look = |id: ObjectId| match id.get() {
        1 => Some((crate::objects::WorkerCount::ZERO, None)),
        2 => Some((crate::objects::WorkerCount::ONE, None)),
        3 => Some((
            crate::objects::WorkerCount::ZERO,
            Some(crate::gathering::PastureId::new(object_id(9))),
        )),
        _ => None,
    };
    let farmer = Searcher {
        farmer: true,
        ..Searcher::default()
    };
    let trapper = Searcher {
        trapper: true,
        ..Searcher::default()
    };
    let (farm, trap) = (farm_of(ids::FARM), farm_of(FISH_TRAP));
    assert!(farm_kept(object_id(1), &farm, filter, farmer, &look).unwrap());
    assert!(!farm_kept(object_id(1), &farm, filter, trapper, &look).unwrap());
    assert!(farm_kept(object_id(1), &trap, filter, trapper, &look).unwrap());
    assert!(!farm_kept(object_id(1), &trap, filter, farmer, &look).unwrap());
    assert!(!farm_kept(object_id(2), &farm, filter, farmer, &look).unwrap());
    let asked = Filter {
        master: Some(master_id(50)),
        ..filter
    };
    assert!(farm_kept(object_id(2), &farm, asked, Searcher::default(), &look).unwrap());
    assert!(!farm_kept(object_id(3), &farm, filter, farmer, &look).unwrap());
    assert_eq!(
        farm_kept(object_id(1), &farm_of(PASTURES[0]), filter, farmer, &look)
            .unwrap_err()
            .reason(),
        "ClosestObject over pastures (their workers and livestock)"
    );
    assert_eq!(
        farm_kept(object_id(4), &farm, filter, farmer, &look)
            .unwrap_err()
            .reason(),
        "a found farm the slice does not hold"
    );
    // through the tests past the relation: the worked farm passed, the free one kept
    let (still, takes) = (|_: ObjectId| false, |_: &Placed| Ok(false));
    let checks: ClosestChecks<'_> = (&still, &takes, &look);
    let s = seer_in(ObjectState::Alive);
    assert!(
        closest_kept(
            object_id(1),
            (&s, &farm),
            filter,
            (PlayerId::new(1), farmer),
            checks
        )
        .unwrap()
    );
    assert!(
        !closest_kept(
            object_id(2),
            (&s, &farm),
            filter,
            (PlayerId::new(1), farmer),
            checks
        )
        .unwrap()
    );
}

/// Player `p`'s relation toward `q` in the spatial copy becomes `r`.
fn set_relation(s: &mut Spatial, (p, q): (u8, u8), r: Relation) {
    s.relations
        .get_mut(PlayerId::new(p))
        .unwrap()
        .set(PlayerId::new(q), r);
}
