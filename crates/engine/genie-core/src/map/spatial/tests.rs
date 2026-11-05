use super::*;
use crate::construction::FoundationId;
use crate::gathering::PastureId;
use crate::map::GridSize;
use crate::map::TerrainMask;
use crate::map::Tile;
use crate::map::spatial::ClosestWorld;
use crate::map::spatial::Movers;
use crate::map::spatial::TileRecords;
use crate::map::{Rect, Segment};
use crate::movement::Speed;
use crate::objects::HitPoints;
use crate::objects::ObjectState;
use crate::objects::actions::ActionList;
use crate::objects::layers::Action;
use crate::objects::masters::BuildableOn;
use crate::objects::masters::Master;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObjectFlags;
use crate::ownership::CaptureKind;
use crate::ownership::artifact::Artifact;
use crate::players::Amount;
use crate::players::ByPlayer;
use crate::players::Relation;
use crate::players::attribute::AttributeId;
use crate::players::game_status::GameStatus;
use crate::players::relation_row;
use crate::players::relations;
use crate::test_ids::class_id;
use crate::test_ids::{master_id, object_id};
use crate::testing::class;
use crate::testing::ids;
use crate::testing::ids::FISH_TRAP;
use crate::testing::roles;
use crate::testing::scene::{Scene, SceneObject};
use crate::testing::tuning;
use crate::unit_ai::Memory;
use crate::vision::VisionBits;
use crate::vision::own_masks;
use crate::vision::registry::UnitUpdate;
use crate::vision::visibility::Seer;
use crate::world::World;
use crate::world::index::TestObjects;
use crate::world::objects_ref::ObjectsRef;

/// The record cells (FD-030-D03) give every query the objects whose record is filed in a
/// cell it visits, as the full scan's cell test does, in id order: on and off the map, on
/// every level, while objects come, move, change masters, get and drop records and go.
#[test]
fn the_record_cells_answer_as_the_full_scan_does() {
    let mut records = Spatial::new(GridSize::new(120, 96));
    let mut seed = 0x2545_f491_u32;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    let kinds = [1u8, 2, 3, 5, 10, 11, 13].map(ObstructionKind::from);
    let placed = |position: Point, radius: f32, kind: Option<ObstructionKind>| Placed {
        position: position.with_height(0.0),
        radius: Footprint::new(radius, radius * 0.5),
        class: class::CLASS_0,
        player: PlayerId::new(1),
        master: master_id(4),
        obstruction_kind: ObstructionKind::NONE,
        buildable_on: BuildableOn::NO,
        tall: false,
        moves: false,
        unknown: None,
        obstruction: kind.map(|kind| Obstruction {
            kind,
            class: ObstructionClass::NONE,
        }),
    };
    let scan = |spatial: &Spatial, rect: Rect| -> Vec<ObjectId> {
        spatial
            .objects
            .iter()
            .filter(|(_, placed)| {
                placed
                    .obstruction
                    .is_some_and(|obstruction| spatial.visits(placed, obstruction.kind, rect))
            })
            .map(|(id, _)| id)
            .collect()
    };
    let at = |next: &mut dyn FnMut() -> u32| (next() % 560) as f32 * 0.25 - 8.0;
    for step in 0..6000 {
        let id = (next() % 300) as i32;
        let position = Point::new(at(&mut next), at(&mut next));
        let radius = (next() % 48) as f32 * 0.25;
        let kind = (next() % 8 != 0).then(|| kinds[(next() % 7) as usize]);
        match next() % 7 {
            0 | 1 => records.insert(object_id(id), placed(position, radius, kind)),
            2 | 3 => records
                .set_position(
                    object_id(id),
                    position.with_height(0.0),
                    &roles(),
                    tuning().visibility,
                )
                .unwrap(),
            4 => {
                let here = records
                    .objects
                    .get(object_id(id))
                    .map_or(position, |placed| placed.position.ground());
                records.change_master(object_id(id), placed(here, radius, kind));
            }
            5 => match kind {
                Some(kind) => records.add_record(
                    object_id(id),
                    Obstruction {
                        kind,
                        class: ObstructionClass::NONE,
                    },
                ),
                None => records.drop_record(object_id(id)),
            },
            _ => records.remove(object_id(id)),
        }
        let (x, y) = (at(&mut next), at(&mut next));
        let (width, height) = ((next() % 160) as f32 * 0.5, (next() % 160) as f32 * 0.5);
        let rect = Rect::new(x, y, x + width, y + height);
        assert_eq!(
            records.visited_records(rect),
            scan(&records, rect),
            "step {step}, {rect:?}"
        );
    }
}

#[test]
fn query_takes_its_kinds_of_players_from_the_save() {
    // player 2's +0x278 as loaded: itself 1, Gaia 0, player 1 an ally (2) whatever its
    // diplomacy byte (2 here, which no derivation knew)
    let mut spatial = Spatial::new(GridSize::new(10, 10));
    spatial.relations = ByPlayer::new(vec![
        relation_row([1, 4, 4]),
        relation_row([1, 0, 4]),
        relation_row([1, 2, 0]),
    ]);
    spatial.unit_diplomacy = ByPlayer::new(vec![
        relations([1, 4, 4]),
        relations([0, 1, 4]),
        relations([0, 2, 1]),
    ]);
    assert_eq!(
        spatial.query_kinds(PlayerId::new(2), 3),
        Ok(relations([0, 2, 1]))
    );
    assert_eq!(
        spatial.query_kinds(PlayerId::new(2), 2),
        Ok(relations([0, 2]))
    );
    assert!(spatial.query_kinds(PlayerId::new(3), 3).is_err());
}

/// `FUN_1410576a0`: counter-clockwise 1, clockwise -1, on the segment 0, beyond its end 1;
/// a NaN falls through every COMISS/JA to 0.
#[test]
fn ccw_answers_as_the_game_compares() {
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
fn walk_meets_box_by_its_sides_or_from_inside() {
    let rect = Rect::new(0.0, 0.0, 1.0, 1.0);
    assert!(box_meets_segment(
        rect,
        Distance::ZERO,
        walk(f32::NAN, 0.5, 5.0, 5.0)
    ));
    assert!(box_meets_segment(
        rect,
        Distance::ZERO,
        walk(0.5, 0.5, 0.6, 0.6)
    ));
    assert!(!box_meets_segment(
        rect,
        Distance::ZERO,
        walk(3.0, 3.0, 5.0, 5.0)
    ));
}

/// `FUN_141051630` (and `FUN_141053130`): a circle record's box is read as it stands, so a
/// box whose x1 lies below its x0 holds no walk inside it; `FUN_1410512f0` orders it first.
#[test]
fn circle_record_box_is_read_unordered_and_another_ordered() {
    let rect = Rect::new(2.0, 0.0, 1.0, 1.0);
    let inside = walk(1.4, 0.4, 1.6, 0.6);
    assert!(record_meets_segment(
        ObstructionKind::KIND_2,
        rect,
        Distance::ZERO,
        inside
    ));
    assert!(!record_meets_segment(
        ObstructionKind::CIRCLE,
        rect,
        Distance::ZERO,
        inside
    ));
    assert!(record_meets_segment(
        ObstructionKind::CIRCLE,
        Rect::new(1.0, 0.0, 2.0, 1.0),
        Distance::ZERO,
        inside
    ));
    // grown, the edges and the inside tests take the grow on the box as it stands: the
    // edges then run at x 1.25 and 1.75, the inside tests hold nothing
    assert!(!record_meets_segment(
        ObstructionKind::CIRCLE,
        rect,
        Distance::new(0.25),
        inside
    ));
    assert!(record_meets_segment(
        ObstructionKind::CIRCLE,
        rect,
        Distance::new(0.25),
        walk(1.0, 0.5, 1.5, 0.5)
    ));
    assert!(record_meets_segment(
        ObstructionKind::KIND_2,
        rect,
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
        obstruction_kind: ObstructionKind::NONE,
        buildable_on: BuildableOn::NO,
        tall: true,
        moves: false,
        unknown: None,
        obstruction: Some(Obstruction {
            kind: ObstructionKind::CIRCLE,
            class: ObstructionClass::UNIT,
        }),
    }
}

#[test]
fn search_near_the_last_reads_its_answer_until_unit_ai_lets_it_go() {
    use crate::unit_ai::{Pick, Search};
    use crate::vision::registry::Registry;
    let mut records = Spatial::new(GridSize::new(120, 120));
    // player 2 counts player 1 its enemy (3)
    records.relations = ByPlayer::new(vec![
        relation_row([0, 3, 3]),
        relation_row([0, 0, 3]),
        relation_row([0, 3, 0]),
    ]);
    records.unit_diplomacy = ByPlayer::new(vec![
        relations([1, 4, 4]),
        relations([0, 1, 4]),
        relations([0, 4, 1]),
    ]);
    let mut registry = Registry::new(3, 65);
    // a villager of player 1 at (12, 10), which player 2 sees
    registry
        .update_unit(UnitUpdate {
            id: object_id(1),
            owner: PlayerId::new(1),
            position: Point::new(12.0, 10.0),
            radii: Footprint::new(0.2, 0.2),
            class: class::VILLAGER,
            old: VisionBits::NONE,
            new: VisionBits::from_bits_retain(1 << 2),
        })
        .unwrap();
    let mut visibility = Visibility::new(
        GridSize::new(120, 120),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        0,
        Vec::new(),
    );
    visibility.registry = Some(registry);
    records.sight = Some(visibility);
    let villagers = [class::VILLAGER];
    let look = |at| Search {
        at,
        radius: Distance::new(3.0),
        classes: &villagers,
        size: Footprint::new(0.2, 0.2),
        pick: Pick::Look,
        memory: Memory::default(),
    };
    let mut reach = |_: &mut Spatial, _: ObjectId, _: Distance| Ok(true);
    let mut search = |spatial: &mut Spatial, at| {
        let found = spatial
            .search(
                SearchQuery {
                    searcher: object_id(9),
                    player: PlayerId::new(2),
                    search: &look(at),
                    attack: None,
                    reach: &mut reach,
                    objects: ObjectsRef::empty(),
                    closest: closest_world(),
                },
                &roles(),
                &tuning().best_target,
                &tuning().spatial,
                tuning().registry,
            )
            .unwrap();
        (found.enemies, found.fresh)
    };
    assert_eq!(search(&mut records, Point::new(10.0, 10.0)), (true, true));
    // 4 tiles on (16 squared, within 25) the query is skipped: the enemy stays found
    assert_eq!(search(&mut records, Point::new(10.0, 14.0)), (true, false));
    // a unit AI's full update lets the cache go: the query runs, and finds nothing there
    records.forget_search();
    assert_eq!(search(&mut records, Point::new(10.0, 14.0)), (false, true));
    // 6 tiles from the last query (36 squared) it runs again
    assert_eq!(search(&mut records, Point::new(10.0, 8.0)), (true, true));
}

#[test]
fn rectangle_leaving_the_map_meets_the_edge_records_where_its_diagonal_crosses() {
    let mut spatial = Spatial::new(GridSize::new(120, 120));
    assert_eq!(
        spatial.edge_records(Rect::new(0.0, 0.0, 120.0, 120.0), 25),
        Vec::<Hit>::new()
    );
    // the diagonal (-0.3, 50) -> (0.3, 50.6) crosses x 0 at y 50.3: the left edge's tile (0, 50)
    let left = spatial.edge_records(Rect::new(-0.3, 50.0, 0.3, 50.6), 25);
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].id.value() as u32, 0xff00_0000 | 0x32);
    assert_eq!(left[0].bounds, Rect::new(0.0, 50.0, 1.0, 51.0));
    // past the bottom right corner: (119.5, 119.7) -> (120.5, 120.7) crosses x 120 at y 120.2
    // and y 120 at x 119.8
    let corner = spatial.edge_records(Rect::new(119.5, 119.7, 120.5, 120.7), 25);
    let ids: Vec<u32> = corner.iter().map(|hit| hit.id.value() as u32).collect();
    assert_eq!(ids, [0xff07_8078, 0xff07_7078]);
    // a list with room for one stops after the first edge
    assert_eq!(
        spatial.edge_records(Rect::new(119.5, 119.7, 120.5, 120.7), 1),
        corner[..1]
    );
    // an infinite end: the crossing's terms give NaN, which CVTTSS2SI makes i32::MIN
    let far = spatial.edge_records(Rect::new(-0.5, 10.0, f32::INFINITY, 10.5), 25);
    assert_eq!(far.len(), 2);
    assert_eq!(far[0].id.value() as u32, 0xff00_000a);
    assert_eq!(
        far[0].bounds,
        Rect::new(-2_147_483_648.0, 10.0, -2_147_483_648.0, 11.0)
    );
    // a unit of radius 0.3 cannot stand 0.2 from the edge
    assert!(
        !spatial
            .point_clear(
                object_id(1),
                Point::new(0.2, 50.0),
                Distance::new(0.3),
                TerrainMask::ALL,
                ObjectsRef::empty()
            )
            .unwrap()
    );
    assert!(
        spatial
            .point_clear(
                object_id(1),
                Point::new(0.4, 50.0),
                Distance::new(0.3),
                TerrainMask::ALL,
                ObjectsRef::empty()
            )
            .unwrap()
    );
}

/// `FormationTransform`'s line query: a standing villager on the line is hit, but not when
/// ignored or moving; a tile the mask lacks is hit once the formation put its record (its
/// box, [`Spatial::put_tile_records`]); a line leaving the map meets the edge; the buffer
/// the formation finds grows the records.
#[test]
fn still_line_meets_standing_records_and_the_temporary_tiles() {
    let mut objects = TestObjects::default();
    let mut classes = vec![1u8; 100];
    classes[2 * 10 + 7] = 2;
    let mut records = Spatial::with_tiles(
        GridSize::new(10, 10),
        Tiles::new(GridSize::new(10, 10), classes),
    );
    records.insert(object_id(7), villager(4.0, 5.0));
    let line = |spatial: &Spatial, ignore: &[RecordId], objects: &TestObjects| {
        spatial
            .still_line_hit(
                Point::new(1.0, 5.0),
                Point::new(8.0, 5.0),
                Distance::ZERO,
                ignore,
                objects.view(),
            )
            .unwrap()
    };
    assert!(line(&records, &[], &objects));
    assert!(!line(&records, &[RecordId::from(object_id(7))], &objects));
    objects.set_gait(
        object_id(7),
        Some(Gait {
            moving: true,
            ..Gait::default()
        }),
    );
    assert!(!line(&records, &[], &objects));
    // the class-2 tile (7, 2) once its record is put, for a mask that lacks its class
    let across = |spatial: &Spatial| {
        spatial
            .still_line_hit(
                Point::new(7.5, 1.0),
                Point::new(7.5, 4.0),
                Distance::ZERO,
                &[],
                objects.view(),
            )
            .unwrap()
    };
    assert!(!across(&records));
    records.put_tile_records(
        Point::new(6.0, 1.0),
        Point::new(8.0, 3.0),
        Distance::ZERO,
        TerrainMask::of_bits(3),
    );
    assert!(!across(&records));
    records.put_tile_records(
        Point::new(6.0, 1.0),
        Point::new(8.0, 3.0),
        Distance::ZERO,
        TerrainMask::of_bits(1),
    );
    assert!(across(&records));
    records.clear_tile_records();
    assert!(!across(&records));
    assert!(
        records
            .still_line_hit(
                Point::new(1.0, 5.0),
                Point::new(10.5, 5.0),
                Distance::ZERO,
                &[],
                objects.view()
            )
            .unwrap()
    );
    // a line 0.3 beside the standing villager (radius 0.2): met only with a buffer of 0.2
    let mut spatial = Spatial::with_tiles(
        GridSize::new(10, 10),
        Tiles::new(GridSize::new(10, 10), vec![1; 100]),
    );
    spatial.insert(object_id(8), villager(4.0, 5.0));
    assert!(
        !spatial
            .still_line_hit(
                Point::new(1.0, 5.3),
                Point::new(8.0, 5.3),
                Distance::ZERO,
                &[],
                objects.view()
            )
            .unwrap()
    );
    assert!(
        spatial
            .still_line_hit(
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
fn point_is_clear_among_the_ignored_objects() {
    let mut classes = vec![1u8; 100];
    classes[5 * 10 + 4] = 2;
    let mut spatial = Spatial::with_tiles(
        GridSize::new(10, 10),
        Tiles::new(GridSize::new(10, 10), classes),
    );
    spatial.insert(object_id(7), villager(4.0, 6.5));
    assert!(
        !spatial
            .point_clear_among(
                object_id(1),
                Point::new(4.0, 6.5),
                Distance::new(0.1),
                TerrainMask::of_bits(1),
                &[],
                ObjectsRef::empty()
            )
            .unwrap()
    );
    assert!(
        spatial
            .point_clear_among(
                object_id(1),
                Point::new(4.0, 6.5),
                Distance::new(0.1),
                TerrainMask::of_bits(1),
                &[object_id(3), object_id(7)],
                ObjectsRef::empty()
            )
            .unwrap()
    );
    assert!(
        spatial
            .point_clear_among(
                object_id(7),
                Point::new(4.0, 6.5),
                Distance::new(0.1),
                TerrainMask::of_bits(1),
                &[],
                ObjectsRef::empty()
            )
            .unwrap()
    );
    assert!(
        !spatial
            .point_clear_among(
                object_id(7),
                Point::new(4.0, 6.5),
                Distance::new(0.1),
                TerrainMask::of_bits(1),
                &[object_id(3)],
                ObjectsRef::empty()
            )
            .unwrap()
    );
    // the class-2 tile (4, 5) and a point off the map
    assert!(
        !spatial
            .point_clear_among(
                object_id(7),
                Point::new(4.5, 5.5),
                Distance::new(0.1),
                TerrainMask::of_bits(1),
                &[],
                ObjectsRef::empty()
            )
            .unwrap()
    );
    assert!(
        spatial
            .point_clear_among(
                object_id(7),
                Point::new(4.5, 5.5),
                Distance::new(0.1),
                TerrainMask::of_bits(3),
                &[],
                ObjectsRef::empty()
            )
            .unwrap()
    );
    assert!(
        !spatial
            .point_clear_among(
                object_id(7),
                Point::new(-0.5, 5.5),
                Distance::new(0.1),
                TerrainMask::of_bits(3),
                &[],
                ObjectsRef::empty()
            )
            .unwrap()
    );
    // CVTTSS2SI: a NaN coordinate is i32::MIN, off the map, not tile 0
    assert!(
        !spatial
            .point_clear_among(
                object_id(7),
                Point::new(f32::NAN, 5.5),
                Distance::new(0.1),
                TerrainMask::of_bits(1),
                &[],
                ObjectsRef::empty()
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
fn line_meets_the_records_and_the_tiles_its_mask_lacks() {
    let mut classes = vec![1u8; 100];
    classes[5 * 10 + 5] = 2;
    let mut spatial = Spatial::with_tiles(
        GridSize::new(10, 10),
        Tiles::new(GridSize::new(10, 10), classes),
    );
    spatial.insert(object_id(7), villager(4.0, 5.5));
    spatial.set_manager_buffer(Some(Distance::new(0.2)));
    let hits = spatial
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
    assert_eq!(spatial.manager_ignore(), Vec::<RecordId>::new());
    assert_eq!(spatial.manager_buffer(), None);
    assert_eq!(spatial.tile_records(), Vec::<Tile>::new());
    let none = spatial
        .line_query(
            Point::new(2.0, 5.5),
            Point::new(8.0, 5.5),
            Distance::new(0.2),
            None,
            &[RecordId::from(object_id(7))],
        )
        .unwrap();
    assert_eq!(none, Vec::<RecordId>::new());
    assert_eq!(spatial.manager_ignore(), [RecordId::from(object_id(7))]);
    // a record still put counts without the routine's own; the pather's test leaves them out
    spatial.put_tile_records(
        Point::new(5.5, 5.5),
        Point::new(5.5, 5.5),
        Distance::ZERO,
        TerrainMask::of_bits(1),
    );
    let kept = spatial
        .line_query(
            Point::new(2.0, 5.5),
            Point::new(8.0, 5.5),
            Distance::new(0.2),
            None,
            &[RecordId::from(object_id(7))],
        )
        .unwrap();
    assert_eq!(kept, [tile_record_id(Tile::new(5, 5))]);
    let movers = spatial
        .line_hits(
            Point::new(2.0, 5.5),
            Point::new(8.0, 5.5),
            Distance::new(0.2),
            TileRecords::Passed,
            &[RecordId::from(object_id(7))],
        )
        .unwrap();
    assert_eq!(movers, Vec::<RecordId>::new());
    spatial.clear_tile_records();
    let edge = spatial
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
    let mut spatial = Spatial::with_tiles(
        GridSize::new(10, 10),
        Tiles::new(GridSize::new(10, 10), classes),
    );
    assert_eq!(
        (spatial.manager_buffer(), spatial.manager_grow()),
        (None, Distance::ZERO)
    );
    spatial.set_manager_buffer(Some(Distance::new(0.1)));
    assert_eq!(spatial.manager_grow(), Distance::new(0.1));
    assert!(
        spatial
            .point_clear(
                object_id(1),
                Point::new(6.5, 6.5),
                Distance::new(0.2),
                TerrainMask::of_bits(1),
                ObjectsRef::empty()
            )
            .unwrap()
    );
    assert_eq!(spatial.manager_buffer(), None);
    spatial.set_manager_buffer(Some(Distance::new(0.1)));
    let _ = spatial
        .walk_hits(WalkQuery {
            objects: ObjectsRef::empty(),
            start: Point::new(2.0, 2.0),
            end: Point::new(3.0, 2.0),
            rect: Rect::new(2.0, 2.0, 3.0, 2.0),
            grow: Distance::new(0.2),
            ally_factor: 1.0,
            player: None,
            ignore: &[],
        })
        .unwrap();
    assert_eq!(spatial.manager_buffer(), None);
    // a point query near tile (4, 5) meets its record once it is put, within the grow
    assert!(
        !spatial
            .point_hit(
                Point::new(3.95, 5.5),
                Distance::new(0.1),
                &[],
                Movers::LeftOut,
                ObjectsRef::empty()
            )
            .unwrap()
    );
    spatial.put_tile_records(
        Point::new(4.5, 5.5),
        Point::new(4.5, 5.5),
        Distance::ZERO,
        TerrainMask::of_bits(1),
    );
    assert!(
        spatial
            .point_hit(
                Point::new(3.95, 5.5),
                Distance::new(0.1),
                &[],
                Movers::LeftOut,
                ObjectsRef::empty()
            )
            .unwrap()
    );
    assert!(
        !spatial
            .point_hit(
                Point::new(3.85, 5.5),
                Distance::new(0.1),
                &[],
                Movers::LeftOut,
                ObjectsRef::empty()
            )
            .unwrap()
    );
    assert!(
        !spatial
            .point_hit(
                Point::new(3.95, 5.5),
                Distance::new(0.1),
                &[tile_record_id(Tile::new(4, 5))],
                Movers::LeftOut,
                ObjectsRef::empty()
            )
            .unwrap()
    );
    // a stand test counts it too
    assert!(
        !spatial
            .point_clear_among(
                object_id(1),
                Point::new(3.95, 5.5),
                Distance::new(0.1),
                TerrainMask::of_bits(1),
                &[],
                ObjectsRef::empty()
            )
            .unwrap()
    );
    spatial.clear_tile_records();
    assert!(
        spatial
            .point_clear_among(
                object_id(1),
                Point::new(3.95, 5.5),
                Distance::new(0.1),
                TerrainMask::of_bits(1),
                &[],
                ObjectsRef::empty()
            )
            .unwrap()
    );
}

/// The walk's query beyond the map's edge meets the edge's record first (`FUN_141057290`),
/// then the objects as before.
#[test]
fn walk_past_the_maps_edge_meets_its_record_first() {
    let mut spatial = Spatial::new(GridSize::new(10, 10));
    spatial.insert(object_id(7), villager(1.0, 5.5));
    let (from, to) = (Point::new(0.1, 5.5), Point::new(2.0, 5.5));
    let rect = Rect::new(0.1, 5.5, 2.0, 5.5);
    let hits = spatial
        .walk_hits(WalkQuery {
            objects: ObjectsRef::empty(),
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
fn units_box_is_its_position_plus_and_minus_its_radii() {
    // MarvinBlue/426048731 tick 680: 9177's record box in the game's obstruction manager
    assert_eq!(
        villager(28.0, 89.0).bounds(),
        Rect::new(27.8, 88.8, 28.2, 89.2)
    );
}

#[test]
fn query_finds_the_boxes_its_rectangle_meets_but_not_the_ignored_or_unobstructing() {
    let mut spatial = Spatial::new(GridSize::new(120, 120));
    spatial.insert(object_id(9177), villager(28.0, 89.0));
    spatial.insert(object_id(9175), villager(30.0, 89.0));
    let mut foundation = villager(30.0, 88.0);
    foundation.radius = Footprint::new(1.0, 1.0);
    foundation.obstruction = None;
    foundation.class = class::BUILDING;
    spatial.insert(object_id(10028), foundation);
    // 9177's first step: from (28, 89) toward (28.0143, 88.9928), grown by its radius
    let step = Rect::new(28.0, 88.99284, 28.01431, 89.0);
    assert_eq!(
        spatial.obstruction_candidates(
            step,
            Distance::new(0.2),
            &[RecordId::from(object_id(9177))]
        ),
        Vec::new()
    );
    assert_eq!(
        spatial.obstruction_candidates(step, Distance::new(0.2), &[]),
        vec![object_id(9177)]
    );
    // 9175's box grown by 0.2 starts at x 29.6
    assert_eq!(
        spatial.obstruction_candidates(
            Rect::new(29.0, 89.0, 29.59, 89.0),
            Distance::new(0.2),
            &[RecordId::from(object_id(9177))]
        ),
        Vec::new()
    );
    assert_eq!(
        spatial.obstruction_candidates(
            Rect::new(29.0, 89.0, 29.61, 89.0),
            Distance::new(0.2),
            &[RecordId::from(object_id(9177))]
        ),
        vec![object_id(9175)]
    );
}

#[test]
fn record_filed_in_cell_the_query_does_not_visit_is_not_found() {
    // records/425906646's update 553: the smoothing of 27272's plan (radius 0.25) tests
    // (32.25, 154.59375) -> (32.5, 151.5); bush 13594's box (31, 154)-(32, 155) grown by 0.25
    // touches the start, but it is filed in level 5's cell (3, 19) (tiles 24..31) and the
    // query's tiles are 32..32
    let mut spatial = Spatial::with_tiles(
        GridSize::new(220, 220),
        Tiles::new(GridSize::new(220, 220), vec![1; 220 * 220]),
    );
    assert_eq!(spatial.grid, Grid { levels: 6 });
    let mut bush = villager(31.5, 154.5);
    bush.radius = Footprint::new(0.5, 0.5);
    bush.player = PlayerId::new(0);
    bush.obstruction = Some(Obstruction {
        kind: ObstructionKind::KIND_2,
        class: ObstructionClass::RESOURCE,
    });
    spatial.insert(object_id(13594), bush);
    assert_eq!(spatial.filed(&bush, ObstructionKind::KIND_2), (5, 3, 19));
    let (from, to) = (Point::new(32.25, 154.59375), Point::new(32.5, 151.5));
    assert!(!spatial.segment_blocked(from, to, Distance::new(0.25), TerrainMask::ALL));
    // a start a hair to the left visits tile 31's cell: the box blocks
    assert!(spatial.segment_blocked(
        Point::new(32.2, 154.59375),
        to,
        Distance::new(0.25),
        TerrainMask::ALL
    ));
    // off a half tile, the box's far edge is its tile: (31.6 ± 0.5) spans tiles 31 and 32,
    // which share only level 2's cells of 64 tiles
    bush.position = Position::new(31.6, 154.5, 0.0);
    assert_eq!(spatial.filed(&bush, ObstructionKind::KIND_2), (2, 0, 2));
}

#[test]
fn record_leaving_tile_another_static_record_stands_on_leaves_its_marks() {
    // records/425906646: Gaia's state-8 bush 0 shares tile (140, 3) with bush 808; its
    // removal (FUN_141055980) stops at the tile, which stays marked, and the planners hear
    // nothing
    let mut spatial = Spatial::with_tiles(
        GridSize::new(220, 220),
        Tiles::new(GridSize::new(220, 220), vec![1; 220 * 220]),
    );
    let mut bush = villager(140.5, 3.5);
    bush.radius = Footprint::new(0.5, 0.5);
    bush.player = PlayerId::new(0);
    bush.obstruction = Some(Obstruction {
        kind: ObstructionKind::KIND_2,
        class: ObstructionClass::RESOURCE,
    });
    spatial.insert(object_id(808), bush);
    spatial.insert(object_id(0), bush);
    let _ = spatial.take_changes();
    spatial.remove(object_id(0));
    assert!(
        spatial
            .tiles()
            .word(Tile::new(140, 3))
            .is_some_and(|word| word & BLOCKED != 0)
    );
    assert_eq!(spatial.take_changes(), Vec::<PlanRect>::new());
    // alone, it clears its tile
    spatial.remove(object_id(808));
    assert_eq!(spatial.tiles().word(Tile::new(140, 3)), Some(1));
    assert_eq!(spatial.take_changes(), vec![PlanRect::new(140, 3, 140, 3)]);
}

/// `FUN_141055b40` by the master's obstruction type: type 12 keeps all (false); type 1 clears
/// its tiles and keeps its record (true); a type 2 foundation without its record still clears
/// its player's marks (false); a unit (type 5) drops its record (true).
#[test]
fn record_goes_by_its_master_obstruction_kind() {
    let mut spatial = Spatial::with_tiles(
        GridSize::new(20, 20),
        Tiles::new(GridSize::new(20, 20), vec![1; 400]),
    );
    let mut rock = villager(5.5, 5.5);
    rock.radius = Footprint::new(0.5, 0.5);
    rock.obstruction_kind = ObstructionKind::KIND_1;
    rock.obstruction = Some(Obstruction {
        kind: ObstructionKind::KIND_1,
        class: ObstructionClass::RESOURCE,
    });
    spatial.insert(object_id(1), rock);
    assert!(
        spatial
            .tiles()
            .word(Tile::new(5, 5))
            .is_some_and(|word| word & BLOCKED != 0)
    );
    assert!(spatial.dropped_record(object_id(1)));
    assert_eq!(spatial.tiles().word(Tile::new(5, 5)), Some(1));
    assert_eq!(
        spatial
            .get(object_id(1))
            .and_then(|placed| placed.obstruction),
        rock.obstruction
    );
    let mut other = rock;
    other.obstruction_kind = ObstructionKind::KIND_12;
    other.position = Position::new(8.5, 8.5, 0.0);
    spatial.insert(object_id(2), other);
    assert!(!spatial.dropped_record(object_id(2)));
    assert!(
        spatial
            .tiles()
            .word(Tile::new(8, 8))
            .is_some_and(|word| word & BLOCKED != 0)
    );
    // a foundation of player 2 without its record on a tile player 2 marked
    let mut wall = rock;
    wall.obstruction_kind = ObstructionKind::KIND_2;
    wall.obstruction = Some(Obstruction {
        kind: ObstructionKind::KIND_2,
        class: ObstructionClass::RESOURCE,
    });
    wall.position = Position::new(12.5, 12.5, 0.0);
    spatial.insert(object_id(3), wall);
    let mut site = wall;
    site.obstruction = None;
    spatial.insert(object_id(4), site);
    spatial.objects.get_mut(object_id(3)).unwrap().obstruction = None;
    assert!(!spatial.dropped_record(object_id(4)));
    assert_eq!(spatial.tiles().word(Tile::new(12, 12)), Some(1));
    let unit = villager(15.5, 15.5);
    let mut unit5 = unit;
    unit5.obstruction_kind = ObstructionKind::CIRCLE;
    spatial.insert(object_id(5), unit5);
    assert!(spatial.dropped_record(object_id(5)));
    assert_eq!(
        spatial
            .get(object_id(5))
            .and_then(|placed| placed.obstruction),
        None
    );
    assert!(!spatial.dropped_record(object_id(5)));
}

#[test]
fn walk_hits_the_boxes_its_step_crosses_and_moving_ally_box_shrinks() {
    let mut objects = TestObjects::default();
    let mut spatial = Spatial::new(GridSize::new(120, 120));
    spatial.relations = ByPlayer::new(vec![
        relation_row([1, 4, 4]),
        relation_row([1, 0, 4]),
        relation_row([1, 4, 0]),
    ]);
    spatial.insert(object_id(9175), villager(30.0, 89.0));
    let (from, to) = (Point::new(29.5, 88.0), Point::new(29.5, 88.3));
    let rect = Rect::new(29.5, 88.0, 29.5, 88.3);
    // the box (29.8..30.2, 88.8..89.2) grown by 0.25 reaches 88.55, past the step's end
    assert_eq!(
        spatial.walk_hits(WalkQuery {
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
        spatial.walk_hits(WalkQuery {
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
        spatial.walk_hits(WalkQuery {
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
        spatial.walk_hits(WalkQuery {
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
    set_relation(&mut spatial, (2, 1), Relation::Enemy);
    assert_eq!(
        spatial.walk_hits(WalkQuery {
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
fn static_record_marks_its_tiles_with_its_class_and_player() {
    let mut spatial = Spatial::with_tiles(
        GridSize::new(4, 4),
        Tiles::new(GridSize::new(4, 4), vec![4; 16]),
    );
    let centre = Placed {
        position: Position::new(2.0, 2.0, 0.0),
        radius: Footprint::new(1.0, 1.0),
        class: class::BUILDING,
        player: PlayerId::new(2),
        master: ids::ID_0,
        obstruction_kind: ObstructionKind::NONE,
        buildable_on: BuildableOn::NO,
        tall: true,
        moves: false,
        unknown: None,
        obstruction: Some(Obstruction {
            kind: ObstructionKind::KIND_2,
            class: ObstructionClass::BUILDING,
        }),
    };
    spatial.insert(object_id(9157), centre);
    let tree = Placed {
        position: Position::new(0.5, 0.5, 0.0),
        radius: Footprint::new(0.5, 0.5),
        class: class::TREE,
        player: PlayerId::new(0),
        master: ids::ID_0,
        obstruction_kind: ObstructionKind::NONE,
        buildable_on: BuildableOn::NO,
        tall: true,
        moves: false,
        unknown: None,
        obstruction: Some(Obstruction {
            kind: ObstructionKind::KIND_2,
            class: ObstructionClass::RESOURCE,
        }),
    };
    spatial.insert(object_id(48), tree);
    let tiles = spatial.tiles();
    assert_eq!(tiles.word(Tile::new(1, 1)), Some(0x202_4304));
    assert_eq!(tiles.word(Tile::new(2, 2)), Some(0x202_4304));
    assert_eq!(tiles.word(Tile::new(3, 3)), Some(4));
    assert_eq!(tiles.word(Tile::new(0, 0)), Some(0x80_4104));
    assert_eq!(
        (tiles.objects(Tile::new(2, 2)), tiles.tall(Tile::new(2, 2))),
        (Some(1), Some(1))
    );
    spatial.remove(object_id(9157));
    assert_eq!(spatial.tiles().word(Tile::new(1, 1)), Some(4));
    assert_eq!(spatial.tiles().objects(Tile::new(2, 2)), Some(0));
}

#[test]
fn unit_counts_on_the_tile_its_centre_is_on_and_marks_nothing() {
    let mut spatial = Spatial::with_tiles(
        GridSize::new(4, 4),
        Tiles::new(GridSize::new(4, 4), vec![4; 16]),
    );
    spatial.insert(object_id(9177), villager(1.9, 1.0));
    assert_eq!(spatial.tiles().objects(Tile::new(1, 1)), Some(1));
    assert_eq!(spatial.tiles().word(Tile::new(1, 1)), Some(4));
    spatial
        .set_position(
            object_id(9177),
            Position::new(2.05, 1.0, 0.0),
            &roles(),
            tuning().visibility,
        )
        .unwrap();
    assert_eq!(
        (
            spatial.tiles().objects(Tile::new(1, 1)),
            spatial.tiles().objects(Tile::new(2, 1))
        ),
        (Some(0), Some(1))
    );
    assert_eq!(spatial.tiles().tall(Tile::new(2, 1)), Some(1));
}

#[test]
fn tile_list_keeps_the_game_order() {
    let mut spatial = Spatial::with_tiles(
        GridSize::new(4, 4),
        Tiles::new(GridSize::new(4, 4), vec![4; 16]),
    );
    for (id, x) in [(9171, 1.1), (10029, 1.5), (9173, 1.9)] {
        spatial.insert(object_id(id), villager(x, 1.0));
    }
    assert_eq!(
        spatial.tiles().list(Tile::new(1, 1)),
        [object_id(9171), object_id(10029), object_id(9173)]
    );
    assert_eq!(spatial.tile_place(object_id(9173)), 2);
    // the leaving object's slot goes to the last
    spatial
        .set_position(
            object_id(9171),
            Position::new(2.1, 1.0, 0.0),
            &roles(),
            tuning().visibility,
        )
        .unwrap();
    assert_eq!(
        spatial.tiles().list(Tile::new(1, 1)),
        [object_id(9173), object_id(10029)]
    );
    assert_eq!(
        (
            spatial.tile_place(object_id(9173)),
            spatial.tile_place(object_id(9171))
        ),
        (0, 0)
    );
    // a move within the tile and a master change keep the slot
    spatial
        .set_position(
            object_id(9173),
            Position::new(1.2, 1.0, 0.0),
            &roles(),
            tuning().visibility,
        )
        .unwrap();
    spatial.change_master(object_id(9173), villager(1.2, 1.0));
    assert_eq!(
        spatial.tiles().list(Tile::new(1, 1)),
        [object_id(9173), object_id(10029)]
    );
    spatial
        .set_position(
            object_id(9171),
            Position::new(1.0, 1.0, 0.0),
            &roles(),
            tuning().visibility,
        )
        .unwrap();
    assert_eq!(
        spatial.tiles().list(Tile::new(1, 1)),
        [object_id(9173), object_id(10029), object_id(9171)]
    );
    assert_eq!(
        (
            spatial.tiles().objects(Tile::new(1, 1)),
            spatial.tiles().tall(Tile::new(1, 1))
        ),
        (Some(3), Some(3))
    );
    assert_eq!(
        spatial.force_tile_lists(&[(
            Tile::new(1, 1),
            vec![object_id(9171), object_id(9173), object_id(10029)]
        )]),
        Vec::new()
    );
    assert_eq!(
        spatial.tiles().list(Tile::new(1, 1)),
        [object_id(9171), object_id(9173), object_id(10029)]
    );
}

/// `FUN_140f5f0a0`: a tile of 250 objects takes no more and keeps its tall count;
/// `FUN_140f5f560`: an object not on the list leaves nothing changed (no recount).
#[test]
fn full_tile_takes_no_object_and_one_not_on_it_leaves_its_count() {
    let mut spatial = Spatial::with_tiles(
        GridSize::new(4, 4),
        Tiles::new(GridSize::new(4, 4), vec![4; 16]),
    );
    for id in 0..250 {
        spatial.insert(object_id(id), villager(1.5, 1.5));
    }
    assert_eq!(spatial.tiles().objects(Tile::new(1, 1)), Some(250));
    assert_eq!(spatial.tiles().tall(Tile::new(1, 1)), Some(250));
    // object 0 is no longer tall; the game does not redo the count
    spatial.set_tall(object_id(0), false);
    spatial.insert(object_id(250), villager(1.5, 1.5));
    assert_eq!(spatial.tiles().objects(Tile::new(1, 1)), Some(250));
    assert!(
        !spatial
            .tiles()
            .list(Tile::new(1, 1))
            .contains(&object_id(250))
    );
    assert_eq!(spatial.tiles().tall(Tile::new(1, 1)), Some(250));
    // 250 leaves a tile whose list never held it: no recount
    spatial.remove(object_id(250));
    assert_eq!(spatial.tiles().tall(Tile::new(1, 1)), Some(250));
    // one that is on it leaves: the count is redone over the list, 0 not tall
    spatial.remove(object_id(249));
    assert_eq!(spatial.tiles().tall(Tile::new(1, 1)), Some(248));
}

/// A ghost enters and leaves a tile's list as any object does (`FUN_140f5f0a0`,
/// `FUN_140f5f560`): the tile's tall count is redone over the list.
#[test]
fn ghost_entering_or_leaving_tile_redoes_its_tall_count() {
    let mut spatial = Spatial::with_tiles(
        GridSize::new(4, 4),
        Tiles::new(GridSize::new(4, 4), vec![4; 16]),
    );
    spatial.insert(object_id(1), villager(1.5, 1.5));
    spatial.insert(object_id(2), villager(1.5, 1.5));
    spatial.set_tall(object_id(1), false);
    assert_eq!(spatial.tiles().tall(Tile::new(1, 1)), Some(2));
    spatial.insert_ghost(object_id(-5), Position::new(1.5, 1.5, 0.0));
    assert_eq!(
        spatial.tiles().list(Tile::new(1, 1)),
        [object_id(1), object_id(2), object_id(-5)]
    );
    assert_eq!(spatial.tiles().tall(Tile::new(1, 1)), Some(1));
    spatial.set_tall(object_id(2), false);
    spatial.remove_ghost(object_id(-5), Position::new(1.5, 1.5, 0.0));
    assert_eq!(spatial.tiles().tall(Tile::new(1, 1)), Some(0));
}

/// The house foundation 10028 at (30, 88) of MarvinBlue/426048731 with its builder 9175 on its
/// footprint at (30, 89), on open land (class 4) and a villager's mask (restriction 7: 6).
fn site() -> (Spatial, Placed) {
    let mut spatial = Spatial::with_tiles(
        GridSize::new(40, 120),
        Tiles::new(GridSize::new(40, 120), vec![4; 40 * 120]),
    );
    spatial.insert(object_id(9175), villager(30.0, 89.0));
    let mut house = villager(30.0, 88.0);
    house.radius = Footprint::new(1.0, 1.0);
    house.class = class::BUILDING;
    house.master = ids::HOUSE;
    house.obstruction_kind = ObstructionKind::KIND_2;
    house.obstruction = None;
    spatial.insert(object_id(10028), house);
    (spatial, house)
}

#[test]
fn tile_is_passable_when_its_class_is_in_the_mask_and_nothing_blocks_it() {
    let (mut spatial, _) = site();
    assert_eq!(
        spatial.passable(
            Tile::new(29, 89),
            TerrainMask::of_bits(6),
            object_id(1),
            Distance::new(0.2),
            ObjectsRef::empty()
        ),
        Ok(true)
    );
    assert_eq!(
        spatial.passable(
            Tile::new(29, 89),
            TerrainMask::of_bits(1),
            object_id(1),
            Distance::new(0.2),
            ObjectsRef::empty()
        ),
        Ok(false)
    );
    assert_eq!(
        spatial.passable(
            Tile::new(-1, 89),
            TerrainMask::of_bits(6),
            object_id(1),
            Distance::new(0.2),
            ObjectsRef::empty()
        ),
        Ok(false)
    );
    let mut tree = villager(29.5, 90.5);
    tree.radius = Footprint::new(0.5, 0.5);
    tree.obstruction = Some(Obstruction {
        kind: ObstructionKind::KIND_2,
        class: ObstructionClass::RESOURCE,
    });
    spatial.insert(object_id(48), tree);
    assert_eq!(
        spatial.passable(
            Tile::new(29, 90),
            TerrainMask::of_bits(6),
            object_id(1),
            Distance::new(0.2),
            ObjectsRef::empty()
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
    let mut spatial = sighted(words.to_vec());
    spatial.reveal_map(revealed);
    assert_eq!(
        spatial.sight.as_ref().unwrap().unified,
        words.map(|vision_bits| vision_bits | revealed)
    );
    let mut spatial = sighted(vec![VisionBits::NONE; 2]);
    spatial.reveal_to(PlayerId::new(1));
    assert!(
        spatial
            .sight
            .as_ref()
            .unwrap()
            .unified
            .iter()
            .all(|&vision_bits| vision_bits == VisionBits::ENDED | VisionBits::ENDED_EXPLORED)
    );
    // player 1 lost: its masks take the ended bits; the tile tests mask them out
    // (OBJECT_MASK holds neither ENDED nor ENDED_EXPLORED)
    assert_eq!(
        spatial.vision.get(PlayerId::new(1)),
        Some(&own_masks(PlayerId::new(1), GameStatus::Won))
    );
    assert_eq!(
        spatial.tile_sight(PlayerId::new(1), Tile::new(0, 0)),
        Some(TileSight::Unknown)
    );
}

#[test]
fn revealed_map_gives_the_player_the_ended_bits() {
    let mut spatial = Spatial {
        vision: ByPlayer::new(vec![
            own_masks(PlayerId::new(0), GameStatus::Playing),
            own_masks(PlayerId::new(1), GameStatus::Playing),
        ]),
        ..Spatial::default()
    };
    spatial.reveal_to(PlayerId::new(1));
    assert_eq!(
        spatial.vision.get(PlayerId::new(1)),
        Some(&own_masks(PlayerId::new(1), GameStatus::Won))
    );
    assert_eq!(
        spatial.vision.get(PlayerId::new(0)),
        Some(&own_masks(PlayerId::new(0), GameStatus::Playing))
    );
}

/// A map of two tiles holding `words`, which players 0 and 1 see with their own masks.
fn sighted(words: Vec<VisionBits>) -> Spatial {
    let count = words.len() as i32;
    Spatial {
        sight: Some(Visibility::new(
            GridSize::new(count, 1),
            Vec::new(),
            words,
            Vec::new(),
            0,
            Vec::new(),
        )),
        vision: ByPlayer::new(vec![
            own_masks(PlayerId::new(0), GameStatus::Playing),
            own_masks(PlayerId::new(1), GameStatus::Playing),
        ]),
        ..Spatial::default()
    }
}

/// `FUN_141663500`: 0xf seen, 0x80 explored, 0 neither; a word revealed to all (bit 29 with
/// an explored hit or bit 61) is seen, one explored for all (bit 61) explored.
#[test]
fn player_tile_test_answers_seen_explored_or_neither_and_reads_the_revealed_bits() {
    let (seen, explored) = (
        VisionBits::from_bits_retain(1 << 1),
        VisionBits::from_bits_retain(1 << 33),
    );
    let spatial = sighted(vec![
        seen,
        explored,
        VisionBits::NO_TILE_EXPLORED,
        VisionBits::NO_TILE | VisionBits::NO_TILE_EXPLORED,
        VisionBits::NO_TILE,
        VisionBits::NO_TILE | explored,
    ]);
    let answers = |player_id: PlayerId| {
        (0..7)
            .map(|x| {
                spatial
                    .tile_sight(player_id, Tile::new(x, 0))
                    .map(TileSight::get)
            })
            .collect::<Vec<_>>()
    };
    let some = |values: [u8; 7]| values.map(Some).to_vec();
    assert_eq!(
        answers(PlayerId::new(1)),
        some([0xf, 0x80, 0x80, 0xf, 0, 0xf, 0])
    );
    assert_eq!(answers(PlayerId::new(0)), some([0, 0, 0x80, 0xf, 0, 0, 0]));
    assert_eq!(
        spatial.tile_sight(PlayerId::new(1), Tile::new(-1, 0)),
        Some(TileSight::Unknown)
    );
    assert_eq!(
        spatial.tile_sight(PlayerId::new(1), Tile::new(0, 1)),
        Some(TileSight::Unknown)
    );
    assert_eq!(
        Spatial::default().tile_sight(PlayerId::new(1), Tile::new(0, 0)),
        None
    );
    // sees_tile is its 0xf, explored any answer
    assert!(
        spatial.sees_tile(PlayerId::new(0), Tile::new(3, 0))
            && !spatial.sees_tile(PlayerId::new(0), Tile::new(2, 0))
    );
    assert_eq!(
        spatial.explored(PlayerId::new(0), Tile::new(2, 0)),
        Ok(true)
    );
    assert_eq!(
        spatial.explored(PlayerId::new(0), Tile::new(4, 0)),
        Ok(false)
    );
}

/// `FUN_141285350` and `FUN_140c68a00`: the word under the registry's mask only, so the
/// revealed bits 29 and 61 count for nothing.
#[test]
fn the_visible_map_tile_test_reads_only_the_players_bits() {
    let (seen, explored) = (
        VisionBits::from_bits_retain(1 << 1),
        VisionBits::from_bits_retain(1 << 33),
    );
    let spatial = sighted(vec![
        seen,
        explored,
        VisionBits::NO_TILE | VisionBits::NO_TILE_EXPLORED,
        VisionBits::NONE,
    ]);
    let scout = (0..5)
        .map(|x| {
            spatial
                .scout_sight(PlayerId::new(1), Tile::new(x, 0))
                .map(TileSight::get)
        })
        .collect::<Vec<_>>();
    assert_eq!(scout, [0xf, 0x80, 0, 0, 0].map(Some).to_vec());
    let seen_now = (0..4)
        .map(|x| {
            spatial
                .visibility(PlayerId::new(1), Tile::new(x, 0))
                .map(TileSight::get)
        })
        .collect::<Vec<_>>();
    assert_eq!(seen_now, [0xf, 0x80, 0, 0].map(Some).to_vec());
    assert!(
        spatial.visible(PlayerId::new(1), Tile::new(0, 0))
            && !spatial.visible(PlayerId::new(1), Tile::new(1, 0))
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
fn the_free_spot_beside_footprint_is_the_closest_empty_tile_around_it() {
    // the game sends 9175 to (29.5, 89.5): (30, 89) is as close but holds 9175 itself
    let (mut spatial, house) = site();
    assert_eq!(
        spatial.spot_beside(
            Position::new(30.0, 89.0, 0.0),
            (object_id(1), 0.2),
            TerrainMask::of_bits(6),
            (house.position, house.radius),
            ObjectsRef::empty()
        ),
        Ok(Some(Point::new(29.5, 89.5)))
    );
}

#[test]
fn the_footprint_occupants_are_the_objects_whose_centre_is_on_it() {
    let (mut spatial, house) = site();
    assert_eq!(
        spatial.footprint_occupants(
            FoundationId::new(object_id(10028)),
            &house,
            &BTreeMap::new()
        ),
        Ok(vec![object_id(9175)])
    );
    spatial.insert(object_id(9177), villager(28.0, 89.0));
    assert_eq!(
        spatial.footprint_occupants(
            FoundationId::new(object_id(10028)),
            &house,
            &BTreeMap::new()
        ),
        Ok(vec![object_id(9175)])
    );
    assert!(!spatial.foreign_marks(&house));
}

/// `FUN_14123bb20` passes over an object in state 7 or later, a building from state 3 on, and
/// a desolid building foundation unless its player and the site's count each other allied
/// (`FUN_141663d90` with the ally flag, both ways): 428016030's palisade foundation 33965
/// (player 3) shares its tile with player 8's foundation 33990, and its builder 31368 starts
/// at once in update 1305. An object the view does not look at (the owner's sleeping
/// objects: types below 30, alive) counts.
#[test]
fn foundation_not_allied_both_ways_or_dead_object_occupies_no_footprint() {
    use crate::construction::build::ClearLook;
    let (mut records, house) = site();
    // player 2 counts player 3 allied, player 3 counts player 2 neutral
    records.relations = ByPlayer::new(vec![
        relation_row([0; 4]),
        relation_row([0; 4]),
        relation_row([0, 3, 1, 0]),
        relation_row([0, 3, 3, 1]),
    ]);
    let mut wall = villager(30.5, 88.5);
    wall.player = PlayerId::new(3);
    wall.radius = Footprint::new(0.5, 0.5);
    records.insert(object_id(33990), wall);
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
                look(ObjectState::Alive, MasterKind::COMBAT, false),
            ),
            (object_id(33990), wall),
        ])
    };
    let occupants = |spatial: &Spatial, wall| {
        spatial.footprint_occupants(FoundationId::new(object_id(10028)), &house, &looks(wall))
    };
    assert_eq!(
        occupants(
            &records,
            look(ObjectState::Foundation, MasterKind::BUILDING, true)
        ),
        Ok(vec![object_id(9175)])
    );
    // allied both ways, the foundation occupies it; built (not desolid) too
    set_relation(&mut records, (3, 2), Relation::Ally);
    assert_eq!(
        occupants(
            &records,
            look(ObjectState::Foundation, MasterKind::BUILDING, true)
        ),
        Ok(vec![object_id(9175), object_id(33990)])
    );
    set_relation(&mut records, (3, 2), Relation::Enemy);
    assert_eq!(
        occupants(
            &records,
            look(ObjectState::Alive, MasterKind::BUILDING, false)
        ),
        Ok(vec![object_id(9175), object_id(33990)])
    );
    // a building from state 3 on, any object from state 7 on, passes
    assert_eq!(
        occupants(
            &records,
            look(ObjectState::Dying, MasterKind::BUILDING, false)
        ),
        Ok(vec![object_id(9175)])
    );
    assert_eq!(
        occupants(
            &records,
            look(ObjectState::Undead, MasterKind::COMBAT, false)
        ),
        Ok(vec![object_id(9175), object_id(33990)])
    );
    assert_eq!(
        occupants(
            &records,
            look(ObjectState::Removed, MasterKind::COMBAT, false)
        ),
        Ok(vec![object_id(9175)])
    );
    // the view does not look at it: it counts
    assert_eq!(
        records.footprint_occupants(
            FoundationId::new(object_id(10028)),
            &house,
            &BTreeMap::new()
        ),
        Ok(vec![object_id(9175), object_id(33990)])
    );
}

/// `FUN_1411d4b80` reads a tile's players as the low 32 bits of `word >> 23` and casts the
/// footprint's edges by `CVTTSS2SI`: a NaN place casts to `i32::MIN`, so no tile is read.
#[test]
fn foreign_marks_read_the_players_in_32_bits_and_skip_nan_place() {
    let (mut spatial, mut house) = site();
    // player 2 of 30.0, 88.0: tile (29, 87) marked by player 3, then a bit past 32 players
    spatial
        .tiles
        .mark_for_test(Tile::new(29, 87), 0x80_0000 << 3);
    assert!(spatial.foreign_marks(&house));
    let (mut wide_marks, _) = site();
    wide_marks.tiles.mark_for_test(Tile::new(29, 87), 1 << 60);
    assert!(!wide_marks.foreign_marks(&house));
    // a NaN place reads nothing, not tile 0 (here marked by player 3)
    spatial.tiles.mark_for_test(Tile::new(0, 0), 0x80_0000 << 3);
    house.position = Position::new(f32::NAN, f32::NAN, 0.0);
    assert!(!spatial.foreign_marks(&house));
}

#[test]
fn walk_meets_static_box_in_its_way_but_not_one_beside_it() {
    // tiles of class 1 everywhere: a unit with mask 1 crosses them all
    let mut spatial = Spatial::with_tiles(
        GridSize::new(16, 16),
        Tiles::new(GridSize::new(16, 16), vec![1; 256]),
    );
    let mut house = villager(8.0, 8.0);
    house.radius = Footprint::new(1.0, 1.0);
    house.obstruction = Some(Obstruction {
        kind: ObstructionKind::KIND_2,
        class: ObstructionClass::BUILDING,
    });
    spatial.insert(object_id(7), house);
    assert!(spatial.segment_blocked(
        Point::new(4.0, 8.0),
        Point::new(12.0, 8.0),
        Distance::new(0.2),
        TerrainMask::of_bits(1)
    ));
    assert!(!spatial.segment_blocked(
        Point::new(4.0, 11.0),
        Point::new(12.0, 11.0),
        Distance::new(0.2),
        TerrainMask::of_bits(1)
    ));
    // within its radius of the box, and the box ignored
    assert!(spatial.segment_blocked(
        Point::new(4.0, 9.1),
        Point::new(12.0, 9.1),
        Distance::new(0.2),
        TerrainMask::of_bits(1)
    ));
    spatial.set_manager_ignore(&[RecordId::from(object_id(7))]);
    assert!(!spatial.segment_blocked(
        Point::new(4.0, 8.0),
        Point::new(12.0, 8.0),
        Distance::new(0.2),
        TerrainMask::of_bits(1),
    ));
    spatial.set_manager_ignore(&[]);
    // a moving unit's record never blocks the walk
    spatial.insert(object_id(8), villager(6.0, 3.0));
    assert!(!spatial.segment_blocked(
        Point::new(4.0, 3.0),
        Point::new(12.0, 3.0),
        Distance::new(0.2),
        TerrainMask::of_bits(1)
    ));
}

#[test]
fn walk_meets_tile_whose_terrain_the_unit_cannot_cross() {
    let mut classes = vec![1; 256];
    classes[5 * 16 + 9] = 2;
    let mut spatial = Spatial::with_tiles(
        GridSize::new(16, 16),
        Tiles::new(GridSize::new(16, 16), classes),
    );
    assert!(spatial.segment_blocked(
        Point::new(2.0, 5.5),
        Point::new(14.0, 5.5),
        Distance::new(0.2),
        TerrainMask::of_bits(1)
    ));
    assert!(!spatial.segment_blocked(
        Point::new(2.0, 5.5),
        Point::new(14.0, 5.5),
        Distance::new(0.2),
        TerrainMask::of_bits(3)
    ));
    // a walk whose grown box leaves the map
    assert!(spatial.segment_blocked(
        Point::new(0.1, 5.5),
        Point::new(14.0, 5.5),
        Distance::new(0.2),
        TerrainMask::of_bits(3)
    ));
}

/// The old short pather's walks within one `FindShortPath` meet the tile records of its
/// earlier walks: the walk from (6.5, 6.5) with a buffer of 0.5 puts no record on tile
/// (5, 5), but touches the record an earlier walk put there, until the records are cleared.
#[test]
fn pather_walk_meets_the_tile_records_of_earlier_walks() {
    let mut classes = vec![1; 64 * 64];
    classes[5 * 64 + 5] = 0;
    let mut spatial = Spatial::with_tiles(
        GridSize::new(64, 64),
        Tiles::new(GridSize::new(64, 64), classes),
    );
    let (from, to) = (Point::new(6.5, 6.5), Point::new(7.5, 7.5));
    assert!(
        !spatial
            .pather_blocked(
                from,
                to,
                Some(Distance::new(0.5)),
                TerrainMask::of_bits(1),
                &[],
                ObjectsRef::empty()
            )
            .unwrap()
    );
    spatial.clear_tile_records();
    assert!(
        spatial
            .pather_blocked(
                Point::new(5.5, 4.0),
                Point::new(5.5, 7.0),
                Some(Distance::new(0.5)),
                TerrainMask::of_bits(1),
                &[],
                ObjectsRef::empty()
            )
            .unwrap()
    );
    assert_eq!(spatial.tile_records(), vec![Tile::new(5, 5)]);
    let hits = spatial
        .pather_hits(
            from,
            to,
            Some(Distance::new(0.5)),
            TerrainMask::of_bits(1),
            &[],
            ObjectsRef::empty(),
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
        !spatial
            .pather_blocked(
                from,
                to,
                Some(Distance::new(0.5)),
                TerrainMask::of_bits(1),
                &[tile_record_id(Tile::new(5, 5))],
                ObjectsRef::empty()
            )
            .unwrap()
    );
    spatial.clear_tile_records();
    assert!(
        !spatial
            .pather_blocked(
                from,
                to,
                Some(Distance::new(0.5)),
                TerrainMask::of_bits(1),
                &[],
                ObjectsRef::empty()
            )
            .unwrap()
    );
}

/// The old short pather's query on a walk whose grown box leaves the map: the first hit is
/// the map's edge record (`FUN_141052350`), the hits list it before the records; a box the
/// walk passes by is no hit, and a record of kind 11 or a gate's meeting the walk is not
/// modelled.
#[test]
fn pather_walk_off_the_map_meets_its_edge_first() {
    let mut spatial = Spatial::with_tiles(
        GridSize::new(16, 16),
        Tiles::new(GridSize::new(16, 16), vec![1; 256]),
    );
    let mut house = villager(4.0, 8.0);
    house.radius = Footprint::new(1.0, 1.0);
    house.obstruction = Some(Obstruction {
        kind: ObstructionKind::KIND_2,
        class: ObstructionClass::BUILDING,
    });
    spatial.insert(object_id(7), house);
    let mut beside = villager(12.0, 2.0);
    beside.obstruction = Some(Obstruction {
        kind: ObstructionKind::KIND_2,
        class: ObstructionClass::BUILDING,
    });
    spatial.insert(object_id(8), beside);
    let (from, to) = (Point::new(0.2, 8.0), Point::new(6.0, 8.0));
    let edge = spatial.edge_records(Rect::new(-0.3, 7.5, 6.5, 8.5), QUERY_HITS);
    assert_ne!(edge.len(), 0);
    assert!(
        spatial
            .pather_blocked(
                from,
                to,
                Some(Distance::new(0.5)),
                TerrainMask::of_bits(1),
                &[],
                ObjectsRef::empty()
            )
            .unwrap()
    );
    spatial.clear_tile_records();
    let hits = spatial
        .pather_hits(
            from,
            to,
            Some(Distance::new(0.5)),
            TerrainMask::of_bits(1),
            &[],
            ObjectsRef::empty(),
        )
        .unwrap();
    assert_eq!(hits.len(), edge.len() + 1);
    assert_eq!(hits[..edge.len()], edge[..]);
    assert_eq!(hits[edge.len()].id, RecordId::from(object_id(7)));
    spatial.clear_tile_records();
    // within the map, the house alone; the box at (12, 2) is passed by
    let hits = spatial
        .pather_hits(
            Point::new(1.0, 8.0),
            Point::new(6.0, 8.0),
            Some(Distance::new(0.5)),
            TerrainMask::of_bits(1),
            &[],
            ObjectsRef::empty(),
        )
        .unwrap();
    assert_eq!(
        hits.iter().map(|hit| hit.id).collect::<Vec<_>>(),
        [RecordId::from(object_id(7))]
    );
    spatial.clear_tile_records();
    let mut own = villager(4.0, 8.0);
    own.obstruction = Some(Obstruction {
        kind: ObstructionKind::KIND_11,
        class: ObstructionClass::BUILDING,
    });
    spatial.insert(object_id(7), own);
    assert!(
        spatial
            .pather_hits(
                Point::new(1.0, 8.0),
                Point::new(6.0, 8.0),
                Some(Distance::new(0.5)),
                TerrainMask::of_bits(1),
                &[],
                ObjectsRef::empty()
            )
            .is_err()
    );
    let mut gate = villager(4.0, 8.0);
    gate.class = class::GATE;
    gate.obstruction = Some(Obstruction {
        kind: ObstructionKind::KIND_2,
        class: ObstructionClass::BUILDING,
    });
    spatial.insert(object_id(7), gate);
    assert!(
        spatial
            .pather_hits(
                Point::new(1.0, 8.0),
                Point::new(6.0, 8.0),
                Some(Distance::new(0.5)),
                TerrainMask::of_bits(1),
                &[],
                ObjectsRef::empty()
            )
            .is_err()
    );
}

/// 426327574's transport at update 309: the smoothing's walk (77.5, 90.5) -> (76.5, 91.5)
/// puts tile (78, 90)'s record, and the next walk from (77.5, 91.5) touches its grown box.
#[test]
fn tile_record_blocks_later_walks_until_the_records_are_cleared() {
    let mut classes = vec![1; 128 * 128];
    classes[90 * 128 + 78] = 0;
    classes[87 * 128 + 78] = 0;
    let mut spatial = Spatial::with_tiles(
        GridSize::new(128, 128),
        Tiles::new(GridSize::new(128, 128), classes),
    );
    let (first, second) = (
        (Point::new(77.5, 90.5), Point::new(76.5, 91.5)),
        (Point::new(77.5, 91.5), Point::new(76.5, 92.5)),
    );
    // alone, the second walk's tiles stop at y = 91 and its test removes its records
    assert!(!spatial.segment_blocked(
        second.0,
        second.1,
        Distance::new(0.5),
        TerrainMask::of_bits(1)
    ));
    assert_eq!(spatial.tile_records(), Vec::<Tile>::new());
    spatial.put_tile_records(
        first.0,
        first.1,
        Distance::new(0.5),
        TerrainMask::of_bits(1),
    );
    assert_eq!(spatial.tile_records(), vec![Tile::new(78, 90)]);
    assert!(spatial.segment_meets(second.0, second.1, Distance::new(0.5), &[]));
    // the record's id in the ignore list leaves it out
    assert!(!spatial.segment_meets(
        second.0,
        second.1,
        Distance::new(0.5),
        &[tile_record_id(Tile::new(78, 90))]
    ));
    // a tile that holds a record takes no second
    spatial.put_tile_records(
        second.0,
        Point::new(79.0, 89.0),
        Distance::new(0.5),
        TerrainMask::of_bits(1),
    );
    assert_eq!(spatial.tile_records(), vec![Tile::new(78, 90)]);
    spatial.clear_tile_records();
    assert!(!spatial.segment_meets(second.0, second.1, Distance::new(0.5), &[]));
    // tile (78, 87)'s grown box touches (77.5, 88.5), but its cell (y 80..87) is not one the
    // walk from y 88.5 visits (y from 88)
    spatial.put_tile_records(
        Point::new(78.5, 86.5),
        Point::new(78.5, 87.5),
        Distance::ZERO,
        TerrainMask::of_bits(1),
    );
    assert_eq!(spatial.tile_records(), vec![Tile::new(78, 87)]);
    assert!(!spatial.segment_meets(
        Point::new(77.5, 88.5),
        Point::new(76.5, 89.5),
        Distance::new(0.5),
        &[]
    ));
    assert!(spatial.segment_meets(
        Point::new(77.5, 88.0),
        Point::new(76.5, 89.5),
        Distance::new(0.5),
        &[]
    ));
    spatial.clear_tile_records();
    assert_eq!(spatial.tile_records(), Vec::<Tile>::new());
}

#[test]
fn frozen_object_may_be_anywhere_its_speed_could_take_it() {
    let (mut spatial, house) = site();
    let mut deer = villager(34.0, 88.0);
    deer.moves = true;
    // its master's speed, a tile a second
    let deer_speed = |_, _| Speed::new(1.0);
    spatial.insert(object_id(9656), deer);
    spatial.freeze(object_id(9656));
    // frozen at once, the deer is where the save has it
    assert!(
        spatial
            .footprint_occupants(
                FoundationId::new(object_id(10028)),
                &house,
                &BTreeMap::new()
            )
            .is_ok()
    );
    assert!(spatial.get_known(object_id(9656)).is_ok());
    // its box reaches x 33.8; 3 s later it may stand on the footprint, which ends at 31
    spatial.grow_unknown(Seconds::new(2.5), deer_speed);
    assert!(
        spatial
            .footprint_occupants(
                FoundationId::new(object_id(10028)),
                &house,
                &BTreeMap::new()
            )
            .is_ok()
    );
    spatial.grow_unknown(Seconds::new(0.5), deer_speed);
    let error = spatial
        .footprint_occupants(
            FoundationId::new(object_id(10028)),
            &house,
            &BTreeMap::new(),
        )
        .unwrap_err();
    assert_eq!(error.context().next(), Some(("object", "9656")));
    let error = spatial.get_known(object_id(9656)).unwrap_err();
    // the error names the object and its player
    let context: Vec<_> = error.context().take(2).collect();
    assert_eq!(context, [("object", "9656"), ("player", "2")]);
}

#[test]
fn frozen_object_that_does_not_move_is_known_unless_stopped_unit_may_reach_it() {
    let mut spatial = Spatial::new(GridSize::new(64, 64));
    let tree = Placed {
        player: PlayerId::new(0),
        moves: false,
        ..villager(24.5, 97.5 - 60.0)
    };
    spatial.insert(object_id(9365), tree);
    spatial.freeze(object_id(9365));
    spatial.reach = Distance::new(1.0);
    let near = Rect::new(24.0, 37.0, 24.6, 38.0);
    assert!(spatial.check_known(near).is_ok());
    // a stopped villager 10 tiles off, 0.8 tiles a second (every master's speed here: the tree
    // does not move, so its place stays known)
    let villager_speed = |_, _| Speed::new(0.8);
    let mut walker = villager(34.5, 37.5);
    walker.player = PlayerId::new(1);
    walker.moves = true;
    spatial.insert(object_id(9200), walker);
    spatial.freeze(object_id(9200));
    spatial.grow_unknown(Seconds::new(10.0), villager_speed);
    assert!(spatial.check_known(near).is_ok());
    spatial.grow_unknown(Seconds::new(2.0), villager_speed);
    assert!(spatial.check_known(near).is_err());
}

/// `FUN_1410562e0`'s list stays with the obstruction manager: the walk's query and the stand
/// tests set it (`FUN_14114cc60`, `FUN_141158720`, `FUN_141158940`), and it holds until the
/// next set; the objects' part of the point query leaves the edge's records out.
#[test]
fn the_manager_keeps_the_last_ignore_list_set() {
    let mut spatial = Spatial::new(GridSize::new(10, 10));
    assert_eq!(spatial.manager_ignore(), Vec::<RecordId>::new());
    let rect = Rect::new(2.0, 5.0, 3.0, 5.0);
    let _ = spatial
        .walk_hits(WalkQuery {
            objects: ObjectsRef::empty(),
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
        spatial.manager_ignore(),
        [RecordId::from(object_id(7)), RecordId::from(object_id(8))]
    );
    assert!(
        spatial
            .point_clear(
                object_id(3),
                Point::new(4.5, 4.5),
                Distance::new(0.1),
                TerrainMask::ALL,
                ObjectsRef::empty()
            )
            .unwrap()
    );
    assert_eq!(spatial.manager_ignore(), [RecordId::from(object_id(3))]);
    assert!(
        spatial
            .point_clear_among(
                object_id(4),
                Point::new(4.5, 4.5),
                Distance::new(0.1),
                TerrainMask::ALL,
                &[],
                ObjectsRef::empty()
            )
            .unwrap()
    );
    assert_eq!(spatial.manager_ignore(), [RecordId::from(object_id(4))]);
    // a point off the map's tiles sets nothing
    assert!(
        !spatial
            .point_clear(
                object_id(5),
                Point::new(-1.0, 4.5),
                Distance::new(0.1),
                TerrainMask::ALL,
                ObjectsRef::empty()
            )
            .unwrap()
    );
    assert_eq!(spatial.manager_ignore(), [RecordId::from(object_id(4))]);
    spatial.set_manager_ignore(&[]);
    assert_eq!(spatial.manager_ignore(), Vec::<RecordId>::new());
    assert!(
        spatial
            .point_hit(
                Point::new(0.05, 4.5),
                Distance::new(0.1),
                &[],
                Movers::Counted,
                ObjectsRef::empty()
            )
            .unwrap()
    );
    assert!(
        !spatial
            .object_holds(
                Point::new(0.05, 4.5),
                Distance::new(0.1),
                &[],
                Movers::Counted,
                ObjectsRef::empty()
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
        workers: WorkerCount::of_data(-1),
        spared: None,
        reach: false,
    }
}

/// The sight of a combat unit of player 2 in `state`.
const fn seer_in(state: ObjectState) -> Seer {
    Seer {
        player: PlayerId::new(2),
        master: ids::ID_0,
        los: Distance::new(4.0),
        state,
        explored: true,
        building: None,
        umv: 0,
        capture: CaptureKind::None,
        kind: MasterKind::COMBAT,
    }
}

/// The nearest so far gives way to a nearer record, to an equally near one of a lower id and
/// after a NaN distance; it stays before a farther record and an equally near one of a higher
/// id.
#[test]
fn closest_object_keeps_the_nearest_record_and_on_tie_the_lower_id() {
    use crate::map::spatial::closest::nearer;
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
/// searcher's player owns it or it is not in state 2, and not while it moves (an object of type
/// 70 or 80 only); a searcher of class 10 passes what it shuns.
#[test]
fn closest_object_keeps_what_its_filter_and_its_searcher_take() {
    use crate::map::spatial::closest::{Searcher, closest_kept};
    let villager = villager(1.0, 1.0);
    // the found object 7 as its world holds it: its master in its player's table, without the
    // scout's object flag; herded, its top action the Artifact with a point; shunned, with the flag
    let found = Master {
        id: villager.master,
        class: villager.class,
        ..Master::default()
    };
    let plain_world = found_world(vec![placed_object(&villager, found.clone(), None)]);
    let herded_world = found_world(vec![placed_object(
        &villager,
        Master {
            kind: MasterKind::COMBAT,
            ..found.clone()
        },
        Some(herding()),
    )]);
    // below type 70 slot 0x288 is −1 (`FUN_14106c3e0`): an action object's Artifact never moves
    let action_world = found_world(vec![placed_object(
        &villager,
        Master {
            kind: MasterKind::ACTION,
            ..found.clone()
        },
        Some(herding()),
    )]);
    let scout = Master {
        object_flags: ObjectFlags::SCOUT,
        ..found
    };
    let scout_world = found_world(vec![placed_object(&villager, scout, None)]);
    let empty_world = found_world(Vec::new());
    let roles = roles();
    let checks = ClosestChecks {
        objects: plain_world.objects(),
        roles: &roles,
        best_target_tuning: &tuning().best_target,
        spatial_tuning: &tuning().spatial,
        world: closest_world(),
    };
    let kept = |filter, placed: &Placed, state, searcher, checks| {
        closest_kept(
            object_id(7),
            (&seer_in(state), placed),
            filter,
            (PlayerId::new(1), searcher),
            checks,
        )
    };
    let plain = Searcher::default();
    let with = |setup: fn(&mut Filter)| {
        let mut filter = any_filter();
        setup(&mut filter);
        filter
    };
    assert!(kept(any_filter(), &villager, ObjectState::Alive, plain, checks).unwrap());
    assert!(
        kept(
            with(|filter| filter.group = Some(class_id(4))),
            &villager,
            ObjectState::Alive,
            plain,
            checks
        )
        .unwrap()
    );
    assert!(
        !kept(
            with(|filter| filter.group = Some(class_id(3))),
            &villager,
            ObjectState::Alive,
            plain,
            checks
        )
        .unwrap()
    );
    assert!(
        !kept(
            with(|filter| filter.master = Some(master_id(5))),
            &villager,
            ObjectState::Alive,
            plain,
            checks
        )
        .unwrap()
    );
    assert!(
        !kept(
            with(|filter| filter.state = Some(ObjectState::Dying)),
            &villager,
            ObjectState::Alive,
            plain,
            checks
        )
        .unwrap()
    );
    // the workers cap (argument 6, `+0x644`): an object below it counts, one at it does not
    // ("Too many workers")
    let found_master = |id| Master {
        id,
        class: villager.class,
        ..Master::default()
    };
    let worked = |master: Master| {
        found_world(vec![SceneObject {
            workers: WorkerCount::ONE,
            ..placed_object(&villager, master, None)
        }])
    };
    let worked_world = worked(found_master(villager.master));
    let capped = with(|filter| filter.workers = WorkerCount::ONE);
    assert!(kept(capped, &villager, ObjectState::Alive, plain, checks).unwrap());
    let worked_checks = ClosestChecks {
        objects: worked_world.objects(),
        ..checks
    };
    assert!(!kept(capped, &villager, ObjectState::Alive, plain, worked_checks).unwrap());
    // a search for state 0 passes a farm (50 or 1187) that has workers ("Unbuilt farm that
    // already has workers"); a free farm and a worked object of another master count
    let unbuilt = with(|filter| filter.state = Some(ObjectState::Foundation));
    let farm = Placed {
        master: ids::FARM,
        ..villager
    };
    let worked_farm_world = worked(found_master(ids::FARM));
    let free_farm_world = found_world(vec![placed_object(&farm, found_master(ids::FARM), None)]);
    assert!(
        !kept(
            unbuilt,
            &farm,
            ObjectState::Foundation,
            plain,
            ClosestChecks {
                objects: worked_farm_world.objects(),
                ..checks
            }
        )
        .unwrap()
    );
    assert!(
        kept(
            unbuilt,
            &farm,
            ObjectState::Foundation,
            plain,
            ClosestChecks {
                objects: free_farm_world.objects(),
                ..checks
            }
        )
        .unwrap()
    );
    assert!(
        kept(
            unbuilt,
            &villager,
            ObjectState::Foundation,
            plain,
            worked_checks
        )
        .unwrap()
    );
    let livestock = with(|filter| filter.group = Some(class_id(58)));
    let mut sheep = Placed {
        class: class::LIVESTOCK,
        ..villager
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
            ClosestChecks {
                objects: herded_world.objects(),
                ..checks
            }
        )
        .unwrap()
    );
    assert!(
        kept(
            livestock,
            &sheep,
            ObjectState::Alive,
            plain,
            ClosestChecks {
                objects: action_world.objects(),
                ..checks
            }
        )
        .unwrap()
    );
    let wolf = Searcher {
        class_10: true,
        ..plain
    };
    assert!(kept(any_filter(), &villager, ObjectState::Alive, wolf, checks).unwrap());
    assert!(
        !kept(
            any_filter(),
            &villager,
            ObjectState::Alive,
            wolf,
            ClosestChecks {
                objects: scout_world.objects(),
                ..checks
            }
        )
        .unwrap()
    );
    assert!(
        kept(
            any_filter(),
            &villager,
            ObjectState::Alive,
            plain,
            ClosestChecks {
                objects: scout_world.objects(),
                ..checks
            }
        )
        .unwrap()
    );
    assert!(
        kept(
            any_filter(),
            &villager,
            ObjectState::Alive,
            wolf,
            ClosestChecks {
                objects: empty_world.objects(),
                ..checks
            }
        )
        .is_err()
    );
}

/// A farm `ClosestObject` found: a free one the searcher gathers from (a fish trap for a
/// fishing ship, any other for a villager) counts; a worked one only with a master asked for;
/// a pasture's livestock never; a pasture and a farm the caller does not hold fail.
#[test]
fn closest_object_takes_free_farm_its_searcher_gathers_from() {
    use crate::map::spatial::closest::{Searcher, closest_kept, farm_kept};
    let filter = Filter {
        group: Some(class_id(49)),
        ..any_filter()
    };
    let farm_of = |master| Placed {
        class: class::FARM,
        master,
        ..villager(1.0, 1.0)
    };
    // the farms 1 (free), 2 (worked) and 3 (a pasture's) as their world holds them; 4 is not held
    let farm_master = Master {
        id: ids::FARM,
        class: class::FARM,
        ..Master::default()
    };
    let farm = |id, workers, pasture, x| SceneObject {
        pasture,
        workers,
        ..SceneObject::new(
            id,
            PlayerId::new(2),
            farm_master.clone(),
            Position::new(x, 1.5, 0.0),
        )
    };
    let farms = found_world(vec![
        farm(1, WorkerCount::ZERO, None, 1.5),
        farm(2, WorkerCount::ONE, None, 3.5),
        farm(
            3,
            WorkerCount::ZERO,
            Some(PastureId::new(object_id(9))),
            5.5,
        ),
    ]);
    let look = farms.objects();
    let farmer = Searcher {
        farmer: true,
        ..Searcher::default()
    };
    let trapper = Searcher {
        trapper: true,
        ..Searcher::default()
    };
    let (farm, trap) = (farm_of(ids::FARM), farm_of(FISH_TRAP));
    let alive = seer_in(ObjectState::Alive);
    let world = closest_world();
    let kept = |id, placed: &Placed, filter, searcher| {
        farm_kept(
            object_id(id),
            (&alive, placed),
            filter,
            searcher,
            (look, world),
        )
    };
    assert!(kept(1, &farm, filter, farmer).unwrap());
    assert!(!kept(1, &farm, filter, trapper).unwrap());
    assert!(kept(1, &trap, filter, trapper).unwrap());
    assert!(!kept(1, &trap, filter, farmer).unwrap());
    assert!(!kept(2, &farm, filter, farmer).unwrap());
    let asked = Filter {
        master: Some(master_id(50)),
        ..filter
    };
    assert!(kept(2, &farm, asked, Searcher::default()).unwrap());
    assert!(!kept(3, &farm, filter, farmer).unwrap());
    assert_eq!(
        kept(4, &farm, filter, farmer).unwrap_err().reason(),
        "a found object the slice does not hold"
    );
    // through the tests past the relation: the worked farm passed, the free one kept
    let roles = roles();
    let checks = ClosestChecks {
        objects: look,
        roles: &roles,
        best_target_tuning: &tuning().best_target,
        spatial_tuning: &tuning().spatial,
        world,
    };
    let seer = seer_in(ObjectState::Alive);
    assert!(
        closest_kept(
            object_id(1),
            (&seer, &farm),
            filter,
            (PlayerId::new(1), farmer),
            checks,
        )
        .unwrap()
    );
    assert!(
        !closest_kept(
            object_id(2),
            (&seer, &farm),
            filter,
            (PlayerId::new(1), farmer),
            checks,
        )
        .unwrap()
    );
}

/// A scene world of two players holding `objects`: the objects `ClosestObject`'s checks read by
/// id.
/// What `ClosestObject` reads of a test world beside its objects: attribute 290 for a pasture's
/// workers, the content mode 0.
const fn closest_world() -> ClosestWorld {
    ClosestWorld {
        pasture_workers: AttributeId::named(290),
        mode_one: false,
    }
}

/// `ClosestObject`'s pasture (masters 1893 and 1897, `FUN_140c488b0`): one in state 2 as a
/// building is kept while its livestock's workers are below its owner's attribute 290 and it
/// holds livestock, then without the farms' workers cap; one in another state skips that test.
#[test]
fn closest_object_takes_pasture_with_room_and_livestock() {
    use crate::map::spatial::closest::{Searcher, farm_kept};
    let filter = Filter {
        group: Some(class_id(49)),
        ..any_filter()
    };
    let pasture_master = Master {
        id: ids::PASTURE_MANGROVE,
        kind: MasterKind::BUILDING,
        class: class::FARM,
        ..Master::default()
    };
    let animal_master = Master {
        id: master_id(1060),
        class: class::FARM,
        ..Master::default()
    };
    let at = |x| Position::new(x, 1.5, 0.0);
    let run = |room: f32, livestock: &[i32], state| {
        let mut world = found_world(vec![
            SceneObject {
                workers: WorkerCount::ONE,
                ..SceneObject::new(5, PlayerId::new(2), pasture_master.clone(), at(1.5))
            },
            SceneObject {
                workers: WorkerCount::ONE,
                ..SceneObject::new(6, PlayerId::new(2), animal_master.clone(), at(3.5))
            },
        ]);
        world.players[2].attributes.resize(300, Amount::ZERO);
        world.players[2].attributes[290] = Amount::new(room);
        world.parts.building_mut(object_id(5)).unwrap().livestock =
            livestock.iter().map(|&id| object_id(id)).collect();
        let placed = Placed {
            class: class::FARM,
            master: ids::PASTURE_MANGROVE,
            player: PlayerId::new(2),
            ..villager(1.5, 1.5)
        };
        let farmer = Searcher {
            farmer: true,
            ..Searcher::default()
        };
        farm_kept(
            object_id(5),
            (&seer_in(state), &placed),
            filter,
            farmer,
            (world.objects(), closest_world()),
        )
        .unwrap()
    };
    assert!(run(2.0, &[6], ObjectState::Alive));
    assert!(!run(1.0, &[6], ObjectState::Alive));
    assert!(!run(2.0, &[], ObjectState::Alive));
    assert!(run(1.0, &[], ObjectState::Foundation));
}

fn found_world(objects: Vec<SceneObject>) -> World {
    objects
        .into_iter()
        .fold(Scene::open(GridSize::new(8, 8)).players(2), Scene::object)
        .build()
}

/// The object 7 that `placed` places, of `master`, with the action layer `actions`.
fn placed_object(placed: &Placed, master: Master, actions: Option<Action>) -> SceneObject {
    SceneObject {
        actions,
        ..SceneObject::new(7, placed.player, master, placed.position)
    }
}

/// A herded animal's action layer: its top action the Artifact with a point to walk to.
fn herding() -> Action {
    Action {
        list: ActionList::default().with_artifact(Some(Artifact {
            goal: Some(Position::new(5.5, 5.5, 0.0)),
            ..Artifact::new(PlayerId::new(1), None)
        })),
        ..Action::default()
    }
}

/// Player `player`'s relation toward `other` in the spatial copy becomes `relation`.
fn set_relation(spatial: &mut Spatial, (player, other): (u8, u8), relation: Relation) {
    spatial
        .relations
        .get_mut(PlayerId::new(player))
        .unwrap()
        .set(PlayerId::new(other), relation);
}

/// `+0x924` is the seer's: an object in the sight registry reads its capture flag there, one
/// outside it (or a map without sight) reads none.
#[test]
fn capture_flag_comes_from_the_seer_of_the_object() {
    let mut spatial = Spatial::new(GridSize::new(10, 10));
    assert_eq!(spatial.capture_of(object_id(5)), CaptureKind::None);
    let mut visibility = Visibility::new(
        GridSize::new(10, 10),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        0,
        Vec::new(),
    );
    visibility.add_seer(
        object_id(5),
        Seer {
            capture: CaptureKind::Again,
            ..seer_in(ObjectState::Alive)
        },
    );
    spatial.sight = Some(visibility);
    assert_eq!(
        (
            spatial.capture_of(object_id(5)),
            spatial.capture_of(object_id(6))
        ),
        (CaptureKind::Again, CaptureKind::None)
    );
}

/// `FUN_141195750` and `FUN_1411945a0`: a villager (class 4) gathers from farms, a fishing
/// ship (masters 13, 2356, 2171) from fish traps; no other class or master does.
#[test]
fn villagers_gather_farms_and_fishing_ships_fish_traps() {
    use crate::map::spatial::closest::{gathers_farms, gathers_fish_traps};
    let roles = roles();
    assert!(gathers_farms(class_id(4), &roles));
    assert!(!gathers_farms(class_id(22), &roles));
    for master in [13, 2356, 2171] {
        assert!(gathers_fish_traps(master_id(master), &roles), "{master}");
    }
    assert!(!gathers_fish_traps(master_id(83), &roles));
}
