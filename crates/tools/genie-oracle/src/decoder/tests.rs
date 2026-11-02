use crate::decoder::Decoder;
use crate::test_ids::object_id;
use genie_core::commands::{Command, UnitList};
use genie_rec::de::command as packets;

/// INTERACT's packet of player 2 with `num` units listed.
fn interact(num: u16, ids: Vec<i32>) -> Vec<u8> {
    packets::Interact {
        commp_id: 2,
        tid: 500,
        x: 1.0,
        y: 2.0,
        num,
        field_0x12: 1,
        field_0x13: 0,
        field_0x14: 1,
        field_0x15: 1,
        field_0x16: 3,
        ids,
    }
    .encode()
    .unwrap()
}

/// An INTERACT of its own list notes its count's units for a primed run; a later count of
/// `0xffff` orders the last commanded units and leaves the note.
#[test]
fn an_interact_notes_its_own_list_and_later_one_keeps_it() {
    let mut d = Decoder::default();
    assert!(matches!(
        d.command(&interact(2, vec![10, 11, 12])),
        Some(Command::Order(_))
    ));
    assert_eq!(
        d.last_commanded.get(&2),
        Some(&vec![Some(object_id(10)), Some(object_id(11))])
    );
    assert!(matches!(
        d.command(&interact(UnitList::LAST_COMMANDED, vec![])),
        Some(Command::Order(_))
    ));
    assert_eq!(
        d.last_commanded.get(&2),
        Some(&vec![Some(object_id(10)), Some(object_id(11))])
    );
}

/// FOLLOW and GUARD list their units without the unit-list reader: no note.
#[test]
fn follow_and_guard_note_no_list() {
    let mut d = Decoder::default();
    let follow = packets::Follow {
        commp_id: 1,
        num: 1,
        gap_0x06: [0; 2],
        target: 900,
        ids: vec![8],
    }
    .encode()
    .unwrap();
    let guard = packets::Guard {
        commp_id: 1,
        num: 1,
        gap_0x06: [0; 2],
        target: 901,
        ids: vec![8],
    }
    .encode()
    .unwrap();
    assert!(d.command(&follow).is_some() && d.command(&guard).is_some());
    assert!(d.last_commanded.is_empty());
}

/// A packet without its player byte gives no command.
#[test]
fn packet_without_player_gives_no_command() {
    assert_eq!(Decoder::default().command(&[0]), None);
}
