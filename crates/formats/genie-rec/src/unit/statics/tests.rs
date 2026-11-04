use crate::element::{ReadOptionalHeader, WriteHeader};
use crate::errors::not_written;
use crate::test_support::{Bytes, header_reader, read_all};
use crate::unit::Unit;
use crate::unit::statics::StaticUnitAttributes;
use crate::unit_kind::UnitBaseClass;

/// An object up to its offsets.
fn identity() -> Bytes {
    Bytes::default()
        .u8(2)
        .u16(83)
        .u16(1000)
        .i32(-1)
        .floats(&[25.0])
        .u8(2)
        .raw(&[1, 0, 1])
        .i32(600)
        .u8(3)
        .floats(&[10.5, 11.5, 0.0])
        .u16(1)
        .u16(2)
        .u16(3)
        .u16(4)
}

/// The rest after the selected group: the attribute held, the damage, two pathing group
/// members, a group id and an empty sprite list.
fn rest(bytes: Bytes) -> Bytes {
    bytes
        .u16(7)
        .floats(&[50.0])
        .raw(&[3, 1, 9, 1])
        .words(&[2, 601, 602, 40])
        .raw(&[1, 1, 0])
}

#[test]
fn an_object_before_11_58_has_selected_group() {
    let object = read_all::<StaticUnitAttributes>(&rest(identity().u8(4)).0, 11.57);
    assert_eq!(u8::from(object.owner_id), 2);
    assert_eq!(u16::from(object.unit_kind_id), 83);
    assert_eq!(u16::from(object.sprite_id), 1000);
    assert_eq!(object.garrisoned_in_id, None);
    assert_eq!(object.hit_points.to_bits(), 25.0_f32.to_bits());
    assert_eq!(object.object_state, 2);
    assert_eq!(
        (
            object.sleep_flag,
            object.dopple_flag,
            object.go_to_sleep_flag
        ),
        (true, false, true)
    );
    assert_eq!(u32::from(object.id), 600);
    assert_eq!(object.facet, 3);
    assert_eq!(object.position, (10.5, 11.5, 0.0));
    assert_eq!(
        (object.screen_offset, object.shadow_offset),
        ((1, 2), (3, 4))
    );
    assert_eq!(object.selected_group, Some(4));
    assert_eq!(object.attribute_kind_held, 7);
    assert_eq!(object.attribute_amount_held.to_bits(), 50.0_f32.to_bits());
    assert_eq!(
        (
            object.worker_count,
            object.current_damage,
            object.damaged_lately_timer
        ),
        (3, 1, 9)
    );
    assert!(object.under_attack);
    assert_eq!(
        object
            .pathing_group_members
            .iter()
            .map(|&id| u32::from(id))
            .collect::<Vec<_>>(),
        [601, 602]
    );
    assert_eq!(object.group_id, Some(40));
    assert_eq!(object.roo_already_called, 1);
    assert_eq!(object.sprite_list.map(|list| list.sprites.len()), Some(0));
}

#[test]
fn an_object_from_11_58_has_no_selected_group() {
    let object = read_all::<StaticUnitAttributes>(&rest(identity()).0, 11.58);
    assert_eq!(object.selected_group, None);
    assert_eq!(object.group_id, Some(40));
    let none = read_all::<StaticUnitAttributes>(&rest(identity().u8(0xFF)).0, 11.0);
    assert_eq!(none.selected_group, None);
}

/// A unit of class 0 is the end of a list; any other reads the classes up to its own.
#[test]
fn static_unit_reads_only_its_static_attributes() {
    let end = ReadOptionalHeader::read_from(&mut header_reader(&[0], 11.58));
    assert!(matches!(end, Ok(None::<Unit>)));
    let bytes = Bytes::default().u8(10).raw(&rest(identity()).0).0;
    let unit: Unit = ReadOptionalHeader::read_from(&mut header_reader(&bytes, 11.58))
        .unwrap()
        .unwrap();
    assert_eq!(unit.unit_base_class, UnitBaseClass::Static);
    assert!(unit.animated.is_none() && unit.moving.is_none() && unit.building.is_none());
}

#[test]
fn the_old_format_units_are_read_only() {
    let bytes = Bytes::default().u8(10).raw(&rest(identity()).0).0;
    let unit: Unit = ReadOptionalHeader::read_from(&mut header_reader(&bytes, 11.58))
        .unwrap()
        .unwrap();
    let error = unit.write_to(&mut vec![]).unwrap_err();
    assert_eq!(error, not_written());
}
