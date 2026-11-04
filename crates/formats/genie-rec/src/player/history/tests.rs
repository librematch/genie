use crate::element::ReadHeader;
use crate::player::history::{DiplomacyOffer, HistoryEvent, HistoryInfo};
use crate::test_support::header_reader;
use crate::test_support::{Bytes, read_all};

/// An event: its type, the time slice, the world time and three parameters.
fn event() -> Bytes {
    Bytes::default()
        .u8(3)
        .words(&[12, 3400])
        .floats(&[1.0, 2.0, 3.0])
}

#[test]
fn an_event_reads_its_times_and_parameters() {
    let event = read_all::<HistoryEvent>(&event().0, 11.97);
    assert_eq!(event.event_kind, 3);
    assert_eq!((event.time_slice, event.world_time), (12, 3400));
    assert_eq!(event.params, (1.0, 2.0, 3.0));
}

/// Two entries and one event, then 180 bytes of battle statistics the reader skips.
#[test]
fn history_reads_its_entries_and_events_and_skips_the_statistics() {
    let bytes = Bytes::default()
        .u8(0)
        .words(&[2, 1, 10])
        .u16(3)
        .u16(1)
        .u16(5)
        .u16(2)
        .u8(0)
        .i32(1)
        .raw(&event().0)
        .raw(&[0; 180])
        .u8(0)
        .0;
    let history = read_all::<HistoryInfo>(&bytes, 11.97);
    assert_eq!(
        history
            .entries
            .iter()
            .map(|entry| (entry.civilian_population, entry.military_population))
            .collect::<Vec<_>>(),
        [(3, 1), (5, 2)]
    );
    assert_eq!(history.events.len(), 1);
    assert_eq!(history.events[0].world_time, 3400);
}

#[test]
fn history_short_of_its_statistics_is_an_error() {
    let bytes = Bytes::default()
        .u8(0)
        .words(&[0, 0, 0])
        .u8(0)
        .i32(0)
        .raw(&[0; 179])
        .0;
    let mut reader = header_reader(&bytes, 11.97);
    let read = <HistoryInfo as ReadHeader>::read_from(&mut reader);
    assert!(read.is_err());
}

#[test]
fn diplomacy_offer_reads_its_terms_and_message() {
    let bytes = Bytes::default()
        .raw(&[1, 2])
        .i32(500)
        .raw(&[1, 0, 1, 0, 1, 0, 1, 0])
        .i32(100)
        .u8(3)
        .raw(b"hi\0")
        .u8(4)
        .0;
    let offer = read_all::<DiplomacyOffer>(&bytes, 11.97);
    assert_eq!(
        (offer.sequence, offer.started_by, offer.game_time),
        (1, 2, 500)
    );
    assert_eq!((offer.new_diplomacy, offer.gold), (1, 100));
    assert_eq!(offer.message.as_deref(), Some("hi"));
    assert_eq!(offer.status, 4);
}
