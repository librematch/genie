//! The ops recorded after a chapter's world save.
//!
//! Every record starts with a `u32` op id (`SIMULATION.md`, "Record ops"). DE recordings carry no
//! sequence byte after it. The reader dispatches on the op id by hand: binrw's enum derive would try
//! the variants in order and allocate an error for every mismatch.

use crate::de::GenieError;
use crate::de::{truncated, unknown_op};
use binrw::binrw;
use binrw::io::Cursor;
use genie_error::GenieResult;
use std::mem::replace;

/// Op 5 as the first record of a recording.
#[binrw]
#[brw(little)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pregame {
    /// Syncs between two checkpoints (500 in every tier-1 recording).
    pub checkpoint_interval: u32,
    /// aoe2rec's name; 1 in every multiplayer recording seen.
    pub multiplayer: u32,
    /// aoe2rec's name ("rec owner"); not verified against the engine.
    pub recording_player: u32,
    /// Not known yet; 2 in the test data.
    pub unknown3: u32,
    /// aoe2rec calls it "use sequence numbers", but recordings with 1 here carry no sequence byte.
    pub unknown4: u32,
    /// Not known yet.
    pub unknown5: u32,
    /// Not known yet.
    pub unknown6: u32,
}

/// The per-player state checkpoint of an op-2 record: 8 lanes of the 11-word block, then the world
/// time. Lanes follow the lobby slots; unused lanes are zero.
#[binrw]
#[brw(little)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Checkpoint {
    /// The 11-word state block of each of the 8 lanes.
    pub lanes: [[u32; 11]; 8],
    /// The world time (game time, in milliseconds) of the checkpoint.
    pub world_time: u32,
}

/// Op 3: the recording player's camera position.
#[binrw]
#[brw(little)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    /// The camera position, x.
    pub x: f32,
    /// The camera position, y.
    pub y: f32,
    /// The player whose camera it is.
    pub player: i32,
}

/// Op 5 after the first record: the recorded outcome of a trigger condition.
#[binrw]
#[brw(little)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TriggerSync {
    /// The index of the condition within its trigger, or -1.
    pub condition: i32,
    /// The index of the trigger.
    pub trigger: i32,
}

/// One record of a chapter's op stream.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Op<'a> {
    /// Op 5 as the first record of a recording: the recording settings.
    Pregame(Pregame),
    /// Op 1: a command packet, executed in the tick it was recorded in.
    Command {
        /// The command bytes.
        packet: &'a [u8],
        /// The world time (game time, in milliseconds) it was recorded at.
        world_time: u32,
    },
    /// Op 2: the next tick's length in milliseconds.
    Sync {
        /// The tick's length in milliseconds.
        dt: u32,
    },
    /// Op 2 carrying a checkpoint. Boxed: it is 356 bytes and comes once per `checkpoint_interval`
    /// syncs, so every other op stays small and allocation-free.
    Checkpoint {
        /// The tick's length in milliseconds.
        dt: u32,
        /// The checkpoint.
        checkpoint: Box<Checkpoint>,
    },
    /// Op 3: the recording player's camera position.
    View(View),
    /// Op 4: a chat line (JSON text in DE).
    Chat {
        /// The chat channel; -1 in the recordings seen.
        channel: i32,
        /// The text bytes.
        text: &'a [u8],
    },
    /// Op 5 after the first record: the recorded outcome of a trigger condition.
    TriggerSync(TriggerSync),
    /// Op 6: the end of playback. The postgame trailer follows.
    End {
        /// The bytes after the end op, up to the end of the chapter.
        trailer: &'a [u8],
    },
}

/// Iterator over the ops of one chapter. It stops after `End`, at the end of the body, or after the
/// first error.
#[derive(Debug, Clone)]
pub struct OpReader<'a> {
    data: &'a [u8],
    base: usize,
    pos: usize,
    pregame_allowed: bool,
    done: bool,
}

impl<'a> OpReader<'a> {
    /// `base` is the body's file offset, used in error offsets. Only the first chapter's first record
    /// may be a [`Pregame`].
    #[must_use]
    pub const fn new(body: &'a [u8], base: usize, first_chapter: bool) -> Self {
        Self {
            data: body,
            base,
            pos: 0,
            pregame_allowed: first_chapter,
            done: false,
        }
    }
}

impl<'a> OpReader<'a> {
    const fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    /// The next `n` bytes, or `Truncated` at the start of the current record.
    fn take(&mut self, n: usize, record: usize) -> GenieResult<&'a [u8]> {
        if self.remaining() < n {
            return Err(truncated(self.base + record));
        }
        let bytes = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(bytes)
    }

    fn u32(&mut self, record: usize) -> GenieResult<u32> {
        let b = self.take(4, record)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// A fixed-size binrw record of `size` bytes.
    fn fixed<T>(&mut self, size: usize, record: usize) -> GenieResult<T>
    where
        T: for<'b> binrw::BinRead<Args<'b> = ()>,
    {
        let bytes = self.take(size, record)?;
        T::read_le(&mut Cursor::new(bytes)).map_err(|_| truncated(self.base + record))
    }

    fn read_op(&mut self) -> GenieResult<Op<'a>> {
        let record = self.pos;
        let first = replace(&mut self.pregame_allowed, false);
        match self.u32(record)? {
            1 => {
                let len = self.u32(record)? as usize;
                let packet = self.take(len, record)?;
                let world_time = self.u32(record)?;
                Ok(Op::Command { packet, world_time })
            }
            2 => {
                let dt = self.u32(record)?;
                let marker = self.data.get(self.pos..self.pos + 4);
                if marker == Some(&[0, 0, 0, 0][..]) {
                    self.pos += 4;
                    let checkpoint = Box::new(self.fixed(CHECKPOINT_SIZE, record)?);
                    Ok(Op::Checkpoint { dt, checkpoint })
                } else {
                    Ok(Op::Sync { dt })
                }
            }
            3 => Ok(Op::View(self.fixed(12, record)?)),
            4 => {
                let channel = self.u32(record)?.cast_signed();
                let len = self.u32(record)? as usize;
                let text = self.take(len, record)?;
                Ok(Op::Chat { channel, text })
            }
            5 if first => Ok(Op::Pregame(self.fixed(28, record)?)),
            5 => Ok(Op::TriggerSync(self.fixed(8, record)?)),
            6 => {
                let trailer = &self.data[self.pos..];
                self.pos = self.data.len();
                self.done = true;
                Ok(Op::End { trailer })
            }
            op => Err(unknown_op(op, self.base + record)),
        }
    }
}

/// 8 lanes x 11 words + the world time.
const CHECKPOINT_SIZE: usize = (8 * 11 + 1) * 4;

impl<'a> Iterator for OpReader<'a> {
    type Item = GenieResult<Op<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done || self.remaining() == 0 {
            return None;
        }
        let op = self.read_op();
        if op.is_err() {
            self.done = true;
        }
        Some(op)
    }
}

impl Op<'_> {
    /// Write the record back in the recording's encoding.
    // The format stores the packet and text lengths as u32; a record read from a recording always
    // fits.
    ///
    /// # Errors
    ///
    /// A fixed-size record that does not write back.
    #[allow(clippy::cast_possible_truncation)]
    pub fn write(&self, out: &mut Vec<u8>) -> GenieResult<()> {
        let words =
            |out: &mut Vec<u8>, ws: &[u32]| ws.iter().for_each(|w| out.extend(w.to_le_bytes()));
        match self {
            Op::Pregame(pregame) => {
                words(out, &[5]);
                write_fixed(pregame, out)?;
            }
            Op::Command { packet, world_time } => {
                words(out, &[1, packet.len() as u32]);
                out.extend_from_slice(packet);
                words(out, &[*world_time]);
            }
            Op::Sync { dt } => words(out, &[2, *dt]),
            Op::Checkpoint { dt, checkpoint } => {
                words(out, &[2, *dt, 0]);
                write_fixed(checkpoint.as_ref(), out)?;
            }
            Op::View(view) => {
                words(out, &[3]);
                write_fixed(view, out)?;
            }
            Op::Chat { channel, text } => {
                words(out, &[4, channel.cast_unsigned(), text.len() as u32]);
                out.extend_from_slice(text);
            }
            Op::TriggerSync(trigger_sync) => {
                words(out, &[5]);
                write_fixed(trigger_sync, out)?;
            }
            Op::End { trailer } => {
                words(out, &[6]);
                out.extend_from_slice(trailer);
            }
        }
        Ok(())
    }
}

/// Append a fixed-size binrw record.
fn write_fixed<T>(value: &T, out: &mut Vec<u8>) -> GenieResult<()>
where
    T: for<'b> binrw::BinWrite<Args<'b> = ()>,
{
    let mut cursor = Cursor::new(out);
    cursor.set_position(cursor.get_ref().len() as u64);
    value.write_le(&mut cursor).map_err(|error| {
        GenieError::io("a fixed-size record that does not write").with("binrw", error)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: usize = 0x100;

    fn le(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    fn read(body: &[u8], first_chapter: bool) -> Vec<GenieResult<Op<'_>>> {
        OpReader::new(body, BASE, first_chapter).collect()
    }

    fn checkpoint_body() -> (Vec<u8>, Checkpoint) {
        let mut lanes = [[0u32; 11]; 8];
        lanes[1] = [0, 700, 11230, 126, 150, 0, 63, 771_100, 2, 62248, 586_462];
        let mut body = le(&[2, 33, 0]);
        body.extend(lanes.iter().flatten().flat_map(|w| w.to_le_bytes()));
        body.extend(le(&[11298]));
        (
            body,
            Checkpoint {
                lanes,
                world_time: 11298,
            },
        )
    }

    #[test]
    fn op_5_is_pregame_only_as_the_first_record_of_the_first_chapter() {
        let mut body = le(&[5, 500, 1, 1, 2, 0, 0, 0]);
        body.extend(le(&[5, 7, 3]));
        let pregame = Pregame {
            checkpoint_interval: 500,
            multiplayer: 1,
            recording_player: 1,
            unknown3: 2,
            unknown4: 0,
            unknown5: 0,
            unknown6: 0,
        };
        assert_eq!(
            read(&body, true),
            vec![
                Ok(Op::Pregame(pregame)),
                Ok(Op::TriggerSync(TriggerSync {
                    condition: 7,
                    trigger: 3
                }))
            ]
        );
        assert_eq!(
            read(&le(&[5, 7, 3]), false),
            vec![Ok(Op::TriggerSync(TriggerSync {
                condition: 7,
                trigger: 3
            }))]
        );
    }

    #[test]
    fn command_borrows_its_packet() {
        let mut body = le(&[1, 3]);
        body.extend([0x67, 1, 0x10]);
        body.extend(le(&[4242]));
        assert_eq!(
            read(&body, false),
            vec![Ok(Op::Command {
                packet: &[0x67, 1, 0x10],
                world_time: 4242
            })]
        );
    }

    #[test]
    fn sync_followed_by_another_op_is_plain_sync() {
        let mut body = le(&[2, 104, 3]);
        body.extend(le(&[0x431e_0000, 0x41c0_0000, 5]));
        let view = View {
            x: 158.0,
            y: 24.0,
            player: 5,
        };
        assert_eq!(
            read(&body, false),
            vec![Ok(Op::Sync { dt: 104 }), Ok(Op::View(view))]
        );
    }

    #[test]
    fn sync_followed_by_zero_marker_carries_checkpoint() {
        let (body, checkpoint) = checkpoint_body();
        let checkpoint = Box::new(checkpoint);
        assert_eq!(
            read(&body, false),
            vec![Ok(Op::Checkpoint { dt: 33, checkpoint })]
        );
    }

    #[test]
    fn sync_at_the_end_of_the_body_is_plain_sync() {
        assert_eq!(read(&le(&[2, 20]), false), vec![Ok(Op::Sync { dt: 20 })]);
    }

    #[test]
    fn chat_borrows_its_text() {
        let mut body = le(&[4, u32::MAX, 2]);
        body.extend(b"hi");
        assert_eq!(
            read(&body, false),
            vec![Ok(Op::Chat {
                channel: -1,
                text: b"hi"
            })]
        );
    }

    #[test]
    fn end_returns_the_trailer_and_stops() {
        let mut body = le(&[6]);
        body.extend(b"postgame");
        assert_eq!(
            read(&body, false),
            vec![Ok(Op::End {
                trailer: b"postgame"
            })]
        );
    }

    #[test]
    fn truncated_record_reports_its_file_offset_and_stops() {
        let mut body = le(&[2, 20, 1, 10]);
        body.extend([1, 2, 3]);
        let ops = read(&body, false);
        assert_eq!(ops, vec![Ok(Op::Sync { dt: 20 }), Err(truncated(BASE + 8))]);
        let error = ops[1].clone().unwrap_err();
        assert_eq!(
            error.context().collect::<Vec<_>>(),
            [("at", format!("{:#x}", BASE + 8).as_str())]
        );
    }

    #[test]
    fn an_unknown_op_is_an_error() {
        let body = le(&[9]);
        let ops = read(&body, false);
        assert_eq!(ops, vec![Err(unknown_op(9, BASE))]);
        let error = ops[0].clone().unwrap_err();
        assert_eq!(
            error.context().collect::<Vec<_>>(),
            [("op", "9"), ("at", format!("{BASE:#x}").as_str())]
        );
    }

    #[test]
    fn writing_every_op_reproduces_the_body() {
        let mut body = le(&[5, 500, 1, 1, 2, 0, 0, 0, 1, 4, 0x0167, 4242, 2, 104]);
        body.extend(checkpoint_body().0);
        body.extend(le(&[3, 0x431e_0000, 0x41c0_0000, 5, 4, u32::MAX, 2]));
        body.extend(b"hi");
        body.extend(le(&[5, 7, 3, 6]));
        body.extend(b"postgame");
        let mut out = Vec::new();
        for op in OpReader::new(&body, BASE, true) {
            op.unwrap().write(&mut out).unwrap();
        }
        assert_eq!(out, body);
    }
}
