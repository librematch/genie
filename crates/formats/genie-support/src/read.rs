use byteorder::{LE, ReadBytesExt};
use genie_error::{GenieError, GenieResult};
use std::convert::{TryFrom, TryInto};
use std::error::Error as StdError;
use std::io::{self, Read};

/// A count or an id that does not fit the width its field has in the file.
pub const FIELD_DOES_NOT_FIT: GenieError =
    GenieError::invalid_input("a value that does not fit its field");

/// An optional value that does not fit its type.
#[track_caller]
fn not_fitting(value: u32, error: impl StdError + Send + Sync + 'static) -> GenieError {
    GenieError::invalid_input("an optional value that does not fit its type")
        .with("value", value)
        .with_source(error)
}

/// Read a 2-byte integer that uses -1 as an "absent" value.
///
/// ## Example
///
/// ```rust
/// # fn main() -> genie_error::GenieResult<()> {
/// use genie_support::read_opt_u16;
///
/// let mut minus_one = std::io::Cursor::new(vec![0xFF, 0xFF]);
/// let mut zero = std::io::Cursor::new(vec![0x00, 0x00]);
///
/// assert_eq!(read_opt_u16::<u16, _>(&mut minus_one)?, None);
/// assert_eq!(read_opt_u16(&mut zero)?, Some(0));
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// Returns an error if reading from `input` fails, or if the value does not fit in `T`
/// (invalid input).
#[inline]
pub fn read_opt_u16<T, R>(input: &mut R) -> GenieResult<Option<T>>
where
    T: TryFrom<u16>,
    T::Error: StdError + Send + Sync + 'static,
    R: Read,
{
    let opt = match input.read_u16::<LE>()? {
        0xFFFF => None,
        value => Some(
            value
                .try_into()
                .map_err(|error| not_fitting(value.into(), error))?,
        ),
    };
    Ok(opt)
}

/// Read a 4-byte integer that uses -1 as an "absent" value.
///
/// ## Example
///
/// ```rust
/// # fn main() -> genie_error::GenieResult<()> {
/// use genie_support::read_opt_u32;
///
/// let mut minus_one = std::io::Cursor::new(vec![0xFF, 0xFF, 0xFF, 0xFF]);
/// let mut one = std::io::Cursor::new(vec![0x01, 0x00, 0x00, 0x00]);
///
/// assert_eq!(read_opt_u32::<u32, _>(&mut minus_one)?, None);
/// assert_eq!(read_opt_u32(&mut one)?, Some(1));
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// Returns an error if reading from `input` fails, or if the value does not fit in `T`
/// (invalid input).
#[inline]
pub fn read_opt_u32<T, R>(input: &mut R) -> GenieResult<Option<T>>
where
    T: TryFrom<u32>,
    T::Error: StdError + Send + Sync + 'static,
    R: Read,
{
    let opt = match input.read_u32::<LE>()? {
        // -1, or -2, which HD Edition uses in some places.
        0xFFFF_FFFF | 0xFFFF_FFFE => None,
        value => Some(
            value
                .try_into()
                .map_err(|error| not_fitting(value, error))?,
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
    fn skip(&mut self, dist: u64) -> GenieResult<()>;
}

impl<T: Read> ReadSkipExt for T {
    fn skip(&mut self, dist: u64) -> GenieResult<()> {
        // The count may fall short of `dist` at the end of the input; that is not checked.
        let _ = io::copy(&mut self.by_ref().take(dist), &mut io::sink())?;
        Ok(())
    }
}
