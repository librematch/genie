//! JASC Palette file reader/writer.
//!
//! Parses and stringifes palette files containing any number of colours.
//!
//! JASC palette files follow a simple format:
//! ```txt
//! JASC-PAL
//! 0100
//! $num_colors
//! $rgb...
//! ```
//!
//! Colours are represented using the [`rgb`](https://crates.io/crates/rgb) crate.
//!
//! ## Example
//! ```rust
//! use std::io::Cursor;
//! use jascpal::{Palette, PaletteIndex, Color};
//! let cursor = Cursor::new(b"JASC-PAL\r\n0100\r\n2\r\n0 0 0\r\n255 255 255\r\n".to_vec());
//! let pal = Palette::read_from(cursor).unwrap();
//! assert_eq!(pal[PaletteIndex::from(0)], Color { r: 0, g: 0, b: 0 });
//! assert_eq!(pal[PaletteIndex::from(1)], Color { r: 255, g: 255, b: 255 });
//! let mut pal = pal;
//! pal[PaletteIndex::from(1)] = Color { r: 0, g: 255, b: 255 };
//! pal.add(Color { r: 255, g: 0, b: 0 });
//! assert_eq!(
//!     pal.to_string(),
//!     "JASC-PAL\r\n0100\r\n3\r\n0 0 0\r\n0 255 255\r\n255 0 0\r\n".to_string()
//! );
//! ```

#![deny(future_incompatible)]
#![deny(nonstandard_style)]
#![deny(rust_2018_idioms)]
#![deny(unsafe_code)]
#![warn(unused)]

use nom::IResult;
use nom::bytes::complete::tag;
use nom::character::complete::{digit1, one_of};
use nom::combinator::{map, map_res};
use nom::multi::many1;
use rgb::RGB;
use std::convert::{TryFrom, TryInto};

use std::fmt::{Display, Error as FmtError, Formatter};
use std::io::Error as IoError;
use std::io::{Read, Write};
use std::num::ParseIntError;
use std::num::TryFromIntError;
use std::ops::{Index, IndexMut};
use std::slice::Iter;
use std::str::{self, FromStr};
use std::vec::IntoIter as VecIntoIter;

/// Represents an RGB colour.
pub type Color = RGB<u8>;

/// A palette index.
///
/// ## Example
/// ```rust
/// use jascpal::{Palette, PaletteIndex, Color};
/// let pal = Palette::default();
/// assert_eq!(pal[PaletteIndex::from(0)], Color { r: 0, g: 0, b: 0 });
/// ```
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PaletteIndex(u8);
impl From<u8> for PaletteIndex {
    #[inline]
    fn from(n: u8) -> Self {
        Self(n)
    }
}

impl TryFrom<i32> for PaletteIndex {
    type Error = TryFromIntError;
    #[inline]
    fn try_from(n: i32) -> Result<Self, Self::Error> {
        n.try_into().map(Self)
    }
}

impl From<PaletteIndex> for u8 {
    #[inline]
    fn from(n: PaletteIndex) -> Self {
        n.0
    }
}

impl From<PaletteIndex> for i32 {
    #[inline]
    fn from(n: PaletteIndex) -> Self {
        n.0.into()
    }
}

impl From<PaletteIndex> for usize {
    #[inline]
    fn from(n: PaletteIndex) -> Self {
        n.0.into()
    }
}

impl FromStr for PaletteIndex {
    type Err = ParseIntError;
    #[inline]
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        input.parse().map(Self)
    }
}

/// Eat any amount of whitespace: ASCII spaces and tabs.
fn whitespace(input: &[u8]) -> IResult<&[u8], ()> {
    map(many1(one_of(&b" \t"[..])), |_| ())(input)
}

/// Parse an UTF-8 byte slice using `FromStr`.
fn parse_bytes<Parsed: FromStr>(string: &[u8]) -> Result<Parsed, ()> {
    str::from_utf8(string)
        .map_err(|_| ())
        .and_then(|s| s.parse().map_err(|_| ()))
}

/// Eat and return a number.
fn parse_number<Parsed: FromStr>(input: &[u8]) -> IResult<&[u8], Parsed> {
    map_res(digit1, parse_bytes)(input)
}

/// Eat and return an RGB value (three `u8` components).
fn parse_rgb(input: &[u8]) -> IResult<&[u8], Color> {
    let (input, r) = parse_number(input)?;
    let (input, ()) = whitespace(input)?;
    let (input, g) = parse_number(input)?;
    let (input, ()) = whitespace(input)?;
    let (input, b) = parse_number(input)?;

    Ok((input, RGB { r, g, b }))
}

/// Parse a header and colours from an input stream.
fn parse(input: &[u8]) -> IResult<&[u8], Vec<Color>> {
    let (input, _) = tag(b"JASC-PAL\r\n")(input)?;
    let (input, _) = tag(b"0100\r\n")(input)?;
    let (input, num_colors): (_, i32) = parse_number(input)?;
    let (input, _) = tag(b"\r\n")(input)?;

    // `digit1` takes no sign, so the count is never negative.
    let mut colors = Vec::with_capacity(usize::try_from(num_colors).unwrap_or(0));
    let mut input = input;
    for _ in 0..num_colors {
        let (remaining, color) = parse_rgb(input)?;
        colors.push(color);
        let (remaining, _) = tag(b"\r\n")(remaining)?;
        input = remaining;
    }

    Ok((input, colors))
}

/// An error occurred during reading.
#[derive(Debug, thiserror::Error)]
pub enum ReadPaletteError {
    /// An I/O error occurred.
    #[error(transparent)]
    IoError(#[from] IoError),
    /// The palette could not be parsed.
    #[error("parse error")]
    ParseError,
}

/// A Palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    colors: Vec<Color>,
}

impl From<Vec<Color>> for Palette {
    /// Create a palette from a vector of colours.
    #[inline]
    fn from(colors: Vec<Color>) -> Self {
        Self { colors }
    }
}

impl Default for Palette {
    /// Create a palette with 256 colours, all black.
    #[inline]
    fn default() -> Self {
        Self::from(vec![Color::default(); 256])
    }
}

impl Palette {
    /// Create an empty palette.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self::from(vec![])
    }

    /// Read a palette from an input stream.
    ///
    /// # Errors
    ///
    /// Returns [`ReadPaletteError::IoError`] if reading fails, and
    /// [`ReadPaletteError::ParseError`] if the input is not a JASC palette.
    #[inline]
    pub fn read_from(mut input: impl Read) -> Result<Self, ReadPaletteError> {
        let mut buffer = vec![];
        // The byte count is `buffer.len()`.
        let _ = input.read_to_end(&mut buffer)?;
        let (_remaining, pal) = parse(&buffer).map_err(|_| ReadPaletteError::ParseError)?;
        Ok(Self::from(pal))
    }

    /// Write a palette to an output stream.
    ///
    /// # Errors
    ///
    /// Returns an error if writing to `output` fails.
    #[inline]
    pub fn write_to(&self, mut output: impl Write) -> Result<(), IoError> {
        write!(output, "{self}")
    }

    /// Return the colours in a Vec so it can be manipulated.
    ///
    /// It can later be wrapped back into a Palette instance using `Palette::from(vec)`.
    #[inline]
    #[must_use]
    pub fn into_inner(self) -> Vec<Color> {
        self.colors
    }

    /// Return the colours as a slice.
    #[inline]
    #[must_use]
    pub fn colors(&self) -> &[Color] {
        &self.colors
    }

    /// Return the colours as a mutable vector.
    #[inline]
    pub const fn colors_mut(&mut self) -> &mut Vec<Color> {
        &mut self.colors
    }

    /// Returns the number of colours in this palette.
    #[inline]
    #[must_use]
    pub const fn len(&self) -> usize {
        self.colors.len()
    }

    /// Iterate over the colours.
    #[inline]
    pub fn iter(&self) -> Iter<'_, Color> {
        self.colors.iter()
    }

    /// Returns `true` if this palette contains 0 colours.
    #[inline]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.colors.is_empty()
    }

    /// Add a colour at the end of the palette.
    #[inline]
    pub fn add(&mut self, color: Color) {
        self.colors.push(color);
    }

    /// Serialize the palette to a byte vector.
    #[inline]
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        self.to_string().into_bytes()
    }
}

impl Index<PaletteIndex> for Palette {
    type Output = Color;
    /// Get the colour at the given index.
    #[inline]
    fn index(&self, index: PaletteIndex) -> &Self::Output {
        let index: usize = index.into();
        &self.colors[index]
    }
}

impl IndexMut<PaletteIndex> for Palette {
    /// Get the colour at the given index.
    #[inline]
    fn index_mut(&mut self, index: PaletteIndex) -> &mut Self::Output {
        let index: usize = index.into();
        &mut self.colors[index]
    }
}

impl FromStr for Palette {
    type Err = ReadPaletteError;
    /// Parse a palette from a UTF-8 string.
    #[inline]
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Self::read_from(input.as_bytes())
    }
}

impl IntoIterator for Palette {
    type Item = Color;
    type IntoIter = VecIntoIter<Color>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.colors.into_iter()
    }
}

impl<'a> IntoIterator for &'a Palette {
    type Item = &'a Color;
    type IntoIter = Iter<'a, Color>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.colors.iter()
    }
}

impl Display for Palette {
    /// Write the palette in the JASC-PAL text format.
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), FmtError> {
        f.write_str("JASC-PAL\r\n0100\r\n")?;
        write!(f, "{}\r\n", self.colors.len())?;
        for c in &self.colors {
            write!(f, "{} {} {}\r\n", c.r, c.g, c.b)?;
        }
        Ok(())
    }
}

impl From<ReadPaletteError> for genie_error::GenieError {
    /// The error as a [`genie_error::GenieError`]: its kind and reason, the original as the
    /// source.
    #[track_caller]
    fn from(e: ReadPaletteError) -> Self {
        let to = match &e {
            ReadPaletteError::IoError(_) => Self::io("a read or write that failed"),
            ReadPaletteError::ParseError => Self::invalid_input("a palette that does not parse"),
        };
        to.with_source(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn parse_test() {
        let (rest, colors) =
            parse(b"JASC-PAL\r\n0100\r\n2\r\n0 0 0\r\n255 255 255\r\n").expect("failed to parse");
        assert_eq!(rest, b"");
        assert_eq!(colors.len(), 2);
    }

    #[test]
    fn from_str() {
        assert_eq!(
            "JASC-PAL\r\n0100\r\n3\r\n255 0 255\r\n0 0 255\r\n0 255 0\r\n"
                .parse::<Palette>()
                .expect("failed to parse"),
            Palette::from(vec![
                RGB {
                    r: 255,
                    g: 0,
                    b: 255
                },
                RGB { r: 0, g: 0, b: 255 },
                RGB { r: 0, g: 255, b: 0 },
            ])
        );
    }

    #[test]
    fn make() {
        let mut pal = Palette::new();
        for r in 0..=255 {
            pal.add(RGB {
                r,
                g: 255 - r,
                b: 127_u8.wrapping_add(r),
            });
        }
        let s = pal.to_string();
        let mut lines = s.lines();
        assert_eq!(lines.next(), Some("JASC-PAL"));
        assert_eq!(lines.next(), Some("0100"));
        assert_eq!(lines.next(), Some("256"));
        for r in 0..=255 {
            assert_eq!(
                lines.next(),
                Some(format!("{} {} {}", r, 255 - r, 127_u8.wrapping_add(r)).as_str())
            );
        }
        assert_eq!(lines.next(), None);
    }

    #[test]
    fn it_works() {
        let cursor = Cursor::new(b"JASC-PAL\r\n0100\r\n2\r\n0 0 0\r\n255 255 255\r\n".to_vec());
        let pal = Palette::read_from(cursor).expect("failed to parse");
        assert_eq!(
            pal.colors,
            vec![
                RGB { r: 0, g: 0, b: 0 },
                RGB {
                    r: 255,
                    g: 255,
                    b: 255
                },
            ]
        );
        assert_eq!(
            pal.to_string(),
            "JASC-PAL\r\n0100\r\n2\r\n0 0 0\r\n255 255 255\r\n".to_string()
        );
    }
}

#[cfg(test)]
mod genie_error_tests {
    use super::*;
    #[cfg(test)]
    use std::error::Error as StdError;

    /// The errors convert into `GenieError`s with their kinds and reasons, the original as
    /// source.
    #[test]
    fn palette_errors_convert_into_genie_errors() {
        let e: genie_error::GenieError = ReadPaletteError::ParseError.into();
        assert_eq!(
            (e.kind(), e.reason()),
            (
                genie_error::ErrorKind::InvalidInput,
                "a palette that does not parse"
            )
        );
        assert!(StdError::source(&e).is_some());
    }
}
