//! What the unit tests share: a header reader over bytes and the write-read-write check.

use crate::element::{ReadHeader, WriteHeader};
use crate::reader::RecordingHeaderReader;
use genie_error::GenieResult;
use std::io::Read;

/// A section of the header a reader skips, at its version.
pub type Section = fn(&mut RecordingHeaderReader<&'static [u8]>) -> GenieResult<()>;

/// What a skipped section reads from.
static ZEROS: [u8; 8192] = [0; 8192];

/// The bytes a skipped section takes at `version`: it reads that many zeros and no more.
pub fn skipped_length(skip: Section, version: f32) -> usize {
    let mut reader = header_reader(&ZEROS, version);
    skip(&mut reader).unwrap();
    let left = reader.read_to_end(&mut vec![]).unwrap();
    ZEROS.len() - left
}

/// A header reader over `bytes` at save `version` (game version `VER 9.4`).
pub fn header_reader(bytes: &[u8], version: f32) -> RecordingHeaderReader<&[u8]> {
    let mut reader = RecordingHeaderReader::new(bytes);
    reader.set_version(b"VER 9.4", version);
    reader
}

/// Whether `reader` has no bytes left.
pub fn consumed(reader: &mut RecordingHeaderReader<&[u8]>) -> bool {
    let mut rest = vec![];
    reader.read_to_end(&mut rest).unwrap() == 0
}

/// `value` written, read back at `version` after `setup` (a map size, a player count) and
/// written again: the same bytes, all of them read. Gives the value read.
pub fn writes_back_with<T: ReadHeader + WriteHeader>(
    value: &T,
    version: f32,
    setup: impl FnOnce(&mut RecordingHeaderReader<&[u8]>),
) -> T {
    let mut bytes = vec![];
    value.write_to(&mut bytes).unwrap();
    let mut reader = header_reader(&bytes, version);
    setup(&mut reader);
    let read = <T as ReadHeader>::read_from(&mut reader).unwrap();
    assert!(consumed(&mut reader), "bytes left after the read");
    let mut again = vec![];
    read.write_to(&mut again).unwrap();
    assert_eq!(again, bytes);
    read
}

/// [`writes_back_with`] without a setup.
pub fn writes_back<T: ReadHeader + WriteHeader>(value: &T, version: f32) -> T {
    writes_back_with(value, version, |_| {})
}

/// The element of type `T` read from `bytes` at `version`, all of them read.
pub fn read_all<T: ReadHeader>(bytes: &[u8], version: f32) -> T {
    let mut reader = header_reader(bytes, version);
    let read = <T as ReadHeader>::read_from(&mut reader).unwrap();
    assert!(consumed(&mut reader), "bytes left after the read");
    read
}

/// Little-endian words.
pub fn words(ws: &[i32]) -> Vec<u8> {
    ws.iter().flat_map(|word| word.to_le_bytes()).collect()
}

/// Little-endian floats.
pub fn floats(fs: &[f32]) -> Vec<u8> {
    fs.iter().flat_map(|float| float.to_le_bytes()).collect()
}

/// Little-endian bytes built field by field, in a reader's order.
#[derive(Debug, Default, Clone)]
pub struct Bytes(pub Vec<u8>);

impl Bytes {
    /// A byte.
    pub fn u8(mut self, value: u8) -> Self {
        self.0.push(value);
        self
    }

    /// A 16-bit value.
    pub fn u16(mut self, value: u16) -> Self {
        self.0.extend(value.to_le_bytes());
        self
    }

    /// A 32-bit word; -1 is the format's "none".
    pub fn i32(mut self, value: i32) -> Self {
        self.0.extend(value.to_le_bytes());
        self
    }

    /// 32-bit words.
    pub fn words(mut self, values: &[i32]) -> Self {
        self.0.extend(words(values));
        self
    }

    /// Floats.
    pub fn floats(mut self, values: &[f32]) -> Self {
        self.0.extend(floats(values));
        self
    }

    /// Bytes as they are.
    pub fn raw(mut self, bytes: &[u8]) -> Self {
        self.0.extend(bytes);
        self
    }
}
