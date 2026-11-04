//! Lists whose length a file names (checklist J10): each reserves room for at most
//! [`RESERVE_CAP`] elements before they are read and grows as they come, so a count that
//! claims more than the file holds fails at the file's end instead of aborting the process on
//! its allocation, in a browser too.

use genie_error::{GenieError, GenieResult};
use std::io::Read;

/// The most elements a list reserves room for before they are read.
pub const RESERVE_CAP: usize = 4096;

/// A list too long for the address space.
const PAST_ADDRESSES: GenieError =
    GenieError::invalid_input("a list longer than the address space");

/// An empty list with room for `count` elements, at most [`RESERVE_CAP`]: it grows as they
/// come.
#[must_use]
pub fn reserved<T>(count: usize) -> Vec<T> {
    Vec::with_capacity(count.min(RESERVE_CAP))
}

/// `count` elements, each `read` in turn, in a list that grows as they come.
///
/// # Errors
///
/// The first error of `read`.
pub fn read_list<T, E>(count: usize, mut read: impl FnMut() -> Result<T, E>) -> Result<Vec<T>, E> {
    let mut list = reserved(count);
    for _ in 0..count {
        list.push(read()?);
    }
    Ok(list)
}

/// The next `len` bytes of `input`, read as they come.
///
/// # Errors
///
/// Invalid input when `input` ends before `len` bytes, and `input`'s errors.
pub fn read_bytes<R: Read + ?Sized>(input: &mut R, len: usize) -> GenieResult<Vec<u8>> {
    let mut bytes = reserved(len);
    let limit = u64::try_from(len).map_err(|_| PAST_ADDRESSES.with("length", len))?;
    let _ = Read::take(&mut *input, limit).read_to_end(&mut bytes)?;
    if bytes.len() < len {
        return Err(GenieError::invalid_input("a list past the input's end")
            .with("length", len)
            .with("read", bytes.len()));
    }
    Ok(bytes)
}

/// The next `count` values of `N` little-endian bytes each, `from` making a value of its bytes
/// (`f32::from_le_bytes`), read as they come.
///
/// # Errors
///
/// Invalid input when `input` ends before them or for a `count` whose bytes no address
/// reaches, and `input`'s errors.
pub fn read_le<const N: usize, T, R: Read + ?Sized>(
    input: &mut R,
    count: usize,
    from: fn([u8; N]) -> T,
) -> GenieResult<Vec<T>> {
    let len = count
        .checked_mul(N)
        .ok_or_else(|| PAST_ADDRESSES.with("count", count))?;
    let bytes = read_bytes(input, len)?;
    Ok(bytes
        .as_chunks::<N>()
        .0
        .iter()
        .map(|&value| from(value))
        .collect())
}

/// The next `count` signed bytes of `input`, read as they come.
///
/// # Errors
///
/// Invalid input when `input` ends before them, and `input`'s errors.
pub fn read_i8s<R: Read + ?Sized>(input: &mut R, count: usize) -> GenieResult<Vec<i8>> {
    Ok(read_bytes(input, count)?
        .into_iter()
        .map(u8::cast_signed)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// A count past the input's bytes fails at the input's end, with room reserved for at most
    /// the cap; one the input holds reads in full.
    #[test]
    fn counts_past_the_input_fail_at_its_end() {
        let list: Vec<u8> = reserved(usize::MAX);
        assert!(list.capacity() <= RESERVE_CAP);
        let mut input = Cursor::new(vec![1u8, 2, 3]);
        assert_eq!(
            read_bytes(&mut input, usize::MAX).map_err(|e| e.reason()),
            Err("a list past the input's end")
        );
        let mut input = Cursor::new(vec![1u8, 2, 3]);
        assert_eq!(read_bytes(&mut input, 2).unwrap(), [1, 2]);
        let mut input = Cursor::new(vec![0xffu8, 2]);
        assert_eq!(read_i8s(&mut input, 2).unwrap(), [-1, 2]);
        let mut input = Cursor::new(1.5f32.to_le_bytes().repeat(2));
        assert_eq!(
            read_le(&mut input, 2, f32::from_le_bytes).unwrap(),
            [1.5, 1.5]
        );
        let mut input = Cursor::new(vec![0u8; 4]);
        assert_eq!(
            read_le(&mut input, usize::MAX, f32::from_le_bytes).map_err(|e| e.reason()),
            Err("a list longer than the address space")
        );
        let mut input = Cursor::new(vec![0u8; 4]);
        assert_eq!(
            read_le(&mut input, 2, f32::from_le_bytes).map_err(|error| error.reason()),
            Err("a list past the input's end")
        );
        let mut next = 0;
        let read = read_list(3, || {
            next += 1;
            Ok::<_, &str>(next)
        });
        assert_eq!(read, Ok(vec![1, 2, 3]));
        let mut n = 0;
        assert_eq!(
            read_list(usize::MAX, || {
                n += 1;
                if n < 4 { Ok(n) } else { Err("the input's end") }
            }),
            Err("the input's end")
        );
    }
}
