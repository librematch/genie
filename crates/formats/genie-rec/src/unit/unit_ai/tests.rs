use crate::element::ReadableHeaderElement;
use crate::test_support::header_reader;
use crate::test_support::{floats, read_all, words};
use crate::unit::unit_ai::{
    UnitAi, UnitAiNotification, UnitAiOrder, UnitAiOrderHistory, UnitAiRetargetEntry,
};

/// An order: issuer, type, priority, target and its player, then the target location and
/// the range.
fn order_bytes() -> Vec<u8> {
    let mut bytes = words(&[1, 2, 3, 500, 2]);
    bytes.extend(floats(&[1.0, 2.0, 0.5, 4.0]));
    bytes
}

/// An order history entry at a version with the target's attack category (10.50 on).
fn history_bytes(category: Option<i32>) -> Vec<u8> {
    let mut bytes = words(&[7, 8, 900]);
    bytes.extend(floats(&[10.0, 11.0, 0.0]));
    bytes.extend(words(&[501]));
    if let Some(category) = category {
        bytes.extend(words(&[category]));
    }
    bytes.extend(floats(&[12.0, 13.0, 1.0]));
    bytes
}

/// A unit AI up to its attacking units at save `version`'s layout before the later fields.
fn unit_ai_bytes() -> Vec<u8> {
    let mut bytes = words(&[1, -1, 5, 6, 502]);
    bytes.extend([0xFF, 0xFF, 0, 0]);
    bytes.extend(floats(&[3.0, 4.0, 0.0, 1.5]));
    bytes.extend(words(&[-1, 9, -1, 3, -2, 10, 20, 30, 40, 50, 60, 503]));
    bytes.extend(floats(&[2.5, 5.0, 6.0, 0.0]));
    bytes.extend([4, 0, 0, 0]);
    bytes.extend(words(&[1]));
    bytes.extend(order_bytes());
    bytes.extend(words(&[1, 11, 12, 13, 14, 15, 16]));
    bytes.extend(words(&[2, 504, 505]));
    bytes.extend([1, 3]);
    bytes.extend(floats(&[7.0, 8.0]));
    bytes.extend(words(&[70]));
    bytes.push(2);
    bytes.extend(words(&[506, 0, 4]));
    bytes
}

#[test]
fn an_order_reads_its_target_and_range() {
    let order = read_all::<UnitAiOrder>(&order_bytes(), 11.97);
    assert_eq!((order.issuer, order.order_kind, order.priority), (1, 2, 3));
    assert_eq!(u32::from(order.target_id), 500);
    assert_eq!(u8::from(order.target_player), 2);
    assert_eq!(order.target_location, (1.0, 2.0, 0.5));
    assert_eq!(order.range.to_bits(), 4.0_f32.to_bits());
}

#[test]
fn notification_reads_its_parameters() {
    let note = read_all::<UnitAiNotification>(&words(&[1, 2, 3, 4, 5, 6]), 11.97);
    assert_eq!(
        (note.caller, note.recipient, note.notification_kind),
        (1, 2, 3)
    );
    assert_eq!(note.params, (4, 5, 6));
}

#[test]
fn an_order_history_entry_has_the_attack_category_from_10_50() {
    let before = read_all::<UnitAiOrderHistory>(&history_bytes(None), 10.48);
    assert_eq!((before.order, before.action, before.time), (7, 8, 900));
    assert_eq!(before.position, (10.0, 11.0, 0.0));
    assert_eq!(u32::from(before.target_id), 501);
    assert_eq!(before.target_attack_category, None);
    assert_eq!(before.target_position, (12.0, 13.0, 1.0));
    let with = read_all::<UnitAiOrderHistory>(&history_bytes(Some(3)), 10.50);
    assert_eq!(with.target_attack_category, Some(3));
    let none = read_all::<UnitAiOrderHistory>(&history_bytes(Some(-1)), 10.50);
    assert_eq!(none.target_attack_category, None);
}

#[test]
fn retarget_entry_reads_its_target_and_timeout() {
    let entry = read_all::<UnitAiRetargetEntry>(&words(&[507, 25]), 11.97);
    assert_eq!(u32::from(entry.target_id), 507);
    assert_eq!(entry.retarget_timeout, 25);
}

/// Before 10.48 the AI ends at the patrol waypoint.
#[test]
fn an_old_unit_ai_ends_at_its_patrol_waypoint() {
    let ai = read_all::<UnitAi>(&unit_ai_bytes(), 10.0);
    assert_eq!(ai.mood, Some(1));
    assert_eq!(ai.current_order, None);
    assert_eq!(ai.current_order_priority, Some(5));
    assert_eq!(ai.current_action, Some(6));
    assert_eq!(ai.current_target, Some(502));
    assert_eq!(ai.current_target_kind, None);
    assert_eq!(ai.current_target_location, (3.0, 4.0, 0.0));
    assert_eq!(ai.desired_target_distance.to_bits(), 1.5_f32.to_bits());
    assert_eq!(
        (ai.last_action, ai.last_order, ai.last_target),
        (None, Some(9), None)
    );
    assert_eq!((ai.last_target_kind, ai.last_update_kind), (Some(3), None));
    assert_eq!(
        [
            ai.idle_timer,
            ai.idle_timeout,
            ai.adjusted_idle_timeout,
            ai.secondary_timer,
            ai.lookaround_timer,
            ai.lookaround_timeout,
        ],
        [10, 20, 30, 40, 50, 60]
    );
    assert_eq!(ai.defend_target.map(u32::from), Some(503));
    assert_eq!(ai.defense_buffer.to_bits(), 2.5_f32.to_bits());
    assert_eq!(ai.last_world_position.location, (5.0, 6.0, 0.0));
    assert_eq!(ai.last_world_position.facet_to_next_waypoint, 4);
    assert_eq!(ai.orders.len(), 1);
    assert_eq!(ai.notifications.len(), 1);
    assert_eq!(ai.notifications[0].params, (14, 15, 16));
    assert_eq!(
        ai.attacking_units
            .iter()
            .map(|&id| u32::from(id))
            .collect::<Vec<_>>(),
        [504, 505]
    );
    assert!(ai.stop_after_target_killed);
    assert_eq!(ai.state, 3);
    assert_eq!(ai.state_position, (7.0, 8.0));
    assert_eq!(ai.time_since_enemy_sighting, 70);
    assert_eq!(ai.alert_mode, 2);
    assert_eq!(ai.alert_mode_object_id.map(u32::from), Some(506));
    assert!(ai.patrol_path.is_none());
    assert_eq!(ai.patrol_current_waypoint, 4);
    assert_eq!(ai.order_history.len(), 0);
}

/// From 11.44 on every later field is there: the history, the retargeting, the best unit
/// to attack and the formation.
#[test]
fn unit_ai_of_11_44_reads_every_later_field() {
    let mut bytes = unit_ai_bytes();
    bytes.extend(words(&[1]));
    bytes.extend(history_bytes(Some(-1)));
    bytes.extend(words(&[80, 90, 1, 508, 30, 509]));
    bytes.push(6);
    let ai = read_all::<UnitAi>(&bytes, 11.44);
    assert_eq!(ai.order_history.len(), 1);
    assert_eq!(
        (ai.last_retarget_time, ai.randomized_retarget_timer),
        (80, 90)
    );
    assert_eq!(ai.retarget_entries.len(), 1);
    assert_eq!(ai.retarget_entries[0].retarget_timeout, 30);
    assert_eq!(ai.best_unit_to_attack, Some(509));
    assert_eq!(ai.formation_kind, 6);
}

/// Each later field starts at its own version: at 11.05 the retarget entries are read, the
/// best unit to attack and the formation are not.
#[test]
fn unit_ai_of_11_05_stops_after_its_retarget_entries() {
    let mut bytes = unit_ai_bytes();
    bytes.extend(words(&[0, 80, 90, 0]));
    let ai = read_all::<UnitAi>(&bytes, 11.05);
    assert_eq!(
        (ai.last_retarget_time, ai.randomized_retarget_timer),
        (80, 90)
    );
    assert_eq!(ai.best_unit_to_attack, None);
}

#[test]
fn short_unit_ai_is_an_error() {
    let bytes = unit_ai_bytes();
    let mut reader = header_reader(&bytes[..bytes.len() - 1], 10.0);
    assert!(<UnitAi as ReadableHeaderElement>::read_from(&mut reader).is_err());
}
