//! Checked conversions for the fields the format fixes the width of: a value that does not fit is
//! invalid input, not a panic.

use genie_error::{GenieError, GenieResult};

/// A count or an id that does not fit the width its field has in the file.
const DOES_NOT_FIT: GenieError = GenieError::invalid_input("a value that does not fit its field");

/// The text of a fixed name field up to its first NUL.
pub(crate) fn name_to_nul(bytes: &[u8]) -> GenieResult<String> {
    String::from_utf8(bytes.iter().copied().take_while(|b| *b != 0).collect())
        .map_err(|e| GenieError::invalid_input("a name that is no UTF-8").with_source(e))
}

/// `value` as the type of the field it is read from or written to.
pub(crate) fn fits<T, U: TryInto<T>>(value: U) -> GenieResult<T> {
    value.try_into().map_err(|_| DOES_NOT_FIT)
}

/// An optional id as its field: `none` (−1) for no id.
pub(crate) fn opt_fits<T, U: TryInto<T>>(value: Option<U>, none: T) -> GenieResult<T> {
    value.map_or(Ok(none), fits)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A value past its field's width is invalid input; an absent id is the none value.
    #[test]
    fn value_past_its_field_is_invalid_input_and_an_absent_id_is_none() {
        assert_eq!(fits::<u16, _>(65535usize).unwrap(), 65535);
        assert_eq!(fits::<u16, _>(65536usize), Err(DOES_NOT_FIT));
        assert_eq!(opt_fits::<i16, u16>(Some(7), -1).unwrap(), 7);
        assert_eq!(opt_fits::<i16, u16>(None, -1).unwrap(), -1);
        assert!(opt_fits::<i16, u32>(Some(70_000), -1).is_err());
    }

    /// A name ends at its first NUL; bytes that are no UTF-8 are invalid input.
    #[test]
    fn name_ends_at_its_nul_and_must_be_utf8() {
        assert_eq!(name_to_nul(b"ab\0cd").unwrap(), "ab");
        assert_eq!(
            name_to_nul(&[0xff]).map_err(|e| e.reason()),
            Err("a name that is no UTF-8")
        );
    }
}
