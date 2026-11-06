use super::*;

/// A version-2 header, the first version that holds a timestamp, stamped at the epoch.
fn header_at_epoch() -> ScxHeader {
    ScxHeader {
        version: 2,
        timestamp: 0,
        description: Some("a scenario".to_owned()),
        author_name: None,
        any_sp_victory: false,
        active_player_count: 2,
        dlc_options: None,
    }
}

/// The timestamp that the header's bytes hold once written and read back.
fn written_seconds(header: &ScxHeader) -> u32 {
    let mut bytes = vec![];
    header
        .write_to(&mut bytes, ScxVersion::default(), header.version)
        .unwrap();
    ScxHeader::read_from(&bytes[..], ScxVersion::default())
        .unwrap()
        .timestamp
}

#[test]
fn touch_writes_the_whole_seconds_of_the_time_it_is_given() {
    let mut header = header_at_epoch();
    header.touch(Duration::from_millis(1_700_000_000_750));
    assert_eq!(header.timestamp, 1_700_000_000);
    assert_eq!(written_seconds(&header), 1_700_000_000);
}

#[test]
fn touch_keeps_the_low_32_bits_of_seconds_past_2106() {
    let mut header = header_at_epoch();
    header.touch(Duration::from_secs((1 << 32) + 5));
    assert_eq!(written_seconds(&header), 5);
}
