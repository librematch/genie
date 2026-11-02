use crate::reader::RecordingHeaderReader;
use std::io::Error;
use std::io::{Read, Write};

/// A part of the recording header that may be absent: reading it can give `None`.
pub trait OptionalReadableElement: Sized {
    /// Read the element from the header, or `None` if the header does not have it here.
    ///
    /// # Errors
    ///
    /// Returns an error if reading or decoding fails.
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> crate::Result<Option<Self>>;
}

impl<T: OptionalReadableElement> ReadableElement<Option<T>> for T {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> crate::Result<Option<T>> {
        OptionalReadableElement::read_from(input)
    }
}

/// Something that reads from the recording header as a `T`; implemented for every
/// [`ReadableHeaderElement`] (as itself) and [`OptionalReadableElement`] (as an `Option`).
pub trait ReadableElement<T>: Sized {
    /// Read the element from the header.
    ///
    /// # Errors
    ///
    /// Returns an error if reading or decoding fails.
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> crate::Result<T>;
}

/// A part of the recording header that can be read from it.
pub trait ReadableHeaderElement: Sized {
    /// Read the element from the header.
    ///
    /// # Errors
    ///
    /// Returns an error if reading or decoding fails.
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> crate::Result<Self>;
}

impl<T: ReadableHeaderElement> ReadableElement<T> for T {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> crate::Result<T> {
        ReadableHeaderElement::read_from(input)
    }
}

/// Something that writes a `T` in the recording header's layout; implemented for every
/// [`WritableHeaderElement`].
pub trait WritableElement<T> {
    /// Write `element` to `output`.
    ///
    /// # Errors
    ///
    /// Returns an error if writing fails.
    fn write_to<W: Write>(element: &T, output: &mut W) -> crate::Result<()>;
}

/// A part of the recording header that can be written back.
pub trait WritableHeaderElement {
    /// Write the element to `output` in the header's layout.
    ///
    /// # Errors
    ///
    /// Returns an error if writing fails.
    ///
    /// The default implementation is a placeholder that always fails with
    /// [`crate::Error::NotWritten`].
    fn write_to<W: Write>(&self, output: &mut W) -> crate::Result<()> {
        // we need to use `output` otherwise we'll get warnings that it's not used
        // prefixing it would make any traits auto completed also be prefixed with _ and that's annoying
        let _ = output;
        Err(crate::Error::NotWritten)
    }
}

impl<T: WritableHeaderElement> WritableElement<T> for T {
    fn write_to<W: Write>(element: &T, output: &mut W) -> crate::Result<()> {
        WritableHeaderElement::write_to(element, output)
    }
}

/// A `u32` count, then that many elements of type `T`.
///
/// # Errors
///
/// Returns an error if reading the count or an element fails.
pub(crate) fn read_counted<T: ReadableHeaderElement, R: Read>(
    input: &mut RecordingHeaderReader<R>,
) -> crate::Result<Vec<T>> {
    use byteorder::{LE, ReadBytesExt};
    let count = input.read_u32::<LE>()?;
    (0..count).map(|_| T::read_from(input)).collect()
}

/// Three little-endian `f32`s: a position, a velocity or an orientation.
///
/// # Errors
///
/// Returns an error if reading fails.
pub(crate) fn read_xyz<R: Read>(input: &mut R) -> Result<(f32, f32, f32), Error> {
    use byteorder::{LE, ReadBytesExt};
    Ok((
        input.read_f32::<LE>()?,
        input.read_f32::<LE>()?,
        input.read_f32::<LE>()?,
    ))
}
