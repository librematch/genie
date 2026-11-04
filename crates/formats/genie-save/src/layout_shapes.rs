//! The value shapes of the generated layouts (`generated/`): what a read helper of the game reads.
//! Hand-written; the generator names them in its helper table (aoe2de-exe-re `layout/b<build>.py`).
//! Each writes back the bytes it read: a length is the length of what follows.

// binrw's expansion of the lengths' `try_calc` casts and qualifies; the code is the macro's
#![allow(trivial_casts, unused_qualifications)]

use binrw::io::{Read, Seek, Write};
use binrw::{BinRead, BinResult, BinWrite, Endian, binrw};
use genie_error::GenieError;
use std::ops::Deref;

/// A u16 length, then that many bytes (158041: `0x140933990`, `SRead<short> &requiredBufSize`).
#[binrw]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[brw(little)]
pub struct LenString16 {
    #[br(temp)]
    #[bw(try_calc = u16::try_from(bytes.len()))]
    len: u16,
    /// The bytes.
    #[br(count = len)]
    pub bytes: Vec<u8>,
}

/// A u32 length, then that many bytes (158041: `0x140932fa0`, `SRead<unsigned int> &requiredBufSize`).
#[binrw]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[brw(little)]
pub struct LenString32 {
    #[br(temp)]
    #[bw(try_calc = u32::try_from(bytes.len()))]
    len: u32,
    /// The bytes.
    #[br(count = len)]
    pub bytes: Vec<u8>,
}

/// A string behind the signature `0x0a60` (158041: `0x140932010`, `strings.cpp`).
///
/// Two u16, the signature `&theSig` and the length `&theLen`, then that many bytes. The game takes
/// the pair in either order: when the first is not the signature but the second is, the first is
/// the length.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SigString {
    /// The length came first and the signature second.
    pub swapped: bool,
    /// The bytes.
    pub bytes: Vec<u8>,
}

/// The signature of a [`SigString`].
pub const STRING_SIGNATURE: u16 = 0x0a60;

impl BinRead for SigString {
    type Args<'a> = ();

    fn read_options<R: Read + Seek>(reader: &mut R, endian: Endian, (): ()) -> BinResult<Self> {
        let pos = reader.stream_position()?;
        let first = u16::read_options(reader, endian, ())?;
        let second = u16::read_options(reader, endian, ())?;
        let (swapped, len) = if first == STRING_SIGNATURE {
            (false, second)
        } else if second == STRING_SIGNATURE {
            (true, first)
        } else {
            return Err(binrw::Error::BadMagic {
                pos,
                found: Box::new([first, second]),
            });
        };
        let args = binrw::VecArgs::builder().count(len as usize).finalize();
        Ok(Self {
            swapped,
            bytes: Vec::<u8>::read_options(reader, endian, args)?,
        })
    }
}

impl BinWrite for SigString {
    type Args<'a> = ();

    fn write_options<W: Write + Seek>(
        &self,
        writer: &mut W,
        endian: Endian,
        (): (),
    ) -> BinResult<()> {
        let len =
            u16::try_from(self.bytes.len()).map_err(|try_from_int_error| binrw::Error::Custom {
                pos: writer.stream_position().unwrap_or_default(),
                err: Box::new(try_from_int_error),
            })?;
        let pair = if self.swapped {
            [len, STRING_SIGNATURE]
        } else {
            [STRING_SIGNATURE, len]
        };
        pair.write_options(writer, endian, ())?;
        self.bytes.write_options(writer, endian, ())
    }
}

/// A u32 count, then that many values (158041: `0x140d46ce0`, a `vector<int64_t>`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LenVec32<T>(pub Vec<T>);

impl<T> BinRead for LenVec32<T>
where
    T: for<'a> BinRead<Args<'a> = ()> + 'static,
{
    type Args<'a> = ();

    fn read_options<R: Read + Seek>(reader: &mut R, endian: Endian, (): ()) -> BinResult<Self> {
        let count = u32::read_options(reader, endian, ())?;
        let args = binrw::VecArgs::builder().count(count as usize).finalize();
        Ok(Self(Vec::<T>::read_options(reader, endian, args)?))
    }
}

impl<T> BinWrite for LenVec32<T>
where
    T: for<'a> BinWrite<Args<'a> = ()> + 'static,
{
    type Args<'a> = ();

    fn write_options<W: Write + Seek>(
        &self,
        writer: &mut W,
        endian: Endian,
        (): (),
    ) -> BinResult<()> {
        let count =
            u32::try_from(self.0.len()).map_err(|try_from_int_error| binrw::Error::Custom {
                pos: writer.stream_position().unwrap_or_default(),
                err: Box::new(try_from_int_error),
            })?;
        count.write_options(writer, endian, ())?;
        self.0.write_options(writer, endian, ())
    }
}

/// The part a read error names as not read yet ([`Unsupported`]) anywhere in it, with its
/// position.
#[must_use]
pub fn unsupported_in(error: &binrw::Error) -> Option<(GenieError, usize)> {
    let root = error.root_cause();
    let unsupported = root.custom_err::<GenieError>()?.clone();
    let at = match root {
        binrw::Error::Custom { pos, .. } => *pos as usize,
        _ => 0,
    };
    Some((unsupported, at))
}

/// A part of the layout the generator does not derive yet (aoe2de-exe-re `layout`, helper kind
/// `unsupported`).
///
/// Reading it fails with an unsupported [`GenieError`] whose reason names the part; it is never
/// written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Unsupported;

impl BinRead for Unsupported {
    type Args<'a> = (&'static str,);

    fn read_options<R: Read + Seek>(
        reader: &mut R,
        _: Endian,
        (what,): Self::Args<'_>,
    ) -> BinResult<Self> {
        Err(binrw::Error::Custom {
            pos: reader.stream_position()?,
            err: Box::new(GenieError::unsupported(what)),
        })
    }
}

impl BinWrite for Unsupported {
    type Args<'a> = ();

    fn write_options<W: Write + Seek>(&self, writer: &mut W, _: Endian, (): ()) -> BinResult<()> {
        Err(binrw::Error::Custom {
            pos: writer.stream_position()?,
            err: Box::new(GenieError::invalid_input("a part this reader never reads")),
        })
    }
}

/// A value with the stream position it was read at.
///
/// The reader reports where the save's sections start (aoe2de-exe-re `layout`, overlay
/// `positioned`). Writes the value only. binrw's own `PosValue` compares only with the bare
/// value, so a struct holding it cannot derive `PartialEq`. Its default is the value's at
/// position 0 (for a test's save parts).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Positioned<T> {
    /// The stream position.
    pub pos: u64,
    /// The value.
    pub val: T,
}

impl<T> Deref for Positioned<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.val
    }
}

impl<T: BinRead> BinRead for Positioned<T> {
    type Args<'a> = T::Args<'a>;

    fn read_options<R: Read + Seek>(
        reader: &mut R,
        endian: Endian,
        args: Self::Args<'_>,
    ) -> BinResult<Self> {
        let pos = reader.stream_position()?;
        Ok(Self {
            pos,
            val: T::read_options(reader, endian, args)?,
        })
    }
}

impl<T: BinWrite> BinWrite for Positioned<T> {
    type Args<'a> = T::Args<'a>;

    fn write_options<W: Write + Seek>(
        &self,
        writer: &mut W,
        endian: Endian,
        args: Self::Args<'_>,
    ) -> BinResult<()> {
        self.val.write_options(writer, endian, args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use binrw::io::Cursor;

    #[test]
    fn positioned_value_keeps_where_it_was_read_and_writes_the_value_only() {
        let mut cursor = Cursor::new(b"\x01\x02\x00".as_slice());
        let _ = u8::read_le(&mut cursor).unwrap();
        let positioned = Positioned::<u16>::read_le(&mut cursor).unwrap();
        assert_eq!((positioned.pos, positioned.val, *positioned), (1, 2, 2));
        let mut out = Cursor::new(Vec::new());
        positioned.write_le(&mut out).unwrap();
        assert_eq!(out.into_inner(), b"\x02\x00");
    }

    #[test]
    fn strings_read_their_length_then_the_bytes() {
        let mut cursor = Cursor::new(b"\x02\x00hi\x01\x00\x00\x00!rest".as_slice());
        assert_eq!(LenString16::read(&mut cursor).unwrap().bytes, b"hi");
        assert_eq!(LenString32::read(&mut cursor).unwrap().bytes, b"!");
        assert_eq!(cursor.position(), 9);
    }

    #[test]
    fn sig_string_takes_the_signature_before_or_after_the_length() {
        let mut cursor = Cursor::new(b"\x60\x0a\x02\x00hi\x01\x00\x60\x0a!".as_slice());
        assert_eq!(
            SigString::read_le(&mut cursor).unwrap(),
            SigString {
                swapped: false,
                bytes: b"hi".to_vec()
            }
        );
        assert_eq!(
            SigString::read_le(&mut cursor).unwrap(),
            SigString {
                swapped: true,
                bytes: b"!".to_vec()
            }
        );
        assert!(SigString::read_le(&mut Cursor::new(b"\x01\x00\x02\x00".as_slice())).is_err());
    }

    /// Read, then write: the same bytes.
    fn round_trip<T>(bytes: &[u8]) -> T
    where
        T: for<'a> BinRead<Args<'a> = ()> + for<'a> BinWrite<Args<'a> = ()>,
    {
        let value = T::read_le(&mut Cursor::new(bytes)).unwrap();
        let mut out = Cursor::new(Vec::new());
        BinWrite::write_le(&value, &mut out).unwrap();
        assert_eq!(out.into_inner(), bytes);
        value
    }

    #[test]
    fn every_value_shape_writes_back_the_bytes_it_read() {
        assert_eq!(round_trip::<LenString16>(b"\x02\x00hi").bytes, b"hi");
        assert_eq!(round_trip::<LenString32>(b"\x01\x00\x00\x00!").bytes, b"!");
        assert!(!round_trip::<SigString>(b"\x60\x0a\x02\x00hi").swapped);
        assert!(round_trip::<SigString>(b"\x01\x00\x60\x0a!").swapped);
        assert_eq!(
            round_trip::<LenVec32<i64>>(b"\x01\x00\x00\x00\x07\x00\x00\x00\x00\x00\x00\x00"),
            LenVec32(vec![7])
        );
    }

    #[test]
    fn an_unsupported_part_fails_to_read_and_names_itself() {
        let e = Unsupported::read_le_args(&mut Cursor::new(b"\0".as_slice()), ("the AI state",))
            .unwrap_err();
        assert_eq!(
            unsupported_in(&e).map(|(error, at)| (error.kind(), error.reason(), at)),
            Some((genie_error::ErrorKind::Unsupported, "the AI state", 0))
        );
    }

    #[test]
    fn vec_reads_its_count_then_the_values() {
        let mut cursor = Cursor::new(
            b"\x02\x00\x00\x00\x01\x00\x00\x00\x00\x00\x00\x00\xff\xff\xff\xff\xff\xff\xff\xff"
                .as_slice(),
        );
        assert_eq!(
            LenVec32::<i64>::read_le(&mut cursor).unwrap(),
            LenVec32(vec![1, -1])
        );
        assert_eq!(cursor.position(), 20);
    }
}
