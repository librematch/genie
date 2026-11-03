use crate::test_support::{floats, header_reader, read_all, words};
use crate::unit::movers::{MovementData, MovingUnitAttributes, PathData};

/// A path at save `version`'s layout: before 10.25 it has the disable flags, from 10.20
/// on the enable flags too.
fn path_bytes(flags: &[i32]) -> Vec<u8> {
    let mut bytes = words(&[1, 2, 3, 4, 5]);
    bytes.extend(words(flags));
    bytes.extend(words(&[6]));
    bytes.extend(floats(&[2.5]));
    bytes.extend(words(&[700]));
    bytes.extend(floats(&[0.25]));
    bytes.extend(words(&[8, 9]));
    bytes
}

/// A moving unit with one path, a future path, movement data and the `waypoints` bytes (a
/// count, then that many points).
fn moving_bytes(waypoints: &[u8], substitute: i32) -> Vec<u8> {
    let mut bytes = words(&[11]);
    bytes.extend(floats(&[0.5, 0.25, 0.0, 1.5]));
    bytes.extend(words(&[12, 13, 14, -1, 2]));
    bytes.extend([1, 2, 1]);
    bytes.extend(words(&[1]));
    bytes.extend(path_bytes(&[]));
    bytes.extend(words(&[1]));
    bytes.extend(path_bytes(&[]));
    bytes.extend(words(&[1]));
    bytes.extend(floats(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]));
    bytes.extend(floats(&[10.0, 20.0, 0.5, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]));
    bytes.extend(words(&[300]));
    bytes.extend(waypoints);
    bytes.extend(words(&[substitute]));
    bytes.extend(floats(&[7.0, 8.0, 0.0]));
    bytes.extend(words(&[2]));
    bytes
}

#[test]
fn path_has_disable_flags_before_10_25_and_enable_flags_from_10_20() {
    let old = read_all::<PathData>(&path_bytes(&[33]), 10.0);
    assert_eq!((old.disable_flags, old.enable_flags), (Some(33), None));
    let both = read_all::<PathData>(&path_bytes(&[33, 44]), 10.20);
    assert_eq!(
        (both.disable_flags, both.enable_flags),
        (Some(33), Some(44))
    );
    let new = read_all::<PathData>(&path_bytes(&[]), 10.25);
    assert_eq!((new.disable_flags, new.enable_flags), (None, None));
    assert_eq!(
        [
            new.id,
            new.linked_path_kind,
            new.waypoint_level,
            new.path_id
        ],
        [1, 2, 3, 4]
    );
    assert_eq!((new.waypoint, new.state, new.target_id), (5, 6, 700));
    assert_eq!(new.range.to_bits(), 2.5_f32.to_bits());
    assert_eq!(new.pause_time.to_bits(), 0.25_f32.to_bits());
    assert_eq!((new.continue_counter, new.flags), (8, 9));
}

#[test]
fn movement_data_writes_back_its_velocity_and_acceleration() {
    let data = MovementData {
        velocity: (1.0, -2.0, 0.5),
        acceleration: (0.0, 0.25, -1.0),
    };
    let mut bytes = vec![];
    data.write_to(&mut bytes).unwrap();
    assert_eq!(bytes, floats(&[1.0, -2.0, 0.5, 0.0, 0.25, -1.0]));
    let read = MovementData::read_from(&mut header_reader(&bytes, 11.97)).unwrap();
    assert_eq!(read.velocity, data.velocity);
    assert_eq!(read.acceleration, data.acceleration);
}

#[test]
fn moving_unit_reads_its_paths_placement_and_waypoints() {
    let mut points = words(&[2]);
    points.extend(floats(&[1.0, 1.0, 0.0, 2.0, 2.0, 0.0]));
    let unit = read_all::<MovingUnitAttributes>(&moving_bytes(&points, 1), 11.97);
    assert_eq!(unit.trail_remainder, 11);
    assert_eq!(unit.velocity, (0.5, 0.25, 0.0));
    assert_eq!(unit.angle.to_bits(), 1.5_f32.to_bits());
    assert_eq!(
        (
            unit.turn_towards_time,
            unit.turn_timer,
            unit.continue_counter
        ),
        (12, 13, 14)
    );
    assert_eq!(unit.current_terrain_exception, (None, Some(2)));
    assert_eq!(
        (unit.waiting_to_move, unit.wait_delays_count, unit.on_ground),
        (1, 2, 1)
    );
    assert_eq!(unit.path_data.len(), 1);
    assert_eq!(unit.future_path_data.map(|path| path.target_id), Some(700));
    assert_eq!(
        unit.movement_data.map(|data| data.acceleration),
        Some((4.0, 5.0, 6.0))
    );
    assert_eq!(unit.position, (10.0, 20.0, 0.5));
    assert_eq!(unit.orientation_forward, (1.0, 0.0, 0.0));
    assert_eq!(unit.orientation_right, (0.0, 1.0, 0.0));
    assert_eq!(unit.last_move_time, 300);
    assert_eq!(
        unit.user_defined_waypoints,
        [(1.0, 1.0, 0.0), (2.0, 2.0, 0.0)]
    );
    assert_eq!(unit.substitute_position, Some((7.0, 8.0, 0.0)));
    assert_eq!(unit.consecutive_substitute_count, 2);
}

/// A negative waypoint count reads no waypoints; the substitute position is read but kept
/// only behind its flag.
#[test]
fn negative_waypoint_count_and_no_substitute_read_nothing() {
    let unit = read_all::<MovingUnitAttributes>(&moving_bytes(&words(&[-1]), 0), 11.97);
    assert_eq!(unit.user_defined_waypoints, []);
    assert_eq!(unit.substitute_position, None);
    assert_eq!(unit.consecutive_substitute_count, 2);
}

/// Without the future path's and the movement data's flags neither is read.
#[test]
fn moving_unit_without_flags_has_no_future_path_or_movement() {
    let mut bytes = words(&[11]);
    bytes.extend(floats(&[0.0; 4]));
    bytes.extend(words(&[0, 0, 0, -1, -1]));
    bytes.extend([0, 0, 0]);
    bytes.extend(words(&[0, 0, 0]));
    bytes.extend(floats(&[0.0; 9]));
    bytes.extend(words(&[0, 0, 0]));
    bytes.extend(floats(&[0.0; 3]));
    bytes.extend(words(&[0]));
    let unit = read_all::<MovingUnitAttributes>(&bytes, 11.97);
    assert!(unit.future_path_data.is_none());
    assert!(unit.movement_data.is_none());
    assert_eq!(unit.path_data.len(), 0);
}
