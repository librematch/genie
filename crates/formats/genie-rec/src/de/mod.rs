//! Recorded games of Age of Empires II: Definitive Edition (save 66.x and later).
//!
//! The rest of this crate reads the older formats (`AoK`, `AoC`, HD). This module reads the current
//! one. Its layouts follow the workspace's reverse engineering of the 158041 engine and the
//! [aoe2rec](https://github.com/aoe2ct/aoe2rec) crate and `ImHex` patterns (MIT, see `NOTICE`).
//!
//! A recording is a list of chapters. Each chapter is a compressed world save (the "header", read
//! by genie-save) and the ops recorded after it, up to the next chapter or the end of the file.
//! Everything here borrows from the recording's bytes.

pub mod body;
pub mod command;

pub use body::{Checkpoint, Op, OpReader, Pregame, TriggerSync, View};
pub use genie_save::SaveVersion;

use flate2::read::DeflateDecoder;
pub use genie_error::{ErrorKind, GenieError};

/// The errors of reading a DE recording: [`GenieError`]s.
///
/// Each has its place in the context (`at`, a byte offset into the recording file). A recording that breaks the format is
/// [`ErrorKind::InvalidInput`]: it ends inside a record ([`truncated`]), has an op this reader
/// does not know ([`unknown_op`]), a chapter that points outside it ([`invalid_chapter`]) or a
/// header that does not inflate ([`not_inflating`]); a header that is no world save gives
/// genie-save's error with the chapter's place.
pub type Error = GenieError;

/// The recording ends inside the record at `at`.
#[must_use]
#[track_caller]
pub fn truncated(at: usize) -> Error {
    GenieError::invalid_input("a recording that ends inside a record")
        .with("at", format_args!("{at:#x}"))
}

/// The op at `at` has a type `op` this reader does not know.
#[must_use]
#[track_caller]
pub fn unknown_op(op: u32, at: usize) -> Error {
    GenieError::invalid_input("an op this reader does not know")
        .with("op", op)
        .with("at", format_args!("{at:#x}"))
}

/// The chapter at `at` points outside the recording or backwards.
#[must_use]
#[track_caller]
pub fn invalid_chapter(at: usize) -> Error {
    GenieError::invalid_input("a chapter that points outside the recording")
        .with("at", format_args!("{at:#x}"))
}

/// The header of the chapter at `at` is not valid raw deflate data.
#[must_use]
#[track_caller]
pub fn not_inflating(at: usize) -> Error {
    GenieError::invalid_input("a chapter header that does not inflate")
        .with("at", format_args!("{at:#x}"))
}

/// One chapter: a compressed world save and the ops recorded after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chapter<'a> {
    /// Position in the recording's chapter list; only chapter 0 starts with a [`Pregame`] record.
    pub index: usize,
    /// File offset of the chapter.
    pub offset: usize,
    /// The raw-deflate-compressed world save.
    pub compressed_header: &'a [u8],
    /// File offset of the first op.
    pub body_offset: usize,
    /// The ops, up to the next chapter or the end of the file.
    pub body: &'a [u8],
}

/// Split a recording into its chapters.
///
/// # Errors
///
/// Returns [`invalid_chapter`] if a chapter's offsets point outside the recording, inside its own
/// 8 offset bytes, or back to an earlier chapter.
pub fn chapters(data: &[u8]) -> Result<Vec<Chapter<'_>>, Error> {
    let mut out = Vec::new();
    let mut offset = 0;
    loop {
        let word = |at: usize| {
            data.get(at..at + 4)
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
        };
        let invalid = invalid_chapter(offset);
        let (header_end, next) = word(offset)
            .zip(word(offset + 4))
            .ok_or_else(|| invalid.clone())?;
        let end = if next == 0 { data.len() } else { next };
        if header_end < offset + 8
            || header_end > end
            || end > data.len()
            || (next != 0 && next <= offset)
        {
            return Err(invalid);
        }
        out.push(Chapter {
            index: out.len(),
            offset,
            compressed_header: &data[offset + 8..header_end],
            body_offset: header_end,
            body: &data[header_end..end],
        });
        if next == 0 {
            return Ok(out);
        }
        offset = next;
    }
}

impl<'a> Chapter<'a> {
    /// The ops of this chapter.
    #[must_use]
    pub const fn ops(&self) -> OpReader<'a> {
        OpReader::new(self.body, self.body_offset, self.index == 0)
    }

    /// Inflate the whole header (the chapter's world save).
    ///
    /// # Errors
    ///
    /// Returns [`not_inflating`] if the header is not valid raw deflate data.
    pub fn inflate_header(&self) -> Result<Vec<u8>, Error> {
        use std::io::Read;
        let mut out = Vec::new();
        // The byte count is `out.len()`.
        let _ = DeflateDecoder::new(self.compressed_header)
            .read_to_end(&mut out)
            .map_err(|_| not_inflating(self.offset))?;
        Ok(out)
    }

    /// Inflate the header and read it as a world save.
    ///
    /// # Errors
    ///
    /// Returns [`not_inflating`] if the header does not inflate, and genie-save's error with the
    /// chapter's place (`chapter`) if it cannot be read as a world save.
    pub fn header(&self) -> Result<genie_save::Header, Error> {
        genie_save::parse(&self.inflate_header()?)
            .map_err(|e| e.with("chapter", format_args!("{:#x}", self.offset)))
    }

    /// Inflate the start of the header and read its version fields.
    ///
    /// # Errors
    ///
    /// Returns [`not_inflating`] if the first 20 bytes of the header do not inflate.
    pub fn save_version(&self) -> Result<SaveVersion, Error> {
        use std::io::Read;
        // "VER 9.4\0", the old f32 save version, minor, major, build
        let mut start = [0u8; 20];
        DeflateDecoder::new(self.compressed_header)
            .read_exact(&mut start)
            .map_err(|_| not_inflating(self.offset))?;
        Ok(SaveVersion {
            minor: u16::from_le_bytes([start[12], start[13]]),
            major: u16::from_le_bytes([start[14], start[15]]),
            build: u32::from_le_bytes([start[16], start[17], start[18], start[19]]),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(test)]
    use flate2::write::DeflateEncoder;
    use std::io::Write;

    fn deflate(bytes: &[u8]) -> Vec<u8> {
        let mut e = DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
        e.write_all(bytes).unwrap();
        e.finish().unwrap()
    }

    fn le(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    /// A recording with the given chapters, each a (compressed header, body) pair.
    fn recording(parts: &[(&[u8], &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        for (i, (header, body)) in parts.iter().enumerate() {
            let start = out.len();
            let header_end = start + 8 + header.len();
            let next = if i + 1 < parts.len() {
                header_end + body.len()
            } else {
                0
            };
            out.extend(le(&[
                u32::try_from(header_end).unwrap(),
                u32::try_from(next).unwrap(),
            ]));
            out.extend_from_slice(header);
            out.extend_from_slice(body);
        }
        out
    }

    #[test]
    fn chapters_split_at_the_next_chapter_offsets() {
        let data = recording(&[(b"HDR0", b"body-zero"), (b"HDR-1", b"body1")]);
        let ch = chapters(&data).unwrap();
        assert_eq!(ch.len(), 2);
        assert_eq!(
            (
                ch[0].index,
                ch[0].offset,
                ch[0].compressed_header,
                ch[0].body
            ),
            (0, 0, &b"HDR0"[..], &b"body-zero"[..])
        );
        assert_eq!(
            (ch[1].index, ch[1].compressed_header, ch[1].body),
            (1, &b"HDR-1"[..], &b"body1"[..])
        );
        assert_eq!(ch[1].body_offset, ch[1].offset + 8 + 5);
    }

    #[test]
    fn chapter_pointing_outside_the_file_is_an_error() {
        let mut data = recording(&[(b"HDR0", b"body")]);
        data[0..4].copy_from_slice(&999u32.to_le_bytes());
        let e = chapters(&data).unwrap_err();
        assert_eq!(e, invalid_chapter(0));
        assert_eq!(e.context().collect::<Vec<_>>(), [("at", "0x0")]);
    }

    #[test]
    fn save_version_reads_the_inflated_header_start() {
        let mut header = b"VER 9.4\0".to_vec();
        header.extend((-1.0f32).to_le_bytes());
        header.extend(39321u16.to_le_bytes());
        header.extend(66u16.to_le_bytes());
        header.extend(158_041_u32.to_le_bytes());
        header.extend([0u8; 64]);
        let data = recording(&[(&deflate(&header), b"")]);
        let v = chapters(&data).unwrap()[0].save_version().unwrap();
        assert_eq!(
            (v.major, v.minor, v.build, v.hundredths()),
            (66, 39321, 158_041, 6660)
        );
    }

    #[test]
    fn header_that_does_not_inflate_is_an_error() {
        let data = recording(&[(&[0xff; 16], b"")]);
        let e = chapters(&data).unwrap()[0].save_version().unwrap_err();
        assert_eq!(e, not_inflating(0));
        assert_eq!(e.context().collect::<Vec<_>>(), [("at", "0x0")]);
    }
}
