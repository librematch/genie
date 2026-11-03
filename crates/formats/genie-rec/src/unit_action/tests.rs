use crate::element::ReadableHeaderElement;
use crate::test_support::{Bytes, header_reader, read_all};
use crate::unit_action::{ActionAttack, ActionKind, UnitAction};

/// An action's head at a version up to 11.76 (a one-byte state): no targets, no task, no
/// sub-actions and no sprite.
fn head(action_kind: u16) -> Bytes {
    Bytes::default()
        .u16(action_kind)
        .u8(2)
        .words(&[0, 0, -1, 650])
        .floats(&[1.0, 2.0, 0.0, 0.5])
        .u8(1)
        .u16(0xFFFF)
        .u8(3)
        .u16(0)
        .u16(0xFFFF)
}

/// An attack's parameters: ranges, missile, delays and flags, the three sprites (the wait
/// sprite none) and the last target position.
fn attack() -> Bytes {
    Bytes::default()
        .floats(&[5.0, 1.0])
        .u16(511)
        .u16(4)
        .u16(1)
        .u16(0)
        .u8(1)
        .u16(10)
        .u16(11)
        .u16(0xFFFF)
        .floats(&[6.0, 7.0, 0.0])
}

#[test]
fn an_attack_reads_its_ranges_sprites_and_last_target() {
    let attack = read_all::<ActionAttack>(&attack().0, 11.76);
    assert_eq!(attack.range.to_bits(), 5.0_f32.to_bits());
    assert_eq!(attack.min_range.to_bits(), 1.0_f32.to_bits());
    assert_eq!(u16::from(attack.missile_id), 511);
    assert_eq!(
        (
            attack.frame_delay,
            attack.need_to_attack,
            attack.was_same_owner
        ),
        (4, 1, 0)
    );
    assert_eq!(attack.indirect_fire_flag, 1);
    assert_eq!(attack.move_sprite_id.map(u16::from), Some(10));
    assert_eq!(attack.fight_sprite_id.map(u16::from), Some(11));
    assert_eq!(attack.wait_sprite_id, None);
    assert_eq!(attack.last_target_position, (6.0, 7.0, 0.0));
}

/// Every action type the reader knows, behind its head, ends a list at type 0.
#[test]
fn list_reads_every_known_action_kind() {
    let mut bytes = Bytes::default();
    for (action_kind, params) in [
        (1, Bytes::default().floats(&[2.0])),
        (3, Bytes::default().i32(77)),
        (4, Bytes::default()),
        (9, attack()),
        (10, Bytes::default()),
        (12, Bytes::default()),
        (13, Bytes::default()),
        (21, Bytes::default().floats(&[0.25])),
        (107, Bytes::default()),
    ] {
        bytes = bytes.raw(&head(action_kind).0).raw(&params.0);
    }
    let bytes = bytes.u16(0).0;
    let mut reader = header_reader(&bytes, 11.76);
    let list = UnitAction::read_list_from(&mut reader).unwrap();
    assert_eq!(list.len(), 9);
    assert!(
        matches!(list[0].params, ActionKind::MoveTo(to) if to.range.to_bits() == 2.0_f32.to_bits())
    );
    assert!(matches!(list[1].params, ActionKind::Enter(enter) if enter.first_time == 77));
    assert!(matches!(list[2].params, ActionKind::Explore));
    assert!(matches!(list[3].params, ActionKind::Attack(_)));
    assert!(matches!(list[4].params, ActionKind::Bird));
    assert!(matches!(list[5].params, ActionKind::Transport));
    assert!(matches!(list[6].params, ActionKind::Guard));
    assert!(
        matches!(list[7].params, ActionKind::Make(make) if make.work_timer.to_bits() == 0.25_f32.to_bits())
    );
    assert!(matches!(list[8].params, ActionKind::Artifact));
    let first = &list[0];
    assert_eq!(first.state, 2);
    assert_eq!(first.target_object_id, None);
    assert_eq!(first.target_object_id_2.map(u32::from), Some(650));
    assert_eq!(first.target_position, (1.0, 2.0, 0.0));
    assert_eq!(first.timer.to_bits(), 0.5_f32.to_bits());
    assert_eq!((first.target_moved_state, first.task_id), (1, None));
    assert_eq!(first.sub_action_value, 3);
    assert!(first.sub_actions.is_empty() && first.sprite_id.is_none());
}

/// After 11.76 the state is a word.
#[test]
fn an_action_after_11_76_has_word_state() {
    let bytes = Bytes::default()
        .u16(4)
        .i32(0x0102)
        .words(&[0, 0, -1, -1])
        .floats(&[0.0; 4])
        .u8(0)
        .u16(0xFFFF)
        .u8(0)
        .u16(0)
        .u16(0xFFFF)
        .0;
    let action = read_all::<UnitAction>(&bytes, 11.77);
    assert_eq!(action.state, 0x0102);
}

#[test]
fn an_unknown_action_kind_is_an_error() {
    let e = UnitAction::read_from(&mut header_reader(&head(2).0, 11.76)).unwrap_err();
    assert!(matches!(e, crate::Error::UnsupportedUnitAction(2)));
}
