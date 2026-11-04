use crate::read_bytes;
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use encoding_rs::WINDOWS_1252;
use genie_error::{GenieError, GenieResult};
use std::io::{Read, Write};

/// A string that does not decode as WINDOWS-1252.
const NOT_DECODED: GenieError =
    GenieError::invalid_input("a string that does not decode as WINDOWS-1252");
/// A string that does not encode as WINDOWS-1252.
const NOT_ENCODED: GenieError =
    GenieError::invalid_input("a string that does not encode as WINDOWS-1252");

/// A string whose length with its NULL terminator does not fit the length prefix.
#[track_caller]
fn too_long(len: usize) -> GenieError {
    GenieError::invalid_input("a string too long for its length prefix").with("length", len)
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
pub fn write_str<W: Write>(output: &mut W, string: &str) -> GenieResult<()> {
    let (bytes, _enc, failed) = WINDOWS_1252.encode(string);
    if failed {
        return Err(NOT_ENCODED);
    }
    let len = i16::try_from(bytes.len() + 1).map_err(|_| too_long(bytes.len()))?;
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
pub fn write_i32_str<W: Write>(output: &mut W, string: &str) -> GenieResult<()> {
    let (bytes, _enc, failed) = WINDOWS_1252.encode(string);
    if failed {
        return Err(NOT_ENCODED);
    }
    let len = i32::try_from(bytes.len() + 1).map_err(|_| too_long(bytes.len()))?;
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
pub fn write_opt_str<W: Write>(output: &mut W, option: &Option<String>) -> GenieResult<()> {
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
pub fn write_opt_i32_str<W: Write>(output: &mut W, option: &Option<String>) -> GenieResult<()> {
    if let Some(string) = option {
        write_i32_str(output, string)
    } else {
        output.write_i32::<LE>(0)?;
        Ok(())
    }
}

/// Decode a string using the WINDOWS-1252 code page.
fn decode_str(bytes: &[u8]) -> GenieResult<String> {
    if bytes.is_empty() {
        return Ok(String::new());
    }

    let (decoded, _enc, failed) = WINDOWS_1252.decode(bytes);
    if failed {
        Err(NOT_DECODED)
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
    fn read_str(&mut self, length: usize) -> GenieResult<Option<String>> {
        if length > 0 {
            let mut bytes = read_bytes(self, length)?;
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
    fn read_u16_length_prefixed_str(&mut self) -> GenieResult<Option<String>> {
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
    fn read_u32_length_prefixed_str(&mut self) -> GenieResult<Option<String>> {
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
    fn read_hd_style_str(&mut self) -> GenieResult<Option<String>> {
        let open = self.read_u16::<LE>()?;
        // Check that this actually is the start of a string
        if open != 0x0A60 {
            return Err(NOT_DECODED.with("signature", open));
        }
        let len = self.read_u16::<LE>()? as usize;
        let bytes = read_bytes(self, len)?;
        Ok(Some(decode_str(&bytes)?))
    }
}

impl<T> ReadStringsExt for T where T: Read {}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!((&other[..]).read_hd_style_str(), Err(NOT_DECODED));
        assert_eq!(
            (&bytes[..5]).read_hd_style_str().map_err(|e| e.reason()),
            Err("a list past the input's end")
        );
    }
}
