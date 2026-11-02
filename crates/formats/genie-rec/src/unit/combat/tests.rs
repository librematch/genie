use crate::element::ReadableHeaderElement;
use crate::test_support::{Bytes, header_reader, read_all};
use crate::unit::combat::{CombatUnitAttributes, MissileUnitAttributes};

/// A combat unit's head: the volley, the special animation, no own base and the six
/// attribute amounts.
fn head() -> Bytes {
    Bytes::default()
        .raw(&[1, 0, 0])
        .u16(10)
        .u16(20)
        .u16(30)
        .u16(40)
        .u16(50)
        .u16(60)
}

#[test]
fn missile_without_its_own_base_reads_its_range_and_shooter() {
    let bytes = Bytes::default().floats(&[7.0]).i32(610).u8(0).0;
    let missile = read_all::<MissileUnitAttributes>(&bytes, 11.97);
    assert_eq!(missile.max_range.to_bits(), 7.0_f32.to_bits());
    assert_eq!(u32::from(missile.fired_from_id), 610);
    assert!(missile.own_base.is_none());
}

/// With its flag set the missile reads its own unit type, here from nothing: an error.
#[test]
fn missile_own_base_is_read_behind_its_flag() {
    let bytes = Bytes::default().floats(&[7.0]).i32(610).u8(1).0;
    let read = MissileUnitAttributes::read_from(&mut header_reader(&bytes, 11.97));
    assert!(read.is_err());
}

/// Before 9.16 the unit ends with its unit AI's flag.
#[test]
fn an_old_combat_unit_has_only_its_amounts_and_ai_flag() {
    let unit = read_all::<CombatUnitAttributes>(&head().i32(0).0, 9.0);
    assert_eq!(
        (unit.next_volley, unit.using_special_attack_animation),
        (1, 0)
    );
    assert_eq!(unit.attribute_amounts, [10, 20, 30, 40, 50, 60]);
    assert_eq!(unit.decay_timer, 0);
    assert!(unit.unit_ai.is_none());
    assert_eq!(unit.town_bell_target_location, None);
}

#[test]
fn combat_unit_of_11_74_reads_every_counter_and_the_town_bell() {
    let bytes = head()
        .u16(5)
        .words(&[6, 7])
        .u8(2)
        .i32(0)
        .u8(1)
        .i32(620)
        .floats(&[30.0, 31.0])
        .words(&[621, 4, 8])
        .floats(&[2.5])
        .raw(&[3, 4])
        .0;
    let unit = read_all::<CombatUnitAttributes>(&bytes, 11.74);
    assert_eq!(unit.decay_timer, 5);
    assert_eq!(
        (unit.raider_build_countdown, unit.locked_down_count),
        (6, 7)
    );
    assert_eq!(unit.inside_garrison_count, 2);
    assert_eq!(unit.town_bell_flag, 1);
    assert_eq!(unit.town_bell_target_id.map(u32::from), Some(620));
    assert_eq!(unit.town_bell_target_location, Some((30.0, 31.0)));
    assert_eq!(unit.town_bell_target_id_2.map(u32::from), Some(621));
    assert_eq!((unit.town_bell_target_type, unit.town_bell_action), (4, 8));
    assert_eq!(unit.berserker_timer.to_bits(), 2.5_f32.to_bits());
    assert_eq!((unit.num_builders, unit.num_healers), (3, 4));
}

/// A negative x is no town bell location.
#[test]
fn town_bell_is_no_location_with_negative_x() {
    let bytes = head()
        .u16(5)
        .words(&[6, 7, 0])
        .u8(0)
        .i32(-1)
        .floats(&[-1.0, 31.0])
        .0;
    let unit = read_all::<CombatUnitAttributes>(&bytes, 10.30);
    assert_eq!(unit.town_bell_target_id, None);
    assert_eq!(unit.town_bell_target_location, None);
}
