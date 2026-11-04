//! The tick oracle's per-tick trace (FD-015, `GRTICK1`): every tick's world time and checkpoint
//! lanes from build 158041's own simulation, the reference the engine is compared with tick by tick.

// the dumps' numbers fit the game's widths; a test runs the whole comparison
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::too_many_lines
)]

use flate2::{Compression, write::ZlibEncoder};
use genie_oracle::tick_trace::{TickTrace, bad_trace};
use std::array::from_fn;
use std::io::Write;

fn encode(ticks: &[(u32, Vec<[u32; 11]>)]) -> Vec<u8> {
    let lanes = ticks.first().map_or(0, |tick| tick.1.len());
    let mut rows = Vec::new();
    for (time, ls) in ticks {
        rows.extend_from_slice(&time.to_le_bytes());
        for lane in ls {
            for word in lane {
                rows.extend_from_slice(&word.to_le_bytes());
            }
        }
    }
    let mut z = ZlibEncoder::new(Vec::new(), Compression::default());
    z.write_all(&rows).unwrap();
    let mut out = b"GRTICK1\0".to_vec();
    out.extend_from_slice(&(ticks.len() as u32).to_le_bytes());
    out.extend_from_slice(&(lanes as u32).to_le_bytes());
    out.extend(z.finish().unwrap());
    out
}

#[test]
fn trace_gives_each_ticks_world_time_and_lanes() {
    let lane = |base: u32| from_fn(|index| base + index as u32);
    let ticks = vec![
        (104, vec![lane(0), lane(11)]),
        (208, vec![lane(1), lane(12)]),
    ];
    let trace = TickTrace::decode(&encode(&ticks)).unwrap();
    assert_eq!(trace.len(), 2);
    assert_eq!(trace.world_time(1), 208);
    assert_eq!(trace.lanes(0), &[lane(0), lane(11)]);
}

#[test]
fn file_of_another_kind_is_refused() {
    assert_eq!(
        TickTrace::decode(b"GRTRACE3\0\0\0\0").map_err(|error| error.reason()),
        Err(bad_trace().reason())
    );
}

#[test]
fn the_first_difference_names_the_tick_lane_and_word() {
    let original =
        TickTrace::decode(&encode(&[(104, vec![[1; 11]]), (208, vec![[2; 11]])])).unwrap();
    let mut changed = [2; 11];
    changed[9] = 7;
    let changed_trace =
        TickTrace::decode(&encode(&[(104, vec![[1; 11]]), (208, vec![changed])])).unwrap();
    assert_eq!(original.first_difference(&changed_trace), Some((1, 0, 9)));
    assert_eq!(original.first_difference(&original), None);
}
