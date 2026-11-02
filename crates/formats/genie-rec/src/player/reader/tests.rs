use crate::player::Player;
use crate::player::reader::{
    expect_marker, read_selection, read_unit_list, skip_diplomacy_and_trade_routes,
    skip_formations_and_escrow, skip_market, skip_object_counts, skip_old_battle_stats,
    skip_queues_and_dodging, skip_updates_and_attacks, skip_view_ai_and_selection,
};
use crate::test_support::{Bytes, consumed, header_reader, skipped_length};

#[test]
fn check_byte_is_read_from_10_55_on() {
    let mut old = header_reader(&[22], 10.54);
    expect_marker(&mut old, 11).unwrap();
    assert!(!consumed(&mut old));
    let mut new = header_reader(&[22], 10.55);
    expect_marker(&mut new, 22).unwrap();
    assert!(consumed(&mut new));
}

#[test]
#[should_panic(expected = "assertion `left == right` failed")]
fn wrong_check_byte_panics() {
    let _ = expect_marker(&mut header_reader(&[33], 10.55), 11);
}

/// The kind, one relation per player, nine diplomacy words, the allied flags and the name.
#[test]
fn player_starts_with_its_relations_and_name() {
    let bytes = Bytes::default()
        .u8(2)
        .raw(&[0, 1, 4])
        .words(&[1, 1, 3, 3, 3, 3, 3, 3, 3])
        .i32(1)
        .u8(0)
        .u16(3)
        .raw(b"Bob")
        .0;
    let mut reader = header_reader(&bytes, 10.0);
    reader.set_num_players(3);
    let player = Player::read_diplomacy_and_name(&mut reader).unwrap();
    assert!(consumed(&mut reader));
    assert_eq!(player.kind, 2);
    assert_eq!(player.relations, [0, 1, 4]);
    assert_eq!(player.diplomacy[2], 3);
    assert!(player.allied_los && !player.allied_victory);
    assert_eq!(player.name(), "Bob");
}

/// From 11.62 on the saved views follow the initial view; a negative count is none.
#[test]
fn the_saved_views_come_from_11_62() {
    let views = |count: i32| {
        Bytes::default()
            .i32(2)
            .floats(&[100.0, 0.5])
            .floats(&[10.0, 20.0])
            .i32(count)
            .floats(&[1.0, 2.0])
    };
    let old = views(1).0;
    let mut reader = header_reader(&old, 10.0);
    let mut player = Player::default();
    player.read_attributes_and_views(&mut reader).unwrap();
    assert_eq!(player.attributes, [100.0, 0.5]);
    assert_eq!(player.initial_view, (10.0, 20.0));
    assert_eq!(player.saved_views, []);

    let new = Bytes::default()
        .u8(22)
        .i32(2)
        .u8(33)
        .floats(&[100.0, 0.5])
        .u8(11);
    let bytes = new.floats(&[10.0, 20.0]).i32(1).floats(&[1.0, 2.0]).0;
    let mut reader = header_reader(&bytes, 11.62);
    player.read_attributes_and_views(&mut reader).unwrap();
    assert!(consumed(&mut reader));
    assert_eq!(player.saved_views, [(1.0, 2.0)]);

    let none = Bytes::default().u8(22).i32(0).u8(33).u8(11);
    let bytes = none.floats(&[10.0, 20.0]).i32(-1).0;
    let mut reader = header_reader(&bytes, 11.62);
    player.read_attributes_and_views(&mut reader).unwrap();
    assert!(consumed(&mut reader));
    assert_eq!(player.saved_views, []);
}

#[test]
fn player_reads_its_civilization_and_status() {
    let bytes = Bytes::default()
        .u16(40)
        .u16(41)
        .raw(&[1, 5, 2, 1])
        .u8(11)
        .u8(3)
        .u8(11)
        .words(&[0, 0])
        .0;
    let mut reader = header_reader(&bytes, 10.55);
    let mut player = Player::default();
    player.read_civilization_and_status(&mut reader).unwrap();
    assert!(consumed(&mut reader));
    assert_eq!(player.spawn_location, (40, 41));
    assert_eq!(player.culture_id, 1);
    assert_eq!(u8::from(player.civilization_id), 5);
    assert_eq!(player.game_status, 2);
    assert!(player.resigned);
}

#[test]
fn selection_reads_its_object_and_ids() {
    let none = read_selection(&mut header_reader(&[0; 4], 11.72)).unwrap();
    assert!(none.is_none());
    let bytes = Bytes::default().words(&[2, 700, 9, 701, 702]).0;
    let mut reader = header_reader(&bytes, 11.72);
    let (object, properties, ids) = read_selection(&mut reader).unwrap().unwrap();
    assert!(consumed(&mut reader));
    assert_eq!((u32::from(object), properties), (700, 9));
    assert_eq!(
        ids.iter().map(|&id| u32::from(id)).collect::<Vec<_>>(),
        [701, 702]
    );
}

#[test]
fn unit_list_ends_at_class_0() {
    let bytes = Bytes::default().words(&[0, 0]).u8(0).0;
    let mut reader = header_reader(&bytes, 11.0);
    assert!(read_unit_list(&mut reader).unwrap().is_empty());
    assert!(consumed(&mut reader));
}

/// Unit types without the available flag read nothing behind their flags.
#[test]
fn unavailable_unit_types_are_none() {
    let bytes = Bytes::default().words(&[2, 0, 0]).0;
    let mut reader = header_reader(&bytes, 10.0);
    let mut player = Player::default();
    player.read_unit_types(&mut reader).unwrap();
    assert!(consumed(&mut reader));
    assert_eq!(player.unit_types.len(), 2);
    assert!(player.unit_types.iter().all(Option::is_none));
}

/// A computer player (kind 3) has a flag for its AI; a human has none, and only Gaia
/// (kind 2) has Gaia's data. Before 9.38 there is no tech tree.
#[test]
fn only_computer_player_has_an_ai_flag() {
    let mut human = Player::default();
    let mut reader = header_reader(&[], 9.0);
    human.read_tech_tree_ai_and_gaia(&mut reader).unwrap();
    assert!(human.tech_tree.is_none() && human.gaia.is_none());
    let mut computer = Player {
        kind: 3,
        ..Player::default()
    };
    let mut reader = header_reader(&[0; 4], 9.0);
    computer.read_tech_tree_ai_and_gaia(&mut reader).unwrap();
    assert!(consumed(&mut reader));
}

#[test]
fn the_object_counts_grow_with_the_categories() {
    assert_eq!(skipped_length(skip_object_counts, 11.50), 3408);
    assert_eq!(skipped_length(skip_object_counts, 11.51), 3808);
    assert_eq!(skipped_length(skip_object_counts, 11.65), 4008);
}

#[test]
fn the_formations_break_by_speed_from_10_81() {
    assert_eq!(skipped_length(skip_formations_and_escrow, 10.80), 72);
    assert_eq!(skipped_length(skip_formations_and_escrow, 10.81), 76);
}

#[test]
fn the_view_ai_and_selection_come_with_their_versions() {
    assert_eq!(skipped_length(skip_view_ai_and_selection, 10.50), 0);
    assert_eq!(skipped_length(skip_view_ai_and_selection, 10.51), 32);
    assert_eq!(skipped_length(skip_view_ai_and_selection, 11.45), 41);
    assert_eq!(skipped_length(skip_view_ai_and_selection, 11.72), 45);
}

#[test]
fn the_attack_data_comes_from_10_02() {
    assert_eq!(skipped_length(skip_updates_and_attacks, 10.01), 17);
    assert_eq!(skipped_length(skip_updates_and_attacks, 10.02), 47);
    let bytes = Bytes::default().raw(&[11, 11]).raw(&[0; 47]).0;
    let mut reader = header_reader(&bytes, 10.55);
    skip_updates_and_attacks(&mut reader).unwrap();
    assert!(consumed(&mut reader));
}

/// Nine players' diplomacy (three bytes and an offer without a message, 20 bytes) and the
/// fealty from 9.13; the trade routes from 9.17 and 9.18.
#[test]
fn the_diplomacy_and_trade_routes_come_with_their_versions() {
    assert_eq!(skipped_length(skip_diplomacy_and_trade_routes, 9.12), 0);
    assert_eq!(skipped_length(skip_diplomacy_and_trade_routes, 9.13), 209);
    assert_eq!(skipped_length(skip_diplomacy_and_trade_routes, 9.17), 229);
    assert_eq!(skipped_length(skip_diplomacy_and_trade_routes, 9.18), 249);
}

#[test]
fn the_market_comes_from_9_22() {
    assert_eq!(skipped_length(skip_market, 9.21), 0);
    assert_eq!(skipped_length(skip_market, 9.22), 44);
}

#[test]
fn the_queues_and_dodging_come_with_their_versions() {
    assert_eq!(skipped_length(skip_queues_and_dodging, 9.66), 0);
    assert_eq!(skipped_length(skip_queues_and_dodging, 9.67), 1);
    assert_eq!(skipped_length(skip_queues_and_dodging, 9.90), 3);
    assert_eq!(skipped_length(skip_queues_and_dodging, 11.42), 7);
    assert_eq!(skipped_length(skip_queues_and_dodging, 11.57), 11);
    assert_eq!(skipped_length(skip_queues_and_dodging, 11.75), 15);
}

#[test]
fn the_old_battle_stats_come_with_their_versions() {
    assert_eq!(skipped_length(skip_old_battle_stats, 9.29), 0);
    assert_eq!(skipped_length(skip_old_battle_stats, 9.30), 24);
    assert_eq!(skipped_length(skip_old_battle_stats, 9.31), 32);
    assert_eq!(skipped_length(skip_old_battle_stats, 9.32), 68);
}
