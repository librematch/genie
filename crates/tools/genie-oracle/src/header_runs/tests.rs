use crate::Error;
use crate::fixtures::{gaia, header, human, recording};
use crate::header_runs::{blocks_per_tick, lane_block};
use crate::setup::header_slice;
use genie_rec::de::Op;

/// Each tick's blocks of the players asked for, in their order; a player the save does not hold
/// is an error.
#[test]
fn each_tick_gives_the_players_blocks() {
    let save = header(vec![gaia(Vec::new()), human(Vec::new())]);
    let start = header_slice(&save, None).unwrap();
    let block = |p: usize| {
        lane_block(
            &save.players()[p],
            p as u32,
            &start.players[p],
            &start.parts,
        )
    };
    let rec = recording(&save, &[Op::Sync { dt: 50 }, Op::Sync { dt: 50 }]);
    let blocks = blocks_per_tick(&rec, &[1, 0], 5, None).unwrap();
    assert_eq!(blocks, [[block(1), block(0)], [block(1), block(0)]]);
    assert_eq!(blocks_per_tick(&rec, &[1], 1, None).unwrap().len(), 1);
    assert!(matches!(
        blocks_per_tick(&rec, &[2], 1, None),
        Err(Error::NoSuchPlayer(2))
    ));
}
