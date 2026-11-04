//! The tick oracle's per-tick trace (FD-015): `GRTICK1`, a `u32` tick count, a `u32` lane count,
//! then zlib of one row per tick, the world time and the lanes' 11 checkpoint words (all `u32`
//! little endian). Build 158041's own simulation writes it while it plays a recording
//! (`aoe2de-exe-re/load_oracle/playback.py --trace`); a row holds the state after its tick.

use flate2::read::ZlibDecoder;
use genie_core::{GenieError, GenieResult};
use std::io::Read;

const MAGIC: &[u8; 8] = b"GRTICK1\0";

/// Bytes that are not a trace (no magic, or a header cut short).
#[must_use]
#[track_caller]
pub const fn bad_trace() -> GenieError {
    GenieError::invalid_input("bytes that are not a GRTICK1 trace")
}

/// Rows that end before the last tick the header announces.
#[must_use]
#[track_caller]
pub const fn truncated_trace() -> GenieError {
    GenieError::invalid_input("a trace whose rows end before the last tick")
}

/// Every tick's world time and checkpoint lanes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TickTrace {
    times: Vec<u32>,
    lanes: Vec<Vec<[u32; 11]>>,
}

/// The little-endian word of `bytes` at `at`.
fn word_at(bytes: &[u8], at: usize) -> Option<u32> {
    let word = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([word[0], word[1], word[2], word[3]]))
}

/// The eleven words of a lane from `at`.
fn lane_words(word: &impl Fn(usize) -> GenieResult<u32>, at: usize) -> GenieResult<[u32; 11]> {
    let mut lane = [0; 11];
    for (index, slot) in lane.iter_mut().enumerate() {
        *slot = word(at + 4 * index)?;
    }
    Ok(lane)
}

impl TickTrace {
    /// A trace from its file's bytes: the magic, the tick and lane counts, then each tick's row
    /// of eleven words per lane.
    ///
    /// # Errors
    ///
    /// Invalid input when the bytes are not a trace or end early, an input or output error when
    /// the rows do not inflate.
    pub fn decode(data: &[u8]) -> GenieResult<Self> {
        if data.get(..8) != Some(MAGIC.as_slice()) {
            return Err(bad_trace());
        }
        let ticks = word_at(data, 8).ok_or(bad_trace())? as usize;
        let lane_count = word_at(data, 12).ok_or(bad_trace())? as usize;
        let mut rows = Vec::new();
        let _ = ZlibDecoder::new(data.get(16..).ok_or(bad_trace())?)
            .read_to_end(&mut rows)
            .map_err(|error| {
                GenieError::io("a trace whose rows do not inflate").with_source(error)
            })?;
        let row = 4 * (1 + 11 * lane_count);
        if rows.len() < ticks * row {
            return Err(truncated_trace());
        }
        let word = |at: usize| word_at(&rows, at).ok_or(truncated_trace());
        let mut times = Vec::with_capacity(ticks);
        let mut lanes = Vec::with_capacity(ticks);
        for index in 0..ticks {
            let base = index * row;
            times.push(word(base)?);
            lanes.push(
                (0..lane_count)
                    .map(|lane| lane_words(&word, base + 4 + 4 * 11 * lane))
                    .collect::<Result<_, _>>()?,
            );
        }
        Ok(Self { times, lanes })
    }

    /// How many ticks the trace holds.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.times.len()
    }

    /// The trace holds no tick.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.times.is_empty()
    }

    /// The world time after tick `tick` (0-based), in milliseconds.
    #[must_use]
    pub fn world_time(&self, tick: usize) -> u32 {
        self.times[tick]
    }

    /// The lanes after tick `tick`, in lobby order.
    #[must_use]
    pub fn lanes(&self, tick: usize) -> &[[u32; 11]] {
        &self.lanes[tick]
    }

    /// The first (tick, lane, word) where two traces differ, over the ticks both hold.
    #[must_use]
    pub fn first_difference(&self, other: &Self) -> Option<(usize, usize, usize)> {
        self.lanes
            .iter()
            .zip(&other.lanes)
            .enumerate()
            .find_map(|(tick, (ours, theirs))| {
                ours.iter()
                    .zip(theirs)
                    .enumerate()
                    .find_map(|(lane, (x, y))| {
                        (0..11)
                            .find(|&word| x[word] != y[word])
                            .map(|word| (tick, lane, word))
                    })
            })
    }
}
