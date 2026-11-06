//! Provides an Age of Empires series recorded game file reader.
//!
//! ## Version Support
//! This crate can read Age of Empires 1, Age of Empires 2: The Conquerors, and HD Edition recorded game files.
//!
//! ## Credits
//! Most of the `.mgl`, `.mgx`, `.mgz` format specification was taken from Bari's classic [mgx
//! format description][], the [recage][] Node.js library, and Happyleaves' [aoc-mgz][] Python library.
//!
//! [mgx format description]: https://web.archive.org/web/20090215065209/http://members.at.infoseek.co.jp/aocai/mgx_format.html
//! [recage]: https://github.com/genie-js/recage
//! [aoc-mgz]: https://github.com/happyleavesaoc/aoc-mgz

#![deny(future_incompatible)]
#![deny(nonstandard_style)]
#![deny(rust_2018_idioms)]
#![deny(unsafe_code)]
#![warn(unused)]
#![warn(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::unimplemented,
    clippy::todo
)]

pub mod actions;
pub mod ai;
pub mod de;
/// The trait shared by the header's parts.
pub mod element;
mod errors;
/// The recorded-game header (pre-DE layout).
pub mod header;
/// Readers that track the header's versions while reading.
pub mod header_reader;
/// The map: tiles, zones, visibility and obstruction data.
pub mod map;
/// The players' state in the header.
pub mod player;
/// The string table in the header.
pub mod string_table;
/// Units (game objects) in the header.
pub mod unit;
/// Unit actions (their current tasks) in the header.
pub mod unit_action;
/// Unit types (the per-player copies of the data file's units) in the header.
pub mod unit_kind;
/// Game variant and version detection.
pub mod version;

#[cfg(test)]
mod test_support;

use crate::actions::{Action, Meta};
use crate::element::ReadHeader;
use crate::errors::{early_sync, missing_sync, unknown_action};
use crate::header_reader::{RecordingHeaderReader, SmallBufReader};
use byteorder::{LE, ReadBytesExt};
use flate2::bufread::DeflateDecoder;
use genie_error::GenieResult;
use genie_support::{
    FIELD_DOES_NOT_FIT, fallible_try_from, fallible_try_into, infallible_try_into,
};
pub use header::Header;
use std::fmt::Debug;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};
pub use version::*;

/// ID identifying a player (0-8).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PlayerId(u8);

impl PlayerId {
    /// Player ID for GAIA, the "nature" player.
    pub const GAIA: Self = Self(0);
}

impl From<u8> for PlayerId {
    #[inline]
    fn from(number: u8) -> Self {
        Self(number)
    }
}

impl From<PlayerId> for u8 {
    #[inline]
    fn from(player_id: PlayerId) -> Self {
        player_id.0
    }
}

fallible_try_from!(PlayerId, i32);
fallible_try_from!(PlayerId, u32);
fallible_try_from!(PlayerId, i16);
fallible_try_from!(PlayerId, u16);
fallible_try_from!(PlayerId, i8);
infallible_try_into!(PlayerId, i16);
infallible_try_into!(PlayerId, u16);
infallible_try_into!(PlayerId, i32);
infallible_try_into!(PlayerId, u32);

/// ID identifying a game object (unit, building, resource …).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ObjectId(u32);

impl From<u32> for ObjectId {
    #[inline]
    fn from(number: u32) -> Self {
        Self(number)
    }
}

impl From<u16> for ObjectId {
    #[inline]
    fn from(number: u16) -> Self {
        Self(number.into())
    }
}

impl From<ObjectId> for u32 {
    #[inline]
    fn from(object_id: ObjectId) -> Self {
        object_id.0
    }
}

fallible_try_from!(ObjectId, i16);
fallible_try_from!(ObjectId, i32);
fallible_try_into!(ObjectId, i16);
fallible_try_into!(ObjectId, i32);

/// `value` as the type of the field it is read from or written to.
///
/// # Errors
///
/// Invalid input when it does not fit.
pub(crate) fn fits<T, U: TryInto<T>>(value: U) -> GenieResult<T> {
    value.try_into().map_err(|_| FIELD_DOES_NOT_FIT)
}

/// The tiles of a map `width` by `height`.
///
/// # Errors
///
/// Invalid input when they do not fit the address space.
pub(crate) fn area(width: u32, height: u32) -> GenieResult<usize> {
    fits(u64::from(width) * u64::from(height))
}

/// An optional id as its field: `none` for no id.
///
/// # Errors
///
/// Invalid input when the id does not fit.
pub(crate) fn opt_fits<T, U: TryInto<T>>(value: Option<U>, none: T) -> GenieResult<T> {
    value.map_or(Ok(none), fits)
}

/// Iterator over body actions.
#[derive(Debug)]
pub struct BodyActions<R>
where
    R: BufRead,
{
    input: R,
    meta: Meta,
    remaining_syncs_until_checksum: u32,
}

impl<R> BodyActions<R>
where
    R: BufRead,
{
    /// Read the body's meta action and start iterating over the actions after it.
    ///
    /// `data_version` is the header's save version: from 11.76 on, the meta action has the mgx
    /// layout, before it the mgl one.
    ///
    /// # Errors
    ///
    /// Returns an error if the meta action cannot be read.
    pub fn new(mut input: R, data_version: f32) -> GenieResult<Self> {
        let meta = if data_version >= 11.76 {
            Meta::read_from_mgx(&mut input)?
        } else {
            Meta::read_from_mgl(&mut input)?
        };
        let remaining_syncs_until_checksum = meta.checksum_interval;
        Ok(Self {
            input,
            meta,
            remaining_syncs_until_checksum,
        })
    }
}

impl<R> Iterator for BodyActions<R>
where
    R: BufRead,
{
    type Item = GenieResult<Action>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.meta.use_sequence_numbers {
            let _sequence = match self.input.read_u8() {
                Ok(byte) => byte,
                Err(err) => return Some(Err(err.into())),
            };
        }
        match self.input.read_i32::<LE>() {
            Ok(0x00) => {
                if self.remaining_syncs_until_checksum == 0 {
                    self.remaining_syncs_until_checksum = self.meta.checksum_interval;
                    Some(actions::Sync::read_from(&mut self.input).map(Action::Sync))
                } else {
                    Some(Err(early_sync(self.remaining_syncs_until_checksum)))
                }
            }
            Ok(0x01) => Some(actions::Command::read_from(&mut self.input).map(Action::Command)),
            Ok(0x02) => {
                match self.remaining_syncs_until_checksum.checked_sub(1) {
                    Some(syncs) => self.remaining_syncs_until_checksum = syncs,
                    None => return Some(Err(missing_sync())),
                }
                Some(actions::Time::read_from(&mut self.input).map(Action::Time))
            }
            Ok(0x03) => Some(actions::ViewLock::read_from(&mut self.input).map(Action::ViewLock)),
            Ok(0x04) => Some(actions::Chat::read_from(&mut self.input).map(Action::Chat)),
            Ok(id) => Some(Err(unknown_action(id))),
            Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => None,
            Err(err) => Some(Err(err.into())),
        }
    }
}

/// Recorded game reader.
#[derive(Debug)]
pub struct RecordedGame<R>
where
    R: Read + Seek,
{
    inner: R,
    /// Offset of the main compressed header.
    header_start: u64,
    /// Size of the compressed header.
    header_end: u64,
    /// Offset of the next header, for saved chapters.
    #[allow(unused)]
    next_header: Option<u64>,
    #[allow(unused)]
    game_version: GameVersion,
    save_version: f32,
}

impl<R> RecordedGame<R>
where
    R: Read + Seek,
{
    /// Read the file's offsets and versions; the header and body are read on demand.
    ///
    /// # Errors
    ///
    /// Returns an error if seeking or reading the first bytes fails.
    pub fn new(mut input: R) -> GenieResult<Self> {
        let file_size = {
            let size = input.seek(SeekFrom::End(0))?;
            // Rewinding to the start: the new position is known.
            let _ = input.seek(SeekFrom::Start(0))?;
            size
        };

        let header_end = u64::from(input.read_u32::<LE>()?);
        let next_header = u64::from(input.read_u32::<LE>()?);

        let header_start = if next_header > file_size { 4 } else { 8 };

        let next_header = if next_header > 0 && next_header < file_size {
            Some(next_header)
        } else {
            None
        };

        let (game_version, save_version) = {
            let _ = input.seek(SeekFrom::Start(header_start))?;
            let version_reader = SmallBufReader::new(&mut input);
            let mut deflate = DeflateDecoder::new(version_reader);
            let game_version = GameVersion::read_from(&mut deflate)?;
            let save_version = deflate.read_f32::<LE>()?;
            (game_version, save_version)
        };

        Ok(Self {
            inner: input,
            header_start,
            header_end,
            next_header,
            game_version,
            save_version,
        })
    }

    fn seek_to_first_header(&mut self) -> GenieResult<()> {
        let _ = self.inner.seek(SeekFrom::Start(self.header_start))?;

        Ok(())
    }

    fn seek_to_body(&mut self) -> GenieResult<()> {
        let _ = self.inner.seek(SeekFrom::Start(self.header_end))?;

        Ok(())
    }

    /// Inflate and read the header.
    ///
    /// # Errors
    ///
    /// Returns an error if the header cannot be read or decoded.
    pub fn header(&mut self) -> GenieResult<Header> {
        self.seek_to_first_header()?;
        let reader = BufReader::new(&mut self.inner).take(self.header_end - self.header_start);
        let deflate = DeflateDecoder::new(reader);
        let header = Header::read_from(&mut RecordingHeaderReader::new(deflate))?;
        Ok(header)
    }

    /// Iterate over the body's actions.
    ///
    /// # Errors
    ///
    /// Returns an error if seeking to the body or reading its meta action fails.
    pub fn actions(&mut self) -> GenieResult<BodyActions<BufReader<&mut R>>> {
        self.seek_to_body()?;
        BodyActions::new(BufReader::new(&mut self.inner), self.save_version)
    }

    /// Give back the underlying reader.
    pub fn into_inner(self) -> R {
        self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Cursor;

    /// A body of the mgx layout whose meta action announces a sync every `interval` time
    /// steps, then `ops`.
    fn body(interval: u32, ops: &[u32]) -> Vec<u8> {
        [4, interval, 0, 1, 0, 0, 0]
            .iter()
            .chain(ops)
            .flat_map(|word| word.to_le_bytes())
            .collect()
    }

    /// A sync before its interval, a time step where a sync is due and an action type the
    /// reader does not know end the actions with their errors; the input's end ends them.
    #[test]
    fn body_actions_out_of_step_fail() {
        let reasons = |interval, ops: &[u32]| -> Vec<Result<(), &'static str>> {
            BodyActions::new(Cursor::new(body(interval, ops)), 12.0)
                .unwrap()
                .map(|action| action.map(|_| ()).map_err(|error| error.reason()))
                .collect()
        };
        assert_eq!(
            reasons(2, &[2, 100, 0]),
            [
                Ok(()),
                Err("a sync message earlier than the log header said")
            ]
        );
        assert_eq!(
            reasons(0, &[2]),
            [Err("a time message where a sync was due")]
        );
        assert_eq!(
            reasons(2, &[9]),
            [Err("an action type this reader does not know")]
        );
        assert_eq!(
            early_sync(1).context().collect::<Vec<_>>(),
            [("ticks", "1")]
        );
        assert_eq!(
            unknown_action(9).context().collect::<Vec<_>>(),
            [("action", "0x9")]
        );
    }

    #[test]
    // AI data parsing is incomplete: remove this attribute when the test starts passing
    #[should_panic = "assertion"] // matches the pre- and post-1.73 assert_eq! panic text
    fn incomplete_up_15_rec_with_ai() {
        let file = File::open("test/rec.20181208-195117.mgz").unwrap();
        let mut recorded_game = RecordedGame::new(file).unwrap();
        let _ = recorded_game
            .header()
            .expect("AI data cannot be fully parsed");
        for act in recorded_game.actions().unwrap() {
            let _act = act.unwrap();
        }
    }

    #[test]
    fn aoc_1_0_rec() -> GenieResult<()> {
        let file = File::open("test/missyou_finally_vs_11.mgx")?;
        let mut recorded_game = RecordedGame::new(file)?;
        let _ = recorded_game.header()?;
        for act in recorded_game.actions()? {
            match act {
                Ok(act) => println!("{act:?}"),
                Err(error) if error.reason() == "a string that does not decode as WINDOWS-1252" => {
                    // Skip invalid utf8 chat for now
                }
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    #[test]
    fn aok_rec() -> GenieResult<()> {
        let file = File::open("test/aok.mgl")?;
        let mut recorded_game = RecordedGame::new(file)?;
        let _ = recorded_game.header()?;
        for act in recorded_game.actions()? {
            println!("{:?}", act?);
        }
        Ok(())
    }
}
