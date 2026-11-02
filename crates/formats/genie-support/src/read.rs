use byteorder::{LE, ReadBytesExt};
use std::convert::{TryFrom, TryInto};
use std::error::Error as StdError;
use std::io::{self, Error, ErrorKind, Read, Result};

/// Read a 2-byte integer that uses -1 as an "absent" value.
///
/// ## Example
///
/// ```rust
/// use genie_support::read_opt_u16;
///
/// let mut minus_one = std::io::Cursor::new(vec![0xFF, 0xFF]);
/// let mut zero = std::io::Cursor::new(vec![0x00, 0x00]);
///
/// assert_eq!(read_opt_u16::<u16, _>(&mut minus_one).unwrap(), None);
/// assert_eq!(read_opt_u16(&mut zero).unwrap(), Some(0));
/// ```
///
/// # Errors
///
/// Returns an error if reading from `input` fails, or if the value does not fit in `T`
/// (`ErrorKind::InvalidData`).
#[inline]
pub fn read_opt_u16<T, R>(input: &mut R) -> Result<Option<T>>
where
    T: TryFrom<u16>,
    T::Error: StdError + Send + Sync + 'static,
    R: Read,
{
    let opt = match input.read_u16::<LE>()? {
        0xFFFF => None,
        v => Some(
            v.try_into()
                .map_err(|e| Error::new(ErrorKind::InvalidData, e))?,
        ),
    };
    Ok(opt)
}

/// Read a 4-byte integer that uses -1 as an "absent" value.
///
/// ## Example
///
/// ```rust
/// use genie_support::read_opt_u32;
///
/// let mut minus_one = std::io::Cursor::new(vec![0xFF, 0xFF, 0xFF, 0xFF]);
/// let mut one = std::io::Cursor::new(vec![0x01, 0x00, 0x00, 0x00]);
///
/// assert_eq!(read_opt_u32::<u32, _>(&mut minus_one).unwrap(), None);
/// assert_eq!(read_opt_u32(&mut one).unwrap(), Some(1));
/// ```
///
/// # Errors
///
/// Returns an error if reading from `input` fails, or if the value does not fit in `T`
/// (`ErrorKind::InvalidData`).
#[inline]
pub fn read_opt_u32<T, R>(input: &mut R) -> Result<Option<T>>
where
    T: TryFrom<u32>,
    T::Error: StdError + Send + Sync + 'static,
    R: Read,
{
    let opt = match input.read_u32::<LE>()? {
        // -1, or -2, which HD Edition uses in some places.
        0xFFFF_FFFF | 0xFFFF_FFFE => None,
        v => Some(
            v.try_into()
                .map_err(|e| Error::new(ErrorKind::InvalidData, e))?,
        ),
    };
    Ok(opt)
}

/// Extension trait that adds a `skip()` method to `Read` instances.
pub trait ReadSkipExt {
    /// Read and discard a number of bytes.
    ///
    /// Stopping early at the end of the input is not an error: fewer than `dist` bytes may be
    /// skipped.
    ///
    /// # Errors
    ///
    /// Returns an error if reading fails.
    fn skip(&mut self, dist: u64) -> Result<()>;
}

impl<T: Read> ReadSkipExt for T {
    fn skip(&mut self, dist: u64) -> Result<()> {
        // The count may fall short of `dist` at the end of the input; that is not checked.
        let _ = io::copy(&mut self.by_ref().take(dist), &mut io::sink())?;
        Ok(())
    }
}
