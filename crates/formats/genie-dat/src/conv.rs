//! Checked conversions for the fields the format fixes the width of: a value that does not fit is
//! an `io::Error` of kind `InvalidData`, not a panic.

use std::io;

/// A count or an id that does not fit the width its field has in the file.
const DOES_NOT_FIT: &str = "a value that does not fit its field";

/// The error of a value the format cannot hold: `InvalidData`, with `reason`.
pub(crate) fn invalid_data(
    reason: impl Into<Box<dyn std::error::Error + Send + Sync>>,
) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, reason)
}

/// `value` as the type of the field it is read from or written to.
pub(crate) fn fits<T, U: TryInto<T>>(value: U) -> io::Result<T> {
    value.try_into().map_err(|_| invalid_data(DOES_NOT_FIT))
}

/// An optional id as its field: `none` (−1) for no id.
pub(crate) fn opt_fits<T, U: TryInto<T>>(value: Option<U>, none: T) -> io::Result<T> {
    value.map_or(Ok(none), fits)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A value past its field's width is an `InvalidData` error; an absent id is the none value.
    #[test]
    fn value_past_its_field_is_invalid_data_and_an_absent_id_is_none() {
        assert_eq!(fits::<u16, _>(65535usize).unwrap(), 65535);
        assert_eq!(
            fits::<u16, _>(65536usize).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(opt_fits::<i16, u16>(Some(7), -1).unwrap(), 7);
        assert_eq!(opt_fits::<i16, u16>(None, -1).unwrap(), -1);
        assert!(opt_fits::<i16, u32>(Some(70_000), -1).is_err());
    }
}
