use crate::test_support::{Bytes, header_reader, read_all};
use crate::unit::building::{BuildingUnitAttributes, GatherPoint};

/// A building's head up to its gather point.
fn head() -> Bytes {
    Bytes::default()
        .u8(1)
        .floats(&[12.5])
        .i32(-1)
        .raw(&[2, 1])
        .words(&[100, 200, 3, 1])
}

/// A gather point: its flag, a location, an object and the object's type.
fn gather_point(exists: i32, object: i32, unit_kind: u16) -> Bytes {
    Bytes::default()
        .i32(exists)
        .floats(&[4.0, 5.0, 0.0])
        .i32(object)
        .u16(unit_kind)
}

/// The rest of an old building: the links (an owner and two children), no actions, a queue
/// of one entry and its totals.
fn links_and_queues(bytes: Bytes) -> Bytes {
    bytes
        .i32(630)
        .words(&[631, -1, 632, -1])
        .u8(1)
        .u16(0)
        .u16(0)
        .u16(1)
        .u16(83)
        .u16(2)
        .u16(1)
        .u16(2)
        .u8(1)
        .u16(0)
}

fn read_gather_point(bytes: &Bytes) -> Option<GatherPoint> {
    GatherPoint::read_from(&mut header_reader(&bytes.0, 11.97)).unwrap()
}

#[test]
fn gather_point_is_none_location_or_an_object() {
    assert!(read_gather_point(&gather_point(0, 5, 5)).is_none());
    assert!(matches!(
        read_gather_point(&gather_point(1, -1, 0xFFFF)),
        Some(GatherPoint::Location { x, y, .. }) if (x, y) == (4.0, 5.0)
    ));
    assert!(matches!(
        read_gather_point(&gather_point(1, 640, 83)),
        Some(GatherPoint::Object { id, unit_kind })
            if u32::from(id) == 640 && u16::from(unit_kind) == 83
    ));
}

/// Before 10.54 the building has no pending order and ends after its queue's actions.
#[test]
fn an_old_building_reads_its_links_and_queue() {
    let bytes = links_and_queues(head().raw(&gather_point(0, 0, 0).0).u8(1)).0;
    let building = read_all::<BuildingUnitAttributes>(&bytes, 10.0);
    assert!(building.built);
    assert_eq!(building.build_points.to_bits(), 12.5_f32.to_bits());
    assert_eq!(building.unique_build_id, None);
    assert_eq!((building.culture, building.burning), (2, 1));
    assert_eq!(
        [
            building.last_burn_time,
            building.last_garrison_time,
            building.relic_count,
            building.specific_relic_count,
        ],
        [100, 200, 3, 1]
    );
    assert!(building.gather_point.is_none());
    assert!(building.desolid_flag);
    assert_eq!(building.pending_order, 0);
    assert_eq!(building.linked_owner.map(u32::from), Some(630));
    assert_eq!(
        building
            .linked_children
            .iter()
            .map(|&id| u32::from(id))
            .collect::<Vec<_>>(),
        [631, 632]
    );
    assert_eq!(building.captured_unit_count, 1);
    assert_eq!(building.production_queue.len(), 1);
    assert_eq!(u16::from(building.production_queue[0].unit_kind_id), 83);
    assert_eq!(building.production_queue[0].count, 2);
    assert_eq!(building.production_queue_total_units, 2);
    assert!(building.production_queue_enabled);
    assert!(building.terrain_kind.is_none());
}

/// From 11.54 on the building has the pending order, the gate's fields, the terrain and both
/// flags; of the two endpoints the second is kept.
#[test]
fn building_of_11_54_reads_every_later_field() {
    let head = head().raw(&gather_point(1, -1, 0xFFFF).0).u8(0).i32(9);
    let bytes = links_and_queues(head)
        .floats(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0])
        .words(&[1, 2, 3])
        .raw(&[7, 1, 1])
        .0;
    let building = read_all::<BuildingUnitAttributes>(&bytes, 11.54);
    assert_eq!(building.pending_order, 9);
    assert_eq!(building.endpoint, (4.0, 5.0, 6.0));
    assert_eq!(
        (
            building.gate_locked,
            building.first_update,
            building.close_timer
        ),
        (1, 2, 3)
    );
    assert_eq!(building.terrain_kind.map(u16::from), Some(7));
    assert!(building.semi_asleep && building.snow_flag);
}
