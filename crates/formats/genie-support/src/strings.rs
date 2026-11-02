use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use encoding_rs::WINDOWS_1252;
use std::io::{self, Read, Write};

/// Failed to decode a string as WINDOWS-1252.
///
/// This means that the scenario file contained a string that could not be decoded using the
/// WINDOWS-1252 code page. In the future, genie-scx will support other encodings.
#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("could not decode string as WINDOWS-1252")]
pub struct DecodeStringError;

/// Failed to encode a string as WINDOWS-1252.
///
/// This means that a string could not be encoded using the WINDOWS-1252 code page. In the future, genie-scx will support other encodings.
#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("could not encode string as WINDOWS-1252")]
pub struct EncodeStringError;

/// Failed to read a string.
#[derive(Debug, thiserror::Error)]
pub enum ReadStringError {
    /// Failed to read a string because the bytes could not be decoded.
    #[error(transparent)]
    DecodeStringError(#[from] DecodeStringError),
    /// Failed to read a string because the underlying I/O failed.
    #[error(transparent)]
    IoError(#[from] io::Error),
}

/// Failed to write a string.
#[derive(Debug, thiserror::Error)]
pub enum WriteStringError {
    /// Failed to read a string because it could not be encoded.
    #[error(transparent)]
    EncodeStringError(#[from] EncodeStringError),
    /// Failed to write a string because its length with the NULL terminator does not fit the
    /// length prefix.
    #[error("string too long for its length prefix")]
    TooLong,
    /// Failed to write a string because the underlying I/O failed.
    #[error(transparent)]
    IoError(#[from] io::Error),
}

/// Write a string to an output stream, using code page 1252, using a `u16` for the length prefix.
///
/// This writes the length of the string (including NULL terminator) as a little-endian i16,
/// followed by the encoded bytes, followed by a NULL terminator.
///
/// # Errors
///
/// Returns an error if the string cannot be encoded as WINDOWS-1252, or if writing fails.
///
/// Also fails if the encoded string is `i16::MAX` bytes or longer, so that its length with the
/// NULL terminator does not fit the prefix.
pub fn write_str<W: Write>(output: &mut W, string: &str) -> Result<(), WriteStringError> {
    let (bytes, _enc, failed) = WINDOWS_1252.encode(string);
    if failed {
        return Err(WriteStringError::EncodeStringError(EncodeStringError));
    }
    let len = i16::try_from(bytes.len() + 1).map_err(|_| WriteStringError::TooLong)?;
    output.write_i16::<LE>(len)?;
    output.write_all(&bytes)?;
    output.write_u8(0)?;
    Ok(())
}

/// Write a string to an output stream, using code page 1252, using an `i32` for the length prefix.
///
/// This writes the length of the string (including NULL terminator) as a little-endian i32,
/// followed by the encoded bytes, followed by a NULL terminator.
///
/// # Errors
///
/// Returns an error if the string cannot be encoded as WINDOWS-1252, or if writing fails.
///
/// Also fails if the encoded string is `i32::MAX` bytes or longer, so that its length with the
/// NULL terminator does not fit the prefix.
pub fn write_i32_str<W: Write>(output: &mut W, string: &str) -> Result<(), WriteStringError> {
    let (bytes, _enc, failed) = WINDOWS_1252.encode(string);
    if failed {
        return Err(WriteStringError::EncodeStringError(EncodeStringError));
    }
    let len = i32::try_from(bytes.len() + 1).map_err(|_| WriteStringError::TooLong)?;
    output.write_i32::<LE>(len)?;
    output.write_all(&bytes)?;
    output.write_u8(0)?;
    Ok(())
}

/// Write a string to an output stream, using code page 1252, using a `u16` for the length prefix.
///
/// When given a `None`, it outputs a 0 for the length. Otherwise, see `write_str`.
///
/// # Errors
///
/// Returns an error if the string cannot be encoded as WINDOWS-1252, or if writing fails.
///
/// Also fails if the encoded string is too long for the `i16` prefix (see `write_str`).
pub fn write_opt_str<W: Write>(
    output: &mut W,
    option: &Option<String>,
) -> Result<(), WriteStringError> {
    if let Some(string) = option {
        write_str(output, string)
    } else {
        output.write_i16::<LE>(0)?;
        Ok(())
    }
}

/// Write a string to an output stream, using code page 1252, using a `u32` for the length prefix.
///
/// When given a `None`, it outputs a 0 for the length. Otherwise, see `write_i32_str`.
///
/// # Errors
///
/// Returns an error if the string cannot be encoded as WINDOWS-1252, or if writing fails.
///
/// Also fails if the encoded string is too long for the `i32` prefix (see `write_i32_str`).
pub fn write_opt_i32_str<W: Write>(
    output: &mut W,
    option: &Option<String>,
) -> Result<(), WriteStringError> {
    if let Some(string) = option {
        write_i32_str(output, string)
    } else {
        output.write_i32::<LE>(0)?;
        Ok(())
    }
}

/// Decode a string using the WINDOWS-1252 code page.
fn decode_str(bytes: &[u8]) -> Result<String, DecodeStringError> {
    if bytes.is_empty() {
        return Ok(String::new());
    }

    let (decoded, _enc, failed) = WINDOWS_1252.decode(bytes);
    if failed {
        Err(DecodeStringError)
    } else {
        Ok(decoded.to_string())
    }
}

/// Functions to read various kinds of strings from input streams.
/// Extension trait for reading strings in several common formats used by `AoE2`.
pub trait ReadStringsExt: Read {
    /// Read an optionally null-terminated WINDOWS-1252-encoded string with the given `length` in bytes.
    ///
    /// Returns `None` for a zero `length` or an empty string.
    ///
    /// # Errors
    ///
    /// Returns an error if reading fails or the bytes are not valid WINDOWS-1252.
    fn read_str(&mut self, length: usize) -> Result<Option<String>, ReadStringError> {
        if length > 0 {
            let mut bytes = vec![0; length];
            self.read_exact(&mut bytes)?;
            if let Some(end) = bytes.iter().position(|&byte| byte == 0) {
                bytes.truncate(end);
            }
            if bytes.is_empty() {
                Ok(None)
            } else {
                Ok(Some(decode_str(&bytes)?))
            }
        } else {
            Ok(None)
        }
    }

    /// Read an u16 value, then read an optionally null-terminated WINDOWS-1252-encoded string of
    /// that length in bytes. A length of `0xFFFF` gives `None`.
    ///
    /// # Errors
    ///
    /// Returns an error if reading fails or the bytes are not valid WINDOWS-1252.
    fn read_u16_length_prefixed_str(&mut self) -> Result<Option<String>, ReadStringError> {
        match self.read_u16::<LE>()? {
            0xFFFF => Ok(None),
            len => self.read_str(len as usize),
        }
    }

    /// Read an u32 value, then read an optionally null-terminated WINDOWS-1252-encoded string of
    /// that length in bytes. A length of `0xFFFF_FFFF` gives `None`.
    ///
    /// # Errors
    ///
    /// Returns an error if reading fails or the bytes are not valid WINDOWS-1252.
    fn read_u32_length_prefixed_str(&mut self) -> Result<Option<String>, ReadStringError> {
        match self.read_u32::<LE>()? {
            0xFFFF_FFFF => Ok(None),
            len => self.read_str(len as usize),
        }
    }

    /// Read an HD Edition style string.
    ///
    /// Reads a 'signature' value, then the `length` as an u16 value, then reads an optionally
    /// null-terminated WINDOWS-1252-encoded string of that length in bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if reading fails, if the signature is not 0x0A60, or if the bytes are not
    /// valid WINDOWS-1252.
    fn read_hd_style_str(&mut self) -> Result<Option<String>, ReadStringError> {
        let open = self.read_u16::<LE>()?;
        // Check that this actually is the start of a string
        if open != 0x0A60 {
            return Err(DecodeStringError.into());
        }
        let len = self.read_u16::<LE>()? as usize;
        let mut bytes = vec![0; len];
        self.read_exact(&mut bytes[0..len])?;
        Ok(Some(decode_str(&bytes)?))
    }
}

impl<T> ReadStringsExt for T where T: Read {}

impl From<DecodeStringError> for genie_error::GenieError {
    /// The error as a [`genie_error::GenieError`]: its kind and reason, the original as the
    /// source.
    #[track_caller]
    fn from(e: DecodeStringError) -> Self {
        Self::invalid_input("a string that does not decode as WINDOWS-1252").with_source(e)
    }
}

impl From<EncodeStringError> for genie_error::GenieError {
    /// The error as a [`genie_error::GenieError`]: its kind and reason, the original as the
    /// source.
    #[track_caller]
    fn from(e: EncodeStringError) -> Self {
        Self::invalid_input("a string that does not encode as WINDOWS-1252").with_source(e)
    }
}

impl From<ReadStringError> for genie_error::GenieError {
    /// The error as a [`genie_error::GenieError`]: its kind and reason, the original as the
    /// source.
    #[track_caller]
    fn from(e: ReadStringError) -> Self {
        let to = match &e {
            ReadStringError::DecodeStringError(_) => {
                Self::invalid_input("a string that does not decode as WINDOWS-1252")
            }
            ReadStringError::IoError(_) => Self::io("a read or write that failed"),
        };
        to.with_source(e)
    }
}

impl From<WriteStringError> for genie_error::GenieError {
    /// The error as a [`genie_error::GenieError`]: its kind and reason, the original as the
    /// source.
    #[track_caller]
    fn from(e: WriteStringError) -> Self {
        let to = match &e {
            WriteStringError::EncodeStringError(_) => {
                Self::invalid_input("a string that does not encode as WINDOWS-1252")
            }
            WriteStringError::TooLong => {
                Self::invalid_input("a string too long for its length prefix")
            }
            WriteStringError::IoError(_) => Self::io("a read or write that failed"),
        };
        to.with_source(e)
    }
}

#[cfg(test)]
mod genie_error_tests {
    use super::*;
    #[cfg(test)]
    use std::error::Error;

    /// The errors convert into `GenieError`s with their kinds and reasons, the original as
    /// source.
    #[test]
    fn string_errors_convert_into_genie_errors() {
        let e: genie_error::GenieError = DecodeStringError.into();
        assert_eq!(
            (e.kind(), e.reason()),
            (
                genie_error::ErrorKind::InvalidInput,
                "a string that does not decode as WINDOWS-1252"
            )
        );
        assert!(Error::source(&e).is_some());
        let e: genie_error::GenieError = EncodeStringError.into();
        assert_eq!(
            (e.kind(), e.reason()),
            (
                genie_error::ErrorKind::InvalidInput,
                "a string that does not encode as WINDOWS-1252"
            )
        );
        assert!(Error::source(&e).is_some());
        let e: genie_error::GenieError = ReadStringError::IoError(io::Error::other("x")).into();
        assert_eq!(
            (e.kind(), e.reason()),
            (genie_error::ErrorKind::Io, "a read or write that failed")
        );
        assert!(Error::source(&e).is_some());
        let e: genie_error::GenieError =
            WriteStringError::EncodeStringError(EncodeStringError).into();
        assert_eq!(
            (e.kind(), e.reason()),
            (
                genie_error::ErrorKind::InvalidInput,
                "a string that does not encode as WINDOWS-1252"
            )
        );
        assert!(Error::source(&e).is_some());
    }

    /// An HD Edition string: the signature 0x0A60, the length and that many bytes; another
    /// signature does not decode, a short one fails to read.
    #[test]
    fn an_hd_style_string_starts_with_its_signature() {
        let bytes = [0x60, 0x0a, 2, 0, b'a', b'b'];
        assert_eq!(
            (&bytes[..]).read_hd_style_str().unwrap(),
            Some("ab".to_owned())
        );
        let other = [0x61, 0x0a, 1, 0, b'a'];
        assert!(matches!(
            (&other[..]).read_hd_style_str(),
            Err(ReadStringError::DecodeStringError(_))
        ));
        assert!(matches!(
            (&bytes[..5]).read_hd_style_str(),
            Err(ReadStringError::IoError(_))
        ));
    }
}
