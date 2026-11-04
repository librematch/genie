use crate::element::{ReadableHeaderElement, WritableHeaderElement};
use crate::errors::{not_utf8, out_of_range};
use crate::fits;
use crate::reader::RecordingHeaderReader;
use byteorder::{LE, ReadBytesExt, WriteBytesExt};
use genie_error::GenieResult;
use genie_support::{read_bytes, reserved};
use std::io::{Read, Write};
use std::vec::IntoIter as VecIntoIter;

/// A table of strings in the recording header, with room for a fixed number of them.
#[derive(Debug, Clone)]
pub struct StringTable {
    max_strings: u16,
    strings: Vec<String>,
}

impl StringTable {
    /// An empty table with room for `max_strings` strings.
    #[must_use]
    pub const fn new(max_strings: u16) -> Self {
        Self {
            max_strings,
            strings: vec![],
        }
    }

    /// The number of strings the table has room for.
    #[must_use]
    pub const fn max_strings(&self) -> u16 {
        self.max_strings
    }

    /// The number of strings in the table.
    ///
    /// # Errors
    ///
    /// Invalid input if the table holds 65535 (`u16::MAX`) strings or more.
    pub fn num_strings(&self) -> GenieResult<u16> {
        let len = self.strings.len();
        if len < usize::from(u16::MAX) {
            fits(len)
        } else {
            Err(out_of_range())
        }
    }

    /// The strings in the table.
    #[must_use]
    pub const fn strings(&self) -> &Vec<String> {
        &self.strings
    }
}

impl ReadableHeaderElement for StringTable {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> GenieResult<Self> {
        let max_strings = input.read_u16::<LE>()?;
        let num_strings = input.read_u16::<LE>()?;
        let _ptr = input.read_u32::<LE>()?;

        let mut strings = reserved(usize::from(max_strings));
        for _ in 0..num_strings {
            let length = input.read_u32::<LE>()?;
            let bytes = read_bytes(input, fits(length)?)?;
            strings.push(String::from_utf8(bytes).map_err(|_| not_utf8())?);
        }

        Ok(Self {
            max_strings,
            strings,
        })
    }
}

impl WritableHeaderElement for StringTable {
    fn write_to<W: Write>(&self, handle: &mut W) -> GenieResult<()> {
        handle.write_u16::<LE>(self.max_strings)?;
        handle.write_u16::<LE>(self.num_strings()?)?;
        handle.write_u32::<LE>(0)?;

        for string in &self.strings {
            let len = string.len();
            if u32::MAX as usize <= len {
                return Err(out_of_range());
            }
            handle.write_u32::<LE>(fits(len)?)?;
            handle.write_all(string.as_bytes())?;
        }

        Ok(())
    }
}

impl IntoIterator for StringTable {
    type Item = String;
    type IntoIter = VecIntoIter<String>;

    fn into_iter(self) -> Self::IntoIter {
        self.strings.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::writes_back;

    /// The table: its capacity, its count, a null pointer, then each string behind its
    /// length.
    #[test]
    fn string_table_writes_back_its_strings() {
        let table = StringTable {
            max_strings: 4,
            strings: vec!["a".to_owned(), "bc".to_owned()],
        };
        let read = writes_back(&table, 11.97);
        assert_eq!((read.max_strings(), read.num_strings().unwrap()), (4, 2));
        assert_eq!(read.into_iter().collect::<Vec<_>>(), ["a", "bc"]);
    }
}
